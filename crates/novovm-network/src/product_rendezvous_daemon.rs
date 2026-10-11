//! Explicit engineering service epoch for the existing relay executable.
//! Platform custody and source publication belong to independent owners. The
//! daemon never generates a trust root, extends an epoch, or invokes legacy RPC.
use crate::{
    carrier_stream::CarrierStreamV1,
    discovery_control::serve_rendezvous,
    iroh_transport::{with_iroh_scope_v1, IrohConfigV1, IrohControlV1},
    opaque_rendezvous::RendezvousStoreV1,
    product_directory::{sign_relay_record_v1, BootstrapSourceV1, BootstrapTrustPolicyV1},
    product_service_lease::{
        serve_service_offer_v2, IrohServiceEndpointV1, ServiceLeaseBookV1, ServiceLeaseFileV1,
        ServiceLeaseHostV1,
    },
};
use anyhow::{ensure, Context, Result};
use ed25519_dalek::{SigningKey, VerifyingKey};
use futures_util::{stream::FuturesUnordered, StreamExt};
use serde::{de::DeserializeOwned, Deserialize, Serialize};
use std::{
    collections::BTreeSet,
    fs::File,
    io::{IsTerminal, Read, Write},
    net::{IpAddr, SocketAddr},
    path::{Path, PathBuf},
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc, Mutex,
    },
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};
use zeroize::Zeroizing;

#[derive(Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RendezvousDaemonConfigV2 {
    pub version: u8,
    pub engineering_direct_allowed: bool,
    pub trusted_signers: Vec<[u8; 32]>,
    pub minimum_signatures: usize,
    pub allowed_clients: Vec<[u8; 32]>,
    pub relays: Vec<String>,
    pub nameserver: IpAddr,
    pub bind_address: Option<SocketAddr>,
    pub relay_only: bool,
    pub journal_path: PathBuf,
    pub source_path: PathBuf,
    /// Explicit creation only. Reopen failures never fall back to enrollment.
    pub enroll: bool,
    pub record_id: String,
    /// A publisher/owner chooses a sequence above persisted history on restart.
    /// A low value is rejected by the existing durable book, never repaired.
    pub grant_sequence: u64,
    pub run_for_ms: u64,
    pub max_sessions: usize,
}

#[derive(Default, Serialize)]
pub struct RendezvousDaemonReportV2 {
    pub accepted: usize,
    pub completed: usize,
    pub rejected_or_timed_out: usize,
}

pub fn now_ms() -> Result<u64> {
    Ok(SystemTime::now()
        .duration_since(UNIX_EPOCH)?
        .as_millis()
        .try_into()?)
}

fn read_json<T: DeserializeOwned>(path: &Path) -> Result<T> {
    let mut bytes = Vec::new();
    File::open(path)?.take(65537).read_to_end(&mut bytes)?;
    ensure!(bytes.len() <= 65536, "service input size");
    Ok(serde_json::from_slice(&bytes)?)
}

fn strong_key(key: &[u8; 32]) -> Result<()> {
    let key = VerifyingKey::from_bytes(key)?;
    ensure!(
        !key.is_weak()
            && key.to_edwards().is_torsion_free()
            && key.to_edwards().compress().to_bytes() == key.to_bytes(),
        "service key"
    );
    Ok(())
}

impl RendezvousDaemonConfigV2 {
    pub fn validate(&self) -> Result<(IrohConfigV1, BootstrapTrustPolicyV1)> {
        ensure!(
            self.version == 2 && self.engineering_direct_allowed,
            "explicit engineering service required"
        );
        ensure!(
            (1..=8).contains(&self.trusted_signers.len())
                && (1..=self.trusted_signers.len()).contains(&self.minimum_signatures)
                && (1..=16).contains(&self.allowed_clients.len()),
            "service authorization bounds"
        );
        for keys in [&self.trusted_signers, &self.allowed_clients] {
            ensure!(
                keys.iter().collect::<BTreeSet<_>>().len() == keys.len(),
                "duplicate service key"
            );
            for key in keys {
                strong_key(key)?;
            }
        }
        ensure!(
            (5_000..=110_000).contains(&self.run_for_ms)
                && (1..=64).contains(&self.max_sessions)
                && self.grant_sequence > 0
                && !self.record_id.is_empty()
                && self.record_id.len() <= 128,
            "bounded service epoch required"
        );
        ensure!(
            self.journal_path.is_absolute()
                && self.source_path.is_absolute()
                && self.journal_path != self.source_path,
            "separate absolute owner paths required"
        );
        let mut carrier = IrohConfigV1::new(&self.relays, self.nameserver)?;
        if let Some(address) = self.bind_address {
            carrier = carrier.with_bind_address(address)?;
        }
        if self.relay_only {
            carrier = carrier.with_relay_only()?;
        }
        let policy = BootstrapTrustPolicyV1 {
            allowed_signer_peer_ids: self
                .trusted_signers
                .iter()
                .map(|key| {
                    BootstrapTrustPolicyV1::single_signer(*key)
                        .allowed_signer_peer_ids
                        .into_iter()
                        .next()
                        .unwrap()
                })
                .collect(),
            minimum_valid_signatures: self.minimum_signatures,
        };
        Ok((carrier, policy))
    }
}

/// Called by the shared relay selector. Key bytes arrive ONLY over a private
/// inherited stdin pipe: NVKEYS02 + independent signing seed + storage key.
/// Public addressing is emitted only to the owner's redirected stdout pipe.
/// Production installation must implement protected custody and source renewal;
/// redirecting raw key files is not a supported custody implementation.
pub fn run_rendezvous_cli_v2(path: &Path) -> Result<()> {
    let config: RendezvousDaemonConfigV2 = read_json(path)?;
    config.validate()?; // Reject configuration before reading keys or networking.
    ensure!(
        !std::io::stdin().is_terminal() && !std::io::stdout().is_terminal(),
        "private owner pipes required"
    );
    let mut material = Zeroizing::new([0u8; 72]);
    std::io::stdin().read_exact(material.as_mut())?;
    ensure!(&material[..8] == b"NVKEYS02", "owner pipe version");
    let seed = Zeroizing::new(<[u8; 32]>::try_from(&material[8..40])?);
    let storage = Zeroizing::new(<[u8; 32]>::try_from(&material[40..])?);
    ensure!(
        *seed != *storage && *seed != [0; 32] && *storage != [0; 32],
        "independent service keys required"
    );
    let key = SigningKey::from_bytes(&seed);
    drop(seed);
    drop(material);
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()?;
    let report = runtime.block_on(run_rendezvous_epoch_v2(config, key, storage, |record| {
        let mut out = std::io::stdout().lock();
        // Self-signed proposal, not an approved source or a privacy assertion.
        serde_json::to_writer(
            &mut out,
            &serde_json::json!({"event":"awaiting_authorization","record":record}),
        )?;
        out.write_all(b"\n")?;
        out.flush()?;
        Ok(())
    }))?;
    serde_json::to_writer(std::io::stdout().lock(), &report)?;
    Ok(())
}

/// Platform supervisor controls preparation and activation separately. Preparing a
/// fresh carrier does not open the authorization journal or serve clients. The
/// previous epoch must release its journal before activation is permitted.
pub struct RendezvousEpochOwnerV1 {
    pub stop: Arc<AtomicBool>,
    pub activate: Arc<AtomicBool>,
    pub prepared: Box<dyn FnOnce() -> Result<()>>,
    pub before_activate: Box<dyn FnOnce() -> Result<()>>,
}

/// Only an absent publisher response is eligible for automatic backoff. Invalid
/// signatures, revocations, storage failures and expired supplied grants are not.
#[derive(Debug)]
pub struct RendezvousAuthorizationUnavailable;
impl std::fmt::Display for RendezvousAuthorizationUnavailable {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("source authorization timed out")
    }
}
impl std::error::Error for RendezvousAuthorizationUnavailable {}

/// One finite instance, no chat state. Restart uses a fresh transport identity
/// and instance, and therefore requires a new independently signed grant.
pub async fn run_rendezvous_epoch_v2(
    config: RendezvousDaemonConfigV2,
    key: SigningKey,
    storage: Zeroizing<[u8; 32]>,
    proposal: impl FnOnce(&crate::product_directory::PeerSignedRelayRecordV1) -> Result<()> + 'static,
) -> Result<RendezvousDaemonReportV2> {
    run_owned_rendezvous_epoch_v2(config, key, storage, proposal, None).await
}

/// The optional owner is local lifecycle control, never a source of network trust.
pub async fn run_owned_rendezvous_epoch_v2(
    config: RendezvousDaemonConfigV2,
    key: SigningKey,
    storage: Zeroizing<[u8; 32]>,
    proposal: impl FnOnce(&crate::product_directory::PeerSignedRelayRecordV1) -> Result<()> + 'static,
    owner: Option<RendezvousEpochOwnerV1>,
) -> Result<RendezvousDaemonReportV2> {
    let (carrier, policy) = config.validate()?;
    ensure!(
        key.to_bytes() != *storage && *storage != [0; 32],
        "independent service keys required"
    );
    let pin = key.verifying_key().to_bytes();
    ensure!(
        !config.allowed_clients.contains(&pin),
        "service and client identities must differ"
    );
    // Fail restore/lock/key corruption BEFORE opening a network endpoint.
    let restore = || -> Result<Option<ServiceLeaseBookV1>> {
        Ok(if config.enroll {
            ensure!(
                !config.journal_path.try_exists()?,
                "service already enrolled"
            );
            None
        } else {
            let (file, checkpoint) =
                ServiceLeaseFileV1::open(&config.journal_path, storage.clone())?;
            let mut book = ServiceLeaseBookV1::restore(policy.clone(), &checkpoint, now_ms()?)?;
            book.attach_persistence(file, &checkpoint)?;
            Some(book)
        })
    };
    let recovered = if owner.is_none() { restore()? } else { None };
    let deadline = Instant::now() + Duration::from_millis(config.run_for_ms);
    let expires = now_ms()?
        .checked_add(config.run_for_ms)
        .context("service clock")?;
    // Leave a small orderly shutdown budget inside the unchanged carrier cap.
    type LiveAuthority = Option<(Arc<ServiceLeaseHostV1>, [u8; 32], [u8; 32])>;
    let authority: Arc<Mutex<LiveAuthority>> = Arc::new(Mutex::new(None));
    let probe = authority.clone();
    let stopped = owner.as_ref().map(|owner| owner.stop.clone());
    let control = IrohControlV1::new(deadline + Duration::from_secs(2), move || {
        ensure!(
            !stopped.as_ref().is_some_and(|s| s.load(Ordering::Acquire)),
            "service stopped by owner"
        );
        let state = probe
            .lock()
            .map_err(|_| anyhow::anyhow!("service supervision unavailable"))?;
        if let Some((host, carrier, instance)) = state.as_ref() {
            host.check_route(*carrier, *instance, now_ms()?)?;
        }
        Ok(())
    })?;
    with_iroh_scope_v1(carrier, control, move |scope| Box::pin(async move {
        if !config.relays.is_empty() { scope.online().await?; }
        let endpoint = IrohServiceEndpointV1 { instance: rand::random(), address: scope.address()? };
        let record = sign_relay_record_v1(&key, &config.record_id, vec![endpoint.endpoint()?],
            now_ms()?, expires, config.grant_sequence)?;
        proposal(&record)?;
        // The separate publisher wraps this exact proposal in a trusted source.
        // An old source file is rejected; no local root or replacement is made.
        let mut sources: Vec<BootstrapSourceV1> = loop {
            if Instant::now() >= deadline { return Err(RendezvousAuthorizationUnavailable.into()); }
            match read_json(&config.source_path) {
                Ok(sources) => break sources,
                Err(e) if e.downcast_ref::<std::io::Error>().is_some_and(|e| e.kind() == std::io::ErrorKind::NotFound) =>
                    tokio::time::sleep(Duration::from_millis(50)).await,
                Err(e) => return Err(e),
            }
        };
        let recovered = if let Some(owner) = owner {
            // Independent trust and exact new identity are checked BEFORE ready.
            // The durable history check is repeated at activation after the old
            // owner exits; passing this check never bypasses a tombstone.
            let mut candidate = ServiceLeaseBookV1::new(policy.clone(), sources.clone(), now_ms()?)?;
            candidate.check_route(pin, endpoint.address.endpoint_key, endpoint.instance, now_ms()?)?;
            (owner.prepared)()?;
            while !owner.activate.load(Ordering::Acquire) {
                ensure!(Instant::now() < deadline && !owner.stop.load(Ordering::Acquire), "epoch activation cancelled/expired");
                tokio::time::sleep(Duration::from_millis(20)).await;
            }
            sources = read_json(&config.source_path)?; // Recheck a revocation arriving during handoff.
            (owner.before_activate)()?;
            if config.enroll {
                ensure!(!config.journal_path.try_exists()?, "service already enrolled");
                None
            } else {
                let (file, checkpoint) = ServiceLeaseFileV1::open(&config.journal_path, storage.clone())?;
                let mut book = ServiceLeaseBookV1::restore(policy.clone(), &checkpoint, now_ms()?)?;
                book.attach_persistence(file, &checkpoint)?;
                Some(book)
            }
        } else { recovered };
        let mut last_sources = serde_json::to_vec(&sources)?;
        let mut book = match recovered {
            Some(mut book) => { book.replace_sources(sources, now_ms()?)?; book }
            None => ServiceLeaseBookV1::new(policy, sources, now_ms()?)?,
        };
        book.check_route(pin, endpoint.address.endpoint_key, endpoint.instance, now_ms()?)?;
        if config.enroll {
            let checkpoint = book.checkpoint()?;
            let file = ServiceLeaseFileV1::create(&config.journal_path, storage, &checkpoint)?;
            book.attach_persistence(file, &checkpoint)?;
        }
        let host = Arc::new(ServiceLeaseHostV1::new(key.clone(), book)?.with_lease_limit(30_000)?);
        *authority.lock().map_err(|_| anyhow::anyhow!("service supervision unavailable"))? =
            Some((host.clone(), endpoint.address.endpoint_key, endpoint.instance));
        let store = Arc::new(Mutex::new(RendezvousStoreV1::new(endpoint.instance, deadline)?));
        let mut active = FuturesUnordered::new();
        let mut accepting = Box::pin(scope.accept_inbound());
        let mut report = RendezvousDaemonReportV2::default();
        // End a little before the wall-clock grant boundary. Wall milliseconds
        // truncate sub-ms precision whereas Instant does not; exact equality
        // must never turn normal shutdown into an expired-authority restart.
        let serving_deadline = deadline - Duration::from_millis(250);
        let mut maintenance = tokio::time::interval(Duration::from_millis(250));
        maintenance.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
        loop {
            tokio::select! {
                biased;
                _ = tokio::time::sleep_until(serving_deadline.into()) => {
                    report.rejected_or_timed_out += active.len();
                    break;
                }
                _ = maintenance.tick() => {
                    let next: Vec<BootstrapSourceV1> = read_json(&config.source_path)?;
                    let encoded = serde_json::to_vec(&next)?;
                    if encoded != last_sources {
                        host.replace_sources(next, now_ms()?)?;
                        last_sources = encoded;
                    }
                    host.check_route(endpoint.address.endpoint_key, endpoint.instance, now_ms()?)?;
                    store.lock().map_err(|_| anyhow::anyhow!("service store unavailable"))?.stats(Instant::now())?;
                }
                Some(result) = active.next(), if !active.is_empty() => {
                    if matches!(result, Ok(Ok(()))) { report.completed += 1; }
                    else { report.rejected_or_timed_out += 1; }
                }
                incoming = &mut accepting, if active.len() < 4 && report.accepted < config.max_sessions => {
                    accepting = Box::pin(scope.accept_inbound());
                    report.accepted += 1;
                    match incoming {
                        Ok(stream) => {
                            let host = &host;
                            let key = &key;
                            let clients = &config.allowed_clients;
                            let store = store.clone();
                            active.push(tokio::time::timeout(Duration::from_secs(30), async move {
                                let mut stream: CarrierStreamV1<'_> = stream.into();
                                tokio::time::timeout(Duration::from_secs(3), serve_service_offer_v2(&mut stream, host, now_ms)).await??;
                                serve_rendezvous(&mut stream, key, clients, store).await?;
                                Ok::<_, anyhow::Error>(())
                            }));
                        }
                        Err(_) => report.rejected_or_timed_out += 1,
                    }
                }
            }
            if report.accepted == config.max_sessions && active.is_empty() { break; }
        }
        drop(active); // End every stream before destroying the instance/store.
        Ok(report)
    })).await
}
