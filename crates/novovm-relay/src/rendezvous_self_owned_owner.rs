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
    product_directory::sign_relay_record_v1,
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
        let root = self.policy.config.root.clone();
        let _status_lock = self._lock.clone();
        atomic(
            &root.join("status.json"),
            br#"{"version":2,"phase":"starting","anonymous":false}"#,
        )?;
        let result = self.run_inner(stop).await;
        atomic(
            &root.join("status.json"),
            &serde_json::to_vec(
                &serde_json::json!({"version":2,"phase":if result.is_ok() {"stopped"} else {"failed_closed"},"anonymous":false}),
            )?,
        )?;
        result
    }

    async fn run_inner(self, stop: Arc<AtomicBool>) -> Result<()> {
        let _running_lock = self._lock.clone();
        let config = self.policy.config.clone();
        let mut carrier = IrohConfigV1::new(&config.relays, config.nameserver)?
            .with_owned_service_identity(self.transport.clone())?;
        if let Some(bind) = config.bind_address {
            carrier = carrier.with_bind_address(bind)?;
        }
        let relay_only = relay_only(&self.policy);
        if relay_only {
            carrier = carrier.with_relay_only()?;
        }
        #[cfg(feature = "tls-transport")]
        let tls = if let Some(settings) = &self.policy.tls_listener {
            let mut seed = Zeroizing::new([0u8; 32]);
            hkdf::Hkdf::<sha2::Sha256>::new(Some(&self.policy.node_pin), self.transport.as_ref())
                .expand(b"NOVOVM/self-owned/TLS-transport/v1", seed.as_mut())
                .map_err(|_| anyhow::anyhow!("TLS key derivation failed"))?;
            Some(
                novovm_network::product_rendezvous_owned::TlsMailboxListenerV1::bind(
                    settings.listen,
                    settings.advertised,
                    seed,
                    stop.clone(),
                )
                .await?,
            )
        } else {
            None
        };
        let key = self.key.clone();
        let publish_root = config.root.clone();
        let mailbox_path = config.root.join("mailbox.sqlite");
        let mailbox_started = config.root.join("mailbox.started");
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
        let mailbox_status = mailbox.clone();
        let activate =
            move |endpoint: &novovm_network::product_service_lease::IrohServiceEndpointV1,
                  expires| {
                let path = self.policy.config.root.join("service.journal");
                let previous = if path.try_exists()? {
                    Some(ServiceLeaseFileV1::open(&path, self.storage.clone())?)
                } else {
                    None
                };
                let now = now_ms()?;
                let (mut book, file) = if let Some((file, bytes)) = previous {
                    let version =
                        serde_json::from_slice::<serde_json::Value>(&bytes)?["version"].as_u64();
                    let mut book = if version == Some(2) {
                        ServiceLeaseBookV1::restore_self_owned(
                            trust(self.policy.node_pin),
                            &bytes,
                            now,
                        )?
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
                        vec![endpoint.endpoint()?],
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
                        vec![endpoint.endpoint()?],
                        now,
                        expires,
                        self.policy.sequence_floor,
                    )?;
                    let mut book = ServiceLeaseBookV1::new_self_owned(
                        trust(self.policy.node_pin),
                        vec![record],
                        now,
                    )?;
                    let bytes = book.checkpoint()?;
                    let file = ServiceLeaseFileV1::create(&path, self.storage.clone(), &bytes)?;
                    book.attach_persistence(file.clone(), &bytes)?;
                    (book, file)
                };
                book.check_route(
                    self.policy.node_pin,
                    endpoint.address.endpoint_key,
                    endpoint.instance,
                    now,
                )?;
                drop(file); // Book retains the sole durable file owner.
                            // Use the common bounded lease policy. The old 30s override was
                            // tied to the unauthenticated stream watchdog, not to protected
                            // operator permission. Offers remain capped by the current
                            // signed store generation (90s), and existing offers never move.
                            // NVMBOX03 alone admits the lease-bounded authenticated window;
                            // legacy stream and client operation deadlines remain unchanged.
                Ok(Arc::new(ServiceLeaseHostV1::new(self.key.clone(), book)?))
            };
        let publish = move |record: &novovm_network::product_directory::PeerSignedRelayRecordV1| {
            atomic(
                &publish_root.join("self-owned-record.json"),
                &serde_json::to_vec(record)?,
            )?;
            atomic(
                &publish_root.join("status.json"),
                &serde_json::to_vec(
                    &serde_json::json!({"version":2,"phase":"serving","publisherRequired":false,"anonymous":false,"carrierMode":if relay_only {"relay-only"} else {"automatic"},"mailboxTimings":mailbox_status.timing_snapshot_v1()}),
                )?,
            )
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
        result
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
