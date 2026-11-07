//! Optional NOVONET fallback inside the original synchronous native tick.
//! One retained current-thread Iroh owner, no second node, listener thread,
//! executor, signing authority or business queue. UDP remains the primary IO.
use super::ResidentNode;
use crate::native_pipeline::consensus::{native::NativeChannel, native_lifecycle::NativeLifecycle};
use anyhow::{ensure, Context, Result};
use novovm_network::{
    bound_session::authenticate_structured_bound_session_observed,
    carrier_stream::CarrierStreamV1,
    duplex::{decode_iroh_envelope_v1, encode_iroh_envelope_v1, HandshakeReplayCacheV1},
    iroh_transport::{IrohAddressV1, IrohConfigV1, IrohEndpointOwnerV1, IrohScopeV1},
    novorudp::{
        carrier_io::NativeCarrierPump, endpoint::NativeEndpoint,
        NovoRudpTransportFrameKindV0 as Kind, NovoRudpTransportFrameV0 as Frame,
    },
    product_directory::{
        sign_relay_record_v1, validate_relay_record_v1, PeerSignedRelayRecordV1,
        RelayCandidatePoolConfigV1, RelayCandidatePoolV1,
    },
    product_overlay::TransportIdentity,
    product_service_lease::{IrohServiceEndpointV1, SelfOwnedTrustV1, ServiceLeaseBookV1},
};
use serde::Deserialize;
use std::{
    collections::BTreeSet,
    future::{poll_fn, Future},
    io::Read,
    net::{IpAddr, SocketAddr},
    path::{Path, PathBuf},
    pin::Pin,
    sync::{
        atomic::{AtomicBool, AtomicU64, Ordering},
        mpsc, Arc, Mutex,
    },
    task::Poll,
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};
use tokio::sync::oneshot;

const MAX_CONFIG: usize = 1024 * 1024;
const SELECTOR: &[u8; 8] = b"NVCHAIN1";
const SELECT_STREAM: u64 = u64::from_be_bytes(*b"NVCHAIN1");
const RECORD_KIND: &str = "native-chain-locator-v1";

#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Config {
    /// Existing approved Iroh relays, not automatically obtained from a peer.
    pub relays: Vec<String>,
    pub nameserver: IpAddr,
    pub bind_address: SocketAddr,
    /// Public signed locator; never a secret/custody file.
    pub public_record_file: PathBuf,
    /// This self-run validator set's public locator inputs, refreshed in place.
    /// Their keys MUST match the original genesis transport mapping.
    pub peer_records: Vec<PathBuf>,
    pub fallback_after_ms: u64,
    pub retry_ms: u64,
    pub udp_recovery_ms: u64,
}
impl Config {
    pub fn load(path: &Path) -> Result<Self> {
        let mut config: Self = read_document(path)?;
        let base = path.parent().context("NOVONET config directory missing")?;
        for path in
            std::iter::once(&mut config.public_record_file).chain(config.peer_records.iter_mut())
        {
            if path.is_relative() {
                *path = base.join(&*path);
            }
        }
        ensure!(
            (100..=60_000).contains(&config.fallback_after_ms)
                && (100..=60_000).contains(&config.retry_ms)
                && (1000..=300_000).contains(&config.udp_recovery_ms),
            "NOVONET bounded timing settings"
        );
        ensure!(
            !config.peer_records.is_empty()
                && config.peer_records.len() <= 63
                && !config.peer_records.contains(&config.public_record_file),
            "NOVONET locator inventory"
        );
        ensure!(
            config.peer_records.iter().collect::<BTreeSet<_>>().len() == config.peer_records.len(),
            "duplicate NOVONET locator source"
        );
        config.carrier()?;
        Ok(config)
    }
    fn carrier(&self) -> Result<IrohConfigV1> {
        IrohConfigV1::new(&self.relays, self.nameserver)?.with_bind_address(self.bind_address)
    }
}
fn read_document<T: serde::de::DeserializeOwned>(path: &Path) -> Result<T> {
    let mut bytes = Vec::new();
    std::fs::File::open(path)?
        .take((MAX_CONFIG + 1) as u64)
        .read_to_end(&mut bytes)?;
    ensure!(bytes.len() <= MAX_CONFIG, "NOVONET local document size");
    serde_json::from_slice(&bytes).context("decode NOVONET local metadata")
}
fn wall_ms() -> Result<u64> {
    Ok(SystemTime::now()
        .duration_since(UNIX_EPOCH)?
        .as_millis()
        .try_into()?)
}

#[derive(Clone, Copy)]
struct PeerView {
    fallback: bool,
    session: Option<[u8; 16]>,
    bound: bool,
}
struct Connected {
    peer: usize,
    endpoint: NativeEndpoint,
    accepted: oneshot::Sender<bool>,
}
type Job<'a> = Pin<Box<dyn Future<Output = Result<()>> + 'a>>;
struct CarrierAttempt<'a> {
    future: Job<'a>,
    endpoint_key: [u8; 32],
    adopted: Arc<AtomicBool>,
}

/// The pinned future owns its scope/streams. No borrowed pump escapes into the
/// node struct, and no partial frame future is discarded between tick polls.
pub struct Driver {
    work: Job<'static>,
    runtime: tokio::runtime::Runtime,
    views: Arc<Mutex<Vec<PeerView>>>,
    connected: mpsc::Receiver<Connected>,
    stop: Arc<AtomicBool>,
    started: Instant,
    config: Config,
    next_udp_probe: Vec<Instant>,
    closed: bool,
    finished: bool,
    activated: bool,
    failures: Arc<AtomicU64>,
}
impl Driver {
    pub fn new(config: Config, node: &ResidentNode) -> Result<Self> {
        let keys = (0..node.controller.peer_count())
            .map(|peer| node.controller.peer_transport_key(peer))
            .collect::<Result<Vec<_>>>()?;
        ensure!(
            keys.len() == config.peer_records.len(),
            "NOVONET locator inputs must cover original validator peers"
        );
        let identity = node.transport_identity();
        let pins = (
            node.template.chain_id,
            node.template.genesis_config_commitment,
            node.template.protocol_commitment,
        );
        Self::with_material(config, identity, keys, pins)
    }
    fn with_material(
        config: Config,
        identity: TransportIdentity,
        keys: Vec<Vec<u8>>,
        pins: (u64, [u8; 32], [u8; 32]),
    ) -> Result<Self> {
        ensure!(
            !keys.is_empty() && keys.len() == config.peer_records.len(),
            "NOVONET peer inventory"
        );
        let started = Instant::now();
        let views = Arc::new(Mutex::new(vec![
            PeerView {
                fallback: false,
                session: None,
                bound: false
            };
            keys.len()
        ]));
        let stop = Arc::new(AtomicBool::new(false));
        let failures = Arc::new(AtomicU64::new(0));
        // One result per configured connection, not a business/data send queue.
        let (sender, connected) = mpsc::sync_channel(keys.len());
        let runtime = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()?;
        let work = Box::pin(run(
            config.clone(),
            identity,
            keys,
            pins,
            views.clone(),
            sender,
            stop.clone(),
            failures.clone(),
        ));
        let next_udp_probe = vec![
            started;
            views
                .lock()
                .map_err(|_| anyhow::anyhow!("NOVONET views poisoned"))?
                .len()
        ];
        Ok(Self {
            work,
            runtime,
            views,
            connected,
            stop,
            started,
            config,
            next_udp_probe,
            closed: false,
            finished: false,
            activated: false,
            failures,
        })
    }
    pub fn step(&mut self, node: &mut NativeLifecycle, now: Instant) -> Result<()> {
        {
            let mut views = self
                .views
                .lock()
                .map_err(|_| anyhow::anyhow!("NOVONET views poisoned"))?;
            for (peer, view) in views.iter_mut().enumerate() {
                let session = node.peer_session_id(peer);
                let bound = session.is_some() && node.peer_uses_bound_carrier(peer)?;
                let stale = node.peer_last_authenticated(peer).unwrap_or(self.started);
                *view = PeerView {
                    session,
                    bound,
                    fallback: !bound
                        && node.peer_pending_deliveries(peer)? > 0
                        && now.saturating_duration_since(stale)
                            >= Duration::from_millis(self.config.fallback_after_ms),
                };
                if bound && now >= self.next_udp_probe[peer] {
                    node.request_udp_recovery_v1(peer, wall_ms()?, now)?;
                    self.next_udp_probe[peer] =
                        now + Duration::from_millis(self.config.udp_recovery_ms);
                }
            }
        }
        self.poll_network()?;
        for _ in 0..node.peer_count() {
            let Ok(connection) = self.connected.try_recv() else {
                break;
            };
            let result = node.replace_bound_carrier_v1(connection.peer, connection.endpoint, now);
            let accepted = result.is_ok();
            if accepted {
                self.views
                    .lock()
                    .map_err(|_| anyhow::anyhow!("NOVONET views poisoned"))?[connection.peer] =
                    PeerView {
                        session: node.peer_session_id(connection.peer),
                        bound: true,
                        fallback: false,
                    };
                self.next_udp_probe[connection.peer] =
                    now + Duration::from_millis(self.config.udp_recovery_ms);
            }
            let _ = connection.accepted.send(accepted);
            result?;
        }
        Ok(())
    }
    fn poll_network(&mut self) -> Result<()> {
        ensure!(!self.finished, "NOVONET owner already exited");
        self.activated = true;
        let mut finished = false;
        let result = self.runtime.block_on(async {
            // Let the SAME retained reactor run its ready drivers. No new
            // thread, nested runtime, timed batch wait or full handshake wait.
            tokio::task::yield_now().await;
            poll_fn(|cx| match self.work.as_mut().poll(cx) {
                Poll::Ready(result) => {
                    finished = true;
                    Poll::Ready(result)
                }
                Poll::Pending => Poll::Ready(Ok(())),
            })
            .await
        });
        self.finished = finished;
        result
    }
    pub fn observation(&self) -> serde_json::Value {
        let bound = self
            .views
            .lock()
            .map(|views| {
                if self.closed || self.finished {
                    0
                } else {
                    views.iter().filter(|view| view.bound).count()
                }
            })
            .ok();
        serde_json::json!({"default":"native_novorudp","fallback":"iroh","active_bound_peers":bound,
            "connection_failures":self.failures.load(Ordering::Relaxed),"global_auto_discovery":false,"native_multihop_accepted":false})
    }
    pub fn close(&mut self) -> Result<()> {
        if self.closed || self.finished || !self.activated {
            return Ok(());
        }
        self.stop.store(true, Ordering::Release);
        let mut finished = false;
        let result = self.runtime.block_on(async {
            tokio::time::timeout(Duration::from_secs(10), async {
                let result = self.work.as_mut().await;
                finished = true;
                result
            })
            .await
            .context("NOVONET owner cleanup deadline")?
        });
        self.finished = finished;
        self.closed = result.is_ok();
        result
    }
}
impl Drop for Driver {
    fn drop(&mut self) {
        if let Err(error) = self.close() {
            eprintln!("NOVONET owner cleanup failed: {error:#}");
        }
    }
}

struct Candidate {
    key: Vec<u8>,
    route: IrohAddressV1,
    expires: u64,
    sequence: u64,
}
#[derive(Debug)]
struct ServiceSelectionMismatch;
impl std::fmt::Display for ServiceSelectionMismatch {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("native service selection or fixed chain pins rejected")
    }
}
impl std::error::Error for ServiceSelectionMismatch {}
// Distinct existing owners are passed explicitly; no combined authority object.
#[allow(clippy::too_many_arguments)]
async fn run(
    config: Config,
    identity: TransportIdentity,
    keys: Vec<Vec<u8>>,
    pins: (u64, [u8; 32], [u8; 32]),
    views: Arc<Mutex<Vec<PeerView>>>,
    connected: mpsc::SyncSender<Connected>,
    stop: Arc<AtomicBool>,
    failures: Arc<AtomicU64>,
) -> Result<()> {
    let check_stop = stop.clone();
    let owner = IrohEndpointOwnerV1::bind(config.carrier()?, move || {
        ensure!(!check_stop.load(Ordering::Acquire), "NOVONET owner stopped");
        Ok(())
    })
    .await?;
    let check_stop = stop.clone();
    let service_stop = stop.clone();
    let result = owner
        .with_continuous_service_v4(
            move || {
                ensure!(!check_stop.load(Ordering::Acquire), "NOVONET owner stopped");
                Ok(())
            },
            move |scope| {
                Box::pin(async move {
                    serve(
                        scope,
                        &config,
                        &identity,
                        &keys,
                        pins,
                        &views,
                        &connected,
                        &service_stop,
                        &failures,
                    )
                    .await
                })
            },
        )
        .await;
    let close = owner.close().await;
    if stop.load(Ordering::Acquire) {
        close
    } else {
        result.and(close)
    }
}

#[allow(clippy::too_many_arguments)]
async fn serve<'a>(
    scope: &'a IrohScopeV1,
    config: &Config,
    identity: &TransportIdentity,
    keys: &[Vec<u8>],
    pins: (u64, [u8; 32], [u8; 32]),
    views: &Arc<Mutex<Vec<PeerView>>>,
    connected: &mpsc::SyncSender<Connected>,
    stop: &Arc<AtomicBool>,
    failures: &Arc<AtomicU64>,
) -> Result<()> {
    let mut books = (0..keys.len())
        .map(|_| None)
        .collect::<Vec<Option<ServiceLeaseBookV1>>>();
    let mut candidates = (0..keys.len())
        .map(|_| None)
        .collect::<Vec<Option<Candidate>>>();
    let mut jobs = (0..keys.len())
        .map(|_| None)
        .collect::<Vec<Option<CarrierAttempt<'a>>>>();
    let mut pools = (0..keys.len())
        .map(|_| {
            RelayCandidatePoolV1::new(RelayCandidatePoolConfigV1 {
                cooldown_base_ms: config.retry_ms,
                cooldown_max_ms: 60_000,
            })
        })
        .collect::<Vec<_>>();
    let mut quarantined = vec![None; keys.len()];
    let instance = rand::random::<[u8; 32]>();
    let mut sequence = if config.public_record_file.exists() {
        let old: PeerSignedRelayRecordV1 = read_document(&config.public_record_file)?;
        novovm_network::product_directory::verify_relay_record_authenticity_v1(&old, identity)?;
        ensure!(
            old.relay_public_key == identity.public_key(),
            "public locator file belongs to a different identity"
        );
        old.sequence
    } else {
        0
    };
    let mut refresh = Instant::now();
    let mut publish_at = refresh;
    let mut accepting = Box::pin(scope.accept_inbound());
    loop {
        if stop.load(Ordering::Acquire) {
            return Ok(());
        }
        let now = Instant::now();
        if now >= refresh {
            let wall = wall_ms()?;
            if now >= publish_at {
                sequence = sequence
                    .checked_add(1)
                    .context("NOVONET locator sequence exhausted")?;
                let route = IrohServiceEndpointV1 {
                    instance,
                    address: scope.address()?,
                };
                let record = sign_relay_record_v1(
                    identity,
                    RECORD_KIND,
                    vec![route.endpoint()?],
                    wall,
                    wall.checked_add(120_000)
                        .context("locator clock overflow")?,
                    sequence,
                )?;
                publish(&config.public_record_file, &record)?;
                publish_at = now + Duration::from_secs(40);
            }
            for path in &config.peer_records {
                let Ok(record) = read_document::<PeerSignedRelayRecordV1>(path) else {
                    continue;
                };
                if record.record_id != RECORD_KIND {
                    continue;
                }
                let Some(peer) = keys.iter().position(|key| *key == record.relay_public_key) else {
                    continue;
                };
                let Ok(validated) = validate_relay_record_v1(&record, wall, identity) else {
                    continue;
                };
                let Ok(endpoint) = IrohServiceEndpointV1::from_record(&record) else {
                    continue;
                };
                let accepted = if let Some(book) = &mut books[peer] {
                    book.replace_self_owned_records(vec![record.clone()], wall)
                } else {
                    ServiceLeaseBookV1::new_self_owned(
                        SelfOwnedTrustV1 {
                            version: 2,
                            node_pins: BTreeSet::from([keys[peer].clone()]),
                        },
                        vec![record.clone()],
                        wall,
                        identity.clone(),
                    )
                    .map(|book| {
                        books[peer] = Some(book);
                    })
                };
                if accepted.is_ok() {
                    pools[peer].upsert_verified_record(validated)?;
                    if jobs[peer].as_ref().is_some_and(|job| {
                        !job.adopted.load(Ordering::Acquire)
                            && job.endpoint_key != endpoint.address.endpoint_key
                    }) {
                        // A newer signed locator can replace a still-connecting
                        // attempt. It must NOT tear down an already live session.
                        jobs[peer] = None;
                    }
                    candidates[peer] = Some(Candidate {
                        key: keys[peer].clone(),
                        route: endpoint.address,
                        expires: record.expires_at_ms,
                        sequence: record.sequence,
                    });
                }
            }
            refresh = now + Duration::from_secs(1);
        }
        let snapshot = views
            .lock()
            .map_err(|_| anyhow::anyhow!("NOVONET views poisoned"))?
            .clone();
        for peer in 0..keys.len() {
            if jobs[peer].is_none()
                && (snapshot[peer].fallback || snapshot[peer].bound)
                && pools[peer].select(wall_ms()?).is_some()
                && identity.public_key() < keys[peer].as_slice()
            {
                if let Some(candidate) = &candidates[peer] {
                    if quarantined[peer].is_some_and(|sequence| sequence >= candidate.sequence) {
                        continue;
                    }
                    if wall_ms()? >= candidate.expires {
                        continue;
                    }
                    let route = candidate.route.clone();
                    let key = candidate.key.clone();
                    let identity = identity.clone();
                    let connected = connected.clone();
                    let views = views.clone();
                    let adopted = Arc::new(AtomicBool::new(false));
                    let flag = adopted.clone();
                    let endpoint_key = route.endpoint_key;
                    jobs[peer] = Some(CarrierAttempt {
                        endpoint_key,
                        adopted,
                        future: Box::pin(async move {
                            let stream = scope.connect(&route).await?;
                            connection(
                                stream.into(),
                                identity,
                                key,
                                peer,
                                true,
                                pins,
                                views,
                                connected,
                                flag,
                            )
                            .await
                        }),
                    });
                }
            }
        }
        // Futures own their streams/pumps across polls; polling a ready result
        // is never interpreted as business delivery or a vote.
        let poll_wall = wall_ms()?;
        poll_fn(|cx| {
            for peer in 0..jobs.len() {
                let Some(job) = &mut jobs[peer] else {
                    continue;
                };
                if let Poll::Ready(result) = job.future.as_mut().poll(cx) {
                    if let Err(error)=result {
                        failures.fetch_add(1, Ordering::Relaxed);
                        pools[peer].record_failure(&novovm_network::product_overlay::peer_id_from_mldsa65_public_key_v1(&keys[peer]),poll_wall);
                        if error.downcast_ref::<ServiceSelectionMismatch>().is_some()
                            || matches!(error.downcast_ref::<novovm_network::bound_session::BoundSessionError>(),
                                Some(novovm_network::bound_session::BoundSessionError::Authentication(_)
                                    | novovm_network::bound_session::BoundSessionError::Local(_))) {
                            quarantined[peer]=candidates[peer].as_ref().map(|candidate|candidate.sequence);
                        }
                    }
                    jobs[peer] = None;
                }
            }
            match accepting.as_mut().poll(cx) {
                Poll::Ready(Ok(stream)) => {
                    let endpoint = stream.remote_endpoint_key();
                    if let Ok(endpoint) = endpoint {
                        if let Some(peer) = candidates.iter().position(|candidate| {
                            candidate.as_ref().is_some_and(|c| {
                                c.route.endpoint_key == endpoint
                                    && wall_ms().is_ok_and(|now| now < c.expires)
                            })
                        }) {
                            if jobs[peer].is_none() && quarantined[peer].is_none_or(|sequence|
                                candidates[peer].as_ref().is_none_or(|candidate|sequence<candidate.sequence)) {
                                let adopted=Arc::new(AtomicBool::new(false));
                                jobs[peer] = Some(CarrierAttempt {endpoint_key:endpoint,adopted:adopted.clone(),future:Box::pin(connection(
                                    stream.into(),
                                    identity.clone(),
                                    keys[peer].clone(),
                                    peer,
                                    false,
                                    pins,
                                    views.clone(),
                                    connected.clone(),
                                    adopted,
                                ))});
                            }
                        }
                    }
                    accepting = Box::pin(scope.accept_inbound());
                }
                Poll::Ready(Err(_)) => {
                    accepting = Box::pin(scope.accept_inbound());
                }
                Poll::Pending => {}
            }
            Poll::Ready(())
        })
        .await;
        tokio::time::sleep(Duration::from_millis(1)).await;
    }
}

#[allow(clippy::too_many_arguments)]
async fn connection(
    mut stream: CarrierStreamV1<'_>,
    identity: TransportIdentity,
    expected: Vec<u8>,
    peer: usize,
    initiator: bool,
    pins: (u64, [u8; 32], [u8; 32]),
    views: Arc<Mutex<Vec<PeerView>>>,
    connected: mpsc::SyncSender<Connected>,
    adopted: Arc<AtomicBool>,
) -> Result<()> {
    let mut channel = authenticate_structured_bound_session_observed(
        &mut stream,
        &identity,
        expected,
        initiator,
        &mut HandshakeReplayCacheV1::new(8),
        &mut |_, _| Ok(()),
    )
    .await?;
    // A versioned, AEAD-bound service selection under the original Finished
    // channel. No mailbox selector, chain override or JSON business payload.
    let mut payload = SELECTOR.to_vec();
    payload.extend_from_slice(&pins.0.to_be_bytes());
    payload.extend_from_slice(&pins.1);
    payload.extend_from_slice(&pins.2);
    let select = Frame::new(
        Kind::Data,
        channel.session_id(),
        0,
        SELECT_STREAM,
        0,
        0,
        payload,
    );
    if initiator {
        stream
            .write_frame(&encode_iroh_envelope_v1(
                &channel.seal_novorudp_frame(&select)?,
            )?)
            .await?;
    }
    let received = stream.read_frame().await?;
    let envelope = decode_iroh_envelope_v1(&received).context(ServiceSelectionMismatch)?;
    let decoded = channel
        .open_novorudp_frame(&envelope)
        .context(ServiceSelectionMismatch)?;
    if decoded != select {
        return Err(ServiceSelectionMismatch.into());
    }
    if !initiator {
        stream
            .write_frame(&encode_iroh_envelope_v1(
                &channel.seal_novorudp_frame(&select)?,
            )?)
            .await?;
    }
    let (endpoint, mut pump) = NativeCarrierPump::attach_continuous_v7(
        &mut stream,
        channel,
        NativeChannel::carrier_streams(),
    )?;
    let session = endpoint
        .session_id(0)
        .context("Finished carrier session missing")?;
    let (accepted, wait) = oneshot::channel();
    connected
        .try_send(Connected {
            peer,
            endpoint,
            accepted,
        })
        .map_err(|_| anyhow::anyhow!("NOVONET connection handoff full or closed"))?;
    ensure!(wait.await?, "NOVONET connection not adopted");
    adopted.store(true, Ordering::Release);
    loop {
        let view = views
            .lock()
            .map_err(|_| anyhow::anyhow!("NOVONET views poisoned"))?[peer];
        if view.session.is_some_and(|current| current != session)
            || (view.session == Some(session) && !view.bound)
        {
            return Ok(());
        }
        tokio::select! {
            result=pump.progress()=>result?,
            _=tokio::time::sleep(Duration::from_millis(100))=>{},
        }
    }
}
fn publish(path: &Path, record: &PeerSignedRelayRecordV1) -> Result<()> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let stage = path.with_extension("locator.pending");
    std::fs::write(&stage, serde_json::to_vec(record)?)?;
    std::fs::rename(stage, path)?;
    Ok(())
}

#[cfg(test)]
mod tests;
