use crate::product_overlay::{
    AuthenticatedPeerV1, NodeHandshakeOfferV1, NodeHandshakeResponseV1, SecureNovoRudpEnvelopeV1,
};
use serde::{Deserialize, Serialize};
use std::{
    collections::{BTreeMap, VecDeque},
    sync::{
        atomic::{AtomicBool, AtomicUsize, Ordering},
        Arc,
    },
};
use thiserror::Error;
use tokio::sync::{mpsc, RwLock};

pub const PRODUCT_RELAY_MAX_WIRE_MESSAGE_BYTES_V1: usize = 1_048_576;
const PRODUCT_RELAY_BYTE_RATE_WINDOW_MS_V1: u64 = 60_000;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProductRelayRuntimeConfigV1 {
    pub max_sessions: usize,
    pub max_tracked_sources: usize,
    pub session_queue_capacity: usize,
    pub session_queue_bytes: usize,
    pub active_queue_total: usize,
    pub active_queue_bytes_total: usize,
    pub offline_queue_per_peer: usize,
    pub offline_queue_bytes_per_peer: usize,
    pub offline_queue_per_source: usize,
    pub offline_queue_bytes_per_source: usize,
    pub offline_queue_total: usize,
    pub offline_queue_bytes_total: usize,
    pub offline_queue_ttl_ms: u64,
    pub session_ttl_ms: u64,
    pub rate_limit_frames: u64,
    pub max_frames_per_window: u64,
    pub rate_limit_window_ms: u64,
    pub source_bytes_per_minute: u64,
    pub max_bytes_per_minute: u64,
}

impl Default for ProductRelayRuntimeConfigV1 {
    fn default() -> Self {
        Self {
            max_sessions: 256,
            max_tracked_sources: 1_024,
            session_queue_capacity: 256,
            session_queue_bytes: 8 * 1024 * 1024,
            active_queue_total: 16_384,
            active_queue_bytes_total: 256 * 1024 * 1024,
            offline_queue_per_peer: 512,
            offline_queue_bytes_per_peer: 16 * 1024 * 1024,
            offline_queue_per_source: 1_024,
            offline_queue_bytes_per_source: 32 * 1024 * 1024,
            offline_queue_total: 16_384,
            offline_queue_bytes_total: 256 * 1024 * 1024,
            offline_queue_ttl_ms: 60_000,
            session_ttl_ms: 45_000,
            rate_limit_frames: 4_096,
            max_frames_per_window: 65_536,
            rate_limit_window_ms: 1_000,
            source_bytes_per_minute: 64 * 1024 * 1024,
            max_bytes_per_minute: 1024 * 1024 * 1024,
        }
    }
}

#[derive(Debug, Error, Clone, PartialEq, Eq)]
pub enum ProductRelayRuntimeErrorV1 {
    #[error("relay runtime is shutting down")]
    ShuttingDown,
    #[error("relay runtime configuration is invalid: {0}")]
    InvalidConfiguration(&'static str),
    #[error("relay authenticated session limit reached: {max_sessions}")]
    SessionLimitReached { max_sessions: usize },
    #[error("relay source-budget tracking limit reached: {max_tracked_sources}")]
    SourceTrackingLimitReached { max_tracked_sources: usize },
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
    ForwardOutcome(RelayForwardOutcomeV1),
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
    RejectedAggregateRateLimited,
    RejectedSourceByteLimited,
    RejectedAggregateByteLimited,
    RejectedWireMessageTooLarge,
    RejectedQueueFull,
    RejectedQueuePeerLimit,
    RejectedQueueSourceLimit,
    RejectedQueueTotalLimit,
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
    pub envelope_session_id: Option<[u8; 16]>,
    pub envelope_sequence: Option<u64>,
    pub admitted_wire_bytes: usize,
}

#[derive(Debug)]
#[must_use = "an admitted relay wire token must be consumed by dispatch or rejection accounting"]
pub struct RelayIngressAdmissionV1 {
    source_peer_id: String,
    source_session_id: [u8; 16],
    wire_bytes: usize,
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
    receiver: mpsc::Receiver<RelayActiveQueueItemV1<OpaqueRelayDeliveryV1>>,
    control_receiver: mpsc::Receiver<RelayActiveQueueItemV1<RelayPeerHandshakeDeliveryV1>>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RelayRuntimeSnapshotV1 {
    pub accepting_new_work: bool,
    pub active_session_count: usize,
    pub tracked_source_count: usize,
    pub active_peer_ids: Vec<String>,
    pub active_sessions: Vec<RelayActiveSessionSnapshotV1>,
    pub queued_frame_count: usize,
    pub queued_bytes: usize,
    pub active_queued_frame_count: usize,
    pub active_queued_bytes: usize,
    pub offline_queued_frame_count: usize,
    pub offline_queued_bytes: usize,
    pub limits: RelayRuntimeLimitsSnapshotV1,
    pub registered_session_total: u64,
    pub session_limit_rejection_total: u64,
    pub shutdown_rejection_total: u64,
    pub replaced_session_total: u64,
    pub disconnected_session_total: u64,
    pub expired_session_total: u64,
    pub forwarded_frame_total: u64,
    pub queued_frame_total: u64,
    pub rate_limited_frame_total: u64,
    pub aggregate_rate_limited_frame_total: u64,
    pub wire_message_too_large_total: u64,
    pub source_byte_limited_frame_total: u64,
    pub aggregate_byte_limited_frame_total: u64,
    pub admitted_wire_bytes_total: u64,
    pub rejected_wire_bytes_total: u64,
    pub active_queue_byte_limited_frame_total: u64,
    pub active_queue_count_limited_frame_total: u64,
    pub offline_peer_limited_frame_total: u64,
    pub offline_source_limited_frame_total: u64,
    pub offline_total_limited_frame_total: u64,
    pub expired_queued_frame_total: u64,
    pub expired_queued_bytes_total: u64,
    pub offline_full_sweep_total: u64,
    pub protocol_rejected_frame_total: u64,
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
    pub pending_control_capacity: usize,
    pub queued_frame_count: usize,
    pub queued_bytes: usize,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RelayRuntimeLimitsSnapshotV1 {
    pub max_sessions: usize,
    pub max_tracked_sources: usize,
    pub session_queue_capacity: usize,
    pub session_queue_bytes: usize,
    pub active_queue_total: usize,
    pub active_queue_bytes_total: usize,
    pub offline_queue_per_peer: usize,
    pub offline_queue_bytes_per_peer: usize,
    pub offline_queue_per_source: usize,
    pub offline_queue_bytes_per_source: usize,
    pub offline_queue_total: usize,
    pub offline_queue_bytes_total: usize,
    pub offline_queue_ttl_ms: u64,
    pub session_ttl_ms: u64,
    pub rate_limit_frames: u64,
    pub max_frames_per_window: u64,
    pub rate_limit_window_ms: u64,
    pub source_bytes_per_minute: u64,
    pub max_bytes_per_minute: u64,
    pub max_wire_message_bytes: usize,
}

#[derive(Clone)]
pub struct ProductRelaySessionManagerV1 {
    config: ProductRelayRuntimeConfigV1,
    accepting: Arc<AtomicBool>,
    state: Arc<RwLock<RelayRuntimeStateV1>>,
}

struct RelayRuntimeStateV1 {
    sessions: BTreeMap<String, RelaySessionEntryV1>,
    source_budgets: BTreeMap<String, RelaySourceBudgetV1>,
    aggregate_frame_budget: RelayFrameBudgetV1,
    aggregate_byte_budget: RelayByteBudgetV1,
    offline_queues: BTreeMap<String, VecDeque<RelayOfflineQueueItemV1>>,
    offline_usage_by_peer: BTreeMap<String, RelayQueueUsageV1>,
    offline_usage_by_source: BTreeMap<String, RelayQueueUsageV1>,
    offline_usage: RelayQueueUsageV1,
    active_queue_accounting: Arc<RelayActiveQueueAccountingV1>,
    counters: RelayRuntimeCountersV1,
}

struct RelaySessionEntryV1 {
    session_id: [u8; 16],
    authenticated_at_ms: u64,
    last_seen_ms: u64,
    sender: mpsc::Sender<RelayActiveQueueItemV1<OpaqueRelayDeliveryV1>>,
    control_sender: mpsc::Sender<RelayActiveQueueItemV1<RelayPeerHandshakeDeliveryV1>>,
    active_queue_accounting: Arc<RelayActiveQueueAccountingV1>,
}

#[derive(Debug, Default)]
struct RelaySourceBudgetV1 {
    frame_window_started_ms: u64,
    frame_count: u64,
    byte_window_started_ms: u64,
    byte_count: u64,
    last_activity_ms: u64,
}

#[derive(Debug, Default)]
struct RelayByteBudgetV1 {
    window_started_ms: u64,
    byte_count: u64,
}

#[derive(Debug, Default)]
struct RelayFrameBudgetV1 {
    window_started_ms: u64,
    frame_count: u64,
}

#[derive(Debug, Clone, Copy, Default)]
struct RelayQueueUsageV1 {
    count: usize,
    bytes: usize,
}

enum RelayOfflineMessageV1 {
    Data(OpaqueRelayDeliveryV1),
    Control(RelayPeerHandshakeDeliveryV1),
}

struct RelayOfflineQueueItemV1 {
    source_peer_id: String,
    target_peer_id: String,
    received_at_ms: u64,
    active_accounted_bytes: usize,
    offline_accounted_bytes: usize,
    message: RelayOfflineMessageV1,
}

#[derive(Debug, Default)]
struct RelayActiveQueueAccountingV1 {
    frames: AtomicUsize,
    bytes: AtomicUsize,
}

struct RelayActiveQueueItemV1<T> {
    item: Option<T>,
    accounted_bytes: usize,
    session_accounting: Arc<RelayActiveQueueAccountingV1>,
    global_accounting: Arc<RelayActiveQueueAccountingV1>,
    released: bool,
}

#[derive(Debug, Default)]
struct RelayRuntimeCountersV1 {
    registered_session_total: u64,
    session_limit_rejection_total: u64,
    shutdown_rejection_total: u64,
    replaced_session_total: u64,
    disconnected_session_total: u64,
    expired_session_total: u64,
    forwarded_frame_total: u64,
    queued_frame_total: u64,
    rate_limited_frame_total: u64,
    aggregate_rate_limited_frame_total: u64,
    wire_message_too_large_total: u64,
    source_byte_limited_frame_total: u64,
    aggregate_byte_limited_frame_total: u64,
    admitted_wire_bytes_total: u64,
    rejected_wire_bytes_total: u64,
    active_queue_byte_limited_frame_total: u64,
    active_queue_count_limited_frame_total: u64,
    offline_peer_limited_frame_total: u64,
    offline_source_limited_frame_total: u64,
    offline_total_limited_frame_total: u64,
    expired_queued_frame_total: u64,
    expired_queued_bytes_total: u64,
    offline_full_sweep_total: u64,
    protocol_rejected_frame_total: u64,
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
                source_budgets: BTreeMap::new(),
                aggregate_frame_budget: RelayFrameBudgetV1::default(),
                aggregate_byte_budget: RelayByteBudgetV1::default(),
                offline_queues: BTreeMap::new(),
                offline_usage_by_peer: BTreeMap::new(),
                offline_usage_by_source: BTreeMap::new(),
                offline_usage: RelayQueueUsageV1::default(),
                active_queue_accounting: Arc::new(RelayActiveQueueAccountingV1::default()),
                counters: RelayRuntimeCountersV1::default(),
            })),
        })
    }

    pub async fn register_authenticated_session(
        &self,
        authenticated_peer: AuthenticatedPeerV1,
        now_ms: u64,
    ) -> Result<(RelaySessionRegistrationV1, RelaySessionInboxV1), ProductRelayRuntimeErrorV1> {
        let peer_id = authenticated_peer.peer_id().to_string();
        let session_id = authenticated_peer.session_id();
        let mut state = self.state.write().await;
        if !self.accepting.load(Ordering::Acquire) {
            state.counters.shutdown_rejection_total =
                state.counters.shutdown_rejection_total.saturating_add(1);
            return Err(ProductRelayRuntimeErrorV1::ShuttingDown);
        }
        prune_stale_sessions_locked_v1(&mut state, &self.config, now_ms);
        let replaced_existing_session = state.sessions.contains_key(&peer_id);
        if !replaced_existing_session && state.sessions.len() >= self.config.max_sessions {
            state.counters.session_limit_rejection_total = state
                .counters
                .session_limit_rejection_total
                .saturating_add(1);
            return Err(ProductRelayRuntimeErrorV1::SessionLimitReached {
                max_sessions: self.config.max_sessions,
            });
        }
        ensure_source_budget_v1(&mut state, &self.config, &peer_id, now_ms)?;
        let (sender, receiver) = mpsc::channel(self.config.session_queue_capacity);
        let (control_sender, control_receiver) = mpsc::channel(self.config.session_queue_capacity);
        let session_accounting = Arc::new(RelayActiveQueueAccountingV1::default());
        let queued_frames_drained = drain_offline_queue_into_session_v1(
            &mut state,
            &self.config,
            &peer_id,
            now_ms,
            &sender,
            &control_sender,
            &session_accounting,
        );
        state.sessions.insert(
            peer_id.clone(),
            RelaySessionEntryV1 {
                session_id,
                authenticated_at_ms: authenticated_peer.authenticated_at_ms(),
                last_seen_ms: now_ms,
                sender,
                control_sender,
                active_queue_accounting: Arc::clone(&session_accounting),
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

    pub async fn drain_queued_for_session(
        &self,
        peer_id: &str,
        session_id: [u8; 16],
        now_ms: u64,
    ) -> u64 {
        let mut state = self.state.write().await;
        expire_session_if_stale_v1(&mut state, &self.config, peer_id, now_ms);
        let Some((sender, control_sender, session_accounting)) = state
            .sessions
            .get(peer_id)
            .filter(|session| session.session_id == session_id)
            .map(|session| {
                (
                    session.sender.clone(),
                    session.control_sender.clone(),
                    Arc::clone(&session.active_queue_accounting),
                )
            })
        else {
            return 0;
        };
        drain_offline_queue_into_session_v1(
            &mut state,
            &self.config,
            peer_id,
            now_ms,
            &sender,
            &control_sender,
            &session_accounting,
        )
    }

    pub async fn admit_authenticated_wire_v1(
        &self,
        source_peer_id: &str,
        source_session_id: [u8; 16],
        wire_bytes: usize,
        now_ms: u64,
    ) -> Result<RelayIngressAdmissionV1, RelayForwardDispositionV1> {
        let mut state = self.state.write().await;
        if !self.accepting.load(Ordering::Acquire) {
            state.counters.shutdown_rejection_total =
                state.counters.shutdown_rejection_total.saturating_add(1);
            record_rejected_wire_v1(&mut state, wire_bytes);
            return Err(RelayForwardDispositionV1::RejectedShuttingDown);
        }
        admit_source_frame_v1(
            &mut state,
            &self.config,
            source_peer_id,
            source_session_id,
            wire_bytes,
            now_ms,
        )?;
        Ok(RelayIngressAdmissionV1 {
            source_peer_id: source_peer_id.to_string(),
            source_session_id,
            wire_bytes,
        })
    }

    pub async fn reject_admitted_wire_v1(&self, admission: RelayIngressAdmissionV1) {
        let mut state = self.state.write().await;
        state.counters.protocol_rejected_frame_total = state
            .counters
            .protocol_rejected_frame_total
            .saturating_add(1);
        record_rejected_wire_v1(&mut state, admission.wire_bytes);
    }

    pub async fn forward_opaque(
        &self,
        source_peer_id: &str,
        source_session_id: [u8; 16],
        envelope: SecureNovoRudpEnvelopeV1,
        now_ms: u64,
    ) -> RelayForwardOutcomeV1 {
        let wire_bytes = serde_json::to_vec(&ProductRelayWireMessageV1::Data(envelope.clone()))
            .map_or(usize::MAX, |wire| wire.len());
        self.forward_opaque_with_wire_bytes(
            source_peer_id,
            source_session_id,
            envelope,
            wire_bytes,
            now_ms,
        )
        .await
    }

    pub async fn forward_opaque_with_wire_bytes(
        &self,
        source_peer_id: &str,
        source_session_id: [u8; 16],
        envelope: SecureNovoRudpEnvelopeV1,
        wire_bytes: usize,
        now_ms: u64,
    ) -> RelayForwardOutcomeV1 {
        let target_peer_id = envelope.recipient_peer_id.clone();
        let envelope_session_id = envelope.session_id;
        let envelope_sequence = envelope.sequence;
        let admission = match self
            .admit_authenticated_wire_v1(source_peer_id, source_session_id, wire_bytes, now_ms)
            .await
        {
            Ok(admission) => admission,
            Err(disposition) => {
                return data_outcome_v1(
                    disposition,
                    source_peer_id,
                    &target_peer_id,
                    envelope_session_id,
                    envelope_sequence,
                    wire_bytes,
                )
            }
        };
        self.forward_opaque_admitted_v1(admission, envelope, now_ms)
            .await
    }

    pub async fn forward_opaque_admitted_v1(
        &self,
        admission: RelayIngressAdmissionV1,
        envelope: SecureNovoRudpEnvelopeV1,
        now_ms: u64,
    ) -> RelayForwardOutcomeV1 {
        let source_peer_id = admission.source_peer_id.clone();
        let wire_bytes = admission.wire_bytes;
        let target_peer_id = envelope.recipient_peer_id.clone();
        let envelope_session_id = envelope.session_id;
        let envelope_sequence = envelope.sequence;
        let mut state = self.state.write().await;
        if !self.accepting.load(Ordering::Acquire) {
            state.counters.shutdown_rejection_total =
                state.counters.shutdown_rejection_total.saturating_add(1);
            record_rejected_wire_v1(&mut state, wire_bytes);
            return data_outcome_v1(
                RelayForwardDispositionV1::RejectedShuttingDown,
                &source_peer_id,
                &target_peer_id,
                envelope_session_id,
                envelope_sequence,
                wire_bytes,
            );
        }
        if let Err(disposition) =
            validate_admitted_source_v1(&mut state, &self.config, &admission, now_ms)
        {
            return data_outcome_v1(
                disposition,
                &source_peer_id,
                &target_peer_id,
                envelope_session_id,
                envelope_sequence,
                wire_bytes,
            );
        }
        if envelope.sender_peer_id != source_peer_id || envelope.recipient_peer_id.is_empty() {
            state.counters.rejected_frame_total =
                state.counters.rejected_frame_total.saturating_add(1);
            state.counters.rejected_wire_bytes_total = state
                .counters
                .rejected_wire_bytes_total
                .saturating_add(wire_bytes as u64);
            return data_outcome_v1(
                RelayForwardDispositionV1::RejectedRouteMismatch,
                &source_peer_id,
                &target_peer_id,
                envelope_session_id,
                envelope_sequence,
                wire_bytes,
            );
        }

        let delivery = OpaqueRelayDeliveryV1 {
            source_peer_id: source_peer_id.clone(),
            target_peer_id: target_peer_id.clone(),
            received_at_ms: now_ms,
            envelope,
        };
        let accounted_bytes =
            serde_json::to_vec(&ProductRelayWireMessageV1::Delivery(delivery.clone()))
                .map_or(usize::MAX, |wire| wire.len());
        if accounted_bytes > PRODUCT_RELAY_MAX_WIRE_MESSAGE_BYTES_V1 {
            state.counters.wire_message_too_large_total = state
                .counters
                .wire_message_too_large_total
                .saturating_add(1);
            record_rejected_wire_v1(&mut state, wire_bytes);
            return data_outcome_v1(
                RelayForwardDispositionV1::RejectedWireMessageTooLarge,
                &source_peer_id,
                &target_peer_id,
                envelope_session_id,
                envelope_sequence,
                wire_bytes,
            );
        }

        expire_session_if_stale_v1(&mut state, &self.config, &target_peer_id, now_ms);
        let target_session = state
            .sessions
            .get(&target_peer_id)
            .filter(|target| {
                now_ms.saturating_sub(target.last_seen_ms) <= self.config.session_ttl_ms
            })
            .map(|target| {
                (
                    target.session_id,
                    target.sender.clone(),
                    Arc::clone(&target.active_queue_accounting),
                )
            });
        match target_session {
            Some((_target_session_id, sender, target_accounting))
                if !state.offline_queues.contains_key(&target_peer_id) =>
            {
                match try_push_active_v1(
                    &sender,
                    delivery,
                    accounted_bytes,
                    &target_accounting,
                    &state.active_queue_accounting,
                    &self.config,
                ) {
                    Ok(()) => {
                        state.counters.forwarded_frame_total =
                            state.counters.forwarded_frame_total.saturating_add(1);
                        data_outcome_v1(
                            RelayForwardDispositionV1::Forwarded,
                            &source_peer_id,
                            &target_peer_id,
                            envelope_session_id,
                            envelope_sequence,
                            wire_bytes,
                        )
                    }
                    Err(error) => {
                        if error.kind == RelayActiveQueuePushErrorKindV1::CountLimit {
                            state.counters.active_queue_count_limited_frame_total = state
                                .counters
                                .active_queue_count_limited_frame_total
                                .saturating_add(1);
                        }
                        if error.kind == RelayActiveQueuePushErrorKindV1::ByteLimit {
                            state.counters.active_queue_byte_limited_frame_total = state
                                .counters
                                .active_queue_byte_limited_frame_total
                                .saturating_add(1);
                        }
                        if error.kind == RelayActiveQueuePushErrorKindV1::Closed {
                            state.sessions.remove(&target_peer_id);
                        }
                        let queued_disposition =
                            if error.kind == RelayActiveQueuePushErrorKindV1::Closed {
                                RelayForwardDispositionV1::QueuedTargetOffline
                            } else {
                                RelayForwardDispositionV1::QueuedBackpressure
                            };
                        finish_offline_data_v1(
                            &mut state,
                            &self.config,
                            error.item,
                            error.accounted_bytes,
                            queued_disposition,
                            RelayDataOutcomeMetadataV1 {
                                envelope_session_id,
                                envelope_sequence,
                                wire_bytes,
                            },
                            now_ms,
                        )
                    }
                }
            }
            Some((_target_session_id, _sender, _target_accounting)) => finish_offline_data_v1(
                &mut state,
                &self.config,
                delivery,
                accounted_bytes,
                RelayForwardDispositionV1::QueuedBackpressure,
                RelayDataOutcomeMetadataV1 {
                    envelope_session_id,
                    envelope_sequence,
                    wire_bytes,
                },
                now_ms,
            ),
            None => finish_offline_data_v1(
                &mut state,
                &self.config,
                delivery,
                accounted_bytes,
                RelayForwardDispositionV1::QueuedTargetOffline,
                RelayDataOutcomeMetadataV1 {
                    envelope_session_id,
                    envelope_sequence,
                    wire_bytes,
                },
                now_ms,
            ),
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
        let wire_bytes = serde_json::to_vec(&ProductRelayWireMessageV1::PeerHandshake {
            target_peer_id: target_peer_id.to_string(),
            handshake: handshake.clone(),
        })
        .map_or(usize::MAX, |wire| wire.len());
        self.forward_peer_handshake_with_wire_bytes(
            source_peer_id,
            source_session_id,
            target_peer_id,
            handshake,
            wire_bytes,
            now_ms,
        )
        .await
    }

    pub async fn forward_peer_handshake_with_wire_bytes(
        &self,
        source_peer_id: &str,
        source_session_id: [u8; 16],
        target_peer_id: &str,
        handshake: RelayPeerHandshakeV1,
        wire_bytes: usize,
        now_ms: u64,
    ) -> RelayForwardOutcomeV1 {
        let admission = match self
            .admit_authenticated_wire_v1(source_peer_id, source_session_id, wire_bytes, now_ms)
            .await
        {
            Ok(admission) => admission,
            Err(disposition) => {
                return outcome_with_wire_v1(
                    disposition,
                    source_peer_id,
                    target_peer_id,
                    wire_bytes,
                )
            }
        };
        self.forward_peer_handshake_admitted_v1(admission, target_peer_id, handshake, now_ms)
            .await
    }

    pub async fn forward_peer_handshake_admitted_v1(
        &self,
        admission: RelayIngressAdmissionV1,
        target_peer_id: &str,
        handshake: RelayPeerHandshakeV1,
        now_ms: u64,
    ) -> RelayForwardOutcomeV1 {
        let source_peer_id = admission.source_peer_id.clone();
        let wire_bytes = admission.wire_bytes;
        let mut state = self.state.write().await;
        if !self.accepting.load(Ordering::Acquire) {
            state.counters.shutdown_rejection_total =
                state.counters.shutdown_rejection_total.saturating_add(1);
            record_rejected_wire_v1(&mut state, wire_bytes);
            return outcome_with_wire_v1(
                RelayForwardDispositionV1::RejectedShuttingDown,
                &source_peer_id,
                target_peer_id,
                wire_bytes,
            );
        }
        if let Err(disposition) =
            validate_admitted_source_v1(&mut state, &self.config, &admission, now_ms)
        {
            return outcome_with_wire_v1(disposition, &source_peer_id, target_peer_id, wire_bytes);
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
            state.counters.rejected_frame_total =
                state.counters.rejected_frame_total.saturating_add(1);
            state.counters.rejected_wire_bytes_total = state
                .counters
                .rejected_wire_bytes_total
                .saturating_add(wire_bytes as u64);
            return outcome_with_wire_v1(
                RelayForwardDispositionV1::RejectedRouteMismatch,
                &source_peer_id,
                target_peer_id,
                wire_bytes,
            );
        }
        let delivery = RelayPeerHandshakeDeliveryV1 {
            source_peer_id: source_peer_id.clone(),
            target_peer_id: target_peer_id.to_string(),
            received_at_ms: now_ms,
            handshake,
        };
        let accounted_bytes = serde_json::to_vec(
            &ProductRelayWireMessageV1::PeerHandshakeDelivery(delivery.clone()),
        )
        .map_or(usize::MAX, |wire| wire.len());
        if accounted_bytes > PRODUCT_RELAY_MAX_WIRE_MESSAGE_BYTES_V1 {
            state.counters.wire_message_too_large_total = state
                .counters
                .wire_message_too_large_total
                .saturating_add(1);
            record_rejected_wire_v1(&mut state, wire_bytes);
            return outcome_with_wire_v1(
                RelayForwardDispositionV1::RejectedWireMessageTooLarge,
                &source_peer_id,
                target_peer_id,
                wire_bytes,
            );
        }
        expire_session_if_stale_v1(&mut state, &self.config, target_peer_id, now_ms);
        let target_session = state
            .sessions
            .get(target_peer_id)
            .filter(|target| {
                now_ms.saturating_sub(target.last_seen_ms) <= self.config.session_ttl_ms
            })
            .map(|target| {
                (
                    target.control_sender.clone(),
                    Arc::clone(&target.active_queue_accounting),
                )
            });
        match target_session {
            Some((sender, target_accounting))
                if !state.offline_queues.contains_key(target_peer_id) =>
            {
                match try_push_active_v1(
                    &sender,
                    delivery,
                    accounted_bytes,
                    &target_accounting,
                    &state.active_queue_accounting,
                    &self.config,
                ) {
                    Ok(()) => {
                        state.counters.forwarded_frame_total =
                            state.counters.forwarded_frame_total.saturating_add(1);
                        outcome_with_wire_v1(
                            RelayForwardDispositionV1::Forwarded,
                            &source_peer_id,
                            target_peer_id,
                            wire_bytes,
                        )
                    }
                    Err(error) => {
                        if error.kind == RelayActiveQueuePushErrorKindV1::CountLimit {
                            state.counters.active_queue_count_limited_frame_total = state
                                .counters
                                .active_queue_count_limited_frame_total
                                .saturating_add(1);
                        }
                        if error.kind == RelayActiveQueuePushErrorKindV1::ByteLimit {
                            state.counters.active_queue_byte_limited_frame_total = state
                                .counters
                                .active_queue_byte_limited_frame_total
                                .saturating_add(1);
                        }
                        if error.kind == RelayActiveQueuePushErrorKindV1::Closed {
                            state.sessions.remove(target_peer_id);
                        }
                        let queued_disposition =
                            if error.kind == RelayActiveQueuePushErrorKindV1::Closed {
                                RelayForwardDispositionV1::QueuedTargetOffline
                            } else {
                                RelayForwardDispositionV1::QueuedBackpressure
                            };
                        finish_offline_control_v1(
                            &mut state,
                            &self.config,
                            error.item,
                            error.accounted_bytes,
                            queued_disposition,
                            wire_bytes,
                            now_ms,
                        )
                    }
                }
            }
            Some((_sender, _target_accounting)) => finish_offline_control_v1(
                &mut state,
                &self.config,
                delivery,
                accounted_bytes,
                RelayForwardDispositionV1::QueuedBackpressure,
                wire_bytes,
                now_ms,
            ),
            None => finish_offline_control_v1(
                &mut state,
                &self.config,
                delivery,
                accounted_bytes,
                RelayForwardDispositionV1::QueuedTargetOffline,
                wire_bytes,
                now_ms,
            ),
        }
    }

    pub async fn heartbeat(&self, peer_id: &str, session_id: [u8; 16], now_ms: u64) -> bool {
        let wire_bytes = serde_json::to_vec(&ProductRelayWireMessageV1::Heartbeat)
            .map_or(usize::MAX, |wire| wire.len());
        self.heartbeat_with_wire_bytes(peer_id, session_id, wire_bytes, now_ms)
            .await
    }

    pub async fn heartbeat_with_wire_bytes(
        &self,
        peer_id: &str,
        session_id: [u8; 16],
        wire_bytes: usize,
        now_ms: u64,
    ) -> bool {
        self.admit_authenticated_wire_v1(peer_id, session_id, wire_bytes, now_ms)
            .await
            .is_ok()
    }

    pub async fn heartbeat_admitted_v1(
        &self,
        admission: RelayIngressAdmissionV1,
        now_ms: u64,
    ) -> bool {
        let mut state = self.state.write().await;
        if !self.accepting.load(Ordering::Acquire) {
            state.counters.shutdown_rejection_total =
                state.counters.shutdown_rejection_total.saturating_add(1);
            record_rejected_wire_v1(&mut state, admission.wire_bytes);
            return false;
        }
        validate_admitted_source_v1(&mut state, &self.config, &admission, now_ms).is_ok()
    }

    pub async fn ping_with_wire_bytes(
        &self,
        peer_id: &str,
        session_id: [u8; 16],
        wire_bytes: usize,
        now_ms: u64,
    ) -> bool {
        self.heartbeat_with_wire_bytes(peer_id, session_id, wire_bytes, now_ms)
            .await
    }

    pub async fn is_current_session(
        &self,
        peer_id: &str,
        session_id: [u8; 16],
        now_ms: u64,
    ) -> bool {
        let mut state = self.state.write().await;
        expire_session_if_stale_v1(&mut state, &self.config, peer_id, now_ms);
        state
            .sessions
            .get(peer_id)
            .is_some_and(|session| session.session_id == session_id)
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
        let expired = prune_stale_sessions_locked_v1(&mut state, &self.config, now_ms);
        prune_expired_offline_v1(&mut state, &self.config, now_ms);
        prune_inactive_source_budgets_v1(&mut state, &self.config, now_ms);
        expired
    }

    pub fn begin_graceful_shutdown(&self) {
        self.accepting.store(false, Ordering::Release);
    }

    pub async fn finish_graceful_shutdown(&self) -> RelayRuntimeSnapshotV1 {
        self.accepting.store(false, Ordering::Release);
        let mut state = self.state.write().await;
        state.sessions.clear();
        snapshot_v1(&state, &self.config, false)
    }

    pub async fn snapshot(&self) -> RelayRuntimeSnapshotV1 {
        let state = self.state.read().await;
        snapshot_v1(&state, &self.config, self.accepting.load(Ordering::Acquire))
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
        self.receiver
            .recv()
            .await
            .map(RelayActiveQueueItemV1::into_inner)
    }

    pub fn try_recv(&mut self) -> Result<OpaqueRelayDeliveryV1, mpsc::error::TryRecvError> {
        self.receiver
            .try_recv()
            .map(RelayActiveQueueItemV1::into_inner)
    }

    pub async fn recv_peer_handshake(&mut self) -> Option<RelayPeerHandshakeDeliveryV1> {
        self.control_receiver
            .recv()
            .await
            .map(RelayActiveQueueItemV1::into_inner)
    }

    pub fn try_recv_peer_handshake(
        &mut self,
    ) -> Result<RelayPeerHandshakeDeliveryV1, mpsc::error::TryRecvError> {
        self.control_receiver
            .try_recv()
            .map(RelayActiveQueueItemV1::into_inner)
    }
}

fn validate_config_v1(
    config: &ProductRelayRuntimeConfigV1,
) -> Result<(), ProductRelayRuntimeErrorV1> {
    if config.max_sessions == 0
        || config.max_tracked_sources < config.max_sessions
        || config.session_queue_capacity == 0
        || config.session_queue_bytes == 0
        || config.active_queue_total == 0
        || config.active_queue_bytes_total == 0
    {
        return Err(ProductRelayRuntimeErrorV1::InvalidConfiguration(
            "session limits must be positive",
        ));
    }
    if config.session_queue_capacity > config.active_queue_total
        || config.session_queue_bytes > config.active_queue_bytes_total
    {
        return Err(ProductRelayRuntimeErrorV1::InvalidConfiguration(
            "per-session active queue limits must not exceed active queue totals",
        ));
    }
    if config.offline_queue_per_peer == 0
        || config.offline_queue_bytes_per_peer == 0
        || config.offline_queue_per_source == 0
        || config.offline_queue_bytes_per_source == 0
        || config.offline_queue_total == 0
        || config.offline_queue_bytes_total == 0
        || config.offline_queue_ttl_ms == 0
    {
        return Err(ProductRelayRuntimeErrorV1::InvalidConfiguration(
            "offline queue limits must be positive",
        ));
    }
    if config.session_ttl_ms == 0
        || config.rate_limit_frames == 0
        || config.max_frames_per_window == 0
        || config.rate_limit_window_ms == 0
        || config.source_bytes_per_minute == 0
        || config.max_bytes_per_minute == 0
    {
        return Err(ProductRelayRuntimeErrorV1::InvalidConfiguration(
            "session and rate limits must be positive",
        ));
    }
    if config.offline_queue_per_peer > config.offline_queue_total
        || config.offline_queue_per_source > config.offline_queue_total
        || config.offline_queue_bytes_per_peer > config.offline_queue_bytes_total
        || config.offline_queue_bytes_per_source > config.offline_queue_bytes_total
    {
        return Err(ProductRelayRuntimeErrorV1::InvalidConfiguration(
            "offline per-peer and per-source limits must not exceed totals",
        ));
    }
    if config.source_bytes_per_minute > config.max_bytes_per_minute {
        return Err(ProductRelayRuntimeErrorV1::InvalidConfiguration(
            "source byte limit must not exceed aggregate byte limit",
        ));
    }
    if config.rate_limit_frames > config.max_frames_per_window {
        return Err(ProductRelayRuntimeErrorV1::InvalidConfiguration(
            "source frame limit must not exceed aggregate frame limit",
        ));
    }
    if config
        .active_queue_total
        .checked_add(config.offline_queue_total)
        .is_none()
        || config
            .active_queue_bytes_total
            .checked_add(config.offline_queue_bytes_total)
            .is_none()
    {
        return Err(ProductRelayRuntimeErrorV1::InvalidConfiguration(
            "aggregate active and offline queue limits overflow usize",
        ));
    }
    Ok(())
}

impl RelaySourceBudgetV1 {
    fn new(now_ms: u64) -> Self {
        Self {
            frame_window_started_ms: now_ms,
            frame_count: 0,
            byte_window_started_ms: now_ms,
            byte_count: 0,
            last_activity_ms: now_ms,
        }
    }
}

impl<T> RelayActiveQueueItemV1<T> {
    fn into_inner(mut self) -> T {
        let item = self.item.take().expect("active relay queue item missing");
        self.release();
        item
    }

    fn into_parts(mut self) -> (T, usize) {
        let item = self.item.take().expect("active relay queue item missing");
        let accounted_bytes = self.accounted_bytes;
        self.release();
        (item, accounted_bytes)
    }

    fn release(&mut self) {
        if self.released {
            return;
        }
        release_active_accounting_v1(&self.session_accounting, self.accounted_bytes);
        release_active_accounting_v1(&self.global_accounting, self.accounted_bytes);
        self.released = true;
    }
}

impl<T> Drop for RelayActiveQueueItemV1<T> {
    fn drop(&mut self) {
        self.release();
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum RelayActiveQueuePushErrorKindV1 {
    CountLimit,
    ByteLimit,
    Full,
    Closed,
}

struct RelayActiveQueuePushErrorV1<T> {
    kind: RelayActiveQueuePushErrorKindV1,
    item: T,
    accounted_bytes: usize,
}

#[derive(Debug, Clone, Copy)]
struct RelayDataOutcomeMetadataV1 {
    envelope_session_id: [u8; 16],
    envelope_sequence: u64,
    wire_bytes: usize,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum RelayOfflineQueueLimitV1 {
    Peer,
    Source,
    Total,
}

fn admit_source_frame_v1(
    state: &mut RelayRuntimeStateV1,
    config: &ProductRelayRuntimeConfigV1,
    source_peer_id: &str,
    source_session_id: [u8; 16],
    wire_bytes: usize,
    now_ms: u64,
) -> Result<(), RelayForwardDispositionV1> {
    if wire_bytes > PRODUCT_RELAY_MAX_WIRE_MESSAGE_BYTES_V1 {
        state.counters.wire_message_too_large_total = state
            .counters
            .wire_message_too_large_total
            .saturating_add(1);
        record_rejected_wire_v1(state, wire_bytes);
        return Err(RelayForwardDispositionV1::RejectedWireMessageTooLarge);
    }
    let Some(source) = state.sessions.get(source_peer_id) else {
        record_rejected_wire_v1(state, wire_bytes);
        return Err(RelayForwardDispositionV1::RejectedSourceSessionMissing);
    };
    if source.session_id != source_session_id {
        record_rejected_wire_v1(state, wire_bytes);
        return Err(RelayForwardDispositionV1::RejectedStaleSourceSession);
    }
    if now_ms.saturating_sub(source.last_seen_ms) > config.session_ttl_ms {
        state.sessions.remove(source_peer_id);
        state.counters.expired_session_total =
            state.counters.expired_session_total.saturating_add(1);
        record_rejected_wire_v1(state, wire_bytes);
        return Err(RelayForwardDispositionV1::RejectedSourceSessionExpired);
    }

    if now_ms.saturating_sub(state.aggregate_frame_budget.window_started_ms)
        >= config.rate_limit_window_ms
    {
        state.aggregate_frame_budget.window_started_ms = now_ms;
        state.aggregate_frame_budget.frame_count = 0;
    }
    if now_ms.saturating_sub(state.aggregate_byte_budget.window_started_ms)
        >= PRODUCT_RELAY_BYTE_RATE_WINDOW_MS_V1
    {
        state.aggregate_byte_budget.window_started_ms = now_ms;
        state.aggregate_byte_budget.byte_count = 0;
    }
    if !state.source_budgets.contains_key(source_peer_id)
        && ensure_source_budget_v1(state, config, source_peer_id, now_ms).is_err()
    {
        state.counters.rate_limited_frame_total =
            state.counters.rate_limited_frame_total.saturating_add(1);
        record_rejected_wire_v1(state, wire_bytes);
        return Err(RelayForwardDispositionV1::RejectedRateLimited);
    }
    let budget = state
        .source_budgets
        .get_mut(source_peer_id)
        .expect("registered relay session source budget missing");
    if now_ms.saturating_sub(budget.frame_window_started_ms) >= config.rate_limit_window_ms {
        budget.frame_window_started_ms = now_ms;
        budget.frame_count = 0;
    }
    if now_ms.saturating_sub(budget.byte_window_started_ms) >= PRODUCT_RELAY_BYTE_RATE_WINDOW_MS_V1
    {
        budget.byte_window_started_ms = now_ms;
        budget.byte_count = 0;
    }
    budget.last_activity_ms = now_ms;
    let wire_bytes_u64 = wire_bytes as u64;
    if budget.frame_count >= config.rate_limit_frames {
        state.counters.rate_limited_frame_total =
            state.counters.rate_limited_frame_total.saturating_add(1);
        record_rejected_wire_v1(state, wire_bytes);
        return Err(RelayForwardDispositionV1::RejectedRateLimited);
    }
    if state.aggregate_frame_budget.frame_count >= config.max_frames_per_window {
        state.counters.rate_limited_frame_total =
            state.counters.rate_limited_frame_total.saturating_add(1);
        state.counters.aggregate_rate_limited_frame_total = state
            .counters
            .aggregate_rate_limited_frame_total
            .saturating_add(1);
        record_rejected_wire_v1(state, wire_bytes);
        return Err(RelayForwardDispositionV1::RejectedAggregateRateLimited);
    }
    if budget.byte_count.saturating_add(wire_bytes_u64) > config.source_bytes_per_minute {
        state.counters.rate_limited_frame_total =
            state.counters.rate_limited_frame_total.saturating_add(1);
        state.counters.source_byte_limited_frame_total = state
            .counters
            .source_byte_limited_frame_total
            .saturating_add(1);
        record_rejected_wire_v1(state, wire_bytes);
        return Err(RelayForwardDispositionV1::RejectedSourceByteLimited);
    }
    if state
        .aggregate_byte_budget
        .byte_count
        .saturating_add(wire_bytes_u64)
        > config.max_bytes_per_minute
    {
        state.counters.rate_limited_frame_total =
            state.counters.rate_limited_frame_total.saturating_add(1);
        state.counters.aggregate_byte_limited_frame_total = state
            .counters
            .aggregate_byte_limited_frame_total
            .saturating_add(1);
        record_rejected_wire_v1(state, wire_bytes);
        return Err(RelayForwardDispositionV1::RejectedAggregateByteLimited);
    }

    budget.frame_count = budget.frame_count.saturating_add(1);
    budget.byte_count = budget.byte_count.saturating_add(wire_bytes_u64);
    state.aggregate_byte_budget.byte_count = state
        .aggregate_byte_budget
        .byte_count
        .saturating_add(wire_bytes_u64);
    state.aggregate_frame_budget.frame_count =
        state.aggregate_frame_budget.frame_count.saturating_add(1);
    state.counters.admitted_wire_bytes_total = state
        .counters
        .admitted_wire_bytes_total
        .saturating_add(wire_bytes_u64);
    if let Some(source) = state.sessions.get_mut(source_peer_id) {
        source.last_seen_ms = now_ms;
    }
    Ok(())
}

fn validate_admitted_source_v1(
    state: &mut RelayRuntimeStateV1,
    config: &ProductRelayRuntimeConfigV1,
    admission: &RelayIngressAdmissionV1,
    now_ms: u64,
) -> Result<(), RelayForwardDispositionV1> {
    let Some(source) = state.sessions.get(&admission.source_peer_id) else {
        record_rejected_wire_v1(state, admission.wire_bytes);
        return Err(RelayForwardDispositionV1::RejectedSourceSessionMissing);
    };
    if source.session_id != admission.source_session_id {
        record_rejected_wire_v1(state, admission.wire_bytes);
        return Err(RelayForwardDispositionV1::RejectedStaleSourceSession);
    }
    if now_ms.saturating_sub(source.last_seen_ms) > config.session_ttl_ms {
        state.sessions.remove(&admission.source_peer_id);
        state.counters.expired_session_total =
            state.counters.expired_session_total.saturating_add(1);
        record_rejected_wire_v1(state, admission.wire_bytes);
        return Err(RelayForwardDispositionV1::RejectedSourceSessionExpired);
    }
    Ok(())
}

fn record_rejected_wire_v1(state: &mut RelayRuntimeStateV1, wire_bytes: usize) {
    state.counters.rejected_frame_total = state.counters.rejected_frame_total.saturating_add(1);
    state.counters.rejected_wire_bytes_total = state
        .counters
        .rejected_wire_bytes_total
        .saturating_add(wire_bytes as u64);
}

fn expire_session_if_stale_v1(
    state: &mut RelayRuntimeStateV1,
    config: &ProductRelayRuntimeConfigV1,
    peer_id: &str,
    now_ms: u64,
) {
    let stale = state
        .sessions
        .get(peer_id)
        .is_some_and(|session| now_ms.saturating_sub(session.last_seen_ms) > config.session_ttl_ms);
    if stale {
        state.sessions.remove(peer_id);
        state.counters.expired_session_total =
            state.counters.expired_session_total.saturating_add(1);
    }
}

fn prune_stale_sessions_locked_v1(
    state: &mut RelayRuntimeStateV1,
    config: &ProductRelayRuntimeConfigV1,
    now_ms: u64,
) -> usize {
    let expired_peer_ids = state
        .sessions
        .iter()
        .filter(|(_, session)| now_ms.saturating_sub(session.last_seen_ms) > config.session_ttl_ms)
        .map(|(peer_id, _)| peer_id.clone())
        .collect::<Vec<_>>();
    for peer_id in &expired_peer_ids {
        state.sessions.remove(peer_id);
    }
    state.counters.expired_session_total = state
        .counters
        .expired_session_total
        .saturating_add(expired_peer_ids.len() as u64);
    expired_peer_ids.len()
}

fn try_push_active_v1<T>(
    sender: &mpsc::Sender<RelayActiveQueueItemV1<T>>,
    item: T,
    accounted_bytes: usize,
    session_accounting: &Arc<RelayActiveQueueAccountingV1>,
    global_accounting: &Arc<RelayActiveQueueAccountingV1>,
    config: &ProductRelayRuntimeConfigV1,
) -> Result<(), RelayActiveQueuePushErrorV1<T>> {
    let aggregate_frame_limit = config.active_queue_total;
    let aggregate_byte_limit = config.active_queue_bytes_total;
    if !try_reserve_active_units_v1(&session_accounting.frames, 1, config.session_queue_capacity) {
        return Err(RelayActiveQueuePushErrorV1 {
            kind: RelayActiveQueuePushErrorKindV1::CountLimit,
            item,
            accounted_bytes,
        });
    }
    if !try_reserve_active_units_v1(&global_accounting.frames, 1, aggregate_frame_limit) {
        session_accounting.frames.fetch_sub(1, Ordering::AcqRel);
        return Err(RelayActiveQueuePushErrorV1 {
            kind: RelayActiveQueuePushErrorKindV1::CountLimit,
            item,
            accounted_bytes,
        });
    }
    if !try_reserve_active_units_v1(
        &session_accounting.bytes,
        accounted_bytes,
        config.session_queue_bytes,
    ) {
        session_accounting.frames.fetch_sub(1, Ordering::AcqRel);
        global_accounting.frames.fetch_sub(1, Ordering::AcqRel);
        return Err(RelayActiveQueuePushErrorV1 {
            kind: RelayActiveQueuePushErrorKindV1::ByteLimit,
            item,
            accounted_bytes,
        });
    }
    if !try_reserve_active_units_v1(
        &global_accounting.bytes,
        accounted_bytes,
        aggregate_byte_limit,
    ) {
        session_accounting
            .bytes
            .fetch_sub(accounted_bytes, Ordering::AcqRel);
        session_accounting.frames.fetch_sub(1, Ordering::AcqRel);
        global_accounting.frames.fetch_sub(1, Ordering::AcqRel);
        return Err(RelayActiveQueuePushErrorV1 {
            kind: RelayActiveQueuePushErrorKindV1::ByteLimit,
            item,
            accounted_bytes,
        });
    }
    let accounted = RelayActiveQueueItemV1 {
        item: Some(item),
        accounted_bytes,
        session_accounting: Arc::clone(session_accounting),
        global_accounting: Arc::clone(global_accounting),
        released: false,
    };
    match sender.try_send(accounted) {
        Ok(()) => Ok(()),
        Err(mpsc::error::TrySendError::Full(accounted)) => {
            let (item, accounted_bytes) = accounted.into_parts();
            Err(RelayActiveQueuePushErrorV1 {
                kind: RelayActiveQueuePushErrorKindV1::Full,
                item,
                accounted_bytes,
            })
        }
        Err(mpsc::error::TrySendError::Closed(accounted)) => {
            let (item, accounted_bytes) = accounted.into_parts();
            Err(RelayActiveQueuePushErrorV1 {
                kind: RelayActiveQueuePushErrorKindV1::Closed,
                item,
                accounted_bytes,
            })
        }
    }
}

fn try_reserve_active_units_v1(counter: &AtomicUsize, amount: usize, limit: usize) -> bool {
    let mut current = counter.load(Ordering::Acquire);
    loop {
        let Some(next) = current.checked_add(amount) else {
            return false;
        };
        if next > limit {
            return false;
        }
        match counter.compare_exchange_weak(current, next, Ordering::AcqRel, Ordering::Acquire) {
            Ok(_) => return true,
            Err(observed) => current = observed,
        }
    }
}

fn release_active_accounting_v1(accounting: &RelayActiveQueueAccountingV1, accounted_bytes: usize) {
    let previous_frames = accounting.frames.fetch_sub(1, Ordering::AcqRel);
    let previous_bytes = accounting
        .bytes
        .fetch_sub(accounted_bytes, Ordering::AcqRel);
    debug_assert!(previous_frames >= 1);
    debug_assert!(previous_bytes >= accounted_bytes);
}

impl RelayOfflineQueueItemV1 {
    fn data(delivery: OpaqueRelayDeliveryV1, active_accounted_bytes: usize) -> Self {
        let offline_accounted_bytes = offline_memory_accounted_bytes_v1(
            active_accounted_bytes,
            delivery.source_peer_id.len(),
            delivery.target_peer_id.len(),
        );
        Self {
            source_peer_id: delivery.source_peer_id.clone(),
            target_peer_id: delivery.target_peer_id.clone(),
            received_at_ms: delivery.received_at_ms,
            active_accounted_bytes,
            offline_accounted_bytes,
            message: RelayOfflineMessageV1::Data(delivery),
        }
    }

    fn control(delivery: RelayPeerHandshakeDeliveryV1, active_accounted_bytes: usize) -> Self {
        let offline_accounted_bytes = offline_memory_accounted_bytes_v1(
            active_accounted_bytes,
            delivery.source_peer_id.len(),
            delivery.target_peer_id.len(),
        );
        Self {
            source_peer_id: delivery.source_peer_id.clone(),
            target_peer_id: delivery.target_peer_id.clone(),
            received_at_ms: delivery.received_at_ms,
            active_accounted_bytes,
            offline_accounted_bytes,
            message: RelayOfflineMessageV1::Control(delivery),
        }
    }
}

fn offline_memory_accounted_bytes_v1(
    delivery_wire_bytes: usize,
    source_peer_id_bytes: usize,
    target_peer_id_bytes: usize,
) -> usize {
    // The serialized delivery already upper-bounds its owned string/payload lengths. Offline
    // ownership additionally clones source/target into the queue item and may create one source
    // usage key plus target usage/queue keys. Charge those possible allocations on every item;
    // this deliberately over-accounts shared map keys so the declared byte caps remain hard.
    delivery_wire_bytes
        .saturating_add(source_peer_id_bytes.saturating_mul(2))
        .saturating_add(target_peer_id_bytes.saturating_mul(3))
        .saturating_add(std::mem::size_of::<RelayOfflineQueueItemV1>())
}

fn enqueue_offline_message_v1(
    state: &mut RelayRuntimeStateV1,
    config: &ProductRelayRuntimeConfigV1,
    item: RelayOfflineQueueItemV1,
    _now_ms: u64,
) -> Result<(), RelayOfflineQueueLimitV1> {
    // Keep authenticated enqueue admission O(1). Expiry is owned by the daemon's bounded
    // maintenance cadence and reconnect/drain paths; a full target queue may therefore retain
    // expired capacity for at most one maintenance interval instead of making every rejected
    // frame scan attacker-controlled queue depth while holding the global state lock.
    if let Some(rejection) = offline_capacity_rejection_v1(state, config, &item) {
        return Err(rejection);
    }
    let peer_usage = state
        .offline_usage_by_peer
        .entry(item.target_peer_id.clone())
        .or_default();
    peer_usage.count = peer_usage.count.saturating_add(1);
    peer_usage.bytes = peer_usage
        .bytes
        .saturating_add(item.offline_accounted_bytes);
    let source_usage = state
        .offline_usage_by_source
        .entry(item.source_peer_id.clone())
        .or_default();
    source_usage.count = source_usage.count.saturating_add(1);
    source_usage.bytes = source_usage
        .bytes
        .saturating_add(item.offline_accounted_bytes);
    state.offline_usage.count = state.offline_usage.count.saturating_add(1);
    state.offline_usage.bytes = state
        .offline_usage
        .bytes
        .saturating_add(item.offline_accounted_bytes);
    state
        .offline_queues
        .entry(item.target_peer_id.clone())
        .or_default()
        .push_back(item);
    Ok(())
}

fn offline_capacity_rejection_v1(
    state: &RelayRuntimeStateV1,
    config: &ProductRelayRuntimeConfigV1,
    item: &RelayOfflineQueueItemV1,
) -> Option<RelayOfflineQueueLimitV1> {
    let peer = state
        .offline_usage_by_peer
        .get(&item.target_peer_id)
        .copied()
        .unwrap_or_default();
    if peer.count.saturating_add(1) > config.offline_queue_per_peer
        || peer.bytes.saturating_add(item.offline_accounted_bytes)
            > config.offline_queue_bytes_per_peer
    {
        return Some(RelayOfflineQueueLimitV1::Peer);
    }
    let source = state
        .offline_usage_by_source
        .get(&item.source_peer_id)
        .copied()
        .unwrap_or_default();
    if source.count.saturating_add(1) > config.offline_queue_per_source
        || source.bytes.saturating_add(item.offline_accounted_bytes)
            > config.offline_queue_bytes_per_source
    {
        return Some(RelayOfflineQueueLimitV1::Source);
    }
    if state.offline_usage.count.saturating_add(1) > config.offline_queue_total
        || state
            .offline_usage
            .bytes
            .saturating_add(item.offline_accounted_bytes)
            > config.offline_queue_bytes_total
    {
        return Some(RelayOfflineQueueLimitV1::Total);
    }
    None
}

fn release_offline_usage_v1(
    state: &mut RelayRuntimeStateV1,
    source_peer_id: &str,
    target_peer_id: &str,
    accounted_bytes: usize,
) {
    decrement_queue_usage_v1(&mut state.offline_usage, accounted_bytes);
    decrement_queue_usage_entry_v1(
        &mut state.offline_usage_by_peer,
        target_peer_id,
        accounted_bytes,
    );
    decrement_queue_usage_entry_v1(
        &mut state.offline_usage_by_source,
        source_peer_id,
        accounted_bytes,
    );
}

fn drain_offline_queue_into_session_v1(
    state: &mut RelayRuntimeStateV1,
    config: &ProductRelayRuntimeConfigV1,
    peer_id: &str,
    now_ms: u64,
    sender: &mpsc::Sender<RelayActiveQueueItemV1<OpaqueRelayDeliveryV1>>,
    control_sender: &mpsc::Sender<RelayActiveQueueItemV1<RelayPeerHandshakeDeliveryV1>>,
    session_accounting: &Arc<RelayActiveQueueAccountingV1>,
) -> u64 {
    prune_expired_offline_peer_v1(state, config, peer_id, now_ms);
    let mut drained = 0u64;
    let mut remaining_queue = VecDeque::new();
    if let Some(mut queued) = state.offline_queues.remove(peer_id) {
        while let Some(item) = queued.pop_front() {
            let RelayOfflineQueueItemV1 {
                source_peer_id,
                target_peer_id,
                received_at_ms,
                active_accounted_bytes,
                offline_accounted_bytes,
                message,
            } = item;
            let pushed = match message {
                RelayOfflineMessageV1::Data(delivery) => try_push_active_v1(
                    sender,
                    delivery,
                    active_accounted_bytes,
                    session_accounting,
                    &state.active_queue_accounting,
                    config,
                )
                .map_err(|error| RelayOfflineMessageV1::Data(error.item)),
                RelayOfflineMessageV1::Control(delivery) => try_push_active_v1(
                    control_sender,
                    delivery,
                    active_accounted_bytes,
                    session_accounting,
                    &state.active_queue_accounting,
                    config,
                )
                .map_err(|error| RelayOfflineMessageV1::Control(error.item)),
            };
            match pushed {
                Ok(()) => {
                    release_offline_usage_v1(
                        state,
                        &source_peer_id,
                        &target_peer_id,
                        offline_accounted_bytes,
                    );
                    drained = drained.saturating_add(1);
                }
                Err(message) => {
                    remaining_queue.push_back(RelayOfflineQueueItemV1 {
                        source_peer_id,
                        target_peer_id,
                        received_at_ms,
                        active_accounted_bytes,
                        offline_accounted_bytes,
                        message,
                    });
                    remaining_queue.append(&mut queued);
                    break;
                }
            }
        }
    }
    if !remaining_queue.is_empty() {
        state
            .offline_queues
            .insert(peer_id.to_string(), remaining_queue);
    }
    drained
}

fn decrement_queue_usage_v1(usage: &mut RelayQueueUsageV1, accounted_bytes: usize) {
    debug_assert!(usage.count >= 1);
    debug_assert!(usage.bytes >= accounted_bytes);
    usage.count = usage.count.saturating_sub(1);
    usage.bytes = usage.bytes.saturating_sub(accounted_bytes);
}

fn decrement_queue_usage_entry_v1(
    usage: &mut BTreeMap<String, RelayQueueUsageV1>,
    key: &str,
    accounted_bytes: usize,
) {
    let remove = if let Some(current) = usage.get_mut(key) {
        decrement_queue_usage_v1(current, accounted_bytes);
        current.count == 0
    } else {
        debug_assert!(false, "relay queue accounting entry missing");
        false
    };
    if remove {
        usage.remove(key);
    }
}

fn prune_expired_offline_v1(
    state: &mut RelayRuntimeStateV1,
    config: &ProductRelayRuntimeConfigV1,
    now_ms: u64,
) {
    state.counters.offline_full_sweep_total =
        state.counters.offline_full_sweep_total.saturating_add(1);
    let peer_ids = state.offline_queues.keys().cloned().collect::<Vec<_>>();
    for peer_id in peer_ids {
        prune_expired_offline_peer_v1(state, config, &peer_id, now_ms);
    }
}

fn prune_expired_offline_peer_v1(
    state: &mut RelayRuntimeStateV1,
    config: &ProductRelayRuntimeConfigV1,
    peer_id: &str,
    now_ms: u64,
) {
    let Some(mut queue) = state.offline_queues.remove(peer_id) else {
        return;
    };
    let mut retained = VecDeque::new();
    while let Some(item) = queue.pop_front() {
        if now_ms.saturating_sub(item.received_at_ms) > config.offline_queue_ttl_ms {
            release_offline_usage_v1(
                state,
                &item.source_peer_id,
                &item.target_peer_id,
                item.offline_accounted_bytes,
            );
            state.counters.expired_queued_frame_total =
                state.counters.expired_queued_frame_total.saturating_add(1);
            state.counters.expired_queued_bytes_total = state
                .counters
                .expired_queued_bytes_total
                .saturating_add(item.offline_accounted_bytes as u64);
        } else {
            retained.push_back(item);
        }
    }
    if !retained.is_empty() {
        state.offline_queues.insert(peer_id.to_string(), retained);
    }
}

fn finish_offline_data_v1(
    state: &mut RelayRuntimeStateV1,
    config: &ProductRelayRuntimeConfigV1,
    delivery: OpaqueRelayDeliveryV1,
    accounted_bytes: usize,
    queued_disposition: RelayForwardDispositionV1,
    metadata: RelayDataOutcomeMetadataV1,
    now_ms: u64,
) -> RelayForwardOutcomeV1 {
    let source_peer_id = delivery.source_peer_id.clone();
    let target_peer_id = delivery.target_peer_id.clone();
    match enqueue_offline_message_v1(
        state,
        config,
        RelayOfflineQueueItemV1::data(delivery, accounted_bytes),
        now_ms,
    ) {
        Ok(()) => {
            state.counters.queued_frame_total = state.counters.queued_frame_total.saturating_add(1);
            data_outcome_v1(
                queued_disposition,
                &source_peer_id,
                &target_peer_id,
                metadata.envelope_session_id,
                metadata.envelope_sequence,
                metadata.wire_bytes,
            )
        }
        Err(limit) => {
            record_offline_limit_rejection_v1(state, limit, metadata.wire_bytes);
            data_outcome_v1(
                offline_limit_disposition_v1(limit),
                &source_peer_id,
                &target_peer_id,
                metadata.envelope_session_id,
                metadata.envelope_sequence,
                metadata.wire_bytes,
            )
        }
    }
}

fn finish_offline_control_v1(
    state: &mut RelayRuntimeStateV1,
    config: &ProductRelayRuntimeConfigV1,
    delivery: RelayPeerHandshakeDeliveryV1,
    accounted_bytes: usize,
    queued_disposition: RelayForwardDispositionV1,
    wire_bytes: usize,
    now_ms: u64,
) -> RelayForwardOutcomeV1 {
    let source_peer_id = delivery.source_peer_id.clone();
    let target_peer_id = delivery.target_peer_id.clone();
    match enqueue_offline_message_v1(
        state,
        config,
        RelayOfflineQueueItemV1::control(delivery, accounted_bytes),
        now_ms,
    ) {
        Ok(()) => {
            state.counters.queued_frame_total = state.counters.queued_frame_total.saturating_add(1);
            outcome_with_wire_v1(
                queued_disposition,
                &source_peer_id,
                &target_peer_id,
                wire_bytes,
            )
        }
        Err(limit) => {
            record_offline_limit_rejection_v1(state, limit, wire_bytes);
            outcome_with_wire_v1(
                offline_limit_disposition_v1(limit),
                &source_peer_id,
                &target_peer_id,
                wire_bytes,
            )
        }
    }
}

fn record_offline_limit_rejection_v1(
    state: &mut RelayRuntimeStateV1,
    limit: RelayOfflineQueueLimitV1,
    wire_bytes: usize,
) {
    match limit {
        RelayOfflineQueueLimitV1::Peer => {
            state.counters.offline_peer_limited_frame_total = state
                .counters
                .offline_peer_limited_frame_total
                .saturating_add(1)
        }
        RelayOfflineQueueLimitV1::Source => {
            state.counters.offline_source_limited_frame_total = state
                .counters
                .offline_source_limited_frame_total
                .saturating_add(1)
        }
        RelayOfflineQueueLimitV1::Total => {
            state.counters.offline_total_limited_frame_total = state
                .counters
                .offline_total_limited_frame_total
                .saturating_add(1)
        }
    }
    record_rejected_wire_v1(state, wire_bytes);
}

fn offline_limit_disposition_v1(limit: RelayOfflineQueueLimitV1) -> RelayForwardDispositionV1 {
    match limit {
        RelayOfflineQueueLimitV1::Peer => RelayForwardDispositionV1::RejectedQueuePeerLimit,
        RelayOfflineQueueLimitV1::Source => RelayForwardDispositionV1::RejectedQueueSourceLimit,
        RelayOfflineQueueLimitV1::Total => RelayForwardDispositionV1::RejectedQueueTotalLimit,
    }
}

fn outcome_with_wire_v1(
    disposition: RelayForwardDispositionV1,
    source_peer_id: &str,
    target_peer_id: &str,
    admitted_wire_bytes: usize,
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
        envelope_session_id: None,
        envelope_sequence: None,
        admitted_wire_bytes,
    }
}

fn data_outcome_v1(
    disposition: RelayForwardDispositionV1,
    source_peer_id: &str,
    target_peer_id: &str,
    envelope_session_id: [u8; 16],
    envelope_sequence: u64,
    admitted_wire_bytes: usize,
) -> RelayForwardOutcomeV1 {
    let mut outcome = outcome_with_wire_v1(
        disposition,
        source_peer_id,
        target_peer_id,
        admitted_wire_bytes,
    );
    outcome.envelope_session_id = Some(envelope_session_id);
    outcome.envelope_sequence = Some(envelope_sequence);
    outcome
}

fn prune_inactive_source_budgets_v1(
    state: &mut RelayRuntimeStateV1,
    config: &ProductRelayRuntimeConfigV1,
    now_ms: u64,
) {
    let retention_ms = PRODUCT_RELAY_BYTE_RATE_WINDOW_MS_V1
        .max(config.rate_limit_window_ms)
        .max(config.session_ttl_ms);
    let stale = state
        .source_budgets
        .iter()
        .filter(|(peer_id, budget)| {
            !state.sessions.contains_key(*peer_id)
                && now_ms.saturating_sub(budget.last_activity_ms) > retention_ms
        })
        .map(|(peer_id, _)| peer_id.clone())
        .collect::<Vec<_>>();
    for peer_id in stale {
        state.source_budgets.remove(&peer_id);
    }
}

fn ensure_source_budget_v1(
    state: &mut RelayRuntimeStateV1,
    config: &ProductRelayRuntimeConfigV1,
    peer_id: &str,
    now_ms: u64,
) -> Result<(), ProductRelayRuntimeErrorV1> {
    if state.source_budgets.contains_key(peer_id) {
        return Ok(());
    }
    prune_inactive_source_budgets_v1(state, config, now_ms);
    while state.source_budgets.len() >= config.max_tracked_sources {
        let oldest_inactive = state
            .source_budgets
            .iter()
            .filter(|(candidate_peer_id, _)| !state.sessions.contains_key(*candidate_peer_id))
            .min_by(|(left_peer_id, left), (right_peer_id, right)| {
                left.last_activity_ms
                    .cmp(&right.last_activity_ms)
                    .then_with(|| left_peer_id.cmp(right_peer_id))
            })
            .map(|(candidate_peer_id, _)| candidate_peer_id.clone());
        let Some(oldest_inactive) = oldest_inactive else {
            return Err(ProductRelayRuntimeErrorV1::SourceTrackingLimitReached {
                max_tracked_sources: config.max_tracked_sources,
            });
        };
        state.source_budgets.remove(&oldest_inactive);
    }
    state
        .source_budgets
        .insert(peer_id.to_string(), RelaySourceBudgetV1::new(now_ms));
    Ok(())
}

fn snapshot_v1(
    state: &RelayRuntimeStateV1,
    config: &ProductRelayRuntimeConfigV1,
    accepting_new_work: bool,
) -> RelayRuntimeSnapshotV1 {
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
            pending_control_capacity: session.control_sender.capacity(),
            queued_frame_count: session
                .active_queue_accounting
                .frames
                .load(Ordering::Acquire),
            queued_bytes: session
                .active_queue_accounting
                .bytes
                .load(Ordering::Acquire),
        })
        .collect();
    let active_queued_frame_count = state.active_queue_accounting.frames.load(Ordering::Acquire);
    let active_queued_bytes = state.active_queue_accounting.bytes.load(Ordering::Acquire);
    RelayRuntimeSnapshotV1 {
        accepting_new_work,
        active_session_count: active_peer_ids.len(),
        tracked_source_count: state.source_budgets.len(),
        active_peer_ids,
        active_sessions,
        queued_frame_count: active_queued_frame_count.saturating_add(state.offline_usage.count),
        queued_bytes: active_queued_bytes.saturating_add(state.offline_usage.bytes),
        active_queued_frame_count,
        active_queued_bytes,
        offline_queued_frame_count: state.offline_usage.count,
        offline_queued_bytes: state.offline_usage.bytes,
        limits: RelayRuntimeLimitsSnapshotV1 {
            max_sessions: config.max_sessions,
            max_tracked_sources: config.max_tracked_sources,
            session_queue_capacity: config.session_queue_capacity,
            session_queue_bytes: config.session_queue_bytes,
            active_queue_total: config.active_queue_total,
            active_queue_bytes_total: config.active_queue_bytes_total,
            offline_queue_per_peer: config.offline_queue_per_peer,
            offline_queue_bytes_per_peer: config.offline_queue_bytes_per_peer,
            offline_queue_per_source: config.offline_queue_per_source,
            offline_queue_bytes_per_source: config.offline_queue_bytes_per_source,
            offline_queue_total: config.offline_queue_total,
            offline_queue_bytes_total: config.offline_queue_bytes_total,
            offline_queue_ttl_ms: config.offline_queue_ttl_ms,
            session_ttl_ms: config.session_ttl_ms,
            rate_limit_frames: config.rate_limit_frames,
            max_frames_per_window: config.max_frames_per_window,
            rate_limit_window_ms: config.rate_limit_window_ms,
            source_bytes_per_minute: config.source_bytes_per_minute,
            max_bytes_per_minute: config.max_bytes_per_minute,
            max_wire_message_bytes: PRODUCT_RELAY_MAX_WIRE_MESSAGE_BYTES_V1,
        },
        registered_session_total: state.counters.registered_session_total,
        session_limit_rejection_total: state.counters.session_limit_rejection_total,
        shutdown_rejection_total: state.counters.shutdown_rejection_total,
        replaced_session_total: state.counters.replaced_session_total,
        disconnected_session_total: state.counters.disconnected_session_total,
        expired_session_total: state.counters.expired_session_total,
        forwarded_frame_total: state.counters.forwarded_frame_total,
        queued_frame_total: state.counters.queued_frame_total,
        rate_limited_frame_total: state.counters.rate_limited_frame_total,
        aggregate_rate_limited_frame_total: state.counters.aggregate_rate_limited_frame_total,
        wire_message_too_large_total: state.counters.wire_message_too_large_total,
        source_byte_limited_frame_total: state.counters.source_byte_limited_frame_total,
        aggregate_byte_limited_frame_total: state.counters.aggregate_byte_limited_frame_total,
        admitted_wire_bytes_total: state.counters.admitted_wire_bytes_total,
        rejected_wire_bytes_total: state.counters.rejected_wire_bytes_total,
        active_queue_byte_limited_frame_total: state.counters.active_queue_byte_limited_frame_total,
        active_queue_count_limited_frame_total: state
            .counters
            .active_queue_count_limited_frame_total,
        offline_peer_limited_frame_total: state.counters.offline_peer_limited_frame_total,
        offline_source_limited_frame_total: state.counters.offline_source_limited_frame_total,
        offline_total_limited_frame_total: state.counters.offline_total_limited_frame_total,
        expired_queued_frame_total: state.counters.expired_queued_frame_total,
        expired_queued_bytes_total: state.counters.expired_queued_bytes_total,
        offline_full_sweep_total: state.counters.offline_full_sweep_total,
        protocol_rejected_frame_total: state.counters.protocol_rejected_frame_total,
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

    fn opaque_envelope(
        source_peer_id: &str,
        target_peer_id: &str,
        sequence: u64,
        ciphertext_bytes: usize,
    ) -> SecureNovoRudpEnvelopeV1 {
        SecureNovoRudpEnvelopeV1 {
            version: 1,
            session_id: [0x44; 16],
            sender_peer_id: source_peer_id.to_string(),
            recipient_peer_id: target_peer_id.to_string(),
            sequence,
            nonce: [0x55; 12],
            ciphertext: vec![0; ciphertext_bytes],
        }
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
            offline_queue_per_source: 1,
            offline_queue_total: 1,
            session_ttl_ms: 100,
            rate_limit_frames: 2,
            rate_limit_window_ms: 1_000,
            ..ProductRelayRuntimeConfigV1::default()
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

    #[tokio::test]
    async fn session_cap_allows_replacement_and_stale_session_releases_capacity() {
        let relay_identity = SigningKey::from_bytes(&[80u8; 32]);
        let node_a = SigningKey::from_bytes(&[81u8; 32]);
        let node_b = SigningKey::from_bytes(&[82u8; 32]);
        let manager = ProductRelaySessionManagerV1::new(ProductRelayRuntimeConfigV1 {
            max_sessions: 1,
            max_tracked_sources: 4,
            session_ttl_ms: 100,
            ..ProductRelayRuntimeConfigV1::default()
        })
        .unwrap();
        let (first, _first_inbox) = manager
            .register_authenticated_session(
                authenticate_to_relay(&node_a, &relay_identity, 1_000),
                1_000,
            )
            .await
            .unwrap();
        let (replacement, _replacement_inbox) = manager
            .register_authenticated_session(
                authenticate_to_relay(&node_a, &relay_identity, 1_010),
                1_010,
            )
            .await
            .unwrap();
        assert!(replacement.replaced_existing_session);
        assert!(
            !manager
                .is_current_session(&first.peer_id, first.session_id, 1_020)
                .await
        );

        let node_b_auth = authenticate_to_relay(&node_b, &relay_identity, 1_020);
        assert!(matches!(
            manager
                .register_authenticated_session(node_b_auth.clone(), 1_020)
                .await,
            Err(ProductRelayRuntimeErrorV1::SessionLimitReached { max_sessions: 1 })
        ));
        assert!(
            !manager
                .heartbeat_with_wire_bytes(&replacement.peer_id, replacement.session_id, 16, 1_111)
                .await
        );
        let (node_b_registration, _node_b_inbox) = manager
            .register_authenticated_session(node_b_auth, 1_111)
            .await
            .unwrap();
        assert_eq!(node_b_registration.peer_id, node_b_auth_peer_id_v1(&node_b));
        let snapshot = manager.snapshot().await;
        assert_eq!(snapshot.active_session_count, 1);
        assert_eq!(snapshot.session_limit_rejection_total, 1);
        assert_eq!(snapshot.expired_session_total, 1);
    }

    #[tokio::test]
    async fn source_budget_tracking_is_hard_bounded_under_identity_churn() {
        let relay_identity = SigningKey::from_bytes(&[83u8; 32]);
        let manager = ProductRelaySessionManagerV1::new(ProductRelayRuntimeConfigV1 {
            max_sessions: 1,
            max_tracked_sources: 4,
            session_ttl_ms: 120_000,
            ..ProductRelayRuntimeConfigV1::default()
        })
        .unwrap();
        for seed in 84u8..96u8 {
            let identity = SigningKey::from_bytes(&[seed; 32]);
            let now_ms = 2_000 + u64::from(seed);
            let (registration, inbox) = manager
                .register_authenticated_session(
                    authenticate_to_relay(&identity, &relay_identity, now_ms),
                    now_ms,
                )
                .await
                .unwrap();
            assert!(
                manager
                    .disconnect(&registration.peer_id, registration.session_id)
                    .await
            );
            drop(inbox);
            assert!(manager.snapshot().await.tracked_source_count <= 4);
        }
        let snapshot = manager.snapshot().await;
        assert_eq!(snapshot.tracked_source_count, 4);
        assert_eq!(snapshot.limits.max_tracked_sources, 4);
    }

    #[tokio::test]
    async fn byte_budgets_survive_reconnect_and_aggregate_limit_blocks_identity_spread() {
        let relay_identity = SigningKey::from_bytes(&[96u8; 32]);
        let node_a = SigningKey::from_bytes(&[97u8; 32]);
        let node_b = SigningKey::from_bytes(&[98u8; 32]);
        let node_c = SigningKey::from_bytes(&[99u8; 32]);
        let node_a_peer_id = node_b_auth_peer_id_v1(&node_a);
        let node_b_peer_id = node_b_auth_peer_id_v1(&node_b);
        let node_c_peer_id = node_b_auth_peer_id_v1(&node_c);
        let manager = ProductRelaySessionManagerV1::new(ProductRelayRuntimeConfigV1 {
            max_sessions: 4,
            max_tracked_sources: 8,
            session_ttl_ms: 120_000,
            source_bytes_per_minute: 100,
            max_bytes_per_minute: 150,
            ..ProductRelayRuntimeConfigV1::default()
        })
        .unwrap();
        let (registration_a, _inbox_a) = manager
            .register_authenticated_session(
                authenticate_to_relay(&node_a, &relay_identity, 3_000),
                3_000,
            )
            .await
            .unwrap();
        let (_registration_b, _inbox_b) = manager
            .register_authenticated_session(
                authenticate_to_relay(&node_b, &relay_identity, 3_000),
                3_000,
            )
            .await
            .unwrap();
        let (registration_c, _inbox_c) = manager
            .register_authenticated_session(
                authenticate_to_relay(&node_c, &relay_identity, 3_000),
                3_000,
            )
            .await
            .unwrap();
        let first = manager
            .forward_opaque_with_wire_bytes(
                &node_a_peer_id,
                registration_a.session_id,
                opaque_envelope(&node_a_peer_id, &node_b_peer_id, 1, 1),
                90,
                3_010,
            )
            .await;
        assert_eq!(first.disposition, RelayForwardDispositionV1::Forwarded);
        assert_eq!(first.envelope_sequence, Some(1));
        let encoded =
            serde_json::to_vec(&ProductRelayWireMessageV1::ForwardOutcome(first.clone())).unwrap();
        assert!(matches!(
            serde_json::from_slice::<ProductRelayWireMessageV1>(&encoded).unwrap(),
            ProductRelayWireMessageV1::ForwardOutcome(decoded) if decoded == first
        ));

        let (replacement_a, _replacement_inbox) = manager
            .register_authenticated_session(
                authenticate_to_relay(&node_a, &relay_identity, 3_020),
                3_020,
            )
            .await
            .unwrap();
        let source_limited = manager
            .forward_opaque_with_wire_bytes(
                &node_a_peer_id,
                replacement_a.session_id,
                opaque_envelope(&node_a_peer_id, &node_b_peer_id, 2, 1),
                20,
                3_030,
            )
            .await;
        assert_eq!(
            source_limited.disposition,
            RelayForwardDispositionV1::RejectedSourceByteLimited
        );
        let aggregate_limited = manager
            .forward_opaque_with_wire_bytes(
                &node_c_peer_id,
                registration_c.session_id,
                opaque_envelope(&node_c_peer_id, &node_b_peer_id, 3, 1),
                70,
                3_040,
            )
            .await;
        assert_eq!(
            aggregate_limited.disposition,
            RelayForwardDispositionV1::RejectedAggregateByteLimited
        );
        let after_window = manager
            .forward_opaque_with_wire_bytes(
                &node_a_peer_id,
                replacement_a.session_id,
                opaque_envelope(&node_a_peer_id, &node_b_peer_id, 4, 1),
                20,
                63_021,
            )
            .await;
        assert_eq!(
            after_window.disposition,
            RelayForwardDispositionV1::Forwarded
        );
        let snapshot = manager.snapshot().await;
        assert_eq!(snapshot.source_byte_limited_frame_total, 1);
        assert_eq!(snapshot.aggregate_byte_limited_frame_total, 1);
    }

    #[tokio::test]
    async fn data_and_control_share_offline_limits_and_expire_together() {
        let relay_identity = SigningKey::from_bytes(&[100u8; 32]);
        let source_a = SigningKey::from_bytes(&[101u8; 32]);
        let source_d = SigningKey::from_bytes(&[102u8; 32]);
        let target_b = SigningKey::from_bytes(&[103u8; 32]);
        let target_c = SigningKey::from_bytes(&[104u8; 32]);
        let target_e = SigningKey::from_bytes(&[105u8; 32]);
        let source_a_peer_id = node_b_auth_peer_id_v1(&source_a);
        let source_d_peer_id = node_b_auth_peer_id_v1(&source_d);
        let target_b_peer_id = node_b_auth_peer_id_v1(&target_b);
        let target_c_peer_id = node_b_auth_peer_id_v1(&target_c);
        let target_e_peer_id = node_b_auth_peer_id_v1(&target_e);
        let manager = ProductRelaySessionManagerV1::new(ProductRelayRuntimeConfigV1 {
            max_sessions: 4,
            max_tracked_sources: 8,
            offline_queue_per_peer: 2,
            offline_queue_per_source: 2,
            offline_queue_total: 3,
            offline_queue_ttl_ms: 100,
            session_ttl_ms: 10_000,
            ..ProductRelayRuntimeConfigV1::default()
        })
        .unwrap();
        let (registration_a, _inbox_a) = manager
            .register_authenticated_session(
                authenticate_to_relay(&source_a, &relay_identity, 4_000),
                4_000,
            )
            .await
            .unwrap();
        let (registration_d, _inbox_d) = manager
            .register_authenticated_session(
                authenticate_to_relay(&source_d, &relay_identity, 4_000),
                4_000,
            )
            .await
            .unwrap();
        assert_eq!(
            manager
                .forward_opaque(
                    &source_a_peer_id,
                    registration_a.session_id,
                    opaque_envelope(&source_a_peer_id, &target_b_peer_id, 1, 1),
                    4_010,
                )
                .await
                .disposition,
            RelayForwardDispositionV1::QueuedTargetOffline
        );
        let offer =
            NodeHandshakeInitiatorV1::start(&source_a, target_b_peer_id.clone(), 4_011, 5_000)
                .unwrap()
                .offer()
                .clone();
        assert_eq!(
            manager
                .forward_peer_handshake(
                    &source_a_peer_id,
                    registration_a.session_id,
                    &target_b_peer_id,
                    RelayPeerHandshakeV1::Offer(offer),
                    4_011,
                )
                .await
                .disposition,
            RelayForwardDispositionV1::QueuedTargetOffline
        );
        assert_eq!(
            manager
                .forward_opaque(
                    &source_a_peer_id,
                    registration_a.session_id,
                    opaque_envelope(&source_a_peer_id, &target_c_peer_id, 2, 1),
                    4_012,
                )
                .await
                .disposition,
            RelayForwardDispositionV1::RejectedQueueSourceLimit
        );
        assert_eq!(
            manager
                .forward_opaque(
                    &source_d_peer_id,
                    registration_d.session_id,
                    opaque_envelope(&source_d_peer_id, &target_c_peer_id, 3, 1),
                    4_013,
                )
                .await
                .disposition,
            RelayForwardDispositionV1::QueuedTargetOffline
        );
        assert_eq!(
            manager
                .forward_opaque(
                    &source_a_peer_id,
                    registration_a.session_id,
                    opaque_envelope(&source_a_peer_id, &target_b_peer_id, 4, 1),
                    4_014,
                )
                .await
                .disposition,
            RelayForwardDispositionV1::RejectedQueuePeerLimit
        );
        for sequence in 40..48 {
            assert_eq!(
                manager
                    .forward_opaque(
                        &source_a_peer_id,
                        registration_a.session_id,
                        opaque_envelope(&source_a_peer_id, &target_b_peer_id, sequence, 1,),
                        4_014,
                    )
                    .await
                    .disposition,
                RelayForwardDispositionV1::RejectedQueuePeerLimit
            );
        }
        assert_eq!(manager.snapshot().await.offline_full_sweep_total, 0);
        assert_eq!(
            manager
                .forward_opaque(
                    &source_d_peer_id,
                    registration_d.session_id,
                    opaque_envelope(&source_d_peer_id, &target_e_peer_id, 5, 1),
                    4_015,
                )
                .await
                .disposition,
            RelayForwardDispositionV1::RejectedQueueTotalLimit
        );
        let snapshot = manager.snapshot().await;
        assert_eq!(snapshot.offline_queued_frame_count, 3);
        assert_eq!(snapshot.offline_peer_limited_frame_total, 9);
        assert_eq!(snapshot.offline_source_limited_frame_total, 1);
        assert_eq!(snapshot.offline_total_limited_frame_total, 1);
        assert_eq!(manager.expire_stale_sessions(4_200).await, 0);
        let snapshot = manager.snapshot().await;
        assert_eq!(snapshot.offline_queued_frame_count, 0);
        assert_eq!(snapshot.expired_queued_frame_total, 3);
        assert_eq!(snapshot.offline_full_sweep_total, 1);
    }

    #[tokio::test]
    async fn raw_wire_admission_is_charged_once_and_malformed_payload_is_accounted() {
        let relay_identity = SigningKey::from_bytes(&[127u8; 32]);
        let source = SigningKey::from_bytes(&[128u8; 32]);
        let source_peer_id = node_b_auth_peer_id_v1(&source);
        let manager =
            ProductRelaySessionManagerV1::new(ProductRelayRuntimeConfigV1::default()).unwrap();
        let (registration, _inbox) = manager
            .register_authenticated_session(
                authenticate_to_relay(&source, &relay_identity, 11_000),
                11_000,
            )
            .await
            .unwrap();
        let admission = manager
            .admit_authenticated_wire_v1(&source_peer_id, registration.session_id, 37, 11_010)
            .await
            .unwrap();
        manager.reject_admitted_wire_v1(admission).await;
        let snapshot = manager.snapshot().await;
        assert_eq!(snapshot.admitted_wire_bytes_total, 37);
        assert_eq!(snapshot.rejected_wire_bytes_total, 37);
        assert_eq!(snapshot.protocol_rejected_frame_total, 1);
        assert_eq!(snapshot.rejected_frame_total, 1);
    }

    #[tokio::test]
    async fn aggregate_frame_budget_spans_authenticated_sources() {
        let relay_identity = SigningKey::from_bytes(&[129u8; 32]);
        let sources = [
            SigningKey::from_bytes(&[130u8; 32]),
            SigningKey::from_bytes(&[131u8; 32]),
            SigningKey::from_bytes(&[132u8; 32]),
        ];
        let manager = ProductRelaySessionManagerV1::new(ProductRelayRuntimeConfigV1 {
            max_sessions: 3,
            max_tracked_sources: 3,
            rate_limit_frames: 2,
            max_frames_per_window: 2,
            ..ProductRelayRuntimeConfigV1::default()
        })
        .unwrap();
        let mut sessions = Vec::new();
        for source in &sources {
            let peer_id = node_b_auth_peer_id_v1(source);
            let (registration, inbox) = manager
                .register_authenticated_session(
                    authenticate_to_relay(source, &relay_identity, 12_000),
                    12_000,
                )
                .await
                .unwrap();
            sessions.push((peer_id, registration.session_id, inbox));
        }
        for (peer_id, session_id, _) in sessions.iter().take(2) {
            let admission = manager
                .admit_authenticated_wire_v1(peer_id, *session_id, 1, 12_010)
                .await
                .unwrap();
            manager.reject_admitted_wire_v1(admission).await;
        }
        assert_eq!(
            manager
                .admit_authenticated_wire_v1(&sessions[2].0, sessions[2].1, 1, 12_010)
                .await
                .unwrap_err(),
            RelayForwardDispositionV1::RejectedAggregateRateLimited
        );
        let snapshot = manager.snapshot().await;
        assert_eq!(snapshot.aggregate_rate_limited_frame_total, 1);
        assert_eq!(snapshot.rate_limited_frame_total, 1);
    }

    #[tokio::test]
    async fn active_data_and_control_share_count_and_refill_without_reconnect() {
        let relay_identity = SigningKey::from_bytes(&[106u8; 32]);
        let node_a = SigningKey::from_bytes(&[107u8; 32]);
        let node_b = SigningKey::from_bytes(&[108u8; 32]);
        let node_a_peer_id = node_b_auth_peer_id_v1(&node_a);
        let node_b_peer_id = node_b_auth_peer_id_v1(&node_b);
        let manager = ProductRelaySessionManagerV1::new(ProductRelayRuntimeConfigV1 {
            session_queue_capacity: 1,
            session_queue_bytes: 1024 * 1024,
            ..ProductRelayRuntimeConfigV1::default()
        })
        .unwrap();
        let (registration_a, _inbox_a) = manager
            .register_authenticated_session(
                authenticate_to_relay(&node_a, &relay_identity, 5_000),
                5_000,
            )
            .await
            .unwrap();
        let (registration_b, mut inbox_b) = manager
            .register_authenticated_session(
                authenticate_to_relay(&node_b, &relay_identity, 5_000),
                5_000,
            )
            .await
            .unwrap();
        assert_eq!(
            manager
                .forward_opaque(
                    &node_a_peer_id,
                    registration_a.session_id,
                    opaque_envelope(&node_a_peer_id, &node_b_peer_id, 1, 1),
                    5_010,
                )
                .await
                .disposition,
            RelayForwardDispositionV1::Forwarded
        );
        let offer = NodeHandshakeInitiatorV1::start(&node_a, node_b_peer_id.clone(), 5_011, 5_000)
            .unwrap()
            .offer()
            .clone();
        assert_eq!(
            manager
                .forward_peer_handshake(
                    &node_a_peer_id,
                    registration_a.session_id,
                    &node_b_peer_id,
                    RelayPeerHandshakeV1::Offer(offer.clone()),
                    5_011,
                )
                .await
                .disposition,
            RelayForwardDispositionV1::QueuedBackpressure
        );
        let snapshot = manager.snapshot().await;
        assert_eq!(snapshot.active_queued_frame_count, 1);
        assert_eq!(snapshot.offline_queued_frame_count, 1);
        assert_eq!(snapshot.active_queue_count_limited_frame_total, 1);
        assert_eq!(inbox_b.recv().await.unwrap().envelope.sequence, 1);
        assert_eq!(
            manager
                .drain_queued_for_session(&node_b_peer_id, registration_b.session_id, 5_012,)
                .await,
            1
        );
        assert_eq!(
            inbox_b.recv_peer_handshake().await.unwrap().handshake,
            RelayPeerHandshakeV1::Offer(offer)
        );
        let snapshot = manager.snapshot().await;
        assert_eq!(snapshot.active_queued_frame_count, 0);
        assert_eq!(snapshot.offline_queued_frame_count, 0);
    }

    #[tokio::test]
    async fn active_and_offline_byte_caps_release_and_refill_without_reconnect() {
        let relay_identity = SigningKey::from_bytes(&[112u8; 32]);
        let node_a = SigningKey::from_bytes(&[113u8; 32]);
        let node_b = SigningKey::from_bytes(&[114u8; 32]);
        let node_a_peer_id = node_b_auth_peer_id_v1(&node_a);
        let node_b_peer_id = node_b_auth_peer_id_v1(&node_b);
        let first_envelope = opaque_envelope(&node_a_peer_id, &node_b_peer_id, 1, 32);
        let delivery_bytes = serde_json::to_vec(&ProductRelayWireMessageV1::Delivery(
            OpaqueRelayDeliveryV1 {
                source_peer_id: node_a_peer_id.clone(),
                target_peer_id: node_b_peer_id.clone(),
                received_at_ms: 5_510,
                envelope: first_envelope.clone(),
            },
        ))
        .unwrap()
        .len();
        let offline_accounted_bytes = offline_memory_accounted_bytes_v1(
            delivery_bytes,
            node_a_peer_id.len(),
            node_b_peer_id.len(),
        );
        let manager = ProductRelaySessionManagerV1::new(ProductRelayRuntimeConfigV1 {
            session_queue_capacity: 4,
            session_queue_bytes: delivery_bytes,
            offline_queue_bytes_per_peer: offline_accounted_bytes,
            ..ProductRelayRuntimeConfigV1::default()
        })
        .unwrap();
        let (registration_a, _inbox_a) = manager
            .register_authenticated_session(
                authenticate_to_relay(&node_a, &relay_identity, 5_500),
                5_500,
            )
            .await
            .unwrap();
        let (registration_b, mut inbox_b) = manager
            .register_authenticated_session(
                authenticate_to_relay(&node_b, &relay_identity, 5_500),
                5_500,
            )
            .await
            .unwrap();
        assert_eq!(
            manager
                .forward_opaque(
                    &node_a_peer_id,
                    registration_a.session_id,
                    first_envelope,
                    5_510,
                )
                .await
                .disposition,
            RelayForwardDispositionV1::Forwarded
        );
        assert_eq!(
            manager
                .forward_opaque(
                    &node_a_peer_id,
                    registration_a.session_id,
                    opaque_envelope(&node_a_peer_id, &node_b_peer_id, 2, 32),
                    5_511,
                )
                .await
                .disposition,
            RelayForwardDispositionV1::QueuedBackpressure
        );
        assert_eq!(
            manager
                .forward_opaque(
                    &node_a_peer_id,
                    registration_a.session_id,
                    opaque_envelope(&node_a_peer_id, &node_b_peer_id, 3, 32),
                    5_512,
                )
                .await
                .disposition,
            RelayForwardDispositionV1::RejectedQueuePeerLimit
        );
        let snapshot = manager.snapshot().await;
        assert_eq!(snapshot.active_queue_byte_limited_frame_total, 1);
        assert_eq!(snapshot.offline_peer_limited_frame_total, 1);
        assert_eq!(snapshot.offline_queued_bytes, offline_accounted_bytes);
        assert!(snapshot.offline_queued_bytes > delivery_bytes);
        assert_eq!(inbox_b.recv().await.unwrap().envelope.sequence, 1);
        assert_eq!(
            manager
                .drain_queued_for_session(&node_b_peer_id, registration_b.session_id, 5_513,)
                .await,
            1
        );
        assert_eq!(inbox_b.recv().await.unwrap().envelope.sequence, 2);
        let snapshot = manager.snapshot().await;
        assert_eq!(snapshot.active_queued_bytes, 0);
        assert_eq!(snapshot.offline_queued_bytes, 0);
    }

    #[test]
    fn independent_active_queue_totals_are_strictly_validated() {
        let defaults = ProductRelayRuntimeConfigV1::default();
        assert_eq!(defaults.active_queue_total, 16_384);
        assert_eq!(defaults.active_queue_bytes_total, 256 * 1024 * 1024);

        let zero_total = ProductRelayRuntimeConfigV1 {
            active_queue_total: 0,
            ..defaults.clone()
        };
        assert!(matches!(
            ProductRelaySessionManagerV1::new(zero_total),
            Err(ProductRelayRuntimeErrorV1::InvalidConfiguration(
                "session limits must be positive"
            ))
        ));

        let session_exceeds_total = ProductRelayRuntimeConfigV1 {
            session_queue_capacity: 2,
            active_queue_total: 1,
            ..defaults.clone()
        };
        assert!(matches!(
            ProductRelaySessionManagerV1::new(session_exceeds_total),
            Err(ProductRelayRuntimeErrorV1::InvalidConfiguration(
                "per-session active queue limits must not exceed active queue totals"
            ))
        ));

        let byte_session_exceeds_total = ProductRelayRuntimeConfigV1 {
            session_queue_bytes: 2,
            active_queue_bytes_total: 1,
            ..defaults.clone()
        };
        assert!(matches!(
            ProductRelaySessionManagerV1::new(byte_session_exceeds_total),
            Err(ProductRelayRuntimeErrorV1::InvalidConfiguration(
                "per-session active queue limits must not exceed active queue totals"
            ))
        ));

        let count_overflow = ProductRelayRuntimeConfigV1 {
            active_queue_total: usize::MAX,
            offline_queue_per_peer: 1,
            offline_queue_per_source: 1,
            offline_queue_total: 1,
            ..defaults.clone()
        };
        assert!(matches!(
            ProductRelaySessionManagerV1::new(count_overflow),
            Err(ProductRelayRuntimeErrorV1::InvalidConfiguration(
                "aggregate active and offline queue limits overflow usize"
            ))
        ));

        let byte_overflow = ProductRelayRuntimeConfigV1 {
            active_queue_bytes_total: usize::MAX,
            offline_queue_bytes_per_peer: 1,
            offline_queue_bytes_per_source: 1,
            offline_queue_bytes_total: 1,
            ..defaults
        };
        assert!(matches!(
            ProductRelaySessionManagerV1::new(byte_overflow),
            Err(ProductRelayRuntimeErrorV1::InvalidConfiguration(
                "aggregate active and offline queue limits overflow usize"
            ))
        ));
    }

    #[tokio::test]
    async fn replacement_held_inboxes_remain_globally_counted_until_drop_or_recv() {
        let relay_identity = SigningKey::from_bytes(&[115u8; 32]);
        let source = SigningKey::from_bytes(&[116u8; 32]);
        let target = SigningKey::from_bytes(&[117u8; 32]);
        let source_peer_id = node_b_auth_peer_id_v1(&source);
        let target_peer_id = node_b_auth_peer_id_v1(&target);
        let manager = ProductRelaySessionManagerV1::new(ProductRelayRuntimeConfigV1 {
            max_sessions: 2,
            session_queue_capacity: 1,
            active_queue_total: 2,
            ..ProductRelayRuntimeConfigV1::default()
        })
        .unwrap();
        let (source_registration, _source_inbox) = manager
            .register_authenticated_session(
                authenticate_to_relay(&source, &relay_identity, 7_000),
                7_000,
            )
            .await
            .unwrap();
        let (_target_registration_one, target_inbox_one) = manager
            .register_authenticated_session(
                authenticate_to_relay(&target, &relay_identity, 7_000),
                7_000,
            )
            .await
            .unwrap();
        assert_eq!(
            manager
                .forward_opaque(
                    &source_peer_id,
                    source_registration.session_id,
                    opaque_envelope(&source_peer_id, &target_peer_id, 1, 16),
                    7_010,
                )
                .await
                .disposition,
            RelayForwardDispositionV1::Forwarded
        );

        let (_target_registration_two, mut target_inbox_two) = manager
            .register_authenticated_session(
                authenticate_to_relay(&target, &relay_identity, 7_020),
                7_020,
            )
            .await
            .unwrap();
        assert_eq!(
            manager
                .forward_opaque(
                    &source_peer_id,
                    source_registration.session_id,
                    opaque_envelope(&source_peer_id, &target_peer_id, 2, 16),
                    7_030,
                )
                .await
                .disposition,
            RelayForwardDispositionV1::Forwarded
        );

        let (target_registration_three, mut target_inbox_three) = manager
            .register_authenticated_session(
                authenticate_to_relay(&target, &relay_identity, 7_040),
                7_040,
            )
            .await
            .unwrap();
        assert_eq!(
            manager
                .forward_opaque(
                    &source_peer_id,
                    source_registration.session_id,
                    opaque_envelope(&source_peer_id, &target_peer_id, 3, 16),
                    7_050,
                )
                .await
                .disposition,
            RelayForwardDispositionV1::QueuedBackpressure
        );
        let snapshot = manager.snapshot().await;
        assert_eq!(snapshot.active_queued_frame_count, 2);
        assert_eq!(snapshot.offline_queued_frame_count, 1);
        assert_eq!(snapshot.limits.active_queue_total, 2);
        assert_eq!(snapshot.active_queue_count_limited_frame_total, 1);

        drop(target_inbox_one);
        assert_eq!(manager.snapshot().await.active_queued_frame_count, 1);
        assert_eq!(
            manager
                .drain_queued_for_session(
                    &target_peer_id,
                    target_registration_three.session_id,
                    7_051,
                )
                .await,
            1
        );
        assert_eq!(manager.snapshot().await.active_queued_frame_count, 2);
        assert_eq!(
            target_inbox_three.recv().await.unwrap().envelope.sequence,
            3
        );
        assert_eq!(manager.snapshot().await.active_queued_frame_count, 1);
        assert_eq!(target_inbox_two.recv().await.unwrap().envelope.sequence, 2);
        assert_eq!(manager.snapshot().await.active_queued_frame_count, 0);
    }

    #[tokio::test]
    async fn disconnect_and_ttl_keep_real_payload_counted_until_inbox_drop() {
        let relay_identity = SigningKey::from_bytes(&[118u8; 32]);
        let source = SigningKey::from_bytes(&[119u8; 32]);
        let target = SigningKey::from_bytes(&[120u8; 32]);
        let source_peer_id = node_b_auth_peer_id_v1(&source);
        let target_peer_id = node_b_auth_peer_id_v1(&target);
        let manager = ProductRelaySessionManagerV1::new(ProductRelayRuntimeConfigV1 {
            max_sessions: 2,
            session_queue_capacity: 1,
            active_queue_total: 1,
            session_ttl_ms: 100,
            ..ProductRelayRuntimeConfigV1::default()
        })
        .unwrap();
        let (source_registration, _source_inbox) = manager
            .register_authenticated_session(
                authenticate_to_relay(&source, &relay_identity, 8_000),
                8_000,
            )
            .await
            .unwrap();
        let (target_registration_one, target_inbox_one) = manager
            .register_authenticated_session(
                authenticate_to_relay(&target, &relay_identity, 8_000),
                8_000,
            )
            .await
            .unwrap();
        assert_eq!(
            manager
                .forward_opaque(
                    &source_peer_id,
                    source_registration.session_id,
                    opaque_envelope(&source_peer_id, &target_peer_id, 1, 16),
                    8_010,
                )
                .await
                .disposition,
            RelayForwardDispositionV1::Forwarded
        );
        assert!(
            manager
                .disconnect(&target_peer_id, target_registration_one.session_id)
                .await
        );
        assert_eq!(manager.snapshot().await.active_queued_frame_count, 1);

        let (target_registration_two, target_inbox_two) = manager
            .register_authenticated_session(
                authenticate_to_relay(&target, &relay_identity, 8_020),
                8_020,
            )
            .await
            .unwrap();
        assert_eq!(
            manager
                .forward_opaque(
                    &source_peer_id,
                    source_registration.session_id,
                    opaque_envelope(&source_peer_id, &target_peer_id, 2, 16),
                    8_030,
                )
                .await
                .disposition,
            RelayForwardDispositionV1::QueuedBackpressure
        );

        drop(target_inbox_one);
        assert_eq!(manager.snapshot().await.active_queued_frame_count, 0);
        assert_eq!(
            manager
                .drain_queued_for_session(
                    &target_peer_id,
                    target_registration_two.session_id,
                    8_031,
                )
                .await,
            1
        );
        assert_eq!(manager.snapshot().await.active_queued_frame_count, 1);
        assert_eq!(manager.expire_stale_sessions(8_131).await, 2);
        let snapshot = manager.snapshot().await;
        assert_eq!(snapshot.active_session_count, 0);
        assert_eq!(snapshot.active_queued_frame_count, 1);

        drop(target_inbox_two);
        assert_eq!(manager.snapshot().await.active_queued_frame_count, 0);
    }

    #[tokio::test]
    async fn replacement_held_inbox_obeys_global_byte_limit_and_refills_after_drop() {
        let relay_identity = SigningKey::from_bytes(&[121u8; 32]);
        let source = SigningKey::from_bytes(&[122u8; 32]);
        let target = SigningKey::from_bytes(&[123u8; 32]);
        let source_peer_id = node_b_auth_peer_id_v1(&source);
        let target_peer_id = node_b_auth_peer_id_v1(&target);
        let first_envelope = opaque_envelope(&source_peer_id, &target_peer_id, 1, 64);
        let second_envelope = opaque_envelope(&source_peer_id, &target_peer_id, 2, 64);
        let first_bytes = serde_json::to_vec(&ProductRelayWireMessageV1::Delivery(
            OpaqueRelayDeliveryV1 {
                source_peer_id: source_peer_id.clone(),
                target_peer_id: target_peer_id.clone(),
                received_at_ms: 9_010,
                envelope: first_envelope.clone(),
            },
        ))
        .unwrap()
        .len();
        let second_bytes = serde_json::to_vec(&ProductRelayWireMessageV1::Delivery(
            OpaqueRelayDeliveryV1 {
                source_peer_id: source_peer_id.clone(),
                target_peer_id: target_peer_id.clone(),
                received_at_ms: 9_030,
                envelope: second_envelope.clone(),
            },
        ))
        .unwrap()
        .len();
        let active_bytes_total = first_bytes
            .checked_add(second_bytes)
            .and_then(|total| total.checked_sub(1))
            .unwrap();
        let manager = ProductRelaySessionManagerV1::new(ProductRelayRuntimeConfigV1 {
            max_sessions: 2,
            session_queue_capacity: 2,
            session_queue_bytes: first_bytes.max(second_bytes),
            active_queue_total: 4,
            active_queue_bytes_total: active_bytes_total,
            ..ProductRelayRuntimeConfigV1::default()
        })
        .unwrap();
        let (source_registration, _source_inbox) = manager
            .register_authenticated_session(
                authenticate_to_relay(&source, &relay_identity, 9_000),
                9_000,
            )
            .await
            .unwrap();
        let (_target_registration_one, target_inbox_one) = manager
            .register_authenticated_session(
                authenticate_to_relay(&target, &relay_identity, 9_000),
                9_000,
            )
            .await
            .unwrap();
        assert_eq!(
            manager
                .forward_opaque(
                    &source_peer_id,
                    source_registration.session_id,
                    first_envelope,
                    9_010,
                )
                .await
                .disposition,
            RelayForwardDispositionV1::Forwarded
        );

        let (target_registration_two, mut target_inbox_two) = manager
            .register_authenticated_session(
                authenticate_to_relay(&target, &relay_identity, 9_020),
                9_020,
            )
            .await
            .unwrap();
        assert_eq!(
            manager
                .forward_opaque(
                    &source_peer_id,
                    source_registration.session_id,
                    second_envelope,
                    9_030,
                )
                .await
                .disposition,
            RelayForwardDispositionV1::QueuedBackpressure
        );
        let snapshot = manager.snapshot().await;
        assert_eq!(snapshot.active_queued_bytes, first_bytes);
        assert_eq!(snapshot.limits.active_queue_bytes_total, active_bytes_total);
        assert_eq!(snapshot.active_queue_byte_limited_frame_total, 1);

        drop(target_inbox_one);
        assert_eq!(manager.snapshot().await.active_queued_bytes, 0);
        assert_eq!(
            manager
                .drain_queued_for_session(
                    &target_peer_id,
                    target_registration_two.session_id,
                    9_031,
                )
                .await,
            1
        );
        assert_eq!(manager.snapshot().await.active_queued_bytes, second_bytes);
        assert_eq!(target_inbox_two.recv().await.unwrap().envelope.sequence, 2);
        assert_eq!(manager.snapshot().await.active_queued_bytes, 0);
    }

    #[tokio::test]
    async fn shutdown_keeps_real_payload_counted_until_inbox_drop() {
        let relay_identity = SigningKey::from_bytes(&[124u8; 32]);
        let source = SigningKey::from_bytes(&[125u8; 32]);
        let target = SigningKey::from_bytes(&[126u8; 32]);
        let source_peer_id = node_b_auth_peer_id_v1(&source);
        let target_peer_id = node_b_auth_peer_id_v1(&target);
        let manager = ProductRelaySessionManagerV1::new(ProductRelayRuntimeConfigV1 {
            max_sessions: 2,
            session_queue_capacity: 1,
            active_queue_total: 1,
            ..ProductRelayRuntimeConfigV1::default()
        })
        .unwrap();
        let (source_registration, _source_inbox) = manager
            .register_authenticated_session(
                authenticate_to_relay(&source, &relay_identity, 10_000),
                10_000,
            )
            .await
            .unwrap();
        let (_target_registration, target_inbox) = manager
            .register_authenticated_session(
                authenticate_to_relay(&target, &relay_identity, 10_000),
                10_000,
            )
            .await
            .unwrap();
        assert_eq!(
            manager
                .forward_opaque(
                    &source_peer_id,
                    source_registration.session_id,
                    opaque_envelope(&source_peer_id, &target_peer_id, 1, 16),
                    10_010,
                )
                .await
                .disposition,
            RelayForwardDispositionV1::Forwarded
        );

        let shutdown = manager.finish_graceful_shutdown().await;
        assert!(!shutdown.accepting_new_work);
        assert_eq!(shutdown.active_session_count, 0);
        assert_eq!(shutdown.active_queued_frame_count, 1);
        drop(target_inbox);
        assert_eq!(manager.snapshot().await.active_queued_frame_count, 0);
    }

    #[tokio::test]
    async fn delivery_wrapper_must_fit_wire_limit_before_admission_ack() {
        let relay_identity = SigningKey::from_bytes(&[109u8; 32]);
        let node_a = SigningKey::from_bytes(&[110u8; 32]);
        let node_b = SigningKey::from_bytes(&[111u8; 32]);
        let node_a_peer_id = node_b_auth_peer_id_v1(&node_a);
        let node_b_peer_id = node_b_auth_peer_id_v1(&node_b);
        let mut low = 0usize;
        let mut high = PRODUCT_RELAY_MAX_WIRE_MESSAGE_BYTES_V1;
        while low < high {
            let midpoint = low + (high - low).div_ceil(2);
            let envelope = opaque_envelope(&node_a_peer_id, &node_b_peer_id, 7, midpoint);
            let input_len = serde_json::to_vec(&ProductRelayWireMessageV1::Data(envelope))
                .unwrap()
                .len();
            if input_len <= PRODUCT_RELAY_MAX_WIRE_MESSAGE_BYTES_V1 {
                low = midpoint;
            } else {
                high = midpoint - 1;
            }
        }
        let envelope = opaque_envelope(&node_a_peer_id, &node_b_peer_id, 7, low);
        let input_len = serde_json::to_vec(&ProductRelayWireMessageV1::Data(envelope.clone()))
            .unwrap()
            .len();
        let delivery_len = serde_json::to_vec(&ProductRelayWireMessageV1::Delivery(
            OpaqueRelayDeliveryV1 {
                source_peer_id: node_a_peer_id.clone(),
                target_peer_id: node_b_peer_id.clone(),
                received_at_ms: 6_010,
                envelope: envelope.clone(),
            },
        ))
        .unwrap()
        .len();
        assert!(input_len <= PRODUCT_RELAY_MAX_WIRE_MESSAGE_BYTES_V1);
        assert!(delivery_len > PRODUCT_RELAY_MAX_WIRE_MESSAGE_BYTES_V1);

        let manager =
            ProductRelaySessionManagerV1::new(ProductRelayRuntimeConfigV1::default()).unwrap();
        let (registration_a, _inbox_a) = manager
            .register_authenticated_session(
                authenticate_to_relay(&node_a, &relay_identity, 6_000),
                6_000,
            )
            .await
            .unwrap();
        let outcome = manager
            .forward_opaque_with_wire_bytes(
                &node_a_peer_id,
                registration_a.session_id,
                envelope,
                input_len,
                6_010,
            )
            .await;
        assert_eq!(
            outcome.disposition,
            RelayForwardDispositionV1::RejectedWireMessageTooLarge
        );
        let snapshot = manager.snapshot().await;
        assert_eq!(snapshot.queued_frame_count, 0);
        assert_eq!(snapshot.wire_message_too_large_total, 1);
    }

    fn node_b_auth_peer_id_v1(identity: &SigningKey) -> String {
        peer_id_from_ed25519_public_key_v1(&identity.verifying_key().to_bytes())
    }
}
