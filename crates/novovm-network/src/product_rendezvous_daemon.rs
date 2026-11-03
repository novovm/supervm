//! Explicit engineering service epoch for the existing relay executable.
//! Platform custody and source publication belong to independent owners. The
//! daemon never generates a trust root, extends an epoch, or invokes legacy RPC.
use crate::{
    carrier_stream::CarrierStreamV1,
    discovery_control::serve_rendezvous,
    iroh_transport::{with_iroh_scope_v1, IrohConfigV1, IrohControlV1},
    opaque_rendezvous::RendezvousStoreV1,
    product_directory::{sign_relay_record_v1, BootstrapSourceV1, BootstrapTrustPolicyV1},
    product_overlay::{TransportIdentity, MLDSA65_PUBLIC_KEY_BYTES},
    product_service_lease::{
        serve_service_offer_v2, IrohServiceEndpointV1, ServiceLeaseBookV1, ServiceLeaseFileV1,
        ServiceLeaseHostV1,
    },
};
use anyhow::{ensure, Context, Result};
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
    pub relay_identity_key_path: PathBuf,
    pub chain_id: u64,
    pub library: PathBuf,
    pub library_sha256: String,
    pub trusted_signers: Vec<Vec<u8>>,
    pub minimum_signatures: usize,
    pub allowed_clients: Vec<Vec<u8>>,
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
    File::open(path)?
        .take(1024 * 1024 + 1)
        .read_to_end(&mut bytes)?;
    ensure!(bytes.len() <= 1024 * 1024, "service input size");
    Ok(serde_json::from_slice(&bytes)?)
}

impl RendezvousDaemonConfigV2 {
    pub fn validate(&self) -> Result<(IrohConfigV1, BootstrapTrustPolicyV1)> {
        ensure!(
            self.version == 3 && self.engineering_direct_allowed,
            "explicit engineering service required"
        );
        ensure!(
            self.chain_id != 0
                && self.relay_identity_key_path.is_absolute()
                && self.library.is_absolute()
                && self.library_sha256.len() == 64
                && self.library_sha256.bytes().all(|b| b.is_ascii_hexdigit()),
            "qualified native identity configuration required"
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
                ensure!(
                    key.len() == MLDSA65_PUBLIC_KEY_BYTES && key.iter().any(|b| *b != 0),
                    "native service key"
                );
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
                    BootstrapTrustPolicyV1::single_signer(key.clone())
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
/// inherited stdin pipe: NVKEYS04 + storage key. The qualified native signer
/// is injected by the original platform custody owner, not read as a raw seed.
/// Public addressing is emitted only to the owner's redirected stdout pipe.
/// Production installation must implement protected custody and source renewal;
/// redirecting raw key files is not a supported custody implementation.
pub fn run_rendezvous_cli_v2(path: &Path, key: TransportIdentity) -> Result<()> {
    let config: RendezvousDaemonConfigV2 = read_json(path)?;
    config.validate()?; // Reject configuration before reading keys or networking.
    ensure!(
        !std::io::stdin().is_terminal() && !std::io::stdout().is_terminal(),
        "private owner pipes required"
    );
    let mut material = Zeroizing::new([0u8; 40]);
    std::io::stdin().read_exact(material.as_mut())?;
    ensure!(&material[..8] == b"NVKEYS04", "owner pipe version");
    let storage = Zeroizing::new(<[u8; 32]>::try_from(&material[8..])?);
    ensure!(*storage != [0; 32], "storage key required");
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
pub type RendezvousSourceObserverV1 = Box<dyn Fn(&[BootstrapSourceV1]) -> Result<()>>;

pub struct RendezvousEpochOwnerV1 {
    pub stop: Arc<AtomicBool>,
    pub activate: Arc<AtomicBool>,
    pub prepared: Box<dyn FnOnce() -> Result<()>>,
    pub before_activate: Box<dyn FnOnce() -> Result<()>>,
    /// Only called after activation/durable acceptance, including revocation.
    pub sources_changed: RendezvousSourceObserverV1,
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
    key: TransportIdentity,
    storage: Zeroizing<[u8; 32]>,
    proposal: impl FnOnce(&crate::product_directory::PeerSignedRelayRecordV1) -> Result<()> + 'static,
) -> Result<RendezvousDaemonReportV2> {
    run_owned_rendezvous_epoch_v2(config, key, storage, proposal, None).await
}

/// The optional owner is local lifecycle control, never a source of network trust.
pub async fn run_owned_rendezvous_epoch_v2(
    config: RendezvousDaemonConfigV2,
    key: TransportIdentity,
    storage: Zeroizing<[u8; 32]>,
    proposal: impl FnOnce(&crate::product_directory::PeerSignedRelayRecordV1) -> Result<()> + 'static,
    owner: Option<RendezvousEpochOwnerV1>,
) -> Result<RendezvousDaemonReportV2> {
    let (carrier, policy) = config.validate()?;
    ensure!(*storage != [0; 32], "independent service keys required");
    let pin = key.public_key().to_vec();
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
            let mut book =
                ServiceLeaseBookV1::restore(policy.clone(), &checkpoint, now_ms()?, key.clone())?;
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
        let mut sources_changed = None;
        let recovered = if let Some(owner) = owner {
            // Independent trust and exact new identity are checked BEFORE ready.
            // The durable history check is repeated at activation after the old
            // owner exits; passing this check never bypasses a tombstone.
            let mut candidate = ServiceLeaseBookV1::new(policy.clone(), sources.clone(), now_ms()?, key.clone())?;
            candidate.check_route(&pin, endpoint.address.endpoint_key, endpoint.instance, now_ms()?)?;
            (owner.prepared)()?;
            while !owner.activate.load(Ordering::Acquire) {
                ensure!(Instant::now() < deadline && !owner.stop.load(Ordering::Acquire), "epoch activation cancelled/expired");
                tokio::time::sleep(Duration::from_millis(20)).await;
            }
            sources = read_json(&config.source_path)?; // Recheck a revocation arriving during handoff.
            (owner.before_activate)()?;
            sources_changed = Some(owner.sources_changed);
            if config.enroll {
                ensure!(!config.journal_path.try_exists()?, "service already enrolled");
                None
            } else {
                let (file, checkpoint) = ServiceLeaseFileV1::open(&config.journal_path, storage.clone())?;
                let mut book = ServiceLeaseBookV1::restore(policy.clone(), &checkpoint, now_ms()?, key.clone())?;
                book.attach_persistence(file, &checkpoint)?;
                Some(book)
            }
        } else { recovered };
        let mut last_sources = serde_json::to_vec(&sources)?;
        let mut book = match recovered {
            Some(mut book) => { book.replace_sources(sources, now_ms()?)?; book }
            None => ServiceLeaseBookV1::new(policy, sources, now_ms()?, key.clone())?,
        };
        book.check_route(&pin, endpoint.address.endpoint_key, endpoint.instance, now_ms()?)?;
        if config.enroll {
            let checkpoint = book.checkpoint()?;
            let file = ServiceLeaseFileV1::create(&config.journal_path, storage, &checkpoint)?;
            book.attach_persistence(file, &checkpoint)?;
        }
        let published = book.sources();
        let host = Arc::new(ServiceLeaseHostV1::new(key.clone(), book)?.with_lease_limit(30_000)?);
        *authority.lock().map_err(|_| anyhow::anyhow!("service supervision unavailable"))? =
            Some((host.clone(), endpoint.address.endpoint_key, endpoint.instance));
        if let Some(publish) = &sources_changed { publish(&published)?; }
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
                        host.replace_sources(next.clone(), now_ms()?)?;
                        if let Some(publish) = &sources_changed { publish(&next)?; }
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn daemon_configuration_requires_explicit_native65_custody_and_rejects_legacy_keys(
    ) -> Result<()> {
        let root = tempfile::tempdir()?;
        let mut config = RendezvousDaemonConfigV2 {
            version: 3,
            engineering_direct_allowed: true,
            relay_identity_key_path: root.path().join("purpose.key"),
            chain_id: 7,
            library: root.path().join("qualified-library"),
            library_sha256: "a".repeat(64),
            trusted_signers: vec![vec![10; MLDSA65_PUBLIC_KEY_BYTES]],
            minimum_signatures: 1,
            allowed_clients: vec![vec![11; MLDSA65_PUBLIC_KEY_BYTES]],
            relays: vec![],
            nameserver: "1.1.1.1".parse()?,
            bind_address: None,
            relay_only: false,
            journal_path: root.path().join("journal"),
            source_path: root.path().join("source"),
            enroll: true,
            record_id: "record".into(),
            grant_sequence: 1,
            run_for_ms: 5000,
            max_sessions: 4,
        };
        config.validate()?;
        config.version = 2;
        assert!(config.validate().is_err());
        config.version = 3;
        config.trusted_signers[0] = vec![10; 32];
        assert!(config.validate().is_err());
        config.trusted_signers[0] = vec![10; MLDSA65_PUBLIC_KEY_BYTES];
        config.allowed_clients[0] = vec![11; 32];
        assert!(config.validate().is_err());
        Ok(())
    }
}
