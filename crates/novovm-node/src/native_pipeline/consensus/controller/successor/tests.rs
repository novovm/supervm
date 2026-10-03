//! Actual AOEM, immutable candidate storage, journal ACKs and assembly owner.
//! Only delivery/consumption timing is controlled here. The disconnected WSS
//! worker is not a four-node network or throughput acceptance fixture.
use super::*;
use crate::native_pipeline::business::direct_nov_fee::{DirectNovFeePolicy, FeeState};
use crate::native_pipeline::business::nov_transfer_batch::{
    balance_key, effect_contract, fee_record_changes, nonce_key, program_id, receipt_codec,
    SEMANTIC_VERSION,
};
use crate::native_pipeline::business::quoted_transfer::Account;
use crate::native_pipeline::consensus::channel::{ChannelConfig, LaneBudget, QueueBudget};
use crate::native_pipeline::consensus::transport::DecodeLimits;
use crate::native_pipeline::consensus::wire::Validator;
use crate::native_pipeline::consensus::JournalOpening;
use crate::native_pipeline::execution::plan::PlanBudget;
use crate::native_pipeline::ingress::authentication::authenticate_transfer_v3;
use crate::native_pipeline::ingress::batch::AuthenticationBudget;
use crate::native_pipeline::ingress::wire::{
    encode_transfer_v3, signing_message, FeePolicy, TransferV3,
};
use crate::native_pipeline::persistence::io::IoBudget;
use crate::native_pipeline::persistence::{
    CandidateStore, OpenMode, PacketBudget, StorageDomain, StoreConfig,
};
use crate::native_pipeline::pipeline::PipelineConfig;
use crate::native_pipeline::state::frontier::CaptureBudget;
use crate::native_pipeline::state::tree::{
    empty_root, stage_state_update, StateChange, StateNodeReader,
};
use anyhow::{bail, Context as _};
use ed25519_dalek::{Signer, SigningKey};
use novovm_exec::resident::StorageConfig;
use novovm_network::duplex::fragments::ReassemblyLimits;
use novovm_network::duplex::product_relay_client::{
    ProductRelayClientConfigV1, ProductRelayTlsTrustV1,
};
use novovm_network::duplex::worker::{NetworkWorker, NetworkWorkerConfig, WorkerLimits};
use sha2::{Digest, Sha256};
use std::fs;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

const CHAIN: u64 = 292;
const GENESIS: Hash = [0x71; 32];
const PROTOCOL: Hash = [0x72; 32];
const DEADLINE: Duration = Duration::from_secs(60);

struct Empty;
impl StateNodeReader for Empty {
    fn read_node(&self, _: &Hash) -> Result<Option<Vec<u8>>> {
        Ok(None)
    }
}

fn key(index: usize) -> SigningKey {
    SigningKey::from_bytes(&[101 + index as u8; 32])
}

fn account(seed: u8) -> Account {
    let public = SigningKey::from_bytes(&[seed; 32])
        .verifying_key()
        .to_bytes();
    Account::try_from(Sha256::digest(public)[12..].to_vec()).unwrap()
}

fn policy() -> DirectNovFeePolicy {
    DirectNovFeePolicy {
        quote_ttl_ms: 15000,
        policy_version: 1,
        policy_source: "default".into(),
        resolution_source: "runtime_state".into(),
        reserve_share_bps: 7000,
        fee_share_bps: 2000,
        risk_buffer_share_bps: 1000,
        min_reserve_bucket_nov: 0,
        min_fee_bucket_nov: 0,
        min_risk_buffer_nov: 1,
        settlement_paused: false,
        redeem_paused: false,
        clearing_enabled: true,
        clearing_daily_nov_hard_limit: 1_000_000,
        clearing_require_healthy_risk_buffer: false,
        clearing_constrained_max_slippage_bps: 500,
        clearing_constrained_daily_usage_bps: 8000,
        clearing_constrained_strategy: "daily_volume_only".into(),
        mapped_asset_auto_heal_rollback_enabled: false,
        mapped_asset_reorg_response_policy: "report_only".into(),
    }
}

fn raw(height: u64, extra: u128) -> Result<Vec<Vec<u8>>> {
    [(1, 100 + extra), (3, 50)]
        .into_iter()
        .map(|(seed, amount)| {
            let signer = SigningKey::from_bytes(&[seed; 32]);
            let mut tx = TransferV3 {
                chain_id: CHAIN,
                from: account(seed).as_bytes().to_vec(),
                to: account(2).as_bytes().to_vec(),
                asset: "NOV".into(),
                amount,
                nonce: height - 1,
                fee_policy: FeePolicy {
                    pay_asset: "NOV".into(),
                    max_pay_amount: 0,
                    slippage_bps: 0,
                },
                signature: Vec::new(),
            };
            let signature = signer.sign(&signing_message(&tx)?);
            tx.signature = signer.verifying_key().to_bytes().to_vec();
            tx.signature.extend_from_slice(&signature.to_bytes());
            encode_transfer_v3(&tx)
        })
        .collect()
}

fn execution(parent: ParentPoint) -> BatchContext {
    BatchContext {
        chain_id: CHAIN,
        genesis_config_commitment: GENESIS,
        protocol_commitment: PROTOCOL,
        business_program: program_id(),
        semantic_version: SEMANTIC_VERSION,
        effect_contract: effect_contract(&policy()).unwrap(),
        parent_block_hash: parent.block_hash,
        parent_height: parent.height,
        parent_state_root: parent.state_root,
        parent_receipt_root: parent.receipt_batch_commitment,
        parent_state_version: parent.state_version,
        receipt_codec: receipt_codec(),
        height: parent.height + 1,
        slot: parent.height,
        timestamp_unix_ms: 172_800_500 + parent.height + 1,
    }
}

fn body(parent: ParentPoint, extra: u128) -> Result<Arc<Message>> {
    Ok(Arc::new(Message::Body {
        context: execution(parent),
        raw_transactions: raw(parent.height + 1, extra)?,
    }))
}

fn index_for(set: &ValidatorSet, id: Hash) -> usize {
    assert!(
        set.member(&id).is_some(),
        "fixture signer not in validator set"
    );
    (0..4)
        .find(|i| {
            Validator::new(key(*i).verifying_key().to_bytes(), 1)
                .unwrap()
                .id()
                == id
        })
        .unwrap()
}

fn channel(config: &ControllerConfig, local: usize) -> Result<HostChannel> {
    let peers: Vec<_> = config.peers.values().cloned().collect();
    let relay = SigningKey::from_bytes(&[91; 32]);
    let network = NetworkWorker::start(
        NetworkWorkerConfig {
            chain_id: CHAIN,
            peers: peers.clone(),
            limits: WorkerLimits::default(),
            relay: ProductRelayClientConfigV1 {
                endpoint: "wss://127.0.0.1:1/novovm".into(),
                expected_relay_peer_id: peer_id_from_ed25519_public_key_v1(
                    &relay.verifying_key().to_bytes(),
                ),
                connect_timeout_ms: 50,
                read_timeout_ms: 10,
                tls_trust: ProductRelayTlsTrustV1::NodeKeyBoundEncrypted,
            },
            handshake_timeout_ms: 1_000,
            reconnect_delay_ms: 100,
            heartbeat_interval_ms: 1_000,
            queue_ttl_ms: 5_000,
        },
        key(local),
    )?;
    let codec = DecodeLimits {
        transactions: 8,
        transaction_bytes: 1024,
        body_bytes: 8192,
        message_bytes: 512 * 1024,
    };
    let lanes = LaneBudget {
        control: QueueBudget {
            messages: 16,
            bytes: 64 * 1024 * 1024,
        },
        body: QueueBudget {
            messages: 8,
            bytes: 32 * 1024 * 1024,
        },
        ingress: QueueBudget {
            messages: 8,
            bytes: 32 * 1024 * 1024,
        },
    };
    HostChannel::start(
        network,
        ChannelConfig {
            chain_id: CHAIN,
            genesis: GENESIS,
            protocol: PROTOCOL,
            peers,
            validators: config.validators.clone(),
            policy: policy(),
            codec,
            reassembly: ReassemblyLimits {
                max_message_bytes: codec.message_bytes,
                messages: 16,
                bytes: 4 * 1024 * 1024,
                peer_messages: 4,
                peer_bytes: 2 * 1024 * 1024,
                ttl: Duration::from_secs(10),
            },
            prepare: lanes,
            send: lanes,
            receive: lanes,
            ttl: Duration::from_secs(10),
        },
    )
}

struct Fixture {
    controller: Option<Controller>,
    pipeline: Option<CandidatePipeline>,
    pipeline_config: PipelineConfig,
}

impl Fixture {
    fn new(name: &str) -> Result<Self> {
        let repository = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../..")
            .canonicalize()?;
        let directory = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../target/runtime-rebuild/controller-successor-tests")
            .join(format!(
                "{name}-{}-{}",
                std::process::id(),
                SystemTime::now().duration_since(UNIX_EPOCH)?.as_nanos()
            ));
        fs::create_dir_all(&directory)?;
        ensure!(
            directory.canonicalize()?.starts_with(repository),
            "fixture outside repository"
        );
        eprintln!("successor safety artifacts={}", directory.display());
        let store = StoreConfig {
            library: fs::canonicalize(PathBuf::from(
                std::env::var_os("NOVOVM_AOEM_TEST_LIBRARY")
                    .context("explicit real NOVOVM_AOEM_TEST_LIBRARY required")?,
            ))?,
            database: directory.join("provider.rocksdb"),
            domain: StorageDomain {
                chain_id: CHAIN,
                genesis_config_commitment: GENESIS,
                protocol_commitment: PROTOCOL,
            },
            storage: StorageConfig::default(),
            packet_budget: PacketBudget::default(),
        };
        let mut changes = fee_record_changes(&policy(), &FeeState::default())?;
        for seed in [1, 3] {
            changes.push(StateChange::Put {
                key: balance_key(&account(seed)),
                value: 1_000_000u128.to_le_bytes().to_vec(),
            });
        }
        let update = stage_state_update(&Empty, empty_root(), &changes)?;
        let initial = CandidateStore::open(store.clone(), OpenMode::CreateNew)?;
        initial.install_unpublished_state(&update)?;
        drop(initial);
        let pipeline_config = PipelineConfig {
            store,
            workers: 4,
            max_batches: 2,
            max_retained_bytes: 256 * 1024 * 1024,
            authentication: AuthenticationBudget {
                transactions: 8,
                transaction_bytes: 1024,
                body_bytes: 8192,
            },
            plan: PlanBudget {
                transactions: 8,
                transaction_bytes: 1024,
                body_bytes: 8192,
                access_keys: 128,
            },
            capture: CaptureBudget {
                keys: 128,
                nodes: 4096,
                bytes: 2 * 1024 * 1024,
            },
            compute_timeout: Duration::from_secs(30),
            io: IoBudget {
                requests: 1,
                ..IoBudget::default()
            },
            capture_edge_quantum: 64,
        };
        let pipeline = CandidatePipeline::start(pipeline_config.clone(), OpenMode::Existing)?;
        let mut fixture = Self {
            controller: None,
            pipeline: Some(pipeline),
            pipeline_config,
        };
        let pipeline = fixture.pipeline.as_ref().unwrap();
        let set = Arc::new(ValidatorSet::new(
            CHAIN,
            1,
            1,
            (0..4)
                .map(|i| Validator::new(key(i).verifying_key().to_bytes(), 1))
                .collect::<Result<_>>()?,
        )?);
        // This node is not the first-height proposer, but is the next-height
        // round-zero proposer AND the first-height round-one proposer.
        let local = set.leader(2, 0)?;
        let index = index_for(&set, local);
        let context = Context {
            chain_id: CHAIN,
            genesis_config_commitment: GENESIS,
            protocol_commitment: PROTOCOL,
            epoch: 1,
            validator_set_hash: set.hash(),
            height: 1,
            parent_block_hash: [0; 32],
            parent_decision_hash: [0; 32],
        };
        let parent = ParentPoint {
            height: 0,
            block_hash: [0; 32],
            state_root: update.root(),
            receipt_batch_commitment: empty_root(),
            state_version: 0,
            decision_hash: [0; 32],
        };
        let config = ControllerConfig {
            peers: set
                .members()
                .iter()
                .filter(|member| member.id() != local)
                .map(|member| {
                    (
                        member.id(),
                        peer_id_from_ed25519_public_key_v1(member.public_key()),
                    )
                })
                .collect(),
            validators: set.clone(),
            local_validator: local,
            execution: execution(parent),
            collector: CollectorLimits {
                max_retained_rounds: 2,
                max_future_round_span: 64,
                max_votes: 28,
            },
            timeouts: TimeoutPolicy {
                propose: Duration::from_secs(30),
                prevote: Duration::from_secs(30),
                precommit: Duration::from_secs(30),
                round_increment: Duration::from_secs(1),
            },
            limits: ControllerLimits::default(),
            retransmit: Duration::from_millis(100),
        };
        let journal = open(
            ValidatorJournal::open(pipeline, context, parent, set, key(index))?,
            pipeline,
        )?;
        let channel = channel(&config, index)?;
        fixture.controller = Some(Controller::new(config, journal, channel)?);
        Ok(fixture)
    }

    fn run(
        name: &str,
        operation: impl FnOnce(&mut Controller, &CandidatePipeline) -> Result<()>,
    ) -> Result<()> {
        let mut fixture = Self::new(name)?;
        let result = operation(
            fixture.controller.as_mut().unwrap(),
            fixture.pipeline.as_ref().unwrap(),
        );
        let shutdown = fixture.shutdown();
        result?;
        shutdown
    }

    fn shutdown(&mut self) -> Result<()> {
        let channel = if let Some(mut controller) = self.controller.take() {
            controller.shutdown()
        } else {
            Ok(())
        };
        let pipeline = if let Some(pipeline) = self.pipeline.take() {
            pipeline.shutdown()
        } else {
            Ok(())
        };
        channel?;
        pipeline
    }
}

impl Drop for Fixture {
    fn drop(&mut self) {
        let _ = self.shutdown();
    }
}

fn open(mut opening: JournalOpening, pipeline: &CandidatePipeline) -> Result<ValidatorJournal> {
    let deadline = Instant::now() + DEADLINE;
    loop {
        ensure!(Instant::now() < deadline, "journal opening timed out");
        if let Some(journal) = opening.poll(pipeline)? {
            return Ok(journal);
        }
        std::thread::yield_now();
    }
}

fn flush(controller: &mut Controller) -> Result<()> {
    let deadline = Instant::now() + DEADLINE;
    while !controller.retired.is_empty() {
        ensure!(Instant::now() < deadline, "retirement timed out");
        controller.flush_retired()?;
        std::thread::yield_now();
    }
    Ok(())
}

fn owner_ready(controller: &mut Controller, message: Arc<Message>) -> Result<Ready> {
    let token = controller.allocate_token()?;
    let mut request = PrepareRequest {
        token,
        input: PrepareInput::New(message),
    };
    let deadline = Instant::now() + DEADLINE;
    loop {
        ensure!(Instant::now() < deadline, "owner admission timed out");
        match controller.channel.try_prepare(request)? {
            PrepareAdmission::Accepted => break,
            PrepareAdmission::Backpressure(returned) => request = returned,
            PrepareAdmission::Rejected { reason, .. } => bail!(reason),
        }
        flush(controller)?;
        std::thread::yield_now();
    }
    loop {
        ensure!(Instant::now() < deadline, "owner reply timed out");
        match controller.channel.try_recv()? {
            Some(ChannelEvent::Prepared {
                token: found,
                result,
            }) if found == token => return result.map_err(anyhow::Error::msg),
            Some(ChannelEvent::Prepared { token, result }) => controller.prepared(token, result)?,
            Some(ChannelEvent::Received(received)) => {
                controller.receive(received.peer, received.ready)?
            }
            None => std::thread::yield_now(),
        }
        flush(controller)?;
    }
}

fn pump_owner(controller: &mut Controller, done: impl Fn(&Controller) -> bool) -> Result<()> {
    let deadline = Instant::now() + DEADLINE;
    while !done(controller) {
        ensure!(
            Instant::now() < deadline,
            "controller assembly reply timed out"
        );
        controller.flush_preparations()?;
        match controller.channel.try_recv()? {
            Some(ChannelEvent::Prepared { token, result }) => controller.prepared(token, result)?,
            Some(ChannelEvent::Received(received)) => {
                controller.receive(received.peer, received.ready)?
            }
            None => std::thread::yield_now(),
        }
        flush(controller)?;
    }
    Ok(())
}

fn submit(pipeline: &CandidatePipeline, mut request: BatchRequest) -> Result<PipelineTicket> {
    let deadline = Instant::now() + DEADLINE;
    loop {
        ensure!(Instant::now() < deadline, "ordinary admission timed out");
        match pipeline
            .try_submit_owned(request)
            .map_err(|rejected| rejected.error)?
        {
            Submission::Accepted(ticket) => return Ok(ticket),
            Submission::Backpressured(returned) => request = returned,
        }
        std::thread::yield_now();
    }
}

fn wait_candidate(controller: &mut Controller, id: Hash) -> Result<DurableCandidate> {
    let deadline = Instant::now() + DEADLINE;
    loop {
        ensure!(
            Instant::now() < deadline,
            "real execution timed out: {:?}",
            controller.stats()
        );
        controller.poll_executions()?;
        if let Some((_, candidate)) = controller
            .bodies
            .get(&id)
            .and_then(|body| body.candidate.as_ref())
        {
            return Ok(candidate.clone());
        }
        flush(controller)?;
        std::thread::yield_now();
    }
}

/// Seed actual local parent execution without injecting a synthetic completion.
/// Admission is explicit to isolate successor behavior from quorum arrival.
fn executed_parent(
    controller: &mut Controller,
    pipeline: &CandidatePipeline,
    extra: u128,
    source: String,
) -> Result<DurableCandidate> {
    let ready = owner_ready(controller, body(controller.parent(), extra)?)?;
    let id = ready
        .prepared
        .body_id()
        .context("owner body identity absent")?;
    controller.keep_body(source.clone(), ready, None)?;
    let request = controller
        .bodies
        .get_mut(&id)
        .context("parent body absent")?
        .request
        .take()
        .context("parent request absent")?;
    let ticket = submit(pipeline, request)?;
    controller.inflight.push(Execution {
        id,
        context: controller.context(),
        parent: controller.parent(),
        requester: source,
        ticket,
        recovery: None,
    });
    let candidate = wait_candidate(controller, id)?;
    ensure!(
        controller.stats.execution_peak_callbacks > 0,
        "no actual AOEM callback"
    );
    flush(controller)?;
    Ok(candidate)
}

fn point(controller: &Controller, candidate: &DurableCandidate) -> Result<ParentPoint> {
    let statement = BlockStatement::from_executed(
        candidate.packet(),
        controller.context(),
        &controller.config.validators,
        &controller.parent(),
    )?;
    Ok(ParentPoint {
        height: controller.context().height,
        block_hash: statement.hash(),
        state_root: candidate.packet().state_root(),
        receipt_batch_commitment: candidate.packet().receipt_batch_commitment(),
        state_version: statement.state_version(),
        decision_hash: crate::native_pipeline::consensus::chain::decision_id(
            &controller.context(),
            statement.hash(),
        ),
    })
}

fn start_successor(
    controller: &mut Controller,
    pipeline: &CandidatePipeline,
    message: &Arc<Message>,
    completed: bool,
) -> Result<(Pin, Hash)> {
    flush(controller)?;
    ensure!(
        controller.try_submit_successor_body(message)?,
        "successor owner admission refused"
    );
    pump_owner(controller, |c| {
        c.successor.as_ref().is_some_and(|s| s.body.is_some())
    })?;
    let deadline = Instant::now() + DEADLINE;
    loop {
        ensure!(
            Instant::now() < deadline,
            "background execution timed out: {:?}",
            controller.stats()
        );
        controller.poll_successor(pipeline)?;
        let successor = controller
            .successor
            .as_ref()
            .context("successor vanished")?;
        let ready = if completed {
            successor
                .body
                .as_ref()
                .is_some_and(|body| body.candidate.is_some())
        } else {
            successor.ticket.is_some()
        };
        if ready {
            return Ok((
                successor.basis.pin,
                successor
                    .body
                    .as_ref()
                    .unwrap()
                    .prepared
                    .body_id()
                    .context("successor body ID absent")?,
            ));
        }
        flush(controller)?;
        std::thread::yield_now();
    }
}

fn acknowledge(controller: &mut Controller, pipeline: &CandidatePipeline) -> Result<()> {
    let deadline = Instant::now() + DEADLINE;
    ensure!(
        controller.journal.is_pending(),
        "no actual metadata transition"
    );
    while controller.journal.is_pending() {
        ensure!(Instant::now() < deadline, "actual metadata ACK timed out");
        controller.poll_journal(pipeline)?;
        controller.flush_preparations()?;
        flush(controller)?;
        std::thread::yield_now();
    }
    Ok(())
}

fn decide_and_advance(
    controller: &mut Controller,
    pipeline: &CandidatePipeline,
    candidate: &DurableCandidate,
) -> Result<ParentPoint> {
    let expected = point(controller, candidate)?;
    let set = controller.config.validators.clone();
    let round = controller.round();
    let leader = index_for(&set, set.leader(controller.context().height, round)?);
    let proposal = novovm_consensus::round_bft::test_vectors::sign_proposal(
        controller.context(),
        round,
        expected.block_hash,
        None,
        &set,
        &key(leader),
    )?
    .verify(&set)?;
    let votes = (0..3)
        .map(|i| {
            novovm_consensus::round_bft::test_vectors::sign_vote(
                controller.context(),
                round,
                Phase::Precommit,
                Some(expected.block_hash),
                &set,
                &key(i),
            )
        })
        .collect::<Result<Vec<_>>>()?;
    let certificate = Quorum::from_votes(&set, votes)?.verify(&set)?;
    controller
        .journal
        .observe_decision(&proposal, candidate, &certificate)?;
    ensure!(
        controller.head() != Some(expected),
        "staged decision was already a head"
    );
    acknowledge(controller, pipeline)?;
    ensure!(
        controller.head() == Some(expected) && controller.context().height == expected.height,
        "decision ACK skipped or prematurely advanced"
    );
    controller.journal.advance_height()?;
    acknowledge(controller, pipeline)?;
    ensure!(
        controller.parent() == expected && controller.context().height == expected.height + 1,
        "advance ACK differs from exact decided parent"
    );
    Ok(expected)
}

fn drain(controller: &mut Controller) -> Result<()> {
    let deadline = Instant::now() + DEADLINE;
    while controller.successor_drain.is_some() {
        ensure!(Instant::now() < deadline, "retired successor did not drain");
        controller.drain_successor()?;
        flush(controller)?;
        std::thread::yield_now();
    }
    Ok(())
}

fn read(pipeline: &CandidatePipeline, root: Hash, key: Vec<u8>) -> Result<Vec<u8>> {
    let deadline = Instant::now() + DEADLINE;
    let mut ticket = loop {
        ensure!(Instant::now() < deadline, "point read admission timed out");
        if let Some(ticket) = pipeline.try_read_value(root, key.clone())? {
            break ticket;
        }
        std::thread::yield_now();
    };
    loop {
        ensure!(Instant::now() < deadline, "point read timed out");
        if let Some(value) = ticket.try_take()? {
            return value.context("expected state record absent");
        }
        std::thread::yield_now();
    }
}

#[test]
#[ignore = "requires explicit real AOEM; early successor completion, exact ACK promotion and ordinary execution oracle"]
fn real_early_completion_has_no_future_authority_then_exact_parent_promotes() -> Result<()> {
    Fixture::run("early-promotion", |controller, pipeline| {
        let original_context = controller.context();
        let original_parent = controller.parent();
        let parent = executed_parent(controller, pipeline, 0, controller.local_peer.clone())?;
        let expected_parent = point(controller, &parent)?;
        let message = body(expected_parent, 0)?;
        let (pin, id) = start_successor(controller, pipeline, &message, true)?;
        let child = controller
            .successor
            .as_ref()
            .unwrap()
            .body
            .as_ref()
            .unwrap()
            .candidate
            .as_ref()
            .unwrap()
            .1
            .clone();
        ensure!(
            controller.stats.successor_completed_before_parent == 1
                && controller.stats.executed_batches == 2,
            "did not observe real early completion"
        );
        ensure!(
            !controller.bodies.contains_key(&id),
            "future candidate entered current body table before parent ACK"
        );
        controller.drive_consensus()?;
        controller.poll_journal(pipeline)?;
        ensure!(
            controller.context() == original_context
                && controller.parent() == original_parent
                && controller.head().is_none()
                && !controller.is_pending()
                && controller.journal.last_durable_message().is_none(),
            "early candidate caused a proposal/vote/head"
        );
        ensure!(
            controller.journal.propose(&child, None).is_err(),
            "journal signed a future completion against the old parent"
        );
        let local = index_for(
            &controller.config.validators,
            controller.config.local_validator,
        );
        let reopened = open(
            ValidatorJournal::open(
                pipeline,
                original_context,
                original_parent,
                controller.config.validators.clone(),
                key(local),
            )?,
            pipeline,
        )?;
        ensure!(
            reopened.head().is_none() && reopened.last_durable_message().is_none(),
            "early completion wrote durable authority"
        );
        drop(reopened);
        ensure!(
            decide_and_advance(controller, pipeline, &parent)? == pin.point,
            "parent point changed"
        );
        ensure!(
            controller.successor.is_none()
                && controller.successor_drain.is_none()
                && controller.stats.successor_reused == 1,
            "exact parent did not promote completed successor"
        );
        let promoted = controller
            .bodies
            .get(&id)
            .and_then(|b| b.candidate.as_ref())
            .context("promoted candidate absent")?;
        ensure!(
            promoted.1.packet().records() == child.packet().records(),
            "promotion changed immutable executed records"
        );
        controller.drive_consensus()?;
        ensure!(
            controller.is_pending() && controller.head() == Some(pin.point),
            "promoted successor did not re-enter persist-before-sign"
        );
        acknowledge(controller, pipeline)?;
        let Some(DurableMessage::Proposal(proposal)) = controller.journal.last_durable_message()
        else {
            bail!("promoted candidate did not propose after ACK");
        };
        ensure!(
            proposal.context == pin.next && controller.head() == Some(pin.point),
            "successor proposal published a head or wrong context"
        );
        // Independent ordinary admission executes the same signed next-height
        // batch again; matching immutable records is not a cached-read oracle.
        let mut oracle = submit(
            pipeline,
            BatchRequest::new(raw(2, 0)?, execution(pin.point), policy())?,
        )?;
        let deadline = Instant::now() + DEADLINE;
        let oracle = loop {
            ensure!(Instant::now() < deadline, "ordinary oracle timed out");
            if let Some(batch) = oracle.try_take()? {
                break batch;
            }
            std::thread::yield_now();
        };
        ensure!(
            oracle.observation.peak_callbacks > 0
                && oracle.packet.records() == child.packet().records(),
            "speculative and ordinary execution differ"
        );
        ensure!(
            read(
                pipeline,
                child.packet().state_root(),
                balance_key(&account(2))
            )? == 300u128.to_le_bytes(),
            "recipient differs after two heights"
        );
        for raw in raw(2, 0)? {
            let authenticated = authenticate_transfer_v3(&raw, CHAIN, 1024)?;
            ensure!(
                read(
                    pipeline,
                    child.packet().state_root(),
                    nonce_key(&authenticated.nonce_identity())
                )? == 2u64.to_le_bytes(),
                "nonce differs after two heights"
            );
        }
        Ok(())
    })
}

#[test]
#[ignore = "requires explicit real AOEM; completed and pending successors of a different winning parent are discarded"]
fn real_different_parent_winner_discards_completed_and_late_successor() -> Result<()> {
    for completed in [true, false] {
        Fixture::run(
            if completed {
                "loser-completed"
            } else {
                "loser-late"
            },
            |controller, pipeline| {
                let loser =
                    executed_parent(controller, pipeline, 0, controller.local_peer.clone())?;
                let loser_point = point(controller, &loser)?;
                let source = controller.config.peers.values().next().unwrap().clone();
                let winner = executed_parent(controller, pipeline, 1, source)?;
                ensure!(
                    loser.packet().state_root() != winner.packet().state_root(),
                    "competing parents are not distinct real executions"
                );
                let (_, id) =
                    start_successor(controller, pipeline, &body(loser_point, 0)?, completed)?;
                let winner_point = decide_and_advance(controller, pipeline, &winner)?;
                ensure!(
                    winner_point != loser_point
                        && controller.successor.is_none()
                        && !controller.bodies.contains_key(&id)
                        && controller.stats.successor_reused == 0
                        && controller.stats.successor_discarded == 1,
                    "losing successor crossed parent adoption"
                );
                ensure!(
                    controller.successor_drain.is_some() != completed,
                    "unconsumed real ticket was lost or invented"
                );
                drain(controller)?;
                ensure!(
                    !controller.bodies.contains_key(&id)
                        && controller.inflight.is_empty()
                        && controller.head() == Some(winner_point),
                    "late loser attached to the new height"
                );
                let ready = owner_ready(controller, body(winner_point, 0)?)?;
                let current = ready
                    .prepared
                    .body_id()
                    .context("winner child identity absent")?;
                controller.keep_body(controller.local_peer.clone(), ready, Some(0))?;
                flush(controller)?;
                controller.submit_executions(pipeline)?;
                let child = wait_candidate(controller, current)?;
                ensure!(
                    child.packet().context().parent_block_hash == winner_point.block_hash
                        && controller.head() == Some(winner_point),
                    "ordinary winning-parent child failed after discard"
                );
                Ok(())
            },
        )?;
    }
    Ok(())
}

#[test]
#[ignore = "requires explicit real AOEM; round retirement retains ticket quota without blocking requester or resurrecting same body"]
fn real_round_discard_drain_does_not_block_requester_or_revive_same_id() -> Result<()> {
    Fixture::run("round-drain", |controller, pipeline| {
        let parent = executed_parent(controller, pipeline, 0, controller.local_peer.clone())?;
        let parent_point = point(controller, &parent)?;
        let successor_body = body(parent_point, 0)?;
        let (_, old_id) = start_successor(controller, pipeline, &successor_body, false)?;
        novovm_consensus::round_bft::test_vectors::round_wait_elapsed(&mut controller.journal, 0)?;
        acknowledge(controller, pipeline)?;
        ensure!(
            controller.round() == 1
                && controller.is_local_leader()?
                && controller.successor.is_none()
                && controller.successor_drain.is_some()
                && controller.inflight.is_empty()
                && controller.inflight_count() == 1,
            "round change failed to isolate the real retired ticket"
        );
        let ready = owner_ready(controller, body(controller.parent(), 1)?)?;
        let current_id = ready.prepared.body_id().context("round-one body absent")?;
        controller.keep_body(controller.local_peer.clone(), ready, Some(1))?;
        flush(controller)?;
        controller.submit_executions(pipeline)?;
        ensure!(
            controller.successor_drain.is_some()
                && controller
                    .inflight
                    .iter()
                    .any(|work| work.id == current_id && work.requester == controller.local_peer)
                && controller.inflight_count() == 2,
            "retired future leader ticket occupied current requester/reserved slot"
        );
        let _ = wait_candidate(controller, current_id)?;
        // The original parent may still win in the new round. Execute its
        // exact body again through the ordinary owner after round retirement;
        // do not fabricate/pin a private candidate back into the controller.
        let source = controller.config.peers.values().next().unwrap().clone();
        let same_parent = executed_parent(controller, pipeline, 0, source)?;
        ensure!(
            same_parent.packet().candidate_id() == parent.packet().candidate_id(),
            "same-parent fixture changed identity"
        );
        ensure!(
            decide_and_advance(controller, pipeline, &same_parent)? == parent_point,
            "round-independent parent identity changed"
        );
        let ready = owner_ready(controller, successor_body)?;
        ensure!(
            ready.prepared.body_id() == Some(old_id),
            "late same-ID fixture changed body"
        );
        controller.keep_body(controller.local_peer.clone(), ready, Some(0))?;
        ensure!(
            controller.successor_drain.is_some(),
            "fixture consumed the abandoned reply too early"
        );
        let before = controller.stats.executed_batches;
        drain(controller)?;
        let current = controller
            .bodies
            .get(&old_id)
            .context("new current body vanished")?;
        ensure!(
            current.candidate.is_none()
                && current.request.is_some()
                && controller.inflight.is_empty()
                && controller.stats.executed_batches == before + 1
                && controller.stats.stale_results >= 1,
            "discarded late reply revived a now-matching current body"
        );
        flush(controller)?;
        controller.submit_executions(pipeline)?;
        let _ = wait_candidate(controller, old_id)?;
        ensure!(
            controller.stats.executed_batches == before + 2
                && controller.head() == Some(parent_point),
            "fresh current execution was skipped or head changed"
        );
        Ok(())
    })
}

#[test]
#[ignore = "requires explicit real AOEM; assembly reply arriving after parent ACK rejoins ordinary current execution"]
fn real_body_preparation_reply_after_parent_advance_uses_current_path_once() -> Result<()> {
    Fixture::run("late-body", |controller, pipeline| {
        let parent = executed_parent(controller, pipeline, 0, controller.local_peer.clone())?;
        let parent_point = point(controller, &parent)?;
        flush(controller)?;
        ensure!(
            controller.try_submit_successor_body(&body(parent_point, 0)?)?,
            "owner did not accept successor body"
        );
        ensure!(
            controller
                .successor
                .as_ref()
                .is_some_and(|s| s.preparing && s.body.is_none()),
            "fixture consumed assembly reply prematurely"
        );
        decide_and_advance(controller, pipeline, &parent)?;
        ensure!(
            controller.successor.is_none() && controller.stats.successor_started == 0,
            "bodyless successor executed or survived promotion"
        );
        pump_owner(controller, |c| {
            c.bodies.values().any(|body| body.local_round == Some(0))
        })?;
        ensure!(
            controller.bodies.len() == 1,
            "late owner reply created duplicate current bodies"
        );
        let id = *controller.bodies.keys().next().unwrap();
        ensure!(
            controller.bodies[&id].candidate.is_none() && controller.bodies[&id].request.is_some(),
            "late encoding reply forged execution"
        );
        controller.submit_executions(pipeline)?;
        let _ = wait_candidate(controller, id)?;
        ensure!(
            controller.stats.executed_batches == 2
                && controller.stats.successor_started == 0
                && controller.head() == Some(parent_point),
            "late body skipped/doubled execution or published future head"
        );
        Ok(())
    })
}

#[test]
#[ignore = "requires explicit real AOEM; legal tight body budget preempts speculative body and fixed cache for current input"]
fn real_current_body_preempts_successor_and_shared_cache_at_valid_byte_ceiling() -> Result<()> {
    Fixture::run("body-budget", |controller, pipeline| {
        let parent = executed_parent(controller, pipeline, 0, controller.local_peer.clone())?;
        let parent_point = point(controller, &parent)?;
        let (_, future_id) = start_successor(controller, pipeline, &body(parent_point, 0)?, false)?;
        let future_fragment = controller
            .successor
            .as_ref()
            .unwrap()
            .body
            .as_ref()
            .unwrap()
            .prepared
            .fragment_id();
        ensure!(
            controller
                .fixed
                .iter()
                .any(|fixed| fixed.prepared.fragment_id() == future_fragment),
            "fixture lacks shared future broadcast cache"
        );
        // Keep the constructor's full legal reserve, not an invalid two-body
        // configuration. Genuine distinct owner-prepared fixed bodies fill
        // the other slots; the incoming current body uses a different source
        // so keep_body cannot make room by replacing the actual parent A.
        let reserved = controller.config.validators.members().len() * 2
            + controller.config.limits.max_inflight
            + 2;
        let charge = controller.channel.preparation_charge();
        controller.config.limits.max_body_bytes = reserved * charge;
        validate_config(&controller.config, &controller.journal)?;
        ensure!(
            controller.config.limits.max_body_bytes >= charge * reserved,
            "fixture violates constructor byte reserve"
        );
        for extra in 2..reserved {
            let message = body(controller.parent(), extra as u128)?;
            let ready = owner_ready(controller, message)?;
            ensure!(
                controller.cache(ready.prepared.clone(), None),
                "valid fixed-body fill rejected"
            );
            controller.retire(Retirement::Ready(ready));
            flush(controller)?;
        }
        ensure!(
            controller.body_bytes() == controller.config.limits.max_body_bytes,
            "fixture did not fill the exact legal byte ceiling"
        );
        let ready = owner_ready(controller, body(controller.parent(), 1)?)?;
        let current_id = ready
            .prepared
            .body_id()
            .context("current body identity absent")?;
        let source = controller.config.peers.values().next().unwrap().clone();
        ensure!(
            ready.prepared.retained_bytes()
                > controller
                    .config
                    .limits
                    .max_body_bytes
                    .saturating_sub(controller.body_bytes()),
            "fixture is not byte constrained"
        );
        controller.keep_body(source, ready, None)?;
        ensure!(
            controller.bodies.contains_key(&current_id)
                && controller.bodies.values().any(|body| body
                    .candidate
                    .as_ref()
                    .is_some_and(|(_, candidate)| candidate.packet().candidate_id()
                        == parent.packet().candidate_id())),
            "current body rejected or durable parent evicted instead of speculation"
        );
        ensure!(
            controller.successor.is_none()
                && controller.successor_drain.is_some()
                && !controller.bodies.contains_key(&future_id)
                && controller
                    .fixed
                    .iter()
                    .all(|fixed| fixed.prepared.fragment_id() != future_fragment)
                && controller.body_bytes() <= controller.config.limits.max_body_bytes,
            "preemption left speculative content/cache or lost accepted native ticket"
        );
        drain(controller)?;
        ensure!(
            controller.bodies[&current_id].request.is_some()
                && controller.bodies[&current_id].candidate.is_none(),
            "retired background reply attached to current competing body"
        );
        Ok(())
    })
}

#[test]
#[ignore = "requires explicit real AOEM; actual database close/reopen preserves orphan content without successor or signing capability"]
fn real_orphan_survives_database_reopen_but_not_execution_or_signing_authority() -> Result<()> {
    let mut fixture = Fixture::new("orphan-reopen")?;
    let controller = fixture.controller.as_mut().unwrap();
    let pipeline = fixture.pipeline.as_ref().unwrap();
    let context = controller.context();
    let anchor = controller.parent();
    let parent = executed_parent(controller, pipeline, 0, controller.local_peer.clone())?;
    let parent_point = point(controller, &parent)?;
    let (_, child_body_id) = start_successor(controller, pipeline, &body(parent_point, 0)?, true)?;
    let orphan = controller
        .successor
        .as_ref()
        .unwrap()
        .body
        .as_ref()
        .unwrap()
        .candidate
        .as_ref()
        .unwrap()
        .1
        .clone();
    let candidate_id = orphan.packet().candidate_id();
    let orphan_root = orphan.packet().state_root();
    let config = ControllerConfig {
        validators: controller.config.validators.clone(),
        local_validator: controller.config.local_validator,
        peers: controller.config.peers.clone(),
        execution: controller.config.execution,
        collector: controller.config.collector,
        timeouts: controller.config.timeouts,
        limits: controller.config.limits,
        retransmit: controller.config.retransmit,
    };
    let local = index_for(&config.validators, config.local_validator);
    ensure!(
        controller.head().is_none() && controller.journal.last_durable_message().is_none(),
        "orphan fixture already finalized/signed"
    );
    // Both resident native owners and the actual database are closed here.
    // Keeping immutable candidates retains no database or live owner session.
    fixture.shutdown()?;
    fixture.pipeline = Some(CandidatePipeline::start(
        fixture.pipeline_config.clone(),
        OpenMode::Existing,
    )?);
    let pipeline = fixture.pipeline.as_ref().unwrap();
    let journal = open(
        ValidatorJournal::open(
            pipeline,
            context,
            anchor,
            config.validators.clone(),
            key(local),
        )?,
        pipeline,
    )?;
    ensure!(
        journal.head().is_none() && journal.last_durable_message().is_none(),
        "reopening adopted speculative content as durable authority"
    );
    let channel = channel(&config, local)?;
    fixture.controller = Some(Controller::new(config, journal, channel)?);
    let controller = fixture.controller.as_mut().unwrap();
    ensure!(
        controller.successor.is_none()
            && controller.successor_drain.is_none()
            && controller.bodies.is_empty()
            && controller.inflight.is_empty()
            && !controller.is_recovering(),
        "restart restored cached future execution permission"
    );
    let deadline = Instant::now() + DEADLINE;
    let mut recovered = loop {
        ensure!(
            Instant::now() < deadline,
            "orphan recovery admission timed out"
        );
        if let Some(ticket) = pipeline.try_recover_consensus_candidate(candidate_id)? {
            break ticket;
        }
        std::thread::yield_now();
    };
    let stored = loop {
        ensure!(Instant::now() < deadline, "orphan content read timed out");
        if let Some(candidate) = recovered.try_take()? {
            break candidate.context("actual persisted orphan missing after database reopen")?;
        }
        std::thread::yield_now();
    };
    ensure!(
        stored.candidate_id() == candidate_id
            && stored.state_root() == orphan_root
            && stored.context().parent_block_hash == parent_point.block_hash,
        "reopened orphan content differs"
    );
    ensure!(
        controller.journal.propose(&orphan, None).is_err(),
        "new owner accepted old speculative capability"
    );
    let fresh_parent = executed_parent(controller, pipeline, 0, controller.local_peer.clone())?;
    ensure!(
        decide_and_advance(controller, pipeline, &fresh_parent)? == parent_point,
        "fresh legal parent could not finalize after orphan restart"
    );
    ensure!(
        !controller.bodies.contains_key(&child_body_id)
            && controller.successor.is_none()
            && controller.head() == Some(parent_point)
            && controller.journal.propose(&orphan, None).is_err(),
        "matching final parent revived old-owner orphan authority without fresh execution"
    );
    fixture.shutdown()
}

mod early;
mod early_limits;
