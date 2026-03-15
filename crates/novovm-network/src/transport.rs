#![forbid(unsafe_code)]

use crate::{
    get_network_runtime_sync_status, observe_eth_native_bodies_pull,
    observe_eth_native_bodies_response, observe_eth_native_discovery,
    observe_eth_native_headers_pull, observe_eth_native_headers_response, observe_eth_native_hello,
    observe_eth_native_rlpx_auth, observe_eth_native_rlpx_auth_ack, observe_eth_native_snap_pull,
    observe_eth_native_snap_response, observe_eth_native_status,
    observe_network_runtime_eth_peer_head, observe_network_runtime_local_head_max,
    observe_network_runtime_peer_head, observe_network_runtime_peer_head_with_local_head_max,
    plan_network_runtime_sync_pull_window, register_network_runtime_peer,
    unregister_network_runtime_peer, upsert_network_runtime_eth_peer_session,
    NetworkRuntimeNativeSyncPhaseV1,
};
use dashmap::DashMap;
use novovm_protocol::{
    decode as protocol_decode, decode_block_header_wire_v1, encode as protocol_encode,
    encode_block_header_wire_v1,
    protocol_catalog::distributed_occc::gossip::MessageType as DistributedOcccMessageType,
    BlockHeaderWireV1, ConsensusPluginBindingV1, EvmNativeMessage, FinalityMessage,
    GossipMessage as ProtocolGossipMessage, NodeId, PacemakerMessage, ProtocolMessage,
    TwoPcMessage, CONSENSUS_PLUGIN_CLASS_CODE,
};
use std::collections::VecDeque;
use std::io::{Read, Write};
use std::net::{IpAddr, SocketAddr, TcpListener, TcpStream, UdpSocket};
use std::sync::{Arc, Mutex, OnceLock};
use std::time::Duration;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum NetworkError {
    #[error("peer not found: {0:?}")]
    PeerNotFound(NodeId),
    #[error("queue full")]
    QueueFull,
    #[error("local node mismatch: expected {expected:?}, got {got:?}")]
    LocalNodeMismatch { expected: NodeId, got: NodeId },
    #[error("address parse failed: {0}")]
    AddressParse(String),
    #[error("io error: {0}")]
    Io(String),
    #[error("encode failed: {0}")]
    Encode(String),
    #[error("decode failed: {0}")]
    Decode(String),
}

/// Minimal transport interface.
///
/// V3 intent: keep protocol concerns in novovm-protocol, keep transport concerns here.
/// Higher-level routing/consensus lives elsewhere.
pub trait Transport: Send + Sync {
    fn send(&self, to: NodeId, msg: ProtocolMessage) -> Result<(), NetworkError>;
    fn try_recv(&self, me: NodeId) -> Result<Option<ProtocolMessage>, NetworkError>;
}

const RUNTIME_SYNC_PULL_REQUEST_MAGIC: [u8; 4] = *b"NSP1";
const RUNTIME_SYNC_PULL_REQUEST_LEN: usize = 4 + 1 + 8 + 8 + 8;
const RUNTIME_SYNC_PULL_RESPONSE_BATCH_MAX: u64 = 128;
const DEFAULT_TCP_CONNECT_RETRY_ATTEMPTS: usize = 2;
const DEFAULT_TCP_CONNECT_RETRY_BACKOFF_MS: u64 = 0;
const PEER_IP_HINT_AMBIGUOUS: u64 = u64::MAX;
const RUNTIME_SYNC_PULL_PREFETCH_MARGIN_HEADERS: u64 = 8;
const RUNTIME_SYNC_PULL_PREFETCH_MARGIN_BODIES: u64 = 4;
const RUNTIME_SYNC_PULL_PREFETCH_MARGIN_STATE: u64 = 2;
const RUNTIME_SYNC_PULL_PREFETCH_MARGIN_FINALIZE: u64 = 1;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct RuntimeSyncPullRequest {
    phase: NetworkRuntimeNativeSyncPhaseV1,
    chain_id: u64,
    from_block: u64,
    to_block: u64,
}

#[derive(Debug, Clone, Copy, Default)]
struct RuntimeSyncPullMessageContext {
    is_sync_pull: bool,
    request: Option<RuntimeSyncPullRequest>,
    header_height: Option<u64>,
}

#[derive(Debug, Clone)]
struct RuntimeSyncPullResponsePlan {
    to: NodeId,
    to_wire: u32,
    msg_type: DistributedOcccMessageType,
    response_from: u64,
    response_to: u64,
    timestamp: u64,
}

#[derive(Debug, Clone, Copy)]
struct RuntimeSyncPullTargetState {
    to_block: u64,
    followup_trigger_block: u64,
}

type RuntimeSyncPullTargetMap = DashMap<(u64, u64, u64), RuntimeSyncPullTargetState>;
static RUNTIME_SYNC_PULL_TARGETS: OnceLock<RuntimeSyncPullTargetMap> = OnceLock::new();
static NETWORK_GOSSIP_SYNC_COMPAT_ENABLED: OnceLock<bool> = OnceLock::new();

fn runtime_sync_pull_target_map() -> &'static RuntimeSyncPullTargetMap {
    RUNTIME_SYNC_PULL_TARGETS.get_or_init(DashMap::new)
}

fn parse_env_bool(name: &str, fallback: bool) -> bool {
    match std::env::var(name) {
        Ok(raw) => {
            let normalized = raw.trim().to_ascii_lowercase();
            if normalized.is_empty() {
                fallback
            } else if matches!(normalized.as_str(), "0" | "false" | "off" | "no") {
                false
            } else if matches!(normalized.as_str(), "1" | "true" | "on" | "yes") {
                true
            } else {
                fallback
            }
        }
        Err(_) => fallback,
    }
}

fn network_gossip_sync_compat_enabled() -> bool {
    *NETWORK_GOSSIP_SYNC_COMPAT_ENABLED
        .get_or_init(|| parse_env_bool("NOVOVM_NETWORK_ENABLE_GOSSIP_SYNC_COMPAT", true))
}

#[cfg(test)]
fn set_runtime_sync_pull_target(
    chain_id: u64,
    local_node: NodeId,
    remote_peer: NodeId,
    to_block: u64,
) {
    set_runtime_sync_pull_target_with_trigger(
        chain_id,
        local_node,
        remote_peer,
        to_block,
        to_block,
    );
}

fn set_runtime_sync_pull_target_with_trigger(
    chain_id: u64,
    local_node: NodeId,
    remote_peer: NodeId,
    to_block: u64,
    followup_trigger_block: u64,
) {
    runtime_sync_pull_target_map().insert(
        (chain_id, local_node.0, remote_peer.0),
        RuntimeSyncPullTargetState {
            to_block,
            followup_trigger_block: followup_trigger_block.min(to_block),
        },
    );
}

#[cfg(test)]
fn get_runtime_sync_pull_target(
    chain_id: u64,
    local_node: NodeId,
    remote_peer: NodeId,
) -> Option<u64> {
    runtime_sync_pull_target_map()
        .get(&(chain_id, local_node.0, remote_peer.0))
        .map(|target| target.to_block)
}

fn clear_runtime_sync_pull_target(chain_id: u64, local_node: NodeId, remote_peer: NodeId) {
    runtime_sync_pull_target_map().remove(&(chain_id, local_node.0, remote_peer.0));
}

fn should_wait_runtime_sync_pull_target_window(
    chain_id: u64,
    local_node: NodeId,
    remote_peer: NodeId,
    observed_height: u64,
) -> bool {
    let key = (chain_id, local_node.0, remote_peer.0);
    let target_map = runtime_sync_pull_target_map();
    if let Some(target) = target_map.get(&key) {
        let target_to = target.to_block;
        let trigger = target.followup_trigger_block;
        drop(target);
        if observed_height < trigger {
            return true;
        }
        if observed_height >= target_to {
            target_map.remove(&key);
            return false;
        }
        // Prefetch trigger: near the tail of current window, start requesting
        // next window to hide pull RTT while preserving deterministic ordering.
        target_map.remove(&key);
    }
    false
}

fn parse_env_usize(name: &str, fallback: usize) -> usize {
    std::env::var(name)
        .ok()
        .and_then(|raw| raw.trim().parse::<usize>().ok())
        .unwrap_or(fallback)
}

fn parse_env_u64(name: &str, fallback: u64) -> u64 {
    std::env::var(name)
        .ok()
        .and_then(|raw| raw.trim().parse::<u64>().ok())
        .unwrap_or(fallback)
}

/// Simple in-memory transport for tests/bench harnesses.
///
/// This intentionally avoids async to keep the skeleton lightweight and portable.
#[derive(Debug, Clone)]
pub struct InMemoryTransport {
    inner: Arc<DashMap<NodeId, VecDeque<ProtocolMessage>>>,
    max_queue_len: usize,
}

impl InMemoryTransport {
    #[must_use]
    pub fn new(max_queue_len: usize) -> Self {
        Self {
            inner: Arc::new(DashMap::new()),
            max_queue_len,
        }
    }

    pub fn register(&self, node: NodeId) {
        self.inner
            .entry(node)
            .or_insert_with(|| VecDeque::with_capacity(self.max_queue_len.min(1024)));
    }
}

impl Transport for InMemoryTransport {
    fn send(&self, to: NodeId, msg: ProtocolMessage) -> Result<(), NetworkError> {
        let mut q = self
            .inner
            .get_mut(&to)
            .ok_or(NetworkError::PeerNotFound(to))?;
        if q.len() >= self.max_queue_len {
            return Err(NetworkError::QueueFull);
        }
        q.push_back(msg);
        Ok(())
    }

    fn try_recv(&self, me: NodeId) -> Result<Option<ProtocolMessage>, NetworkError> {
        let mut q = self
            .inner
            .get_mut(&me)
            .ok_or(NetworkError::PeerNotFound(me))?;
        Ok(q.pop_front())
    }
}

/// UDP transport for multi-process probe and lightweight local-node networking.
#[derive(Debug, Clone)]
pub struct UdpTransport {
    node: NodeId,
    chain_id: u64,
    socket: Arc<UdpSocket>,
    peers: Arc<DashMap<NodeId, SocketAddr>>,
    peer_addr_index: Arc<DashMap<SocketAddr, NodeId>>,
    peer_ip_hint_index: Arc<DashMap<IpAddr, u64>>,
    runtime_peer_registered: Arc<DashMap<NodeId, ()>>,
    recv_buf: Arc<Mutex<Vec<u8>>>,
}

/// TCP transport for multi-process / multi-host cluster probes.
///
/// This implementation intentionally prefers simplicity over throughput:
/// each `send` opens a short-lived TCP connection and sends a single frame.
#[derive(Debug, Clone)]
pub struct TcpTransport {
    node: NodeId,
    chain_id: u64,
    listener: Arc<TcpListener>,
    peers: Arc<DashMap<NodeId, SocketAddr>>,
    peer_addr_index: Arc<DashMap<SocketAddr, NodeId>>,
    peer_ip_hint_index: Arc<DashMap<IpAddr, u64>>,
    outbound_streams: Arc<DashMap<NodeId, Arc<Mutex<TcpStream>>>>,
    max_packet_size: usize,
    recv_frame_buf: Arc<Mutex<Vec<u8>>>,
    connect_timeout_ms: u64,
    connect_retry_attempts: usize,
    connect_retry_backoff_ms: u64,
}

impl TcpTransport {
    const DEFAULT_CHAIN_ID: u64 = 1;

    pub fn bind(node: NodeId, listen_addr: &str) -> Result<Self, NetworkError> {
        Self::bind_with_packet_size_for_chain(node, listen_addr, 64 * 1024, Self::DEFAULT_CHAIN_ID)
    }

    pub fn bind_with_packet_size(
        node: NodeId,
        listen_addr: &str,
        max_packet_size: usize,
    ) -> Result<Self, NetworkError> {
        Self::bind_with_packet_size_for_chain(
            node,
            listen_addr,
            max_packet_size,
            Self::DEFAULT_CHAIN_ID,
        )
    }

    pub fn bind_for_chain(
        node: NodeId,
        listen_addr: &str,
        chain_id: u64,
    ) -> Result<Self, NetworkError> {
        Self::bind_with_packet_size_for_chain(node, listen_addr, 64 * 1024, chain_id)
    }

    pub fn bind_with_packet_size_for_chain(
        node: NodeId,
        listen_addr: &str,
        max_packet_size: usize,
        chain_id: u64,
    ) -> Result<Self, NetworkError> {
        let listener =
            TcpListener::bind(listen_addr).map_err(|e| NetworkError::Io(e.to_string()))?;
        listener
            .set_nonblocking(true)
            .map_err(|e| NetworkError::Io(e.to_string()))?;
        Ok(Self {
            node,
            chain_id,
            listener: Arc::new(listener),
            peers: Arc::new(DashMap::new()),
            peer_addr_index: Arc::new(DashMap::new()),
            peer_ip_hint_index: Arc::new(DashMap::new()),
            outbound_streams: Arc::new(DashMap::new()),
            max_packet_size: max_packet_size.max(1024),
            recv_frame_buf: Arc::new(Mutex::new(vec![0u8; max_packet_size.max(1024)])),
            connect_timeout_ms: 500,
            connect_retry_attempts: parse_env_usize(
                "NOVOVM_NETWORK_TCP_CONNECT_RETRY_ATTEMPTS",
                DEFAULT_TCP_CONNECT_RETRY_ATTEMPTS,
            )
            .max(1),
            connect_retry_backoff_ms: parse_env_u64(
                "NOVOVM_NETWORK_TCP_CONNECT_RETRY_BACKOFF_MS",
                DEFAULT_TCP_CONNECT_RETRY_BACKOFF_MS,
            ),
        })
    }

    pub fn register_peer(&self, node: NodeId, addr: &str) -> Result<(), NetworkError> {
        let parsed: SocketAddr = addr
            .parse()
            .map_err(|e: std::net::AddrParseError| NetworkError::AddressParse(e.to_string()))?;
        if let Some(old_addr) = self.peers.insert(node, parsed) {
            self.peer_addr_index.remove(&old_addr);
            if old_addr.ip() != parsed.ip() {
                refresh_peer_ip_hint_for_ip(&self.peers, &self.peer_ip_hint_index, old_addr.ip());
            }
        }
        self.peer_addr_index.insert(parsed, node);
        refresh_peer_ip_hint_for_ip(&self.peers, &self.peer_ip_hint_index, parsed.ip());
        let _ = register_network_runtime_peer(self.chain_id, node.0);
        Ok(())
    }

    pub fn unregister_peer(&self, node: NodeId) -> Result<(), NetworkError> {
        let Some((_, removed_addr)) = self.peers.remove(&node) else {
            return Err(NetworkError::PeerNotFound(node));
        };
        self.peer_addr_index.remove(&removed_addr);
        refresh_peer_ip_hint_for_ip(&self.peers, &self.peer_ip_hint_index, removed_addr.ip());
        clear_runtime_sync_pull_target(self.chain_id, self.node, node);
        self.outbound_streams.remove(&node);
        let _ = unregister_network_runtime_peer(self.chain_id, node.0);
        Ok(())
    }

    pub fn local_addr(&self) -> Result<SocketAddr, NetworkError> {
        self.listener
            .local_addr()
            .map_err(|e| NetworkError::Io(e.to_string()))
    }

    pub fn set_connect_timeout_ms(&mut self, timeout_ms: u64) {
        self.connect_timeout_ms = timeout_ms.max(1);
    }

    pub fn set_connect_retry_attempts(&mut self, attempts: usize) {
        self.connect_retry_attempts = attempts.max(1);
    }

    pub fn set_connect_retry_backoff_ms(&mut self, backoff_ms: u64) {
        self.connect_retry_backoff_ms = backoff_ms;
    }

    fn send_internal(&self, to: NodeId, msg: &ProtocolMessage) -> Result<(), NetworkError> {
        let peer = *self.peers.get(&to).ok_or(NetworkError::PeerNotFound(to))?;
        let encoded = protocol_encode(msg).map_err(|e| NetworkError::Encode(e.to_string()))?;
        if let Some(stream_arc) = self
            .outbound_streams
            .get(&to)
            .map(|entry| Arc::clone(entry.value()))
        {
            let write_result = {
                let mut guard = stream_arc
                    .lock()
                    .map_err(|_| NetworkError::Io("tcp stream lock poisoned".to_string()))?;
                write_tcp_frame(&mut guard, &encoded)
            };
            match write_result {
                Ok(()) => {
                    maybe_track_runtime_sync_pull_request_outbound_send(
                        self.chain_id,
                        self.node,
                        to,
                        msg,
                    );
                    maybe_update_runtime_sync_local_progress_from_send(
                        self.chain_id,
                        self.node,
                        msg,
                    );
                    return Ok(());
                }
                Err(e) => {
                    self.outbound_streams.remove(&to);
                    if should_mark_peer_disconnected(&e) {
                        clear_runtime_sync_pull_target(self.chain_id, self.node, to);
                        let _ = unregister_network_runtime_peer(self.chain_id, to.0);
                    }
                }
            }
        }

        let mut last_err = None;
        let mut last_connect_io_error: Option<std::io::Error> = None;
        let mut stream_opt = None;
        for attempt_idx in 0..self.connect_retry_attempts {
            match TcpStream::connect_timeout(&peer, Duration::from_millis(self.connect_timeout_ms))
            {
                Ok(s) => {
                    stream_opt = Some(s);
                    break;
                }
                Err(e) => {
                    last_err = Some(e.to_string());
                    last_connect_io_error = Some(e);
                    let should_backoff = attempt_idx + 1 < self.connect_retry_attempts
                        && self.connect_retry_backoff_ms > 0;
                    if should_backoff {
                        std::thread::sleep(Duration::from_millis(self.connect_retry_backoff_ms));
                    }
                }
            }
        }
        if stream_opt.is_none() {
            if let Some(io_err) = last_connect_io_error.as_ref() {
                if should_mark_peer_disconnected(io_err) {
                    clear_runtime_sync_pull_target(self.chain_id, self.node, to);
                    let _ = unregister_network_runtime_peer(self.chain_id, to.0);
                }
            }
        }
        let mut stream = stream_opt.ok_or_else(|| {
            NetworkError::Io(format!(
                "tcp connect failed after retries: {}",
                last_err.unwrap_or_else(|| "unknown".to_string())
            ))
        })?;
        stream
            .set_nodelay(true)
            .map_err(|e| NetworkError::Io(e.to_string()))?;
        write_tcp_frame(&mut stream, &encoded).map_err(|e| {
            if should_mark_peer_disconnected(&e) {
                clear_runtime_sync_pull_target(self.chain_id, self.node, to);
                let _ = unregister_network_runtime_peer(self.chain_id, to.0);
            }
            NetworkError::Io(e.to_string())
        })?;
        self.outbound_streams
            .insert(to, Arc::new(Mutex::new(stream)));
        let _ = register_network_runtime_peer(self.chain_id, to.0);
        maybe_track_runtime_sync_pull_request_outbound_send(self.chain_id, self.node, to, msg);
        maybe_update_runtime_sync_local_progress_from_send(self.chain_id, self.node, msg);
        Ok(())
    }
}

impl UdpTransport {
    const DEFAULT_CHAIN_ID: u64 = 1;

    pub fn bind(node: NodeId, listen_addr: &str) -> Result<Self, NetworkError> {
        Self::bind_with_packet_size_for_chain(node, listen_addr, 64 * 1024, Self::DEFAULT_CHAIN_ID)
    }

    pub fn bind_with_packet_size(
        node: NodeId,
        listen_addr: &str,
        max_packet_size: usize,
    ) -> Result<Self, NetworkError> {
        Self::bind_with_packet_size_for_chain(
            node,
            listen_addr,
            max_packet_size,
            Self::DEFAULT_CHAIN_ID,
        )
    }

    pub fn bind_for_chain(
        node: NodeId,
        listen_addr: &str,
        chain_id: u64,
    ) -> Result<Self, NetworkError> {
        Self::bind_with_packet_size_for_chain(node, listen_addr, 64 * 1024, chain_id)
    }

    pub fn bind_with_packet_size_for_chain(
        node: NodeId,
        listen_addr: &str,
        max_packet_size: usize,
        chain_id: u64,
    ) -> Result<Self, NetworkError> {
        let socket = UdpSocket::bind(listen_addr).map_err(|e| NetworkError::Io(e.to_string()))?;
        socket
            .set_nonblocking(true)
            .map_err(|e| NetworkError::Io(e.to_string()))?;
        Ok(Self {
            node,
            chain_id,
            socket: Arc::new(socket),
            peers: Arc::new(DashMap::new()),
            peer_addr_index: Arc::new(DashMap::new()),
            peer_ip_hint_index: Arc::new(DashMap::new()),
            runtime_peer_registered: Arc::new(DashMap::new()),
            recv_buf: Arc::new(Mutex::new(vec![0u8; max_packet_size.max(1024)])),
        })
    }

    pub fn register_peer(&self, node: NodeId, addr: &str) -> Result<(), NetworkError> {
        let parsed: SocketAddr = addr
            .parse()
            .map_err(|e: std::net::AddrParseError| NetworkError::AddressParse(e.to_string()))?;
        if let Some(old_addr) = self.peers.insert(node, parsed) {
            self.peer_addr_index.remove(&old_addr);
            if old_addr.ip() != parsed.ip() {
                refresh_peer_ip_hint_for_ip(&self.peers, &self.peer_ip_hint_index, old_addr.ip());
            }
        }
        self.peer_addr_index.insert(parsed, node);
        refresh_peer_ip_hint_for_ip(&self.peers, &self.peer_ip_hint_index, parsed.ip());
        if self.runtime_peer_registered.insert(node, ()).is_none() {
            let _ = register_network_runtime_peer(self.chain_id, node.0);
        }
        Ok(())
    }

    pub fn unregister_peer(&self, node: NodeId) -> Result<(), NetworkError> {
        let Some((_, removed_addr)) = self.peers.remove(&node) else {
            return Err(NetworkError::PeerNotFound(node));
        };
        self.peer_addr_index.remove(&removed_addr);
        refresh_peer_ip_hint_for_ip(&self.peers, &self.peer_ip_hint_index, removed_addr.ip());
        clear_runtime_sync_pull_target(self.chain_id, self.node, node);
        self.runtime_peer_registered.remove(&node);
        let _ = unregister_network_runtime_peer(self.chain_id, node.0);
        Ok(())
    }

    pub fn local_addr(&self) -> Result<SocketAddr, NetworkError> {
        self.socket
            .local_addr()
            .map_err(|e| NetworkError::Io(e.to_string()))
    }

    fn send_internal(&self, to: NodeId, msg: &ProtocolMessage) -> Result<(), NetworkError> {
        let peer = *self.peers.get(&to).ok_or(NetworkError::PeerNotFound(to))?;
        let encoded = protocol_encode(msg).map_err(|e| NetworkError::Encode(e.to_string()))?;
        let sent = match self.socket.send_to(&encoded, peer) {
            Ok(sent) => sent,
            Err(e) => {
                if should_mark_peer_disconnected(&e) {
                    clear_runtime_sync_pull_target(self.chain_id, self.node, to);
                    self.runtime_peer_registered.remove(&to);
                    let _ = unregister_network_runtime_peer(self.chain_id, to.0);
                }
                return Err(NetworkError::Io(e.to_string()));
            }
        };
        if sent != encoded.len() {
            return Err(NetworkError::Io(format!(
                "partial udp send: sent={sent} expected={}",
                encoded.len()
            )));
        }
        if self.runtime_peer_registered.insert(to, ()).is_none() {
            let _ = register_network_runtime_peer(self.chain_id, to.0);
        }
        maybe_track_runtime_sync_pull_request_outbound_send(self.chain_id, self.node, to, msg);
        maybe_update_runtime_sync_local_progress_from_send(self.chain_id, self.node, msg);
        Ok(())
    }
}

#[cfg(test)]
fn maybe_update_runtime_sync_from_protocol_message(
    chain_id: u64,
    msg: &ProtocolMessage,
    msg_peer_id: Option<u64>,
    source_peer_id_hint: Option<u64>,
) {
    let sync_ctx = runtime_sync_pull_message_context(msg);
    maybe_update_runtime_sync_from_protocol_message_with_context(
        chain_id,
        msg,
        msg_peer_id,
        source_peer_id_hint,
        &sync_ctx,
    );
}

fn maybe_update_runtime_sync_from_protocol_message_with_context(
    chain_id: u64,
    msg: &ProtocolMessage,
    msg_peer_id: Option<u64>,
    source_peer_id_hint: Option<u64>,
    sync_ctx: &RuntimeSyncPullMessageContext,
) {
    let fallback_peer_id = msg_peer_id.or(source_peer_id_hint);

    match msg {
        ProtocolMessage::Gossip(ProtocolGossipMessage::PeerList { from, peers }) => {
            let _ = register_network_runtime_peer(chain_id, from.0);
            for peer in peers {
                if peer.0 != from.0 {
                    let _ = register_network_runtime_peer(chain_id, peer.0);
                }
            }
        }
        ProtocolMessage::Pacemaker(PacemakerMessage::ViewSync { from, height, .. }) => {
            let _ = observe_network_runtime_peer_head_with_local_head_max(
                chain_id, from.0, *height, None,
            );
        }
        ProtocolMessage::Pacemaker(PacemakerMessage::NewView {
            from,
            height,
            high_qc_height,
            ..
        }) => {
            let _ = observe_network_runtime_peer_head_with_local_head_max(
                chain_id,
                from.0,
                (*height).max(*high_qc_height),
                None,
            );
        }
        ProtocolMessage::DistributedOcccGossip(gossip_msg) => {
            if sync_ctx.is_sync_pull {
                if let Some(height) = sync_ctx.header_height {
                    // Treat downloader state headers as local progress.
                    // This keeps runtime current_block advancing from real ingress
                    // messages instead of waiting for external snapshot injection.
                    let _ = observe_network_runtime_peer_head_with_local_head_max(
                        chain_id,
                        gossip_msg.from as u64,
                        height,
                        Some(height),
                    );
                } else {
                    let _ = register_network_runtime_peer(chain_id, gossip_msg.from as u64);
                }
            } else {
                let _ = register_network_runtime_peer(chain_id, gossip_msg.from as u64);
            }
        }
        ProtocolMessage::Finality(FinalityMessage::Vote { id, from, .. }) => {
            let _ =
                observe_network_runtime_peer_head_with_local_head_max(chain_id, from.0, id.0, None);
        }
        ProtocolMessage::Finality(FinalityMessage::CheckpointPropose { id, from, .. })
        | ProtocolMessage::Finality(FinalityMessage::Cert { id, from, .. }) => {
            let _ =
                observe_network_runtime_peer_head_with_local_head_max(chain_id, from.0, id.0, None);
        }
        ProtocolMessage::EvmNative(native_msg) => match native_msg {
            EvmNativeMessage::DiscoveryPing { from, .. }
            | EvmNativeMessage::DiscoveryPong { from, .. }
            | EvmNativeMessage::DiscoveryFindNode { from, .. }
            | EvmNativeMessage::DiscoveryNeighbors { from, .. } => {
                let _ = register_network_runtime_peer(chain_id, from.0);
                observe_eth_native_discovery(chain_id);
            }
            EvmNativeMessage::RlpxAuth { from, .. } => {
                let _ = register_network_runtime_peer(chain_id, from.0);
                observe_eth_native_rlpx_auth(chain_id);
            }
            EvmNativeMessage::RlpxAuthAck { from, .. } => {
                let _ = register_network_runtime_peer(chain_id, from.0);
                observe_eth_native_rlpx_auth_ack(chain_id);
            }
            EvmNativeMessage::Hello {
                from,
                chain_id: hello_chain_id,
                eth_versions,
                snap_versions,
                ..
            } => {
                let _ = register_network_runtime_peer(chain_id, from.0);
                observe_eth_native_hello(chain_id);
                if *hello_chain_id == chain_id {
                    let _ = upsert_network_runtime_eth_peer_session(
                        chain_id,
                        from.0,
                        eth_versions,
                        snap_versions,
                        None,
                    );
                }
            }
            EvmNativeMessage::Status {
                from,
                chain_id: status_chain_id,
                head_height,
                ..
            } => {
                let _ = register_network_runtime_peer(chain_id, from.0);
                observe_eth_native_status(chain_id);
                if *status_chain_id == chain_id {
                    observe_network_runtime_eth_peer_head(chain_id, from.0, *head_height);
                }
                let _ = observe_network_runtime_peer_head_with_local_head_max(
                    chain_id,
                    from.0,
                    *head_height,
                    None,
                );
            }
            EvmNativeMessage::NewBlockHashes { from, blocks } => {
                let _ = register_network_runtime_peer(chain_id, from.0);
                if let Some((_, height)) = blocks.iter().max_by_key(|(_, h)| *h) {
                    observe_network_runtime_eth_peer_head(chain_id, from.0, *height);
                    let _ = observe_network_runtime_peer_head_with_local_head_max(
                        chain_id, from.0, *height, None,
                    );
                }
            }
            EvmNativeMessage::BlockHeaders { from, heights } => {
                let _ = register_network_runtime_peer(chain_id, from.0);
                observe_eth_native_headers_response(chain_id);
                if let Some(height) = heights.iter().copied().max() {
                    observe_network_runtime_eth_peer_head(chain_id, from.0, height);
                    let _ = observe_network_runtime_peer_head_with_local_head_max(
                        chain_id,
                        from.0,
                        height,
                        Some(height),
                    );
                }
            }
            EvmNativeMessage::GetBlockHeaders { from, .. }
            | EvmNativeMessage::Transactions { from, .. }
            | EvmNativeMessage::GetBlockBodies { from, .. }
            | EvmNativeMessage::BlockBodies { from, .. }
            | EvmNativeMessage::SnapGetAccountRange { from, .. }
            | EvmNativeMessage::SnapAccountRange { from, .. } => {
                let _ = register_network_runtime_peer(chain_id, from.0);
                match native_msg {
                    EvmNativeMessage::BlockBodies { .. } => {
                        observe_eth_native_bodies_response(chain_id);
                    }
                    EvmNativeMessage::SnapAccountRange { .. } => {
                        observe_eth_native_snap_response(chain_id);
                    }
                    _ => {}
                }
            }
        },
        _ => {
            if let Some(peer_id) = fallback_peer_id {
                let _ = register_network_runtime_peer(chain_id, peer_id);
            }
        }
    }
}

fn maybe_update_runtime_sync_local_progress_from_send(
    chain_id: u64,
    local_node: NodeId,
    msg: &ProtocolMessage,
) {
    match msg {
        ProtocolMessage::Pacemaker(PacemakerMessage::ViewSync { from, height, .. }) => {
            if *from == local_node {
                let _ = observe_network_runtime_local_head_max(chain_id, *height);
            }
        }
        ProtocolMessage::Pacemaker(PacemakerMessage::NewView {
            from,
            height,
            high_qc_height,
            ..
        }) => {
            if *from == local_node {
                let _ = observe_network_runtime_local_head_max(
                    chain_id,
                    (*height).max(*high_qc_height),
                );
            }
        }
        ProtocolMessage::DistributedOcccGossip(gossip_msg) => {
            if gossip_msg.from == local_node.0 as u32
                && is_runtime_sync_pull_msg_type(&gossip_msg.msg_type)
            {
                if let Ok(header) = decode_block_header_wire_v1(&gossip_msg.payload) {
                    let _ = observe_network_runtime_local_head_max(chain_id, header.height);
                }
            }
        }
        ProtocolMessage::Finality(FinalityMessage::Vote { id, from, .. }) => {
            if *from == local_node {
                let _ = observe_network_runtime_local_head_max(chain_id, id.0);
            }
        }
        ProtocolMessage::Finality(FinalityMessage::CheckpointPropose { id, from, .. })
        | ProtocolMessage::Finality(FinalityMessage::Cert { id, from, .. }) => {
            if *from == local_node {
                let _ = observe_network_runtime_local_head_max(chain_id, id.0);
            }
        }
        ProtocolMessage::EvmNative(native_msg) => match native_msg {
            EvmNativeMessage::Status {
                from, head_height, ..
            } => {
                if *from == local_node {
                    let _ = observe_network_runtime_local_head_max(chain_id, *head_height);
                }
            }
            EvmNativeMessage::BlockHeaders { from, heights } => {
                if *from == local_node {
                    if let Some(height) = heights.iter().copied().max() {
                        let _ = observe_network_runtime_local_head_max(chain_id, height);
                    }
                }
            }
            EvmNativeMessage::NewBlockHashes { from, blocks } => {
                if *from == local_node {
                    if let Some((_, height)) = blocks.iter().max_by_key(|(_, h)| *h) {
                        let _ = observe_network_runtime_local_head_max(chain_id, *height);
                    }
                }
            }
            EvmNativeMessage::Transactions { .. } => {}
            _ => {}
        },
        _ => {}
    }
}

fn is_runtime_sync_pull_msg_type(msg_type: &DistributedOcccMessageType) -> bool {
    matches!(
        msg_type,
        DistributedOcccMessageType::StateSync | DistributedOcccMessageType::ShardState
    )
}

fn decode_runtime_sync_pull_request(payload: &[u8]) -> Option<RuntimeSyncPullRequest> {
    if payload.len() < RUNTIME_SYNC_PULL_REQUEST_LEN {
        return None;
    }
    if payload.get(0..4)? != RUNTIME_SYNC_PULL_REQUEST_MAGIC {
        return None;
    }
    let phase = decode_runtime_sync_phase_byte(*payload.get(4)?);
    let chain_id = u64::from_le_bytes(payload.get(5..13)?.try_into().ok()?);
    let from_block = u64::from_le_bytes(payload.get(13..21)?.try_into().ok()?);
    let to_block = u64::from_le_bytes(payload.get(21..29)?.try_into().ok()?);
    Some(RuntimeSyncPullRequest {
        phase,
        chain_id,
        from_block,
        to_block,
    })
}

fn runtime_sync_pull_message_context(msg: &ProtocolMessage) -> RuntimeSyncPullMessageContext {
    let ProtocolMessage::DistributedOcccGossip(gossip_msg) = msg else {
        return RuntimeSyncPullMessageContext::default();
    };
    if !network_gossip_sync_compat_enabled() {
        return RuntimeSyncPullMessageContext::default();
    }
    if !is_runtime_sync_pull_msg_type(&gossip_msg.msg_type) {
        return RuntimeSyncPullMessageContext::default();
    }
    let request = decode_runtime_sync_pull_request(&gossip_msg.payload);
    let header_height = if request.is_none() {
        decode_block_header_wire_v1(&gossip_msg.payload)
            .ok()
            .map(|header| header.height)
    } else {
        None
    };
    RuntimeSyncPullMessageContext {
        is_sync_pull: true,
        request,
        header_height,
    }
}

fn decode_runtime_sync_phase_byte(raw: u8) -> NetworkRuntimeNativeSyncPhaseV1 {
    match raw {
        0 => NetworkRuntimeNativeSyncPhaseV1::Idle,
        1 => NetworkRuntimeNativeSyncPhaseV1::Discovery,
        2 => NetworkRuntimeNativeSyncPhaseV1::Headers,
        3 => NetworkRuntimeNativeSyncPhaseV1::Bodies,
        4 => NetworkRuntimeNativeSyncPhaseV1::State,
        5 => NetworkRuntimeNativeSyncPhaseV1::Finalize,
        _ => NetworkRuntimeNativeSyncPhaseV1::Headers,
    }
}

fn runtime_sync_pull_msg_type_for_phase(
    phase: NetworkRuntimeNativeSyncPhaseV1,
) -> DistributedOcccMessageType {
    match phase {
        NetworkRuntimeNativeSyncPhaseV1::Headers => DistributedOcccMessageType::StateSync,
        _ => DistributedOcccMessageType::ShardState,
    }
}

fn runtime_sync_pull_response_batch_max_by_phase(phase: NetworkRuntimeNativeSyncPhaseV1) -> u64 {
    match phase {
        NetworkRuntimeNativeSyncPhaseV1::Headers => RUNTIME_SYNC_PULL_RESPONSE_BATCH_MAX,
        NetworkRuntimeNativeSyncPhaseV1::Bodies => 64,
        NetworkRuntimeNativeSyncPhaseV1::State => 32,
        NetworkRuntimeNativeSyncPhaseV1::Finalize => 16,
        NetworkRuntimeNativeSyncPhaseV1::Discovery | NetworkRuntimeNativeSyncPhaseV1::Idle => 16,
    }
}

fn encode_runtime_sync_phase_byte(phase: NetworkRuntimeNativeSyncPhaseV1) -> u8 {
    match phase {
        NetworkRuntimeNativeSyncPhaseV1::Idle => 0,
        NetworkRuntimeNativeSyncPhaseV1::Discovery => 1,
        NetworkRuntimeNativeSyncPhaseV1::Headers => 2,
        NetworkRuntimeNativeSyncPhaseV1::Bodies => 3,
        NetworkRuntimeNativeSyncPhaseV1::State => 4,
        NetworkRuntimeNativeSyncPhaseV1::Finalize => 5,
    }
}

fn encode_runtime_sync_pull_request_payload(
    chain_id: u64,
    phase: NetworkRuntimeNativeSyncPhaseV1,
    from_block: u64,
    to_block: u64,
) -> Vec<u8> {
    let mut payload = Vec::with_capacity(RUNTIME_SYNC_PULL_REQUEST_LEN);
    payload.extend_from_slice(&RUNTIME_SYNC_PULL_REQUEST_MAGIC);
    payload.push(encode_runtime_sync_phase_byte(phase));
    payload.extend_from_slice(&chain_id.to_le_bytes());
    payload.extend_from_slice(&from_block.to_le_bytes());
    payload.extend_from_slice(&to_block.to_le_bytes());
    payload
}

fn runtime_sync_pull_response_cap_to(request: &RuntimeSyncPullRequest) -> u64 {
    let phase_batch = runtime_sync_pull_response_batch_max_by_phase(request.phase).max(1);
    request.to_block.min(
        request
            .from_block
            .saturating_add(phase_batch.saturating_sub(1)),
    )
}

fn runtime_sync_pull_prefetch_margin_by_phase(phase: NetworkRuntimeNativeSyncPhaseV1) -> u64 {
    match phase {
        NetworkRuntimeNativeSyncPhaseV1::Headers => RUNTIME_SYNC_PULL_PREFETCH_MARGIN_HEADERS,
        NetworkRuntimeNativeSyncPhaseV1::Bodies => RUNTIME_SYNC_PULL_PREFETCH_MARGIN_BODIES,
        NetworkRuntimeNativeSyncPhaseV1::State => RUNTIME_SYNC_PULL_PREFETCH_MARGIN_STATE,
        NetworkRuntimeNativeSyncPhaseV1::Finalize => RUNTIME_SYNC_PULL_PREFETCH_MARGIN_FINALIZE,
        NetworkRuntimeNativeSyncPhaseV1::Discovery | NetworkRuntimeNativeSyncPhaseV1::Idle => 0,
    }
}

fn runtime_sync_pull_followup_trigger_height(
    request: &RuntimeSyncPullRequest,
    capped_target_to: u64,
) -> u64 {
    let window_span = capped_target_to.saturating_sub(request.from_block);
    let phase_margin = runtime_sync_pull_prefetch_margin_by_phase(request.phase);
    let bounded_margin = phase_margin.min(window_span / 2);
    capped_target_to.saturating_sub(bounded_margin)
}

fn decode_evm_native_sync_marker_range(marker: &[u8; 32]) -> (u64, u64, u64) {
    let chain_id = u64::from_le_bytes(marker[0..8].try_into().unwrap_or([0u8; 8]));
    let from_block = u64::from_le_bytes(marker[8..16].try_into().unwrap_or([0u8; 8]));
    let to_block = u64::from_le_bytes(marker[16..24].try_into().unwrap_or([0u8; 8]));
    (chain_id, from_block, to_block)
}

fn maybe_extract_evm_native_sync_pull_request(
    chain_id: u64,
    local_node: NodeId,
    msg: &ProtocolMessage,
) -> Option<RuntimeSyncPullRequest> {
    let ProtocolMessage::EvmNative(native_msg) = msg else {
        return None;
    };
    match native_msg {
        EvmNativeMessage::GetBlockHeaders {
            from,
            start_height,
            max,
            skip,
            reverse,
        } => {
            if *from != local_node {
                return None;
            }
            let step = skip.saturating_add(1);
            let span = max.saturating_sub(1).saturating_mul(step);
            let to_block = if *reverse {
                *start_height
            } else {
                start_height.saturating_add(span)
            };
            Some(RuntimeSyncPullRequest {
                phase: NetworkRuntimeNativeSyncPhaseV1::Headers,
                chain_id,
                from_block: *start_height,
                to_block: to_block.max(*start_height),
            })
        }
        EvmNativeMessage::GetBlockBodies { from, hashes } => {
            if *from != local_node {
                return None;
            }
            let marker = hashes.first()?;
            let (marker_chain_id, from_block, to_block) =
                decode_evm_native_sync_marker_range(marker);
            if marker_chain_id != chain_id {
                return None;
            }
            Some(RuntimeSyncPullRequest {
                phase: NetworkRuntimeNativeSyncPhaseV1::Bodies,
                chain_id,
                from_block,
                to_block: to_block.max(from_block),
            })
        }
        EvmNativeMessage::SnapGetAccountRange {
            from,
            block_hash,
            origin,
            limit,
        } => {
            if *from != local_node {
                return None;
            }
            let marker_chain_id =
                u64::from_le_bytes(block_hash[0..8].try_into().unwrap_or([0u8; 8]));
            if marker_chain_id != chain_id {
                return None;
            }
            let from_block = u64::from_le_bytes(origin[0..8].try_into().unwrap_or([0u8; 8]));
            let to_hint = u64::from_le_bytes(block_hash[8..16].try_into().unwrap_or([0u8; 8]));
            let to_block = if to_hint >= from_block {
                to_hint
            } else {
                from_block.saturating_add(limit.saturating_sub(1))
            };
            Some(RuntimeSyncPullRequest {
                phase: NetworkRuntimeNativeSyncPhaseV1::State,
                chain_id,
                from_block,
                to_block: to_block.max(from_block),
            })
        }
        _ => None,
    }
}

fn maybe_track_runtime_sync_pull_request_outbound(
    chain_id: u64,
    local_node: NodeId,
    msg: &ProtocolMessage,
) {
    if !network_gossip_sync_compat_enabled() {
        return;
    }
    let ProtocolMessage::DistributedOcccGossip(gossip_msg) = msg else {
        return;
    };
    if !is_runtime_sync_pull_msg_type(&gossip_msg.msg_type) {
        return;
    }
    if gossip_msg.from != local_node.0 as u32 {
        return;
    }
    let Some(request) = decode_runtime_sync_pull_request(&gossip_msg.payload) else {
        return;
    };
    if request.chain_id != chain_id {
        return;
    }
    let capped_target_to = runtime_sync_pull_response_cap_to(&request);
    let followup_trigger = runtime_sync_pull_followup_trigger_height(&request, capped_target_to);
    set_runtime_sync_pull_target_with_trigger(
        chain_id,
        local_node,
        NodeId(gossip_msg.to as u64),
        capped_target_to,
        followup_trigger,
    );
}

fn maybe_track_runtime_sync_pull_request_outbound_send(
    chain_id: u64,
    local_node: NodeId,
    remote_peer: NodeId,
    msg: &ProtocolMessage,
) {
    maybe_track_runtime_sync_pull_request_outbound(chain_id, local_node, msg);
    let Some(request) = maybe_extract_evm_native_sync_pull_request(chain_id, local_node, msg)
    else {
        return;
    };
    match request.phase {
        NetworkRuntimeNativeSyncPhaseV1::Headers
        | NetworkRuntimeNativeSyncPhaseV1::Finalize
        | NetworkRuntimeNativeSyncPhaseV1::Discovery
        | NetworkRuntimeNativeSyncPhaseV1::Idle => observe_eth_native_headers_pull(chain_id),
        NetworkRuntimeNativeSyncPhaseV1::Bodies => observe_eth_native_bodies_pull(chain_id),
        NetworkRuntimeNativeSyncPhaseV1::State => observe_eth_native_snap_pull(chain_id),
    }
    let capped_target_to = runtime_sync_pull_response_cap_to(&request);
    let followup_trigger = runtime_sync_pull_followup_trigger_height(&request, capped_target_to);
    set_runtime_sync_pull_target_with_trigger(
        chain_id,
        local_node,
        remote_peer,
        capped_target_to,
        followup_trigger,
    );
}

fn encode_evm_native_sync_marker(chain_id: u64, from_block: u64, to_block: u64) -> [u8; 32] {
    let mut marker = [0u8; 32];
    marker[0..8].copy_from_slice(&chain_id.to_le_bytes());
    marker[8..16].copy_from_slice(&from_block.to_le_bytes());
    marker[16..24].copy_from_slice(&to_block.to_le_bytes());
    marker
}

fn build_evm_native_sync_pull_request(
    local_node: NodeId,
    chain_id: u64,
    phase: NetworkRuntimeNativeSyncPhaseV1,
    from_block: u64,
    to_block: u64,
) -> ProtocolMessage {
    let span = to_block.saturating_sub(from_block).saturating_add(1).max(1);
    match phase {
        NetworkRuntimeNativeSyncPhaseV1::Bodies => {
            ProtocolMessage::EvmNative(EvmNativeMessage::GetBlockBodies {
                from: local_node,
                hashes: vec![encode_evm_native_sync_marker(
                    chain_id, from_block, to_block,
                )],
            })
        }
        NetworkRuntimeNativeSyncPhaseV1::State => {
            let mut block_hash = [0u8; 32];
            block_hash[0..8].copy_from_slice(&chain_id.to_le_bytes());
            block_hash[8..16].copy_from_slice(&to_block.to_le_bytes());
            let mut origin = [0u8; 32];
            origin[0..8].copy_from_slice(&from_block.to_le_bytes());
            ProtocolMessage::EvmNative(EvmNativeMessage::SnapGetAccountRange {
                from: local_node,
                block_hash,
                origin,
                limit: span,
            })
        }
        NetworkRuntimeNativeSyncPhaseV1::Headers
        | NetworkRuntimeNativeSyncPhaseV1::Finalize
        | NetworkRuntimeNativeSyncPhaseV1::Discovery
        | NetworkRuntimeNativeSyncPhaseV1::Idle => {
            ProtocolMessage::EvmNative(EvmNativeMessage::GetBlockHeaders {
                from: local_node,
                start_height: from_block,
                max: span,
                skip: 0,
                reverse: false,
            })
        }
    }
}

fn encode_runtime_sync_block_header_payload(response_height: u64) -> Vec<u8> {
    let header = BlockHeaderWireV1 {
        height: response_height,
        epoch_id: 0,
        parent_hash: [0u8; 32],
        state_root: [0u8; 32],
        governance_chain_audit_root: [0u8; 32],
        tx_count: 0,
        batch_count: 0,
        consensus_binding: ConsensusPluginBindingV1 {
            plugin_class_code: CONSENSUS_PLUGIN_CLASS_CODE,
            adapter_hash: [0u8; 32],
        },
    };
    encode_block_header_wire_v1(&header)
}

fn compute_runtime_sync_pull_response_range(
    chain_id: u64,
    phase: NetworkRuntimeNativeSyncPhaseV1,
    from_block: u64,
    to_block: u64,
) -> Option<(u64, u64)> {
    let local_head = get_network_runtime_sync_status(chain_id)
        .map(|s| s.current_block)
        .unwrap_or(0);
    if local_head < from_block {
        return None;
    }
    let response_to = local_head.min(to_block);
    let phase_batch = runtime_sync_pull_response_batch_max_by_phase(phase).max(1);
    let capped_to = response_to.min(from_block.saturating_add(phase_batch.saturating_sub(1)));
    if capped_to < from_block {
        return None;
    }
    Some((from_block, capped_to))
}

fn maybe_plan_runtime_sync_pull_responses_with_context(
    chain_id: u64,
    local_node: NodeId,
    msg: &ProtocolMessage,
    sync_ctx: &RuntimeSyncPullMessageContext,
) -> Option<RuntimeSyncPullResponsePlan> {
    if !network_gossip_sync_compat_enabled() {
        return None;
    }
    let ProtocolMessage::DistributedOcccGossip(gossip_msg) = msg else {
        return None;
    };
    if !sync_ctx.is_sync_pull {
        return None;
    }
    if gossip_msg.to != local_node.0 as u32 {
        return None;
    }
    let request = sync_ctx.request?;
    if request.chain_id != chain_id {
        return None;
    }
    // Pull request provides remote desired sync edge; ingest as remote progress hint.
    let _ = observe_network_runtime_peer_head(chain_id, gossip_msg.from as u64, request.to_block);

    let (response_from, response_to) = compute_runtime_sync_pull_response_range(
        chain_id,
        request.phase,
        request.from_block,
        request.to_block,
    )?;
    Some(RuntimeSyncPullResponsePlan {
        to: NodeId(gossip_msg.from as u64),
        to_wire: gossip_msg.from,
        msg_type: runtime_sync_pull_msg_type_for_phase(request.phase),
        response_from,
        response_to,
        timestamp: std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_millis() as u64)
            .unwrap_or(0),
    })
}

fn maybe_build_evm_native_sync_response(
    chain_id: u64,
    local_node: NodeId,
    msg: &ProtocolMessage,
) -> Option<(NodeId, ProtocolMessage)> {
    let ProtocolMessage::EvmNative(native_msg) = msg else {
        return None;
    };
    match native_msg {
        EvmNativeMessage::RlpxAuth {
            from,
            chain_id: auth_chain_id,
            network_id,
            auth_tag,
        } => {
            if *from == local_node || *auth_chain_id != chain_id {
                return None;
            }
            let mut ack_tag = *auth_tag;
            ack_tag.reverse();
            Some((
                *from,
                ProtocolMessage::EvmNative(EvmNativeMessage::RlpxAuthAck {
                    from: local_node,
                    chain_id,
                    network_id: *network_id,
                    ack_tag,
                }),
            ))
        }
        EvmNativeMessage::GetBlockHeaders {
            from,
            start_height,
            max,
            skip,
            reverse,
        } => {
            if *from == local_node {
                return None;
            }
            let head = get_network_runtime_sync_status(chain_id)
                .map(|s| s.current_block.max(s.highest_block))
                .unwrap_or(0);
            let max_count = (*max).max(1).min(256) as usize;
            let step = skip.saturating_add(1);
            let mut heights = Vec::with_capacity(max_count);
            let mut cursor = *start_height;
            for _ in 0..max_count {
                if *reverse {
                    heights.push(cursor);
                    if cursor < step {
                        break;
                    }
                    cursor = cursor.saturating_sub(step);
                } else {
                    if head > 0 && cursor > head {
                        break;
                    }
                    heights.push(cursor);
                    cursor = cursor.saturating_add(step);
                }
            }
            Some((
                *from,
                ProtocolMessage::EvmNative(EvmNativeMessage::BlockHeaders {
                    from: local_node,
                    heights,
                }),
            ))
        }
        EvmNativeMessage::GetBlockBodies { from, hashes } => {
            if *from == local_node {
                return None;
            }
            Some((
                *from,
                ProtocolMessage::EvmNative(EvmNativeMessage::BlockBodies {
                    from: local_node,
                    body_count: hashes.len() as u64,
                }),
            ))
        }
        EvmNativeMessage::SnapGetAccountRange { from, limit, .. } => {
            if *from == local_node {
                return None;
            }
            let account_count = (*limit).min(2048);
            let proof_node_count = account_count.saturating_div(8).max(1);
            Some((
                *from,
                ProtocolMessage::EvmNative(EvmNativeMessage::SnapAccountRange {
                    from: local_node,
                    account_count,
                    proof_node_count,
                }),
            ))
        }
        _ => None,
    }
}

fn emit_runtime_sync_pull_responses(
    local_node: NodeId,
    plan: &RuntimeSyncPullResponsePlan,
    mut send_one: impl FnMut(NodeId, &ProtocolMessage) -> bool,
    mut send_one_fallback: impl FnMut(&ProtocolMessage),
) {
    for (offset, height) in (plan.response_from..=plan.response_to).enumerate() {
        let response_payload = encode_runtime_sync_block_header_payload(height);
        let seq = plan.timestamp.saturating_add(offset as u64);
        let response = ProtocolMessage::DistributedOcccGossip(
            novovm_protocol::protocol_catalog::distributed_occc::gossip::GossipMessage {
                from: local_node.0 as u32,
                to: plan.to_wire,
                msg_type: plan.msg_type.clone(),
                payload: response_payload,
                timestamp: plan.timestamp,
                seq,
            },
        );
        if !send_one(plan.to, &response) {
            send_one_fallback(&response);
        }
    }
}

#[cfg(test)]
#[allow(dead_code)]
fn maybe_build_runtime_sync_pull_responses(
    chain_id: u64,
    local_node: NodeId,
    msg: &ProtocolMessage,
) -> Option<(NodeId, Vec<ProtocolMessage>)> {
    let sync_ctx = runtime_sync_pull_message_context(msg);
    let plan =
        maybe_plan_runtime_sync_pull_responses_with_context(chain_id, local_node, msg, &sync_ctx)?;
    let response_count = plan
        .response_to
        .saturating_sub(plan.response_from)
        .saturating_add(1);
    let mut responses = Vec::with_capacity(response_count as usize);
    for (offset, height) in (plan.response_from..=plan.response_to).enumerate() {
        let response_payload = encode_runtime_sync_block_header_payload(height);
        let seq = plan.timestamp.saturating_add(offset as u64);
        responses.push(ProtocolMessage::DistributedOcccGossip(
            novovm_protocol::protocol_catalog::distributed_occc::gossip::GossipMessage {
                from: local_node.0 as u32,
                to: plan.to_wire,
                msg_type: plan.msg_type.clone(),
                payload: response_payload,
                timestamp: plan.timestamp,
                seq,
            },
        ));
    }
    Some((plan.to, responses))
}

#[cfg(test)]
fn maybe_build_runtime_sync_pull_followup_request(
    chain_id: u64,
    local_node: NodeId,
    msg: &ProtocolMessage,
) -> Option<(NodeId, ProtocolMessage)> {
    let sync_ctx = runtime_sync_pull_message_context(msg);
    maybe_build_runtime_sync_pull_followup_request_with_context(
        chain_id, local_node, msg, &sync_ctx,
    )
}

fn maybe_build_runtime_sync_pull_followup_request_with_context(
    chain_id: u64,
    local_node: NodeId,
    msg: &ProtocolMessage,
    sync_ctx: &RuntimeSyncPullMessageContext,
) -> Option<(NodeId, ProtocolMessage)> {
    match msg {
        ProtocolMessage::DistributedOcccGossip(gossip_msg) => {
            if !sync_ctx.is_sync_pull {
                return None;
            }
            if gossip_msg.to != local_node.0 as u32 {
                return None;
            }
            // Incoming NSP1 is already a pull request, not a downloaded sync result.
            if sync_ctx.request.is_some() {
                return None;
            }
            // Only continue pull loop when response payload is a valid sync header.
            let response_height = sync_ctx.header_height?;
            let target = NodeId(gossip_msg.from as u64);
            if should_wait_runtime_sync_pull_target_window(
                chain_id,
                local_node,
                target,
                response_height,
            ) {
                return None;
            }
            let window = plan_network_runtime_sync_pull_window(chain_id)?;
            if window.from_block > window.to_block {
                return None;
            }

            let payload = encode_runtime_sync_pull_request_payload(
                chain_id,
                window.phase,
                window.from_block,
                window.to_block,
            );
            let now = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_millis() as u64)
                .unwrap_or(0);
            let request = ProtocolMessage::DistributedOcccGossip(
                novovm_protocol::protocol_catalog::distributed_occc::gossip::GossipMessage {
                    from: local_node.0 as u32,
                    to: gossip_msg.from,
                    msg_type: runtime_sync_pull_msg_type_for_phase(window.phase),
                    payload,
                    timestamp: now,
                    seq: now,
                },
            );
            Some((target, request))
        }
        ProtocolMessage::EvmNative(native_msg) => {
            let (target, observed_height_opt) = match native_msg {
                EvmNativeMessage::BlockHeaders { from, heights } => {
                    (*from, heights.iter().copied().max())
                }
                EvmNativeMessage::BlockBodies { from, .. } => (*from, None),
                EvmNativeMessage::SnapAccountRange { from, .. } => (*from, None),
                _ => return None,
            };
            if target == local_node {
                return None;
            }
            let target_state = runtime_sync_pull_target_map()
                .get(&(chain_id, local_node.0, target.0))
                .map(|entry| *entry)?;
            let observed_height = observed_height_opt.unwrap_or(target_state.to_block);
            if should_wait_runtime_sync_pull_target_window(
                chain_id,
                local_node,
                target,
                observed_height,
            ) {
                return None;
            }
            let window = plan_network_runtime_sync_pull_window(chain_id)?;
            if window.from_block > window.to_block {
                return None;
            }
            let request = build_evm_native_sync_pull_request(
                local_node,
                chain_id,
                window.phase,
                window.from_block,
                window.to_block,
            );
            Some((target, request))
        }
        _ => None,
    }
}

fn runtime_peer_id_from_protocol_message(msg: &ProtocolMessage) -> Option<u64> {
    match msg {
        ProtocolMessage::Gossip(ProtocolGossipMessage::Heartbeat { from, .. })
        | ProtocolMessage::Gossip(ProtocolGossipMessage::PeerList { from, .. })
        | ProtocolMessage::Pacemaker(PacemakerMessage::ViewSync { from, .. })
        | ProtocolMessage::Pacemaker(PacemakerMessage::NewView { from, .. })
        | ProtocolMessage::Finality(FinalityMessage::Vote { from, .. })
        | ProtocolMessage::Finality(FinalityMessage::CheckpointPropose { from, .. })
        | ProtocolMessage::Finality(FinalityMessage::Cert { from, .. }) => Some(from.0),
        ProtocolMessage::TwoPc(TwoPcMessage::Propose { tx }) => Some(tx.from.0),
        ProtocolMessage::EvmNative(native_msg) => match native_msg {
            EvmNativeMessage::DiscoveryPing { from, .. }
            | EvmNativeMessage::DiscoveryPong { from, .. }
            | EvmNativeMessage::DiscoveryFindNode { from, .. }
            | EvmNativeMessage::DiscoveryNeighbors { from, .. }
            | EvmNativeMessage::RlpxAuth { from, .. }
            | EvmNativeMessage::RlpxAuthAck { from, .. }
            | EvmNativeMessage::Hello { from, .. }
            | EvmNativeMessage::Status { from, .. }
            | EvmNativeMessage::NewBlockHashes { from, .. }
            | EvmNativeMessage::Transactions { from, .. }
            | EvmNativeMessage::GetBlockHeaders { from, .. }
            | EvmNativeMessage::BlockHeaders { from, .. }
            | EvmNativeMessage::GetBlockBodies { from, .. }
            | EvmNativeMessage::BlockBodies { from, .. }
            | EvmNativeMessage::SnapGetAccountRange { from, .. }
            | EvmNativeMessage::SnapAccountRange { from, .. } => Some(from.0),
        },
        ProtocolMessage::DistributedOcccGossip(gossip_msg) => Some(gossip_msg.from as u64),
        _ => None,
    }
}

fn refresh_peer_ip_hint_for_ip(
    peers: &DashMap<NodeId, SocketAddr>,
    peer_ip_hint_index: &DashMap<IpAddr, u64>,
    ip: IpAddr,
) {
    let mut found: Option<u64> = None;
    for entry in peers.iter() {
        if entry.value().ip() != ip {
            continue;
        }
        let peer_id = entry.key().0;
        if found.is_some() {
            peer_ip_hint_index.insert(ip, PEER_IP_HINT_AMBIGUOUS);
            return;
        }
        found = Some(peer_id);
    }
    if let Some(peer_id) = found {
        peer_ip_hint_index.insert(ip, peer_id);
    } else {
        peer_ip_hint_index.remove(&ip);
    }
}

fn maybe_learn_peer_addr(
    peers: &DashMap<NodeId, SocketAddr>,
    peer_addr_index: &DashMap<SocketAddr, NodeId>,
    peer_ip_hint_index: &DashMap<IpAddr, u64>,
    local_node: NodeId,
    src: SocketAddr,
    msg_peer_id: Option<u64>,
) {
    let Some(peer_id) = msg_peer_id else {
        return;
    };
    if peer_id == local_node.0 {
        return;
    }
    let peer_node = NodeId(peer_id);
    let should_update = peers
        .get(&peer_node)
        .map(|existing| {
            let existing_addr = *existing;
            if existing_addr.ip() != src.ip() {
                return false;
            }
            existing_addr != src
        })
        .unwrap_or(true);
    if should_update {
        if let Some(old_addr) = peers.insert(peer_node, src) {
            peer_addr_index.remove(&old_addr);
            if old_addr.ip() != src.ip() {
                refresh_peer_ip_hint_for_ip(peers, peer_ip_hint_index, old_addr.ip());
            }
        }
        peer_addr_index.insert(src, peer_node);
        refresh_peer_ip_hint_for_ip(peers, peer_ip_hint_index, src.ip());
    }
}

fn infer_peer_id_from_src_addr(
    peers: &DashMap<NodeId, SocketAddr>,
    src: SocketAddr,
) -> Option<u64> {
    let mut same_ip_peer: Option<u64> = None;
    for entry in peers.iter() {
        let addr = *entry.value();
        if addr == src {
            return Some(entry.key().0);
        }
        if addr.ip() == src.ip() {
            if same_ip_peer.is_some() {
                return None;
            }
            same_ip_peer = Some(entry.key().0);
        }
    }
    same_ip_peer
}

fn infer_peer_id_from_src_addr_with_index(
    peers: &DashMap<NodeId, SocketAddr>,
    peer_addr_index: &DashMap<SocketAddr, NodeId>,
    peer_ip_hint_index: &DashMap<IpAddr, u64>,
    src: SocketAddr,
) -> Option<u64> {
    if let Some(peer) = peer_addr_index.get(&src) {
        return Some(peer.value().0);
    }
    if let Some(peer_hint) = peer_ip_hint_index.get(&src.ip()) {
        let peer_id = *peer_hint;
        if peer_id != PEER_IP_HINT_AMBIGUOUS {
            return Some(peer_id);
        }
        return None;
    }
    infer_peer_id_from_src_addr(peers, src)
}

fn should_mark_peer_disconnected(io_err: &std::io::Error) -> bool {
    matches!(
        io_err.kind(),
        std::io::ErrorKind::ConnectionRefused
            | std::io::ErrorKind::ConnectionReset
            | std::io::ErrorKind::ConnectionAborted
            | std::io::ErrorKind::NotConnected
            | std::io::ErrorKind::TimedOut
            | std::io::ErrorKind::BrokenPipe
    ) || matches!(
        io_err.raw_os_error(),
        Some(10051 | 10054 | 10060 | 10061 | 111 | 113)
    )
}

impl Transport for UdpTransport {
    fn send(&self, to: NodeId, msg: ProtocolMessage) -> Result<(), NetworkError> {
        self.send_internal(to, &msg)
    }

    fn try_recv(&self, me: NodeId) -> Result<Option<ProtocolMessage>, NetworkError> {
        if me != self.node {
            return Err(NetworkError::LocalNodeMismatch {
                expected: self.node,
                got: me,
            });
        }

        let mut recv_buf = {
            let mut shared = self
                .recv_buf
                .lock()
                .map_err(|_| NetworkError::Io("udp recv buffer lock poisoned".to_string()))?;
            std::mem::take(&mut *shared)
        };
        if recv_buf.is_empty() {
            recv_buf.resize(1024, 0);
        }
        let recv_outcome = self.socket.recv_from(recv_buf.as_mut_slice());
        let decode_outcome = match recv_outcome {
            Ok((n, src)) => protocol_decode(&recv_buf[..n])
                .map(|decoded| Some((decoded, src)))
                .map_err(|e| NetworkError::Decode(e.to_string())),
            Err(e)
                if e.kind() == std::io::ErrorKind::WouldBlock
                    || e.kind() == std::io::ErrorKind::TimedOut
                    || e.raw_os_error() == Some(10054) =>
            {
                Ok(None)
            }
            Err(e) => Err(NetworkError::Io(e.to_string())),
        };
        let _ = self.recv_buf.lock().map(|mut shared| {
            *shared = recv_buf;
        });
        let (decoded, src) = match decode_outcome {
            Ok(Some(v)) => v,
            Ok(None) => return Ok(None),
            Err(e) => return Err(e),
        };
        let msg_peer_id = runtime_peer_id_from_protocol_message(&decoded);
        let source_peer_id_hint = if msg_peer_id.is_none() {
            infer_peer_id_from_src_addr_with_index(
                &self.peers,
                &self.peer_addr_index,
                &self.peer_ip_hint_index,
                src,
            )
        } else {
            None
        };
        let sync_ctx = runtime_sync_pull_message_context(&decoded);
        maybe_learn_peer_addr(
            &self.peers,
            &self.peer_addr_index,
            &self.peer_ip_hint_index,
            self.node,
            src,
            msg_peer_id,
        );
        if let Some(plan) = maybe_plan_runtime_sync_pull_responses_with_context(
            self.chain_id,
            self.node,
            &decoded,
            &sync_ctx,
        ) {
            emit_runtime_sync_pull_responses(
                self.node,
                &plan,
                |to, response| {
                    // Prefer registry route to keep peer activity updates on send path.
                    self.send_internal(to, response).is_ok()
                },
                |response| {
                    // Fallback to raw src addr for cases where peer registry is stale.
                    if let Ok(encoded) = protocol_encode(response) {
                        let _ = self.socket.send_to(&encoded, src);
                    }
                },
            );
        }
        if let Some((to, response)) =
            maybe_build_evm_native_sync_response(self.chain_id, self.node, &decoded)
        {
            if self.send_internal(to, &response).is_err() {
                if let Ok(encoded) = protocol_encode(&response) {
                    let _ = self.socket.send_to(&encoded, src);
                }
            }
        }
        maybe_update_runtime_sync_from_protocol_message_with_context(
            self.chain_id,
            &decoded,
            msg_peer_id,
            source_peer_id_hint,
            &sync_ctx,
        );
        if let Some((to, followup)) = maybe_build_runtime_sync_pull_followup_request_with_context(
            self.chain_id,
            self.node,
            &decoded,
            &sync_ctx,
        ) {
            if self.send_internal(to, &followup).is_err() {
                // `send` path already tracks outbound pull targets on success.
                // When falling back to raw socket send, track once here.
                maybe_track_runtime_sync_pull_request_outbound_send(
                    self.chain_id,
                    self.node,
                    to,
                    &followup,
                );
                if let Ok(encoded) = protocol_encode(&followup) {
                    let _ = self.socket.send_to(&encoded, src);
                }
            }
        }
        Ok(Some(decoded))
    }
}

impl Transport for TcpTransport {
    fn send(&self, to: NodeId, msg: ProtocolMessage) -> Result<(), NetworkError> {
        self.send_internal(to, &msg)
    }

    fn try_recv(&self, me: NodeId) -> Result<Option<ProtocolMessage>, NetworkError> {
        if me != self.node {
            return Err(NetworkError::LocalNodeMismatch {
                expected: self.node,
                got: me,
            });
        }

        let (mut stream, addr) = match self.listener.accept() {
            Ok(v) => v,
            Err(e)
                if e.kind() == std::io::ErrorKind::WouldBlock
                    || e.kind() == std::io::ErrorKind::TimedOut =>
            {
                return Ok(None);
            }
            Err(e) => return Err(NetworkError::Io(e.to_string())),
        };
        stream
            .set_nonblocking(false)
            .map_err(|e| NetworkError::Io(e.to_string()))?;

        let mut len_buf = [0u8; 4];
        stream
            .read_exact(&mut len_buf)
            .map_err(|e| NetworkError::Io(e.to_string()))?;
        let frame_len = u32::from_le_bytes(len_buf) as usize;
        if frame_len == 0 || frame_len > self.max_packet_size {
            return Err(NetworkError::Decode(format!(
                "invalid tcp frame len={frame_len}, max={}",
                self.max_packet_size
            )));
        }
        let mut recv_frame_buf = {
            let mut shared = self
                .recv_frame_buf
                .lock()
                .map_err(|_| NetworkError::Io("tcp recv buffer lock poisoned".to_string()))?;
            std::mem::take(&mut *shared)
        };
        if recv_frame_buf.len() < frame_len {
            recv_frame_buf.resize(frame_len, 0);
        }
        stream
            .read_exact(&mut recv_frame_buf[..frame_len])
            .map_err(|e| NetworkError::Io(e.to_string()))?;
        let decode_outcome = protocol_decode(&recv_frame_buf[..frame_len])
            .map_err(|e| NetworkError::Decode(e.to_string()));
        let _ = self.recv_frame_buf.lock().map(|mut shared| {
            *shared = recv_frame_buf;
        });
        let decoded = decode_outcome?;
        let msg_peer_id = runtime_peer_id_from_protocol_message(&decoded);
        let source_peer_id_hint = if msg_peer_id.is_none() {
            infer_peer_id_from_src_addr_with_index(
                &self.peers,
                &self.peer_addr_index,
                &self.peer_ip_hint_index,
                addr,
            )
        } else {
            None
        };
        let sync_ctx = runtime_sync_pull_message_context(&decoded);
        maybe_learn_peer_addr(
            &self.peers,
            &self.peer_addr_index,
            &self.peer_ip_hint_index,
            self.node,
            addr,
            msg_peer_id,
        );
        if let Some(plan) = maybe_plan_runtime_sync_pull_responses_with_context(
            self.chain_id,
            self.node,
            &decoded,
            &sync_ctx,
        ) {
            emit_runtime_sync_pull_responses(
                self.node,
                &plan,
                |to, response| self.send_internal(to, response).is_ok(),
                |response| {
                    if let Ok(encoded) = protocol_encode(response) {
                        let _ = write_tcp_frame(&mut stream, &encoded);
                    }
                },
            );
        }
        if let Some((to, response)) =
            maybe_build_evm_native_sync_response(self.chain_id, self.node, &decoded)
        {
            if self.send_internal(to, &response).is_err() {
                if let Ok(encoded) = protocol_encode(&response) {
                    let _ = write_tcp_frame(&mut stream, &encoded);
                }
            }
        }
        maybe_update_runtime_sync_from_protocol_message_with_context(
            self.chain_id,
            &decoded,
            msg_peer_id,
            source_peer_id_hint,
            &sync_ctx,
        );
        if let Some((to, followup)) = maybe_build_runtime_sync_pull_followup_request_with_context(
            self.chain_id,
            self.node,
            &decoded,
            &sync_ctx,
        ) {
            if self.send_internal(to, &followup).is_err() {
                // `send` path already tracks outbound pull targets on success.
                // When falling back to raw tcp stream send, track once here.
                maybe_track_runtime_sync_pull_request_outbound_send(
                    self.chain_id,
                    self.node,
                    to,
                    &followup,
                );
                if let Ok(encoded) = protocol_encode(&followup) {
                    let _ = write_tcp_frame(&mut stream, &encoded);
                }
            }
        }
        Ok(Some(decoded))
    }
}

fn write_tcp_frame(stream: &mut TcpStream, payload: &[u8]) -> Result<(), std::io::Error> {
    let len_u32 = u32::try_from(payload.len())
        .map_err(|_| std::io::Error::new(std::io::ErrorKind::InvalidData, "tcp frame too large"))?;
    stream.write_all(&len_u32.to_le_bytes())?;
    stream.write_all(payload)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        get_network_runtime_sync_status, set_network_runtime_sync_status, NetworkRuntimeSyncStatus,
    };
    use novovm_protocol::{
        encode_block_header_wire_v1,
        protocol_catalog::distributed_occc::gossip::{
            GossipMessage as DistributedGossipMessage, MessageType as DistributedMessageType,
        },
        BlockHeaderWireV1, CheckpointId, ConsensusPluginBindingV1, FinalityMessage, GossipMessage,
        PacemakerMessage, ShardId, CONSENSUS_PLUGIN_CLASS_CODE,
    };
    use std::collections::HashSet;
    use std::net::TcpListener;
    use std::thread;
    use std::time::Duration;

    #[test]
    fn in_memory_transport_roundtrip() {
        let t = InMemoryTransport::new(8);
        let n0 = NodeId(0);
        let n1 = NodeId(1);
        t.register(n0);
        t.register(n1);
        let msg = ProtocolMessage::Gossip(GossipMessage::Heartbeat {
            from: n0,
            shard: ShardId(1),
        });
        t.send(n1, msg).unwrap();
        let recv = t.try_recv(n1).unwrap();
        assert!(matches!(
            recv,
            Some(ProtocolMessage::Gossip(GossipMessage::Heartbeat { .. }))
        ));
    }

    #[test]
    fn udp_transport_roundtrip() {
        let n0 = NodeId(0);
        let n1 = NodeId(1);
        let t0 = UdpTransport::bind(n0, "127.0.0.1:0").unwrap();
        let t1 = UdpTransport::bind(n1, "127.0.0.1:0").unwrap();
        let a0 = t0.local_addr().unwrap();
        let a1 = t1.local_addr().unwrap();
        t0.register_peer(n1, &a1.to_string()).unwrap();
        t1.register_peer(n0, &a0.to_string()).unwrap();

        let msg = ProtocolMessage::Gossip(GossipMessage::Heartbeat {
            from: n0,
            shard: ShardId(7),
        });
        t0.send(n1, msg).unwrap();

        let started = std::time::Instant::now();
        let mut got = None;
        while started.elapsed() < Duration::from_millis(500) {
            if let Some(m) = t1.try_recv(n1).unwrap() {
                got = Some(m);
                break;
            }
            thread::sleep(Duration::from_millis(5));
        }
        assert!(matches!(
            got,
            Some(ProtocolMessage::Gossip(GossipMessage::Heartbeat { .. }))
        ));
    }

    #[test]
    fn udp_register_peer_updates_runtime_sync_peer_count() {
        let chain_id = 9_991_u64;
        let n0 = NodeId(100);
        let n1 = NodeId(101);
        let n2 = NodeId(102);

        let t0 = UdpTransport::bind_for_chain(n0, "127.0.0.1:0", chain_id).unwrap();
        let t1 = UdpTransport::bind_for_chain(n1, "127.0.0.1:0", chain_id).unwrap();
        let t2 = UdpTransport::bind_for_chain(n2, "127.0.0.1:0", chain_id).unwrap();
        let a1 = t1.local_addr().unwrap();
        let a2 = t2.local_addr().unwrap();

        t0.register_peer(n1, &a1.to_string()).unwrap();
        t0.register_peer(n2, &a2.to_string()).unwrap();

        let status =
            get_network_runtime_sync_status(chain_id).expect("runtime sync status should exist");
        assert_eq!(status.peer_count, 2);
    }

    #[test]
    fn udp_unregister_peer_updates_runtime_sync_peer_count() {
        let chain_id = 9_994_u64;
        let n0 = NodeId(120);
        let n1 = NodeId(121);
        let n2 = NodeId(122);

        let t0 = UdpTransport::bind_for_chain(n0, "127.0.0.1:0", chain_id).unwrap();
        let t1 = UdpTransport::bind_for_chain(n1, "127.0.0.1:0", chain_id).unwrap();
        let t2 = UdpTransport::bind_for_chain(n2, "127.0.0.1:0", chain_id).unwrap();
        let a1 = t1.local_addr().unwrap();
        let a2 = t2.local_addr().unwrap();

        t0.register_peer(n1, &a1.to_string()).unwrap();
        t0.register_peer(n2, &a2.to_string()).unwrap();
        let status =
            get_network_runtime_sync_status(chain_id).expect("runtime sync status should exist");
        assert_eq!(status.peer_count, 2);

        t0.unregister_peer(n1).unwrap();
        let status =
            get_network_runtime_sync_status(chain_id).expect("runtime sync status should exist");
        assert_eq!(status.peer_count, 1);

        t0.unregister_peer(n2).unwrap();
        let status =
            get_network_runtime_sync_status(chain_id).expect("runtime sync status should exist");
        assert_eq!(status.peer_count, 0);
        assert_eq!(status.highest_block, status.current_block);
    }

    #[test]
    fn tcp_send_connect_failure_marks_runtime_peer_disconnected() {
        let chain_id = 9_995_u64;
        let n0 = NodeId(130);
        let n1 = NodeId(131);

        let mut t0 = TcpTransport::bind_for_chain(n0, "127.0.0.1:0", chain_id).unwrap();
        t0.set_connect_timeout_ms(20);

        let tmp_listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let peer_addr = tmp_listener.local_addr().unwrap();
        drop(tmp_listener);
        t0.register_peer(n1, &peer_addr.to_string()).unwrap();

        let before =
            get_network_runtime_sync_status(chain_id).expect("runtime sync status should exist");
        assert_eq!(before.peer_count, 1);

        let msg = ProtocolMessage::Gossip(GossipMessage::Heartbeat {
            from: n0,
            shard: ShardId(1),
        });
        let res = t0.send(n1, msg);
        assert!(res.is_err());

        let after =
            get_network_runtime_sync_status(chain_id).expect("runtime sync status should exist");
        assert_eq!(after.peer_count, 0);
    }

    #[test]
    fn udp_try_recv_updates_runtime_progress_from_pacemaker_messages() {
        let chain_id = 9_992_u64;
        let n0 = NodeId(200);
        let n1 = NodeId(201);

        let t0 = UdpTransport::bind_for_chain(n0, "127.0.0.1:0", chain_id).unwrap();
        let t1 = UdpTransport::bind_for_chain(n1, "127.0.0.1:0", chain_id).unwrap();
        let a0 = t0.local_addr().unwrap();
        let a1 = t1.local_addr().unwrap();
        t0.register_peer(n1, &a1.to_string()).unwrap();
        t1.register_peer(n0, &a0.to_string()).unwrap();

        let msg = ProtocolMessage::Pacemaker(PacemakerMessage::NewView {
            from: n0,
            height: 12,
            view: 3,
            high_qc_height: 19,
        });
        t0.send(n1, msg).unwrap();

        let started = std::time::Instant::now();
        let mut got = None;
        while started.elapsed() < Duration::from_millis(500) {
            if let Some(m) = t1.try_recv(n1).unwrap() {
                got = Some(m);
                break;
            }
            thread::sleep(Duration::from_millis(5));
        }
        assert!(matches!(
            got,
            Some(ProtocolMessage::Pacemaker(PacemakerMessage::NewView { .. }))
        ));
        let status =
            get_network_runtime_sync_status(chain_id).expect("runtime sync status should exist");
        assert_eq!(status.current_block, 19);
        assert_eq!(status.highest_block, 19);
        assert_eq!(status.starting_block, 19);
    }

    #[test]
    fn udp_try_recv_registers_runtime_peer_from_message_sender() {
        let chain_id = 9_996_u64;
        let n0 = NodeId(220);
        let n1 = NodeId(221);

        let t0 = UdpTransport::bind_for_chain(n0, "127.0.0.1:0", chain_id).unwrap();
        let t1 = UdpTransport::bind_for_chain(n1, "127.0.0.1:0", chain_id).unwrap();
        let a1 = t1.local_addr().unwrap();
        t0.register_peer(n1, &a1.to_string()).unwrap();

        let before =
            get_network_runtime_sync_status(chain_id).expect("runtime sync status should exist");
        assert_eq!(before.peer_count, 1);

        let msg = ProtocolMessage::Gossip(GossipMessage::Heartbeat {
            from: n0,
            shard: ShardId(5),
        });
        t0.send(n1, msg).unwrap();

        let started = std::time::Instant::now();
        while started.elapsed() < Duration::from_millis(500) {
            if t1.try_recv(n1).unwrap().is_some() {
                break;
            }
            thread::sleep(Duration::from_millis(5));
        }

        let after =
            get_network_runtime_sync_status(chain_id).expect("runtime sync status should exist");
        assert_eq!(after.peer_count, 2);
    }

    #[test]
    fn udp_try_recv_autolearns_sender_addr_for_reply_send() {
        let chain_id = 9_997_u64;
        let n0 = NodeId(230);
        let n1 = NodeId(231);

        let t0 = UdpTransport::bind_for_chain(n0, "127.0.0.1:0", chain_id).unwrap();
        let t1 = UdpTransport::bind_for_chain(n1, "127.0.0.1:0", chain_id).unwrap();
        let a1 = t1.local_addr().unwrap();
        t0.register_peer(n1, &a1.to_string()).unwrap();

        t0.send(
            n1,
            ProtocolMessage::Gossip(GossipMessage::Heartbeat {
                from: n0,
                shard: ShardId(8),
            }),
        )
        .unwrap();

        let started = std::time::Instant::now();
        while started.elapsed() < Duration::from_millis(500) {
            if t1.try_recv(n1).unwrap().is_some() {
                break;
            }
            thread::sleep(Duration::from_millis(5));
        }

        let send_back = t1.send(
            n0,
            ProtocolMessage::Gossip(GossipMessage::Heartbeat {
                from: n1,
                shard: ShardId(9),
            }),
        );
        assert!(send_back.is_ok());

        let started = std::time::Instant::now();
        let mut got_back = false;
        while started.elapsed() < Duration::from_millis(500) {
            if t0.try_recv(n0).unwrap().is_some() {
                got_back = true;
                break;
            }
            thread::sleep(Duration::from_millis(5));
        }
        assert!(got_back);
    }

    #[test]
    fn udp_try_recv_updates_runtime_progress_from_state_sync_block_header_wire() {
        let chain_id = 9_993_u64;
        let n0 = NodeId(210);
        let n1 = NodeId(211);

        let t0 = UdpTransport::bind_for_chain(n0, "127.0.0.1:0", chain_id).unwrap();
        let t1 = UdpTransport::bind_for_chain(n1, "127.0.0.1:0", chain_id).unwrap();
        let a0 = t0.local_addr().unwrap();
        let a1 = t1.local_addr().unwrap();
        t0.register_peer(n1, &a1.to_string()).unwrap();
        t1.register_peer(n0, &a0.to_string()).unwrap();

        let header = BlockHeaderWireV1 {
            height: 88,
            epoch_id: 7,
            parent_hash: [1u8; 32],
            state_root: [2u8; 32],
            governance_chain_audit_root: [3u8; 32],
            tx_count: 5,
            batch_count: 2,
            consensus_binding: ConsensusPluginBindingV1 {
                plugin_class_code: CONSENSUS_PLUGIN_CLASS_CODE,
                adapter_hash: [4u8; 32],
            },
        };
        let state_sync = ProtocolMessage::DistributedOcccGossip(DistributedGossipMessage {
            from: n0.0 as u32,
            to: n1.0 as u32,
            msg_type: DistributedMessageType::StateSync,
            payload: encode_block_header_wire_v1(&header),
            timestamp: 0,
            seq: 1,
        });
        t0.send(n1, state_sync).unwrap();

        let started = std::time::Instant::now();
        let mut got = None;
        while started.elapsed() < Duration::from_millis(500) {
            if let Some(m) = t1.try_recv(n1).unwrap() {
                got = Some(m);
                break;
            }
            thread::sleep(Duration::from_millis(5));
        }
        assert!(matches!(
            got,
            Some(ProtocolMessage::DistributedOcccGossip(_))
        ));
        let status =
            get_network_runtime_sync_status(chain_id).expect("runtime sync status should exist");
        assert_eq!(status.current_block, 88);
        assert_eq!(status.highest_block, 88);
        assert_eq!(status.starting_block, 88);
    }

    #[test]
    fn udp_try_recv_state_sync_advances_local_progress_when_sender_field_is_remote() {
        let chain_id = 9_877_u64;
        let n0 = NodeId(240);
        let n1 = NodeId(241);
        let remote_sender = NodeId(999);

        let t0 = UdpTransport::bind_for_chain(n0, "127.0.0.1:0", chain_id).unwrap();
        let t1 = UdpTransport::bind_for_chain(n1, "127.0.0.1:0", chain_id).unwrap();
        let a0 = t0.local_addr().unwrap();
        let a1 = t1.local_addr().unwrap();
        t0.register_peer(n1, &a1.to_string()).unwrap();
        t1.register_peer(n0, &a0.to_string()).unwrap();

        let header = BlockHeaderWireV1 {
            height: 233,
            epoch_id: 5,
            parent_hash: [9u8; 32],
            state_root: [8u8; 32],
            governance_chain_audit_root: [7u8; 32],
            tx_count: 4,
            batch_count: 1,
            consensus_binding: ConsensusPluginBindingV1 {
                plugin_class_code: CONSENSUS_PLUGIN_CLASS_CODE,
                adapter_hash: [6u8; 32],
            },
        };
        let state_sync = ProtocolMessage::DistributedOcccGossip(DistributedGossipMessage {
            from: remote_sender.0 as u32,
            to: n1.0 as u32,
            msg_type: DistributedMessageType::StateSync,
            payload: encode_block_header_wire_v1(&header),
            timestamp: 0,
            seq: 3,
        });
        t0.send(n1, state_sync).unwrap();

        let started = std::time::Instant::now();
        while started.elapsed() < Duration::from_millis(500) {
            if t1.try_recv(n1).unwrap().is_some() {
                break;
            }
            thread::sleep(Duration::from_millis(5));
        }

        let status =
            get_network_runtime_sync_status(chain_id).expect("runtime sync status should exist");
        assert_eq!(status.current_block, 233);
        assert_eq!(status.highest_block, 233);
        assert_eq!(status.starting_block, 233);
    }

    #[test]
    fn udp_try_recv_updates_runtime_progress_from_shard_state_block_header_wire() {
        let chain_id = 9_883_u64;
        let n0 = NodeId(212);
        let n1 = NodeId(213);

        let t0 = UdpTransport::bind_for_chain(n0, "127.0.0.1:0", chain_id).unwrap();
        let t1 = UdpTransport::bind_for_chain(n1, "127.0.0.1:0", chain_id).unwrap();
        let a0 = t0.local_addr().unwrap();
        let a1 = t1.local_addr().unwrap();
        t0.register_peer(n1, &a1.to_string()).unwrap();
        t1.register_peer(n0, &a0.to_string()).unwrap();

        let header = BlockHeaderWireV1 {
            height: 144,
            epoch_id: 11,
            parent_hash: [5u8; 32],
            state_root: [6u8; 32],
            governance_chain_audit_root: [7u8; 32],
            tx_count: 9,
            batch_count: 3,
            consensus_binding: ConsensusPluginBindingV1 {
                plugin_class_code: CONSENSUS_PLUGIN_CLASS_CODE,
                adapter_hash: [8u8; 32],
            },
        };
        let shard_state = ProtocolMessage::DistributedOcccGossip(DistributedGossipMessage {
            from: n0.0 as u32,
            to: n1.0 as u32,
            msg_type: DistributedMessageType::ShardState,
            payload: encode_block_header_wire_v1(&header),
            timestamp: 0,
            seq: 2,
        });
        t0.send(n1, shard_state).unwrap();

        let started = std::time::Instant::now();
        let mut got = None;
        while started.elapsed() < Duration::from_millis(500) {
            if let Some(m) = t1.try_recv(n1).unwrap() {
                got = Some(m);
                break;
            }
            thread::sleep(Duration::from_millis(5));
        }
        assert!(matches!(
            got,
            Some(ProtocolMessage::DistributedOcccGossip(_))
        ));
        let status =
            get_network_runtime_sync_status(chain_id).expect("runtime sync status should exist");
        assert_eq!(status.current_block, 144);
        assert_eq!(status.highest_block, 144);
        assert_eq!(status.starting_block, 144);
    }

    #[test]
    fn runtime_sync_receive_path_treats_shard_state_as_local_progress() {
        let chain_id = 9_888_u64;
        let remote = NodeId(901);
        let local = NodeId(902);

        let header = BlockHeaderWireV1 {
            height: 777,
            epoch_id: 13,
            parent_hash: [0x11u8; 32],
            state_root: [0x22u8; 32],
            governance_chain_audit_root: [0x33u8; 32],
            tx_count: 3,
            batch_count: 1,
            consensus_binding: ConsensusPluginBindingV1 {
                plugin_class_code: CONSENSUS_PLUGIN_CLASS_CODE,
                adapter_hash: [0x44u8; 32],
            },
        };
        let shard_state = ProtocolMessage::DistributedOcccGossip(DistributedGossipMessage {
            from: remote.0 as u32,
            to: local.0 as u32,
            msg_type: DistributedMessageType::ShardState,
            payload: encode_block_header_wire_v1(&header),
            timestamp: 0,
            seq: 1,
        });

        maybe_update_runtime_sync_from_protocol_message(chain_id, &shard_state, None, None);

        let status =
            get_network_runtime_sync_status(chain_id).expect("runtime sync status should exist");
        assert_eq!(status.current_block, 777);
        assert_eq!(status.highest_block, 777);
    }

    #[test]
    fn runtime_sync_pull_request_payload_decodes_nsp1() {
        let mut payload = Vec::new();
        payload.extend_from_slice(&RUNTIME_SYNC_PULL_REQUEST_MAGIC);
        payload.push(3);
        payload.extend_from_slice(&55u64.to_le_bytes());
        payload.extend_from_slice(&101u64.to_le_bytes());
        payload.extend_from_slice(&164u64.to_le_bytes());

        let decoded = decode_runtime_sync_pull_request(&payload).expect("decode nsp1 payload");
        assert_eq!(decoded.phase, NetworkRuntimeNativeSyncPhaseV1::Bodies);
        assert_eq!(decoded.chain_id, 55);
        assert_eq!(decoded.from_block, 101);
        assert_eq!(decoded.to_block, 164);
    }

    #[test]
    fn runtime_sync_pull_tracking_uses_capped_response_target() {
        let chain_id = 9_892_u64;
        let local = NodeId(940);
        let remote = NodeId(941);
        clear_runtime_sync_pull_target(chain_id, local, remote);

        let outbound = ProtocolMessage::DistributedOcccGossip(DistributedGossipMessage {
            from: local.0 as u32,
            to: remote.0 as u32,
            msg_type: DistributedMessageType::StateSync,
            payload: encode_runtime_sync_pull_request_payload(
                chain_id,
                NetworkRuntimeNativeSyncPhaseV1::Headers,
                1_000,
                4_000,
            ),
            timestamp: 0,
            seq: 1,
        });
        maybe_track_runtime_sync_pull_request_outbound(chain_id, local, &outbound);
        let tracked_to =
            get_runtime_sync_pull_target(chain_id, local, remote).expect("target should exist");
        assert_eq!(
            tracked_to,
            1_000 + RUNTIME_SYNC_PULL_RESPONSE_BATCH_MAX - 1,
            "tracked target should follow single-response capped upper bound"
        );
        clear_runtime_sync_pull_target(chain_id, local, remote);
    }

    #[test]
    fn runtime_sync_pull_shard_state_request_triggers_shard_state_response() {
        let chain_id = 9_893_u64;
        let requester = NodeId(950);
        let responder = NodeId(951);

        let tx = UdpTransport::bind_for_chain(requester, "127.0.0.1:0", chain_id).unwrap();
        let rx = UdpTransport::bind_for_chain(responder, "127.0.0.1:0", chain_id).unwrap();
        let tx_addr = tx.local_addr().unwrap();
        let rx_addr = rx.local_addr().unwrap();
        tx.register_peer(responder, &rx_addr.to_string()).unwrap();
        rx.register_peer(requester, &tx_addr.to_string()).unwrap();

        set_network_runtime_sync_status(
            chain_id,
            NetworkRuntimeSyncStatus {
                peer_count: 2,
                starting_block: 400,
                current_block: 420,
                highest_block: 520,
            },
        );

        let pull_request = ProtocolMessage::DistributedOcccGossip(DistributedGossipMessage {
            from: requester.0 as u32,
            to: responder.0 as u32,
            msg_type: DistributedMessageType::ShardState,
            payload: encode_runtime_sync_pull_request_payload(
                chain_id,
                NetworkRuntimeNativeSyncPhaseV1::Bodies,
                410,
                415,
            ),
            timestamp: 0,
            seq: 1,
        });
        tx.send(responder, pull_request).unwrap();

        let started = std::time::Instant::now();
        while started.elapsed() < Duration::from_millis(500) {
            if rx.try_recv(responder).unwrap().is_some() {
                break;
            }
            thread::sleep(Duration::from_millis(5));
        }

        let started_reply = std::time::Instant::now();
        let mut got_reply = false;
        while started_reply.elapsed() < Duration::from_millis(500) {
            if let Some(msg) = tx.try_recv(requester).unwrap() {
                let ProtocolMessage::DistributedOcccGossip(reply) = msg else {
                    continue;
                };
                if matches!(reply.msg_type, DistributedMessageType::ShardState)
                    && decode_block_header_wire_v1(&reply.payload).is_ok()
                {
                    got_reply = true;
                    break;
                }
            }
            thread::sleep(Duration::from_millis(5));
        }
        assert!(got_reply, "expected shard-state sync response");
    }

    #[test]
    fn runtime_sync_pull_followup_request_builds_next_window() {
        let chain_id = 9_890_u64;
        let local = NodeId(920);
        let remote = NodeId(921);
        set_network_runtime_sync_status(
            chain_id,
            NetworkRuntimeSyncStatus {
                peer_count: 3,
                starting_block: 600,
                current_block: 640,
                highest_block: 700,
            },
        );
        set_runtime_sync_pull_target(chain_id, local, remote, 650);

        let header_before_target = BlockHeaderWireV1 {
            height: 640,
            epoch_id: 1,
            parent_hash: [0x11u8; 32],
            state_root: [0x22u8; 32],
            governance_chain_audit_root: [0x33u8; 32],
            tx_count: 0,
            batch_count: 0,
            consensus_binding: ConsensusPluginBindingV1 {
                plugin_class_code: CONSENSUS_PLUGIN_CLASS_CODE,
                adapter_hash: [0x44u8; 32],
            },
        };
        let sync_reply_before_target =
            ProtocolMessage::DistributedOcccGossip(DistributedGossipMessage {
                from: remote.0 as u32,
                to: local.0 as u32,
                msg_type: DistributedMessageType::StateSync,
                payload: encode_block_header_wire_v1(&header_before_target),
                timestamp: 0,
                seq: 1,
            });
        assert!(
            maybe_build_runtime_sync_pull_followup_request(
                chain_id,
                local,
                &sync_reply_before_target
            )
            .is_none(),
            "should wait until current window target is reached"
        );

        set_network_runtime_sync_status(
            chain_id,
            NetworkRuntimeSyncStatus {
                peer_count: 3,
                starting_block: 600,
                current_block: 650,
                highest_block: 700,
            },
        );
        let header_on_target = BlockHeaderWireV1 {
            height: 650,
            epoch_id: 1,
            parent_hash: [0x11u8; 32],
            state_root: [0x22u8; 32],
            governance_chain_audit_root: [0x33u8; 32],
            tx_count: 0,
            batch_count: 0,
            consensus_binding: ConsensusPluginBindingV1 {
                plugin_class_code: CONSENSUS_PLUGIN_CLASS_CODE,
                adapter_hash: [0x44u8; 32],
            },
        };
        let sync_reply_on_target =
            ProtocolMessage::DistributedOcccGossip(DistributedGossipMessage {
                from: remote.0 as u32,
                to: local.0 as u32,
                msg_type: DistributedMessageType::StateSync,
                payload: encode_block_header_wire_v1(&header_on_target),
                timestamp: 0,
                seq: 2,
            });

        let (target, followup) =
            maybe_build_runtime_sync_pull_followup_request(chain_id, local, &sync_reply_on_target)
                .expect("followup request should be generated");
        assert_eq!(target, remote);
        let ProtocolMessage::DistributedOcccGossip(followup_msg) = followup else {
            panic!("followup should be distributed gossip");
        };
        assert!(matches!(
            followup_msg.msg_type,
            DistributedMessageType::ShardState
        ));
        let payload = decode_runtime_sync_pull_request(&followup_msg.payload)
            .expect("followup payload should be NSP1");
        assert_eq!(payload.phase, NetworkRuntimeNativeSyncPhaseV1::Finalize);
        assert_eq!(payload.chain_id, chain_id);
        assert_eq!(payload.from_block, 651);
        assert!(payload.to_block >= payload.from_block);
        assert!(payload.to_block <= 700);
    }

    #[test]
    fn runtime_sync_pull_state_phase_uses_smaller_response_cap() {
        let chain_id = 9_895_u64;
        let local = NodeId(970);
        let remote = NodeId(971);
        clear_runtime_sync_pull_target(chain_id, local, remote);

        let outbound = ProtocolMessage::DistributedOcccGossip(DistributedGossipMessage {
            from: local.0 as u32,
            to: remote.0 as u32,
            msg_type: DistributedMessageType::ShardState,
            payload: encode_runtime_sync_pull_request_payload(
                chain_id,
                NetworkRuntimeNativeSyncPhaseV1::State,
                1_000,
                4_000,
            ),
            timestamp: 0,
            seq: 1,
        });
        maybe_track_runtime_sync_pull_request_outbound(chain_id, local, &outbound);
        let tracked_to =
            get_runtime_sync_pull_target(chain_id, local, remote).expect("target should exist");
        assert_eq!(tracked_to, 1_031);
        clear_runtime_sync_pull_target(chain_id, local, remote);
    }

    #[test]
    fn runtime_sync_pull_headers_prefetch_can_trigger_followup_before_window_tail() {
        let chain_id = 9_896_u64;
        let local = NodeId(972);
        let remote = NodeId(973);
        clear_runtime_sync_pull_target(chain_id, local, remote);

        set_network_runtime_sync_status(
            chain_id,
            NetworkRuntimeSyncStatus {
                peer_count: 3,
                starting_block: 600,
                current_block: 640,
                highest_block: 700,
            },
        );

        let outbound = ProtocolMessage::DistributedOcccGossip(DistributedGossipMessage {
            from: local.0 as u32,
            to: remote.0 as u32,
            msg_type: DistributedMessageType::StateSync,
            payload: encode_runtime_sync_pull_request_payload(
                chain_id,
                NetworkRuntimeNativeSyncPhaseV1::Headers,
                641,
                700,
            ),
            timestamp: 0,
            seq: 1,
        });
        maybe_track_runtime_sync_pull_request_outbound(chain_id, local, &outbound);
        let tracked_to =
            get_runtime_sync_pull_target(chain_id, local, remote).expect("target should exist");
        assert_eq!(tracked_to, 700);

        set_network_runtime_sync_status(
            chain_id,
            NetworkRuntimeSyncStatus {
                peer_count: 3,
                starting_block: 600,
                current_block: 691,
                highest_block: 700,
            },
        );
        let before_prefetch = ProtocolMessage::DistributedOcccGossip(DistributedGossipMessage {
            from: remote.0 as u32,
            to: local.0 as u32,
            msg_type: DistributedMessageType::StateSync,
            payload: encode_block_header_wire_v1(&BlockHeaderWireV1 {
                height: 691,
                epoch_id: 1,
                parent_hash: [0x11u8; 32],
                state_root: [0x22u8; 32],
                governance_chain_audit_root: [0x33u8; 32],
                tx_count: 0,
                batch_count: 0,
                consensus_binding: ConsensusPluginBindingV1 {
                    plugin_class_code: CONSENSUS_PLUGIN_CLASS_CODE,
                    adapter_hash: [0x44u8; 32],
                },
            }),
            timestamp: 0,
            seq: 2,
        });
        assert!(
            maybe_build_runtime_sync_pull_followup_request(chain_id, local, &before_prefetch)
                .is_none(),
            "should still wait before prefetch trigger height"
        );

        set_network_runtime_sync_status(
            chain_id,
            NetworkRuntimeSyncStatus {
                peer_count: 3,
                starting_block: 600,
                current_block: 692,
                highest_block: 700,
            },
        );
        let on_prefetch = ProtocolMessage::DistributedOcccGossip(DistributedGossipMessage {
            from: remote.0 as u32,
            to: local.0 as u32,
            msg_type: DistributedMessageType::StateSync,
            payload: encode_block_header_wire_v1(&BlockHeaderWireV1 {
                height: 692,
                epoch_id: 1,
                parent_hash: [0x11u8; 32],
                state_root: [0x22u8; 32],
                governance_chain_audit_root: [0x33u8; 32],
                tx_count: 0,
                batch_count: 0,
                consensus_binding: ConsensusPluginBindingV1 {
                    plugin_class_code: CONSENSUS_PLUGIN_CLASS_CODE,
                    adapter_hash: [0x44u8; 32],
                },
            }),
            timestamp: 0,
            seq: 3,
        });
        let (_target, followup) =
            maybe_build_runtime_sync_pull_followup_request(chain_id, local, &on_prefetch)
                .expect("prefetch trigger should generate followup");
        let ProtocolMessage::DistributedOcccGossip(followup_msg) = followup else {
            panic!("followup should be distributed gossip");
        };
        let payload = decode_runtime_sync_pull_request(&followup_msg.payload)
            .expect("followup payload should be NSP1");
        assert_eq!(payload.chain_id, chain_id);
        assert_eq!(payload.from_block, 693);
        assert!(payload.to_block >= payload.from_block);
        clear_runtime_sync_pull_target(chain_id, local, remote);
    }

    #[test]
    fn runtime_sync_pull_followup_preserves_shard_state_channel() {
        let chain_id = 9_894_u64;
        let local = NodeId(960);
        let remote = NodeId(961);
        set_network_runtime_sync_status(
            chain_id,
            NetworkRuntimeSyncStatus {
                peer_count: 2,
                starting_block: 50,
                current_block: 60,
                highest_block: 90,
            },
        );
        set_runtime_sync_pull_target(chain_id, local, remote, 60);

        let reply_header = BlockHeaderWireV1 {
            height: 60,
            epoch_id: 1,
            parent_hash: [0x11u8; 32],
            state_root: [0x22u8; 32],
            governance_chain_audit_root: [0x33u8; 32],
            tx_count: 0,
            batch_count: 0,
            consensus_binding: ConsensusPluginBindingV1 {
                plugin_class_code: CONSENSUS_PLUGIN_CLASS_CODE,
                adapter_hash: [0x44u8; 32],
            },
        };
        let shard_reply = ProtocolMessage::DistributedOcccGossip(DistributedGossipMessage {
            from: remote.0 as u32,
            to: local.0 as u32,
            msg_type: DistributedMessageType::ShardState,
            payload: encode_block_header_wire_v1(&reply_header),
            timestamp: 0,
            seq: 1,
        });
        let (_target, followup) =
            maybe_build_runtime_sync_pull_followup_request(chain_id, local, &shard_reply)
                .expect("followup should exist");
        let ProtocolMessage::DistributedOcccGossip(next_msg) = followup else {
            panic!("followup should be distributed gossip");
        };
        assert!(
            matches!(next_msg.msg_type, DistributedMessageType::ShardState),
            "followup should preserve request channel"
        );
        assert!(
            decode_runtime_sync_pull_request(&next_msg.payload).is_some(),
            "followup payload should remain NSP1"
        );
        clear_runtime_sync_pull_target(chain_id, local, remote);
    }

    #[test]
    fn udp_state_sync_pull_request_triggers_block_header_response() {
        let chain_id = 9_889_u64;
        let requester = NodeId(910);
        let responder = NodeId(911);

        let tx = UdpTransport::bind_for_chain(requester, "127.0.0.1:0", chain_id).unwrap();
        let rx = UdpTransport::bind_for_chain(responder, "127.0.0.1:0", chain_id).unwrap();
        let tx_addr = tx.local_addr().unwrap();
        let rx_addr = rx.local_addr().unwrap();
        tx.register_peer(responder, &rx_addr.to_string()).unwrap();
        rx.register_peer(requester, &tx_addr.to_string()).unwrap();

        set_network_runtime_sync_status(
            chain_id,
            NetworkRuntimeSyncStatus {
                peer_count: 2,
                starting_block: 500,
                current_block: 500,
                highest_block: 800,
            },
        );

        let mut pull_payload = Vec::new();
        pull_payload.extend_from_slice(&RUNTIME_SYNC_PULL_REQUEST_MAGIC);
        pull_payload.push(2);
        pull_payload.extend_from_slice(&chain_id.to_le_bytes());
        pull_payload.extend_from_slice(&490u64.to_le_bytes());
        pull_payload.extend_from_slice(&520u64.to_le_bytes());
        let pull_request = ProtocolMessage::DistributedOcccGossip(DistributedGossipMessage {
            from: requester.0 as u32,
            to: responder.0 as u32,
            msg_type: DistributedMessageType::StateSync,
            payload: pull_payload,
            timestamp: 0,
            seq: 1,
        });
        tx.send(responder, pull_request).unwrap();

        let started = std::time::Instant::now();
        while started.elapsed() < Duration::from_millis(500) {
            if rx.try_recv(responder).unwrap().is_some() {
                break;
            }
            thread::sleep(Duration::from_millis(5));
        }

        let started_reply = std::time::Instant::now();
        let mut response_heights = Vec::<u64>::new();
        while started_reply.elapsed() < Duration::from_millis(500) {
            if let Some(msg) = tx.try_recv(requester).unwrap() {
                let ProtocolMessage::DistributedOcccGossip(reply) = msg else {
                    continue;
                };
                if !matches!(reply.msg_type, DistributedMessageType::StateSync) {
                    continue;
                }
                if let Ok(header) = decode_block_header_wire_v1(&reply.payload) {
                    response_heights.push(header.height);
                }
            } else if !response_heights.is_empty() {
                break;
            }
            thread::sleep(Duration::from_millis(5));
        }

        assert!(
            !response_heights.is_empty(),
            "expected at least one state-sync response"
        );
        assert_eq!(response_heights.first().copied(), Some(490));
        assert_eq!(response_heights.last().copied(), Some(500));
        for pair in response_heights.windows(2) {
            assert_eq!(pair[1], pair[0].saturating_add(1));
        }

        let status = get_network_runtime_sync_status(chain_id).expect("runtime status");
        assert!(status.highest_block >= 520);
    }

    #[test]
    fn udp_state_sync_pull_request_without_local_range_updates_peer_hint_only() {
        let chain_id = 9_891_u64;
        let requester = NodeId(930);
        let responder = NodeId(931);

        let tx = UdpTransport::bind_for_chain(requester, "127.0.0.1:0", chain_id).unwrap();
        let rx = UdpTransport::bind_for_chain(responder, "127.0.0.1:0", chain_id).unwrap();
        let tx_addr = tx.local_addr().unwrap();
        let rx_addr = rx.local_addr().unwrap();
        tx.register_peer(responder, &rx_addr.to_string()).unwrap();
        rx.register_peer(requester, &tx_addr.to_string()).unwrap();

        set_network_runtime_sync_status(
            chain_id,
            NetworkRuntimeSyncStatus {
                peer_count: 2,
                starting_block: 700,
                current_block: 700,
                highest_block: 700,
            },
        );

        let mut pull_payload = Vec::new();
        pull_payload.extend_from_slice(&RUNTIME_SYNC_PULL_REQUEST_MAGIC);
        pull_payload.push(2);
        pull_payload.extend_from_slice(&chain_id.to_le_bytes());
        pull_payload.extend_from_slice(&701u64.to_le_bytes());
        pull_payload.extend_from_slice(&740u64.to_le_bytes());
        let pull_request = ProtocolMessage::DistributedOcccGossip(DistributedGossipMessage {
            from: requester.0 as u32,
            to: responder.0 as u32,
            msg_type: DistributedMessageType::StateSync,
            payload: pull_payload,
            timestamp: 0,
            seq: 1,
        });
        tx.send(responder, pull_request).unwrap();

        let started = std::time::Instant::now();
        while started.elapsed() < Duration::from_millis(500) {
            if rx.try_recv(responder).unwrap().is_some() {
                break;
            }
            thread::sleep(Duration::from_millis(5));
        }

        let started_reply = std::time::Instant::now();
        let mut got_reply = false;
        while started_reply.elapsed() < Duration::from_millis(200) {
            if tx.try_recv(requester).unwrap().is_some() {
                got_reply = true;
                break;
            }
            thread::sleep(Duration::from_millis(5));
        }
        assert!(
            !got_reply,
            "should not reply when local head < requested from"
        );

        let status = get_network_runtime_sync_status(chain_id).expect("runtime status");
        assert!(status.highest_block >= 740);
    }

    #[test]
    fn udp_send_updates_runtime_local_progress_from_state_sync_block_header_wire() {
        let chain_id = 9_881_u64;
        let n0 = NodeId(300);
        let n1 = NodeId(301);

        let t0 = UdpTransport::bind_for_chain(n0, "127.0.0.1:0", chain_id).unwrap();
        let t1 = UdpTransport::bind_for_chain(n1, "127.0.0.1:0", chain_id).unwrap();
        let a1 = t1.local_addr().unwrap();
        t0.register_peer(n1, &a1.to_string()).unwrap();

        let header = BlockHeaderWireV1 {
            height: 321,
            epoch_id: 7,
            parent_hash: [1u8; 32],
            state_root: [2u8; 32],
            governance_chain_audit_root: [3u8; 32],
            tx_count: 5,
            batch_count: 2,
            consensus_binding: ConsensusPluginBindingV1 {
                plugin_class_code: CONSENSUS_PLUGIN_CLASS_CODE,
                adapter_hash: [4u8; 32],
            },
        };
        let state_sync = ProtocolMessage::DistributedOcccGossip(DistributedGossipMessage {
            from: n0.0 as u32,
            to: n1.0 as u32,
            msg_type: DistributedMessageType::StateSync,
            payload: encode_block_header_wire_v1(&header),
            timestamp: 0,
            seq: 1,
        });
        t0.send(n1, state_sync).unwrap();

        let status =
            get_network_runtime_sync_status(chain_id).expect("runtime sync status should exist");
        assert_eq!(status.current_block, 321);
        assert_eq!(status.highest_block, 321);
    }

    #[test]
    fn tcp_send_updates_runtime_local_progress_from_state_sync_block_header_wire() {
        let chain_id = 9_882_u64;
        let n0 = NodeId(302);
        let n1 = NodeId(303);

        let t0 = TcpTransport::bind_for_chain(n0, "127.0.0.1:0", chain_id).unwrap();
        let t1 = TcpTransport::bind_for_chain(n1, "127.0.0.1:0", chain_id).unwrap();
        let a1 = t1.listener.local_addr().unwrap();
        t0.register_peer(n1, &a1.to_string()).unwrap();

        let header = BlockHeaderWireV1 {
            height: 654,
            epoch_id: 9,
            parent_hash: [1u8; 32],
            state_root: [2u8; 32],
            governance_chain_audit_root: [3u8; 32],
            tx_count: 7,
            batch_count: 3,
            consensus_binding: ConsensusPluginBindingV1 {
                plugin_class_code: CONSENSUS_PLUGIN_CLASS_CODE,
                adapter_hash: [4u8; 32],
            },
        };
        let state_sync = ProtocolMessage::DistributedOcccGossip(DistributedGossipMessage {
            from: n0.0 as u32,
            to: n1.0 as u32,
            msg_type: DistributedMessageType::StateSync,
            payload: encode_block_header_wire_v1(&header),
            timestamp: 0,
            seq: 1,
        });
        t0.send(n1, state_sync).unwrap();

        let status =
            get_network_runtime_sync_status(chain_id).expect("runtime sync status should exist");
        assert_eq!(status.current_block, 654);
        assert_eq!(status.highest_block, 654);
    }

    #[test]
    fn tcp_try_recv_updates_runtime_progress_from_checkpoint_propose_with_same_ip_hint() {
        let chain_id = 9_878_u64;
        let n0 = NodeId(304);
        let n1 = NodeId(305);

        let t0 = TcpTransport::bind_for_chain(n0, "127.0.0.1:0", chain_id).unwrap();
        let t1 = TcpTransport::bind_for_chain(n1, "127.0.0.1:0", chain_id).unwrap();
        let a0 = t0.listener.local_addr().unwrap();
        let a1 = t1.listener.local_addr().unwrap();
        t0.register_peer(n1, &a1.to_string()).unwrap();
        t1.register_peer(n0, &a0.to_string()).unwrap();

        let msg = ProtocolMessage::Finality(FinalityMessage::CheckpointPropose {
            id: CheckpointId(777),
            from: n0,
            payload: vec![0x01, 0x02, 0x03],
        });
        t0.send(n1, msg).unwrap();

        let started = std::time::Instant::now();
        let mut got = None;
        while started.elapsed() < Duration::from_millis(500) {
            if let Some(m) = t1.try_recv(n1).unwrap() {
                got = Some(m);
                break;
            }
            thread::sleep(Duration::from_millis(5));
        }
        assert!(matches!(
            got,
            Some(ProtocolMessage::Finality(
                FinalityMessage::CheckpointPropose { .. }
            ))
        ));

        let status =
            get_network_runtime_sync_status(chain_id).expect("runtime sync status should exist");
        assert_eq!(status.highest_block, 777);
        assert!(status.peer_count >= 1);
    }

    #[test]
    fn infer_peer_id_from_src_addr_prefers_exact_and_unique_same_ip() {
        let peers = DashMap::new();
        peers.insert(NodeId(1), "127.0.0.1:12001".parse().expect("addr node1"));

        let exact =
            infer_peer_id_from_src_addr(&peers, "127.0.0.1:12001".parse().expect("src exact"));
        assert_eq!(exact, Some(1));

        let unique_same_ip =
            infer_peer_id_from_src_addr(&peers, "127.0.0.1:55000".parse().expect("src same ip"));
        assert_eq!(unique_same_ip, Some(1));

        peers.insert(NodeId(2), "127.0.0.1:12002".parse().expect("addr node2"));
        let ambiguous_same_ip =
            infer_peer_id_from_src_addr(&peers, "127.0.0.1:56000".parse().expect("src ambiguous"));
        assert_eq!(ambiguous_same_ip, None);
    }

    #[test]
    fn udp_try_recv_updates_runtime_progress_from_finality_vote() {
        let chain_id = 9_999_u64;
        let n0 = NodeId(240);
        let n1 = NodeId(241);

        let t0 = UdpTransport::bind_for_chain(n0, "127.0.0.1:0", chain_id).unwrap();
        let t1 = UdpTransport::bind_for_chain(n1, "127.0.0.1:0", chain_id).unwrap();
        let a0 = t0.local_addr().unwrap();
        let a1 = t1.local_addr().unwrap();
        t0.register_peer(n1, &a1.to_string()).unwrap();
        t1.register_peer(n0, &a0.to_string()).unwrap();

        let msg = ProtocolMessage::Finality(FinalityMessage::Vote {
            id: CheckpointId(55),
            from: n0,
            sig: vec![1u8; 64],
        });
        t0.send(n1, msg).unwrap();

        let started = std::time::Instant::now();
        let mut got = None;
        while started.elapsed() < Duration::from_millis(500) {
            if let Some(m) = t1.try_recv(n1).unwrap() {
                got = Some(m);
                break;
            }
            thread::sleep(Duration::from_millis(5));
        }
        assert!(matches!(
            got,
            Some(ProtocolMessage::Finality(FinalityMessage::Vote { .. }))
        ));
        let status =
            get_network_runtime_sync_status(chain_id).expect("runtime sync status should exist");
        assert_eq!(status.current_block, 55);
        assert_eq!(status.highest_block, 55);
        assert_eq!(status.starting_block, 55);
    }

    #[test]
    fn udp_try_recv_registers_runtime_peers_from_peerlist_payload() {
        let chain_id = 5_555u64;
        let n0 = NodeId(10);
        let n1 = NodeId(11);
        let t0 = UdpTransport::bind_for_chain(n0, "127.0.0.1:0", chain_id).unwrap();
        let t1 = UdpTransport::bind_for_chain(n1, "127.0.0.1:0", chain_id).unwrap();
        let a0 = t0.local_addr().unwrap();
        let a1 = t1.local_addr().unwrap();
        t0.register_peer(n1, &a1.to_string()).unwrap();
        t1.register_peer(n0, &a0.to_string()).unwrap();

        let msg = ProtocolMessage::Gossip(ProtocolGossipMessage::PeerList {
            from: n0,
            peers: vec![NodeId(12), NodeId(13)],
        });
        t0.send(n1, msg).unwrap();
        let started = std::time::Instant::now();
        while started.elapsed() < Duration::from_millis(500) {
            if t1.try_recv(n1).unwrap().is_some() {
                break;
            }
            thread::sleep(Duration::from_millis(5));
        }

        let status = get_network_runtime_sync_status(chain_id).expect("runtime sync status");
        assert!(
            status.peer_count >= 3,
            "peer_count should include sender + peerlist payload peers"
        );
    }

    #[test]
    fn udp_transport_mesh_three_nodes_closure() {
        let n0 = NodeId(0);
        let n1 = NodeId(1);
        let n2 = NodeId(2);
        let t0 = UdpTransport::bind(n0, "127.0.0.1:0").unwrap();
        let t1 = UdpTransport::bind(n1, "127.0.0.1:0").unwrap();
        let t2 = UdpTransport::bind(n2, "127.0.0.1:0").unwrap();

        let a0 = t0.local_addr().unwrap();
        let a1 = t1.local_addr().unwrap();
        let a2 = t2.local_addr().unwrap();

        t0.register_peer(n1, &a1.to_string()).unwrap();
        t0.register_peer(n2, &a2.to_string()).unwrap();
        t1.register_peer(n0, &a0.to_string()).unwrap();
        t1.register_peer(n2, &a2.to_string()).unwrap();
        t2.register_peer(n0, &a0.to_string()).unwrap();
        t2.register_peer(n1, &a1.to_string()).unwrap();

        let send_triplet =
            |from: NodeId, to: NodeId, transport: &UdpTransport, peers: Vec<NodeId>| {
                transport
                    .send(
                        to,
                        ProtocolMessage::Gossip(GossipMessage::PeerList { from, peers }),
                    )
                    .unwrap();
                transport
                    .send(
                        to,
                        ProtocolMessage::Gossip(GossipMessage::Heartbeat {
                            from,
                            shard: ShardId((from.0 as u32).saturating_add(1)),
                        }),
                    )
                    .unwrap();
                transport
                    .send(
                        to,
                        ProtocolMessage::DistributedOcccGossip(DistributedGossipMessage {
                            from: from.0 as u32,
                            to: to.0 as u32,
                            msg_type: DistributedMessageType::StateSync,
                            payload: vec![from.0 as u8, to.0 as u8],
                            timestamp: 0,
                            seq: from.0,
                        }),
                    )
                    .unwrap();
            };

        send_triplet(n0, n1, &t0, vec![n1, n2]);
        send_triplet(n0, n2, &t0, vec![n1, n2]);
        send_triplet(n1, n0, &t1, vec![n0, n2]);
        send_triplet(n1, n2, &t1, vec![n0, n2]);
        send_triplet(n2, n0, &t2, vec![n0, n1]);
        send_triplet(n2, n1, &t2, vec![n0, n1]);

        let mut d0: HashSet<u64> = HashSet::new();
        let mut g0: HashSet<u64> = HashSet::new();
        let mut s0: HashSet<u64> = HashSet::new();
        let mut d1: HashSet<u64> = HashSet::new();
        let mut g1: HashSet<u64> = HashSet::new();
        let mut s1: HashSet<u64> = HashSet::new();
        let mut d2: HashSet<u64> = HashSet::new();
        let mut g2: HashSet<u64> = HashSet::new();
        let mut s2: HashSet<u64> = HashSet::new();

        let started = std::time::Instant::now();
        while started.elapsed() < Duration::from_millis(1_500) {
            while let Some(msg) = t0.try_recv(n0).unwrap() {
                match msg {
                    ProtocolMessage::Gossip(GossipMessage::PeerList { from, .. }) => {
                        d0.insert(from.0);
                    }
                    ProtocolMessage::Gossip(GossipMessage::Heartbeat { from, .. }) => {
                        g0.insert(from.0);
                    }
                    ProtocolMessage::DistributedOcccGossip(DistributedGossipMessage {
                        from,
                        msg_type: DistributedMessageType::StateSync,
                        ..
                    }) => {
                        s0.insert(from as u64);
                    }
                    _ => {}
                }
            }
            while let Some(msg) = t1.try_recv(n1).unwrap() {
                match msg {
                    ProtocolMessage::Gossip(GossipMessage::PeerList { from, .. }) => {
                        d1.insert(from.0);
                    }
                    ProtocolMessage::Gossip(GossipMessage::Heartbeat { from, .. }) => {
                        g1.insert(from.0);
                    }
                    ProtocolMessage::DistributedOcccGossip(DistributedGossipMessage {
                        from,
                        msg_type: DistributedMessageType::StateSync,
                        ..
                    }) => {
                        s1.insert(from as u64);
                    }
                    _ => {}
                }
            }
            while let Some(msg) = t2.try_recv(n2).unwrap() {
                match msg {
                    ProtocolMessage::Gossip(GossipMessage::PeerList { from, .. }) => {
                        d2.insert(from.0);
                    }
                    ProtocolMessage::Gossip(GossipMessage::Heartbeat { from, .. }) => {
                        g2.insert(from.0);
                    }
                    ProtocolMessage::DistributedOcccGossip(DistributedGossipMessage {
                        from,
                        msg_type: DistributedMessageType::StateSync,
                        ..
                    }) => {
                        s2.insert(from as u64);
                    }
                    _ => {}
                }
            }

            let ok0 = d0.len() == 2 && g0.len() == 2 && s0.len() == 2;
            let ok1 = d1.len() == 2 && g1.len() == 2 && s1.len() == 2;
            let ok2 = d2.len() == 2 && g2.len() == 2 && s2.len() == 2;
            if ok0 && ok1 && ok2 {
                break;
            }
            thread::sleep(Duration::from_millis(5));
        }

        assert_eq!(d0.len(), 2, "node0 discovery set: {d0:?}");
        assert_eq!(g0.len(), 2, "node0 gossip set: {g0:?}");
        assert_eq!(s0.len(), 2, "node0 sync set: {s0:?}");
        assert_eq!(d1.len(), 2, "node1 discovery set: {d1:?}");
        assert_eq!(g1.len(), 2, "node1 gossip set: {g1:?}");
        assert_eq!(s1.len(), 2, "node1 sync set: {s1:?}");
        assert_eq!(d2.len(), 2, "node2 discovery set: {d2:?}");
        assert_eq!(g2.len(), 2, "node2 gossip set: {g2:?}");
        assert_eq!(s2.len(), 2, "node2 sync set: {s2:?}");
    }
}
