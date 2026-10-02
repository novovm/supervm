//! Four actual OS processes, each owning its network, AOEM sessions and signer.
//! The parent only injects a fixture workload and starts/stops processes; it
//! never constructs a proposal, a vote, a QC, or chooses a decided candidate.
//! This is NOT four machines, a production launcher, or a throughput benchmark.

use super::network_integration::Relay;
use super::*;
use crate::business::nov_transfer_batch::nonce_key;
use crate::consensus::channel::{ChannelConfig, HostChannel, LaneBudget, QueueBudget};
use crate::consensus::collector::CollectorLimits;
use crate::consensus::controller::{Controller, ControllerConfig, ControllerLimits};
use crate::consensus::pacemaker::TimeoutPolicy;
use crate::consensus::transport::{DecodeLimits, Message};
use crate::consensus::{ArchiveBlock, ArchiveRead};
use crate::ingress::authentication::authenticate_transfer_v3;
use novovm_network::fragments::ReassemblyLimits;
use novovm_network::peer_id_from_ed25519_public_key_v1;
use novovm_network::product_relay_client::{ProductRelayClientConfigV1, ProductRelayTlsTrustV1};
use novovm_network::worker::{NetworkWorker, NetworkWorkerConfig, WorkerLimits};
use serde::{Deserialize, Serialize};
use std::process::{Child, Command, Stdio};

const TEST_NAME: &str = "consensus::tests::controller_integration::real_four_process_controllers_finalize_and_late_join_from_archive";
const CHILD_CONFIG: &str = "NOVOVM_CONTROLLER_TEST_CONFIG";
const CHILD_MODE: &str = "NOVOVM_CONTROLLER_TEST_MODE";
const HEIGHTS: u64 = 3;
const RUN_BUDGET: Duration = Duration::from_secs(120);

#[derive(Clone, Serialize, Deserialize)]
struct Fixture {
    index: usize,
    directory: PathBuf,
    ledger: PathBuf,
    endpoint: String,
    certificate: PathBuf,
    root: Hash,
}

#[derive(Serialize, Deserialize)]
struct Progress {
    pid: u32,
    height: u64,
    round: u64,
    prevote_weight: u64,
    precommit_weight: u64,
    current_height: u64,
    pending: bool,
    step: String,
    local_leader: bool,
    details: String,
    executed_batches: u64,
    durable_decisions: u64,
    head: Option<ParentPoint>,
    last_error: Option<String>,
}

#[derive(Debug, Serialize, Deserialize)]
struct Recovered {
    pid: u32,
    head: ParentPoint,
    candidates: Vec<Hash>,
    receipts: Vec<Hash>,
    recipient: u128,
}

fn validator_set() -> Result<Arc<ValidatorSet>> {
    Ok(Arc::new(ValidatorSet::new(
        CHAIN,
        1,
        1,
        (0..4)
            .map(|i| Validator::new(validator_key(i).verifying_key().to_bytes(), 1))
            .collect::<Result<Vec<_>>>()?,
    )?))
}

fn peer(index: usize) -> String {
    peer_id_from_ed25519_public_key_v1(&validator_key(index).verifying_key().to_bytes())
}

fn anchor(root: Hash, set: &ValidatorSet) -> (ConsensusContext, ParentPoint) {
    (
        ConsensusContext {
            chain_id: CHAIN,
            genesis_config_commitment: GENESIS,
            protocol_commitment: PROTOCOL,
            epoch: 1,
            validator_set_hash: set.hash(),
            height: 1,
            parent_block_hash: [0; 32],
            parent_decision_hash: [0; 32],
        },
        ParentPoint {
            height: 0,
            block_hash: [0; 32],
            state_root: root,
            receipt_batch_commitment: empty_root(),
            state_version: 0,
            decision_hash: [0; 32],
        },
    )
}

fn raw_height(height: u64) -> Result<Vec<Vec<u8>>> {
    [(1, if height == 2 { 2_000_000 } else { 100 }), (3, 50)]
        .into_iter()
        .map(|(seed, amount)| {
            let key = SigningKey::from_bytes(&[seed; 32]);
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
            let signature = key.sign(&signing_message(&tx)?);
            tx.signature = key.verifying_key().to_bytes().to_vec();
            tx.signature.extend_from_slice(&signature.to_bytes());
            encode_transfer_v3(&tx)
        })
        .collect()
}

fn channel(fixture: &Fixture, set: Arc<ValidatorSet>) -> Result<HostChannel> {
    let peers: Vec<_> = (0..4).filter(|i| *i != fixture.index).map(peer).collect();
    let relay = SigningKey::from_bytes(&[91; 32]);
    let network = NetworkWorker::start(
        NetworkWorkerConfig {
            chain_id: CHAIN,
            relay: ProductRelayClientConfigV1 {
                endpoint: fixture.endpoint.clone(),
                expected_relay_peer_id: peer_id_from_ed25519_public_key_v1(
                    &relay.verifying_key().to_bytes(),
                ),
                connect_timeout_ms: 2_000,
                read_timeout_ms: 10,
                tls_trust: ProductRelayTlsTrustV1::ExplicitCa {
                    certificate_path: fixture.certificate.clone(),
                },
            },
            peers: peers.clone(),
            limits: WorkerLimits::default(),
            handshake_timeout_ms: 5_000,
            reconnect_delay_ms: 50,
            heartbeat_interval_ms: 1_000,
            queue_ttl_ms: 30_000,
        },
        validator_key(fixture.index),
    )?;
    let codec = DecodeLimits {
        transactions: 8,
        transaction_bytes: 1024,
        body_bytes: 8192,
        message_bytes: 512 * 1024,
    };
    let lane = LaneBudget {
        control: QueueBudget {
            messages: 32,
            bytes: 64 * 1024 * 1024,
        },
        body: QueueBudget {
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
            validators: set,
            policy: policy(),
            codec,
            reassembly: ReassemblyLimits {
                max_message_bytes: codec.message_bytes,
                messages: 32,
                bytes: 8 * 1024 * 1024,
                peer_messages: 8,
                peer_bytes: 2 * 1024 * 1024,
                ttl: Duration::from_secs(30),
            },
            prepare: lane,
            send: lane,
            receive: lane,
            ttl: Duration::from_secs(30),
        },
    )
}

fn read_value(pipeline: &CandidatePipeline, root: Hash, key: Vec<u8>) -> Result<Vec<u8>> {
    let deadline = Instant::now() + DEADLINE;
    let mut ticket = loop {
        ensure!(
            Instant::now() < deadline,
            "controller recovery query admission timeout"
        );
        if let Some(ticket) = pipeline.try_read_value(root, key.clone())? {
            break ticket;
        }
        std::thread::yield_now();
    };
    loop {
        ensure!(
            Instant::now() < deadline,
            "controller recovery query completion timeout"
        );
        if let Some(value) = ticket.try_take()? {
            return value.context("recovered value absent");
        }
        std::thread::yield_now();
    }
}

fn read_archive(
    pipeline: &CandidatePipeline,
    context: ConsensusContext,
    head: ParentPoint,
    height: u64,
    set: Arc<ValidatorSet>,
) -> Result<ArchiveBlock> {
    let mut reader = ArchiveRead::new(height, head, context, set)?;
    let deadline = Instant::now() + DEADLINE;
    loop {
        ensure!(
            Instant::now() < deadline,
            "controller archive read timed out"
        );
        if let Some(block) = reader.poll(pipeline)? {
            return block.context("decided archive missing");
        }
        std::thread::yield_now();
    }
}

fn recover(fixture: &Fixture) -> Result<()> {
    let set = validator_set()?;
    let (context, parent) = anchor(fixture.root, &set);
    let pipeline = CandidatePipeline::start(pipeline_config(&fixture.ledger)?, OpenMode::Existing)?;
    let result = (|| -> Result<Recovered> {
        let journal = open_journal(
            ValidatorJournal::open(
                &pipeline,
                context,
                parent,
                set.clone(),
                validator_key(fixture.index),
            )?,
            &pipeline,
        )?;
        let head = journal
            .head()
            .context("reopened process lacks durable head")?;
        ensure!(head.height == HEIGHTS, "wrong reopened chain height");
        let mut candidates = Vec::new();
        let mut receipts = Vec::new();
        for height in 1..=HEIGHTS {
            let block = read_archive(&pipeline, journal.context(), head, height, set.clone())?;
            ensure!(
                block.stored().raw_transactions() == raw_height(height)?,
                "archive differs from original signed transaction body"
            );
            block.proposal().verify(&set)?;
            let qc = block.certificate().verify(&set)?;
            ensure!(
                qc.signed_weight() >= 3 && qc.value() == Some(block.point().block_hash),
                "recovered block lacks independently verified quorum"
            );
            candidates.push(block.stored().candidate_id());
            let mut hash = Sha256::new();
            for receipt in block.stored().receipt_bytes() {
                hash.update((receipt.len() as u64).to_be_bytes());
                hash.update(receipt);
            }
            receipts.push(hash.finalize().into());
        }
        let recipient = u128::from_le_bytes(
            read_value(&pipeline, head.state_root, balance_key(&account(2)))?
                .try_into()
                .map_err(|_| anyhow::anyhow!("bad balance width"))?,
        );
        ensure!(
            recipient == 350,
            "successful-prefix balance differs after failed transfer"
        );
        for raw in raw_height(HEIGHTS)? {
            let authenticated = authenticate_transfer_v3(&raw, CHAIN, 1024)?;
            // The business nonce is the next admissible sequence, including
            // the second-height business failure; authentication failures differ.
            let saved = read_value(
                &pipeline,
                head.state_root,
                nonce_key(&authenticated.nonce_identity()),
            )?;
            ensure!(
                saved == HEIGHTS.to_le_bytes(),
                "durable nonce did not consume every admitted execution"
            );
        }
        Ok(Recovered {
            pid: std::process::id(),
            head,
            candidates,
            receipts,
            recipient,
        })
    })();
    pipeline.shutdown()?;
    let recovered = result?;
    fs::write(
        fixture
            .directory
            .join(format!("recovered-{}.json", fixture.index)),
        serde_json::to_vec(&recovered)?,
    )?;
    Ok(())
}

#[derive(Default)]
struct Children(Vec<Child>);
impl Drop for Children {
    fn drop(&mut self) {
        for child in &mut self.0 {
            let _ = child.kill();
            let _ = child.wait();
        }
    }
}

fn spawn(fixture: &Fixture, mode: &str) -> Result<Child> {
    let path = fixture
        .directory
        .join(format!("config-{}.json", fixture.index));
    fs::write(&path, serde_json::to_vec(fixture)?)?;
    let out = fs::File::create(
        fixture
            .directory
            .join(format!("{mode}-{}.stdout.log", fixture.index)),
    )?;
    let err = fs::File::create(
        fixture
            .directory
            .join(format!("{mode}-{}.stderr.log", fixture.index)),
    )?;
    Command::new(std::env::current_exe()?)
        .args([
            "--exact",
            TEST_NAME,
            "--ignored",
            "--nocapture",
            "--test-threads=1",
        ])
        .env(CHILD_CONFIG, path)
        .env(CHILD_MODE, mode)
        .stdout(Stdio::from(out))
        .stderr(Stdio::from(err))
        .spawn()
        .context("spawn independent controller fixture process")
}

fn progress(fixture: &Fixture) -> Option<Progress> {
    fs::read(
        fixture
            .directory
            .join(format!("progress-{}.json", fixture.index)),
    )
    .ok()
    .and_then(|bytes| serde_json::from_slice(&bytes).ok())
}

fn wait_for(
    children: &mut Children,
    deadline: Instant,
    mut condition: impl FnMut() -> bool,
) -> Result<()> {
    loop {
        if condition() {
            return Ok(());
        }
        for child in &mut children.0 {
            ensure!(
                child.try_wait()?.is_none(),
                "controller child {} exited early; inspect retained logs",
                child.id()
            );
        }
        ensure!(
            Instant::now() < deadline,
            "four-process controller condition timed out; inspect retained logs"
        );
        std::thread::sleep(Duration::from_millis(10));
    }
}

fn run_controller(fixture: &Fixture) -> Result<()> {
    let set = validator_set()?;
    let (context, parent) = anchor(fixture.root, &set);
    let pipeline = CandidatePipeline::start(pipeline_config(&fixture.ledger)?, OpenMode::Existing)?;
    let result = (|| -> Result<()> {
        let journal = open_journal(
            ValidatorJournal::open(
                &pipeline,
                context,
                parent,
                set.clone(),
                validator_key(fixture.index),
            )?,
            &pipeline,
        )?;
        let ids = (0..4)
            .map(|i| {
                Validator::new(validator_key(i).verifying_key().to_bytes(), 1)
                    .map(|validator| validator.id())
            })
            .collect::<Result<Vec<_>>>()?;
        let mut controller = Controller::new(
            ControllerConfig {
                validators: set.clone(),
                local_validator: ids[fixture.index],
                peers: (0..4)
                    .filter(|i| *i != fixture.index)
                    .map(|i| (ids[i], peer(i)))
                    .collect(),
                execution: batch_context(fixture.root),
                collector: CollectorLimits {
                    max_retained_rounds: 2,
                    max_future_round_span: 64,
                    max_votes: set.members().len() * 7,
                },
                timeouts: TimeoutPolicy {
                    propose: Duration::from_secs(5),
                    prevote: Duration::from_secs(5),
                    precommit: Duration::from_secs(5),
                    round_increment: Duration::from_secs(1),
                },
                limits: ControllerLimits::default(),
                retransmit: Duration::from_millis(100),
            },
            journal,
            channel(fixture, set)?,
        )?;
        let result = (|| -> Result<()> {
            let deadline = Instant::now() + RUN_BUDGET;
            let mut next_report = Instant::now();
            let mut offered = None;
            let mut input: Option<((u64, u64), Arc<Message>)> = None;
            loop {
                let now = Instant::now();
                ensure!(
                    now < deadline,
                    "controller lifetime exceeded; head={:?}, stats={:?}",
                    controller.head(),
                    controller.stats()
                );
                controller.poll(&pipeline, now)?;
                let current = (controller.context().height, controller.round());
                // This is workload injection only: all phase transitions,
                // signatures, quorum selection and archive requests are local
                // controller actions, without commands from the parent process.
                if current.0 <= HEIGHTS && controller.is_local_leader()? && offered != Some(current)
                {
                    if input
                        .as_ref()
                        .is_none_or(|(position, _)| *position != current)
                    {
                        let parent = controller.parent();
                        let mut context = batch_context(fixture.root);
                        context.height = current.0;
                        context.parent_height = parent.height;
                        context.parent_block_hash = parent.block_hash;
                        context.parent_state_root = parent.state_root;
                        context.parent_receipt_root = parent.receipt_batch_commitment;
                        context.parent_state_version = parent.state_version;
                        context.slot = current.0;
                        context.timestamp_unix_ms += current.0;
                        input = Some((
                            current,
                            Arc::new(Message::Body {
                                context,
                                raw_transactions: raw_height(current.0)?,
                            }),
                        ));
                    }
                    if controller.try_submit_body(&input.as_ref().unwrap().1)? {
                        offered = Some(current);
                        input = None;
                    }
                }
                if now >= next_report {
                    let stats = controller.stats();
                    let report = Progress {
                        pid: std::process::id(),
                        height: controller.head().map_or(0, |head| head.height),
                        round: controller.round(),
                        prevote_weight: stats.prevote_weight,
                        precommit_weight: stats.precommit_weight,
                        current_height: controller.context().height,
                        pending: controller.is_pending(),
                        step: format!("{:?}", controller.step()),
                        local_leader: controller.is_local_leader()?,
                        details: format!("{stats:?}"),
                        executed_batches: stats.executed_batches,
                        durable_decisions: stats.durable_decisions,
                        head: controller.head(),
                        last_error: stats.last_error.clone(),
                    };
                    fs::write(
                        fixture
                            .directory
                            .join(format!("progress-{}.json", fixture.index)),
                        serde_json::to_vec(&report)?,
                    )?;
                    if fixture.directory.join("stop").exists() {
                        break;
                    }
                    next_report = now + Duration::from_millis(50);
                }
                std::thread::sleep(Duration::from_millis(1));
            }
            ensure!(
                controller.head().is_some_and(|head| head.height == HEIGHTS),
                "controller stopped without every durable decision"
            );
            eprintln!(
                "validator={} pid={} stats={:?}",
                fixture.index,
                std::process::id(),
                controller.stats()
            );
            Ok(())
        })();
        controller.shutdown()?;
        result
    })();
    pipeline.shutdown()?;
    result
}

fn wait_exited(children: &mut Children) -> Result<()> {
    let deadline = Instant::now() + DEADLINE;
    loop {
        let mut remaining = false;
        for child in &mut children.0 {
            match child.try_wait()? {
                Some(status) => ensure!(
                    status.success(),
                    "child {} failed: {status}; inspect retained logs",
                    child.id()
                ),
                None => remaining = true,
            }
        }
        if !remaining {
            return Ok(());
        }
        ensure!(Instant::now() < deadline, "child shutdown/recovery timeout");
        std::thread::sleep(Duration::from_millis(10));
    }
}

#[test]
#[ignore = "requires explicit real AOEM library; four OS processes over WSS, not four machines or TPS"]
fn real_four_process_controllers_finalize_and_late_join_from_archive() -> Result<()> {
    let _ = library()?;
    if let Some(path) = std::env::var_os(CHILD_CONFIG) {
        let fixture: Fixture = serde_json::from_slice(&fs::read(path)?)?;
        return match std::env::var(CHILD_MODE)?.as_str() {
            "live" => run_controller(&fixture),
            "recover" => recover(&fixture),
            mode => bail!("unknown controller fixture child mode {mode}"),
        };
    }
    let repository = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .canonicalize()?;
    let artifacts = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../target/runtime-rebuild");
    fs::create_dir_all(&artifacts)?;
    ensure!(
        artifacts.canonicalize()?.starts_with(&repository),
        "test artifacts escaped repository"
    );
    let directory = artifacts.join(format!(
        "autonomous-controllers-{}-{}",
        std::process::id(),
        SystemTime::now().duration_since(UNIX_EPOCH)?.as_nanos()
    ));
    fs::create_dir(&directory)?;
    eprintln!("independent controller artifacts={}", directory.display());
    let mut relay = Relay::start(&directory.join("relay"))?;
    let mut fixtures = Vec::new();
    for index in 0..4 {
        let ledger = directory.join(format!("validator-{index}"));
        let root = initialize(&ledger)?;
        fixtures.push(Fixture {
            index,
            ledger,
            root,
            directory: directory.clone(),
            endpoint: relay.endpoint.clone(),
            certificate: relay.certificate.clone(),
        });
    }
    ensure!(
        fixtures
            .iter()
            .all(|fixture| fixture.root == fixtures[0].root),
        "fixture genesis roots differ"
    );
    let set = validator_set()?;
    let leader = set.leader(1, 0)?;
    let leader = (0..4)
        .find(|i| {
            Validator::new(validator_key(*i).verifying_key().to_bytes(), 1)
                .is_ok_and(|validator| validator.id() == leader)
        })
        .context("fixture leader missing")?;
    let mut order = vec![leader];
    order.extend((0..4).filter(|i| *i != leader));
    let mut children = Children::default();
    let deadline = Instant::now() + RUN_BUDGET;
    for index in &order[..2] {
        children.0.push(spawn(&fixtures[*index], "live")?);
    }
    let mut minority = Vec::new();
    wait_for(&mut children, deadline, || {
        let Some(reports) = order[..2]
            .iter()
            .map(|index| progress(&fixtures[*index]))
            .collect::<Option<Vec<_>>>()
        else {
            return false;
        };
        if reports
            .iter()
            .all(|report| report.height == 0 && report.prevote_weight == 2)
        {
            minority = reports;
            true
        } else {
            false
        }
    })?;
    // The two genuine processes have collected each other's durable votes,
    // but neither may publish a canonical head without the third validator.
    fs::write(
        directory.join("two-of-four-observed.json"),
        serde_json::to_vec(&minority)?,
    )?;
    children.0.push(spawn(&fixtures[order[2]], "live")?);
    wait_for(&mut children, deadline, || {
        order[..3]
            .iter()
            .all(|index| progress(&fixtures[*index]).is_some_and(|report| report.height == HEIGHTS))
    })?;
    // The fourth node has received nothing. All prior current-height bodies
    // have been retired by the leading controllers; replay must use AOEM's
    // immutable height archives and exact durable outbox, not a fixture cache.
    children.0.push(spawn(&fixtures[order[3]], "live")?);
    let mut reports = Vec::new();
    wait_for(&mut children, deadline, || {
        let Some(current) = fixtures.iter().map(progress).collect::<Option<Vec<_>>>() else {
            return false;
        };
        if current
            .iter()
            .all(|report| report.height == HEIGHTS && report.durable_decisions == HEIGHTS)
        {
            reports = current;
            true
        } else {
            false
        }
    })?;
    ensure!(
        reports
            .iter()
            .all(|report| report.head == reports[0].head && report.executed_batches >= HEIGHTS),
        "controllers did not independently execute and converge"
    );
    let pids: std::collections::BTreeSet<_> = reports.iter().map(|report| report.pid).collect();
    ensure!(
        pids.len() == 4 && !pids.contains(&std::process::id()),
        "validators were not independent processes"
    );
    fs::write(directory.join("stop"), b"fixture complete")?;
    wait_exited(&mut children)?;
    relay.shutdown()?;
    let mut recovery = Children::default();
    for fixture in &fixtures {
        recovery.0.push(spawn(fixture, "recover")?);
    }
    wait_exited(&mut recovery)?;
    let reopened = fixtures
        .iter()
        .map(|fixture| -> Result<Recovered> {
            Ok(serde_json::from_slice(&fs::read(
                directory.join(format!("recovered-{}.json", fixture.index)),
            )?)?)
        })
        .collect::<Result<Vec<_>>>()?;
    for value in &reopened {
        ensure!(
            value.head == reopened[0].head
                && value.candidates == reopened[0].candidates
                && value.receipts == reopened[0].receipts
                && value.recipient == 350,
            "independent restart changed head, body, receipt or balance"
        );
        ensure!(
            value.pid != std::process::id(),
            "recovery did not use a new process"
        );
    }
    eprintln!("four autonomous processes passed: 2/4 no head; 3 heights; 6 signed originals; late archive replay; four cold reopens. artifacts={}", directory.display());
    Ok(())
}
