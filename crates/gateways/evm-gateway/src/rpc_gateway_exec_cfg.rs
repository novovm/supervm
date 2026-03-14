use super::*;
use dashmap::DashMap;
use novovm_network::{
    get_network_runtime_peer_heads_top_k, get_network_runtime_sync_status,
    plan_network_runtime_sync_pull_window, TcpTransport, Transport, UdpTransport,
};
#[cfg(test)]
use novovm_network::{set_network_runtime_sync_status, NetworkRuntimeSyncStatus};
use novovm_protocol::{
    protocol_catalog::distributed_occc::gossip::{
        GossipMessage as DistributedOcccGossipMessage, MessageType as DistributedOcccMessageType,
    },
    GossipMessage as ProtocolGossipMessage, NodeId, ProtocolMessage, ShardId,
};
use std::sync::Arc;

#[derive(Clone)]
enum GatewayEthNativeBroadcaster {
    Udp {
        node: NodeId,
        socket: UdpTransport,
        registered_peers: Arc<Mutex<HashMap<u64, String>>>,
    },
    Tcp {
        node: NodeId,
        socket: TcpTransport,
        registered_peers: Arc<Mutex<HashMap<u64, String>>>,
    },
}

impl GatewayEthNativeBroadcaster {
    fn registered_peers(&self) -> &Mutex<HashMap<u64, String>> {
        match self {
            Self::Udp {
                registered_peers, ..
            }
            | Self::Tcp {
                registered_peers, ..
            } => registered_peers,
        }
    }

    fn needs_peer_registration(&self, peers: &[(NodeId, String)]) -> bool {
        let Ok(guard) = self.registered_peers().lock() else {
            return true;
        };
        for (peer, addr) in peers {
            if guard.get(&peer.0).is_none_or(|cached| cached != addr) {
                return true;
            }
        }
        false
    }

    fn register_peer(&self, node: NodeId, addr: &str) -> Result<(), anyhow::Error> {
        match self {
            Self::Udp {
                socket,
                registered_peers,
                ..
            } => register_gateway_eth_native_peer_cached(
                registered_peers,
                node,
                addr,
                |peer, addr| {
                    socket
                        .register_peer(peer, addr)
                        .map_err(|e| anyhow::anyhow!(e.to_string()))
                },
            ),
            Self::Tcp {
                socket,
                registered_peers,
                ..
            } => register_gateway_eth_native_peer_cached(
                registered_peers,
                node,
                addr,
                |peer, addr| {
                    socket
                        .register_peer(peer, addr)
                        .map_err(|e| anyhow::anyhow!(e.to_string()))
                },
            ),
        }
    }

    fn send(&self, peer: NodeId, msg: ProtocolMessage) -> Result<(), anyhow::Error> {
        match self {
            Self::Udp { socket, .. } => socket.send(peer, msg),
            Self::Tcp { socket, .. } => socket.send(peer, msg),
        }
        .map_err(|e| anyhow::anyhow!(e.to_string()))
    }

    fn drain_incoming(&self, max_frames: usize) {
        let cap = max_frames.max(1);
        match self {
            Self::Udp { node, socket, .. } => {
                for _ in 0..cap {
                    match socket.try_recv(*node) {
                        Ok(Some(_)) => {}
                        Ok(None) | Err(_) => break,
                    }
                }
            }
            Self::Tcp { node, socket, .. } => {
                for _ in 0..cap {
                    match socket.try_recv(*node) {
                        Ok(Some(_)) => {}
                        Ok(None) | Err(_) => break,
                    }
                }
            }
        }
    }
}

fn register_gateway_eth_native_peer_cached<F>(
    registered_peers: &Mutex<HashMap<u64, String>>,
    node: NodeId,
    addr: &str,
    register_peer: F,
) -> Result<(), anyhow::Error>
where
    F: FnOnce(NodeId, &str) -> Result<(), anyhow::Error>,
{
    if let Ok(guard) = registered_peers.lock() {
        if guard.get(&node.0).is_some_and(|cached| cached == addr) {
            return Ok(());
        }
    }
    register_peer(node, addr)?;
    if let Ok(mut guard) = registered_peers.lock() {
        guard.insert(node.0, addr.to_string());
        if guard.len() > 8_192 {
            guard.clear();
        }
    }
    Ok(())
}

static GATEWAY_ETH_NATIVE_BROADCASTER_CACHE: OnceLock<
    Mutex<HashMap<String, GatewayEthNativeBroadcaster>>,
> = OnceLock::new();
static GATEWAY_ETH_NATIVE_PEERS_CACHE: OnceLock<DashMap<u64, GatewayEthNativePeersCache>> =
    OnceLock::new();
static GATEWAY_ETH_NATIVE_RUNTIME_WORKER_STARTED: OnceLock<
    Mutex<std::collections::HashSet<String>>,
> = OnceLock::new();
static GATEWAY_ETH_NATIVE_SYNC_PULL_TRACKER: OnceLock<
    DashMap<String, GatewayEthNativeSyncPullState>,
> = OnceLock::new();
const GATEWAY_ETH_NATIVE_DISCOVERY_INTERVAL_MS: u128 = 3_000;
const GATEWAY_ETH_NATIVE_RECV_DRAIN_MAX_PER_BROADCAST: usize = 32;
const GATEWAY_ETH_NATIVE_RECV_DRAIN_MAX_SYNC_TICK: usize = 128;
const GATEWAY_ETH_NATIVE_RUNTIME_WORKER_TICK_MS: u64 = 1_000;
const GATEWAY_ETH_NATIVE_RUNTIME_WORKER_SYNC_TICK_MS: u64 = 250;
const GATEWAY_ETH_NATIVE_SYNC_PULL_RESEND_HEADERS_MS: u128 = 1_200;
const GATEWAY_ETH_NATIVE_SYNC_PULL_RESEND_BODIES_MS: u128 = 1_500;
const GATEWAY_ETH_NATIVE_SYNC_PULL_RESEND_STATE_MS: u128 = 2_000;
const GATEWAY_ETH_NATIVE_SYNC_PULL_RESEND_FINALIZE_MS: u128 = 2_500;
const GATEWAY_ETH_NATIVE_SYNC_PULL_RESEND_DEFAULT_MS: u128 = 3_000;
const GATEWAY_ETH_NATIVE_SYNC_PULL_RESEND_MS_MIN: u64 = 50;
const GATEWAY_ETH_NATIVE_SYNC_PULL_RESEND_MS_MAX: u64 = 60_000;
const GATEWAY_ETH_NATIVE_SYNC_PULL_PAYLOAD_MAGIC: [u8; 4] = *b"NSP1";
const GATEWAY_ETH_NATIVE_SYNC_PULL_FANOUT_CAP_HARD_MAX: u64 = 1_024;
const GATEWAY_ETH_NATIVE_SYNC_PULL_SEGMENTS_CAP_HARD_MAX: u64 = 1_024;
const GATEWAY_ETH_NATIVE_SYNC_PULL_SEGMENT_MIN_BLOCKS_DEFAULT: u64 = 1;
const GATEWAY_ETH_NATIVE_SYNC_PULL_SEGMENT_MIN_BLOCKS_HARD_MAX: u64 = 1_000_000;

#[derive(Clone)]
struct GatewayEthNativeSyncPullState {
    phase_tag: u8,
    from_block: u64,
    to_block: u64,
    last_sent_ms: u128,
    resend_round: u32,
}

#[derive(Clone)]
struct GatewayEthNativePeersCache {
    raw: String,
    peers: GatewayEthNativePeers,
    peer_nodes: GatewayEthNativePeerNodes,
}

type GatewayEthNativePeers = Arc<Vec<(NodeId, String)>>;
type GatewayEthNativePeerNodes = Arc<Vec<NodeId>>;

fn gateway_eth_native_broadcaster_cache(
) -> &'static Mutex<HashMap<String, GatewayEthNativeBroadcaster>> {
    GATEWAY_ETH_NATIVE_BROADCASTER_CACHE.get_or_init(|| Mutex::new(HashMap::new()))
}

fn gateway_eth_native_peers_cache() -> &'static DashMap<u64, GatewayEthNativePeersCache> {
    GATEWAY_ETH_NATIVE_PEERS_CACHE.get_or_init(DashMap::new)
}

fn gateway_eth_native_runtime_worker_started() -> &'static Mutex<std::collections::HashSet<String>>
{
    GATEWAY_ETH_NATIVE_RUNTIME_WORKER_STARTED
        .get_or_init(|| Mutex::new(std::collections::HashSet::new()))
}

fn gateway_eth_native_sync_pull_tracker() -> &'static DashMap<String, GatewayEthNativeSyncPullState>
{
    GATEWAY_ETH_NATIVE_SYNC_PULL_TRACKER.get_or_init(DashMap::new)
}

fn next_gateway_eth_native_sync_pull_fanout(
    worker_key: &str,
    phase_tag: u8,
    from_block: u64,
    to_block: u64,
    now_ms: u128,
    resend_interval_ms: u128,
) -> Option<usize> {
    let tracker = gateway_eth_native_sync_pull_tracker();
    let mut resend_round = 0u32;
    let should_send = if let Some(last) = tracker.get_mut(worker_key) {
        let window_changed = last.phase_tag != phase_tag
            || last.from_block != from_block
            || last.to_block != to_block;
        if window_changed {
            resend_round = 0;
            true
        } else if now_ms.saturating_sub(last.last_sent_ms) >= resend_interval_ms {
            resend_round = last.resend_round.saturating_add(1);
            true
        } else {
            resend_round = last.resend_round;
            false
        }
    } else {
        true
    };
    if !should_send {
        return None;
    }
    if let Some(mut last) = tracker.get_mut(worker_key) {
        last.phase_tag = phase_tag;
        last.from_block = from_block;
        last.to_block = to_block;
        last.last_sent_ms = now_ms;
        last.resend_round = resend_round;
    } else {
        if tracker.len() > 256 {
            tracker.clear();
        }
        tracker.insert(
            worker_key.to_string(),
            GatewayEthNativeSyncPullState {
                phase_tag,
                from_block,
                to_block,
                last_sent_ms: now_ms,
                resend_round,
            },
        );
    }
    let fanout = if resend_round == 0 {
        1
    } else if resend_round == 1 {
        2
    } else {
        usize::MAX
    };
    Some(fanout)
}

fn gateway_eth_native_sync_pull_resend_ms_by_tag(phase_tag: u8) -> u128 {
    match phase_tag {
        1 => GATEWAY_ETH_NATIVE_SYNC_PULL_RESEND_HEADERS_MS,
        2 => GATEWAY_ETH_NATIVE_SYNC_PULL_RESEND_BODIES_MS,
        3 => GATEWAY_ETH_NATIVE_SYNC_PULL_RESEND_STATE_MS,
        4 => GATEWAY_ETH_NATIVE_SYNC_PULL_RESEND_FINALIZE_MS,
        _ => GATEWAY_ETH_NATIVE_SYNC_PULL_RESEND_DEFAULT_MS,
    }
}

fn gateway_eth_native_sync_pull_resend_ms(chain_id: u64, phase_tag: u8) -> u128 {
    let default_ms = gateway_eth_native_sync_pull_resend_ms_by_tag(phase_tag) as u64;
    let configured_ms = gateway_eth_native_sync_pull_chain_phase_u64_env(
        chain_id,
        phase_tag,
        "NOVOVM_GATEWAY_ETH_NATIVE_SYNC_PULL_RESEND_MS",
        default_ms,
    );
    configured_ms.clamp(
        GATEWAY_ETH_NATIVE_SYNC_PULL_RESEND_MS_MIN,
        GATEWAY_ETH_NATIVE_SYNC_PULL_RESEND_MS_MAX,
    ) as u128
}

fn gateway_eth_native_sync_pull_phase_env_suffix(phase_tag: u8) -> Option<&'static str> {
    match phase_tag {
        1 => Some("HEADERS"),
        2 => Some("BODIES"),
        3 => Some("STATE"),
        4 => Some("FINALIZE"),
        5 => Some("DISCOVERY"),
        _ => None,
    }
}

fn gateway_eth_native_sync_pull_chain_u64_env_opt(chain_id: u64, base_key: &str) -> Option<u64> {
    let chain_key_dec = format!("{base_key}_CHAIN_{chain_id}");
    let chain_key_hex_lower = format!("{base_key}_CHAIN_0x{:x}", chain_id);
    let chain_key_hex_upper = format!("{base_key}_CHAIN_0x{:X}", chain_id);
    string_env_nonempty(&chain_key_dec)
        .or_else(|| string_env_nonempty(&chain_key_hex_lower))
        .or_else(|| string_env_nonempty(&chain_key_hex_upper))
        .or_else(|| string_env_nonempty(base_key))
        .and_then(|raw| raw.trim().parse::<u64>().ok())
}

fn gateway_eth_native_sync_pull_chain_u64_env(chain_id: u64, base_key: &str, default: u64) -> u64 {
    gateway_eth_native_sync_pull_chain_u64_env_opt(chain_id, base_key).unwrap_or(default)
}

fn gateway_eth_native_sync_pull_chain_phase_u64_env(
    chain_id: u64,
    phase_tag: u8,
    base_key: &str,
    default: u64,
) -> u64 {
    if let Some(phase_suffix) = gateway_eth_native_sync_pull_phase_env_suffix(phase_tag) {
        let phase_key = format!("{base_key}_{phase_suffix}");
        if let Some(value) = gateway_eth_native_sync_pull_chain_u64_env_opt(chain_id, &phase_key) {
            return value;
        }
    }
    gateway_eth_native_sync_pull_chain_u64_env(chain_id, base_key, default)
}

fn gateway_eth_native_sync_pull_fanout_cap(chain_id: u64, phase_tag: u8) -> Option<usize> {
    let raw = gateway_eth_native_sync_pull_chain_phase_u64_env(
        chain_id,
        phase_tag,
        "NOVOVM_GATEWAY_ETH_NATIVE_SYNC_PULL_FANOUT_MAX",
        0,
    );
    if raw == 0 {
        None
    } else {
        Some(raw.min(GATEWAY_ETH_NATIVE_SYNC_PULL_FANOUT_CAP_HARD_MAX) as usize)
    }
}

fn gateway_eth_native_sync_pull_segments_cap(chain_id: u64, phase_tag: u8) -> Option<usize> {
    let raw = gateway_eth_native_sync_pull_chain_phase_u64_env(
        chain_id,
        phase_tag,
        "NOVOVM_GATEWAY_ETH_NATIVE_SYNC_PULL_SEGMENTS_MAX",
        0,
    );
    if raw == 0 {
        None
    } else {
        Some(raw.min(GATEWAY_ETH_NATIVE_SYNC_PULL_SEGMENTS_CAP_HARD_MAX) as usize)
    }
}

fn gateway_eth_native_sync_pull_segment_min_blocks(chain_id: u64, phase_tag: u8) -> u64 {
    gateway_eth_native_sync_pull_chain_phase_u64_env(
        chain_id,
        phase_tag,
        "NOVOVM_GATEWAY_ETH_NATIVE_SYNC_PULL_SEGMENT_MIN_BLOCKS",
        GATEWAY_ETH_NATIVE_SYNC_PULL_SEGMENT_MIN_BLOCKS_DEFAULT,
    )
    .clamp(1, GATEWAY_ETH_NATIVE_SYNC_PULL_SEGMENT_MIN_BLOCKS_HARD_MAX)
}

fn resolve_gateway_eth_native_sync_pull_fanout(
    chain_id: u64,
    phase_tag: u8,
    requested_fanout: usize,
    peer_count: usize,
) -> usize {
    if peer_count == 0 {
        return 0;
    }
    let base = if requested_fanout == usize::MAX {
        peer_count
    } else {
        requested_fanout.min(peer_count)
    };
    let capped = if let Some(cap) = gateway_eth_native_sync_pull_fanout_cap(chain_id, phase_tag) {
        base.min(cap)
    } else {
        base
    };
    capped.max(1)
}

fn clear_gateway_eth_native_sync_pull_tracker(worker_key: &str) {
    gateway_eth_native_sync_pull_tracker().remove(worker_key);
}

fn should_send_gateway_eth_native_discovery(now_ms: u128, last_sent_ms: u128) -> bool {
    last_sent_ms == 0
        || now_ms.saturating_sub(last_sent_ms) >= GATEWAY_ETH_NATIVE_DISCOVERY_INTERVAL_MS
}

fn gateway_eth_native_should_emit_discovery(chain_id: u64) -> bool {
    match get_network_runtime_sync_status(chain_id) {
        None => true,
        Some(status) => {
            if status.peer_count == 0 {
                return true;
            }
            // During active syncing, keep light discovery when peer set is thin.
            status.highest_block > status.current_block && status.peer_count <= 1
        }
    }
}

#[cfg(test)]
fn merge_gateway_eth_sync_pull_candidates(
    preferred: Vec<NodeId>,
    full_ordered: Vec<NodeId>,
) -> Vec<NodeId> {
    let mut out = preferred;
    let mut seen = std::collections::HashSet::<u64>::new();
    for node in &out {
        seen.insert(node.0);
    }
    for node in full_ordered {
        if seen.insert(node.0) {
            out.push(node);
        }
    }
    out
}

fn select_gateway_eth_sync_pull_peers_from_snapshot(
    peer_nodes: &[NodeId],
    runtime_heads: &[(u64, u64)],
    fanout: usize,
) -> Vec<NodeId> {
    if peer_nodes.is_empty() || fanout == 0 {
        return Vec::new();
    }
    let finite_fanout = if fanout == usize::MAX {
        peer_nodes.len()
    } else {
        fanout.min(peer_nodes.len())
    };
    if finite_fanout == 0 {
        return Vec::new();
    }
    if runtime_heads.is_empty() {
        return peer_nodes.iter().take(finite_fanout).copied().collect();
    }
    let mut configured = std::collections::HashSet::<u64>::with_capacity(peer_nodes.len());
    for node in peer_nodes {
        configured.insert(node.0);
    }
    let mut ordered = Vec::<NodeId>::new();
    let mut seen = std::collections::HashSet::<u64>::with_capacity(peer_nodes.len());
    let configured_len = configured.len();
    for (peer_id, _head) in runtime_heads {
        if seen.len() >= configured_len {
            break;
        }
        if configured.contains(peer_id) && seen.insert(*peer_id) {
            ordered.push(NodeId(*peer_id));
        }
    }
    for node in peer_nodes {
        if seen.insert(node.0) {
            ordered.push(*node);
        }
    }
    ordered.into_iter().take(finite_fanout).collect()
}

fn select_gateway_eth_native_sync_pull_peers(
    chain_id: u64,
    peer_nodes: &[NodeId],
    fanout: usize,
) -> Vec<NodeId> {
    if peer_nodes.is_empty() || fanout == 0 {
        return Vec::new();
    }
    let head_budget = if fanout == usize::MAX {
        peer_nodes.len()
    } else {
        fanout.min(peer_nodes.len())
    };
    let runtime_heads = get_network_runtime_peer_heads_top_k(chain_id, head_budget);
    select_gateway_eth_sync_pull_peers_from_snapshot(peer_nodes, &runtime_heads, fanout)
}

#[allow(clippy::too_many_arguments)]
fn send_gateway_eth_native_sync_pull_requests(
    broadcaster: &GatewayEthNativeBroadcaster,
    local_node: NodeId,
    ordered_peers: &[NodeId],
    chain_id: u64,
    phase_tag: u8,
    from_block: u64,
    to_block: u64,
    fanout: usize,
    now: u64,
) -> usize {
    if ordered_peers.is_empty() || fanout == 0 || to_block < from_block {
        return 0;
    }
    let from_u32 = match u32::try_from(local_node.0) {
        Ok(v) => v,
        Err(_) => return 0,
    };
    let requested_target_success = if fanout == usize::MAX {
        ordered_peers.len()
    } else {
        fanout.min(ordered_peers.len()).max(1)
    };
    let window_span = to_block.saturating_sub(from_block).saturating_add(1) as usize;
    let mut target_success = requested_target_success.min(window_span.max(1));
    if let Some(segments_cap) = gateway_eth_native_sync_pull_segments_cap(chain_id, phase_tag) {
        target_success = target_success.min(segments_cap.max(1));
    }
    let min_segment_blocks = gateway_eth_native_sync_pull_segment_min_blocks(chain_id, phase_tag);
    if min_segment_blocks > 1 {
        let by_min_blocks = (window_span as u64)
            .saturating_add(min_segment_blocks.saturating_sub(1))
            .saturating_div(min_segment_blocks)
            .max(1) as usize;
        target_success = target_success.min(by_min_blocks);
    }
    target_success = target_success.max(1);
    let msg_type = gateway_eth_native_sync_pull_msg_type_by_tag(phase_tag);
    if target_success == 1 {
        if let Some(peer) = ordered_peers
            .iter()
            .copied()
            .find(|peer| u32::try_from(peer.0).is_ok())
        {
            if let Ok(to_u32) = u32::try_from(peer.0) {
                let payload = encode_gateway_eth_native_sync_pull_payload(
                    chain_id, phase_tag, from_block, to_block,
                );
                let sync_pull =
                    ProtocolMessage::DistributedOcccGossip(DistributedOcccGossipMessage {
                        from: from_u32,
                        to: to_u32,
                        msg_type,
                        payload,
                        timestamp: now,
                        seq: now,
                    });
                return usize::from(broadcaster.send(peer, sync_pull).is_ok());
            }
        }
        return 0;
    }
    let ranges = split_gateway_eth_sync_pull_window(from_block, to_block, target_success);
    if ranges.is_empty() {
        return 0;
    }
    let mut success_count = 0usize;
    let mut next_range_idx = 0usize;
    for peer in ordered_peers.iter().copied() {
        if next_range_idx >= ranges.len() {
            break;
        }
        if let Ok(to_u32) = u32::try_from(peer.0) {
            let (range_from, range_to) = ranges[next_range_idx];
            let payload = encode_gateway_eth_native_sync_pull_payload(
                chain_id, phase_tag, range_from, range_to,
            );
            let sync_pull = ProtocolMessage::DistributedOcccGossip(DistributedOcccGossipMessage {
                from: from_u32,
                to: to_u32,
                msg_type: msg_type.clone(),
                payload,
                timestamp: now,
                seq: now,
            });
            if broadcaster.send(peer, sync_pull).is_ok() {
                success_count = success_count.saturating_add(1);
                next_range_idx = next_range_idx.saturating_add(1);
            }
        }
    }
    success_count
}

fn split_gateway_eth_sync_pull_window(
    from_block: u64,
    to_block: u64,
    segments: usize,
) -> Vec<(u64, u64)> {
    if segments == 0 || to_block < from_block {
        return Vec::new();
    }
    let total_len = to_block.saturating_sub(from_block).saturating_add(1) as usize;
    let seg_count = segments.min(total_len.max(1));
    let base_len = total_len / seg_count;
    let remainder = total_len % seg_count;
    let mut out = Vec::with_capacity(seg_count);
    let mut cursor = from_block;
    for idx in 0..seg_count {
        let chunk_len = base_len + usize::from(idx < remainder);
        let end = cursor.saturating_add(chunk_len as u64).saturating_sub(1);
        out.push((cursor, end));
        cursor = end.saturating_add(1);
    }
    out
}

pub(super) fn poll_gateway_eth_public_broadcast_native_runtime(chain_id: u64, max_frames: usize) {
    ensure_gateway_eth_public_broadcast_native_runtime(chain_id);

    let prefix = format!("{chain_id}:");
    let broadcasters: Vec<GatewayEthNativeBroadcaster> =
        if let Ok(guard) = gateway_eth_native_broadcaster_cache().lock() {
            guard
                .iter()
                .filter(|(key, _)| key.starts_with(&prefix))
                .map(|(_, broadcaster)| broadcaster.clone())
                .collect()
        } else {
            Vec::new()
        };
    for broadcaster in broadcasters {
        broadcaster.drain_incoming(max_frames);
    }
}

fn build_gateway_eth_native_broadcaster_key(
    chain_id: u64,
    transport: GatewayEthPublicBroadcastNativeTransport,
    local_node: NodeId,
    listen_addr: &str,
) -> String {
    format!(
        "{}:{}:{}:{}",
        chain_id,
        transport.as_mode(),
        local_node.0,
        listen_addr
    )
}

fn get_or_create_gateway_eth_native_broadcaster(
    chain_id: u64,
    transport: GatewayEthPublicBroadcastNativeTransport,
    local_node: NodeId,
    listen_addr: &str,
) -> Result<(String, GatewayEthNativeBroadcaster)> {
    let broadcaster_key =
        build_gateway_eth_native_broadcaster_key(chain_id, transport, local_node, listen_addr);

    if let Ok(guard) = gateway_eth_native_broadcaster_cache().lock() {
        if let Some(existing) = guard.get(&broadcaster_key) {
            return Ok((broadcaster_key, existing.clone()));
        }
    }

    let created = match transport {
        GatewayEthPublicBroadcastNativeTransport::Udp => {
            let socket = UdpTransport::bind_for_chain(local_node, listen_addr, chain_id)
                .with_context(|| {
                    format!(
                        "native runtime failed: chain_id={} reason=udp_bind_failed listen={}",
                        chain_id, listen_addr
                    )
                })?;
            GatewayEthNativeBroadcaster::Udp {
                node: local_node,
                socket,
                registered_peers: Arc::new(Mutex::new(HashMap::new())),
            }
        }
        GatewayEthPublicBroadcastNativeTransport::Tcp => {
            let socket = TcpTransport::bind_for_chain(local_node, listen_addr, chain_id)
                .with_context(|| {
                    format!(
                        "native runtime failed: chain_id={} reason=tcp_bind_failed listen={}",
                        chain_id, listen_addr
                    )
                })?;
            GatewayEthNativeBroadcaster::Tcp {
                node: local_node,
                socket,
                registered_peers: Arc::new(Mutex::new(HashMap::new())),
            }
        }
    };

    if let Ok(mut guard) = gateway_eth_native_broadcaster_cache().lock() {
        if guard.len() > 64 {
            guard.clear();
        }
        guard.insert(broadcaster_key.clone(), created.clone());
    }

    Ok((broadcaster_key, created))
}

fn ensure_gateway_eth_public_broadcast_native_runtime(chain_id: u64) {
    let Some((peers, peer_nodes)) = gateway_eth_public_broadcast_native_peers_snapshot(chain_id)
    else {
        return;
    };
    if peers.is_empty() {
        return;
    }

    let transport = gateway_eth_public_broadcast_native_transport(chain_id);
    let listen_addr = gateway_eth_public_broadcast_native_listen_addr(chain_id, transport);
    let local_node = gateway_eth_public_broadcast_native_node_id(chain_id);
    let Ok((broadcaster_key, broadcaster)) =
        get_or_create_gateway_eth_native_broadcaster(chain_id, transport, local_node, &listen_addr)
    else {
        return;
    };

    if broadcaster.needs_peer_registration(peers.as_ref()) {
        for (peer, addr) in peers.iter() {
            let _ = broadcaster.register_peer(*peer, addr.as_str());
        }
    }
    ensure_gateway_eth_native_runtime_worker(
        chain_id,
        &broadcaster_key,
        broadcaster.clone(),
        local_node,
        peer_nodes,
    );
    broadcaster.drain_incoming(GATEWAY_ETH_NATIVE_RECV_DRAIN_MAX_PER_BROADCAST);
}

fn ensure_gateway_eth_native_runtime_worker(
    chain_id: u64,
    worker_key: &str,
    broadcaster: GatewayEthNativeBroadcaster,
    local_node: NodeId,
    peer_nodes: Arc<Vec<NodeId>>,
) {
    if peer_nodes.is_empty() {
        return;
    }
    let should_spawn = if let Ok(mut guard) = gateway_eth_native_runtime_worker_started().lock() {
        guard.insert(worker_key.to_string())
    } else {
        false
    };
    if !should_spawn {
        return;
    }
    let worker_key_owned = worker_key.to_string();
    thread::spawn(move || {
        thread::sleep(Duration::from_millis(
            GATEWAY_ETH_NATIVE_RUNTIME_WORKER_TICK_MS,
        ));
        let mut last_discovery_sent_ms: u128 = 0;
        let mut recv_drain_max = GATEWAY_ETH_NATIVE_RECV_DRAIN_MAX_PER_BROADCAST;
        loop {
            // Drain first so sync window planning can use freshest runtime observations.
            broadcaster.drain_incoming(recv_drain_max);
            let now = now_unix_millis() as u64;
            let now_ms = now as u128;
            let mut worker_tick_ms = GATEWAY_ETH_NATIVE_RUNTIME_WORKER_TICK_MS;
            let mut next_recv_drain_max = GATEWAY_ETH_NATIVE_RECV_DRAIN_MAX_PER_BROADCAST;
            if gateway_eth_native_should_emit_discovery(chain_id)
                && should_send_gateway_eth_native_discovery(now_ms, last_discovery_sent_ms)
            {
                let heartbeat = ProtocolMessage::Gossip(ProtocolGossipMessage::Heartbeat {
                    from: local_node,
                    shard: ShardId(1),
                });
                let peer_list = ProtocolMessage::Gossip(ProtocolGossipMessage::PeerList {
                    from: local_node,
                    peers: peer_nodes.as_ref().clone(),
                });
                for peer in peer_nodes.iter() {
                    let _ = broadcaster.send(*peer, heartbeat.clone());
                    let _ = broadcaster.send(*peer, peer_list.clone());
                }
                last_discovery_sent_ms = now_ms;
            }
            if let Some(window) = plan_network_runtime_sync_pull_window(chain_id) {
                worker_tick_ms = GATEWAY_ETH_NATIVE_RUNTIME_WORKER_SYNC_TICK_MS;
                next_recv_drain_max = GATEWAY_ETH_NATIVE_RECV_DRAIN_MAX_SYNC_TICK;
                let phase_tag = gateway_eth_native_sync_pull_phase_tag(window.phase.as_str());
                let resend_interval_ms =
                    gateway_eth_native_sync_pull_resend_ms(chain_id, phase_tag);
                if let Some(fanout) = next_gateway_eth_native_sync_pull_fanout(
                    &worker_key_owned,
                    phase_tag,
                    window.from_block,
                    window.to_block,
                    now_ms,
                    resend_interval_ms,
                ) {
                    let resolved_fanout = resolve_gateway_eth_native_sync_pull_fanout(
                        chain_id,
                        phase_tag,
                        fanout,
                        peer_nodes.len(),
                    );
                    if resolved_fanout == 0 {
                        continue;
                    }
                    let ordered_peers = select_gateway_eth_native_sync_pull_peers(
                        chain_id,
                        peer_nodes.as_ref(),
                        resolved_fanout,
                    );
                    let sent = send_gateway_eth_native_sync_pull_requests(
                        &broadcaster,
                        local_node,
                        ordered_peers.as_slice(),
                        chain_id,
                        phase_tag,
                        window.from_block,
                        window.to_block,
                        resolved_fanout,
                        now,
                    );
                    if sent == 0 {
                        // All selected peers failed in this round, clear tracker to allow
                        // immediate retry on next tick instead of waiting resend timeout.
                        clear_gateway_eth_native_sync_pull_tracker(&worker_key_owned);
                        // Force next tick to emit discovery frame immediately.
                        last_discovery_sent_ms = 0;
                    }
                }
            } else {
                clear_gateway_eth_native_sync_pull_tracker(&worker_key_owned);
            }
            recv_drain_max = next_recv_drain_max;
            let elapsed = (now_unix_millis() as u64).saturating_sub(now);
            let sleep_ms = worker_tick_ms.saturating_sub(elapsed);
            thread::sleep(Duration::from_millis(sleep_ms.max(10)));
        }
    });
}

fn gateway_eth_native_sync_pull_phase_tag(phase: &str) -> u8 {
    match phase {
        "headers" => 1,
        "bodies" => 2,
        "state" => 3,
        "finalize" => 4,
        "discovery" => 5,
        _ => 0,
    }
}

#[cfg(test)]
fn gateway_eth_native_sync_pull_msg_type(phase: &str) -> DistributedOcccMessageType {
    gateway_eth_native_sync_pull_msg_type_by_tag(gateway_eth_native_sync_pull_phase_tag(phase))
}

fn gateway_eth_native_sync_pull_msg_type_by_tag(phase_tag: u8) -> DistributedOcccMessageType {
    match phase_tag {
        // Header phase keeps StateSync channel for compatibility.
        1 => DistributedOcccMessageType::StateSync,
        // Bodies/state/finalize/discovery/unknown go through shard-state channel.
        _ => DistributedOcccMessageType::ShardState,
    }
}

fn encode_gateway_eth_native_sync_pull_payload(
    chain_id: u64,
    phase_tag: u8,
    from_block: u64,
    to_block: u64,
) -> Vec<u8> {
    let mut out = Vec::with_capacity(4 + 1 + 8 + 8 + 8);
    out.extend_from_slice(&GATEWAY_ETH_NATIVE_SYNC_PULL_PAYLOAD_MAGIC);
    out.push(phase_tag);
    out.extend_from_slice(&chain_id.to_le_bytes());
    out.extend_from_slice(&from_block.to_le_bytes());
    out.extend_from_slice(&to_block.to_le_bytes());
    out
}

pub(super) fn gateway_evm_atomic_broadcast_exec_path(chain_id: u64) -> Option<PathBuf> {
    let chain_key_dec = format!("NOVOVM_GATEWAY_EVM_ATOMIC_BROADCAST_EXEC_CHAIN_{chain_id}");
    let chain_key_hex = format!(
        "NOVOVM_GATEWAY_EVM_ATOMIC_BROADCAST_EXEC_CHAIN_0x{:x}",
        chain_id
    );
    string_env_nonempty(&chain_key_dec)
        .or_else(|| string_env_nonempty(&chain_key_hex))
        .or_else(|| string_env_nonempty("NOVOVM_GATEWAY_EVM_ATOMIC_BROADCAST_EXEC"))
        .map(PathBuf::from)
}

fn gateway_evm_atomic_broadcast_chain_u64_env(chain_id: u64, base_key: &str, default: u64) -> u64 {
    let chain_key_dec = format!("{base_key}_CHAIN_{chain_id}");
    let chain_key_hex = format!("{base_key}_CHAIN_0x{:x}", chain_id);
    string_env_nonempty(&chain_key_dec)
        .or_else(|| string_env_nonempty(&chain_key_hex))
        .and_then(|raw| raw.trim().parse::<u64>().ok())
        .unwrap_or_else(|| u64_env(base_key, default))
}

pub(super) fn gateway_evm_atomic_broadcast_exec_retry_default(chain_id: u64) -> u64 {
    gateway_evm_atomic_broadcast_chain_u64_env(
        chain_id,
        "NOVOVM_GATEWAY_EVM_ATOMIC_BROADCAST_EXEC_RETRY",
        GATEWAY_EVM_ATOMIC_BROADCAST_EXEC_RETRY_DEFAULT,
    )
    .min(16)
}

pub(super) fn gateway_evm_atomic_broadcast_exec_timeout_ms_default(chain_id: u64) -> u64 {
    let timeout_ms = gateway_evm_atomic_broadcast_chain_u64_env(
        chain_id,
        "NOVOVM_GATEWAY_EVM_ATOMIC_BROADCAST_EXEC_TIMEOUT_MS",
        GATEWAY_EVM_ATOMIC_BROADCAST_EXEC_TIMEOUT_MS_DEFAULT,
    );
    if timeout_ms == 0 {
        0
    } else {
        timeout_ms.min(300_000)
    }
}

pub(super) fn gateway_evm_atomic_broadcast_exec_retry_backoff_ms_default(chain_id: u64) -> u64 {
    gateway_evm_atomic_broadcast_chain_u64_env(
        chain_id,
        "NOVOVM_GATEWAY_EVM_ATOMIC_BROADCAST_EXEC_RETRY_BACKOFF_MS",
        GATEWAY_EVM_ATOMIC_BROADCAST_EXEC_RETRY_BACKOFF_MS_DEFAULT,
    )
    .min(10_000)
}

pub(super) fn gateway_evm_atomic_broadcast_exec_batch_hard_max() -> u64 {
    u64_env(
        "NOVOVM_GATEWAY_EVM_ATOMIC_BROADCAST_EXEC_BATCH_HARD_MAX",
        GATEWAY_EVM_ATOMIC_BROADCAST_EXEC_BATCH_HARD_MAX,
    )
    .clamp(1, 4096)
}

pub(super) fn gateway_evm_atomic_broadcast_exec_batch_default() -> u64 {
    let hard_max = gateway_evm_atomic_broadcast_exec_batch_hard_max();
    u64_env(
        "NOVOVM_GATEWAY_EVM_ATOMIC_BROADCAST_EXEC_BATCH_DEFAULT",
        GATEWAY_EVM_ATOMIC_BROADCAST_EXEC_BATCH_DEFAULT,
    )
    .clamp(1, hard_max)
}

pub(super) fn gateway_atomic_broadcast_force_native(params: &serde_json::Value) -> bool {
    param_as_bool(params, "native")
        .or_else(|| param_as_bool(params, "force_native"))
        .unwrap_or(false)
}

pub(super) fn gateway_atomic_broadcast_use_external_executor(params: &serde_json::Value) -> bool {
    param_as_bool(params, "use_external_executor")
        .or_else(|| param_as_bool(params, "exec"))
        .unwrap_or(false)
}

pub(super) fn gateway_eth_public_broadcast_exec_path(chain_id: u64) -> Option<PathBuf> {
    let chain_key_dec = format!("NOVOVM_GATEWAY_ETH_PUBLIC_BROADCAST_EXEC_CHAIN_{chain_id}");
    let chain_key_hex = format!(
        "NOVOVM_GATEWAY_ETH_PUBLIC_BROADCAST_EXEC_CHAIN_0x{:x}",
        chain_id
    );
    string_env_nonempty(&chain_key_dec)
        .or_else(|| string_env_nonempty(&chain_key_hex))
        .or_else(|| string_env_nonempty("NOVOVM_GATEWAY_ETH_PUBLIC_BROADCAST_EXEC"))
        .map(PathBuf::from)
}

pub(super) fn gateway_eth_public_broadcast_required(chain_id: u64) -> bool {
    let chain_key_dec = format!("NOVOVM_GATEWAY_ETH_PUBLIC_BROADCAST_REQUIRED_CHAIN_{chain_id}");
    let chain_key_hex = format!(
        "NOVOVM_GATEWAY_ETH_PUBLIC_BROADCAST_REQUIRED_CHAIN_0x{:x}",
        chain_id
    );
    string_env_nonempty(&chain_key_dec)
        .or_else(|| string_env_nonempty(&chain_key_hex))
        .or_else(|| string_env_nonempty("NOVOVM_GATEWAY_ETH_PUBLIC_BROADCAST_REQUIRED"))
        .and_then(|raw| {
            let v = raw.trim().to_ascii_lowercase();
            match v.as_str() {
                "1" | "true" | "yes" | "on" => Some(true),
                "0" | "false" | "no" | "off" => Some(false),
                _ => None,
            }
        })
        .unwrap_or(false)
}

pub(super) fn gateway_eth_public_broadcast_capability_json(chain_id: u64) -> serde_json::Value {
    let required = gateway_eth_public_broadcast_required(chain_id);
    let exec_path = gateway_eth_public_broadcast_exec_path(chain_id);
    let native_peer_count = gateway_eth_public_broadcast_native_peers_snapshot(chain_id)
        .map(|(peers, _)| peers.len() as u64)
        .unwrap_or(0);
    let transport = gateway_eth_public_broadcast_native_transport(chain_id).as_mode();
    let mode = if exec_path.is_some() {
        "external_executor"
    } else if native_peer_count > 0 {
        "native_transport"
    } else {
        "none"
    };
    let available = exec_path.is_some() || native_peer_count > 0;
    let ready = available || !required;
    serde_json::json!({
        "chain_id": format!("0x{:x}", chain_id),
        "required": required,
        "ready": ready,
        "available": available,
        "mode": mode,
        "executor_configured": exec_path.is_some(),
        "executor_path": exec_path
            .as_ref()
            .map(|p| p.display().to_string())
            .unwrap_or_default(),
        "native_transport": transport,
        "native_peer_count": format!("0x{:x}", native_peer_count),
    })
}

fn gateway_eth_public_broadcast_chain_u64_env(chain_id: u64, base_key: &str, default: u64) -> u64 {
    let chain_key_dec = format!("{base_key}_CHAIN_{chain_id}");
    let chain_key_hex = format!("{base_key}_CHAIN_0x{:x}", chain_id);
    string_env_nonempty(&chain_key_dec)
        .or_else(|| string_env_nonempty(&chain_key_hex))
        .and_then(|raw| raw.trim().parse::<u64>().ok())
        .unwrap_or_else(|| u64_env(base_key, default))
}

fn gateway_eth_public_broadcast_chain_string_env(chain_id: u64, base_key: &str) -> Option<String> {
    let chain_key_dec = format!("{base_key}_CHAIN_{chain_id}");
    let chain_key_hex = format!("{base_key}_CHAIN_0x{:x}", chain_id);
    string_env_nonempty(&chain_key_dec)
        .or_else(|| string_env_nonempty(&chain_key_hex))
        .or_else(|| string_env_nonempty(base_key))
}

fn gateway_eth_public_broadcast_upstream_rpc_url(chain_id: u64) -> Option<String> {
    gateway_eth_public_broadcast_chain_string_env(
        chain_id,
        "NOVOVM_GATEWAY_ETH_PUBLIC_BROADCAST_UPSTREAM_RPC",
    )
    .or_else(|| {
        gateway_eth_public_broadcast_chain_string_env(chain_id, "NOVOVM_GATEWAY_ETH_UPSTREAM_RPC")
    })
}

fn gateway_eth_public_broadcast_upstream_rpc_timeout_ms(chain_id: u64) -> u64 {
    gateway_eth_public_broadcast_chain_u64_env(
        chain_id,
        "NOVOVM_GATEWAY_ETH_PUBLIC_BROADCAST_UPSTREAM_RPC_TIMEOUT_MS",
        GATEWAY_ETH_PUBLIC_BROADCAST_EXEC_TIMEOUT_MS_DEFAULT,
    )
}

fn parse_u64_with_optional_hex_prefix(raw: &str) -> Option<u64> {
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return None;
    }
    if let Some(hex) = trimmed
        .strip_prefix("0x")
        .or_else(|| trimmed.strip_prefix("0X"))
    {
        return u64::from_str_radix(hex, 16).ok();
    }
    trimmed.parse::<u64>().ok()
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum GatewayEthPublicBroadcastNativeTransport {
    Udp,
    Tcp,
}

impl GatewayEthPublicBroadcastNativeTransport {
    fn as_mode(self) -> &'static str {
        match self {
            Self::Udp => "udp",
            Self::Tcp => "tcp",
        }
    }
}

fn gateway_eth_public_broadcast_native_transport(
    chain_id: u64,
) -> GatewayEthPublicBroadcastNativeTransport {
    let raw = gateway_eth_public_broadcast_chain_string_env(
        chain_id,
        "NOVOVM_GATEWAY_ETH_PUBLIC_BROADCAST_NATIVE_TRANSPORT",
    )
    .unwrap_or_else(|| "udp".to_string());
    match raw.trim().to_ascii_lowercase().as_str() {
        "tcp" => GatewayEthPublicBroadcastNativeTransport::Tcp,
        _ => GatewayEthPublicBroadcastNativeTransport::Udp,
    }
}

fn gateway_eth_public_broadcast_native_node_id(chain_id: u64) -> NodeId {
    let raw = gateway_eth_public_broadcast_chain_string_env(
        chain_id,
        "NOVOVM_GATEWAY_ETH_PUBLIC_BROADCAST_NATIVE_NODE_ID",
    );
    let id = raw
        .as_deref()
        .and_then(parse_u64_with_optional_hex_prefix)
        .unwrap_or(1);
    NodeId(id)
}

fn gateway_eth_public_broadcast_native_listen_addr(
    chain_id: u64,
    transport: GatewayEthPublicBroadcastNativeTransport,
) -> String {
    gateway_eth_public_broadcast_chain_string_env(
        chain_id,
        "NOVOVM_GATEWAY_ETH_PUBLIC_BROADCAST_NATIVE_LISTEN",
    )
    .unwrap_or_else(|| match transport {
        GatewayEthPublicBroadcastNativeTransport::Udp => "0.0.0.0:0".to_string(),
        GatewayEthPublicBroadcastNativeTransport::Tcp => "127.0.0.1:0".to_string(),
    })
}

fn parse_gateway_eth_public_broadcast_native_peers(raw: &str) -> Vec<(NodeId, String)> {
    let mut peers = Vec::<(NodeId, String)>::new();
    for token in raw.split([',', ';', '\n', '\r', '\t', ' ']) {
        let entry = token.trim();
        if entry.is_empty() {
            continue;
        }
        let (node_raw, addr_raw) = entry
            .split_once('@')
            .or_else(|| entry.split_once('='))
            .unwrap_or(("", ""));
        if node_raw.is_empty() || addr_raw.trim().is_empty() {
            continue;
        }
        let Some(node_id) = parse_u64_with_optional_hex_prefix(node_raw) else {
            continue;
        };
        peers.push((NodeId(node_id), addr_raw.trim().to_string()));
    }
    peers
}

fn gateway_eth_public_broadcast_native_peers_snapshot(
    chain_id: u64,
) -> Option<(GatewayEthNativePeers, GatewayEthNativePeerNodes)> {
    let raw = gateway_eth_public_broadcast_chain_string_env(
        chain_id,
        "NOVOVM_GATEWAY_ETH_PUBLIC_BROADCAST_NATIVE_PEERS",
    )?;
    let cache = gateway_eth_native_peers_cache();
    if let Some(existing) = cache.get(&chain_id) {
        if existing.raw == raw {
            return Some((existing.peers.clone(), existing.peer_nodes.clone()));
        }
    }

    let peers_vec = parse_gateway_eth_public_broadcast_native_peers(raw.as_str());
    let peer_nodes_vec: Vec<NodeId> = peers_vec.iter().map(|(peer, _)| *peer).collect();
    let peers = Arc::new(peers_vec);
    let peer_nodes = Arc::new(peer_nodes_vec);
    cache.insert(
        chain_id,
        GatewayEthNativePeersCache {
            raw,
            peers: peers.clone(),
            peer_nodes: peer_nodes.clone(),
        },
    );
    Some((peers, peer_nodes))
}

fn gateway_eth_public_broadcast_payload_bytes(
    payload: GatewayEthPublicBroadcastPayload<'_>,
) -> Option<Vec<u8>> {
    payload
        .raw_tx
        .filter(|bytes| !bytes.is_empty())
        .map(|bytes| bytes.to_vec())
        .or_else(|| {
            payload
                .tx_ir_bincode
                .filter(|bytes| !bytes.is_empty())
                .map(|bytes| bytes.to_vec())
        })
}

fn execute_gateway_eth_public_broadcast_native(
    chain_id: u64,
    tx_hash: &[u8; 32],
    payload: GatewayEthPublicBroadcastPayload<'_>,
) -> Result<Option<(String, u64, String)>> {
    let Some((peers, peer_nodes)) = gateway_eth_public_broadcast_native_peers_snapshot(chain_id)
    else {
        return Ok(None);
    };
    if peers.is_empty() {
        return Ok(None);
    }
    let payload_bytes = gateway_eth_public_broadcast_payload_bytes(payload).ok_or_else(|| {
        anyhow::anyhow!(
            "public broadcast failed: chain_id={} tx_hash=0x{} reason=native_payload_missing",
            chain_id,
            to_hex(tx_hash)
        )
    })?;
    let transport = gateway_eth_public_broadcast_native_transport(chain_id);
    let listen_addr = gateway_eth_public_broadcast_native_listen_addr(chain_id, transport);
    let local_node = gateway_eth_public_broadcast_native_node_id(chain_id);
    let from_u32 = u32::try_from(local_node.0).map_err(|_| {
        anyhow::anyhow!(
            "public broadcast failed: chain_id={} tx_hash=0x{} reason=native_node_id_out_of_range node_id={}",
            chain_id,
            to_hex(tx_hash),
            local_node.0
        )
    })?;
    let now = now_unix_millis() as u64;
    let mut success = 0u64;
    let mut failed = 0u64;
    let mut errors = Vec::<String>::new();

    let (broadcaster_key, broadcaster) =
        get_or_create_gateway_eth_native_broadcaster(chain_id, transport, local_node, &listen_addr)
            .with_context(|| {
                format!(
            "public broadcast failed: chain_id={} tx_hash=0x{} reason=native_runtime_init_failed",
            chain_id,
            to_hex(tx_hash)
        )
            })?;
    ensure_gateway_eth_native_runtime_worker(
        chain_id,
        &broadcaster_key,
        broadcaster.clone(),
        local_node,
        peer_nodes.clone(),
    );

    if broadcaster.needs_peer_registration(peers.as_ref()) {
        for (peer, addr) in peers.iter() {
            if let Err(e) = broadcaster.register_peer(*peer, addr.as_str()) {
                failed = failed.saturating_add(1);
                errors.push(format!("register_peer({}:{})={}", peer.0, addr, e));
            }
        }
    }

    for (peer, _addr) in peers.iter() {
        let to_u32 = match u32::try_from(peer.0) {
            Ok(v) => v,
            Err(_) => {
                failed = failed.saturating_add(1);
                errors.push(format!("peer_id_out_of_range({})", peer.0));
                continue;
            }
        };
        let msg = ProtocolMessage::DistributedOcccGossip(DistributedOcccGossipMessage {
            from: from_u32,
            to: to_u32,
            msg_type: DistributedOcccMessageType::TxProposal,
            payload: payload_bytes.clone(),
            timestamp: now,
            seq: now,
        });
        match broadcaster.send(*peer, msg) {
            Ok(_) => success = success.saturating_add(1),
            Err(e) => {
                failed = failed.saturating_add(1);
                errors.push(format!("send({})={}", peer.0, e));
            }
        }
    }
    broadcaster.drain_incoming(GATEWAY_ETH_NATIVE_RECV_DRAIN_MAX_PER_BROADCAST);

    if success == 0 {
        bail!(
            "public broadcast failed: chain_id={} tx_hash=0x{} reason=native_send_failed failed={} details={}",
            chain_id,
            to_hex(tx_hash),
            failed,
            errors.join(";")
        );
    }
    let mut result = serde_json::json!({
        "broadcasted": true,
        "chain_id": format!("0x{:x}", chain_id),
        "tx_hash": format!("0x{}", to_hex(tx_hash)),
        "mode": format!("native_{}", transport.as_mode()),
        "sent": format!("0x{:x}", success),
        "failed": format!("0x{:x}", failed),
        "peers": format!("0x{:x}", success.saturating_add(failed)),
    });
    if failed > 0 {
        if let Some(map) = result.as_object_mut() {
            map.insert(
                "errors".to_string(),
                serde_json::Value::Array(
                    errors
                        .into_iter()
                        .take(8)
                        .map(serde_json::Value::String)
                        .collect(),
                ),
            );
        }
    }
    Ok(Some((
        result.to_string(),
        1,
        format!("native:{}", transport.as_mode()),
    )))
}

pub(super) fn gateway_eth_public_broadcast_exec_retry_default(chain_id: u64) -> u64 {
    gateway_eth_public_broadcast_chain_u64_env(
        chain_id,
        "NOVOVM_GATEWAY_ETH_PUBLIC_BROADCAST_EXEC_RETRY",
        GATEWAY_ETH_PUBLIC_BROADCAST_EXEC_RETRY_DEFAULT,
    )
    .min(16)
}

pub(super) fn gateway_eth_public_broadcast_exec_timeout_ms_default(chain_id: u64) -> u64 {
    let timeout_ms = gateway_eth_public_broadcast_chain_u64_env(
        chain_id,
        "NOVOVM_GATEWAY_ETH_PUBLIC_BROADCAST_EXEC_TIMEOUT_MS",
        GATEWAY_ETH_PUBLIC_BROADCAST_EXEC_TIMEOUT_MS_DEFAULT,
    );
    if timeout_ms == 0 {
        0
    } else {
        timeout_ms.min(300_000)
    }
}

pub(super) fn gateway_eth_public_broadcast_exec_retry_backoff_ms_default(chain_id: u64) -> u64 {
    gateway_eth_public_broadcast_chain_u64_env(
        chain_id,
        "NOVOVM_GATEWAY_ETH_PUBLIC_BROADCAST_EXEC_RETRY_BACKOFF_MS",
        GATEWAY_ETH_PUBLIC_BROADCAST_EXEC_RETRY_BACKOFF_MS_DEFAULT,
    )
    .min(10_000)
}

#[derive(Clone, Copy)]
pub(super) struct GatewayEthPublicBroadcastPayload<'a> {
    pub(super) raw_tx: Option<&'a [u8]>,
    pub(super) tx_ir_bincode: Option<&'a [u8]>,
}

pub(super) fn build_gateway_eth_public_broadcast_executor_request(
    chain_id: u64,
    tx_hash: &[u8; 32],
    payload: GatewayEthPublicBroadcastPayload<'_>,
) -> serde_json::Value {
    let mut req = serde_json::json!({
        "chain_id": format!("0x{:x}", chain_id),
        "tx_hash": format!("0x{}", to_hex(tx_hash)),
    });
    if let Some(raw_tx) = payload.raw_tx.filter(|payload| !payload.is_empty()) {
        if let Some(map) = req.as_object_mut() {
            map.insert(
                "raw_tx".to_string(),
                serde_json::Value::String(format!("0x{}", to_hex(raw_tx))),
            );
            map.insert(
                "raw_tx_len".to_string(),
                serde_json::Value::String(format!("0x{:x}", raw_tx.len())),
            );
        }
    }
    if let Some(tx_ir_bincode) = payload.tx_ir_bincode.filter(|payload| !payload.is_empty()) {
        if let Some(map) = req.as_object_mut() {
            map.insert(
                "tx_ir_bincode".to_string(),
                serde_json::Value::String(format!("0x{}", to_hex(tx_ir_bincode))),
            );
            map.insert(
                "tx_ir_format".to_string(),
                serde_json::Value::String("bincode_v1".to_string()),
            );
        }
    }
    req
}

pub(super) fn validate_gateway_eth_public_broadcast_executor_output(
    output: &str,
    chain_id: u64,
    tx_hash: &[u8; 32],
) -> Result<()> {
    let trimmed = output.trim();
    if trimmed.is_empty() {
        return Ok(());
    }
    let value: serde_json::Value = match serde_json::from_str(trimmed) {
        Ok(v) => v,
        Err(_) => return Ok(()),
    };
    let Some(map) = value.as_object() else {
        return Ok(());
    };
    if let Some(flag) = map
        .get("broadcasted")
        .or_else(|| map.get("ok"))
        .and_then(serde_json::Value::as_bool)
    {
        if !flag {
            bail!("eth public-broadcast executor reported broadcasted=false");
        }
    }
    if let Some(error) = map.get("error").and_then(serde_json::Value::as_str) {
        let reason = error.trim();
        if !reason.is_empty() {
            bail!(
                "eth public-broadcast executor reported error: tx_hash=0x{} reason={}",
                to_hex(tx_hash),
                reason
            );
        }
    }
    if let Some(raw_tx_hash) = map.get("tx_hash").or_else(|| map.get("txHash")) {
        let Some(tx_hash_hex) = raw_tx_hash.as_str() else {
            bail!("eth public-broadcast executor tx_hash must be string");
        };
        let actual = parse_hex32_from_string(tx_hash_hex, "executor.tx_hash")
            .context("decode executor tx_hash failed")?;
        if actual != *tx_hash {
            bail!(
                "eth public-broadcast executor tx_hash mismatch: expected=0x{} actual=0x{}",
                to_hex(tx_hash),
                to_hex(&actual)
            );
        }
    }
    if let Some(raw_chain_id) = map.get("chain_id").or_else(|| map.get("chainId")) {
        let Some(actual_chain_id) = value_to_u64(raw_chain_id) else {
            bail!("eth public-broadcast executor chain_id must be decimal or hex number");
        };
        if actual_chain_id != chain_id {
            bail!(
                "eth public-broadcast executor chain_id mismatch: expected={} actual={}",
                chain_id,
                actual_chain_id
            );
        }
    }
    Ok(())
}

pub(super) fn execute_gateway_eth_public_broadcast(
    exec_path: &Path,
    chain_id: u64,
    tx_hash: &[u8; 32],
    payload: GatewayEthPublicBroadcastPayload<'_>,
    timeout_ms: u64,
) -> Result<String> {
    let req = build_gateway_eth_public_broadcast_executor_request(chain_id, tx_hash, payload);
    let req_body =
        serde_json::to_vec(&req).context("serialize eth public-broadcast request failed")?;
    let mut child = Command::new(exec_path)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .with_context(|| {
            format!(
                "spawn eth public-broadcast executor failed: {}",
                exec_path.display()
            )
        })?;
    if let Some(stdin) = child.stdin.as_mut() {
        stdin.write_all(&req_body).with_context(|| {
            format!(
                "write eth public-broadcast request into executor stdin failed: {}",
                exec_path.display()
            )
        })?;
    }
    let output = if timeout_ms == 0 {
        child.wait_with_output().with_context(|| {
            format!(
                "wait eth public-broadcast executor output failed: {}",
                exec_path.display()
            )
        })?
    } else {
        let timeout = Duration::from_millis(timeout_ms);
        let start = SystemTime::now();
        loop {
            match child.try_wait().with_context(|| {
                format!(
                    "poll eth public-broadcast executor failed: {}",
                    exec_path.display()
                )
            })? {
                Some(_) => {
                    break child.wait_with_output().with_context(|| {
                        format!(
                            "read eth public-broadcast executor output failed: {}",
                            exec_path.display()
                        )
                    })?;
                }
                None => {
                    if start.elapsed().unwrap_or_else(|_| Duration::from_millis(0)) >= timeout {
                        let _ = child.kill();
                        let timed_out_output = child.wait_with_output().with_context(|| {
                            format!(
                                "read timed-out eth public-broadcast executor output failed: {}",
                                exec_path.display()
                            )
                        })?;
                        let stderr = String::from_utf8_lossy(&timed_out_output.stderr);
                        bail!(
                            "eth public-broadcast executor timed out: timeout_ms={} stderr={}",
                            timeout_ms,
                            stderr.trim()
                        );
                    }
                    thread::sleep(Duration::from_millis(2));
                }
            }
        }
    };
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        bail!(
            "eth public-broadcast executor exit={} stderr={}",
            output.status,
            stderr.trim()
        );
    }
    let stdout = String::from_utf8_lossy(&output.stdout).trim().to_string();
    validate_gateway_eth_public_broadcast_executor_output(&stdout, chain_id, tx_hash)?;
    Ok(stdout)
}

pub(super) fn execute_gateway_eth_public_broadcast_with_retry(
    exec_path: &Path,
    chain_id: u64,
    tx_hash: &[u8; 32],
    payload: GatewayEthPublicBroadcastPayload<'_>,
    retry: u64,
    timeout_ms: u64,
    retry_backoff_ms: u64,
) -> std::result::Result<(String, u64), (anyhow::Error, u64)> {
    let mut attempts = 0u64;
    loop {
        attempts = attempts.saturating_add(1);
        match execute_gateway_eth_public_broadcast(
            exec_path, chain_id, tx_hash, payload, timeout_ms,
        ) {
            Ok(output) => return Ok((output, attempts)),
            Err(e) => {
                if attempts > retry {
                    return Err((e, attempts));
                }
                if retry_backoff_ms > 0 {
                    thread::sleep(Duration::from_millis(retry_backoff_ms));
                }
            }
        }
    }
}

fn execute_gateway_eth_public_broadcast_upstream(
    chain_id: u64,
    tx_hash: &[u8; 32],
    payload: GatewayEthPublicBroadcastPayload<'_>,
) -> Result<Option<(String, u64, String)>> {
    let Some(url) = gateway_eth_public_broadcast_upstream_rpc_url(chain_id) else {
        return Ok(None);
    };
    let Some(raw_tx) = payload.raw_tx.filter(|raw| !raw.is_empty()) else {
        return Ok(None);
    };
    let timeout_ms = gateway_eth_public_broadcast_upstream_rpc_timeout_ms(chain_id);
    let raw_tx_hex = format!("0x{}", to_hex(raw_tx));
    let result = execute_gateway_eth_upstream_json_rpc(
        &url,
        "eth_sendRawTransaction",
        serde_json::json!([raw_tx_hex]),
        timeout_ms,
    )?;
    let Some(returned_hash) = result.as_str() else {
        bail!(
            "upstream eth_sendRawTransaction returned non-string result: {}",
            result
        );
    };
    let expected_hash = format!("0x{}", to_hex(tx_hash));
    if !returned_hash.eq_ignore_ascii_case(&expected_hash) {
        bail!(
            "upstream eth_sendRawTransaction hash mismatch: expected={} got={}",
            expected_hash,
            returned_hash
        );
    }
    Ok(Some((
        returned_hash.to_string(),
        1,
        format!("upstream_rpc:{url}"),
    )))
}

pub(super) fn maybe_execute_gateway_eth_public_broadcast(
    chain_id: u64,
    tx_hash: &[u8; 32],
    payload: GatewayEthPublicBroadcastPayload<'_>,
    required_override: bool,
) -> Result<Option<(String, u64, String)>> {
    let Some(exec_path) = gateway_eth_public_broadcast_exec_path(chain_id) else {
        let mut upstream_error: Option<anyhow::Error> = None;
        match execute_gateway_eth_public_broadcast_upstream(chain_id, tx_hash, payload) {
            Ok(Some(result)) => return Ok(Some(result)),
            Ok(None) => {}
            Err(error) => upstream_error = Some(error),
        }
        if let Some(native_result) =
            execute_gateway_eth_public_broadcast_native(chain_id, tx_hash, payload)?
        {
            return Ok(Some(native_result));
        }
        if let Some(error) = upstream_error {
            bail!(
                "public broadcast failed: chain_id={} tx_hash=0x{} reason=upstream_rpc err={}",
                chain_id,
                to_hex(tx_hash),
                error
            );
        }
        if required_override || gateway_eth_public_broadcast_required(chain_id) {
            bail!(
                "public broadcast failed: chain_id={} tx_hash=0x{} reason=executor_not_configured",
                chain_id,
                to_hex(tx_hash),
            );
        }
        return Ok(None);
    };
    let retry = gateway_eth_public_broadcast_exec_retry_default(chain_id);
    let timeout_ms = gateway_eth_public_broadcast_exec_timeout_ms_default(chain_id);
    let retry_backoff_ms = gateway_eth_public_broadcast_exec_retry_backoff_ms_default(chain_id);
    match execute_gateway_eth_public_broadcast_with_retry(
        exec_path.as_path(),
        chain_id,
        tx_hash,
        payload,
        retry,
        timeout_ms,
        retry_backoff_ms,
    ) {
        Ok((output, attempts)) => Ok(Some((output, attempts, exec_path.display().to_string()))),
        Err((e, attempts)) => {
            bail!(
                "public broadcast failed: chain_id={} tx_hash=0x{} attempts={} err={}",
                chain_id,
                to_hex(tx_hash),
                attempts,
                e
            );
        }
    }
}

#[cfg(test)]
#[allow(clippy::items_after_test_module)]
mod tests {
    use super::*;

    #[test]
    fn native_sync_pull_tracker_dedups_same_window_before_timeout() {
        let worker_key = "chain1:udp:node1:0.0.0.0:0:test";
        clear_gateway_eth_native_sync_pull_tracker(worker_key);
        let now = 10_000u128;
        let phase_tag = gateway_eth_native_sync_pull_phase_tag("headers");
        let resend_ms = gateway_eth_native_sync_pull_resend_ms_by_tag(phase_tag);
        assert_eq!(
            next_gateway_eth_native_sync_pull_fanout(
                worker_key, phase_tag, 101, 200, now, resend_ms
            ),
            Some(1)
        );
        assert_eq!(
            next_gateway_eth_native_sync_pull_fanout(
                worker_key,
                phase_tag,
                101,
                200,
                now.saturating_add(500),
                resend_ms,
            ),
            None
        );
        clear_gateway_eth_native_sync_pull_tracker(worker_key);
    }

    #[test]
    fn native_sync_pull_tracker_escalates_fanout_on_timeout_retries() {
        let worker_key = "chain1:udp:node1:0.0.0.0:0:test2";
        clear_gateway_eth_native_sync_pull_tracker(worker_key);
        let now = 20_000u128;
        let phase_tag = gateway_eth_native_sync_pull_phase_tag("headers");
        let resend_ms = gateway_eth_native_sync_pull_resend_ms_by_tag(phase_tag);
        assert_eq!(
            next_gateway_eth_native_sync_pull_fanout(worker_key, phase_tag, 1, 100, now, resend_ms),
            Some(1)
        );
        let resend_now = now.saturating_add(resend_ms + 10);
        assert_eq!(
            next_gateway_eth_native_sync_pull_fanout(
                worker_key, phase_tag, 1, 100, resend_now, resend_ms
            ),
            Some(2)
        );
        let resend_again = resend_now.saturating_add(resend_ms + 10);
        assert_eq!(
            next_gateway_eth_native_sync_pull_fanout(
                worker_key,
                phase_tag,
                1,
                100,
                resend_again,
                resend_ms
            ),
            Some(usize::MAX)
        );
        assert_eq!(
            next_gateway_eth_native_sync_pull_fanout(
                worker_key,
                phase_tag,
                101,
                200,
                resend_again.saturating_add(100),
                resend_ms,
            ),
            Some(1),
            "window change should reset to single-peer first shot"
        );
        clear_gateway_eth_native_sync_pull_tracker(worker_key);
    }

    #[test]
    fn select_sync_pull_peers_prefers_highest_runtime_peer() {
        let peers = vec![NodeId(10), NodeId(11), NodeId(12)];
        let runtime_heads = vec![(11u64, 200u64), (12u64, 180u64), (10u64, 150u64)];
        let selected = select_gateway_eth_sync_pull_peers_from_snapshot(&peers, &runtime_heads, 2);
        assert_eq!(selected, vec![NodeId(11), NodeId(12)]);
    }

    #[test]
    fn select_sync_pull_peers_falls_back_to_configured_order_when_unknown() {
        let peers = vec![NodeId(10), NodeId(11)];
        let runtime_heads = vec![(99u64, 300u64)];
        let selected = select_gateway_eth_sync_pull_peers_from_snapshot(&peers, &runtime_heads, 1);
        assert_eq!(selected, vec![NodeId(10)]);
    }

    #[test]
    fn native_discovery_send_is_rate_limited() {
        assert!(should_send_gateway_eth_native_discovery(10_000, 0));
        assert!(!should_send_gateway_eth_native_discovery(10_100, 10_000));
        assert!(should_send_gateway_eth_native_discovery(
            10_000 + GATEWAY_ETH_NATIVE_DISCOVERY_INTERVAL_MS + 1,
            10_000
        ));
    }

    #[test]
    fn merge_sync_pull_candidates_keeps_priority_and_uniqueness() {
        let preferred = vec![NodeId(11), NodeId(12)];
        let full = vec![NodeId(11), NodeId(10), NodeId(12), NodeId(13)];
        let merged = merge_gateway_eth_sync_pull_candidates(preferred, full);
        assert_eq!(merged, vec![NodeId(11), NodeId(12), NodeId(10), NodeId(13)]);
    }

    #[test]
    fn split_sync_pull_window_creates_contiguous_ranges() {
        let ranges = split_gateway_eth_sync_pull_window(101, 120, 3);
        assert_eq!(ranges.len(), 3);
        assert_eq!(ranges[0].0, 101);
        assert_eq!(ranges[2].1, 120);
        for pair in ranges.windows(2) {
            assert_eq!(pair[0].1.saturating_add(1), pair[1].0);
        }
        let total = ranges
            .iter()
            .map(|(from, to)| to.saturating_sub(*from).saturating_add(1))
            .sum::<u64>();
        assert_eq!(total, 20);
    }

    #[test]
    fn split_sync_pull_window_caps_segments_by_window_span() {
        let ranges = split_gateway_eth_sync_pull_window(1_000, 1_003, 16);
        assert_eq!(
            ranges,
            vec![
                (1_000, 1_000),
                (1_001, 1_001),
                (1_002, 1_002),
                (1_003, 1_003)
            ]
        );
    }

    #[test]
    fn resolve_sync_pull_fanout_respects_requested_and_peer_count_without_cap() {
        let chain_id = 8_101_u64;
        let phase_tag = gateway_eth_native_sync_pull_phase_tag("headers");
        let resolved = resolve_gateway_eth_native_sync_pull_fanout(chain_id, phase_tag, 2, 10);
        assert_eq!(resolved, 2);
        let resolved_all =
            resolve_gateway_eth_native_sync_pull_fanout(chain_id, phase_tag, usize::MAX, 3);
        assert_eq!(resolved_all, 3);
    }

    #[test]
    fn sync_pull_phase_env_suffix_is_stable() {
        assert_eq!(
            gateway_eth_native_sync_pull_phase_env_suffix(gateway_eth_native_sync_pull_phase_tag(
                "headers"
            )),
            Some("HEADERS")
        );
        assert_eq!(
            gateway_eth_native_sync_pull_phase_env_suffix(gateway_eth_native_sync_pull_phase_tag(
                "bodies"
            )),
            Some("BODIES")
        );
        assert_eq!(
            gateway_eth_native_sync_pull_phase_env_suffix(gateway_eth_native_sync_pull_phase_tag(
                "state"
            )),
            Some("STATE")
        );
        assert_eq!(
            gateway_eth_native_sync_pull_phase_env_suffix(gateway_eth_native_sync_pull_phase_tag(
                "finalize"
            )),
            Some("FINALIZE")
        );
    }

    #[test]
    fn sync_pull_phase_routes_to_expected_message_type() {
        assert!(matches!(
            gateway_eth_native_sync_pull_msg_type("headers"),
            DistributedOcccMessageType::StateSync
        ));
        assert!(matches!(
            gateway_eth_native_sync_pull_msg_type("bodies"),
            DistributedOcccMessageType::ShardState
        ));
        assert!(matches!(
            gateway_eth_native_sync_pull_msg_type("state"),
            DistributedOcccMessageType::ShardState
        ));
        assert!(matches!(
            gateway_eth_native_sync_pull_msg_type("finalize"),
            DistributedOcccMessageType::ShardState
        ));
    }

    #[test]
    fn native_discovery_emission_follows_runtime_sync_state() {
        let chain_id = 8_001_u64;
        set_network_runtime_sync_status(
            chain_id,
            NetworkRuntimeSyncStatus {
                peer_count: 0,
                starting_block: 0,
                current_block: 10,
                highest_block: 20,
            },
        );
        assert!(gateway_eth_native_should_emit_discovery(chain_id));

        set_network_runtime_sync_status(
            chain_id,
            NetworkRuntimeSyncStatus {
                peer_count: 2,
                starting_block: 0,
                current_block: 20,
                highest_block: 20,
            },
        );
        assert!(!gateway_eth_native_should_emit_discovery(chain_id));

        set_network_runtime_sync_status(
            chain_id,
            NetworkRuntimeSyncStatus {
                peer_count: 1,
                starting_block: 0,
                current_block: 20,
                highest_block: 40,
            },
        );
        assert!(gateway_eth_native_should_emit_discovery(chain_id));

        assert!(gateway_eth_native_should_emit_discovery(9_999_991_u64));
    }
}

pub(super) fn validate_gateway_atomic_broadcast_executor_output(
    output: &str,
    ticket: &GatewayEvmAtomicBroadcastTicketV1,
) -> Result<()> {
    let trimmed = output.trim();
    if trimmed.is_empty() {
        return Ok(());
    }
    let value: serde_json::Value = match serde_json::from_str(trimmed) {
        Ok(v) => v,
        Err(_) => return Ok(()),
    };
    let Some(map) = value.as_object() else {
        return Ok(());
    };

    if let Some(flag) = map
        .get("broadcasted")
        .or_else(|| map.get("ok"))
        .and_then(serde_json::Value::as_bool)
    {
        if !flag {
            bail!("evm atomic-broadcast executor reported broadcasted=false");
        }
    }

    if let Some(error) = map.get("error").and_then(serde_json::Value::as_str) {
        let reason = error.trim();
        if !reason.is_empty() {
            bail!(
                "evm atomic-broadcast executor reported error: intent_id={} reason={}",
                ticket.intent_id,
                reason
            );
        }
    }

    if let Some(raw_intent_id) = map.get("intent_id").or_else(|| map.get("intentId")) {
        let Some(intent_id) = raw_intent_id.as_str() else {
            bail!("evm atomic-broadcast executor intent_id must be string");
        };
        if intent_id != ticket.intent_id {
            bail!(
                "evm atomic-broadcast executor intent_id mismatch: expected={} actual={}",
                ticket.intent_id,
                intent_id
            );
        }
    }

    if let Some(raw_tx_hash) = map.get("tx_hash").or_else(|| map.get("txHash")) {
        let Some(tx_hash_hex) = raw_tx_hash.as_str() else {
            bail!("evm atomic-broadcast executor tx_hash must be string");
        };
        let tx_hash = parse_hex32_from_string(tx_hash_hex, "executor.tx_hash")
            .context("decode executor tx_hash failed")?;
        if tx_hash != ticket.tx_hash {
            bail!(
                "evm atomic-broadcast executor tx_hash mismatch: expected=0x{} actual=0x{}",
                to_hex(&ticket.tx_hash),
                to_hex(&tx_hash)
            );
        }
    }

    if let Some(raw_chain_id) = map.get("chain_id").or_else(|| map.get("chainId")) {
        let Some(chain_id) = value_to_u64(raw_chain_id) else {
            bail!("evm atomic-broadcast executor chain_id must be decimal or hex number");
        };
        if chain_id != ticket.chain_id {
            bail!(
                "evm atomic-broadcast executor chain_id mismatch: expected={} actual={}",
                ticket.chain_id,
                chain_id
            );
        }
    }

    Ok(())
}

pub(super) fn build_gateway_atomic_broadcast_executor_request(
    ticket: &GatewayEvmAtomicBroadcastTicketV1,
    tx_ir_bincode: Option<&[u8]>,
) -> serde_json::Value {
    let mut req = serde_json::json!({
        "intent_id": ticket.intent_id,
        "chain_id": format!("0x{:x}", ticket.chain_id),
        "tx_hash": format!("0x{}", to_hex(&ticket.tx_hash)),
    });
    if let Some(tx_ir_bincode) = tx_ir_bincode.filter(|payload| !payload.is_empty()) {
        if let Some(map) = req.as_object_mut() {
            map.insert(
                "tx_ir_bincode".to_string(),
                serde_json::Value::String(format!("0x{}", to_hex(tx_ir_bincode))),
            );
            map.insert(
                "tx_ir_format".to_string(),
                serde_json::Value::String("bincode_v1".to_string()),
            );
        }
    }
    req
}

pub(super) fn decode_gateway_atomic_broadcast_tx_ir_bincode(payload: &[u8]) -> Result<TxIR> {
    if payload.is_empty() {
        bail!("atomic-broadcast tx_ir_bincode is empty");
    }
    if let Ok(tx) = bincode::deserialize::<TxIR>(payload) {
        return Ok(tx);
    }
    if let Ok(mut txs) = bincode::deserialize::<Vec<TxIR>>(payload) {
        if txs.len() == 1 {
            return Ok(txs.remove(0));
        }
        bail!(
            "atomic-broadcast tx_ir_bincode must contain exactly one tx, got {}",
            txs.len()
        );
    }
    bail!("decode atomic-broadcast tx_ir_bincode failed");
}
