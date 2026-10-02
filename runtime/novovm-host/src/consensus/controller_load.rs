//! Same controller, signed ingress, encrypted network, resident AOEM and journal
//! as the failure/recovery fixture. Finite continuously available backlog, not
//! an engine benchmark, RPC benchmark, four machines or production parameters.
use super::*;
use crate::consensus::tests::controller_workload::{Workload, BATCH_SIZES};
use std::sync::mpsc::{self, Receiver, SyncSender, TryRecvError};
use std::thread::JoinHandle;

#[path = "controller_load_recovery.rs"]
mod recovery;

pub(super) fn recover(fixture: &Fixture, spec: LoadSpec) -> Result<()> {
    recovery::recover(fixture, spec)
}

const LOAD_HEIGHTS: u64 = 8;
const LONG_LOAD_HEIGHTS: u64 = 64;

#[derive(Clone, Copy, Debug, Serialize, Deserialize)]
pub(super) struct LoadSpec {
    pub batch_size: usize,
    pub heights: u64,
    #[serde(default)]
    pub successor: bool,
}

fn pipeline_config_for(fixture: &Fixture) -> Result<PipelineConfig> {
    let spec = fixture.load.context("load configuration absent")?;
    ensure!(
        BATCH_SIZES.contains(&spec.batch_size)
            && [LOAD_HEIGHTS, LONG_LOAD_HEIGHTS].contains(&spec.heights),
        "unbounded or unknown load configuration"
    );
    let mut config = pipeline_config(&fixture.ledger)?;
    config.authentication.transactions = spec.batch_size;
    config.authentication.body_bytes = spec.batch_size * 1024;
    config.plan.transactions = spec.batch_size;
    config.plan.body_bytes = spec.batch_size * 1024;
    // Two account/nonce keys per sender, one shared recipient and the complete
    // fixed-size paged fee state. These are fixture admission ceilings, not a
    // change to the production packet, controller or consensus resource limits.
    config.plan.access_keys = 2 * spec.batch_size + 128;
    config.capture.keys = config.plan.access_keys;
    config.capture.nodes = 65_536;
    config.capture.bytes = 16 * 1024 * 1024;
    Ok(config)
}

enum WalletReply {
    Ready,
    Body((u64, u64), Arc<Message>),
}

/// Signing, whole-body cloning and destruction stay off the controller thread.
/// Only this bounded test wallet knows the future backlog; validators receive
/// no expected writes, receipt, proposal, certificate or publish permission.
struct Wallet {
    tx: Option<SyncSender<((u64, u64), BatchContext)>>,
    rx: Option<Receiver<Result<WalletReply>>>,
    join: Option<JoinHandle<()>>,
}

impl Wallet {
    fn start(spec: LoadSpec) -> Result<Self> {
        let (tx, requests) = mpsc::sync_channel::<((u64, u64), BatchContext)>(1);
        let (replies, rx) = mpsc::sync_channel(1);
        let join = std::thread::Builder::new()
            .name("fixture-signed-wallet".into())
            .spawn(move || {
                let prepared = (|| -> Result<Vec<Vec<Vec<u8>>>> {
                    let workload = Workload::new(spec.batch_size, spec.heights, policy())?;
                    (1..=spec.heights).map(|h| workload.raw_height(h)).collect()
                })();
                let raw = match prepared {
                    Ok(raw) => raw,
                    Err(error) => {
                        let _ = replies.send(Err(error));
                        return;
                    }
                };
                if replies.send(Ok(WalletReply::Ready)).is_err() {
                    return;
                }
                // Retain the previous result until replacing it on this owner;
                // dropping a stale reply on the control thread is then O(1).
                let mut last = None;
                while let Ok((position, context)) = requests.recv() {
                    let Some(body) = raw.get(position.0.saturating_sub(1) as usize) else {
                        let _ = replies.send(Err(anyhow::anyhow!("wallet height out of range")));
                        break;
                    };
                    let message = Arc::new(Message::Body {
                        context,
                        raw_transactions: body.clone(),
                    });
                    last = Some(message.clone());
                    if replies
                        .send(Ok(WalletReply::Body(position, message)))
                        .is_err()
                    {
                        break;
                    }
                }
                drop(last);
            })?;
        Ok(Self {
            tx: Some(tx),
            rx: Some(rx),
            join: Some(join),
        })
    }

    fn take(&self) -> Result<Option<WalletReply>> {
        match self.rx.as_ref().unwrap().try_recv() {
            Ok(reply) => reply.map(Some),
            Err(TryRecvError::Empty) => Ok(None),
            Err(TryRecvError::Disconnected) => bail!("wallet owner stopped"),
        }
    }
}

impl Drop for Wallet {
    fn drop(&mut self) {
        // Drop both directions before join: a failed fixture cannot strand a
        // producer waiting to return its one bounded reply.
        drop(self.tx.take());
        drop(self.rx.take());
        if let Some(join) = self.join.take() {
            let _ = join.join();
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
struct Observation {
    pid: u32,
    head: Option<ParentPoint>,
    current_height: u64,
    round: u64,
    executed_batches: u64,
    successor_started: u64,
    successor_completed_before_parent: u64,
    successor_reused: u64,
    successor_promoted_inflight: u64,
    successor_discarded: u64,
    execution_failures: u64,
    stale_results: u64,
    durable_decisions: u64,
    components: u64,
    credit_only_accounts: u64,
    recomputed_transactions: u64,
    peak_callbacks: usize,
    observation_saturated: bool,
    last_error: Option<String>,
}

impl Observation {
    fn from_controller(controller: &Controller) -> Self {
        let stats = controller.stats();
        Self {
            pid: std::process::id(),
            head: controller.head(),
            current_height: controller.context().height,
            round: controller.round(),
            executed_batches: stats.executed_batches,
            successor_started: stats.successor_started,
            successor_completed_before_parent: stats.successor_completed_before_parent,
            successor_reused: stats.successor_reused,
            successor_promoted_inflight: stats.successor_promoted_inflight,
            successor_discarded: stats.successor_discarded,
            execution_failures: stats.execution_failures,
            stale_results: stats.stale_results,
            durable_decisions: stats.durable_decisions,
            components: stats.execution_components_total,
            credit_only_accounts: stats.execution_credit_only_accounts_total,
            recomputed_transactions: stats.execution_recomputed_transactions_total,
            peak_callbacks: stats.execution_peak_callbacks,
            observation_saturated: stats.execution_observation_saturated,
            last_error: stats.last_error.clone(),
        }
    }
}

enum Event {
    Ready,
    Head(Observation),
    Heartbeat(Observation),
}

struct Observer {
    tx: Option<SyncSender<Event>>,
    join: Option<JoinHandle<Result<()>>>,
}

impl Observer {
    fn start(fixture: &Fixture) -> Result<Self> {
        let fixture = fixture.clone();
        let (tx, rx) = mpsc::sync_channel(64);
        let join = std::thread::Builder::new()
            .name("fixture-observer".into())
            .spawn(move || {
                while let Ok(event) = rx.recv() {
                    let (name, bytes) = match event {
                        Event::Ready => (format!("ready-{}", fixture.index), Vec::new()),
                        Event::Head(value) => (
                            format!(
                                "head-{}-{}.json",
                                fixture.index,
                                value.head.context("head event absent")?.height
                            ),
                            serde_json::to_vec(&value)?,
                        ),
                        Event::Heartbeat(value) => (
                            format!("live-{}.json", fixture.index),
                            serde_json::to_vec(&value)?,
                        ),
                    };
                    fs::write(fixture.directory.join(name), bytes)?;
                }
                Ok(())
            })?;
        Ok(Self {
            tx: Some(tx),
            join: Some(join),
        })
    }
    fn send(&self, event: Event) -> Result<()> {
        self.tx
            .as_ref()
            .unwrap()
            .try_send(event)
            .map_err(|_| anyhow::anyhow!("bounded observer unavailable"))
    }
    fn finish(&mut self) -> Result<()> {
        drop(self.tx.take());
        if let Some(join) = self.join.take() {
            join.join()
                .map_err(|_| anyhow::anyhow!("observer panicked"))??;
        }
        Ok(())
    }
}
impl Drop for Observer {
    fn drop(&mut self) {
        let _ = self.finish();
    }
}

pub(super) fn run_controller(fixture: &Fixture, spec: LoadSpec) -> Result<()> {
    let wallet = Wallet::start(spec)?;
    let mut observer = Observer::start(fixture)?;
    let pipeline = CandidatePipeline::start(pipeline_config_for(fixture)?, OpenMode::Existing)?;
    let result = (|| -> Result<()> {
        let mut controller = open_controller(fixture, &pipeline)?;
        let result = (|| -> Result<()> {
            let deadline = Instant::now() + RUN_BUDGET;
            let mut ready = false;
            let mut go = false;
            let mut requested = false;
            let mut offered = None;
            let mut input = None;
            let mut last_head = 0;
            let mut next_heartbeat = Instant::now();
            loop {
                let now = Instant::now();
                ensure!(
                    now < deadline,
                    "load controller deadline: {:?}",
                    controller.stats()
                );
                controller.poll(&pipeline, now)?;
                if let Some(reply) = wallet.take()? {
                    match reply {
                        WalletReply::Ready => {
                            ready = true;
                            observer.send(Event::Ready)?;
                        }
                        WalletReply::Body(position, body) => {
                            requested = false;
                            input = Some((position, body));
                        }
                    }
                }
                if !go && ready {
                    go = fixture.directory.join("go").exists();
                }
                let current = (controller.context().height, controller.round());
                let desired = if controller.is_local_leader()? && current.0 <= spec.heights {
                    Some((current, controller.parent(), false))
                } else if spec.successor {
                    controller
                        .successor_parent()?
                        .filter(|parent| parent.height < spec.heights)
                        .map(|parent| ((parent.height + 1, 0), parent, true))
                } else {
                    None
                };
                if input
                    .as_ref()
                    .is_some_and(|(position, body)| desired.is_none_or(|(wanted, parent, _)| {
                        *position != wanted || !matches!(body.as_ref(), Message::Body { context, .. } if context.parent_block_hash == parent.block_hash)
                    }))
                {
                    input = None;
                }
                if let Some((position, parent, future)) =
                    desired.filter(|(position, parent, future)| {
                        go && offered != Some((*position, parent.block_hash, *future))
                    })
                {
                    if input.is_none() && !requested {
                        let mut context = batch_context(fixture.root);
                        context.height = position.0;
                        context.parent_height = parent.height;
                        context.parent_block_hash = parent.block_hash;
                        context.parent_state_root = parent.state_root;
                        context.parent_receipt_root = parent.receipt_batch_commitment;
                        context.parent_state_version = parent.state_version;
                        context.slot = position.0;
                        context.timestamp_unix_ms += position.0;
                        wallet
                            .tx
                            .as_ref()
                            .unwrap()
                            .try_send((position, context))
                            .map_err(|_| anyhow::anyhow!("bounded wallet request unavailable"))?;
                        requested = true;
                    }
                    if let Some((_, body)) = &input {
                        let accepted = if future {
                            controller.try_submit_successor_body(body)?
                        } else {
                            controller.try_submit_body(body)?
                        };
                        if accepted {
                            offered = Some((position, parent.block_hash, future));
                            input = None;
                        }
                    }
                }
                if let Some(head) = controller.head() {
                    if head.height > last_head {
                        ensure!(
                            head.height == last_head + 1,
                            "observer missed a durable height"
                        );
                        observer.send(Event::Head(Observation::from_controller(&controller)))?;
                        last_head = head.height;
                    }
                }
                if now >= next_heartbeat {
                    observer.send(Event::Heartbeat(Observation::from_controller(&controller)))?;
                    if fixture.directory.join("stop").exists() {
                        break;
                    }
                    next_heartbeat = now + Duration::from_millis(100);
                }
                std::thread::sleep(Duration::from_millis(1));
            }
            ensure!(
                last_head == spec.heights,
                "stopped before every durable height"
            );
            Ok(())
        })();
        controller.shutdown()?;
        result
    })();
    pipeline.shutdown()?;
    observer.finish()?;
    result
}

fn read_observation(path: &Path) -> Option<Observation> {
    fs::read(path)
        .ok()
        .and_then(|bytes| serde_json::from_slice(&bytes).ok())
}

#[derive(Serialize)]
struct Measurement {
    successor_enabled: bool,
    schema: &'static str,
    topology: &'static str,
    batch_size: usize,
    heights: u64,
    unique_finalized_transactions: u64,
    successful_transactions: u64,
    business_failed_transactions: u64,
    elapsed_to_all_four_durable_heads_seconds: f64,
    finalized_tps: f64,
    backlog_release_to_all_four_observed_seconds_by_height: Vec<f64>,
    observed_backlog_latency_p95_seconds: f64,
    observed_backlog_latency_p99_seconds: f64,
    measurement_scope: String,
    observers: Vec<Observation>,
    cold_recovery_pass: bool,
    relay_lifetime_admitted_wire_bytes: u64,
    relay_lifetime_source_byte_rejections: u64,
    relay_lifetime_registered_sessions: u64,
    cold_ledger_file_bytes: Vec<u64>,
    executable_sha256: String,
    aoem_sha256: String,
    os: &'static str,
    logical_parallelism: usize,
}

fn digest_file(path: &Path) -> Result<String> {
    Ok(format!("{:x}", Sha256::digest(fs::read(path)?)))
}

fn directory_bytes(path: &Path) -> Result<u64> {
    let mut total = 0u64;
    for entry in fs::read_dir(path)? {
        let entry = entry?;
        let kind = entry.file_type()?;
        ensure!(!kind.is_symlink(), "unexpected symlink in fixture ledger");
        let bytes = if kind.is_dir() {
            directory_bytes(&entry.path())?
        } else {
            entry.metadata()?.len()
        };
        total = total
            .checked_add(bytes)
            .context("ledger byte count overflow")?;
    }
    Ok(total)
}

fn percentile(sorted: &[f64], percentage: usize) -> f64 {
    sorted[(sorted.len() * percentage).div_ceil(100).saturating_sub(1)]
}

fn one_load(spec: LoadSpec) -> Result<()> {
    let repository = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .canonicalize()?;
    let artifacts = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../target/runtime-rebuild");
    fs::create_dir_all(&artifacts)?;
    ensure!(
        artifacts.canonicalize()?.starts_with(repository),
        "artifacts outside repository"
    );
    let directory = artifacts.join(format!(
        "controller-load-{}-{}-{}",
        spec.batch_size,
        std::process::id(),
        SystemTime::now().duration_since(UNIX_EPOCH)?.as_nanos()
    ));
    fs::create_dir(&directory)?;
    eprintln!(
        "continuous controller load artifacts={}",
        directory.display()
    );
    let mut relay = Relay::start(&directory.join("relay"))?;
    let workload = Workload::new(spec.batch_size, spec.heights, policy())?;
    let update = stage_state_update(
        &Memory::default(),
        empty_root(),
        &workload.initial_changes()?,
    )?;
    let mut fixtures = Vec::new();
    for index in 0..4 {
        let ledger = directory.join(format!("validator-{index}"));
        fs::create_dir(&ledger)?;
        let store = CandidateStore::open(store_config(&ledger)?, OpenMode::CreateNew)?;
        store.install_unpublished_state(&update)?;
        drop(store);
        fixtures.push(Fixture {
            index,
            ledger,
            directory: directory.clone(),
            endpoint: relay.endpoint.clone(),
            certificate: relay.certificate.clone(),
            root: update.root(),
            load: Some(spec),
        });
    }
    let mut children = Children::default();
    for fixture in &fixtures {
        children.0.push(spawn(fixture, "live")?);
    }
    wait_for(&mut children, Instant::now() + RUN_BUDGET, || {
        (0..4).all(|i| directory.join(format!("ready-{i}")).exists())
    })?;
    // One parent's monotonic clock, no subtraction of clocks from different
    // processes/machines. All signed backlog already exists; start BEFORE go.
    let started = Instant::now();
    fs::write(directory.join("go"), b"release signed backlog")?;
    let deadline = started + RUN_BUDGET;
    let mut times = Vec::new();
    let mut final_reports = Vec::new();
    for height in 1..=spec.heights {
        loop {
            let current = (0..4)
                .map(|i| read_observation(&directory.join(format!("head-{i}-{height}.json"))))
                .collect::<Option<Vec<_>>>();
            if let Some(reports) = current {
                ensure!(
                    reports
                        .iter()
                        .all(|r| r.head == reports[0].head
                            && r.head.is_some_and(|h| h.height == height)),
                    "four durable heads diverged"
                );
                times.push(started.elapsed().as_secs_f64());
                final_reports = reports;
                break;
            }
            for child in &mut children.0 {
                ensure!(
                    child.try_wait()?.is_none(),
                    "load child {} exited",
                    child.id()
                );
            }
            ensure!(
                Instant::now() < deadline,
                "load deadline at height {height}; artifacts={}",
                directory.display()
            );
            std::thread::sleep(Duration::from_millis(1));
        }
    }
    let elapsed = *times.last().context("no finalized samples")?;
    let pids: std::collections::BTreeSet<_> = final_reports.iter().map(|r| r.pid).collect();
    ensure!(
        pids.len() == 4 && !pids.contains(&std::process::id()),
        "not four independent validators"
    );
    ensure!(
        final_reports
            .iter()
            .all(|r| r.durable_decisions == spec.heights
                && r.executed_batches >= spec.heights
                && r.execution_failures == 0
                && !r.observation_saturated),
        "execution/decision measurement incomplete"
    );
    fs::write(directory.join("stop"), b"all four durable heads observed")?;
    wait_exited(&mut children)?;
    relay.shutdown()?;
    // Read only after relay shutdown: on Windows an ordinary observer's open
    // handle can otherwise race its atomic report replacement (Delete sharing).
    let relay_report: novovm_network::product_relay_daemon::ProductRelayDaemonReportV1 =
        serde_json::from_slice(&fs::read(directory.join("relay/report.json"))?)?;
    let mut reopened = Children::default();
    for fixture in &fixtures {
        reopened.0.push(spawn(fixture, "recover")?);
    }
    wait_exited(&mut reopened)?;
    let recovered = fixtures
        .iter()
        .map(|f| -> Result<Recovered> {
            Ok(serde_json::from_slice(&fs::read(
                directory.join(format!("recovered-{}.json", f.index)),
            )?)?)
        })
        .collect::<Result<Vec<_>>>()?;
    let count = spec.heights * spec.batch_size as u64;
    for (i, value) in recovered.iter().enumerate() {
        ensure!(
            value.head == final_reports[i].head.unwrap()
                && value.candidates == recovered[0].candidates
                && value.receipts == recovered[0].receipts
                && value.recipient == u128::from(count),
            "cold recovery disagrees with live heads, raw execution or full state oracle"
        );
        ensure!(
            !pids.contains(&value.pid) && value.pid != std::process::id(),
            "not a new recovery process"
        );
    }
    let report = Measurement {
        successor_enabled: spec.successor,
        schema: "novovm/controller-load/v1", topology: "one host; four OS validator processes; one real WSS/E2E relay",
        batch_size: spec.batch_size, heights: spec.heights, unique_finalized_transactions: count,
        successful_transactions: count, business_failed_transactions: 0,
        elapsed_to_all_four_durable_heads_seconds: elapsed, finalized_tps: count as f64 / elapsed,
        observed_backlog_latency_p95_seconds: percentile(&times, 95),
        observed_backlog_latency_p99_seconds: percentile(&times, 99),
        backlog_release_to_all_four_observed_seconds_by_height: times,
        measurement_scope: format!("excludes key generation/signing, process startup, genesis and cold recovery; includes body construction, signature verification, execution, WSS/E2E including any remaining handshake, consensus and durable ACK; parent observes owner-published per-height files with 1ms requested polling (not guaranteed resolution); latency starts at release of entire finite backlog, includes waiting for prior heights; {} equal-size batch samples, not stable tail-latency or sustained mainnet capacity", spec.heights),
        observers: final_reports, cold_recovery_pass: true,
        relay_lifetime_admitted_wire_bytes: relay_report.relay_runtime.admitted_wire_bytes_total,
        relay_lifetime_source_byte_rejections: relay_report.relay_runtime.source_byte_limited_frame_total,
        relay_lifetime_registered_sessions: relay_report.relay_runtime.registered_session_total,
        cold_ledger_file_bytes: fixtures.iter().map(|fixture| directory_bytes(&fixture.ledger)).collect::<Result<_>>()?,
        executable_sha256: digest_file(&std::env::current_exe()?)?, aoem_sha256: digest_file(&library()?)?,
        os: std::env::consts::OS, logical_parallelism: std::thread::available_parallelism()?.get(),
    };
    fs::write(
        directory.join("measurement.json"),
        serde_json::to_vec_pretty(&report)?,
    )?;
    if spec.successor {
        ensure!(
            report
                .observers
                .iter()
                .any(|node| node.successor_started > 0)
                && report.observers.iter().any(|node| {
                    node.successor_reused > 0 || node.successor_promoted_inflight > 0
                }),
            "successor fixture completed without executing and promoting any successor"
        );
    }
    eprintln!("batch={} unique_finalized={} all_four_seconds={elapsed:.6} finalized_tps={:.6}; full cold-state oracle passed", spec.batch_size, count, report.finalized_tps);
    Ok(())
}

#[test]
#[ignore = "requires real AOEM; four-process finite continuous signed load, all-node durable heads and cold oracle"]
fn four_process_continuous_signed_load() -> Result<()> {
    let _ = library()?;
    let heights = load_setting("NOVOVM_CONTROLLER_LOAD_HEIGHTS")?
        .map(|value| value.parse::<u64>())
        .transpose()?
        .unwrap_or(LOAD_HEIGHTS);
    ensure!(
        [LOAD_HEIGHTS, LONG_LOAD_HEIGHTS].contains(&heights),
        "unsupported load height count"
    );
    let selected = load_setting("NOVOVM_CONTROLLER_LOAD_BATCH")?
        .map(|value| value.parse::<usize>())
        .transpose()?;
    ensure!(
        selected.is_none_or(|size| BATCH_SIZES.contains(&size)),
        "unsupported load batch size"
    );
    for batch_size in BATCH_SIZES
        .into_iter()
        .filter(|size| selected.is_none_or(|value| value == *size))
    {
        one_load(LoadSpec {
            batch_size,
            heights,
            successor: load_setting("NOVOVM_CONTROLLER_LOAD_SUCCESSOR")?.as_deref() == Some("1"),
        })?;
    }
    Ok(())
}

#[test]
#[ignore = "requires real AOEM; actual successor execution, four-process finality and cold economic oracle"]
fn four_process_successor_signed_load() -> Result<()> {
    let _ = library()?;
    for batch_size in [32, 1024] {
        one_load(LoadSpec {
            batch_size,
            heights: LOAD_HEIGHTS,
            successor: true,
        })?;
    }
    Ok(())
}

fn load_setting(name: &str) -> Result<Option<String>> {
    match std::env::var(name) {
        Ok(value) => Ok(Some(value)),
        Err(std::env::VarError::NotPresent) => Ok(None),
        Err(error) => Err(error.into()),
    }
}

#[test]
fn nearest_rank_percentile_is_explicit_for_short_backlog_samples() {
    assert_eq!(percentile(&[1., 2., 3., 4., 5., 6., 7., 8.], 95), 8.);
    assert_eq!(percentile(&[1., 2., 3., 4.], 50), 2.);
}
