use crate::product_overlay::{
    AuthenticatedPeerV1, NodeHandshakeOfferV1, NodeHandshakeResponseV1, SecureNovoRudpEnvelopeV1,
};
use serde::{Deserialize, Serialize};
use std::{
    collections::{BTreeMap, VecDeque},
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc,
    },
};
use thiserror::Error;
use tokio::sync::{mpsc, RwLock};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProductRelayRuntimeConfigV1 {
    pub session_queue_capacity: usize,
    pub offline_queue_per_peer: usize,
    pub offline_queue_total: usize,
    pub session_ttl_ms: u64,
    pub rate_limit_frames: u64,
    pub rate_limit_window_ms: u64,
}

impl Default for ProductRelayRuntimeConfigV1 {
    fn default() -> Self {
        Self {
            session_queue_capacity: 256,
            offline_queue_per_peer: 512,
            offline_queue_total: 16_384,
            session_ttl_ms: 45_000,
            rate_limit_frames: 4_096,
            rate_limit_window_ms: 1_000,
        }
    }
}

#[derive(Debug, Error, Clone, PartialEq, Eq)]
pub enum ProductRelayRuntimeErrorV1 {
    #[error("relay runtime is shutting down")]
    ShuttingDown,
    #[error("relay runtime configuration is invalid: {0}")]
    InvalidConfiguration(&'static str),
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct OpaqueRelayDeliveryV1 {
    pub source_peer_id: String,
    pub target_peer_id: String,
    pub received_at_ms: u64,
    pub envelope: SecureNovoRudpEnvelopeV1,
}

/// Signed peer-handshake signalling forwarded by the relay before an E2E channel exists.
/// It contains no NOVORUDP payload. Both offer and response are independently signed by their
/// source node and bind the intended peer id, session id, challenge, and expiry.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", content = "body", rename_all = "snake_case")]
pub enum RelayPeerHandshakeV1 {
    Offer(NodeHandshakeOfferV1),
    Response(NodeHandshakeResponseV1),
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RelayPeerHandshakeDeliveryV1 {
    pub source_peer_id: String,
    pub target_peer_id: String,
    pub received_at_ms: u64,
    pub handshake: RelayPeerHandshakeV1,
}

/// Transport-neutral binary message contract for an authenticated product relay session.
/// WSS is one carrier; the messages contain neither plaintext NOVORUDP nor execution semantics.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "kind", content = "body", rename_all = "snake_case")]
pub enum ProductRelayWireMessageV1 {
    HandshakeOffer(NodeHandshakeOfferV1),
    HandshakeResponse(NodeHandshakeResponseV1),
    Data(SecureNovoRudpEnvelopeV1),
    Delivery(OpaqueRelayDeliveryV1),
    PeerHandshake {
        target_peer_id: String,
        handshake: RelayPeerHandshakeV1,
    },
    PeerHandshakeDelivery(RelayPeerHandshakeDeliveryV1),
    Heartbeat,
    HeartbeatAck,
    Close,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RelayForwardDispositionV1 {
    Forwarded,
    QueuedTargetOffline,
    QueuedBackpressure,
    RejectedSourceSessionMissing,
    RejectedStaleSourceSession,
    RejectedSourceSessionExpired,
    RejectedRouteMismatch,
    RejectedRateLimited,
    RejectedQueueFull,
    RejectedShuttingDown,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RelayForwardOutcomeV1 {
    pub disposition: RelayForwardDispositionV1,
    pub source_peer_id: String,
    pub target_peer_id: String,
    pub forwarded: bool,
    pub queued: bool,
    pub payload_treated_opaque: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RelaySessionRegistrationV1 {
    pub peer_id: String,
    pub session_id: [u8; 16],
    pub replaced_existing_session: bool,
    pub queued_frames_drained: u64,
}

pub struct RelaySessionInboxV1 {
    peer_id: String,
    session_id: [u8; 16],
    receiver: mpsc::Receiver<OpaqueRelayDeliveryV1>,
    control_receiver: mpsc::Receiver<RelayPeerHandshakeDeliveryV1>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RelayRuntimeSnapshotV1 {
    pub accepting_new_work: bool,
    pub active_session_count: usize,
    pub active_peer_ids: Vec<String>,
    pub active_sessions: Vec<RelayActiveSessionSnapshotV1>,
    pub queued_frame_count: usize,
    pub registered_session_total: u64,
    pub replaced_session_total: u64,
    pub disconnected_session_total: u64,
    pub expired_session_total: u64,
    pub forwarded_frame_total: u64,
    pub queued_frame_total: u64,
    pub rate_limited_frame_total: u64,
    pub rejected_frame_total: u64,
    pub payload_treated_opaque: bool,
    pub relay_is_trusted_authority: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RelayActiveSessionSnapshotV1 {
    pub peer_id: String,
    pub session_id: [u8; 16],
    pub authenticated_at_ms: u64,
    pub last_seen_ms: u64,
    pub pending_delivery_capacity: usize,
}

#[derive(Clone)]
pub struct ProductRelaySessionManagerV1 {
    config: ProductRelayRuntimeConfigV1,
    accepting: Arc<AtomicBool>,
    state: Arc<RwLock<RelayRuntimeStateV1>>,
}

struct RelayRuntimeStateV1 {
    sessions: BTreeMap<String, RelaySessionEntryV1>,
    offline_queues: BTreeMap<String, VecDeque<OpaqueRelayDeliveryV1>>,
    offline_control_queues: BTreeMap<String, VecDeque<RelayPeerHandshakeDeliveryV1>>,
    counters: RelayRuntimeCountersV1,
}

struct RelaySessionEntryV1 {
    session_id: [u8; 16],
    authenticated_at_ms: u64,
    last_seen_ms: u64,
    rate_window_started_ms: u64,
    rate_window_frame_count: u64,
    sender: mpsc::Sender<OpaqueRelayDeliveryV1>,
    control_sender: mpsc::Sender<RelayPeerHandshakeDeliveryV1>,
}

#[derive(Debug, Default)]
struct RelayRuntimeCountersV1 {
    registered_session_total: u64,
    replaced_session_total: u64,
    disconnected_session_total: u64,
    expired_session_total: u64,
    forwarded_frame_total: u64,
    queued_frame_total: u64,
    rate_limited_frame_total: u64,
    rejected_frame_total: u64,
}

impl ProductRelaySessionManagerV1 {
    pub fn new(config: ProductRelayRuntimeConfigV1) -> Result<Self, ProductRelayRuntimeErrorV1> {
        validate_config_v1(&config)?;
        Ok(Self {
            config,
            accepting: Arc::new(AtomicBool::new(true)),
            state: Arc::new(RwLock::new(RelayRuntimeStateV1 {
                sessions: BTreeMap::new(),
                offline_queues: BTreeMap::new(),
                offline_control_queues: BTreeMap::new(),
                counters: RelayRuntimeCountersV1::default(),
            })),
        })
    }

    pub async fn register_authenticated_session(
        &self,
        authenticated_peer: AuthenticatedPeerV1,
        now_ms: u64,
    ) -> Result<(RelaySessionRegistrationV1, RelaySessionInboxV1), ProductRelayRuntimeErrorV1> {
        if !self.accepting.load(Ordering::Acquire) {
            return Err(ProductRelayRuntimeErrorV1::ShuttingDown);
        }
        let peer_id = authenticated_peer.peer_id().to_string();
        let session_id = authenticated_peer.session_id();
        let (sender, receiver) = mpsc::channel(self.config.session_queue_capacity);
        let (control_sender, control_receiver) = mpsc::channel(self.config.session_queue_capacity);
        let mut state = self.state.write().await;
        let replaced_existing_session = state.sessions.contains_key(&peer_id);
        let mut queued_frames_drained = 0u64;
        let mut remaining_queue = VecDeque::new();
        if let Some(mut queued) = state.offline_queues.remove(&peer_id) {
            while let Some(delivery) = queued.pop_front() {
                match sender.try_send(delivery) {
                    Ok(()) => queued_frames_drained = queued_frames_drained.saturating_add(1),
                    Err(mpsc::error::TrySendError::Full(delivery)) => {
                        remaining_queue.push_back(delivery);
                        remaining_queue.append(&mut queued);
                        break;
                    }
                    Err(mpsc::error::TrySendError::Closed(delivery)) => {
                        remaining_queue.push_back(delivery);
                        remaining_queue.append(&mut queued);
                        break;
                    }
                }
            }
        }
        if !remaining_queue.is_empty() {
            state
                .offline_queues
                .insert(peer_id.clone(), remaining_queue);
        }
        let mut remaining_control_queue = VecDeque::new();
        if let Some(mut queued) = state.offline_control_queues.remove(&peer_id) {
            while let Some(delivery) = queued.pop_front() {
                match control_sender.try_send(delivery) {
                    Ok(()) => queued_frames_drained = queued_frames_drained.saturating_add(1),
                    Err(mpsc::error::TrySendError::Full(delivery))
                    | Err(mpsc::error::TrySendError::Closed(delivery)) => {
                        remaining_control_queue.push_back(delivery);
                        remaining_control_queue.append(&mut queued);
                        break;
                    }
                }
            }
        }
        if !remaining_control_queue.is_empty() {
            state
                .offline_control_queues
                .insert(peer_id.clone(), remaining_control_queue);
        }
        state.sessions.insert(
            peer_id.clone(),
            RelaySessionEntryV1 {
                session_id,
                authenticated_at_ms: authenticated_peer.authenticated_at_ms(),
                last_seen_ms: now_ms,
                rate_window_started_ms: now_ms,
                rate_window_frame_count: 0,
                sender,
                control_sender,
            },
        );
        state.counters.registered_session_total =
            state.counters.registered_session_total.saturating_add(1);
        if replaced_existing_session {
            state.counters.replaced_session_total =
                state.counters.replaced_session_total.saturating_add(1);
        }
        Ok((
            RelaySessionRegistrationV1 {
                peer_id: peer_id.clone(),
                session_id,
                replaced_existing_session,
                queued_frames_drained,
            },
            RelaySessionInboxV1 {
                peer_id,
                session_id,
                receiver,
                control_receiver,
            },
        ))
    }

    pub async fn forward_opaque(
        &self,
        source_peer_id: &str,
        source_session_id: [u8; 16],
        envelope: SecureNovoRudpEnvelopeV1,
        now_ms: u64,
    ) -> RelayForwardOutcomeV1 {
        let target_peer_id = envelope.recipient_peer_id.clone();
        if !self.accepting.load(Ordering::Acquire) {
            return outcome_v1(
                RelayForwardDispositionV1::RejectedShuttingDown,
                source_peer_id,
                &target_peer_id,
            );
        }
        if envelope.sender_peer_id != source_peer_id || envelope.recipient_peer_id.is_empty() {
            let mut state = self.state.write().await;
            state.counters.rejected_frame_total =
                state.counters.rejected_frame_total.saturating_add(1);
            return outcome_v1(
                RelayForwardDispositionV1::RejectedRouteMismatch,
                source_peer_id,
                &target_peer_id,
            );
        }

        let delivery = OpaqueRelayDeliveryV1 {
            source_peer_id: source_peer_id.to_string(),
            target_peer_id: target_peer_id.clone(),
            received_at_ms: now_ms,
            envelope,
        };
        let mut state = self.state.write().await;
        let source_status = match state.sessions.get_mut(source_peer_id) {
            None => Some(RelayForwardDispositionV1::RejectedSourceSessionMissing),
            Some(source) if source.session_id != source_session_id => {
                Some(RelayForwardDispositionV1::RejectedStaleSourceSession)
            }
            Some(source)
                if now_ms.saturating_sub(source.last_seen_ms) > self.config.session_ttl_ms =>
            {
                Some(RelayForwardDispositionV1::RejectedSourceSessionExpired)
            }
            Some(source) => {
                if now_ms.saturating_sub(source.rate_window_started_ms)
                    >= self.config.rate_limit_window_ms
                {
                    source.rate_window_started_ms = now_ms;
                    source.rate_window_frame_count = 0;
                }
                if source.rate_window_frame_count >= self.config.rate_limit_frames {
                    Some(RelayForwardDispositionV1::RejectedRateLimited)
                } else {
                    source.rate_window_frame_count =
                        source.rate_window_frame_count.saturating_add(1);
                    source.last_seen_ms = now_ms;
                    None
                }
            }
        };
        if let Some(disposition) = source_status {
            if disposition == RelayForwardDispositionV1::RejectedRateLimited {
                state.counters.rate_limited_frame_total =
                    state.counters.rate_limited_frame_total.saturating_add(1);
            } else {
                state.counters.rejected_frame_total =
                    state.counters.rejected_frame_total.saturating_add(1);
            }
            if disposition == RelayForwardDispositionV1::RejectedSourceSessionExpired {
                state.sessions.remove(source_peer_id);
                state.counters.expired_session_total =
                    state.counters.expired_session_total.saturating_add(1);
            }
            return outcome_v1(disposition, source_peer_id, &target_peer_id);
        }

        let target_sender = state
            .sessions
            .get(&target_peer_id)
            .filter(|target| {
                now_ms.saturating_sub(target.last_seen_ms) <= self.config.session_ttl_ms
            })
            .map(|target| target.sender.clone());
        match target_sender {
            Some(sender) => match sender.try_send(delivery) {
                Ok(()) => {
                    state.counters.forwarded_frame_total =
                        state.counters.forwarded_frame_total.saturating_add(1);
                    outcome_v1(
                        RelayForwardDispositionV1::Forwarded,
                        source_peer_id,
                        &target_peer_id,
                    )
                }
                Err(mpsc::error::TrySendError::Full(delivery)) => {
                    if enqueue_offline_v1(&mut state, &self.config, delivery) {
                        state.counters.queued_frame_total =
                            state.counters.queued_frame_total.saturating_add(1);
                        outcome_v1(
                            RelayForwardDispositionV1::QueuedBackpressure,
                            source_peer_id,
                            &target_peer_id,
                        )
                    } else {
                        state.counters.rejected_frame_total =
                            state.counters.rejected_frame_total.saturating_add(1);
                        outcome_v1(
                            RelayForwardDispositionV1::RejectedQueueFull,
                            source_peer_id,
                            &target_peer_id,
                        )
                    }
                }
                Err(mpsc::error::TrySendError::Closed(delivery)) => {
                    state.sessions.remove(&target_peer_id);
                    if enqueue_offline_v1(&mut state, &self.config, delivery) {
                        state.counters.queued_frame_total =
                            state.counters.queued_frame_total.saturating_add(1);
                        outcome_v1(
                            RelayForwardDispositionV1::QueuedTargetOffline,
                            source_peer_id,
                            &target_peer_id,
                        )
                    } else {
                        state.counters.rejected_frame_total =
                            state.counters.rejected_frame_total.saturating_add(1);
                        outcome_v1(
                            RelayForwardDispositionV1::RejectedQueueFull,
                            source_peer_id,
                            &target_peer_id,
                        )
                    }
                }
            },
            None => {
                if enqueue_offline_v1(&mut state, &self.config, delivery) {
                    state.counters.queued_frame_total =
                        state.counters.queued_frame_total.saturating_add(1);
                    outcome_v1(
                        RelayForwardDispositionV1::QueuedTargetOffline,
                        source_peer_id,
                        &target_peer_id,
                    )
                } else {
                    state.counters.rejected_frame_total =
                        state.counters.rejected_frame_total.saturating_add(1);
                    outcome_v1(
                        RelayForwardDispositionV1::RejectedQueueFull,
                        source_peer_id,
                        &target_peer_id,
                    )
                }
            }
        }
    }

    pub async fn forward_peer_handshake(
        &self,
        source_peer_id: &str,
        source_session_id: [u8; 16],
        target_peer_id: &str,
        handshake: RelayPeerHandshakeV1,
        now_ms: u64,
    ) -> RelayForwardOutcomeV1 {
        if !self.accepting.load(Ordering::Acquire) {
            return outcome_v1(
                RelayForwardDispositionV1::RejectedShuttingDown,
                source_peer_id,
                target_peer_id,
            );
        }
        let route_matches = match &handshake {
            RelayPeerHandshakeV1::Offer(offer) => {
                offer.initiator_peer_id == source_peer_id
                    && offer.responder_peer_id == target_peer_id
            }
            RelayPeerHandshakeV1::Response(response) => {
                response.responder_peer_id == source_peer_id
                    && response.initiator_peer_id == target_peer_id
            }
        };
        if !route_matches || target_peer_id.is_empty() {
            let mut state = self.state.write().await;
            state.counters.rejected_frame_total =
                state.counters.rejected_frame_total.saturating_add(1);
            return outcome_v1(
                RelayForwardDispositionV1::RejectedRouteMismatch,
                source_peer_id,
                target_peer_id,
            );
        }
        let delivery = RelayPeerHandshakeDeliveryV1 {
            source_peer_id: source_peer_id.to_string(),
            target_peer_id: target_peer_id.to_string(),
            received_at_ms: now_ms,
            handshake,
        };
        let mut state = self.state.write().await;
        let source_status = match state.sessions.get_mut(source_peer_id) {
            None => Some(RelayForwardDispositionV1::RejectedSourceSessionMissing),
            Some(source) if source.session_id != source_session_id => {
                Some(RelayForwardDispositionV1::RejectedStaleSourceSession)
            }
            Some(source)
                if now_ms.saturating_sub(source.last_seen_ms) > self.config.session_ttl_ms =>
            {
                Some(RelayForwardDispositionV1::RejectedSourceSessionExpired)
            }
            Some(source) => {
                if now_ms.saturating_sub(source.rate_window_started_ms)
                    >= self.config.rate_limit_window_ms
                {
                    source.rate_window_started_ms = now_ms;
                    source.rate_window_frame_count = 0;
                }
                if source.rate_window_frame_count >= self.config.rate_limit_frames {
                    Some(RelayForwardDispositionV1::RejectedRateLimited)
                } else {
                    source.rate_window_frame_count =
                        source.rate_window_frame_count.saturating_add(1);
                    source.last_seen_ms = now_ms;
                    None
                }
            }
        };
        if let Some(disposition) = source_status {
            if disposition == RelayForwardDispositionV1::RejectedRateLimited {
                state.counters.rate_limited_frame_total =
                    state.counters.rate_limited_frame_total.saturating_add(1);
            } else {
                state.counters.rejected_frame_total =
                    state.counters.rejected_frame_total.saturating_add(1);
            }
            if disposition == RelayForwardDispositionV1::RejectedSourceSessionExpired {
                state.sessions.remove(source_peer_id);
                state.counters.expired_session_total =
                    state.counters.expired_session_total.saturating_add(1);
            }
            return outcome_v1(disposition, source_peer_id, target_peer_id);
        }
        let target_sender = state
            .sessions
            .get(target_peer_id)
            .filter(|target| {
                now_ms.saturating_sub(target.last_seen_ms) <= self.config.session_ttl_ms
            })
            .map(|target| target.control_sender.clone());
        match target_sender {
            Some(sender) => match sender.try_send(delivery) {
                Ok(()) => {
                    state.counters.forwarded_frame_total =
                        state.counters.forwarded_frame_total.saturating_add(1);
                    outcome_v1(
                        RelayForwardDispositionV1::Forwarded,
                        source_peer_id,
                        target_peer_id,
                    )
                }
                Err(mpsc::error::TrySendError::Full(delivery)) => {
                    if enqueue_offline_control_v1(&mut state, &self.config, delivery) {
                        state.counters.queued_frame_total =
                            state.counters.queued_frame_total.saturating_add(1);
                        outcome_v1(
                            RelayForwardDispositionV1::QueuedBackpressure,
                            source_peer_id,
                            target_peer_id,
                        )
                    } else {
                        state.counters.rejected_frame_total =
                            state.counters.rejected_frame_total.saturating_add(1);
                        outcome_v1(
                            RelayForwardDispositionV1::RejectedQueueFull,
                            source_peer_id,
                            target_peer_id,
                        )
                    }
                }
                Err(mpsc::error::TrySendError::Closed(delivery)) => {
                    state.sessions.remove(target_peer_id);
                    if enqueue_offline_control_v1(&mut state, &self.config, delivery) {
                        state.counters.queued_frame_total =
                            state.counters.queued_frame_total.saturating_add(1);
                        outcome_v1(
                            RelayForwardDispositionV1::QueuedTargetOffline,
                            source_peer_id,
                            target_peer_id,
                        )
                    } else {
                        state.counters.rejected_frame_total =
                            state.counters.rejected_frame_total.saturating_add(1);
                        outcome_v1(
                            RelayForwardDispositionV1::RejectedQueueFull,
                            source_peer_id,
                            target_peer_id,
                        )
                    }
                }
            },
            None => {
                if enqueue_offline_control_v1(&mut state, &self.config, delivery) {
                    state.counters.queued_frame_total =
                        state.counters.queued_frame_total.saturating_add(1);
                    outcome_v1(
                        RelayForwardDispositionV1::QueuedTargetOffline,
                        source_peer_id,
                        target_peer_id,
                    )
                } else {
                    state.counters.rejected_frame_total =
                        state.counters.rejected_frame_total.saturating_add(1);
                    outcome_v1(
                        RelayForwardDispositionV1::RejectedQueueFull,
                        source_peer_id,
                        target_peer_id,
                    )
                }
            }
        }
    }

    pub async fn heartbeat(&self, peer_id: &str, session_id: [u8; 16], now_ms: u64) -> bool {
        let mut state = self.state.write().await;
        let Some(session) = state.sessions.get_mut(peer_id) else {
            return false;
        };
        if session.session_id != session_id {
            return false;
        }
        session.last_seen_ms = now_ms;
        true
    }

    pub async fn disconnect(&self, peer_id: &str, session_id: [u8; 16]) -> bool {
        let mut state = self.state.write().await;
        let matching = state
            .sessions
            .get(peer_id)
            .is_some_and(|session| session.session_id == session_id);
        if !matching {
            return false;
        }
        state.sessions.remove(peer_id);
        state.counters.disconnected_session_total =
            state.counters.disconnected_session_total.saturating_add(1);
        true
    }

    pub async fn expire_stale_sessions(&self, now_ms: u64) -> usize {
        let mut state = self.state.write().await;
        let before = state.sessions.len();
        state.sessions.retain(|_, session| {
            now_ms.saturating_sub(session.last_seen_ms) <= self.config.session_ttl_ms
        });
        let expired = before.saturating_sub(state.sessions.len());
        state.counters.expired_session_total = state
            .counters
            .expired_session_total
            .saturating_add(expired as u64);
        expired
    }

    pub fn begin_graceful_shutdown(&self) {
        self.accepting.store(false, Ordering::Release);
    }

    pub async fn finish_graceful_shutdown(&self) -> RelayRuntimeSnapshotV1 {
        self.accepting.store(false, Ordering::Release);
        let mut state = self.state.write().await;
        state.sessions.clear();
        snapshot_v1(&state, false)
    }

    pub async fn snapshot(&self) -> RelayRuntimeSnapshotV1 {
        let state = self.state.read().await;
        snapshot_v1(&state, self.accepting.load(Ordering::Acquire))
    }
}

impl RelaySessionInboxV1 {
    #[must_use]
    pub fn peer_id(&self) -> &str {
        &self.peer_id
    }

    #[must_use]
    pub fn session_id(&self) -> [u8; 16] {
        self.session_id
    }

    pub async fn recv(&mut self) -> Option<OpaqueRelayDeliveryV1> {
        self.receiver.recv().await
    }

    pub fn try_recv(&mut self) -> Result<OpaqueRelayDeliveryV1, mpsc::error::TryRecvError> {
        self.receiver.try_recv()
    }

    pub async fn recv_peer_handshake(&mut self) -> Option<RelayPeerHandshakeDeliveryV1> {
        self.control_receiver.recv().await
    }

    pub fn try_recv_peer_handshake(
        &mut self,
    ) -> Result<RelayPeerHandshakeDeliveryV1, mpsc::error::TryRecvError> {
        self.control_receiver.try_recv()
    }
}

fn validate_config_v1(
    config: &ProductRelayRuntimeConfigV1,
) -> Result<(), ProductRelayRuntimeErrorV1> {
    if config.session_queue_capacity == 0 {
        return Err(ProductRelayRuntimeErrorV1::InvalidConfiguration(
            "session_queue_capacity must be positive",
        ));
    }
    if config.offline_queue_per_peer == 0 || config.offline_queue_total == 0 {
        return Err(ProductRelayRuntimeErrorV1::InvalidConfiguration(
            "offline queue limits must be positive",
        ));
    }
    if config.session_ttl_ms == 0
        || config.rate_limit_frames == 0
        || config.rate_limit_window_ms == 0
    {
        return Err(ProductRelayRuntimeErrorV1::InvalidConfiguration(
            "session and rate limits must be positive",
        ));
    }
    Ok(())
}

fn enqueue_offline_v1(
    state: &mut RelayRuntimeStateV1,
    config: &ProductRelayRuntimeConfigV1,
    delivery: OpaqueRelayDeliveryV1,
) -> bool {
    let total = state
        .offline_queues
        .values()
        .map(VecDeque::len)
        .sum::<usize>()
        .saturating_add(
            state
                .offline_control_queues
                .values()
                .map(VecDeque::len)
                .sum::<usize>(),
        );
    if total >= config.offline_queue_total {
        return false;
    }
    let queue = state
        .offline_queues
        .entry(delivery.target_peer_id.clone())
        .or_default();
    if queue.len() >= config.offline_queue_per_peer {
        return false;
    }
    queue.push_back(delivery);
    true
}

fn enqueue_offline_control_v1(
    state: &mut RelayRuntimeStateV1,
    config: &ProductRelayRuntimeConfigV1,
    delivery: RelayPeerHandshakeDeliveryV1,
) -> bool {
    let total = state
        .offline_queues
        .values()
        .map(VecDeque::len)
        .sum::<usize>()
        .saturating_add(
            state
                .offline_control_queues
                .values()
                .map(VecDeque::len)
                .sum::<usize>(),
        );
    if total >= config.offline_queue_total {
        return false;
    }
    let queue = state
        .offline_control_queues
        .entry(delivery.target_peer_id.clone())
        .or_default();
    if queue.len() >= config.offline_queue_per_peer {
        return false;
    }
    queue.push_back(delivery);
    true
}

fn outcome_v1(
    disposition: RelayForwardDispositionV1,
    source_peer_id: &str,
    target_peer_id: &str,
) -> RelayForwardOutcomeV1 {
    RelayForwardOutcomeV1 {
        forwarded: disposition == RelayForwardDispositionV1::Forwarded,
        queued: matches!(
            disposition,
            RelayForwardDispositionV1::QueuedTargetOffline
                | RelayForwardDispositionV1::QueuedBackpressure
        ),
        disposition,
        source_peer_id: source_peer_id.to_string(),
        target_peer_id: target_peer_id.to_string(),
        payload_treated_opaque: true,
    }
}

fn snapshot_v1(state: &RelayRuntimeStateV1, accepting_new_work: bool) -> RelayRuntimeSnapshotV1 {
    let mut active_peer_ids = state.sessions.keys().cloned().collect::<Vec<_>>();
    active_peer_ids.sort();
    let active_sessions = state
        .sessions
        .iter()
        .map(|(peer_id, session)| RelayActiveSessionSnapshotV1 {
            peer_id: peer_id.clone(),
            session_id: session.session_id,
            authenticated_at_ms: session.authenticated_at_ms,
            last_seen_ms: session.last_seen_ms,
            pending_delivery_capacity: session.sender.capacity(),
        })
        .collect();
    RelayRuntimeSnapshotV1 {
        accepting_new_work,
        active_session_count: active_peer_ids.len(),
        active_peer_ids,
        active_sessions,
        queued_frame_count: state
            .offline_queues
            .values()
            .map(VecDeque::len)
            .sum::<usize>()
            .saturating_add(
                state
                    .offline_control_queues
                    .values()
                    .map(VecDeque::len)
                    .sum::<usize>(),
            ),
        registered_session_total: state.counters.registered_session_total,
        replaced_session_total: state.counters.replaced_session_total,
        disconnected_session_total: state.counters.disconnected_session_total,
        expired_session_total: state.counters.expired_session_total,
        forwarded_frame_total: state.counters.forwarded_frame_total,
        queued_frame_total: state.counters.queued_frame_total,
        rate_limited_frame_total: state.counters.rate_limited_frame_total,
        rejected_frame_total: state.counters.rejected_frame_total,
        payload_treated_opaque: true,
        relay_is_trusted_authority: false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        novorudp::{NovoRudpTransportFrameKindV0, NovoRudpTransportFrameV0},
        product_overlay::{
            peer_id_from_ed25519_public_key_v1, E2eSecureChannelV1, HandshakeReplayCacheV1,
            NodeHandshakeInitiatorV1, NodeHandshakeResponderV1,
        },
    };
    use ed25519_dalek::SigningKey;

    fn authenticate_to_relay(
        node_identity: &SigningKey,
        relay_identity: &SigningKey,
        now_ms: u64,
    ) -> AuthenticatedPeerV1 {
        let relay_peer_id =
            peer_id_from_ed25519_public_key_v1(&relay_identity.verifying_key().to_bytes());
        let initiator =
            NodeHandshakeInitiatorV1::start(node_identity, relay_peer_id, now_ms, 5_000)
                .expect("start relay auth");
        let mut replay = HandshakeReplayCacheV1::default();
        NodeHandshakeResponderV1::respond(
            initiator.offer(),
            relay_identity,
            now_ms + 1,
            5_000,
            &mut replay,
        )
        .expect("authenticate node")
        .authenticated_remote()
        .clone()
    }

    fn peer_channels(
        initiator_identity: &SigningKey,
        responder_identity: &SigningKey,
        now_ms: u64,
    ) -> (E2eSecureChannelV1, E2eSecureChannelV1) {
        let responder_peer_id =
            peer_id_from_ed25519_public_key_v1(&responder_identity.verifying_key().to_bytes());
        let initiator =
            NodeHandshakeInitiatorV1::start(initiator_identity, responder_peer_id, now_ms, 5_000)
                .expect("start peer handshake");
        let mut responder_replay = HandshakeReplayCacheV1::default();
        let responder = NodeHandshakeResponderV1::respond(
            initiator.offer(),
            responder_identity,
            now_ms + 1,
            5_000,
            &mut responder_replay,
        )
        .expect("respond peer handshake");
        let response = responder.response().clone();
        let responder_channel = responder.into_channel();
        let mut initiator_replay = HandshakeReplayCacheV1::default();
        let initiator_channel = initiator
            .complete(&response, now_ms + 2, &mut initiator_replay)
            .expect("complete peer handshake");
        (initiator_channel, responder_channel)
    }

    fn frame(sequence: u64) -> NovoRudpTransportFrameV0 {
        NovoRudpTransportFrameV0::new(
            NovoRudpTransportFrameKindV0::Data,
            [0x72; 16],
            1,
            2,
            sequence,
            3,
            format!("relay-opaque-{sequence}").into_bytes(),
        )
    }

    #[tokio::test]
    async fn authenticated_sessions_forward_end_to_end_ciphertext() {
        let relay_identity = SigningKey::from_bytes(&[60u8; 32]);
        let node_a_identity = SigningKey::from_bytes(&[61u8; 32]);
        let node_b_identity = SigningKey::from_bytes(&[62u8; 32]);
        let auth_a = authenticate_to_relay(&node_a_identity, &relay_identity, 1_000);
        let auth_b = authenticate_to_relay(&node_b_identity, &relay_identity, 1_000);
        let (mut channel_a, mut channel_b) =
            peer_channels(&node_a_identity, &node_b_identity, 1_100);
        let manager = ProductRelaySessionManagerV1::new(ProductRelayRuntimeConfigV1::default())
            .expect("relay manager");
        let (registration_a, _inbox_a) = manager
            .register_authenticated_session(auth_a, 1_200)
            .await
            .expect("register a");
        let (_registration_b, mut inbox_b) = manager
            .register_authenticated_session(auth_b, 1_200)
            .await
            .expect("register b");
        let envelope = channel_a
            .seal_novorudp_frame(&frame(7))
            .expect("seal frame");
        let outcome = manager
            .forward_opaque(
                channel_a.local_peer_id(),
                registration_a.session_id,
                envelope,
                1_300,
            )
            .await;
        assert_eq!(outcome.disposition, RelayForwardDispositionV1::Forwarded);
        let delivered = inbox_b.recv().await.expect("delivery");
        assert_eq!(
            channel_b
                .open_novorudp_frame(&delivered.envelope)
                .expect("open frame"),
            frame(7)
        );
        let snapshot = manager.snapshot().await;
        assert_eq!(snapshot.active_session_count, 2);
        assert_eq!(snapshot.forwarded_frame_total, 1);
        assert!(snapshot.payload_treated_opaque);
        assert!(!snapshot.relay_is_trusted_authority);
    }

    #[tokio::test]
    async fn authenticated_sessions_relay_signed_peer_handshake_without_payload_access() {
        let relay_identity = SigningKey::from_bytes(&[73u8; 32]);
        let node_a_identity = SigningKey::from_bytes(&[74u8; 32]);
        let node_b_identity = SigningKey::from_bytes(&[75u8; 32]);
        let auth_a = authenticate_to_relay(&node_a_identity, &relay_identity, 5_000);
        let auth_b = authenticate_to_relay(&node_b_identity, &relay_identity, 5_000);
        let manager = ProductRelaySessionManagerV1::new(ProductRelayRuntimeConfigV1::default())
            .expect("relay manager");
        let (registration_a, _inbox_a) = manager
            .register_authenticated_session(auth_a, 5_100)
            .await
            .expect("register a");
        let (_registration_b, mut inbox_b) = manager
            .register_authenticated_session(auth_b, 5_100)
            .await
            .expect("register b");
        let node_b_peer_id =
            peer_id_from_ed25519_public_key_v1(&node_b_identity.verifying_key().to_bytes());
        let offer =
            NodeHandshakeInitiatorV1::start(&node_a_identity, node_b_peer_id.clone(), 5_200, 5_000)
                .expect("start peer offer")
                .offer()
                .clone();
        let outcome = manager
            .forward_peer_handshake(
                &registration_a.peer_id,
                registration_a.session_id,
                &node_b_peer_id,
                RelayPeerHandshakeV1::Offer(offer.clone()),
                5_300,
            )
            .await;
        assert_eq!(outcome.disposition, RelayForwardDispositionV1::Forwarded);
        let delivered = inbox_b
            .recv_peer_handshake()
            .await
            .expect("peer handshake delivery");
        assert_eq!(delivered.source_peer_id, registration_a.peer_id);
        assert_eq!(delivered.target_peer_id, node_b_peer_id);
        assert_eq!(delivered.handshake, RelayPeerHandshakeV1::Offer(offer));
        assert!(manager.snapshot().await.payload_treated_opaque);
    }

    #[tokio::test]
    async fn stale_replaced_session_cannot_send() {
        let relay_identity = SigningKey::from_bytes(&[63u8; 32]);
        let node_identity = SigningKey::from_bytes(&[64u8; 32]);
        let target_identity = SigningKey::from_bytes(&[65u8; 32]);
        let first = authenticate_to_relay(&node_identity, &relay_identity, 2_000);
        let second = authenticate_to_relay(&node_identity, &relay_identity, 2_100);
        let target = authenticate_to_relay(&target_identity, &relay_identity, 2_100);
        let (mut source_channel, _target_channel) =
            peer_channels(&node_identity, &target_identity, 2_200);
        let manager = ProductRelaySessionManagerV1::new(ProductRelayRuntimeConfigV1::default())
            .expect("relay manager");
        let (first_registration, _first_inbox) = manager
            .register_authenticated_session(first, 2_300)
            .await
            .expect("first registration");
        let (second_registration, _second_inbox) = manager
            .register_authenticated_session(second, 2_400)
            .await
            .expect("replacement registration");
        manager
            .register_authenticated_session(target, 2_400)
            .await
            .expect("target registration");
        assert!(second_registration.replaced_existing_session);
        let envelope = source_channel
            .seal_novorudp_frame(&frame(8))
            .expect("seal frame");
        let outcome = manager
            .forward_opaque(
                source_channel.local_peer_id(),
                first_registration.session_id,
                envelope,
                2_500,
            )
            .await;
        assert_eq!(
            outcome.disposition,
            RelayForwardDispositionV1::RejectedStaleSourceSession
        );
    }

    #[tokio::test]
    async fn offline_queue_drains_on_authenticated_reconnect() {
        let relay_identity = SigningKey::from_bytes(&[66u8; 32]);
        let node_a_identity = SigningKey::from_bytes(&[67u8; 32]);
        let node_b_identity = SigningKey::from_bytes(&[68u8; 32]);
        let auth_a = authenticate_to_relay(&node_a_identity, &relay_identity, 3_000);
        let auth_b = authenticate_to_relay(&node_b_identity, &relay_identity, 3_100);
        let (mut channel_a, mut channel_b) =
            peer_channels(&node_a_identity, &node_b_identity, 3_200);
        let manager = ProductRelaySessionManagerV1::new(ProductRelayRuntimeConfigV1::default())
            .expect("relay manager");
        let (registration_a, _inbox_a) = manager
            .register_authenticated_session(auth_a, 3_300)
            .await
            .expect("register a");
        let envelope = channel_a
            .seal_novorudp_frame(&frame(9))
            .expect("seal frame");
        let queued = manager
            .forward_opaque(
                channel_a.local_peer_id(),
                registration_a.session_id,
                envelope,
                3_400,
            )
            .await;
        assert_eq!(
            queued.disposition,
            RelayForwardDispositionV1::QueuedTargetOffline
        );
        let (registration_b, mut inbox_b) = manager
            .register_authenticated_session(auth_b, 3_500)
            .await
            .expect("register b");
        assert_eq!(registration_b.queued_frames_drained, 1);
        let delivered = inbox_b.recv().await.expect("queued delivery");
        assert_eq!(
            channel_b
                .open_novorudp_frame(&delivered.envelope)
                .expect("open queued frame"),
            frame(9)
        );
    }

    #[tokio::test]
    async fn rate_limit_backpressure_expiry_and_shutdown_are_enforced() {
        let relay_identity = SigningKey::from_bytes(&[69u8; 32]);
        let node_a_identity = SigningKey::from_bytes(&[70u8; 32]);
        let node_b_identity = SigningKey::from_bytes(&[71u8; 32]);
        let auth_a = authenticate_to_relay(&node_a_identity, &relay_identity, 4_000);
        let auth_b = authenticate_to_relay(&node_b_identity, &relay_identity, 4_000);
        let (mut channel_a, _channel_b) = peer_channels(&node_a_identity, &node_b_identity, 4_100);
        let manager = ProductRelaySessionManagerV1::new(ProductRelayRuntimeConfigV1 {
            session_queue_capacity: 1,
            offline_queue_per_peer: 1,
            offline_queue_total: 1,
            session_ttl_ms: 100,
            rate_limit_frames: 2,
            rate_limit_window_ms: 1_000,
        })
        .expect("relay manager");
        let (registration_a, _inbox_a) = manager
            .register_authenticated_session(auth_a, 4_200)
            .await
            .expect("register a");
        let (_registration_b, _inbox_b) = manager
            .register_authenticated_session(auth_b, 4_200)
            .await
            .expect("register b");
        let source_peer_id = channel_a.local_peer_id().to_string();

        let first_envelope = channel_a
            .seal_novorudp_frame(&frame(10))
            .expect("seal first");
        let first = manager
            .forward_opaque(
                &source_peer_id,
                registration_a.session_id,
                first_envelope,
                4_210,
            )
            .await;
        assert_eq!(first.disposition, RelayForwardDispositionV1::Forwarded);
        let second_envelope = channel_a
            .seal_novorudp_frame(&frame(11))
            .expect("seal second");
        let second = manager
            .forward_opaque(
                &source_peer_id,
                registration_a.session_id,
                second_envelope,
                4_220,
            )
            .await;
        assert_eq!(
            second.disposition,
            RelayForwardDispositionV1::QueuedBackpressure
        );
        let limited_envelope = channel_a
            .seal_novorudp_frame(&frame(12))
            .expect("seal limited");
        let limited = manager
            .forward_opaque(
                &source_peer_id,
                registration_a.session_id,
                limited_envelope,
                4_230,
            )
            .await;
        assert_eq!(
            limited.disposition,
            RelayForwardDispositionV1::RejectedRateLimited
        );
        assert_eq!(manager.expire_stale_sessions(4_401).await, 2);
        manager.begin_graceful_shutdown();
        let shutdown_envelope = channel_a
            .seal_novorudp_frame(&frame(13))
            .expect("seal shutdown");
        let shutdown = manager
            .forward_opaque(
                &source_peer_id,
                registration_a.session_id,
                shutdown_envelope,
                4_410,
            )
            .await;
        assert_eq!(
            shutdown.disposition,
            RelayForwardDispositionV1::RejectedShuttingDown
        );
    }
}
