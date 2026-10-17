//! Windows ownership for the shared self-owned rendezvous runtime. Reuses the
//! existing DPAPI node/storage keys and encrypted lease journal on migration.
use crate::{
    windows_custody,
    windows_owner_files::{atomic, check_path, create, read_bounded},
};
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

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Policy {
    config: Config,
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
    legacy: Option<crate::rendezvous_owner::Config>,
    // Retain reserved sequence/enrollment provenance, including allocations
    // whose proposal never reached the old service journal.
    legacy_owner_checkpoint: Option<String>,
}

#[derive(Clone, Copy, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum CarrierMode {
    Automatic,
    RelayOnly,
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
    pub fn open(
        config: Config,
        create_policy: bool,
        legacy: Option<crate::rendezvous_owner::Config>,
    ) -> Result<Self> {
        let root = &config.root;
        check_path(&root.join("owner.lock"))?;
        let lock = OpenOptions::new()
            .read(true)
            .write(true)
            .create(true)
            .truncate(false)
            .open(root.join("owner.lock"))?;
        lock.try_lock()
            .map_err(|_| anyhow::anyhow!("owner already running"))?;
        let custody = root.join("self-owned-policy.dpapi");
        if create_policy && !custody.try_exists()? {
            let (sequence_floor, legacy_owner_checkpoint) = if let Some(old) = &legacy {
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
            } else {
                ensure!(
                    !root.join("enrollment.started").try_exists()?
                        && !root.join("keys.dpapi").try_exists()?
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
                create(
                    &root.join("keys.dpapi"),
                    &windows_custody::protect(keys.as_ref())?,
                )?;
                (1, None)
            };
            let (key, _) = read_keys(root)?;
            let policy = Policy {
                config: config.clone(),
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
            create(&custody, &windows_custody::protect(&protected)?)?;
        }
        ensure!(
            custody.is_file(),
            "explicit self-owned provisioning required"
        );
        let protected = windows_custody::unprotect(&read_bounded(&custody, 65536)?)?;
        ensure!(
            protected.len() > 40 && &protected[..8] == b"NVOWN002",
            "self-owned custody version"
        );
        let transport = Zeroizing::new(<[u8; 32]>::try_from(&protected[8..40])?);
        let policy: Policy = serde_json::from_slice(&protected[40..])?;
        ensure!(policy.config == config, "protected local policy differs");
        ensure!(
            !(policy.native_carrier_mailbox_v7 && policy.native_carrier_mailbox_v8),
            "native carrier mailbox versions are mutually exclusive"
        );
        ensure!(
            !(policy.native_carrier_mailbox_v7 || policy.native_carrier_mailbox_v8)
                || policy.native_listener.is_none(),
            "native carrier mailbox conflicts with native-only listener"
        );
        if let Some(tls) = &policy.tls_listener {
            ensure!(
                cfg!(feature = "tls-transport"),
                "configured TLS carrier missing from binary"
            );
            tls.validate()?;
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
        self.policy.tls_listener = listener;
        self.save_policy()
    }

    pub fn set_native_listener(mut self, listener: Option<NativeListenerConfig>) -> Result<()> {
        if let Some(value) = &listener {
            value.validate()?;
            ensure!(
                !(self.native_carrier_mailbox_v7() || self.native_carrier_mailbox_v8()),
                "disable native carrier mailbox before enabling native-only listener"
            );
            ensure!(
                !relay_only(&self.policy),
                "native listener conflicts with relay-only policy"
            );
        }
        // TLS/carrier preferences are retained for explicit return to that mode.
        // They never cause a hidden fallback while native is selected.
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
        self.policy.native_carrier_mailbox_v7 = enabled;
        self.policy.native_carrier_mailbox_v8 = false;
        self.save_policy()
    }

    /// Explicit codec selection, atomically replacing V7 in the same custody.
    /// Disabling either selector disables the carrier codec, including V8.
    pub fn set_native_carrier_mailbox_v8(mut self, enabled: bool) -> Result<()> {
        ensure!(
            !enabled || self.policy.native_listener.is_none(),
            "disable native-only listener before enabling native carrier mailbox"
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
        let sealed = windows_custody::protect(&bytes)?;
        ensure!(
            *windows_custody::unprotect(&sealed)? == *bytes,
            "custody roundtrip differs"
        );
        replace_protected(
            &self.policy.config.root.join("self-owned-policy.dpapi"),
            &sealed,
        )
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
                let old = self
                    .policy
                    .legacy
                    .as_ref()
                    .context("legacy policy missing")?;
                ServiceLeaseBookV1::restore(old.epoch.validate()?.1, &bytes, now)?
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
            host.rotate_native_carrier_mailbox_v8(true, now_ms()?, expires)?;
        } else if self.native_carrier_mailbox_v7() {
            host.rotate_native_carrier_mailbox_v7(true, now_ms()?, expires)?;
        }
        Ok(host)
    }

    async fn run_inner(self, stop: Arc<AtomicBool>) -> Result<()> {
        let _running_lock = self._lock.clone();
        let config = self.policy.config.clone();
        let relay_only = relay_only(&self.policy);
        let native_settings = self.policy.native_listener.clone();
        let native_selected = native_settings.is_some();
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
            let mailbox = Arc::new(
                novovm_network::mailbox_rpc::MailboxServiceV1::new(
                    novovm_network::mailbox_store::MailboxStore::open(
                        &mailbox_path,
                        Default::default(),
                    )?,
                    key.clone(),
                )
                // Desktop L3 serves concurrent Put/Fetch/Consume/Watch lanes. The
                // conservative mobile default (2 requests/s for the entire node) makes
                // those lanes queue behind one another even on healthy connections.
                // This remains one shared budget; reconnecting cannot replenish it.
                // Keep storage, byte quotas and connection bounds unchanged.
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
                    &serde_json::json!({"version":2,"phase":"serving","publisherRequired":false,"anonymous":false,"carrierMode":if native_selected {"native"} else if relay_only {"relay-only"} else {"automatic"},"mailboxTimings":mailbox_status.timing_snapshot_v1()}),
                )?,
            )
            })().context(FailureStage::Publish)
        };
        if let Some(settings) = native_settings {
            let listener = novovm_network::product_rendezvous_owned::NativeMailboxListenerV7::bind_with_wire_budget(
                settings.listen, settings.advertised, key.clone(), stop.clone(),
                settings.max_connections, settings.max_bytes_per_minute,
            ).context(FailureStage::NativeBind)?;
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
            let mut carrier = IrohConfigV1::new(&config.relays, config.nameserver)?
                .with_owned_service_identity(self.transport.clone())?;
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
            Some(
                novovm_network::product_rendezvous_owned::TlsMailboxListenerV1::bind_with_capacity(
                    settings.listen,
                    settings.advertised,
                    seed,
                    stop.clone(),
                    settings.max_connections,
                )
                .await
                .context(FailureStage::TlsBind)?,
            )
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
        let result = if let Some(tls) = tls {
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
        let result = run_self_owned_node_v1(
            carrier,
            key,
            config.allowed_clients.clone(),
            stop,
            Some(mailbox),
            activate,
            publish,
        )
        .await;
        stage_result(result, FailureStage::TransportRun)
    }
}
fn relay_only(policy: &Policy) -> bool {
    match policy.carrier_mode {
        Some(CarrierMode::Automatic) => false,
        Some(CarrierMode::RelayOnly) => true,
        None => policy.config.relay_only,
    }
}
fn trust(pin: [u8; 32]) -> SelfOwnedTrustV1 {
    SelfOwnedTrustV1 {
        version: 1,
        node_pins: [pin].into(),
    }
}
fn read_keys(root: &Path) -> Result<(SigningKey, Zeroizing<[u8; 32]>)> {
    let material = windows_custody::unprotect(&read_bounded(&root.join("keys.dpapi"), 16384)?)?;
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

/// Replace an already validated DPAPI blob under the exclusive owner lock.
/// Both old and new contents are sealed; no raw key is written to disk. A
/// same-directory rename is the sole commit point, with Windows write-through.
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
    impl Fixture {
        fn new() -> Result<Self> {
            let root = std::env::temp_dir().join(format!(
                "novovm-native-owner-{:016x}",
                rand::random::<u64>()
            ));
            std::fs::create_dir(&root)?;
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
        let baseline = ["keys.dpapi", "self-owned-policy.dpapi", "service.journal"]
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
        for (name, expected) in ["keys.dpapi", "self-owned-policy.dpapi", "service.journal"]
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
        let baseline = ["keys.dpapi", "service.journal"]
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
        let path = fixture.0.root.join("self-owned-policy.dpapi");
        replace_protected(&path, &windows_custody::protect(&material)?)?;
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
        for (name, expected) in ["keys.dpapi", "service.journal"].iter().zip(baseline) {
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
        let path = fixture.0.root.join("self-owned-policy.dpapi");
        replace_protected(&path, &windows_custody::protect(&material)?)?;
        let pin = owner.policy.node_pin;
        let transport = owner.transport.clone();
        drop(owner);
        let names = ["keys.dpapi", "self-owned-policy.dpapi", "service.journal"];
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
    fn native_carrier_policy_roundtrip_preserves_identity_and_rejects_udp_conflicts() -> Result<()>
    {
        let fixture = Fixture::new()?;
        let owner = fixture.provision()?;
        assert!(!owner.native_carrier_mailbox_v7());
        let policy = serde_json::to_value(&owner.policy)?;
        let pin = owner.policy.node_pin;
        let transport = owner.transport.clone();
        let baseline = ["keys.dpapi", "service.journal"]
            .map(|name| std::fs::read(fixture.0.root.join(name)).unwrap());
        let path = fixture.0.root.join("self-owned-policy.dpapi");

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
        let enabled = std::fs::read(&path)?;
        assert!(owner.set_native_listener(Some(settings())).is_err());
        assert_eq!(std::fs::read(&path)?, enabled);

        // The existing CLI's off action must also disable a selected V8.
        fixture.open()?.set_native_carrier_mailbox_v7(false)?;
        let owner = fixture.open()?;
        assert!(!owner.native_carrier_mailbox_v7());
        assert!(!owner.native_carrier_mailbox_v8());
        owner.set_native_listener(Some(settings()))?;
        let native = std::fs::read(&path)?;
        assert!(fixture.open()?.set_native_carrier_mailbox_v7(true).is_err());
        assert_eq!(std::fs::read(&path)?, native);
        assert!(fixture.open()?.set_native_carrier_mailbox_v8(true).is_err());
        assert_eq!(std::fs::read(&path)?, native);
        let owner = fixture.open()?;
        assert_eq!(owner.policy.native_listener, Some(settings()));
        assert!(!owner.native_carrier_mailbox_v7());
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
        for (name, expected) in ["keys.dpapi", "service.journal"].iter().zip(baseline) {
            assert_eq!(std::fs::read(fixture.0.root.join(name))?, expected);
        }
        Ok(())
    }

    #[test]
    fn native_carrier_policy_rejects_authenticated_conflicting_state_before_start() -> Result<()> {
        for (v7, v8, native) in [
            (true, false, true),
            (false, true, true),
            (true, true, false),
        ] {
            let fixture = Fixture::new()?;
            let mut owner = fixture.provision()?;
            // A correctly sealed but contradictory policy must fail at open,
            // before choosing a version or binding any transport.
            owner.policy.native_carrier_mailbox_v7 = v7;
            owner.policy.native_carrier_mailbox_v8 = v8;
            owner.policy.native_listener = native.then(settings);
            owner.save_policy()?;
            drop(owner);
            let names = ["keys.dpapi", "self-owned-policy.dpapi", "service.journal"];
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
        let key_custody = std::fs::read(fixture.0.root.join("keys.dpapi"))?;
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
            assert_eq!(
                std::fs::read(fixture.0.root.join("keys.dpapi"))?,
                key_custody
            );
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

    #[test]
    fn native_listener_policy_roundtrip_preserves_identity_and_history() -> Result<()> {
        let fixture = Fixture::new()?;
        let owner = fixture.provision()?;
        let pin = owner.policy.node_pin;
        let transport = owner.transport.clone();
        let baseline = ["keys.dpapi", "service.journal"]
            .map(|p| std::fs::read(fixture.0.root.join(p)).unwrap());
        owner.set_carrier_mode(CarrierMode::Automatic)?;
        #[cfg(feature = "tls-transport")]
        fixture.open()?.set_tls_listener(Some(TlsListenerConfig {
            version: 1,
            listen: "127.0.0.1:0".parse()?,
            advertised: "127.0.0.1:0".parse()?,
            max_connections: 4,
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
        let sealed = std::fs::read(fixture.0.root.join("self-owned-policy.dpapi"))?;
        assert!(owner.set_carrier_mode(CarrierMode::RelayOnly).is_err());
        assert_eq!(
            std::fs::read(fixture.0.root.join("self-owned-policy.dpapi"))?,
            sealed
        );
        let mut bad = settings();
        bad.max_connections = 0;
        assert!(fixture.open()?.set_native_listener(Some(bad)).is_err());
        assert_eq!(
            std::fs::read(fixture.0.root.join("self-owned-policy.dpapi"))?,
            sealed
        );
        fixture.open()?.set_native_listener(None)?;
        let restored = fixture.open()?;
        assert!(restored.policy.native_listener.is_none());
        assert_eq!(restored.policy.node_pin, pin);
        assert_eq!(*restored.transport, *transport);
        for (name, bytes) in ["keys.dpapi", "service.journal"].iter().zip(baseline) {
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
                self.session.try_send(&mut self.link, id, now, || Ok(()))?;
                for done in self.link.poll(now_ms()?, now, 64)?.completed {
                    ensure!(
                        self.session.transport_complete(&done).is_none(),
                        "fixture request transport failed"
                    );
                }
                if let Some(body) = self.link.poll_complete(STREAM, now, 64)? {
                    let (got, mut replies) =
                        self.session.receive(&self.link, body, now, || Ok(()))?;
                    ensure!(got == id && replies.len() == 1, "fixture response binding");
                    return Ok(replies.remove(0));
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
        let keys = std::fs::read(fixture.0.root.join("keys.dpapi"))?;
        let expires = (now_ms()? / 1000) as i64 + 600;
        let (first, sequence) = exchange(&fixture, None, expires).await?;
        let (second, next) = exchange(&fixture, Some(&first), expires).await?;
        assert!(next > sequence);
        assert_eq!(first.signature, second.signature);
        assert_eq!(std::fs::read(fixture.0.root.join("keys.dpapi"))?, keys);
        Ok(())
    }
}
