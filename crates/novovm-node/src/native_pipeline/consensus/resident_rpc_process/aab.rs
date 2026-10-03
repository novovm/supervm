//! Three unchanged product executables and one explicitly Byzantine test peer.
//! The peer signs ONLY its own two conflicting round-zero proposals. It never
//! signs an honest validator's vote or constructs a QC, never owns a journal or
//! publishes a head, and is never restarted as a clean honest signer. This is
//! deliberately NOT evidence of four unmodified executables equivocating.

use super::*;
use crate::native_pipeline::consensus::chain::ChainRecord;
use crate::native_pipeline::consensus::transport::{self, DecodeLimits, Message};
use crate::native_pipeline::consensus::wire::{Proposal, Quorum, Vote};
use crate::native_pipeline::ingress::apfl::{ApflLimits, ApflTransferBatch};
use crate::native_pipeline::state::tree::StagedStateUpdate;
use novovm_consensus::round_bft::{journal::codec::decode_archived_decision, VerifiedDecision};
use novovm_network::duplex::fragments::{FragmentAdmission, Reassembler, ReassemblyLimits};
use novovm_network::duplex::product_relay_client::{
    ProductRelayClientConfigV1, ProductRelayTlsTrustV1,
};
use novovm_network::duplex::worker::{
    NetworkWorker, NetworkWorkerConfig, SendAdmission, WorkerLimits,
};

const FAULT_DEADLINE: Duration = Duration::from_secs(90);

fn codec() -> DecodeLimits {
    DecodeLimits {
        transactions: 2,
        transaction_bytes: 1024,
        body_bytes: 2048,
        message_bytes: 512 * 1024,
    }
}

fn peer(index: usize) -> String {
    peer_id_from_ed25519_public_key_v1(&validator_key(index).verifying_key().to_bytes())
}

struct FaultPeer {
    worker: NetworkWorker,
    assembler: Reassembler,
    domain: Hash,
    set: Arc<ValidatorSet>,
    honest: BTreeMap<String, Hash>,
    bodies: BTreeMap<Hash, Message>,
    votes: BTreeMap<String, Vote>,
    decisions: BTreeMap<u64, (Proposal, Quorum)>,
    received: u64,
}

impl FaultPeer {
    fn start(
        index: usize,
        honest: &[usize],
        relay: &Relay,
        genesis: &GenesisConfig,
        set: Arc<ValidatorSet>,
    ) -> Result<Self> {
        let peers: Vec<_> = honest.iter().map(|index| peer(*index)).collect();
        let domain = transport::fragment_domain(
            CHAIN,
            genesis.genesis_config_commitment,
            genesis.protocol_commitment,
        );
        let worker = NetworkWorker::start(
            NetworkWorkerConfig {
                chain_id: CHAIN,
                relay: ProductRelayClientConfigV1 {
                    endpoint: relay.endpoint.clone(),
                    expected_relay_peer_id: peer_id_from_ed25519_public_key_v1(
                        &SigningKey::from_bytes(&[91; 32]).verifying_key().to_bytes(),
                    ),
                    connect_timeout_ms: 2000,
                    read_timeout_ms: 10,
                    tls_trust: ProductRelayTlsTrustV1::ExplicitCa {
                        certificate_path: relay.certificate.clone(),
                    },
                },
                peers: peers.clone(),
                limits: WorkerLimits::default(),
                handshake_timeout_ms: 5000,
                reconnect_delay_ms: 50,
                heartbeat_interval_ms: 1000,
                queue_ttl_ms: 30_000,
            },
            validator_key(index),
        )?;
        Ok(Self {
            worker,
            assembler: Reassembler::new(
                domain,
                peers,
                ReassemblyLimits {
                    max_message_bytes: codec().message_bytes,
                    messages: 32,
                    bytes: 8 * 1024 * 1024,
                    peer_messages: 8,
                    peer_bytes: 2 * 1024 * 1024,
                    ttl: Duration::from_secs(30),
                },
            )?,
            domain,
            set,
            honest: honest
                .iter()
                .map(|index| {
                    Ok((
                        peer(*index),
                        Validator::new(validator_key(*index).verifying_key().to_bytes(), 1)?.id(),
                    ))
                })
                .collect::<Result<_>>()?,
            bodies: BTreeMap::new(),
            votes: BTreeMap::new(),
            decisions: BTreeMap::new(),
            received: 0,
        })
    }

    fn send(&mut self, destination: &str, message: &Message) -> Result<()> {
        let outgoing = transport::prepare_message(self.domain, message, codec())?;
        for index in 0..outgoing.frame_count() {
            let mut frame = outgoing.frame(index)?;
            let deadline = Instant::now() + Duration::from_secs(5);
            loop {
                match self.worker.try_send(destination.to_owned(), frame)? {
                    SendAdmission::Accepted => break,
                    SendAdmission::Backpressure(returned) => frame = returned.bytes,
                    SendAdmission::Rejected { reason, .. } => {
                        bail!("fault peer send rejected: {reason:?}")
                    }
                }
                ensure!(
                    Instant::now() < deadline,
                    "fault peer send backpressure timeout"
                );
                std::thread::sleep(Duration::from_millis(1));
            }
        }
        Ok(())
    }

    fn pump(&mut self) -> Result<()> {
        let now = Instant::now();
        for _ in 0..128 {
            let Some(inbound) = self.worker.try_recv()? else {
                break;
            };
            ensure!(
                self.assembler.push(&inbound.peer_id, &inbound.bytes, now)?
                    != FragmentAdmission::Backpressure,
                "fault observer exceeded bounded fragment budget"
            );
        }
        for _ in 0..32 {
            let Some(inbound) = self.assembler.poll_complete(now, 8)? else {
                break;
            };
            self.received += 1;
            let sender = *self
                .honest
                .get(&inbound.peer)
                .context("observer received an unknown peer")?;
            match transport::decode(&inbound.chunks.concat(), codec())? {
                Message::Vote(vote) => {
                    vote.verify(&self.set)?;
                    ensure!(
                        vote.validator_id == sender,
                        "network source differs from honest vote signer"
                    );
                    let key = format!(
                        "{}:{}:{:?}:{}",
                        vote.context.height,
                        vote.round,
                        vote.phase,
                        hex(&vote.validator_id)
                    );
                    if let Some(old) = self.votes.get(&key) {
                        ensure!(
                            old == &vote,
                            "honest signer emitted conflicting same-height/round/phase vote"
                        );
                    } else {
                        ensure!(
                            self.votes.len() < 512,
                            "fault observer vote evidence budget exceeded"
                        );
                        self.votes.insert(key, vote);
                    }
                }
                Message::Decision {
                    proposal,
                    certificate,
                    ..
                } => {
                    proposal.verify(&self.set)?;
                    let checked = certificate.verify(&self.set)?;
                    ensure!(
                        checked.context() == &proposal.context
                            && checked.round() == proposal.round
                            && checked.phase() == Phase::Precommit
                            && checked.value() == Some(proposal.value),
                        "observed decision certificate does not bind proposal"
                    );
                    ensure!(
                        certificate.votes.len() == 3
                            && checked.signed_weight() == 3
                            && certificate.votes.iter().all(|vote| self
                                .honest
                                .values()
                                .any(|id| *id == vote.validator_id)),
                        "decision did not originate from three actual honest signers"
                    );
                    if let Some((old, _)) = self.decisions.get(&proposal.context.height) {
                        ensure!(
                            old.context == proposal.context && old.value == proposal.value,
                            "observed conflicting decided values at one height"
                        );
                    } else {
                        ensure!(
                            self.decisions.len() < 8,
                            "fault observer decision budget exceeded"
                        );
                        self.decisions
                            .insert(proposal.context.height, (proposal, certificate));
                    }
                }
                Message::RequestBody { body_id } => {
                    if let Some(body) = self.bodies.get(&body_id).cloned() {
                        self.send(&inbound.peer, &body)?;
                    }
                }
                // The faulty member casts no votes, sends no decided value and
                // never forwards an honest signature as a locally created one.
                _ => {}
            }
        }
        Ok(())
    }

    fn phase_votes(&self, context: ConsensusContext, phase: Phase) -> Vec<&Vote> {
        self.votes
            .values()
            .filter(|vote| vote.context == context && vote.round == 0 && vote.phase == phase)
            .collect()
    }

    fn report(&self) -> Value {
        json!({"received_messages":self.received,"honest_signed_votes":self.votes.values().collect::<Vec<_>>(),
            "observed_decisions":self.decisions.values().collect::<Vec<_>>(),
            "fault_peer_votes_signed":0,"externally_constructed_qcs":0,
            "lock_state_directly_exposed":false,"scope":"wire-verified real signer behavior, not an in-memory lock getter"})
    }
}

impl Drop for FaultPeer {
    fn drop(&mut self) {
        let _ = self.worker.shutdown();
    }
}

fn initial_state(genesis: &GenesisConfig) -> Result<StagedStateUpdate> {
    genesis.validate()?;
    let mut changes = fee_record_changes(&genesis.policy, &FeeState::default())?;
    for allocation in &genesis.allocations {
        let text = allocation
            .account_hex
            .strip_prefix("0x")
            .unwrap_or(&allocation.account_hex);
        let bytes = text
            .as_bytes()
            .chunks_exact(2)
            .map(|pair| Ok(u8::from_str_radix(std::str::from_utf8(pair)?, 16)?))
            .collect::<Result<Vec<_>>>()?;
        let account = Account::try_from(bytes).map_err(anyhow::Error::msg)?;
        changes.push(StateChange::Put {
            key: balance_key(&account),
            value: allocation.amount.parse::<u128>()?.to_le_bytes().to_vec(),
        });
    }
    stage_state_update(&Memory::default(), empty_root(), &changes)
}

fn metadata(store: &CandidateStore, key: MetaKey) -> Result<Vec<u8>> {
    let mut values = store.read_metadata(&[key])?.values;
    ensure!(values.len() == 1, "fault archive metadata count differs");
    values
        .pop()
        .flatten()
        .context("fault archive metadata missing")
}

/// Read existing stopped stores; verify actual signed decisions, exact parent
/// ancestry and the executed statement. This is not another economic oracle,
/// does not re-execute transactions, and does not fabricate a certificate.
pub(super) fn audit_archived_quorums(
    nodes: &ProductNodes,
    indices: &[usize],
    genesis: &GenesisConfig,
    set: &ValidatorSet,
    head: ParentPoint,
    allowed_signers: &BTreeSet<Hash>,
) -> Result<Value> {
    ensure!(
        head.height > 0 && head.height <= 1024,
        "fault audit height exceeds bounded fixture scope"
    );
    ensure!(
        !indices.is_empty() && indices.iter().collect::<BTreeSet<_>>().len() == indices.len(),
        "fault archive audit needs distinct actual nodes"
    );
    let initial = initial_state(genesis)?;
    let mut reports = Vec::new();
    for index in indices {
        ensure!(
            nodes.processes.get(*index).is_some_and(Option::is_none),
            "stop original process before archive audit"
        );
        let store = CandidateStore::open(
            StoreConfig {
                library: library()?,
                database: nodes.directory.join(format!("validator-{index}.rocksdb")),
                domain: StorageDomain {
                    chain_id: genesis.chain_id,
                    genesis_config_commitment: genesis.genesis_config_commitment,
                    protocol_commitment: genesis.protocol_commitment,
                },
                storage: StorageConfig::default(),
                packet_budget: PacketBudget::default(),
            },
            OpenMode::Existing,
        )?;
        let mut parent = ParentPoint {
            height: 0,
            block_hash: [0; 32],
            state_root: initial.root(),
            receipt_batch_commitment: empty_root(),
            state_version: 0,
            decision_hash: [0; 32],
        };
        let mut blocks = Vec::new();
        let mut last_record = None;
        for height in 1..=head.height {
            let record = ChainRecord::decode(&metadata(&store, MetaKey::ChainBlock { height })?)?;
            let context = ConsensusContext {
                chain_id: genesis.chain_id,
                genesis_config_commitment: genesis.genesis_config_commitment,
                protocol_commitment: genesis.protocol_commitment,
                epoch: genesis.validator_epoch,
                validator_set_hash: set.hash(),
                height,
                parent_block_hash: parent.block_hash,
                parent_decision_hash: parent.decision_hash,
            };
            ensure!(
                record.parent() == parent && record.context() == context,
                "fault archive parent/domain differs"
            );
            let stored = store
                .recover(record.candidate_id())?
                .context("fault archive candidate absent")?;
            let statement = BlockStatement::from_stored(&stored, context, set, &parent)?;
            let point = record.point();
            ensure!(
                statement.hash() == point.block_hash
                    && stored.state_root() == point.state_root
                    && stored.receipt_batch_commitment() == point.receipt_batch_commitment
                    && statement.state_version() == point.state_version,
                "fault archive execution statement/roots differ"
            );
            let outbox = record.outbox_key();
            let MetaKey::ConsensusOutbox { sequence, .. } = &outbox else {
                bail!("fault archive outbox locator differs")
            };
            let outbox_bytes = metadata(&store, outbox.clone())?;
            // The record's existing serialization exposes these immutable
            // commitments to this independent read-only fixture, without a
            // new production accessor or a second archive representation.
            let record_fields = serde_json::to_value(&record)?;
            ensure!(
                record_fields["document_digest"] == json!(stored.document_digest())
                    && record_fields["outbox_digest"]
                        == json!(
                            crate::native_pipeline::consensus::chain::archived_outbox_digest(
                                &outbox_bytes
                            )
                        ),
                "fault archive record document/outbox commitment differs"
            );
            let (proposal, certificate) = decode_archived_decision(&outbox_bytes, *sequence)?;
            VerifiedDecision::verify(&proposal, &certificate, set, context, point.block_hash)?;
            ensure!(
                certificate.votes.len() >= 3
                    && certificate
                        .votes
                        .iter()
                        .all(|vote| allowed_signers.contains(&vote.validator_id)),
                "fault archive quorum count or actual signer set differs"
            );
            blocks.push(json!({"height":height,"round":proposal.round,"block_hash":hex(&point.block_hash),
                "transaction_count":stored.raw_transactions().len(),
                "qc_signers":certificate.votes.iter().map(|vote|hex(&vote.validator_id)).collect::<Vec<_>>(),
                "qc_signature_count":certificate.votes.len()}));
            parent = point;
            last_record = Some(record);
        }
        ensure!(
            parent == head
                && metadata(&store, MetaKey::ChainHead)?
                    == last_record.context("archive has no head")?.head_bytes()?,
            "fault archive head pointer differs from verified chain"
        );
        reports.push(json!({"node":index,"head":head,"blocks":blocks,"readonly_existing_archive_verified":true}));
    }
    Ok(json!(reports))
}

/// Compute two candidate values using the same native pipeline/business policy,
/// in an unpublished fixture store. No validator journal or chain head exists
/// here; the honest executables independently execute the transmitted bodies.
type CandidateBodies = (ConsensusContext, Vec<(Hash, Hash, Message)>);

fn candidates(
    directory: &Path,
    genesis: &GenesisConfig,
    set: &ValidatorSet,
) -> Result<CandidateBodies> {
    fs::create_dir_all(directory)?;
    let mut config = super::super::pipeline_config(directory)?;
    config.store.domain = StorageDomain {
        chain_id: CHAIN,
        genesis_config_commitment: genesis.genesis_config_commitment,
        protocol_commitment: genesis.protocol_commitment,
    };
    let initial = initial_state(genesis)?;
    {
        let store = CandidateStore::open(config.store.clone(), OpenMode::CreateNew)?;
        store.install_unpublished_state(&initial)?;
    }
    let parent = ParentPoint {
        height: 0,
        block_hash: [0; 32],
        state_root: initial.root(),
        receipt_batch_commitment: empty_root(),
        state_version: 0,
        decision_hash: [0; 32],
    };
    let context = ConsensusContext {
        chain_id: CHAIN,
        genesis_config_commitment: genesis.genesis_config_commitment,
        protocol_commitment: genesis.protocol_commitment,
        epoch: genesis.validator_epoch,
        validator_set_hash: set.hash(),
        height: 1,
        parent_block_hash: [0; 32],
        parent_decision_hash: [0; 32],
    };
    let mut execution = batch_context(initial.root());
    execution.genesis_config_commitment = genesis.genesis_config_commitment;
    execution.protocol_commitment = genesis.protocol_commitment;
    execution.effect_contract = effect_contract(&genesis.policy)?;
    execution.slot = 1;
    execution.timestamp_unix_ms = genesis
        .timestamp_unix_ms
        .checked_add(1)
        .context("fixture timestamp overflow")?;
    let mut services = Services(vec![CandidatePipeline::start(config, OpenMode::Existing)?]);
    let pipeline = &services.0[0];
    let mut bodies = Vec::new();
    for amount in [101, 100] {
        let batch = Arc::new(ApflTransferBatch::from_raw(
            &[signed(1, 0, amount)?, signed(3, 0, 50)?],
            ApflLimits {
                transactions: 2,
                transaction_bytes: 1024,
                body_bytes: 2048,
            },
        )?);
        let mut request =
            BatchRequest::from_apfl(batch.clone(), execution, genesis.policy.clone())?;
        let deadline = Instant::now() + FAULT_DEADLINE;
        let mut ticket = loop {
            ensure!(
                Instant::now() < deadline,
                "fault candidate admission timeout"
            );
            match pipeline.try_submit(request)? {
                Submission::Accepted(ticket) => break ticket,
                Submission::Backpressured(returned) => request = returned,
            }
            std::thread::sleep(Duration::from_millis(1));
        };
        let candidate = loop {
            ensure!(
                Instant::now() < deadline,
                "fault candidate execution timeout"
            );
            if let Some(done) = ticket.try_take()? {
                break done.candidate().clone();
            }
            std::thread::sleep(Duration::from_millis(1));
        };
        let value =
            BlockStatement::from_executed(candidate.packet(), context, set, &parent)?.hash();
        let body_id = transport::apfl_body_id(&execution, &batch, codec())?;
        bodies.push((
            value,
            body_id,
            Message::ApflBody {
                context: execution,
                batch,
            },
        ));
    }
    ensure!(bodies[0].0 != bodies[1].0, "fault candidates must differ");
    services.shutdown()?;
    Ok((context, bodies))
}

fn status_all(nodes: &mut ProductNodes, honest: &[usize]) -> Result<Vec<Value>> {
    nodes.alive()?;
    honest
        .iter()
        .map(|index| nodes.rpc(*index, "nov_chainStatus", json!([])))
        .collect()
}

fn wait_decision(
    peer: &mut FaultPeer,
    nodes: &mut ProductNodes,
    honest: &[usize],
    height: u64,
) -> Result<()> {
    let deadline = Instant::now() + FAULT_DEADLINE;
    loop {
        nodes.alive()?;
        peer.pump()?;
        if peer.decisions.contains_key(&height) {
            return Ok(());
        }
        ensure!(
            Instant::now() < deadline,
            "honest product decision {height} timeout; statuses={:?}",
            status_all(nodes, honest)?
        );
        std::thread::sleep(Duration::from_millis(2));
    }
}

#[test]
#[ignore = "requires actual product binary and AOEM; three honest original executables + one Byzantine network peer, not four-machine/mainnet acceptance"]
fn actual_product_rpc_a_a_b_equivocation_recovers_with_three_honest_signers() -> Result<()> {
    let binary = PathBuf::from(
        std::env::var_os("NOVOVM_RESIDENT_NODE_BINARY")
            .context("explicit NOVOVM_RESIDENT_NODE_BINARY required")?,
    )
    .canonicalize()?;
    ensure!(binary.is_file(), "actual product binary missing");
    let directory = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../target/resident-rpc-aab-tests")
        .join(format!(
            "{}-{}",
            std::process::id(),
            SystemTime::now().duration_since(UNIX_EPOCH)?.as_nanos()
        ));
    fs::create_dir_all(&directory)?;
    eprintln!("actual product A/A/B artifacts={}", directory.display());
    let genesis = genesis()?;
    let mut relay = Relay::start(&directory.join("relay"))?;
    let (mut nodes, set) =
        setup_with_genesis(&directory, &relay, binary.clone(), genesis.clone(), 2)?;
    let faulty_id = set.leader(1, 0)?;
    let faulty = (0..4)
        .find(|index| {
            Validator::new(validator_key(*index).verifying_key().to_bytes(), 1)
                .is_ok_and(|validator| validator.id() == faulty_id)
        })
        .context("fixture leader missing")?;
    let honest: Vec<_> = (0..4).filter(|index| *index != faulty).collect();
    let (context, bodies) = candidates(
        &directory.join("unpublished-candidate-fixture"),
        &genesis,
        &set,
    )?;
    let value_a = bodies[0].0;
    let value_b = bodies[1].0;
    let mut proposals = Vec::new();
    for (value, body_id, _) in &bodies {
        let proposal = novovm_consensus::round_bft::test_vectors::sign_proposal(
            context,
            0,
            *value,
            None,
            &set,
            &validator_key(faulty),
        )?;
        proposals.push(Message::Proposal {
            proposal,
            valid_quorum: None,
            body_id: *body_id,
        });
    }
    let fault_artifact = json!({"faulty_validator":faulty,"faulty_validator_id":hex(&faulty_id),
        "signed_proposal_wire":proposals.iter().map(|message| match message {
            Message::Proposal { proposal, .. } => Ok(hex(&wire::encode_proposal(proposal)?)),
            _ => unreachable!(),
        }).collect::<Result<Vec<_>>>()?,
        "value_a":hex(&value_a),"value_b":hex(&value_b),"honest_indices":honest,
        "assignment":"A/A/B in honest index order; zero Byzantine votes; never clean-start this identity"});
    fs::write(
        directory.join("byzantine-signed-proposals.json"),
        serde_json::to_vec_pretty(&fault_artifact)?,
    )?;
    let mut fault = FaultPeer::start(faulty, &honest, &relay, &genesis, set.clone())?;
    for (_, body_id, body) in &bodies {
        fault.bodies.insert(*body_id, body.clone());
    }
    let mut evidence = json!({"fault":fault_artifact});
    let mut phase = "start-honest-processes";
    let result = (|| -> Result<()> {
        for index in &honest {
            nodes.start(*index, "create")?;
        }
        nodes.wait_ready(&honest)?;
        let raw = vec![signed(1, 0, 100)?, signed(3, 0, 50)?];
        let mut hashes = transaction_hashes(&raw)?;
        for index in &honest {
            nodes.submit_batch(*index, &raw)?;
        }
        let deadline = Instant::now() + Duration::from_secs(4);
        loop {
            fault.pump()?;
            let network = fault.worker.status()?;
            if honest
                .iter()
                .all(|index| network.active_peers.contains(&peer(*index)))
            {
                break;
            }
            ensure!(
                Instant::now() < deadline,
                "Byzantine peer handshake timeout"
            );
            std::thread::sleep(Duration::from_millis(1));
        }
        ensure!(
            status_all(&mut nodes, &honest)?
                .iter()
                .all(|status| status["round"] == 0 && status["head"].is_null()),
            "fault must be injected into the original round, not relabeled later"
        );
        phase = "inject-own-equivocating-proposals";
        let send_split = |fault: &mut FaultPeer| -> Result<()> {
            for (position, index) in honest.iter().enumerate() {
                let selected = usize::from(position == 2);
                fault.send(&peer(*index), &bodies[selected].2)?;
                fault.send(&peer(*index), &proposals[selected])?;
            }
            Ok(())
        };
        send_split(&mut fault)?;
        phase = "observe-real-a-a-b-prevotes";
        let deadline = Instant::now() + Duration::from_secs(4);
        let mut resent = Instant::now();
        loop {
            nodes.alive()?;
            fault.pump()?;
            let votes = fault.phase_votes(context, Phase::Prevote);
            if votes.len() == 3 {
                for (position, index) in honest.iter().enumerate() {
                    let id =
                        Validator::new(validator_key(*index).verifying_key().to_bytes(), 1)?.id();
                    let value = if position == 2 { value_b } else { value_a };
                    ensure!(
                        votes
                            .iter()
                            .any(|vote| vote.validator_id == id && vote.value == Some(value)),
                        "actual honest round-zero prevotes were not A/A/B"
                    );
                }
                break;
            }
            ensure!(
                Instant::now() < deadline,
                "actual A/A/B prevotes missing; observed={:?}",
                fault.report()
            );
            if resent.elapsed() >= Duration::from_millis(100) {
                send_split(&mut fault)?;
                resent = Instant::now();
            }
            std::thread::sleep(Duration::from_millis(1));
        }
        let split_status = status_all(&mut nodes, &honest)?;
        evidence["a_a_b_status"] = json!(split_status);
        evidence["a_a_b_signed_evidence"] = fault.report();
        fs::write(
            directory.join("a-a-b-before-recovery.json"),
            serde_json::to_vec_pretty(&evidence)?,
        )?;
        ensure!(
            split_status.iter().all(|status| status["head"].is_null()),
            "split votes falsely published a head"
        );
        phase = "natural-pacemaker-and-three-honest-qc";
        wait_decision(&mut fault, &mut nodes, &honest, 1)?;
        let nil_votes = fault.phase_votes(context, Phase::Precommit);
        ensure!(
            nil_votes.len() == 3 && nil_votes.iter().all(|vote| vote.value.is_none()),
            "A/A/B did not transition through three real nil precommits"
        );
        let (proposal, _) = fault
            .decisions
            .get(&1)
            .context("missing observed first decision")?;
        ensure!(
            proposal.round > 0 && proposal.value == value_b,
            "honest next-round proposal did not converge to independently executed B"
        );
        let receipts = nodes.wait_receipts(&honest, &hashes)?;
        ensure!(
            receipts[0]
                .iter()
                .all(|receipt| receipt["success"] == true && receipt["nonce_after"] == 1),
            "recovered B economics/nonce differs"
        );
        let statuses = nodes.wait_ready(&honest)?;
        ensure!(
            statuses
                .iter()
                .all(|status| status["head"] == statuses[0]["head"]
                    && status["head"]["height"] == 1
                    && status["head"]["state_version"] == 2),
            "three honest finalized heads differ"
        );
        let balances = nodes.balances(&honest, &receipts[0], [100, 50])?;
        evidence["decided_receipts"] = json!(receipts);
        evidence["decided_status"] = json!(statuses);
        evidence["decided_balances"] = json!(balances);
        // A delayed old equivocation is not permission to replace the published
        // state. Replay ONLY the fault identity's original signed proposal.
        for index in &honest {
            fault.send(&peer(*index), &proposals[0])?;
        }
        evidence["late_fault_proposal"] = json!({"resent":true,"application_receipt_observed":false,
            "scope":"queued original fault proposal before stop; not acceptance of delayed-evidence handling"});
        phase = "cold-restart-with-real-journals";
        let live_pids: Vec<_> = nodes.processes.iter().flatten().map(Child::id).collect();
        nodes.stop_all()?;
        let honest_ids: BTreeSet<_> = fault.honest.values().copied().collect();
        let head: ParentPoint = serde_json::from_value(statuses[0]["head"].clone())?;
        evidence["before_restart_archive"] =
            audit_archived_quorums(&nodes, &honest, &genesis, &set, head, &honest_ids)?;
        for index in &honest {
            nodes.start(*index, "existing")?;
        }
        nodes.wait_ready(&honest)?;
        ensure!(
            nodes.processes[faulty].is_none(),
            "Byzantine identity must never be clean-started"
        );
        let cold = nodes.wait_receipts(&honest, &hashes)?;
        ensure!(
            cold == receipts,
            "honest cold restart changed finalized receipts"
        );
        ensure!(
            nodes.balances(&honest, &cold[0], [100, 50])? == balances,
            "honest cold restart changed balances"
        );
        let cold_pids: Vec<_> = nodes.processes.iter().flatten().map(Child::id).collect();
        ensure!(
            cold_pids.iter().all(|pid| !live_pids.contains(pid)),
            "honest processes did not restart"
        );
        phase = "successor-without-fault-vote";
        let next = vec![signed(1, 1, 100)?, signed(3, 1, 50)?];
        for index in &honest {
            nodes.submit_batch(*index, &next)?;
        }
        hashes.extend(transaction_hashes(&next)?);
        wait_decision(&mut fault, &mut nodes, &honest, 2)?;
        let continued = nodes.wait_receipts(&honest, &hashes)?;
        ensure!(
            continued[0][..2] == receipts[0]
                && continued[0][2..]
                    .iter()
                    .all(|receipt| receipt["success"] == true && receipt["nonce_after"] == 2),
            "cold successor nonce/receipt mismatch"
        );
        let final_status = nodes.wait_ready(&honest)?;
        ensure!(
            final_status
                .iter()
                .all(|status| status["head"] == final_status[0]["head"]
                    && status["head"]["height"] == 2
                    && status["head"]["state_version"] == 4),
            "cold successor heads differ"
        );
        evidence["final_balances"] = json!(nodes.balances(&honest, &continued[0], [200, 100])?);
        evidence["final_receipts"] = json!(continued);
        evidence["final_status"] = json!(final_status);
        evidence["live_pids"] = json!(live_pids);
        evidence["cold_pids"] = json!(cold_pids);
        nodes.stop_all()?;
        let head: ParentPoint = serde_json::from_value(final_status[0]["head"].clone())?;
        evidence["final_archive"] =
            audit_archived_quorums(&nodes, &honest, &genesis, &set, head, &honest_ids)?;
        phase = "passed";
        Ok(())
    })();
    evidence["wire_observations"] = fault.report();
    let report = json!({"schema":"novovm/resident-product-rpc-aab/v1","passed":result.is_ok(),
        "failure":result.as_ref().err().map(|error|format!("{error:#}")),"phase":phase,"evidence":evidence,
        "product_binary":binary,"product_binary_sha256":hex(&Sha256::digest(fs::read(&binary)?)),
        "aoem_sha256":hex(&Sha256::digest(fs::read(library()?)?)),
        "topology":"one host; three unchanged original novovm-node processes + one test-only Byzantine WSS/E2E peer",
        "validator_count":4,"honest_original_executables":3,"qc_required":3,"fault_peer_votes":0,
        "honest_signer_reset":false,"external_qc_manufactured":false,"four_original_node_acceptance":false,
        "four_machine_test":false,"performance_measured":false,"production_acceptance":false});
    fs::write(
        directory.join("result.json"),
        serde_json::to_vec_pretty(&report)?,
    )?;
    nodes.stop_all()?;
    fault.worker.shutdown()?;
    relay.shutdown()?;
    eprintln!(
        "actual product A/A/B report={}",
        directory.join("result.json").display()
    );
    result
}
