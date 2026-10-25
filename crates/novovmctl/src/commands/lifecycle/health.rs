//! Read-only observations through existing product entrypoints. A configuration
//! check, TCP accept, or saved status file is never authenticated service health.
//! This module does not provision keys, send transactions, or write custody.
use novovm_exec::pq_signature::AoemMldsaParameterSet;
use novovm_node::native_pipeline::service::client::operator::MAX_CLIENT_CONFIG_BYTES;
use novovm_node::native_pipeline::service::config::NativePeerConfig;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::ffi::OsString;
use std::fs::File;
use std::io::Read;
use std::net::{SocketAddr, TcpStream};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::mpsc;
use std::thread;
use std::time::{Duration, Instant};

const MAX_OUTPUT_BYTES: usize = 64 * 1024;
const MAX_LISTENERS: usize = 8;

fn default_timeout_ms() -> u64 {
    5_000
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct HealthSpec {
    #[serde(default)]
    pub native_client: Option<NativeClientProbe>,
    #[serde(default)]
    pub rendezvous: Option<RendezvousProbe>,
    /// TCP connect only; no protocol bytes, no UDP reachability inference.
    #[serde(default)]
    pub tcp_listeners: Vec<SocketAddr>,
    /// Shared wall-clock budget for all probes, not a budget per endpoint.
    #[serde(default = "default_timeout_ms")]
    pub timeout_ms: u64,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct NativeClientProbe {
    pub binary: PathBuf,
    pub config: PathBuf,
    /// Existing binary NVQUERY request. The original client rejects submissions
    /// in query mode; only a completed state read counts as ledger readability.
    #[serde(default)]
    pub ledger_query: Option<PathBuf>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct RendezvousProbe {
    pub binary: PathBuf,
    pub config: PathBuf,
}

impl HealthSpec {
    pub(crate) fn executable_paths(&self) -> Vec<&Path> {
        self.native_client
            .iter()
            .map(|p| p.binary.as_path())
            .chain(self.rendezvous.iter().map(|p| p.binary.as_path()))
            .collect()
    }

    pub(crate) fn input_paths(&self) -> Vec<&Path> {
        let mut paths = Vec::new();
        if let Some(native) = &self.native_client {
            paths.push(native.config.as_path());
            paths.extend(native.ledger_query.as_deref());
        }
        paths.extend(self.rendezvous.iter().map(|p| p.config.as_path()));
        paths
    }

    fn valid(&self) -> bool {
        (100..=30_000).contains(&self.timeout_ms)
            && self.tcp_listeners.len() <= MAX_LISTENERS
            && self.tcp_listeners.iter().all(|address| {
                address.port() != 0
                    && !address.ip().is_unspecified()
                    && !address.ip().is_multicast()
            })
            && self.executable_paths().iter().all(|p| p.is_absolute())
            && self.input_paths().iter().all(|p| p.is_absolute())
            && self
                .native_client
                .as_ref()
                .is_none_or(|p| binary_name(&p.binary, "novovm-native-client"))
            && self
                .rendezvous
                .as_ref()
                .is_none_or(|p| binary_name(&p.binary, "novovm-rendezvous-service"))
    }
}

fn binary_name(path: &Path, name: &str) -> bool {
    path.file_name()
        .and_then(|n| n.to_str())
        .is_some_and(|n| n == name || n == format!("{name}.exe"))
}

#[derive(Clone, Copy, Debug, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub(crate) enum CheckState {
    Verified,
    Failed,
    Unavailable,
}

#[derive(Clone, Debug, Serialize)]
pub(crate) struct HealthCheck {
    pub name: String,
    pub state: CheckState,
    pub detail: &'static str,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub observation: Option<Value>,
}

#[derive(Clone, Debug, Serialize)]
pub(crate) struct HealthReport {
    pub scope: &'static str,
    /// Full-stack readiness is not delivered by these partial observations.
    pub ready: bool,
    pub elapsed_ms: u128,
    pub checks: Vec<HealthCheck>,
}

impl HealthReport {
    fn add(&mut self, name: &str, state: CheckState, detail: &'static str) {
        self.checks.push(HealthCheck {
            name: name.to_owned(),
            state,
            detail,
            observation: None,
        });
    }
}

#[derive(Debug)]
enum ProbeFailure {
    Failed,
    TimedOut,
    OutputBound,
}

// The command has fixed product arguments constructed below, never an operator
// shell command. stdout is bounded and stderr is discarded rather than exposing
// configuration paths, environment, or key-related errors in a health report.
trait ProbeRunner {
    fn run(
        &mut self,
        binary: &Path,
        args: &[OsString],
        remaining: Duration,
    ) -> Result<Vec<u8>, ProbeFailure>;
    fn tcp(&mut self, address: SocketAddr, remaining: Duration) -> bool;
    fn client_identity(&mut self, config: &Path) -> Result<ClientIdentity, ProbeFailure>;
    fn rendezvous_context(&mut self, config: &Path) -> OwnerContext;
}

#[derive(Clone, Copy)]
enum OwnerContext {
    Matching,
    Unavailable,
    InvalidConfiguration,
}

#[derive(Clone, Debug, Deserialize)]
struct ClientIdentity {
    chain_id: u64,
    genesis: [u8; 32],
    protocol: [u8; 32],
    node: NativePeerConfig,
}

impl ClientIdentity {
    fn validate_validator_identity(&self) -> Result<(), ProbeFailure> {
        // Exactly the original client's validator identity, not its independent
        // 32-byte transport key. The original client/config owns that binding.
        if self.node.public_key.len() != AoemMldsaParameterSet::MlDsa65.public_key_bytes()
            || self.node.public_key.iter().all(|byte| *byte == 0)
        {
            return Err(ProbeFailure::Failed);
        }
        Ok(())
    }
}

struct ProductProbes;

impl ProbeRunner for ProductProbes {
    fn run(
        &mut self,
        binary: &Path,
        args: &[OsString],
        remaining: Duration,
    ) -> Result<Vec<u8>, ProbeFailure> {
        if remaining.is_zero() {
            return Err(ProbeFailure::TimedOut);
        }
        let deadline = Instant::now() + remaining;
        let mut command = Command::new(binary);
        command
            .args(args)
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::null());
        #[cfg(windows)]
        {
            use std::os::windows::process::CommandExt;
            command.creation_flags(0x0800_0000); // CREATE_NO_WINDOW
        }
        let mut child = command.spawn().map_err(|_| ProbeFailure::Failed)?;
        let Some(mut stdout) = child.stdout.take() else {
            let _ = child.kill();
            let _ = child.wait();
            return Err(ProbeFailure::Failed);
        };
        let (sender, receiver) = mpsc::sync_channel(1);
        thread::spawn(move || {
            // Keep draining a large output to avoid blocking the child, but
            // retain at most the fixed local observation budget.
            let mut bytes = Vec::new();
            let mut overflow = false;
            let mut chunk = [0u8; 4096];
            let result = loop {
                match stdout.read(&mut chunk) {
                    Ok(0) => break Ok(()),
                    Ok(n) => {
                        if n <= MAX_OUTPUT_BYTES.saturating_sub(bytes.len()) {
                            bytes.extend_from_slice(&chunk[..n]);
                        } else {
                            overflow = true;
                        }
                    }
                    Err(_) => break Err(ProbeFailure::Failed),
                }
            };
            let _ = sender.send(match result {
                Err(error) => Err(error),
                Ok(()) if overflow => Err(ProbeFailure::OutputBound),
                Ok(()) => Ok(bytes),
            });
        });
        loop {
            match child.try_wait() {
                Ok(Some(status)) if status.success() => {
                    return receiver
                        .recv_timeout(deadline.saturating_duration_since(Instant::now()))
                        .map_err(|_| ProbeFailure::TimedOut)?;
                }
                Ok(Some(_)) => return Err(ProbeFailure::Failed),
                Ok(None) if Instant::now() < deadline => {
                    thread::sleep(
                        Duration::from_millis(5)
                            .min(deadline.saturating_duration_since(Instant::now())),
                    );
                }
                result => {
                    let _ = child.kill();
                    let _ = child.wait();
                    return Err(if result.is_err() {
                        ProbeFailure::Failed
                    } else {
                        ProbeFailure::TimedOut
                    });
                }
            }
        }
    }

    fn tcp(&mut self, address: SocketAddr, remaining: Duration) -> bool {
        !remaining.is_zero() && TcpStream::connect_timeout(&address, remaining).is_ok()
    }

    fn client_identity(&mut self, config: &Path) -> Result<ClientIdentity, ProbeFailure> {
        let mut bytes = Vec::new();
        File::open(config)
            .map_err(|_| ProbeFailure::Failed)?
            .take((MAX_CLIENT_CONFIG_BYTES + 1) as u64)
            .read_to_end(&mut bytes)
            .map_err(|_| ProbeFailure::Failed)?;
        if bytes.len() > MAX_CLIENT_CONFIG_BYTES {
            return Err(ProbeFailure::OutputBound);
        }
        let identity: ClientIdentity =
            serde_json::from_slice(&bytes).map_err(|_| ProbeFailure::Failed)?;
        identity.validate_validator_identity()?;
        Ok(identity)
    }

    fn rendezvous_context(&mut self, config: &Path) -> OwnerContext {
        let Some(config_text) = read_bounded_text(config) else {
            return OwnerContext::Unavailable;
        };
        let Ok(config_value) = serde_json::from_str::<Value>(&config_text) else {
            return OwnerContext::InvalidConfiguration;
        };
        #[cfg(target_os = "linux")]
        {
            use std::os::unix::fs::MetadataExt;
            // Original --check requires the private root owner to equal EUID.
            // Root's ability to manage systemd does not make it the mailbox
            // owner. Observe procfs without impersonation or privilege changes.
            let Some(effective_uid) = read_bounded_text(Path::new("/proc/self/status"))
                .and_then(|text| effective_uid_from_status(&text))
            else {
                return OwnerContext::Unavailable;
            };
            let Some(root) = config_value["root"].as_str() else {
                return OwnerContext::InvalidConfiguration;
            };
            let Ok(metadata) = std::fs::metadata(root) else {
                return OwnerContext::Unavailable;
            };
            classify_owner_context(effective_uid, metadata.uid())
        }
        #[cfg(not(target_os = "linux"))]
        {
            let _ = config_value;
            OwnerContext::Matching
        }
    }
}

fn read_bounded_text(path: &Path) -> Option<String> {
    let mut text = String::new();
    File::open(path)
        .ok()?
        .take((MAX_OUTPUT_BYTES + 1) as u64)
        .read_to_string(&mut text)
        .ok()?;
    (text.len() <= MAX_OUTPUT_BYTES).then_some(text)
}

#[cfg(any(target_os = "linux", test))]
fn effective_uid_from_status(status: &str) -> Option<u32> {
    let mut lines = status.lines().filter_map(|line| line.strip_prefix("Uid:"));
    let line = lines.next()?;
    if lines.next().is_some() {
        return None;
    }
    let values: Vec<u32> = line
        .split_whitespace()
        .map(str::parse)
        .collect::<Result<_, _>>()
        .ok()?;
    (values.len() == 4).then(|| values[1])
}

#[cfg(any(target_os = "linux", test))]
fn classify_owner_context(effective_uid: u32, root_uid: u32) -> OwnerContext {
    if effective_uid == root_uid {
        OwnerContext::Matching
    } else {
        OwnerContext::Unavailable
    }
}

pub(crate) fn inspect(spec: &HealthSpec) -> HealthReport {
    inspect_with(spec, &mut ProductProbes)
}

fn inspect_with(spec: &HealthSpec, runner: &mut impl ProbeRunner) -> HealthReport {
    let started = Instant::now();
    let mut report = HealthReport {
        scope: "unified_read_only_observations_v1",
        ready: false,
        elapsed_ms: 0,
        checks: Vec::new(),
    };
    if !spec.valid() {
        report.add(
            "probe_configuration",
            CheckState::Failed,
            "absolute product paths, fixed executable names, valid endpoints and 100..30000 ms total timeout required",
        );
        return report;
    }
    let deadline = started + Duration::from_millis(spec.timeout_ms);
    if let Some(native) = &spec.native_client {
        inspect_native(native, runner, deadline, &mut report);
    } else {
        report.add(
            "native_status",
            CheckState::Unavailable,
            "existing native client configuration and transport-purpose credential not supplied; no identity is created",
        );
        report.add(
            "ledger_read",
            CheckState::Unavailable,
            "no live native state observation",
        );
        report.add(
            "recovery_projection",
            CheckState::Unavailable,
            "no live native recovery observation",
        );
    }
    if let Some(owner) = &spec.rendezvous {
        match runner.rendezvous_context(&owner.config) {
            OwnerContext::Matching => {
                let result = runner.run(
                    &owner.binary,
                    &[OsString::from("--check"), owner.config.as_os_str().to_owned()],
                    deadline.saturating_duration_since(Instant::now()),
                );
                report.add(
                    "mailbox_configuration",
                    if result.is_ok() { CheckState::Verified } else { CheckState::Failed },
                    "original rendezvous --check validates configuration only; it does not open a listener or authenticate a mailbox request",
                );
            }
            OwnerContext::Unavailable => report.add(
                "mailbox_configuration", CheckState::Unavailable,
                "owner execution context is different or unavailable; no impersonation, provisioning or weakened ownership check is attempted",
            ),
            OwnerContext::InvalidConfiguration => report.add(
                "mailbox_configuration", CheckState::Failed,
                "owner configuration is malformed; no runtime health inference",
            ),
        }
    } else {
        report.add(
            "mailbox_configuration",
            CheckState::Unavailable,
            "owner configuration not supplied",
        );
    }
    for address in &spec.tcp_listeners {
        let accepted = runner.tcp(*address, deadline.saturating_duration_since(Instant::now()));
        report.checks.push(HealthCheck {
            name: "tcp_accept".to_owned(),
            state: if accepted { CheckState::Verified } else { CheckState::Failed },
            detail: "TCP accept only; not identity authentication, relay forwarding or message delivery",
            observation: Some(json!({"endpoint":address})),
        });
    }
    if spec.tcp_listeners.is_empty() {
        report.add(
            "tcp_accept",
            CheckState::Unavailable,
            "no TCP listener probe configured; UDP is not inferred from TCP",
        );
    }
    for (name, detail) in [
        ("mailbox_authenticated_roundtrip", "no read-only authenticated mailbox CLI probe is connected; saved status.json is not live evidence"),
        ("content_custody_retrieval", "no custody-write probe is run by health; actual encrypted object retrieval acceptance remains separate"),
        ("aoem_loaded_identity", "native startup loads AOEM but current native status does not expose the live library identity; package hash is not runtime observation"),
        ("validator_peer_connections", "current typed native status does not expose authenticated validator peer connection state"),
        ("service_economic_settlement", "funded service settlement to a specific UCA and subsequent spending are not connected; no charging or rewards are activated"),
    ] {
        report.add(name, CheckState::Unavailable, detail);
    }
    report.elapsed_ms = started.elapsed().as_millis();
    report
}

fn inspect_native(
    native: &NativeClientProbe,
    runner: &mut impl ProbeRunner,
    deadline: Instant,
    report: &mut HealthReport,
) {
    let identity = match runner.client_identity(&native.config) {
        Ok(identity) => identity,
        Err(_) => {
            report.add("native_status", CheckState::Failed, "unable to read bounded pinned native client identity; credential contents are not reported");
            report.add(
                "ledger_read",
                CheckState::Unavailable,
                "native client identity unavailable",
            );
            report.add(
                "recovery_projection",
                CheckState::Unavailable,
                "native client identity unavailable",
            );
            return;
        }
    };
    let status = runner
        .run(
            &native.binary,
            &[
                native.config.as_os_str().to_owned(),
                OsString::from("status"),
            ],
            deadline.saturating_duration_since(Instant::now()),
        )
        .and_then(|bytes| parse_answer(&bytes, &identity))
        .and_then(|answer| {
            serde_json::from_value::<StatusObservation>(answer["Status"].clone())
                .map_err(|_| ProbeFailure::Failed)
        });
    match status {
        Ok(status) => {
            report.checks.push(HealthCheck {
                name: "native_status".to_owned(),
                state: if status.projection_failed { CheckState::Failed } else { CheckState::Verified },
                detail: "fresh authenticated native node observation; not a finality proof, an AOEM read or a complete peer-health report",
                observation: Some(json!({"height":status.height,"state_root":status.state_root,
                    "indexed_height":status.indexed_height,"current_height":status.current_height,
                    "recovering":status.recovering,"projection_failed":status.projection_failed})),
            });
            report.add(
                "recovery_projection",
                if status.projection_failed || status.indexed_height > status.height {
                    CheckState::Failed
                } else if status.recovering || status.indexed_height < status.height {
                    CheckState::Unavailable
                } else {
                    CheckState::Verified
                },
                "observed node recovery/projection only; no claim it has caught up with network peers",
            );
        }
        Err(_) => {
            report.add("native_status", CheckState::Failed, "live authenticated native query failed, timed out or returned an invalid observation; no cached status fallback");
            report.add(
                "recovery_projection",
                CheckState::Unavailable,
                "no live native recovery observation",
            );
        }
    }
    let Some(query) = &native.ledger_query else {
        report.add("ledger_read", CheckState::Unavailable, "an explicit read-only NVQUERY state request is required; status observes the head but does not read AOEM state");
        return;
    };
    let result = runner
        .run(
            &native.binary,
            &[
                native.config.as_os_str().to_owned(),
                OsString::from("query"),
                query.as_os_str().to_owned(),
            ],
            deadline.saturating_duration_since(Instant::now()),
        )
        .and_then(|bytes| parse_answer(&bytes, &identity));
    match result {
        Ok(answer) => {
            let state = ledger_observation(&answer);
            report.add("ledger_read", state, "only a non-pending native balance/UCA-state response observes the original state owner; absence is not spend permission and roots from separate requests are not an atomic snapshot");
        }
        Err(_) => report.add(
            "ledger_read",
            CheckState::Failed,
            "live read-only native state query failed, timed out or returned invalid output",
        ),
    }
}

// This is only the original CLI's local presentation, never network JSON.
fn parse_answer(bytes: &[u8], expected: &ClientIdentity) -> Result<Value, ProbeFailure> {
    if bytes.len() > MAX_OUTPUT_BYTES {
        return Err(ProbeFailure::OutputBound);
    }
    let value: Value = serde_json::from_slice(bytes).map_err(|_| ProbeFailure::Failed)?;
    let identity: ClientIdentity =
        serde_json::from_value(value.clone()).map_err(|_| ProbeFailure::Failed)?;
    identity.validate_validator_identity()?;
    expected.validate_validator_identity()?;
    if value["scope"] != "native_sdk_operator_v1"
        || value["result"]["kind"] != "node_observation"
        || value["result"]["independently_verified_finality"] != false
        || identity.chain_id != expected.chain_id
        || identity.genesis != expected.genesis
        || identity.protocol != expected.protocol
        || identity.node.public_key != expected.node.public_key
        || identity.node.address != expected.node.address
        || expected.genesis == [0; 32]
        || expected.protocol == [0; 32]
    {
        return Err(ProbeFailure::Failed);
    }
    let answer = value["result"]["answer"].clone();
    if !answer.is_object()
        || answer.as_object().is_some_and(|a| a.len() != 1)
        || answer.get("Error").is_some()
    {
        return Err(ProbeFailure::Failed);
    }
    Ok(answer)
}

#[derive(Deserialize)]
struct StatusObservation {
    height: u64,
    state_root: [u8; 32],
    indexed_height: u64,
    current_height: u64,
    recovering: bool,
    projection_failed: bool,
}

fn ledger_observation(answer: &Value) -> CheckState {
    let state = if let Some(balance) = answer.get("NovBalance") {
        if balance.is_null() {
            return CheckState::Unavailable;
        }
        if !decimal_u128(&balance["amount"]) {
            return CheckState::Failed;
        }
        balance
    } else if let Some(uca) = answer.get("UcaSpendState") {
        if uca == "Pending" {
            return CheckState::Unavailable;
        }
        if let Some(present) = uca.get("Present") {
            if ![
                "max_debit_per_intent",
                "remaining_debit_budget",
                "payer_balance",
                "recipient_balance",
            ]
            .iter()
            .all(|field| decimal_u128(&present[field]))
            {
                return CheckState::Failed;
            }
            present
        } else if let Some(missing) = uca.get("Missing") {
            missing
        } else {
            return CheckState::Failed;
        }
    } else {
        return CheckState::Unavailable;
    };
    if state["height"].as_u64().is_some()
        && serde_json::from_value::<[u8; 32]>(state["state_root"].clone()).is_ok()
    {
        CheckState::Verified
    } else {
        CheckState::Failed
    }
}

fn decimal_u128(value: &Value) -> bool {
    value.as_str().is_some_and(|v| {
        !v.is_empty() && v.bytes().all(|b| b.is_ascii_digit()) && v.parse::<u128>().is_ok()
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::VecDeque;

    struct FakeRunner {
        outputs: VecDeque<Result<Vec<u8>, ProbeFailure>>,
        calls: Vec<Vec<OsString>>,
        tcp_calls: usize,
    }
    impl ProbeRunner for FakeRunner {
        fn run(
            &mut self,
            _: &Path,
            args: &[OsString],
            _: Duration,
        ) -> Result<Vec<u8>, ProbeFailure> {
            self.calls.push(args.to_vec());
            self.outputs
                .pop_front()
                .expect("only configured fixed probes")
        }
        fn tcp(&mut self, _: SocketAddr, _: Duration) -> bool {
            self.tcp_calls += 1;
            true
        }
        fn client_identity(&mut self, _: &Path) -> Result<ClientIdentity, ProbeFailure> {
            Ok(identity())
        }
        fn rendezvous_context(&mut self, _: &Path) -> OwnerContext {
            OwnerContext::Matching
        }
    }

    fn identity() -> ClientIdentity {
        ClientIdentity {
            chain_id: 9,
            genesis: [1; 32],
            protocol: [2; 32],
            node: NativePeerConfig {
                public_key: vec![3; 1952],
                address: "127.0.0.1:41050".parse().unwrap(),
            },
        }
    }
    fn envelope(answer: Value) -> Vec<u8> {
        let identity = identity();
        serde_json::to_vec(&json!({"scope":"native_sdk_operator_v1", "chain_id":identity.chain_id,
            "genesis":identity.genesis,"protocol":identity.protocol,"node":identity.node,
            "result":{"kind":"node_observation","independently_verified_finality":false,"answer":answer}})).unwrap()
    }
    fn fixture_path(name: &str) -> PathBuf {
        if cfg!(windows) {
            PathBuf::from("C:/fixture").join(name)
        } else {
            PathBuf::from("/fixture").join(name)
        }
    }
    fn spec() -> HealthSpec {
        HealthSpec {
            native_client: Some(NativeClientProbe {
                binary: fixture_path("novovm-native-client"),
                config: fixture_path("client.json"),
                ledger_query: Some(fixture_path("balance.nvquery")),
            }),
            rendezvous: Some(RendezvousProbe {
                binary: fixture_path("novovm-rendezvous-service"),
                config: fixture_path("owner.json"),
            }),
            tcp_listeners: vec!["127.0.0.1:43172".parse().unwrap()],
            timeout_ms: 1000,
        }
    }
    fn status(recovering: bool) -> Value {
        json!({"Status":{"height":7,"indexed_height":7,"current_height":8,"state_root":vec![4;32],"recovering":recovering,"projection_failed":false}})
    }
    fn state(report: &HealthReport, name: &str) -> CheckState {
        report
            .checks
            .iter()
            .find(|check| check.name == name)
            .unwrap()
            .state
    }

    #[test]
    fn live_observations_are_not_full_stack_readiness() {
        let mut runner = FakeRunner { outputs: [Ok(envelope(status(false))), Ok(envelope(json!({"NovBalance":{"height":7,"state_root":vec![4;32],"amount":"340282366920938463463374607431768211455"}}))), Ok(Vec::new())].into(), calls: vec![], tcp_calls: 0 };
        let report = inspect_with(&spec(), &mut runner);
        assert!(!report.ready);
        assert_eq!(state(&report, "native_status"), CheckState::Verified);
        assert_eq!(state(&report, "ledger_read"), CheckState::Verified);
        assert_eq!(state(&report, "recovery_projection"), CheckState::Verified);
        assert_eq!(
            state(&report, "mailbox_configuration"),
            CheckState::Verified
        );
        for name in [
            "mailbox_authenticated_roundtrip",
            "validator_peer_connections",
            "aoem_loaded_identity",
            "service_economic_settlement",
        ] {
            assert_eq!(state(&report, name), CheckState::Unavailable);
        }
        assert_eq!(runner.calls[0][1], "status");
        assert_eq!(runner.calls[1][1], "query");
        assert_eq!(runner.calls[2][0], "--check");
        assert_eq!(runner.tcp_calls, 1);
    }

    #[test]
    fn no_probe_does_not_read_stale_status_or_create_identity() {
        let mut runner = FakeRunner {
            outputs: VecDeque::new(),
            calls: vec![],
            tcp_calls: 0,
        };
        let spec = HealthSpec {
            native_client: None,
            rendezvous: None,
            tcp_listeners: vec![],
            timeout_ms: 1000,
        };
        let report = inspect_with(&spec, &mut runner);
        assert!(!report.ready);
        assert!(runner.calls.is_empty());
        assert!(report
            .checks
            .iter()
            .all(|check| check.state == CheckState::Unavailable));
    }

    #[test]
    fn failed_live_query_does_not_fall_back_to_cached_status() {
        let mut runner = FakeRunner {
            outputs: [
                Err(ProbeFailure::TimedOut),
                Err(ProbeFailure::OutputBound),
                Ok(vec![]),
            ]
            .into(),
            calls: vec![],
            tcp_calls: 0,
        };
        let report = inspect_with(&spec(), &mut runner);
        assert_eq!(state(&report, "native_status"), CheckState::Failed);
        assert_eq!(state(&report, "ledger_read"), CheckState::Failed);
        assert!(!report.ready);
    }

    #[test]
    fn recovery_and_pending_reads_are_not_ready() {
        let mut runner = FakeRunner {
            outputs: [
                Ok(envelope(status(true))),
                Ok(envelope(json!({"NovBalance":null}))),
                Ok(vec![]),
            ]
            .into(),
            calls: vec![],
            tcp_calls: 0,
        };
        let report = inspect_with(&spec(), &mut runner);
        assert_eq!(state(&report, "native_status"), CheckState::Verified);
        assert_eq!(
            state(&report, "recovery_projection"),
            CheckState::Unavailable
        );
        assert_eq!(state(&report, "ledger_read"), CheckState::Unavailable);
    }

    #[test]
    fn reject_unpinned_domain_wrong_kind_or_identity() {
        let original: Value = serde_json::from_slice(&envelope(status(false))).unwrap();
        for (path, replacement) in [
            (vec!["chain_id"], json!(10)),
            (vec!["scope"], json!("saved_status")),
            (vec!["node", "address"], json!("127.0.0.1:41051")),
            (vec!["result", "kind"], json!("transport_delivery_only")),
            (
                vec!["result", "independently_verified_finality"],
                json!(true),
            ),
        ] {
            let mut changed = original.clone();
            let mut field = &mut changed;
            for part in path {
                field = &mut field[part];
            }
            *field = replacement;
            assert!(parse_answer(&serde_json::to_vec(&changed).unwrap(), &identity()).is_err());
        }
    }

    #[test]
    fn native_health_uses_current_mldsa_validator_identity_not_transport_key() {
        let original: Value = serde_json::from_slice(&envelope(status(false))).unwrap();
        assert!(parse_answer(&serde_json::to_vec(&original).unwrap(), &identity()).is_ok());
        for key in [vec![3; 32], vec![3; 1951], vec![3; 1953], vec![0; 1952]] {
            let mut changed = original.clone();
            changed["node"]["public_key"] = json!(key);
            let invalid: ClientIdentity = serde_json::from_value(changed.clone()).unwrap();
            assert!(invalid.validate_validator_identity().is_err());
            // Matching two invalid identities must not turn a transport key or
            // wrong-width validator into an accepted observation.
            assert!(parse_answer(&serde_json::to_vec(&changed).unwrap(), &invalid).is_err());
            assert!(parse_answer(&serde_json::to_vec(&changed).unwrap(), &identity()).is_err());
        }
    }

    #[test]
    fn native_health_reads_pretty_mldsa_configs_with_shared_budget_not_output_budget() {
        use novovm_node::native_pipeline::service::{
            client::operator::ClientConfig, GenesisValidator,
        };
        let root = std::env::temp_dir().join(format!(
            "novovm-health-client-budget-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        std::fs::create_dir(&root).unwrap();
        let path = root.join("client.json");
        let mut probes = ProductProbes;
        for count in [4, 64] {
            let validators: Vec<_> = (0..count)
                .map(|index| {
                    // Only public metadata is needed to exercise file parsing.
                    let mut public_key = vec![255; 1952];
                    public_key[..8].copy_from_slice(&(index as u64).to_be_bytes());
                    GenesisValidator {
                        public_key,
                        transport_public_key: [index as u8 + 1; 32],
                        weight: 1,
                    }
                })
                .collect();
            let config = ClientConfig {
                chain_id: 9,
                genesis: [1; 32],
                protocol: [2; 32],
                validator_epoch: 1,
                node: NativePeerConfig {
                    public_key: validators[0].public_key.clone(),
                    address: "127.0.0.1:41050".parse().unwrap(),
                },
                validators,
                bind_addr: "127.0.0.1:0".parse().unwrap(),
                transport_key_file: "not-read.key".into(),
                timeout_ms: 1000,
            };
            let bytes = serde_json::to_vec_pretty(&config).unwrap();
            assert!(bytes.len() > MAX_OUTPUT_BYTES);
            assert!(bytes.len() < MAX_CLIENT_CONFIG_BYTES);
            std::fs::write(&path, &bytes).unwrap();
            let observed = probes.client_identity(&path).unwrap();
            assert_eq!(observed.node.public_key, config.node.public_key);
            assert_eq!(observed.node.address, config.node.address);
            assert_eq!(observed.chain_id, config.chain_id);
            assert!(!root.join("not-read.key").exists());
        }
        std::fs::write(&path, vec![b' '; MAX_CLIENT_CONFIG_BYTES + 1]).unwrap();
        assert!(matches!(
            probes.client_identity(&path),
            Err(ProbeFailure::OutputBound)
        ));
        // Configuration capacity must not enlarge child-output acceptance.
        assert!(parse_answer(&vec![b' '; MAX_OUTPUT_BYTES + 1], &identity()).is_err());
    }

    #[test]
    fn balances_keep_full_width_and_receipts_do_not_prove_state_reads() {
        assert_eq!(
            ledger_observation(&json!({"Receipt":{"height":1}})),
            CheckState::Unavailable
        );
        assert_eq!(
            ledger_observation(&json!({"UcaSpendState":"Pending"})),
            CheckState::Unavailable
        );
        assert_eq!(
            ledger_observation(
                &json!({"UcaSpendState":{"Missing":{"height":1,"state_root":vec![4;32]}}})
            ),
            CheckState::Verified
        );
        for amount in [
            json!(1.5),
            json!(0),
            json!("-1"),
            json!("340282366920938463463374607431768211456"),
        ] {
            assert_eq!(
                ledger_observation(
                    &json!({"NovBalance":{"height":1,"state_root":vec![4;32],"amount":amount}})
                ),
                CheckState::Failed
            );
        }
    }

    #[test]
    fn invalid_probe_spec_never_spawns_commands() {
        let mut runner = FakeRunner {
            outputs: VecDeque::new(),
            calls: vec![],
            tcp_calls: 0,
        };
        let mut invalid = spec();
        invalid.native_client.as_mut().unwrap().binary = fixture_path("powershell.exe");
        let report = inspect_with(&invalid, &mut runner);
        assert_eq!(state(&report, "probe_configuration"), CheckState::Failed);
        assert!(runner.calls.is_empty());
        assert_eq!(runner.tcp_calls, 0);
        invalid = spec();
        invalid.timeout_ms = 0;
        assert!(!invalid.valid());
        invalid = spec();
        invalid.tcp_listeners = vec!["0.0.0.0:1234".parse().unwrap()];
        assert!(!invalid.valid());
        invalid = spec();
        invalid.native_client.as_mut().unwrap().config = "relative.json".into();
        assert!(!invalid.valid());
    }

    #[test]
    fn linux_effective_uid_is_not_real_uid_and_mismatch_is_unavailable() {
        assert_eq!(
            effective_uid_from_status("Name:\ttest\nUid:\t1000\t0\t0\t0\n"),
            Some(0)
        );
        assert!(matches!(
            classify_owner_context(0, 1000),
            OwnerContext::Unavailable
        ));
        assert!(matches!(
            classify_owner_context(1000, 1000),
            OwnerContext::Matching
        ));
        for text in [
            "Uid: 0 0 0",
            "Uid: 0 invalid 0 0",
            "Uid: 0 0 0 0\nUid: 1 1 1 1",
            "",
        ] {
            assert_eq!(effective_uid_from_status(text), None);
        }
    }

    #[test]
    fn mailbox_context_mismatch_does_not_execute_as_root_or_hide_invalid_config() {
        struct OwnerRunner {
            context: OwnerContext,
            calls: usize,
        }
        impl ProbeRunner for OwnerRunner {
            fn run(
                &mut self,
                _: &Path,
                _: &[OsString],
                _: Duration,
            ) -> Result<Vec<u8>, ProbeFailure> {
                self.calls += 1;
                Err(ProbeFailure::Failed)
            }
            fn tcp(&mut self, _: SocketAddr, _: Duration) -> bool {
                panic!("no listener configured")
            }
            fn client_identity(&mut self, _: &Path) -> Result<ClientIdentity, ProbeFailure> {
                panic!("no native client configured")
            }
            fn rendezvous_context(&mut self, _: &Path) -> OwnerContext {
                self.context
            }
        }
        let mut config = spec();
        config.native_client = None;
        config.tcp_listeners.clear();
        for (context, expected, calls) in [
            (OwnerContext::Unavailable, CheckState::Unavailable, 0),
            (OwnerContext::InvalidConfiguration, CheckState::Failed, 0),
            (OwnerContext::Matching, CheckState::Failed, 1),
        ] {
            let mut runner = OwnerRunner { context, calls: 0 };
            let report = inspect_with(&config, &mut runner);
            assert_eq!(state(&report, "mailbox_configuration"), expected);
            assert_eq!(runner.calls, calls);
            assert!(!report.ready);
        }
    }
}
