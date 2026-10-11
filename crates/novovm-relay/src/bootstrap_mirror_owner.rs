//! Distribution of already-authorized sources. No issuer key, discovery
//! directory, service lease signing, or chat state belongs to this owner.
use crate::windows_owner_files::{atomic, check_path, create, read_bounded};
use anyhow::{ensure, Context, Result};
use ed25519_dalek::SigningKey;
use futures_util::{stream::FuturesUnordered, StreamExt};
use novovm_network::{
    carrier_stream::CarrierStreamV1,
    iroh_transport::{with_iroh_scope_v1, IrohAddressV1, IrohConfigV1, IrohControlV1},
    product_bootstrap_refresh::{validate_bootstrap_refresh_peers_v1, BootstrapRefreshPeerV1},
    product_directory::{BootstrapSourceV1, BootstrapTrustPolicyV1},
    product_rendezvous_daemon::now_ms,
    product_service_lease::{serve_bootstrap_sources_v2, ServiceLeaseBookV1, ServiceLeaseFileV1},
};
use rand::{rngs::OsRng, RngCore};
use serde::{Deserialize, Serialize};
use std::{
    collections::BTreeSet,
    fs::{File, OpenOptions},
    net::{IpAddr, SocketAddr},
    path::{Path, PathBuf},
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc,
    },
    time::{Duration, Instant},
};
use zeroize::Zeroizing;

#[derive(Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct Config {
    pub version: u8,
    pub engineering_direct_allowed: bool,
    pub state_directory: PathBuf,
    pub source_path: PathBuf,
    pub trust_anchors: Vec<[u8; 32]>,
    pub minimum_signatures: usize,
    pub source_ids: Vec<String>,
    pub relays: Vec<String>,
    pub dns: IpAddr,
    pub bind_address: Option<SocketAddr>,
    pub relay_only: bool,
    pub permission_expires_at_ms: u64,
}
impl Config {
    pub fn load(path: &Path) -> Result<Self> {
        let value: Self = serde_json::from_slice(&read_bounded(path, 8192)?)?;
        ensure!(
            value.version == 1 && value.engineering_direct_allowed,
            "explicit engineering mirror required"
        );
        check_path(&value.state_directory)?;
        check_path(&value.source_path)?;
        ensure!(
            value.state_directory.is_dir() && value.source_path.is_file(),
            "mirror managed paths missing"
        );
        ensure!(
            !value
                .source_path
                .canonicalize()?
                .starts_with(value.state_directory.canonicalize()?),
            "mirror source must be external"
        );
        ensure!(
            (1..=4).contains(&value.source_ids.len())
                && value.source_ids.iter().collect::<BTreeSet<_>>().len() == value.source_ids.len()
                && value
                    .source_ids
                    .iter()
                    .all(|id| !id.is_empty() && id.len() <= 128),
            "mirror source IDs"
        );
        value.policy()?;
        value.carrier()?;
        Ok(value)
    }
    fn policy(&self) -> Result<BootstrapTrustPolicyV1> {
        ensure!(
            (1..=8).contains(&self.trust_anchors.len())
                && (1..=self.trust_anchors.len()).contains(&self.minimum_signatures),
            "mirror trust bounds"
        );
        let mut ids = BTreeSet::new();
        for key in &self.trust_anchors {
            let vk = ed25519_dalek::VerifyingKey::from_bytes(key)?;
            ensure!(
                !vk.is_weak()
                    && vk.to_edwards().is_torsion_free()
                    && vk.to_edwards().compress().to_bytes() == *key,
                "mirror trust key"
            );
            ensure!(
                ids.insert(
                    BootstrapTrustPolicyV1::single_signer(*key)
                        .allowed_signer_peer_ids
                        .into_iter()
                        .next()
                        .unwrap()
                ),
                "duplicate mirror anchor"
            );
        }
        Ok(BootstrapTrustPolicyV1 {
            allowed_signer_peer_ids: ids,
            minimum_valid_signatures: self.minimum_signatures,
        })
    }
    fn carrier(&self) -> Result<IrohConfigV1> {
        let mut config = IrohConfigV1::new(&self.relays, self.dns)?;
        if self.relay_only {
            ensure!(
                self.bind_address.is_none(),
                "relay mirror cannot bind direct transport"
            );
            config = config.with_relay_only()?;
        } else {
            let address = self
                .bind_address
                .context("direct mirror requires a stable local bind port")?;
            ensure!(address.port() != 0, "mirror bind port must survive restart");
            config = config.with_bind_address(address)?;
        }
        Ok(config)
    }
    fn sources(&self) -> Result<Vec<BootstrapSourceV1>> {
        let sources: Vec<BootstrapSourceV1> =
            serde_json::from_slice(&read_bounded(&self.source_path, 65536)?)?;
        ensure!(
            sources.len() == self.source_ids.len()
                && sources
                    .iter()
                    .map(|s| &s.manifest.manifest_id)
                    .collect::<BTreeSet<_>>()
                    == self.source_ids.iter().collect(),
            "mirror source set changed"
        );
        for record in sources.iter().flat_map(|s| &s.manifest.relay_records) {
            let address =
                novovm_network::product_service_lease::IrohServiceEndpointV1::from_record(record)?
                    .address;
            ensure!(
                address.relays.iter().all(|r| self.relays.contains(r)),
                "unapproved source relay"
            );
        }
        Ok(sources)
    }
}
pub struct Owner {
    config: Config,
    seed: Zeroizing<[u8; 32]>,
    book: ServiceLeaseBookV1,
    _lock: File,
}
fn lock(root: &Path) -> Result<File> {
    let path = root.join("mirror.lock");
    check_path(&path)?;
    let file = OpenOptions::new()
        .read(true)
        .write(true)
        .create(true)
        .truncate(false)
        .open(path)?;
    file.try_lock()
        .map_err(|_| anyhow::anyhow!("mirror already running"))?;
    Ok(file)
}
impl Owner {
    pub fn initialize(config: &Config) -> Result<()> {
        let root = &config.state_directory;
        let _lock = lock(root)?;
        ensure!(
            std::fs::read_dir(root)?
                .all(|entry| entry.is_ok_and(|e| e.file_name() == "mirror.lock")),
            "mirror enrollment requires an empty private directory"
        );
        let now = now_ms()?;
        ensure!(
            config.permission_expires_at_ms > now
                && config.permission_expires_at_ms - now <= 86400000,
            "mirror permission requires explicit expiry within 24 hours"
        );
        let book = ServiceLeaseBookV1::new(config.policy()?, config.sources()?, now)?;
        create(
            &root.join("enrollment.started"),
            b"NOVOVM mirror custody v1",
        )?;
        let mut material = Zeroizing::new(vec![0u8; 72]);
        material[..8].copy_from_slice(b"NVMIRK01");
        OsRng.try_fill_bytes(&mut material[8..])?;
        material.extend_from_slice(&serde_json::to_vec(config)?);
        create(
            &root.join("keys.dpapi"),
            &crate::windows_custody::protect(&material)?,
        )?;
        let storage = Zeroizing::new(<[u8; 32]>::try_from(&material[40..72])?);
        ServiceLeaseFileV1::create(&root.join("mirror.journal"), storage, &book.checkpoint()?)?;
        Ok(())
    }
    fn open(config: Config) -> Result<Self> {
        let root = &config.state_directory;
        let owned = lock(root)?;
        ensure!(
            read_bounded(&root.join("enrollment.started"), 128)? == b"NOVOVM mirror custody v1",
            "mirror enrollment marker"
        );
        let material =
            crate::windows_custody::unprotect(&read_bounded(&root.join("keys.dpapi"), 16384)?)?;
        ensure!(
            material.len() > 72 && &material[..8] == b"NVMIRK01",
            "mirror custody version"
        );
        let policy: Config = serde_json::from_slice(&material[72..])?;
        ensure!(
            policy == config,
            "mirror permission/policy changed; no implicit extension"
        );
        let seed = Zeroizing::new(<[u8; 32]>::try_from(&material[8..40])?);
        let storage = Zeroizing::new(<[u8; 32]>::try_from(&material[40..72])?);
        ensure!(
            *seed != [0; 32] && *storage != [0; 32] && seed != storage,
            "independent mirror material"
        );
        let pin = SigningKey::from_bytes(&seed).verifying_key().to_bytes();
        ensure!(
            !config.trust_anchors.contains(&pin),
            "mirror cannot be issuer"
        );
        let (file, bytes) = ServiceLeaseFileV1::open(&root.join("mirror.journal"), storage)?;
        let mut book = ServiceLeaseBookV1::restore(config.policy()?, &bytes, now_ms()?)?;
        book.attach_persistence(file, &bytes)?;
        ensure!(
            book.sources()
                .iter()
                .flat_map(|s| &s.manifest.relay_records)
                .all(|r| r.relay_public_key != pin),
            "mirror cannot reuse node identity"
        );
        Ok(Self {
            config,
            seed,
            book,
            _lock: owned,
        })
    }
    fn reload(&mut self) -> Result<bool> {
        let sources = self.config.sources()?;
        if serde_json::to_vec(&sources)? == serde_json::to_vec(&self.book.sources())? {
            return Ok(false);
        }
        self.book.accept_bootstrap_sources(sources, now_ms()?)?;
        Ok(true)
    }
    fn live_sources(&self) -> Result<Vec<BootstrapSourceV1>> {
        let sources = self.book.sources();
        ServiceLeaseBookV1::new(self.config.policy()?, sources.clone(), now_ms()?)?;
        Ok(sources)
    }
    fn status(&self, phase: &str) -> Result<()> {
        atomic(
            &self.config.state_directory.join("status.json"),
            &serde_json::to_vec(&serde_json::json!({
                "version":1,"phase":phase,"anonymous":false
            }))?,
        )
    }
    fn availability(&self) -> &'static str {
        match self.live_sources() {
            Ok(sources) if sources.iter().any(|s| !s.manifest.relay_records.is_empty()) => {
                "serving"
            }
            Ok(_) => "revoked",
            Err(_) => "waiting_sources",
        }
    }
    fn publish_invite(&self, address: IrohAddressV1) -> Result<()> {
        let peer = BootstrapRefreshPeerV1 {
            address,
            expires_at_ms: self.config.permission_expires_at_ms,
        };
        validate_bootstrap_refresh_peers_v1(
            std::slice::from_ref(&peer),
            &self.config.relays,
            now_ms()?,
            false,
        )?;
        atomic(
            &self.config.state_directory.join("peer.json"),
            &serde_json::to_vec(&peer)?,
        )?;
        // Export the existing product invitation consumed by NOVOCHAT. The
        // operator must deliver/pin it through an authenticated out-of-band path.
        // Signing a source does not authenticate a mirror pointer inside JSON.
        if let Ok(sources) = self.live_sources() {
            let bytes = serde_json::to_vec(&serde_json::json!({"transport":"novovm-bootstrap-v1",
                "trust_anchors":self.config.trust_anchors,"minimum_signatures":self.config.minimum_signatures,
                "sources":sources,"refresh_peers":[peer],"relays":self.config.relays,"dns":self.config.dns}))?;
            ensure!(bytes.len() <= 8192, "mobile invitation size");
            atomic(&self.config.state_directory.join("invitation.json"), &bytes)?;
        }
        Ok(())
    }
    async fn epoch(mut self, stop: Arc<AtomicBool>, until: Instant) -> Result<Self> {
        self.reload()?; // Bad supplied data stops before any bind or DNS.
        let allowed = self.config.permission_expires_at_ms;
        ensure!(now_ms()? < allowed, "mirror permission expired");
        let control = IrohControlV1::new(until + Duration::from_secs(6), move || {
            ensure!(
                !stop.load(Ordering::Acquire) && now_ms()? < allowed,
                "mirror stopped/permission expired"
            );
            Ok(())
        })?;
        let carrier = self
            .config
            .carrier()?
            .with_owned_service_identity(self.seed.clone())?;
        with_iroh_scope_v1(carrier, control, move |scope| Box::pin(async move {
            if !self.config.relays.is_empty() { scope.online().await?; }
            self.publish_invite(scope.address()?)?;
            let mut phase = self.availability();
            self.status(phase)?;
            let mut active = FuturesUnordered::new();
            let mut accepting = Box::pin(scope.accept_inbound());
            let mut update = tokio::time::interval(Duration::from_millis(100));
            update.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
            let mut window = Instant::now();
            let mut requests = 0;
            loop {
                tokio::select! {
                    biased;
                    _ = tokio::time::sleep_until(until.into()) => break,
                    _ = update.tick() => {
                        if self.reload()? {
                            active.clear(); // Superseded/revoked sources cannot finish in flight.
                            self.publish_invite(scope.address()?)?;
                        }
                        let next_phase = self.availability();
                        if next_phase != phase { self.status(next_phase)?; phase = next_phase; }
                        if window.elapsed() >= Duration::from_secs(1) { requests = 0; window = Instant::now(); }
                    }
                    Some(_) = active.next(), if !active.is_empty() => {},
                    incoming = &mut accepting, if active.len() < 4 && requests < 16 => {
                        accepting = Box::pin(scope.accept_inbound());
                        requests += 1;
                        if let Ok(stream) = incoming {
                            if let Ok(sources) = self.live_sources() {
                                let expires = sources.iter().map(|s| s.manifest.expires_at_ms).min().context("mirror source absent")?;
                                let budget = Duration::from_millis(expires.saturating_sub(now_ms()?).min(3000));
                                active.push(async move {
                                    let mut stream: CarrierStreamV1<'_> = stream.into();
                                    tokio::time::timeout(budget, async {
                                        serve_bootstrap_sources_v2(&mut stream, sources).await?;
                                        stream.finish().await
                                    }).await
                                });
                            }
                        }
                    }
                }
            }
            drop(active);
            drop(accepting);
            Ok(self)
        })).await
    }
    pub async fn run(config: Config, stop: Arc<AtomicBool>, limit: Option<Duration>) -> Result<()> {
        let root = config.state_directory.clone();
        let mut owner = Self::open(config)?;
        let deadline = limit.map(|d| Instant::now() + d);
        let result = async {
            while !stop.load(Ordering::Acquire) && deadline.is_none_or(|d| Instant::now() < d) {
                let until = deadline
                    .unwrap_or(Instant::now() + Duration::from_secs(110))
                    .min(Instant::now() + Duration::from_secs(110));
                // Rebind under the SAME custody identity only inside the original
                // operator permission. Original scope/connection caps remain.
                owner = owner.epoch(stop.clone(), until).await?;
            }
            Ok::<_, anyhow::Error>(())
        }
        .await;
        let result = if stop.load(Ordering::Acquire) {
            Ok(())
        } else {
            result
        };
        atomic(
            &root.join("status.json"),
            &serde_json::to_vec(&serde_json::json!({
                "version":1,"phase":if result.is_ok(){"stopped"}else{"failed_closed"},"anonymous":false
            }))?,
        )?;
        result
    }
}
