//! Bounded opaque-byte network boundary used by the original product node.
//!
//! The fair peer turns, drain-before-send rule and session-generation filtering
//! are preserved from the fixed a7db795 duplex implementation; its historical
//! source was `product_mainline_overlay.rs`. See this module's parent for the
//! full provenance and explicit carrier compatibility boundary.
//! This is not its bootstrap, ingress, journal, or node lifecycle. Admission to
//! this queue (or to the relay) is NOT peer delivery, durable receipt or finality.
//! The caller must retry its immutable protocol messages until its own protocol
//! establishes progress. Queues may expire or drop inbound traffic under load.

use crate::duplex::novorudp::{NovoRudpTransportFrameKindV0, NovoRudpTransportFrameV0};
use crate::duplex::product_overlay::{
    peer_id_from_ed25519_public_key_v1, E2eSecureChannelV1, HandshakeReplayCacheV1,
    NodeHandshakeInitiatorV1, NodeHandshakeResponderV1, NodeHandshakeResponseV1,
};
use crate::duplex::product_relay::{
    OpaqueRelayDeliveryV1, RelayForwardDispositionV1, RelayPeerHandshakeV1,
};
use crate::duplex::product_relay_client::{
    ProductRelayClientConfigV1, ProductRelayClientEventV1, ProductRelayClientV1,
};
use anyhow::{bail, Context, Result};
use ed25519_dalek::SigningKey;
use std::collections::{BTreeMap, BTreeSet, VecDeque};
use std::sync::{
    atomic::{AtomicBool, Ordering},
    Arc, Mutex, TryLockError,
};
use std::task::Waker;
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

pub const NETWORK_WORKER_MAX_PAYLOAD_BYTES: usize = 192 * 1024;
const FRAME_DOMAIN: u64 = u64::from_le_bytes(*b"NVNET001");
const FRAME_OVERHEAD: usize = 96 + 16;
const MAX_PEERS: usize = 1024;

mod outbound;
mod session;
use outbound::OutboundQueues;
use session::run_session;

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
    /// Actual data submissions awaiting their precisely correlated relay result.
    /// This is not application delivery or consensus finality.
    pub pending_forwards: usize,
    pub peak_pending_forwards: usize,
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
    outbound: OutboundQueues,
    inbound: Queues<Vec<u8>>,
    status: WorkerStatus,
    relay_read_waker: Option<Waker>,
    #[cfg(test)]
    read_wait_probe: Option<Arc<crate::duplex::product_relay_io::ReadWaitProbe>>,
}

impl Shared {
    /// Publish under the SAME lock as queue admission. The returned wake must
    /// be issued after releasing the lock; pre-existing work cannot be missed
    /// just because it arrived before this authenticated connection existed.
    fn install_relay_read_waker(&mut self, waker: Waker) -> Option<Waker> {
        self.relay_read_waker = Some(waker);
        (self.outbound.count != 0).then(|| {
            self.relay_read_waker
                .as_ref()
                .expect("installed waker")
                .clone()
        })
    }

    fn clear_relay_read_waker(&mut self) {
        self.relay_read_waker = None;
        #[cfg(test)]
        {
            self.read_wait_probe = None;
        }
    }

    fn runnable_outbound_waker(
        &self,
        peers: &BTreeMap<String, Peer>,
        now: Instant,
        ttl: Duration,
        can_submit: bool,
    ) -> Option<Waker> {
        // A full forward window or an all-in-flight queue cannot be advanced
        // by self-waking: only a real ACK/socket event can release that budget.
        (can_submit
            && self
                .outbound
                .has_ready(|id| peers.get(id).is_some_and(Peer::can_send), now, ttl))
        .then(|| self.relay_read_waker.clone())
        .flatten()
    }
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
            outbound: OutboundQueues::new(&config.peers, config.limits.outbound.clone()),
            inbound: Queues::new(&config.peers, config.limits.inbound.clone()),
            status: WorkerStatus::default(),
            relay_read_waker: None,
            #[cfg(test)]
            read_wait_probe: None,
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
                    shared.clear_relay_read_waker();
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
        if !shared.outbound.contains_peer(&message.peer_id) {
            return Ok(SendAdmission::Rejected {
                message,
                reason: SendRejection::UnknownPeer,
            });
        }
        // Ready and in-flight originals share this same count/byte budget.
        // Only the owner can settle exact entries or expire unsent originals.
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
        let read_waker = shared.relay_read_waker.clone();
        drop(shared);
        if let Some(waker) = read_waker {
            waker.wake();
        }
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
    /// wakes the owner's I/O wait. Dropping a partial write closes that TLS
    /// session; its ciphertext is never retried on another connection.
    pub fn shutdown(&mut self) -> Result<()> {
        self.stop.store(true, Ordering::Release);
        let read_waker = self
            .shared
            .lock()
            .ok()
            .and_then(|shared| shared.relay_read_waker.clone());
        if let Some(waker) = read_waker {
            waker.wake();
        }
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
    /// A rejected frame freezes new sends until all already submitted frames
    /// for this generation have been settled individually.
    retiring: bool,
}

impl Peer {
    fn new() -> Self {
        Self {
            phase: Phase::Idle,
            replay: HandshakeReplayCacheV1::new(256),
            frame_sequence: 0,
            retiring: false,
        }
    }
    fn can_send(&self) -> bool {
        matches!(self.phase, Phase::Active(_)) && !self.retiring
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
    peer.retiring = false;
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
        let connection = ProductRelayClientV1::connect(identity, &config.relay)
            .and_then(ProductRelayClientV1::into_pipeline);
        match connection {
            Ok(mut relay) => {
                let initial_wake = {
                    let mut shared = shared
                        .lock()
                        .map_err(|_| anyhow::anyhow!("network queue poisoned"))?;
                    shared.status.relay_connected = true;
                    #[cfg(test)]
                    {
                        shared.read_wait_probe = Some(relay.read_wait_probe());
                    }
                    shared.status.relay_reconnects =
                        shared.status.relay_reconnects.saturating_add(1);
                    shared.install_relay_read_waker(relay.read_waker())
                };
                if let Some(waker) = initial_wake {
                    waker.wake();
                }
                let result = run_session(&mut relay, config, identity, &mut peers, shared, stop);
                // One owner switches generations. Clear before dropping the
                // stream; a clone taken by an earlier admission can only wake
                // the old Poll, never restore it as the current registration.
                shared
                    .lock()
                    .map_err(|_| anyhow::anyhow!("network queue poisoned"))?
                    .clear_relay_read_waker();
                if let Err(error) = result {
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
            peer.retiring = false;
        }
        {
            let mut shared = shared
                .lock()
                .map_err(|_| anyhow::anyhow!("network queue poisoned"))?;
            shared.status.relay_connected = false;
            shared.status.active_peers.clear();
            shared.status.active_sessions.clear();
            // Every return (including unknown partial writes) drops the old
            // TLS owner first. Retain original admission time and re-encrypt
            // only after a fresh authenticated E2E session is established.
            shared.outbound.release_all();
            shared.status.pending_forwards = 0;
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
