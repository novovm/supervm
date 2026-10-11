//! One long-lived platform owner, finite network epochs, and external signed
//! source handoff. This module owns no publisher key and no chat protocol.
use anyhow::{ensure, Context, Result};
use ed25519_dalek::SigningKey;
use novovm_network::{
    product_rendezvous_daemon::{
        run_owned_rendezvous_epoch_v2, RendezvousAuthorizationUnavailable,
        RendezvousDaemonConfigV2, RendezvousDaemonReportV2, RendezvousEpochOwnerV1,
    },
    product_service_lease::{ServiceLeaseBookV1, ServiceLeaseFileV1, ServiceLeasePersistenceV1},
};
use rand::{rngs::OsRng, RngCore};
use serde::{Deserialize, Serialize};
use std::{
    fs::{File, OpenOptions},
    future::Future,
    io::{Read, Write},
    path::{Path, PathBuf},
    pin::Pin,
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc, Mutex,
    },
    time::{Duration, Instant},
};
use tokio::sync::oneshot;
use zeroize::Zeroizing;

#[derive(Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Config {
    pub version: u8,
    pub epoch: RendezvousDaemonConfigV2,
    pub prepare_before_ms: u64,
}
impl Config {
    pub fn load(path: &Path) -> Result<Self> {
        let value: Self = serde_json::from_slice(&read_bounded(path, 65536)?)?;
        value.epoch.validate()?;
        ensure!(
            value.version == 1
                && !value.epoch.enroll
                && value.epoch.grant_sequence == 1
                && (5000..=30000).contains(&value.prepare_before_ms)
                && value.epoch.run_for_ms >= value.prepare_before_ms * 2,
            "owner preparation bounds"
        );
        ensure!(
            value
                .epoch
                .journal_path
                .file_name()
                .is_some_and(|n| n == "service.journal")
                && value.epoch.source_path == value.root()?.join("authority.json"),
            "managed owner paths required"
        );
        ensure!(
            value.epoch.bind_address.is_none_or(|a| a.port() == 0),
            "overlapping epochs require independently allocated UDP ports"
        );
        check_path(value.root()?)?;
        Ok(value)
    }
    pub fn status(&self, phase: &str, epochs: u64) -> Result<()> {
        atomic(
            &self.root()?.join("status.json"),
            &serde_json::to_vec(&serde_json::json!({
                "version":1,"phase":phase,"completed_epochs":epochs,"anonymous":false
            }))?,
        )
    }
    pub fn record_start_failure(&self) {
        let Ok(root) = self.root() else { return };
        let path = root.join("owner.lock");
        if check_path(&path).is_err() {
            return;
        }
        if let Ok(lock) = OpenOptions::new().read(true).write(true).open(path) {
            if lock.try_lock().is_ok() {
                let _ = self.status("failed_closed", 0);
            }
        }
    }
    fn root(&self) -> Result<&Path> {
        self.epoch.journal_path.parent().context("owner directory")
    }
}

pub fn check_path(path: &Path) -> Result<()> {
    ensure!(path.is_absolute(), "absolute private path required");
    for part in path.ancestors() {
        if let Ok(meta) = std::fs::symlink_metadata(part) {
            use std::os::windows::fs::MetadataExt;
            ensure!(
                !meta.file_type().is_symlink() && meta.file_attributes() & 0x400 == 0,
                "managed path must not use a reparse point"
            );
        }
    }
    Ok(())
}
pub fn read_bounded(path: &Path, limit: usize) -> Result<Vec<u8>> {
    check_path(path)?;
    let mut data = Vec::new();
    File::open(path)?
        .take((limit + 1) as u64)
        .read_to_end(&mut data)?;
    ensure!(data.len() <= limit, "owner file size");
    Ok(data)
}
fn create(path: &Path, data: &[u8]) -> Result<()> {
    check_path(path)?;
    let mut file = OpenOptions::new().write(true).create_new(true).open(path)?;
    file.write_all(data)?;
    file.sync_all()?;
    Ok(())
}
fn atomic(path: &Path, data: &[u8]) -> Result<()> {
    check_path(path)?;
    let stage = path.with_extension("pending");
    // A stale interrupted status write is safe to replace: it contains no keys
    // or authoritative state. This helper is never used for a signing journal.
    if stage.try_exists()? {
        std::fs::remove_file(&stage)?;
    }
    create(&stage, data)?;
    std::fs::rename(stage, path)?;
    Ok(())
}

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct State {
    version: u8,
    next_sequence: u64,
    enrollment_started: bool,
}
struct Durable {
    store: Arc<ServiceLeaseFileV1>,
    bytes: Vec<u8>,
    state: State,
}
impl Durable {
    fn save(&mut self) -> Result<()> {
        let next = serde_json::to_vec(&self.state)?;
        self.store.commit(&self.bytes, &next)?;
        self.bytes = next;
        Ok(())
    }
    fn reserve(&mut self, count: usize) -> Result<(u64, bool)> {
        let sequence = self.state.next_sequence;
        // Reserve the proposal plus every possible online lease before signing
        // anything. Crashes/failed handoffs consume the reservation permanently.
        self.state.next_sequence = sequence
            .checked_add(count as u64 + 1)
            .context("owner sequence exhausted")?;
        self.save()?;
        Ok((sequence, !self.state.enrollment_started))
    }
    fn activate(&mut self) -> Result<()> {
        if !self.state.enrollment_started {
            self.state.enrollment_started = true;
            self.save()?; // Missing service journal after this point is fatal.
        }
        Ok(())
    }
}

pub struct Owner {
    config: Config,
    key: SigningKey,
    storage: Zeroizing<[u8; 32]>,
    durable: Arc<Mutex<Durable>>,
    _lock: File,
}
impl Owner {
    pub fn open(config: Config) -> Result<Self> {
        let root = config.root()?;
        check_path(&root.join("owner.lock"))?;
        let lock = OpenOptions::new()
            .read(true)
            .write(true)
            .create(true)
            .truncate(false)
            .open(root.join("owner.lock"))?;
        lock.try_lock()
            .map_err(|_| anyhow::anyhow!("owner already running"))?;
        config.status("initializing", 0)?;
        let keys = root.join("keys.dpapi");
        let marker = root.join("enrollment.started");
        let request = root.join("initialize.request");
        let journal = root.join("owner.journal");
        if !marker.try_exists()? {
            ensure!(
                request.is_file()
                    && !keys.try_exists()?
                    && !journal.try_exists()?
                    && !config.epoch.journal_path.try_exists()?,
                "explicit fresh owner enrollment required"
            );
            create(&marker, b"NOVOVM account custody v1")?;
            std::fs::remove_file(&request)?;
            let mut material = Zeroizing::new([0u8; 72]);
            material[..8].copy_from_slice(b"NVKEYS03");
            OsRng.try_fill_bytes(&mut material[8..])?;
            create(&keys, &crate::windows_custody::protect(material.as_ref())?)?;
            let storage = Zeroizing::new(<[u8; 32]>::try_from(&material[40..])?);
            let initial = serde_json::to_vec(&State {
                version: 1,
                next_sequence: 1,
                enrollment_started: false,
            })?;
            ServiceLeaseFileV1::create(&journal, storage, &initial)?;
        }
        ensure!(!request.try_exists()?, "ambiguous owner enrollment");
        let material = crate::windows_custody::unprotect(&read_bounded(&keys, 16384)?)?;
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
        let key = SigningKey::from_bytes(&seed);
        ensure!(
            !config
                .epoch
                .trusted_signers
                .contains(&key.verifying_key().to_bytes()),
            "node cannot be its own publisher"
        );
        let (store, bytes) = ServiceLeaseFileV1::open(&journal, storage.clone())?;
        let state: State = serde_json::from_slice(&bytes)?;
        ensure!(
            state.version == 1 && state.next_sequence > 0,
            "owner state version/sequence"
        );
        ensure!(
            state.enrollment_started == config.epoch.journal_path.try_exists()?,
            "incomplete owner activation; manual recovery required"
        );
        if state.enrollment_started {
            // Validate damaged history before opening any new carrier.
            let (lease_file, checkpoint) =
                ServiceLeaseFileV1::open(&config.epoch.journal_path, storage.clone())?;
            ServiceLeaseBookV1::restore(
                config.epoch.validate()?.1,
                &checkpoint,
                novovm_network::product_rendezvous_daemon::now_ms()?,
            )?;
            drop(lease_file);
        }
        // Crash leftovers contain only obsolete public proposals/signed grants.
        // Hold the owner lock and remove only exact, already-reserved names.
        for item in std::fs::read_dir(root)? {
            let path = item?.path();
            let name = path.file_name().and_then(|n| n.to_str()).unwrap_or("");
            let epoch = name.strip_prefix("epoch-").and_then(|s| {
                s.strip_suffix(".proposal.json")
                    .or_else(|| s.strip_suffix(".authority.json"))
            });
            if epoch
                .and_then(|n| n.parse::<u64>().ok())
                .is_some_and(|n| n > 0 && n < state.next_sequence)
            {
                check_path(&path)?;
                std::fs::remove_file(path)?;
            }
        }
        atomic(
            &root.join("identity.json"),
            &serde_json::to_vec(
                &serde_json::json!({"version":1,"node_public_key":key.verifying_key().to_bytes()}),
            )?,
        )?;
        Ok(Self {
            config,
            key,
            storage,
            durable: Arc::new(Mutex::new(Durable {
                store,
                bytes,
                state,
            })),
            _lock: lock,
        })
    }
    fn status(&self, phase: &str, epochs: u64) -> Result<()> {
        self.config.status(phase, epochs)
    }
    fn prepare(&self, activate: bool) -> Result<Epoch> {
        let mut config = self.config.epoch.clone();
        let (sequence, enroll) = self
            .durable
            .lock()
            .map_err(|_| anyhow::anyhow!("owner unavailable"))?
            .reserve(config.max_sessions)?;
        config.grant_sequence = sequence;
        config.enroll = enroll;
        let proposal_path = self
            .config
            .root()?
            .join(format!("epoch-{sequence}.proposal.json"));
        config.source_path = self
            .config
            .root()?
            .join(format!("epoch-{sequence}.authority.json"));
        ensure!(
            !proposal_path.try_exists()? && !config.source_path.try_exists()?,
            "unexpected previous epoch files"
        );
        let stop = Arc::new(AtomicBool::new(false));
        let activation = Arc::new(AtomicBool::new(activate));
        let (send, ready) = oneshot::channel();
        let durable = self.durable.clone();
        let hooks = RendezvousEpochOwnerV1 {
            stop: stop.clone(),
            activate: activation.clone(),
            prepared: Box::new(move || send.send(()).map_err(|_| anyhow::anyhow!("owner stopped"))),
            before_activate: Box::new(move || {
                durable
                    .lock()
                    .map_err(|_| anyhow::anyhow!("owner unavailable"))?
                    .activate()
            }),
        };
        let output = proposal_path.clone();
        let source = config.source_path.clone();
        let future = Box::pin(run_owned_rendezvous_epoch_v2(
            config,
            self.key.clone(),
            self.storage.clone(),
            move |record| create(&output, &serde_json::to_vec(record)?),
            Some(hooks),
        ));
        Ok(Epoch {
            future,
            ready,
            stop,
            activate: activation,
            started: Instant::now(),
            files: [proposal_path, source],
        })
    }
    /// No tight restart loop: one prepared replacement, finite network leases,
    /// and fail-stop on source/storage/policy errors. SCM may recover crashes a
    /// bounded number of times; normal security failures are not crash retries.
    pub async fn run(&self, stop: Arc<AtomicBool>, max_epochs: Option<u64>) -> Result<()> {
        let mut failures = 0u32;
        loop {
            self.status("waiting_authorization", 0)?;
            let result = self.run_inner(stop.clone(), max_epochs).await;
            if max_epochs.is_none()
                && result.as_ref().err().is_some_and(|e| {
                    e.downcast_ref::<RendezvousAuthorizationUnavailable>()
                        .is_some()
                })
            {
                // Same supervised process, no live expired carrier. Only a
                // missing response is retried; no supplied bad grant is ignored.
                let base = (1000u64 << failures.min(6)).min(60000);
                failures = failures.saturating_add(1);
                self.status("waiting_publisher_retry", 0)?;
                tokio::select! {
                    _ = tokio::time::sleep(Duration::from_millis(base + rand::random::<u64>() % (base / 4 + 1))) => {}
                    _ = stopped(stop.clone()) => { return self.status("stopped", 0); }
                }
                continue;
            }
            if result.is_err() {
                let _ = self.status("failed_closed", 0);
            }
            return result;
        }
    }
    async fn run_inner(&self, stop: Arc<AtomicBool>, max_epochs: Option<u64>) -> Result<()> {
        if stop.load(Ordering::Acquire) {
            return self.status("stopped", 0);
        }
        let mut active = self.prepare(true)?;
        let mut completed = 0;
        loop {
            tokio::select! {
                result = &mut active.future => { result?; anyhow::bail!("epoch ended before authorization"); }
                result = &mut active.ready => {
                    if result.is_err() {
                        // The sender is dropped before carrier cleanup finishes.
                        // Preserve the actual typed failure for recovery policy.
                        active.future.as_mut().await?;
                        anyhow::bail!("epoch preparation stopped");
                    }
                }
                _ = stopped(stop.clone()) => { active.stop.store(true, Ordering::Release); return self.status("stopped", completed); }
            }
            self.status("authorized", completed)?;
            let prepare_at = active.started
                + Duration::from_millis(
                    self.config.epoch.run_for_ms - self.config.prepare_before_ms,
                );
            tokio::select! {
                result = &mut active.future => {
                    result?;
                    completed += 1;
                    if max_epochs.is_some_and(|max| completed >= max) { return self.status("stopped", completed); }
                    active = self.prepare(true)?;
                    continue;
                }
                _ = tokio::time::sleep_until(prepare_at.into()) => {}
                _ = stopped(stop.clone()) => { active.stop.store(true, Ordering::Release); return self.status("stopped", completed); }
            }
            if max_epochs.is_some_and(|max| completed + 1 >= max) {
                tokio::select! {
                    result = &mut active.future => { result?; return self.status("stopped", completed + 1); }
                    _ = stopped(stop.clone()) => { active.stop.store(true, Ordering::Release); return self.status("stopped", completed); }
                }
            }
            let mut next = self.prepare(false)?;
            let mut ended = false;
            // Keep polling the old service while the external publisher signs
            // the fresh carrier. Ready is never inferred from a self-signature.
            loop {
                tokio::select! {
                    result = &mut next.future => { result?; anyhow::bail!("prepared epoch ended early"); }
                    result = &mut next.ready => {
                        if result.is_err() {
                            next.future.as_mut().await?;
                            anyhow::bail!("next preparation stopped");
                        }
                        break;
                    }
                    result = &mut active.future, if !ended => { result?; ended = true; self.status("waiting_authorization", completed)?; }
                    _ = stopped(stop.clone()) => { return self.status("stopped", completed); }
                }
            }
            active.stop.store(true, Ordering::Release);
            if !ended {
                // Cancellation is expected only after authenticated readiness.
                // Dropping on timeout also releases the old book; no old IO can
                // outlive its borrowed Iroh scope.
                let _ = tokio::time::timeout(Duration::from_secs(3), &mut active.future).await;
            }
            drop(active);
            next.activate.store(true, Ordering::Release);
            // Its ready notification was consumed above; replace with a local
            // already-complete notification, not another network authorization.
            let (send, receive) = oneshot::channel();
            let _ = send.send(());
            next.ready = receive;
            active = next;
            completed += 1;
        }
    }
}
async fn stopped(stop: Arc<AtomicBool>) {
    while !stop.load(Ordering::Acquire) {
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
}
struct Epoch {
    future: Pin<Box<dyn Future<Output = Result<RendezvousDaemonReportV2>>>>,
    ready: oneshot::Receiver<()>,
    stop: Arc<AtomicBool>,
    activate: Arc<AtomicBool>,
    started: Instant,
    files: [PathBuf; 2],
}
impl Drop for Epoch {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Release);
        // Only this generation's public proposal and signed response; no
        // journal/key cleanup. Failed removal is harmless and never recursive.
        for file in &self.files {
            let _ = std::fs::remove_file(file);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[tokio::test(flavor = "current_thread")]
    async fn autonomous_owner_retries_missing_publisher_then_stops_without_serving() -> Result<()> {
        let root =
            std::env::temp_dir().join(format!("novovm-owner-retry-{}", rand::random::<u64>()));
        std::fs::create_dir(&root)?;
        let pin = |n: u8| SigningKey::from_bytes(&[n; 32]).verifying_key().to_bytes();
        let config_path = root.join("config.json");
        create(&root.join("initialize.request"), b"explicit test fixture")?;
        create(
            &config_path,
            &serde_json::to_vec(&serde_json::json!({
                "version":1,"prepare_before_ms":5000,"epoch":{
                    "version":2,"engineering_direct_allowed":true,"trusted_signers":[pin(71)],
                    "minimum_signatures":1,"allowed_clients":[pin(73)],"relays":[],
                    "nameserver":"127.0.0.1","bind_address":"127.0.0.1:0","relay_only":false,
                    "journal_path":root.join("service.journal"),"source_path":root.join("authority.json"),
                    "enroll":false,"record_id":"owner-retry-test","grant_sequence":1,"run_for_ms":10000,"max_sessions":64
                }
            }))?,
        )?;
        let config = Config::load(&config_path)?;
        let owner = Owner::open(config.clone())?;
        let identity = owner.key.verifying_key();
        let previous_status = std::fs::read(root.join("status.json"))?;
        assert!(Owner::open(config.clone()).is_err());
        config.record_start_failure();
        assert_eq!(std::fs::read(root.join("status.json"))?, previous_status);
        let stop = Arc::new(AtomicBool::new(false));
        {
            let task = owner.run(stop.clone(), None);
            tokio::pin!(task);
            let until = tokio::time::sleep(Duration::from_secs(20));
            tokio::pin!(until);
            loop {
                tokio::select! {
                    result = &mut task => { result?; anyhow::bail!("owner exited instead of backing off"); }
                    _ = &mut until => { anyhow::bail!("owner did not retry absent publisher"); }
                    _ = tokio::time::sleep(Duration::from_millis(25)) => {
                        if root.join("epoch-66.proposal.json").exists() { break; }
                    }
                }
            }
            assert!(!root.join("service.journal").exists());
            stop.store(true, Ordering::Release);
            tokio::time::timeout(Duration::from_secs(2), &mut task).await??;
        }
        drop(owner);
        let restored = Owner::open(config)?;
        assert_eq!(restored.key.verifying_key(), identity);
        assert_eq!(restored.durable.lock().unwrap().state.next_sequence, 131);
        drop(restored);
        std::fs::remove_dir_all(root)?;
        Ok(())
    }
}
