//! Retained A/A/B counterexample on the current original-node native carrier.
//! Three honest executables sign their own votes. One test-only Byzantine peer
//! signs only its own conflicting proposals; it never runs an honest journal,
//! fabricates a QC, publishes a head, or clean-starts its compromised identity.
use super::*;
use crate::native_pipeline::consensus::{
    chain::ChainRecord,
    native::{NativeChannel, PreparedMessage, ReceiveEvent, ValidatorTransportBindings},
    transport::{self, DecodeLimits, Message},
    wire::Vote,
};
use crate::native_pipeline::ingress::apfl::{ApflLimits, ApflTransferBatch};
use novovm_consensus::round_bft::{journal::codec::decode_archived_decision, VerifiedDecision};
use novovm_network::UdpTransport;
use novovm_protocol::NodeId;
use std::{
    collections::{BTreeMap, BTreeSet, VecDeque},
    sync::{
        atomic::{AtomicBool, Ordering},
        mpsc, Mutex,
    },
    thread::JoinHandle,
};

fn codec() -> DecodeLimits {
    DecodeLimits {
        transactions: 2,
        transaction_bytes: 1024,
        body_bytes: 2048,
        message_bytes: 512 * 1024,
    }
}

#[derive(Clone, Default)]
struct Observation {
    sessions: usize,
    votes: BTreeMap<String, Vote>,
    error: Option<String>,
}

struct FaultPeer {
    command: mpsc::SyncSender<(usize, Message)>,
    observed: Arc<Mutex<Observation>>,
    stop: Arc<AtomicBool>,
    thread: Option<JoinHandle<()>>,
}

impl FaultPeer {
    fn start(
        nodes: &ProductNodes,
        faulty: usize,
        set: Arc<ValidatorSet>,
        bodies: Vec<(Hash, Message)>,
    ) -> Result<Self> {
        let config: crate::native_pipeline::service::ResidentConfig =
            serde_json::from_slice(&fs::read(&nodes.configs[faulty])?)?;
        let observed = Arc::new(Mutex::new(Observation::default()));
        let stop = Arc::new(AtomicBool::new(false));
        let (command, receiver) = mpsc::sync_channel::<(usize, Message)>(32);
        let state = observed.clone();
        let stopping = stop.clone();
        // Transport-only test peer, using the original production UDP owner
        // with a separate transport key. Honest nodes are NOT test workers.
        let thread = std::thread::spawn(move || {
            let run = (|| -> Result<()> {
                let domain = MessageDomain {
                    chain_id: CHAIN,
                    genesis: config.genesis.genesis_config_commitment,
                    protocol: config.genesis.protocol_commitment,
                    validators: set.clone(),
                };
                let bindings = ValidatorTransportBindings::new(
                    &set,
                    config
                        .genesis
                        .validators
                        .iter()
                        .map(|v| (v.public_key, v.transport_public_key)),
                )?;
                let peers = config
                    .peers
                    .iter()
                    .map(|peer| {
                        let binding = config
                            .genesis
                            .validators
                            .iter()
                            .find(|v| v.public_key == peer.public_key)
                            .context("fault fixture peer binding missing")?;
                        Ok(NativePeer {
                            key: binding.transport_public_key,
                            address: peer.address,
                        })
                    })
                    .collect::<Result<Vec<_>>>()?;
                let identity = transport_key(faulty);
                let public = identity.verifying_key().to_bytes();
                let transport = UdpTransport::bind_for_chain(
                    NodeId(u64::from_le_bytes(public[..8].try_into()?)),
                    &config.udp_addr.to_string(),
                    CHAIN,
                )?;
                let lanes = std::array::from_fn(|_| LaneLimits {
                    receive: ReassemblyLimits {
                        max_message_bytes: codec().message_bytes,
                        messages: 24,
                        bytes: 8 * 1024 * 1024,
                        peer_messages: 8,
                        peer_bytes: 2 * 1024 * 1024,
                        ttl: Duration::from_secs(30),
                    },
                    send: NativeSendLimits {
                        messages: 24,
                        bytes: 8 * 1024 * 1024,
                        peer_messages: 8,
                        peer_bytes: 2 * 1024 * 1024,
                    },
                });
                let mut channel = NativeChannel::from_transport(
                    transport,
                    identity,
                    bindings,
                    peers,
                    0,
                    domain,
                    codec(),
                    lanes,
                    NativeSendPolicy {
                        window: NovoRudpWindowConfig {
                            packet_copies: 1,
                            tail_packet_copies: 1,
                            batch_size: 32,
                            batch_pause_ms: 0,
                            ack_wait_ms: 10,
                            ..Default::default()
                        },
                        pacing: NovoRudpPacingProfile {
                            packet_copies: 1,
                            tail_packet_copies: 1,
                            batch_size: 32,
                            batch_pause_ms: 0,
                            ack_wait_ms: 10,
                            no_progress_backoff: false,
                        },
                        ttl: Duration::from_secs(30),
                    },
                )?;
                let mut pending: VecDeque<(usize, PreparedMessage)> = VecDeque::new();
                // Same configured-peer possession boundary as the product:
                // ordinary AEAD traffic confirms responders. Do not wait for
                // every session before sending on already-ready initiators.
                let probe = channel.prepare(&Message::RequestBody {
                    body_id: [0xf9; 32],
                })?;
                for peer in 0..channel.peer_count() {
                    pending.push_back((peer, probe.clone()));
                }
                while !stopping.load(Ordering::Acquire) {
                    let now = Instant::now();
                    for _ in 0..16 {
                        let Ok((peer, message)) = receiver.try_recv() else {
                            break;
                        };
                        ensure!(pending.len() < 32, "fault fixture outbound budget");
                        pending.push_back((peer, channel.prepare(&message)?));
                    }
                    let _ = channel.poll_io(wall_ms()?, now, 64)?;
                    state
                        .lock()
                        .map_err(|_| anyhow::anyhow!("fault observer poisoned"))?
                        .sessions = (0..channel.peer_count())
                        .filter(|peer| channel.session_id(*peer).is_some())
                        .count();
                    for _ in 0..pending.len() {
                        let (peer, prepared) = pending.pop_front().unwrap();
                        if !channel.try_send(peer, &prepared, now)? {
                            pending.push_back((peer, prepared));
                        }
                    }
                    for _ in 0..24 {
                        let Some(event) = channel.poll_receive(now, 64)? else {
                            continue;
                        };
                        let ReceiveEvent::Message(received) = event else {
                            anyhow::bail!("native fault observer rejected honest message");
                        };
                        match received.message {
                            Message::Vote(vote) => {
                                vote.verify(&set)?;
                                let peer = (0..channel.peer_count())
                                    .find(|peer| {
                                        channel.peer_id(*peer).is_ok_and(|id| id == received.peer)
                                    })
                                    .context("unknown honest source")?;
                                let expected =
                                    Validator::new(config.peers[peer].public_key, 1)?.id();
                                ensure!(
                                    vote.validator_id == expected,
                                    "native source differs from honest signer"
                                );
                                let slot = format!(
                                    "{}:{}:{:?}:{}",
                                    vote.context.height,
                                    vote.round,
                                    vote.phase,
                                    hex(&vote.validator_id)
                                );
                                let mut observation = state
                                    .lock()
                                    .map_err(|_| anyhow::anyhow!("fault observer poisoned"))?;
                                if let Some(old) = observation.votes.get(&slot) {
                                    ensure!(old == &vote, "honest same-slot conflicting votes");
                                } else {
                                    ensure!(
                                        observation.votes.len() < 256,
                                        "fault observer vote budget"
                                    );
                                    observation.votes.insert(slot, vote);
                                }
                            }
                            Message::RequestBody { body_id } => {
                                if let Some((_, body)) =
                                    bodies.iter().find(|(id, _)| *id == body_id)
                                {
                                    let peer = (0..channel.peer_count())
                                        .find(|peer| {
                                            channel
                                                .peer_id(*peer)
                                                .is_ok_and(|id| id == received.peer)
                                        })
                                        .context("body requester unknown")?;
                                    ensure!(pending.len() < 32, "fault fixture body budget");
                                    pending.push_back((peer, channel.prepare(body)?));
                                }
                            }
                            _ => {} // No faulty vote, journal, decision or QC.
                        }
                    }
                    std::thread::sleep(Duration::from_millis(1));
                }
                Ok(())
            })();
            if let Err(error) = run {
                if let Ok(mut state) = state.lock() {
                    state.error = Some(format!("{error:#}"));
                }
            }
        });
        Ok(Self {
            command,
            observed,
            stop,
            thread: Some(thread),
        })
    }
    fn send(&self, peer: usize, message: &Message) -> Result<()> {
        self.command
            .try_send((peer, message.clone()))
            .context("bounded native fault command queue")
    }
    fn snapshot(&self) -> Result<Observation> {
        let value = self
            .observed
            .lock()
            .map_err(|_| anyhow::anyhow!("fault observer poisoned"))?
            .clone();
        ensure!(
            value.error.is_none(),
            "native fault peer failed: {:?}",
            value.error
        );
        Ok(value)
    }
}
impl Drop for FaultPeer {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Release);
        if let Some(thread) = self.thread.take() {
            let _ = thread.join();
        }
    }
}
fn wall_ms() -> Result<u64> {
    Ok(SystemTime::now()
        .duration_since(UNIX_EPOCH)?
        .as_millis()
        .try_into()?)
}

/// Direct migration of the unpublished candidate preparation in the retained
/// resident_rpc_process/aab.rs. Same real AOEM business compiler, no signer/head.
type EquivocationBodies = (ConsensusContext, ParentPoint, Vec<(Hash, Hash, Message)>);

fn candidates(
    directory: &Path,
    genesis: &GenesisConfig,
    set: &ValidatorSet,
) -> Result<EquivocationBodies> {
    fs::create_dir_all(directory)?;
    let mut changes = fee_record_changes(&genesis.policy, &FeeState::default())?;
    for allocation in &genesis.allocations {
        let account = if allocation.account_hex == hex(account(1).as_bytes()) {
            account(1)
        } else {
            ensure!(
                allocation.account_hex == hex(account(3).as_bytes()),
                "unexpected AAB genesis account"
            );
            account(3)
        };
        changes.push(StateChange::Put {
            key: balance_key(&account),
            value: allocation.amount.parse::<u128>()?.to_le_bytes().to_vec(),
        });
    }
    let initial = stage_state_update(&Memory::default(), empty_root(), &changes)?;
    let mut config = pipeline_config(directory)?;
    config.store.domain = StorageDomain {
        chain_id: CHAIN,
        genesis_config_commitment: genesis.genesis_config_commitment,
        protocol_commitment: genesis.protocol_commitment,
    };
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
        parent_block_hash: parent.block_hash,
        parent_decision_hash: parent.decision_hash,
    };
    let mut execution = batch_context(initial.root());
    execution.genesis_config_commitment = genesis.genesis_config_commitment;
    execution.protocol_commitment = genesis.protocol_commitment;
    execution.effect_contract = effect_contract(&genesis.policy)?;
    execution.slot = 1;
    execution.timestamp_unix_ms = genesis
        .timestamp_unix_ms
        .checked_add(1)
        .context("AAB candidate time overflow")?;
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
        let deadline = Instant::now() + Duration::from_secs(90);
        let mut ticket = loop {
            ensure!(
                Instant::now() < deadline,
                "unpublished AAB admission timeout"
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
                "unpublished AAB execution timeout"
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
    ensure!(bodies[0].0 != bodies[1].0, "AAB candidate values equal");
    services.shutdown()?;
    Ok((context, parent, bodies))
}

fn audit_archive(
    nodes: &ProductNodes,
    honest: &[usize],
    set: &ValidatorSet,
    initial: ParentPoint,
    final_head: Status,
) -> Result<Value> {
    let allowed: BTreeSet<_> = honest
        .iter()
        .map(|index| {
            Validator::new(validator_key(*index).verifying_key().to_bytes(), 1).map(|v| v.id())
        })
        .collect::<Result<_>>()?;
    let mut reports = Vec::new();
    for index in honest {
        ensure!(
            nodes.processes[*index].is_none(),
            "archive audit on running signer"
        );
        let config = crate::native_pipeline::service::ResidentConfig::load(&nodes.configs[*index])?;
        let store = CandidateStore::open(
            StoreConfig {
                library: config.library,
                database: config.database,
                domain: StorageDomain {
                    chain_id: CHAIN,
                    genesis_config_commitment: config.genesis.genesis_config_commitment,
                    protocol_commitment: config.genesis.protocol_commitment,
                },
                storage: StorageConfig::default(),
                packet_budget: PacketBudget::default(),
            },
            OpenMode::Existing,
        )?;
        let read = |key| -> Result<Vec<u8>> {
            store
                .read_metadata(&[key])?
                .values
                .pop()
                .flatten()
                .context("AAB archive record missing")
        };
        let mut parent = initial;
        let mut blocks = Vec::new();
        for height in 1..=final_head.height {
            let record = ChainRecord::decode(&read(MetaKey::ChainBlock { height })?)?;
            let point = record.point();
            ensure!(
                record.parent() == parent && record.context().height == height,
                "AAB archive parent chain mismatch"
            );
            let stored = store
                .recover(record.candidate_id())?
                .context("AAB executed candidate missing")?;
            let statement = BlockStatement::from_stored(&stored, record.context(), set, &parent)?;
            ensure!(
                statement.hash() == point.block_hash
                    && stored.state_root() == point.state_root
                    && stored.receipt_batch_commitment() == point.receipt_batch_commitment,
                "AAB execution result mismatch"
            );
            let outbox = record.outbox_key();
            let MetaKey::ConsensusOutbox { sequence, .. } = &outbox else {
                anyhow::bail!("AAB archive outbox key");
            };
            let (proposal, certificate) =
                decode_archived_decision(&read(outbox.clone())?, *sequence)?;
            VerifiedDecision::verify(
                &proposal,
                &certificate,
                set,
                record.context(),
                point.block_hash,
            )?;
            ensure!(
                certificate.votes.len() == 3
                    && certificate
                        .votes
                        .iter()
                        .all(|vote| allowed.contains(&vote.validator_id)),
                "AAB decision did not use three actual honest signers"
            );
            ensure!(
                height != 1 || proposal.round > 0,
                "AAB falsely finalized round zero"
            );
            blocks.push(json!({"height":height,"round":proposal.round,"point":point,"qc_signers":certificate.votes.iter().map(|v|v.validator_id).collect::<Vec<_>>() }));
            parent = point;
        }
        ensure!(
            parent.height == final_head.height
                && parent.block_hash == final_head.block_hash
                && parent.state_root == final_head.state_root
                && parent.receipt_batch_commitment == final_head.receipt_batch_commitment,
            "AAB archived final head mismatch"
        );
        reports.push(json!({"node":index,"blocks":blocks}));
    }
    Ok(json!(reports))
}

#[test]
#[ignore = "three current original nodes + one Byzantine native peer; real AOEM; run alone"]
fn native_only_a_a_b_real_honest_signers_recover_and_restart() -> Result<()> {
    let binary = PathBuf::from(
        std::env::var_os("NOVOVM_RESIDENT_NODE_BINARY").context("explicit node binary required")?,
    )
    .canonicalize()?;
    let directory = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../target/native-product-rpc")
        .join(format!(
            "native-aab-{}-{}",
            std::process::id(),
            SystemTime::now().duration_since(UNIX_EPOCH)?.as_nanos()
        ));
    fs::create_dir_all(&directory)?;
    let directory =
        crate::native_pipeline::service::config::native_path(&directory.canonicalize()?);
    eprintln!("native actual AAB artifacts={}", directory.display());
    let genesis = genesis()?;
    let (mut nodes, set) = setup_with_genesis(&directory, binary, genesis.clone(), 2)?;
    for path in &nodes.configs {
        let mut config: Value = serde_json::from_slice(&fs::read(path)?)?;
        config["rpc_addr"] = Value::Null;
        config["profile"] = json!(ORIGINAL_PROFILE);
        config["native_client_limit"] = json!(2);
        fs::write(path, serde_json::to_vec_pretty(&config)?)?;
    }
    let faulty = (0..4)
        .find(|index| {
            Validator::new(validator_key(*index).verifying_key().to_bytes(), 1)
                .is_ok_and(|v| v.id() == set.leader(1, 0).unwrap())
        })
        .context("round zero faulty leader missing")?;
    let honest: Vec<_> = (0..4).filter(|index| *index != faulty).collect();
    let (context, parent, bodies) =
        candidates(&directory.join("unpublished-candidates"), &genesis, &set)?;
    let proposals = bodies
        .iter()
        .map(|(value, body_id, _)| {
            Ok(Message::Proposal {
                proposal: novovm_consensus::round_bft::test_vectors::sign_proposal(
                    context,
                    0,
                    *value,
                    None,
                    &set,
                    &validator_key(faulty),
                )?,
                valid_quorum: None,
                body_id: *body_id,
            })
        })
        .collect::<Result<Vec<_>>>()?;
    let fault = FaultPeer::start(
        &nodes,
        faulty,
        set.clone(),
        bodies
            .iter()
            .map(|(_, id, body)| (*id, body.clone()))
            .collect(),
    )?;
    let signed_wire = proposals
        .iter()
        .map(|message| match message {
            Message::Proposal { proposal, .. } => Ok(hex(&wire::encode_proposal(proposal)?)),
            _ => unreachable!(),
        })
        .collect::<Result<Vec<_>>>()?;
    let mut evidence = json!({"source":"current_original_native_execution_tick","transport":"native_novorudp_apfl","http_listeners":0,"honest_processes":3,"real_machines":1,"faulty_validator":faulty,"faulty_votes_signed":0,"externally_constructed_qcs":0,"throughput_acceptance":false,"completed":false,"assignment":"A/A/B in honest configuration index order","signed_fault_proposal_wire":signed_wire,"phase":"start"});
    let result = (|| -> Result<()> {
        for index in &honest {
            nodes.start(*index, "create")?;
        }
        let mut clients = honest
            .iter()
            .map(|index| connect(&nodes, *index, set.clone()))
            .collect::<Result<Vec<_>>>()?;
        ready(&mut nodes, &mut clients)?;
        // Same historical counterexample: all honest local wallets prepare B;
        // the faulty proposer supplies A to two, B to one. These are original
        // signed transactions; no honest node receives an execution result.
        for client in &mut clients {
            send(client, &[signed(1, 0, 100)?, signed(3, 0, 50)?])?;
        }
        let handshake_deadline = Instant::now() + Duration::from_secs(4);
        while fault.snapshot()?.sessions != 3 {
            ensure!(
                Instant::now() < handshake_deadline,
                "native Byzantine peer handshake timeout"
            );
            std::thread::sleep(Duration::from_millis(1));
        }
        let before = clients.iter_mut().map(status).collect::<Result<Vec<_>>>()?;
        ensure!(
            before.iter().all(|s| s.height == 0 && s.round == 0),
            "AAB injection missed original round zero: {before:?}"
        );
        evidence["phase"] = json!("inject_original_round_zero_equivocation");
        for (peer, _) in honest.iter().enumerate() {
            let selected = usize::from(peer == 2);
            fault.send(peer, &bodies[selected].2)?;
            fault.send(peer, &proposals[selected])?;
        }
        let deadline = Instant::now() + Duration::from_secs(4);
        loop {
            nodes.alive()?;
            let votes = fault.snapshot()?.votes;
            let prevotes: Vec<_> = votes
                .values()
                .filter(|v| v.context == context && v.round == 0 && v.phase == Phase::Prevote)
                .collect();
            if prevotes.len() == 3 {
                for (position, index) in honest.iter().enumerate() {
                    let id =
                        Validator::new(validator_key(*index).verifying_key().to_bytes(), 1)?.id();
                    ensure!(
                        prevotes.iter().any(|v| v.validator_id == id
                            && v.value == Some(bodies[usize::from(position == 2)].0)),
                        "actual native prevotes were not A/A/B"
                    );
                }
                evidence["a_a_b_real_signed_prevotes"] = json!(prevotes);
                break;
            }
            ensure!(
                Instant::now() < deadline,
                "real native A/A/B prevotes missing: {:?}",
                votes.values().collect::<Vec<_>>()
            );
            std::thread::sleep(Duration::from_millis(1));
        }
        fs::write(
            directory.join("aab-before-recovery.json"),
            serde_json::to_vec_pretty(&evidence)?,
        )?;
        evidence["phase"] = json!("natural_pacemaker_recovery");
        let deadline = Instant::now() + Duration::from_secs(90);
        let decided = loop {
            nodes.alive()?;
            let _ = fault.snapshot()?;
            let statuses = clients.iter_mut().map(status).collect::<Result<Vec<_>>>()?;
            evidence["last_status"] = json!(statuses);
            if statuses
                .iter()
                .all(|s| s.height == 1 && s.indexed_height == 1)
            {
                ensure!(
                    statuses
                        .iter()
                        .all(|s| s.block_hash == statuses[0].block_hash
                            && s.state_root == statuses[0].state_root
                            && s.receipt_batch_commitment == statuses[0].receipt_batch_commitment),
                    "honest AAB heads conflict"
                );
                break statuses;
            }
            ensure!(
                Instant::now() < deadline,
                "native honest AAB publication timeout: {statuses:?}"
            );
            std::thread::sleep(Duration::from_millis(2));
        };
        let observed = fault.snapshot()?;
        let nil: Vec<_> = observed
            .votes
            .values()
            .filter(|v| v.context == context && v.round == 0 && v.phase == Phase::Precommit)
            .collect();
        ensure!(
            nil.len() == 3 && nil.iter().all(|v| v.value.is_none()),
            "split did not produce three real nil precommits"
        );
        evidence["round_zero_real_nil_precommits"] = json!(nil);
        let chosen = bodies
            .iter()
            .position(|(value, _, _)| *value == decided[0].block_hash)
            .context("honest decision not one of independently executed A/B")?;
        let moved = if chosen == 0 { 101 } else { 100 };
        let raw = [signed(1, 0, moved)?, signed(3, 0, 50)?];
        let ids = hashes(&raw)?;
        let done = receipts(&mut nodes, &mut clients, &ids)?;
        ensure!(
            done.iter().all(|r| matches!(
                r,
                ReceiptState::Finalized {
                    success: true,
                    nonce_after: 1,
                    ..
                }
            )),
            "AAB result/nonce differs"
        );
        let first_balances = balances(&mut clients, &done, [moved, 50])?;
        evidence["decided_statuses"] = json!(decided);
        evidence["selected_candidate"] = json!(chosen);
        evidence["first_receipts"] = json!(done);
        evidence["first_balances"] = json!(first_balances);
        evidence["phase"] = json!("restart_original_honest_journals");
        nodes.stop_all()?;
        evidence["first_archive_three_honest_qcs"] =
            audit_archive(&nodes, &honest, &set, parent, decided[0])?;
        drop(clients);
        for index in &honest {
            nodes.start(*index, "existing")?;
        }
        let mut clients = honest
            .iter()
            .map(|index| connect(&nodes, *index, set.clone()))
            .collect::<Result<Vec<_>>>()?;
        ready(&mut nodes, &mut clients)?;
        ensure!(
            receipts(&mut nodes, &mut clients, &ids)? == done,
            "AAB restart changed receipts"
        );
        ensure!(
            balances(&mut clients, &done, [moved, 50])? == first_balances,
            "AAB restart changed fees/balances"
        );
        ensure!(
            nodes.processes[faulty].is_none(),
            "fault identity clean-started as honest signer"
        );
        let next = [signed(1, 1, 100)?, signed(3, 1, 50)?];
        send(&mut clients[0], &next)?;
        let next_ids = hashes(&next)?;
        let next_done = receipts(&mut nodes, &mut clients, &next_ids)?;
        ensure!(
            next_done.iter().all(|r| matches!(
                r,
                ReceiptState::Finalized {
                    success: true,
                    nonce_after: 2,
                    ..
                }
            )),
            "AAB successor differs"
        );
        let mut all_done = done;
        all_done.extend(next_done);
        evidence["final_balances"] = json!(balances(&mut clients, &all_done, [moved + 100, 100])?);
        let final_heads = ready(&mut nodes, &mut clients)?;
        ensure!(
            final_heads.iter().all(|s| s.height == 2
                && s.block_hash == final_heads[0].block_hash
                && s.state_root == final_heads[0].state_root),
            "AAB successor heads differ"
        );
        nodes.stop_all()?;
        evidence["final_archive_three_honest_qcs"] =
            audit_archive(&nodes, &honest, &set, parent, final_heads[0])?;
        evidence["final_heads"] = json!(final_heads);
        evidence["honest_signed_votes"] =
            json!(fault.snapshot()?.votes.values().collect::<Vec<_>>());
        evidence["completed"] = json!(true);
        evidence["phase"] = json!("complete");
        Ok(())
    })();
    if let Err(error) = &result {
        evidence["failure"] = json!(format!("{error:#}"));
        evidence["last_fault_observation"] = match fault.snapshot() {
            Ok(value) => {
                json!({"sessions":value.sessions,"honest_votes":value.votes.values().collect::<Vec<_>>()})
            }
            Err(error) => json!({"error":format!("{error:#}")}),
        };
    }
    fs::write(
        directory.join("native-aab-report.json"),
        serde_json::to_vec_pretty(&evidence)?,
    )?;
    result
}
