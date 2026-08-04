//! Product Overlay ownership inside the NOVOVM main node lifecycle.
//!
//! The relay remains transport-only. Decrypted payloads retain their explicit Host-owned class.
//! Native transactions return to normal ingress for chain-domain, signature, identity, and nonce
//! policy before AOEM execution; native seal bytes remain opaque to this transport module.

use anyhow::{bail, Context, Result};
use ed25519_dalek::SigningKey;
use novovm_network::{
    peer_id_from_ed25519_public_key_v1, E2eSecureChannelV1, HandshakeReplayCacheV1,
    NodeHandshakeInitiatorV1, NodeHandshakeResponderV1, NovoRudpTransportFrameKindV0,
    NovoRudpTransportFrameV0, OpaqueRelayDeliveryV1, ProductRelayWireMessageV1,
    RelayForwardDispositionV1, RelayForwardOutcomeV1, RelayPeerHandshakeV1, RelayTransportV1,
    StrategyPathV1,
};
use serde::{Deserialize, Serialize};
use std::{
    collections::{BTreeMap, BTreeSet, VecDeque},
    fs,
    path::{Path, PathBuf},
    sync::{
        atomic::{AtomicBool, AtomicUsize, Ordering},
        mpsc::{self, Receiver, SyncSender, TryRecvError, TrySendError},
        Arc, Mutex,
    },
    thread::{self, JoinHandle},
    time::{Duration, SystemTime, UNIX_EPOCH},
};

use crate::{
    product_node_overlay::{
        ProductNodeBootstrapStatusV1, ProductNodeOverlayConfigV1, ProductNodeOverlayRuntimeV1,
        ProductNodeRoutePlanV1,
    },
    product_relay_client::{
        ProductRelayClientConfigV1, ProductRelayClientEventV1, ProductRelayClientV1,
        ProductRelayTlsTrustV1,
    },
    tx_ingress::ingest_local_nov_raw_tx_payload_v1,
};

const PRODUCT_MAINLINE_OVERLAY_SCOPE_V1: &str = "novovm_product_mainline_overlay_runtime_v1";
const PRODUCT_MAINLINE_OVERLAY_SESSION_ID_V1: [u8; 16] = *b"NOVOVM-OVERLAY-1";
const PRODUCT_MAINLINE_OVERLAY_PREAUTH_BUFFER_LIMIT_V1: usize = 64;
const PRODUCT_MAINLINE_OVERLAY_PEER_HANDSHAKE_TTL_MS_V1: u64 = 30_000;
const PRODUCT_MAINLINE_OVERLAY_PEER_FAULT_REASON_MAX_BYTES_V1: usize = 512;
const PRODUCT_MAINLINE_OVERLAY_PAYLOAD_MAGIC_V1: [u8; 8] = *b"NOVOPLD1";
const PRODUCT_MAINLINE_OVERLAY_PAYLOAD_VERSION_V1: u16 = 1;
const PRODUCT_MAINLINE_OVERLAY_PAYLOAD_HEADER_LEN_V1: usize = 8 + 2 + 1 + 1 + 32 + 4;
pub const PRODUCT_MAINLINE_OVERLAY_MAX_CLASSIFIED_LOGICAL_PAYLOAD_BYTES_V1: usize = 192 * 1024;
const PRODUCT_MAINLINE_OVERLAY_EVENT_SLOT_OVERHEAD_BYTES_V1: usize = 4 * 1024;
const PRODUCT_MAINLINE_OVERLAY_EVENT_RETRY_DELAY_MS_V1: u64 = 5;
const PRODUCT_MAINLINE_OVERLAY_EVENT_TEXT_MAX_BYTES_V1: usize = 4 * 1024;
const PRODUCT_MAINLINE_OVERLAY_EVENT_DRAIN_MAX_BATCH_V1: usize = 256;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ProductMainlineOverlayRoleV1 {
    Initiator,
    Responder,
    Duplex,
}

/// Host routing metadata only. This class does not authenticate or validate its opaque payload.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ProductMainlineOverlayPayloadClassV1 {
    NativeTransaction,
    NativeSeal,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct ProductMainlineOverlayResourceLimitsV1 {
    pub pending_per_peer_count: usize,
    pub pending_per_peer_bytes: usize,
    pub pending_total_count: usize,
    pub pending_total_bytes: usize,
    pub pending_ttl_ms: u64,
    pub event_total_bytes: usize,
    pub preauth_per_peer_count: usize,
    pub preauth_per_peer_bytes: usize,
    pub preauth_total_count: usize,
    pub preauth_total_bytes: usize,
    pub preauth_ttl_ms: u64,
}

impl Default for ProductMainlineOverlayResourceLimitsV1 {
    fn default() -> Self {
        Self {
            pending_per_peer_count: 1_024,
            pending_per_peer_bytes: 64 * 1024 * 1024,
            pending_total_count: 16_384,
            pending_total_bytes: 256 * 1024 * 1024,
            pending_ttl_ms: 60_000,
            event_total_bytes: 256 * 1024 * 1024,
            preauth_per_peer_count: PRODUCT_MAINLINE_OVERLAY_PREAUTH_BUFFER_LIMIT_V1,
            preauth_per_peer_bytes: 4 * 1024 * 1024,
            preauth_total_count: 1_024,
            preauth_total_bytes: 64 * 1024 * 1024,
            preauth_ttl_ms: PRODUCT_MAINLINE_OVERLAY_PEER_HANDSHAKE_TTL_MS_V1,
        }
    }
}

impl ProductMainlineOverlayPayloadClassV1 {
    const fn code(self) -> u8 {
        match self {
            Self::NativeTransaction => 1,
            Self::NativeSeal => 2,
        }
    }

    fn from_code(code: u8) -> Result<Self> {
        match code {
            1 => Ok(Self::NativeTransaction),
            2 => Ok(Self::NativeSeal),
            _ => bail!("product mainline overlay payload class {code} is unsupported"),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProductMainlineOverlayPeerConfigV1 {
    pub peer_id: String,
    pub metric_peer_id: u64,
}

#[derive(Debug, Clone)]
pub struct ProductMainlineOverlayConfigV1 {
    pub chain_id: u64,
    pub role: ProductMainlineOverlayRoleV1,
    pub identity_key_path: PathBuf,
    pub overlay: ProductNodeOverlayConfigV1,
    pub target_peer_id: Option<String>,
    pub expected_source_peer_id: Option<String>,
    pub peers: Vec<ProductMainlineOverlayPeerConfigV1>,
    pub connect_timeout_ms: u64,
    pub read_timeout_ms: u64,
    pub tls_trust: ProductRelayTlsTrustV1,
    pub channel_capacity: usize,
    pub resource_limits: ProductMainlineOverlayResourceLimitsV1,
    pub metric_peer_id: u64,
    pub reconnect_base_delay_ms: u64,
    pub reconnect_max_delay_ms: u64,
}

#[derive(Deserialize)]
struct ProductMainlineOverlayConfigSerdeV1 {
    chain_id: u64,
    role: ProductMainlineOverlayRoleV1,
    identity_key_path: PathBuf,
    overlay: ProductNodeOverlayConfigV1,
    #[serde(default)]
    target_peer_id: Option<String>,
    #[serde(default)]
    expected_source_peer_id: Option<String>,
    #[serde(default)]
    peers: Vec<ProductMainlineOverlayPeerConfigV1>,
    #[serde(default = "default_connect_timeout_ms_v1")]
    connect_timeout_ms: u64,
    #[serde(default = "default_read_timeout_ms_v1")]
    read_timeout_ms: u64,
    #[serde(default = "default_tls_trust_v1")]
    tls_trust: ProductRelayTlsTrustV1,
    #[serde(default = "default_channel_capacity_v1")]
    channel_capacity: usize,
    #[serde(default)]
    resource_limits: Option<ProductMainlineOverlayResourceLimitsV1>,
    #[serde(default = "default_metric_peer_id_v1")]
    metric_peer_id: u64,
    #[serde(default = "default_reconnect_base_delay_ms_v1")]
    reconnect_base_delay_ms: u64,
    #[serde(default = "default_reconnect_max_delay_ms_v1")]
    reconnect_max_delay_ms: u64,
}

impl<'de> Deserialize<'de> for ProductMainlineOverlayConfigV1 {
    fn deserialize<D>(deserializer: D) -> std::result::Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        let decoded = ProductMainlineOverlayConfigSerdeV1::deserialize(deserializer)?;
        let resource_limits = decoded
            .resource_limits
            .unwrap_or_else(|| omitted_resource_limits_compatibility_v1(decoded.channel_capacity));
        Ok(Self {
            chain_id: decoded.chain_id,
            role: decoded.role,
            identity_key_path: decoded.identity_key_path,
            overlay: decoded.overlay,
            target_peer_id: decoded.target_peer_id,
            expected_source_peer_id: decoded.expected_source_peer_id,
            peers: decoded.peers,
            connect_timeout_ms: decoded.connect_timeout_ms,
            read_timeout_ms: decoded.read_timeout_ms,
            tls_trust: decoded.tls_trust,
            channel_capacity: decoded.channel_capacity,
            resource_limits,
            metric_peer_id: decoded.metric_peer_id,
            reconnect_base_delay_ms: decoded.reconnect_base_delay_ms,
            reconnect_max_delay_ms: decoded.reconnect_max_delay_ms,
        })
    }
}

fn omitted_resource_limits_compatibility_v1(
    _channel_capacity: usize,
) -> ProductMainlineOverlayResourceLimitsV1 {
    ProductMainlineOverlayResourceLimitsV1::default()
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProductMainlineOverlayStartupV1 {
    pub scope: String,
    pub local_peer_id: String,
    pub remote_peer_id: String,
    pub remote_peer_ids: Vec<String>,
    pub role: ProductMainlineOverlayRoleV1,
    pub bootstrap: ProductNodeBootstrapStatusV1,
    pub route_plan: ProductNodeRoutePlanV1,
    pub payload_treated_opaque_by_relay: bool,
    pub relay_is_trusted_authority: bool,
    pub aoem_transport_policy_embedded: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProductMainlineOverlayInboundV1 {
    pub payload_class: ProductMainlineOverlayPayloadClassV1,
    pub object_hash: [u8; 32],
    pub source_peer_id: String,
    /// The logical transport frame. Its payload is restored to the exact caller-supplied bytes
    /// after the authenticated Product Overlay classification envelope is removed.
    pub frame: NovoRudpTransportFrameV0,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProductMainlineOverlayDeliveryV1 {
    pub payload_class: ProductMainlineOverlayPayloadClassV1,
    pub object_hash: [u8; 32],
    pub remote_peer_id: String,
    pub metric_peer_id: u64,
    pub delivered: bool,
    pub error: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProductMainlineOverlayIngressReceiptV1 {
    pub chain_id: u64,
    pub tx_hash: [u8; 32],
    pub ingress_entry: String,
    pub pending_only: bool,
    pub execution_owner: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProductMainlineOverlayIngressFailureClassV1 {
    PeerRejected,
    LocalFault,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProductMainlineOverlayIngressFailureV1 {
    pub class: ProductMainlineOverlayIngressFailureClassV1,
    pub message: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ProductMainlineOverlayEventV1 {
    RelayConnected {
        relay_peer_id: String,
    },
    RelayDisconnected {
        relay_peer_id: String,
        error: String,
        reconnect_in_ms: u64,
    },
    RelayRotated {
        previous_relay_peer_id: String,
        next_relay_peer_id: String,
    },
    E2eSessionEstablished {
        remote_peer_id: String,
    },
    PeerIsolated {
        remote_peer_id: String,
        reason: String,
        session_failure_count: u32,
        retry_in_ms: u64,
    },
    Inbound(ProductMainlineOverlayInboundV1),
    Delivery(ProductMainlineOverlayDeliveryV1),
    WorkerStopped,
    WorkerFailed(String),
}

#[derive(Clone)]
struct ProductMainlineOverlayEventSenderV1 {
    sender: SyncSender<ProductMainlineOverlayAccountedEventV1>,
    bytes_in_flight: Arc<AtomicUsize>,
    max_bytes: usize,
}

struct ProductMainlineOverlayAccountedEventV1 {
    event: ProductMainlineOverlayEventV1,
    _permit: ProductMainlineOverlayEventPermitV1,
}

struct ProductMainlineOverlayEventPermitV1 {
    bytes_in_flight: Arc<AtomicUsize>,
    bytes: usize,
}

impl Drop for ProductMainlineOverlayEventPermitV1 {
    fn drop(&mut self) {
        let previous = self.bytes_in_flight.fetch_sub(self.bytes, Ordering::AcqRel);
        debug_assert!(previous >= self.bytes);
    }
}

impl ProductMainlineOverlayEventSenderV1 {
    fn try_reserve_v1(&self, bytes: usize) -> Option<ProductMainlineOverlayEventPermitV1> {
        if bytes > self.max_bytes {
            return None;
        }
        self.bytes_in_flight
            .fetch_update(Ordering::AcqRel, Ordering::Acquire, |current| {
                current
                    .checked_add(bytes)
                    .filter(|next| *next <= self.max_bytes)
            })
            .ok()?;
        Some(ProductMainlineOverlayEventPermitV1 {
            bytes_in_flight: Arc::clone(&self.bytes_in_flight),
            bytes,
        })
    }
}

fn bound_product_mainline_overlay_event_text_v1(event: &mut ProductMainlineOverlayEventV1) {
    let text = match event {
        ProductMainlineOverlayEventV1::RelayDisconnected { error, .. } => Some(error),
        ProductMainlineOverlayEventV1::PeerIsolated { reason, .. } => Some(reason),
        ProductMainlineOverlayEventV1::Delivery(delivery) => delivery.error.as_mut(),
        ProductMainlineOverlayEventV1::WorkerFailed(error) => Some(error),
        _ => None,
    };
    if let Some(text) = text {
        let mut boundary = PRODUCT_MAINLINE_OVERLAY_EVENT_TEXT_MAX_BYTES_V1.min(text.len());
        while !text.is_char_boundary(boundary) {
            boundary = boundary.saturating_sub(1);
        }
        text.truncate(boundary);
        text.shrink_to_fit();
    }
}

fn product_mainline_overlay_event_owned_bytes_v1(event: &ProductMainlineOverlayEventV1) -> usize {
    let dynamic = match event {
        ProductMainlineOverlayEventV1::RelayConnected { relay_peer_id } => relay_peer_id.capacity(),
        ProductMainlineOverlayEventV1::RelayDisconnected {
            relay_peer_id,
            error,
            ..
        } => relay_peer_id.capacity().saturating_add(error.capacity()),
        ProductMainlineOverlayEventV1::RelayRotated {
            previous_relay_peer_id,
            next_relay_peer_id,
        } => previous_relay_peer_id
            .capacity()
            .saturating_add(next_relay_peer_id.capacity()),
        ProductMainlineOverlayEventV1::E2eSessionEstablished { remote_peer_id } => {
            remote_peer_id.capacity()
        }
        ProductMainlineOverlayEventV1::PeerIsolated {
            remote_peer_id,
            reason,
            ..
        } => remote_peer_id.capacity().saturating_add(reason.capacity()),
        ProductMainlineOverlayEventV1::Inbound(inbound) => inbound
            .source_peer_id
            .capacity()
            .saturating_add(inbound.frame.payload.capacity()),
        ProductMainlineOverlayEventV1::Delivery(delivery) => delivery
            .remote_peer_id
            .capacity()
            .saturating_add(delivery.error.as_ref().map_or(0, String::capacity)),
        ProductMainlineOverlayEventV1::WorkerStopped => 0,
        ProductMainlineOverlayEventV1::WorkerFailed(error) => error.capacity(),
    };
    std::mem::size_of::<ProductMainlineOverlayAccountedEventV1>().saturating_add(dynamic)
}

fn publish_product_mainline_overlay_event_v1(
    events: &ProductMainlineOverlayEventSenderV1,
    stop: &AtomicBool,
    mut event: ProductMainlineOverlayEventV1,
) -> Result<()> {
    bound_product_mainline_overlay_event_text_v1(&mut event);
    let event_bytes = product_mainline_overlay_event_owned_bytes_v1(&event);
    if event_bytes > events.max_bytes {
        bail!(
            "product mainline overlay event exceeds event_total_bytes: event={event_bytes} limit={}",
            events.max_bytes
        );
    }
    let permit = loop {
        if stop.load(Ordering::Acquire) {
            return Ok(());
        }
        if let Some(permit) = events.try_reserve_v1(event_bytes) {
            break permit;
        }
        thread::sleep(Duration::from_millis(
            PRODUCT_MAINLINE_OVERLAY_EVENT_RETRY_DELAY_MS_V1,
        ));
    };
    let mut accounted = ProductMainlineOverlayAccountedEventV1 {
        event,
        _permit: permit,
    };
    loop {
        if stop.load(Ordering::Acquire) {
            return Ok(());
        }
        match events.sender.try_send(accounted) {
            Ok(()) => return Ok(()),
            Err(TrySendError::Full(returned)) => {
                accounted = returned;
                thread::sleep(Duration::from_millis(
                    PRODUCT_MAINLINE_OVERLAY_EVENT_RETRY_DELAY_MS_V1,
                ));
            }
            Err(TrySendError::Disconnected(_)) => {
                bail!("product mainline overlay event receiver is disconnected")
            }
        }
    }
}

#[derive(Debug)]
struct ProductMainlineOverlayOutboundItemV1 {
    payload_class: ProductMainlineOverlayPayloadClassV1,
    object_hash: [u8; 32],
    payload: Arc<[u8]>,
    enqueued_at_ms: u64,
    expires_at_ms: u64,
}

impl ProductMainlineOverlayOutboundItemV1 {
    fn expired_at(&self, now_ms: u64) -> bool {
        now_ms >= self.expires_at_ms
    }
}

struct ProductMainlineOverlayOutboundV1 {
    item: Arc<ProductMainlineOverlayOutboundItemV1>,
    reservations: BTreeMap<String, ProductMainlineOverlayPendingPermitV1>,
}

struct ProductMainlineOverlayPendingV1 {
    item: Arc<ProductMainlineOverlayOutboundItemV1>,
    _reservation: ProductMainlineOverlayPendingPermitV1,
}

#[cfg(test)]
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
struct ProductMainlineOverlayResourceUsageV1 {
    pending_count: usize,
    pending_bytes: usize,
}

#[derive(Debug, Clone, Copy, Default)]
struct ProductMainlineOverlayPeerUsageV1 {
    count: usize,
    bytes: usize,
}

struct ProductMainlineOverlayPendingBudgetV1 {
    limits: ProductMainlineOverlayResourceLimitsV1,
    total: ProductMainlineOverlayPeerUsageV1,
    by_peer: BTreeMap<String, ProductMainlineOverlayPeerUsageV1>,
}

struct ProductMainlineOverlayPendingPermitV1 {
    budget: Arc<Mutex<ProductMainlineOverlayPendingBudgetV1>>,
    peer_id: String,
    bytes: usize,
}

impl Drop for ProductMainlineOverlayPendingPermitV1 {
    fn drop(&mut self) {
        let mut budget = self
            .budget
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        budget.total.count = budget.total.count.saturating_sub(1);
        budget.total.bytes = budget.total.bytes.saturating_sub(self.bytes);
        let remove_peer = if let Some(peer) = budget.by_peer.get_mut(&self.peer_id) {
            peer.count = peer.count.saturating_sub(1);
            peer.bytes = peer.bytes.saturating_sub(self.bytes);
            peer.count == 0 && peer.bytes == 0
        } else {
            false
        };
        if remove_peer {
            budget.by_peer.remove(&self.peer_id);
        }
    }
}

impl ProductMainlineOverlayPendingBudgetV1 {
    fn new(limits: ProductMainlineOverlayResourceLimitsV1) -> Self {
        Self {
            limits,
            total: ProductMainlineOverlayPeerUsageV1::default(),
            by_peer: BTreeMap::new(),
        }
    }
}

struct ProductMainlineOverlayPreauthBudgetV1 {
    limits: ProductMainlineOverlayResourceLimitsV1,
    total: ProductMainlineOverlayPeerUsageV1,
    by_peer: BTreeMap<String, ProductMainlineOverlayPeerUsageV1>,
}

struct ProductMainlineOverlayPreauthPermitV1 {
    budget: Arc<Mutex<ProductMainlineOverlayPreauthBudgetV1>>,
    peer_id: String,
    bytes: usize,
}

impl Drop for ProductMainlineOverlayPreauthPermitV1 {
    fn drop(&mut self) {
        let mut budget = self
            .budget
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        budget.total.count = budget.total.count.saturating_sub(1);
        budget.total.bytes = budget.total.bytes.saturating_sub(self.bytes);
        let remove_peer = if let Some(peer) = budget.by_peer.get_mut(&self.peer_id) {
            peer.count = peer.count.saturating_sub(1);
            peer.bytes = peer.bytes.saturating_sub(self.bytes);
            peer.count == 0 && peer.bytes == 0
        } else {
            false
        };
        if remove_peer {
            budget.by_peer.remove(&self.peer_id);
        }
    }
}

impl ProductMainlineOverlayPreauthBudgetV1 {
    fn new(limits: ProductMainlineOverlayResourceLimitsV1) -> Self {
        Self {
            limits,
            total: ProductMainlineOverlayPeerUsageV1::default(),
            by_peer: BTreeMap::new(),
        }
    }
}

struct ProductMainlineOverlayBufferedDeliveryV1 {
    delivery: OpaqueRelayDeliveryV1,
    buffered_at_ms: u64,
    ttl_ms: u64,
    _reservation: ProductMainlineOverlayPreauthPermitV1,
}

impl ProductMainlineOverlayBufferedDeliveryV1 {
    fn expired_at(&self, now_ms: u64) -> bool {
        now_ms.saturating_sub(self.buffered_at_ms) >= self.ttl_ms
    }
}

fn try_reserve_pending_fanout_v1(
    budget: &Arc<Mutex<ProductMainlineOverlayPendingBudgetV1>>,
    peer_ids: &[String],
    bytes_per_peer: usize,
) -> Option<BTreeMap<String, ProductMainlineOverlayPendingPermitV1>> {
    let required_count = peer_ids.len();
    let required_bytes = bytes_per_peer.checked_mul(required_count)?;
    let mut state = budget
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    if state.total.count.checked_add(required_count)? > state.limits.pending_total_count
        || state.total.bytes.checked_add(required_bytes)? > state.limits.pending_total_bytes
    {
        return None;
    }
    for peer_id in peer_ids {
        let peer = state.by_peer.get(peer_id).copied().unwrap_or_default();
        if peer.count.checked_add(1)? > state.limits.pending_per_peer_count
            || peer.bytes.checked_add(bytes_per_peer)? > state.limits.pending_per_peer_bytes
        {
            return None;
        }
    }

    state.total.count += required_count;
    state.total.bytes += required_bytes;
    for peer_id in peer_ids {
        let peer = state.by_peer.entry(peer_id.clone()).or_default();
        peer.count += 1;
        peer.bytes += bytes_per_peer;
    }
    drop(state);

    Some(
        peer_ids
            .iter()
            .map(|peer_id| {
                (
                    peer_id.clone(),
                    ProductMainlineOverlayPendingPermitV1 {
                        budget: Arc::clone(budget),
                        peer_id: peer_id.clone(),
                        bytes: bytes_per_peer,
                    },
                )
            })
            .collect(),
    )
}

fn try_reserve_preauth_v1(
    budget: &Arc<Mutex<ProductMainlineOverlayPreauthBudgetV1>>,
    peer_id: &str,
    bytes: usize,
) -> Option<ProductMainlineOverlayPreauthPermitV1> {
    let mut state = budget
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    let peer = state.by_peer.get(peer_id).copied().unwrap_or_default();
    if state.total.count.checked_add(1)? > state.limits.preauth_total_count
        || state.total.bytes.checked_add(bytes)? > state.limits.preauth_total_bytes
        || peer.count.checked_add(1)? > state.limits.preauth_per_peer_count
        || peer.bytes.checked_add(bytes)? > state.limits.preauth_per_peer_bytes
    {
        return None;
    }
    state.total.count += 1;
    state.total.bytes += bytes;
    let peer = state.by_peer.entry(peer_id.to_string()).or_default();
    peer.count += 1;
    peer.bytes += bytes;
    drop(state);
    Some(ProductMainlineOverlayPreauthPermitV1 {
        budget: Arc::clone(budget),
        peer_id: peer_id.to_string(),
        bytes,
    })
}

struct ProductMainlineOverlayWorkerV1 {
    identity: SigningKey,
    overlay: ProductNodeOverlayRuntimeV1,
    route_plan: ProductNodeRoutePlanV1,
    relay_overrides: BTreeMap<String, ProductRelayClientConfigV1>,
    connect_timeout_ms: u64,
    read_timeout_ms: u64,
    tls_trust: ProductRelayTlsTrustV1,
    reconnect_base_delay_ms: u64,
    reconnect_max_delay_ms: u64,
    role: ProductMainlineOverlayRoleV1,
    remote_peer_id: String,
    remote_peers: Vec<ProductMainlineOverlayPeerConfigV1>,
    expected_source_peer_id: Option<String>,
    chain_id: u64,
    outbound: Receiver<ProductMainlineOverlayOutboundV1>,
    resource_limits: ProductMainlineOverlayResourceLimitsV1,
    events: ProductMainlineOverlayEventSenderV1,
    stop: Arc<AtomicBool>,
}

enum ProductMainlineMeshPeerPhaseV1 {
    Idle,
    Handshaking {
        initiator: NodeHandshakeInitiatorV1,
        expires_at_ms: u64,
    },
    Active(E2eSecureChannelV1),
    Cooldown {
        retry_at_ms: u64,
    },
}

struct ProductMainlineMeshPeerStateV1 {
    phase: ProductMainlineMeshPeerPhaseV1,
    session_failure_count: u32,
    replay: HandshakeReplayCacheV1,
    buffered_deliveries: VecDeque<ProductMainlineOverlayBufferedDeliveryV1>,
    frame_sequence: u64,
}

impl Default for ProductMainlineMeshPeerStateV1 {
    fn default() -> Self {
        Self {
            phase: ProductMainlineMeshPeerPhaseV1::Idle,
            session_failure_count: 0,
            replay: HandshakeReplayCacheV1::default(),
            buffered_deliveries: VecDeque::new(),
            frame_sequence: 0,
        }
    }
}

pub struct ProductMainlineOverlayRuntimeV1 {
    startup: ProductMainlineOverlayStartupV1,
    role: ProductMainlineOverlayRoleV1,
    metric_peer_id: u64,
    remote_peer_ids: Vec<String>,
    outbound: SyncSender<ProductMainlineOverlayOutboundV1>,
    pending_budget: Arc<Mutex<ProductMainlineOverlayPendingBudgetV1>>,
    events: Receiver<ProductMainlineOverlayAccountedEventV1>,
    stop: Arc<AtomicBool>,
    worker: Option<JoinHandle<()>>,
}

impl ProductMainlineOverlayRuntimeV1 {
    pub fn start(config: ProductMainlineOverlayConfigV1, now_ms: u64) -> Result<Self> {
        Self::start_inner_v1(config, now_ms, BTreeMap::new())
    }

    fn start_inner_v1(
        config: ProductMainlineOverlayConfigV1,
        now_ms: u64,
        relay_overrides: BTreeMap<String, ProductRelayClientConfigV1>,
    ) -> Result<Self> {
        validate_config_v1(&config)?;
        let identity = load_ed25519_identity_v1(&config.identity_key_path)?;
        let local_peer_id =
            peer_id_from_ed25519_public_key_v1(&identity.verifying_key().to_bytes());
        let remote_peers = configured_remote_peers_v1(&config)?;
        let remote_peer_id = remote_peers
            .first()
            .context("product mainline overlay requires at least one remote peer")?
            .peer_id
            .clone();
        if remote_peers
            .iter()
            .any(|remote| local_peer_id == remote.peer_id)
        {
            bail!("product mainline overlay remote peers must differ from local peer");
        }
        let remote_peer_ids = remote_peers
            .iter()
            .map(|remote| remote.peer_id.clone())
            .collect::<Vec<_>>();

        let overlay = ProductNodeOverlayRuntimeV1::bootstrap(&config.overlay, now_ms)
            .context("bootstrap product overlay inside main node lifecycle")?;
        let route_plan = overlay.select_relay_route(
            &identity,
            format!("mainline-overlay-{now_ms}"),
            &remote_peer_id,
            now_ms,
            None,
            false,
        );
        if route_plan.selected_path != StrategyPathV1::RelayNovoRudp {
            bail!(
                "product mainline overlay requires a reachable signed relay candidate: {:?}",
                route_plan.fallback_reason
            );
        }
        let selected = route_plan
            .selected_relay
            .as_ref()
            .context("relay route selected without a signed relay record")?;
        if selected.selected_endpoint.transport != RelayTransportV1::Wss443 {
            bail!(
                "product mainline overlay selected unsupported live transport {:?}; WSS is required",
                selected.selected_endpoint.transport
            );
        }
        let startup = ProductMainlineOverlayStartupV1 {
            scope: PRODUCT_MAINLINE_OVERLAY_SCOPE_V1.into(),
            local_peer_id,
            remote_peer_id: remote_peer_id.clone(),
            remote_peer_ids: remote_peer_ids.clone(),
            role: config.role,
            bootstrap: overlay.bootstrap_status().clone(),
            route_plan: route_plan.clone(),
            payload_treated_opaque_by_relay: true,
            relay_is_trusted_authority: false,
            aoem_transport_policy_embedded: false,
        };
        let capacity = config.channel_capacity.clamp(1, 65_536);
        let (outbound_tx, outbound_rx) = mpsc::sync_channel(capacity);
        let (event_sender, event_rx) = mpsc::sync_channel(capacity);
        let event_tx = ProductMainlineOverlayEventSenderV1 {
            sender: event_sender,
            bytes_in_flight: Arc::new(AtomicUsize::new(0)),
            max_bytes: config.resource_limits.event_total_bytes,
        };
        let pending_budget = Arc::new(Mutex::new(ProductMainlineOverlayPendingBudgetV1::new(
            config.resource_limits.clone(),
        )));
        let worker_resource_limits = config.resource_limits.clone();
        let stop = Arc::new(AtomicBool::new(false));
        let worker_stop = Arc::clone(&stop);
        let worker_role = config.role;
        let worker_remote_peer_id = remote_peer_id;
        let worker_remote_peers = remote_peers;
        let chain_id = config.chain_id;
        let expected_source_peer_id = config.expected_source_peer_id.clone();
        let connect_timeout_ms = config.connect_timeout_ms.max(1);
        let read_timeout_ms = config.read_timeout_ms.clamp(1, 1_000);
        let tls_trust = config.tls_trust.clone();
        let reconnect_base_delay_ms = config.reconnect_base_delay_ms.max(1);
        let reconnect_max_delay_ms = config.reconnect_max_delay_ms.max(reconnect_base_delay_ms);
        let worker = thread::Builder::new()
            .name("novovm-product-overlay".into())
            .spawn(move || {
                let result = run_worker_v1(ProductMainlineOverlayWorkerV1 {
                    identity,
                    overlay,
                    route_plan,
                    relay_overrides,
                    connect_timeout_ms,
                    read_timeout_ms,
                    tls_trust,
                    reconnect_base_delay_ms,
                    reconnect_max_delay_ms,
                    role: worker_role,
                    remote_peer_id: worker_remote_peer_id,
                    remote_peers: worker_remote_peers,
                    expected_source_peer_id,
                    chain_id,
                    outbound: outbound_rx,
                    resource_limits: worker_resource_limits,
                    events: event_tx.clone(),
                    stop: Arc::clone(&worker_stop),
                });
                if let Err(error) = result {
                    let _ = publish_product_mainline_overlay_event_v1(
                        &event_tx,
                        &worker_stop,
                        ProductMainlineOverlayEventV1::WorkerFailed(error.to_string()),
                    );
                }
                let _ = publish_product_mainline_overlay_event_v1(
                    &event_tx,
                    &worker_stop,
                    ProductMainlineOverlayEventV1::WorkerStopped,
                );
            })
            .context("spawn product overlay mainline lifecycle worker")?;

        Ok(Self {
            startup,
            role: config.role,
            metric_peer_id: config.metric_peer_id,
            remote_peer_ids,
            outbound: outbound_tx,
            pending_budget,
            events: event_rx,
            stop,
            worker: Some(worker),
        })
    }

    #[cfg(test)]
    fn start_with_relay_override_v1(
        config: ProductMainlineOverlayConfigV1,
        now_ms: u64,
        relay: ProductRelayClientConfigV1,
    ) -> Result<Self> {
        Self::start_with_relay_overrides_v1(config, now_ms, vec![relay])
    }

    #[cfg(test)]
    fn start_with_relay_overrides_v1(
        config: ProductMainlineOverlayConfigV1,
        now_ms: u64,
        relays: Vec<ProductRelayClientConfigV1>,
    ) -> Result<Self> {
        let relay_overrides = relays
            .into_iter()
            .map(|relay| (relay.expected_relay_peer_id.clone(), relay))
            .collect();
        Self::start_inner_v1(config, now_ms, relay_overrides)
    }

    #[must_use]
    pub fn startup(&self) -> &ProductMainlineOverlayStartupV1 {
        &self.startup
    }

    #[must_use]
    pub fn role(&self) -> ProductMainlineOverlayRoleV1 {
        self.role
    }

    #[must_use]
    pub fn metric_peer_id(&self) -> u64 {
        self.metric_peer_id
    }

    #[must_use]
    pub fn remote_peer_ids(&self) -> &[String] {
        &self.remote_peer_ids
    }

    pub fn try_submit(&self, tx_hash: [u8; 32], payload: Vec<u8>) -> Result<bool> {
        self.try_submit_classified_v1(
            ProductMainlineOverlayPayloadClassV1::NativeTransaction,
            tx_hash,
            payload,
        )
    }

    /// Queues opaque native-seal bytes for authenticated transport without decoding the seal.
    pub fn try_submit_native_seal(&self, object_hash: [u8; 32], payload: Vec<u8>) -> Result<bool> {
        self.try_submit_classified_v1(
            ProductMainlineOverlayPayloadClassV1::NativeSeal,
            object_hash,
            payload,
        )
    }

    fn try_submit_classified_v1(
        &self,
        payload_class: ProductMainlineOverlayPayloadClassV1,
        object_hash: [u8; 32],
        payload: Vec<u8>,
    ) -> Result<bool> {
        validate_classified_logical_payload_len_v1(payload.len())?;
        if self.role == ProductMainlineOverlayRoleV1::Responder {
            return Ok(false);
        }
        if payload.is_empty() {
            match payload_class {
                ProductMainlineOverlayPayloadClassV1::NativeTransaction => {
                    bail!("product mainline overlay refuses an empty transaction payload")
                }
                ProductMainlineOverlayPayloadClassV1::NativeSeal => {
                    bail!("product mainline overlay refuses an empty native seal payload")
                }
            }
        }
        let Some(reservations) = try_reserve_pending_fanout_v1(
            &self.pending_budget,
            &self.remote_peer_ids,
            payload.len(),
        ) else {
            return Ok(false);
        };
        let enqueued_at_ms = now_ms_v1();
        let item = Arc::new(ProductMainlineOverlayOutboundItemV1 {
            payload_class,
            object_hash,
            payload: Arc::from(payload),
            enqueued_at_ms,
            expires_at_ms: enqueued_at_ms.saturating_add(
                self.pending_budget
                    .lock()
                    .unwrap_or_else(std::sync::PoisonError::into_inner)
                    .limits
                    .pending_ttl_ms,
            ),
        });
        match self
            .outbound
            .try_send(ProductMainlineOverlayOutboundV1 { item, reservations })
        {
            Ok(()) => Ok(true),
            Err(TrySendError::Full(_)) => Ok(false),
            Err(TrySendError::Disconnected(_)) => {
                bail!("product mainline overlay worker is disconnected")
            }
        }
    }

    pub fn drain_events(&self, limit: usize) -> Vec<ProductMainlineOverlayEventV1> {
        let mut events = Vec::new();
        // Byte permits protect the runtime-owned channel backlog. Ownership transfers to the
        // caller when an event is popped, so cap one transfer batch as a separate caller-side
        // memory guard rather than claiming returned values remain charged to the runtime.
        for _ in 0..limit.clamp(1, PRODUCT_MAINLINE_OVERLAY_EVENT_DRAIN_MAX_BATCH_V1) {
            match self.events.try_recv() {
                Ok(accounted) => {
                    let ProductMainlineOverlayAccountedEventV1 { event, _permit } = accounted;
                    events.push(event);
                }
                Err(TryRecvError::Empty | TryRecvError::Disconnected) => break,
            }
        }
        events
    }

    #[cfg(test)]
    fn pending_resource_usage_v1(&self) -> ProductMainlineOverlayResourceUsageV1 {
        let budget = self
            .pending_budget
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        ProductMainlineOverlayResourceUsageV1 {
            pending_count: budget.total.count,
            pending_bytes: budget.total.bytes,
        }
    }

    pub fn shutdown(&mut self) {
        self.stop.store(true, Ordering::Release);
        if let Some(worker) = self.worker.take() {
            let _ = worker.join();
        }
    }
}

impl Drop for ProductMainlineOverlayRuntimeV1 {
    fn drop(&mut self) {
        self.shutdown();
    }
}

fn encode_classified_payload_v1(
    outbound: &ProductMainlineOverlayOutboundItemV1,
) -> Result<Vec<u8>> {
    validate_classified_logical_payload_len_v1(outbound.payload.len())?;
    let payload_len = u32::try_from(outbound.payload.len())
        .context("product mainline overlay payload exceeds the v1 wire length limit")?;
    let mut encoded = Vec::with_capacity(
        PRODUCT_MAINLINE_OVERLAY_PAYLOAD_HEADER_LEN_V1.saturating_add(outbound.payload.len()),
    );
    encoded.extend_from_slice(&PRODUCT_MAINLINE_OVERLAY_PAYLOAD_MAGIC_V1);
    encoded.extend_from_slice(&PRODUCT_MAINLINE_OVERLAY_PAYLOAD_VERSION_V1.to_le_bytes());
    encoded.push(outbound.payload_class.code());
    encoded.push(0);
    encoded.extend_from_slice(&outbound.object_hash);
    encoded.extend_from_slice(&payload_len.to_le_bytes());
    encoded.extend_from_slice(outbound.payload.as_ref());
    Ok(encoded)
}

fn decode_classified_payload_v1(
    encoded: &[u8],
) -> Result<(ProductMainlineOverlayPayloadClassV1, [u8; 32], Vec<u8>)> {
    if encoded.len() < PRODUCT_MAINLINE_OVERLAY_PAYLOAD_HEADER_LEN_V1 {
        bail!(
            "product mainline overlay classified payload is too short: len={}",
            encoded.len()
        );
    }
    if encoded[..8] != PRODUCT_MAINLINE_OVERLAY_PAYLOAD_MAGIC_V1 {
        bail!("product mainline overlay classified payload magic is invalid");
    }
    let version = u16::from_le_bytes([encoded[8], encoded[9]]);
    if version != PRODUCT_MAINLINE_OVERLAY_PAYLOAD_VERSION_V1 {
        bail!("product mainline overlay payload version {version} is unsupported");
    }
    let payload_class = ProductMainlineOverlayPayloadClassV1::from_code(encoded[10])?;
    if encoded[11] != 0 {
        bail!("product mainline overlay payload reserved byte must be zero");
    }
    let mut object_hash = [0u8; 32];
    object_hash.copy_from_slice(&encoded[12..44]);
    let payload_len =
        u32::from_le_bytes([encoded[44], encoded[45], encoded[46], encoded[47]]) as usize;
    validate_classified_logical_payload_len_v1(payload_len)?;
    let expected_len = PRODUCT_MAINLINE_OVERLAY_PAYLOAD_HEADER_LEN_V1
        .checked_add(payload_len)
        .context("product mainline overlay classified payload length overflow")?;
    if encoded.len() != expected_len {
        bail!(
            "product mainline overlay classified payload length mismatch: expected={expected_len} actual={}",
            encoded.len()
        );
    }
    if payload_len == 0 {
        bail!("product mainline overlay classified payload must not be empty");
    }
    Ok((
        payload_class,
        object_hash,
        encoded[PRODUCT_MAINLINE_OVERLAY_PAYLOAD_HEADER_LEN_V1..].to_vec(),
    ))
}

fn validate_classified_logical_payload_len_v1(payload_len: usize) -> Result<()> {
    if payload_len > PRODUCT_MAINLINE_OVERLAY_MAX_CLASSIFIED_LOGICAL_PAYLOAD_BYTES_V1 {
        bail!(
            "product mainline overlay classified logical payload exceeds the v1 limit: len={payload_len} max={}",
            PRODUCT_MAINLINE_OVERLAY_MAX_CLASSIFIED_LOGICAL_PAYLOAD_BYTES_V1
        );
    }
    Ok(())
}

fn open_classified_inbound_frame_v1(
    frame: NovoRudpTransportFrameV0,
    chain_id: u64,
) -> Result<(
    ProductMainlineOverlayPayloadClassV1,
    [u8; 32],
    NovoRudpTransportFrameV0,
)> {
    validate_inbound_frame_v1(&frame, chain_id)?;
    let (payload_class, object_hash, payload) =
        decode_classified_payload_v1(frame.payload.as_slice())?;
    let expected_object_id = u64::from_le_bytes(
        object_hash[..8]
            .try_into()
            .context("product mainline overlay object hash prefix is invalid")?,
    );
    if frame.object_id != expected_object_id {
        bail!("product mainline overlay object hash does not match the transport object id");
    }
    let logical_frame = NovoRudpTransportFrameV0::new(
        frame.kind,
        frame.session_id,
        frame.stream_id,
        frame.object_id,
        frame.sequence,
        frame.ack_epoch,
        payload,
    );
    Ok((payload_class, object_hash, logical_frame))
}

pub fn load_product_mainline_overlay_config_v1(
    path: impl AsRef<Path>,
) -> Result<ProductMainlineOverlayConfigV1> {
    let path = path.as_ref();
    let absolute_path = fs::canonicalize(path).with_context(|| {
        format!(
            "resolve product mainline overlay config: {}",
            path.display()
        )
    })?;
    let bytes = fs::read(&absolute_path).with_context(|| {
        format!(
            "read product mainline overlay config: {}",
            absolute_path.display()
        )
    })?;
    let mut config: ProductMainlineOverlayConfigV1 =
        serde_json::from_slice(&bytes).with_context(|| {
            format!(
                "decode product mainline overlay config: {}",
                absolute_path.display()
            )
        })?;
    let base = absolute_path
        .parent()
        .context("product mainline overlay config has no parent directory")?;
    rebase_relative_path_v1(base, &mut config.identity_key_path);
    rebase_relative_path_v1(base, &mut config.overlay.cache_path);
    if let ProductRelayTlsTrustV1::ExplicitCa { certificate_path } = &mut config.tls_trust {
        rebase_relative_path_v1(base, certificate_path);
    }
    Ok(config)
}

fn rebase_relative_path_v1(base: &Path, path: &mut PathBuf) {
    if path.is_relative() {
        *path = base.join(&*path);
    }
}

pub fn ingest_product_mainline_overlay_payload_v1(
    chain_id: u64,
    payload: &[u8],
) -> Result<ProductMainlineOverlayIngressReceiptV1> {
    let (native_tx, _ir, tx_hash) = ingest_local_nov_raw_tx_payload_v1(
        &serde_json::json!({
            "chain_id": chain_id,
            "pipeline_only": true,
            "ingress_source": PRODUCT_MAINLINE_OVERLAY_SCOPE_V1,
        }),
        payload,
    )
    .context("product overlay payload rejected by native transaction ingress")?;
    if native_tx.chain_id != chain_id {
        bail!(
            "product overlay native tx chain_id {} does not match pipeline chain_id {}",
            native_tx.chain_id,
            chain_id
        );
    }
    Ok(ProductMainlineOverlayIngressReceiptV1 {
        chain_id,
        tx_hash,
        ingress_entry: "ingest_local_nov_raw_tx_payload_v1".into(),
        pending_only: true,
        execution_owner: "aoem_runtime".into(),
    })
}

pub fn ingest_product_mainline_overlay_peer_payload_v1(
    chain_id: u64,
    payload: &[u8],
) -> std::result::Result<
    ProductMainlineOverlayIngressReceiptV1,
    ProductMainlineOverlayIngressFailureV1,
> {
    ingest_product_mainline_overlay_payload_v1(chain_id, payload).map_err(|error| {
        ProductMainlineOverlayIngressFailureV1 {
            class: classify_product_mainline_overlay_ingress_failure_v1(&error),
            message: format!("{error:#}"),
        }
    })
}

fn classify_product_mainline_overlay_ingress_failure_v1(
    error: &anyhow::Error,
) -> ProductMainlineOverlayIngressFailureClassV1 {
    const PEER_REJECTION_PREFIXES_V1: &[&str] = &[
        "nov_sendRawTransaction payload is empty",
        "nov_sendRawTransaction payload decode failed:",
        "compute native signed-intent commitment failed",
        "nov native authentication rejected: chain_id must be non-zero",
        "nov native authentication rejected: chain domain mismatch",
        "nov native authentication rejected: legacy or malformed signature",
        "nov native authentication rejected: signature or signer identity mismatch",
        "nov native authentication rejected: wire v3 carries only",
        "nov native signer account is invalid",
        "nov native authentication rejected: account_id=",
        "nov native authentication rejected: fee_owner_account_id=",
        "nov native authentication rejected: nonce_owner_account_id=",
        "nov native authenticated ingress rejected:",
        "nov native authentication rejected: durable nonce conflict",
        "nov native authentication rejected: durable nonce sequence mismatch",
        "nov native authentication rejected: nonce conflict",
        "nov native authentication rejected: nonce sequence mismatch",
        "product overlay native tx chain_id ",
    ];
    let is_peer_rejection = error.chain().any(|cause| {
        let message = cause.to_string();
        PEER_REJECTION_PREFIXES_V1
            .iter()
            .any(|prefix| message.starts_with(prefix))
    });
    if is_peer_rejection {
        ProductMainlineOverlayIngressFailureClassV1::PeerRejected
    } else {
        // Unknown failures are conservatively node-local. This prevents a new store, lock,
        // configuration, or verifier failure from being silently downgraded to hostile input.
        ProductMainlineOverlayIngressFailureClassV1::LocalFault
    }
}

pub fn validate_product_mainline_overlay_config_v1(
    config: &ProductMainlineOverlayConfigV1,
) -> Result<()> {
    validate_config_v1(config)
}

pub fn product_mainline_overlay_local_peer_id_v1(
    config: &ProductMainlineOverlayConfigV1,
) -> Result<String> {
    let identity = load_ed25519_identity_v1(&config.identity_key_path)?;
    Ok(peer_id_from_ed25519_public_key_v1(
        &identity.verifying_key().to_bytes(),
    ))
}

fn run_worker_v1(mut worker: ProductMainlineOverlayWorkerV1) -> Result<()> {
    let mut pending = VecDeque::<ProductMainlineOverlayPendingV1>::new();
    let mut mesh_pending = worker
        .remote_peers
        .iter()
        .map(|remote| {
            (
                remote.peer_id.clone(),
                VecDeque::<ProductMainlineOverlayPendingV1>::new(),
            )
        })
        .collect::<BTreeMap<_, _>>();
    let mut consecutive_failures = 0u32;
    while !worker.stop.load(Ordering::Acquire) {
        service_pending_resources_v1(&worker, &mut pending, &mut mesh_pending, 256)?;
        if worker.route_plan.selected_relay.is_none() {
            wait_for_reconnect_servicing_pending_v1(
                &worker,
                &mut pending,
                &mut mesh_pending,
                reconnect_delay_ms_v1(
                    consecutive_failures.max(1),
                    worker.reconnect_base_delay_ms,
                    worker.reconnect_max_delay_ms,
                ),
            )?;
            if worker.stop.load(Ordering::Acquire) {
                break;
            }
            worker.route_plan = worker.overlay.select_relay_route(
                &worker.identity,
                format!("mainline-overlay-reselect-{}", now_ms_v1()),
                &worker.remote_peer_id,
                now_ms_v1(),
                None,
                false,
            );
            continue;
        }

        let selected = worker
            .route_plan
            .selected_relay
            .as_ref()
            .context("product overlay route lost its signed relay candidate")?;
        let relay_peer_id = selected.relay_peer_id.clone();
        let relay_config = worker
            .relay_overrides
            .get(&relay_peer_id)
            .cloned()
            .unwrap_or_else(|| ProductRelayClientConfigV1 {
                endpoint: selected.selected_endpoint.uri.clone(),
                expected_relay_peer_id: relay_peer_id.clone(),
                connect_timeout_ms: worker.connect_timeout_ms,
                read_timeout_ms: worker.read_timeout_ms,
                tls_trust: worker.tls_trust.clone(),
            });

        let session_result = match ProductRelayClientV1::connect(&worker.identity, &relay_config) {
            Ok(mut relay) => {
                worker.overlay.record_relay_success(
                    relay.session().relay_peer_id.as_str(),
                    1,
                    now_ms_v1(),
                );
                consecutive_failures = 0;
                publish_product_mainline_overlay_event_v1(
                    &worker.events,
                    &worker.stop,
                    ProductMainlineOverlayEventV1::RelayConnected {
                        relay_peer_id: relay.session().relay_peer_id.clone(),
                    },
                )
                .context("publish product overlay relay-connected event")?;
                let result = if worker.role == ProductMainlineOverlayRoleV1::Duplex
                    && worker.remote_peers.len() > 1
                {
                    run_duplex_mesh_session_v1(&mut relay, &worker, &mut mesh_pending)
                } else {
                    run_authenticated_role_v1(&mut relay, &worker, &mut pending)
                };
                if worker.stop.load(Ordering::Acquire) {
                    let _ = relay.close();
                    return Ok(());
                }
                let _ = relay.close();
                result
            }
            Err(error) => Err(error.context("connect authenticated product relay")),
        };
        let error = match session_result {
            Ok(()) if worker.stop.load(Ordering::Acquire) => return Ok(()),
            Ok(()) => anyhow::anyhow!("product overlay relay session ended"),
            Err(error) => error,
        };
        consecutive_failures = consecutive_failures.saturating_add(1);
        let reconnect_in_ms = reconnect_delay_ms_v1(
            consecutive_failures,
            worker.reconnect_base_delay_ms,
            worker.reconnect_max_delay_ms,
        );
        let next_route = worker.overlay.record_relay_failure_and_rotate(
            &worker.identity,
            format!("mainline-overlay-rotate-{}", now_ms_v1()),
            &worker.remote_peer_id,
            &relay_peer_id,
            now_ms_v1(),
        );
        if let Some(next) = next_route.selected_relay.as_ref() {
            if next.relay_peer_id != relay_peer_id {
                publish_product_mainline_overlay_event_v1(
                    &worker.events,
                    &worker.stop,
                    ProductMainlineOverlayEventV1::RelayRotated {
                        previous_relay_peer_id: relay_peer_id.clone(),
                        next_relay_peer_id: next.relay_peer_id.clone(),
                    },
                )
                .context("publish product overlay relay-rotation event")?;
            }
        }
        publish_product_mainline_overlay_event_v1(
            &worker.events,
            &worker.stop,
            ProductMainlineOverlayEventV1::RelayDisconnected {
                relay_peer_id,
                error: error.to_string(),
                reconnect_in_ms,
            },
        )
        .context("publish product overlay relay-disconnected event")?;
        worker.route_plan = next_route;
        wait_for_reconnect_servicing_pending_v1(
            &worker,
            &mut pending,
            &mut mesh_pending,
            reconnect_in_ms,
        )?;
    }
    Ok(())
}

fn service_pending_resources_v1(
    worker: &ProductMainlineOverlayWorkerV1,
    pending: &mut VecDeque<ProductMainlineOverlayPendingV1>,
    pending_by_peer: &mut BTreeMap<String, VecDeque<ProductMainlineOverlayPendingV1>>,
    drain_limit: usize,
) -> Result<()> {
    if worker.role == ProductMainlineOverlayRoleV1::Duplex && worker.remote_peers.len() > 1 {
        drain_mesh_outbound_v1(worker, pending_by_peer, drain_limit)?;
        expire_mesh_pending_v1(worker, pending_by_peer, now_ms_v1())
    } else {
        drain_single_outbound_v1(worker, pending, drain_limit)?;
        expire_single_pending_v1(worker, pending, now_ms_v1())
    }
}

fn drain_mesh_outbound_v1(
    worker: &ProductMainlineOverlayWorkerV1,
    pending_by_peer: &mut BTreeMap<String, VecDeque<ProductMainlineOverlayPendingV1>>,
    limit: usize,
) -> Result<()> {
    for _ in 0..limit.max(1) {
        let mut outbound = match worker.outbound.try_recv() {
            Ok(outbound) => outbound,
            Err(TryRecvError::Empty | TryRecvError::Disconnected) => break,
        };
        if outbound.item.expired_at(now_ms_v1()) {
            publish_submission_expired_v1(worker, &outbound)?;
            continue;
        }
        for peer in &worker.remote_peers {
            let reservation = outbound
                .reservations
                .remove(&peer.peer_id)
                .context("mesh outbound lost its peer resource reservation")?;
            pending_by_peer
                .get_mut(&peer.peer_id)
                .context("mesh outbound target queue disappeared")?
                .push_back(ProductMainlineOverlayPendingV1 {
                    item: Arc::clone(&outbound.item),
                    _reservation: reservation,
                });
        }
        if !outbound.reservations.is_empty() {
            bail!("mesh outbound retained unknown peer resource reservations");
        }
    }
    Ok(())
}

fn drain_single_outbound_v1(
    worker: &ProductMainlineOverlayWorkerV1,
    pending: &mut VecDeque<ProductMainlineOverlayPendingV1>,
    limit: usize,
) -> Result<()> {
    for _ in 0..limit.max(1) {
        let mut outbound = match worker.outbound.try_recv() {
            Ok(outbound) => outbound,
            Err(TryRecvError::Empty | TryRecvError::Disconnected) => break,
        };
        if outbound.item.expired_at(now_ms_v1()) {
            publish_submission_expired_v1(worker, &outbound)?;
            continue;
        }
        let peer_id = worker
            .remote_peers
            .first()
            .context("single-peer product overlay has no remote peer")?
            .peer_id
            .clone();
        let reservation = outbound
            .reservations
            .remove(&peer_id)
            .context("single-peer outbound lost its resource reservation")?;
        if !outbound.reservations.is_empty() {
            bail!("single-peer outbound retained unknown resource reservations");
        }
        pending.push_back(ProductMainlineOverlayPendingV1 {
            item: outbound.item,
            _reservation: reservation,
        });
    }
    Ok(())
}

fn publish_submission_expired_v1(
    worker: &ProductMainlineOverlayWorkerV1,
    outbound: &ProductMainlineOverlayOutboundV1,
) -> Result<()> {
    let peer_ids = outbound.reservations.keys().cloned().collect::<Vec<_>>();
    for peer_id in peer_ids {
        publish_pending_expired_v1(worker, &outbound.item, &peer_id)?;
    }
    Ok(())
}

fn expire_mesh_pending_v1(
    worker: &ProductMainlineOverlayWorkerV1,
    pending_by_peer: &mut BTreeMap<String, VecDeque<ProductMainlineOverlayPendingV1>>,
    now_ms: u64,
) -> Result<()> {
    for peer in &worker.remote_peers {
        let queue = pending_by_peer
            .get_mut(&peer.peer_id)
            .context("mesh pending queue disappeared during expiry")?;
        while queue
            .front()
            .is_some_and(|pending| pending.item.expired_at(now_ms))
        {
            let pending = queue
                .pop_front()
                .context("expired mesh pending item disappeared")?;
            publish_pending_expired_v1(worker, &pending.item, &peer.peer_id)?;
        }
    }
    Ok(())
}

fn expire_single_pending_v1(
    worker: &ProductMainlineOverlayWorkerV1,
    pending: &mut VecDeque<ProductMainlineOverlayPendingV1>,
    now_ms: u64,
) -> Result<()> {
    let peer_id = worker
        .remote_peers
        .first()
        .context("single-peer product overlay has no remote peer")?
        .peer_id
        .clone();
    while pending
        .front()
        .is_some_and(|pending| pending.item.expired_at(now_ms))
    {
        let expired = pending
            .pop_front()
            .context("expired single-peer pending item disappeared")?;
        publish_pending_expired_v1(worker, &expired.item, &peer_id)?;
    }
    Ok(())
}

fn publish_pending_expired_v1(
    worker: &ProductMainlineOverlayWorkerV1,
    item: &ProductMainlineOverlayOutboundItemV1,
    peer_id: &str,
) -> Result<()> {
    let metric_peer_id = worker
        .remote_peers
        .iter()
        .find(|peer| peer.peer_id == peer_id)
        .context("expired pending item belongs to an unconfigured peer")?
        .metric_peer_id;
    publish_product_mainline_overlay_event_v1(
        &worker.events,
        &worker.stop,
        ProductMainlineOverlayEventV1::Delivery(
            ProductMainlineOverlayDeliveryV1 {
                payload_class: item.payload_class,
                object_hash: item.object_hash,
                remote_peer_id: peer_id.to_string(),
                metric_peer_id,
                delivered: false,
                error: Some(format!(
                    "product overlay pending delivery expired for remote peer {peer_id}: ttl_ms={} enqueued_at_ms={} expires_at_ms={}",
                    item.expires_at_ms.saturating_sub(item.enqueued_at_ms),
                    item.enqueued_at_ms,
                    item.expires_at_ms
                )),
            },
        ),
    )
        .context("publish expired product overlay delivery")
}

fn wait_for_reconnect_servicing_pending_v1(
    worker: &ProductMainlineOverlayWorkerV1,
    pending: &mut VecDeque<ProductMainlineOverlayPendingV1>,
    pending_by_peer: &mut BTreeMap<String, VecDeque<ProductMainlineOverlayPendingV1>>,
    delay_ms: u64,
) -> Result<()> {
    let mut remaining = delay_ms;
    while remaining > 0 && !worker.stop.load(Ordering::Acquire) {
        service_pending_resources_v1(worker, pending, pending_by_peer, 256)?;
        let slice = remaining.min(25);
        thread::sleep(Duration::from_millis(slice));
        remaining = remaining.saturating_sub(slice);
    }
    service_pending_resources_v1(worker, pending, pending_by_peer, 256)
}

fn run_duplex_mesh_session_v1(
    relay: &mut ProductRelayClientV1,
    worker: &ProductMainlineOverlayWorkerV1,
    pending_by_peer: &mut BTreeMap<String, VecDeque<ProductMainlineOverlayPendingV1>>,
) -> Result<()> {
    let local_peer_id =
        peer_id_from_ed25519_public_key_v1(&worker.identity.verifying_key().to_bytes());
    let mut peers = worker
        .remote_peers
        .iter()
        .map(|peer| {
            (
                peer.peer_id.clone(),
                ProductMainlineMeshPeerStateV1::default(),
            )
        })
        .collect::<BTreeMap<_, _>>();
    let preauth_budget = Arc::new(Mutex::new(ProductMainlineOverlayPreauthBudgetV1::new(
        worker.resource_limits.clone(),
    )));

    let mut last_heartbeat_ms = now_ms_v1();
    let mut next_outbound_peer_index = 0usize;
    while !worker.stop.load(Ordering::Acquire) {
        drain_mesh_outbound_v1(worker, pending_by_peer, 256)?;
        expire_mesh_pending_v1(worker, pending_by_peer, now_ms_v1())?;
        expire_mesh_preauth_v1(&mut peers, now_ms_v1());

        start_due_mesh_peer_handshakes_v1(relay, worker, &mut peers)?;

        let peer_count = worker.remote_peers.len();
        for offset in 0..peer_count {
            let peer_index = (next_outbound_peer_index + offset) % peer_count;
            let peer = &worker.remote_peers[peer_index];
            let Some(outbound) = pending_by_peer
                .get(&peer.peer_id)
                .and_then(|queue| queue.front())
                .map(|pending| Arc::clone(&pending.item))
            else {
                continue;
            };
            if outbound.expired_at(now_ms_v1()) {
                let expired = pending_by_peer
                    .get_mut(&peer.peer_id)
                    .and_then(VecDeque::pop_front)
                    .context("expired mesh pending item disappeared before send")?;
                publish_pending_expired_v1(worker, &expired.item, &peer.peer_id)?;
                next_outbound_peer_index = (peer_index + 1) % peer_count;
                break;
            }
            let Some(state) = peers.get(&peer.peer_id) else {
                continue;
            };
            let ProductMainlineMeshPeerPhaseV1::Active(_) = state.phase else {
                continue;
            };
            let frame_sequence = state.frame_sequence;
            let object_id =
                u64::from_le_bytes(outbound.object_hash[..8].try_into().unwrap_or_default());
            let encoded_payload = encode_classified_payload_v1(&outbound)?;
            let frame = NovoRudpTransportFrameV0::new(
                NovoRudpTransportFrameKindV0::Data,
                PRODUCT_MAINLINE_OVERLAY_SESSION_ID_V1,
                worker.chain_id,
                object_id,
                frame_sequence,
                0,
                encoded_payload,
            );
            let seal_result = {
                let state = peers
                    .get_mut(&peer.peer_id)
                    .context("configured mesh peer state disappeared")?;
                match &mut state.phase {
                    ProductMainlineMeshPeerPhaseV1::Active(channel) => {
                        channel.seal_novorudp_frame(&frame)
                    }
                    _ => continue,
                }
            };
            let envelope = match seal_result {
                Ok(envelope) => envelope,
                Err(error) => {
                    isolate_mesh_peer_v1(
                        &mut peers,
                        &peer.peer_id,
                        format!("seal outbound E2E payload: {error:#}"),
                        worker,
                    )?;
                    next_outbound_peer_index = (peer_index + 1) % peer_count;
                    break;
                }
            };
            let outcome = relay
                .send_envelope_with_outcome_v1(envelope)
                .context("send multi-peer product overlay payload; retained for reconnect")?;
            if outcome.disposition == RelayForwardDispositionV1::RejectedQueuePeerLimit {
                isolate_mesh_peer_v1(
                    &mut peers,
                    &peer.peer_id,
                    "relay rejected target-local offline queue admission",
                    worker,
                )?;
                next_outbound_peer_index = (peer_index + 1) % peer_count;
                break;
            }
            ensure_shared_relay_forward_accepted_v1(&outcome)
                .context("shared relay rejected multi-peer payload admission")?;
            peers
                .get_mut(&peer.peer_id)
                .context("configured mesh peer state disappeared after send")?
                .frame_sequence = frame_sequence.saturating_add(1);
            pending_by_peer
                .get_mut(&peer.peer_id)
                .and_then(VecDeque::pop_front)
                .context("multi-peer pending queue lost delivered payload")?;
            publish_product_mainline_overlay_event_v1(
                &worker.events,
                &worker.stop,
                ProductMainlineOverlayEventV1::Delivery(ProductMainlineOverlayDeliveryV1 {
                    payload_class: outbound.payload_class,
                    object_hash: outbound.object_hash,
                    remote_peer_id: peer.peer_id.clone(),
                    metric_peer_id: peer.metric_peer_id,
                    delivered: true,
                    error: None,
                }),
            )
            .context("publish multi-peer product overlay delivery")?;
            next_outbound_peer_index = (peer_index + 1) % peer_count;
            break;
        }

        let buffered_ready_peer = peers.iter().find_map(|(peer_id, state)| {
            let ProductMainlineMeshPeerPhaseV1::Active(channel) = &state.phase else {
                return None;
            };
            state
                .buffered_deliveries
                .front()
                .is_some_and(|delivery| {
                    delivery.delivery.envelope.session_id == channel.session_id()
                })
                .then(|| peer_id.clone())
        });
        if let Some(peer_id) = buffered_ready_peer {
            let buffered = peers
                .get_mut(&peer_id)
                .and_then(|state| state.buffered_deliveries.pop_front())
                .context("multi-peer buffered delivery disappeared")?;
            match open_mesh_inbound_v1(
                peers
                    .get_mut(&peer_id)
                    .context("configured mesh peer state disappeared")?,
                buffered.delivery,
                worker.chain_id,
            ) {
                Ok(inbound) => publish_product_mainline_overlay_event_v1(
                    &worker.events,
                    &worker.stop,
                    ProductMainlineOverlayEventV1::Inbound(inbound),
                )
                .context("publish buffered multi-peer product overlay inbound event")?,
                Err(error) => isolate_mesh_peer_v1(
                    &mut peers,
                    &peer_id,
                    format!("open buffered E2E payload: {error:#}"),
                    worker,
                )?,
            }
            continue;
        }

        let now_ms = now_ms_v1();
        if now_ms.saturating_sub(last_heartbeat_ms) >= 2_000 {
            relay
                .heartbeat()
                .context("send multi-peer product overlay relay heartbeat")?;
            last_heartbeat_ms = now_ms;
        }
        let Some(event) = recv_relay_event_or_idle_v1(relay)? else {
            continue;
        };
        match event {
            ProductRelayClientEventV1::PeerHandshake(delivery) => {
                let source_peer_id = delivery.source_peer_id.clone();
                let Some(state) = peers.get(&source_peer_id) else {
                    continue;
                };
                if matches!(state.phase, ProductMainlineMeshPeerPhaseV1::Cooldown { .. }) {
                    continue;
                }
                match delivery.handshake {
                    RelayPeerHandshakeV1::Offer(offer) => {
                        if offer.initiator_peer_id != source_peer_id {
                            isolate_mesh_peer_v1(
                                &mut peers,
                                &source_peer_id,
                                "relay source does not match signed handshake offer",
                                worker,
                            )?;
                            continue;
                        }
                        if matches!(
                            peers.get(&source_peer_id).map(|state| &state.phase),
                            Some(ProductMainlineMeshPeerPhaseV1::Handshaking { .. })
                        ) && local_peer_id < source_peer_id
                        {
                            continue;
                        }
                        let responder = {
                            let state = peers
                                .get_mut(&source_peer_id)
                                .context("configured mesh peer state disappeared")?;
                            NodeHandshakeResponderV1::respond(
                                &offer,
                                &worker.identity,
                                now_ms_v1(),
                                PRODUCT_MAINLINE_OVERLAY_PEER_HANDSHAKE_TTL_MS_V1,
                                &mut state.replay,
                            )
                        };
                        let responder = match responder {
                            Ok(responder) => responder,
                            Err(error) => {
                                isolate_mesh_peer_v1(
                                    &mut peers,
                                    &source_peer_id,
                                    format!("reject peer handshake offer: {error:#}"),
                                    worker,
                                )?;
                                continue;
                            }
                        };
                        let response = responder.response().clone();
                        let channel = responder.into_channel();
                        let outcome = relay.send_peer_handshake_with_outcome_v1(
                            source_peer_id.clone(),
                            RelayPeerHandshakeV1::Response(response),
                        )?;
                        if outcome.disposition == RelayForwardDispositionV1::RejectedQueuePeerLimit
                        {
                            isolate_mesh_peer_v1(
                                &mut peers,
                                &source_peer_id,
                                "relay rejected target-local peer response admission",
                                worker,
                            )?;
                            continue;
                        }
                        ensure_shared_relay_forward_accepted_v1(&outcome)
                            .context("shared relay rejected peer response admission")?;
                        activate_mesh_peer_v1(&mut peers, &source_peer_id, channel, worker)?;
                    }
                    RelayPeerHandshakeV1::Response(response) => {
                        if !mesh_peer_expects_handshake_session_v1(
                            peers
                                .get(&source_peer_id)
                                .context("configured mesh peer state disappeared")?,
                            response.session_id,
                        ) {
                            continue;
                        }
                        let initiator = {
                            let state = peers
                                .get_mut(&source_peer_id)
                                .context("configured mesh peer state disappeared")?;
                            match std::mem::replace(
                                &mut state.phase,
                                ProductMainlineMeshPeerPhaseV1::Idle,
                            ) {
                                ProductMainlineMeshPeerPhaseV1::Handshaking {
                                    initiator, ..
                                } => Some(initiator),
                                phase => {
                                    state.phase = phase;
                                    None
                                }
                            }
                        };
                        let Some(initiator) = initiator else {
                            continue;
                        };
                        let channel = {
                            let state = peers
                                .get_mut(&source_peer_id)
                                .context("configured mesh peer state disappeared")?;
                            initiator.complete(&response, now_ms_v1(), &mut state.replay)
                        };
                        match channel {
                            Ok(channel) => {
                                activate_mesh_peer_v1(&mut peers, &source_peer_id, channel, worker)?
                            }
                            Err(error) => isolate_mesh_peer_v1(
                                &mut peers,
                                &source_peer_id,
                                format!("reject peer handshake response: {error:#}"),
                                worker,
                            )?,
                        }
                    }
                }
            }
            ProductRelayClientEventV1::Delivery(delivery) => {
                let peer_id = delivery.source_peer_id.clone();
                let Some(state) = peers.get(&peer_id) else {
                    continue;
                };
                if matches!(state.phase, ProductMainlineMeshPeerPhaseV1::Cooldown { .. }) {
                    continue;
                }
                let matching_active_channel = matches!(
                    &state.phase,
                    ProductMainlineMeshPeerPhaseV1::Active(channel)
                        if channel.session_id() == delivery.envelope.session_id
                );
                let has_active_channel =
                    matches!(state.phase, ProductMainlineMeshPeerPhaseV1::Active(_));
                if matching_active_channel {
                    match open_mesh_inbound_v1(
                        peers
                            .get_mut(&peer_id)
                            .context("configured mesh peer state disappeared")?,
                        delivery,
                        worker.chain_id,
                    ) {
                        Ok(inbound) => publish_product_mainline_overlay_event_v1(
                            &worker.events,
                            &worker.stop,
                            ProductMainlineOverlayEventV1::Inbound(inbound),
                        )
                        .context("publish multi-peer product overlay inbound event")?,
                        Err(error) => isolate_mesh_peer_v1(
                            &mut peers,
                            &peer_id,
                            format!("open peer E2E payload: {error:#}"),
                            worker,
                        )?,
                    }
                } else if !has_active_channel
                    && mesh_peer_expects_handshake_session_v1(state, delivery.envelope.session_id)
                {
                    let buffer_result = buffer_preauth_delivery_v1(
                        &mut peers
                            .get_mut(&peer_id)
                            .context("configured mesh peer state disappeared")?
                            .buffered_deliveries,
                        delivery,
                        Some(&peer_id),
                        &preauth_budget,
                        worker.resource_limits.preauth_ttl_ms,
                    );
                    if let Err(error) = buffer_result {
                        isolate_mesh_peer_v1(
                            &mut peers,
                            &peer_id,
                            format!("buffer pre-auth peer payload: {error:#}"),
                            worker,
                        )?;
                    }
                }
            }
            ProductRelayClientEventV1::HeartbeatAck => {}
            ProductRelayClientEventV1::Closed => {
                bail!("product overlay relay closed the multi-peer session")
            }
        }
    }
    Ok(())
}

fn start_due_mesh_peer_handshakes_v1(
    relay: &mut ProductRelayClientV1,
    worker: &ProductMainlineOverlayWorkerV1,
    peers: &mut BTreeMap<String, ProductMainlineMeshPeerStateV1>,
) -> Result<()> {
    let now_ms = now_ms_v1();
    let peer_ids = peers.keys().cloned().collect::<Vec<_>>();
    for peer_id in peer_ids {
        let expired = matches!(
            peers.get(&peer_id).map(|state| &state.phase),
            Some(ProductMainlineMeshPeerPhaseV1::Handshaking { expires_at_ms, .. })
                if *expires_at_ms <= now_ms
        );
        if expired {
            isolate_mesh_peer_v1(
                peers,
                &peer_id,
                "peer handshake expired before authentication",
                worker,
            )?;
        }
        let handshake_due = match peers.get(&peer_id).map(|state| &state.phase) {
            Some(ProductMainlineMeshPeerPhaseV1::Idle) => true,
            Some(ProductMainlineMeshPeerPhaseV1::Cooldown { retry_at_ms }) => {
                *retry_at_ms <= now_ms
            }
            _ => false,
        };
        if !handshake_due {
            continue;
        }
        let initiator = NodeHandshakeInitiatorV1::start(
            &worker.identity,
            &peer_id,
            now_ms,
            PRODUCT_MAINLINE_OVERLAY_PEER_HANDSHAKE_TTL_MS_V1,
        )?;
        let outcome = relay.send_peer_handshake_with_outcome_v1(
            peer_id.clone(),
            RelayPeerHandshakeV1::Offer(initiator.offer().clone()),
        )?;
        if outcome.disposition == RelayForwardDispositionV1::RejectedQueuePeerLimit {
            isolate_mesh_peer_v1(
                peers,
                &peer_id,
                "relay rejected target-local peer offer admission",
                worker,
            )?;
            break;
        }
        ensure_shared_relay_forward_accepted_v1(&outcome)
            .context("shared relay rejected peer offer admission")?;
        peers
            .get_mut(&peer_id)
            .context("configured mesh peer state disappeared")?
            .phase = ProductMainlineMeshPeerPhaseV1::Handshaking {
            initiator,
            expires_at_ms: now_ms.saturating_add(PRODUCT_MAINLINE_OVERLAY_PEER_HANDSHAKE_TTL_MS_V1),
        };
        break;
    }
    Ok(())
}

fn ensure_shared_relay_forward_accepted_v1(outcome: &RelayForwardOutcomeV1) -> Result<()> {
    match outcome.disposition {
        RelayForwardDispositionV1::Forwarded
        | RelayForwardDispositionV1::QueuedTargetOffline
        | RelayForwardDispositionV1::QueuedBackpressure => Ok(()),
        disposition => bail!("relay forward admission rejected: {disposition:?}"),
    }
}

fn isolate_mesh_peer_v1(
    peers: &mut BTreeMap<String, ProductMainlineMeshPeerStateV1>,
    peer_id: &str,
    reason: impl Into<String>,
    worker: &ProductMainlineOverlayWorkerV1,
) -> Result<()> {
    let state = peers
        .get_mut(peer_id)
        .context("cannot isolate an unconfigured mesh peer")?;
    state.session_failure_count = state.session_failure_count.saturating_add(1);
    let retry_in_ms = reconnect_delay_ms_v1(
        state.session_failure_count,
        worker.reconnect_base_delay_ms,
        worker.reconnect_max_delay_ms,
    );
    state.phase = ProductMainlineMeshPeerPhaseV1::Cooldown {
        retry_at_ms: now_ms_v1().saturating_add(retry_in_ms),
    };
    state.buffered_deliveries.clear();
    state.frame_sequence = 0;
    let reason = bounded_peer_fault_reason_v1(reason.into());
    publish_product_mainline_overlay_event_v1(
        &worker.events,
        &worker.stop,
        ProductMainlineOverlayEventV1::PeerIsolated {
            remote_peer_id: peer_id.to_string(),
            reason,
            session_failure_count: state.session_failure_count,
            retry_in_ms,
        },
    )
    .context("publish mesh peer-isolated event")
}

fn activate_mesh_peer_v1(
    peers: &mut BTreeMap<String, ProductMainlineMeshPeerStateV1>,
    peer_id: &str,
    channel: E2eSecureChannelV1,
    worker: &ProductMainlineOverlayWorkerV1,
) -> Result<()> {
    let session_id = channel.session_id();
    let state = peers
        .get_mut(peer_id)
        .context("cannot activate an unconfigured mesh peer")?;
    state.buffered_deliveries.retain(|delivery| {
        !delivery.expired_at(now_ms_v1()) && delivery.delivery.envelope.session_id == session_id
    });
    state.phase = ProductMainlineMeshPeerPhaseV1::Active(channel);
    state.frame_sequence = 0;
    publish_product_mainline_overlay_event_v1(
        &worker.events,
        &worker.stop,
        ProductMainlineOverlayEventV1::E2eSessionEstablished {
            remote_peer_id: peer_id.to_string(),
        },
    )
    .context("publish multi-peer E2E-ready event")
}

fn expire_mesh_preauth_v1(
    peers: &mut BTreeMap<String, ProductMainlineMeshPeerStateV1>,
    now_ms: u64,
) {
    for state in peers.values_mut() {
        state
            .buffered_deliveries
            .retain(|delivery| !delivery.expired_at(now_ms));
    }
}

fn open_mesh_inbound_v1(
    state: &mut ProductMainlineMeshPeerStateV1,
    delivery: OpaqueRelayDeliveryV1,
    chain_id: u64,
) -> Result<ProductMainlineOverlayInboundV1> {
    let source_peer_id = delivery.source_peer_id;
    let ProductMainlineMeshPeerPhaseV1::Active(channel) = &mut state.phase else {
        bail!("multi-peer delivery arrived without an active E2E channel");
    };
    let frame = channel.open_novorudp_frame(&delivery.envelope)?;
    let (payload_class, object_hash, frame) = open_classified_inbound_frame_v1(frame, chain_id)?;
    Ok(ProductMainlineOverlayInboundV1 {
        payload_class,
        object_hash,
        source_peer_id,
        frame,
    })
}

fn mesh_peer_expects_handshake_session_v1(
    state: &ProductMainlineMeshPeerStateV1,
    session_id: [u8; 16],
) -> bool {
    matches!(
        &state.phase,
        ProductMainlineMeshPeerPhaseV1::Handshaking { initiator, .. }
            if initiator.offer().session_id == session_id
    )
}

fn bounded_peer_fault_reason_v1(mut reason: String) -> String {
    if reason.len() <= PRODUCT_MAINLINE_OVERLAY_PEER_FAULT_REASON_MAX_BYTES_V1 {
        return reason;
    }
    let mut boundary = PRODUCT_MAINLINE_OVERLAY_PEER_FAULT_REASON_MAX_BYTES_V1;
    while !reason.is_char_boundary(boundary) {
        boundary = boundary.saturating_sub(1);
    }
    reason.truncate(boundary);
    reason
}

fn run_authenticated_role_v1(
    relay: &mut ProductRelayClientV1,
    worker: &ProductMainlineOverlayWorkerV1,
    pending: &mut VecDeque<ProductMainlineOverlayPendingV1>,
) -> Result<()> {
    let preauth_budget = Arc::new(Mutex::new(ProductMainlineOverlayPreauthBudgetV1::new(
        worker.resource_limits.clone(),
    )));
    let remote_metric_peer_id = worker
        .remote_peers
        .first()
        .context("product overlay worker has no remote peer")?
        .metric_peer_id;
    let (channel, buffered_deliveries, allow_outbound, allow_inbound) = match worker.role {
        ProductMainlineOverlayRoleV1::Initiator => {
            let (channel, buffered) = establish_initiator_channel_v1(
                relay,
                &worker.identity,
                &worker.remote_peer_id,
                &worker.events,
                &worker.stop,
                &preauth_budget,
                worker.resource_limits.preauth_ttl_ms,
                worker,
                pending,
            )?;
            (channel, buffered, true, false)
        }
        ProductMainlineOverlayRoleV1::Responder => {
            let (channel, buffered) = establish_responder_channel_v1(
                relay,
                &worker.identity,
                worker.expected_source_peer_id.as_deref(),
                &worker.events,
                &worker.stop,
                &preauth_budget,
                worker.resource_limits.preauth_ttl_ms,
                worker,
                pending,
            )?;
            (channel, buffered, false, true)
        }
        ProductMainlineOverlayRoleV1::Duplex => {
            let local_peer_id =
                peer_id_from_ed25519_public_key_v1(&worker.identity.verifying_key().to_bytes());
            let (channel, buffered) = if local_peer_id < worker.remote_peer_id {
                establish_initiator_channel_v1(
                    relay,
                    &worker.identity,
                    &worker.remote_peer_id,
                    &worker.events,
                    &worker.stop,
                    &preauth_budget,
                    worker.resource_limits.preauth_ttl_ms,
                    worker,
                    pending,
                )?
            } else {
                establish_responder_channel_v1(
                    relay,
                    &worker.identity,
                    Some(&worker.remote_peer_id),
                    &worker.events,
                    &worker.stop,
                    &preauth_budget,
                    worker.resource_limits.preauth_ttl_ms,
                    worker,
                    pending,
                )?
            };
            (channel, buffered, true, true)
        }
    };
    run_authenticated_session_v1(
        relay,
        channel,
        worker,
        pending,
        buffered_deliveries,
        allow_outbound,
        allow_inbound,
        remote_metric_peer_id,
    )
}

#[allow(clippy::too_many_arguments)]
fn establish_initiator_channel_v1(
    relay: &mut ProductRelayClientV1,
    identity: &SigningKey,
    remote_peer_id: &str,
    events: &ProductMainlineOverlayEventSenderV1,
    stop: &AtomicBool,
    preauth_budget: &Arc<Mutex<ProductMainlineOverlayPreauthBudgetV1>>,
    preauth_ttl_ms: u64,
    worker: &ProductMainlineOverlayWorkerV1,
    pending: &mut VecDeque<ProductMainlineOverlayPendingV1>,
) -> Result<(
    E2eSecureChannelV1,
    VecDeque<ProductMainlineOverlayBufferedDeliveryV1>,
)> {
    let mut buffered_deliveries = VecDeque::new();
    let initiator = NodeHandshakeInitiatorV1::start(identity, remote_peer_id, now_ms_v1(), 30_000)?;
    relay.send_peer_handshake(
        remote_peer_id,
        RelayPeerHandshakeV1::Offer(initiator.offer().clone()),
    )?;
    let response = loop {
        if stop.load(Ordering::Acquire) {
            bail!("product overlay stopped while awaiting peer response");
        }
        drain_single_outbound_v1(worker, pending, 256)?;
        expire_single_pending_v1(worker, pending, now_ms_v1())?;
        expire_buffered_preauth_v1(&mut buffered_deliveries, now_ms_v1());
        if now_ms_v1() >= initiator.offer().expires_at_ms {
            bail!("product overlay peer handshake expired while awaiting response");
        }
        let Some(event) = recv_relay_event_or_idle_v1(relay)? else {
            continue;
        };
        match event {
            ProductRelayClientEventV1::PeerHandshake(delivery) => match delivery.handshake {
                RelayPeerHandshakeV1::Response(response) => break response,
                RelayPeerHandshakeV1::Offer(_) => {
                    bail!("product overlay initiator received an unexpected peer offer")
                }
            },
            ProductRelayClientEventV1::HeartbeatAck => {}
            ProductRelayClientEventV1::Closed => {
                bail!("product overlay relay closed before peer response")
            }
            ProductRelayClientEventV1::Delivery(delivery) => {
                buffer_preauth_delivery_v1(
                    &mut buffered_deliveries,
                    delivery,
                    Some(remote_peer_id),
                    preauth_budget,
                    preauth_ttl_ms,
                )?;
            }
        }
    };
    let mut replay = HandshakeReplayCacheV1::default();
    let channel = initiator.complete(&response, now_ms_v1(), &mut replay)?;
    publish_product_mainline_overlay_event_v1(
        events,
        stop,
        ProductMainlineOverlayEventV1::E2eSessionEstablished {
            remote_peer_id: remote_peer_id.into(),
        },
    )
    .context("publish product overlay E2E-ready event")?;
    Ok((channel, buffered_deliveries))
}

#[allow(clippy::too_many_arguments)]
fn establish_responder_channel_v1(
    relay: &mut ProductRelayClientV1,
    identity: &SigningKey,
    expected_source_peer_id: Option<&str>,
    events: &ProductMainlineOverlayEventSenderV1,
    stop: &AtomicBool,
    preauth_budget: &Arc<Mutex<ProductMainlineOverlayPreauthBudgetV1>>,
    preauth_ttl_ms: u64,
    worker: &ProductMainlineOverlayWorkerV1,
    pending: &mut VecDeque<ProductMainlineOverlayPendingV1>,
) -> Result<(
    E2eSecureChannelV1,
    VecDeque<ProductMainlineOverlayBufferedDeliveryV1>,
)> {
    let mut buffered_deliveries = VecDeque::new();
    let offer = loop {
        if stop.load(Ordering::Acquire) {
            bail!("product overlay stopped while awaiting peer offer");
        }
        drain_single_outbound_v1(worker, pending, 256)?;
        expire_single_pending_v1(worker, pending, now_ms_v1())?;
        expire_buffered_preauth_v1(&mut buffered_deliveries, now_ms_v1());
        let Some(event) = recv_relay_event_or_idle_v1(relay)? else {
            continue;
        };
        match event {
            ProductRelayClientEventV1::PeerHandshake(delivery) => match delivery.handshake {
                RelayPeerHandshakeV1::Offer(offer) => {
                    if expected_source_peer_id
                        .is_some_and(|expected| expected != offer.initiator_peer_id)
                    {
                        bail!("product overlay responder rejected unexpected source peer");
                    }
                    break offer;
                }
                RelayPeerHandshakeV1::Response(_) => {
                    bail!("product overlay responder received an unexpected peer response")
                }
            },
            ProductRelayClientEventV1::HeartbeatAck => {}
            ProductRelayClientEventV1::Closed => {
                bail!("product overlay relay closed before peer offer")
            }
            ProductRelayClientEventV1::Delivery(delivery) => {
                buffer_preauth_delivery_v1(
                    &mut buffered_deliveries,
                    delivery,
                    expected_source_peer_id,
                    preauth_budget,
                    preauth_ttl_ms,
                )?;
            }
        }
    };
    let source_peer_id = offer.initiator_peer_id.clone();
    let mut replay = HandshakeReplayCacheV1::default();
    let responder =
        NodeHandshakeResponderV1::respond(&offer, identity, now_ms_v1(), 30_000, &mut replay)?;
    let response = responder.response().clone();
    let channel = responder.into_channel();
    relay.send_peer_handshake(
        source_peer_id.clone(),
        RelayPeerHandshakeV1::Response(response),
    )?;
    publish_product_mainline_overlay_event_v1(
        events,
        stop,
        ProductMainlineOverlayEventV1::E2eSessionEstablished {
            remote_peer_id: source_peer_id.clone(),
        },
    )
    .context("publish product overlay E2E-ready event")?;
    Ok((channel, buffered_deliveries))
}

fn buffer_preauth_delivery_v1(
    buffered: &mut VecDeque<ProductMainlineOverlayBufferedDeliveryV1>,
    delivery: OpaqueRelayDeliveryV1,
    expected_source_peer_id: Option<&str>,
    budget: &Arc<Mutex<ProductMainlineOverlayPreauthBudgetV1>>,
    ttl_ms: u64,
) -> Result<()> {
    if expected_source_peer_id.is_some_and(|expected| expected != delivery.source_peer_id) {
        bail!("product overlay rejected pre-auth data from an unexpected source peer");
    }
    let source_peer_id = delivery.source_peer_id.clone();
    let bytes = serde_json::to_vec(&ProductRelayWireMessageV1::Delivery(delivery.clone()))
        .context("measure product overlay pre-auth relay delivery bytes")?
        .len();
    let reservation = try_reserve_preauth_v1(budget, &source_peer_id, bytes)
        .context("product overlay pre-auth count or byte resource limit exceeded")?;
    let buffered_at_ms = now_ms_v1();
    buffered.push_back(ProductMainlineOverlayBufferedDeliveryV1 {
        delivery,
        buffered_at_ms,
        ttl_ms,
        _reservation: reservation,
    });
    Ok(())
}

fn expire_buffered_preauth_v1(
    buffered: &mut VecDeque<ProductMainlineOverlayBufferedDeliveryV1>,
    now_ms: u64,
) {
    buffered.retain(|delivery| !delivery.expired_at(now_ms));
}

#[allow(clippy::too_many_arguments)]
fn run_authenticated_session_v1(
    relay: &mut ProductRelayClientV1,
    mut channel: E2eSecureChannelV1,
    worker: &ProductMainlineOverlayWorkerV1,
    pending: &mut VecDeque<ProductMainlineOverlayPendingV1>,
    mut buffered_deliveries: VecDeque<ProductMainlineOverlayBufferedDeliveryV1>,
    allow_outbound: bool,
    allow_inbound: bool,
    remote_metric_peer_id: u64,
) -> Result<()> {
    let source_peer_id = channel.remote_peer_id().to_string();
    let mut frame_sequence = 0u64;
    let mut last_heartbeat_ms = now_ms_v1();
    while !worker.stop.load(Ordering::Acquire) {
        if allow_outbound {
            drain_single_outbound_v1(worker, pending, 256)?;
            expire_single_pending_v1(worker, pending, now_ms_v1())?;
            if let Some(outbound) = pending.pop_front() {
                if outbound.item.expired_at(now_ms_v1()) {
                    publish_pending_expired_v1(worker, &outbound.item, &source_peer_id)?;
                    continue;
                }
                let object_id = u64::from_le_bytes(
                    outbound.item.object_hash[..8]
                        .try_into()
                        .unwrap_or_default(),
                );
                let encoded_payload = encode_classified_payload_v1(&outbound.item)?;
                let frame = NovoRudpTransportFrameV0::new(
                    NovoRudpTransportFrameKindV0::Data,
                    PRODUCT_MAINLINE_OVERLAY_SESSION_ID_V1,
                    worker.chain_id,
                    object_id,
                    frame_sequence,
                    0,
                    encoded_payload,
                );
                frame_sequence = frame_sequence.saturating_add(1);
                let send_result = channel
                    .seal_novorudp_frame(&frame)
                    .map_err(anyhow::Error::from)
                    .and_then(|envelope| relay.send_envelope(envelope));
                if let Err(error) = send_result {
                    pending.push_front(outbound);
                    return Err(error.context(
                        "send encrypted product overlay payload; transaction retained for reconnect",
                    ));
                }
                publish_product_mainline_overlay_event_v1(
                    &worker.events,
                    &worker.stop,
                    ProductMainlineOverlayEventV1::Delivery(ProductMainlineOverlayDeliveryV1 {
                        payload_class: outbound.item.payload_class,
                        object_hash: outbound.item.object_hash,
                        remote_peer_id: channel.remote_peer_id().to_string(),
                        metric_peer_id: remote_metric_peer_id,
                        delivered: true,
                        error: None,
                    }),
                )
                .context("publish product overlay delivery event")?;
            }
        }
        expire_buffered_preauth_v1(&mut buffered_deliveries, now_ms_v1());
        let now_ms = now_ms_v1();
        if now_ms.saturating_sub(last_heartbeat_ms) >= 2_000 {
            relay
                .heartbeat()
                .context("send product overlay relay heartbeat")?;
            last_heartbeat_ms = now_ms;
        }
        let event = if let Some(delivery) = buffered_deliveries.pop_front() {
            ProductRelayClientEventV1::Delivery(delivery.delivery)
        } else {
            let Some(event) = recv_relay_event_or_idle_v1(relay)? else {
                continue;
            };
            event
        };
        match event {
            ProductRelayClientEventV1::Delivery(delivery) => {
                if !allow_inbound {
                    bail!("product overlay send-only role received an inbound payload");
                }
                let frame = channel.open_novorudp_frame(&delivery.envelope)?;
                let (payload_class, object_hash, frame) =
                    open_classified_inbound_frame_v1(frame, worker.chain_id)?;
                publish_product_mainline_overlay_event_v1(
                    &worker.events,
                    &worker.stop,
                    ProductMainlineOverlayEventV1::Inbound(ProductMainlineOverlayInboundV1 {
                        payload_class,
                        object_hash,
                        source_peer_id: source_peer_id.clone(),
                        frame,
                    }),
                )
                .context("publish product overlay inbound event")?;
            }
            ProductRelayClientEventV1::HeartbeatAck => {}
            ProductRelayClientEventV1::Closed => {
                bail!("product overlay relay closed the authenticated session")
            }
            ProductRelayClientEventV1::PeerHandshake(_) => {
                bail!("product overlay received a second peer handshake on an active session")
            }
        }
    }
    Ok(())
}

fn validate_config_v1(config: &ProductMainlineOverlayConfigV1) -> Result<()> {
    if config.chain_id == 0 {
        bail!("product mainline overlay chain_id must be positive");
    }
    if config.channel_capacity == 0 {
        bail!("product mainline overlay channel_capacity must be positive");
    }
    if config.channel_capacity > 65_536 {
        bail!("product mainline overlay channel_capacity must not exceed 65536");
    }
    let limits = &config.resource_limits;
    if limits.pending_per_peer_count == 0
        || limits.pending_per_peer_bytes == 0
        || limits.pending_total_count == 0
        || limits.pending_total_bytes == 0
        || limits.pending_ttl_ms == 0
    {
        bail!("product mainline overlay pending resource limits must be positive");
    }
    if limits.pending_per_peer_count > limits.pending_total_count
        || limits.pending_per_peer_bytes > limits.pending_total_bytes
    {
        bail!("product mainline overlay per-peer pending limits must not exceed total limits");
    }
    if limits.event_total_bytes == 0 {
        bail!("product mainline overlay event_total_bytes must be positive");
    }
    let event_slot_max_bytes = PRODUCT_MAINLINE_OVERLAY_MAX_CLASSIFIED_LOGICAL_PAYLOAD_BYTES_V1
        .checked_add(PRODUCT_MAINLINE_OVERLAY_EVENT_SLOT_OVERHEAD_BYTES_V1)
        .context("product mainline overlay event slot byte ceiling overflow")?;
    if event_slot_max_bytes > limits.event_total_bytes {
        bail!(
            "product mainline overlay maximum event exceeds event_total_bytes: event={event_slot_max_bytes} limit={}",
            limits.event_total_bytes
        );
    }
    if limits.preauth_per_peer_count == 0
        || limits.preauth_per_peer_bytes == 0
        || limits.preauth_total_count == 0
        || limits.preauth_total_bytes == 0
        || limits.preauth_ttl_ms == 0
    {
        bail!("product mainline overlay pre-auth resource limits must be positive");
    }
    if limits.preauth_per_peer_count > limits.preauth_total_count
        || limits.preauth_per_peer_bytes > limits.preauth_total_bytes
    {
        bail!("product mainline overlay per-peer pre-auth limits must not exceed total limits");
    }
    if config.reconnect_base_delay_ms == 0
        || config.reconnect_max_delay_ms < config.reconnect_base_delay_ms
    {
        bail!("product mainline overlay reconnect delay policy is invalid");
    }
    if config.peers.len() > 64 {
        bail!("product mainline overlay supports at most 64 configured peers");
    }
    if !config.peers.is_empty() && config.role != ProductMainlineOverlayRoleV1::Duplex {
        bail!("product mainline overlay peers list requires the duplex role");
    }
    let mut peer_ids = BTreeSet::new();
    let mut metric_peer_ids = BTreeSet::new();
    for peer in &config.peers {
        if peer.peer_id.is_empty() {
            bail!("product mainline overlay peer_id must not be empty");
        }
        if peer.metric_peer_id == 0 {
            bail!("product mainline overlay metric_peer_id must be positive");
        }
        if !peer_ids.insert(peer.peer_id.as_str()) {
            bail!("product mainline overlay peer_id values must be unique");
        }
        if !metric_peer_ids.insert(peer.metric_peer_id) {
            bail!("product mainline overlay metric_peer_id values must be unique");
        }
    }
    match config.role {
        ProductMainlineOverlayRoleV1::Initiator => {
            if config.target_peer_id.as_deref().is_none_or(str::is_empty) {
                bail!("product mainline overlay initiator requires target_peer_id");
            }
        }
        ProductMainlineOverlayRoleV1::Responder => {
            if config
                .expected_source_peer_id
                .as_deref()
                .is_none_or(str::is_empty)
            {
                bail!("product mainline overlay responder requires expected_source_peer_id");
            }
        }
        ProductMainlineOverlayRoleV1::Duplex => {
            if config.peers.is_empty() {
                let target = config
                    .target_peer_id
                    .as_deref()
                    .filter(|target| !target.is_empty())
                    .context(
                        "product mainline overlay duplex role requires target_peer_id or peers",
                    )?;
                if config
                    .expected_source_peer_id
                    .as_deref()
                    .is_some_and(|expected| expected != target)
                {
                    bail!(
                        "product mainline overlay duplex expected_source_peer_id must match target_peer_id"
                    );
                }
            } else if config.expected_source_peer_id.is_some() {
                bail!(
                    "product mainline overlay multi-peer duplex must not set expected_source_peer_id"
                );
            }
        }
    }
    Ok(())
}

fn recv_relay_event_or_idle_v1(
    relay: &mut ProductRelayClientV1,
) -> Result<Option<ProductRelayClientEventV1>> {
    match relay.recv_event() {
        Ok(event) => Ok(Some(event)),
        Err(error) if relay_read_timed_out_v1(&error) => Ok(None),
        Err(error) => Err(error),
    }
}

fn relay_read_timed_out_v1(error: &anyhow::Error) -> bool {
    error.chain().any(|cause| {
        cause.downcast_ref::<std::io::Error>().is_some_and(|io| {
            matches!(
                io.kind(),
                std::io::ErrorKind::WouldBlock | std::io::ErrorKind::TimedOut
            )
        })
    })
}

fn configured_remote_peer_id_v1(config: &ProductMainlineOverlayConfigV1) -> Result<&str> {
    match config.role {
        ProductMainlineOverlayRoleV1::Initiator => config
            .target_peer_id
            .as_deref()
            .context("product overlay initiator target peer is missing"),
        ProductMainlineOverlayRoleV1::Responder => config
            .expected_source_peer_id
            .as_deref()
            .context("product overlay responder source peer is missing"),
        ProductMainlineOverlayRoleV1::Duplex => config
            .target_peer_id
            .as_deref()
            .context("product overlay duplex target peer is missing"),
    }
}

fn configured_remote_peers_v1(
    config: &ProductMainlineOverlayConfigV1,
) -> Result<Vec<ProductMainlineOverlayPeerConfigV1>> {
    if !config.peers.is_empty() {
        let mut peers = config.peers.clone();
        peers.sort_by(|left, right| left.peer_id.cmp(&right.peer_id));
        return Ok(peers);
    }
    Ok(vec![ProductMainlineOverlayPeerConfigV1 {
        peer_id: configured_remote_peer_id_v1(config)?.to_string(),
        metric_peer_id: config.metric_peer_id,
    }])
}

fn reconnect_delay_ms_v1(consecutive_failures: u32, base_delay_ms: u64, max_delay_ms: u64) -> u64 {
    let exponent = consecutive_failures.saturating_sub(1).min(16);
    base_delay_ms
        .saturating_mul(1u64 << exponent)
        .min(max_delay_ms)
}

fn validate_inbound_frame_v1(frame: &NovoRudpTransportFrameV0, chain_id: u64) -> Result<()> {
    if frame.kind != NovoRudpTransportFrameKindV0::Data {
        bail!("product mainline overlay accepts only NovoRUDP data frames");
    }
    if frame.stream_id != chain_id {
        bail!(
            "product mainline overlay frame chain {} does not match node chain {}",
            frame.stream_id,
            chain_id
        );
    }
    if frame.payload.is_empty() {
        bail!("product mainline overlay received an empty transaction payload");
    }
    Ok(())
}

fn load_ed25519_identity_v1(path: &Path) -> Result<SigningKey> {
    let text = fs::read_to_string(path)
        .with_context(|| format!("read product mainline identity key: {}", path.display()))?;
    let text = text.trim();
    if text.len() != 64 {
        bail!("product mainline identity key must contain exactly 64 hexadecimal characters");
    }
    let mut secret = [0u8; 32];
    for (index, output) in secret.iter_mut().enumerate() {
        *output = u8::from_str_radix(&text[index * 2..index * 2 + 2], 16)
            .context("decode product mainline identity key hex")?;
    }
    Ok(SigningKey::from_bytes(&secret))
}

fn now_ms_v1() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as u64
}

fn default_connect_timeout_ms_v1() -> u64 {
    5_000
}

fn default_read_timeout_ms_v1() -> u64 {
    250
}

fn default_tls_trust_v1() -> ProductRelayTlsTrustV1 {
    ProductRelayTlsTrustV1::NativeWebPki
}

fn default_channel_capacity_v1() -> usize {
    1_024
}

fn default_metric_peer_id_v1() -> u64 {
    9_990_777
}

fn default_reconnect_base_delay_ms_v1() -> u64 {
    250
}

fn default_reconnect_max_delay_ms_v1() -> u64 {
    30_000
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        product_node_overlay::ProductBootstrapSourceV1,
        product_relay_daemon::{run_product_relay_daemon_v1, ProductRelayDaemonConfigV1},
        tx_ingress::sign_nov_native_tx_with_seed_v1,
    };
    use novovm_network::{
        sign_bootstrap_manifest_v1, sign_relay_record_v1, BootstrapSourceKindV1,
        PeerSignedRelayRecordV1, RelayEndpointV1, SecureNovoRudpEnvelopeV1,
        SignedBootstrapManifestV1,
    };
    use novovm_protocol::{
        encode_nov_native_tx_wire_v1, NovExecuteTxV1, NovExecutionModeV1, NovExecutionPolicyV1,
        NovExecutionTargetV1, NovFeePolicyV1, NovNativeTxWireV1, NovPrivacyModeV1, NovTxKindV1,
        NovVerificationModeV1,
    };
    use std::{net::TcpListener, thread, time::Instant};

    fn config(role: ProductMainlineOverlayRoleV1) -> ProductMainlineOverlayConfigV1 {
        ProductMainlineOverlayConfigV1 {
            chain_id: 7,
            role,
            identity_key_path: PathBuf::from("identity.hex"),
            overlay: ProductNodeOverlayConfigV1 {
                cache_path: PathBuf::from("cache.json"),
                trusted_signer_public_keys: Vec::new(),
                minimum_bootstrap_signatures: 1,
                embedded_sources: Vec::new(),
                cooldown_base_ms: None,
                cooldown_max_ms: None,
            },
            target_peer_id: None,
            expected_source_peer_id: None,
            peers: Vec::new(),
            connect_timeout_ms: 5_000,
            read_timeout_ms: 250,
            tls_trust: ProductRelayTlsTrustV1::NodeKeyBoundEncrypted,
            channel_capacity: 16,
            resource_limits: ProductMainlineOverlayResourceLimitsV1::default(),
            metric_peer_id: 91,
            reconnect_base_delay_ms: 10,
            reconnect_max_delay_ms: 100,
        }
    }

    fn outbound_item_v1(
        payload_class: ProductMainlineOverlayPayloadClassV1,
        object_hash: [u8; 32],
        payload: Vec<u8>,
    ) -> ProductMainlineOverlayOutboundItemV1 {
        ProductMainlineOverlayOutboundItemV1 {
            payload_class,
            object_hash,
            payload: Arc::from(payload),
            enqueued_at_ms: 0,
            expires_at_ms: u64::MAX,
        }
    }

    #[test]
    fn product_mainline_pending_fanout_reservation_is_atomic_bounded_and_shared() {
        let limits = ProductMainlineOverlayResourceLimitsV1 {
            pending_per_peer_count: 2,
            pending_per_peer_bytes: 8,
            pending_total_count: 4,
            pending_total_bytes: 16,
            ..ProductMainlineOverlayResourceLimitsV1::default()
        };
        let budget = Arc::new(Mutex::new(ProductMainlineOverlayPendingBudgetV1::new(
            limits,
        )));
        let peers = vec!["peer-a".to_string(), "peer-b".to_string()];

        let first = try_reserve_pending_fanout_v1(&budget, &peers, 4).unwrap();
        let second = try_reserve_pending_fanout_v1(&budget, &peers, 4).unwrap();
        assert!(try_reserve_pending_fanout_v1(&budget, &peers, 1).is_none());
        {
            let state = budget.lock().unwrap();
            assert_eq!(state.total.count, 4);
            assert_eq!(state.total.bytes, 16);
            assert_eq!(state.by_peer["peer-a"].count, 2);
            assert_eq!(state.by_peer["peer-b"].bytes, 8);
        }

        let shared = Arc::new(outbound_item_v1(
            ProductMainlineOverlayPayloadClassV1::NativeTransaction,
            [0x31; 32],
            vec![1, 2, 3, 4],
        ));
        let peer_a_payload = Arc::clone(&shared);
        let peer_b_payload = Arc::clone(&shared);
        assert!(Arc::ptr_eq(&peer_a_payload, &peer_b_payload));

        drop(first);
        assert!(try_reserve_pending_fanout_v1(&budget, &peers, 5).is_none());
        drop(second);
        let state = budget.lock().unwrap();
        assert_eq!(state.total.count, 0);
        assert_eq!(state.total.bytes, 0);
        assert!(state.by_peer.is_empty());
    }

    #[test]
    fn product_mainline_preauth_uses_local_age_and_shared_count_byte_bounds() {
        let delivery = |source: &str, ciphertext_len: usize| OpaqueRelayDeliveryV1 {
            source_peer_id: source.to_string(),
            target_peer_id: "local-peer".into(),
            received_at_ms: u64::MAX,
            envelope: SecureNovoRudpEnvelopeV1 {
                version: 1,
                session_id: [1; 16],
                sender_peer_id: source.to_string(),
                recipient_peer_id: "local-peer".into(),
                sequence: 0,
                nonce: [0; 12],
                ciphertext: vec![0x5a; ciphertext_len],
            },
        };
        let one_delivery_bytes =
            serde_json::to_vec(&ProductRelayWireMessageV1::Delivery(delivery("peer-a", 6)))
                .unwrap()
                .len();
        let limits = ProductMainlineOverlayResourceLimitsV1 {
            preauth_per_peer_count: 2,
            preauth_per_peer_bytes: one_delivery_bytes,
            preauth_total_count: 3,
            preauth_total_bytes: one_delivery_bytes * 2,
            preauth_ttl_ms: 5,
            ..ProductMainlineOverlayResourceLimitsV1::default()
        };
        let budget = Arc::new(Mutex::new(ProductMainlineOverlayPreauthBudgetV1::new(
            limits,
        )));
        let mut peer_a = VecDeque::new();
        let mut peer_b = VecDeque::new();
        let mut peer_c = VecDeque::new();
        buffer_preauth_delivery_v1(
            &mut peer_a,
            delivery("peer-a", 6),
            Some("peer-a"),
            &budget,
            5,
        )
        .unwrap();
        assert!(buffer_preauth_delivery_v1(
            &mut peer_a,
            delivery("peer-a", 5),
            Some("peer-a"),
            &budget,
            5,
        )
        .is_err());
        buffer_preauth_delivery_v1(
            &mut peer_b,
            delivery("peer-b", 6),
            Some("peer-b"),
            &budget,
            5,
        )
        .unwrap();
        assert!(buffer_preauth_delivery_v1(
            &mut peer_c,
            delivery("peer-c", 4),
            Some("peer-c"),
            &budget,
            5,
        )
        .is_err());
        {
            let state = budget.lock().unwrap();
            assert_eq!(state.total.count, 2);
            assert_eq!(state.total.bytes, one_delivery_bytes * 2);
        }

        expire_buffered_preauth_v1(&mut peer_a, u64::MAX);
        expire_buffered_preauth_v1(&mut peer_b, u64::MAX);
        assert!(peer_a.is_empty() && peer_b.is_empty());
        let state = budget.lock().unwrap();
        assert_eq!(state.total.count, 0);
        assert_eq!(state.total.bytes, 0);
        assert!(state.by_peer.is_empty());
        drop(state);

        let count_limits = ProductMainlineOverlayResourceLimitsV1 {
            preauth_per_peer_count: 1,
            preauth_per_peer_bytes: usize::MAX,
            preauth_total_count: 2,
            preauth_total_bytes: usize::MAX,
            preauth_ttl_ms: 5,
            ..ProductMainlineOverlayResourceLimitsV1::default()
        };
        let count_budget = Arc::new(Mutex::new(ProductMainlineOverlayPreauthBudgetV1::new(
            count_limits,
        )));
        let mut count_peer_a = VecDeque::new();
        let mut count_peer_b = VecDeque::new();
        let mut count_peer_c = VecDeque::new();
        buffer_preauth_delivery_v1(
            &mut count_peer_a,
            delivery("peer-a", 1),
            Some("peer-a"),
            &count_budget,
            5,
        )
        .unwrap();
        assert!(buffer_preauth_delivery_v1(
            &mut count_peer_a,
            delivery("peer-a", 1),
            Some("peer-a"),
            &count_budget,
            5,
        )
        .is_err());
        buffer_preauth_delivery_v1(
            &mut count_peer_b,
            delivery("peer-b", 1),
            Some("peer-b"),
            &count_budget,
            5,
        )
        .unwrap();
        assert!(buffer_preauth_delivery_v1(
            &mut count_peer_c,
            delivery("peer-c", 1),
            Some("peer-c"),
            &count_budget,
            5,
        )
        .is_err());
        {
            let state = count_budget.lock().unwrap();
            assert_eq!(state.total.count, 2);
            assert_eq!(state.by_peer["peer-a"].count, 1);
            assert_eq!(state.by_peer["peer-b"].count, 1);
        }
        expire_buffered_preauth_v1(&mut count_peer_a, u64::MAX);
        expire_buffered_preauth_v1(&mut count_peer_b, u64::MAX);
        let state = count_budget.lock().unwrap();
        assert_eq!(state.total.count, 0);
        assert_eq!(state.total.bytes, 0);
        assert!(state.by_peer.is_empty());
    }

    #[test]
    fn product_mainline_disconnected_pending_expires_once_without_true_delivery() {
        let now = now_ms_v1();
        let root =
            std::env::temp_dir().join(format!("novovm-product-overlay-pending-expiry-{now}"));
        fs::create_dir_all(&root).unwrap();
        let local_identity_path = root.join("local.hex");
        write_identity_v1(&local_identity_path, [0x34; 32]);
        let relay_identity = SigningKey::from_bytes(&[0x35; 32]);
        let remote_identity = SigningKey::from_bytes(&[0x36; 32]);
        let remote_peer_id =
            peer_id_from_ed25519_public_key_v1(&remote_identity.verifying_key().to_bytes());
        let relay_peer_id =
            peer_id_from_ed25519_public_key_v1(&relay_identity.verifying_key().to_bytes());
        let bootstrap_signer = SigningKey::from_bytes(&[0x37; 32]);
        let source = signed_bootstrap_source_v1(
            &bootstrap_signer,
            &relay_identity,
            now.saturating_sub(1_000),
            now.saturating_add(30_000),
        );
        let chain_id = 8_400_000 + now % 100_000;
        let mut runtime_config = mainline_config_v1(
            chain_id,
            ProductMainlineOverlayRoleV1::Duplex,
            local_identity_path,
            root.join("cache.json"),
            bootstrap_signer.verifying_key().to_bytes(),
            source,
            Some(remote_peer_id.clone()),
            None,
        );
        runtime_config.connect_timeout_ms = 20;
        runtime_config.reconnect_base_delay_ms = 10;
        runtime_config.reconnect_max_delay_ms = 20;
        runtime_config.resource_limits = ProductMainlineOverlayResourceLimitsV1 {
            pending_per_peer_count: 1,
            pending_per_peer_bytes: 1024 * 1024,
            pending_total_count: 1,
            pending_total_bytes: 1024 * 1024,
            pending_ttl_ms: 200,
            ..ProductMainlineOverlayResourceLimitsV1::default()
        };
        let dead_port = TcpListener::bind("127.0.0.1:0")
            .unwrap()
            .local_addr()
            .unwrap()
            .port();
        let relay_override = ProductRelayClientConfigV1 {
            endpoint: format!("wss://127.0.0.1:{dead_port}/novovm"),
            expected_relay_peer_id: relay_peer_id,
            connect_timeout_ms: 20,
            read_timeout_ms: 20,
            tls_trust: ProductRelayTlsTrustV1::NodeKeyBoundEncrypted,
        };
        let mut runtime = ProductMainlineOverlayRuntimeV1::start_with_relay_override_v1(
            runtime_config,
            now_ms_v1(),
            relay_override,
        )
        .unwrap();
        let object_hash = [0x38; 32];
        assert!(runtime
            .try_submit(
                object_hash,
                signed_native_tx_v1(chain_id, &format!("pending-expiry-{now}")),
            )
            .unwrap());
        assert!(!runtime.try_submit([0x39; 32], vec![0x39]).unwrap());

        let started = Instant::now();
        let mut expiration = None;
        while started.elapsed() < Duration::from_secs(2) && expiration.is_none() {
            for event in runtime.drain_events(64) {
                match event {
                    ProductMainlineOverlayEventV1::Delivery(delivery)
                        if delivery.object_hash == object_hash =>
                    {
                        assert!(!delivery.delivered, "expired delivery was reported true");
                        assert!(delivery
                            .error
                            .as_deref()
                            .is_some_and(|error| error.contains("expired")));
                        expiration = Some(delivery);
                    }
                    ProductMainlineOverlayEventV1::WorkerFailed(error) => {
                        panic!("resource expiry worker failed: {error}")
                    }
                    _ => {}
                }
            }
            thread::sleep(Duration::from_millis(5));
        }
        assert!(
            expiration.is_some(),
            "disconnected pending item did not expire"
        );
        wait_until_v1(Duration::from_secs(1), || {
            runtime.pending_resource_usage_v1().pending_count == 0
                && runtime.pending_resource_usage_v1().pending_bytes == 0
        });

        runtime.shutdown();
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn product_mainline_shutdown_returns_when_event_channel_is_full() {
        let now = now_ms_v1();
        let root =
            std::env::temp_dir().join(format!("novovm-product-overlay-full-event-shutdown-{now}"));
        fs::create_dir_all(&root).unwrap();
        let local_identity_path = root.join("local.hex");
        write_identity_v1(&local_identity_path, [0x41; 32]);
        let relay_identity = SigningKey::from_bytes(&[0x42; 32]);
        let remote_identity = SigningKey::from_bytes(&[0x43; 32]);
        let remote_peer_id =
            peer_id_from_ed25519_public_key_v1(&remote_identity.verifying_key().to_bytes());
        let relay_peer_id =
            peer_id_from_ed25519_public_key_v1(&relay_identity.verifying_key().to_bytes());
        let bootstrap_signer = SigningKey::from_bytes(&[0x44; 32]);
        let source = signed_bootstrap_source_v1(
            &bootstrap_signer,
            &relay_identity,
            now.saturating_sub(1_000),
            now.saturating_add(30_000),
        );
        let mut runtime_config = mainline_config_v1(
            8_500_000 + now % 100_000,
            ProductMainlineOverlayRoleV1::Duplex,
            local_identity_path,
            root.join("cache.json"),
            bootstrap_signer.verifying_key().to_bytes(),
            source,
            Some(remote_peer_id),
            None,
        );
        runtime_config.channel_capacity = 1;
        runtime_config.connect_timeout_ms = 20;
        runtime_config.reconnect_base_delay_ms = 10;
        runtime_config.reconnect_max_delay_ms = 10;

        let dead_port = TcpListener::bind("127.0.0.1:0")
            .unwrap()
            .local_addr()
            .unwrap()
            .port();
        let relay_override = ProductRelayClientConfigV1 {
            endpoint: format!("wss://127.0.0.1:{dead_port}/novovm"),
            expected_relay_peer_id: relay_peer_id,
            connect_timeout_ms: 20,
            read_timeout_ms: 20,
            tls_trust: ProductRelayTlsTrustV1::NodeKeyBoundEncrypted,
        };
        let runtime = ProductMainlineOverlayRuntimeV1::start_with_relay_override_v1(
            runtime_config,
            now_ms_v1(),
            relay_override,
        )
        .unwrap();

        thread::sleep(Duration::from_millis(150));
        let (shutdown_complete_tx, shutdown_complete_rx) = mpsc::sync_channel(1);
        let shutdown = thread::spawn(move || {
            let mut runtime = runtime;
            runtime.shutdown();
            let _ = shutdown_complete_tx.send(());
        });
        assert!(
            shutdown_complete_rx
                .recv_timeout(Duration::from_secs(2))
                .is_ok(),
            "full product overlay event channel blocked shutdown"
        );
        shutdown.join().unwrap();
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn lifecycle_role_requires_an_authenticated_remote_peer() {
        let initiator = config(ProductMainlineOverlayRoleV1::Initiator);
        assert!(validate_config_v1(&initiator)
            .unwrap_err()
            .to_string()
            .contains("target_peer_id"));

        let responder = config(ProductMainlineOverlayRoleV1::Responder);
        assert!(validate_config_v1(&responder)
            .unwrap_err()
            .to_string()
            .contains("expected_source_peer_id"));

        let duplex = config(ProductMainlineOverlayRoleV1::Duplex);
        assert!(validate_config_v1(&duplex)
            .unwrap_err()
            .to_string()
            .contains("target_peer_id"));

        let mut mesh = config(ProductMainlineOverlayRoleV1::Duplex);
        mesh.peers = vec![
            ProductMainlineOverlayPeerConfigV1 {
                peer_id: "peer-a".into(),
                metric_peer_id: 1,
            },
            ProductMainlineOverlayPeerConfigV1 {
                peer_id: "peer-b".into(),
                metric_peer_id: 1,
            },
        ];
        assert!(validate_config_v1(&mesh)
            .unwrap_err()
            .to_string()
            .contains("metric_peer_id values must be unique"));
    }

    #[test]
    fn product_mainline_default_tls_trust_requires_native_web_pki() {
        let decoded: ProductMainlineOverlayConfigV1 = serde_json::from_value(serde_json::json!({
            "chain_id": 7,
            "role": "duplex",
            "identity_key_path": "node.hex",
            "target_peer_id": "peer-b",
            "overlay": {
                "cache_path": "bootstrap-cache.json",
                "trusted_signer_public_keys": [],
                "embedded_sources": []
            }
        }))
        .unwrap();
        assert!(matches!(
            decoded.tls_trust,
            ProductRelayTlsTrustV1::NativeWebPki
        ));
    }

    #[test]
    fn product_mainline_event_channel_has_checked_byte_ceiling() {
        let mut bounded_default = config(ProductMainlineOverlayRoleV1::Initiator);
        bounded_default.target_peer_id = Some("peer-a".into());
        bounded_default.channel_capacity = default_channel_capacity_v1();
        assert!(validate_config_v1(&bounded_default).is_ok());

        let mut oversized = bounded_default.clone();
        oversized.channel_capacity = 65_536;
        assert!(validate_config_v1(&oversized).is_ok());

        let mut overflow = bounded_default;
        overflow.channel_capacity = usize::MAX;
        let error = validate_config_v1(&overflow).unwrap_err().to_string();
        assert!(error.contains("channel_capacity must not exceed"));

        oversized.resource_limits.event_total_bytes = 1;
        let error = validate_config_v1(&oversized).unwrap_err().to_string();
        assert!(error.contains("maximum event exceeds"));
    }

    #[test]
    fn omitted_resource_limits_preserve_legacy_channel_capacity() {
        let legacy_json = serde_json::json!({
            "chain_id": 7,
            "role": "duplex",
            "identity_key_path": "node.hex",
            "target_peer_id": "peer-b",
            "overlay": {
                "cache_path": "bootstrap-cache.json",
                "trusted_signer_public_keys": [],
                "embedded_sources": []
            },
            "channel_capacity": 4096
        });
        let decoded: ProductMainlineOverlayConfigV1 =
            serde_json::from_value(legacy_json.clone()).unwrap();
        assert_eq!(
            decoded.resource_limits.event_total_bytes,
            ProductMainlineOverlayResourceLimitsV1::default().event_total_bytes
        );
        validate_config_v1(&decoded).unwrap();

        let mut explicit_json = legacy_json;
        explicit_json["resource_limits"] =
            serde_json::to_value(ProductMainlineOverlayResourceLimitsV1::default()).unwrap();
        let explicit: ProductMainlineOverlayConfigV1 =
            serde_json::from_value(explicit_json).unwrap();
        validate_config_v1(&explicit).unwrap();
    }

    #[test]
    fn event_byte_permit_tracks_actual_owned_payload_until_receive() {
        let (sender, receiver) = mpsc::sync_channel(2);
        let bytes_in_flight = Arc::new(AtomicUsize::new(0));
        let events = ProductMainlineOverlayEventSenderV1 {
            sender,
            bytes_in_flight: Arc::clone(&bytes_in_flight),
            max_bytes: 1024 * 1024,
        };
        let stop = AtomicBool::new(false);
        let event = ProductMainlineOverlayEventV1::Inbound(ProductMainlineOverlayInboundV1 {
            payload_class: ProductMainlineOverlayPayloadClassV1::NativeTransaction,
            object_hash: [7; 32],
            source_peer_id: "peer-a".into(),
            frame: NovoRudpTransportFrameV0::new(
                NovoRudpTransportFrameKindV0::Data,
                [1; 16],
                7,
                8,
                9,
                10,
                vec![11; 4_096],
            ),
        });
        let expected = product_mainline_overlay_event_owned_bytes_v1(&event);
        publish_product_mainline_overlay_event_v1(&events, &stop, event).unwrap();
        assert_eq!(bytes_in_flight.load(Ordering::Acquire), expected);
        let accounted = receiver.recv().unwrap();
        assert!(matches!(
            accounted.event,
            ProductMainlineOverlayEventV1::Inbound(_)
        ));
        drop(accounted);
        assert_eq!(bytes_in_flight.load(Ordering::Acquire), 0);

        let isolated = ProductMainlineOverlayEventV1::PeerIsolated {
            remote_peer_id: String::with_capacity(128),
            reason: String::with_capacity(4_096),
            session_failure_count: 1,
            retry_in_ms: 50,
        };
        assert!(
            product_mainline_overlay_event_owned_bytes_v1(&isolated)
                >= std::mem::size_of::<ProductMainlineOverlayAccountedEventV1>() + 4_224
        );
    }

    #[test]
    fn product_mainline_mesh_filters_stale_handshake_generation_traffic() {
        let local_identity = SigningKey::from_bytes(&[0x68; 32]);
        let remote_identity = SigningKey::from_bytes(&[0x69; 32]);
        let remote_peer_id =
            peer_id_from_ed25519_public_key_v1(&remote_identity.verifying_key().to_bytes());
        let initiator = NodeHandshakeInitiatorV1::start(
            &local_identity,
            remote_peer_id,
            1_000,
            PRODUCT_MAINLINE_OVERLAY_PEER_HANDSHAKE_TTL_MS_V1,
        )
        .unwrap();
        let current_session_id = initiator.offer().session_id;
        let mut stale_session_id = current_session_id;
        stale_session_id[0] ^= 0xff;
        let mut state = ProductMainlineMeshPeerStateV1 {
            phase: ProductMainlineMeshPeerPhaseV1::Handshaking {
                initiator,
                expires_at_ms: 2_000,
            },
            ..ProductMainlineMeshPeerStateV1::default()
        };

        assert!(mesh_peer_expects_handshake_session_v1(
            &state,
            current_session_id
        ));
        assert!(!mesh_peer_expects_handshake_session_v1(
            &state,
            stale_session_id
        ));

        state.phase = ProductMainlineMeshPeerPhaseV1::Cooldown { retry_at_ms: 3_000 };
        assert!(!mesh_peer_expects_handshake_session_v1(
            &state,
            current_session_id
        ));
    }

    #[test]
    fn product_mainline_ingress_failure_classification_is_conservative() {
        let peer_failure =
            ingest_product_mainline_overlay_peer_payload_v1(7, b"not-a-native-transaction")
                .unwrap_err();
        assert_eq!(
            peer_failure.class,
            ProductMainlineOverlayIngressFailureClassV1::PeerRejected
        );

        for local_error in [
            anyhow::anyhow!("load durable native authentication state failed: disk unavailable"),
            anyhow::anyhow!("nov native authentication nonce registry poisoned"),
            anyhow::anyhow!(
                "nov native authentication rejected: invalid NOVOVM_NATIVE_CHAIN_ID: bad config"
            ),
            anyhow::anyhow!(
                "nov native authentication rejected: configured chain domain mismatch configured=8 signed=7"
            ),
            anyhow::anyhow!("unknown future verifier failure"),
        ] {
            assert_eq!(
                classify_product_mainline_overlay_ingress_failure_v1(&local_error),
                ProductMainlineOverlayIngressFailureClassV1::LocalFault
            );
        }
    }

    #[test]
    fn config_relative_paths_are_based_on_the_config_directory() {
        let now = now_ms_v1();
        let root = std::env::temp_dir().join(format!("novovm-overlay-relative-config-{now}"));
        let config_dir = root.join("config");
        fs::create_dir_all(&config_dir).unwrap();
        let config_path = config_dir.join("node.json");
        fs::write(
            &config_path,
            serde_json::to_vec_pretty(&serde_json::json!({
                "chain_id": 7,
                "role": "duplex",
                "identity_key_path": "secrets/node.hex",
                "target_peer_id": "peer-b",
                "overlay": {
                    "cache_path": "runtime/bootstrap-cache.json",
                    "trusted_signer_public_keys": [],
                    "embedded_sources": []
                },
                "tls_trust": {
                    "explicit_ca": {
                        "certificate_path": "tls/ca.pem"
                    }
                }
            }))
            .unwrap(),
        )
        .unwrap();
        let loaded = load_product_mainline_overlay_config_v1(&config_path).unwrap();
        let absolute_config_dir = fs::canonicalize(&config_dir).unwrap();
        assert_eq!(
            loaded.identity_key_path,
            absolute_config_dir.join("secrets/node.hex")
        );
        assert_eq!(
            loaded.overlay.cache_path,
            absolute_config_dir.join("runtime/bootstrap-cache.json")
        );
        assert!(matches!(
            loaded.tls_trust,
            ProductRelayTlsTrustV1::ExplicitCa { certificate_path }
                if certificate_path == absolute_config_dir.join("tls/ca.pem")
        ));
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn inbound_boundary_rejects_non_data_cross_chain_and_empty_frames() {
        let control = NovoRudpTransportFrameV0::new(
            NovoRudpTransportFrameKindV0::Ack,
            PRODUCT_MAINLINE_OVERLAY_SESSION_ID_V1,
            7,
            1,
            1,
            0,
            vec![1],
        );
        assert!(validate_inbound_frame_v1(&control, 7).is_err());

        let cross_chain = NovoRudpTransportFrameV0::new(
            NovoRudpTransportFrameKindV0::Data,
            PRODUCT_MAINLINE_OVERLAY_SESSION_ID_V1,
            8,
            1,
            1,
            0,
            vec![1],
        );
        assert!(validate_inbound_frame_v1(&cross_chain, 7).is_err());

        let empty = NovoRudpTransportFrameV0::new(
            NovoRudpTransportFrameKindV0::Data,
            PRODUCT_MAINLINE_OVERLAY_SESSION_ID_V1,
            7,
            1,
            1,
            0,
            Vec::new(),
        );
        assert!(validate_inbound_frame_v1(&empty, 7).is_err());
        assert!(ingest_product_mainline_overlay_payload_v1(7, &[1, 2, 3]).is_err());
    }

    #[test]
    fn classified_payload_limit_accepts_boundary_and_rejects_oversize() {
        let max = PRODUCT_MAINLINE_OVERLAY_MAX_CLASSIFIED_LOGICAL_PAYLOAD_BYTES_V1;
        assert_eq!(max, 192 * 1024);
        assert!(validate_classified_logical_payload_len_v1(max).is_ok());
        assert!(validate_classified_logical_payload_len_v1(max + 1).is_err());

        let object_hash = [0x5a; 32];
        let boundary_payload = vec![0xa5; max];
        let boundary = outbound_item_v1(
            ProductMainlineOverlayPayloadClassV1::NativeSeal,
            object_hash,
            boundary_payload.clone(),
        );
        let encoded = encode_classified_payload_v1(&boundary).unwrap();
        assert_eq!(
            encoded.len(),
            PRODUCT_MAINLINE_OVERLAY_PAYLOAD_HEADER_LEN_V1 + max
        );
        let (payload_class, decoded_hash, decoded_payload) =
            decode_classified_payload_v1(&encoded).unwrap();
        assert_eq!(
            payload_class,
            ProductMainlineOverlayPayloadClassV1::NativeSeal
        );
        assert_eq!(decoded_hash, object_hash);
        assert_eq!(decoded_payload, boundary_payload);

        let oversized = outbound_item_v1(
            ProductMainlineOverlayPayloadClassV1::NativeSeal,
            object_hash,
            vec![0xa5; max + 1],
        );
        assert!(encode_classified_payload_v1(&oversized)
            .unwrap_err()
            .to_string()
            .contains("classified logical payload exceeds"));

        let mut oversized_wire = encoded;
        oversized_wire[44..48].copy_from_slice(&u32::try_from(max + 1).unwrap().to_le_bytes());
        oversized_wire.push(0xa5);
        assert!(decode_classified_payload_v1(&oversized_wire)
            .unwrap_err()
            .to_string()
            .contains("classified logical payload exceeds"));
    }

    #[test]
    fn maximum_classified_payload_fits_existing_websocket_carrier_limit() {
        const EXISTING_WEBSOCKET_FRAME_LIMIT_BYTES_V1: usize = 1_048_576;

        let initiator_identity = SigningKey::from_bytes(&[0x71; 32]);
        let responder_identity = SigningKey::from_bytes(&[0x72; 32]);
        let responder_peer_id =
            peer_id_from_ed25519_public_key_v1(&responder_identity.verifying_key().to_bytes());
        let initiator =
            NodeHandshakeInitiatorV1::start(&initiator_identity, responder_peer_id, 1_000, 5_000)
                .unwrap();
        let mut responder_replay = HandshakeReplayCacheV1::default();
        let responder = NodeHandshakeResponderV1::respond(
            initiator.offer(),
            &responder_identity,
            1_100,
            5_000,
            &mut responder_replay,
        )
        .unwrap();
        let response = responder.response().clone();
        let mut initiator_replay = HandshakeReplayCacheV1::default();
        let mut channel = initiator
            .complete(&response, 1_200, &mut initiator_replay)
            .unwrap();

        let object_hash = [0x73; 32];
        let outbound = outbound_item_v1(
            ProductMainlineOverlayPayloadClassV1::NativeSeal,
            object_hash,
            vec![0xff; PRODUCT_MAINLINE_OVERLAY_MAX_CLASSIFIED_LOGICAL_PAYLOAD_BYTES_V1],
        );
        let classified_payload = encode_classified_payload_v1(&outbound).unwrap();
        let frame = NovoRudpTransportFrameV0::new(
            NovoRudpTransportFrameKindV0::Data,
            PRODUCT_MAINLINE_OVERLAY_SESSION_ID_V1,
            7,
            u64::from_le_bytes(object_hash[..8].try_into().unwrap()),
            0,
            0,
            classified_payload,
        );
        let envelope = channel.seal_novorudp_frame(&frame).unwrap();
        let carrier_json = serde_json::to_vec(&ProductRelayWireMessageV1::Data(envelope)).unwrap();
        assert!(
            carrier_json.len() < EXISTING_WEBSOCKET_FRAME_LIMIT_BYTES_V1,
            "maximum legal classified payload produced an oversized carrier JSON: len={} limit={EXISTING_WEBSOCKET_FRAME_LIMIT_BYTES_V1}",
            carrier_json.len()
        );
    }

    #[test]
    fn duplex_mainline_lifecycle_owns_authenticated_bidirectional_classified_delivery() {
        let now = now_ms_v1();
        let root = std::env::temp_dir().join(format!("novovm-product-mainline-overlay-{now}"));
        fs::create_dir_all(&root).unwrap();
        let certificate = rcgen::generate_simple_self_signed(vec!["localhost".into()]).unwrap();
        let certificate_path = root.join("relay-cert.pem");
        let tls_key_path = root.join("relay-key.pem");
        let relay_identity_path = root.join("relay-identity.hex");
        let node_a_identity_path = root.join("node-a.hex");
        let node_b_identity_path = root.join("node-b.hex");
        fs::write(&certificate_path, certificate.serialize_pem().unwrap()).unwrap();
        fs::write(&tls_key_path, certificate.serialize_private_key_pem()).unwrap();
        write_identity_v1(&relay_identity_path, [31; 32]);
        write_identity_v1(&node_a_identity_path, [32; 32]);
        write_identity_v1(&node_b_identity_path, [33; 32]);

        let port = TcpListener::bind("127.0.0.1:0")
            .unwrap()
            .local_addr()
            .unwrap()
            .port();
        let relay_report_path = root.join("relay-report.json");
        let daemon_config = ProductRelayDaemonConfigV1 {
            bind_addr: format!("127.0.0.1:{port}"),
            tls_cert_path: certificate_path,
            tls_key_path,
            relay_identity_key_path: relay_identity_path,
            report_path: relay_report_path.clone(),
            report_interval_ms: 20,
            // Keep the bounded test relay alive for the complete duplex transaction and seal
            // exchange, including every assertion timeout on a loaded CI runner.
            run_for_ms: Some(20_000),
            max_connections: None,
            handshake_timeout_ms: None,
            max_sessions: None,
            max_tracked_sources: None,
            session_queue_capacity: Some(16),
            session_queue_bytes: None,
            active_queue_total: None,
            active_queue_bytes_total: None,
            offline_queue_per_peer: Some(16),
            offline_queue_bytes_per_peer: None,
            offline_queue_per_source: Some(32),
            offline_queue_bytes_per_source: None,
            offline_queue_total: Some(32),
            offline_queue_bytes_total: None,
            offline_queue_ttl_ms: None,
            session_ttl_ms: Some(5_000),
            rate_limit_frames: Some(1_000),
            max_frames_per_window: Some(10_000),
            rate_limit_window_ms: Some(1_000),
            source_bytes_per_minute: None,
            max_bytes_per_minute: None,
        };
        let daemon = thread::spawn(move || run_product_relay_daemon_v1(daemon_config));
        wait_until_v1(Duration::from_secs(3), || relay_report_path.exists());

        let relay_identity = SigningKey::from_bytes(&[31; 32]);
        let relay_peer_id =
            peer_id_from_ed25519_public_key_v1(&relay_identity.verifying_key().to_bytes());
        let bootstrap_signer = SigningKey::from_bytes(&[34; 32]);
        let source = signed_bootstrap_source_v1(
            &bootstrap_signer,
            &relay_identity,
            now.saturating_sub(1_000),
            now.saturating_add(30_000),
        );
        let node_a = SigningKey::from_bytes(&[32; 32]);
        let node_b = SigningKey::from_bytes(&[33; 32]);
        let node_a_peer_id = peer_id_from_ed25519_public_key_v1(&node_a.verifying_key().to_bytes());
        let node_b_peer_id = peer_id_from_ed25519_public_key_v1(&node_b.verifying_key().to_bytes());
        let chain_id = 8_000_000 + now % 100_000;
        let node_b_config = mainline_config_v1(
            chain_id,
            ProductMainlineOverlayRoleV1::Duplex,
            node_b_identity_path,
            root.join("node-b-cache.json"),
            bootstrap_signer.verifying_key().to_bytes(),
            source.clone(),
            Some(node_a_peer_id),
            None,
        );
        let node_a_config = mainline_config_v1(
            chain_id,
            ProductMainlineOverlayRoleV1::Duplex,
            node_a_identity_path,
            root.join("node-a-cache.json"),
            bootstrap_signer.verifying_key().to_bytes(),
            source,
            Some(node_b_peer_id),
            None,
        );

        let relay_override = ProductRelayClientConfigV1 {
            endpoint: format!("wss://127.0.0.1:{port}/novovm"),
            expected_relay_peer_id: relay_peer_id.clone(),
            connect_timeout_ms: 1_000,
            read_timeout_ms: 50,
            tls_trust: ProductRelayTlsTrustV1::NodeKeyBoundEncrypted,
        };
        let mut node_b_runtime = ProductMainlineOverlayRuntimeV1::start_with_relay_override_v1(
            node_b_config,
            now_ms_v1(),
            relay_override.clone(),
        )
        .unwrap();
        let mut node_a_runtime = ProductMainlineOverlayRuntimeV1::start_with_relay_override_v1(
            node_a_config,
            now_ms_v1(),
            relay_override,
        )
        .unwrap();
        assert_eq!(
            node_a_runtime
                .startup()
                .route_plan
                .selected_relay
                .as_ref()
                .unwrap()
                .relay_peer_id,
            relay_peer_id
        );
        wait_for_event_v1(&node_a_runtime, Duration::from_secs(2), |event| {
            matches!(
                event,
                ProductMainlineOverlayEventV1::E2eSessionEstablished { .. }
            )
        });
        wait_for_event_v1(&node_b_runtime, Duration::from_secs(2), |event| {
            matches!(
                event,
                ProductMainlineOverlayEventV1::E2eSessionEstablished { .. }
            )
        });
        assert!(node_a_runtime
            .try_submit_native_seal(
                [0x44; 32],
                vec![0; PRODUCT_MAINLINE_OVERLAY_MAX_CLASSIFIED_LOGICAL_PAYLOAD_BYTES_V1 + 1],
            )
            .unwrap_err()
            .to_string()
            .contains("classified logical payload exceeds"));
        let node_a_tx_hash = [0x45; 32];
        let node_b_tx_hash = [0x46; 32];
        let node_a_raw_tx = signed_native_tx_v1(chain_id, &format!("overlay-a-{now}"));
        let node_b_raw_tx = signed_native_tx_v1(chain_id, &format!("overlay-b-{now}"));
        assert!(node_a_runtime
            .try_submit(node_a_tx_hash, node_a_raw_tx.clone())
            .unwrap());
        wait_for_event_v1(&node_a_runtime, Duration::from_secs(1), |event| {
            matches!(
                event,
                ProductMainlineOverlayEventV1::Delivery(
                    ProductMainlineOverlayDeliveryV1 {
                        payload_class: ProductMainlineOverlayPayloadClassV1::NativeTransaction,
                        object_hash: delivered_hash,
                        delivered: true,
                        ..
                    }
                ) if *delivered_hash == node_a_tx_hash
            )
        });
        let inbound_at_b = wait_for_inbound_v1(&node_b_runtime, Duration::from_secs(2));
        assert!(node_b_runtime
            .try_submit(node_b_tx_hash, node_b_raw_tx.clone())
            .unwrap());
        wait_for_event_v1(&node_b_runtime, Duration::from_secs(1), |event| {
            matches!(
                event,
                ProductMainlineOverlayEventV1::Delivery(
                    ProductMainlineOverlayDeliveryV1 {
                        payload_class: ProductMainlineOverlayPayloadClassV1::NativeTransaction,
                        object_hash: delivered_hash,
                        delivered: true,
                        ..
                    }
                ) if *delivered_hash == node_b_tx_hash
            )
        });
        let inbound_at_a = wait_for_inbound_v1(&node_a_runtime, Duration::from_secs(2));
        assert_eq!(
            inbound_at_b.source_peer_id,
            node_a_runtime.startup().local_peer_id
        );
        assert_eq!(
            inbound_at_a.source_peer_id,
            node_b_runtime.startup().local_peer_id
        );
        assert_eq!(inbound_at_b.frame.stream_id, chain_id);
        assert_eq!(inbound_at_a.frame.stream_id, chain_id);
        assert_eq!(
            inbound_at_b.payload_class,
            ProductMainlineOverlayPayloadClassV1::NativeTransaction
        );
        assert_eq!(inbound_at_b.object_hash, node_a_tx_hash);
        assert_eq!(
            inbound_at_a.payload_class,
            ProductMainlineOverlayPayloadClassV1::NativeTransaction
        );
        assert_eq!(inbound_at_a.object_hash, node_b_tx_hash);
        assert_eq!(inbound_at_b.frame.payload, node_a_raw_tx);
        assert_eq!(inbound_at_a.frame.payload, node_b_raw_tx);
        for inbound in [&inbound_at_a, &inbound_at_b] {
            let ingress =
                ingest_product_mainline_overlay_payload_v1(chain_id, &inbound.frame.payload)
                    .unwrap();
            assert_eq!(ingress.chain_id, chain_id);
            assert!(ingress.pending_only);
            assert_eq!(ingress.execution_owner, "aoem_runtime");
        }

        let seal_object_hash = [0x47; 32];
        let seal_payload = b"opaque-native-seal-payload-v1".to_vec();
        assert!(node_a_runtime
            .try_submit_native_seal(seal_object_hash, seal_payload.clone())
            .unwrap());
        wait_for_event_v1(&node_a_runtime, Duration::from_secs(1), |event| {
            matches!(
                event,
                ProductMainlineOverlayEventV1::Delivery(
                    ProductMainlineOverlayDeliveryV1 {
                        payload_class: ProductMainlineOverlayPayloadClassV1::NativeSeal,
                        object_hash,
                        delivered: true,
                        ..
                    }
                ) if *object_hash == seal_object_hash
            )
        });
        let seal_inbound_at_b = wait_for_inbound_v1(&node_b_runtime, Duration::from_secs(2));
        assert_eq!(
            seal_inbound_at_b.payload_class,
            ProductMainlineOverlayPayloadClassV1::NativeSeal
        );
        assert_eq!(seal_inbound_at_b.object_hash, seal_object_hash);
        assert_eq!(seal_inbound_at_b.frame.payload, seal_payload);
        node_a_runtime.shutdown();
        node_b_runtime.shutdown();
        daemon.join().unwrap().unwrap();
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn one_relay_session_multiplexes_three_node_duplex_mesh() {
        let now = now_ms_v1();
        let root = std::env::temp_dir().join(format!("novovm-product-overlay-mesh-{now}"));
        fs::create_dir_all(&root).unwrap();
        let certificate = rcgen::generate_simple_self_signed(vec!["localhost".into()]).unwrap();
        let certificate_path = root.join("relay-cert.pem");
        let tls_key_path = root.join("relay-key.pem");
        let relay_identity_path = root.join("relay-identity.hex");
        fs::write(&certificate_path, certificate.serialize_pem().unwrap()).unwrap();
        fs::write(&tls_key_path, certificate.serialize_private_key_pem()).unwrap();
        write_identity_v1(&relay_identity_path, [59; 32]);

        let node_seeds = [[60; 32], [61; 32], [62; 32]];
        let node_identity_paths = [
            root.join("node-a.hex"),
            root.join("node-b.hex"),
            root.join("node-c.hex"),
        ];
        for (path, seed) in node_identity_paths.iter().zip(node_seeds) {
            write_identity_v1(path, seed);
        }
        let node_peer_ids = node_seeds
            .iter()
            .map(|seed| {
                let identity = SigningKey::from_bytes(seed);
                peer_id_from_ed25519_public_key_v1(&identity.verifying_key().to_bytes())
            })
            .collect::<Vec<_>>();

        let port = TcpListener::bind("127.0.0.1:0")
            .unwrap()
            .local_addr()
            .unwrap()
            .port();
        let relay_report_path = root.join("relay-report.json");
        let daemon_config = ProductRelayDaemonConfigV1 {
            bind_addr: format!("127.0.0.1:{port}"),
            tls_cert_path: certificate_path,
            tls_key_path,
            relay_identity_key_path: relay_identity_path,
            report_path: relay_report_path.clone(),
            report_interval_ms: 20,
            run_for_ms: Some(8_000),
            max_connections: None,
            handshake_timeout_ms: None,
            max_sessions: None,
            max_tracked_sources: None,
            session_queue_capacity: Some(32),
            session_queue_bytes: None,
            active_queue_total: None,
            active_queue_bytes_total: None,
            offline_queue_per_peer: Some(32),
            offline_queue_bytes_per_peer: None,
            offline_queue_per_source: Some(128),
            offline_queue_bytes_per_source: None,
            offline_queue_total: Some(128),
            offline_queue_bytes_total: None,
            offline_queue_ttl_ms: None,
            session_ttl_ms: Some(10_000),
            rate_limit_frames: Some(2_000),
            max_frames_per_window: Some(10_000),
            rate_limit_window_ms: Some(1_000),
            source_bytes_per_minute: None,
            max_bytes_per_minute: None,
        };
        let daemon = thread::spawn(move || run_product_relay_daemon_v1(daemon_config));
        wait_until_v1(Duration::from_secs(3), || relay_report_path.exists());

        let relay_identity = SigningKey::from_bytes(&[59; 32]);
        let relay_peer_id =
            peer_id_from_ed25519_public_key_v1(&relay_identity.verifying_key().to_bytes());
        let bootstrap_signer = SigningKey::from_bytes(&[63; 32]);
        let source = signed_bootstrap_source_v1(
            &bootstrap_signer,
            &relay_identity,
            now.saturating_sub(1_000),
            now.saturating_add(30_000),
        );
        let chain_id = 8_200_000 + now % 100_000;
        let relay_override = ProductRelayClientConfigV1 {
            endpoint: format!("wss://127.0.0.1:{port}/novovm"),
            expected_relay_peer_id: relay_peer_id,
            connect_timeout_ms: 1_000,
            read_timeout_ms: 50,
            tls_trust: ProductRelayTlsTrustV1::NodeKeyBoundEncrypted,
        };

        let mut configs = Vec::new();
        for (index, identity_path) in node_identity_paths.iter().enumerate() {
            let mut config = mainline_config_v1(
                chain_id,
                ProductMainlineOverlayRoleV1::Duplex,
                identity_path.clone(),
                root.join(format!("node-{index}-cache.json")),
                bootstrap_signer.verifying_key().to_bytes(),
                source.clone(),
                None,
                None,
            );
            config.peers = node_peer_ids
                .iter()
                .enumerate()
                .filter(|(peer_index, _)| *peer_index != index)
                .map(|(peer_index, peer_id)| ProductMainlineOverlayPeerConfigV1 {
                    peer_id: peer_id.clone(),
                    metric_peer_id: 92_000 + peer_index as u64,
                })
                .collect();
            configs.push(config);
        }

        let node_c_restart_config = configs[2].clone();
        let mut node_c = ProductMainlineOverlayRuntimeV1::start_with_relay_override_v1(
            configs.remove(2),
            now_ms_v1(),
            relay_override.clone(),
        )
        .unwrap();
        let mut node_b = ProductMainlineOverlayRuntimeV1::start_with_relay_override_v1(
            configs.remove(1),
            now_ms_v1(),
            relay_override.clone(),
        )
        .unwrap();
        let mut node_a = ProductMainlineOverlayRuntimeV1::start_with_relay_override_v1(
            configs.remove(0),
            now_ms_v1(),
            relay_override.clone(),
        )
        .unwrap();

        wait_for_e2e_peers_v1(
            &node_a,
            &[node_peer_ids[1].clone(), node_peer_ids[2].clone()],
            Duration::from_secs(3),
        );
        wait_for_e2e_peers_v1(
            &node_b,
            &[node_peer_ids[0].clone(), node_peer_ids[2].clone()],
            Duration::from_secs(3),
        );
        wait_for_e2e_peers_v1(
            &node_c,
            &[node_peer_ids[0].clone(), node_peer_ids[1].clone()],
            Duration::from_secs(3),
        );

        let tx_hash_a = [0x51; 32];
        let raw_tx_a = signed_native_tx_v1(chain_id, &format!("overlay-mesh-a-{now}"));
        assert!(node_a.try_submit(tx_hash_a, raw_tx_a.clone()).unwrap());
        let inbound_at_b = wait_for_inbound_v1(&node_b, Duration::from_secs(2));
        let inbound_at_c = wait_for_inbound_v1(&node_c, Duration::from_secs(2));
        assert_eq!(inbound_at_b.frame.payload, raw_tx_a);
        assert_eq!(inbound_at_c.frame.payload, raw_tx_a);
        wait_for_delivery_peers_v1(
            &node_a,
            tx_hash_a,
            &[node_peer_ids[1].clone(), node_peer_ids[2].clone()],
            Duration::from_secs(2),
        );

        let tx_hash_b = [0x52; 32];
        let raw_tx_b = signed_native_tx_v1(chain_id, &format!("overlay-mesh-b-{now}"));
        assert!(node_b.try_submit(tx_hash_b, raw_tx_b.clone()).unwrap());
        let inbound_at_a = wait_for_inbound_v1(&node_a, Duration::from_secs(2));
        let second_inbound_at_c = wait_for_inbound_v1(&node_c, Duration::from_secs(2));
        assert_eq!(inbound_at_a.frame.payload, raw_tx_b);
        assert_eq!(second_inbound_at_c.frame.payload, raw_tx_b);
        wait_for_delivery_peers_v1(
            &node_b,
            tx_hash_b,
            &[node_peer_ids[0].clone(), node_peer_ids[2].clone()],
            Duration::from_secs(2),
        );

        for inbound in [
            inbound_at_a,
            inbound_at_b,
            inbound_at_c,
            second_inbound_at_c,
        ] {
            let receipt =
                ingest_product_mainline_overlay_payload_v1(chain_id, &inbound.frame.payload)
                    .unwrap();
            assert_eq!(receipt.execution_owner, "aoem_runtime");
        }

        node_c.shutdown();
        let mut node_c_restarted = ProductMainlineOverlayRuntimeV1::start_with_relay_override_v1(
            node_c_restart_config,
            now_ms_v1(),
            relay_override,
        )
        .unwrap();
        wait_for_e2e_peers_v1(
            &node_c_restarted,
            &[node_peer_ids[0].clone(), node_peer_ids[1].clone()],
            Duration::from_secs(2),
        );
        wait_for_e2e_peers_v1(&node_a, &[node_peer_ids[2].clone()], Duration::from_secs(2));
        wait_for_e2e_peers_v1(&node_b, &[node_peer_ids[2].clone()], Duration::from_secs(2));

        let tx_hash_c = [0x53; 32];
        let raw_tx_c = signed_native_tx_v1(chain_id, &format!("overlay-mesh-c-restart-{now}"));
        assert!(node_c_restarted
            .try_submit(tx_hash_c, raw_tx_c.clone())
            .unwrap());
        let post_restart_inbound_at_a = wait_for_inbound_v1(&node_a, Duration::from_secs(2));
        let post_restart_inbound_at_b = wait_for_inbound_v1(&node_b, Duration::from_secs(2));
        assert_eq!(post_restart_inbound_at_a.frame.payload, raw_tx_c);
        assert_eq!(post_restart_inbound_at_b.frame.payload, raw_tx_c);
        wait_for_delivery_peers_v1(
            &node_c_restarted,
            tx_hash_c,
            &[node_peer_ids[0].clone(), node_peer_ids[1].clone()],
            Duration::from_secs(2),
        );

        node_a.shutdown();
        node_b.shutdown();
        node_c_restarted.shutdown();
        daemon.join().unwrap().unwrap();
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn product_mainline_mesh_bad_mac_isolates_only_attributable_peer() {
        let now = now_ms_v1();
        let root =
            std::env::temp_dir().join(format!("novovm-product-overlay-peer-error-domain-{now}"));
        fs::create_dir_all(&root).unwrap();
        let certificate = rcgen::generate_simple_self_signed(vec!["localhost".into()]).unwrap();
        let certificate_path = root.join("relay-cert.pem");
        let tls_key_path = root.join("relay-key.pem");
        let relay_identity_path = root.join("relay-identity.hex");
        let node_a_identity_path = root.join("node-a.hex");
        let node_b_identity_path = root.join("node-b.hex");
        fs::write(&certificate_path, certificate.serialize_pem().unwrap()).unwrap();
        fs::write(&tls_key_path, certificate.serialize_private_key_pem()).unwrap();
        write_identity_v1(&relay_identity_path, [74; 32]);
        write_identity_v1(&node_a_identity_path, [75; 32]);
        write_identity_v1(&node_b_identity_path, [76; 32]);

        let port = TcpListener::bind("127.0.0.1:0")
            .unwrap()
            .local_addr()
            .unwrap()
            .port();
        let relay_report_path = root.join("relay-report.json");
        let daemon_config = ProductRelayDaemonConfigV1 {
            bind_addr: format!("127.0.0.1:{port}"),
            tls_cert_path: certificate_path,
            tls_key_path,
            relay_identity_key_path: relay_identity_path,
            report_path: relay_report_path.clone(),
            report_interval_ms: 20,
            run_for_ms: Some(20_000),
            max_connections: None,
            handshake_timeout_ms: None,
            max_sessions: None,
            max_tracked_sources: None,
            session_queue_capacity: Some(32),
            session_queue_bytes: None,
            active_queue_total: None,
            active_queue_bytes_total: None,
            offline_queue_per_peer: Some(32),
            offline_queue_bytes_per_peer: None,
            offline_queue_per_source: Some(128),
            offline_queue_bytes_per_source: None,
            offline_queue_total: Some(128),
            offline_queue_bytes_total: None,
            offline_queue_ttl_ms: None,
            session_ttl_ms: Some(15_000),
            rate_limit_frames: Some(2_000),
            max_frames_per_window: Some(10_000),
            rate_limit_window_ms: Some(1_000),
            source_bytes_per_minute: None,
            max_bytes_per_minute: None,
        };
        let daemon = thread::spawn(move || run_product_relay_daemon_v1(daemon_config));
        wait_until_v1(Duration::from_secs(3), || relay_report_path.exists());

        let relay_identity = SigningKey::from_bytes(&[74; 32]);
        let relay_peer_id =
            peer_id_from_ed25519_public_key_v1(&relay_identity.verifying_key().to_bytes());
        let node_a_identity = SigningKey::from_bytes(&[75; 32]);
        let node_b_identity = SigningKey::from_bytes(&[76; 32]);
        let malicious_identity = SigningKey::from_bytes(&[77; 32]);
        let node_a_peer_id =
            peer_id_from_ed25519_public_key_v1(&node_a_identity.verifying_key().to_bytes());
        let node_b_peer_id =
            peer_id_from_ed25519_public_key_v1(&node_b_identity.verifying_key().to_bytes());
        let malicious_peer_id =
            peer_id_from_ed25519_public_key_v1(&malicious_identity.verifying_key().to_bytes());
        let bootstrap_signer = SigningKey::from_bytes(&[78; 32]);
        let source = signed_bootstrap_source_v1(
            &bootstrap_signer,
            &relay_identity,
            now.saturating_sub(1_000),
            now.saturating_add(30_000),
        );
        let chain_id = 8_300_000 + now % 100_000;
        let mut node_a_config = mainline_config_v1(
            chain_id,
            ProductMainlineOverlayRoleV1::Duplex,
            node_a_identity_path,
            root.join("node-a-cache.json"),
            bootstrap_signer.verifying_key().to_bytes(),
            source.clone(),
            None,
            None,
        );
        node_a_config.peers = vec![
            ProductMainlineOverlayPeerConfigV1 {
                peer_id: node_b_peer_id.clone(),
                metric_peer_id: 93_001,
            },
            ProductMainlineOverlayPeerConfigV1 {
                peer_id: malicious_peer_id.clone(),
                metric_peer_id: 93_002,
            },
        ];
        let node_b_config = mainline_config_v1(
            chain_id,
            ProductMainlineOverlayRoleV1::Duplex,
            node_b_identity_path,
            root.join("node-b-cache.json"),
            bootstrap_signer.verifying_key().to_bytes(),
            source,
            Some(node_a_peer_id.clone()),
            None,
        );
        let relay_override = ProductRelayClientConfigV1 {
            endpoint: format!("wss://127.0.0.1:{port}/novovm"),
            expected_relay_peer_id: relay_peer_id,
            connect_timeout_ms: 1_000,
            read_timeout_ms: 50,
            tls_trust: ProductRelayTlsTrustV1::NodeKeyBoundEncrypted,
        };
        let mut node_b = ProductMainlineOverlayRuntimeV1::start_with_relay_override_v1(
            node_b_config,
            now_ms_v1(),
            relay_override.clone(),
        )
        .unwrap();
        let mut node_a = ProductMainlineOverlayRuntimeV1::start_with_relay_override_v1(
            node_a_config,
            now_ms_v1(),
            relay_override.clone(),
        )
        .unwrap();
        wait_for_event_without_relay_failure_v1(&node_a, Duration::from_secs(3), None, |event| {
            matches!(
                event,
                ProductMainlineOverlayEventV1::E2eSessionEstablished { remote_peer_id }
                    if remote_peer_id == &node_b_peer_id
            )
        });
        wait_for_event_v1(&node_b, Duration::from_secs(3), |event| {
            matches!(
                event,
                ProductMainlineOverlayEventV1::E2eSessionEstablished { remote_peer_id }
                    if remote_peer_id == &node_a_peer_id
            )
        });

        let malicious_relay_config = ProductRelayClientConfigV1 {
            read_timeout_ms: 100,
            ..relay_override
        };
        let mut malicious_relay =
            ProductRelayClientV1::connect(&malicious_identity, &malicious_relay_config).unwrap();
        let initial_offer =
            match wait_for_relay_event_v1(&mut malicious_relay, Duration::from_secs(2), |event| {
                matches!(
                    event,
                    ProductRelayClientEventV1::PeerHandshake(delivery)
                        if delivery.source_peer_id == node_a_peer_id
                            && matches!(delivery.handshake, RelayPeerHandshakeV1::Offer(_))
                )
            }) {
                ProductRelayClientEventV1::PeerHandshake(delivery) => match delivery.handshake {
                    RelayPeerHandshakeV1::Offer(offer) => offer,
                    RelayPeerHandshakeV1::Response(_) => unreachable!(),
                },
                _ => unreachable!(),
            };
        let mut malicious_replay = HandshakeReplayCacheV1::default();
        let responder = NodeHandshakeResponderV1::respond(
            &initial_offer,
            &malicious_identity,
            now_ms_v1(),
            PRODUCT_MAINLINE_OVERLAY_PEER_HANDSHAKE_TTL_MS_V1,
            &mut malicious_replay,
        )
        .unwrap();
        let response = responder.response().clone();
        let mut malicious_channel = responder.into_channel();
        malicious_relay
            .send_peer_handshake(
                node_a_peer_id.clone(),
                RelayPeerHandshakeV1::Response(response),
            )
            .unwrap();
        wait_for_event_without_relay_failure_v1(&node_a, Duration::from_secs(2), None, |event| {
            matches!(
                event,
                ProductMainlineOverlayEventV1::E2eSessionEstablished { remote_peer_id }
                    if remote_peer_id == &malicious_peer_id
            )
        });

        let malicious_object_hash = [0x81; 32];
        let malicious_payload = outbound_item_v1(
            ProductMainlineOverlayPayloadClassV1::NativeTransaction,
            malicious_object_hash,
            signed_native_tx_v1(chain_id, &format!("malicious-peer-{now}")),
        );
        let malicious_frame = NovoRudpTransportFrameV0::new(
            NovoRudpTransportFrameKindV0::Data,
            PRODUCT_MAINLINE_OVERLAY_SESSION_ID_V1,
            chain_id,
            u64::from_le_bytes(malicious_object_hash[..8].try_into().unwrap()),
            0,
            0,
            encode_classified_payload_v1(&malicious_payload).unwrap(),
        );
        let mut bad_envelope = malicious_channel
            .seal_novorudp_frame(&malicious_frame)
            .unwrap();
        let last_byte = bad_envelope.ciphertext.last_mut().unwrap();
        *last_byte ^= 0x80;
        malicious_relay.send_envelope(bad_envelope).unwrap();
        let isolated = wait_for_event_without_relay_failure_v1(
            &node_a,
            Duration::from_secs(2),
            Some(&node_b_peer_id),
            |event| {
                matches!(
                    event,
                    ProductMainlineOverlayEventV1::PeerIsolated { remote_peer_id, .. }
                        if remote_peer_id == &malicious_peer_id
                )
            },
        );
        assert!(matches!(
            isolated,
            ProductMainlineOverlayEventV1::PeerIsolated {
                session_failure_count: 1,
                retry_in_ms: 10,
                ..
            }
        ));

        let healthy_tx_hash = [0x82; 32];
        let healthy_raw_tx = signed_native_tx_v1(chain_id, &format!("healthy-after-attack-{now}"));
        assert!(node_a
            .try_submit(healthy_tx_hash, healthy_raw_tx.clone())
            .unwrap());
        wait_for_event_without_relay_failure_v1(
            &node_a,
            Duration::from_secs(2),
            Some(&node_b_peer_id),
            |event| {
                matches!(
                    event,
                    ProductMainlineOverlayEventV1::Delivery(delivery)
                        if delivery.remote_peer_id == node_b_peer_id
                            && delivery.object_hash == healthy_tx_hash
                            && delivery.delivered
                )
            },
        );
        let inbound_at_b = wait_for_inbound_v1(&node_b, Duration::from_secs(2));
        assert_eq!(inbound_at_b.object_hash, healthy_tx_hash);
        assert_eq!(inbound_at_b.frame.payload, healthy_raw_tx);

        let retry_offer =
            match wait_for_relay_event_v1(&mut malicious_relay, Duration::from_secs(2), |event| {
                matches!(
                    event,
                    ProductRelayClientEventV1::PeerHandshake(delivery)
                        if delivery.source_peer_id == node_a_peer_id
                            && matches!(delivery.handshake, RelayPeerHandshakeV1::Offer(_))
                )
            }) {
                ProductRelayClientEventV1::PeerHandshake(delivery) => match delivery.handshake {
                    RelayPeerHandshakeV1::Offer(offer) => offer,
                    RelayPeerHandshakeV1::Response(_) => unreachable!(),
                },
                _ => unreachable!(),
            };
        let responder = NodeHandshakeResponderV1::respond(
            &retry_offer,
            &malicious_identity,
            now_ms_v1(),
            PRODUCT_MAINLINE_OVERLAY_PEER_HANDSHAKE_TTL_MS_V1,
            &mut malicious_replay,
        )
        .unwrap();
        let response = responder.response().clone();
        malicious_channel = responder.into_channel();
        malicious_relay
            .send_peer_handshake(
                node_a_peer_id.clone(),
                RelayPeerHandshakeV1::Response(response),
            )
            .unwrap();
        wait_for_event_without_relay_failure_v1(
            &node_a,
            Duration::from_secs(2),
            Some(&node_b_peer_id),
            |event| {
                matches!(
                    event,
                    ProductMainlineOverlayEventV1::E2eSessionEstablished { remote_peer_id }
                        if remote_peer_id == &malicious_peer_id
                )
            },
        );
        let delivery_after_retry =
            match wait_for_relay_event_v1(&mut malicious_relay, Duration::from_secs(2), |event| {
                matches!(event, ProductRelayClientEventV1::Delivery(_))
            }) {
                ProductRelayClientEventV1::Delivery(delivery) => delivery,
                _ => unreachable!(),
            };
        let recovered_frame = malicious_channel
            .open_novorudp_frame(&delivery_after_retry.envelope)
            .unwrap();
        let (_, recovered_hash, recovered_frame) =
            open_classified_inbound_frame_v1(recovered_frame, chain_id).unwrap();
        assert_eq!(recovered_hash, healthy_tx_hash);
        assert_eq!(recovered_frame.payload, healthy_raw_tx);

        let reverse_tx_hash = [0x83; 32];
        let reverse_raw_tx = signed_native_tx_v1(chain_id, &format!("healthy-reverse-{now}"));
        assert!(node_b
            .try_submit(reverse_tx_hash, reverse_raw_tx.clone())
            .unwrap());
        let reverse_inbound = wait_for_event_without_relay_failure_v1(
            &node_a,
            Duration::from_secs(2),
            Some(&node_b_peer_id),
            |event| {
                matches!(
                    event,
                    ProductMainlineOverlayEventV1::Inbound(inbound)
                        if inbound.source_peer_id == node_b_peer_id
                            && inbound.object_hash == reverse_tx_hash
                )
            },
        );
        match reverse_inbound {
            ProductMainlineOverlayEventV1::Inbound(inbound) => {
                assert_eq!(inbound.frame.payload, reverse_raw_tx)
            }
            _ => unreachable!(),
        }

        let _ = malicious_relay.close();
        node_a.shutdown();
        node_b.shutdown();
        daemon.join().unwrap().unwrap();
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn mainline_lifecycle_rotates_from_failed_signed_relay_candidate() {
        let now = now_ms_v1();
        let root = std::env::temp_dir().join(format!("novovm-product-overlay-rotation-{now}"));
        fs::create_dir_all(&root).unwrap();
        let certificate = rcgen::generate_simple_self_signed(vec!["localhost".into()]).unwrap();
        let certificate_path = root.join("relay-cert.pem");
        let tls_key_path = root.join("relay-key.pem");
        let live_relay_identity_path = root.join("live-relay-identity.hex");
        let node_a_identity_path = root.join("node-a.hex");
        let node_b_identity_path = root.join("node-b.hex");
        fs::write(&certificate_path, certificate.serialize_pem().unwrap()).unwrap();
        fs::write(&tls_key_path, certificate.serialize_private_key_pem()).unwrap();
        write_identity_v1(&node_a_identity_path, [52; 32]);
        write_identity_v1(&node_b_identity_path, [53; 32]);

        let relay_a = SigningKey::from_bytes(&[40; 32]);
        let relay_b = SigningKey::from_bytes(&[41; 32]);
        let relay_a_peer_id =
            peer_id_from_ed25519_public_key_v1(&relay_a.verifying_key().to_bytes());
        let relay_b_peer_id =
            peer_id_from_ed25519_public_key_v1(&relay_b.verifying_key().to_bytes());
        let (dead_relay, live_relay, live_relay_seed) = if relay_a_peer_id < relay_b_peer_id {
            (relay_a, relay_b, [41; 32])
        } else {
            (relay_b, relay_a, [40; 32])
        };
        let dead_relay_peer_id =
            peer_id_from_ed25519_public_key_v1(&dead_relay.verifying_key().to_bytes());
        let live_relay_peer_id =
            peer_id_from_ed25519_public_key_v1(&live_relay.verifying_key().to_bytes());
        write_identity_v1(&live_relay_identity_path, live_relay_seed);

        let live_port = TcpListener::bind("127.0.0.1:0")
            .unwrap()
            .local_addr()
            .unwrap()
            .port();
        let dead_port = TcpListener::bind("127.0.0.1:0")
            .unwrap()
            .local_addr()
            .unwrap()
            .port();
        let relay_report_path = root.join("relay-report.json");
        let daemon_config = ProductRelayDaemonConfigV1 {
            bind_addr: format!("127.0.0.1:{live_port}"),
            tls_cert_path: certificate_path,
            tls_key_path,
            relay_identity_key_path: live_relay_identity_path,
            report_path: relay_report_path.clone(),
            report_interval_ms: 20,
            run_for_ms: Some(5_000),
            max_connections: None,
            handshake_timeout_ms: None,
            max_sessions: None,
            max_tracked_sources: None,
            session_queue_capacity: Some(16),
            session_queue_bytes: None,
            active_queue_total: None,
            active_queue_bytes_total: None,
            offline_queue_per_peer: Some(16),
            offline_queue_bytes_per_peer: None,
            offline_queue_per_source: Some(32),
            offline_queue_bytes_per_source: None,
            offline_queue_total: Some(32),
            offline_queue_bytes_total: None,
            offline_queue_ttl_ms: None,
            session_ttl_ms: Some(5_000),
            rate_limit_frames: Some(1_000),
            max_frames_per_window: Some(10_000),
            rate_limit_window_ms: Some(1_000),
            source_bytes_per_minute: None,
            max_bytes_per_minute: None,
        };
        let daemon = thread::spawn(move || run_product_relay_daemon_v1(daemon_config));
        wait_until_v1(Duration::from_secs(3), || relay_report_path.exists());

        let bootstrap_signer = SigningKey::from_bytes(&[54; 32]);
        let source = signed_bootstrap_source_with_relays_v1(
            &bootstrap_signer,
            &[&dead_relay, &live_relay],
            now.saturating_sub(1_000),
            now.saturating_add(30_000),
        );
        let node_a = SigningKey::from_bytes(&[52; 32]);
        let node_b = SigningKey::from_bytes(&[53; 32]);
        let node_a_peer_id = peer_id_from_ed25519_public_key_v1(&node_a.verifying_key().to_bytes());
        let node_b_peer_id = peer_id_from_ed25519_public_key_v1(&node_b.verifying_key().to_bytes());
        let chain_id = 8_100_000 + now % 100_000;
        let node_a_config = mainline_config_v1(
            chain_id,
            ProductMainlineOverlayRoleV1::Duplex,
            node_a_identity_path,
            root.join("node-a-cache.json"),
            bootstrap_signer.verifying_key().to_bytes(),
            source.clone(),
            Some(node_b_peer_id),
            None,
        );
        let node_b_config = mainline_config_v1(
            chain_id,
            ProductMainlineOverlayRoleV1::Duplex,
            node_b_identity_path,
            root.join("node-b-cache.json"),
            bootstrap_signer.verifying_key().to_bytes(),
            source,
            Some(node_a_peer_id),
            None,
        );
        let dead_override = ProductRelayClientConfigV1 {
            endpoint: format!("wss://127.0.0.1:{dead_port}/novovm"),
            expected_relay_peer_id: dead_relay_peer_id.clone(),
            connect_timeout_ms: 100,
            read_timeout_ms: 50,
            tls_trust: ProductRelayTlsTrustV1::NodeKeyBoundEncrypted,
        };
        let live_override = ProductRelayClientConfigV1 {
            endpoint: format!("wss://127.0.0.1:{live_port}/novovm"),
            expected_relay_peer_id: live_relay_peer_id.clone(),
            connect_timeout_ms: 500,
            read_timeout_ms: 50,
            tls_trust: ProductRelayTlsTrustV1::NodeKeyBoundEncrypted,
        };
        let mut node_a_runtime = ProductMainlineOverlayRuntimeV1::start_with_relay_overrides_v1(
            node_a_config,
            now_ms_v1(),
            vec![dead_override.clone(), live_override.clone()],
        )
        .unwrap();
        let mut node_b_runtime = ProductMainlineOverlayRuntimeV1::start_with_relay_overrides_v1(
            node_b_config,
            now_ms_v1(),
            vec![dead_override, live_override],
        )
        .unwrap();
        assert_eq!(
            node_a_runtime
                .startup()
                .route_plan
                .selected_relay
                .as_ref()
                .unwrap()
                .relay_peer_id,
            dead_relay_peer_id
        );
        let tx_hash = [0x47; 32];
        let raw_tx = signed_native_tx_v1(chain_id, &format!("overlay-reconnect-{now}"));
        assert!(node_a_runtime.try_submit(tx_hash, raw_tx.clone()).unwrap());

        let started = Instant::now();
        let mut node_a_rotated = false;
        let mut node_b_rotated = false;
        let mut node_a_connected_to_live = false;
        let mut node_b_connected_to_live = false;
        let mut delivered_after_reconnect = false;
        let mut inbound_after_reconnect = None;
        let mut observed = Vec::new();
        while started.elapsed() < Duration::from_secs(4)
            && !(node_a_rotated
                && node_b_rotated
                && node_a_connected_to_live
                && node_b_connected_to_live
                && delivered_after_reconnect
                && inbound_after_reconnect.is_some())
        {
            for event in node_a_runtime.drain_events(32) {
                observed.push(format!("node_a:{event:?}"));
                match event {
                    ProductMainlineOverlayEventV1::RelayRotated {
                        previous_relay_peer_id,
                        next_relay_peer_id,
                    } => {
                        if previous_relay_peer_id == dead_relay_peer_id
                            && next_relay_peer_id == live_relay_peer_id
                        {
                            node_a_rotated = true;
                        }
                    }
                    ProductMainlineOverlayEventV1::RelayConnected { relay_peer_id } => {
                        if relay_peer_id == live_relay_peer_id {
                            node_a_connected_to_live = true;
                        }
                    }
                    ProductMainlineOverlayEventV1::Delivery(delivery)
                        if delivery.payload_class
                            == ProductMainlineOverlayPayloadClassV1::NativeTransaction
                            && delivery.object_hash == tx_hash
                            && delivery.delivered =>
                    {
                        delivered_after_reconnect = true;
                    }
                    ProductMainlineOverlayEventV1::WorkerFailed(error) => {
                        panic!("product overlay worker failed: {error}")
                    }
                    _ => {}
                }
            }
            for event in node_b_runtime.drain_events(32) {
                observed.push(format!("node_b:{event:?}"));
                match event {
                    ProductMainlineOverlayEventV1::RelayRotated {
                        previous_relay_peer_id,
                        next_relay_peer_id,
                    } => {
                        if previous_relay_peer_id == dead_relay_peer_id
                            && next_relay_peer_id == live_relay_peer_id
                        {
                            node_b_rotated = true;
                        }
                    }
                    ProductMainlineOverlayEventV1::RelayConnected { relay_peer_id } => {
                        if relay_peer_id == live_relay_peer_id {
                            node_b_connected_to_live = true;
                        }
                    }
                    ProductMainlineOverlayEventV1::Inbound(inbound) => {
                        inbound_after_reconnect = Some(inbound);
                    }
                    ProductMainlineOverlayEventV1::WorkerFailed(error) => {
                        panic!("product overlay worker failed: {error}")
                    }
                    _ => {}
                }
            }
            thread::sleep(Duration::from_millis(10));
        }
        assert!(
            node_a_rotated,
            "node A did not rotate the failed relay: {observed:?}"
        );
        assert!(
            node_b_rotated,
            "node B did not rotate the failed relay: {observed:?}"
        );
        assert!(
            node_a_connected_to_live && node_b_connected_to_live,
            "replacement signed relay was not authenticated by both nodes: {observed:?}"
        );
        assert!(
            delivered_after_reconnect,
            "queued transaction was not delivered after reconnect: {observed:?}"
        );
        let inbound = inbound_after_reconnect.unwrap_or_else(|| {
            panic!("queued transaction did not reach remote node: {observed:?}")
        });
        assert_eq!(inbound.frame.payload, raw_tx);
        let ingress =
            ingest_product_mainline_overlay_payload_v1(chain_id, &inbound.frame.payload).unwrap();
        assert_eq!(ingress.execution_owner, "aoem_runtime");

        node_a_runtime.shutdown();
        node_b_runtime.shutdown();
        daemon.join().unwrap().unwrap();
        let _ = fs::remove_dir_all(root);
    }

    #[allow(clippy::too_many_arguments)]
    fn mainline_config_v1(
        chain_id: u64,
        role: ProductMainlineOverlayRoleV1,
        identity_key_path: PathBuf,
        cache_path: PathBuf,
        trusted_signer: [u8; 32],
        source: ProductBootstrapSourceV1,
        target_peer_id: Option<String>,
        expected_source_peer_id: Option<String>,
    ) -> ProductMainlineOverlayConfigV1 {
        ProductMainlineOverlayConfigV1 {
            chain_id,
            role,
            identity_key_path,
            overlay: ProductNodeOverlayConfigV1 {
                cache_path,
                trusted_signer_public_keys: vec![trusted_signer],
                minimum_bootstrap_signatures: 1,
                embedded_sources: vec![source],
                cooldown_base_ms: Some(10),
                cooldown_max_ms: Some(100),
            },
            target_peer_id,
            expected_source_peer_id,
            peers: Vec::new(),
            connect_timeout_ms: 1_000,
            read_timeout_ms: 50,
            tls_trust: ProductRelayTlsTrustV1::NodeKeyBoundEncrypted,
            channel_capacity: 16,
            resource_limits: ProductMainlineOverlayResourceLimitsV1::default(),
            metric_peer_id: 91,
            reconnect_base_delay_ms: 10,
            reconnect_max_delay_ms: 100,
        }
    }

    fn signed_bootstrap_source_v1(
        signer: &SigningKey,
        relay: &SigningKey,
        issued_at_ms: u64,
        expires_at_ms: u64,
    ) -> ProductBootstrapSourceV1 {
        signed_bootstrap_source_with_relays_v1(signer, &[relay], issued_at_ms, expires_at_ms)
    }

    fn signed_bootstrap_source_with_relays_v1(
        signer: &SigningKey,
        relays: &[&SigningKey],
        issued_at_ms: u64,
        expires_at_ms: u64,
    ) -> ProductBootstrapSourceV1 {
        let records = relays
            .iter()
            .enumerate()
            .map(|(index, relay)| {
                sign_relay_record_v1(
                    relay,
                    format!("mainline-relay-{index}"),
                    vec![RelayEndpointV1 {
                        transport: RelayTransportV1::Wss443,
                        uri: "wss://localhost:443/novovm".into(),
                        priority: 1,
                        max_sessions: 16,
                        max_bytes_per_minute: 1_000_000,
                    }],
                    issued_at_ms,
                    expires_at_ms,
                    1,
                )
                .unwrap()
            })
            .collect::<Vec<PeerSignedRelayRecordV1>>();
        let mut manifest = SignedBootstrapManifestV1 {
            version: 1,
            manifest_id: "mainline-overlay-manifest".into(),
            issued_at_ms,
            expires_at_ms,
            candidate_limit: records.len().max(1) as u16,
            full_raw_ip_directory_embedded: false,
            requires_single_official_relay: false,
            requires_single_official_domain: false,
            relay_records: records,
            signatures: Vec::new(),
        };
        sign_bootstrap_manifest_v1(&mut manifest, signer).unwrap();
        ProductBootstrapSourceV1 {
            source_kind: BootstrapSourceKindV1::EmbeddedInstall,
            priority: 1,
            manifest,
        }
    }

    fn write_identity_v1(path: &Path, key: [u8; 32]) {
        let hex = key
            .iter()
            .map(|byte| format!("{byte:02x}"))
            .collect::<String>();
        fs::write(path, hex).unwrap();
    }

    fn signed_native_tx_v1(chain_id: u64, account: &str) -> Vec<u8> {
        let mut tx = NovNativeTxWireV1 {
            chain_id,
            kind: NovTxKindV1::Execute(NovExecuteTxV1 {
                caller: Vec::new(),
                account_id: Some(account.into()),
                fee_owner_account_id: Some(account.into()),
                nonce_owner_account_id: Some(account.into()),
                target: NovExecutionTargetV1::NativeModule("treasury".into()),
                method: "deposit_reserve".into(),
                args: serde_json::to_vec(&serde_json::json!({
                    "asset": "USDT",
                    "amount": 7
                }))
                .unwrap(),
                execution_mode: NovExecutionModeV1::Standard,
                execution_policy: NovExecutionPolicyV1::Standard,
                privacy_mode: NovPrivacyModeV1::Public,
                verification_mode: NovVerificationModeV1::Standard,
                fee_policy: NovFeePolicyV1 {
                    pay_asset: "USDT".into(),
                    max_pay_amount: 50,
                    slippage_bps: 100,
                },
                gas_like_limit: Some(90_000),
                nonce: 0,
            }),
            signature: Vec::new(),
        };
        sign_nov_native_tx_with_seed_v1(&mut tx, [0x44; 32]).unwrap();
        encode_nov_native_tx_wire_v1(&tx).unwrap()
    }

    fn wait_until_v1(timeout: Duration, mut predicate: impl FnMut() -> bool) {
        let started = Instant::now();
        while started.elapsed() < timeout {
            if predicate() {
                return;
            }
            thread::sleep(Duration::from_millis(10));
        }
        panic!("timed out waiting for product overlay condition");
    }

    fn wait_for_event_v1(
        runtime: &ProductMainlineOverlayRuntimeV1,
        timeout: Duration,
        predicate: impl Fn(&ProductMainlineOverlayEventV1) -> bool,
    ) {
        let started = Instant::now();
        while started.elapsed() < timeout {
            for event in runtime.drain_events(32) {
                if let ProductMainlineOverlayEventV1::WorkerFailed(error) = &event {
                    panic!("product overlay worker failed: {error}");
                }
                if predicate(&event) {
                    return;
                }
            }
            thread::sleep(Duration::from_millis(10));
        }
        panic!("timed out waiting for product overlay lifecycle event");
    }

    fn wait_for_event_without_relay_failure_v1(
        runtime: &ProductMainlineOverlayRuntimeV1,
        timeout: Duration,
        forbidden_healthy_rehandshake_peer_id: Option<&str>,
        predicate: impl Fn(&ProductMainlineOverlayEventV1) -> bool,
    ) -> ProductMainlineOverlayEventV1 {
        let started = Instant::now();
        while started.elapsed() < timeout {
            let mut matched = None;
            for event in runtime.drain_events(64) {
                match &event {
                    ProductMainlineOverlayEventV1::RelayDisconnected { .. }
                    | ProductMainlineOverlayEventV1::RelayRotated { .. }
                    | ProductMainlineOverlayEventV1::WorkerStopped
                    | ProductMainlineOverlayEventV1::WorkerFailed(_) => {
                        panic!("unexpected relay-global failure while waiting for peer event: {event:?}")
                    }
                    ProductMainlineOverlayEventV1::E2eSessionEstablished { remote_peer_id }
                        if forbidden_healthy_rehandshake_peer_id
                            .is_some_and(|peer_id| peer_id == remote_peer_id) =>
                    {
                        panic!(
                            "healthy peer unexpectedly re-established after another peer fault: {event:?}"
                        )
                    }
                    _ => {}
                }
                if matched.is_none() && predicate(&event) {
                    matched = Some(event);
                }
            }
            if let Some(event) = matched {
                return event;
            }
            thread::sleep(Duration::from_millis(10));
        }
        panic!("timed out waiting for peer-local product overlay lifecycle event");
    }

    fn wait_for_relay_event_v1(
        relay: &mut ProductRelayClientV1,
        timeout: Duration,
        predicate: impl Fn(&ProductRelayClientEventV1) -> bool,
    ) -> ProductRelayClientEventV1 {
        let started = Instant::now();
        while started.elapsed() < timeout {
            match relay.recv_event() {
                Ok(event) if predicate(&event) => return event,
                Ok(_) => {}
                Err(error) if relay_read_timed_out_v1(&error) => {}
                Err(error) => panic!("failed while waiting for direct relay event: {error:#}"),
            }
        }
        panic!("timed out waiting for direct relay event");
    }

    fn wait_for_inbound_v1(
        runtime: &ProductMainlineOverlayRuntimeV1,
        timeout: Duration,
    ) -> ProductMainlineOverlayInboundV1 {
        let started = Instant::now();
        while started.elapsed() < timeout {
            for event in runtime.drain_events(32) {
                match event {
                    ProductMainlineOverlayEventV1::Inbound(inbound) => return inbound,
                    ProductMainlineOverlayEventV1::WorkerFailed(error) => {
                        panic!("product overlay worker failed: {error}")
                    }
                    _ => {}
                }
            }
            thread::sleep(Duration::from_millis(10));
        }
        panic!("timed out waiting for product overlay inbound payload");
    }

    fn wait_for_e2e_peers_v1(
        runtime: &ProductMainlineOverlayRuntimeV1,
        expected: &[String],
        timeout: Duration,
    ) {
        let expected = expected.iter().cloned().collect::<BTreeSet<_>>();
        let mut observed = BTreeSet::new();
        let started = Instant::now();
        while started.elapsed() < timeout {
            for event in runtime.drain_events(64) {
                match event {
                    ProductMainlineOverlayEventV1::E2eSessionEstablished { remote_peer_id } => {
                        observed.insert(remote_peer_id);
                    }
                    ProductMainlineOverlayEventV1::WorkerFailed(error) => {
                        panic!("product overlay worker failed: {error}")
                    }
                    _ => {}
                }
            }
            if observed == expected {
                return;
            }
            thread::sleep(Duration::from_millis(10));
        }
        panic!("timed out waiting for E2E peers: expected={expected:?} observed={observed:?}");
    }

    fn wait_for_delivery_peers_v1(
        runtime: &ProductMainlineOverlayRuntimeV1,
        tx_hash: [u8; 32],
        expected: &[String],
        timeout: Duration,
    ) {
        let expected = expected.iter().cloned().collect::<BTreeSet<_>>();
        let mut observed = BTreeSet::new();
        let started = Instant::now();
        while started.elapsed() < timeout {
            for event in runtime.drain_events(64) {
                match event {
                    ProductMainlineOverlayEventV1::Delivery(delivery)
                        if delivery.payload_class
                            == ProductMainlineOverlayPayloadClassV1::NativeTransaction
                            && delivery.object_hash == tx_hash
                            && delivery.delivered =>
                    {
                        observed.insert(delivery.remote_peer_id);
                    }
                    ProductMainlineOverlayEventV1::WorkerFailed(error) => {
                        panic!("product overlay worker failed: {error}")
                    }
                    _ => {}
                }
            }
            if observed == expected {
                return;
            }
            thread::sleep(Duration::from_millis(10));
        }
        panic!("timed out waiting for delivery peers: expected={expected:?} observed={observed:?}");
    }
}
