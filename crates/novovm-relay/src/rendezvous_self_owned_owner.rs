//! Platform ownership for the shared self-owned rendezvous runtime. Windows
//! retains DPAPI migration; Linux uses the existing encrypted checkpoint format.
#[cfg(target_os = "linux")]
use crate::linux_owner_files::{atomic, check_path, create, read_bounded};
#[cfg(windows)]
use crate::{
    windows_custody,
    windows_owner_files::{atomic, check_path, create, read_bounded},
};
#[cfg(windows)]
type LegacyConfig = crate::rendezvous_owner::Config;
#[cfg(target_os = "linux")]
type LegacyConfig = serde_json::Value;
#[cfg(windows)]
const KEYS_FILE: &str = "keys.dpapi";
#[cfg(windows)]
const POLICY_FILE: &str = "self-owned-policy.dpapi";
#[cfg(target_os = "linux")]
const KEYS_FILE: &str = "keys.sealed";
#[cfg(target_os = "linux")]
const POLICY_FILE: &str = "self-owned-policy.sealed";
use anyhow::{ensure, Context, Result};
use ed25519_dalek::SigningKey;
use novovm_network::{
    iroh_transport::IrohConfigV1,
    product_directory::{sign_relay_record_v1, RelayEndpointV1},
    product_rendezvous_daemon::now_ms,
    product_rendezvous_owned::run_self_owned_node_v1,
    product_service_lease::{
        SelfOwnedTrustV1, ServiceLeaseBookV1, ServiceLeaseFileV1, ServiceLeaseHostV1,
        ServiceLeasePersistenceV1,
    },
};
use rand::{rngs::OsRng, RngCore};
use serde::{Deserialize, Serialize};
use std::{
    collections::BTreeSet,
    fs::{File, OpenOptions},
    net::{IpAddr, SocketAddr},
    path::{Path, PathBuf},
    sync::{atomic::AtomicBool, Arc},
};
use zeroize::Zeroizing;

// Fixed vocabulary only: never serialize an anyhow message, address, path,
// identity or configuration. Context attaches at the actual owner boundary.
#[derive(Clone, Copy, Debug, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
enum FailureStage {
    StatusWrite,
    MailboxOpen,
    CarrierSetup,
    NativeBind,
    #[cfg(feature = "tls-transport")]
    TlsBind,
    Activate,
    Publish,
    TransportRun,
}
impl std::fmt::Display for FailureStage {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "owner stage {self:?}")
    }
}
fn stage_result<T>(result: Result<T>, stage: FailureStage) -> Result<T> {
    result.map_err(|error| {
        if error.downcast_ref::<FailureStage>().is_some() {
            error
        } else {
            error.context(stage)
        }
    })
}
#[derive(Serialize, Debug, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
enum FailureKind {
    Io,
    Timeout,
    Encoding,
    Other,
}
#[derive(Serialize)]
struct FailureEvidence {
    stage: FailureStage,
    kind: FailureKind,
    #[serde(rename = "ioKind", skip_serializing_if = "Option::is_none")]
    io_kind: Option<&'static str>,
    #[serde(rename = "snapshotWriteFailed")]
    snapshot_write_failed: bool,
}
impl FailureEvidence {
    fn from_error(error: &anyhow::Error) -> Self {
        // anyhow preserves typed contexts through outer transport wrappers.
        let stage = error
            .downcast_ref::<FailureStage>()
            .copied()
            .unwrap_or(FailureStage::TransportRun);
        let io = error.downcast_ref::<std::io::Error>();
        let kind = if io.is_some() {
            FailureKind::Io
        } else if error
            .downcast_ref::<tokio::time::error::Elapsed>()
            .is_some()
        {
            FailureKind::Timeout
        } else if error.downcast_ref::<serde_json::Error>().is_some() {
            FailureKind::Encoding
        } else {
            FailureKind::Other
        };
        let io_kind = io.map(|error| match error.kind() {
            std::io::ErrorKind::NotFound => "not_found",
            std::io::ErrorKind::PermissionDenied => "permission_denied",
            std::io::ErrorKind::AddrInUse => "address_in_use",
            std::io::ErrorKind::AddrNotAvailable => "address_unavailable",
            std::io::ErrorKind::TimedOut => "timed_out",
            std::io::ErrorKind::WouldBlock => "would_block",
            std::io::ErrorKind::Interrupted => "interrupted",
            std::io::ErrorKind::InvalidData => "invalid_data",
            std::io::ErrorKind::WriteZero => "write_zero",
            std::io::ErrorKind::UnexpectedEof => "unexpected_eof",
            _ => "other",
        });
        Self {
            stage,
            kind,
            io_kind,
            snapshot_write_failed: false,
        }
    }
}
#[derive(Debug)]
struct EvidenceWriteFailed;
impl std::fmt::Display for EvidenceWriteFailed {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("owner failure evidence could not be fully persisted")
    }
}
// Created only from an opened owner. Keeps its exclusive file lock alive through
// both snapshots, including after run_inner consumes the owner. No new writer.
struct RunEvidence {
    root: PathBuf,
    _lock: Arc<File>,
}
impl RunEvidence {
    fn finish(&self, result: Result<()>) -> Result<()> {
        let mut result = result;
        let failure = result.as_ref().err().map(FailureEvidence::from_error);
        let snapshot = serde_json::json!({
            "version": 2,
            "phase": if result.is_ok() { "stopped" } else { "failed_closed" },
            "anonymous": false,
            "failure": failure,
        });
        let written = atomic(
            &self.root.join("status.json"),
            snapshot.to_string().as_bytes(),
        );
        let snapshot_failed = written.is_err();
        if result.is_ok() {
            result = written.context(FailureStage::StatusWrite);
        }
        if let Err(error) = result {
            let mut evidence = FailureEvidence::from_error(&error);
            evidence.snapshot_write_failed = snapshot_failed;
            // One bounded record survives later starts/serving/stops. It is
            // diagnostic only, never an input to identity or service authority.
            let saved = atomic(
                &self.root.join("last-failure.json"),
                serde_json::json!({"version": 1, "failure": evidence})
                    .to_string()
                    .as_bytes(),
            );
            return Err(if snapshot_failed || saved.is_err() {
                error.context(EvidenceWriteFailed)
            } else {
                error
            });
        }
        Ok(())
    }
}

#[derive(Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Config {
    pub version: u8,
    pub root: PathBuf,
    pub allowed_clients: BTreeSet<[u8; 32]>,
    pub relays: Vec<String>,
    pub nameserver: IpAddr,
    pub bind_address: Option<SocketAddr>,
    pub relay_only: bool,
    pub record_id: String,
}
impl Config {
    pub fn load(path: &Path) -> Result<Self> {
        let config: Self = serde_json::from_slice(&read_bounded(path, 32768)?)?;
        ensure!(
            config.version == 2
                && (1..=16).contains(&config.allowed_clients.len())
                && !config.record_id.is_empty()
                && config.record_id.len() <= 128,
            "self-owned configuration bounds"
        );
        check_path(&config.root)?;
        ensure!(config.root.is_dir(), "private owner directory missing");
        IrohConfigV1::new(&config.relays, config.nameserver)?;
        for pin in &config.allowed_clients {
            ensure!(
                !ed25519_dalek::VerifyingKey::from_bytes(pin)?.is_weak(),
                "weak client key"
            );
        }
        Ok(config)
    }
}

fn storage_limits(limit_mib: Option<u64>) -> Result<novovm_network::mailbox_store::Limits> {
    use novovm_network::mailbox_store::Limits;
    match limit_mib {
        None => Ok(Limits::unlimited_storage()),
        Some(mib) => {
            ensure!(mib >= 500, "shared storage limit must be at least 500 MiB");
            let bytes = mib
                .checked_mul(1024 * 1024)
                .context("storage limit overflow")?;
            Ok(Limits::for_storage_budget(usize::try_from(bytes)?)?)
        }
    }
}

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Policy {
    config: Config,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    shared_storage_limit_mib: Option<u64>,
    // Config remains the immutable enrollment/trust baseline. Only an explicit
    // local owner command can change this persisted transport preference.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    carrier_mode: Option<CarrierMode>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    tls_listener: Option<TlsListenerConfig>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    native_listener: Option<NativeListenerConfig>,
    // Explicit signed NVMBOX07 capability on the existing Iroh carrier. Older
    // protected policies remain disabled until the local owner opts in.
    #[serde(default)]
    native_carrier_mailbox_v7: bool,
    // Explicit NVMBOX08 opt-in. Missing fields retain the existing codec;
    // accepting a new binary never upgrades a protected owner policy.
    #[serde(default)]
    native_carrier_mailbox_v8: bool,
    node_pin: [u8; 32],
    sequence_floor: u64,
    legacy: Option<LegacyConfig>,
    // Retain reserved sequence/enrollment provenance, including allocations
    // whose proposal never reached the old service journal.
    legacy_owner_checkpoint: Option<String>,
}

#[derive(Clone, Copy, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum CarrierMode {
    Automatic,
    RelayOnly,
    /// Disable relay services without rewriting the protected operator config.
    /// Direct QUIC remains a library carrier; this is not an anonymity policy.
    DirectOnly,
}

/// Local listener settings only; transport secrets never enter this JSON.
#[derive(Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TlsListenerConfig {
    pub version: u8,
    pub listen: SocketAddr,
    pub advertised: SocketAddr,
    #[serde(default = "default_tls_connections")]
    pub max_connections: u16,
    #[serde(default, skip_serializing_if = "tls_native_disabled")]
    pub native_mailbox_v8: bool,
}
fn tls_native_disabled(value: &bool) -> bool {
    !value
}
fn default_tls_connections() -> u16 {
    4
}
impl TlsListenerConfig {
    pub fn load(path: &Path) -> Result<Self> {
        let value: Self = serde_json::from_slice(&read_bounded(path, 4096)?)?;
        value.validate()?;
        Ok(value)
    }
    fn validate(&self) -> Result<()> {
        let admissible =
            |ip: IpAddr| !ip.is_multicast() && !matches!(ip, IpAddr::V4(v) if v.is_broadcast());
        ensure!(
            self.version == 1
                && (1..=64).contains(&self.max_connections)
                && admissible(self.listen.ip())
                && admissible(self.advertised.ip())
                && !self.advertised.ip().is_unspecified()
                && self.listen.is_ipv4() == self.advertised.is_ipv4(),
            "TLS listener address bounds"
        );
        // Port zero requests the actual listener port in the signed record.
        Ok(())
    }
}

/// Explicit native-only listener selection. Addresses and resource bounds are
/// protected by the existing owner policy; signing keys remain in DPAPI custody.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct NativeListenerConfig {
    pub version: u8,
    pub listen: SocketAddr,
    pub advertised: SocketAddr,
    pub max_connections: u16,
    pub max_bytes_per_minute: u64,
}
impl NativeListenerConfig {
    pub fn load(path: &Path) -> Result<Self> {
        let value: Self = serde_json::from_slice(&read_bounded(path, 4096)?)?;
        value.validate()?;
        Ok(value)
    }
    fn validate(&self) -> Result<()> {
        ensure!(
            self.version == 1
                && self.listen.is_ipv4() == self.advertised.is_ipv4()
                && !self.listen.ip().is_multicast()
                && !matches!(self.listen.ip(), IpAddr::V4(ip) if ip.is_broadcast())
                && !matches!(self.listen, SocketAddr::V6(a) if a.scope_id() != 0 || a.flowinfo() != 0),
            "native listener address bounds"
        );
        // Zero requests the actual bound port, just as the existing TLS owner.
        let mut address = self.advertised;
        if address.port() == 0 {
            address.set_port(1);
        }
        novovm_network::product_service_lease::NativeMailboxEndpointV7 {
            address,
            max_connections: self.max_connections,
            max_bytes_per_minute: self.max_bytes_per_minute,
        }
        .endpoint()?;
        Ok(())
    }
}

pub struct Owner {
    policy: Policy,
    key: SigningKey,
    storage: Zeroizing<[u8; 32]>,
    transport: Zeroizing<[u8; 32]>,
    transition: bool,
    _lock: Arc<File>,
}
impl Owner {
    /// Creation or migration are explicit local owner actions. Ordinary start
    /// never creates replacement keys, resets a journal, or infers new trust.
    pub fn open(config: Config, create_policy: bool, legacy: Option<LegacyConfig>) -> Result<Self> {
        let root = &config.root;
        #[cfg(target_os = "linux")]
        {
            crate::linux_owner_files::check_private_root(root)?;
            ensure!(
                legacy.is_none(),
                "Windows legacy migration is unavailable on Linux"
            );
            crate::linux_custody::validate_credential()?;
        }
        check_path(&root.join("owner.lock"))?;
        let mut lock_options = OpenOptions::new();
        #[cfg(target_os = "linux")]
        {
            use std::os::unix::fs::OpenOptionsExt;
            lock_options.mode(0o600).custom_flags(libc::O_NOFOLLOW);
        }
        let lock = lock_options
            .read(true)
            .write(true)
            .create(true)
            .truncate(false)
            .open(root.join("owner.lock"))?;
        lock.try_lock()
            .map_err(|_| anyhow::anyhow!("owner already running"))?;
        let custody = root.join(POLICY_FILE);
        if create_policy && !custody.try_exists()? {
            let (sequence_floor, legacy_owner_checkpoint) = if let Some(old) = &legacy {
                #[cfg(target_os = "linux")]
                {
                    let _ = old;
                    anyhow::bail!("Windows legacy migration is unavailable on Linux");
                }
                #[cfg(windows)]
                {
                    ensure!(
                        old.epoch.journal_path == root.join("service.journal")
                            && old
                                .epoch
                                .allowed_clients
                                .iter()
                                .copied()
                                .collect::<BTreeSet<_>>()
                                == config.allowed_clients
                            && old.epoch.relays == config.relays
                            && old.epoch.nameserver == config.nameserver
                            && old.epoch.relay_only == config.relay_only
                            && old.epoch.record_id == config.record_id,
                        "migration changes identity/transport/resource policy"
                    );
                    let (_, storage) = read_keys(root)?;
                    let (_journal, bytes) =
                        ServiceLeaseFileV1::open(&root.join("owner.journal"), storage.clone())?;
                    let state: serde_json::Value = serde_json::from_slice(&bytes)?;
                    ensure!(
                        state["version"] == 1 && state["enrollment_started"] == true,
                        "legacy enrollment required"
                    );
                    let floor = state["next_sequence"]
                        .as_u64()
                        .filter(|n| *n > 0)
                        .context("legacy sequence floor")?;
                    let (_journal, checkpoint) =
                        ServiceLeaseFileV1::open(&root.join("service.journal"), storage)?;
                    ServiceLeaseBookV1::restore(old.epoch.validate()?.1, &checkpoint, now_ms()?)?;
                    (floor, Some(String::from_utf8(bytes)?))
                }
            } else {
                ensure!(
                    !root.join("enrollment.started").try_exists()?
                        && !root.join(KEYS_FILE).try_exists()?
                        && !root.join("service.journal").try_exists()?
                        && !root.join("owner.journal").try_exists()?,
                    "fresh enrollment only"
                );
                create(
                    &root.join("enrollment.started"),
                    b"NOVOVM self-owned custody v2",
                )?;
                let mut keys = Zeroizing::new([0u8; 72]);
                keys[..8].copy_from_slice(b"NVKEYS03");
                OsRng.try_fill_bytes(&mut keys[8..])?;
                create_custody(&root.join(KEYS_FILE), keys.as_ref())?;
                (1, None)
            };
            let (key, _) = read_keys(root)?;
            let policy = Policy {
                config: config.clone(),
                shared_storage_limit_mib: None,
                carrier_mode: None,
                tls_listener: None,
                native_listener: None,
                native_carrier_mailbox_v7: false,
                native_carrier_mailbox_v8: false,
                node_pin: key.verifying_key().to_bytes(),
                sequence_floor,
                legacy,
                legacy_owner_checkpoint,
            };
            // Secret transport seed has a binary representation, never JSON,
            // command line, environment, export, status or console output.
            let mut protected = Zeroizing::new(vec![0u8; 40]);
            protected[..8].copy_from_slice(b"NVOWN002");
            OsRng.try_fill_bytes(&mut protected[8..40])?;
            protected.extend_from_slice(&serde_json::to_vec(&policy)?);
            create_custody(&custody, &protected)?;
        }
        ensure!(
            custody.is_file(),
            "explicit self-owned provisioning required"
        );
        let protected = read_custody(&custody)?;
        ensure!(
            protected.len() > 40 && &protected[..8] == b"NVOWN002",
            "self-owned custody version"
        );
        let transport = Zeroizing::new(<[u8; 32]>::try_from(&protected[8..40])?);
        let policy: Policy = serde_json::from_slice(&protected[40..])?;
        ensure!(policy.config == config, "protected local policy differs");
        #[cfg(target_os = "linux")]
        ensure!(
            policy.legacy.is_none() && policy.legacy_owner_checkpoint.is_none(),
            "Linux does not accept legacy migration policy"
        );
        ensure!(
            !(policy.native_carrier_mailbox_v7 && policy.native_carrier_mailbox_v8),
            "native carrier mailbox versions are mutually exclusive"
        );
        ensure!(
            !policy.native_carrier_mailbox_v7 || policy.native_listener.is_none(),
            "V7 carrier mailbox conflicts with native listener; select V8 for coexistence"
        );
        if let Some(tls) = &policy.tls_listener {
            ensure!(
                cfg!(feature = "tls-transport"),
                "configured TLS carrier missing from binary"
            );
            tls.validate()?;
            ensure!(
                !tls.native_mailbox_v8 || policy.native_carrier_mailbox_v8,
                "native TLS requires the V8 carrier mailbox"
            );
            ensure!(
                !relay_only(&policy),
                "TLS listener conflicts with relay-only policy"
            );
        }
        if let Some(native) = &policy.native_listener {
            native.validate()?;
            ensure!(
                !relay_only(&policy),
                "native listener conflicts with relay-only policy"
            );
        }
        let (key, storage) = read_keys(root)?;
        ensure!(
            policy.node_pin == key.verifying_key().to_bytes()
                && !config.allowed_clients.contains(&policy.node_pin)
                && *transport != key.to_bytes()
                && *transport != *storage,
            "custody identity separation"
        );
        // Check corruption and policy BEFORE binding any transport.
        let path = root.join("service.journal");
        if path.try_exists()? {
            let (_file, bytes) = ServiceLeaseFileV1::open(&path, storage.clone())?;
            match serde_json::from_slice::<serde_json::Value>(&bytes)?["version"].as_u64() {
                Some(2) => {
                    ServiceLeaseBookV1::restore_self_owned(
                        trust(policy.node_pin),
                        &bytes,
                        now_ms()?,
                    )?;
                }
                #[cfg(windows)]
                Some(1) if create_policy => {
                    let old = policy
                        .legacy
                        .as_ref()
                        .context("explicit legacy transition required")?;
                    ServiceLeaseBookV1::restore(old.epoch.validate()?.1, &bytes, now_ms()?)?;
                }
                _ => anyhow::bail!("incomplete self-owned migration; explicit recovery required"),
            }
        } else {
            ensure!(
                create_policy
                    && policy.legacy.is_none()
                    && !root.join("self-owned-activation.started").try_exists()?,
                "missing enrolled service history"
            );
        }
        Ok(Self {
            policy,
            key,
            storage,
            transport,
            transition: create_policy,
            _lock: Arc::new(lock),
        })
    }

    /// Local owner maintenance only: the exclusive lock prevents live-policy
    /// changes. No identity, peer admission, relay/DNS trust or history changes.
    pub fn set_carrier_mode(mut self, mode: CarrierMode) -> Result<()> {
        if matches!(mode, CarrierMode::RelayOnly) {
            ensure!(
                self.policy.native_listener.is_none(),
                "disable native listener before relay-only"
            );
            ensure!(
                self.policy.tls_listener.is_none(),
                "disable TLS listener before relay-only"
            );
            ensure!(
                self.policy.config.bind_address.is_none(),
                "relay-only cannot bind IP"
            );
            IrohConfigV1::new(&self.policy.config.relays, self.policy.config.nameserver)?
                .with_relay_only()?;
        }
        self.policy.carrier_mode = Some(mode);
        self.save_policy()
    }

    /// Local node owner choice. No explicit limit by default; finite values
    /// use MiB and must be at least 500. Existing custody is never deleted.
    pub fn set_storage_limit_mib(mut self, limit: Option<u64>) -> Result<()> {
        storage_limits(limit)?;
        self.policy.shared_storage_limit_mib = limit;
        self.save_policy()
    }

    pub fn set_tls_listener(mut self, listener: Option<TlsListenerConfig>) -> Result<()> {
        ensure!(
            cfg!(feature = "tls-transport"),
            "TLS carrier missing from binary"
        );
        if let Some(value) = &listener {
            value.validate()?;
            ensure!(
                !relay_only(&self.policy),
                "TLS listener conflicts with relay-only policy"
            );
        }
        ensure!(
            !listener
                .as_ref()
                .is_some_and(|value| value.native_mailbox_v8)
                || self.native_carrier_mailbox_v8(),
            "select the V8 carrier mailbox before enabling native TLS"
        );
        self.policy.tls_listener = listener;
        self.save_policy()
    }

    pub fn set_native_listener(mut self, listener: Option<NativeListenerConfig>) -> Result<()> {
        if let Some(value) = &listener {
            value.validate()?;
            ensure!(
                !self.native_carrier_mailbox_v7(),
                "select V8 before combining carrier and native listener"
            );
            ensure!(
                !relay_only(&self.policy),
                "native listener conflicts with relay-only policy"
            );
        }
        // V8 explicitly composes the listeners on the existing node; otherwise
        // the original native-only mode retains inactive carrier preferences.
        self.policy.native_listener = listener;
        self.save_policy()
    }

    /// Uses the same protected policy and exclusive owner lock. This selects
    /// a mailbox codec, without changing admission, identity or resource limits.
    pub fn set_native_carrier_mailbox_v7(mut self, enabled: bool) -> Result<()> {
        ensure!(
            !enabled || self.policy.native_listener.is_none(),
            "disable native-only listener before enabling native carrier mailbox"
        );
        ensure!(
            !self
                .policy
                .tls_listener
                .as_ref()
                .is_some_and(|tls| tls.native_mailbox_v8),
            "disable native TLS before leaving the V8 carrier mailbox"
        );
        self.policy.native_carrier_mailbox_v7 = enabled;
        self.policy.native_carrier_mailbox_v8 = false;
        self.save_policy()
    }

    /// Explicit codec selection, atomically replacing V7 in the same custody.
    /// Disabling either selector disables the carrier codec, including V8.
    pub fn set_native_carrier_mailbox_v8(mut self, enabled: bool) -> Result<()> {
        ensure!(
            enabled
                || !self
                    .policy
                    .tls_listener
                    .as_ref()
                    .is_some_and(|tls| tls.native_mailbox_v8),
            "disable native TLS before leaving the V8 carrier mailbox"
        );
        self.policy.native_carrier_mailbox_v7 = false;
        self.policy.native_carrier_mailbox_v8 = enabled;
        self.save_policy()
    }

    fn native_carrier_mailbox_v7(&self) -> bool {
        self.policy.native_carrier_mailbox_v7
    }

    fn native_carrier_mailbox_v8(&self) -> bool {
        self.policy.native_carrier_mailbox_v8
    }

    fn save_policy(&self) -> Result<()> {
        let mut bytes = Zeroizing::new(Vec::with_capacity(4096));
        bytes.extend_from_slice(b"NVOWN002");
        bytes.extend_from_slice(self.transport.as_ref());
        bytes.extend_from_slice(&serde_json::to_vec(&self.policy)?);
        save_custody(&self.policy.config.root.join(POLICY_FILE), &bytes)
    }

    pub async fn run(self, stop: Arc<AtomicBool>) -> Result<()> {
        // Only the lock holder writes runtime status. A failed second start
        // must not overwrite the live owner's status; failed binding must not
        // leave an earlier process's "serving" snapshot behind.
        let evidence = RunEvidence {
            root: self.policy.config.root.clone(),
            _lock: self._lock.clone(),
        };
        let started = atomic(
            &evidence.root.join("status.json"),
            br#"{"version":2,"phase":"starting","anonymous":false}"#,
        )
        .context(FailureStage::StatusWrite);
        let result = match started {
            Ok(()) => self.run_inner(stop).await,
            Err(error) => Err(error),
        };
        evidence.finish(result)
    }

    fn activate_book(
        &self,
        endpoints: Vec<RelayEndpointV1>,
        expires: u64,
    ) -> Result<ServiceLeaseBookV1> {
        let path = self.policy.config.root.join("service.journal");
        let previous = if path.try_exists()? {
            Some(ServiceLeaseFileV1::open(&path, self.storage.clone())?)
        } else {
            None
        };
        let now = now_ms()?;
        let (book, file) = if let Some((file, bytes)) = previous {
            let version = serde_json::from_slice::<serde_json::Value>(&bytes)?["version"].as_u64();
            let mut book = if version == Some(2) {
                ServiceLeaseBookV1::restore_self_owned(trust(self.policy.node_pin), &bytes, now)?
            } else {
                ensure!(self.transition, "explicit migration required");
                #[cfg(target_os = "linux")]
                anyhow::bail!("Linux does not accept legacy service history");
                #[cfg(windows)]
                {
                    let old = self
                        .policy
                        .legacy
                        .as_ref()
                        .context("legacy policy missing")?;
                    ServiceLeaseBookV1::restore(old.epoch.validate()?.1, &bytes, now)?
                }
            };
            let state: serde_json::Value = serde_json::from_slice(&bytes)?;
            let floor = state["current"]
                .as_array()
                .context("service sequence history")?
                .iter()
                .filter_map(|r| r["sequence"].as_u64())
                .max()
                .context("service revoked")?;
            let sequence = floor
                .max(self.policy.sequence_floor)
                .checked_add(1)
                .context("sequence exhausted")?;
            let record = sign_relay_record_v1(
                &self.key,
                &self.policy.config.record_id,
                endpoints,
                now,
                expires,
                sequence,
            )?;
            if book.is_self_owned() {
                book.attach_persistence(file.clone(), &bytes)?;
                book.replace_self_owned_records(vec![record], now)?;
            } else {
                book = book.prepare_self_owned_migration(
                    trust(self.policy.node_pin),
                    vec![record],
                    now,
                )?;
                let next = book.checkpoint()?;
                file.commit(&bytes, &next)?;
                book.attach_persistence(file.clone(), &next)?;
            }
            (book, file)
        } else {
            ensure!(
                self.transition && self.policy.legacy.is_none(),
                "fresh enrollment not authorized"
            );
            create(
                &self
                    .policy
                    .config
                    .root
                    .join("self-owned-activation.started"),
                b"NOVOVM self-owned activation v2",
            )?;
            let record = sign_relay_record_v1(
                &self.key,
                &self.policy.config.record_id,
                endpoints,
                now,
                expires,
                self.policy.sequence_floor,
            )?;
            let mut book =
                ServiceLeaseBookV1::new_self_owned(trust(self.policy.node_pin), vec![record], now)?;
            let bytes = book.checkpoint()?;
            let file = ServiceLeaseFileV1::create(&path, self.storage.clone(), &bytes)?;
            book.attach_persistence(file.clone(), &bytes)?;
            (book, file)
        };
        drop(file); // The book retains its original durable file owner.
        Ok(book)
    }

    fn activate_carrier(
        &self,
        endpoint: &novovm_network::product_service_lease::IrohServiceEndpointV1,
        expires: u64,
    ) -> Result<Arc<ServiceLeaseHostV1>> {
        let mut book = self.activate_book(vec![endpoint.endpoint()?], expires)?;
        book.check_route(
            self.policy.node_pin,
            endpoint.address.endpoint_key,
            endpoint.instance,
            now_ms()?,
        )?;
        let host = Arc::new(ServiceLeaseHostV1::new(self.key.clone(), book)?);
        // Publish the selected capability before Node::serve sees this host.
        // Identity, journal and term are unchanged; TLS keeps its own codec.
        if self.native_carrier_mailbox_v8() {
            host.rotate_native_carrier_mailbox_bulk_v1(true, now_ms()?, expires)?;
        } else if self.native_carrier_mailbox_v7() {
            host.rotate_native_carrier_mailbox_v7(true, now_ms()?, expires)?;
        }
        Ok(host)
    }

    async fn run_inner(self, stop: Arc<AtomicBool>) -> Result<()> {
        let _running_lock = self._lock.clone();
        let config = self.policy.config.clone();
        let relay_only = relay_only(&self.policy);
        let direct_only = matches!(self.policy.carrier_mode, Some(CarrierMode::DirectOnly));
        let native_settings = self.policy.native_listener.clone();
        let native_selected = native_settings.is_some();
        let dual_selected = native_selected && self.native_carrier_mailbox_v8();
        let relay_services_enabled =
            (!native_selected || dual_selected) && !effective_relays(&self.policy).is_empty();
        let key = self.key.clone();
        let publish_root = config.root.clone();
        let mailbox_path = config.root.join("mailbox.sqlite");
        let mailbox_started = config.root.join("mailbox.started");
        let mailbox = (|| -> Result<_> {
            check_path(&mailbox_path)?;
            if mailbox_started.try_exists()? {
                ensure!(mailbox_path.is_file(), "missing enrolled mailbox storage");
            } else {
                ensure!(!mailbox_path.try_exists()?, "unrecognized mailbox storage");
                create(&mailbox_started, b"NOVONET shared mailbox v1")?;
            }
            let limits = storage_limits(self.policy.shared_storage_limit_mib)?;
            let mut store =
                novovm_network::mailbox_store::MailboxStore::open(&mailbox_path, limits)?;
            // The protected node-owner policy is authoritative at startup;
            // reconcile a prior local store preference without deleting data.
            store.set_storage_budget(
                self.policy
                    .shared_storage_limit_mib
                    .map(|_| limits.total_bytes),
                i64::try_from(now_ms()? / 1000)?,
            )?;
            let mailbox = Arc::new(
                novovm_network::mailbox_rpc::MailboxServiceV1::new(store, key.clone())
                    // Desktop L3 serves concurrent Put/Fetch/Consume/Watch lanes. The
                    // conservative mobile default (2 requests/s for the entire node) makes
                    // those lanes queue behind one another even on healthy connections.
                    // This remains one shared budget; reconnecting cannot replenish it.
                    // Connection bounds remain independent of storage policy.
                    .with_continuous_request_rate_v4(64, 32)?,
            );
            Ok(mailbox)
        })()
        .context(FailureStage::MailboxOpen)?;
        let mailbox_status = mailbox.clone();
        let publish = move |record: &novovm_network::product_directory::PeerSignedRelayRecordV1| {
            (|| -> Result<()> {
            atomic(
                &publish_root.join("self-owned-record.json"),
                &serde_json::to_vec(record)?,
            )?;
            atomic(
                &publish_root.join("status.json"),
                &serde_json::to_vec(
                    &serde_json::json!({"version":2,"phase":"serving","publisherRequired":false,"anonymous":false,"carrierMode":if dual_selected {"native+carrier"} else if native_selected {"native"} else if relay_only {"relay-only"} else if direct_only {"direct-only"} else {"automatic"},"relayServicesEnabled":relay_services_enabled,"mailboxTimings":mailbox_status.timing_snapshot_v1()}),
                )?,
            )
            })().context(FailureStage::Publish)
        };
        let concurrency = novovm_network::service_concurrency::ServiceConcurrencyV1::from_host();
        let native = if let Some(settings) = native_settings {
            Some(
                novovm_network::product_rendezvous_owned::NativeMailboxListenerV7::bind_adaptive(
                    settings.listen,
                    settings.advertised,
                    key.clone(),
                    stop.clone(),
                    settings.max_bytes_per_minute,
                    concurrency.clone(),
                )
                .context(FailureStage::NativeBind)?,
            )
        } else {
            None
        };
        if native_selected && !dual_selected {
            let listener = native.context("native listener missing")?;
            let result =
                novovm_network::product_rendezvous_owned::run_self_owned_native_mailbox_v7(
                    listener,
                    key,
                    stop,
                    mailbox,
                    move |endpoint, expires| {
                        (|| -> Result<_> {
                            let mut book =
                                self.activate_book(vec![endpoint.endpoint()?], expires)?;
                            book.check_native_mailbox_route_v7(
                                self.policy.node_pin,
                                endpoint,
                                now_ms()?,
                            )?;
                            Ok(Arc::new(ServiceLeaseHostV1::new(self.key.clone(), book)?))
                        })()
                        .context(FailureStage::Activate)
                    },
                    publish,
                )
                .await;
            return stage_result(result, FailureStage::TransportRun);
        }
        let carrier = (|| -> Result<_> {
            let mut carrier = IrohConfigV1::new(effective_relays(&self.policy), config.nameserver)?
                .with_owned_service_identity(self.transport.clone())?
                .with_service_concurrency(concurrency.clone());
            if let Some(bind) = config.bind_address {
                carrier = carrier.with_bind_address(bind)?;
            }
            if relay_only {
                carrier = carrier.with_relay_only()?;
            }
            Ok(carrier)
        })()
        .context(FailureStage::CarrierSetup)?;
        #[cfg(feature = "tls-transport")]
        let tls = if let Some(settings) = &self.policy.tls_listener {
            let mut seed = Zeroizing::new([0u8; 32]);
            hkdf::Hkdf::<sha2::Sha256>::new(Some(&self.policy.node_pin), self.transport.as_ref())
                .expand(b"NOVOVM/self-owned/TLS-transport/v1", seed.as_mut())
                .map_err(|_| anyhow::anyhow!("TLS key derivation failed"))
                .context(FailureStage::CarrierSetup)?;
            let listener =
                novovm_network::product_rendezvous_owned::TlsMailboxListenerV1::bind_adaptive(
                    settings.listen,
                    settings.advertised,
                    seed,
                    stop.clone(),
                )
                .await
                .context(FailureStage::TlsBind)?;
            Some(if settings.native_mailbox_v8 {
                listener.with_native_mailbox_v8()
            } else {
                listener
            })
        } else {
            None
        };
        let activate =
            move |endpoint: &novovm_network::product_service_lease::IrohServiceEndpointV1,
                  expires| {
                self.activate_carrier(endpoint, expires)
                    .context(FailureStage::Activate)
            };
        #[cfg(feature = "tls-transport")]
        let result = if let Some(native) = native {
            novovm_network::product_rendezvous_owned::run_self_owned_node_with_native_tls_v8(
                carrier,
                key,
                config.allowed_clients.clone(),
                stop,
                mailbox,
                native,
                tls,
                activate,
                publish,
            )
            .await
        } else if let Some(tls) = tls {
            novovm_network::product_rendezvous_owned::run_self_owned_node_with_tls_v1(
                carrier,
                key,
                config.allowed_clients.clone(),
                stop,
                mailbox,
                tls,
                activate,
                publish,
            )
            .await
        } else {
            run_self_owned_node_v1(
                carrier,
                key,
                config.allowed_clients.clone(),
                stop,
                Some(mailbox),
                activate,
                publish,
            )
            .await
        };
        #[cfg(not(feature = "tls-transport"))]
        let result = if let Some(native) = native {
            novovm_network::product_rendezvous_owned::run_self_owned_node_with_native_v7(
                carrier,
                key,
                config.allowed_clients.clone(),
                stop,
                mailbox,
                native,
                activate,
                publish,
            )
            .await
        } else {
            run_self_owned_node_v1(
                carrier,
                key,
                config.allowed_clients.clone(),
                stop,
                Some(mailbox),
                activate,
                publish,
            )
            .await
        };
        stage_result(result, FailureStage::TransportRun)
    }
}
fn relay_only(policy: &Policy) -> bool {
    match policy.carrier_mode {
        Some(CarrierMode::Automatic | CarrierMode::DirectOnly) => false,
        Some(CarrierMode::RelayOnly) => true,
        None => policy.config.relay_only,
    }
}
fn effective_relays(policy: &Policy) -> &[String] {
    if matches!(policy.carrier_mode, Some(CarrierMode::DirectOnly)) {
        &[]
    } else {
        &policy.config.relays
    }
}
fn trust(pin: [u8; 32]) -> SelfOwnedTrustV1 {
    SelfOwnedTrustV1 {
        version: 1,
        node_pins: [pin].into(),
    }
}
fn read_keys(root: &Path) -> Result<(SigningKey, Zeroizing<[u8; 32]>)> {
    let material = read_custody(&root.join(KEYS_FILE))?;
    ensure!(
        material.len() == 72 && &material[..8] == b"NVKEYS03",
        "custody version"
    );
    let seed = Zeroizing::new(<[u8; 32]>::try_from(&material[8..40])?);
    let storage = Zeroizing::new(<[u8; 32]>::try_from(&material[40..])?);
    ensure!(
        *seed != *storage && *seed != [0; 32] && *storage != [0; 32],
        "independent owner keys"
    );
    Ok((SigningKey::from_bytes(&seed), storage))
}

#[cfg(windows)]
fn create_custody(path: &Path, data: &[u8]) -> Result<()> {
    create(path, &windows_custody::protect(data)?)
}
#[cfg(windows)]
fn read_custody(path: &Path) -> Result<Zeroizing<Vec<u8>>> {
    windows_custody::unprotect(&read_bounded(path, 65536)?)
}
#[cfg(windows)]
fn save_custody(path: &Path, data: &[u8]) -> Result<()> {
    let sealed = windows_custody::protect(data)?;
    ensure!(
        windows_custody::unprotect(&sealed)?.as_slice() == data,
        "custody roundtrip differs"
    );
    replace_protected(path, &sealed)
}
#[cfg(target_os = "linux")]
use crate::linux_custody::{create_custody, read_custody, save_custody};

/// Replace an already validated DPAPI blob under the exclusive owner lock.
/// Both old and new contents are sealed; no raw key is written to disk. A
/// same-directory rename is the sole commit point, with Windows write-through.
#[cfg(windows)]
fn replace_protected(path: &Path, sealed: &[u8]) -> Result<()> {
    use std::os::windows::ffi::OsStrExt;
    use windows_sys::Win32::Storage::FileSystem::{
        MoveFileExW, MOVEFILE_REPLACE_EXISTING, MOVEFILE_WRITE_THROUGH,
    };
    check_path(path)?;
    ensure!(path.is_file(), "existing custody required");
    let stage = path.with_extension(format!("{:016x}.pending", rand::random::<u64>()));
    create(&stage, sealed)?;
    let from: Vec<_> = stage.as_os_str().encode_wide().chain(Some(0)).collect();
    let to: Vec<_> = path.as_os_str().encode_wide().chain(Some(0)).collect();
    // SAFETY: both zero-terminated paths are live; files are closed and synced,
    // same-directory, checked for reparse points, and owned by the lock holder.
    if unsafe {
        MoveFileExW(
            from.as_ptr(),
            to.as_ptr(),
            MOVEFILE_REPLACE_EXISTING | MOVEFILE_WRITE_THROUGH,
        )
    } == 0
    {
        let error = std::io::Error::last_os_error();
        let _ = std::fs::remove_file(&stage);
        return Err(error.into());
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use novovm_network::{
        mailbox_receipt::StoreReceiptV1,
        mailbox_rpc::{MailboxReplyV1 as Reply, MailboxRequestV1 as Request},
        mailbox_structured::{native_session::*, LIMITS, STREAM},
        novorudp::{
            endpoint::{NativeEndpoint, NativeHandshakeBudget, NativePeer},
            fragments::ReassemblyLimits,
            object_link::{NativeLaneConfig, NativeObjectLink, NativeSendLimits, NativeSendPolicy},
            NovoRudpPacingProfile, NovoRudpWindowConfig,
        },
        product_directory::PeerSignedRelayRecordV1,
        product_service_lease::NativeMailboxEndpointV7,
    };
    use std::{
        net::UdpSocket,
        sync::atomic::Ordering,
        time::{Duration, Instant},
    };

    struct Fixture(Config);
    #[test]
    fn shared_storage_owner_choice_survives_policy_reopen() -> Result<()> {
        let fixture = Fixture::new()?;
        let owner = fixture.provision()?;
        ensure!(owner.policy.shared_storage_limit_mib.is_none());
        owner.set_storage_limit_mib(Some(500))?;
        let owner = fixture.open()?;
        ensure!(owner.policy.shared_storage_limit_mib == Some(500));
        owner.set_storage_limit_mib(None)?;
        let owner = fixture.open()?;
        ensure!(owner.policy.shared_storage_limit_mib.is_none());
        ensure!(owner.set_storage_limit_mib(Some(499)).is_err());
        ensure!(fixture.open()?.policy.shared_storage_limit_mib.is_none());
        ensure!(storage_limits(Some(u64::MAX)).is_err());
        Ok(())
    }

    impl Fixture {
        fn new() -> Result<Self> {
            let root = std::env::temp_dir().join(format!(
                "novovm-native-owner-{:016x}",
                rand::random::<u64>()
            ));
            std::fs::create_dir(&root)?;
            #[cfg(target_os = "linux")]
            {
                use std::os::unix::fs::PermissionsExt;
                std::fs::set_permissions(&root, std::fs::Permissions::from_mode(0o700))?;
                crate::linux_custody::prepare_test_credential()?;
            }
            let config = Config {
                version: 2,
                root,
                allowed_clients: [SigningKey::from_bytes(&[41; 32]).verifying_key().to_bytes()]
                    .into(),
                relays: vec![],
                nameserver: "127.0.0.1".parse()?,
                bind_address: Some("127.0.0.1:0".parse()?),
                relay_only: false,
                record_id: "native-owner-fixture".into(),
            };
            Ok(Self(config))
        }
        fn provision(&self) -> Result<Owner> {
            let owner = Owner::open(self.0.clone(), true, None)?;
            let mut route = settings();
            route.advertised.set_port(1);
            let endpoint = NativeMailboxEndpointV7 {
                address: route.advertised,
                max_connections: route.max_connections,
                max_bytes_per_minute: route.max_bytes_per_minute,
            };
            // Test-only initial journal; run() replaces it with the bound route.
            drop(owner.activate_book(vec![endpoint.endpoint()?], now_ms()? + 90_000)?);
            Ok(owner)
        }
        fn open(&self) -> Result<Owner> {
            Owner::open(self.0.clone(), false, None)
        }
    }
    impl Drop for Fixture {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0.root);
        }
    }
    fn settings() -> NativeListenerConfig {
        NativeListenerConfig {
            version: 1,
            listen: "127.0.0.1:0".parse().unwrap(),
            advertised: "127.0.0.1:0".parse().unwrap(),
            max_connections: 4,
            max_bytes_per_minute: 64 * 1024 * 1024,
        }
    }

    #[test]
    fn failure_evidence_keeps_typed_first_boundary_without_private_error_text() -> Result<()> {
        let error = anyhow::Error::new(std::io::Error::new(
            std::io::ErrorKind::PermissionDenied,
            "private-path-and-identity",
        ))
        .context(FailureStage::Publish);
        let error = stage_result::<()>(Err(error), FailureStage::TransportRun).unwrap_err();
        let evidence = FailureEvidence::from_error(&error);
        assert_eq!(evidence.stage, FailureStage::Publish);
        assert_eq!(evidence.kind, FailureKind::Io);
        assert_eq!(evidence.io_kind, Some("permission_denied"));
        let encoded = serde_json::to_string(&evidence)?;
        assert!(!encoded.contains("private-path"));
        // A similar-looking message is not promoted to an IO or timeout class.
        let untyped = FailureEvidence::from_error(&anyhow::anyhow!("permission denied timeout"));
        assert_eq!(untyped.kind, FailureKind::Other);
        Ok(())
    }

    #[tokio::test(flavor = "current_thread")]
    async fn mailbox_start_failure_is_retained_and_duplicate_start_cannot_overwrite_it(
    ) -> Result<()> {
        let fixture = Fixture::new()?;
        let owner = fixture.provision()?;
        let root = &fixture.0.root;
        // Real incompatible local storage fails before any network is opened.
        std::fs::create_dir(root.join("mailbox.sqlite"))?;
        owner
            .run(Arc::new(AtomicBool::new(false)))
            .await
            .unwrap_err();
        let bytes = std::fs::read(root.join("last-failure.json"))?;
        let failure: serde_json::Value = serde_json::from_slice(&bytes)?;
        assert_eq!(failure["failure"]["stage"], "mailbox_open");
        let status: serde_json::Value =
            serde_json::from_slice(&std::fs::read(root.join("status.json"))?)?;
        assert_eq!(status["phase"], "failed_closed");
        assert_eq!(status["failure"], failure["failure"]);
        let reopened = fixture.open()?;
        let status_before = std::fs::read(root.join("status.json"))?;
        assert!(fixture.open().is_err());
        assert_eq!(std::fs::read(root.join("status.json"))?, status_before);
        assert_eq!(std::fs::read(root.join("last-failure.json"))?, bytes);
        // Successful later lifecycle reporting does not erase the last failure.
        let evidence = RunEvidence {
            root: root.clone(),
            _lock: reopened._lock.clone(),
        };
        atomic(
            &root.join("status.json"),
            br#"{"version":2,"phase":"starting"}"#,
        )?;
        evidence.finish(Ok(()))?;
        assert_eq!(std::fs::read(root.join("last-failure.json"))?, bytes);
        Ok(())
    }

    #[tokio::test(flavor = "current_thread")]
    async fn occupied_native_listener_reports_binding_without_changing_identity_or_journal(
    ) -> Result<()> {
        let fixture = Fixture::new()?;
        let socket = UdpSocket::bind("127.0.0.1:0")?;
        let mut listener = settings();
        listener.listen = socket.local_addr()?;
        listener.advertised = socket.local_addr()?;
        fixture.provision()?.set_native_listener(Some(listener))?;
        let root = &fixture.0.root;
        let baseline = [KEYS_FILE, POLICY_FILE, "service.journal"]
            .map(|name| std::fs::read(root.join(name)).unwrap());
        let error = fixture
            .open()?
            .run(Arc::new(AtomicBool::new(false)))
            .await
            .unwrap_err();
        assert_eq!(
            error.downcast_ref::<FailureStage>(),
            Some(&FailureStage::NativeBind)
        );
        let bytes = std::fs::read(root.join("last-failure.json"))?;
        assert!(
            bytes.len() < 512,
            "one fixed-size diagnostic, not an error log"
        );
        let failure: serde_json::Value = serde_json::from_slice(&bytes)?;
        assert_eq!(failure["failure"]["kind"], "io");
        assert_eq!(failure["failure"]["ioKind"], "address_in_use");
        for (name, expected) in [KEYS_FILE, POLICY_FILE, "service.journal"]
            .iter()
            .zip(baseline)
        {
            assert_eq!(std::fs::read(root.join(name))?, expected);
        }
        Ok(())
    }

    #[test]
    fn failed_status_write_does_not_replace_original_runtime_failure() -> Result<()> {
        let fixture = Fixture::new()?;
        let owner = fixture.provision()?;
        let root = &fixture.0.root;
        std::fs::create_dir(root.join("status.json"))?;
        let evidence = RunEvidence {
            root: root.clone(),
            _lock: owner._lock.clone(),
        };
        let original = anyhow::Error::new(std::io::Error::new(
            std::io::ErrorKind::AddrInUse,
            "private-listener-address",
        ))
        .context(FailureStage::NativeBind);
        let returned = evidence.finish(Err(original)).unwrap_err();
        assert!(returned.downcast_ref::<EvidenceWriteFailed>().is_some());
        assert_eq!(
            returned.downcast_ref::<FailureStage>(),
            Some(&FailureStage::NativeBind)
        );
        assert_eq!(
            returned.downcast_ref::<std::io::Error>().unwrap().kind(),
            std::io::ErrorKind::AddrInUse
        );
        let bytes = std::fs::read(root.join("last-failure.json"))?;
        let saved: serde_json::Value = serde_json::from_slice(&bytes)?;
        assert_eq!(saved["failure"]["stage"], "native_bind");
        assert_eq!(saved["failure"]["snapshotWriteFailed"], true);
        assert!(!String::from_utf8(bytes)?.contains("private-listener"));
        // The lock outlives Owner consumption and still excludes a second writer.
        drop(owner);
        assert!(fixture.open().is_err());
        drop(evidence);
        drop(fixture.open()?);
        Ok(())
    }

    #[tokio::test(flavor = "current_thread")]
    async fn typed_timeout_is_not_reported_as_an_io_or_rejection() -> Result<()> {
        let elapsed = tokio::time::timeout(Duration::ZERO, std::future::pending::<()>())
            .await
            .unwrap_err();
        let evidence = FailureEvidence::from_error(
            &anyhow::Error::new(elapsed).context(FailureStage::TransportRun),
        );
        assert_eq!(evidence.kind, FailureKind::Timeout);
        assert_eq!(evidence.io_kind, None);
        Ok(())
    }

    #[test]
    fn native_carrier_policy_opens_old_schema_without_rewriting_custody() -> Result<()> {
        let fixture = Fixture::new()?;
        let owner = fixture.provision()?;
        let pin = owner.policy.node_pin;
        let transport = owner.transport.clone();
        let baseline = [KEYS_FILE, "service.journal"]
            .map(|name| std::fs::read(fixture.0.root.join(name)).unwrap());
        let mut old_policy = serde_json::to_value(&owner.policy)?;
        old_policy
            .as_object_mut()
            .context("policy object")?
            .remove("native_carrier_mailbox_v7");
        old_policy
            .as_object_mut()
            .context("policy object")?
            .remove("native_carrier_mailbox_v8");
        let mut material = Zeroizing::new(Vec::new());
        material.extend_from_slice(b"NVOWN002");
        material.extend_from_slice(owner.transport.as_ref());
        material.extend_from_slice(&serde_json::to_vec(&old_policy)?);
        let path = fixture.0.root.join(POLICY_FILE);
        save_custody(&path, &material)?;
        drop(owner);
        let old_custody = std::fs::read(&path)?;

        let owner = fixture.open()?;
        assert!(!owner.native_carrier_mailbox_v7());
        assert!(!owner.native_carrier_mailbox_v8());
        assert_eq!(owner.policy.node_pin, pin);
        assert!(*owner.transport == *transport);
        assert_eq!(std::fs::read(&path)?, old_custody);
        owner.set_native_carrier_mailbox_v7(true)?;
        let owner = fixture.open()?;
        assert!(owner.native_carrier_mailbox_v7());
        assert_eq!(owner.policy.node_pin, pin);
        assert!(*owner.transport == *transport);
        // Opt-in adds only the new setting; no provenance or trust migration.
        let mut new_policy = serde_json::to_value(&owner.policy)?;
        assert_eq!(new_policy["native_carrier_mailbox_v7"], true);
        new_policy
            .as_object_mut()
            .context("policy object")?
            .remove("native_carrier_mailbox_v7");
        assert_eq!(new_policy["native_carrier_mailbox_v8"], false);
        new_policy
            .as_object_mut()
            .context("policy object")?
            .remove("native_carrier_mailbox_v8");
        assert_eq!(new_policy, old_policy);
        for (name, expected) in [KEYS_FILE, "service.journal"].iter().zip(baseline) {
            assert_eq!(std::fs::read(fixture.0.root.join(name))?, expected);
        }
        Ok(())
    }

    #[test]
    fn native_carrier_v8_does_not_upgrade_old_v7_policy_on_open() -> Result<()> {
        let fixture = Fixture::new()?;
        fixture.provision()?.set_native_carrier_mailbox_v7(true)?;
        let owner = fixture.open()?;
        let mut old_policy = serde_json::to_value(&owner.policy)?;
        old_policy
            .as_object_mut()
            .context("policy object")?
            .remove("native_carrier_mailbox_v8");
        let mut material = Zeroizing::new(Vec::new());
        material.extend_from_slice(b"NVOWN002");
        material.extend_from_slice(owner.transport.as_ref());
        material.extend_from_slice(&serde_json::to_vec(&old_policy)?);
        let path = fixture.0.root.join(POLICY_FILE);
        save_custody(&path, &material)?;
        let pin = owner.policy.node_pin;
        let transport = owner.transport.clone();
        drop(owner);
        let names = [KEYS_FILE, POLICY_FILE, "service.journal"];
        let baseline = names.map(|name| std::fs::read(fixture.0.root.join(name)).unwrap());
        let owner = fixture.open()?;
        assert!(owner.native_carrier_mailbox_v7());
        assert!(!owner.native_carrier_mailbox_v8());
        assert_eq!(owner.policy.node_pin, pin);
        assert!(*owner.transport == *transport);
        for (name, expected) in names.iter().zip(baseline) {
            assert_eq!(std::fs::read(fixture.0.root.join(name))?, expected);
        }
        Ok(())
    }

    #[test]
    fn native_carrier_policy_roundtrip_preserves_identity_and_allows_v8_udp() -> Result<()> {
        let fixture = Fixture::new()?;
        let owner = fixture.provision()?;
        assert!(!owner.native_carrier_mailbox_v7());
        let policy = serde_json::to_value(&owner.policy)?;
        let pin = owner.policy.node_pin;
        let transport = owner.transport.clone();
        let baseline = [KEYS_FILE, "service.journal"]
            .map(|name| std::fs::read(fixture.0.root.join(name)).unwrap());
        let path = fixture.0.root.join(POLICY_FILE);

        owner.set_native_carrier_mailbox_v7(true)?;
        let owner = fixture.open()?;
        assert!(owner.native_carrier_mailbox_v7());
        assert!(
            fixture.open().is_err(),
            "the existing owner lock still applies"
        );
        let enabled = std::fs::read(&path)?;
        assert!(owner.set_native_listener(Some(settings())).is_err());
        assert_eq!(std::fs::read(&path)?, enabled);

        fixture.open()?.set_native_carrier_mailbox_v8(true)?;
        let owner = fixture.open()?;
        assert!(!owner.native_carrier_mailbox_v7());
        assert!(owner.native_carrier_mailbox_v8());
        owner.set_native_listener(Some(settings()))?;
        let owner = fixture.open()?;
        assert_eq!(owner.policy.native_listener, Some(settings()));
        assert!(owner.native_carrier_mailbox_v8());
        owner.set_native_listener(None)?;

        // The existing CLI's off action must also disable a selected V8.
        fixture.open()?.set_native_carrier_mailbox_v7(false)?;
        let owner = fixture.open()?;
        assert!(!owner.native_carrier_mailbox_v7());
        assert!(!owner.native_carrier_mailbox_v8());
        owner.set_native_listener(Some(settings()))?;
        let native = std::fs::read(&path)?;
        assert!(fixture.open()?.set_native_carrier_mailbox_v7(true).is_err());
        assert_eq!(std::fs::read(&path)?, native);
        fixture.open()?.set_native_carrier_mailbox_v8(true)?;
        let owner = fixture.open()?;
        assert_eq!(owner.policy.native_listener, Some(settings()));
        assert!(!owner.native_carrier_mailbox_v7());
        assert!(owner.native_carrier_mailbox_v8());
        owner.set_native_listener(None)?;

        fixture.open()?.set_native_carrier_mailbox_v8(true)?;
        fixture.open()?.set_native_carrier_mailbox_v7(true)?;
        let owner = fixture.open()?;
        assert!(owner.native_carrier_mailbox_v7());
        assert!(!owner.native_carrier_mailbox_v8());
        drop(owner);
        fixture.open()?.set_native_carrier_mailbox_v7(false)?;
        let owner = fixture.open()?;
        assert_eq!(serde_json::to_value(&owner.policy)?, policy);
        assert_eq!(owner.policy.node_pin, pin);
        assert!(*owner.transport == *transport);
        for (name, expected) in [KEYS_FILE, "service.journal"].iter().zip(baseline) {
            assert_eq!(std::fs::read(fixture.0.root.join(name))?, expected);
        }
        Ok(())
    }

    #[test]
    fn native_carrier_policy_rejects_authenticated_conflicting_state_before_start() -> Result<()> {
        for (v7, v8, native) in [(true, false, true), (true, true, false)] {
            let fixture = Fixture::new()?;
            let mut owner = fixture.provision()?;
            // A correctly sealed but contradictory policy must fail at open,
            // before choosing a version or binding any transport.
            owner.policy.native_carrier_mailbox_v7 = v7;
            owner.policy.native_carrier_mailbox_v8 = v8;
            owner.policy.native_listener = native.then(settings);
            owner.save_policy()?;
            drop(owner);
            let names = [KEYS_FILE, POLICY_FILE, "service.journal"];
            let baseline = names.map(|name| std::fs::read(fixture.0.root.join(name)).unwrap());
            assert!(fixture.open().is_err());
            for (name, expected) in names.iter().zip(baseline) {
                assert_eq!(std::fs::read(fixture.0.root.join(name))?, expected);
            }
            assert!(!fixture.0.root.join("mailbox.started").exists());
            assert!(!fixture.0.root.join("status.json").exists());
        }
        Ok(())
    }

    #[test]
    fn native_carrier_activation_switches_signed_version_without_identity_or_expiry_reset(
    ) -> Result<()> {
        use novovm_network::{
            iroh_transport::IrohAddressV1,
            product_service_lease::{
                IrohServiceEndpointV1, NativeCarrierMailboxEndpointV7,
                NativeCarrierMailboxEndpointV8,
            },
        };
        let fixture = Fixture::new()?;
        let owner = fixture.provision()?;
        let pin = owner.policy.node_pin;
        let transport = owner.transport.clone();
        let key_custody = std::fs::read(fixture.0.root.join(KEYS_FILE))?;
        let endpoint = IrohServiceEndpointV1 {
            instance: [7; 32],
            address: IrohAddressV1 {
                endpoint_key: SigningKey::from_bytes(&owner.transport)
                    .verifying_key()
                    .to_bytes(),
                direct: vec!["127.0.0.1:23456".parse()?],
                relays: vec![],
            },
        };
        drop(owner);
        let expires = now_ms()? + 90_000;
        let mut sequence = 0;
        for version in [0, 7, 8, 7, 8, 0] {
            let owner = fixture.open()?;
            match version {
                8 => owner.set_native_carrier_mailbox_v8(true)?,
                7 => owner.set_native_carrier_mailbox_v7(true)?,
                _ => owner.set_native_carrier_mailbox_v7(false)?,
            }
            let owner = fixture.open()?;
            assert_eq!(owner.policy.node_pin, pin);
            assert!(*owner.transport == *transport);
            // This is the activation path actually passed to the running Node.
            drop(owner.activate_carrier(&endpoint, expires)?);
            let (_journal, bytes) = ServiceLeaseFileV1::open(
                &fixture.0.root.join("service.journal"),
                owner.storage.clone(),
            )?;
            let book = ServiceLeaseBookV1::restore_self_owned(trust(pin), &bytes, now_ms()?)?;
            let records = book.self_owned_records()?;
            assert_eq!(records.len(), 1);
            let record = &records[0];
            assert_eq!(record.relay_public_key, pin);
            assert_eq!(record.expires_at_ms, expires);
            assert!(record.sequence > sequence);
            assert_eq!(
                NativeCarrierMailboxEndpointV7::from_record(record)?.is_some(),
                version == 7
            );
            assert_eq!(
                NativeCarrierMailboxEndpointV8::from_record(record)?.is_some(),
                version == 8
            );
            let actual = IrohServiceEndpointV1::from_record(record)?;
            assert_eq!(actual.instance, endpoint.instance);
            assert_eq!(actual.address.endpoint_key, endpoint.address.endpoint_key);
            assert_eq!(actual.address.direct, endpoint.address.direct);
            assert_eq!(std::fs::read(fixture.0.root.join(KEYS_FILE))?, key_custody);
            sequence = record.sequence;
        }
        Ok(())
    }

    #[test]
    fn native_carrier_v8_selection_cannot_revive_revoked_journal() -> Result<()> {
        use novovm_network::{
            iroh_transport::IrohAddressV1, product_service_lease::IrohServiceEndpointV1,
        };
        let fixture = Fixture::new()?;
        let owner = fixture.provision()?;
        let pin = owner.policy.node_pin;
        let endpoint = IrohServiceEndpointV1 {
            instance: [7; 32],
            address: IrohAddressV1 {
                endpoint_key: SigningKey::from_bytes(&owner.transport)
                    .verifying_key()
                    .to_bytes(),
                direct: vec!["127.0.0.1:23456".parse()?],
                relays: vec![],
            },
        };
        let mut book = owner.activate_book(vec![endpoint.endpoint()?], now_ms()? + 90_000)?;
        book.revoke_self_owned(pin, now_ms()?)?;
        drop(book);
        let path = fixture.0.root.join("service.journal");
        let revoked = std::fs::read(&path)?;
        owner.set_native_carrier_mailbox_v8(true)?;
        let owner = fixture.open()?;
        assert!(owner.native_carrier_mailbox_v8());
        assert!(owner
            .activate_carrier(&endpoint, now_ms()? + 90_000)
            .is_err());
        assert_eq!(std::fs::read(&path)?, revoked);
        Ok(())
    }

    #[cfg(feature = "tls-transport")]
    fn tls_settings(native: bool) -> TlsListenerConfig {
        TlsListenerConfig {
            version: 1,
            listen: "127.0.0.1:0".parse().unwrap(),
            advertised: "127.0.0.1:0".parse().unwrap(),
            max_connections: 4,
            native_mailbox_v8: native,
        }
    }

    #[cfg(feature = "tls-transport")]
    #[test]
    fn native_tls_policy_requires_explicit_v8_and_preserves_old_custody() -> Result<()> {
        let old = r#"{"version":1,"listen":"127.0.0.1:0","advertised":"127.0.0.1:0","max_connections":4}"#;
        let parsed: TlsListenerConfig = serde_json::from_str(old)?;
        assert!(!parsed.native_mailbox_v8);
        assert_eq!(serde_json::to_string(&parsed)?, old);
        let fixture = Fixture::new()?;
        let owner = fixture.provision()?;
        let pin = owner.policy.node_pin;
        let transport = owner.transport.clone();
        let baseline =
            [KEYS_FILE, "service.journal"].map(|p| std::fs::read(fixture.0.root.join(p)).unwrap());
        let policy_path = fixture.0.root.join(POLICY_FILE);
        let original = std::fs::read(&policy_path)?;
        assert!(owner.set_tls_listener(Some(tls_settings(true))).is_err());
        assert_eq!(std::fs::read(&policy_path)?, original);
        fixture.open()?.set_native_carrier_mailbox_v8(true)?;
        fixture.open()?.set_tls_listener(Some(tls_settings(true)))?;
        let enabled = std::fs::read(&policy_path)?;
        assert!(fixture.open()?.set_native_carrier_mailbox_v7(true).is_err());
        assert!(fixture
            .open()?
            .set_native_carrier_mailbox_v7(false)
            .is_err());
        assert!(fixture
            .open()?
            .set_native_carrier_mailbox_v8(false)
            .is_err());
        assert_eq!(std::fs::read(&policy_path)?, enabled);
        let owner = fixture.open()?;
        assert!(
            owner
                .policy
                .tls_listener
                .as_ref()
                .unwrap()
                .native_mailbox_v8
        );
        assert_eq!(owner.policy.node_pin, pin);
        assert_eq!(*owner.transport, *transport);
        assert!(fixture.open().is_err());
        owner.set_tls_listener(Some(tls_settings(false)))?;
        fixture.open()?.set_native_carrier_mailbox_v7(true)?;
        for (name, bytes) in [KEYS_FILE, "service.journal"].iter().zip(baseline) {
            assert_eq!(std::fs::read(fixture.0.root.join(name))?, bytes);
        }
        // Even an internally sealed contradictory policy fails before binding.
        let mut owner = fixture.open()?;
        owner.policy.tls_listener = Some(tls_settings(true));
        owner.save_policy()?;
        drop(owner);
        let contradictory = std::fs::read(&policy_path)?;
        assert!(fixture.open().is_err());
        assert_eq!(std::fs::read(&policy_path)?, contradictory);
        assert!(!fixture.0.root.join("status.json").exists());
        Ok(())
    }

    #[cfg(feature = "tls-transport")]
    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn native_tls_real_owner_publishes_and_authenticates_after_restart() -> Result<()> {
        use novovm_network::{
            bound_session::peer_time::authenticate_mailbox_native_v8,
            carrier_stream::CarrierStreamV1, duplex::HandshakeReplayCacheV1,
            product_service_lease::TlsServiceEndpointV1, tls_transport::TlsScopeV1,
        };
        let fixture = Fixture::new()?;
        fixture.provision()?.set_native_carrier_mailbox_v8(true)?;
        fixture.open()?.set_tls_listener(Some(tls_settings(true)))?;
        let keys = std::fs::read(fixture.0.root.join(KEYS_FILE))?;
        let mut sequence = 0;
        let mut tls_key = None;
        for _ in 0..2 {
            let owner = fixture.open()?;
            let pin = owner.policy.node_pin;
            let root = fixture.0.root.clone();
            let stop = Arc::new(AtomicBool::new(false));
            let serving = owner.run(stop.clone());
            tokio::pin!(serving);
            let work = async move {
                let until = Instant::now() + Duration::from_secs(10);
                let record: PeerSignedRelayRecordV1 = loop {
                    ensure!(Instant::now() < until, "native TLS publication deadline");
                    if let Ok(bytes) = std::fs::read(root.join("self-owned-record.json")) {
                        let record: PeerSignedRelayRecordV1 = serde_json::from_slice(&bytes)?;
                        let status: serde_json::Value =
                            serde_json::from_slice(&std::fs::read(root.join("status.json"))?)?;
                        if status["phase"] == "serving" && record.sequence > sequence {
                            break record;
                        }
                    }
                    tokio::time::sleep(Duration::from_millis(5)).await;
                };
                assert_eq!(record.relay_public_key, pin);
                let route =
                    TlsServiceEndpointV1::from_record(&record)?.context("TLS record missing")?;
                assert!(route.native_mailbox_v8);
                assert_ne!(route.address.port(), 0);
                if let Some(prior) = tls_key {
                    assert_eq!(route.endpoint_key, prior);
                }
                let scope = TlsScopeV1::new_continuous_v4(|| Ok(()))?;
                let tcp = tokio::net::TcpStream::connect(route.address).await?;
                tcp.set_nodelay(true)?;
                let mut stream =
                    CarrierStreamV1::from(scope.connect(tcp, route.endpoint_key).await?);
                let (_, observed) = authenticate_mailbox_native_v8(
                    &mut stream,
                    &SigningKey::from_bytes(&[41; 32]),
                    pin,
                    until,
                    &mut HandshakeReplayCacheV1::new(8),
                    &mut |_, _| Ok(()),
                )
                .await?;
                assert_eq!(observed.peer_identity_public_key, pin);
                Ok::<_, anyhow::Error>((record.sequence, route.endpoint_key))
            };
            tokio::pin!(work);
            let outcome = tokio::select! {
                biased;
                result = &mut serving => { result?; anyhow::bail!("native TLS owner ended early") },
                result = tokio::time::timeout(Duration::from_secs(12), &mut work) => result?,
            };
            stop.store(true, Ordering::Release);
            let stopped = tokio::time::timeout(Duration::from_secs(5), &mut serving).await?;
            // Existing owned Iroh scope reports explicit owner stop at its final
            // authority check. Match only that known terminal result in this
            // fixture; do not suppress transport/authentication failures in runtime.
            if let Err(error) = stopped {
                assert_eq!(
                    FailureEvidence::from_error(&error).stage,
                    FailureStage::TransportRun
                );
                ensure!(
                    error.root_cause().to_string() == "service stopped by owner",
                    "unexpected stop: {error:#}"
                );
            }
            let (next, key) = outcome?;
            assert!(next > sequence);
            sequence = next;
            tls_key = Some(key);
            assert_eq!(std::fs::read(fixture.0.root.join(KEYS_FILE))?, keys);
        }
        Ok(())
    }

    #[test]
    fn direct_only_policy_keeps_relay_preferences_keys_and_history() -> Result<()> {
        let mut fixture = Fixture::new()?;
        fixture.0.relays = vec!["https://unused.example.invalid/".into()];
        let owner = fixture.provision()?;
        let baseline =
            [KEYS_FILE, "service.journal"].map(|p| std::fs::read(fixture.0.root.join(p)).unwrap());
        let pin = owner.policy.node_pin;
        owner.set_carrier_mode(CarrierMode::DirectOnly)?;
        let owner = fixture.open()?;
        assert_eq!(owner.policy.node_pin, pin);
        assert_eq!(owner.policy.config.relays, fixture.0.relays);
        assert!(effective_relays(&owner.policy).is_empty());
        assert!(!relay_only(&owner.policy));
        assert!(
            fixture.open().is_err(),
            "exclusive policy owner was bypassed"
        );
        owner.set_carrier_mode(CarrierMode::Automatic)?;
        let owner = fixture.open()?;
        assert_eq!(effective_relays(&owner.policy), fixture.0.relays);
        for (name, bytes) in [KEYS_FILE, "service.journal"].into_iter().zip(baseline) {
            assert_eq!(std::fs::read(fixture.0.root.join(name))?, bytes);
        }
        Ok(())
    }

    #[test]
    fn native_listener_policy_roundtrip_preserves_identity_and_history() -> Result<()> {
        let fixture = Fixture::new()?;
        let owner = fixture.provision()?;
        let pin = owner.policy.node_pin;
        let transport = owner.transport.clone();
        let baseline =
            [KEYS_FILE, "service.journal"].map(|p| std::fs::read(fixture.0.root.join(p)).unwrap());
        owner.set_carrier_mode(CarrierMode::Automatic)?;
        #[cfg(feature = "tls-transport")]
        fixture.open()?.set_tls_listener(Some(TlsListenerConfig {
            version: 1,
            listen: "127.0.0.1:0".parse()?,
            advertised: "127.0.0.1:0".parse()?,
            max_connections: 4,
            native_mailbox_v8: false,
        }))?;
        fixture.open()?.set_native_listener(Some(settings()))?;
        let owner = fixture.open()?;
        assert_eq!(owner.policy.native_listener, Some(settings()));
        assert_eq!(owner.policy.node_pin, pin);
        assert_eq!(*owner.transport, *transport);
        assert!(matches!(
            owner.policy.carrier_mode,
            Some(CarrierMode::Automatic)
        ));
        #[cfg(feature = "tls-transport")]
        assert!(owner.policy.tls_listener.is_some());
        assert!(
            fixture.open().is_err(),
            "configuration cannot bypass running owner lock"
        );
        let sealed = std::fs::read(fixture.0.root.join(POLICY_FILE))?;
        assert!(owner.set_carrier_mode(CarrierMode::RelayOnly).is_err());
        assert_eq!(std::fs::read(fixture.0.root.join(POLICY_FILE))?, sealed);
        let mut bad = settings();
        bad.max_connections = 0;
        assert!(fixture.open()?.set_native_listener(Some(bad)).is_err());
        assert_eq!(std::fs::read(fixture.0.root.join(POLICY_FILE))?, sealed);
        fixture.open()?.set_native_listener(None)?;
        let restored = fixture.open()?;
        assert!(restored.policy.native_listener.is_none());
        assert_eq!(restored.policy.node_pin, pin);
        assert_eq!(*restored.transport, *transport);
        for (name, bytes) in [KEYS_FILE, "service.journal"].iter().zip(baseline) {
            assert_eq!(std::fs::read(fixture.0.root.join(name))?, bytes);
        }
        Ok(())
    }

    #[test]
    fn native_listener_rejects_invalid_configuration_before_policy_mutation() -> Result<()> {
        let fixture = Fixture::new()?;
        let path = fixture.0.root.join("native.json");
        let good = settings();
        std::fs::write(&path, serde_json::to_vec(&good)?)?;
        assert_eq!(NativeListenerConfig::load(&path)?, good);
        let mut invalid = Vec::new();
        let mut v = good.clone();
        v.version = 2;
        invalid.push(v);
        let mut v = good.clone();
        v.max_connections = 65;
        invalid.push(v);
        let mut v = good.clone();
        v.max_bytes_per_minute = 0;
        invalid.push(v);
        let mut v = good.clone();
        v.advertised = "0.0.0.0:1".parse()?;
        invalid.push(v);
        let mut v = good.clone();
        v.advertised = "[::1]:1".parse()?;
        invalid.push(v);
        let mut v = good.clone();
        v.listen = "224.0.0.1:1".parse()?;
        invalid.push(v);
        for value in invalid {
            std::fs::write(&path, serde_json::to_vec(&value)?)?;
            assert!(NativeListenerConfig::load(&path).is_err());
        }
        Ok(())
    }

    #[test]
    fn native_listener_journal_switches_carriers_without_reset() -> Result<()> {
        use novovm_network::{
            iroh_transport::IrohAddressV1, product_service_lease::IrohServiceEndpointV1,
        };
        let fixture = Fixture::new()?;
        let owner = fixture.provision()?;
        let pin = owner.policy.node_pin;
        let iroh = IrohServiceEndpointV1 {
            instance: [7; 32],
            address: IrohAddressV1 {
                endpoint_key: SigningKey::from_bytes(&owner.transport)
                    .verifying_key()
                    .to_bytes(),
                direct: vec!["127.0.0.1:23456".parse()?],
                relays: vec![],
            },
        };
        let native = NativeMailboxEndpointV7 {
            address: "127.0.0.1:23457".parse()?,
            max_connections: 4,
            max_bytes_per_minute: settings().max_bytes_per_minute,
        };
        let mut sequence = 0;
        for endpoint in [iroh.endpoint()?, native.endpoint()?, iroh.endpoint()?] {
            let book = owner.activate_book(vec![endpoint.clone()], now_ms()? + 90_000)?;
            let records = book.self_owned_records()?;
            assert_eq!(records.len(), 1);
            assert_eq!(records[0].relay_public_key, pin);
            assert!(records[0].sequence > sequence);
            assert_eq!(records[0].endpoints.len(), 1);
            assert_eq!(records[0].endpoints[0].uri, endpoint.uri);
            sequence = records[0].sequence;
        }
        Ok(())
    }

    struct Client {
        link: NativeObjectLink,
        session: NativeMailboxClient,
    }
    impl Client {
        async fn connect(record: &PeerSignedRelayRecordV1) -> Result<Self> {
            let route = NativeMailboxEndpointV7::from_record(record)?.context("native route")?;
            let mut book = ServiceLeaseBookV1::new_self_owned(
                trust(record.relay_public_key),
                vec![record.clone()],
                now_ms()?,
            )?;
            book.check_native_mailbox_route_v7(record.relay_public_key, &route, now_ms()?)?;
            let mut link = NativeObjectLink::new(
                NativeEndpoint::connect_client(
                    UdpSocket::bind("127.0.0.1:0")?,
                    SigningKey::from_bytes(&[52; 32]),
                    NativePeer {
                        key: record.relay_public_key,
                        address: route.address,
                    },
                    NativeHandshakeBudget {
                        lifetime: Duration::from_secs(5),
                        retry: Duration::from_millis(50),
                    },
                    now_ms()?,
                    Instant::now(),
                )?,
                vec![NativeLaneConfig {
                    stream: STREAM,
                    domain: domain(),
                    receive: ReassemblyLimits {
                        max_message_bytes: LIMITS.wire_bytes,
                        messages: 4,
                        bytes: LIMITS.wire_bytes * 4,
                        peer_messages: 4,
                        peer_bytes: LIMITS.wire_bytes * 4,
                        ttl: REQUEST_DEADLINE,
                    },
                    send: NativeSendLimits {
                        messages: 4,
                        bytes: LIMITS.wire_bytes * 4,
                        peer_messages: 4,
                        peer_bytes: LIMITS.wire_bytes * 4,
                    },
                }],
                NativeSendPolicy {
                    window: NovoRudpWindowConfig::default(),
                    pacing: NovoRudpPacingProfile::default(),
                    ttl: REQUEST_DEADLINE,
                },
            )?;
            let until = Instant::now() + Duration::from_secs(5);
            while link.session_id(0).is_none() {
                ensure!(Instant::now() < until, "fixture handshake deadline");
                let _ = link.poll(now_ms()?, Instant::now(), 64)?;
                tokio::time::sleep(Duration::from_millis(1)).await;
            }
            let session = NativeMailboxClient::attach(&mut link, 0, record.relay_public_key)?;
            Ok(Self { link, session })
        }
        async fn request(&mut self, request: Request) -> Result<Reply> {
            let id = self.session.begin(0, vec![request], Instant::now())?;
            let until = Instant::now() + Duration::from_secs(8);
            loop {
                let now = Instant::now();
                ensure!(now < until, "fixture request deadline");
                self.session.progress_capacity(&mut self.link, now)?;
                if self.session.negotiated_retained_bytes().is_some() {
                    self.session.try_send(&mut self.link, id, now, || Ok(()))?;
                }
                for done in self.link.poll(now_ms()?, now, 64)?.completed {
                    ensure!(
                        self.session.transport_complete(&done).is_none(),
                        "fixture request transport failed"
                    );
                }
                if let Some(body) = self.link.poll_complete(STREAM, now, 64)? {
                    if !self.session.receive_capacity(&self.link, &body, now)? {
                        let (got, mut replies) =
                            self.session.receive(&self.link, body, now, || Ok(()))?;
                        ensure!(got == id && replies.len() == 1, "fixture response binding");
                        return Ok(replies.remove(0));
                    }
                }
                tokio::time::sleep(Duration::from_millis(1)).await;
            }
        }
    }

    async fn exchange(
        fixture: &Fixture,
        prior: Option<&StoreReceiptV1>,
        expires: i64,
    ) -> Result<(StoreReceiptV1, u64)> {
        let owner = fixture.open()?;
        let pin = owner.policy.node_pin;
        let root = fixture.0.root.clone();
        let stop = Arc::new(AtomicBool::new(false));
        let serving = owner.run(stop.clone());
        tokio::pin!(serving);
        let work = async {
            let until = Instant::now() + Duration::from_secs(5);
            let record: PeerSignedRelayRecordV1 = loop {
                ensure!(Instant::now() < until, "fixture publish deadline");
                if let Ok(bytes) = std::fs::read(root.join("self-owned-record.json")) {
                    let record: PeerSignedRelayRecordV1 = serde_json::from_slice(&bytes)?;
                    // This future starts after the owner writes status=starting;
                    // publish rewrites status=serving after committing this record.
                    let status: serde_json::Value =
                        serde_json::from_slice(&std::fs::read(root.join("status.json"))?)?;
                    if status["phase"] == "serving" {
                        assert_eq!(status["carrierMode"], "native");
                        break record;
                    }
                }
                tokio::time::sleep(Duration::from_millis(1)).await;
            };
            assert_eq!(record.relay_public_key, pin);
            let mut client = Client::connect(&record).await?;
            if prior.is_none() {
                ensure!(matches!(
                    client
                        .request(Request::Register {
                            mailbox: [1; 32],
                            write: [2; 32],
                            read: [3; 32],
                            expires
                        })
                        .await?,
                    Reply::Registered
                ));
            }
            if prior.is_some() {
                // Prove recovery before any retransmitted Put could restore data.
                let Reply::Object {
                    ciphertext,
                    expires: actual,
                    ..
                } = client
                    .request(Request::Fetch {
                        mailbox: [1; 32],
                        read: [3; 32],
                    })
                    .await?
                else {
                    anyhow::bail!("retained custody missing after restart")
                };
                assert_eq!(ciphertext, vec![8; 256]);
                assert_eq!(actual, expires);
            }
            let put = Request::Put {
                mailbox: [1; 32],
                write: [2; 32],
                object: [4; 32],
                ciphertext: vec![8; 256],
                expires,
            };
            let Reply::Stored(receipt) = client.request(put).await? else {
                anyhow::bail!("fixture Put not stored")
            };
            receipt.verify(pin, [1; 32], [4; 32], &vec![8; 256], expires)?;
            if let Some(previous) = prior {
                assert_eq!(receipt.signature, previous.signature);
            }
            let Reply::Object {
                ciphertext,
                expires: actual,
                ..
            } = client
                .request(Request::Fetch {
                    mailbox: [1; 32],
                    read: [3; 32],
                })
                .await?
            else {
                anyhow::bail!("fixture Fetch missing")
            };
            assert_eq!(ciphertext, vec![8; 256]);
            assert_eq!(actual, expires);
            Ok::<_, anyhow::Error>((receipt, record.sequence))
        };
        tokio::pin!(work);
        let outcome = tokio::select! {
            biased;
            result = &mut serving => { result?; anyhow::bail!("fixture owner ended early") },
            result = &mut work => result,
        };
        stop.store(true, Ordering::Release);
        tokio::time::timeout(Duration::from_secs(5), &mut serving).await??;
        outcome
    }

    #[tokio::test(flavor = "current_thread")]
    async fn native_listener_real_owner_udp_custody_survives_restart() -> Result<()> {
        let fixture = Fixture::new()?;
        fixture.provision()?.set_native_listener(Some(settings()))?;
        let keys = std::fs::read(fixture.0.root.join(KEYS_FILE))?;
        let expires = (now_ms()? / 1000) as i64 + 600;
        let (first, sequence) = exchange(&fixture, None, expires).await?;
        let (second, next) = exchange(&fixture, Some(&first), expires).await?;
        assert!(next > sequence);
        assert_eq!(first.signature, second.signature);
        assert_eq!(std::fs::read(fixture.0.root.join(KEYS_FILE))?, keys);
        Ok(())
    }

    include!("rendezvous_dual_owner_tests.rs");
}

#[cfg(all(test, target_os = "linux"))]
#[path = "rendezvous_linux_owner_tests.rs"]
mod linux_tests;
