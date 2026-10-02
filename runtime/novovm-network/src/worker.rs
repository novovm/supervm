//! Bounded opaque-byte network boundary for the replacement runtime.
//!
//! The fair peer turns, drain-before-send rule and session-generation filtering
//! are selectively migrated from the isolated `product_mainline_overlay.rs`.
//! This is not its bootstrap, ingress, journal, or node lifecycle. Admission to
//! this queue (or to the relay) is NOT peer delivery, durable receipt or finality.
//! The caller must retry its immutable protocol messages until its own protocol
//! establishes progress. Queues may expire or drop inbound traffic under load.

use crate::novorudp::{NovoRudpTransportFrameKindV0, NovoRudpTransportFrameV0};
use crate::product_overlay::{
    peer_id_from_ed25519_public_key_v1, E2eSecureChannelV1, HandshakeReplayCacheV1,
    NodeHandshakeInitiatorV1, NodeHandshakeResponderV1, NodeHandshakeResponseV1,
};
use crate::product_relay::{
    OpaqueRelayDeliveryV1, RelayForwardDispositionV1, RelayPeerHandshakeV1,
};
use crate::product_relay_client::{
    product_relay_client_read_is_idle_timeout_v1, ProductRelayClientConfigV1,
    ProductRelayClientEventV1, ProductRelayClientV1,
};
use anyhow::{bail, Context, Result};
use ed25519_dalek::SigningKey;
use std::collections::{BTreeMap, BTreeSet, VecDeque};
use std::sync::{
    atomic::{AtomicBool, Ordering},
    Arc, Mutex, TryLockError,
};
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

pub const NETWORK_WORKER_MAX_PAYLOAD_BYTES: usize = 192 * 1024;
const FRAME_DOMAIN: u64 = u64::from_le_bytes(*b"NVNET001");
const FRAME_OVERHEAD: usize = 96 + 16;
const MAX_PEERS: usize = 1024;
// Scheduling quantum, not a larger queue or a protocol delivery guarantee.
const MAX_PAYLOADS_PER_TURN: usize = 8;

#[derive(Debug, Clone)]
pub struct QueueLimits {
    pub max_messages: usize,
    pub max_bytes: usize,
    pub peer_max_messages: usize,
    pub peer_max_bytes: usize,
}

impl Default for QueueLimits {
    fn default() -> Self {
        Self {
            max_messages: 256,
            max_bytes: 8 * 1024 * 1024,
            peer_max_messages: 64,
            peer_max_bytes: 2 * 1024 * 1024,
        }
    }
}

#[derive(Debug, Clone)]
pub struct WorkerLimits {
    pub outbound: QueueLimits,
    pub inbound: QueueLimits,
    /// A separate, also count-and-byte bounded budget for encrypted messages
    /// arriving before their exact in-progress handshake has completed.
    pub preauth: QueueLimits,
    pub max_payload_bytes: usize,
}

impl Default for WorkerLimits {
    fn default() -> Self {
        Self {
            outbound: QueueLimits::default(),
            inbound: QueueLimits::default(),
            preauth: QueueLimits {
                max_messages: 64,
                max_bytes: 2 * 1024 * 1024,
                peer_max_messages: 16,
                peer_max_bytes: 512 * 1024,
            },
            max_payload_bytes: NETWORK_WORKER_MAX_PAYLOAD_BYTES,
        }
    }
}

#[derive(Debug, Clone)]
pub struct NetworkWorkerConfig {
    pub chain_id: u64,
    pub relay: ProductRelayClientConfigV1,
    pub peers: Vec<String>,
    pub limits: WorkerLimits,
    pub handshake_timeout_ms: u64,
    pub reconnect_delay_ms: u64,
    pub heartbeat_interval_ms: u64,
    pub queue_ttl_ms: u64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Outbound {
    pub peer_id: String,
    pub bytes: Vec<u8>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Inbound {
    pub peer_id: String,
    pub bytes: Vec<u8>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SendRejection {
    UnknownPeer,
    PayloadTooLarge,
    Stopped,
}

#[derive(Debug, PartialEq, Eq)]
pub enum SendAdmission {
    /// Local bounded queue admission only; not a delivery acknowledgement.
    Accepted,
    Backpressure(Outbound),
    Rejected {
        message: Outbound,
        reason: SendRejection,
    },
}

#[derive(Debug, Clone, Default)]
pub struct WorkerStatus {
    pub relay_connected: bool,
    pub active_peers: Vec<String>,
    pub active_sessions: BTreeMap<String, [u8; 16]>,
    pub outbound_messages: usize,
    pub outbound_bytes: usize,
    pub inbound_messages: usize,
    pub inbound_bytes: usize,
    pub outbound_expired: u64,
    pub inbound_dropped: u64,
    pub invalid_frames: u64,
    pub relay_admissions: u64,
    pub relay_reconnects: u64,
    pub last_error: Option<String>,
}

struct Queued<T> {
    item: T,
    size: usize,
    enqueued: Instant,
}

/// All entries, including a send currently waiting for relay admission, remain
/// charged here. No socket/crypto operation is performed while holding its lock.
struct Queues<T> {
    peers: BTreeMap<String, VecDeque<Queued<T>>>,
    count: usize,
    bytes: usize,
    turn: usize,
    limits: QueueLimits,
}

impl<T> Queues<T> {
    fn new(peers: &[String], limits: QueueLimits) -> Self {
        Self {
            peers: peers
                .iter()
                .map(|id| (id.clone(), VecDeque::new()))
                .collect(),
            count: 0,
            bytes: 0,
            turn: 0,
            limits,
        }
    }

    fn push(&mut self, peer: &str, item: T, size: usize, now: Instant) -> Result<(), T> {
        let Some(queue) = self.peers.get_mut(peer) else {
            return Err(item);
        };
        let peer_bytes: usize = queue.iter().map(|item| item.size).sum();
        if self.count >= self.limits.max_messages
            || queue.len() >= self.limits.peer_max_messages
            || size > self.limits.max_bytes.saturating_sub(self.bytes)
            || size > self.limits.peer_max_bytes.saturating_sub(peer_bytes)
        {
            return Err(item);
        }
        queue.push_back(Queued {
            item,
            size,
            enqueued: now,
        });
        self.count += 1;
        self.bytes += size;
        Ok(())
    }

    fn pop(&mut self, peer: &str) -> Option<T> {
        let queued = self.peers.get_mut(peer)?.pop_front()?;
        self.count -= 1;
        self.bytes -= queued.size;
        Some(queued.item)
    }

    fn clear_peer(&mut self, peer: &str) -> u64 {
        let mut removed = 0;
        while self.pop(peer).is_some() {
            removed += 1;
        }
        removed
    }

    fn expire(&mut self, now: Instant, ttl: Duration) -> u64 {
        let mut removed = 0;
        for queue in self.peers.values_mut() {
            while queue
                .front()
                .is_some_and(|item| now.saturating_duration_since(item.enqueued) >= ttl)
            {
                let entry = queue.pop_front().expect("checked queue front");
                self.count -= 1;
                self.bytes -= entry.size;
                removed += 1;
            }
        }
        removed
    }

    fn next_peer(&mut self, mut ready: impl FnMut(&str) -> bool) -> Option<String> {
        let count = self.peers.len();
        if count == 0 {
            return None;
        }
        for offset in 0..count {
            let index = (self.turn + offset) % count;
            let (peer, queue) = self.peers.iter().nth(index)?;
            if !queue.is_empty() && ready(peer) {
                let id = peer.clone();
                self.turn = (index + 1) % count;
                return Some(id);
            }
        }
        None
    }
}

struct Shared {
    outbound: Queues<Vec<u8>>,
    inbound: Queues<Vec<u8>>,
    status: WorkerStatus,
}

pub struct NetworkWorker {
    shared: Arc<Mutex<Shared>>,
    stop: Arc<AtomicBool>,
    worker: Option<JoinHandle<()>>,
    max_payload: usize,
    ttl: Duration,
}

impl NetworkWorker {
    /// Validates locally and spawns; never connects or waits for a relay here.
    /// Identity is moved to the dedicated worker, never attached to messages.
    pub fn start(config: NetworkWorkerConfig, identity: SigningKey) -> Result<Self> {
        validate_config(&config, &identity)?;
        let shared = Arc::new(Mutex::new(Shared {
            outbound: Queues::new(&config.peers, config.limits.outbound.clone()),
            inbound: Queues::new(&config.peers, config.limits.inbound.clone()),
            status: WorkerStatus::default(),
        }));
        let stop = Arc::new(AtomicBool::new(false));
        let max_payload = config.limits.max_payload_bytes;
        let ttl = Duration::from_millis(config.queue_ttl_ms);
        let worker_shared = Arc::clone(&shared);
        let worker_stop = Arc::clone(&stop);
        let worker = thread::Builder::new()
            .name("novovm-network".into())
            .spawn(move || {
                if let Err(error) = run_worker(&config, &identity, &worker_shared, &worker_stop) {
                    record_error(&worker_shared, &error);
                }
                worker_stop.store(true, Ordering::Release);
                if let Ok(mut shared) = worker_shared.lock() {
                    shared.status.relay_connected = false;
                    shared.status.active_peers.clear();
                    shared.status.active_sessions.clear();
                }
            })
            .context("spawn network worker")?;
        Ok(Self {
            shared,
            stop,
            worker: Some(worker),
            max_payload,
            ttl,
        })
    }

    pub fn try_send(&self, peer_id: String, bytes: Vec<u8>) -> Result<SendAdmission> {
        let message = Outbound { peer_id, bytes };
        let reason = if self.stop.load(Ordering::Acquire) {
            Some(SendRejection::Stopped)
        } else if message.bytes.len() > self.max_payload {
            Some(SendRejection::PayloadTooLarge)
        } else {
            None
        };
        if let Some(reason) = reason {
            return Ok(SendAdmission::Rejected { message, reason });
        }
        let mut shared = match self.shared.try_lock() {
            Ok(shared) => shared,
            Err(TryLockError::WouldBlock) => return Ok(SendAdmission::Backpressure(message)),
            Err(TryLockError::Poisoned(_)) => bail!("network queue poisoned"),
        };
        if !shared.outbound.peers.contains_key(&message.peer_id) {
            return Ok(SendAdmission::Rejected {
                message,
                reason: SendRejection::UnknownPeer,
            });
        }
        // Only the network owner expires/removes outbound fronts: a front may
        // currently be in flight without holding the queue lock.
        let size = message.bytes.len();
        if let Err(bytes) =
            shared
                .outbound
                .push(&message.peer_id, message.bytes, size, Instant::now())
        {
            return Ok(SendAdmission::Backpressure(Outbound {
                peer_id: message.peer_id,
                bytes,
            }));
        }
        drop(shared);
        if let Some(worker) = &self.worker {
            worker.thread().unpark();
        }
        Ok(SendAdmission::Accepted)
    }

    pub fn try_recv(&self) -> Result<Option<Inbound>> {
        let mut shared = match self.shared.try_lock() {
            Ok(shared) => shared,
            Err(TryLockError::WouldBlock) => return Ok(None),
            Err(TryLockError::Poisoned(_)) => bail!("network queue poisoned"),
        };
        let expired = shared.inbound.expire(Instant::now(), self.ttl);
        shared.status.inbound_dropped = shared.status.inbound_dropped.saturating_add(expired);
        let Some(peer_id) = shared.inbound.next_peer(|_| true) else {
            return Ok(None);
        };
        let bytes = shared
            .inbound
            .pop(&peer_id)
            .expect("selected nonempty queue");
        Ok(Some(Inbound { peer_id, bytes }))
    }

    pub fn status(&self) -> Result<WorkerStatus> {
        let shared = self
            .shared
            .try_lock()
            .map_err(|_| anyhow::anyhow!("network status busy or poisoned"))?;
        let mut status = shared.status.clone();
        status.outbound_messages = shared.outbound.count;
        status.outbound_bytes = shared.outbound.bytes;
        status.inbound_messages = shared.inbound.count;
        status.inbound_bytes = shared.inbound.bytes;
        Ok(status)
    }

    /// Signals and joins the socket owner. Unlike try_send/try_recv, shutdown
    /// waits for the client's bounded current I/O operation to finish.
    pub fn shutdown(&mut self) -> Result<()> {
        self.stop.store(true, Ordering::Release);
        if let Some(worker) = self.worker.take() {
            worker.thread().unpark();
            worker
                .join()
                .map_err(|_| anyhow::anyhow!("network worker panicked"))?;
        }
        Ok(())
    }
}

impl Drop for NetworkWorker {
    fn drop(&mut self) {
        let _ = self.shutdown();
    }
}

fn validate_config(config: &NetworkWorkerConfig, identity: &SigningKey) -> Result<()> {
    if config.chain_id == 0 {
        bail!("network chain id must be nonzero");
    }
    let local = peer_id_from_ed25519_public_key_v1(&identity.verifying_key().to_bytes());
    let unique: BTreeSet<_> = config.peers.iter().collect();
    if config.peers.is_empty()
        || config.peers.len() > MAX_PEERS
        || unique.len() != config.peers.len()
        || config
            .peers
            .iter()
            .any(|id| id == &local || !valid_peer_id(id))
    {
        bail!("invalid configured network peers");
    }
    if config.limits.max_payload_bytes == 0
        || config.limits.max_payload_bytes > NETWORK_WORKER_MAX_PAYLOAD_BYTES
    {
        bail!("network payload limit must be 1..=192 KiB");
    }
    if !valid_peer_id(&config.relay.expected_relay_peer_id)
        || !config.relay.endpoint.starts_with("wss://")
        || config.relay.endpoint.len() > 4096
    {
        bail!("relay requires an explicit WSS endpoint and identity pin");
    }
    for limits in [
        &config.limits.inbound,
        &config.limits.outbound,
        &config.limits.preauth,
    ] {
        if limits.max_messages == 0
            || limits.max_messages > 65_536
            || limits.peer_max_messages == 0
            || limits.peer_max_messages > limits.max_messages
            || limits.max_bytes == 0
            || limits.max_bytes > 256 * 1024 * 1024
            || limits.peer_max_bytes == 0
            || limits.peer_max_bytes > limits.max_bytes
        {
            bail!("invalid bounded network queue limits");
        }
    }
    if !(1..=60_000).contains(&config.relay.connect_timeout_ms)
        || !(1..=1000).contains(&config.relay.read_timeout_ms)
        || !(100..=60_000).contains(&config.handshake_timeout_ms)
        || !(1..=60_000).contains(&config.reconnect_delay_ms)
        || !(100..=5000).contains(&config.heartbeat_interval_ms)
        || !(1..=300_000).contains(&config.queue_ttl_ms)
    {
        bail!("invalid network timing limits");
    }
    Ok(())
}

fn valid_peer_id(id: &str) -> bool {
    id.strip_prefix("novovm-ed25519:").is_some_and(|key| {
        key.len() == 64
            && key
                .bytes()
                .all(|b| b.is_ascii_hexdigit() && !b.is_ascii_uppercase())
    })
}

enum Phase {
    Idle,
    Handshaking {
        initiator: NodeHandshakeInitiatorV1,
        deadline: Instant,
    },
    Responding {
        response: Box<NodeHandshakeResponseV1>,
        channel: E2eSecureChannelV1,
        deadline: Instant,
    },
    Active(E2eSecureChannelV1),
    Cooldown(Instant),
}

struct Peer {
    phase: Phase,
    replay: HandshakeReplayCacheV1,
    frame_sequence: u64,
}

impl Peer {
    fn new() -> Self {
        Self {
            phase: Phase::Idle,
            replay: HandshakeReplayCacheV1::new(256),
            frame_sequence: 0,
        }
    }
    fn expected_session(&self, session: [u8; 16]) -> bool {
        match &self.phase {
            Phase::Handshaking { initiator, .. } => initiator.offer().session_id == session,
            Phase::Responding { channel, .. } | Phase::Active(channel) => {
                channel.session_id() == session
            }
            _ => false,
        }
    }

    fn local_peer_id(&self) -> Option<&str> {
        match &self.phase {
            Phase::Handshaking { initiator, .. } => Some(&initiator.offer().initiator_peer_id),
            Phase::Responding { channel, .. } | Phase::Active(channel) => {
                Some(channel.local_peer_id())
            }
            _ => None,
        }
    }
}

fn record_error(shared: &Mutex<Shared>, error: &anyhow::Error) {
    if let Ok(mut shared) = shared.lock() {
        shared.status.last_error = Some(format!("{error:#}").chars().take(512).collect());
    }
}

fn publish_active(shared: &Mutex<Shared>, peers: &BTreeMap<String, Peer>) -> Result<()> {
    let mut shared = shared
        .lock()
        .map_err(|_| anyhow::anyhow!("network queue poisoned"))?;
    shared.status.active_sessions = peers
        .iter()
        .filter_map(|(id, peer)| match &peer.phase {
            Phase::Active(channel) => Some((id.clone(), channel.session_id())),
            _ => None,
        })
        .collect();
    shared.status.active_peers = shared.status.active_sessions.keys().cloned().collect();
    Ok(())
}

fn isolate(peer: &mut Peer, config: &NetworkWorkerConfig) {
    peer.phase = Phase::Cooldown(Instant::now() + Duration::from_millis(config.reconnect_delay_ms));
    peer.frame_sequence = 0;
}

fn admitted(disposition: RelayForwardDispositionV1) -> bool {
    matches!(
        disposition,
        RelayForwardDispositionV1::Forwarded
            | RelayForwardDispositionV1::QueuedTargetOffline
            | RelayForwardDispositionV1::QueuedBackpressure
    )
}

fn now_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis()
        .min(u128::from(u64::MAX)) as u64
}

fn run_worker(
    config: &NetworkWorkerConfig,
    identity: &SigningKey,
    shared: &Mutex<Shared>,
    stop: &AtomicBool,
) -> Result<()> {
    let mut peers: BTreeMap<_, _> = config
        .peers
        .iter()
        .map(|id| (id.clone(), Peer::new()))
        .collect();
    while !stop.load(Ordering::Acquire) {
        let connection = ProductRelayClientV1::connect(identity, &config.relay);
        match connection {
            Ok(mut relay) => {
                {
                    let mut shared = shared
                        .lock()
                        .map_err(|_| anyhow::anyhow!("network queue poisoned"))?;
                    shared.status.relay_connected = true;
                    shared.status.relay_reconnects =
                        shared.status.relay_reconnects.saturating_add(1);
                }
                if let Err(error) =
                    run_session(&mut relay, config, identity, &mut peers, shared, stop)
                {
                    record_error(shared, &error);
                }
                // Dropping a failed/uncertain TLS stream must not retry a write.
                drop(relay);
            }
            Err(error) => record_error(shared, &error),
        }
        for peer in peers.values_mut() {
            peer.phase = Phase::Idle;
            peer.frame_sequence = 0;
        }
        {
            let mut shared = shared
                .lock()
                .map_err(|_| anyhow::anyhow!("network queue poisoned"))?;
            shared.status.relay_connected = false;
            shared.status.active_peers.clear();
            shared.status.active_sessions.clear();
            let expired = shared
                .outbound
                .expire(Instant::now(), Duration::from_millis(config.queue_ttl_ms));
            shared.status.outbound_expired = shared.status.outbound_expired.saturating_add(expired);
        }
        if !stop.load(Ordering::Acquire) {
            thread::park_timeout(Duration::from_millis(config.reconnect_delay_ms));
        }
    }
    Ok(())
}

fn run_session(
    relay: &mut ProductRelayClientV1,
    config: &NetworkWorkerConfig,
    identity: &SigningKey,
    peers: &mut BTreeMap<String, Peer>,
    shared: &Mutex<Shared>,
    stop: &AtomicBool,
) -> Result<()> {
    let local = peer_id_from_ed25519_public_key_v1(&identity.verifying_key().to_bytes());
    let mut preauth = Queues::new(&config.peers, config.limits.preauth.clone());
    let mut handshake_turn = 0;
    let mut heartbeat_at = Instant::now();
    let ttl = Duration::from_millis(config.queue_ttl_ms);
    while !stop.load(Ordering::Acquire) {
        let now = Instant::now();
        {
            let mut shared = shared
                .lock()
                .map_err(|_| anyhow::anyhow!("network queue poisoned"))?;
            let expired = shared.outbound.expire(now, ttl);
            shared.status.outbound_expired = shared.status.outbound_expired.saturating_add(expired);
            let expired = shared.inbound.expire(now, ttl)
                + preauth.expire(now, Duration::from_millis(config.handshake_timeout_ms));
            shared.status.inbound_dropped = shared.status.inbound_dropped.saturating_add(expired);
        }
        for (id, peer) in peers.iter_mut() {
            if matches!(&peer.phase, Phase::Handshaking { deadline, .. } | Phase::Responding { deadline, .. } if *deadline <= now)
            {
                isolate(peer, config);
                let dropped = preauth.clear_peer(id);
                let mut shared = shared
                    .lock()
                    .map_err(|_| anyhow::anyhow!("network queue poisoned"))?;
                shared.status.inbound_dropped =
                    shared.status.inbound_dropped.saturating_add(dropped);
            }
        }
        publish_active(shared, peers)?;
        // Each active send can buffer decoded events while waiting for its
        // outcome. Re-check immediately before EVERY active-send stage.
        if !relay.has_buffered_events()
            && heartbeat_at.elapsed() >= Duration::from_millis(config.heartbeat_interval_ms)
        {
            relay.heartbeat()?;
            heartbeat_at = Instant::now();
        }
        if !relay.has_buffered_events() {
            send_one_handshake(relay, config, identity, peers, &mut handshake_turn)?;
        }
        // The outcome wait already reads the WSS stream and buffers incoming
        // events. Continue queued sends only while there is no such event;
        // do not pay an idle receive timeout after every queued payload.
        // Both count and the existing read-idle budget bound this turn. An
        // in-progress outcome keeps its original absolute deadline, and no
        // additional send starts after a slow outcome exhausts this budget.
        let send_started = Instant::now();
        for _ in 0..MAX_PAYLOADS_PER_TURN {
            if stop.load(Ordering::Acquire)
                || relay.has_buffered_events()
                || send_started.elapsed() >= Duration::from_millis(config.relay.read_timeout_ms)
                || !send_one_payload(relay, config, peers, shared)?
            {
                break;
            }
        }
        if let Some(id) = preauth.next_peer(|id| {
            peers
                .get(id)
                .is_some_and(|peer| matches!(peer.phase, Phase::Active(_)))
        }) {
            let delivery = preauth.pop(&id).expect("selected preauth queue");
            receive_delivery(delivery, config, peers, shared, &mut preauth)?;
            continue;
        }
        match relay.recv_event() {
            Ok(ProductRelayClientEventV1::Delivery(delivery)) => {
                receive_delivery(delivery, config, peers, shared, &mut preauth)?
            }
            Ok(ProductRelayClientEventV1::PeerHandshake(delivery)) => {
                if delivery.target_peer_id != local {
                    continue;
                }
                let Some(peer) = peers.get_mut(&delivery.source_peer_id) else {
                    continue;
                };
                match delivery.handshake {
                    RelayPeerHandshakeV1::Offer(offer) => {
                        if offer.initiator_peer_id != delivery.source_peer_id
                            || offer.responder_peer_id != local
                        {
                            continue;
                        }
                        if matches!(peer.phase, Phase::Handshaking { .. })
                            && local < delivery.source_peer_id
                        {
                            continue;
                        }
                        // Verify before replacing a live peer generation. Bad or
                        // replayed offers cannot reset an authenticated channel.
                        match NodeHandshakeResponderV1::respond(
                            &offer,
                            identity,
                            now_ms(),
                            config.handshake_timeout_ms,
                            &mut peer.replay,
                        ) {
                            Ok(responder) => {
                                peer.phase = Phase::Responding {
                                    response: Box::new(responder.response().clone()),
                                    channel: responder.into_channel(),
                                    deadline: Instant::now()
                                        + Duration::from_millis(config.handshake_timeout_ms),
                                };
                                peer.frame_sequence = 0;
                                let dropped = preauth.clear_peer(&delivery.source_peer_id);
                                let mut shared = shared
                                    .lock()
                                    .map_err(|_| anyhow::anyhow!("network queue poisoned"))?;
                                shared.status.inbound_dropped =
                                    shared.status.inbound_dropped.saturating_add(dropped);
                            }
                            Err(error) => record_error(shared, &error.into()),
                        }
                    }
                    RelayPeerHandshakeV1::Response(response) => {
                        if !peer.expected_session(response.session_id)
                            || response.responder_peer_id != delivery.source_peer_id
                        {
                            continue;
                        }
                        if !matches!(peer.phase, Phase::Handshaking { .. }) {
                            continue;
                        }
                        let Phase::Handshaking { initiator, .. } =
                            std::mem::replace(&mut peer.phase, Phase::Idle)
                        else {
                            unreachable!()
                        };
                        match initiator.complete(&response, now_ms(), &mut peer.replay) {
                            Ok(channel) => {
                                peer.phase = Phase::Active(channel);
                                peer.frame_sequence = 0;
                            }
                            Err(error) => {
                                record_error(shared, &error.into());
                                isolate(peer, config);
                            }
                        }
                    }
                }
                publish_active(shared, peers)?;
            }
            Ok(ProductRelayClientEventV1::HeartbeatAck) => {}
            Ok(ProductRelayClientEventV1::Closed) => bail!("relay closed network session"),
            Err(error) if product_relay_client_read_is_idle_timeout_v1(&error) => {}
            Err(error) => return Err(error),
        }
    }
    Ok(())
}

fn send_one_handshake(
    relay: &mut ProductRelayClientV1,
    config: &NetworkWorkerConfig,
    identity: &SigningKey,
    peers: &mut BTreeMap<String, Peer>,
    turn: &mut usize,
) -> Result<()> {
    for offset in 0..config.peers.len() {
        let index = (*turn + offset) % config.peers.len();
        let id = &config.peers[index];
        let peer = peers.get_mut(id).expect("configured peer");
        let due = matches!(peer.phase, Phase::Idle | Phase::Responding { .. })
            || matches!(peer.phase, Phase::Cooldown(at) if at <= Instant::now());
        if !due {
            continue;
        }
        *turn = (index + 1) % config.peers.len();
        let phase = std::mem::replace(&mut peer.phase, Phase::Idle);
        let (message, next) = match phase {
            Phase::Responding {
                response, channel, ..
            } => (
                RelayPeerHandshakeV1::Response(*response),
                Phase::Active(channel),
            ),
            _ => {
                let initiator = NodeHandshakeInitiatorV1::start(
                    identity,
                    id,
                    now_ms(),
                    config.handshake_timeout_ms,
                )?;
                let message = RelayPeerHandshakeV1::Offer(initiator.offer().clone());
                (
                    message,
                    Phase::Handshaking {
                        initiator,
                        deadline: Instant::now()
                            + Duration::from_millis(config.handshake_timeout_ms),
                    },
                )
            }
        };
        let outcome = relay.send_peer_handshake_with_outcome_v1(id.clone(), message)?;
        if admitted(outcome.disposition) {
            peer.phase = next;
        } else {
            isolate(peer, config);
        }
        break;
    }
    Ok(())
}

fn send_one_payload(
    relay: &mut ProductRelayClientV1,
    config: &NetworkWorkerConfig,
    peers: &mut BTreeMap<String, Peer>,
    shared: &Mutex<Shared>,
) -> Result<bool> {
    let selected = {
        let mut shared = shared
            .lock()
            .map_err(|_| anyhow::anyhow!("network queue poisoned"))?;
        // Every send keeps the original TTL check, including later sends in a
        // burst. A previous outcome wait may have consumed substantial time.
        let expired = shared
            .outbound
            .expire(Instant::now(), Duration::from_millis(config.queue_ttl_ms));
        shared.status.outbound_expired = shared.status.outbound_expired.saturating_add(expired);
        shared
            .outbound
            .next_peer(|id| {
                peers
                    .get(id)
                    .is_some_and(|peer| matches!(peer.phase, Phase::Active(_)))
            })
            .map(|id| {
                let bytes = shared.outbound.peers[&id]
                    .front()
                    .expect("selected outgoing queue")
                    .item
                    .clone();
                (id, bytes)
            })
    };
    let Some((id, bytes)) = selected else {
        return Ok(false);
    };
    let peer = peers.get_mut(&id).expect("configured outgoing peer");
    let Phase::Active(channel) = &mut peer.phase else {
        return Ok(false);
    };
    let Some(next_sequence) = peer.frame_sequence.checked_add(1) else {
        isolate(peer, config);
        return Ok(true);
    };
    let frame = NovoRudpTransportFrameV0::new(
        NovoRudpTransportFrameKindV0::Data,
        channel.session_id(),
        config.chain_id,
        FRAME_DOMAIN,
        peer.frame_sequence,
        0,
        bytes,
    );
    let envelope = match channel.seal_novorudp_frame(&frame) {
        Ok(envelope) => envelope,
        Err(error) => {
            record_error(shared, &error.into());
            isolate(peer, config);
            return Ok(true);
        }
    };
    let outcome = relay.send_envelope_with_outcome_v1(envelope)?;
    if admitted(outcome.disposition) {
        peer.frame_sequence = next_sequence;
        let mut shared = shared
            .lock()
            .map_err(|_| anyhow::anyhow!("network queue poisoned"))?;
        shared
            .outbound
            .pop(&id)
            .context("outgoing in-flight front disappeared")?;
        shared.status.relay_admissions = shared.status.relay_admissions.saturating_add(1);
    } else {
        // Retain the original plaintext and isolate only this peer. Other peer
        // channels and their replay windows remain live.
        isolate(peer, config);
        record_error(
            shared,
            &anyhow::anyhow!("relay admission rejected: {:?}", outcome.disposition),
        );
    }
    Ok(true)
}

fn receive_delivery(
    delivery: OpaqueRelayDeliveryV1,
    config: &NetworkWorkerConfig,
    peers: &mut BTreeMap<String, Peer>,
    shared: &Mutex<Shared>,
    preauth: &mut Queues<OpaqueRelayDeliveryV1>,
) -> Result<()> {
    let id = &delivery.source_peer_id;
    let Some(peer) = peers.get_mut(id) else {
        return Ok(());
    };
    // Check the complete route even before buffering. Otherwise relay-controlled
    // strings outside the configured identity set would evade the byte budget.
    let route_matches = peer.local_peer_id().is_some_and(|local| {
        delivery.target_peer_id == local
            && delivery.envelope.recipient_peer_id == local
            && delivery.envelope.sender_peer_id == *id
    });
    let size = delivery.envelope.ciphertext.len();
    let awaiting_session = peer.expected_session(delivery.envelope.session_id)
        && !matches!(peer.phase, Phase::Active(_))
        && route_matches;
    let opened = match &mut peer.phase {
        Phase::Active(channel)
            if channel.session_id() == delivery.envelope.session_id
                && route_matches
                && size <= config.limits.max_payload_bytes + FRAME_OVERHEAD =>
        {
            channel
                .open_novorudp_frame(&delivery.envelope)
                .ok()
                .filter(|frame| {
                    frame.kind == NovoRudpTransportFrameKindV0::Data
                        && frame.stream_id == config.chain_id
                        && frame.object_id == FRAME_DOMAIN
                        && frame.session_id == channel.session_id()
                        && frame.ack_epoch == 0
                        && frame.payload.len() <= config.limits.max_payload_bytes
                })
        }
        _ if awaiting_session && size <= config.limits.max_payload_bytes + FRAME_OVERHEAD => {
            if preauth
                .push(&id.clone(), delivery, size, Instant::now())
                .is_err()
            {
                let mut shared = shared
                    .lock()
                    .map_err(|_| anyhow::anyhow!("network queue poisoned"))?;
                shared.status.inbound_dropped = shared.status.inbound_dropped.saturating_add(1);
            }
            return Ok(());
        }
        _ => None,
    };
    let mut shared = shared
        .lock()
        .map_err(|_| anyhow::anyhow!("network queue poisoned"))?;
    match opened {
        Some(frame) => {
            let size = frame.payload.len();
            if shared
                .inbound
                .push(id, frame.payload, size, Instant::now())
                .is_err()
            {
                shared.status.inbound_dropped = shared.status.inbound_dropped.saturating_add(1);
            }
        }
        None => shared.status.invalid_frames = shared.status.invalid_frames.saturating_add(1),
    }
    Ok(())
}

#[cfg(test)]
mod tests;
