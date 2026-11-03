//! Independent deployment issuer for the existing signed bootstrap manifests.
//! One explicitly enrolled node per owner; no node discovery, TOFU or wallet role.
use crate::windows_owner_files::{atomic, check_path, create, read_bounded};
use anyhow::{ensure, Result};
use novovm_network::{
    iroh_transport::IrohConfigV1,
    product_directory::{
        sign_bootstrap_manifest_v1, BootstrapSourceKindV1, BootstrapSourceV1,
        BootstrapTrustPolicyV1, PeerSignedRelayRecordV1, SignedBootstrapManifestV1,
    },
    product_overlay::{TransportIdentity, MLDSA65_PUBLIC_KEY_BYTES},
    product_rendezvous_daemon::now_ms,
    product_service_lease::{
        IrohServiceEndpointV1, ServiceLeaseBookV1, ServiceLeaseFileV1, ServiceLeasePersistenceV1,
    },
};
use rand::{rngs::OsRng, RngCore};
use serde::{Deserialize, Serialize};
use std::{
    fs::{File, OpenOptions},
    path::{Path, PathBuf},
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc,
    },
    time::{Duration, Instant},
};
use zeroize::Zeroizing;

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Config {
    pub version: u8,
    pub state_directory: PathBuf,
    pub exchange_directory: PathBuf,
    pub publisher_public_key: Vec<u8>,
    pub node_public_key: Vec<u8>,
    pub transport_identity_path: PathBuf,
    pub chain_id: u64,
    pub library: PathBuf,
    pub library_sha256: String,
    pub record_id: String,
    pub manifest_id: String,
    pub allowed_relays: Vec<String>,
    pub max_grant_ms: u64,
}
impl Config {
    pub fn load(path: &Path) -> Result<Self> {
        let value: Self = serde_json::from_slice(&read_bounded(path, 65536)?)?;
        ensure!(
            value.version == 2 && (5000..=110000).contains(&value.max_grant_ms),
            "publisher bounds"
        );
        for id in [&value.record_id, &value.manifest_id] {
            ensure!(
                !id.is_empty() && id.len() <= 128,
                "publisher identity bounds"
            );
        }
        for key in [&value.node_public_key, &value.publisher_public_key] {
            ensure!(
                key.len() == MLDSA65_PUBLIC_KEY_BYTES && key.iter().any(|b| *b != 0),
                "publisher native policy key"
            );
        }
        ensure!(
            value.node_public_key != value.publisher_public_key,
            "node cannot publish its own authority"
        );
        ensure!(
            value.chain_id != 0
                && value.transport_identity_path.is_absolute()
                && value.library.is_absolute()
                && value.library_sha256.len() == 64
                && value.library_sha256.bytes().all(|b| b.is_ascii_hexdigit()),
            "qualified publisher identity required"
        );
        for dir in [&value.state_directory, &value.exchange_directory] {
            check_path(dir)?;
            ensure!(dir.is_dir(), "publisher directory missing");
        }
        let state = value.state_directory.canonicalize()?;
        let exchange = value.exchange_directory.canonicalize()?;
        ensure!(
            !state.starts_with(&exchange) && !exchange.starts_with(&state),
            "separate publisher custody required"
        );
        IrohConfigV1::new(&value.allowed_relays, "127.0.0.1".parse()?)?;
        Ok(value)
    }
}

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Grant {
    proposal: PeerSignedRelayRecordV1,
    source: BootstrapSourceV1,
}
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct State {
    version: u8,
    policy: Option<Config>,
    revoked: bool,
    last_time_ms: u64,
    last_sequence: u64,
    last_node_time_ms: u64,
    // Only the current and prepared epoch are retained, not a global directory.
    grants: Vec<Grant>,
}
pub struct Owner {
    root: PathBuf,
    key: TransportIdentity,
    store: Arc<ServiceLeaseFileV1>,
    state: State,
    bytes: Vec<u8>,
    _lock: File,
}
fn lock(root: &Path) -> Result<File> {
    check_path(root)?;
    ensure!(root.is_dir(), "publisher directory missing");
    let path = root.join("publisher.lock");
    check_path(&path)?;
    let lock = OpenOptions::new()
        .read(true)
        .write(true)
        .create(true)
        .truncate(false)
        .open(path)?;
    lock.try_lock()
        .map_err(|_| anyhow::anyhow!("publisher already running"))?;
    Ok(lock)
}
impl Owner {
    /// The publisher purpose key is already explicitly protected. This creates
    /// only independent storage custody under the eventual service identity.
    pub fn initialize(config: &Config) -> Result<()> {
        let key = Self::identity(config)?;
        ensure!(
            key.public_key() == config.publisher_public_key,
            "publisher key pin mismatch"
        );
        Self::initialize_with_identity(&config.state_directory, key)
    }
    fn identity(config: &Config) -> Result<TransportIdentity> {
        ensure!(config.version == 2, "publisher configuration version");
        crate::transport_identity::load_transport_identity(
            &config.transport_identity_path,
            config.chain_id,
            &config.library,
            &config.library_sha256,
        )
    }
    fn initialize_with_identity(root: &Path, key: TransportIdentity) -> Result<()> {
        let _lock = lock(root)?;
        ensure!(
            std::fs::read_dir(root)?
                .all(|entry| entry.is_ok_and(|e| e.file_name() == "publisher.lock")),
            "publisher initialization requires an empty private directory"
        );
        create(
            &root.join("enrollment.started"),
            b"NOVOVM publisher custody v2",
        )?;
        let mut material = Zeroizing::new([0u8; 40]);
        material[..8].copy_from_slice(b"NVPUBK02");
        OsRng.try_fill_bytes(&mut material[8..])?;
        create(
            &root.join("keys.dpapi"),
            &crate::windows_custody::protect(material.as_ref())?,
        )?;
        let storage = Zeroizing::new(<[u8; 32]>::try_from(&material[8..])?);
        ensure!(*storage != [0; 32], "publisher storage key");
        let state = State {
            version: 2,
            policy: None,
            revoked: false,
            last_time_ms: 0,
            last_sequence: 0,
            last_node_time_ms: 0,
            grants: vec![],
        };
        ServiceLeaseFileV1::create(
            &root.join("publisher.journal"),
            storage,
            &serde_json::to_vec(&state)?,
        )?;
        atomic(
            &root.join("identity.json"),
            &serde_json::to_vec(&serde_json::json!({
                "version":2, "suite":65, "publisher_public_key":key.public_key()
            }))?,
        )?;
        Ok(())
    }
    fn open_with_identity(root: &Path, key: TransportIdentity) -> Result<Self> {
        let owner_lock = lock(root)?;
        ensure!(
            read_bounded(&root.join("enrollment.started"), 128)? == b"NOVOVM publisher custody v2",
            "publisher enrollment marker"
        );
        let material =
            crate::windows_custody::unprotect(&read_bounded(&root.join("keys.dpapi"), 16384)?)?;
        ensure!(
            material.len() == 40 && &material[..8] == b"NVPUBK02",
            "publisher custody version"
        );
        let storage = Zeroizing::new(<[u8; 32]>::try_from(&material[8..])?);
        ensure!(*storage != [0; 32], "publisher storage key");
        let (store, bytes) = ServiceLeaseFileV1::open(&root.join("publisher.journal"), storage)?;
        let state: State = serde_json::from_slice(&bytes)?;
        ensure!(
            state.version == 2 && state.grants.len() <= 2,
            "publisher journal state"
        );
        Ok(Self {
            root: root.into(),
            key,
            store,
            state,
            bytes,
            _lock: owner_lock,
        })
    }
    fn save(&mut self) -> Result<()> {
        let next = serde_json::to_vec(&self.state)?;
        self.store.commit(&self.bytes, &next)?;
        self.bytes = next;
        self.status("ready")
    }
    fn status(&self, phase: &str) -> Result<()> {
        atomic(
            &self.root.join("status.json"),
            &serde_json::to_vec(&serde_json::json!({
                "version":1,"phase":phase,"revoked":self.state.revoked,
                "last_sequence":self.state.last_sequence,"anonymous":false
            }))?,
        )
    }
    fn check_key(&self, config: &Config) -> Result<()> {
        ensure!(
            config.publisher_public_key == self.key.public_key(),
            "publisher key pin mismatch"
        );
        Ok(())
    }
    pub fn enroll(config: Config) -> Result<()> {
        let key = Self::identity(&config)?;
        Self::enroll_with_identity(config, key)
    }
    fn enroll_with_identity(config: Config, key: TransportIdentity) -> Result<()> {
        let mut owner = Self::open_with_identity(&config.state_directory, key)?;
        owner.check_key(&config)?;
        ensure!(
            owner.state.policy.is_none() && !owner.state.revoked && owner.state.last_sequence == 0,
            "publisher policy already enrolled; no implicit trust replacement"
        );
        owner.state.policy = Some(config);
        owner.save()
    }
    fn configured(config: &Config) -> Result<Self> {
        Self::configured_with_identity(config, Self::identity(config)?)
    }
    fn configured_with_identity(config: &Config, key: TransportIdentity) -> Result<Self> {
        let owner = Self::open_with_identity(&config.state_directory, key)?;
        owner.check_key(config)?;
        ensure!(
            owner.state.policy.as_ref() == Some(config),
            "publisher policy changed or not enrolled"
        );
        Ok(owner)
    }
    pub fn request_revoke(config: &Config) -> Result<()> {
        // Durable operator intent, processed ahead of every new grant. This
        // command reports a REQUEST, not completion of online revocation.
        let path = config.state_directory.join("revoke.request");
        let request = serde_json::to_vec(
            &serde_json::json!({"version":1,"node_public_key":config.node_public_key}),
        )?;
        if path.try_exists()? {
            ensure!(
                read_bounded(&path, 16384)? == request,
                "conflicting revoke request"
            );
        } else {
            create(&path, &request)?;
        }
        Ok(())
    }
    fn source(
        &self,
        config: &Config,
        record: Option<PeerSignedRelayRecordV1>,
        now: u64,
        expires: u64,
    ) -> Result<BootstrapSourceV1> {
        let mut manifest = SignedBootstrapManifestV1 {
            version: 2,
            suite: 65,
            manifest_id: config.manifest_id.clone(),
            issued_at_ms: now,
            expires_at_ms: expires,
            candidate_limit: 1,
            full_raw_ip_directory_embedded: false,
            requires_single_official_relay: false,
            requires_single_official_domain: false,
            relay_records: record.into_iter().collect(),
            signatures: vec![],
        };
        sign_bootstrap_manifest_v1(&mut manifest, &self.key)?;
        let source = BootstrapSourceV1 {
            source_kind: BootstrapSourceKindV1::FriendInvite,
            priority: 0,
            manifest,
        };
        // Reuse consumers' strict signature, TTL, canonical endpoint and bounds
        // validation; do not accept a weaker publisher-only interpretation.
        ServiceLeaseBookV1::new(
            BootstrapTrustPolicyV1::single_signer(config.publisher_public_key.clone()),
            vec![source.clone()],
            now,
            self.key.clone(),
        )?;
        Ok(source)
    }
    fn revocation(&mut self, config: &Config, now: u64) -> Result<()> {
        let path = self.root.join("revoke.request");
        if !self.state.revoked && path.try_exists()? {
            let expected = serde_json::to_vec(
                &serde_json::json!({"version":1,"node_public_key":config.node_public_key}),
            )?;
            ensure!(
                read_bounded(&path, 16384)? == expected,
                "invalid revoke request"
            );
            self.state.revoked = true;
            // Persist the tombstone BEFORE signing or exposing revocation.
            self.state.last_time_ms = now;
            self.save()?;
        }
        if self.state.revoked
            && self
                .state
                .grants
                .iter()
                .any(|g| !g.source.manifest.relay_records.is_empty())
        {
            let source = self.source(
                config,
                None,
                now,
                now.checked_add(config.max_grant_ms)
                    .ok_or_else(|| anyhow::anyhow!("publisher clock overflow"))?,
            )?;
            for grant in &mut self.state.grants {
                grant.source = source.clone();
            }
            self.state.last_time_ms = now;
            self.save()?;
        }
        Ok(())
    }
    fn publish(&self, config: &Config, now: u64) -> Result<()> {
        for grant in &self.state.grants {
            if grant.source.manifest.expires_at_ms <= now {
                continue;
            }
            // Never recreate an old epoch after its node owner removed it.
            let proposal = config
                .exchange_directory
                .join(format!("epoch-{}.proposal.json", grant.proposal.sequence));
            if !proposal.try_exists()? {
                continue;
            }
            let path = config
                .exchange_directory
                .join(format!("epoch-{}.authority.json", grant.proposal.sequence));
            let bytes = serde_json::to_vec(&vec![grant.source.clone()])?;
            if !path.try_exists()? || read_bounded(&path, 1024 * 1024)? != bytes {
                atomic(&path, &bytes)?;
            }
        }
        Ok(())
    }
    fn tick(&mut self, config: &Config) -> Result<()> {
        let now = now_ms()?;
        ensure!(now >= self.state.last_time_ms, "publisher clock rollback");
        // Avoid issuing two different manifests at the same millisecond.
        if now == self.state.last_time_ms {
            return Ok(());
        }
        self.revocation(config, now)?;
        if self.state.last_time_ms == now {
            return self.publish(config, now);
        }
        let mut proposals = Vec::new();
        // A managed handoff contains at most two generations. Bound even the
        // directory scan, rather than accepting an unbounded request queue.
        for (i, entry) in std::fs::read_dir(&config.exchange_directory)?.enumerate() {
            ensure!(i < 64, "publisher exchange directory budget");
            let entry = entry?;
            let name = entry.file_name();
            let Some(sequence) = name
                .to_str()
                .and_then(|s| s.strip_prefix("epoch-"))
                .and_then(|s| s.strip_suffix(".proposal.json"))
                .and_then(|s| s.parse::<u64>().ok())
            else {
                continue;
            };
            ensure!(
                sequence > 0 && name == format!("epoch-{sequence}.proposal.json").as_str(),
                "canonical epoch filename"
            );
            proposals.push((sequence, entry.path()));
            ensure!(proposals.len() <= 2, "publisher proposal budget");
        }
        proposals.sort_by_key(|(sequence, _)| *sequence);
        for (sequence, path) in proposals {
            let record: PeerSignedRelayRecordV1 = match read_bounded(
                &path,
                novovm_network::product_service_lease::MAX_SERVICE_LEASE_BYTES,
            ) {
                Ok(bytes) => serde_json::from_slice(&bytes)?,
                Err(e)
                    if e.downcast_ref::<std::io::Error>()
                        .is_some_and(|e| e.kind() == std::io::ErrorKind::NotFound) =>
                {
                    continue
                }
                Err(e) => return Err(e),
            };
            ensure!(
                record.sequence == sequence
                    && record.relay_public_key == config.node_public_key
                    && record.record_id == config.record_id,
                "proposal not authorized by enrolled policy"
            );
            if let Some(old) = self
                .state
                .grants
                .iter()
                .find(|g| g.proposal.sequence == sequence)
            {
                ensure!(old.proposal == record, "publisher proposal equivocation");
                continue;
            }
            ensure!(
                sequence > self.state.last_sequence,
                "publisher proposal rollback"
            );
            ensure!(
                record.issued_at_ms >= self.state.last_node_time_ms && record.issued_at_ms <= now,
                "publisher proposal clock"
            );
            ensure!(
                record
                    .expires_at_ms
                    .checked_sub(record.issued_at_ms)
                    .is_some_and(|ttl| ttl > 0 && ttl <= config.max_grant_ms),
                "publisher proposal lifetime"
            );
            if record.expires_at_ms <= now {
                continue;
            }
            let endpoint = IrohServiceEndpointV1::from_record(&record)?;
            ensure!(
                endpoint
                    .address
                    .relays
                    .iter()
                    .all(|r| config.allowed_relays.contains(r)),
                "unapproved publisher relay"
            );
            // Iroh endpoint keys remain carrier-owned 32-byte material;
            // native issuer/node authority uses separately held 1952-byte keys.
            // Verify the node signature even when refusing it after revocation.
            let approved = self.source(config, Some(record.clone()), now, record.expires_at_ms)?;
            let source = if self.state.revoked {
                self.source(config, None, now, record.expires_at_ms)?
            } else {
                approved
            };
            // First signing is private. Nothing leaves this owner before its
            // exact signed result and monotonic history are committed.
            self.state.last_sequence = sequence;
            self.state.last_node_time_ms = record.issued_at_ms;
            self.state.last_time_ms = now;
            self.state.grants.push(Grant {
                proposal: record,
                source,
            });
            if self.state.grants.len() > 2 {
                self.state.grants.remove(0);
            }
            self.save()?;
            // Only one new manifest per tick to avoid same-time equivocation.
            break;
        }
        self.publish(config, now)
    }
    pub fn run(config: Config, stop: Arc<AtomicBool>, limit: Option<Duration>) -> Result<()> {
        let mut owner = Self::configured(&config)?;
        owner.status("ready")?;
        let start = Instant::now();
        let result = (|| {
            while !stop.load(Ordering::Acquire) && limit.is_none_or(|v| start.elapsed() < v) {
                owner.tick(&config)?;
                std::thread::sleep(Duration::from_millis(100));
            }
            Ok(())
        })();
        owner.status(if result.is_err() {
            "failed_closed"
        } else {
            "stopped"
        })?;
        result
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use novovm_network::{iroh_transport::IrohAddressV1, product_directory::sign_relay_record_v1};

    struct Fixture {
        root: PathBuf,
        config: Config,
        node: TransportIdentity,
    }
    impl Drop for Fixture {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.root);
        }
    }
    impl Fixture {
        fn new() -> Result<Self> {
            let root = std::env::temp_dir().join(format!(
                "novovm-publisher-owner-test-{}",
                rand::random::<u64>()
            ));
            std::fs::create_dir(&root)?;
            let state_directory = root.join("publisher");
            let exchange_directory = root.join("node");
            std::fs::create_dir(&state_directory)?;
            std::fs::create_dir(&exchange_directory)?;
            let publisher = crate::transport_identity::component_identity([11; 32]);
            Owner::initialize_with_identity(&state_directory, publisher.clone())?;
            let publisher_public_key = publisher.public_key().to_vec();
            let node = crate::transport_identity::component_identity([12; 32]);
            let config = Config {
                version: 2,
                transport_identity_path: root.join("component-purpose"),
                chain_id: 7,
                library: root.join("component-library"),
                library_sha256: "a".repeat(64),
                state_directory,
                exchange_directory,
                publisher_public_key,
                node_public_key: node.public_key().to_vec(),
                record_id: "test-node".into(),
                manifest_id: "test-publisher".into(),
                allowed_relays: vec![],
                max_grant_ms: 10000,
            };
            Owner::enroll_with_identity(config.clone(), publisher)?;
            Ok(Self { root, config, node })
        }
        fn proposal(
            &self,
            sequence: u64,
            ttl: u64,
            relays: Vec<String>,
        ) -> Result<PeerSignedRelayRecordV1> {
            let endpoint = IrohServiceEndpointV1 {
                instance: rand::random(),
                address: IrohAddressV1 {
                    endpoint_key: ed25519_dalek::SigningKey::generate(&mut OsRng)
                        .verifying_key()
                        .to_bytes(),
                    direct: vec!["127.0.0.1:9".parse()?],
                    relays,
                },
            };
            let now = now_ms()?;
            let record = sign_relay_record_v1(
                &self.node,
                "test-node",
                vec![endpoint.endpoint()?],
                now,
                now + ttl,
                sequence,
            )?;
            atomic(
                &self
                    .config
                    .exchange_directory
                    .join(format!("epoch-{sequence}.proposal.json")),
                &serde_json::to_vec(&record)?,
            )?;
            Ok(record)
        }
    }
    #[test]
    fn old_publisher_config_and_signing_custody_are_not_promoted() -> Result<()> {
        let fixture = Fixture::new()?;
        let path = fixture.root.join("config.json");
        let mut old = fixture.config.clone();
        old.version = 1;
        atomic(&path, &serde_json::to_vec(&old)?)?;
        assert!(Config::load(&path).is_err());
        old.version = 2;
        old.publisher_public_key = vec![21; 32];
        atomic(&path, &serde_json::to_vec(&old)?)?;
        assert!(Config::load(&path).is_err());
        let mut raw = Zeroizing::new([0u8; 72]);
        raw[..8].copy_from_slice(b"NVPUBK01");
        raw[8..40].fill(21);
        raw[40..].fill(22);
        let sealed = crate::windows_custody::protect(raw.as_ref())?;
        let key_path = fixture.config.state_directory.join("keys.dpapi");
        std::fs::write(&key_path, &sealed)?;
        let journal = std::fs::read(fixture.config.state_directory.join("publisher.journal"))?;
        assert!(Owner::configured_with_identity(
            &fixture.config,
            crate::transport_identity::component_identity([11; 32])
        )
        .is_err());
        assert_eq!(std::fs::read(key_path)?, sealed);
        assert_eq!(
            std::fs::read(fixture.config.state_directory.join("publisher.journal"))?,
            journal
        );
        Ok(())
    }
    #[test]
    fn interrupted_revocation_recovery_cannot_publish_a_previous_live_grant() -> Result<()> {
        let fixture = Fixture::new()?;
        fixture.proposal(1, 10000, vec![])?;
        let mut owner = Owner::configured_with_identity(
            &fixture.config,
            crate::transport_identity::component_identity([11; 32]),
        )?;
        owner.tick(&fixture.config)?;
        // Crash point: tombstone durable, replacement manifests not committed.
        owner.state.revoked = true;
        owner.save()?;
        drop(owner);
        std::thread::sleep(Duration::from_millis(5));
        fixture.proposal(66, 10000, vec![])?;
        let mut owner = Owner::configured_with_identity(
            &fixture.config,
            crate::transport_identity::component_identity([11; 32]),
        )?;
        owner.tick(&fixture.config)?;
        let path = fixture
            .config
            .exchange_directory
            .join("epoch-1.authority.json");
        let sources: Vec<BootstrapSourceV1> =
            serde_json::from_slice(&read_bounded(&path, 1024 * 1024)?)?;
        ensure!(
            sources[0].manifest.relay_records.is_empty(),
            "live grant replayed after durable revocation"
        );
        ensure!(
            !fixture
                .config
                .exchange_directory
                .join("epoch-66.authority.json")
                .exists(),
            "same-time different manifests"
        );
        std::thread::sleep(Duration::from_millis(5));
        owner.tick(&fixture.config)?;
        let path = fixture
            .config
            .exchange_directory
            .join("epoch-66.authority.json");
        let sources: Vec<BootstrapSourceV1> =
            serde_json::from_slice(&read_bounded(&path, 1024 * 1024)?)?;
        ensure!(
            sources[0].manifest.relay_records.is_empty(),
            "successor restored revoked authority"
        );
        Ok(())
    }
    #[test]
    fn ttl_relay_clock_and_missing_journal_are_fail_closed() -> Result<()> {
        for (ttl, relays) in [
            (20000, vec![]),
            (10000, vec!["https://unapproved.example".into()]),
        ] {
            let fixture = Fixture::new()?;
            fixture.proposal(1, ttl, relays)?;
            let mut owner = Owner::configured_with_identity(
                &fixture.config,
                crate::transport_identity::component_identity([11; 32]),
            )?;
            ensure!(
                owner.tick(&fixture.config).is_err(),
                "unauthorized lifetime or carrier accepted"
            );
            ensure!(
                !fixture
                    .config
                    .exchange_directory
                    .join("epoch-1.authority.json")
                    .exists(),
                "invalid grant published"
            );
        }
        let fixture = Fixture::new()?;
        let mut owner = Owner::configured_with_identity(
            &fixture.config,
            crate::transport_identity::component_identity([11; 32]),
        )?;
        owner.state.last_time_ms = now_ms()? + 60000;
        owner.save()?;
        ensure!(
            owner.tick(&fixture.config).is_err(),
            "clock rollback accepted"
        );
        drop(owner);
        std::fs::remove_file(fixture.config.state_directory.join("publisher.journal"))?;
        ensure!(
            Owner::configured_with_identity(
                &fixture.config,
                crate::transport_identity::component_identity([11; 32])
            )
            .is_err(),
            "missing history reopened"
        );
        ensure!(
            Owner::initialize_with_identity(
                &fixture.config.state_directory,
                crate::transport_identity::component_identity([11; 32])
            )
            .is_err(),
            "missing history reenrolled"
        );
        Ok(())
    }
}
