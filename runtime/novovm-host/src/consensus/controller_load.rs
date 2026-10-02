//! Same controller, signed ingress, encrypted network, resident AOEM and journal
//! as the failure/recovery fixture. Finite continuously available backlog, not
//! an engine benchmark, RPC benchmark, four machines or production parameters.
use super::*;
use crate::consensus::tests::controller_workload::{Workload, BATCH_SIZES};
use crate::consensus::transport::EarlyBodyScope;
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
    #[serde(default)]
    pub early_body: bool,
}

fn pipeline_config_for(fixture: &Fixture) -> Result<PipelineConfig> {
    let spec = fixture.load.context("load configuration absent")?;
    ensure!(
        BATCH_SIZES.contains(&spec.batch_size)
            && [LOAD_HEIGHTS, LONG_LOAD_HEIGHTS].contains(&spec.heights)
            && !(spec.successor && spec.early_body),
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

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum WalletRequest {
    Full(BatchContext),
    Early(EarlyBodyScope),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum WalletOffer {
    Full {
        position: (u64, u64),
        context: BatchContext,
        successor: bool,
    },
    Early(EarlyBodyScope),
}

impl WalletOffer {
    fn position(self) -> (u64, u64) {
        match self {
            Self::Full { position, .. } => position,
            Self::Early(scope) => (scope.target_height, 0),
        }
    }

    fn request(self) -> WalletRequest {
        match self {
            Self::Full { context, .. } => WalletRequest::Full(context),
            Self::Early(scope) => WalletRequest::Early(scope),
        }
    }

    // Exact scalar metadata only: no hashing/cloning a whole body on this
    // control loop. The source context AND source round pin an early reply.
    fn matches(self, position: (u64, u64), message: &Message) -> bool {
        if position != self.position() {
            return false;
        }
        match (self.request(), message) {
            (WalletRequest::Full(expected), Message::Body { context, .. }) => expected == *context,
            (WalletRequest::Early(expected), Message::EarlyBody { scope, .. }) => {
                expected == *scope
            }
            _ => false,
        }
    }
}

fn full_offer(
    mut context: BatchContext,
    position: (u64, u64),
    parent: ParentPoint,
    successor: bool,
) -> Result<WalletOffer> {
    context.height = position.0;
    context.parent_height = parent.height;
    context.parent_block_hash = parent.block_hash;
    context.parent_state_root = parent.state_root;
    context.parent_receipt_root = parent.receipt_batch_commitment;
    context.parent_state_version = parent.state_version;
    context.slot = position.0;
    context.timestamp_unix_ms = context
        .timestamp_unix_ms
        .checked_add(position.0)
        .context("fixture timestamp overflow")?;
    Ok(WalletOffer::Full {
        position,
        context,
        successor,
    })
}

fn desired_offer(
    controller: &Controller,
    template: BatchContext,
    spec: LoadSpec,
) -> Result<Option<WalletOffer>> {
    let current = (controller.context().height, controller.round());
    if controller.is_local_leader()?
        && current.0 <= spec.heights
        && !(spec.early_body && controller.has_early_target(current.0))
    {
        // Missing current-height work always wins over optional preparation.
        return full_offer(template, current, controller.parent(), false).map(Some);
    }
    if spec.early_body {
        return Ok(controller
            .early_body_scope()?
            .filter(|scope| scope.target_height <= spec.heights)
            .map(WalletOffer::Early));
    }
    if spec.successor {
        return controller
            .successor_parent()?
            .filter(|parent| parent.height < spec.heights)
            .map(|parent| full_offer(template, (parent.height + 1, 0), parent, true))
            .transpose();
    }
    Ok(None)
}

/// Signing, whole-body cloning and destruction stay off the controller thread.
/// Only this bounded test wallet knows the future backlog; validators receive
/// no expected writes, receipt, proposal, certificate or publish permission.
struct Wallet {
    tx: Option<SyncSender<((u64, u64), WalletRequest)>>,
    rx: Option<Receiver<Result<WalletReply>>>,
    join: Option<JoinHandle<()>>,
}

impl Wallet {
    fn start(spec: LoadSpec) -> Result<Self> {
        let (tx, requests) = mpsc::sync_channel::<((u64, u64), WalletRequest)>(1);
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
                while let Ok((position, request)) = requests.recv() {
                    let Some(body) = raw.get(position.0.saturating_sub(1) as usize) else {
                        let _ = replies.send(Err(anyhow::anyhow!("wallet height out of range")));
                        break;
                    };
                    let message = Arc::new(match request {
                        WalletRequest::Full(context) => Message::Body {
                            context,
                            raw_transactions: body.clone(),
                        },
                        WalletRequest::Early(scope) => Message::EarlyBody {
                            scope,
                            raw_transactions: body.clone(),
                        },
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
    early_authentication_started: u64,
    early_authentication_completed: u64,
    // Actual authentication result taken while the corresponding local
    // durable parent candidate is absent, not submission or pre-finality work.
    early_authentication_completed_before_parent: u64,
    early_bind_reused: u64,
    early_discarded: u64,
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
            early_authentication_started: stats.early_authentication_started,
            early_authentication_completed: stats.early_authentication_completed,
            early_authentication_completed_before_parent: stats
                .early_authentication_completed_before_parent,
            early_bind_reused: stats.early_bind_reused,
            early_discarded: stats.early_discarded,
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
            // Pure immutable fixture pins, not a cached live parent or permit.
            // Avoid recomputing program/policy commitments on every poll.
            let context_template = batch_context(fixture.root);
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
                let desired = desired_offer(&controller, context_template, spec)?;
                if input.as_ref().is_some_and(|(position, body)| {
                    desired.is_none_or(|wanted| !wanted.matches(*position, body))
                }) {
                    input = None;
                }
                if let Some(wanted) = desired.filter(|wanted| go && offered != Some(*wanted)) {
                    if input.is_none() && !requested {
                        wallet
                            .tx
                            .as_ref()
                            .unwrap()
                            .try_send((wanted.position(), wanted.request()))
                            .map_err(|_| anyhow::anyhow!("bounded wallet request unavailable"))?;
                        requested = true;
                    }
                    if let Some((_, body)) = &input {
                        let accepted = match wanted {
                            WalletOffer::Full {
                                successor: true, ..
                            } => controller.try_submit_successor_body(body)?,
                            WalletOffer::Full {
                                successor: false, ..
                            } => controller.try_submit_body(body)?,
                            WalletOffer::Early(scope) => {
                                let timestamp = context_template
                                    .timestamp_unix_ms
                                    .checked_add(scope.target_height)
                                    .context("fixture timestamp overflow")?;
                                controller.try_submit_early_body(
                                    body,
                                    scope.target_height,
                                    timestamp,
                                )?
                            }
                        };
                        if accepted {
                            offered = Some(wanted);
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
    early_body_enabled: bool,
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
        early_body_enabled: spec.early_body,
        schema: "novovm/controller-load/v1", topology: "one host; four OS validator processes; one real WSS/E2E relay",
        batch_size: spec.batch_size, heights: spec.heights, unique_finalized_transactions: count,
        successful_transactions: count, business_failed_transactions: 0,
        elapsed_to_all_four_durable_heads_seconds: elapsed, finalized_tps: count as f64 / elapsed,
        observed_backlog_latency_p95_seconds: percentile(&times, 95),
        observed_backlog_latency_p99_seconds: percentile(&times, 99),
        backlog_release_to_all_four_observed_seconds_by_height: times,
        measurement_scope: format!("excludes key generation/signing, process startup, genesis and cold recovery; includes body construction, signature verification, execution, WSS/E2E including any remaining handshake, consensus and durable ACK; parent observes owner-published per-height files with 1ms requested polling (not guaranteed resolution); latency starts at release of entire finite backlog, includes waiting for prior heights; {} equal-size batch samples, not stable tail-latency or sustained mainnet capacity{}", spec.heights, if spec.early_body { "; early_authentication_completed_before_parent counts a real authentication result taken without the corresponding local durable parent candidate, not admission, business execution or parent-finality overlap" } else { "" }),
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
    if spec.early_body {
        ensure!(
            report.observers.iter().any(|node| node.early_authentication_started > 0)
                && report.observers.iter().any(|node| node.early_authentication_completed > 0)
                && report.observers.iter().any(|node| node.early_authentication_completed_before_parent > 0)
                && report.observers.iter().any(|node| node.early_bind_reused > 0),
            "early-body fixture completed without real pre-parent authentication and exact-parent bind reuse"
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
            early_body: load_setting("NOVOVM_CONTROLLER_LOAD_EARLY_BODY")?.as_deref() == Some("1"),
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
            early_body: false,
        })?;
    }
    Ok(())
}

#[test]
#[ignore = "requires real AOEM; actual parent-independent authentication, four-process finality and full cold economic oracle"]
fn four_process_early_body_signed_load() -> Result<()> {
    let _ = library()?;
    // Same admitted batch and height sizes as the baseline measurements, not
    // larger queues or a longer deadline. The small case is not a TPS claim.
    for (batch_size, heights) in [(32, LOAD_HEIGHTS), (1024, LONG_LOAD_HEIGHTS)] {
        one_load(LoadSpec {
            batch_size,
            heights,
            successor: false,
            early_body: true,
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

#[test]
fn old_load_configuration_defaults_to_neither_optional_mode() -> Result<()> {
    let old: LoadSpec = serde_json::from_str(r#"{"batch_size":32,"heights":8}"#)?;
    assert!(!old.successor && !old.early_body);
    let successor: LoadSpec =
        serde_json::from_str(r#"{"batch_size":32,"heights":8,"successor":true}"#)?;
    assert!(successor.successor && !successor.early_body);
    let early = LoadSpec {
        early_body: true,
        ..old
    };
    let restored: LoadSpec = serde_json::from_slice(&serde_json::to_vec(&early)?)?;
    assert!(restored.early_body && !restored.successor);
    assert_eq!((restored.batch_size, restored.heights), (32, 8));
    Ok(())
}

#[test]
fn wallet_reply_matching_pins_exact_source_round_and_full_context() {
    // Only metadata matching is tested here. Opaque bytes are never evidence
    // of a valid transaction, successful authentication or parent authority.
    let source = ConsensusContext {
        chain_id: CHAIN,
        genesis_config_commitment: GENESIS,
        protocol_commitment: PROTOCOL,
        epoch: 1,
        validator_set_hash: [3; 32],
        height: 7,
        parent_block_hash: [4; 32],
        parent_decision_hash: [5; 32],
    };
    let scope = EarlyBodyScope {
        source,
        source_round: 2,
        target_height: 8,
    };
    let early = WalletOffer::Early(scope);
    let message = Message::EarlyBody {
        scope,
        raw_transactions: vec![vec![1]],
    };
    assert!(early.matches((8, 0), &message));
    assert!(!early.matches((8, 1), &message));
    for stale in [
        EarlyBodyScope {
            source_round: 3,
            ..scope
        },
        EarlyBodyScope {
            source: ConsensusContext {
                parent_block_hash: [6; 32],
                ..source
            },
            ..scope
        },
        EarlyBodyScope {
            source: ConsensusContext {
                parent_decision_hash: [6; 32],
                ..source
            },
            ..scope
        },
        EarlyBodyScope {
            source: ConsensusContext {
                validator_set_hash: [6; 32],
                ..source
            },
            ..scope
        },
        EarlyBodyScope {
            source: ConsensusContext { epoch: 2, ..source },
            ..scope
        },
        EarlyBodyScope {
            target_height: 9,
            ..scope
        },
    ] {
        assert!(!early.matches(
            (8, 0),
            &Message::EarlyBody {
                scope: stale,
                raw_transactions: vec![vec![1]]
            }
        ));
    }
    let context = batch_context([7; 32]);
    let full = WalletOffer::Full {
        position: (8, 0),
        context,
        successor: false,
    };
    let body = Message::Body {
        context,
        raw_transactions: vec![vec![1]],
    };
    assert!(full.matches((8, 0), &body));
    assert!(!full.matches((8, 1), &body));
    assert!(!early.matches((8, 0), &body));
    assert!(!full.matches((8, 0), &message));
    for stale in [
        BatchContext {
            parent_state_root: [8; 32],
            ..context
        },
        BatchContext {
            parent_receipt_root: [8; 32],
            ..context
        },
        BatchContext {
            slot: context.slot + 1,
            ..context
        },
        BatchContext {
            timestamp_unix_ms: context.timestamp_unix_ms + 1,
            ..context
        },
    ] {
        assert!(!full.matches(
            (8, 0),
            &Message::Body {
                context: stale,
                raw_transactions: vec![vec![1]]
            }
        ));
    }
}
