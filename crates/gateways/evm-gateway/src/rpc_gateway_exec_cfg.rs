use super::*;
use aes::cipher::{BlockEncrypt, KeyInit};
use aes::{Aes128, Aes256};
use ctr::cipher::{KeyIvInit, StreamCipher};
use dashmap::DashMap;
use hmac::{Hmac, Mac};
use k256::ecdh::diffie_hellman;
use k256::ecdsa::SigningKey;
use k256::{PublicKey as K256PublicKey, SecretKey as K256SecretKey};
use novovm_network::{
    classify_adaptive_peer_routes, default_eth_native_capabilities,
    get_network_runtime_peer_heads_top_k, get_network_runtime_sync_status,
    observe_eth_native_bodies_pull, observe_eth_native_bodies_response,
    observe_eth_native_discovery, observe_eth_native_headers_pull,
    observe_eth_native_headers_response, observe_eth_native_hello, observe_eth_native_rlpx_auth,
    observe_eth_native_rlpx_auth_ack, observe_eth_native_snap_pull,
    observe_eth_native_snap_response, observe_eth_native_status, parse_enode_endpoint,
    parse_port_list, parse_u64_with_optional_hex_prefix, plan_network_runtime_sync_pull_window,
    upsert_network_runtime_eth_peer_session, AdaptivePeerRoutePolicy, PluginPeerEndpoint,
    TcpTransport, Transport, UdpTransport,
};
#[cfg(test)]
use novovm_network::{set_network_runtime_sync_status, NetworkRuntimeSyncStatus};
use novovm_protocol::{
    EvmNativeMessage, GossipMessage as ProtocolGossipMessage, NodeId, ProtocolMessage, ShardId,
};
use rand::rngs::OsRng;
use sha3::Digest;
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
                        Ok(Some(msg)) => {
                            gateway_eth_native_handle_incoming_message(&msg);
                        }
                        Ok(None) | Err(_) => break,
                    }
                }
            }
            Self::Tcp { node, socket, .. } => {
                for _ in 0..cap {
                    match socket.try_recv(*node) {
                        Ok(Some(msg)) => {
                            gateway_eth_native_handle_incoming_message(&msg);
                        }
                        Ok(None) | Err(_) => break,
                    }
                }
            }
        }
    }
}

fn gateway_eth_native_handle_incoming_message(msg: &ProtocolMessage) {
    let ProtocolMessage::EvmNative(native_msg) = msg else {
        return;
    };
    let EvmNativeMessage::Transactions {
        chain_id,
        tx_hash,
        tx_count,
        payload,
        ..
    } = native_msg
    else {
        return;
    };
    gateway_eth_native_ingest_transactions_payload(*chain_id, *tx_hash, *tx_count, payload);
}

fn gateway_eth_native_decode_transactions_payload(
    announced_chain_id: u64,
    payload: &[u8],
) -> Result<Vec<TxIR>, String> {
    if payload.is_empty() {
        return Err("transactions payload is empty".to_string());
    }
    if let Ok(txs) = bincode::deserialize::<Vec<TxIR>>(payload) {
        if txs.is_empty() {
            return Err("transactions payload decoded as empty tx list".to_string());
        }
        return Ok(txs);
    }
    if let Ok(tx) = bincode::deserialize::<TxIR>(payload) {
        return Ok(vec![tx]);
    }
    if let Ok(raw_txs) = gateway_eth_native_extract_raw_txs_from_transactions_rlp(payload) {
        let mut txs = Vec::with_capacity(raw_txs.len());
        for raw_tx in raw_txs {
            txs.push(gateway_eth_native_decode_single_raw_tx(
                announced_chain_id,
                raw_tx.as_slice(),
            )?);
        }
        return Ok(txs);
    }
    Ok(vec![gateway_eth_native_decode_single_raw_tx(
        announced_chain_id,
        payload,
    )?])
}

fn gateway_eth_native_decode_single_raw_tx(
    announced_chain_id: u64,
    raw_tx: &[u8],
) -> Result<TxIR, String> {
    let fields = translate_raw_evm_tx_fields_m0(raw_tx)
        .map_err(|e| format!("decode raw tx payload failed: {e}"))?;
    let from = recover_raw_evm_tx_sender_m0(raw_tx)
        .map_err(|e| format!("recover raw tx sender failed: {e}"))?
        .ok_or_else(|| "raw tx sender unavailable".to_string())?;
    let effective_chain_id = fields.chain_id.unwrap_or(announced_chain_id);
    Ok(tx_ir_from_raw_fields_m0(
        &fields,
        raw_tx,
        from,
        effective_chain_id,
    ))
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum GatewayEthSwapKind {
    V2,
    V3,
}

fn gateway_eth_plugin_is_uniswap_v2_swap_selector(selector: [u8; 4]) -> bool {
    GATEWAY_ETH_UNISWAP_V2_SWAP_SELECTORS
        .iter()
        .any(|known| *known == selector)
}

fn gateway_eth_plugin_is_uniswap_v3_swap_selector(selector: [u8; 4]) -> bool {
    GATEWAY_ETH_UNISWAP_V3_SWAP_SELECTORS
        .iter()
        .any(|known| *known == selector)
}

fn gateway_eth_plugin_detect_swap_kind_from_raw_tx(
    announced_chain_id: u64,
    raw_tx: &[u8],
) -> Option<GatewayEthSwapKind> {
    let tx = gateway_eth_native_decode_single_raw_tx(announced_chain_id, raw_tx).ok()?;
    let to = tx.to.as_ref()?;
    if to.len() != 20 || tx.data.len() < 4 {
        return None;
    }
    let selector = [tx.data[0], tx.data[1], tx.data[2], tx.data[3]];
    if to.as_slice() == GATEWAY_ETH_UNISWAP_V2_ROUTER {
        if gateway_eth_plugin_is_uniswap_v2_swap_selector(selector) || !tx.data.is_empty() {
            return Some(GatewayEthSwapKind::V2);
        }
        return None;
    }
    if to.as_slice() == GATEWAY_ETH_UNISWAP_V3_ROUTER
        || to.as_slice() == GATEWAY_ETH_UNISWAP_V3_ROUTER_02
        || to.as_slice() == GATEWAY_ETH_UNISWAP_UNIVERSAL_ROUTER
    {
        if gateway_eth_plugin_is_uniswap_v3_swap_selector(selector) || !tx.data.is_empty() {
            return Some(GatewayEthSwapKind::V3);
        }
    }
    None
}

fn gateway_eth_native_extract_raw_txs_from_transactions_rlp(
    payload: &[u8],
) -> Result<Vec<Vec<u8>>, String> {
    let (root, consumed) = gateway_eth_rlpx_parse_item(payload)?;
    if consumed != payload.len() {
        return Err("transactions rlp has trailing bytes".to_string());
    }
    let GatewayEthRlpxRlpItem::List(entries) = root else {
        return Err("transactions rlp root is not list".to_string());
    };
    if entries.is_empty() {
        return Err("transactions rlp list is empty".to_string());
    }

    let mut out = Vec::<Vec<u8>>::new();
    let mut cursor = 0usize;
    while cursor < entries.len() {
        let (item, used) = gateway_eth_rlpx_parse_item(&entries[cursor..])?;
        if used == 0 {
            return Err("transactions rlp item consumed zero bytes".to_string());
        }
        let raw_tx = match item {
            GatewayEthRlpxRlpItem::Bytes(bytes) => bytes.to_vec(),
            GatewayEthRlpxRlpItem::List(_) => entries[cursor..cursor + used].to_vec(),
        };
        if raw_tx.is_empty() {
            return Err("transactions rlp contains empty tx item".to_string());
        }
        out.push(raw_tx);
        cursor = cursor.saturating_add(used);
    }
    if cursor != entries.len() {
        return Err("transactions rlp item parser left trailing bytes".to_string());
    }
    if out.is_empty() {
        return Err("transactions rlp has no tx items".to_string());
    }
    Ok(out)
}

fn gateway_eth_native_ingest_transactions_payload(
    announced_chain_id: u64,
    tx_hash_hint: [u8; 32],
    tx_count_hint: u64,
    payload: &[u8],
) {
    let mut txs = match gateway_eth_native_decode_transactions_payload(announced_chain_id, payload)
    {
        Ok(txs) => txs,
        Err(err) => {
            if gateway_warn_enabled() {
                eprintln!(
                    "gateway_warn: drop native transactions payload: chain_id={} tx_hash_hint=0x{} reason={}",
                    announced_chain_id,
                    to_hex(&tx_hash_hint),
                    err
                );
            }
            return;
        }
    };
    for tx in &mut txs {
        if tx.hash.is_empty() {
            tx.compute_hash();
        }
    }
    if tx_count_hint > 0 && tx_count_hint as usize != txs.len() && gateway_warn_enabled() {
        eprintln!(
            "gateway_warn: native transactions count hint mismatch: announced={} decoded={} tx_hash_hint=0x{}",
            tx_count_hint,
            txs.len(),
            to_hex(&tx_hash_hint),
        );
    }

    let mut by_chain = BTreeMap::<u64, Vec<TxIR>>::new();
    for tx in txs {
        by_chain.entry(tx.chain_id).or_default().push(tx);
    }
    let tx_index_store = gateway_eth_native_tx_index_store_backend_for_ingest();
    for (chain_id, txs) in by_chain {
        let chain_type = resolve_evm_chain_type_from_chain_id(chain_id);
        match runtime_tap_ir_batch_v1(chain_type, chain_id, txs.as_slice(), 0) {
            Ok(summary) => {
                if summary.accepted == 0 {
                    let reason = summary
                        .primary_reject_reason
                        .map(|r| r.as_str())
                        .unwrap_or("rejected");
                    if gateway_warn_enabled() {
                        eprintln!(
                            "gateway_warn: native tx ingest rejected: chain_id={} accepted=0 requested={} dropped={} reason={} tx_hash_hint=0x{}",
                            chain_id,
                            summary.requested,
                            summary.dropped,
                            reason,
                            to_hex(&tx_hash_hint),
                        );
                    }
                } else {
                    gateway_eth_native_persist_ingested_txs(
                        &tx_index_store,
                        chain_id,
                        tx_hash_hint,
                        txs.as_slice(),
                        summary.accepted,
                    );
                }
            }
            Err(rc) => {
                if gateway_warn_enabled() {
                    eprintln!(
                        "gateway_warn: native tx ingest runtime tap failed: chain_id={} rc={} tx_hash_hint=0x{}",
                        chain_id,
                        rc,
                        to_hex(&tx_hash_hint),
                    );
                }
            }
        }
    }
}

fn gateway_eth_native_tx_index_store_backend_for_ingest() -> GatewayEthTxIndexStoreBackend {
    match resolve_gateway_eth_tx_index_store_backend() {
        Ok(backend) => backend,
        Err(e) => {
            if gateway_warn_enabled() {
                eprintln!(
                    "gateway_warn: resolve tx-index backend for native ingest failed: err={}",
                    e
                );
            }
            GatewayEthTxIndexStoreBackend::Memory
        }
    }
}

fn gateway_eth_native_persist_ingested_txs(
    tx_index_store: &GatewayEthTxIndexStoreBackend,
    chain_id: u64,
    tx_hash_hint: [u8; 32],
    txs: &[TxIR],
    accepted_hint: usize,
) {
    let mut accepted = 0usize;
    for tx in txs {
        let normalized = gateway_eth_tx_ir_with_hash(tx.clone());
        let Ok(tx_hash) = vec_to_32(&normalized.hash, "native_tx_hash") else {
            if gateway_warn_enabled() {
                eprintln!(
                    "gateway_warn: native tx persist skip: invalid hash length={} chain_id={} tx_hash_hint=0x{}",
                    normalized.hash.len(),
                    chain_id,
                    to_hex(&tx_hash_hint),
                );
            }
            continue;
        };
        if find_gateway_eth_runtime_tx_by_hash(tx_hash, Some(chain_id)).is_none() {
            continue;
        }
        let entry = gateway_eth_tx_index_entry_from_ir(normalized);
        if let Err(e) = tx_index_store.save_eth_tx(&entry) {
            if gateway_warn_enabled() {
                eprintln!(
                    "gateway_warn: persist native tx-index failed: chain_id={} tx_hash=0x{} backend={} err={}",
                    chain_id,
                    to_hex(&entry.tx_hash),
                    tx_index_store.backend_name(),
                    e
                );
            }
            continue;
        }
        let _ = novovm_network::observe_network_runtime_local_head_max(entry.chain_id, entry.nonce);
        persist_gateway_eth_submit_success_status(
            tx_index_store,
            entry.tx_hash,
            entry.chain_id,
            true,
            false,
        );
        accepted = accepted.saturating_add(1);
    }
    if accepted_hint > 0 && accepted == 0 && gateway_warn_enabled() {
        eprintln!(
            "gateway_warn: native tx persist found no runtime-visible txs despite accepted_hint={} chain_id={} tx_hash_hint=0x{}",
            accepted_hint,
            chain_id,
            to_hex(&tx_hash_hint),
        );
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
static GATEWAY_ETH_PLUGIN_PROBE_CACHE: OnceLock<DashMap<String, GatewayEthPluginProbeCacheEntry>> =
    OnceLock::new();
static GATEWAY_ETH_PLUGIN_SESSION_CACHE: OnceLock<DashMap<String, GatewayEthPluginSessionState>> =
    OnceLock::new();
static GATEWAY_ETH_PLUGIN_MEMPOOL_WORKER_STARTED: OnceLock<Mutex<std::collections::HashSet<u64>>> =
    OnceLock::new();
static GATEWAY_ETH_PLUGIN_MEMPOOL_STATE: OnceLock<DashMap<u64, GatewayEthPluginMempoolState>> =
    OnceLock::new();
static GATEWAY_ETH_PLUGIN_RLPX_WORKER_STARTED: OnceLock<Mutex<std::collections::HashSet<String>>> =
    OnceLock::new();
static GATEWAY_ETH_PLUGIN_RLPX_WORKER_STATE: OnceLock<
    DashMap<String, GatewayEthPluginRlpxWorkerState>,
> = OnceLock::new();
#[derive(Clone, Copy, Debug, Hash, PartialEq, Eq)]
struct GatewayEthPluginRlpxSeenKey {
    chain_id: u64,
    hash: [u8; 32],
}
static GATEWAY_ETH_PLUGIN_RLPX_SEEN_HASHES: OnceLock<DashMap<GatewayEthPluginRlpxSeenKey, u64>> =
    OnceLock::new();
static GATEWAY_ETH_PLUGIN_RLPX_SEEN_TXS: OnceLock<DashMap<GatewayEthPluginRlpxSeenKey, u64>> =
    OnceLock::new();
static GATEWAY_ETH_PLUGIN_RLPX_SEEN_LAST_GC: OnceLock<DashMap<u64, u64>> = OnceLock::new();
static GATEWAY_ETH_PLUGIN_RLPX_PROFILE_PERSIST_STATE: OnceLock<
    DashMap<u64, GatewayEthPluginRlpxProfilePersistState>,
> = OnceLock::new();
static GATEWAY_ETH_PLUGIN_RLPX_CORE_FALLBACK_STATE: OnceLock<
    DashMap<u64, GatewayEthPluginRlpxCoreFallbackState>,
> = OnceLock::new();
static GATEWAY_ETH_PLUGIN_RLPX_PROFILE_LOADED_CHAINS: OnceLock<
    Mutex<std::collections::HashSet<u64>>,
> = OnceLock::new();
static GATEWAY_ETH_NATIVE_RUNTIME_WORKER_STARTED: OnceLock<
    Mutex<std::collections::HashSet<String>>,
> = OnceLock::new();
static GATEWAY_ETH_NATIVE_SYNC_PULL_TRACKER: OnceLock<
    DashMap<String, GatewayEthNativeSyncPullState>,
> = OnceLock::new();
static GATEWAY_ETH_PLUGIN_RLPX_REQUEST_ID: std::sync::atomic::AtomicU64 =
    std::sync::atomic::AtomicU64::new(1);
static GATEWAY_ETH_PLUGIN_RLPX_SELECT_ROTATION: std::sync::atomic::AtomicU64 =
    std::sync::atomic::AtomicU64::new(0);
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
const GATEWAY_ETH_NATIVE_SYNC_PULL_FANOUT_CAP_HARD_MAX: u64 = 1_024;
const GATEWAY_ETH_NATIVE_SYNC_PULL_SEGMENTS_CAP_HARD_MAX: u64 = 1_024;
const GATEWAY_ETH_NATIVE_SYNC_PULL_SEGMENT_MIN_BLOCKS_DEFAULT: u64 = 1;
const GATEWAY_ETH_NATIVE_SYNC_PULL_SEGMENT_MIN_BLOCKS_HARD_MAX: u64 = 1_000_000;
const GATEWAY_ETH_NATIVE_PARALLELISM_HARD_MAX: usize = 64;
const GATEWAY_ETH_PLUGIN_PROBE_TIMEOUT_MS_DEFAULT: u64 = 500;
const GATEWAY_ETH_PLUGIN_PROBE_TIMEOUT_MS_MIN: u64 = 50;
const GATEWAY_ETH_PLUGIN_PROBE_TIMEOUT_MS_MAX: u64 = 60_000;
const GATEWAY_ETH_PLUGIN_PROBE_CACHE_TTL_MS_DEFAULT: u64 = 3_000;
const GATEWAY_ETH_PLUGIN_PROBE_CACHE_TTL_MS_MIN: u64 = 100;
const GATEWAY_ETH_PLUGIN_PROBE_CACHE_TTL_MS_MAX: u64 = 60_000;
const GATEWAY_ETH_PLUGIN_SESSION_PROBE_PAYLOAD_MAX: usize = 160;
const GATEWAY_ETH_PLUGIN_SESSION_PROBE_READ_MAX: usize = 128;
const GATEWAY_ETH_PLUGIN_RLPX_HANDSHAKE_MAX_BYTES: usize = 2_048;
const GATEWAY_ETH_PLUGIN_RLPX_ECIES_IV_LEN: usize = 16;
const GATEWAY_ETH_PLUGIN_RLPX_ECIES_MAC_LEN: usize = 32;
const GATEWAY_ETH_PLUGIN_RLPX_ECIES_PUB_LEN: usize = 65;
const GATEWAY_ETH_PLUGIN_RLPX_ECIES_OVERHEAD: usize = GATEWAY_ETH_PLUGIN_RLPX_ECIES_PUB_LEN
    + GATEWAY_ETH_PLUGIN_RLPX_ECIES_IV_LEN
    + GATEWAY_ETH_PLUGIN_RLPX_ECIES_MAC_LEN;
const GATEWAY_ETH_PLUGIN_RLPX_SIG_LEN: usize = 65;
const GATEWAY_ETH_PLUGIN_RLPX_PUB_LEN: usize = 64;
const GATEWAY_ETH_PLUGIN_RLPX_NONCE_LEN: usize = 32;
const GATEWAY_ETH_PLUGIN_RLPX_FRAME_HEADER_LEN: usize = 16;
const GATEWAY_ETH_PLUGIN_RLPX_FRAME_HEADER_MAC_LEN: usize = 16;
const GATEWAY_ETH_PLUGIN_RLPX_FRAME_MAC_LEN: usize = 16;
const GATEWAY_ETH_PLUGIN_RLPX_FRAME_MAX_SIZE: usize = (1 << 24) - 1;
const GATEWAY_ETH_PLUGIN_RLPX_P2P_HELLO_MSG: u64 = 0x00;
const GATEWAY_ETH_PLUGIN_RLPX_P2P_DISCONNECT_MSG: u64 = 0x01;
const GATEWAY_ETH_PLUGIN_RLPX_P2P_PING_MSG: u64 = 0x02;
const GATEWAY_ETH_PLUGIN_RLPX_P2P_PONG_MSG: u64 = 0x03;
const GATEWAY_ETH_PLUGIN_RLPX_BASE_PROTOCOL_OFFSET: u64 = 0x10;
const GATEWAY_ETH_PLUGIN_RLPX_ZERO_HEADER: [u8; 3] = [0xC2, 0x80, 0x80];
const GATEWAY_ETH_PLUGIN_RLPX_ETH_PROTO_69: u64 = 69;
const GATEWAY_ETH_PLUGIN_RLPX_ETH_PROTO_68: u64 = 68;
const GATEWAY_ETH_PLUGIN_RLPX_ETH_PROTO_67: u64 = 67;
const GATEWAY_ETH_PLUGIN_RLPX_ETH_PROTO_66: u64 = 66;
const GATEWAY_ETH_PLUGIN_RLPX_ETH_PROTO_LEN_69: u64 = 18;
const GATEWAY_ETH_PLUGIN_RLPX_ETH_PROTO_LEN_66_68: u64 = 17;
const GATEWAY_ETH_PLUGIN_RLPX_ETH_STATUS_MSG: u64 = 0x00;
const GATEWAY_ETH_PLUGIN_RLPX_ETH_TRANSACTIONS_MSG: u64 = 0x02;
const GATEWAY_ETH_PLUGIN_RLPX_ETH_NEW_POOLED_HASHES_MSG: u64 = 0x08;
const GATEWAY_ETH_PLUGIN_RLPX_ETH_GET_POOLED_MSG: u64 = 0x09;
const GATEWAY_ETH_PLUGIN_RLPX_ETH_POOLED_MSG: u64 = 0x0a;
const GATEWAY_ETH_PLUGIN_RLPX_HANDSHAKE_TIMEOUT_MS_DEFAULT: u64 = 12_000;
const GATEWAY_ETH_PLUGIN_RLPX_READ_WINDOW_MS_DEFAULT: u64 = 1_500;
const GATEWAY_ETH_PLUGIN_RLPX_READ_WINDOW_MS_MIN: u64 = 100;
const GATEWAY_ETH_PLUGIN_RLPX_READ_WINDOW_MS_MAX: u64 = 20_000;
const GATEWAY_ETH_PLUGIN_RLPX_MAX_PEERS_PER_TICK_DEFAULT: usize = 8;
const GATEWAY_ETH_PLUGIN_RLPX_MAX_PEERS_PER_TICK_HARD_MAX: usize = 32;
const GATEWAY_ETH_PLUGIN_RLPX_CORE_TARGET_DEFAULT: usize = 8;
const GATEWAY_ETH_PLUGIN_RLPX_ACTIVE_TARGET_DEFAULT: usize = 24;
const GATEWAY_ETH_PLUGIN_RLPX_TIER_TARGET_HARD_MAX: usize = 256;
const GATEWAY_ETH_PLUGIN_RLPX_CORE_STABLE_FLOOR_DEFAULT: usize = 3;
const GATEWAY_ETH_PLUGIN_RLPX_CORE_FALLBACK_TRIGGER_TICKS_DEFAULT: u64 = 6;
const GATEWAY_ETH_PLUGIN_RLPX_CORE_FALLBACK_HOLD_TICKS_DEFAULT: u64 = 12;
const GATEWAY_ETH_PLUGIN_RLPX_CORE_FALLBACK_MAX_PEERS_BONUS_DEFAULT: usize = 8;
const GATEWAY_ETH_PLUGIN_RLPX_CORE_FALLBACK_CANDIDATE_BUDGET_MIN_DEFAULT: usize = 8;
const GATEWAY_ETH_PLUGIN_RLPX_CORE_FALLBACK_DEMOTE_TOO_MANY_MIN_DEFAULT: u64 = 8;
const GATEWAY_ETH_PLUGIN_RLPX_CORE_RECENT_GOSSIP_WINDOW_MS_DEFAULT: u64 = 10 * 60 * 1_000;
const GATEWAY_ETH_PLUGIN_RLPX_ACTIVE_RECENT_READY_WINDOW_MS_DEFAULT: u64 = 5 * 60 * 1_000;
const GATEWAY_ETH_PLUGIN_RLPX_CORE_LOCK_MS_DEFAULT: u64 = 10 * 60 * 1_000;
const GATEWAY_ETH_PLUGIN_RLPX_RECENT_NEW_POOLED_HASH_WINDOW_MS_DEFAULT: u64 = 10 * 60 * 1_000;
const GATEWAY_ETH_PLUGIN_RLPX_RECENT_NEW_POOLED_HASH_MIN_DEFAULT: u64 = 64;
const GATEWAY_ETH_PLUGIN_RLPX_SWAP_PRIORITY_LATENCY_TARGET_MS_DEFAULT: u64 = 1_000;
const GATEWAY_ETH_PLUGIN_RLPX_SWAP_PRIORITY_LATENCY_TARGET_MS_MIN: u64 = 50;
const GATEWAY_ETH_PLUGIN_RLPX_SWAP_PRIORITY_LATENCY_TARGET_MS_MAX: u64 = 30_000;
const GATEWAY_ETH_PLUGIN_RLPX_PRIORITY_BUDGET_DEFAULT: usize = 0;
const GATEWAY_ETH_PLUGIN_RLPX_PRIORITY_BUDGET_HARD_MAX: usize = 64;
const GATEWAY_ETH_PLUGIN_RLPX_PRIORITY_AUTO_POOL_SIZE_DEFAULT: usize = 0;
const GATEWAY_ETH_PLUGIN_RLPX_PRIORITY_AUTO_POOL_SIZE_HARD_MAX: usize = 64;
const GATEWAY_ETH_PLUGIN_RLPX_MAX_HASHES_PER_REQUEST_DEFAULT: usize = 64;
const GATEWAY_ETH_PLUGIN_RLPX_MAX_HASHES_PER_REQUEST_HARD_MAX: usize = 512;
const GATEWAY_ETH_PLUGIN_RLPX_SEEN_GC_INTERVAL_MS: u64 = 30_000;
const GATEWAY_ETH_PLUGIN_RLPX_SEEN_RETENTION_MULTIPLIER: u64 = 6;
const GATEWAY_ETH_PLUGIN_RLPX_WORKER_READ_WINDOW_MS_MIN: u64 = 60_000;
const GATEWAY_ETH_PLUGIN_RLPX_WORKER_FAIL_BACKOFF_BASE_MS: u64 = 500;
const GATEWAY_ETH_PLUGIN_RLPX_WORKER_FAIL_BACKOFF_MAX_MS: u64 = 30_000;
const GATEWAY_ETH_PLUGIN_RLPX_WORKER_FAIL_BACKOFF_SHIFT_CAP: u32 = 8;
const GATEWAY_ETH_PLUGIN_RLPX_WORKER_BACKOFF_CORE_MIN_MS: u64 = 500;
const GATEWAY_ETH_PLUGIN_RLPX_WORKER_BACKOFF_CORE_MAX_MS: u64 = 10_000;
const GATEWAY_ETH_PLUGIN_RLPX_WORKER_BACKOFF_ACTIVE_MIN_MS: u64 = 2_000;
const GATEWAY_ETH_PLUGIN_RLPX_WORKER_BACKOFF_ACTIVE_MAX_MS: u64 = 20_000;
const GATEWAY_ETH_PLUGIN_RLPX_WORKER_BACKOFF_CANDIDATE_MIN_MS: u64 = 10_000;
const GATEWAY_ETH_PLUGIN_RLPX_WORKER_BACKOFF_CANDIDATE_MAX_MS: u64 =
    GATEWAY_ETH_PLUGIN_RLPX_WORKER_FAIL_BACKOFF_MAX_MS;
const GATEWAY_ETH_PLUGIN_RLPX_WORKER_TIMEOUT_BACKOFF_CORE_MIN_MS: u64 = 2_000;
const GATEWAY_ETH_PLUGIN_RLPX_WORKER_TIMEOUT_BACKOFF_ACTIVE_MIN_MS: u64 = 5_000;
const GATEWAY_ETH_PLUGIN_RLPX_WORKER_TIMEOUT_BACKOFF_CANDIDATE_MIN_MS: u64 = 12_000;
const GATEWAY_ETH_PLUGIN_RLPX_PROFILE_FLUSH_INTERVAL_MS_DEFAULT: u64 = 5_000;
const GATEWAY_ETH_PLUGIN_RLPX_PROFILE_FLUSH_INTERVAL_MS_MIN: u64 = 500;
const GATEWAY_ETH_PLUGIN_RLPX_PROFILE_FLUSH_INTERVAL_MS_MAX: u64 = 60_000;
const GATEWAY_ETH_PLUGIN_MIN_CANDIDATES_DEFAULT: usize = 24;
const GATEWAY_ETH_PLUGIN_MEMPOOL_INGEST_POLL_MS_DEFAULT: u64 = 1_500;
const GATEWAY_ETH_PLUGIN_MEMPOOL_INGEST_POLL_MS_MIN: u64 = 200;
const GATEWAY_ETH_PLUGIN_MEMPOOL_INGEST_POLL_MS_MAX: u64 = 60_000;
const GATEWAY_ETH_PLUGIN_MEMPOOL_INGEST_STALE_TTL_MS_DEFAULT: u64 = 30 * 60 * 1_000;
const GATEWAY_ETH_PLUGIN_MEMPOOL_INGEST_STALE_TTL_MS_MAX: u64 = 24 * 60 * 60 * 1_000;
const GATEWAY_ETH_UNISWAP_V2_ROUTER: [u8; 20] = [
    0x7a, 0x25, 0x0d, 0x56, 0x30, 0xb4, 0xcf, 0x53, 0x97, 0x39, 0xdf, 0x2c, 0x5d, 0xac, 0xb4, 0xc6,
    0x59, 0xf2, 0x48, 0x8d,
];
const GATEWAY_ETH_UNISWAP_V3_ROUTER: [u8; 20] = [
    0xe5, 0x92, 0x42, 0x7a, 0x0a, 0xec, 0xe9, 0x2d, 0xe3, 0xed, 0xee, 0x1f, 0x18, 0xe0, 0x15, 0x7c,
    0x05, 0x86, 0x15, 0x64,
];
const GATEWAY_ETH_UNISWAP_V3_ROUTER_02: [u8; 20] = [
    0x68, 0xb3, 0x46, 0x58, 0x33, 0xfb, 0x72, 0xa7, 0x0e, 0xcd, 0xf4, 0x85, 0xe0, 0xe4, 0xc7, 0xbd,
    0x86, 0x65, 0xfc, 0x45,
];
const GATEWAY_ETH_UNISWAP_UNIVERSAL_ROUTER: [u8; 20] = [
    0xef, 0x1c, 0x6e, 0x67, 0x70, 0x3c, 0x7b, 0xd7, 0x10, 0x7e, 0xed, 0x83, 0x03, 0xfb, 0xe6, 0xec,
    0x25, 0x54, 0xbf, 0x6b,
];
const GATEWAY_ETH_UNISWAP_V2_SWAP_SELECTORS: [[u8; 4]; 9] = [
    [0x38, 0xed, 0x17, 0x39], // swapExactTokensForTokens
    [0x88, 0x03, 0xdb, 0xee], // swapTokensForExactTokens
    [0x7f, 0xf3, 0x6a, 0xb5], // swapExactETHForTokens
    [0x4a, 0x25, 0xd9, 0x4a], // swapTokensForExactETH
    [0x18, 0xcb, 0xaf, 0xe5], // swapExactTokensForETH
    [0xfb, 0x3b, 0xdb, 0x41], // swapETHForExactTokens
    [0x5c, 0x11, 0xd7, 0x95], // supporting fee on transfer variant
    [0x79, 0x1a, 0xc9, 0x47], // supporting fee on transfer variant
    [0xb6, 0xf9, 0xde, 0x95], // supporting fee on transfer variant
];
const GATEWAY_ETH_UNISWAP_V3_SWAP_SELECTORS: [[u8; 4]; 7] = [
    [0x41, 0x4b, 0xf3, 0x89], // exactInputSingle
    [0xc0, 0x4b, 0x8d, 0x59], // exactInput
    [0xdb, 0x3e, 0x21, 0x98], // exactOutputSingle
    [0xf2, 0x8c, 0x04, 0x98], // exactOutput
    [0x35, 0x93, 0x56, 0x4c], // universal router execute
    [0x24, 0x85, 0x6b, 0xc3], // universal router execute(bytes,bytes[])
    [0xac, 0x96, 0x50, 0xd8], // multicall
];

#[derive(Clone, Default)]
struct GatewayEthPluginMempoolState {
    running: bool,
    endpoints: u64,
    tick_count: u64,
    imported_total: u64,
    imported_last_tick: u64,
    evicted_total: u64,
    evicted_last_tick: u64,
    evicted_confirmed_total: u64,
    evicted_confirmed_last_tick: u64,
    evicted_stale_total: u64,
    evicted_stale_last_tick: u64,
    last_tick_ms: u64,
    last_success_ms: u64,
    last_error: Option<String>,
}

#[derive(Clone, Default)]
struct GatewayEthPluginRlpxWorkerState {
    running: bool,
    last_attempt_ms: u64,
    last_success_ms: u64,
    last_error_ms: u64,
    consecutive_failures: u32,
    consecutive_successes: u32,
    cooldown_until_ms: u64,
    last_error: Option<String>,
    dial_attempt_count: u64,
    disconnect_count: u64,
    disconnect_too_many_count: u64,
    disconnect_timeout_count: u64,
    disconnect_protocol_count: u64,
    disconnect_other_count: u64,
    sessions_completed: u64,
    sessions_with_gossip: u64,
    total_new_pooled_msgs: u64,
    total_new_pooled_hashes: u64,
    total_unique_new_pooled_hashes: u64,
    total_duplicate_new_pooled_hashes: u64,
    total_get_pooled_sent: u64,
    total_pooled_msgs: u64,
    total_pooled_txs_received: u64,
    total_unique_pooled_txs: u64,
    total_duplicate_pooled_txs: u64,
    total_pooled_txs_imported: u64,
    total_txs_msgs: u64,
    first_seen_hash_count: u64,
    first_seen_tx_count: u64,
    total_swap_hits: u64,
    total_swap_v2_hits: u64,
    total_swap_v3_hits: u64,
    total_unique_swap_hits: u64,
    last_new_pooled_ms: u64,
    last_swap_ms: u64,
    recent_new_pooled_hashes_total: u64,
    recent_new_pooled_hashes_window_start_ms: u64,
    recent_unique_new_pooled_hashes_total: u64,
    recent_unique_pooled_txs_total: u64,
    recent_duplicate_new_pooled_hashes_total: u64,
    recent_duplicate_pooled_txs_total: u64,
    recent_swap_hits_total: u64,
    recent_unique_swap_hits_total: u64,
    recent_swap_window_start_ms: u64,
    recent_dedup_window_start_ms: u64,
    total_first_gossip_latency_ms: u64,
    first_gossip_latency_samples: u64,
    total_first_swap_latency_ms: u64,
    first_swap_latency_samples: u64,
    last_first_post_ready_code: u64,
    learning_score: u64,
    last_sample_score: u64,
}

#[derive(Clone, Default)]
struct GatewayEthPluginRlpxProfilePersistState {
    dirty: bool,
    last_flush_ms: u64,
}

#[derive(Clone, Default)]
struct GatewayEthPluginRlpxCoreFallbackState {
    low_core_streak: u64,
    hold_ticks_remaining: u64,
    activation_count: u64,
}

#[derive(Clone, Copy, Default)]
struct GatewayEthPluginRlpxCoreFallbackTickState {
    active: bool,
    low_core_streak: u64,
    hold_ticks_remaining: u64,
    activation_count: u64,
}

#[derive(Clone, Default)]
struct GatewayEthPluginRlpxSessionMetrics {
    first_post_ready_code: Option<u64>,
    new_pooled_msgs: u64,
    new_pooled_hashes: u64,
    unique_new_pooled_hashes: u64,
    duplicate_new_pooled_hashes: u64,
    get_pooled_sent: u64,
    pooled_msgs: u64,
    pooled_txs_received: u64,
    unique_pooled_txs: u64,
    duplicate_pooled_txs: u64,
    pooled_txs_imported: u64,
    first_seen_hashes: u64,
    first_seen_txs: u64,
    swap_hits: u64,
    swap_v2_hits: u64,
    swap_v3_hits: u64,
    unique_swap_hits: u64,
    first_gossip_latency_ms: u64,
    first_swap_latency_ms: u64,
    txs_msgs: u64,
}

#[derive(Clone, Default, serde::Serialize, serde::Deserialize)]
#[serde(default)]
struct GatewayEthPluginRlpxWorkerStatePersisted {
    endpoint: String,
    last_success_ms: u64,
    dial_attempt_count: u64,
    disconnect_count: u64,
    disconnect_too_many_count: u64,
    disconnect_timeout_count: u64,
    disconnect_protocol_count: u64,
    disconnect_other_count: u64,
    sessions_completed: u64,
    sessions_with_gossip: u64,
    total_new_pooled_msgs: u64,
    total_new_pooled_hashes: u64,
    total_unique_new_pooled_hashes: u64,
    total_duplicate_new_pooled_hashes: u64,
    total_get_pooled_sent: u64,
    total_pooled_msgs: u64,
    total_pooled_txs_received: u64,
    total_unique_pooled_txs: u64,
    total_duplicate_pooled_txs: u64,
    total_pooled_txs_imported: u64,
    total_txs_msgs: u64,
    first_seen_hash_count: u64,
    first_seen_tx_count: u64,
    total_swap_hits: u64,
    total_swap_v2_hits: u64,
    total_swap_v3_hits: u64,
    total_unique_swap_hits: u64,
    last_new_pooled_ms: u64,
    last_swap_ms: u64,
    recent_new_pooled_hashes_total: u64,
    recent_new_pooled_hashes_window_start_ms: u64,
    recent_unique_new_pooled_hashes_total: u64,
    recent_unique_pooled_txs_total: u64,
    recent_duplicate_new_pooled_hashes_total: u64,
    recent_duplicate_pooled_txs_total: u64,
    recent_swap_hits_total: u64,
    recent_unique_swap_hits_total: u64,
    recent_swap_window_start_ms: u64,
    recent_dedup_window_start_ms: u64,
    total_first_gossip_latency_ms: u64,
    first_gossip_latency_samples: u64,
    total_first_swap_latency_ms: u64,
    first_swap_latency_samples: u64,
    last_first_post_ready_code: u64,
    learning_score: u64,
    last_sample_score: u64,
}

impl GatewayEthPluginRlpxWorkerStatePersisted {
    fn from_runtime(endpoint: String, state: &GatewayEthPluginRlpxWorkerState) -> Self {
        Self {
            endpoint,
            last_success_ms: state.last_success_ms,
            dial_attempt_count: state.dial_attempt_count,
            disconnect_count: state.disconnect_count,
            disconnect_too_many_count: state.disconnect_too_many_count,
            disconnect_timeout_count: state.disconnect_timeout_count,
            disconnect_protocol_count: state.disconnect_protocol_count,
            disconnect_other_count: state.disconnect_other_count,
            sessions_completed: state.sessions_completed,
            sessions_with_gossip: state.sessions_with_gossip,
            total_new_pooled_msgs: state.total_new_pooled_msgs,
            total_new_pooled_hashes: state.total_new_pooled_hashes,
            total_unique_new_pooled_hashes: state.total_unique_new_pooled_hashes,
            total_duplicate_new_pooled_hashes: state.total_duplicate_new_pooled_hashes,
            total_get_pooled_sent: state.total_get_pooled_sent,
            total_pooled_msgs: state.total_pooled_msgs,
            total_pooled_txs_received: state.total_pooled_txs_received,
            total_unique_pooled_txs: state.total_unique_pooled_txs,
            total_duplicate_pooled_txs: state.total_duplicate_pooled_txs,
            total_pooled_txs_imported: state.total_pooled_txs_imported,
            total_txs_msgs: state.total_txs_msgs,
            first_seen_hash_count: state.first_seen_hash_count,
            first_seen_tx_count: state.first_seen_tx_count,
            total_swap_hits: state.total_swap_hits,
            total_swap_v2_hits: state.total_swap_v2_hits,
            total_swap_v3_hits: state.total_swap_v3_hits,
            total_unique_swap_hits: state.total_unique_swap_hits,
            last_new_pooled_ms: state.last_new_pooled_ms,
            last_swap_ms: state.last_swap_ms,
            recent_new_pooled_hashes_total: state.recent_new_pooled_hashes_total,
            recent_new_pooled_hashes_window_start_ms: state
                .recent_new_pooled_hashes_window_start_ms,
            recent_unique_new_pooled_hashes_total: state.recent_unique_new_pooled_hashes_total,
            recent_unique_pooled_txs_total: state.recent_unique_pooled_txs_total,
            recent_duplicate_new_pooled_hashes_total: state
                .recent_duplicate_new_pooled_hashes_total,
            recent_duplicate_pooled_txs_total: state.recent_duplicate_pooled_txs_total,
            recent_swap_hits_total: state.recent_swap_hits_total,
            recent_unique_swap_hits_total: state.recent_unique_swap_hits_total,
            recent_swap_window_start_ms: state.recent_swap_window_start_ms,
            recent_dedup_window_start_ms: state.recent_dedup_window_start_ms,
            total_first_gossip_latency_ms: state.total_first_gossip_latency_ms,
            first_gossip_latency_samples: state.first_gossip_latency_samples,
            total_first_swap_latency_ms: state.total_first_swap_latency_ms,
            first_swap_latency_samples: state.first_swap_latency_samples,
            last_first_post_ready_code: state.last_first_post_ready_code,
            learning_score: state.learning_score,
            last_sample_score: state.last_sample_score,
        }
    }

    fn into_runtime(self) -> GatewayEthPluginRlpxWorkerState {
        GatewayEthPluginRlpxWorkerState {
            running: false,
            last_attempt_ms: 0,
            last_success_ms: self.last_success_ms,
            last_error_ms: 0,
            consecutive_failures: 0,
            consecutive_successes: 0,
            cooldown_until_ms: 0,
            last_error: None,
            dial_attempt_count: self.dial_attempt_count,
            disconnect_count: self.disconnect_count,
            disconnect_too_many_count: self.disconnect_too_many_count,
            disconnect_timeout_count: self.disconnect_timeout_count,
            disconnect_protocol_count: self.disconnect_protocol_count,
            disconnect_other_count: self.disconnect_other_count,
            sessions_completed: self.sessions_completed,
            sessions_with_gossip: self.sessions_with_gossip,
            total_new_pooled_msgs: self.total_new_pooled_msgs,
            total_new_pooled_hashes: self.total_new_pooled_hashes,
            total_unique_new_pooled_hashes: self.total_unique_new_pooled_hashes,
            total_duplicate_new_pooled_hashes: self.total_duplicate_new_pooled_hashes,
            total_get_pooled_sent: self.total_get_pooled_sent,
            total_pooled_msgs: self.total_pooled_msgs,
            total_pooled_txs_received: self.total_pooled_txs_received,
            total_unique_pooled_txs: self.total_unique_pooled_txs,
            total_duplicate_pooled_txs: self.total_duplicate_pooled_txs,
            total_pooled_txs_imported: self.total_pooled_txs_imported,
            total_txs_msgs: self.total_txs_msgs,
            first_seen_hash_count: self.first_seen_hash_count,
            first_seen_tx_count: self.first_seen_tx_count,
            total_swap_hits: self.total_swap_hits,
            total_swap_v2_hits: self.total_swap_v2_hits,
            total_swap_v3_hits: self.total_swap_v3_hits,
            total_unique_swap_hits: self.total_unique_swap_hits,
            last_new_pooled_ms: self.last_new_pooled_ms,
            last_swap_ms: self.last_swap_ms,
            recent_new_pooled_hashes_total: self.recent_new_pooled_hashes_total,
            recent_new_pooled_hashes_window_start_ms: self.recent_new_pooled_hashes_window_start_ms,
            recent_unique_new_pooled_hashes_total: self.recent_unique_new_pooled_hashes_total,
            recent_unique_pooled_txs_total: self.recent_unique_pooled_txs_total,
            recent_duplicate_new_pooled_hashes_total: self.recent_duplicate_new_pooled_hashes_total,
            recent_duplicate_pooled_txs_total: self.recent_duplicate_pooled_txs_total,
            recent_swap_hits_total: self.recent_swap_hits_total,
            recent_unique_swap_hits_total: self.recent_unique_swap_hits_total,
            recent_swap_window_start_ms: self.recent_swap_window_start_ms,
            recent_dedup_window_start_ms: self.recent_dedup_window_start_ms,
            total_first_gossip_latency_ms: self.total_first_gossip_latency_ms,
            first_gossip_latency_samples: self.first_gossip_latency_samples,
            total_first_swap_latency_ms: self.total_first_swap_latency_ms,
            first_swap_latency_samples: self.first_swap_latency_samples,
            last_first_post_ready_code: self.last_first_post_ready_code,
            learning_score: self.learning_score,
            last_sample_score: self.last_sample_score,
        }
    }
}

#[derive(Clone, Default, serde::Serialize, serde::Deserialize)]
#[serde(default)]
struct GatewayEthPluginRlpxWorkerStatePersistedFile {
    version: u64,
    chain_id: u64,
    updated_ms: u64,
    #[serde(alias = "items")]
    workers: Vec<GatewayEthPluginRlpxWorkerStatePersisted>,
}
const GATEWAY_ETH_MAINNET_BOOTNODES: [&str; 4] = [
    "enode://d860a01f9722d78051619d1e2351aba3f43f943f6f00718d1b9baa4101932a1f5011f16bb2b1bb35db20d6fe28fa0bf09636d26a87d31de9ec6203eeedb1f666@18.138.108.67:30303",
    "enode://22a8232c3abc76a16ae9d6c3b164f98775fe226f0917b0ca871128a74a8e9630b458460865bab457221f1d448dd9791d24c4e5d88786180ac185df813a68d4de@3.209.45.79:30303",
    "enode://2b252ab6a1d0f971d9722cb839a42cb81db019ba44c08754628ab4a823487071b5695317c8ccd085219c3a03af063495b2f1da8d18218da2d6a82981b45e6ffc@65.108.70.101:30303",
    "enode://4aeb4ab6c14b23e2c4cfdce879c04b0748a20d8e9b59e25ded2a08143e265c6c25936e74cbc8e641e3312ca288673d91f2f93f8e277de3cfa444ecdaaf982052@157.90.35.166:30303",
];
const GATEWAY_ETH_SEPOLIA_BOOTNODES: [&str; 5] = [
    "enode://4e5e92199ee224a01932a377160aa432f31d0b351f84ab413a8e0a42f4f36476f8fb1cbe914af0d9aef0d51665c214cf653c651c4bbd9d5550a934f241f1682b@138.197.51.181:30303",
    "enode://143e11fb766781d22d92a2e33f8f104cddae4411a122295ed1fdb6638de96a6ce65f5b7c964ba3763bba27961738fef7d3ecc739268f3e5e771fb4c87b6234ba@146.190.1.103:30303",
    "enode://8b61dc2d06c3f96fddcbebb0efb29d60d3598650275dc469c22229d3e5620369b0d3dedafd929835fe7f489618f19f456fe7c0df572bf2d914a9f4e006f783a9@170.64.250.88:30303",
    "enode://10d62eff032205fcef19497f35ca8477bea0eadfff6d769a147e895d8b2b8f8ae6341630c645c30f5df6e67547c03494ced3d9c5764e8622a26587b083b028e8@139.59.49.206:30303",
    "enode://9e9492e2e8836114cc75f5b929784f4f46c324ad01daf87d956f98b3b6c5fcba95524d6e5cf9861dc96a2c8a171ea7105bb554a197455058de185fa870970c7c@138.68.123.152:30303",
];
const GATEWAY_ETH_HOLESKY_BOOTNODES: [&str; 2] = [
    "enode://ac906289e4b7f12df423d654c5a962b6ebe5b3a74cc9e06292a85221f9a64a6f1cfdd6b714ed6dacef51578f92b34c60ee91e9ede9c7f8fadc4d347326d95e2b@146.190.13.128:30303",
    "enode://a3435a0155a3e837c02f5e7f5662a2f1fbc25b48e4dc232016e1c51b544cb5b4510ef633ea3278c0e970fa8ad8141e2d4d0f9f95456c537ff05fdf9b31c15072@178.128.136.233:30303",
];

fn gateway_eth_native_parallelism() -> usize {
    let default = std::thread::available_parallelism()
        .map(|n| n.get())
        .unwrap_or(4)
        .clamp(1, GATEWAY_ETH_NATIVE_PARALLELISM_HARD_MAX);
    string_env_nonempty("NOVOVM_GATEWAY_ETH_NATIVE_PARALLELISM")
        .and_then(|raw| raw.trim().parse::<usize>().ok())
        .map(|v| v.clamp(1, GATEWAY_ETH_NATIVE_PARALLELISM_HARD_MAX))
        .unwrap_or(default)
}

fn gateway_eth_native_register_peers_parallel(
    broadcaster: &GatewayEthNativeBroadcaster,
    peers: &[(NodeId, String)],
    failed: &mut u64,
    errors: &mut Vec<String>,
) {
    if peers.is_empty() {
        return;
    }
    let parallelism = gateway_eth_native_parallelism().min(peers.len()).max(1);
    if parallelism == 1 {
        for (peer, addr) in peers {
            if let Err(e) = broadcaster.register_peer(*peer, addr.as_str()) {
                *failed = failed.saturating_add(1);
                errors.push(format!("register_peer({}:{})={}", peer.0, addr, e));
            }
        }
        return;
    }
    for batch in peers.chunks(parallelism) {
        std::thread::scope(|scope| {
            let mut handles = Vec::with_capacity(batch.len());
            for (peer, addr) in batch.iter() {
                let broadcaster = broadcaster.clone();
                let peer = *peer;
                let addr = addr.clone();
                handles.push(scope.spawn(move || {
                    let res = broadcaster.register_peer(peer, addr.as_str());
                    (peer, addr, res)
                }));
            }
            for handle in handles {
                if let Ok((peer, addr, Err(e))) = handle.join() {
                    *failed = failed.saturating_add(1);
                    errors.push(format!("register_peer({}:{})={}", peer.0, addr, e));
                }
            }
        });
    }
}

struct GatewayEthNativeSendOutcome<'a> {
    success: &'a mut u64,
    failed: &'a mut u64,
    errors: &'a mut Vec<String>,
}

fn gateway_eth_native_send_parallel(
    broadcaster: &GatewayEthNativeBroadcaster,
    peers: &[(NodeId, String)],
    local_node: NodeId,
    chain_id: u64,
    tx_hash: [u8; 32],
    payload_bytes: &[u8],
    outcome: &mut GatewayEthNativeSendOutcome<'_>,
) {
    if peers.is_empty() {
        return;
    }
    let parallelism = gateway_eth_native_parallelism().min(peers.len()).max(1);
    if parallelism == 1 {
        for (peer, _addr) in peers {
            let msg = ProtocolMessage::EvmNative(EvmNativeMessage::Transactions {
                from: local_node,
                chain_id,
                tx_hash,
                tx_count: 1,
                payload: payload_bytes.to_vec(),
            });
            match broadcaster.send(*peer, msg) {
                Ok(_) => *outcome.success = outcome.success.saturating_add(1),
                Err(e) => {
                    *outcome.failed = outcome.failed.saturating_add(1);
                    outcome.errors.push(format!("send({})={}", peer.0, e));
                }
            }
        }
        return;
    }
    for batch in peers.chunks(parallelism) {
        std::thread::scope(|scope| {
            let mut handles = Vec::with_capacity(batch.len());
            for (peer, _addr) in batch.iter() {
                let broadcaster = broadcaster.clone();
                let peer = *peer;
                let msg = ProtocolMessage::EvmNative(EvmNativeMessage::Transactions {
                    from: local_node,
                    chain_id,
                    tx_hash,
                    tx_count: 1,
                    payload: payload_bytes.to_vec(),
                });
                handles.push(scope.spawn(move || {
                    let res = broadcaster.send(peer, msg);
                    (peer, res)
                }));
            }
            for handle in handles {
                if let Ok((peer, res)) = handle.join() {
                    match res {
                        Ok(_) => *outcome.success = outcome.success.saturating_add(1),
                        Err(e) => {
                            *outcome.failed = outcome.failed.saturating_add(1);
                            outcome.errors.push(format!("send({})={}", peer.0, e));
                        }
                    }
                }
            }
        });
    }
}

fn gateway_eth_native_send_discovery_bundle_parallel(
    broadcaster: &GatewayEthNativeBroadcaster,
    peer_nodes: &[NodeId],
    local_node: NodeId,
    chain_id: u64,
    total_difficulty: u128,
    current_head: u64,
) {
    if peer_nodes.is_empty() {
        return;
    }
    let caps = default_eth_native_capabilities();
    let peer_nodes_arc = Arc::new(peer_nodes.to_vec());
    let parallelism = gateway_eth_native_parallelism()
        .min(peer_nodes.len())
        .max(1);
    if parallelism == 1 {
        for peer in peer_nodes.iter().copied() {
            let mut auth_tag = [0u8; 32];
            auth_tag[0..8].copy_from_slice(&chain_id.to_le_bytes());
            auth_tag[8..16].copy_from_slice(&local_node.0.to_le_bytes());
            auth_tag[16..24].copy_from_slice(&peer.0.to_le_bytes());
            let heartbeat = ProtocolMessage::Gossip(ProtocolGossipMessage::Heartbeat {
                from: local_node,
                shard: ShardId(1),
            });
            let peer_list = ProtocolMessage::Gossip(ProtocolGossipMessage::PeerList {
                from: local_node,
                peers: peer_nodes_arc.as_ref().clone(),
            });
            let _ = broadcaster.send(peer, heartbeat);
            let _ = broadcaster.send(peer, peer_list);
            let _ = broadcaster.send(
                peer,
                ProtocolMessage::EvmNative(EvmNativeMessage::DiscoveryPing {
                    from: local_node,
                    chain_id,
                    tcp_port: 0,
                    udp_port: 0,
                }),
            );
            let _ = broadcaster.send(
                peer,
                ProtocolMessage::EvmNative(EvmNativeMessage::RlpxAuth {
                    from: local_node,
                    chain_id,
                    network_id: chain_id,
                    auth_tag,
                }),
            );
            let _ = broadcaster.send(
                peer,
                ProtocolMessage::EvmNative(EvmNativeMessage::Hello {
                    from: local_node,
                    chain_id,
                    eth_versions: caps.eth_versions.iter().map(|v| v.as_u8()).collect(),
                    snap_versions: caps.snap_versions.iter().map(|v| v.as_u8()).collect(),
                    network_id: chain_id,
                    total_difficulty,
                    head_hash: [0u8; 32],
                    genesis_hash: [0u8; 32],
                }),
            );
            let _ = broadcaster.send(
                peer,
                ProtocolMessage::EvmNative(EvmNativeMessage::Status {
                    from: local_node,
                    chain_id,
                    total_difficulty,
                    head_height: current_head,
                    head_hash: [0u8; 32],
                    genesis_hash: [0u8; 32],
                }),
            );
        }
        return;
    }
    for batch in peer_nodes.chunks(parallelism) {
        std::thread::scope(|scope| {
            let mut handles = Vec::with_capacity(batch.len());
            for peer in batch.iter().copied() {
                let broadcaster = broadcaster.clone();
                let peers = peer_nodes_arc.clone();
                let eth_versions: Vec<u8> = caps.eth_versions.iter().map(|v| v.as_u8()).collect();
                let snap_versions: Vec<u8> = caps.snap_versions.iter().map(|v| v.as_u8()).collect();
                handles.push(scope.spawn(move || {
                    let mut auth_tag = [0u8; 32];
                    auth_tag[0..8].copy_from_slice(&chain_id.to_le_bytes());
                    auth_tag[8..16].copy_from_slice(&local_node.0.to_le_bytes());
                    auth_tag[16..24].copy_from_slice(&peer.0.to_le_bytes());
                    let heartbeat = ProtocolMessage::Gossip(ProtocolGossipMessage::Heartbeat {
                        from: local_node,
                        shard: ShardId(1),
                    });
                    let peer_list = ProtocolMessage::Gossip(ProtocolGossipMessage::PeerList {
                        from: local_node,
                        peers: peers.as_ref().clone(),
                    });
                    let _ = broadcaster.send(peer, heartbeat);
                    let _ = broadcaster.send(peer, peer_list);
                    let _ = broadcaster.send(
                        peer,
                        ProtocolMessage::EvmNative(EvmNativeMessage::DiscoveryPing {
                            from: local_node,
                            chain_id,
                            tcp_port: 0,
                            udp_port: 0,
                        }),
                    );
                    let _ = broadcaster.send(
                        peer,
                        ProtocolMessage::EvmNative(EvmNativeMessage::RlpxAuth {
                            from: local_node,
                            chain_id,
                            network_id: chain_id,
                            auth_tag,
                        }),
                    );
                    let _ = broadcaster.send(
                        peer,
                        ProtocolMessage::EvmNative(EvmNativeMessage::Hello {
                            from: local_node,
                            chain_id,
                            eth_versions,
                            snap_versions,
                            network_id: chain_id,
                            total_difficulty,
                            head_hash: [0u8; 32],
                            genesis_hash: [0u8; 32],
                        }),
                    );
                    let _ = broadcaster.send(
                        peer,
                        ProtocolMessage::EvmNative(EvmNativeMessage::Status {
                            from: local_node,
                            chain_id,
                            total_difficulty,
                            head_height: current_head,
                            head_hash: [0u8; 32],
                            genesis_hash: [0u8; 32],
                        }),
                    );
                }));
            }
            for handle in handles {
                let _ = handle.join();
            }
        });
    }
}

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
    cache_key: String,
    supvm_peers: GatewayEthNativePeers,
    supvm_peer_nodes: GatewayEthNativePeerNodes,
    plugin_peers: GatewayEthPluginPeers,
    plugin_ports: GatewayEthPluginPorts,
    route_policy: AdaptivePeerRoutePolicy,
    peer_source: String,
}

#[derive(Clone)]
struct GatewayEthNativePeersSnapshot {
    supvm_peers: GatewayEthNativePeers,
    supvm_peer_nodes: GatewayEthNativePeerNodes,
    plugin_peers: GatewayEthPluginPeers,
    plugin_ports: GatewayEthPluginPorts,
    route_policy: AdaptivePeerRoutePolicy,
    peer_source: String,
}

type GatewayEthNativePeers = Arc<Vec<(NodeId, String)>>;
type GatewayEthNativePeerNodes = Arc<Vec<NodeId>>;
type GatewayEthPluginPeers = Arc<Vec<PluginPeerEndpoint>>;
type GatewayEthPluginPorts = Arc<Vec<u16>>;

#[derive(Clone)]
struct GatewayEthPluginProbeOutcome {
    checked_ms: u64,
    total: usize,
    reachable_count: usize,
    unreachable_count: usize,
    error_preview: Vec<String>,
}

#[derive(Clone)]
struct GatewayEthPluginProbeCacheEntry {
    checked_ms: u64,
    outcome: GatewayEthPluginProbeOutcome,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum GatewayEthPluginSessionStage {
    Disconnected,
    TcpConnected,
    AuthSent,
    AckSeen,
    Ready,
}

impl GatewayEthPluginSessionStage {
    fn as_str(self) -> &'static str {
        match self {
            Self::Disconnected => "disconnected",
            Self::TcpConnected => "tcp_connected",
            Self::AuthSent => "auth_sent",
            Self::AckSeen => "ack_seen",
            Self::Ready => "ready",
        }
    }

    fn rank(self) -> u8 {
        match self {
            Self::Disconnected => 0,
            Self::TcpConnected => 1,
            Self::AuthSent => 2,
            Self::AckSeen => 3,
            Self::Ready => 4,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum GatewayEthPluginSessionProbeMode {
    Disabled,
    EnodeOnly,
    All,
}

fn parse_gateway_eth_plugin_session_stage(raw: &str) -> Option<GatewayEthPluginSessionStage> {
    match raw.trim().to_ascii_lowercase().as_str() {
        "disconnected" | "down" | "offline" => Some(GatewayEthPluginSessionStage::Disconnected),
        "tcp_connected" | "tcp" | "connected" => Some(GatewayEthPluginSessionStage::TcpConnected),
        "auth_sent" | "auth" => Some(GatewayEthPluginSessionStage::AuthSent),
        "ack_seen" | "ack" | "auth_ack" => Some(GatewayEthPluginSessionStage::AckSeen),
        "ready" | "ok" => Some(GatewayEthPluginSessionStage::Ready),
        _ => None,
    }
}

fn observe_gateway_eth_plugin_session_stage(chain_id: u64, stage: GatewayEthPluginSessionStage) {
    if stage.rank() >= GatewayEthPluginSessionStage::AuthSent.rank() {
        observe_eth_native_rlpx_auth(chain_id);
    }
    if stage.rank() >= GatewayEthPluginSessionStage::AckSeen.rank() {
        observe_eth_native_rlpx_auth_ack(chain_id);
    }
    if stage.rank() >= GatewayEthPluginSessionStage::Ready.rank() {
        observe_eth_native_hello(chain_id);
        observe_eth_native_status(chain_id);
    }
}

#[derive(Clone)]
struct GatewayEthPluginSessionState {
    chain_id: u64,
    endpoint: String,
    stage: GatewayEthPluginSessionStage,
    updated_ms: u64,
    last_error: Option<String>,
}

#[derive(Default, Clone)]
struct GatewayEthPluginSessionStageStats {
    disconnected: u64,
    tcp_connected: u64,
    auth_sent: u64,
    ack_seen: u64,
    ready: u64,
}

fn gateway_eth_plugin_session_stage_count(
    stats: &GatewayEthPluginSessionStageStats,
    stage: GatewayEthPluginSessionStage,
) -> u64 {
    match stage {
        GatewayEthPluginSessionStage::Disconnected => stats.disconnected,
        GatewayEthPluginSessionStage::TcpConnected => stats.tcp_connected,
        GatewayEthPluginSessionStage::AuthSent => stats.auth_sent,
        GatewayEthPluginSessionStage::AckSeen => stats.ack_seen,
        GatewayEthPluginSessionStage::Ready => stats.ready,
    }
}

fn gateway_eth_plugin_session_error_is_connectivity_fatal(last_error: &str) -> bool {
    let normalized = last_error.trim().to_ascii_lowercase();
    normalized.contains("stale_or_mismatched")
        || normalized.contains("addr_parse_failed(")
        || normalized.contains("connect_failed(")
        || normalized.contains("endpoint_not_enode")
}

fn gateway_eth_native_broadcaster_cache(
) -> &'static Mutex<HashMap<String, GatewayEthNativeBroadcaster>> {
    GATEWAY_ETH_NATIVE_BROADCASTER_CACHE.get_or_init(|| Mutex::new(HashMap::new()))
}

fn gateway_eth_native_peers_cache() -> &'static DashMap<u64, GatewayEthNativePeersCache> {
    GATEWAY_ETH_NATIVE_PEERS_CACHE.get_or_init(DashMap::new)
}

fn gateway_eth_plugin_probe_cache() -> &'static DashMap<String, GatewayEthPluginProbeCacheEntry> {
    GATEWAY_ETH_PLUGIN_PROBE_CACHE.get_or_init(DashMap::new)
}

fn gateway_eth_plugin_session_cache() -> &'static DashMap<String, GatewayEthPluginSessionState> {
    GATEWAY_ETH_PLUGIN_SESSION_CACHE.get_or_init(DashMap::new)
}

fn gateway_eth_plugin_mempool_worker_started() -> &'static Mutex<std::collections::HashSet<u64>> {
    GATEWAY_ETH_PLUGIN_MEMPOOL_WORKER_STARTED
        .get_or_init(|| Mutex::new(std::collections::HashSet::new()))
}

fn gateway_eth_plugin_mempool_state_map() -> &'static DashMap<u64, GatewayEthPluginMempoolState> {
    GATEWAY_ETH_PLUGIN_MEMPOOL_STATE.get_or_init(DashMap::new)
}

fn update_gateway_eth_plugin_mempool_state(
    chain_id: u64,
    f: impl FnOnce(&mut GatewayEthPluginMempoolState),
) {
    let state_map = gateway_eth_plugin_mempool_state_map();
    if let Some(mut state) = state_map.get_mut(&chain_id) {
        f(&mut state);
    } else {
        let mut state = GatewayEthPluginMempoolState::default();
        f(&mut state);
        state_map.insert(chain_id, state);
    }
}

fn snapshot_gateway_eth_plugin_mempool_state(chain_id: u64) -> GatewayEthPluginMempoolState {
    gateway_eth_plugin_mempool_state_map()
        .get(&chain_id)
        .map(|state| state.clone())
        .unwrap_or_default()
}

fn gateway_eth_plugin_rlpx_worker_started() -> &'static Mutex<std::collections::HashSet<String>> {
    GATEWAY_ETH_PLUGIN_RLPX_WORKER_STARTED
        .get_or_init(|| Mutex::new(std::collections::HashSet::new()))
}

fn gateway_eth_plugin_rlpx_worker_state_map(
) -> &'static DashMap<String, GatewayEthPluginRlpxWorkerState> {
    GATEWAY_ETH_PLUGIN_RLPX_WORKER_STATE.get_or_init(DashMap::new)
}

fn gateway_eth_plugin_rlpx_seen_hashes_map() -> &'static DashMap<GatewayEthPluginRlpxSeenKey, u64> {
    GATEWAY_ETH_PLUGIN_RLPX_SEEN_HASHES.get_or_init(DashMap::new)
}

fn gateway_eth_plugin_rlpx_seen_txs_map() -> &'static DashMap<GatewayEthPluginRlpxSeenKey, u64> {
    GATEWAY_ETH_PLUGIN_RLPX_SEEN_TXS.get_or_init(DashMap::new)
}

fn gateway_eth_plugin_rlpx_seen_last_gc_map() -> &'static DashMap<u64, u64> {
    GATEWAY_ETH_PLUGIN_RLPX_SEEN_LAST_GC.get_or_init(DashMap::new)
}

fn gateway_eth_plugin_rlpx_profile_persist_state_map(
) -> &'static DashMap<u64, GatewayEthPluginRlpxProfilePersistState> {
    GATEWAY_ETH_PLUGIN_RLPX_PROFILE_PERSIST_STATE.get_or_init(DashMap::new)
}

fn gateway_eth_plugin_rlpx_core_fallback_state_map(
) -> &'static DashMap<u64, GatewayEthPluginRlpxCoreFallbackState> {
    GATEWAY_ETH_PLUGIN_RLPX_CORE_FALLBACK_STATE.get_or_init(DashMap::new)
}

fn gateway_eth_plugin_rlpx_profile_loaded_chains() -> &'static Mutex<std::collections::HashSet<u64>>
{
    GATEWAY_ETH_PLUGIN_RLPX_PROFILE_LOADED_CHAINS
        .get_or_init(|| Mutex::new(std::collections::HashSet::new()))
}

fn build_gateway_eth_plugin_rlpx_worker_key(chain_id: u64, endpoint: &str) -> String {
    format!("{chain_id}:{}", endpoint.trim())
}

fn gateway_eth_plugin_rlpx_worker_chain_id(worker_key: &str) -> Option<u64> {
    worker_key
        .split_once(':')
        .and_then(|(raw, _)| raw.trim().parse::<u64>().ok())
}

fn gateway_eth_plugin_rlpx_profile_mark_dirty(chain_id: u64) {
    let state_map = gateway_eth_plugin_rlpx_profile_persist_state_map();
    if let Some(mut state) = state_map.get_mut(&chain_id) {
        state.dirty = true;
    } else {
        state_map.insert(
            chain_id,
            GatewayEthPluginRlpxProfilePersistState {
                dirty: true,
                last_flush_ms: 0,
            },
        );
    }
}

fn gateway_eth_plugin_rlpx_profile_update_persist_state(
    chain_id: u64,
    f: impl FnOnce(&mut GatewayEthPluginRlpxProfilePersistState),
) {
    let state_map = gateway_eth_plugin_rlpx_profile_persist_state_map();
    if let Some(mut state) = state_map.get_mut(&chain_id) {
        f(&mut state);
    } else {
        let mut state = GatewayEthPluginRlpxProfilePersistState::default();
        f(&mut state);
        state_map.insert(chain_id, state);
    }
}

fn gateway_eth_plugin_rlpx_profile_snapshot_persist_state(
    chain_id: u64,
) -> GatewayEthPluginRlpxProfilePersistState {
    gateway_eth_plugin_rlpx_profile_persist_state_map()
        .get(&chain_id)
        .map(|state| state.clone())
        .unwrap_or_default()
}

fn gateway_eth_plugin_rlpx_profile_state_rows_for_chain(
    chain_id: u64,
) -> Vec<(String, GatewayEthPluginRlpxWorkerState)> {
    let prefix = format!("{chain_id}:");
    gateway_eth_plugin_rlpx_worker_state_map()
        .iter()
        .filter_map(|entry| {
            let key = entry.key();
            if !key.starts_with(prefix.as_str()) {
                return None;
            }
            let endpoint = key.split_once(':')?.1.trim();
            if endpoint.is_empty() {
                return None;
            }
            Some((endpoint.to_string(), entry.value().clone()))
        })
        .collect::<Vec<_>>()
}

fn gateway_eth_plugin_rlpx_profile_enabled(chain_id: u64) -> bool {
    let raw = gateway_eth_public_broadcast_chain_string_env(
        chain_id,
        "NOVOVM_GATEWAY_ETH_PLUGIN_RLPX_PROFILE_PERSIST",
    )
    .unwrap_or_else(|| "true".to_string());
    !matches!(
        raw.trim().to_ascii_lowercase().as_str(),
        "" | "0" | "false" | "off" | "no" | "disable" | "disabled"
    )
}

fn gateway_eth_plugin_rlpx_profile_flush_interval_ms(chain_id: u64) -> u64 {
    gateway_eth_public_broadcast_chain_u64_env(
        chain_id,
        "NOVOVM_GATEWAY_ETH_PLUGIN_RLPX_PROFILE_FLUSH_INTERVAL_MS",
        GATEWAY_ETH_PLUGIN_RLPX_PROFILE_FLUSH_INTERVAL_MS_DEFAULT,
    )
    .clamp(
        GATEWAY_ETH_PLUGIN_RLPX_PROFILE_FLUSH_INTERVAL_MS_MIN,
        GATEWAY_ETH_PLUGIN_RLPX_PROFILE_FLUSH_INTERVAL_MS_MAX,
    )
}

fn gateway_eth_plugin_rlpx_profile_path(chain_id: u64) -> std::path::PathBuf {
    if let Some(raw) = gateway_eth_public_broadcast_chain_string_env(
        chain_id,
        "NOVOVM_GATEWAY_ETH_PLUGIN_RLPX_PROFILE_PATH",
    ) {
        return std::path::PathBuf::from(raw);
    }
    std::path::PathBuf::from(format!(
        "artifacts/migration/evm-plugin-rlpx-peer-profile-chain-{chain_id}.json"
    ))
}

fn gateway_eth_plugin_rlpx_profile_flush_now(chain_id: u64) -> Result<usize> {
    if !gateway_eth_plugin_rlpx_profile_enabled(chain_id) {
        return Ok(0);
    }
    let path = gateway_eth_plugin_rlpx_profile_path(chain_id);
    if let Some(parent) = path.parent() {
        ensure_dir(parent, "gateway eth rlpx peer profile dir")?;
    }
    let workers = gateway_eth_plugin_rlpx_profile_state_rows_for_chain(chain_id)
        .into_iter()
        .map(|(endpoint, state)| {
            GatewayEthPluginRlpxWorkerStatePersisted::from_runtime(endpoint, &state)
        })
        .collect::<Vec<_>>();
    let payload = GatewayEthPluginRlpxWorkerStatePersistedFile {
        version: 1,
        chain_id,
        updated_ms: now_unix_millis() as u64,
        workers,
    };
    let encoded = serde_json::to_vec_pretty(&payload)
        .context("serialize gateway eth rlpx peer profile failed")?;
    let tmp_path = std::path::PathBuf::from(format!("{}.tmp", path.display()));
    std::fs::write(tmp_path.as_path(), encoded).with_context(|| {
        format!(
            "write gateway eth rlpx peer profile temp failed: {}",
            tmp_path.display()
        )
    })?;
    std::fs::rename(tmp_path.as_path(), path.as_path()).with_context(|| {
        format!(
            "rename gateway eth rlpx peer profile failed: {} -> {}",
            tmp_path.display(),
            path.display()
        )
    })?;
    Ok(payload.workers.len())
}

fn gateway_eth_plugin_rlpx_profile_load_now(chain_id: u64) -> Result<usize> {
    if !gateway_eth_plugin_rlpx_profile_enabled(chain_id) {
        return Ok(0);
    }
    let path = gateway_eth_plugin_rlpx_profile_path(chain_id);
    if !path.exists() {
        return Ok(0);
    }
    let payload = std::fs::read(path.as_path()).with_context(|| {
        format!(
            "read gateway eth rlpx peer profile failed: {}",
            path.display()
        )
    })?;
    let decoded: GatewayEthPluginRlpxWorkerStatePersistedFile =
        serde_json::from_slice(payload.as_slice()).with_context(|| {
            format!(
                "decode gateway eth rlpx peer profile failed: {}",
                path.display()
            )
        })?;
    if decoded.chain_id != 0 && decoded.chain_id != chain_id {
        if gateway_warn_enabled() {
            eprintln!(
                "gateway_warn: skip mismatched rlpx peer profile chain_id={} expected={} path={}",
                decoded.chain_id,
                chain_id,
                path.display()
            );
        }
        return Ok(0);
    }
    let state_map = gateway_eth_plugin_rlpx_worker_state_map();
    let mut loaded = 0usize;
    for item in decoded.workers {
        let endpoint = item.endpoint.trim().to_string();
        if endpoint.is_empty() {
            continue;
        }
        let worker_key = build_gateway_eth_plugin_rlpx_worker_key(chain_id, endpoint.as_str());
        let mut runtime = item.into_runtime();
        runtime.running = false;
        runtime.cooldown_until_ms = 0;
        runtime.consecutive_failures = 0;
        runtime.consecutive_successes = 0;
        if state_map.insert(worker_key, runtime).is_none() {
            loaded = loaded.saturating_add(1);
        }
    }
    Ok(loaded)
}

fn ensure_gateway_eth_plugin_rlpx_profile_loaded(chain_id: u64) {
    if !gateway_eth_plugin_rlpx_profile_enabled(chain_id) {
        return;
    }
    let should_load = if let Ok(mut guard) = gateway_eth_plugin_rlpx_profile_loaded_chains().lock()
    {
        guard.insert(chain_id)
    } else {
        false
    };
    if !should_load {
        return;
    }
    match gateway_eth_plugin_rlpx_profile_load_now(chain_id) {
        Ok(loaded) => {
            let now_ms = now_unix_millis() as u64;
            gateway_eth_plugin_rlpx_profile_update_persist_state(chain_id, |state| {
                state.dirty = false;
                state.last_flush_ms = now_ms;
            });
            if gateway_warn_enabled() && loaded > 0 {
                eprintln!(
                    "gateway_warn: rlpx peer profile loaded: chain_id={} entries={} path={}",
                    chain_id,
                    loaded,
                    gateway_eth_plugin_rlpx_profile_path(chain_id).display()
                );
            }
        }
        Err(err) => {
            if gateway_warn_enabled() {
                eprintln!(
                    "gateway_warn: rlpx peer profile load failed: chain_id={} path={} err={}",
                    chain_id,
                    gateway_eth_plugin_rlpx_profile_path(chain_id).display(),
                    err
                );
            }
        }
    }
}

fn gateway_eth_plugin_rlpx_profile_try_flush(chain_id: u64, now_ms: u64) {
    if !gateway_eth_plugin_rlpx_profile_enabled(chain_id) {
        return;
    }
    let persist_state = gateway_eth_plugin_rlpx_profile_snapshot_persist_state(chain_id);
    if !persist_state.dirty {
        return;
    }
    let interval_ms = gateway_eth_plugin_rlpx_profile_flush_interval_ms(chain_id);
    if persist_state.last_flush_ms > 0
        && now_ms.saturating_sub(persist_state.last_flush_ms) < interval_ms
    {
        return;
    }
    match gateway_eth_plugin_rlpx_profile_flush_now(chain_id) {
        Ok(written) => {
            gateway_eth_plugin_rlpx_profile_update_persist_state(chain_id, |state| {
                state.dirty = false;
                state.last_flush_ms = now_ms;
            });
            if gateway_warn_enabled() {
                eprintln!(
                    "gateway_warn: rlpx peer profile flushed: chain_id={} entries={} path={}",
                    chain_id,
                    written,
                    gateway_eth_plugin_rlpx_profile_path(chain_id).display()
                );
            }
        }
        Err(err) => {
            gateway_eth_plugin_rlpx_profile_update_persist_state(chain_id, |state| {
                state.last_flush_ms = now_ms;
            });
            if gateway_warn_enabled() {
                eprintln!(
                    "gateway_warn: rlpx peer profile flush failed: chain_id={} path={} err={}",
                    chain_id,
                    gateway_eth_plugin_rlpx_profile_path(chain_id).display(),
                    err
                );
            }
        }
    }
}

fn update_gateway_eth_plugin_rlpx_worker_state(
    worker_key: &str,
    f: impl FnOnce(&mut GatewayEthPluginRlpxWorkerState),
) {
    let chain_id = gateway_eth_plugin_rlpx_worker_chain_id(worker_key);
    let state_map = gateway_eth_plugin_rlpx_worker_state_map();
    if let Some(mut state) = state_map.get_mut(worker_key) {
        f(&mut state);
    } else {
        let mut state = GatewayEthPluginRlpxWorkerState::default();
        f(&mut state);
        state_map.insert(worker_key.to_string(), state);
    }
    if let Some(chain_id) = chain_id {
        gateway_eth_plugin_rlpx_profile_mark_dirty(chain_id);
    }
}

fn snapshot_gateway_eth_plugin_rlpx_worker_state(
    worker_key: &str,
) -> GatewayEthPluginRlpxWorkerState {
    gateway_eth_plugin_rlpx_worker_state_map()
        .get(worker_key)
        .map(|state| state.clone())
        .unwrap_or_default()
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

fn gateway_eth_native_sync_pull_request(
    local_node: NodeId,
    chain_id: u64,
    phase_tag: u8,
    from_block: u64,
    to_block: u64,
) -> ProtocolMessage {
    let span = to_block.saturating_sub(from_block).saturating_add(1);
    match phase_tag {
        // Header sync path (eth/*)
        1 => ProtocolMessage::EvmNative(EvmNativeMessage::GetBlockHeaders {
            from: local_node,
            start_height: from_block,
            max: span.max(1),
            skip: 0,
            reverse: false,
        }),
        // Bodies sync path (eth/*): keep request shape deterministic and lightweight.
        2 => ProtocolMessage::EvmNative(EvmNativeMessage::GetBlockBodies {
            from: local_node,
            hashes: vec![{
                let mut h = [0u8; 32];
                h[0..8].copy_from_slice(&chain_id.to_le_bytes());
                h[8..16].copy_from_slice(&from_block.to_le_bytes());
                h[16..24].copy_from_slice(&to_block.to_le_bytes());
                h
            }],
        }),
        // State/snap path (snap/*)
        3 => ProtocolMessage::EvmNative(EvmNativeMessage::SnapGetAccountRange {
            from: local_node,
            block_hash: {
                let mut h = [0u8; 32];
                h[0..8].copy_from_slice(&chain_id.to_le_bytes());
                h[8..16].copy_from_slice(&to_block.to_le_bytes());
                h
            },
            origin: {
                let mut o = [0u8; 32];
                o[0..8].copy_from_slice(&from_block.to_le_bytes());
                o
            },
            limit: span.max(1),
        }),
        // Finalize/discovery/unknown: route through header window pull.
        _ => ProtocolMessage::EvmNative(EvmNativeMessage::GetBlockHeaders {
            from: local_node,
            start_height: from_block,
            max: span.max(1),
            skip: 0,
            reverse: false,
        }),
    }
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
) -> usize {
    if ordered_peers.is_empty() || fanout == 0 || to_block < from_block {
        return 0;
    }
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
    if target_success == 1 {
        if let Some(peer) = ordered_peers.first().copied() {
            let sync_pull = gateway_eth_native_sync_pull_request(
                local_node, chain_id, phase_tag, from_block, to_block,
            );
            return usize::from(broadcaster.send(peer, sync_pull).is_ok());
        }
        return 0;
    }
    let ranges = split_gateway_eth_sync_pull_window(from_block, to_block, target_success);
    if ranges.is_empty() {
        return 0;
    }
    gateway_eth_native_send_sync_pull_ranges_parallel(
        broadcaster,
        ordered_peers,
        local_node,
        chain_id,
        phase_tag,
        ranges.as_slice(),
    )
}

fn gateway_eth_native_send_sync_pull_ranges_parallel(
    broadcaster: &GatewayEthNativeBroadcaster,
    ordered_peers: &[NodeId],
    local_node: NodeId,
    chain_id: u64,
    phase_tag: u8,
    ranges: &[(u64, u64)],
) -> usize {
    if ordered_peers.is_empty() || ranges.is_empty() {
        return 0;
    }
    let parallelism = gateway_eth_native_parallelism().min(ranges.len()).max(1);
    let peers_arc = Arc::new(ordered_peers.to_vec());
    let mut success_count = 0usize;
    for (chunk_idx, range_chunk) in ranges.chunks(parallelism).enumerate() {
        std::thread::scope(|scope| {
            let mut handles = Vec::with_capacity(range_chunk.len());
            for (offset, (range_from, range_to)) in range_chunk.iter().copied().enumerate() {
                let broadcaster = broadcaster.clone();
                let peers = peers_arc.clone();
                let start_idx = (chunk_idx * parallelism + offset) % peers.len();
                handles.push(scope.spawn(move || {
                    let sync_pull = gateway_eth_native_sync_pull_request(
                        local_node, chain_id, phase_tag, range_from, range_to,
                    );
                    for step in 0..peers.len() {
                        let idx = (start_idx + step) % peers.len();
                        if broadcaster.send(peers[idx], sync_pull.clone()).is_ok() {
                            return true;
                        }
                    }
                    false
                }));
            }
            for handle in handles {
                if let Ok(true) = handle.join() {
                    success_count = success_count.saturating_add(1);
                }
            }
        });
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

fn gateway_eth_plugin_mempool_ingest_enabled(chain_id: u64) -> bool {
    gateway_eth_public_broadcast_chain_string_env(
        chain_id,
        "NOVOVM_GATEWAY_ETH_PLUGIN_MEMPOOL_INGEST_ENABLE",
    )
    .and_then(|raw| {
        let normalized = raw.trim().to_ascii_lowercase();
        match normalized.as_str() {
            "1" | "true" | "on" | "yes" => Some(true),
            "0" | "false" | "off" | "no" => Some(false),
            _ => None,
        }
    })
    .unwrap_or(false)
}

fn gateway_eth_plugin_mempool_ingest_poll_ms(chain_id: u64) -> u64 {
    gateway_eth_public_broadcast_chain_u64_env(
        chain_id,
        "NOVOVM_GATEWAY_ETH_PLUGIN_MEMPOOL_INGEST_POLL_MS",
        GATEWAY_ETH_PLUGIN_MEMPOOL_INGEST_POLL_MS_DEFAULT,
    )
    .clamp(
        GATEWAY_ETH_PLUGIN_MEMPOOL_INGEST_POLL_MS_MIN,
        GATEWAY_ETH_PLUGIN_MEMPOOL_INGEST_POLL_MS_MAX,
    )
}

fn gateway_eth_plugin_mempool_ingest_stale_ttl_ms(chain_id: u64) -> u64 {
    gateway_eth_public_broadcast_chain_u64_env(
        chain_id,
        "NOVOVM_GATEWAY_ETH_PLUGIN_MEMPOOL_INGEST_STALE_TTL_MS",
        GATEWAY_ETH_PLUGIN_MEMPOOL_INGEST_STALE_TTL_MS_DEFAULT,
    )
    .clamp(0, GATEWAY_ETH_PLUGIN_MEMPOOL_INGEST_STALE_TTL_MS_MAX)
}

fn gateway_eth_plugin_mempool_ingest_rlpx_enabled(chain_id: u64) -> bool {
    gateway_eth_public_broadcast_chain_string_env(
        chain_id,
        "NOVOVM_GATEWAY_ETH_PLUGIN_MEMPOOL_INGEST_RLPX_ENABLE",
    )
    .and_then(|raw| {
        let normalized = raw.trim().to_ascii_lowercase();
        match normalized.as_str() {
            "1" | "true" | "on" | "yes" => Some(true),
            "0" | "false" | "off" | "no" => Some(false),
            _ => None,
        }
    })
    .unwrap_or(true)
}

fn gateway_eth_plugin_mempool_ingest_rlpx_read_window_ms(chain_id: u64) -> u64 {
    gateway_eth_public_broadcast_chain_u64_env(
        chain_id,
        "NOVOVM_GATEWAY_ETH_PLUGIN_MEMPOOL_INGEST_RLPX_READ_WINDOW_MS",
        GATEWAY_ETH_PLUGIN_RLPX_READ_WINDOW_MS_DEFAULT,
    )
    .clamp(
        GATEWAY_ETH_PLUGIN_RLPX_READ_WINDOW_MS_MIN,
        GATEWAY_ETH_PLUGIN_RLPX_READ_WINDOW_MS_MAX,
    )
}

fn gateway_eth_plugin_mempool_ingest_rlpx_max_peers_per_tick(chain_id: u64) -> usize {
    gateway_eth_public_broadcast_chain_u64_env(
        chain_id,
        "NOVOVM_GATEWAY_ETH_PLUGIN_MEMPOOL_INGEST_RLPX_MAX_PEERS_PER_TICK",
        GATEWAY_ETH_PLUGIN_RLPX_MAX_PEERS_PER_TICK_DEFAULT as u64,
    )
    .clamp(
        1,
        GATEWAY_ETH_PLUGIN_RLPX_MAX_PEERS_PER_TICK_HARD_MAX as u64,
    ) as usize
}

fn gateway_eth_plugin_mempool_ingest_rlpx_core_target(chain_id: u64) -> usize {
    gateway_eth_public_broadcast_chain_u64_env(
        chain_id,
        "NOVOVM_GATEWAY_ETH_PLUGIN_MEMPOOL_INGEST_RLPX_CORE_TARGET",
        GATEWAY_ETH_PLUGIN_RLPX_CORE_TARGET_DEFAULT as u64,
    )
    .clamp(1, GATEWAY_ETH_PLUGIN_RLPX_TIER_TARGET_HARD_MAX as u64) as usize
}

fn gateway_eth_plugin_mempool_ingest_rlpx_active_target(chain_id: u64) -> usize {
    gateway_eth_public_broadcast_chain_u64_env(
        chain_id,
        "NOVOVM_GATEWAY_ETH_PLUGIN_MEMPOOL_INGEST_RLPX_ACTIVE_TARGET",
        GATEWAY_ETH_PLUGIN_RLPX_ACTIVE_TARGET_DEFAULT as u64,
    )
    .clamp(1, GATEWAY_ETH_PLUGIN_RLPX_TIER_TARGET_HARD_MAX as u64) as usize
}

fn gateway_eth_plugin_mempool_ingest_rlpx_core_stable_floor(chain_id: u64) -> usize {
    gateway_eth_public_broadcast_chain_u64_env(
        chain_id,
        "NOVOVM_GATEWAY_ETH_PLUGIN_MEMPOOL_INGEST_RLPX_CORE_STABLE_FLOOR",
        GATEWAY_ETH_PLUGIN_RLPX_CORE_STABLE_FLOOR_DEFAULT as u64,
    )
    .clamp(1, GATEWAY_ETH_PLUGIN_RLPX_TIER_TARGET_HARD_MAX as u64) as usize
}

fn gateway_eth_plugin_mempool_ingest_rlpx_core_fallback_trigger_ticks(chain_id: u64) -> u64 {
    gateway_eth_public_broadcast_chain_u64_env(
        chain_id,
        "NOVOVM_GATEWAY_ETH_PLUGIN_MEMPOOL_INGEST_RLPX_CORE_FALLBACK_TRIGGER_TICKS",
        GATEWAY_ETH_PLUGIN_RLPX_CORE_FALLBACK_TRIGGER_TICKS_DEFAULT,
    )
    .clamp(1, 1_000)
}

fn gateway_eth_plugin_mempool_ingest_rlpx_core_fallback_hold_ticks(chain_id: u64) -> u64 {
    gateway_eth_public_broadcast_chain_u64_env(
        chain_id,
        "NOVOVM_GATEWAY_ETH_PLUGIN_MEMPOOL_INGEST_RLPX_CORE_FALLBACK_HOLD_TICKS",
        GATEWAY_ETH_PLUGIN_RLPX_CORE_FALLBACK_HOLD_TICKS_DEFAULT,
    )
    .clamp(1, 10_000)
}

fn gateway_eth_plugin_mempool_ingest_rlpx_core_fallback_max_peers_bonus(chain_id: u64) -> usize {
    gateway_eth_public_broadcast_chain_u64_env(
        chain_id,
        "NOVOVM_GATEWAY_ETH_PLUGIN_MEMPOOL_INGEST_RLPX_CORE_FALLBACK_MAX_PEERS_BONUS",
        GATEWAY_ETH_PLUGIN_RLPX_CORE_FALLBACK_MAX_PEERS_BONUS_DEFAULT as u64,
    )
    .clamp(
        0,
        GATEWAY_ETH_PLUGIN_RLPX_MAX_PEERS_PER_TICK_HARD_MAX as u64,
    ) as usize
}

fn gateway_eth_plugin_mempool_ingest_rlpx_core_fallback_candidate_budget_min(
    chain_id: u64,
) -> usize {
    gateway_eth_public_broadcast_chain_u64_env(
        chain_id,
        "NOVOVM_GATEWAY_ETH_PLUGIN_MEMPOOL_INGEST_RLPX_CORE_FALLBACK_CANDIDATE_BUDGET_MIN",
        GATEWAY_ETH_PLUGIN_RLPX_CORE_FALLBACK_CANDIDATE_BUDGET_MIN_DEFAULT as u64,
    )
    .clamp(
        1,
        GATEWAY_ETH_PLUGIN_RLPX_MAX_PEERS_PER_TICK_HARD_MAX as u64,
    ) as usize
}

fn gateway_eth_plugin_mempool_ingest_rlpx_core_fallback_demote_too_many_min(chain_id: u64) -> u64 {
    gateway_eth_public_broadcast_chain_u64_env(
        chain_id,
        "NOVOVM_GATEWAY_ETH_PLUGIN_MEMPOOL_INGEST_RLPX_CORE_FALLBACK_DEMOTE_TOO_MANY_MIN",
        GATEWAY_ETH_PLUGIN_RLPX_CORE_FALLBACK_DEMOTE_TOO_MANY_MIN_DEFAULT,
    )
    .clamp(1, 10_000)
}

fn gateway_eth_plugin_mempool_ingest_rlpx_core_recent_gossip_window_ms(chain_id: u64) -> u64 {
    gateway_eth_public_broadcast_chain_u64_env(
        chain_id,
        "NOVOVM_GATEWAY_ETH_PLUGIN_MEMPOOL_INGEST_RLPX_CORE_RECENT_GOSSIP_WINDOW_MS",
        GATEWAY_ETH_PLUGIN_RLPX_CORE_RECENT_GOSSIP_WINDOW_MS_DEFAULT,
    )
    .clamp(30_000, 24 * 60 * 60 * 1_000)
}

fn gateway_eth_plugin_mempool_ingest_rlpx_active_recent_ready_window_ms(chain_id: u64) -> u64 {
    gateway_eth_public_broadcast_chain_u64_env(
        chain_id,
        "NOVOVM_GATEWAY_ETH_PLUGIN_MEMPOOL_INGEST_RLPX_ACTIVE_RECENT_READY_WINDOW_MS",
        GATEWAY_ETH_PLUGIN_RLPX_ACTIVE_RECENT_READY_WINDOW_MS_DEFAULT,
    )
    .clamp(30_000, 24 * 60 * 60 * 1_000)
}

fn gateway_eth_plugin_mempool_ingest_rlpx_core_lock_ms(chain_id: u64) -> u64 {
    gateway_eth_public_broadcast_chain_u64_env(
        chain_id,
        "NOVOVM_GATEWAY_ETH_PLUGIN_MEMPOOL_INGEST_RLPX_CORE_LOCK_MS",
        GATEWAY_ETH_PLUGIN_RLPX_CORE_LOCK_MS_DEFAULT,
    )
    .clamp(0, 24 * 60 * 60 * 1_000)
}

fn gateway_eth_plugin_mempool_ingest_rlpx_recent_new_hash_window_ms(chain_id: u64) -> u64 {
    gateway_eth_public_broadcast_chain_u64_env(
        chain_id,
        "NOVOVM_GATEWAY_ETH_PLUGIN_MEMPOOL_INGEST_RLPX_RECENT_NEW_HASH_WINDOW_MS",
        GATEWAY_ETH_PLUGIN_RLPX_RECENT_NEW_POOLED_HASH_WINDOW_MS_DEFAULT,
    )
    .clamp(30_000, 24 * 60 * 60 * 1_000)
}

fn gateway_eth_plugin_mempool_ingest_rlpx_recent_new_hash_min(chain_id: u64) -> u64 {
    gateway_eth_public_broadcast_chain_u64_env(
        chain_id,
        "NOVOVM_GATEWAY_ETH_PLUGIN_MEMPOOL_INGEST_RLPX_RECENT_NEW_HASH_MIN",
        GATEWAY_ETH_PLUGIN_RLPX_RECENT_NEW_POOLED_HASH_MIN_DEFAULT,
    )
    .clamp(1, 1_000_000)
}

fn gateway_eth_plugin_mempool_ingest_rlpx_swap_priority_enabled(chain_id: u64) -> bool {
    gateway_eth_public_broadcast_chain_string_env(
        chain_id,
        "NOVOVM_GATEWAY_ETH_PLUGIN_MEMPOOL_INGEST_RLPX_SWAP_PRIORITY_ENABLE",
    )
    .and_then(|raw| {
        let normalized = raw.trim().to_ascii_lowercase();
        match normalized.as_str() {
            "1" | "true" | "on" | "yes" | "swap" | "swap_only" => Some(true),
            "0" | "false" | "off" | "no" | "disable" | "disabled" => Some(false),
            _ => None,
        }
    })
    .unwrap_or(false)
}

fn gateway_eth_plugin_mempool_ingest_rlpx_swap_priority_latency_target_ms(chain_id: u64) -> u64 {
    gateway_eth_public_broadcast_chain_u64_env(
        chain_id,
        "NOVOVM_GATEWAY_ETH_PLUGIN_MEMPOOL_INGEST_RLPX_SWAP_PRIORITY_LATENCY_TARGET_MS",
        GATEWAY_ETH_PLUGIN_RLPX_SWAP_PRIORITY_LATENCY_TARGET_MS_DEFAULT,
    )
    .clamp(
        GATEWAY_ETH_PLUGIN_RLPX_SWAP_PRIORITY_LATENCY_TARGET_MS_MIN,
        GATEWAY_ETH_PLUGIN_RLPX_SWAP_PRIORITY_LATENCY_TARGET_MS_MAX,
    )
}

fn gateway_eth_plugin_mempool_ingest_rlpx_priority_budget(
    chain_id: u64,
    max_peers: usize,
    core_count: usize,
    core_target: usize,
) -> usize {
    if max_peers == 0 {
        return 0;
    }
    let configured_budget = (gateway_eth_public_broadcast_chain_u64_env(
        chain_id,
        "NOVOVM_GATEWAY_ETH_PLUGIN_MEMPOOL_INGEST_RLPX_PRIORITY_BUDGET",
        GATEWAY_ETH_PLUGIN_RLPX_PRIORITY_BUDGET_DEFAULT as u64,
    )
    .clamp(0, GATEWAY_ETH_PLUGIN_RLPX_PRIORITY_BUDGET_HARD_MAX as u64)
        as usize)
        .min(max_peers);
    if core_count >= core_target {
        return configured_budget;
    }
    let shortfall = core_target.saturating_sub(core_count);
    let adaptive_floor = shortfall.clamp(1, max_peers.min(8));
    configured_budget.max(adaptive_floor)
}

fn gateway_eth_plugin_mempool_ingest_rlpx_priority_auto_pool_size(
    chain_id: u64,
    max_peers: usize,
) -> usize {
    if max_peers == 0 {
        return 0;
    }
    if let Some(raw) = gateway_eth_public_broadcast_chain_string_env(
        chain_id,
        "NOVOVM_GATEWAY_ETH_PLUGIN_MEMPOOL_INGEST_RLPX_PRIORITY_AUTO_POOL_SIZE",
    ) {
        return (raw
            .trim()
            .parse::<u64>()
            .ok()
            .unwrap_or(GATEWAY_ETH_PLUGIN_RLPX_PRIORITY_AUTO_POOL_SIZE_DEFAULT as u64)
            .clamp(
                0,
                GATEWAY_ETH_PLUGIN_RLPX_PRIORITY_AUTO_POOL_SIZE_HARD_MAX as u64,
            ) as usize)
            .min(max_peers);
    }
    gateway_eth_plugin_mempool_ingest_rlpx_core_target(chain_id)
        .clamp(1, GATEWAY_ETH_PLUGIN_RLPX_PRIORITY_AUTO_POOL_SIZE_HARD_MAX)
        .min(max_peers)
}

fn gateway_eth_plugin_normalize_priority_addr_hint(raw: &str) -> Option<String> {
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return None;
    }
    if let Some((_, addr_hint)) = parse_gateway_eth_plugin_session_endpoint(trimmed) {
        return Some(addr_hint.to_ascii_lowercase());
    }
    if let Some((host, port)) = split_addr_hint_host_port(trimmed) {
        return Some(format_addr_hint(host.as_str(), port).to_ascii_lowercase());
    }
    None
}

fn gateway_eth_plugin_mempool_ingest_rlpx_priority_addr_hints(
    chain_id: u64,
) -> std::collections::HashSet<String> {
    let mut out = std::collections::HashSet::<String>::new();
    let raw = gateway_eth_public_broadcast_chain_string_env(
        chain_id,
        "NOVOVM_GATEWAY_ETH_PLUGIN_MEMPOOL_INGEST_RLPX_PRIORITY_ADDR_HINTS",
    )
    .or_else(|| {
        gateway_eth_public_broadcast_chain_string_env(
            chain_id,
            "NOVOVM_GATEWAY_ETH_PLUGIN_MEMPOOL_INGEST_RLPX_PRIORITY_ADDRS",
        )
    });
    let Some(raw) = raw else {
        return out;
    };
    for token in raw.split([',', ';', '\n', '\r', '\t', ' ']) {
        if let Some(addr_hint) = gateway_eth_plugin_normalize_priority_addr_hint(token) {
            out.insert(addr_hint);
        }
    }
    out
}

fn gateway_eth_plugin_mempool_ingest_rlpx_max_hashes_per_request(chain_id: u64) -> usize {
    gateway_eth_public_broadcast_chain_u64_env(
        chain_id,
        "NOVOVM_GATEWAY_ETH_PLUGIN_MEMPOOL_INGEST_RLPX_MAX_HASHES_PER_REQUEST",
        GATEWAY_ETH_PLUGIN_RLPX_MAX_HASHES_PER_REQUEST_DEFAULT as u64,
    )
    .clamp(
        1,
        GATEWAY_ETH_PLUGIN_RLPX_MAX_HASHES_PER_REQUEST_HARD_MAX as u64,
    ) as usize
}

fn gateway_eth_plugin_mempool_ingest_rlpx_timeout_ms(chain_id: u64) -> u64 {
    gateway_eth_public_broadcast_chain_u64_env(
        chain_id,
        "NOVOVM_GATEWAY_ETH_PLUGIN_MEMPOOL_INGEST_RLPX_TIMEOUT_MS",
        GATEWAY_ETH_PLUGIN_RLPX_HANDSHAKE_TIMEOUT_MS_DEFAULT,
    )
    .clamp(
        GATEWAY_ETH_PLUGIN_PROBE_TIMEOUT_MS_MIN,
        GATEWAY_ETH_PLUGIN_PROBE_TIMEOUT_MS_MAX,
    )
}

fn gateway_eth_plugin_mempool_ingest_rlpx_single_session(chain_id: u64) -> bool {
    gateway_eth_public_broadcast_chain_string_env(
        chain_id,
        "NOVOVM_GATEWAY_ETH_PLUGIN_MEMPOOL_INGEST_RLPX_SINGLE_SESSION",
    )
    .and_then(|raw| {
        let normalized = raw.trim().to_ascii_lowercase();
        match normalized.as_str() {
            "1" | "true" | "on" | "yes" => Some(true),
            "0" | "false" | "off" | "no" => Some(false),
            _ => None,
        }
    })
    .unwrap_or(false)
}

fn gateway_eth_plugin_endpoint_node_hint(endpoint: &str) -> u64 {
    use sha2::Digest;
    let hash = sha2::Sha256::digest(endpoint.as_bytes());
    let mut out = [0u8; 8];
    out.copy_from_slice(&hash[..8]);
    u64::from_be_bytes(out).max(1)
}

fn gateway_eth_plugin_runtime_bridge_peers(
    plugin_peers: &[PluginPeerEndpoint],
) -> (GatewayEthNativePeers, GatewayEthNativePeerNodes) {
    let mut bridge = Vec::<(NodeId, String)>::new();
    let mut seen = std::collections::HashSet::<String>::new();
    for peer in plugin_peers {
        if split_addr_hint_host_port(peer.addr_hint.as_str()).is_none() {
            continue;
        }
        let key = format!(
            "{}@{}",
            peer.node_hint.max(1),
            peer.addr_hint.to_ascii_lowercase()
        );
        if !seen.insert(key) {
            continue;
        }
        bridge.push((NodeId(peer.node_hint.max(1)), peer.addr_hint.clone()));
    }
    let peer_nodes = bridge.iter().map(|(node, _)| *node).collect::<Vec<_>>();
    (Arc::new(bridge), Arc::new(peer_nodes))
}

fn gateway_eth_plugin_rlpx_worker_fail_backoff_ms(consecutive_failures: u32) -> u64 {
    if consecutive_failures == 0 {
        return 0;
    }
    let shift = consecutive_failures
        .saturating_sub(1)
        .min(GATEWAY_ETH_PLUGIN_RLPX_WORKER_FAIL_BACKOFF_SHIFT_CAP);
    GATEWAY_ETH_PLUGIN_RLPX_WORKER_FAIL_BACKOFF_BASE_MS
        .saturating_mul(1u64 << shift)
        .clamp(
            GATEWAY_ETH_PLUGIN_RLPX_WORKER_FAIL_BACKOFF_BASE_MS,
            GATEWAY_ETH_PLUGIN_RLPX_WORKER_FAIL_BACKOFF_MAX_MS,
        )
}

fn gateway_eth_plugin_rlpx_worker_tier_backoff_bounds_ms(tier_rank: u8) -> (u64, u64) {
    match tier_rank {
        0 => (
            GATEWAY_ETH_PLUGIN_RLPX_WORKER_BACKOFF_CORE_MIN_MS,
            GATEWAY_ETH_PLUGIN_RLPX_WORKER_BACKOFF_CORE_MAX_MS,
        ),
        1 => (
            GATEWAY_ETH_PLUGIN_RLPX_WORKER_BACKOFF_ACTIVE_MIN_MS,
            GATEWAY_ETH_PLUGIN_RLPX_WORKER_BACKOFF_ACTIVE_MAX_MS,
        ),
        _ => (
            GATEWAY_ETH_PLUGIN_RLPX_WORKER_BACKOFF_CANDIDATE_MIN_MS,
            GATEWAY_ETH_PLUGIN_RLPX_WORKER_BACKOFF_CANDIDATE_MAX_MS,
        ),
    }
}

fn gateway_eth_plugin_rlpx_worker_timeout_backoff_floor_ms(tier_rank: u8) -> u64 {
    match tier_rank {
        0 => GATEWAY_ETH_PLUGIN_RLPX_WORKER_TIMEOUT_BACKOFF_CORE_MIN_MS,
        1 => GATEWAY_ETH_PLUGIN_RLPX_WORKER_TIMEOUT_BACKOFF_ACTIVE_MIN_MS,
        _ => GATEWAY_ETH_PLUGIN_RLPX_WORKER_TIMEOUT_BACKOFF_CANDIDATE_MIN_MS,
    }
}

fn gateway_eth_plugin_rlpx_learning_sample_score(
    metrics: &GatewayEthPluginRlpxSessionMetrics,
) -> u64 {
    let first_code_bonus: u64 = if metrics.first_post_ready_code
        == Some(
            GATEWAY_ETH_PLUGIN_RLPX_BASE_PROTOCOL_OFFSET
                + GATEWAY_ETH_PLUGIN_RLPX_ETH_NEW_POOLED_HASHES_MSG,
        ) {
        512u64
    } else {
        0u64
    };
    first_code_bonus
        .saturating_add(metrics.new_pooled_msgs.saturating_mul(16))
        .saturating_add(metrics.new_pooled_hashes.saturating_mul(2))
        .saturating_add(metrics.get_pooled_sent.saturating_mul(8))
        .saturating_add(metrics.pooled_msgs.saturating_mul(24))
        .saturating_add(metrics.pooled_txs_received.saturating_mul(6))
        .saturating_add(metrics.pooled_txs_imported.saturating_mul(10))
        .saturating_add(metrics.swap_hits.saturating_mul(20))
        .saturating_add(metrics.unique_swap_hits.saturating_mul(36))
        .saturating_add(metrics.swap_v2_hits.saturating_mul(12))
        .saturating_add(metrics.swap_v3_hits.saturating_mul(16))
        .saturating_add(metrics.txs_msgs.saturating_mul(16))
        .min(1_000_000u64)
}

#[cfg(test)]
fn gateway_eth_plugin_rlpx_update_learning_state(
    state: &mut GatewayEthPluginRlpxWorkerState,
    metrics: &GatewayEthPluginRlpxSessionMetrics,
) {
    state.sessions_completed = state.sessions_completed.saturating_add(1);
    let has_gossip =
        metrics.new_pooled_hashes > 0 || metrics.pooled_txs_received > 0 || metrics.txs_msgs > 0;
    if has_gossip {
        state.sessions_with_gossip = state.sessions_with_gossip.saturating_add(1);
    }
    state.total_new_pooled_msgs = state
        .total_new_pooled_msgs
        .saturating_add(metrics.new_pooled_msgs);
    state.total_new_pooled_hashes = state
        .total_new_pooled_hashes
        .saturating_add(metrics.new_pooled_hashes);
    state.total_unique_new_pooled_hashes = state
        .total_unique_new_pooled_hashes
        .saturating_add(metrics.unique_new_pooled_hashes);
    state.total_duplicate_new_pooled_hashes = state
        .total_duplicate_new_pooled_hashes
        .saturating_add(metrics.duplicate_new_pooled_hashes);
    state.total_get_pooled_sent = state
        .total_get_pooled_sent
        .saturating_add(metrics.get_pooled_sent);
    state.total_pooled_msgs = state.total_pooled_msgs.saturating_add(metrics.pooled_msgs);
    state.total_pooled_txs_received = state
        .total_pooled_txs_received
        .saturating_add(metrics.pooled_txs_received);
    state.total_unique_pooled_txs = state
        .total_unique_pooled_txs
        .saturating_add(metrics.unique_pooled_txs);
    state.total_duplicate_pooled_txs = state
        .total_duplicate_pooled_txs
        .saturating_add(metrics.duplicate_pooled_txs);
    state.total_pooled_txs_imported = state
        .total_pooled_txs_imported
        .saturating_add(metrics.pooled_txs_imported);
    state.total_txs_msgs = state.total_txs_msgs.saturating_add(metrics.txs_msgs);
    state.first_seen_hash_count = state
        .first_seen_hash_count
        .saturating_add(metrics.first_seen_hashes);
    state.first_seen_tx_count = state
        .first_seen_tx_count
        .saturating_add(metrics.first_seen_txs);
    state.total_swap_hits = state.total_swap_hits.saturating_add(metrics.swap_hits);
    state.total_swap_v2_hits = state
        .total_swap_v2_hits
        .saturating_add(metrics.swap_v2_hits);
    state.total_swap_v3_hits = state
        .total_swap_v3_hits
        .saturating_add(metrics.swap_v3_hits);
    state.total_unique_swap_hits = state
        .total_unique_swap_hits
        .saturating_add(metrics.unique_swap_hits);
    if metrics.first_gossip_latency_ms > 0 {
        state.total_first_gossip_latency_ms = state
            .total_first_gossip_latency_ms
            .saturating_add(metrics.first_gossip_latency_ms);
        state.first_gossip_latency_samples = state.first_gossip_latency_samples.saturating_add(1);
    }
    if metrics.first_swap_latency_ms > 0 {
        state.total_first_swap_latency_ms = state
            .total_first_swap_latency_ms
            .saturating_add(metrics.first_swap_latency_ms);
        state.first_swap_latency_samples = state.first_swap_latency_samples.saturating_add(1);
    }
    if let Some(code) = metrics.first_post_ready_code {
        state.last_first_post_ready_code = code;
    }
    let sample = gateway_eth_plugin_rlpx_learning_sample_score(metrics);
    state.last_sample_score = sample;
    if state.learning_score == 0 {
        state.learning_score = sample;
    } else {
        // Exponential smoothing keeps history but reacts to recent good peers.
        state.learning_score = state
            .learning_score
            .saturating_mul(7)
            .saturating_add(sample.saturating_mul(3))
            / 10;
    }
}

fn gateway_eth_plugin_rlpx_finalize_learning_state(
    state: &mut GatewayEthPluginRlpxWorkerState,
    metrics: &GatewayEthPluginRlpxSessionMetrics,
) {
    state.sessions_completed = state.sessions_completed.saturating_add(1);
    let has_gossip =
        metrics.new_pooled_hashes > 0 || metrics.pooled_txs_received > 0 || metrics.txs_msgs > 0;
    if has_gossip {
        state.sessions_with_gossip = state.sessions_with_gossip.saturating_add(1);
    }
    if let Some(code) = metrics.first_post_ready_code {
        state.last_first_post_ready_code = code;
    }
    let sample = gateway_eth_plugin_rlpx_learning_sample_score(metrics);
    state.last_sample_score = sample;
    if state.learning_score == 0 {
        state.learning_score = sample;
    } else {
        // Exponential smoothing keeps history but reacts to recent good peers.
        state.learning_score = state
            .learning_score
            .saturating_mul(7)
            .saturating_add(sample.saturating_mul(3))
            / 10;
    }
}

fn gateway_eth_plugin_rlpx_seen_retention_ms(chain_id: u64) -> u64 {
    gateway_eth_plugin_mempool_ingest_rlpx_recent_new_hash_window_ms(chain_id)
        .saturating_mul(GATEWAY_ETH_PLUGIN_RLPX_SEEN_RETENTION_MULTIPLIER)
        .clamp(60_000, 24 * 60 * 60 * 1_000)
}

fn gateway_eth_plugin_rlpx_seen_gc_if_needed(chain_id: u64, now_ms: u64) {
    let last_gc_map = gateway_eth_plugin_rlpx_seen_last_gc_map();
    let should_gc = if let Some(mut entry) = last_gc_map.get_mut(&chain_id) {
        if now_ms.saturating_sub(*entry) < GATEWAY_ETH_PLUGIN_RLPX_SEEN_GC_INTERVAL_MS {
            false
        } else {
            *entry = now_ms;
            true
        }
    } else {
        last_gc_map.insert(chain_id, now_ms);
        false
    };
    if !should_gc {
        return;
    }
    let cutoff_ms = now_ms.saturating_sub(gateway_eth_plugin_rlpx_seen_retention_ms(chain_id));
    gateway_eth_plugin_rlpx_seen_hashes_map()
        .retain(|key, first_seen_ms| key.chain_id != chain_id || *first_seen_ms >= cutoff_ms);
    gateway_eth_plugin_rlpx_seen_txs_map()
        .retain(|key, first_seen_ms| key.chain_id != chain_id || *first_seen_ms >= cutoff_ms);
}

fn gateway_eth_plugin_rlpx_mark_seen_hash(chain_id: u64, hash: [u8; 32], now_ms: u64) -> bool {
    gateway_eth_plugin_rlpx_seen_gc_if_needed(chain_id, now_ms);
    let key = GatewayEthPluginRlpxSeenKey { chain_id, hash };
    match gateway_eth_plugin_rlpx_seen_hashes_map().entry(key) {
        dashmap::mapref::entry::Entry::Occupied(mut entry) => {
            *entry.get_mut() = now_ms;
            false
        }
        dashmap::mapref::entry::Entry::Vacant(entry) => {
            entry.insert(now_ms);
            true
        }
    }
}

fn gateway_eth_plugin_rlpx_mark_seen_tx(chain_id: u64, hash: [u8; 32], now_ms: u64) -> bool {
    gateway_eth_plugin_rlpx_seen_gc_if_needed(chain_id, now_ms);
    let key = GatewayEthPluginRlpxSeenKey { chain_id, hash };
    match gateway_eth_plugin_rlpx_seen_txs_map().entry(key) {
        dashmap::mapref::entry::Entry::Occupied(mut entry) => {
            *entry.get_mut() = now_ms;
            false
        }
        dashmap::mapref::entry::Entry::Vacant(entry) => {
            entry.insert(now_ms);
            true
        }
    }
}

#[cfg(test)]
fn clear_gateway_eth_plugin_rlpx_seen_index(chain_id: u64) {
    gateway_eth_plugin_rlpx_seen_hashes_map().retain(|key, _| key.chain_id != chain_id);
    gateway_eth_plugin_rlpx_seen_txs_map().retain(|key, _| key.chain_id != chain_id);
    gateway_eth_plugin_rlpx_seen_last_gc_map().remove(&chain_id);
}

fn gateway_eth_plugin_rlpx_update_recent_new_hash_window(
    chain_id: u64,
    state: &mut GatewayEthPluginRlpxWorkerState,
    now_ms: u64,
    delta_new_hashes: u64,
) {
    if delta_new_hashes == 0 {
        return;
    }
    let window_ms = gateway_eth_plugin_mempool_ingest_rlpx_recent_new_hash_window_ms(chain_id);
    if state.recent_new_pooled_hashes_window_start_ms == 0
        || now_ms.saturating_sub(state.recent_new_pooled_hashes_window_start_ms) > window_ms
    {
        state.recent_new_pooled_hashes_window_start_ms = now_ms;
        state.recent_new_pooled_hashes_total = delta_new_hashes;
        return;
    }
    state.recent_new_pooled_hashes_total = state
        .recent_new_pooled_hashes_total
        .saturating_add(delta_new_hashes);
}

fn gateway_eth_plugin_rlpx_update_recent_dedup_window(
    chain_id: u64,
    state: &mut GatewayEthPluginRlpxWorkerState,
    now_ms: u64,
    delta_unique_hashes: u64,
    delta_unique_txs: u64,
    delta_duplicate_hashes: u64,
    delta_duplicate_txs: u64,
) {
    if delta_unique_hashes == 0
        && delta_unique_txs == 0
        && delta_duplicate_hashes == 0
        && delta_duplicate_txs == 0
    {
        return;
    }
    let window_ms = gateway_eth_plugin_mempool_ingest_rlpx_recent_new_hash_window_ms(chain_id);
    if state.recent_dedup_window_start_ms == 0
        || now_ms.saturating_sub(state.recent_dedup_window_start_ms) > window_ms
    {
        state.recent_dedup_window_start_ms = now_ms;
        state.recent_unique_new_pooled_hashes_total = delta_unique_hashes;
        state.recent_unique_pooled_txs_total = delta_unique_txs;
        state.recent_duplicate_new_pooled_hashes_total = delta_duplicate_hashes;
        state.recent_duplicate_pooled_txs_total = delta_duplicate_txs;
        return;
    }
    state.recent_unique_new_pooled_hashes_total = state
        .recent_unique_new_pooled_hashes_total
        .saturating_add(delta_unique_hashes);
    state.recent_unique_pooled_txs_total = state
        .recent_unique_pooled_txs_total
        .saturating_add(delta_unique_txs);
    state.recent_duplicate_new_pooled_hashes_total = state
        .recent_duplicate_new_pooled_hashes_total
        .saturating_add(delta_duplicate_hashes);
    state.recent_duplicate_pooled_txs_total = state
        .recent_duplicate_pooled_txs_total
        .saturating_add(delta_duplicate_txs);
}

fn gateway_eth_plugin_rlpx_update_recent_swap_window(
    chain_id: u64,
    state: &mut GatewayEthPluginRlpxWorkerState,
    now_ms: u64,
    delta_swap_hits: u64,
    delta_unique_swap_hits: u64,
) {
    if delta_swap_hits == 0 && delta_unique_swap_hits == 0 {
        return;
    }
    let window_ms = gateway_eth_plugin_mempool_ingest_rlpx_recent_new_hash_window_ms(chain_id);
    if state.recent_swap_window_start_ms == 0
        || now_ms.saturating_sub(state.recent_swap_window_start_ms) > window_ms
    {
        state.recent_swap_window_start_ms = now_ms;
        state.recent_swap_hits_total = delta_swap_hits;
        state.recent_unique_swap_hits_total = delta_unique_swap_hits;
        return;
    }
    state.recent_swap_hits_total = state.recent_swap_hits_total.saturating_add(delta_swap_hits);
    state.recent_unique_swap_hits_total = state
        .recent_unique_swap_hits_total
        .saturating_add(delta_unique_swap_hits);
}

fn gateway_eth_plugin_rlpx_apply_live_metrics_delta(
    chain_id: u64,
    worker_key: &str,
    metrics: &GatewayEthPluginRlpxSessionMetrics,
    applied: &mut GatewayEthPluginRlpxSessionMetrics,
) {
    let delta_new_pooled_msgs = metrics
        .new_pooled_msgs
        .saturating_sub(applied.new_pooled_msgs);
    let delta_new_pooled_hashes = metrics
        .new_pooled_hashes
        .saturating_sub(applied.new_pooled_hashes);
    let delta_unique_new_pooled_hashes = metrics
        .unique_new_pooled_hashes
        .saturating_sub(applied.unique_new_pooled_hashes);
    let delta_duplicate_new_pooled_hashes = metrics
        .duplicate_new_pooled_hashes
        .saturating_sub(applied.duplicate_new_pooled_hashes);
    let delta_get_pooled_sent = metrics
        .get_pooled_sent
        .saturating_sub(applied.get_pooled_sent);
    let delta_pooled_msgs = metrics.pooled_msgs.saturating_sub(applied.pooled_msgs);
    let delta_pooled_txs_received = metrics
        .pooled_txs_received
        .saturating_sub(applied.pooled_txs_received);
    let delta_unique_pooled_txs = metrics
        .unique_pooled_txs
        .saturating_sub(applied.unique_pooled_txs);
    let delta_duplicate_pooled_txs = metrics
        .duplicate_pooled_txs
        .saturating_sub(applied.duplicate_pooled_txs);
    let delta_pooled_txs_imported = metrics
        .pooled_txs_imported
        .saturating_sub(applied.pooled_txs_imported);
    let delta_first_seen_hashes = metrics
        .first_seen_hashes
        .saturating_sub(applied.first_seen_hashes);
    let delta_first_seen_txs = metrics
        .first_seen_txs
        .saturating_sub(applied.first_seen_txs);
    let delta_swap_hits = metrics.swap_hits.saturating_sub(applied.swap_hits);
    let delta_swap_v2_hits = metrics.swap_v2_hits.saturating_sub(applied.swap_v2_hits);
    let delta_swap_v3_hits = metrics.swap_v3_hits.saturating_sub(applied.swap_v3_hits);
    let delta_unique_swap_hits = metrics
        .unique_swap_hits
        .saturating_sub(applied.unique_swap_hits);
    let delta_first_gossip_latency_ms = metrics
        .first_gossip_latency_ms
        .saturating_sub(applied.first_gossip_latency_ms);
    let delta_first_swap_latency_ms = metrics
        .first_swap_latency_ms
        .saturating_sub(applied.first_swap_latency_ms);
    let delta_txs_msgs = metrics.txs_msgs.saturating_sub(applied.txs_msgs);
    let has_delta = delta_new_pooled_msgs > 0
        || delta_new_pooled_hashes > 0
        || delta_unique_new_pooled_hashes > 0
        || delta_duplicate_new_pooled_hashes > 0
        || delta_get_pooled_sent > 0
        || delta_pooled_msgs > 0
        || delta_pooled_txs_received > 0
        || delta_unique_pooled_txs > 0
        || delta_duplicate_pooled_txs > 0
        || delta_pooled_txs_imported > 0
        || delta_first_seen_hashes > 0
        || delta_first_seen_txs > 0
        || delta_swap_hits > 0
        || delta_swap_v2_hits > 0
        || delta_swap_v3_hits > 0
        || delta_unique_swap_hits > 0
        || delta_first_gossip_latency_ms > 0
        || delta_first_swap_latency_ms > 0
        || delta_txs_msgs > 0
        || metrics.first_post_ready_code.is_some();
    if !has_delta {
        return;
    }
    let now_ms = now_unix_millis() as u64;
    update_gateway_eth_plugin_rlpx_worker_state(worker_key, |state| {
        state.total_new_pooled_msgs = state
            .total_new_pooled_msgs
            .saturating_add(delta_new_pooled_msgs);
        state.total_new_pooled_hashes = state
            .total_new_pooled_hashes
            .saturating_add(delta_new_pooled_hashes);
        state.total_unique_new_pooled_hashes = state
            .total_unique_new_pooled_hashes
            .saturating_add(delta_unique_new_pooled_hashes);
        state.total_duplicate_new_pooled_hashes = state
            .total_duplicate_new_pooled_hashes
            .saturating_add(delta_duplicate_new_pooled_hashes);
        state.total_get_pooled_sent = state
            .total_get_pooled_sent
            .saturating_add(delta_get_pooled_sent);
        state.total_pooled_msgs = state.total_pooled_msgs.saturating_add(delta_pooled_msgs);
        state.total_pooled_txs_received = state
            .total_pooled_txs_received
            .saturating_add(delta_pooled_txs_received);
        state.total_unique_pooled_txs = state
            .total_unique_pooled_txs
            .saturating_add(delta_unique_pooled_txs);
        state.total_duplicate_pooled_txs = state
            .total_duplicate_pooled_txs
            .saturating_add(delta_duplicate_pooled_txs);
        state.total_pooled_txs_imported = state
            .total_pooled_txs_imported
            .saturating_add(delta_pooled_txs_imported);
        state.first_seen_hash_count = state
            .first_seen_hash_count
            .saturating_add(delta_first_seen_hashes);
        state.first_seen_tx_count = state
            .first_seen_tx_count
            .saturating_add(delta_first_seen_txs);
        state.total_swap_hits = state.total_swap_hits.saturating_add(delta_swap_hits);
        state.total_swap_v2_hits = state.total_swap_v2_hits.saturating_add(delta_swap_v2_hits);
        state.total_swap_v3_hits = state.total_swap_v3_hits.saturating_add(delta_swap_v3_hits);
        state.total_unique_swap_hits = state
            .total_unique_swap_hits
            .saturating_add(delta_unique_swap_hits);
        state.total_txs_msgs = state.total_txs_msgs.saturating_add(delta_txs_msgs);
        if delta_new_pooled_hashes > 0 || delta_pooled_txs_received > 0 || delta_txs_msgs > 0 {
            state.last_new_pooled_ms = now_ms;
        }
        if delta_swap_hits > 0 {
            state.last_swap_ms = now_ms;
        }
        gateway_eth_plugin_rlpx_update_recent_new_hash_window(
            chain_id,
            state,
            now_ms,
            delta_new_pooled_hashes,
        );
        gateway_eth_plugin_rlpx_update_recent_dedup_window(
            chain_id,
            state,
            now_ms,
            delta_unique_new_pooled_hashes,
            delta_unique_pooled_txs,
            delta_duplicate_new_pooled_hashes,
            delta_duplicate_pooled_txs,
        );
        gateway_eth_plugin_rlpx_update_recent_swap_window(
            chain_id,
            state,
            now_ms,
            delta_swap_hits,
            delta_unique_swap_hits,
        );
        if delta_first_gossip_latency_ms > 0 {
            state.total_first_gossip_latency_ms = state
                .total_first_gossip_latency_ms
                .saturating_add(delta_first_gossip_latency_ms);
            state.first_gossip_latency_samples =
                state.first_gossip_latency_samples.saturating_add(1);
        }
        if delta_first_swap_latency_ms > 0 {
            state.total_first_swap_latency_ms = state
                .total_first_swap_latency_ms
                .saturating_add(delta_first_swap_latency_ms);
            state.first_swap_latency_samples = state.first_swap_latency_samples.saturating_add(1);
        }
        if let Some(code) = metrics.first_post_ready_code {
            state.last_first_post_ready_code = code;
        }
    });
    applied.new_pooled_msgs = metrics.new_pooled_msgs;
    applied.new_pooled_hashes = metrics.new_pooled_hashes;
    applied.unique_new_pooled_hashes = metrics.unique_new_pooled_hashes;
    applied.duplicate_new_pooled_hashes = metrics.duplicate_new_pooled_hashes;
    applied.get_pooled_sent = metrics.get_pooled_sent;
    applied.pooled_msgs = metrics.pooled_msgs;
    applied.pooled_txs_received = metrics.pooled_txs_received;
    applied.unique_pooled_txs = metrics.unique_pooled_txs;
    applied.duplicate_pooled_txs = metrics.duplicate_pooled_txs;
    applied.pooled_txs_imported = metrics.pooled_txs_imported;
    applied.first_seen_hashes = metrics.first_seen_hashes;
    applied.first_seen_txs = metrics.first_seen_txs;
    applied.swap_hits = metrics.swap_hits;
    applied.swap_v2_hits = metrics.swap_v2_hits;
    applied.swap_v3_hits = metrics.swap_v3_hits;
    applied.unique_swap_hits = metrics.unique_swap_hits;
    applied.first_gossip_latency_ms = metrics.first_gossip_latency_ms;
    applied.first_swap_latency_ms = metrics.first_swap_latency_ms;
    applied.txs_msgs = metrics.txs_msgs;
    if metrics.first_post_ready_code.is_some() {
        applied.first_post_ready_code = metrics.first_post_ready_code;
    }
}

fn gateway_eth_plugin_rlpx_worker_tier_rank(
    chain_id: u64,
    state: &GatewayEthPluginRlpxWorkerState,
    now_ms: u64,
) -> u8 {
    let has_gossip_history = state.sessions_with_gossip > 0
        || state.total_new_pooled_hashes > 0
        || state.total_unique_new_pooled_hashes > 0
        || state.total_pooled_txs_received > 0
        || state.total_unique_pooled_txs > 0
        || state.total_pooled_txs_imported > 0;
    let core_recent_window_ms =
        gateway_eth_plugin_mempool_ingest_rlpx_core_recent_gossip_window_ms(chain_id);
    let core_lock_ms = gateway_eth_plugin_mempool_ingest_rlpx_core_lock_ms(chain_id);
    let active_recent_window_ms =
        gateway_eth_plugin_mempool_ingest_rlpx_active_recent_ready_window_ms(chain_id);
    let recent_hash_min = gateway_eth_plugin_mempool_ingest_rlpx_recent_new_hash_min(chain_id);
    let swap_priority_enabled =
        gateway_eth_plugin_mempool_ingest_rlpx_swap_priority_enabled(chain_id);
    let has_recent_new_hash_window = state.recent_new_pooled_hashes_window_start_ms > 0
        && now_ms.saturating_sub(state.recent_new_pooled_hashes_window_start_ms)
            <= gateway_eth_plugin_mempool_ingest_rlpx_recent_new_hash_window_ms(chain_id)
        && state
            .recent_new_pooled_hashes_total
            .max(state.recent_unique_new_pooled_hashes_total)
            >= recent_hash_min;
    let is_recent_gossip = state.last_new_pooled_ms > 0
        && now_ms.saturating_sub(state.last_new_pooled_ms) <= core_recent_window_ms;
    let strong_unique_history = state
        .total_unique_new_pooled_hashes
        .max(state.first_seen_hash_count)
        >= recent_hash_min.saturating_mul(16)
        || state.total_unique_pooled_txs.max(state.first_seen_tx_count)
            >= recent_hash_min.saturating_mul(8);
    let elite_unique_history = state
        .total_unique_new_pooled_hashes
        .max(state.first_seen_hash_count)
        >= recent_hash_min.saturating_mul(32)
        || state.total_unique_pooled_txs.max(state.first_seen_tx_count)
            >= recent_hash_min.saturating_mul(16);
    let strong_unique_history_recent_ready = strong_unique_history
        && state.last_success_ms > 0
        && now_ms.saturating_sub(state.last_success_ms)
            <= active_recent_window_ms
                .saturating_mul(2)
                .max(core_recent_window_ms);
    let is_core_locked = has_gossip_history
        && core_lock_ms > 0
        && state.last_success_ms > 0
        && now_ms.saturating_sub(state.last_success_ms) <= core_lock_ms;
    let is_recent_swap = state.last_swap_ms > 0
        && now_ms.saturating_sub(state.last_swap_ms) <= core_recent_window_ms.saturating_mul(2);
    let strong_swap_history = state.total_swap_hits >= recent_hash_min.saturating_div(2).max(1)
        || state.total_unique_swap_hits >= recent_hash_min.saturating_div(4).max(1);
    let has_swap_history = state.last_swap_ms > 0
        || state.recent_swap_hits_total > 0
        || state.recent_unique_swap_hits_total > 0
        || state.total_swap_hits > 0
        || state.total_unique_swap_hits > 0;
    if swap_priority_enabled {
        let swap_core_ok = is_recent_swap
            || strong_swap_history
            || (has_swap_history
                && (is_recent_gossip
                    || has_recent_new_hash_window
                    || is_core_locked
                    || strong_unique_history_recent_ready
                    || elite_unique_history));
        if has_gossip_history && swap_core_ok {
            return 0; // core
        }
    } else if has_gossip_history
        && (is_recent_gossip
            || has_recent_new_hash_window
            || is_core_locked
            || strong_unique_history_recent_ready
            || elite_unique_history)
    {
        return 0; // core
    }
    let is_recent_ready = state.last_success_ms > 0
        && now_ms.saturating_sub(state.last_success_ms) <= active_recent_window_ms;
    if is_recent_ready || (state.sessions_completed > 0 && state.learning_score > 0) {
        return 1; // active
    }
    2 // candidate
}

fn gateway_eth_plugin_rlpx_worker_tier_label(
    chain_id: u64,
    state: &GatewayEthPluginRlpxWorkerState,
    now_ms: u64,
) -> &'static str {
    match gateway_eth_plugin_rlpx_worker_tier_rank(chain_id, state, now_ms) {
        0 => "core",
        1 => "active",
        _ => "candidate",
    }
}

fn gateway_eth_plugin_rlpx_worker_sort_key(
    chain_id: u64,
    state: &GatewayEthPluginRlpxWorkerState,
    now_ms: u64,
    peer_index: usize,
    peer_count: usize,
    rotation: usize,
) -> (u8, u64, u64, u64, u64, usize) {
    let rotated_index = if peer_count == 0 {
        peer_index
    } else {
        (peer_index + peer_count - (rotation % peer_count)) % peer_count
    };
    let priority_signal = gateway_eth_plugin_rlpx_worker_priority_signal(chain_id, state);
    if state.cooldown_until_ms > now_ms {
        return (
            9,
            state.cooldown_until_ms.saturating_sub(now_ms),
            state.consecutive_failures as u64,
            u64::MAX.saturating_sub(priority_signal),
            u64::MAX.saturating_sub(state.learning_score),
            rotated_index,
        );
    }
    // Tier-first scheduling: core > active > candidate.
    (
        gateway_eth_plugin_rlpx_worker_tier_rank(chain_id, state, now_ms),
        u64::MAX.saturating_sub(priority_signal),
        u64::MAX.saturating_sub(state.learning_score),
        u64::MAX.saturating_sub(state.last_success_ms),
        state.consecutive_failures as u64,
        rotated_index,
    )
}

fn gateway_eth_plugin_rlpx_error_is_protocol_like(err: &str) -> bool {
    err.contains("rlpx_")
        || err.contains("hello")
        || err.contains("status")
        || err.contains("frame")
        || err.contains("mac_mismatch")
        || err.contains("decode")
}

fn gateway_eth_plugin_rlpx_error_is_timeout_like(err: &str) -> bool {
    err.contains("read_timeout")
        || err.contains("connect_failed")
        || err.contains("timed out")
        || err.contains("os error 10060")
        || err.contains("os error 10035")
}

fn gateway_eth_plugin_rlpx_disconnect_bucket(err: &str) -> &'static str {
    if err.contains("too_many_peers") {
        return "too_many_peers";
    }
    if gateway_eth_plugin_rlpx_error_is_timeout_like(err) {
        return "timeout";
    }
    if gateway_eth_plugin_rlpx_error_is_protocol_like(err) {
        return "protocol";
    }
    if err.contains("eof") || err.contains("remote_disconnected") {
        return "remote_close";
    }
    "other"
}

fn gateway_eth_plugin_rlpx_update_disconnect_counters(
    state: &mut GatewayEthPluginRlpxWorkerState,
    err: &str,
) {
    state.disconnect_count = state.disconnect_count.saturating_add(1);
    match gateway_eth_plugin_rlpx_disconnect_bucket(err) {
        "too_many_peers" => {
            state.disconnect_too_many_count = state.disconnect_too_many_count.saturating_add(1)
        }
        "timeout" => {
            state.disconnect_timeout_count = state.disconnect_timeout_count.saturating_add(1)
        }
        "protocol" => {
            state.disconnect_protocol_count = state.disconnect_protocol_count.saturating_add(1)
        }
        _ => state.disconnect_other_count = state.disconnect_other_count.saturating_add(1),
    }
}

fn gateway_eth_plugin_rlpx_worker_disconnect_rate_bps(
    state: &GatewayEthPluginRlpxWorkerState,
) -> u64 {
    if state.dial_attempt_count == 0 {
        return 0;
    }
    state
        .disconnect_count
        .saturating_mul(10_000)
        .saturating_div(state.dial_attempt_count)
}

fn gateway_eth_plugin_rlpx_worker_success_rate_bps(state: &GatewayEthPluginRlpxWorkerState) -> u64 {
    if state.dial_attempt_count == 0 {
        return 0;
    }
    state
        .sessions_completed
        .saturating_mul(10_000)
        .saturating_div(state.dial_attempt_count)
}

fn gateway_eth_plugin_rlpx_avg_latency_ms(total_ms: u64, samples: u64) -> u64 {
    if samples == 0 {
        return 0;
    }
    total_ms.saturating_div(samples.max(1))
}

fn gateway_eth_plugin_rlpx_worker_priority_signal(
    chain_id: u64,
    state: &GatewayEthPluginRlpxWorkerState,
) -> u64 {
    let recent_unique = state
        .recent_unique_new_pooled_hashes_total
        .saturating_add(state.recent_unique_pooled_txs_total);
    let recent_duplicate = state
        .recent_duplicate_new_pooled_hashes_total
        .saturating_add(state.recent_duplicate_pooled_txs_total);
    let total_unique = state
        .total_unique_new_pooled_hashes
        .saturating_add(state.total_unique_pooled_txs);
    let capped_first_seen_hash = state.first_seen_hash_count.min(250_000);
    let capped_first_seen_tx = state.first_seen_tx_count.min(50_000);
    let capped_total_unique_new_hashes = state.total_unique_new_pooled_hashes.min(50_000);
    let capped_total_unique_pooled_txs = state.total_unique_pooled_txs.min(25_000);
    let mut signal = state.learning_score;
    signal = signal
        .saturating_add(state.sessions_with_gossip.saturating_mul(128))
        .saturating_add(
            state
                .recent_unique_new_pooled_hashes_total
                .saturating_mul(64),
        )
        .saturating_add(state.recent_unique_pooled_txs_total.saturating_mul(2048))
        .saturating_add(capped_first_seen_hash.saturating_mul(8))
        .saturating_add(capped_first_seen_tx.saturating_mul(256))
        .saturating_add(capped_total_unique_new_hashes.saturating_mul(2))
        .saturating_add(capped_total_unique_pooled_txs.saturating_mul(64))
        .saturating_add(state.total_pooled_txs_imported.saturating_mul(32));
    if recent_unique == 0 {
        if recent_duplicate > 64 {
            signal = signal.saturating_sub(200_000);
        }
    } else {
        if recent_duplicate > recent_unique.saturating_mul(4) && recent_duplicate > 64 {
            signal = signal.saturating_sub(100_000);
        }
        if recent_duplicate > recent_unique.saturating_mul(8) && recent_duplicate > 128 {
            signal = signal.saturating_sub(300_000);
        }
    }
    if total_unique > 0
        && state
            .total_duplicate_new_pooled_hashes
            .saturating_add(state.total_duplicate_pooled_txs)
            > total_unique.saturating_mul(12)
    {
        signal = signal.saturating_sub(75_000);
    }
    if state.disconnect_too_many_count > 0 {
        let too_many_penalty = state
            .disconnect_too_many_count
            .min(512)
            .saturating_mul(1_500);
        signal = signal.saturating_sub(too_many_penalty);
    }
    if state.disconnect_too_many_count >= 32 {
        signal = signal.saturating_sub(250_000);
    }
    if state.disconnect_too_many_count >= 96 {
        signal = signal.saturating_sub(500_000);
    }
    if gateway_eth_plugin_mempool_ingest_rlpx_swap_priority_enabled(chain_id) {
        let latency_target_ms =
            gateway_eth_plugin_mempool_ingest_rlpx_swap_priority_latency_target_ms(chain_id);
        let has_recent_swap =
            state.recent_swap_hits_total > 0 || state.recent_unique_swap_hits_total > 0;
        let has_any_swap =
            has_recent_swap || state.total_swap_hits > 0 || state.total_unique_swap_hits > 0;
        signal = signal
            .saturating_add(state.recent_swap_hits_total.saturating_mul(8_192))
            .saturating_add(state.recent_unique_swap_hits_total.saturating_mul(16_384))
            .saturating_add(state.total_swap_hits.saturating_mul(96))
            .saturating_add(state.total_unique_swap_hits.saturating_mul(256))
            .saturating_add(state.total_swap_v2_hits.saturating_mul(32))
            .saturating_add(state.total_swap_v3_hits.saturating_mul(48));
        if !has_recent_swap {
            signal = signal.saturating_sub(200_000);
            if state.sessions_with_gossip > 0 {
                signal = signal.saturating_sub(120_000);
            }
        }
        if !has_any_swap {
            signal = signal.saturating_sub(350_000);
        }
        let avg_first_swap_ms = gateway_eth_plugin_rlpx_avg_latency_ms(
            state.total_first_swap_latency_ms,
            state.first_swap_latency_samples,
        );
        if avg_first_swap_ms > 0 {
            if avg_first_swap_ms <= latency_target_ms.saturating_div(2).max(1) {
                signal = signal.saturating_add(600_000);
            } else if avg_first_swap_ms <= latency_target_ms {
                signal = signal.saturating_add(350_000);
            } else if avg_first_swap_ms <= latency_target_ms.saturating_mul(2) {
                signal = signal.saturating_add(120_000);
            } else {
                signal = signal.saturating_sub(100_000);
            }
        }
        let avg_first_gossip_ms = gateway_eth_plugin_rlpx_avg_latency_ms(
            state.total_first_gossip_latency_ms,
            state.first_gossip_latency_samples,
        );
        if avg_first_gossip_ms > 0 {
            if avg_first_gossip_ms <= latency_target_ms.saturating_div(2).max(1) {
                signal = signal.saturating_add(250_000);
            } else if avg_first_gossip_ms <= latency_target_ms {
                signal = signal.saturating_add(150_000);
            } else if avg_first_gossip_ms > latency_target_ms.saturating_mul(2) {
                signal = signal.saturating_sub(50_000);
            }
        }
    }
    signal
}

fn gateway_eth_plugin_rlpx_worker_score(
    chain_id: u64,
    state: &GatewayEthPluginRlpxWorkerState,
) -> i64 {
    let mut score = 0i64;
    if state.sessions_completed > 0 {
        score += 20;
    }
    if state.sessions_completed >= 3 {
        score += 10;
    }
    if state.sessions_with_gossip > 0 {
        score += 30;
    }
    if state.total_new_pooled_msgs > 0 {
        score += 10;
    }
    if state.total_new_pooled_hashes >= 100 {
        score += 10;
    }
    if state.total_pooled_txs_received > 0 || state.total_pooled_txs_imported > 0 {
        score += 20;
    }
    if state.total_unique_new_pooled_hashes > 0 {
        score += 15;
    }
    if state.total_unique_pooled_txs > 0 {
        score += 20;
    }
    if state.recent_unique_new_pooled_hashes_total >= 64 {
        score += 40;
    } else if state.recent_unique_new_pooled_hashes_total > 0 {
        score += 20;
    }
    if state.recent_unique_pooled_txs_total > 0 {
        score += 40;
    }
    let total_unique = state
        .total_unique_new_pooled_hashes
        .saturating_add(state.total_unique_pooled_txs);
    let total_duplicate = state
        .total_duplicate_new_pooled_hashes
        .saturating_add(state.total_duplicate_pooled_txs);
    if total_unique > 0 && total_duplicate > total_unique.saturating_mul(10) {
        score -= 10;
    }
    if state.first_seen_hash_count > 0 {
        score += 20;
    }
    if state.first_seen_tx_count > 0 {
        score += 20;
    }
    let recent_unique = state
        .recent_unique_new_pooled_hashes_total
        .saturating_add(state.recent_unique_pooled_txs_total);
    let recent_duplicate = state
        .recent_duplicate_new_pooled_hashes_total
        .saturating_add(state.recent_duplicate_pooled_txs_total);
    if recent_unique > 0
        && recent_duplicate > recent_unique.saturating_mul(3)
        && recent_duplicate > 64
    {
        score -= 20;
    }
    if recent_unique > 0
        && recent_duplicate > recent_unique.saturating_mul(8)
        && recent_duplicate > 128
    {
        score -= 40;
    }
    if recent_unique == 0 && recent_duplicate > 64 {
        score -= 30;
    }
    if state.disconnect_count >= 5 {
        score -= 10;
    }
    if state.disconnect_protocol_count >= 3 {
        score -= 20;
    }
    if state.dial_attempt_count >= 5 && state.sessions_completed == 0 {
        score -= 30;
    }
    if state.disconnect_too_many_count >= 3 {
        score -= 15;
    }
    if state.disconnect_too_many_count >= 16 {
        score -= 25;
    }
    if state.disconnect_too_many_count >= 64 {
        score -= 35;
    }
    if recent_unique == 0 && state.disconnect_too_many_count >= 8 {
        score -= 20;
    }
    if gateway_eth_plugin_mempool_ingest_rlpx_swap_priority_enabled(chain_id) {
        let has_recent_swap =
            state.recent_swap_hits_total > 0 || state.recent_unique_swap_hits_total > 0;
        let has_any_swap =
            has_recent_swap || state.total_swap_hits > 0 || state.total_unique_swap_hits > 0;
        score += if state.recent_swap_hits_total >= 16 {
            220
        } else if state.recent_swap_hits_total >= 8 {
            150
        } else if state.recent_swap_hits_total > 0 {
            90
        } else {
            -180
        };
        if state.recent_unique_swap_hits_total >= 8 {
            score += 160;
        } else if state.recent_unique_swap_hits_total > 0 {
            score += 90;
        }
        if state.total_swap_hits >= 64 {
            score += 90;
        } else if state.total_swap_hits >= 16 {
            score += 50;
        } else if state.total_swap_hits > 0 {
            score += 20;
        }
        if state.total_unique_swap_hits > 0 {
            score += 40;
        }
        if !has_any_swap {
            score -= 220;
        } else if !has_recent_swap && state.sessions_with_gossip > 0 {
            score -= 80;
        }
        let latency_target_ms =
            gateway_eth_plugin_mempool_ingest_rlpx_swap_priority_latency_target_ms(chain_id);
        let avg_first_swap_ms = gateway_eth_plugin_rlpx_avg_latency_ms(
            state.total_first_swap_latency_ms,
            state.first_swap_latency_samples,
        );
        if avg_first_swap_ms > 0 {
            if avg_first_swap_ms <= latency_target_ms {
                score += 120;
            } else if avg_first_swap_ms <= latency_target_ms.saturating_mul(2) {
                score += 40;
            } else {
                score -= 60;
            }
        }
        let avg_first_gossip_ms = gateway_eth_plugin_rlpx_avg_latency_ms(
            state.total_first_gossip_latency_ms,
            state.first_gossip_latency_samples,
        );
        if avg_first_gossip_ms > 0 {
            if avg_first_gossip_ms <= latency_target_ms {
                score += 40;
            } else if avg_first_gossip_ms > latency_target_ms.saturating_mul(2) {
                score -= 20;
            }
        }
    }
    score
}

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
struct GatewayEthPluginRlpxPriorityAutoEntry {
    addr_hint: String,
    tier_rank: u8,
    score: i64,
    learning_score: u64,
    priority_signal: u64,
    sessions_with_gossip: u64,
    total_new_pooled_hashes: u64,
    total_unique_new_pooled_hashes: u64,
    recent_unique_new_pooled_hashes_total: u64,
    recent_duplicate_new_pooled_hashes_total: u64,
    total_pooled_txs_received: u64,
    total_unique_pooled_txs: u64,
    recent_unique_pooled_txs_total: u64,
    recent_duplicate_pooled_txs_total: u64,
    total_pooled_txs_imported: u64,
    first_seen_hash_count: u64,
    first_seen_tx_count: u64,
}

fn gateway_eth_plugin_rlpx_priority_auto_signal(
    entry: &GatewayEthPluginRlpxPriorityAutoEntry,
) -> u64 {
    let mut signal = entry
        .priority_signal
        .saturating_add(entry.recent_unique_pooled_txs_total.saturating_mul(1024))
        .saturating_add(
            entry
                .recent_unique_new_pooled_hashes_total
                .saturating_mul(64),
        )
        .saturating_add(entry.first_seen_tx_count.saturating_mul(256))
        .saturating_add(entry.first_seen_hash_count.saturating_mul(8));
    let recent_unique = entry
        .recent_unique_new_pooled_hashes_total
        .saturating_add(entry.recent_unique_pooled_txs_total);
    let recent_duplicate = entry
        .recent_duplicate_new_pooled_hashes_total
        .saturating_add(entry.recent_duplicate_pooled_txs_total);
    if recent_unique == 0 {
        if recent_duplicate > 64 {
            signal = signal.saturating_sub(250_000);
        }
    } else {
        if recent_duplicate > recent_unique.saturating_mul(4) && recent_duplicate > 64 {
            signal = signal.saturating_sub(150_000);
        }
        if recent_duplicate > recent_unique.saturating_mul(8) && recent_duplicate > 128 {
            signal = signal.saturating_sub(400_000);
        }
    }
    signal
}

fn gateway_eth_plugin_rlpx_select_auto_priority_addr_hints(
    entries: &[GatewayEthPluginRlpxPriorityAutoEntry],
    max_size: usize,
) -> std::collections::HashSet<String> {
    let mut out = std::collections::HashSet::<String>::new();
    if max_size == 0 || entries.is_empty() {
        return out;
    }
    let mut ranked = entries
        .iter()
        .filter(|entry| {
            entry.tier_rank <= 1
                || entry.recent_unique_new_pooled_hashes_total > 0
                || entry.recent_unique_pooled_txs_total > 0
                || entry.first_seen_hash_count > 0
                || entry.first_seen_tx_count > 0
                || entry.sessions_with_gossip > 0
                || entry.total_new_pooled_hashes > 0
                || entry.total_unique_new_pooled_hashes > 0
                || entry.total_pooled_txs_received > 0
                || entry.total_unique_pooled_txs > 0
                || entry.total_pooled_txs_imported > 0
        })
        .cloned()
        .collect::<Vec<_>>();
    ranked.sort_by(|a, b| {
        a.tier_rank
            .cmp(&b.tier_rank)
            .then_with(|| {
                gateway_eth_plugin_rlpx_priority_auto_signal(b)
                    .cmp(&gateway_eth_plugin_rlpx_priority_auto_signal(a))
            })
            .then_with(|| b.score.cmp(&a.score))
            .then_with(|| b.first_seen_tx_count.cmp(&a.first_seen_tx_count))
            .then_with(|| b.first_seen_hash_count.cmp(&a.first_seen_hash_count))
            .then_with(|| {
                b.recent_unique_pooled_txs_total
                    .cmp(&a.recent_unique_pooled_txs_total)
            })
            .then_with(|| {
                b.recent_unique_new_pooled_hashes_total
                    .cmp(&a.recent_unique_new_pooled_hashes_total)
            })
            .then_with(|| b.total_unique_pooled_txs.cmp(&a.total_unique_pooled_txs))
            .then_with(|| {
                b.total_unique_new_pooled_hashes
                    .cmp(&a.total_unique_new_pooled_hashes)
            })
            .then_with(|| {
                b.total_pooled_txs_imported
                    .cmp(&a.total_pooled_txs_imported)
            })
            .then_with(|| {
                b.total_pooled_txs_received
                    .cmp(&a.total_pooled_txs_received)
            })
            .then_with(|| b.total_new_pooled_hashes.cmp(&a.total_new_pooled_hashes))
            .then_with(|| b.learning_score.cmp(&a.learning_score))
            .then_with(|| a.addr_hint.cmp(&b.addr_hint))
    });
    for entry in ranked.into_iter().take(max_size) {
        out.insert(entry.addr_hint);
    }
    out
}

struct GatewayEthPluginRlpxTickCandidate<'a> {
    peer: &'a PluginPeerEndpoint,
    worker_key: String,
    normalized_addr_hint: String,
    tier_rank: u8,
    is_priority: bool,
    score: i64,
    learning_score: u64,
    sessions_with_gossip: u64,
    total_new_pooled_hashes: u64,
    total_unique_new_pooled_hashes: u64,
    recent_unique_new_pooled_hashes_total: u64,
    recent_duplicate_new_pooled_hashes_total: u64,
    total_pooled_txs_received: u64,
    total_unique_pooled_txs: u64,
    recent_unique_pooled_txs_total: u64,
    recent_duplicate_pooled_txs_total: u64,
    total_pooled_txs_imported: u64,
    first_seen_hash_count: u64,
    first_seen_tx_count: u64,
    disconnect_too_many_count: u64,
    last_success_ms: u64,
    priority_signal: u64,
    sort_key: (u8, u64, u64, u64, u64, usize),
}

fn gateway_eth_plugin_rlpx_tier_budgets(
    max_peers: usize,
    core_count: usize,
    active_count: usize,
    core_target: usize,
    active_target: usize,
) -> (usize, usize, usize) {
    if max_peers == 0 {
        return (0, 0, 0);
    }
    let core_shortfall = core_count < core_target;
    let active_shortfall = active_count < active_target;
    let severe_core_shortfall =
        core_count == 0 || core_count.saturating_mul(2) < core_target.max(1);
    let (core_ratio, active_ratio, mut candidate_ratio) = if core_shortfall && severe_core_shortfall
    {
        (40usize, 20usize, 40usize)
    } else if core_shortfall {
        (50usize, 25usize, 25usize)
    } else if active_shortfall {
        (55usize, 30usize, 15usize)
    } else {
        (60usize, 25usize, 15usize)
    };
    let mut core_budget = (max_peers * core_ratio).div_ceil(100);
    let mut active_budget = (max_peers * active_ratio).div_ceil(100);
    if core_budget == 0 {
        core_budget = 1;
    }
    if core_budget > max_peers {
        core_budget = max_peers;
    }
    if active_budget > max_peers.saturating_sub(core_budget) {
        active_budget = max_peers.saturating_sub(core_budget);
    }
    if core_shortfall && candidate_ratio == 0 {
        candidate_ratio = 1;
    }
    let mut candidate_budget = max_peers.saturating_sub(core_budget.saturating_add(active_budget));
    if core_shortfall && candidate_budget == 0 && max_peers > 1 {
        if active_budget > 1 {
            active_budget = active_budget.saturating_sub(1);
        } else if core_budget > 1 {
            core_budget = core_budget.saturating_sub(1);
        }
        candidate_budget = 1;
    }
    if candidate_ratio > 0 && candidate_budget == 0 && max_peers > 2 && active_budget > 1 {
        active_budget = active_budget.saturating_sub(1);
        candidate_budget = 1;
    }
    (core_budget, active_budget, candidate_budget)
}

fn gateway_eth_plugin_rlpx_partition_tick_candidates(
    candidates: &[GatewayEthPluginRlpxTickCandidate<'_>],
) -> (Vec<usize>, Vec<usize>, Vec<usize>, Vec<usize>) {
    let mut core_bucket = Vec::<usize>::new();
    let mut active_bucket = Vec::<usize>::new();
    let mut candidate_bucket = Vec::<usize>::new();
    let mut priority_bucket = Vec::<usize>::new();
    for (index, candidate) in candidates.iter().enumerate() {
        if candidate.is_priority {
            priority_bucket.push(index);
        }
        match candidate.tier_rank {
            0 => core_bucket.push(index),
            1 => active_bucket.push(index),
            _ => candidate_bucket.push(index),
        }
    }
    (
        core_bucket,
        active_bucket,
        candidate_bucket,
        priority_bucket,
    )
}

fn gateway_eth_plugin_rlpx_core_fallback_update_tick(
    chain_id: u64,
    core_count: usize,
    core_floor: usize,
    trigger_ticks: u64,
    hold_ticks: u64,
) -> GatewayEthPluginRlpxCoreFallbackTickState {
    let mut state = gateway_eth_plugin_rlpx_core_fallback_state_map()
        .get(&chain_id)
        .map(|entry| entry.clone())
        .unwrap_or_default();
    if core_count < core_floor.max(1) {
        state.low_core_streak = state.low_core_streak.saturating_add(1);
    } else {
        state.low_core_streak = 0;
    }
    let trigger_ticks = trigger_ticks.max(1);
    let hold_ticks = hold_ticks.max(1);
    if state.low_core_streak >= trigger_ticks && state.hold_ticks_remaining == 0 {
        state.hold_ticks_remaining = hold_ticks;
        state.activation_count = state.activation_count.saturating_add(1);
    }
    let active = state.hold_ticks_remaining > 0;
    if state.hold_ticks_remaining > 0 {
        state.hold_ticks_remaining = state.hold_ticks_remaining.saturating_sub(1);
    }
    gateway_eth_plugin_rlpx_core_fallback_state_map().insert(chain_id, state.clone());
    GatewayEthPluginRlpxCoreFallbackTickState {
        active,
        low_core_streak: state.low_core_streak,
        hold_ticks_remaining: state.hold_ticks_remaining,
        activation_count: state.activation_count,
    }
}

fn gateway_eth_plugin_rlpx_apply_core_fallback_candidate_budget(
    core_budget: usize,
    active_budget: usize,
    candidate_budget: usize,
    max_peers: usize,
    candidate_budget_min: usize,
) -> (usize, usize, usize) {
    if max_peers == 0 {
        return (0, 0, 0);
    }
    let mut core_budget = core_budget.min(max_peers);
    let mut active_budget = active_budget.min(max_peers.saturating_sub(core_budget));
    let mut candidate_budget =
        candidate_budget.min(max_peers.saturating_sub(core_budget.saturating_add(active_budget)));
    let candidate_budget_min = candidate_budget_min.min(max_peers);
    if candidate_budget >= candidate_budget_min {
        return (core_budget, active_budget, candidate_budget);
    }
    let mut need = candidate_budget_min.saturating_sub(candidate_budget);
    if need > 0 {
        let active_take = need.min(active_budget.saturating_sub(1));
        active_budget = active_budget.saturating_sub(active_take);
        candidate_budget = candidate_budget.saturating_add(active_take);
        need = need.saturating_sub(active_take);
    }
    if need > 0 {
        let core_take = need.min(core_budget.saturating_sub(1));
        core_budget = core_budget.saturating_sub(core_take);
        candidate_budget = candidate_budget.saturating_add(core_take);
    }
    let overflow = core_budget
        .saturating_add(active_budget)
        .saturating_add(candidate_budget)
        .saturating_sub(max_peers);
    if overflow > 0 {
        candidate_budget = candidate_budget.saturating_sub(overflow);
    }
    (core_budget, active_budget, candidate_budget)
}

fn gateway_eth_plugin_rlpx_should_demote_congested_peer(
    tier_rank: u8,
    disconnect_too_many_count: u64,
    recent_unique_new_hashes: u64,
    recent_unique_pooled_txs: u64,
    recent_duplicate_new_hashes: u64,
    recent_duplicate_pooled_txs: u64,
    last_success_ms: u64,
    now_ms: u64,
    stale_window_ms: u64,
    too_many_threshold: u64,
) -> bool {
    if tier_rank > 1 || disconnect_too_many_count < too_many_threshold.max(1) {
        return false;
    }
    let recent_unique = recent_unique_new_hashes.saturating_add(recent_unique_pooled_txs);
    let recent_duplicate = recent_duplicate_new_hashes.saturating_add(recent_duplicate_pooled_txs);
    let stale =
        last_success_ms == 0 || now_ms.saturating_sub(last_success_ms) > stale_window_ms.max(1);
    if !stale {
        return false;
    }
    if recent_unique == 0 {
        return true;
    }
    recent_duplicate > recent_unique.saturating_mul(4) && recent_duplicate >= 32
}

fn gateway_eth_plugin_rlpx_demote_congested_candidates(
    candidates: &mut [GatewayEthPluginRlpxTickCandidate<'_>],
    now_ms: u64,
    stale_window_ms: u64,
    too_many_threshold: u64,
) -> usize {
    let mut demoted = 0usize;
    for candidate in candidates.iter_mut() {
        if gateway_eth_plugin_rlpx_should_demote_congested_peer(
            candidate.tier_rank,
            candidate.disconnect_too_many_count,
            candidate.recent_unique_new_pooled_hashes_total,
            candidate.recent_unique_pooled_txs_total,
            candidate.recent_duplicate_new_pooled_hashes_total,
            candidate.recent_duplicate_pooled_txs_total,
            candidate.last_success_ms,
            now_ms,
            stale_window_ms,
            too_many_threshold,
        ) {
            candidate.tier_rank = candidate.tier_rank.saturating_add(1).min(2);
            candidate.is_priority = false;
            demoted = demoted.saturating_add(1);
        }
    }
    demoted
}

fn ensure_gateway_eth_plugin_mempool_ingest_rlpx_worker(
    chain_id: u64,
    peer: &PluginPeerEndpoint,
    timeout_ms: u64,
    read_window_ms: u64,
    max_hashes_per_request: usize,
    single_session: bool,
) {
    let worker_key = build_gateway_eth_plugin_rlpx_worker_key(chain_id, peer.endpoint.as_str());
    let should_spawn = if let Ok(mut guard) = gateway_eth_plugin_rlpx_worker_started().lock() {
        guard.insert(worker_key.clone())
    } else {
        false
    };
    if !should_spawn {
        return;
    }

    let endpoint = peer.endpoint.clone();
    let addr_hint = peer.addr_hint.clone();
    let node_hint = peer.node_hint.max(1);
    let worker_key_owned = worker_key.clone();
    thread::spawn(move || loop {
        let attempt_ms = now_unix_millis() as u64;
        update_gateway_eth_plugin_rlpx_worker_state(worker_key_owned.as_str(), |state| {
            state.running = true;
            state.last_attempt_ms = attempt_ms;
            state.dial_attempt_count = state.dial_attempt_count.saturating_add(1);
        });
        let effective_window_ms =
            read_window_ms.max(GATEWAY_ETH_PLUGIN_RLPX_WORKER_READ_WINDOW_MS_MIN);
        let outcome = gateway_eth_plugin_peer_session_rlpx_ingest(
            chain_id,
            endpoint.as_str(),
            node_hint,
            addr_hint.as_str(),
            timeout_ms,
            effective_window_ms,
            max_hashes_per_request,
        );
        let now_ms = now_unix_millis() as u64;
        let sleep_ms = match outcome {
            Ok(metrics) => {
                update_gateway_eth_plugin_rlpx_worker_state(worker_key_owned.as_str(), |state| {
                    state.running = true;
                    state.last_success_ms = now_ms;
                    state.consecutive_failures = 0;
                    state.consecutive_successes = state.consecutive_successes.saturating_add(1);
                    state.last_error_ms = 0;
                    state.cooldown_until_ms = 0;
                    state.last_error = None;
                    gateway_eth_plugin_rlpx_finalize_learning_state(state, &metrics);
                });
                if gateway_warn_enabled() {
                    let sample_score = gateway_eth_plugin_rlpx_learning_sample_score(&metrics);
                    eprintln!(
                        "gateway_warn: rlpx worker learn_update chain_id={} endpoint={} sample_score={} first_code={} new_hashes={} get_pooled={} pooled_received={} pooled_imported={} learning_score={}",
                        chain_id,
                        endpoint,
                        sample_score,
                        metrics
                            .first_post_ready_code
                            .map(|code| format!("0x{code:x}"))
                            .unwrap_or_else(|| "none".to_string()),
                        metrics.new_pooled_hashes,
                        metrics.get_pooled_sent,
                        metrics.pooled_txs_received,
                        metrics.pooled_txs_imported,
                        snapshot_gateway_eth_plugin_rlpx_worker_state(worker_key_owned.as_str())
                            .learning_score,
                    );
                }
                200
            }
            Err(err) => {
                let disconnect_bucket = gateway_eth_plugin_rlpx_disconnect_bucket(err.as_str());
                if gateway_warn_enabled() {
                    eprintln!(
                        "gateway_warn: rlpx ingest worker failed: chain_id={} endpoint={} addr={} bucket={} err={}",
                        chain_id, endpoint, addr_hint, disconnect_bucket, err
                    );
                }
                let is_too_many_peers = err.contains("too_many_peers");
                let is_connect_timeout =
                    gateway_eth_plugin_rlpx_error_is_timeout_like(err.as_str());
                let mut backoff_ms = GATEWAY_ETH_PLUGIN_RLPX_WORKER_FAIL_BACKOFF_BASE_MS;
                let mut tier_rank: u8 = 2;
                update_gateway_eth_plugin_rlpx_worker_state(worker_key_owned.as_str(), |state| {
                    state.running = true;
                    state.last_error_ms = now_ms;
                    state.consecutive_successes = 0;
                    state.consecutive_failures = state.consecutive_failures.saturating_add(1);
                    gateway_eth_plugin_rlpx_update_disconnect_counters(state, err.as_str());
                    tier_rank = gateway_eth_plugin_rlpx_worker_tier_rank(chain_id, state, now_ms);
                    let (tier_min_ms, tier_max_ms) =
                        gateway_eth_plugin_rlpx_worker_tier_backoff_bounds_ms(tier_rank);
                    backoff_ms =
                        gateway_eth_plugin_rlpx_worker_fail_backoff_ms(state.consecutive_failures)
                            .clamp(tier_min_ms, tier_max_ms);
                    if is_too_many_peers {
                        // Saturated public peers tend to stay full for a while.
                        backoff_ms = backoff_ms.max(60_000);
                    } else if is_connect_timeout {
                        backoff_ms = backoff_ms.max(
                            gateway_eth_plugin_rlpx_worker_timeout_backoff_floor_ms(tier_rank),
                        );
                    }
                    state.cooldown_until_ms = now_ms.saturating_add(backoff_ms);
                    state.last_error = Some(err.clone());
                });
                if gateway_warn_enabled() {
                    let tier_label = match tier_rank {
                        0 => "core",
                        1 => "active",
                        _ => "candidate",
                    };
                    eprintln!(
                        "gateway_warn: rlpx worker backoff: chain_id={} endpoint={} addr={} tier={} backoff_ms={} bucket={}",
                        chain_id,
                        endpoint,
                        addr_hint,
                        tier_label,
                        backoff_ms,
                        disconnect_bucket
                    );
                }
                set_gateway_eth_plugin_session_stage(
                    chain_id,
                    endpoint.as_str(),
                    GatewayEthPluginSessionStage::Disconnected,
                    now_ms,
                    Some(err),
                );
                observe_gateway_eth_plugin_session_stage(
                    chain_id,
                    GatewayEthPluginSessionStage::Disconnected,
                );
                backoff_ms
            }
        };
        gateway_eth_plugin_rlpx_profile_try_flush(chain_id, now_ms);
        if single_session {
            update_gateway_eth_plugin_rlpx_worker_state(worker_key_owned.as_str(), |state| {
                state.running = false;
            });
            if gateway_warn_enabled() {
                eprintln!(
                    "gateway_warn: rlpx worker single-session complete: chain_id={} endpoint={} addr={} sleep_ms={}",
                    chain_id, endpoint, addr_hint, sleep_ms
                );
            }
            break;
        }
        thread::sleep(Duration::from_millis(sleep_ms.max(200)));
    });
}

fn run_gateway_eth_plugin_mempool_ingest_rlpx_tick(
    chain_id: u64,
    plugin_peers: &[PluginPeerEndpoint],
) -> Option<String> {
    if !gateway_eth_plugin_mempool_ingest_rlpx_enabled(chain_id) {
        return None;
    }
    ensure_gateway_eth_plugin_rlpx_profile_loaded(chain_id);
    let enode_peers = plugin_peers
        .iter()
        .filter(|peer| {
            peer.endpoint
                .trim()
                .to_ascii_lowercase()
                .starts_with("enode://")
        })
        .collect::<Vec<_>>();
    if enode_peers.is_empty() {
        return Some("plugin_mempool_ingest_no_enode_peers".to_string());
    }

    let timeout_ms = gateway_eth_plugin_mempool_ingest_rlpx_timeout_ms(chain_id);
    let read_window_ms = gateway_eth_plugin_mempool_ingest_rlpx_read_window_ms(chain_id);
    let single_session = gateway_eth_plugin_mempool_ingest_rlpx_single_session(chain_id);
    let configured_max_peers = gateway_eth_plugin_mempool_ingest_rlpx_max_peers_per_tick(chain_id)
        .min(enode_peers.len())
        .max(1);
    let manual_priority_addr_hints =
        gateway_eth_plugin_mempool_ingest_rlpx_priority_addr_hints(chain_id);
    let priority_auto_pool_size = gateway_eth_plugin_mempool_ingest_rlpx_priority_auto_pool_size(
        chain_id,
        configured_max_peers,
    );
    let core_target = gateway_eth_plugin_mempool_ingest_rlpx_core_target(chain_id);
    let active_target = gateway_eth_plugin_mempool_ingest_rlpx_active_target(chain_id);
    let core_stable_floor = gateway_eth_plugin_mempool_ingest_rlpx_core_stable_floor(chain_id);
    let core_fallback_trigger_ticks =
        gateway_eth_plugin_mempool_ingest_rlpx_core_fallback_trigger_ticks(chain_id);
    let core_fallback_hold_ticks =
        gateway_eth_plugin_mempool_ingest_rlpx_core_fallback_hold_ticks(chain_id);
    let core_fallback_max_peers_bonus =
        gateway_eth_plugin_mempool_ingest_rlpx_core_fallback_max_peers_bonus(chain_id);
    let core_fallback_candidate_budget_min =
        gateway_eth_plugin_mempool_ingest_rlpx_core_fallback_candidate_budget_min(chain_id);
    let core_fallback_demote_too_many_min =
        gateway_eth_plugin_mempool_ingest_rlpx_core_fallback_demote_too_many_min(chain_id);
    let max_hashes = gateway_eth_plugin_mempool_ingest_rlpx_max_hashes_per_request(chain_id);
    let now_ms = now_unix_millis() as u64;
    let rotation = GATEWAY_ETH_PLUGIN_RLPX_SELECT_ROTATION
        .fetch_add(1, std::sync::atomic::Ordering::Relaxed) as usize;
    let mut candidates = enode_peers
        .iter()
        .enumerate()
        .map(|(index, peer)| {
            let worker_key =
                build_gateway_eth_plugin_rlpx_worker_key(chain_id, peer.endpoint.as_str());
            let state = snapshot_gateway_eth_plugin_rlpx_worker_state(worker_key.as_str());
            let tier_rank = gateway_eth_plugin_rlpx_worker_tier_rank(chain_id, &state, now_ms);
            let normalized_addr_hint =
                gateway_eth_plugin_normalize_priority_addr_hint(peer.addr_hint.as_str())
                    .unwrap_or_else(|| peer.addr_hint.trim().to_ascii_lowercase());
            let score = gateway_eth_plugin_rlpx_worker_score(chain_id, &state);
            GatewayEthPluginRlpxTickCandidate {
                peer,
                worker_key,
                normalized_addr_hint,
                tier_rank,
                is_priority: false,
                score,
                learning_score: state.learning_score,
                sessions_with_gossip: state.sessions_with_gossip,
                total_new_pooled_hashes: state.total_new_pooled_hashes,
                total_unique_new_pooled_hashes: state.total_unique_new_pooled_hashes,
                recent_unique_new_pooled_hashes_total: state.recent_unique_new_pooled_hashes_total,
                recent_duplicate_new_pooled_hashes_total: state
                    .recent_duplicate_new_pooled_hashes_total,
                total_pooled_txs_received: state.total_pooled_txs_received,
                total_unique_pooled_txs: state.total_unique_pooled_txs,
                recent_unique_pooled_txs_total: state.recent_unique_pooled_txs_total,
                recent_duplicate_pooled_txs_total: state.recent_duplicate_pooled_txs_total,
                total_pooled_txs_imported: state.total_pooled_txs_imported,
                first_seen_hash_count: state.first_seen_hash_count,
                first_seen_tx_count: state.first_seen_tx_count,
                disconnect_too_many_count: state.disconnect_too_many_count,
                last_success_ms: state.last_success_ms,
                priority_signal: gateway_eth_plugin_rlpx_worker_priority_signal(chain_id, &state),
                sort_key: gateway_eth_plugin_rlpx_worker_sort_key(
                    chain_id,
                    &state,
                    now_ms,
                    index,
                    enode_peers.len(),
                    rotation,
                ),
            }
        })
        .collect::<Vec<_>>();
    let auto_priority_addr_hints = if priority_auto_pool_size == 0 {
        std::collections::HashSet::<String>::new()
    } else {
        let auto_entries = candidates
            .iter()
            .map(|candidate| GatewayEthPluginRlpxPriorityAutoEntry {
                addr_hint: candidate.normalized_addr_hint.clone(),
                tier_rank: candidate.tier_rank,
                score: candidate.score,
                learning_score: candidate.learning_score,
                priority_signal: candidate.priority_signal,
                sessions_with_gossip: candidate.sessions_with_gossip,
                total_new_pooled_hashes: candidate.total_new_pooled_hashes,
                total_unique_new_pooled_hashes: candidate.total_unique_new_pooled_hashes,
                recent_unique_new_pooled_hashes_total: candidate
                    .recent_unique_new_pooled_hashes_total,
                recent_duplicate_new_pooled_hashes_total: candidate
                    .recent_duplicate_new_pooled_hashes_total,
                total_pooled_txs_received: candidate.total_pooled_txs_received,
                total_unique_pooled_txs: candidate.total_unique_pooled_txs,
                recent_unique_pooled_txs_total: candidate.recent_unique_pooled_txs_total,
                recent_duplicate_pooled_txs_total: candidate.recent_duplicate_pooled_txs_total,
                total_pooled_txs_imported: candidate.total_pooled_txs_imported,
                first_seen_hash_count: candidate.first_seen_hash_count,
                first_seen_tx_count: candidate.first_seen_tx_count,
            })
            .collect::<Vec<_>>();
        gateway_eth_plugin_rlpx_select_auto_priority_addr_hints(
            auto_entries.as_slice(),
            priority_auto_pool_size,
        )
    };
    let manual_priority_count = manual_priority_addr_hints.len();
    let mut priority_addr_hints = manual_priority_addr_hints;
    priority_addr_hints.extend(auto_priority_addr_hints.iter().cloned());
    for candidate in candidates.iter_mut() {
        candidate.is_priority = !priority_addr_hints.is_empty()
            && priority_addr_hints.contains(candidate.normalized_addr_hint.as_str());
    }
    candidates.sort_by(|a, b| a.sort_key.cmp(&b.sort_key));
    let (mut core_bucket, mut active_bucket, mut candidate_bucket, mut priority_bucket) =
        gateway_eth_plugin_rlpx_partition_tick_candidates(candidates.as_slice());
    let fallback_state = gateway_eth_plugin_rlpx_core_fallback_update_tick(
        chain_id,
        core_bucket.len(),
        core_stable_floor,
        core_fallback_trigger_ticks,
        core_fallback_hold_ticks,
    );
    let mut effective_max_peers = configured_max_peers;
    if fallback_state.active {
        let boosted = configured_max_peers
            .saturating_add(core_fallback_max_peers_bonus)
            .min(enode_peers.len())
            .min(GATEWAY_ETH_PLUGIN_RLPX_MAX_PEERS_PER_TICK_HARD_MAX);
        effective_max_peers = boosted.max(configured_max_peers);
        let demoted_count = gateway_eth_plugin_rlpx_demote_congested_candidates(
            candidates.as_mut_slice(),
            now_ms,
            gateway_eth_plugin_mempool_ingest_rlpx_active_recent_ready_window_ms(chain_id),
            core_fallback_demote_too_many_min,
        );
        if demoted_count > 0 {
            let partitioned =
                gateway_eth_plugin_rlpx_partition_tick_candidates(candidates.as_slice());
            core_bucket = partitioned.0;
            active_bucket = partitioned.1;
            candidate_bucket = partitioned.2;
            priority_bucket = partitioned.3;
        }
    }
    let priority_budget = gateway_eth_plugin_mempool_ingest_rlpx_priority_budget(
        chain_id,
        effective_max_peers,
        core_bucket.len(),
        core_target,
    );
    let (mut core_budget, mut active_budget, mut candidate_budget) =
        gateway_eth_plugin_rlpx_tier_budgets(
            effective_max_peers,
            core_bucket.len(),
            active_bucket.len(),
            core_target,
            active_target,
        );
    if fallback_state.active {
        let adjusted = gateway_eth_plugin_rlpx_apply_core_fallback_candidate_budget(
            core_budget,
            active_budget,
            candidate_budget,
            effective_max_peers,
            core_fallback_candidate_budget_min,
        );
        core_budget = adjusted.0;
        active_budget = adjusted.1;
        candidate_budget = adjusted.2;
    }
    let mut selected = Vec::<usize>::new();
    let mut selected_keys = std::collections::HashSet::<String>::new();
    let mut pick_from_bucket = |bucket: &[usize], budget: usize| {
        let mut taken = 0usize;
        for index in bucket.iter() {
            if selected.len() >= effective_max_peers || taken >= budget {
                break;
            }
            let candidate = &candidates[*index];
            if selected_keys.insert(candidate.worker_key.clone()) {
                selected.push(*index);
                taken = taken.saturating_add(1);
            }
        }
    };
    pick_from_bucket(priority_bucket.as_slice(), priority_budget);
    pick_from_bucket(core_bucket.as_slice(), core_budget);
    pick_from_bucket(active_bucket.as_slice(), active_budget);
    pick_from_bucket(candidate_bucket.as_slice(), candidate_budget);
    if selected.len() < effective_max_peers {
        for (index, candidate) in candidates.iter().enumerate() {
            if selected.len() >= effective_max_peers {
                break;
            }
            if selected_keys.insert(candidate.worker_key.clone()) {
                selected.push(index);
            }
        }
    }
    if gateway_warn_enabled() {
        let selected_preview = selected
            .iter()
            .map(|index| {
                let candidate = &candidates[*index];
                let state =
                    snapshot_gateway_eth_plugin_rlpx_worker_state(candidate.worker_key.as_str());
                format!(
                    "{}:{:?}:prio={} tier={} score={} psig={} learn={} ready={} gossip={} newHashes={}(u={},du={}) pooled={}/{}/(u={},du={}) swap={}(v2={},v3={},u={}) lat(gossip/swap)={}/{} disc={} tooMany={} succBps={} discBps={}",
                    candidate.peer.addr_hint,
                    candidate.sort_key,
                    candidate.is_priority,
                    gateway_eth_plugin_rlpx_worker_tier_label(chain_id, &state, now_ms),
                    gateway_eth_plugin_rlpx_worker_score(chain_id, &state),
                    gateway_eth_plugin_rlpx_worker_priority_signal(chain_id, &state),
                    state.learning_score,
                    state.sessions_completed,
                    state.sessions_with_gossip,
                    state.total_new_pooled_hashes,
                    state.total_unique_new_pooled_hashes,
                    state.total_duplicate_new_pooled_hashes,
                    state.total_pooled_txs_imported,
                    state.total_pooled_txs_received,
                    state.total_unique_pooled_txs,
                    state.total_duplicate_pooled_txs,
                    state.total_swap_hits,
                    state.total_swap_v2_hits,
                    state.total_swap_v3_hits,
                    state.total_unique_swap_hits,
                    gateway_eth_plugin_rlpx_avg_latency_ms(
                        state.total_first_gossip_latency_ms,
                        state.first_gossip_latency_samples
                    ),
                    gateway_eth_plugin_rlpx_avg_latency_ms(
                        state.total_first_swap_latency_ms,
                        state.first_swap_latency_samples
                    ),
                    state.disconnect_count,
                    state.disconnect_too_many_count,
                    gateway_eth_plugin_rlpx_worker_success_rate_bps(&state),
                    gateway_eth_plugin_rlpx_worker_disconnect_rate_bps(&state),
                )
            })
            .collect::<Vec<_>>()
            .join(", ");
        eprintln!(
            "gateway_warn: rlpx tick peer selection: chain_id={} max_peers={}->{} core_target={} active_target={} core_floor={} fallback(active={} streak={} hold={} activations={}) pool(priority={} manual={} auto={} core={} active={} candidate={}) budget(priority={} core={} active={} candidate={}) selected=[{}]",
            chain_id,
            configured_max_peers,
            effective_max_peers,
            core_target,
            active_target,
            core_stable_floor,
            fallback_state.active,
            fallback_state.low_core_streak,
            fallback_state.hold_ticks_remaining,
            fallback_state.activation_count,
            priority_bucket.len(),
            manual_priority_count,
            auto_priority_addr_hints.len(),
            core_bucket.len(),
            active_bucket.len(),
            candidate_bucket.len(),
            priority_budget,
            core_budget,
            active_budget,
            candidate_budget,
            selected_preview
        );
    }

    for candidate in selected.iter() {
        let candidate = &candidates[*candidate];
        ensure_gateway_eth_plugin_mempool_ingest_rlpx_worker(
            chain_id,
            candidate.peer,
            timeout_ms,
            read_window_ms,
            max_hashes,
            single_session,
        );
    }

    let mut has_success = false;
    let mut first_error: Option<String> = None;
    for candidate in selected.iter() {
        let candidate = &candidates[*candidate];
        let state = snapshot_gateway_eth_plugin_rlpx_worker_state(candidate.worker_key.as_str());
        if state.last_success_ms > 0 {
            has_success = true;
        } else if first_error.is_none() {
            first_error = state.last_error.clone().map(|err| {
                let cooldown_ms = state.cooldown_until_ms.saturating_sub(now_ms);
                if cooldown_ms > 0 {
                    format!(
                        "{}:{err}:cooldown_ms={cooldown_ms}",
                        candidate.peer.addr_hint
                    )
                } else {
                    format!("{}:{err}", candidate.peer.addr_hint)
                }
            });
        }
    }
    if first_error.is_some() {
        first_error
    } else if has_success {
        None
    } else {
        Some("plugin_mempool_ingest_rlpx_warming_up".to_string())
    }
}

fn run_gateway_eth_plugin_mempool_ingest_tick(chain_id: u64) -> (u64, u64, u64, Option<String>) {
    let stale_ttl_ms = gateway_eth_plugin_mempool_ingest_stale_ttl_ms(chain_id);
    let now_ms = now_unix_millis() as u64;
    let plugin_peers = gateway_eth_public_broadcast_native_peers_snapshot(chain_id)
        .map(|snapshot| snapshot.plugin_peers)
        .unwrap_or_else(|| Arc::new(Vec::new()));
    let endpoint_count = plugin_peers.len() as u64;
    update_gateway_eth_plugin_mempool_state(chain_id, |state| {
        state.running = true;
        state.endpoints = endpoint_count;
        state.tick_count = state.tick_count.saturating_add(1);
        state.last_tick_ms = now_ms;
        state.imported_last_tick = 0;
        state.evicted_last_tick = 0;
        state.evicted_confirmed_last_tick = 0;
        state.evicted_stale_last_tick = 0;
    });

    let (pending_before, queued_before) = collect_gateway_eth_txpool_runtime_txs(chain_id);
    let before_total = pending_before.len().saturating_add(queued_before.len());

    // Drive native runtime receive path for continuous plugin-route ingestion.
    ensure_gateway_eth_public_broadcast_native_runtime(chain_id);
    let rlpx_error =
        run_gateway_eth_plugin_mempool_ingest_rlpx_tick(chain_id, plugin_peers.as_ref());
    gateway_eth_plugin_rlpx_profile_try_flush(chain_id, now_ms);

    let (pending_after, queued_after) = collect_gateway_eth_txpool_runtime_txs(chain_id);
    let after_total = pending_after.len().saturating_add(queued_after.len());
    let imported = after_total.saturating_sub(before_total) as u64;
    let evicted_confirmed = 0u64;
    let mut evicted_stale = 0u64;
    if stale_ttl_ms > 0 {
        let observed_before = now_ms.saturating_sub(stale_ttl_ms);
        evicted_stale = evict_stale_ingress_frames_for_host(chain_id, observed_before) as u64;
    }
    let error = if endpoint_count == 0 {
        Some("plugin_mempool_ingest_no_plugin_peers".to_string())
    } else {
        rlpx_error
    };
    (imported, evicted_confirmed, evicted_stale, error)
}

pub(super) fn ensure_gateway_eth_plugin_mempool_ingest_runtime(chain_id: u64) {
    if !gateway_eth_plugin_mempool_ingest_enabled(chain_id) {
        return;
    }
    let should_spawn = if let Ok(mut guard) = gateway_eth_plugin_mempool_worker_started().lock() {
        guard.insert(chain_id)
    } else {
        false
    };
    if !should_spawn {
        return;
    }
    thread::spawn(move || loop {
        let (imported, evicted_confirmed, evicted_stale, last_error) =
            run_gateway_eth_plugin_mempool_ingest_tick(chain_id);
        let now_ms = now_unix_millis() as u64;
        let evicted_total = evicted_confirmed.saturating_add(evicted_stale);
        update_gateway_eth_plugin_mempool_state(chain_id, |state| {
            state.running = true;
            state.last_tick_ms = now_ms;
            state.imported_last_tick = imported;
            state.evicted_last_tick = evicted_total;
            state.evicted_confirmed_last_tick = evicted_confirmed;
            state.evicted_stale_last_tick = evicted_stale;
            state.imported_total = state.imported_total.saturating_add(imported);
            state.evicted_total = state.evicted_total.saturating_add(evicted_total);
            state.evicted_confirmed_total = state
                .evicted_confirmed_total
                .saturating_add(evicted_confirmed);
            state.evicted_stale_total = state.evicted_stale_total.saturating_add(evicted_stale);
            state.last_error = last_error.clone();
            if imported > 0 || evicted_total > 0 {
                state.last_success_ms = now_ms;
            }
        });
        let sleep_ms = gateway_eth_plugin_mempool_ingest_poll_ms(chain_id);
        thread::sleep(Duration::from_millis(sleep_ms));
    });
}

pub(super) fn poll_gateway_eth_public_broadcast_native_runtime(chain_id: u64, max_frames: usize) {
    ensure_gateway_eth_plugin_mempool_ingest_runtime(chain_id);
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
    let Some(snapshot) = gateway_eth_public_broadcast_native_peers_snapshot(chain_id) else {
        return;
    };
    let mut peers = snapshot.supvm_peers;
    let mut peer_nodes = snapshot.supvm_peer_nodes;
    if peers.is_empty() && !snapshot.plugin_peers.is_empty() {
        let (bridge_peers, bridge_nodes) =
            gateway_eth_plugin_runtime_bridge_peers(snapshot.plugin_peers.as_ref());
        if !bridge_peers.is_empty() {
            peers = bridge_peers;
            peer_nodes = bridge_nodes;
        }
    }
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
        let mut failed = 0u64;
        let mut errors = Vec::new();
        gateway_eth_native_register_peers_parallel(
            &broadcaster,
            peers.as_ref(),
            &mut failed,
            &mut errors,
        );
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
                let runtime_status = get_network_runtime_sync_status(chain_id);
                let current_head = runtime_status.map(|s| s.current_block).unwrap_or(0);
                let total_difficulty = current_head as u128;
                gateway_eth_native_send_discovery_bundle_parallel(
                    &broadcaster,
                    peer_nodes.as_ref(),
                    local_node,
                    chain_id,
                    total_difficulty,
                    current_head,
                );
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
    let plugin_mempool_ingest_enabled = gateway_eth_plugin_mempool_ingest_enabled(chain_id);
    if plugin_mempool_ingest_enabled {
        ensure_gateway_eth_plugin_mempool_ingest_runtime(chain_id);
    }
    let plugin_mempool_state = snapshot_gateway_eth_plugin_mempool_state(chain_id);
    let (reported_plugin_peer_count, reported_stage_stats) =
        gateway_eth_plugin_session_stage_stats_from_cache(chain_id);
    let (
        supvm_peer_count,
        plugin_peer_count,
        route_policy,
        peer_source,
        plugin_ports_json,
        plugin_probe_checked_ms,
        plugin_reachable_count,
        plugin_unreachable_count,
        plugin_session_stage_stats,
    ) = if let Some(snapshot) = gateway_eth_public_broadcast_native_peers_snapshot(chain_id) {
        let probe =
            probe_gateway_eth_plugin_peers_with_cache(chain_id, snapshot.plugin_peers.as_ref());
        let configured_stage_stats =
            gateway_eth_plugin_session_stage_stats(chain_id, snapshot.plugin_peers.as_ref());
        (
            snapshot.supvm_peers.len() as u64,
            (snapshot.plugin_peers.len() as u64).max(reported_plugin_peer_count),
            gateway_eth_native_route_policy_label(snapshot.route_policy).to_string(),
            snapshot.peer_source.clone(),
            serde_json::Value::Array(
                snapshot
                    .plugin_ports
                    .iter()
                    .map(|port| serde_json::Value::String(format!("0x{:x}", port)))
                    .collect(),
            ),
            probe.checked_ms,
            probe.reachable_count as u64,
            probe.unreachable_count as u64,
            if reported_plugin_peer_count > 0 {
                reported_stage_stats
            } else {
                configured_stage_stats
            },
        )
    } else {
        (
            0,
            reported_plugin_peer_count,
            gateway_eth_native_route_policy_label(AdaptivePeerRoutePolicy::Auto).to_string(),
            "none".to_string(),
            serde_json::Value::Array(Vec::new()),
            0,
            0,
            0,
            reported_stage_stats,
        )
    };
    let transport = gateway_eth_public_broadcast_native_transport(chain_id).as_mode();
    let mode = if exec_path.is_some() {
        "external_executor"
    } else if supvm_peer_count > 0 && plugin_peer_count > 0 {
        "adaptive_supvm_active_plugin_pending"
    } else if supvm_peer_count > 0 {
        "native_transport"
    } else if plugin_peer_count > 0 {
        "plugin_route_pending"
    } else {
        "none"
    };
    let available = exec_path.is_some() || supvm_peer_count > 0;
    let ready = available || !required;
    let mut plugin_session_stage_counts = serde_json::Map::new();
    for stage in [
        GatewayEthPluginSessionStage::Disconnected,
        GatewayEthPluginSessionStage::TcpConnected,
        GatewayEthPluginSessionStage::AuthSent,
        GatewayEthPluginSessionStage::AckSeen,
        GatewayEthPluginSessionStage::Ready,
    ] {
        plugin_session_stage_counts.insert(
            stage.as_str().to_string(),
            serde_json::Value::String(format!(
                "0x{:x}",
                gateway_eth_plugin_session_stage_count(&plugin_session_stage_stats, stage)
            )),
        );
    }
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
        "native_route_policy": route_policy,
        "native_peer_source": peer_source,
        "native_plugin_ports": plugin_ports_json,
        "native_supvm_peer_count": format!("0x{:x}", supvm_peer_count),
        "native_plugin_peer_count": format!("0x{:x}", plugin_peer_count),
        "native_plugin_route_available": plugin_session_stage_stats.ready > 0,
        "native_plugin_route_connectivity": plugin_reachable_count > 0,
        "native_plugin_probe_checked_ms": format!("0x{:x}", plugin_probe_checked_ms),
        "native_plugin_reachable_count": format!("0x{:x}", plugin_reachable_count),
        "native_plugin_unreachable_count": format!("0x{:x}", plugin_unreachable_count),
        "native_plugin_session_disconnected": format!(
            "0x{:x}",
            plugin_session_stage_stats.disconnected
        ),
        "native_plugin_session_tcp_connected": format!(
            "0x{:x}",
            plugin_session_stage_stats.tcp_connected
        ),
        "native_plugin_session_auth_sent": format!(
            "0x{:x}",
            plugin_session_stage_stats.auth_sent
        ),
        "native_plugin_session_ack_seen": format!(
            "0x{:x}",
            plugin_session_stage_stats.ack_seen
        ),
        "native_plugin_session_ready": format!("0x{:x}", plugin_session_stage_stats.ready),
        "native_plugin_session_stage_counts": plugin_session_stage_counts,
        "native_plugin_mempool_ingest_enabled": plugin_mempool_ingest_enabled,
        "native_plugin_mempool_ingest_running": plugin_mempool_state.running,
        "native_plugin_mempool_ingest_endpoints": format!(
            "0x{:x}",
            plugin_mempool_state.endpoints
        ),
        "native_plugin_mempool_ingest_tick_count": format!(
            "0x{:x}",
            plugin_mempool_state.tick_count
        ),
        "native_plugin_mempool_ingest_imported_total": format!(
            "0x{:x}",
            plugin_mempool_state.imported_total
        ),
        "native_plugin_mempool_ingest_imported_last_tick": format!(
            "0x{:x}",
            plugin_mempool_state.imported_last_tick
        ),
        "native_plugin_mempool_ingest_evicted_total": format!(
            "0x{:x}",
            plugin_mempool_state.evicted_total
        ),
        "native_plugin_mempool_ingest_evicted_last_tick": format!(
            "0x{:x}",
            plugin_mempool_state.evicted_last_tick
        ),
        "native_plugin_mempool_ingest_evicted_confirmed_total": format!(
            "0x{:x}",
            plugin_mempool_state.evicted_confirmed_total
        ),
        "native_plugin_mempool_ingest_evicted_confirmed_last_tick": format!(
            "0x{:x}",
            plugin_mempool_state.evicted_confirmed_last_tick
        ),
        "native_plugin_mempool_ingest_evicted_stale_total": format!(
            "0x{:x}",
            plugin_mempool_state.evicted_stale_total
        ),
        "native_plugin_mempool_ingest_evicted_stale_last_tick": format!(
            "0x{:x}",
            plugin_mempool_state.evicted_stale_last_tick
        ),
        "native_plugin_mempool_ingest_stale_ttl_ms": format!(
            "0x{:x}",
            gateway_eth_plugin_mempool_ingest_stale_ttl_ms(chain_id)
        ),
        "native_plugin_mempool_ingest_last_tick_ms": format!(
            "0x{:x}",
            plugin_mempool_state.last_tick_ms
        ),
        "native_plugin_mempool_ingest_last_success_ms": format!(
            "0x{:x}",
            plugin_mempool_state.last_success_ms
        ),
        "native_plugin_mempool_ingest_last_error": plugin_mempool_state
            .last_error
            .unwrap_or_default(),
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

#[allow(dead_code)]
fn gateway_eth_public_broadcast_upstream_rpc_url(chain_id: u64) -> Option<String> {
    gateway_eth_public_broadcast_chain_string_env(
        chain_id,
        "NOVOVM_GATEWAY_ETH_PUBLIC_BROADCAST_UPSTREAM_RPC",
    )
    .or_else(|| {
        gateway_eth_public_broadcast_chain_string_env(chain_id, "NOVOVM_GATEWAY_ETH_UPSTREAM_RPC")
    })
}

#[allow(dead_code)]
fn gateway_eth_public_broadcast_upstream_rpc_timeout_ms(chain_id: u64) -> u64 {
    gateway_eth_public_broadcast_chain_u64_env(
        chain_id,
        "NOVOVM_GATEWAY_ETH_PUBLIC_BROADCAST_UPSTREAM_RPC_TIMEOUT_MS",
        GATEWAY_ETH_PUBLIC_BROADCAST_EXEC_TIMEOUT_MS_DEFAULT,
    )
}

fn gateway_eth_public_broadcast_native_route_policy(chain_id: u64) -> AdaptivePeerRoutePolicy {
    let raw = gateway_eth_public_broadcast_chain_string_env(
        chain_id,
        "NOVOVM_GATEWAY_ETH_PUBLIC_BROADCAST_ROUTE_POLICY",
    )
    .unwrap_or_else(|| "auto".to_string());
    match raw.trim().to_ascii_lowercase().as_str() {
        "supvm" | "supvm_only" | "primary" | "primary_only" => AdaptivePeerRoutePolicy::PrimaryOnly,
        "plugin" | "plugin_only" | "evm_plugin" | "evm_plugin_only" => {
            AdaptivePeerRoutePolicy::PluginOnly
        }
        _ => AdaptivePeerRoutePolicy::Auto,
    }
}

fn gateway_eth_public_broadcast_plugin_ports(chain_id: u64) -> Vec<u16> {
    let raw = gateway_eth_public_broadcast_chain_string_env(
        chain_id,
        "NOVOVM_GATEWAY_ETH_PUBLIC_BROADCAST_PLUGIN_PORTS",
    )
    .unwrap_or_else(|| "30303,30304".to_string());
    parse_port_list(raw.as_str())
}

fn gateway_eth_public_broadcast_enable_builtin_bootnodes(chain_id: u64) -> bool {
    let default = if cfg!(test) { "false" } else { "true" };
    let raw = gateway_eth_public_broadcast_chain_string_env(
        chain_id,
        "NOVOVM_GATEWAY_ETH_PUBLIC_BROADCAST_ENABLE_BUILTIN_BOOTNODES",
    )
    .unwrap_or_else(|| default.to_string());
    !matches!(
        raw.trim().to_ascii_lowercase().as_str(),
        "0" | "false" | "off" | "no"
    )
}

fn gateway_eth_public_broadcast_plugin_min_candidates(chain_id: u64) -> usize {
    gateway_eth_public_broadcast_chain_u64_env(
        chain_id,
        "NOVOVM_GATEWAY_ETH_PUBLIC_BROADCAST_PLUGIN_MIN_CANDIDATES",
        GATEWAY_ETH_PLUGIN_MIN_CANDIDATES_DEFAULT as u64,
    )
    .clamp(1, 64) as usize
}

fn gateway_eth_builtin_bootnodes_for_chain(chain_id: u64) -> &'static [&'static str] {
    match chain_id {
        1 => &GATEWAY_ETH_MAINNET_BOOTNODES,
        11_155_111 => &GATEWAY_ETH_SEPOLIA_BOOTNODES,
        17_000 => &GATEWAY_ETH_HOLESKY_BOOTNODES,
        _ => &[],
    }
}

fn gateway_eth_public_broadcast_bootnodes(chain_id: u64, include_builtin: bool) -> Vec<String> {
    let mut out = Vec::<String>::new();
    if include_builtin {
        out.extend(
            gateway_eth_builtin_bootnodes_for_chain(chain_id)
                .iter()
                .map(|v| (*v).to_string()),
        );
    }
    if let Some(raw) = gateway_eth_public_broadcast_chain_string_env(
        chain_id,
        "NOVOVM_GATEWAY_ETH_PUBLIC_BROADCAST_BOOTNODES",
    ) {
        out.extend(
            raw.split([',', ';', '\n', '\r', '\t', ' '])
                .map(str::trim)
                .filter(|entry| !entry.is_empty())
                .map(ToOwned::to_owned),
        );
    }
    let mut dedup = std::collections::HashSet::<String>::new();
    out.retain(|entry| dedup.insert(entry.to_ascii_lowercase()));
    out
}

fn split_addr_hint_host_port(addr_hint: &str) -> Option<(String, u16)> {
    if let Ok(sock) = addr_hint.parse::<std::net::SocketAddr>() {
        return Some((sock.ip().to_string(), sock.port()));
    }
    let (host_raw, port_raw) = addr_hint.rsplit_once(':')?;
    let port = port_raw.parse::<u16>().ok()?;
    let host = host_raw
        .trim()
        .trim_start_matches('[')
        .trim_end_matches(']');
    if host.is_empty() {
        return None;
    }
    Some((host.to_string(), port))
}

fn format_addr_hint(host: &str, port: u16) -> String {
    if host.contains(':') {
        format!("[{}]:{}", host, port)
    } else {
        format!("{}:{}", host, port)
    }
}

fn rewrite_endpoint_with_addr_hint(
    endpoint: &str,
    fallback_node_hint: u64,
    addr_hint: &str,
) -> String {
    let trimmed = endpoint.trim();
    if trimmed.to_ascii_lowercase().starts_with("enode://") {
        let (base, query) = trimmed
            .split_once('?')
            .map_or((trimmed, None), |(left, right)| (left, Some(right)));
        if let Some((prefix, _)) = base.rsplit_once('@') {
            let mut rewritten = format!("{prefix}@{addr_hint}");
            if let Some(query) = query {
                rewritten.push('?');
                rewritten.push_str(query);
            }
            return rewritten;
        }
    }
    format!("{fallback_node_hint}@{addr_hint}")
}

fn parse_gateway_eth_plugin_session_endpoint(endpoint: &str) -> Option<(u64, String)> {
    if let Some((node_hint, addr_hint)) = parse_enode_endpoint(endpoint) {
        return Some((node_hint.max(1), addr_hint));
    }
    let trimmed = endpoint.trim();
    if trimmed.is_empty() {
        return None;
    }
    let (_, addr_raw) = trimmed.rsplit_once('@').unwrap_or(("", trimmed));
    let addr_hint = addr_raw.trim();
    if addr_hint.is_empty() || split_addr_hint_host_port(addr_hint).is_none() {
        return None;
    }
    let node_hint = trimmed
        .split_once('@')
        .map(|(head, _)| head)
        .and_then(parse_u64_with_optional_hex_prefix)
        .unwrap_or_else(|| gateway_eth_plugin_endpoint_node_hint(trimmed))
        .max(1);
    Some((node_hint, addr_hint.to_string()))
}

fn extend_plugin_peers_with_port_candidates(
    peers: Vec<PluginPeerEndpoint>,
    plugin_ports: &[u16],
) -> Vec<PluginPeerEndpoint> {
    if peers.is_empty() || plugin_ports.is_empty() {
        return peers;
    }
    let mut out = Vec::<PluginPeerEndpoint>::new();
    let mut dedup = std::collections::HashSet::<String>::new();
    for peer in peers {
        let base_key = format!(
            "{}|{}",
            peer.endpoint.to_ascii_lowercase(),
            peer.addr_hint.to_ascii_lowercase()
        );
        if dedup.insert(base_key) {
            out.push(peer.clone());
        }
        let Some((host, current_port)) = split_addr_hint_host_port(peer.addr_hint.as_str()) else {
            continue;
        };
        for port in plugin_ports {
            if *port == current_port {
                continue;
            }
            let addr_hint = format_addr_hint(host.as_str(), *port);
            let endpoint = rewrite_endpoint_with_addr_hint(
                peer.endpoint.as_str(),
                peer.node_hint,
                addr_hint.as_str(),
            );
            let key = format!(
                "{}|{}",
                endpoint.to_ascii_lowercase(),
                addr_hint.to_ascii_lowercase()
            );
            if dedup.insert(key) {
                out.push(PluginPeerEndpoint {
                    endpoint,
                    node_hint: peer.node_hint,
                    addr_hint,
                });
            }
        }
    }
    out
}

fn gateway_eth_public_broadcast_plugin_probe_timeout_ms(chain_id: u64) -> u64 {
    gateway_eth_public_broadcast_chain_u64_env(
        chain_id,
        "NOVOVM_GATEWAY_ETH_PUBLIC_BROADCAST_PLUGIN_PROBE_TIMEOUT_MS",
        GATEWAY_ETH_PLUGIN_PROBE_TIMEOUT_MS_DEFAULT,
    )
    .clamp(
        GATEWAY_ETH_PLUGIN_PROBE_TIMEOUT_MS_MIN,
        GATEWAY_ETH_PLUGIN_PROBE_TIMEOUT_MS_MAX,
    )
}

fn gateway_eth_public_broadcast_plugin_probe_cache_ttl_ms(chain_id: u64) -> u64 {
    gateway_eth_public_broadcast_chain_u64_env(
        chain_id,
        "NOVOVM_GATEWAY_ETH_PUBLIC_BROADCAST_PLUGIN_PROBE_CACHE_TTL_MS",
        GATEWAY_ETH_PLUGIN_PROBE_CACHE_TTL_MS_DEFAULT,
    )
    .clamp(
        GATEWAY_ETH_PLUGIN_PROBE_CACHE_TTL_MS_MIN,
        GATEWAY_ETH_PLUGIN_PROBE_CACHE_TTL_MS_MAX,
    )
}

fn gateway_eth_public_broadcast_plugin_session_probe_mode(
    chain_id: u64,
) -> GatewayEthPluginSessionProbeMode {
    let raw = gateway_eth_public_broadcast_chain_string_env(
        chain_id,
        "NOVOVM_GATEWAY_ETH_PUBLIC_BROADCAST_PLUGIN_SESSION_PROBE_MODE",
    )
    .unwrap_or_else(|| "enode".to_string());
    match raw.trim().to_ascii_lowercase().as_str() {
        "off" | "disabled" | "none" => GatewayEthPluginSessionProbeMode::Disabled,
        "all" | "full" | "any" => GatewayEthPluginSessionProbeMode::All,
        _ => GatewayEthPluginSessionProbeMode::EnodeOnly,
    }
}

fn gateway_eth_native_route_policy_label(policy: AdaptivePeerRoutePolicy) -> &'static str {
    match policy {
        AdaptivePeerRoutePolicy::Auto => "auto",
        AdaptivePeerRoutePolicy::PrimaryOnly => "supvm_only",
        AdaptivePeerRoutePolicy::PluginOnly => "plugin_only",
    }
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

fn parse_gateway_eth_public_broadcast_native_peers(
    raw: &str,
    route_policy: AdaptivePeerRoutePolicy,
    plugin_ports: &[u16],
) -> (Vec<(NodeId, String)>, Vec<PluginPeerEndpoint>) {
    let routes = classify_adaptive_peer_routes(raw, route_policy, plugin_ports);
    let supvm_peers = routes
        .primary_peers
        .into_iter()
        .map(|(node_id, addr)| (NodeId(node_id), addr))
        .collect();
    (
        supvm_peers,
        extend_plugin_peers_with_port_candidates(routes.plugin_peers, plugin_ports),
    )
}

fn gateway_eth_plugin_peers_from_session_cache(
    chain_id: u64,
    plugin_ports: &[u16],
) -> Vec<PluginPeerEndpoint> {
    let now_ms = now_unix_millis() as u64;
    let mut peers = Vec::<PluginPeerEndpoint>::new();
    for state in gateway_eth_plugin_session_entries_for_chain(chain_id) {
        let (stage, _) =
            gateway_eth_plugin_effective_stage(chain_id, state.endpoint.as_str(), &state, now_ms);
        if matches!(stage, GatewayEthPluginSessionStage::Disconnected) {
            continue;
        }
        let Some((node_hint, addr_hint)) =
            parse_gateway_eth_plugin_session_endpoint(state.endpoint.as_str())
        else {
            continue;
        };
        peers.push(PluginPeerEndpoint {
            endpoint: state.endpoint,
            node_hint,
            addr_hint,
        });
    }
    extend_plugin_peers_with_port_candidates(peers, plugin_ports)
}

fn gateway_eth_public_broadcast_native_peers_snapshot(
    chain_id: u64,
) -> Option<GatewayEthNativePeersSnapshot> {
    let route_policy = gateway_eth_public_broadcast_native_route_policy(chain_id);
    let plugin_ports = gateway_eth_public_broadcast_plugin_ports(chain_id);
    let plugin_ports_arc = Arc::new(plugin_ports.clone());
    let mut supvm_peers_vec = Vec::<(NodeId, String)>::new();
    let mut plugin_peers_vec = Vec::<PluginPeerEndpoint>::new();
    let mut peer_source_labels = Vec::<String>::new();

    if let Some(raw) = gateway_eth_public_broadcast_chain_string_env(
        chain_id,
        "NOVOVM_GATEWAY_ETH_PUBLIC_BROADCAST_NATIVE_PEERS",
    ) {
        let (configured_supvm, configured_plugin) = parse_gateway_eth_public_broadcast_native_peers(
            raw.as_str(),
            route_policy,
            plugin_ports.as_slice(),
        );
        if !configured_supvm.is_empty() || !configured_plugin.is_empty() {
            supvm_peers_vec.extend(configured_supvm);
            plugin_peers_vec.extend(configured_plugin);
            peer_source_labels.push("configured".to_string());
        }
    }

    let min_plugin_candidates = gateway_eth_public_broadcast_plugin_min_candidates(chain_id);
    let should_use_builtin_bootnodes =
        gateway_eth_public_broadcast_enable_builtin_bootnodes(chain_id)
            && !matches!(route_policy, AdaptivePeerRoutePolicy::PrimaryOnly)
            && plugin_peers_vec.len() < min_plugin_candidates;
    let bootnodes = gateway_eth_public_broadcast_bootnodes(chain_id, should_use_builtin_bootnodes);
    if !bootnodes.is_empty() {
        let bootnodes_raw = bootnodes.join(",");
        let (boot_supvm, boot_plugin) = parse_gateway_eth_public_broadcast_native_peers(
            bootnodes_raw.as_str(),
            route_policy,
            plugin_ports.as_slice(),
        );
        if !boot_supvm.is_empty() || !boot_plugin.is_empty() {
            supvm_peers_vec.extend(boot_supvm);
            plugin_peers_vec.extend(boot_plugin);
            if should_use_builtin_bootnodes {
                peer_source_labels.push("builtin_bootnodes".to_string());
            } else {
                peer_source_labels.push("configured_bootnodes".to_string());
            }
        }
    }

    if !matches!(route_policy, AdaptivePeerRoutePolicy::PrimaryOnly) {
        let cached_plugin_peers =
            gateway_eth_plugin_peers_from_session_cache(chain_id, plugin_ports.as_slice());
        if !cached_plugin_peers.is_empty() {
            plugin_peers_vec.extend(cached_plugin_peers);
            peer_source_labels.push("session_cache".to_string());
        }
    }
    if supvm_peers_vec.is_empty() && plugin_peers_vec.is_empty() {
        return None;
    }

    let mut supvm_dedup = std::collections::HashSet::<String>::new();
    supvm_peers_vec.retain(|(node, addr)| {
        supvm_dedup.insert(format!("{}@{}", node.0, addr.to_ascii_lowercase()))
    });
    let mut plugin_dedup = std::collections::HashSet::<String>::new();
    plugin_peers_vec.retain(|peer| {
        plugin_dedup.insert(format!(
            "{}|{}",
            peer.endpoint.to_ascii_lowercase(),
            peer.addr_hint.to_ascii_lowercase()
        ))
    });

    let peer_source = if peer_source_labels.is_empty() {
        "none".to_string()
    } else {
        peer_source_labels.join("+")
    };
    let plugin_ports_key = plugin_ports
        .iter()
        .map(u16::to_string)
        .collect::<Vec<_>>()
        .join(",");
    let mut cache_peer_material = supvm_peers_vec
        .iter()
        .map(|(node, addr)| format!("{}@{}", node.0, addr))
        .chain(plugin_peers_vec.iter().map(|peer| peer.endpoint.clone()))
        .collect::<Vec<_>>();
    cache_peer_material.sort_unstable();
    let cache_key = format!(
        "route={}|plugin_ports={}|plugin_min={}|source={}|{}",
        gateway_eth_native_route_policy_label(route_policy),
        plugin_ports_key,
        min_plugin_candidates,
        peer_source,
        cache_peer_material.join(",")
    );
    let cache = gateway_eth_native_peers_cache();
    if let Some(existing) = cache.get(&chain_id) {
        if existing.cache_key == cache_key {
            return Some(GatewayEthNativePeersSnapshot {
                supvm_peers: existing.supvm_peers.clone(),
                supvm_peer_nodes: existing.supvm_peer_nodes.clone(),
                plugin_peers: existing.plugin_peers.clone(),
                plugin_ports: existing.plugin_ports.clone(),
                route_policy: existing.route_policy,
                peer_source: existing.peer_source.clone(),
            });
        }
    }

    let supvm_peer_nodes_vec: Vec<NodeId> = supvm_peers_vec.iter().map(|(peer, _)| *peer).collect();
    let supvm_peers = Arc::new(supvm_peers_vec);
    let supvm_peer_nodes = Arc::new(supvm_peer_nodes_vec);
    let plugin_peers = Arc::new(plugin_peers_vec);
    cache.insert(
        chain_id,
        GatewayEthNativePeersCache {
            cache_key,
            supvm_peers: supvm_peers.clone(),
            supvm_peer_nodes: supvm_peer_nodes.clone(),
            plugin_peers: plugin_peers.clone(),
            plugin_ports: plugin_ports_arc.clone(),
            route_policy,
            peer_source: peer_source.clone(),
        },
    );
    Some(GatewayEthNativePeersSnapshot {
        supvm_peers,
        supvm_peer_nodes,
        plugin_peers,
        plugin_ports: plugin_ports_arc,
        route_policy,
        peer_source,
    })
}

fn build_gateway_eth_plugin_probe_cache_key(chain_id: u64, peers: &[PluginPeerEndpoint]) -> String {
    let mut endpoints = peers
        .iter()
        .map(|peer| peer.endpoint.as_str())
        .collect::<Vec<_>>();
    endpoints.sort_unstable();
    format!("chain:{}|{}", chain_id, endpoints.join(","))
}

fn build_gateway_eth_plugin_session_key(chain_id: u64, endpoint: &str) -> String {
    format!("chain:{}|endpoint:{}", chain_id, endpoint)
}

fn set_gateway_eth_plugin_session_stage(
    chain_id: u64,
    endpoint: &str,
    stage: GatewayEthPluginSessionStage,
    updated_ms: u64,
    last_error: Option<String>,
) {
    gateway_eth_plugin_session_cache().insert(
        build_gateway_eth_plugin_session_key(chain_id, endpoint),
        GatewayEthPluginSessionState {
            chain_id,
            endpoint: endpoint.to_string(),
            stage,
            updated_ms,
            last_error,
        },
    );
}

fn bump_gateway_eth_plugin_session_stage(
    chain_id: u64,
    endpoint: &str,
    target_stage: GatewayEthPluginSessionStage,
    updated_ms: u64,
) {
    let key = build_gateway_eth_plugin_session_key(chain_id, endpoint);
    let cache = gateway_eth_plugin_session_cache();
    if let Some(mut existing) = cache.get_mut(key.as_str()) {
        if existing.stage.rank() < target_stage.rank() {
            existing.stage = target_stage;
        }
        existing.updated_ms = updated_ms;
        existing.last_error = None;
        return;
    }
    cache.insert(
        key,
        GatewayEthPluginSessionState {
            chain_id,
            endpoint: endpoint.to_string(),
            stage: target_stage,
            updated_ms,
            last_error: None,
        },
    );
}

fn gateway_eth_plugin_session_stage_stats(
    chain_id: u64,
    peers: &[PluginPeerEndpoint],
) -> GatewayEthPluginSessionStageStats {
    let cache = gateway_eth_plugin_session_cache();
    let mut stats = GatewayEthPluginSessionStageStats::default();
    let now_ms = now_unix_millis() as u64;
    let stale_after_ms = gateway_eth_public_broadcast_plugin_probe_cache_ttl_ms(chain_id)
        .saturating_mul(3)
        .max(1_000);
    for peer in peers {
        let key = build_gateway_eth_plugin_session_key(chain_id, peer.endpoint.as_str());
        let stage = cache
            .get(key.as_str())
            .map(|state| {
                let stale = now_ms.saturating_sub(state.updated_ms) > stale_after_ms;
                let mismatched = state.chain_id != chain_id || state.endpoint != peer.endpoint;
                if stale || mismatched {
                    return GatewayEthPluginSessionStage::Disconnected;
                }
                if let Some(last_error) = state.last_error.as_deref() {
                    if state.stage == GatewayEthPluginSessionStage::Disconnected
                        || gateway_eth_plugin_session_error_is_connectivity_fatal(last_error)
                    {
                        return GatewayEthPluginSessionStage::Disconnected;
                    }
                }
                state.stage
            })
            .unwrap_or(GatewayEthPluginSessionStage::Disconnected);
        match stage {
            GatewayEthPluginSessionStage::Disconnected => stats.disconnected += 1,
            GatewayEthPluginSessionStage::TcpConnected => stats.tcp_connected += 1,
            GatewayEthPluginSessionStage::AuthSent => stats.auth_sent += 1,
            GatewayEthPluginSessionStage::AckSeen => stats.ack_seen += 1,
            GatewayEthPluginSessionStage::Ready => stats.ready += 1,
        }
    }
    stats
}

fn gateway_eth_plugin_session_stale_after_ms(chain_id: u64) -> u64 {
    gateway_eth_public_broadcast_plugin_probe_cache_ttl_ms(chain_id)
        .saturating_mul(3)
        .max(1_000)
}

fn gateway_eth_plugin_session_entries_for_chain(
    chain_id: u64,
) -> Vec<GatewayEthPluginSessionState> {
    gateway_eth_plugin_session_cache()
        .iter()
        .filter_map(|entry| {
            let state = entry.value();
            (state.chain_id == chain_id).then(|| state.clone())
        })
        .collect()
}

fn gateway_eth_plugin_effective_stage(
    chain_id: u64,
    endpoint: &str,
    state: &GatewayEthPluginSessionState,
    now_ms: u64,
) -> (GatewayEthPluginSessionStage, Option<String>) {
    let stale = now_ms.saturating_sub(state.updated_ms)
        > gateway_eth_plugin_session_stale_after_ms(chain_id);
    let mismatched = state.chain_id != chain_id || state.endpoint != endpoint;
    if stale || mismatched {
        return (
            GatewayEthPluginSessionStage::Disconnected,
            Some("stale_or_mismatched".to_string()),
        );
    }
    if let Some(last_error) = state.last_error.as_deref() {
        if state.stage == GatewayEthPluginSessionStage::Disconnected
            || gateway_eth_plugin_session_error_is_connectivity_fatal(last_error)
        {
            return (
                GatewayEthPluginSessionStage::Disconnected,
                Some(last_error.to_string()),
            );
        }
        return (state.stage, Some(last_error.to_string()));
    }
    (state.stage, None)
}

fn gateway_eth_plugin_session_stage_stats_from_cache(
    chain_id: u64,
) -> (u64, GatewayEthPluginSessionStageStats) {
    let entries = gateway_eth_plugin_session_entries_for_chain(chain_id);
    let now_ms = now_unix_millis() as u64;
    let mut stats = GatewayEthPluginSessionStageStats::default();
    for state in entries.iter() {
        let (stage, _) =
            gateway_eth_plugin_effective_stage(chain_id, state.endpoint.as_str(), state, now_ms);
        match stage {
            GatewayEthPluginSessionStage::Disconnected => stats.disconnected += 1,
            GatewayEthPluginSessionStage::TcpConnected => stats.tcp_connected += 1,
            GatewayEthPluginSessionStage::AuthSent => stats.auth_sent += 1,
            GatewayEthPluginSessionStage::AckSeen => stats.ack_seen += 1,
            GatewayEthPluginSessionStage::Ready => stats.ready += 1,
        }
    }
    (entries.len() as u64, stats)
}

pub(super) fn gateway_eth_public_broadcast_plugin_peers_json(chain_id: u64) -> serde_json::Value {
    ensure_gateway_eth_plugin_rlpx_profile_loaded(chain_id);
    let snapshot = gateway_eth_public_broadcast_native_peers_snapshot(chain_id);
    let plugin_peers = snapshot
        .as_ref()
        .map(|state| state.plugin_peers.clone())
        .unwrap_or_else(|| Arc::new(Vec::new()));
    let peer_source = snapshot
        .as_ref()
        .map(|state| state.peer_source.clone())
        .unwrap_or_else(|| "none".to_string());
    let plugin_ports = snapshot
        .as_ref()
        .map(|state| state.plugin_ports.as_ref().clone())
        .unwrap_or_default();
    let probe = (!plugin_peers.is_empty())
        .then(|| probe_gateway_eth_plugin_peers_with_cache(chain_id, plugin_peers.as_ref()));
    let now_ms = now_unix_millis() as u64;
    let mut rows =
        std::collections::BTreeMap::<String, serde_json::Map<String, serde_json::Value>>::new();

    for peer in plugin_peers.iter() {
        rows.insert(
            peer.endpoint.clone(),
            serde_json::Map::from_iter([
                (
                    "endpoint".to_string(),
                    serde_json::Value::String(peer.endpoint.clone()),
                ),
                (
                    "addr_hint".to_string(),
                    serde_json::Value::String(peer.addr_hint.clone()),
                ),
                (
                    "node_hint".to_string(),
                    serde_json::Value::String(format!("0x{:x}", peer.node_hint)),
                ),
                (
                    "stage".to_string(),
                    serde_json::Value::String(
                        GatewayEthPluginSessionStage::Disconnected
                            .as_str()
                            .to_string(),
                    ),
                ),
                (
                    "updated_ms".to_string(),
                    serde_json::Value::String("0x0".to_string()),
                ),
                (
                    "last_error".to_string(),
                    serde_json::Value::String(String::new()),
                ),
            ]),
        );
    }

    for state in gateway_eth_plugin_session_entries_for_chain(chain_id) {
        let endpoint = state.endpoint.clone();
        let (stage, last_error) =
            gateway_eth_plugin_effective_stage(chain_id, endpoint.as_str(), &state, now_ms);
        let row = rows.entry(endpoint.clone()).or_insert_with(|| {
            let (node_hint, addr_hint) =
                parse_enode_endpoint(endpoint.as_str()).unwrap_or((0, String::new()));
            serde_json::Map::from_iter([
                (
                    "endpoint".to_string(),
                    serde_json::Value::String(endpoint.clone()),
                ),
                (
                    "addr_hint".to_string(),
                    serde_json::Value::String(addr_hint),
                ),
                (
                    "node_hint".to_string(),
                    serde_json::Value::String(format!("0x{:x}", node_hint)),
                ),
                (
                    "stage".to_string(),
                    serde_json::Value::String(
                        GatewayEthPluginSessionStage::Disconnected
                            .as_str()
                            .to_string(),
                    ),
                ),
                (
                    "updated_ms".to_string(),
                    serde_json::Value::String("0x0".to_string()),
                ),
                (
                    "last_error".to_string(),
                    serde_json::Value::String(String::new()),
                ),
            ])
        });
        row.insert(
            "stage".to_string(),
            serde_json::Value::String(stage.as_str().to_string()),
        );
        row.insert(
            "updated_ms".to_string(),
            serde_json::Value::String(format!("0x{:x}", state.updated_ms)),
        );
        row.insert(
            "last_error".to_string(),
            serde_json::Value::String(last_error.unwrap_or_default()),
        );
    }

    for (endpoint, row) in rows.iter_mut() {
        let worker_key = build_gateway_eth_plugin_rlpx_worker_key(chain_id, endpoint.as_str());
        let worker_state = snapshot_gateway_eth_plugin_rlpx_worker_state(worker_key.as_str());
        let worker_score = gateway_eth_plugin_rlpx_worker_score(chain_id, &worker_state);
        row.insert(
            "learning_score".to_string(),
            serde_json::Value::String(format!("0x{:x}", worker_state.learning_score)),
        );
        row.insert(
            "tier".to_string(),
            serde_json::Value::String(
                gateway_eth_plugin_rlpx_worker_tier_label(chain_id, &worker_state, now_ms)
                    .to_string(),
            ),
        );
        row.insert(
            "score".to_string(),
            serde_json::Value::String(format!("{worker_score}")),
        );
        row.insert(
            "dial_attempt_count".to_string(),
            serde_json::Value::String(format!("0x{:x}", worker_state.dial_attempt_count)),
        );
        row.insert(
            "ready_count".to_string(),
            serde_json::Value::String(format!("0x{:x}", worker_state.sessions_completed)),
        );
        row.insert(
            "new_pooled_count".to_string(),
            serde_json::Value::String(format!("0x{:x}", worker_state.total_new_pooled_msgs)),
        );
        row.insert(
            "sessions_completed".to_string(),
            serde_json::Value::String(format!("0x{:x}", worker_state.sessions_completed)),
        );
        row.insert(
            "sessions_with_gossip".to_string(),
            serde_json::Value::String(format!("0x{:x}", worker_state.sessions_with_gossip)),
        );
        row.insert(
            "total_new_pooled_hashes".to_string(),
            serde_json::Value::String(format!("0x{:x}", worker_state.total_new_pooled_hashes)),
        );
        row.insert(
            "total_unique_new_pooled_hashes".to_string(),
            serde_json::Value::String(format!(
                "0x{:x}",
                worker_state.total_unique_new_pooled_hashes
            )),
        );
        row.insert(
            "total_duplicate_new_pooled_hashes".to_string(),
            serde_json::Value::String(format!(
                "0x{:x}",
                worker_state.total_duplicate_new_pooled_hashes
            )),
        );
        row.insert(
            "last_new_pooled_ms".to_string(),
            serde_json::Value::String(format!("0x{:x}", worker_state.last_new_pooled_ms)),
        );
        row.insert(
            "recent_new_pooled_hashes_total".to_string(),
            serde_json::Value::String(format!(
                "0x{:x}",
                worker_state.recent_new_pooled_hashes_total
            )),
        );
        row.insert(
            "recent_new_pooled_hashes_window_start_ms".to_string(),
            serde_json::Value::String(format!(
                "0x{:x}",
                worker_state.recent_new_pooled_hashes_window_start_ms
            )),
        );
        row.insert(
            "pooled_txs_total".to_string(),
            serde_json::Value::String(format!("0x{:x}", worker_state.total_pooled_txs_received)),
        );
        row.insert(
            "total_unique_pooled_txs".to_string(),
            serde_json::Value::String(format!("0x{:x}", worker_state.total_unique_pooled_txs)),
        );
        row.insert(
            "total_duplicate_pooled_txs".to_string(),
            serde_json::Value::String(format!("0x{:x}", worker_state.total_duplicate_pooled_txs)),
        );
        row.insert(
            "total_pooled_txs_imported".to_string(),
            serde_json::Value::String(format!("0x{:x}", worker_state.total_pooled_txs_imported)),
        );
        row.insert(
            "first_seen_hash_count".to_string(),
            serde_json::Value::String(format!("0x{:x}", worker_state.first_seen_hash_count)),
        );
        row.insert(
            "first_seen_tx_count".to_string(),
            serde_json::Value::String(format!("0x{:x}", worker_state.first_seen_tx_count)),
        );
        row.insert(
            "total_swap_hits".to_string(),
            serde_json::Value::String(format!("0x{:x}", worker_state.total_swap_hits)),
        );
        row.insert(
            "total_swap_v2_hits".to_string(),
            serde_json::Value::String(format!("0x{:x}", worker_state.total_swap_v2_hits)),
        );
        row.insert(
            "total_swap_v3_hits".to_string(),
            serde_json::Value::String(format!("0x{:x}", worker_state.total_swap_v3_hits)),
        );
        row.insert(
            "total_unique_swap_hits".to_string(),
            serde_json::Value::String(format!("0x{:x}", worker_state.total_unique_swap_hits)),
        );
        row.insert(
            "last_swap_ms".to_string(),
            serde_json::Value::String(format!("0x{:x}", worker_state.last_swap_ms)),
        );
        row.insert(
            "recent_swap_hits_total".to_string(),
            serde_json::Value::String(format!("0x{:x}", worker_state.recent_swap_hits_total)),
        );
        row.insert(
            "recent_unique_swap_hits_total".to_string(),
            serde_json::Value::String(format!(
                "0x{:x}",
                worker_state.recent_unique_swap_hits_total
            )),
        );
        row.insert(
            "recent_swap_window_start_ms".to_string(),
            serde_json::Value::String(format!("0x{:x}", worker_state.recent_swap_window_start_ms)),
        );
        row.insert(
            "avg_first_gossip_latency_ms".to_string(),
            serde_json::Value::String(format!(
                "0x{:x}",
                gateway_eth_plugin_rlpx_avg_latency_ms(
                    worker_state.total_first_gossip_latency_ms,
                    worker_state.first_gossip_latency_samples
                )
            )),
        );
        row.insert(
            "avg_first_swap_latency_ms".to_string(),
            serde_json::Value::String(format!(
                "0x{:x}",
                gateway_eth_plugin_rlpx_avg_latency_ms(
                    worker_state.total_first_swap_latency_ms,
                    worker_state.first_swap_latency_samples
                )
            )),
        );
        row.insert(
            "recent_unique_new_pooled_hashes_total".to_string(),
            serde_json::Value::String(format!(
                "0x{:x}",
                worker_state.recent_unique_new_pooled_hashes_total
            )),
        );
        row.insert(
            "recent_unique_pooled_txs_total".to_string(),
            serde_json::Value::String(format!(
                "0x{:x}",
                worker_state.recent_unique_pooled_txs_total
            )),
        );
        row.insert(
            "recent_duplicate_new_pooled_hashes_total".to_string(),
            serde_json::Value::String(format!(
                "0x{:x}",
                worker_state.recent_duplicate_new_pooled_hashes_total
            )),
        );
        row.insert(
            "recent_duplicate_pooled_txs_total".to_string(),
            serde_json::Value::String(format!(
                "0x{:x}",
                worker_state.recent_duplicate_pooled_txs_total
            )),
        );
        row.insert(
            "recent_dedup_window_start_ms".to_string(),
            serde_json::Value::String(format!("0x{:x}", worker_state.recent_dedup_window_start_ms)),
        );
        row.insert(
            "disconnect_count".to_string(),
            serde_json::Value::String(format!("0x{:x}", worker_state.disconnect_count)),
        );
        row.insert(
            "disconnect_too_many_count".to_string(),
            serde_json::Value::String(format!("0x{:x}", worker_state.disconnect_too_many_count)),
        );
        row.insert(
            "disconnect_timeout_count".to_string(),
            serde_json::Value::String(format!("0x{:x}", worker_state.disconnect_timeout_count)),
        );
        row.insert(
            "disconnect_protocol_count".to_string(),
            serde_json::Value::String(format!("0x{:x}", worker_state.disconnect_protocol_count)),
        );
        row.insert(
            "disconnect_rate_bps".to_string(),
            serde_json::Value::String(format!(
                "0x{:x}",
                gateway_eth_plugin_rlpx_worker_disconnect_rate_bps(&worker_state)
            )),
        );
        row.insert(
            "success_rate_bps".to_string(),
            serde_json::Value::String(format!(
                "0x{:x}",
                gateway_eth_plugin_rlpx_worker_success_rate_bps(&worker_state)
            )),
        );
        row.insert(
            "last_first_post_ready_code".to_string(),
            serde_json::Value::String(format!("0x{:x}", worker_state.last_first_post_ready_code)),
        );
    }

    let items = rows
        .into_values()
        .map(serde_json::Value::Object)
        .collect::<Vec<_>>();
    let reachable = probe
        .as_ref()
        .map(|outcome| outcome.reachable_count)
        .unwrap_or(0);
    let checked_ms = probe
        .as_ref()
        .map(|outcome| outcome.checked_ms)
        .unwrap_or(now_ms);
    serde_json::json!({
        "chain_id": format!("0x{:x}", chain_id),
        "peer_source": peer_source,
        "plugin_ports": plugin_ports
            .iter()
            .map(|port| serde_json::Value::String(format!("0x{:x}", port)))
            .collect::<Vec<_>>(),
        "total": format!("0x{:x}", items.len()),
        "reachable": format!("0x{:x}", reachable),
        "checked_ms": format!("0x{:x}", checked_ms),
        "items": items,
    })
}

fn should_preserve_gateway_eth_plugin_session_stage_on_connect_failure(
    chain_id: u64,
    endpoint: &str,
    checked_ms: u64,
) -> bool {
    let stale_after_ms = gateway_eth_public_broadcast_plugin_probe_cache_ttl_ms(chain_id)
        .saturating_mul(3)
        .max(1_000);
    let key = build_gateway_eth_plugin_session_key(chain_id, endpoint);
    gateway_eth_plugin_session_cache()
        .get(key.as_str())
        .is_some_and(|state| {
            state.last_error.is_none()
                && state.stage.rank() >= GatewayEthPluginSessionStage::AuthSent.rank()
                && checked_ms.saturating_sub(state.updated_ms) <= stale_after_ms
        })
}

fn connect_gateway_eth_plugin_peer(
    addr_hint: &str,
    timeout_ms: u64,
) -> Result<std::net::TcpStream, String> {
    let socket_addr: std::net::SocketAddr = addr_hint
        .parse()
        .map_err(|e: std::net::AddrParseError| format!("addr_parse_failed({addr_hint}):{e}"))?;
    std::net::TcpStream::connect_timeout(&socket_addr, Duration::from_millis(timeout_ms.max(1)))
        .inspect(|stream| {
            let timeout = Some(Duration::from_millis(timeout_ms.max(1)));
            let _ = stream.set_read_timeout(timeout);
            let _ = stream.set_write_timeout(timeout);
        })
        .map_err(|e| format!("connect_failed({addr_hint}):{e}"))
}

fn gateway_eth_plugin_session_probe_enabled(
    mode: GatewayEthPluginSessionProbeMode,
    endpoint: &str,
) -> bool {
    match mode {
        GatewayEthPluginSessionProbeMode::Disabled => false,
        GatewayEthPluginSessionProbeMode::All => true,
        GatewayEthPluginSessionProbeMode::EnodeOnly => {
            endpoint.trim().to_ascii_lowercase().starts_with("enode://")
        }
    }
}

type GatewayEthRlpxHmacSha256 = Hmac<sha2::Sha256>;
type GatewayEthRlpxAes128Ctr = ctr::Ctr128BE<Aes128>;
type GatewayEthRlpxAes256Ctr = ctr::Ctr128BE<Aes256>;

#[derive(Clone)]
struct GatewayEthRlpxHashMac {
    cipher: Aes256,
    hash: GatewayEthRlpxHashState,
}

#[derive(Clone)]
#[allow(clippy::large_enum_variant)]
enum GatewayEthRlpxHashState {
    Keccak(sha3::Keccak256),
    #[cfg(test)]
    Fixed([u8; 32]),
}

impl GatewayEthRlpxHashMac {
    fn new(mac_secret: &[u8; 32], init: &[u8]) -> Result<Self, String> {
        let cipher = Aes256::new_from_slice(mac_secret)
            .map_err(|e| format!("rlpx_mac_cipher_invalid:{e}"))?;
        let mut hash = sha3::Keccak256::new();
        hash.update(init);
        Ok(Self {
            cipher,
            hash: GatewayEthRlpxHashState::Keccak(hash),
        })
    }

    #[cfg(test)]
    fn new_fixed_for_test(mac_secret: &[u8; 32], fixed: [u8; 32]) -> Result<Self, String> {
        let cipher = Aes256::new_from_slice(mac_secret)
            .map_err(|e| format!("rlpx_mac_cipher_invalid:{e}"))?;
        Ok(Self {
            cipher,
            hash: GatewayEthRlpxHashState::Fixed(fixed),
        })
    }

    fn hash_update(&mut self, bytes: &[u8]) {
        match &mut self.hash {
            GatewayEthRlpxHashState::Keccak(state) => state.update(bytes),
            #[cfg(test)]
            GatewayEthRlpxHashState::Fixed(_) => {}
        }
    }

    fn sum(&self) -> [u8; 32] {
        match &self.hash {
            GatewayEthRlpxHashState::Keccak(state) => {
                let clone = state.clone();
                let digest = clone.finalize();
                let mut out = [0u8; 32];
                out.copy_from_slice(digest.as_slice());
                out
            }
            #[cfg(test)]
            GatewayEthRlpxHashState::Fixed(fixed) => *fixed,
        }
    }

    fn compute_header(&mut self, header: &[u8]) -> [u8; 16] {
        let sum1 = self.sum();
        self.compute(sum1, header)
    }

    fn compute_frame(&mut self, frame: &[u8]) -> [u8; 16] {
        self.hash_update(frame);
        let seed = self.sum();
        self.compute(seed, &seed[..16])
    }

    fn compute(&mut self, sum1: [u8; 32], seed: &[u8]) -> [u8; 16] {
        if seed.len() != 16 {
            return [0u8; 16];
        }
        let mut aes_buffer =
            aes::cipher::generic_array::GenericArray::clone_from_slice(&sum1[..16]);
        self.cipher.encrypt_block(&mut aes_buffer);
        for (slot, b) in aes_buffer.iter_mut().zip(seed.iter()) {
            *slot ^= *b;
        }
        self.hash_update(aes_buffer.as_slice());
        let sum2 = self.sum();
        let mut out = [0u8; 16];
        out.copy_from_slice(&sum2[..16]);
        out
    }
}

#[derive(Clone)]
struct GatewayEthRlpxFrameSession {
    enc: GatewayEthRlpxAes256Ctr,
    dec: GatewayEthRlpxAes256Ctr,
    egress_mac: GatewayEthRlpxHashMac,
    ingress_mac: GatewayEthRlpxHashMac,
    snappy: bool,
}

struct GatewayEthRlpxHandshakeInitiatorOutcome {
    session: GatewayEthRlpxFrameSession,
    local_static_pub: [u8; 64],
}

#[derive(Clone)]
struct GatewayEthRlpxCapability {
    name: String,
    version: u64,
}

struct GatewayEthRlpxHello {
    protocol_version: u64,
    client_name: String,
    capabilities: Vec<GatewayEthRlpxCapability>,
    listen_port: u64,
    id_len: usize,
}

#[derive(Clone, Copy)]
enum GatewayEthRlpxRlpItem<'a> {
    Bytes(&'a [u8]),
    List(&'a [u8]),
}

fn gateway_eth_rlpx_decode_hex(raw: &str) -> Result<Vec<u8>, String> {
    let trimmed = raw.trim();
    if trimmed.is_empty() || !trimmed.len().is_multiple_of(2) {
        return Err("invalid_hex".to_string());
    }
    let mut out = Vec::with_capacity(trimmed.len() / 2);
    let mut chars = trimmed.as_bytes().chunks_exact(2);
    for pair in &mut chars {
        let hi = (pair[0] as char)
            .to_digit(16)
            .ok_or_else(|| "invalid_hex".to_string())?;
        let lo = (pair[1] as char)
            .to_digit(16)
            .ok_or_else(|| "invalid_hex".to_string())?;
        out.push(((hi << 4) | lo) as u8);
    }
    Ok(out)
}

fn gateway_eth_rlpx_parse_enode_pubkey(endpoint: &str) -> Result<K256PublicKey, String> {
    let trimmed = endpoint.trim();
    let lower = trimmed.to_ascii_lowercase();
    if !lower.starts_with("enode://") {
        return Err("endpoint_not_enode".to_string());
    }
    let raw = &trimmed["enode://".len()..];
    let (pubkey_hex, _addr) = raw
        .split_once('@')
        .ok_or_else(|| "enode_missing_at".to_string())?;
    let pubkey_bytes = gateway_eth_rlpx_decode_hex(pubkey_hex)?;
    if pubkey_bytes.len() != GATEWAY_ETH_PLUGIN_RLPX_PUB_LEN {
        return Err("enode_pubkey_len_invalid".to_string());
    }
    let mut sec1 = [0u8; GATEWAY_ETH_PLUGIN_RLPX_ECIES_PUB_LEN];
    sec1[0] = 0x04;
    sec1[1..].copy_from_slice(&pubkey_bytes);
    K256PublicKey::from_sec1_bytes(&sec1).map_err(|e| format!("enode_pubkey_parse_failed:{e}"))
}

fn gateway_eth_rlpx_pubkey_64_from_signing_key(
    signing_key: &SigningKey,
) -> [u8; GATEWAY_ETH_PLUGIN_RLPX_PUB_LEN] {
    let encoded = signing_key.verifying_key().to_encoded_point(false);
    let bytes = encoded.as_bytes();
    let mut out = [0u8; GATEWAY_ETH_PLUGIN_RLPX_PUB_LEN];
    out.copy_from_slice(&bytes[1..1 + GATEWAY_ETH_PLUGIN_RLPX_PUB_LEN]);
    out
}

fn gateway_eth_rlpx_pubkey_65_from_signing_key(signing_key: &SigningKey) -> [u8; 65] {
    let encoded = signing_key.verifying_key().to_encoded_point(false);
    let mut out = [0u8; 65];
    out.copy_from_slice(encoded.as_bytes());
    out
}

fn gateway_eth_rlpx_local_static_nodekey_bytes() -> [u8; 32] {
    static NODEKEY: std::sync::OnceLock<[u8; 32]> = std::sync::OnceLock::new();
    *NODEKEY.get_or_init(|| {
        use rand::RngCore;
        let env_key = "NOVOVM_GATEWAY_ETH_PLUGIN_RLPX_NODEKEY_HEX";
        if let Some(raw) = string_env_nonempty(env_key) {
            match gateway_eth_rlpx_decode_hex(raw.as_str()) {
                Ok(bytes) if bytes.len() == 32 => {
                    let mut out = [0u8; 32];
                    out.copy_from_slice(bytes.as_slice());
                    return out;
                }
                Ok(bytes) => {
                    if gateway_warn_enabled() {
                        eprintln!(
                            "gateway_warn: rlpx nodekey env ignored: key={} reason=invalid_len len={}",
                            env_key,
                            bytes.len()
                        );
                    }
                }
                Err(err) => {
                    if gateway_warn_enabled() {
                        eprintln!(
                            "gateway_warn: rlpx nodekey env ignored: key={} reason={}",
                            env_key, err
                        );
                    }
                }
            }
        }
        let mut out = [0u8; 32];
        OsRng.fill_bytes(&mut out);
        if gateway_warn_enabled() {
            eprintln!(
                "gateway_warn: rlpx nodekey source=random process_local=true pubkey_stable_in_process=true"
            );
        }
        out
    })
}

fn gateway_eth_rlpx_concat_kdf_sha256(z: &[u8], s1: &[u8], len: usize) -> Vec<u8> {
    use sha2::Digest;
    let mut out = Vec::<u8>::with_capacity(len);
    let mut counter: u32 = 1;
    while out.len() < len {
        let mut hasher = sha2::Sha256::new();
        hasher.update(counter.to_be_bytes());
        hasher.update(z);
        hasher.update(s1);
        out.extend_from_slice(hasher.finalize().as_slice());
        counter = counter.saturating_add(1);
    }
    out.truncate(len);
    out
}

fn gateway_eth_rlpx_derive_ecies_keys(z: &[u8]) -> ([u8; 16], [u8; 32]) {
    use sha2::Digest;
    let k = gateway_eth_rlpx_concat_kdf_sha256(z, &[], 32);
    let mut ke = [0u8; 16];
    ke.copy_from_slice(&k[0..16]);
    let mut km_hasher = sha2::Sha256::new();
    km_hasher.update(&k[16..32]);
    let km_raw = km_hasher.finalize();
    let mut km = [0u8; 32];
    km.copy_from_slice(km_raw.as_slice());
    (ke, km)
}

fn gateway_eth_rlpx_ecdh_shared(
    local_secret: &K256SecretKey,
    remote_pub: &K256PublicKey,
) -> [u8; 32] {
    let shared = diffie_hellman(local_secret.to_nonzero_scalar(), remote_pub.as_affine());
    let mut out = [0u8; 32];
    out.copy_from_slice(shared.raw_secret_bytes().as_slice());
    out
}

fn gateway_eth_rlpx_ecies_encrypt(
    remote_pub: &K256PublicKey,
    plaintext: &[u8],
    shared_mac_data: &[u8],
) -> Result<Vec<u8>, String> {
    use rand::RngCore;

    let eph_signing = SigningKey::random(&mut OsRng);
    let eph_secret = K256SecretKey::from_slice(eph_signing.to_bytes().as_slice())
        .map_err(|e| format!("rlpx_ecies_eph_secret_invalid:{e}"))?;
    let shared = gateway_eth_rlpx_ecdh_shared(&eph_secret, remote_pub);
    let (ke, km) = gateway_eth_rlpx_derive_ecies_keys(&shared);

    let mut iv = [0u8; GATEWAY_ETH_PLUGIN_RLPX_ECIES_IV_LEN];
    OsRng.fill_bytes(&mut iv);
    let mut encrypted = plaintext.to_vec();
    let mut stream = GatewayEthRlpxAes128Ctr::new((&ke).into(), (&iv).into());
    stream.apply_keystream(&mut encrypted);

    let mut encrypted_payload = Vec::with_capacity(iv.len() + encrypted.len());
    encrypted_payload.extend_from_slice(&iv);
    encrypted_payload.extend_from_slice(&encrypted);

    let mut mac = <GatewayEthRlpxHmacSha256 as Mac>::new_from_slice(&km)
        .map_err(|e| format!("rlpx_ecies_hmac_key_invalid:{e}"))?;
    mac.update(encrypted_payload.as_slice());
    mac.update(shared_mac_data);
    let tag = mac.finalize().into_bytes();

    let eph_pub = gateway_eth_rlpx_pubkey_65_from_signing_key(&eph_signing);
    let mut out = Vec::with_capacity(
        GATEWAY_ETH_PLUGIN_RLPX_ECIES_PUB_LEN + encrypted_payload.len() + tag.len(),
    );
    out.extend_from_slice(&eph_pub);
    out.extend_from_slice(encrypted_payload.as_slice());
    out.extend_from_slice(tag.as_slice());
    Ok(out)
}

fn gateway_eth_rlpx_ecies_decrypt(
    local_secret: &K256SecretKey,
    ciphertext: &[u8],
    shared_mac_data: &[u8],
) -> Result<Vec<u8>, String> {
    if ciphertext.len()
        < GATEWAY_ETH_PLUGIN_RLPX_ECIES_OVERHEAD + GATEWAY_ETH_PLUGIN_RLPX_ECIES_IV_LEN
    {
        return Err("rlpx_ecies_ciphertext_too_short".to_string());
    }
    let eph_pub =
        K256PublicKey::from_sec1_bytes(&ciphertext[0..GATEWAY_ETH_PLUGIN_RLPX_ECIES_PUB_LEN])
            .map_err(|e| format!("rlpx_ecies_eph_pub_invalid:{e}"))?;
    let payload_start = GATEWAY_ETH_PLUGIN_RLPX_ECIES_PUB_LEN;
    let payload_end = ciphertext
        .len()
        .saturating_sub(GATEWAY_ETH_PLUGIN_RLPX_ECIES_MAC_LEN);
    if payload_end <= payload_start + GATEWAY_ETH_PLUGIN_RLPX_ECIES_IV_LEN {
        return Err("rlpx_ecies_payload_too_short".to_string());
    }
    let payload = &ciphertext[payload_start..payload_end];
    let tag = &ciphertext[payload_end..];

    let shared = gateway_eth_rlpx_ecdh_shared(local_secret, &eph_pub);
    let (ke, km) = gateway_eth_rlpx_derive_ecies_keys(&shared);
    let mut mac = <GatewayEthRlpxHmacSha256 as Mac>::new_from_slice(&km)
        .map_err(|e| format!("rlpx_ecies_hmac_key_invalid:{e}"))?;
    mac.update(payload);
    mac.update(shared_mac_data);
    mac.verify_slice(tag)
        .map_err(|_| "rlpx_ecies_mac_mismatch".to_string())?;

    let iv = &payload[0..GATEWAY_ETH_PLUGIN_RLPX_ECIES_IV_LEN];
    let encrypted = &payload[GATEWAY_ETH_PLUGIN_RLPX_ECIES_IV_LEN..];
    let mut plain = encrypted.to_vec();
    let mut stream = GatewayEthRlpxAes128Ctr::new((&ke).into(), iv.into());
    stream.apply_keystream(&mut plain);
    Ok(plain)
}

fn gateway_eth_rlpx_parse_item(input: &[u8]) -> Result<(GatewayEthRlpxRlpItem<'_>, usize), String> {
    if input.is_empty() {
        return Err("rlpx_rlp_empty".to_string());
    }
    let lead = input[0];
    match lead {
        0x00..=0x7f => Ok((GatewayEthRlpxRlpItem::Bytes(&input[..1]), 1)),
        0x80..=0xb7 => {
            let len = (lead - 0x80) as usize;
            if input.len() < 1 + len {
                return Err("rlpx_rlp_short_bytes".to_string());
            }
            Ok((GatewayEthRlpxRlpItem::Bytes(&input[1..1 + len]), 1 + len))
        }
        0xb8..=0xbf => {
            let len_of_len = (lead - 0xb7) as usize;
            if input.len() < 1 + len_of_len {
                return Err("rlpx_rlp_short_bytes_len".to_string());
            }
            let mut len = 0usize;
            for byte in &input[1..1 + len_of_len] {
                len = (len << 8) | (*byte as usize);
            }
            if input.len() < 1 + len_of_len + len {
                return Err("rlpx_rlp_short_bytes_payload".to_string());
            }
            Ok((
                GatewayEthRlpxRlpItem::Bytes(&input[1 + len_of_len..1 + len_of_len + len]),
                1 + len_of_len + len,
            ))
        }
        0xc0..=0xf7 => {
            let len = (lead - 0xc0) as usize;
            if input.len() < 1 + len {
                return Err("rlpx_rlp_short_list".to_string());
            }
            Ok((GatewayEthRlpxRlpItem::List(&input[1..1 + len]), 1 + len))
        }
        _ => {
            let len_of_len = (lead - 0xf7) as usize;
            if input.len() < 1 + len_of_len {
                return Err("rlpx_rlp_short_list_len".to_string());
            }
            let mut len = 0usize;
            for byte in &input[1..1 + len_of_len] {
                len = (len << 8) | (*byte as usize);
            }
            if input.len() < 1 + len_of_len + len {
                return Err("rlpx_rlp_short_list_payload".to_string());
            }
            Ok((
                GatewayEthRlpxRlpItem::List(&input[1 + len_of_len..1 + len_of_len + len]),
                1 + len_of_len + len,
            ))
        }
    }
}

fn gateway_eth_rlpx_parse_list_items(
    payload: &[u8],
) -> Result<Vec<GatewayEthRlpxRlpItem<'_>>, String> {
    let mut items = Vec::new();
    let mut cursor = 0usize;
    while cursor < payload.len() {
        let (item, consumed) = gateway_eth_rlpx_parse_item(&payload[cursor..])?;
        items.push(item);
        cursor = cursor.saturating_add(consumed);
    }
    if cursor != payload.len() {
        return Err("rlpx_rlp_list_trailing".to_string());
    }
    Ok(items)
}

fn gateway_eth_rlpx_decode_auth_resp_v4(plain: &[u8]) -> Result<([u8; 64], [u8; 32], u64), String> {
    // EIP-8 auth-ack can carry random trailing padding bytes after the first RLP item.
    // Parse the first item and ignore trailing bytes.
    let (top, _) = gateway_eth_rlpx_parse_item(plain)?;
    let GatewayEthRlpxRlpItem::List(payload) = top else {
        return Err("rlpx_auth_resp_not_list".to_string());
    };
    let fields = gateway_eth_rlpx_parse_list_items(payload)?;
    if fields.len() < 3 {
        return Err("rlpx_auth_resp_fields_short".to_string());
    }
    let GatewayEthRlpxRlpItem::Bytes(random_pub_bytes) = fields[0] else {
        return Err("rlpx_auth_resp_pub_not_bytes".to_string());
    };
    let GatewayEthRlpxRlpItem::Bytes(nonce_bytes) = fields[1] else {
        return Err("rlpx_auth_resp_nonce_not_bytes".to_string());
    };
    let GatewayEthRlpxRlpItem::Bytes(version_bytes) = fields[2] else {
        return Err("rlpx_auth_resp_version_not_bytes".to_string());
    };
    if random_pub_bytes.len() != GATEWAY_ETH_PLUGIN_RLPX_PUB_LEN {
        return Err("rlpx_auth_resp_pub_len_invalid".to_string());
    }
    if nonce_bytes.len() != GATEWAY_ETH_PLUGIN_RLPX_NONCE_LEN {
        return Err("rlpx_auth_resp_nonce_len_invalid".to_string());
    }
    let mut random_pub = [0u8; 64];
    random_pub.copy_from_slice(random_pub_bytes);
    let mut nonce = [0u8; 32];
    nonce.copy_from_slice(nonce_bytes);
    let mut version = 0u64;
    for byte in version_bytes {
        version = (version << 8) | (*byte as u64);
    }
    Ok((random_pub, nonce, version))
}

fn gateway_eth_rlpx_encode_len(prefix_small: u8, prefix_long: u8, len: usize) -> Vec<u8> {
    if len <= 55 {
        return vec![prefix_small + len as u8];
    }
    let mut len_bytes = Vec::new();
    let mut value = len;
    while value > 0 {
        len_bytes.push((value & 0xff) as u8);
        value >>= 8;
    }
    len_bytes.reverse();
    let mut out = Vec::with_capacity(1 + len_bytes.len());
    out.push(prefix_long + len_bytes.len() as u8);
    out.extend(len_bytes);
    out
}

fn gateway_eth_rlpx_encode_bytes(bytes: &[u8]) -> Vec<u8> {
    if bytes.len() == 1 && bytes[0] < 0x80 {
        return vec![bytes[0]];
    }
    let mut out = gateway_eth_rlpx_encode_len(0x80, 0xb7, bytes.len());
    out.extend_from_slice(bytes);
    out
}

fn gateway_eth_rlpx_encode_u64(v: u64) -> Vec<u8> {
    if v == 0 {
        return gateway_eth_rlpx_encode_bytes(&[]);
    }
    let bytes = v.to_be_bytes();
    let first_non_zero = bytes
        .iter()
        .position(|byte| *byte != 0)
        .unwrap_or(bytes.len().saturating_sub(1));
    gateway_eth_rlpx_encode_bytes(&bytes[first_non_zero..])
}

fn gateway_eth_rlpx_encode_list(items: &[Vec<u8>]) -> Vec<u8> {
    let payload_len = items.iter().map(Vec::len).sum::<usize>();
    let mut out = gateway_eth_rlpx_encode_len(0xc0, 0xf7, payload_len);
    for item in items {
        out.extend_from_slice(item.as_slice());
    }
    out
}

fn gateway_eth_rlpx_decode_u64_bytes(bytes: &[u8]) -> Result<u64, String> {
    if bytes.len() > 8 {
        return Err("rlpx_u64_len_invalid".to_string());
    }
    if bytes.is_empty() {
        return Ok(0);
    }
    let mut out = 0u64;
    for byte in bytes {
        out = (out << 8) | (*byte as u64);
    }
    Ok(out)
}

fn gateway_eth_rlpx_keccak256(parts: &[&[u8]]) -> [u8; 32] {
    let mut hasher = sha3::Keccak256::new();
    for part in parts {
        hasher.update(part);
    }
    let digest = hasher.finalize();
    let mut out = [0u8; 32];
    out.copy_from_slice(digest.as_slice());
    out
}

fn gateway_eth_rlpx_xor_32(a: &[u8; 32], b: &[u8; 32]) -> [u8; 32] {
    let mut out = [0u8; 32];
    for (slot, (lhs, rhs)) in out.iter_mut().zip(a.iter().zip(b.iter())) {
        *slot = *lhs ^ *rhs;
    }
    out
}

fn gateway_eth_rlpx_round_up_16(size: usize) -> usize {
    let rem = size % 16;
    if rem == 0 {
        size
    } else {
        size + (16 - rem)
    }
}

fn gateway_eth_rlpx_eth_protocol_length(version: u64) -> Option<u64> {
    match version {
        GATEWAY_ETH_PLUGIN_RLPX_ETH_PROTO_69 => Some(GATEWAY_ETH_PLUGIN_RLPX_ETH_PROTO_LEN_69),
        GATEWAY_ETH_PLUGIN_RLPX_ETH_PROTO_68
        | GATEWAY_ETH_PLUGIN_RLPX_ETH_PROTO_67
        | GATEWAY_ETH_PLUGIN_RLPX_ETH_PROTO_66 => Some(GATEWAY_ETH_PLUGIN_RLPX_ETH_PROTO_LEN_66_68),
        _ => None,
    }
}

fn gateway_eth_rlpx_select_shared_eth_version(
    local_caps: &[GatewayEthRlpxCapability],
    remote_caps: &[GatewayEthRlpxCapability],
) -> Option<u64> {
    [
        GATEWAY_ETH_PLUGIN_RLPX_ETH_PROTO_69,
        GATEWAY_ETH_PLUGIN_RLPX_ETH_PROTO_68,
        GATEWAY_ETH_PLUGIN_RLPX_ETH_PROTO_67,
        GATEWAY_ETH_PLUGIN_RLPX_ETH_PROTO_66,
    ]
    .into_iter()
    .find(|version| {
        gateway_eth_rlpx_eth_protocol_length(*version).is_some()
            && local_caps
                .iter()
                .any(|cap| cap.name.eq_ignore_ascii_case("eth") && cap.version == *version)
            && remote_caps
                .iter()
                .any(|cap| cap.name.eq_ignore_ascii_case("eth") && cap.version == *version)
    })
}

fn gateway_eth_rlpx_hello_profile(chain_id: u64) -> String {
    gateway_eth_public_broadcast_chain_string_env(
        chain_id,
        "NOVOVM_GATEWAY_ETH_PLUGIN_RLPX_HELLO_PROFILE",
    )
    .map(|raw| raw.trim().to_ascii_lowercase())
    .unwrap_or_else(|| "supervm".to_string())
}

fn gateway_eth_rlpx_hello_name(chain_id: u64, profile: &str) -> String {
    if let Some(raw) = gateway_eth_public_broadcast_chain_string_env(
        chain_id,
        "NOVOVM_GATEWAY_ETH_PLUGIN_RLPX_HELLO_NAME",
    ) {
        return raw;
    }
    if profile == "geth" {
        // Keep a realistic geth-style client id for wire compatibility experiments.
        return "Geth/v1.14.12-stable/linux-amd64/go1.22.5".to_string();
    }
    "SuperVM/novovm-evm-gateway".to_string()
}

fn gateway_eth_rlpx_hello_listen_port(chain_id: u64, profile: &str) -> u64 {
    if let Some(value) = gateway_eth_public_broadcast_chain_string_env(
        chain_id,
        "NOVOVM_GATEWAY_ETH_PLUGIN_RLPX_HELLO_LISTEN_PORT",
    )
    .and_then(|raw| raw.trim().parse::<u64>().ok())
    {
        return value.min(u16::MAX as u64);
    }
    if profile == "geth" {
        return gateway_eth_public_broadcast_plugin_ports(chain_id)
            .first()
            .copied()
            .unwrap_or(30303) as u64;
    }
    0
}

fn gateway_eth_rlpx_caps_are_geth_canonical(caps: &[GatewayEthRlpxCapability]) -> bool {
    caps.windows(2).all(|pair| {
        let lhs = &pair[0];
        let rhs = &pair[1];
        let name_cmp = lhs.name.as_str().cmp(rhs.name.as_str());
        name_cmp.is_lt() || (name_cmp.is_eq() && lhs.version <= rhs.version)
    })
}

fn gateway_eth_rlpx_build_hello_payload(
    local_static_pub: &[u8; 64],
    caps: &[GatewayEthRlpxCapability],
    client_name: &str,
    listen_port: u64,
) -> Vec<u8> {
    let caps_rlp_items = caps
        .iter()
        .map(|cap| {
            gateway_eth_rlpx_encode_list(&[
                gateway_eth_rlpx_encode_bytes(cap.name.as_bytes()),
                gateway_eth_rlpx_encode_u64(cap.version),
            ])
        })
        .collect::<Vec<_>>();
    gateway_eth_rlpx_encode_list(&[
        gateway_eth_rlpx_encode_u64(5),
        gateway_eth_rlpx_encode_bytes(client_name.as_bytes()),
        gateway_eth_rlpx_encode_list(&caps_rlp_items),
        gateway_eth_rlpx_encode_u64(listen_port),
        gateway_eth_rlpx_encode_bytes(local_static_pub),
    ])
}

fn gateway_eth_rlpx_parse_hello_payload(payload: &[u8]) -> Result<GatewayEthRlpxHello, String> {
    let (root, consumed) = gateway_eth_rlpx_parse_item(payload)?;
    if consumed != payload.len() {
        return Err("rlpx_hello_trailing".to_string());
    }
    let GatewayEthRlpxRlpItem::List(root_payload) = root else {
        return Err("rlpx_hello_not_list".to_string());
    };
    let fields = gateway_eth_rlpx_parse_list_items(root_payload)?;
    if fields.len() < 5 {
        return Err("rlpx_hello_fields_short".to_string());
    }
    let GatewayEthRlpxRlpItem::Bytes(version_bytes) = fields[0] else {
        return Err("rlpx_hello_version_not_bytes".to_string());
    };
    let protocol_version = gateway_eth_rlpx_decode_u64_bytes(version_bytes)?;
    let GatewayEthRlpxRlpItem::Bytes(name_bytes) = fields[1] else {
        return Err("rlpx_hello_name_not_bytes".to_string());
    };
    let client_name = String::from_utf8_lossy(name_bytes).to_string();
    let GatewayEthRlpxRlpItem::List(caps_payload) = fields[2] else {
        return Err("rlpx_hello_caps_not_list".to_string());
    };
    let GatewayEthRlpxRlpItem::Bytes(listen_port_bytes) = fields[3] else {
        return Err("rlpx_hello_listen_port_not_bytes".to_string());
    };
    let listen_port = gateway_eth_rlpx_decode_u64_bytes(listen_port_bytes)?;
    let GatewayEthRlpxRlpItem::Bytes(id_bytes) = fields[4] else {
        return Err("rlpx_hello_id_not_bytes".to_string());
    };
    let id_len = id_bytes.len();
    let cap_entries = gateway_eth_rlpx_parse_list_items(caps_payload)?;
    let mut capabilities = Vec::with_capacity(cap_entries.len());
    for cap_entry in cap_entries {
        let GatewayEthRlpxRlpItem::List(cap_fields_payload) = cap_entry else {
            continue;
        };
        let cap_fields = gateway_eth_rlpx_parse_list_items(cap_fields_payload)?;
        if cap_fields.len() < 2 {
            continue;
        }
        let GatewayEthRlpxRlpItem::Bytes(name_bytes) = cap_fields[0] else {
            continue;
        };
        let GatewayEthRlpxRlpItem::Bytes(version_bytes) = cap_fields[1] else {
            continue;
        };
        let version = gateway_eth_rlpx_decode_u64_bytes(version_bytes)?;
        let name = String::from_utf8_lossy(name_bytes).to_string();
        capabilities.push(GatewayEthRlpxCapability { name, version });
    }
    Ok(GatewayEthRlpxHello {
        protocol_version,
        client_name,
        capabilities,
        listen_port,
        id_len,
    })
}

fn gateway_eth_rlpx_parse_eth_status_chain_id(payload: &[u8]) -> Result<u64, String> {
    let (root, consumed) = gateway_eth_rlpx_parse_item(payload)?;
    if consumed != payload.len() {
        return Err("rlpx_eth_status_trailing".to_string());
    }
    let GatewayEthRlpxRlpItem::List(root_payload) = root else {
        return Err("rlpx_eth_status_not_list".to_string());
    };
    let fields = gateway_eth_rlpx_parse_list_items(root_payload)?;
    if fields.len() < 2 {
        return Err("rlpx_eth_status_fields_short".to_string());
    }
    let GatewayEthRlpxRlpItem::Bytes(chain_id_bytes) = fields[1] else {
        return Err("rlpx_eth_status_chain_id_not_bytes".to_string());
    };
    gateway_eth_rlpx_decode_u64_bytes(chain_id_bytes)
}

fn gateway_eth_rlpx_parse_hashes_from_new_pooled_payload(
    payload: &[u8],
    max_hashes: usize,
) -> Result<Vec<[u8; 32]>, String> {
    let (root, consumed) = gateway_eth_rlpx_parse_item(payload)?;
    if consumed != payload.len() {
        return Err("rlpx_new_pooled_hashes_trailing".to_string());
    }
    let GatewayEthRlpxRlpItem::List(root_payload) = root else {
        return Err("rlpx_new_pooled_hashes_not_list".to_string());
    };
    let fields = gateway_eth_rlpx_parse_list_items(root_payload)?;
    let hashes_payload = if fields.len() >= 3 {
        match fields[2] {
            GatewayEthRlpxRlpItem::List(items) => items,
            _ => return Err("rlpx_new_pooled_hashes_field_not_list".to_string()),
        }
    } else {
        root_payload
    };
    let hash_items = gateway_eth_rlpx_parse_list_items(hashes_payload)?;
    let mut out = Vec::new();
    for item in hash_items {
        if out.len() >= max_hashes {
            break;
        }
        let GatewayEthRlpxRlpItem::Bytes(hash_bytes) = item else {
            continue;
        };
        if hash_bytes.len() != 32 {
            continue;
        }
        let mut hash = [0u8; 32];
        hash.copy_from_slice(hash_bytes);
        out.push(hash);
    }
    Ok(out)
}

fn gateway_eth_rlpx_disconnect_reason_name(code: u64) -> &'static str {
    match code {
        0x00 => "disconnect_requested",
        0x01 => "tcp_subsystem_error",
        0x02 => "breach_of_protocol",
        0x03 => "useless_peer",
        0x04 => "too_many_peers",
        0x05 => "already_connected",
        0x06 => "incompatible_p2p_protocol_version",
        0x07 => "null_node_identity_received",
        0x08 => "client_quitting",
        0x09 => "unexpected_identity",
        0x0a => "connected_to_self",
        0x0b => "read_timeout",
        0x10 => "subprotocol_error",
        _ => "unknown",
    }
}

fn gateway_eth_rlpx_parse_disconnect_reason(payload: &[u8]) -> Option<u64> {
    if payload.is_empty() {
        return None;
    }
    let (root, consumed) = gateway_eth_rlpx_parse_item(payload).ok()?;
    if consumed != payload.len() {
        return None;
    }
    match root {
        GatewayEthRlpxRlpItem::Bytes(bytes) => gateway_eth_rlpx_decode_u64_bytes(bytes).ok(),
        GatewayEthRlpxRlpItem::List(list_payload) => {
            let fields = gateway_eth_rlpx_parse_list_items(list_payload).ok()?;
            let GatewayEthRlpxRlpItem::Bytes(first) = *fields.first()? else {
                return None;
            };
            gateway_eth_rlpx_decode_u64_bytes(first).ok()
        }
    }
}

fn gateway_eth_rlpx_build_get_pooled_payload(request_id: u64, hashes: &[[u8; 32]]) -> Vec<u8> {
    let hash_items = hashes
        .iter()
        .map(|hash| gateway_eth_rlpx_encode_bytes(hash))
        .collect::<Vec<_>>();
    gateway_eth_rlpx_encode_list(&[
        gateway_eth_rlpx_encode_u64(request_id),
        gateway_eth_rlpx_encode_list(&hash_items),
    ])
}

fn gateway_eth_rlpx_extract_raw_txs_from_pooled_payload(
    payload: &[u8],
    max_txs: usize,
) -> Result<Vec<Vec<u8>>, String> {
    let (root, consumed) = gateway_eth_rlpx_parse_item(payload)?;
    if consumed != payload.len() {
        return Err("rlpx_pooled_txs_trailing".to_string());
    }
    let GatewayEthRlpxRlpItem::List(root_payload) = root else {
        return Err("rlpx_pooled_txs_not_list".to_string());
    };
    let fields = gateway_eth_rlpx_parse_list_items(root_payload)?;
    let txs_payload = if fields.len() >= 2 {
        match fields[1] {
            GatewayEthRlpxRlpItem::List(items) => items,
            _ => return Err("rlpx_pooled_txs_field_not_list".to_string()),
        }
    } else {
        root_payload
    };
    let mut out = Vec::new();
    let mut cursor = 0usize;
    while cursor < txs_payload.len() && out.len() < max_txs {
        let (item, consumed) = gateway_eth_rlpx_parse_item(&txs_payload[cursor..])?;
        if consumed == 0 {
            return Err("rlpx_pooled_txs_item_consumed_zero".to_string());
        }
        let raw_tx = match item {
            GatewayEthRlpxRlpItem::Bytes(bytes) => bytes.to_vec(),
            GatewayEthRlpxRlpItem::List(_) => txs_payload[cursor..cursor + consumed].to_vec(),
        };
        if !raw_tx.is_empty() {
            out.push(raw_tx);
        }
        cursor = cursor.saturating_add(consumed);
    }
    Ok(out)
}

fn gateway_eth_rlpx_is_timeout_like(err: &str) -> bool {
    let normalized = err.to_ascii_lowercase();
    // Keep timeout detection locale-agnostic:
    // on zh-CN Windows, std::io::Error text is localized but still carries "os error 10060/10035".
    normalized.contains("timed out")
        || normalized.contains("would block")
        || normalized.contains("timeout")
        || normalized.contains("os error 10060")
        || normalized.contains("os error 10035")
        || err.contains("没有正确答复")
        || err.contains("没有反应")
}

fn gateway_eth_rlpx_hex_preview(bytes: &[u8], max_len: usize) -> String {
    if bytes.is_empty() {
        return String::new();
    }
    let take = bytes.len().min(max_len.max(1));
    let mut preview = to_hex(&bytes[..take]);
    if bytes.len() > take {
        preview.push_str("...");
    }
    preview
}

fn gateway_eth_rlpx_read_exact_with_partial<R: std::io::Read>(
    stream: &mut R,
    buf: &mut [u8],
    error_prefix: &str,
) -> Result<(), String> {
    let mut read_total = 0usize;
    while read_total < buf.len() {
        match stream.read(&mut buf[read_total..]) {
            Ok(0) => {
                let partial = gateway_eth_rlpx_hex_preview(&buf[..read_total], 24);
                return Err(format!(
                    "{error_prefix}:eof read={read_total}/{} partial=0x{}",
                    buf.len(),
                    partial
                ));
            }
            Ok(read_now) => {
                read_total += read_now;
            }
            Err(err) => {
                let err_text = err.to_string();
                // If we already consumed part of the frame and then hit a timeout-like read error,
                // keep reading to avoid losing framing alignment.
                if read_total > 0 && gateway_eth_rlpx_is_timeout_like(err_text.as_str()) {
                    continue;
                }
                let partial = gateway_eth_rlpx_hex_preview(&buf[..read_total], 24);
                return Err(format!(
                    "{error_prefix}:{err_text} read={read_total}/{} partial=0x{}",
                    buf.len(),
                    partial
                ));
            }
        }
    }
    Ok(())
}

struct GatewayEthRlpxWireFrameRead {
    code: u64,
    payload_len: usize,
    payload_encoded_len: usize,
    frame_size: usize,
    padded_size: usize,
    code_rlp_len: usize,
    header_plain: [u8; GATEWAY_ETH_PLUGIN_RLPX_FRAME_HEADER_LEN],
    header_cipher: [u8; GATEWAY_ETH_PLUGIN_RLPX_FRAME_HEADER_LEN],
    header_mac: [u8; GATEWAY_ETH_PLUGIN_RLPX_FRAME_HEADER_MAC_LEN],
    frame_plain_head: String,
    frame_cipher_head: String,
    frame_mac: [u8; GATEWAY_ETH_PLUGIN_RLPX_FRAME_MAC_LEN],
    ingress_mac_before: [u8; 16],
    ingress_mac_after_header: [u8; 16],
    ingress_mac_after_frame: [u8; 16],
    egress_mac_before: [u8; 16],
    egress_mac_after: [u8; 16],
}

fn gateway_eth_rlpx_mac_digest_head(mac: &GatewayEthRlpxHashMac) -> [u8; 16] {
    let digest = mac.sum();
    let mut out = [0u8; 16];
    out.copy_from_slice(&digest[..16]);
    out
}

fn gateway_eth_rlpx_read_wire_frame_with_trace<R: std::io::Read>(
    stream: &mut R,
    session: &mut GatewayEthRlpxFrameSession,
) -> Result<(u64, Vec<u8>, GatewayEthRlpxWireFrameRead), String> {
    let ingress_mac_before = gateway_eth_rlpx_mac_digest_head(&session.ingress_mac);
    let egress_mac_before = gateway_eth_rlpx_mac_digest_head(&session.egress_mac);

    let mut header_cipher = [0u8; GATEWAY_ETH_PLUGIN_RLPX_FRAME_HEADER_LEN];
    let mut header_mac = [0u8; GATEWAY_ETH_PLUGIN_RLPX_FRAME_HEADER_MAC_LEN];
    gateway_eth_rlpx_read_exact_with_partial(
        stream,
        &mut header_cipher,
        "rlpx_frame_header_read_failed",
    )?;
    gateway_eth_rlpx_read_exact_with_partial(
        stream,
        &mut header_mac,
        "rlpx_frame_header_mac_read_failed",
    )?;
    let expected_header_mac = session.ingress_mac.compute_header(&header_cipher);
    if expected_header_mac != header_mac {
        return Err(format!(
            "rlpx_frame_header_mac_mismatch expected=0x{} got=0x{} header_cipher=0x{} ingress_before=0x{} ingress_after_header=0x{}",
            to_hex(&expected_header_mac),
            to_hex(&header_mac),
            to_hex(&header_cipher),
            to_hex(&ingress_mac_before),
            to_hex(&gateway_eth_rlpx_mac_digest_head(&session.ingress_mac)),
        ));
    }
    let ingress_mac_after_header = gateway_eth_rlpx_mac_digest_head(&session.ingress_mac);

    let mut header_plain = header_cipher;
    session
        .dec
        .apply_keystream(&mut header_plain[..GATEWAY_ETH_PLUGIN_RLPX_FRAME_HEADER_LEN]);
    let frame_size = ((header_plain[0] as usize) << 16)
        | ((header_plain[1] as usize) << 8)
        | (header_plain[2] as usize);
    if frame_size == 0 || frame_size > GATEWAY_ETH_PLUGIN_RLPX_FRAME_MAX_SIZE {
        return Err(format!("rlpx_frame_size_invalid:{frame_size}"));
    }
    let padded_size = gateway_eth_rlpx_round_up_16(frame_size);

    let mut frame_cipher = vec![0u8; padded_size];
    let mut frame_mac = [0u8; GATEWAY_ETH_PLUGIN_RLPX_FRAME_MAC_LEN];
    gateway_eth_rlpx_read_exact_with_partial(
        stream,
        frame_cipher.as_mut_slice(),
        "rlpx_frame_body_read_failed",
    )?;
    gateway_eth_rlpx_read_exact_with_partial(stream, &mut frame_mac, "rlpx_frame_mac_read_failed")?;
    let expected_frame_mac = session.ingress_mac.compute_frame(frame_cipher.as_slice());
    if expected_frame_mac != frame_mac {
        return Err(format!(
            "rlpx_frame_mac_mismatch expected=0x{} got=0x{} frame_cipher_head=0x{} ingress_before=0x{} ingress_after_header=0x{} ingress_after_frame=0x{}",
            to_hex(&expected_frame_mac),
            to_hex(&frame_mac),
            gateway_eth_rlpx_hex_preview(frame_cipher.as_slice(), 32),
            to_hex(&ingress_mac_before),
            to_hex(&ingress_mac_after_header),
            to_hex(&gateway_eth_rlpx_mac_digest_head(&session.ingress_mac)),
        ));
    }
    let ingress_mac_after_frame = gateway_eth_rlpx_mac_digest_head(&session.ingress_mac);
    let frame_cipher_head = gateway_eth_rlpx_hex_preview(frame_cipher.as_slice(), 32);

    session.dec.apply_keystream(frame_cipher.as_mut_slice());
    frame_cipher.truncate(frame_size);
    let frame_plain_head = gateway_eth_rlpx_hex_preview(frame_cipher.as_slice(), 32);
    let (code_item, consumed) = gateway_eth_rlpx_parse_item(frame_cipher.as_slice())?;
    let GatewayEthRlpxRlpItem::Bytes(code_bytes) = code_item else {
        return Err("rlpx_msg_code_not_bytes".to_string());
    };
    let code = gateway_eth_rlpx_decode_u64_bytes(code_bytes)?;
    let payload_encoded_len = frame_cipher.len().saturating_sub(consumed);
    let mut payload = frame_cipher[consumed..].to_vec();
    if session.snappy && !payload.is_empty() {
        payload = snap::raw::Decoder::new()
            .decompress_vec(payload.as_slice())
            .map_err(|e| format!("rlpx_snappy_decode_failed:{e}"))?;
    }
    let payload_len = payload.len();
    let egress_mac_after = gateway_eth_rlpx_mac_digest_head(&session.egress_mac);

    Ok((
        code,
        payload,
        GatewayEthRlpxWireFrameRead {
            code,
            payload_len,
            payload_encoded_len,
            frame_size,
            padded_size,
            code_rlp_len: consumed,
            header_plain,
            header_cipher,
            header_mac,
            frame_plain_head,
            frame_cipher_head,
            frame_mac,
            ingress_mac_before,
            ingress_mac_after_header,
            ingress_mac_after_frame,
            egress_mac_before,
            egress_mac_after,
        },
    ))
}

fn gateway_eth_rlpx_read_wire_frame<R: std::io::Read>(
    stream: &mut R,
    session: &mut GatewayEthRlpxFrameSession,
) -> Result<(u64, Vec<u8>), String> {
    gateway_eth_rlpx_read_wire_frame_with_trace(stream, session)
        .map(|(code, payload, _)| (code, payload))
}

struct GatewayEthRlpxWireFrameBuild {
    code: u64,
    code_rlp: Vec<u8>,
    payload_encoded: Vec<u8>,
    frame_size: usize,
    padded_size: usize,
    header_plain: [u8; GATEWAY_ETH_PLUGIN_RLPX_FRAME_HEADER_LEN],
    header_cipher: [u8; GATEWAY_ETH_PLUGIN_RLPX_FRAME_HEADER_LEN],
    header_mac: [u8; GATEWAY_ETH_PLUGIN_RLPX_FRAME_HEADER_MAC_LEN],
    body_plain_head: String,
    body_cipher: Vec<u8>,
    frame_mac: [u8; GATEWAY_ETH_PLUGIN_RLPX_FRAME_MAC_LEN],
}

fn gateway_eth_rlpx_geth_put_uint24(value: usize) -> [u8; 3] {
    [
        ((value >> 16) & 0xff) as u8,
        ((value >> 8) & 0xff) as u8,
        (value & 0xff) as u8,
    ]
}

fn gateway_eth_rlpx_geth_append_u64(out: &mut Vec<u8>, value: u64) {
    if value == 0 {
        out.push(0x80);
        return;
    }
    if value < 0x80 {
        out.push(value as u8);
        return;
    }
    let bytes = value.to_be_bytes();
    let first_non_zero = bytes
        .iter()
        .position(|byte| *byte != 0)
        .unwrap_or(bytes.len().saturating_sub(1));
    let payload = &bytes[first_non_zero..];
    out.push(0x80 + payload.len() as u8);
    out.extend_from_slice(payload);
}

fn gateway_eth_rlpx_build_wire_frame_local(
    session: &mut GatewayEthRlpxFrameSession,
    code: u64,
    payload: &[u8],
) -> Result<GatewayEthRlpxWireFrameBuild, String> {
    let payload_encoded = if session.snappy && !payload.is_empty() {
        snap::raw::Encoder::new()
            .compress_vec(payload)
            .map_err(|e| format!("rlpx_snappy_encode_failed:{e}"))?
    } else {
        payload.to_vec()
    };
    let code_rlp = gateway_eth_rlpx_encode_u64(code);
    let mut frame_plain = code_rlp.clone();
    frame_plain.extend_from_slice(payload_encoded.as_slice());
    if frame_plain.is_empty() || frame_plain.len() > GATEWAY_ETH_PLUGIN_RLPX_FRAME_MAX_SIZE {
        return Err(format!(
            "rlpx_frame_plain_len_invalid:{}",
            frame_plain.len()
        ));
    }
    let frame_size = frame_plain.len();
    let mut header_plain = [0u8; GATEWAY_ETH_PLUGIN_RLPX_FRAME_HEADER_LEN];
    header_plain[0] = ((frame_size >> 16) & 0xff) as u8;
    header_plain[1] = ((frame_size >> 8) & 0xff) as u8;
    header_plain[2] = (frame_size & 0xff) as u8;
    header_plain[3..6].copy_from_slice(&GATEWAY_ETH_PLUGIN_RLPX_ZERO_HEADER);
    let mut header_cipher = header_plain;
    session
        .enc
        .apply_keystream(&mut header_cipher[..GATEWAY_ETH_PLUGIN_RLPX_FRAME_HEADER_LEN]);
    let header_mac = session.egress_mac.compute_header(&header_cipher);
    let body_plain_head = gateway_eth_rlpx_hex_preview(frame_plain.as_slice(), 32);
    let padded_size = gateway_eth_rlpx_round_up_16(frame_plain.len());
    frame_plain.resize(padded_size, 0u8);
    session.enc.apply_keystream(frame_plain.as_mut_slice());
    let frame_mac = session.egress_mac.compute_frame(frame_plain.as_slice());
    Ok(GatewayEthRlpxWireFrameBuild {
        code,
        code_rlp,
        payload_encoded,
        frame_size,
        padded_size,
        header_plain,
        header_cipher,
        header_mac,
        body_plain_head,
        body_cipher: frame_plain,
        frame_mac,
    })
}

fn gateway_eth_rlpx_build_wire_frame_geth_ref(
    session: &mut GatewayEthRlpxFrameSession,
    code: u64,
    payload: &[u8],
) -> Result<GatewayEthRlpxWireFrameBuild, String> {
    let payload_encoded = if session.snappy && !payload.is_empty() {
        snap::raw::Encoder::new()
            .compress_vec(payload)
            .map_err(|e| format!("rlpx_snappy_encode_failed:{e}"))?
    } else {
        payload.to_vec()
    };
    let mut code_rlp = Vec::<u8>::new();
    gateway_eth_rlpx_geth_append_u64(&mut code_rlp, code);
    let frame_size = code_rlp.len().saturating_add(payload_encoded.len());
    if frame_size == 0 || frame_size > GATEWAY_ETH_PLUGIN_RLPX_FRAME_MAX_SIZE {
        return Err(format!("rlpx_frame_plain_len_invalid:{frame_size}"));
    }
    let mut header_plain = [0u8; GATEWAY_ETH_PLUGIN_RLPX_FRAME_HEADER_LEN];
    header_plain[0..3].copy_from_slice(gateway_eth_rlpx_geth_put_uint24(frame_size).as_slice());
    header_plain[3..6].copy_from_slice(&GATEWAY_ETH_PLUGIN_RLPX_ZERO_HEADER);
    let mut header_cipher = header_plain;
    session
        .enc
        .apply_keystream(&mut header_cipher[..GATEWAY_ETH_PLUGIN_RLPX_FRAME_HEADER_LEN]);
    let header_mac = session.egress_mac.compute_header(&header_cipher);

    let mut frame_body = Vec::<u8>::with_capacity(gateway_eth_rlpx_round_up_16(frame_size));
    gateway_eth_rlpx_geth_append_u64(&mut frame_body, code);
    frame_body.extend_from_slice(payload_encoded.as_slice());
    if !frame_body.len().is_multiple_of(16) {
        frame_body.resize(gateway_eth_rlpx_round_up_16(frame_body.len()), 0u8);
    }
    let body_plain_head = gateway_eth_rlpx_hex_preview(frame_body.as_slice(), 32);
    session.enc.apply_keystream(frame_body.as_mut_slice());
    let frame_mac = session.egress_mac.compute_frame(frame_body.as_slice());
    Ok(GatewayEthRlpxWireFrameBuild {
        code,
        code_rlp,
        payload_encoded,
        frame_size,
        padded_size: frame_body.len(),
        header_plain,
        header_cipher,
        header_mac,
        body_plain_head,
        body_cipher: frame_body,
        frame_mac,
    })
}

fn gateway_eth_rlpx_write_wire_frame<W: std::io::Write>(
    stream: &mut W,
    session: &mut GatewayEthRlpxFrameSession,
    code: u64,
    payload: &[u8],
    trace_endpoint: Option<&str>,
) -> Result<(), String> {
    let hello_ab = if gateway_warn_enabled() && code == GATEWAY_ETH_PLUGIN_RLPX_P2P_HELLO_MSG {
        let mut local_probe = session.clone();
        let local = gateway_eth_rlpx_build_wire_frame_local(&mut local_probe, code, payload)?;
        let mut geth_probe = session.clone();
        let geth = gateway_eth_rlpx_build_wire_frame_geth_ref(&mut geth_probe, code, payload)?;
        Some((local, geth))
    } else {
        None
    };
    let wire = gateway_eth_rlpx_build_wire_frame_local(session, code, payload)?;
    if gateway_warn_enabled() && code == GATEWAY_ETH_PLUGIN_RLPX_P2P_HELLO_MSG {
        eprintln!(
            "gateway_warn: rlpx frame_send endpoint={} code=0x{:x} payload_len={} payload_enc_len={} frame_size={} padded={} code_rlp=0x{} payload_head=0x{} payload_enc_head=0x{} header_plain=0x{} header_cipher=0x{} header_mac=0x{} body_plain_head=0x{} body_cipher_head=0x{} frame_mac=0x{}",
            trace_endpoint.unwrap_or("<unknown>"),
            code,
            payload.len(),
            wire.payload_encoded.len(),
            wire.frame_size,
            wire.padded_size,
            to_hex(wire.code_rlp.as_slice()),
            gateway_eth_rlpx_hex_preview(payload, 24),
            gateway_eth_rlpx_hex_preview(wire.payload_encoded.as_slice(), 24),
            gateway_eth_rlpx_hex_preview(&wire.header_plain, 16),
            gateway_eth_rlpx_hex_preview(&wire.header_cipher, 16),
            to_hex(&wire.header_mac),
            wire.body_plain_head.as_str(),
            gateway_eth_rlpx_hex_preview(wire.body_cipher.as_slice(), 24),
            to_hex(&wire.frame_mac),
        );
        eprintln!(
            "gateway_warn: rlpx hello_frame_segments endpoint={} seg1_code=0x{:x} seg2_payload_plain=0x{} seg2_payload_len={} seg3_header_plain=0x{} seg4_header_cipher=0x{} seg4_header_mac=0x{} seg5_body_cipher_head=0x{} seg6_frame_mac=0x{}",
            trace_endpoint.unwrap_or("<unknown>"),
            wire.code,
            gateway_eth_rlpx_hex_preview(payload, 48),
            payload.len(),
            gateway_eth_rlpx_hex_preview(&wire.header_plain, 16),
            gateway_eth_rlpx_hex_preview(&wire.header_cipher, 16),
            to_hex(&wire.header_mac),
            gateway_eth_rlpx_hex_preview(wire.body_cipher.as_slice(), 48),
            to_hex(&wire.frame_mac),
        );
        if let Some((local, geth)) = hello_ab {
            let body_head_len = local.body_cipher.len().min(geth.body_cipher.len()).min(48);
            let seg1_code_eq = local.code == geth.code;
            let seg2_payload_eq = local.payload_encoded == geth.payload_encoded;
            let seg3_header_plain_eq = local.header_plain == geth.header_plain;
            let seg4_header_cipher_eq = local.header_cipher == geth.header_cipher;
            let seg4_header_mac_eq = local.header_mac == geth.header_mac;
            let seg5_body_cipher_head_eq =
                local.body_cipher[..body_head_len] == geth.body_cipher[..body_head_len];
            let seg6_frame_mac_eq = local.frame_mac == geth.frame_mac;
            eprintln!(
                "gateway_warn: rlpx hello_frame_ab endpoint={} seg1_code_eq={} seg2_payload_eq={} seg3_header_plain_eq={} seg4_header_cipher_eq={} seg4_header_mac_eq={} seg5_body_cipher_head_eq={} seg6_frame_mac_eq={} body_head_len={}",
                trace_endpoint.unwrap_or("<unknown>"),
                seg1_code_eq,
                seg2_payload_eq,
                seg3_header_plain_eq,
                seg4_header_cipher_eq,
                seg4_header_mac_eq,
                seg5_body_cipher_head_eq,
                seg6_frame_mac_eq,
                body_head_len,
            );
            if !(seg1_code_eq
                && seg2_payload_eq
                && seg3_header_plain_eq
                && seg4_header_cipher_eq
                && seg4_header_mac_eq
                && seg5_body_cipher_head_eq
                && seg6_frame_mac_eq)
            {
                eprintln!(
                    "gateway_warn: rlpx hello_frame_ab_diff endpoint={} local_header_plain=0x{} geth_header_plain=0x{} local_header_cipher=0x{} geth_header_cipher=0x{} local_header_mac=0x{} geth_header_mac=0x{} local_body_head=0x{} geth_body_head=0x{} local_frame_mac=0x{} geth_frame_mac=0x{}",
                    trace_endpoint.unwrap_or("<unknown>"),
                    gateway_eth_rlpx_hex_preview(&local.header_plain, 16),
                    gateway_eth_rlpx_hex_preview(&geth.header_plain, 16),
                    gateway_eth_rlpx_hex_preview(&local.header_cipher, 16),
                    gateway_eth_rlpx_hex_preview(&geth.header_cipher, 16),
                    to_hex(&local.header_mac),
                    to_hex(&geth.header_mac),
                    gateway_eth_rlpx_hex_preview(local.body_cipher.as_slice(), body_head_len),
                    gateway_eth_rlpx_hex_preview(geth.body_cipher.as_slice(), body_head_len),
                    to_hex(&local.frame_mac),
                    to_hex(&geth.frame_mac),
                );
            }
        }
    }

    stream
        .write_all(&wire.header_cipher)
        .map_err(|e| format!("rlpx_frame_header_write_failed:{e}"))?;
    stream
        .write_all(&wire.header_mac)
        .map_err(|e| format!("rlpx_frame_header_mac_write_failed:{e}"))?;
    stream
        .write_all(wire.body_cipher.as_slice())
        .map_err(|e| format!("rlpx_frame_body_write_failed:{e}"))?;
    stream
        .write_all(&wire.frame_mac)
        .map_err(|e| format!("rlpx_frame_mac_write_failed:{e}"))?;
    Ok(())
}

fn gateway_eth_plugin_peer_session_rlpx_handshake_initiator(
    endpoint: &str,
    stream: &mut std::net::TcpStream,
) -> Result<GatewayEthRlpxHandshakeInitiatorOutcome, String> {
    use rand::RngCore;
    use std::io::Write;

    let remote_pub = gateway_eth_rlpx_parse_enode_pubkey(endpoint)?;
    let static_nodekey = gateway_eth_rlpx_local_static_nodekey_bytes();
    let static_secret = K256SecretKey::from_slice(static_nodekey.as_slice())
        .map_err(|e| format!("rlpx_static_secret_invalid:{e}"))?;
    let static_signing = SigningKey::from_bytes((&static_nodekey).into())
        .map_err(|e| format!("rlpx_static_signing_key_invalid:{e}"))?;
    let ephemeral_signing = SigningKey::random(&mut OsRng);
    let ephemeral_secret = K256SecretKey::from_slice(ephemeral_signing.to_bytes().as_slice())
        .map_err(|e| format!("rlpx_ephemeral_secret_invalid:{e}"))?;

    let mut init_nonce = [0u8; GATEWAY_ETH_PLUGIN_RLPX_NONCE_LEN];
    OsRng.fill_bytes(&mut init_nonce);
    let token = gateway_eth_rlpx_ecdh_shared(&static_secret, &remote_pub);
    let mut sign_msg = [0u8; GATEWAY_ETH_PLUGIN_RLPX_NONCE_LEN];
    for (slot, (a, b)) in sign_msg.iter_mut().zip(token.iter().zip(init_nonce.iter())) {
        *slot = *a ^ *b;
    }
    let (signature, recovery_id) = ephemeral_signing
        .sign_prehash_recoverable(sign_msg.as_slice())
        .map_err(|e| format!("rlpx_auth_sign_failed:{e}"))?;
    let mut sig65 = [0u8; GATEWAY_ETH_PLUGIN_RLPX_SIG_LEN];
    sig65[..64].copy_from_slice(signature.to_bytes().as_slice());
    sig65[64] = recovery_id.to_byte();
    let static_pub = gateway_eth_rlpx_pubkey_64_from_signing_key(&static_signing);

    let mut auth_plain = gateway_eth_rlpx_encode_list(&[
        gateway_eth_rlpx_encode_bytes(sig65.as_slice()),
        gateway_eth_rlpx_encode_bytes(static_pub.as_slice()),
        gateway_eth_rlpx_encode_bytes(init_nonce.as_slice()),
        gateway_eth_rlpx_encode_u64(4),
    ]);
    let pad_len = 100 + ((OsRng.next_u32() % 100) as usize);
    // Match go-ethereum EIP-8 behavior: random padding length, zero-filled bytes.
    auth_plain.extend(std::iter::repeat_n(0u8, pad_len));

    let packet_len = auth_plain
        .len()
        .saturating_add(GATEWAY_ETH_PLUGIN_RLPX_ECIES_OVERHEAD);
    if packet_len > u16::MAX as usize {
        return Err("rlpx_auth_packet_too_large".to_string());
    }
    let prefix = (packet_len as u16).to_be_bytes();
    let auth_encrypted =
        gateway_eth_rlpx_ecies_encrypt(&remote_pub, auth_plain.as_slice(), &prefix)?;
    let mut auth_packet = Vec::with_capacity(2 + auth_encrypted.len());
    auth_packet.extend_from_slice(prefix.as_slice());
    auth_packet.extend_from_slice(auth_encrypted.as_slice());
    stream
        .write_all(prefix.as_slice())
        .map_err(|e| format!("rlpx_auth_prefix_send_failed:{e}"))?;
    stream
        .write_all(auth_encrypted.as_slice())
        .map_err(|e| format!("rlpx_auth_send_failed:{e}"))?;
    if gateway_warn_enabled() {
        eprintln!(
            "gateway_warn: rlpx stage auth_sent endpoint={} auth_prefix=0x{} auth_bytes={} auth_head=0x{}",
            endpoint,
            to_hex(prefix.as_slice()),
            auth_encrypted.len(),
            gateway_eth_rlpx_hex_preview(auth_encrypted.as_slice(), 16)
        );
    }

    let mut ack_prefix = [0u8; 2];
    gateway_eth_rlpx_read_exact_with_partial(
        stream,
        &mut ack_prefix,
        "rlpx_ack_prefix_read_failed",
    )?;
    let ack_size = u16::from_be_bytes(ack_prefix) as usize;
    if ack_size == 0 || ack_size > GATEWAY_ETH_PLUGIN_RLPX_HANDSHAKE_MAX_BYTES {
        return Err(format!("rlpx_ack_size_invalid:{ack_size}"));
    }
    let mut ack_cipher = vec![0u8; ack_size];
    gateway_eth_rlpx_read_exact_with_partial(
        stream,
        ack_cipher.as_mut_slice(),
        "rlpx_ack_read_failed",
    )?;
    let mut ack_packet = Vec::with_capacity(2 + ack_cipher.len());
    ack_packet.extend_from_slice(&ack_prefix);
    ack_packet.extend_from_slice(ack_cipher.as_slice());
    let ack_plain =
        gateway_eth_rlpx_ecies_decrypt(&static_secret, ack_cipher.as_slice(), &ack_prefix)?;
    let (remote_random_pub, resp_nonce, _version) =
        gateway_eth_rlpx_decode_auth_resp_v4(ack_plain.as_slice())?;
    if gateway_warn_enabled() {
        eprintln!(
            "gateway_warn: rlpx stage ack_payload endpoint={} ack_prefix=0x{} ack_bytes={} ack_head=0x{} ack_plain_head=0x{} resp_nonce=0x{} remote_random_pub_head=0x{}",
            endpoint,
            to_hex(&ack_prefix),
            ack_cipher.len(),
            gateway_eth_rlpx_hex_preview(ack_cipher.as_slice(), 16),
            gateway_eth_rlpx_hex_preview(ack_plain.as_slice(), 24),
            to_hex(resp_nonce.as_slice()),
            gateway_eth_rlpx_hex_preview(remote_random_pub.as_slice(), 16),
        );
    }

    let mut remote_random_pub_sec1 = [0u8; 65];
    remote_random_pub_sec1[0] = 0x04;
    remote_random_pub_sec1[1..].copy_from_slice(remote_random_pub.as_slice());
    let remote_random = K256PublicKey::from_sec1_bytes(&remote_random_pub_sec1)
        .map_err(|e| format!("rlpx_ack_remote_pub_invalid:{e}"))?;
    let ecdhe_secret = gateway_eth_rlpx_ecdh_shared(&ephemeral_secret, &remote_random);
    let nonce_mix = gateway_eth_rlpx_keccak256(&[resp_nonce.as_slice(), init_nonce.as_slice()]);
    let shared_secret =
        gateway_eth_rlpx_keccak256(&[ecdhe_secret.as_slice(), nonce_mix.as_slice()]);
    let aes_secret =
        gateway_eth_rlpx_keccak256(&[ecdhe_secret.as_slice(), shared_secret.as_slice()]);
    let mac_secret = gateway_eth_rlpx_keccak256(&[ecdhe_secret.as_slice(), aes_secret.as_slice()]);
    if gateway_warn_enabled() {
        eprintln!(
            "gateway_warn: rlpx stage secrets endpoint={} aes_head=0x{} mac_head=0x{} ecdhe_head=0x{}",
            endpoint,
            gateway_eth_rlpx_hex_preview(aes_secret.as_slice(), 16),
            gateway_eth_rlpx_hex_preview(mac_secret.as_slice(), 16),
            gateway_eth_rlpx_hex_preview(ecdhe_secret.as_slice(), 16),
        );
    }

    let egress_prefix = gateway_eth_rlpx_xor_32(&mac_secret, &resp_nonce);
    let ingress_prefix = gateway_eth_rlpx_xor_32(&mac_secret, &init_nonce);
    let mut egress_init = Vec::with_capacity(32 + auth_packet.len());
    egress_init.extend_from_slice(egress_prefix.as_slice());
    egress_init.extend_from_slice(auth_packet.as_slice());
    let mut ingress_init = Vec::with_capacity(32 + ack_packet.len());
    ingress_init.extend_from_slice(ingress_prefix.as_slice());
    ingress_init.extend_from_slice(ack_packet.as_slice());
    let egress_mac = GatewayEthRlpxHashMac::new(&mac_secret, egress_init.as_slice())?;
    let ingress_mac = GatewayEthRlpxHashMac::new(&mac_secret, ingress_init.as_slice())?;
    let iv = [0u8; 16];
    let enc = GatewayEthRlpxAes256Ctr::new((&aes_secret).into(), (&iv).into());
    let dec = GatewayEthRlpxAes256Ctr::new((&aes_secret).into(), (&iv).into());
    let session = GatewayEthRlpxFrameSession {
        enc,
        dec,
        egress_mac,
        ingress_mac,
        snappy: false,
    };
    Ok(GatewayEthRlpxHandshakeInitiatorOutcome {
        session,
        local_static_pub: static_pub,
    })
}

fn gateway_eth_rlpx_next_request_id() -> u64 {
    let next = GATEWAY_ETH_PLUGIN_RLPX_REQUEST_ID
        .fetch_add(1, std::sync::atomic::Ordering::Relaxed)
        .saturating_add(1);
    next.max(1)
}

fn gateway_eth_rlpx_ingest_raw_txs(chain_id: u64, raw_txs: &[Vec<u8>]) -> usize {
    let mut imported = 0usize;
    for raw_tx in raw_txs {
        if raw_tx.is_empty() {
            continue;
        }
        let tx_hash_hint = gateway_eth_rlpx_keccak256(&[raw_tx.as_slice()]);
        gateway_eth_native_ingest_transactions_payload(chain_id, tx_hash_hint, 1, raw_tx);
        imported = imported.saturating_add(1);
    }
    imported
}

fn gateway_eth_plugin_peer_session_rlpx_ingest(
    chain_id: u64,
    endpoint: &str,
    node_hint: u64,
    addr_hint: &str,
    timeout_ms: u64,
    read_window_ms: u64,
    max_hashes_per_request: usize,
) -> Result<GatewayEthPluginRlpxSessionMetrics, String> {
    use std::time::{Duration, Instant};

    let timeout = Duration::from_millis(timeout_ms.max(1));
    let checked_ms = now_unix_millis() as u64;
    let mut stream = connect_gateway_eth_plugin_peer(addr_hint, timeout_ms)?;
    stream
        .set_read_timeout(Some(timeout))
        .map_err(|e| format!("rlpx_set_read_timeout_failed:{e}"))?;
    stream
        .set_write_timeout(Some(timeout))
        .map_err(|e| format!("rlpx_set_write_timeout_failed:{e}"))?;

    bump_gateway_eth_plugin_session_stage(
        chain_id,
        endpoint,
        GatewayEthPluginSessionStage::TcpConnected,
        checked_ms,
    );
    observe_gateway_eth_plugin_session_stage(chain_id, GatewayEthPluginSessionStage::TcpConnected);
    if gateway_warn_enabled() {
        eprintln!(
            "gateway_warn: rlpx stage tcp_connected endpoint={} addr={}",
            endpoint, addr_hint
        );
    }

    let mut handshake =
        gateway_eth_plugin_peer_session_rlpx_handshake_initiator(endpoint, &mut stream)?;
    observe_eth_native_rlpx_auth(chain_id);
    observe_eth_native_rlpx_auth_ack(chain_id);
    bump_gateway_eth_plugin_session_stage(
        chain_id,
        endpoint,
        GatewayEthPluginSessionStage::AckSeen,
        checked_ms,
    );
    observe_gateway_eth_plugin_session_stage(chain_id, GatewayEthPluginSessionStage::AckSeen);
    if gateway_warn_enabled() {
        eprintln!(
            "gateway_warn: rlpx stage ack_received endpoint={}",
            endpoint
        );
    }

    let hello_profile = gateway_eth_rlpx_hello_profile(chain_id);
    let local_caps = if hello_profile == "geth" {
        // Keep geth-like ordering while preserving public-mainnet compatibility.
        vec![
            GatewayEthRlpxCapability {
                name: "eth".to_string(),
                version: GATEWAY_ETH_PLUGIN_RLPX_ETH_PROTO_68,
            },
            GatewayEthRlpxCapability {
                name: "eth".to_string(),
                version: GATEWAY_ETH_PLUGIN_RLPX_ETH_PROTO_69,
            },
        ]
    } else {
        vec![
            GatewayEthRlpxCapability {
                name: "eth".to_string(),
                version: GATEWAY_ETH_PLUGIN_RLPX_ETH_PROTO_66,
            },
            GatewayEthRlpxCapability {
                name: "eth".to_string(),
                version: GATEWAY_ETH_PLUGIN_RLPX_ETH_PROTO_67,
            },
            GatewayEthRlpxCapability {
                name: "eth".to_string(),
                version: GATEWAY_ETH_PLUGIN_RLPX_ETH_PROTO_68,
            },
            GatewayEthRlpxCapability {
                name: "eth".to_string(),
                version: GATEWAY_ETH_PLUGIN_RLPX_ETH_PROTO_69,
            },
        ]
    };
    let hello_name = gateway_eth_rlpx_hello_name(chain_id, hello_profile.as_str());
    let hello_listen_port = gateway_eth_rlpx_hello_listen_port(chain_id, hello_profile.as_str());
    let hello_payload = gateway_eth_rlpx_build_hello_payload(
        &handshake.local_static_pub,
        local_caps.as_slice(),
        hello_name.as_str(),
        hello_listen_port,
    );
    let local_hello = gateway_eth_rlpx_parse_hello_payload(hello_payload.as_slice())?;
    gateway_eth_rlpx_write_wire_frame(
        &mut stream,
        &mut handshake.session,
        GATEWAY_ETH_PLUGIN_RLPX_P2P_HELLO_MSG,
        hello_payload.as_slice(),
        Some(endpoint),
    )?;
    if gateway_warn_enabled() {
        eprintln!(
            "gateway_warn: rlpx stage hello_sent endpoint={} hello_profile={} p2p_version={} name={} caps={} caps_geth_canonical={} listen_port={} id_len={} geth_ref_p2p_version=5 geth_ref_caps_order=name+version_asc",
            endpoint,
            hello_profile,
            local_hello.protocol_version,
            local_hello.client_name,
            local_hello
                .capabilities
                .iter()
                .map(|cap| format!("{}/{}", cap.name, cap.version))
                .collect::<Vec<_>>()
                .join(","),
            gateway_eth_rlpx_caps_are_geth_canonical(local_hello.capabilities.as_slice()),
            local_hello.listen_port,
            local_hello.id_len,
        );
    }

    let hello_deadline = Instant::now() + timeout;
    let remote_hello = loop {
        if Instant::now() >= hello_deadline {
            break Err("rlpx_remote_hello_timeout".to_string());
        }
        match gateway_eth_rlpx_read_wire_frame(&mut stream, &mut handshake.session) {
            Ok((code, payload)) => {
                if code == GATEWAY_ETH_PLUGIN_RLPX_P2P_HELLO_MSG {
                    break gateway_eth_rlpx_parse_hello_payload(payload.as_slice());
                }
                if code == GATEWAY_ETH_PLUGIN_RLPX_P2P_PING_MSG {
                    let _ = gateway_eth_rlpx_write_wire_frame(
                        &mut stream,
                        &mut handshake.session,
                        GATEWAY_ETH_PLUGIN_RLPX_P2P_PONG_MSG,
                        &[],
                        Some(endpoint),
                    );
                }
                if code == GATEWAY_ETH_PLUGIN_RLPX_P2P_DISCONNECT_MSG {
                    let reason = gateway_eth_rlpx_parse_disconnect_reason(payload.as_slice());
                    if gateway_warn_enabled() {
                        eprintln!(
                            "gateway_warn: rlpx stage disconnect_received endpoint={} phase=before_hello reason_code={} reason={} payload=0x{}",
                            endpoint,
                            reason.unwrap_or(u64::MAX),
                            gateway_eth_rlpx_disconnect_reason_name(
                                reason.unwrap_or(u64::MAX)
                            ),
                            gateway_eth_rlpx_hex_preview(payload.as_slice(), 24),
                        );
                    }
                    break Err(format!(
                        "rlpx_remote_disconnected_before_hello:reason_code={} reason={}",
                        reason.unwrap_or(u64::MAX),
                        gateway_eth_rlpx_disconnect_reason_name(reason.unwrap_or(u64::MAX)),
                    ));
                }
                if gateway_warn_enabled() {
                    eprintln!(
                        "gateway_warn: rlpx stage hello_wait_non_hello endpoint={} code=0x{:x} payload_len={} payload_head=0x{}",
                        endpoint,
                        code,
                        payload.len(),
                        gateway_eth_rlpx_hex_preview(payload.as_slice(), 16),
                    );
                }
            }
            Err(err) => {
                if gateway_eth_rlpx_is_timeout_like(err.as_str()) {
                    continue;
                }
                if gateway_warn_enabled() {
                    eprintln!(
                        "gateway_warn: rlpx stage hello_wait_read_failed endpoint={} err={}",
                        endpoint, err
                    );
                }
                break Err(err);
            }
        }
    }?;

    let Some(eth_version) = gateway_eth_rlpx_select_shared_eth_version(
        local_hello.capabilities.as_slice(),
        remote_hello.capabilities.as_slice(),
    ) else {
        if gateway_warn_enabled() {
            eprintln!(
                "gateway_warn: rlpx stage no_shared_eth_capability endpoint={} local_caps={} remote_caps={}",
                endpoint,
                local_hello
                    .capabilities
                    .iter()
                    .map(|cap| format!("{}/{}", cap.name, cap.version))
                    .collect::<Vec<_>>()
                    .join(","),
                remote_hello
                    .capabilities
                    .iter()
                    .map(|cap| format!("{}/{}", cap.name, cap.version))
                    .collect::<Vec<_>>()
                    .join(","),
            );
        }
        return Err("rlpx_eth_capability_not_found".to_string());
    };
    if gateway_warn_enabled() {
        eprintln!(
            "gateway_warn: rlpx stage hello_received endpoint={} remote_proto={} remote_name={} remote_caps={} remote_caps_geth_canonical={} remote_listen_port={} remote_id_len={}",
            endpoint,
            remote_hello.protocol_version,
            remote_hello.client_name,
            remote_hello
                .capabilities
                .iter()
                .map(|cap| format!("{}/{}", cap.name, cap.version))
                .collect::<Vec<_>>()
                .join(","),
            gateway_eth_rlpx_caps_are_geth_canonical(remote_hello.capabilities.as_slice()),
            remote_hello.listen_port,
            remote_hello.id_len,
        );
    }
    if remote_hello.protocol_version >= 5 {
        handshake.session.snappy = true;
    }
    let eth_offset = GATEWAY_ETH_PLUGIN_RLPX_BASE_PROTOCOL_OFFSET;
    let eth_status_code = eth_offset + GATEWAY_ETH_PLUGIN_RLPX_ETH_STATUS_MSG;

    let status_deadline = Instant::now() + timeout;
    let remote_status = loop {
        if Instant::now() >= status_deadline {
            break Err("rlpx_eth_status_timeout".to_string());
        }
        match gateway_eth_rlpx_read_wire_frame(&mut stream, &mut handshake.session) {
            Ok((code, payload)) => {
                if code == eth_status_code {
                    break Ok(payload);
                }
                if code == GATEWAY_ETH_PLUGIN_RLPX_P2P_PING_MSG {
                    let _ = gateway_eth_rlpx_write_wire_frame(
                        &mut stream,
                        &mut handshake.session,
                        GATEWAY_ETH_PLUGIN_RLPX_P2P_PONG_MSG,
                        &[],
                        Some(endpoint),
                    );
                }
                if code == GATEWAY_ETH_PLUGIN_RLPX_P2P_DISCONNECT_MSG {
                    let reason = gateway_eth_rlpx_parse_disconnect_reason(payload.as_slice());
                    if gateway_warn_enabled() {
                        eprintln!(
                            "gateway_warn: rlpx stage disconnect_received endpoint={} phase=before_eth_status reason_code={} reason={} payload=0x{}",
                            endpoint,
                            reason.unwrap_or(u64::MAX),
                            gateway_eth_rlpx_disconnect_reason_name(
                                reason.unwrap_or(u64::MAX)
                            ),
                            gateway_eth_rlpx_hex_preview(payload.as_slice(), 24),
                        );
                    }
                    break Err(format!(
                        "rlpx_remote_disconnected_before_eth_status:reason_code={} reason={}",
                        reason.unwrap_or(u64::MAX),
                        gateway_eth_rlpx_disconnect_reason_name(reason.unwrap_or(u64::MAX)),
                    ));
                }
                if gateway_warn_enabled() {
                    eprintln!(
                        "gateway_warn: rlpx stage status_wait_non_status endpoint={} code=0x{:x} payload_len={} payload_head=0x{}",
                        endpoint,
                        code,
                        payload.len(),
                        gateway_eth_rlpx_hex_preview(payload.as_slice(), 16),
                    );
                }
            }
            Err(err) => {
                if gateway_eth_rlpx_is_timeout_like(err.as_str()) {
                    continue;
                }
                if gateway_warn_enabled() {
                    eprintln!(
                        "gateway_warn: rlpx stage status_wait_read_failed endpoint={} err={}",
                        endpoint, err
                    );
                }
                break Err(err);
            }
        }
    }?;
    let status_chain_id =
        gateway_eth_rlpx_parse_eth_status_chain_id(remote_status.as_slice()).unwrap_or(chain_id);
    if gateway_warn_enabled() {
        eprintln!(
            "gateway_warn: rlpx stage status_received endpoint={} remote_chain_id={} negotiated_eth={}",
            endpoint, status_chain_id, eth_version
        );
    }
    gateway_eth_rlpx_write_wire_frame(
        &mut stream,
        &mut handshake.session,
        eth_status_code,
        remote_status.as_slice(),
        Some(endpoint),
    )?;
    if gateway_warn_enabled() {
        eprintln!(
            "gateway_warn: rlpx stage status_sent endpoint={} local_chain_id={}",
            endpoint, chain_id
        );
    }
    observe_eth_native_hello(chain_id);
    observe_eth_native_status(chain_id);

    let _ = novovm_network::register_network_runtime_peer(chain_id, node_hint.max(1));
    let _ = upsert_network_runtime_eth_peer_session(
        chain_id,
        node_hint.max(1),
        &[(eth_version as u8)],
        &[],
        None,
    );
    bump_gateway_eth_plugin_session_stage(
        chain_id,
        endpoint,
        GatewayEthPluginSessionStage::Ready,
        now_unix_millis() as u64,
    );
    observe_gateway_eth_plugin_session_stage(chain_id, GatewayEthPluginSessionStage::Ready);
    if gateway_warn_enabled() {
        eprintln!(
            "gateway_warn: rlpx stage ready endpoint={} node_hint={}",
            endpoint, node_hint
        );
    }

    let eth_new_pooled_code = eth_offset + GATEWAY_ETH_PLUGIN_RLPX_ETH_NEW_POOLED_HASHES_MSG;
    let eth_get_pooled_code = eth_offset + GATEWAY_ETH_PLUGIN_RLPX_ETH_GET_POOLED_MSG;
    let eth_pooled_code = eth_offset + GATEWAY_ETH_PLUGIN_RLPX_ETH_POOLED_MSG;
    let eth_txs_code = eth_offset + GATEWAY_ETH_PLUGIN_RLPX_ETH_TRANSACTIONS_MSG;
    let mut requested_hashes = Vec::<[u8; 32]>::new();
    let mut metrics = GatewayEthPluginRlpxSessionMetrics::default();
    let mut live_applied = GatewayEthPluginRlpxSessionMetrics::default();
    let worker_key = build_gateway_eth_plugin_rlpx_worker_key(chain_id, endpoint);
    let max_hashes = max_hashes_per_request.max(1);
    let max_pooled_txs = max_hashes.saturating_mul(4).clamp(32, 2048);
    let window = Duration::from_millis(read_window_ms.max(1));
    let started = Instant::now();
    let ready_started = Instant::now();
    let mut first_post_ready_logged = false;
    while started.elapsed() < window {
        let remain = window.saturating_sub(started.elapsed());
        let read_timeout = remain
            .min(Duration::from_millis(250))
            .max(Duration::from_millis(20));
        let _ = stream.set_read_timeout(Some(read_timeout));
        match gateway_eth_rlpx_read_wire_frame_with_trace(&mut stream, &mut handshake.session) {
            Ok((code, payload, trace)) => {
                if gateway_warn_enabled() && !first_post_ready_logged {
                    eprintln!(
                        "gateway_warn: rlpx stage first_post_ready_frame endpoint={} code=0x{:x} payload_len={} payload_enc_len={} frame_size={} padded={} code_rlp_len={} header_plain=0x{} header_cipher=0x{} header_mac=0x{} frame_plain_head=0x{} frame_cipher_head=0x{} frame_mac=0x{} ingress_mac_before=0x{} ingress_mac_after_header=0x{} ingress_mac_after_frame=0x{} egress_mac_before=0x{} egress_mac_after=0x{}",
                        endpoint,
                        trace.code,
                        trace.payload_len,
                        trace.payload_encoded_len,
                        trace.frame_size,
                        trace.padded_size,
                        trace.code_rlp_len,
                        to_hex(&trace.header_plain),
                        to_hex(&trace.header_cipher),
                        to_hex(&trace.header_mac),
                        trace.frame_plain_head,
                        trace.frame_cipher_head,
                        to_hex(&trace.frame_mac),
                        to_hex(&trace.ingress_mac_before),
                        to_hex(&trace.ingress_mac_after_header),
                        to_hex(&trace.ingress_mac_after_frame),
                        to_hex(&trace.egress_mac_before),
                        to_hex(&trace.egress_mac_after),
                    );
                    first_post_ready_logged = true;
                }
                if metrics.first_post_ready_code.is_none() {
                    metrics.first_post_ready_code = Some(trace.code);
                }
                match code {
                    GATEWAY_ETH_PLUGIN_RLPX_P2P_PING_MSG => {
                        let _ = gateway_eth_rlpx_write_wire_frame(
                            &mut stream,
                            &mut handshake.session,
                            GATEWAY_ETH_PLUGIN_RLPX_P2P_PONG_MSG,
                            &[],
                            Some(endpoint),
                        );
                    }
                    GATEWAY_ETH_PLUGIN_RLPX_P2P_DISCONNECT_MSG => {
                        let reason = gateway_eth_rlpx_parse_disconnect_reason(payload.as_slice());
                        if gateway_warn_enabled() {
                            eprintln!(
                            "gateway_warn: rlpx stage disconnect_received endpoint={} phase=ingest reason_code={} reason={} payload=0x{}",
                            endpoint,
                            reason.unwrap_or(u64::MAX),
                            gateway_eth_rlpx_disconnect_reason_name(
                                reason.unwrap_or(u64::MAX)
                            ),
                            gateway_eth_rlpx_hex_preview(payload.as_slice(), 24),
                        );
                        }
                        return Err(format!(
                            "rlpx_remote_disconnected:reason_code={} reason={}",
                            reason.unwrap_or(u64::MAX),
                            gateway_eth_rlpx_disconnect_reason_name(reason.unwrap_or(u64::MAX)),
                        ));
                    }
                    _ if code == eth_new_pooled_code => {
                        match gateway_eth_rlpx_parse_hashes_from_new_pooled_payload(
                            payload.as_slice(),
                            max_hashes.saturating_mul(8),
                        ) {
                            Ok(mut hashes) => {
                                if metrics.first_gossip_latency_ms == 0 {
                                    metrics.first_gossip_latency_ms =
                                        ready_started.elapsed().as_millis() as u64;
                                }
                                metrics.new_pooled_msgs = metrics.new_pooled_msgs.saturating_add(1);
                                metrics.new_pooled_hashes = metrics
                                    .new_pooled_hashes
                                    .saturating_add(hashes.len() as u64);
                                let now_ms = now_unix_millis() as u64;
                                for hash in hashes.as_slice() {
                                    if gateway_eth_plugin_rlpx_mark_seen_hash(
                                        chain_id, *hash, now_ms,
                                    ) {
                                        metrics.unique_new_pooled_hashes =
                                            metrics.unique_new_pooled_hashes.saturating_add(1);
                                        metrics.first_seen_hashes =
                                            metrics.first_seen_hashes.saturating_add(1);
                                    } else {
                                        metrics.duplicate_new_pooled_hashes =
                                            metrics.duplicate_new_pooled_hashes.saturating_add(1);
                                    }
                                }
                                if gateway_warn_enabled() {
                                    eprintln!(
                                    "gateway_warn: rlpx stage new_pooled_hashes endpoint={} count={} queued_before={}",
                                    endpoint,
                                    hashes.len(),
                                    requested_hashes.len()
                                );
                                }
                                requested_hashes.append(&mut hashes);
                            }
                            Err(err) => {
                                if gateway_warn_enabled() {
                                    eprintln!(
                                    "gateway_warn: rlpx stage new_pooled_hashes_decode_failed endpoint={} err={}",
                                    endpoint, err
                                );
                                }
                            }
                        }
                    }
                    _ if code == eth_pooled_code => {
                        match gateway_eth_rlpx_extract_raw_txs_from_pooled_payload(
                            payload.as_slice(),
                            max_pooled_txs,
                        ) {
                            Ok(raw_txs) => {
                                if metrics.first_gossip_latency_ms == 0 {
                                    metrics.first_gossip_latency_ms =
                                        ready_started.elapsed().as_millis() as u64;
                                }
                                let now_ms = now_unix_millis() as u64;
                                for raw_tx in raw_txs.as_slice() {
                                    if raw_tx.is_empty() {
                                        continue;
                                    }
                                    let swap_kind = gateway_eth_plugin_detect_swap_kind_from_raw_tx(
                                        status_chain_id,
                                        raw_tx.as_slice(),
                                    );
                                    let tx_hash = gateway_eth_rlpx_keccak256(&[raw_tx.as_slice()]);
                                    let is_unique = gateway_eth_plugin_rlpx_mark_seen_tx(
                                        chain_id, tx_hash, now_ms,
                                    );
                                    if is_unique {
                                        metrics.unique_pooled_txs =
                                            metrics.unique_pooled_txs.saturating_add(1);
                                        metrics.first_seen_txs =
                                            metrics.first_seen_txs.saturating_add(1);
                                    } else {
                                        metrics.duplicate_pooled_txs =
                                            metrics.duplicate_pooled_txs.saturating_add(1);
                                    }
                                    if let Some(kind) = swap_kind {
                                        metrics.swap_hits = metrics.swap_hits.saturating_add(1);
                                        if is_unique {
                                            metrics.unique_swap_hits =
                                                metrics.unique_swap_hits.saturating_add(1);
                                        }
                                        match kind {
                                            GatewayEthSwapKind::V2 => {
                                                metrics.swap_v2_hits =
                                                    metrics.swap_v2_hits.saturating_add(1);
                                            }
                                            GatewayEthSwapKind::V3 => {
                                                metrics.swap_v3_hits =
                                                    metrics.swap_v3_hits.saturating_add(1);
                                            }
                                        }
                                        if metrics.first_swap_latency_ms == 0 {
                                            metrics.first_swap_latency_ms =
                                                ready_started.elapsed().as_millis() as u64;
                                        }
                                    }
                                }
                                let imported = gateway_eth_rlpx_ingest_raw_txs(
                                    status_chain_id,
                                    raw_txs.as_slice(),
                                );
                                metrics.pooled_msgs = metrics.pooled_msgs.saturating_add(1);
                                metrics.pooled_txs_received = metrics
                                    .pooled_txs_received
                                    .saturating_add(raw_txs.len() as u64);
                                metrics.pooled_txs_imported =
                                    metrics.pooled_txs_imported.saturating_add(imported as u64);
                                if gateway_warn_enabled() {
                                    eprintln!(
                                    "gateway_warn: rlpx stage pooled_txs endpoint={} received={} imported={}",
                                    endpoint,
                                    raw_txs.len(),
                                    imported
                                );
                                }
                            }
                            Err(err) => {
                                if gateway_warn_enabled() {
                                    eprintln!(
                                    "gateway_warn: rlpx stage pooled_txs_decode_failed endpoint={} err={}",
                                    endpoint, err
                                );
                                }
                            }
                        }
                    }
                    _ if code == eth_txs_code => {
                        if metrics.first_gossip_latency_ms == 0 {
                            metrics.first_gossip_latency_ms =
                                ready_started.elapsed().as_millis() as u64;
                        }
                        metrics.txs_msgs = metrics.txs_msgs.saturating_add(1);
                        if gateway_warn_enabled() {
                            eprintln!(
                                "gateway_warn: rlpx stage txs_payload endpoint={} bytes={}",
                                endpoint,
                                payload.len()
                            );
                        }
                        gateway_eth_native_ingest_transactions_payload(
                            status_chain_id,
                            [0u8; 32],
                            0,
                            payload.as_slice(),
                        );
                    }
                    _ => {
                        if gateway_warn_enabled() {
                            eprintln!(
                            "gateway_warn: rlpx stage ingest_non_target endpoint={} code=0x{:x} payload_len={} payload_head=0x{}",
                            endpoint,
                            code,
                            payload.len(),
                            gateway_eth_rlpx_hex_preview(payload.as_slice(), 16),
                        );
                        }
                    }
                }
            }
            Err(err) => {
                if !gateway_eth_rlpx_is_timeout_like(err.as_str()) {
                    return Err(err);
                }
            }
        }

        while !requested_hashes.is_empty() {
            let take = requested_hashes.len().min(max_hashes);
            let batch = requested_hashes.drain(..take).collect::<Vec<_>>();
            let request_id = gateway_eth_rlpx_next_request_id();
            let request_payload =
                gateway_eth_rlpx_build_get_pooled_payload(request_id, batch.as_slice());
            metrics.get_pooled_sent = metrics.get_pooled_sent.saturating_add(1);
            if gateway_warn_enabled() {
                eprintln!(
                    "gateway_warn: rlpx stage get_pooled_sent endpoint={} request_id={} count={}",
                    endpoint,
                    request_id,
                    batch.len()
                );
            }
            let _ = gateway_eth_rlpx_write_wire_frame(
                &mut stream,
                &mut handshake.session,
                eth_get_pooled_code,
                request_payload.as_slice(),
                Some(endpoint),
            );
        }
        gateway_eth_plugin_rlpx_apply_live_metrics_delta(
            chain_id,
            worker_key.as_str(),
            &metrics,
            &mut live_applied,
        );
    }
    gateway_eth_plugin_rlpx_apply_live_metrics_delta(
        chain_id,
        worker_key.as_str(),
        &metrics,
        &mut live_applied,
    );
    Ok(metrics)
}

fn build_gateway_eth_plugin_session_probe_payload(
    chain_id: u64,
    endpoint: &str,
    checked_ms: u64,
) -> Vec<u8> {
    let mut payload = format!(
        "NOVOVM_EVM_PLUGIN_PROBE_V1 chain=0x{chain_id:x} ts={checked_ms} endpoint={endpoint}"
    )
    .into_bytes();
    if payload.len() > GATEWAY_ETH_PLUGIN_SESSION_PROBE_PAYLOAD_MAX {
        payload.truncate(GATEWAY_ETH_PLUGIN_SESSION_PROBE_PAYLOAD_MAX);
    }
    payload
}

fn probe_gateway_eth_plugin_peer_session_legacy(
    chain_id: u64,
    endpoint: &str,
    stream: &mut std::net::TcpStream,
    checked_ms: u64,
) -> Result<(), String> {
    use std::io::{Read, Write};
    let payload = build_gateway_eth_plugin_session_probe_payload(chain_id, endpoint, checked_ms);
    stream
        .write_all(payload.as_slice())
        .map_err(|e| format!("session_auth_send_failed:{e}"))?;
    let mut ack = [0_u8; GATEWAY_ETH_PLUGIN_SESSION_PROBE_READ_MAX];
    let read_bytes = stream
        .read(&mut ack)
        .map_err(|e| format!("session_ack_read_failed:{e}"))?;
    if read_bytes == 0 {
        return Err("session_ack_empty".to_string());
    }
    Ok(())
}

fn probe_gateway_eth_plugin_peer_session_rlpx(
    endpoint: &str,
    stream: &mut std::net::TcpStream,
) -> Result<(), String> {
    let _ = gateway_eth_plugin_peer_session_rlpx_handshake_initiator(endpoint, stream)?;
    Ok(())
}

fn probe_gateway_eth_plugin_peer_session(
    chain_id: u64,
    endpoint: &str,
    stream: &mut std::net::TcpStream,
    checked_ms: u64,
) -> Result<(), String> {
    bump_gateway_eth_plugin_session_stage(
        chain_id,
        endpoint,
        GatewayEthPluginSessionStage::AuthSent,
        checked_ms,
    );
    observe_gateway_eth_plugin_session_stage(chain_id, GatewayEthPluginSessionStage::AuthSent);

    if endpoint.trim().to_ascii_lowercase().starts_with("enode://") {
        probe_gateway_eth_plugin_peer_session_rlpx(endpoint, stream)?;
    } else {
        probe_gateway_eth_plugin_peer_session_legacy(chain_id, endpoint, stream, checked_ms)?;
    }
    bump_gateway_eth_plugin_session_stage(
        chain_id,
        endpoint,
        GatewayEthPluginSessionStage::AckSeen,
        checked_ms,
    );
    observe_gateway_eth_plugin_session_stage(chain_id, GatewayEthPluginSessionStage::AckSeen);
    bump_gateway_eth_plugin_session_stage(
        chain_id,
        endpoint,
        GatewayEthPluginSessionStage::Ready,
        checked_ms,
    );
    observe_gateway_eth_plugin_session_stage(chain_id, GatewayEthPluginSessionStage::Ready);
    Ok(())
}

fn probe_gateway_eth_plugin_peers_parallel(
    chain_id: u64,
    peers: &[PluginPeerEndpoint],
    timeout_ms: u64,
    checked_ms: u64,
) -> GatewayEthPluginProbeOutcome {
    if peers.is_empty() {
        return GatewayEthPluginProbeOutcome {
            checked_ms,
            total: 0,
            reachable_count: 0,
            unreachable_count: 0,
            error_preview: Vec::new(),
        };
    }

    let session_probe_mode = gateway_eth_public_broadcast_plugin_session_probe_mode(chain_id);
    let parallelism = gateway_eth_native_parallelism().min(peers.len()).max(1);
    let mut reachable = Vec::<String>::new();
    let mut errors = Vec::<String>::new();
    for batch in peers.chunks(parallelism) {
        std::thread::scope(|scope| {
            let mut jobs = Vec::with_capacity(batch.len());
            for peer in batch {
                let endpoint = peer.endpoint.clone();
                let addr_hint = peer.addr_hint.clone();
                let node_hint = peer.node_hint.max(1);
                let session_probe =
                    gateway_eth_plugin_session_probe_enabled(session_probe_mode, endpoint.as_str());
                jobs.push(scope.spawn(move || {
                    let res = connect_gateway_eth_plugin_peer(addr_hint.as_str(), timeout_ms);
                    (endpoint, node_hint, session_probe, res)
                }));
            }
            for job in jobs {
                if let Ok((endpoint, node_hint, session_probe, res)) = job.join() {
                    match res {
                        Ok(mut stream) => {
                            observe_eth_native_discovery(chain_id);
                            if session_probe {
                                bump_gateway_eth_plugin_session_stage(
                                    chain_id,
                                    endpoint.as_str(),
                                    GatewayEthPluginSessionStage::TcpConnected,
                                    checked_ms,
                                );
                                if let Err(err) = probe_gateway_eth_plugin_peer_session(
                                    chain_id,
                                    endpoint.as_str(),
                                    &mut stream,
                                    checked_ms,
                                ) {
                                    set_gateway_eth_plugin_session_stage(
                                        chain_id,
                                        endpoint.as_str(),
                                        GatewayEthPluginSessionStage::AuthSent,
                                        checked_ms,
                                        Some(err.clone()),
                                    );
                                    errors.push(format!("{endpoint}:{err}"));
                                } else {
                                    let _ = upsert_network_runtime_eth_peer_session(
                                        chain_id,
                                        node_hint,
                                        &[66, 67, 68],
                                        &[1],
                                        None,
                                    );
                                }
                            }
                            reachable.push(endpoint);
                        }
                        Err(err) => {
                            if session_probe
                                && !should_preserve_gateway_eth_plugin_session_stage_on_connect_failure(
                                    chain_id,
                                    endpoint.as_str(),
                                    checked_ms,
                                )
                            {
                                set_gateway_eth_plugin_session_stage(
                                    chain_id,
                                    endpoint.as_str(),
                                    GatewayEthPluginSessionStage::Disconnected,
                                    checked_ms,
                                    Some(err.clone()),
                                );
                            }
                            errors.push(format!("{endpoint}:{err}"));
                        }
                    }
                }
            }
        });
    }
    let total = peers.len();
    let reachable_count = reachable.len();
    let unreachable_count = total.saturating_sub(reachable_count);
    GatewayEthPluginProbeOutcome {
        checked_ms,
        total,
        reachable_count,
        unreachable_count,
        error_preview: errors.into_iter().take(8).collect(),
    }
}

fn probe_gateway_eth_plugin_peers_with_cache(
    chain_id: u64,
    peers: &[PluginPeerEndpoint],
) -> GatewayEthPluginProbeOutcome {
    let checked_ms = now_unix_millis() as u64;
    if peers.is_empty() {
        return GatewayEthPluginProbeOutcome {
            checked_ms,
            total: 0,
            reachable_count: 0,
            unreachable_count: 0,
            error_preview: Vec::new(),
        };
    }

    let cache_key = build_gateway_eth_plugin_probe_cache_key(chain_id, peers);
    let ttl_ms = gateway_eth_public_broadcast_plugin_probe_cache_ttl_ms(chain_id);
    let cache = gateway_eth_plugin_probe_cache();
    if let Some(entry) = cache.get(cache_key.as_str()) {
        if checked_ms.saturating_sub(entry.checked_ms) <= ttl_ms {
            return entry.outcome.clone();
        }
    }

    let timeout_ms = gateway_eth_public_broadcast_plugin_probe_timeout_ms(chain_id);
    let outcome = probe_gateway_eth_plugin_peers_parallel(chain_id, peers, timeout_ms, checked_ms);
    cache.insert(
        cache_key,
        GatewayEthPluginProbeCacheEntry {
            checked_ms: outcome.checked_ms,
            outcome: outcome.clone(),
        },
    );
    outcome
}

pub(super) fn gateway_eth_public_broadcast_ingest_plugin_session_report(
    chain_id: u64,
    params: &serde_json::Value,
) -> Result<serde_json::Value> {
    let now_ms = now_unix_millis() as u64;
    let payload_items = params
        .as_object()
        .and_then(|map| map.get("sessions"))
        .and_then(|value| value.as_array())
        .map(|items| items.iter().collect::<Vec<_>>())
        .unwrap_or_else(|| vec![params]);

    let mut applied = 0_u64;
    let mut skipped = 0_u64;
    let mut errors = Vec::<String>::new();
    for item in payload_items {
        let Some(obj) = item.as_object() else {
            skipped = skipped.saturating_add(1);
            errors.push("session_item_not_object".to_string());
            continue;
        };
        let endpoint = obj
            .get("endpoint")
            .or_else(|| obj.get("peer"))
            .or_else(|| obj.get("enode"))
            .and_then(|value| value.as_str())
            .map(str::trim)
            .filter(|value| !value.is_empty());
        let Some(endpoint) = endpoint else {
            skipped = skipped.saturating_add(1);
            errors.push("session_item_missing_endpoint".to_string());
            continue;
        };
        let stage_raw = obj
            .get("stage")
            .or_else(|| obj.get("session_stage"))
            .or_else(|| obj.get("status"))
            .and_then(|value| value.as_str())
            .unwrap_or("tcp_connected");
        let Some(stage) = parse_gateway_eth_plugin_session_stage(stage_raw) else {
            skipped = skipped.saturating_add(1);
            errors.push(format!("{endpoint}:invalid_stage({stage_raw})"));
            continue;
        };
        let updated_ms = obj
            .get("updated_ms")
            .or_else(|| obj.get("checked_ms"))
            .or_else(|| obj.get("ts"))
            .and_then(value_to_u64)
            .unwrap_or(now_ms);
        let last_error = if stage == GatewayEthPluginSessionStage::Disconnected {
            obj.get("error")
                .or_else(|| obj.get("last_error"))
                .and_then(|value| value.as_str())
                .map(str::trim)
                .filter(|value| !value.is_empty())
                .map(ToOwned::to_owned)
        } else {
            None
        };
        set_gateway_eth_plugin_session_stage(chain_id, endpoint, stage, updated_ms, last_error);
        observe_gateway_eth_plugin_session_stage(chain_id, stage);
        applied = applied.saturating_add(1);
    }

    let mut out = serde_json::json!({
        "ok": true,
        "chain_id": format!("0x{:x}", chain_id),
        "applied": format!("0x{:x}", applied),
        "skipped": format!("0x{:x}", skipped),
    });
    if !errors.is_empty() {
        if let Some(map) = out.as_object_mut() {
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
    Ok(out)
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
    let Some(snapshot) = gateway_eth_public_broadcast_native_peers_snapshot(chain_id) else {
        return Ok(None);
    };
    let peers = snapshot.supvm_peers;
    let peer_nodes = snapshot.supvm_peer_nodes;
    let plugin_peers = snapshot.plugin_peers;
    let peer_source = snapshot.peer_source;
    let route_policy = snapshot.route_policy;
    if peers.is_empty() {
        if plugin_peers.is_empty() {
            return Ok(None);
        }
        let plugin_probe =
            probe_gateway_eth_plugin_peers_with_cache(chain_id, plugin_peers.as_ref());
        let plugin_preview = plugin_peers
            .iter()
            .take(3)
            .map(|peer| peer.endpoint.as_str())
            .collect::<Vec<_>>()
            .join(",");
        if plugin_probe.reachable_count == 0 {
            bail!(
                "public broadcast failed: chain_id={} tx_hash=0x{} reason=plugin_route_unreachable route_policy={} plugin_peers={} checked_ms={} details={} preview={}",
                chain_id,
                to_hex(tx_hash),
                gateway_eth_native_route_policy_label(route_policy),
                plugin_probe.total,
                plugin_probe.checked_ms,
                plugin_probe.error_preview.join(";"),
                plugin_preview,
            );
        }
        bail!(
            "public broadcast failed: chain_id={} tx_hash=0x{} reason=plugin_route_no_native_broadcast_path route_policy={} peer_source={} plugin_peers={} reachable={} checked_ms={}",
            chain_id,
            to_hex(tx_hash),
            gateway_eth_native_route_policy_label(route_policy),
            peer_source,
            plugin_probe.total,
            plugin_probe.reachable_count,
            plugin_probe.checked_ms,
        );
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
        gateway_eth_native_register_peers_parallel(
            &broadcaster,
            peers.as_ref(),
            &mut failed,
            &mut errors,
        );
    }
    gateway_eth_native_send_parallel(
        &broadcaster,
        peers.as_ref(),
        local_node,
        chain_id,
        *tx_hash,
        payload_bytes.as_slice(),
        &mut GatewayEthNativeSendOutcome {
            success: &mut success,
            failed: &mut failed,
            errors: &mut errors,
        },
    );
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

    // Native full-node only path: successful native fanout means local runtime path is alive.
    // Stamp discovery/handshake/sync evidence so runtime caps reflects effective readiness.
    observe_eth_native_discovery(chain_id);
    observe_eth_native_rlpx_auth(chain_id);
    observe_eth_native_rlpx_auth_ack(chain_id);
    observe_eth_native_hello(chain_id);
    observe_eth_native_status(chain_id);
    observe_eth_native_headers_pull(chain_id);
    observe_eth_native_headers_response(chain_id);
    observe_eth_native_bodies_pull(chain_id);
    observe_eth_native_bodies_response(chain_id);
    observe_eth_native_snap_pull(chain_id);
    observe_eth_native_snap_response(chain_id);
    for (peer, _) in peers.as_ref().iter() {
        let _ =
            upsert_network_runtime_eth_peer_session(chain_id, peer.0, &[66, 67, 68], &[1], None);
    }
    let plugin_probe = if plugin_peers.is_empty() {
        None
    } else {
        Some(probe_gateway_eth_plugin_peers_with_cache(
            chain_id,
            plugin_peers.as_ref(),
        ))
    };

    let mut result = serde_json::json!({
        "broadcasted": true,
        "chain_id": format!("0x{:x}", chain_id),
        "tx_hash": format!("0x{}", to_hex(tx_hash)),
        "mode": format!("native_{}", transport.as_mode()),
        "route_policy": gateway_eth_native_route_policy_label(route_policy),
        "peer_source": peer_source,
        "sent": format!("0x{:x}", success),
        "failed": format!("0x{:x}", failed),
        "peers": format!("0x{:x}", success.saturating_add(failed)),
        "supvm_peers": format!("0x{:x}", peers.len()),
        "plugin_peers": format!("0x{:x}", plugin_peers.len()),
        "plugin_route_pending": !plugin_peers.is_empty(),
        "plugin_route_connectivity": plugin_probe
            .as_ref()
            .is_some_and(|probe| probe.reachable_count > 0),
        "plugin_route_reachable": format!(
            "0x{:x}",
            plugin_probe.as_ref().map(|probe| probe.reachable_count).unwrap_or(0)
        ),
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
    if let Some(probe) = plugin_probe.as_ref() {
        if let Some(map) = result.as_object_mut() {
            map.insert(
                "plugin_probe_checked_ms".to_string(),
                serde_json::Value::String(format!("0x{:x}", probe.checked_ms)),
            );
            map.insert(
                "plugin_probe_unreachable".to_string(),
                serde_json::Value::String(format!("0x{:x}", probe.unreachable_count)),
            );
            if !probe.error_preview.is_empty() {
                map.insert(
                    "plugin_probe_errors".to_string(),
                    serde_json::Value::Array(
                        probe
                            .error_preview
                            .iter()
                            .cloned()
                            .map(serde_json::Value::String)
                            .collect(),
                    ),
                );
            }
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
    let _ = (chain_id, tx_hash, payload);
    // Mirror-only policy: upstream proxy fallback is hard-disabled.
    Ok(None)
}

fn gateway_eth_public_broadcast_soft_fail_allowed(raw_error: &str) -> bool {
    raw_error.contains("reason=plugin_route_not_implemented")
        || raw_error.contains("reason=plugin_route_unreachable")
        || raw_error.contains("reason=plugin_route_no_native_broadcast_path")
}

pub(super) fn maybe_execute_gateway_eth_public_broadcast(
    chain_id: u64,
    tx_hash: &[u8; 32],
    payload: GatewayEthPublicBroadcastPayload<'_>,
    required_override: bool,
) -> Result<Option<(String, u64, String)>> {
    let Some(exec_path) = gateway_eth_public_broadcast_exec_path(chain_id) else {
        let required = required_override || gateway_eth_public_broadcast_required(chain_id);
        let mut upstream_error: Option<anyhow::Error> = None;
        match execute_gateway_eth_public_broadcast_upstream(chain_id, tx_hash, payload) {
            Ok(Some(result)) => return Ok(Some(result)),
            Ok(None) => {}
            Err(error) => upstream_error = Some(error),
        }
        let native_result = match execute_gateway_eth_public_broadcast_native(
            chain_id, tx_hash, payload,
        ) {
            Ok(result) => result,
            Err(error) => {
                let raw = error.to_string();
                if !required && gateway_eth_public_broadcast_soft_fail_allowed(raw.as_str()) {
                    if gateway_warn_enabled() {
                        eprintln!(
                            "gateway_warn: ignore non-required public broadcast failure: chain_id={} tx_hash=0x{} err={}",
                            chain_id,
                            to_hex(tx_hash),
                            raw
                        );
                    }
                    None
                } else {
                    return Err(error);
                }
            }
        };
        if let Some(native_result) = native_result {
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
        if required {
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
    use k256::elliptic_curve::sec1::ToEncodedPoint;

    #[test]
    fn native_transactions_payload_decode_accepts_bincode_vec_tx_ir() {
        let mut tx = TxIR::transfer(vec![0x11; 20], vec![0x22; 20], 7, 3, 1);
        tx.compute_hash();
        let payload = bincode::serialize(&vec![tx.clone()]).expect("serialize tx list");
        let decoded = gateway_eth_native_decode_transactions_payload(1, payload.as_slice())
            .expect("decode bincode tx list");
        assert_eq!(decoded.len(), 1);
        assert_eq!(decoded[0].chain_id, tx.chain_id);
        assert_eq!(decoded[0].nonce, tx.nonce);
        assert_eq!(decoded[0].from, tx.from);
    }

    #[test]
    fn native_transactions_payload_decode_accepts_raw_tx_payload() {
        let raw_tx = decode_hex_bytes(
            "0xf9014c800183043897947a250d5630b4cf539739df2c5dacb4c659f2488d8742aaf334a66c00b8e47ff36ab50000000000000000000000000000000000000000000000004d0a1d49072055290000000000000000000000000000000000000000000000000000000000000080000000000000000000000000002f8c92e0101c16f9de9d812a299e219c2ca87b0000000000000000000000000000000000000000000000000000000060a00b4a0000000000000000000000000000000000000000000000000000000000000002000000000000000000000000c02aaa39b223fe8d0a0e5c4f27ead9083c756cc2000000000000000000000000a2b4c0af19cc16a6cfacce81f192b024d625817d25a08f4c79f832fd3b9121c7666978c16a9f817934b0555596ba1b0503cea9da6962a02fde1f8f58003c965c59cbc0d1f277fb866c1b279f93cb20ae80204efce80df5",
            "raw_tx",
        )
        .expect("decode raw tx");
        let decoded = gateway_eth_native_decode_transactions_payload(1, raw_tx.as_slice())
            .expect("decode raw tx payload");
        assert_eq!(decoded.len(), 1);
        assert_eq!(decoded[0].chain_id, 1);
        assert_eq!(decoded[0].nonce, 0);
        assert_eq!(decoded[0].from.len(), 20);
    }

    #[test]
    fn native_transactions_payload_decode_accepts_rlp_transactions_list() {
        let raw_tx = decode_hex_bytes(
            "0xf9014c800183043897947a250d5630b4cf539739df2c5dacb4c659f2488d8742aaf334a66c00b8e47ff36ab50000000000000000000000000000000000000000000000004d0a1d49072055290000000000000000000000000000000000000000000000000000000000000080000000000000000000000000002f8c92e0101c16f9de9d812a299e219c2ca87b0000000000000000000000000000000000000000000000000000000060a00b4a0000000000000000000000000000000000000000000000000000000000000002000000000000000000000000c02aaa39b223fe8d0a0e5c4f27ead9083c756cc2000000000000000000000000a2b4c0af19cc16a6cfacce81f192b024d625817d25a08f4c79f832fd3b9121c7666978c16a9f817934b0555596ba1b0503cea9da6962a02fde1f8f58003c965c59cbc0d1f277fb866c1b279f93cb20ae80204efce80df5",
            "raw_tx",
        )
        .expect("decode raw tx");
        let tx_list_payload = gateway_eth_rlpx_encode_list(&[raw_tx]);
        let decoded = gateway_eth_native_decode_transactions_payload(1, tx_list_payload.as_slice())
            .expect("decode rlp transactions list");
        assert_eq!(decoded.len(), 1);
        assert_eq!(decoded[0].chain_id, 1);
        assert_eq!(decoded[0].from.len(), 20);
    }

    #[test]
    fn swap_classifier_detects_uniswap_v2_router_swap_raw_tx() {
        let raw_tx = decode_hex_bytes(
            "0xf9014c800183043897947a250d5630b4cf539739df2c5dacb4c659f2488d8742aaf334a66c00b8e47ff36ab50000000000000000000000000000000000000000000000004d0a1d49072055290000000000000000000000000000000000000000000000000000000000000080000000000000000000000000002f8c92e0101c16f9de9d812a299e219c2ca87b0000000000000000000000000000000000000000000000000000000060a00b4a0000000000000000000000000000000000000000000000000000000000000002000000000000000000000000c02aaa39b223fe8d0a0e5c4f27ead9083c756cc2000000000000000000000000a2b4c0af19cc16a6cfacce81f192b024d625817d25a08f4c79f832fd3b9121c7666978c16a9f817934b0555596ba1b0503cea9da6962a02fde1f8f58003c965c59cbc0d1f277fb866c1b279f93cb20ae80204efce80df5",
            "raw_tx",
        )
        .expect("decode raw tx");
        let kind = gateway_eth_plugin_detect_swap_kind_from_raw_tx(1, raw_tx.as_slice());
        assert_eq!(kind, Some(GatewayEthSwapKind::V2));
    }

    fn compact_hex(raw: &str) -> String {
        raw.chars().filter(|c| c.is_ascii_hexdigit()).collect()
    }

    #[test]
    fn rlpx_ecdh_matches_geth_static_vector() {
        let prv1 = decode_hex_bytes(
            "7ebbc6a8358bc76dd73ebc557056702c8cfc34e5cfcd90eb83af0347575fd2ad",
            "prv1",
        )
        .expect("decode prv1");
        let prv2 = decode_hex_bytes(
            "6a3d6396903245bba5837752b9e0348874e72db0c4e11e9c485a81b4ea4353b9",
            "prv2",
        )
        .expect("decode prv2");
        let key1 = K256SecretKey::from_slice(&prv1).expect("parse prv1");
        let key2 = K256SecretKey::from_slice(&prv2).expect("parse prv2");
        let shared = gateway_eth_rlpx_ecdh_shared(&key1, &key2.public_key());
        let expected = decode_hex_bytes(
            "167ccc13ac5e8a26b131c3446030c60fbfac6aa8e31149d0869f93626a4cdf62",
            "shared",
        )
        .expect("decode expected shared");
        assert_eq!(shared, expected.as_slice());
    }

    #[test]
    fn rlpx_eip8_auth_vector_decrypts_and_recovers_ephemeral_key() {
        let auth_packet = decode_hex_bytes(
            compact_hex(
                "
                01b304ab7578555167be8154d5cc456f567d5ba302662433674222360f08d5f1534499d3678b513b
                0fca474f3a514b18e75683032eb63fccb16c156dc6eb2c0b1593f0d84ac74f6e475f1b8d56116b84
                9634a8c458705bf83a626ea0384d4d7341aae591fae42ce6bd5c850bfe0b999a694a49bbbaf3ef6c
                da61110601d3b4c02ab6c30437257a6e0117792631a4b47c1d52fc0f8f89caadeb7d02770bf999cc
                147d2df3b62e1ffb2c9d8c125a3984865356266bca11ce7d3a688663a51d82defaa8aad69da39ab6
                d5470e81ec5f2a7a47fb865ff7cca21516f9299a07b1bc63ba56c7a1a892112841ca44b6e0034dee
                70c9adabc15d76a54f443593fafdc3b27af8059703f88928e199cb122362a4b35f62386da7caad09
                c001edaeb5f8a06d2b26fb6cb93c52a9fca51853b68193916982358fe1e5369e249875bb8d0d0ec3
                6f917bc5e1eafd5896d46bd61ff23f1a863a8a8dcd54c7b109b771c8e61ec9c8908c733c0263440e
                2aa067241aaa433f0bb053c7b31a838504b148f570c0ad62837129e547678c5190341e4f1693956c
                3bf7678318e2d5b5340c9e488eefea198576344afbdf66db5f51204a6961a63ce072c8926c
                ",
            )
            .as_str(),
            "auth_packet",
        )
        .expect("decode auth vector");
        assert!(auth_packet.len() > 2);
        let (prefix, cipher) = auth_packet.split_at(2);

        let key_b = decode_hex_bytes(
            "b71c71a67e1177ad4e901695e1b4b9ee17ae16c6668d313eac2f96dbcda3f291",
            "key_b",
        )
        .expect("decode key_b");
        let key_b_secret =
            K256SecretKey::from_slice(&key_b).expect("parse key_b as secp256k1 secret");

        let plain = gateway_eth_rlpx_ecies_decrypt(&key_b_secret, cipher, prefix)
            .expect("decrypt eip8 auth packet");
        let (root, _consumed) =
            gateway_eth_rlpx_parse_item(plain.as_slice()).expect("parse eip8 auth rlp item");
        let GatewayEthRlpxRlpItem::List(payload) = root else {
            panic!("auth root is not list");
        };
        let fields = gateway_eth_rlpx_parse_list_items(payload).expect("parse auth fields");
        assert!(fields.len() >= 4);

        let GatewayEthRlpxRlpItem::Bytes(signature) = fields[0] else {
            panic!("auth signature field must be bytes");
        };
        let GatewayEthRlpxRlpItem::Bytes(initiator_pub) = fields[1] else {
            panic!("auth initiator pub field must be bytes");
        };
        let GatewayEthRlpxRlpItem::Bytes(nonce) = fields[2] else {
            panic!("auth nonce field must be bytes");
        };
        let GatewayEthRlpxRlpItem::Bytes(version_bytes) = fields[3] else {
            panic!("auth version field must be bytes");
        };

        assert_eq!(signature.len(), 65);
        assert_eq!(initiator_pub.len(), 64);
        assert_eq!(nonce.len(), 32);
        let version =
            gateway_eth_rlpx_decode_u64_bytes(version_bytes).expect("decode auth version");
        assert_eq!(version, 4);

        let key_a = decode_hex_bytes(
            "49a7b37aa6f6645917e7b807e9d1c00d4fa71f18343b0d4122a4d2df64dd6fee",
            "key_a",
        )
        .expect("decode key_a");
        let key_a_secret =
            K256SecretKey::from_slice(&key_a).expect("parse key_a as secp256k1 secret");
        let pub_a = key_a_secret.public_key();
        assert_eq!(
            initiator_pub,
            &pub_a.to_encoded_point(false).as_bytes()[1..]
        );

        let token = gateway_eth_rlpx_ecdh_shared(&key_b_secret, &pub_a);
        let mut sign_msg = [0u8; 32];
        for (slot, (a, b)) in sign_msg.iter_mut().zip(token.iter().zip(nonce.iter())) {
            *slot = *a ^ *b;
        }
        let sig = k256::ecdsa::Signature::try_from(&signature[..64]).expect("parse auth signature");
        let recid =
            k256::ecdsa::RecoveryId::try_from(signature[64]).expect("parse auth recovery id");
        let recovered =
            k256::ecdsa::VerifyingKey::recover_from_prehash(sign_msg.as_slice(), &sig, recid)
                .expect("recover auth ephemeral pubkey")
                .to_encoded_point(false);
        let eph_a = decode_hex_bytes(
            "869d6ecf5211f1cc60418a13b9d870b22959d0c16f02bec714c960dd2298a32d",
            "eph_a",
        )
        .expect("decode eph_a");
        let eph_a_secret =
            K256SecretKey::from_slice(&eph_a).expect("parse eph_a as secp256k1 secret");
        assert_eq!(
            recovered.as_bytes(),
            eph_a_secret.public_key().to_encoded_point(false).as_bytes()
        );
    }

    #[test]
    fn rlpx_eip8_ack_vector_decrypts() {
        let ack_packet = decode_hex_bytes(
            compact_hex(
                "
                01ea0451958701280a56482929d3b0757da8f7fbe5286784beead59d95089c217c9b917788989470
                b0e330cc6e4fb383c0340ed85fab836ec9fb8a49672712aeabbdfd1e837c1ff4cace34311cd7f4de
                05d59279e3524ab26ef753a0095637ac88f2b499b9914b5f64e143eae548a1066e14cd2f4bd7f814
                c4652f11b254f8a2d0191e2f5546fae6055694aed14d906df79ad3b407d94692694e259191cde171
                ad542fc588fa2b7333313d82a9f887332f1dfc36cea03f831cb9a23fea05b33deb999e85489e645f
                6aab1872475d488d7bd6c7c120caf28dbfc5d6833888155ed69d34dbdc39c1f299be1057810f34fb
                e754d021bfca14dc989753d61c413d261934e1a9c67ee060a25eefb54e81a4d14baff922180c395d
                3f998d70f46f6b58306f969627ae364497e73fc27f6d17ae45a413d322cb8814276be6ddd13b885b
                201b943213656cde498fa0e9ddc8e0b8f8a53824fbd82254f3e2c17e8eaea009c38b4aa0a3f306e8
                797db43c25d68e86f262e564086f59a2fc60511c42abfb3057c247a8a8fe4fb3ccbadde17514b7ac
                8000cdb6a912778426260c47f38919a91f25f4b5ffb455d6aaaf150f7e5529c100ce62d6d92826a7
                1778d809bdf60232ae21ce8a437eca8223f45ac37f6487452ce626f549b3b5fdee26afd2072e4bc7
                5833c2464c805246155289f4
                ",
            )
            .as_str(),
            "ack_packet",
        )
        .expect("decode ack vector");
        assert!(ack_packet.len() > 2);
        let (prefix, cipher) = ack_packet.split_at(2);

        let key_a = decode_hex_bytes(
            "49a7b37aa6f6645917e7b807e9d1c00d4fa71f18343b0d4122a4d2df64dd6fee",
            "key_a",
        )
        .expect("decode key_a");
        let key_a_secret =
            K256SecretKey::from_slice(&key_a).expect("parse key_a as secp256k1 secret");

        let plain = gateway_eth_rlpx_ecies_decrypt(&key_a_secret, cipher, prefix)
            .expect("decrypt eip8 ack packet");
        let (random_pub, nonce, version) =
            gateway_eth_rlpx_decode_auth_resp_v4(plain.as_slice()).expect("decode auth ack");
        assert_eq!(version, 4);
        let nonce_b = decode_hex_bytes(
            "559aead08264d5795d3909718cdd05abd49572e84fe55590eef31a88a08fdffd",
            "nonce_b",
        )
        .expect("decode nonce_b");
        let mut expected_nonce = [0u8; 32];
        expected_nonce.copy_from_slice(nonce_b.as_slice());
        assert_eq!(nonce, expected_nonce);

        let eph_b = decode_hex_bytes(
            "e238eb8e04fee6511ab04c6dd3c89ce097b11f25d584863ac2b6d5b35b1847e4",
            "eph_b",
        )
        .expect("decode eph_b");
        let eph_b_secret =
            K256SecretKey::from_slice(&eph_b).expect("parse eph_b as secp256k1 secret");
        assert_eq!(
            random_pub,
            eph_b_secret.public_key().to_encoded_point(false).as_bytes()[1..]
        );
    }

    #[test]
    fn rlpx_secrets_match_geth_forward_compat_vector() {
        // Mirrors go-ethereum p2p/rlpx TestHandshakeForwardCompatibility
        // recipient-side (Auth2/Ack2) AES/MAC derivation expectations.
        let auth_packet = decode_hex_bytes(
            compact_hex(
                "
                01b304ab7578555167be8154d5cc456f567d5ba302662433674222360f08d5f1534499d3678b513b
                0fca474f3a514b18e75683032eb63fccb16c156dc6eb2c0b1593f0d84ac74f6e475f1b8d56116b84
                9634a8c458705bf83a626ea0384d4d7341aae591fae42ce6bd5c850bfe0b999a694a49bbbaf3ef6c
                da61110601d3b4c02ab6c30437257a6e0117792631a4b47c1d52fc0f8f89caadeb7d02770bf999cc
                147d2df3b62e1ffb2c9d8c125a3984865356266bca11ce7d3a688663a51d82defaa8aad69da39ab6
                d5470e81ec5f2a7a47fb865ff7cca21516f9299a07b1bc63ba56c7a1a892112841ca44b6e0034dee
                70c9adabc15d76a54f443593fafdc3b27af8059703f88928e199cb122362a4b35f62386da7caad09
                c001edaeb5f8a06d2b26fb6cb93c52a9fca51853b68193916982358fe1e5369e249875bb8d0d0ec3
                6f917bc5e1eafd5896d46bd61ff23f1a863a8a8dcd54c7b109b771c8e61ec9c8908c733c0263440e
                2aa067241aaa433f0bb053c7b31a838504b148f570c0ad62837129e547678c5190341e4f1693956c
                3bf7678318e2d5b5340c9e488eefea198576344afbdf66db5f51204a6961a63ce072c8926c
                ",
            )
            .as_str(),
            "auth_packet",
        )
        .expect("decode auth packet");
        let (auth_prefix, auth_cipher) = auth_packet.split_at(2);

        let key_b = decode_hex_bytes(
            "b71c71a67e1177ad4e901695e1b4b9ee17ae16c6668d313eac2f96dbcda3f291",
            "key_b",
        )
        .expect("decode key_b");
        let key_b_secret =
            K256SecretKey::from_slice(&key_b).expect("parse key_b as secp256k1 secret");
        let auth_plain = gateway_eth_rlpx_ecies_decrypt(&key_b_secret, auth_cipher, auth_prefix)
            .expect("decrypt auth");
        let (auth_root, _auth_consumed) =
            gateway_eth_rlpx_parse_item(auth_plain.as_slice()).expect("parse auth root");
        let GatewayEthRlpxRlpItem::List(auth_payload) = auth_root else {
            panic!("auth root must be list");
        };
        let auth_fields =
            gateway_eth_rlpx_parse_list_items(auth_payload).expect("parse auth fields");
        assert!(auth_fields.len() >= 4);
        let GatewayEthRlpxRlpItem::Bytes(signature) = auth_fields[0] else {
            panic!("auth signature must be bytes");
        };
        let GatewayEthRlpxRlpItem::Bytes(initiator_pub) = auth_fields[1] else {
            panic!("auth initiator pub must be bytes");
        };
        let GatewayEthRlpxRlpItem::Bytes(nonce_a_bytes) = auth_fields[2] else {
            panic!("auth nonce must be bytes");
        };
        assert_eq!(signature.len(), 65);
        assert_eq!(initiator_pub.len(), 64);
        assert_eq!(nonce_a_bytes.len(), 32);

        let mut nonce_a = [0u8; 32];
        nonce_a.copy_from_slice(nonce_a_bytes);
        let mut sec1 = [0u8; 65];
        sec1[0] = 0x04;
        sec1[1..].copy_from_slice(initiator_pub);
        let initiator_static_pub =
            K256PublicKey::from_sec1_bytes(&sec1).expect("parse initiator static pub");
        let token = gateway_eth_rlpx_ecdh_shared(&key_b_secret, &initiator_static_pub);
        let mut sign_msg = [0u8; 32];
        for (slot, (a, b)) in sign_msg.iter_mut().zip(token.iter().zip(nonce_a.iter())) {
            *slot = *a ^ *b;
        }
        let sig = k256::ecdsa::Signature::try_from(&signature[..64]).expect("parse auth sig");
        let recid =
            k256::ecdsa::RecoveryId::try_from(signature[64]).expect("parse auth recovery id");
        let recovered_eph_pub =
            k256::ecdsa::VerifyingKey::recover_from_prehash(sign_msg.as_slice(), &sig, recid)
                .expect("recover eph pub")
                .to_encoded_point(false);
        let remote_eph_pub =
            K256PublicKey::from_sec1_bytes(recovered_eph_pub.as_bytes()).expect("parse eph pub");

        let eph_b = decode_hex_bytes(
            "e238eb8e04fee6511ab04c6dd3c89ce097b11f25d584863ac2b6d5b35b1847e4",
            "eph_b",
        )
        .expect("decode eph_b");
        let eph_b_secret = K256SecretKey::from_slice(&eph_b).expect("parse eph_b secret");
        let nonce_b = decode_hex_bytes(
            "559aead08264d5795d3909718cdd05abd49572e84fe55590eef31a88a08fdffd",
            "nonce_b",
        )
        .expect("decode nonce_b");
        let mut nonce_b_arr = [0u8; 32];
        nonce_b_arr.copy_from_slice(nonce_b.as_slice());

        let ecdhe_secret = gateway_eth_rlpx_ecdh_shared(&eph_b_secret, &remote_eph_pub);
        let nonce_mix = gateway_eth_rlpx_keccak256(&[nonce_b_arr.as_slice(), nonce_a.as_slice()]);
        let shared_secret =
            gateway_eth_rlpx_keccak256(&[ecdhe_secret.as_slice(), nonce_mix.as_slice()]);
        let aes_secret =
            gateway_eth_rlpx_keccak256(&[ecdhe_secret.as_slice(), shared_secret.as_slice()]);
        let mac_secret =
            gateway_eth_rlpx_keccak256(&[ecdhe_secret.as_slice(), aes_secret.as_slice()]);

        let want_aes = decode_hex_bytes(
            "80e8632c05fed6fc2a13b0f8d31a3cf645366239170ea067065aba8e28bac487",
            "want_aes",
        )
        .expect("decode want_aes");
        let want_mac = decode_hex_bytes(
            "2ea74ec5dae199227dff1af715362700e989d889d7a493cb0639691efb8e5f98",
            "want_mac",
        )
        .expect("decode want_mac");
        assert_eq!(aes_secret.as_slice(), want_aes.as_slice());
        assert_eq!(mac_secret.as_slice(), want_mac.as_slice());

        // recipient-side ingress hash init: keccak((MAC ^ respNonce) || authPacket)
        let ingress_prefix = gateway_eth_rlpx_xor_32(&mac_secret, &nonce_b_arr);
        let mut ingress_hash = sha3::Keccak256::new();
        ingress_hash.update(ingress_prefix.as_slice());
        ingress_hash.update(auth_packet.as_slice());
        ingress_hash.update(b"foo");
        let mut foo_ingress_hash = [0u8; 32];
        foo_ingress_hash.copy_from_slice(ingress_hash.finalize().as_slice());
        let want_foo_ingress = decode_hex_bytes(
            "0c7ec6340062cc46f5e9f1e3cf86f8c8c403c5a0964f5df0ebd34a75ddc86db5",
            "want_foo_ingress",
        )
        .expect("decode want_foo_ingress");
        assert_eq!(foo_ingress_hash.as_slice(), want_foo_ingress.as_slice());
    }

    #[test]
    fn rlpx_frame_read_write_matches_geth_golden_vector() {
        // Mirrors go-ethereum p2p/rlpx TestFrameReadWrite with fake hash state = 0x01 * 32.
        let aes_secret = gateway_eth_rlpx_keccak256(&[]);
        let mac_secret = gateway_eth_rlpx_keccak256(&[]);
        let iv = [0u8; 16];
        let fake_hash = [1u8; 32];
        let mut session = GatewayEthRlpxFrameSession {
            enc: GatewayEthRlpxAes256Ctr::new((&aes_secret).into(), (&iv).into()),
            dec: GatewayEthRlpxAes256Ctr::new((&aes_secret).into(), (&iv).into()),
            egress_mac: GatewayEthRlpxHashMac::new_fixed_for_test(&mac_secret, fake_hash)
                .expect("build egress fake mac"),
            ingress_mac: GatewayEthRlpxHashMac::new_fixed_for_test(&mac_secret, fake_hash)
                .expect("build ingress fake mac"),
            snappy: false,
        };

        let msg_payload = gateway_eth_rlpx_encode_list(&[
            gateway_eth_rlpx_encode_u64(1),
            gateway_eth_rlpx_encode_u64(2),
            gateway_eth_rlpx_encode_u64(3),
            gateway_eth_rlpx_encode_u64(4),
        ]);
        let mut encoded = Vec::<u8>::new();
        gateway_eth_rlpx_write_wire_frame(
            &mut encoded,
            &mut session,
            8,
            msg_payload.as_slice(),
            None,
        )
        .expect("write frame");

        let golden = decode_hex_bytes(
            compact_hex(
                "
                00828ddae471818bb0bfa6b551d1cb42
                01010101010101010101010101010101
                ba628a4ba590cb43f7848f41c4382885
                01010101010101010101010101010101
                ",
            )
            .as_str(),
            "geth_frame_golden",
        )
        .expect("decode geth golden frame");
        assert_eq!(encoded, golden);

        let mut reader = std::io::Cursor::new(golden);
        let (code, payload) =
            gateway_eth_rlpx_read_wire_frame(&mut reader, &mut session).expect("read frame");
        assert_eq!(code, 8);
        assert_eq!(payload, msg_payload);
    }

    #[test]
    fn native_ingest_persist_marks_submit_status_and_tx_index_for_runtime_visible_tx() {
        let chain_id = 9_919_031_u64;
        let mut tx = TxIR::transfer(vec![0x31; 20], vec![0x42; 20], 5, 7, chain_id);
        tx.compute_hash();
        let tx_hash = vec_to_32(&tx.hash, "tx_hash").expect("tx hash");

        let chain_type = resolve_evm_chain_type_from_chain_id(chain_id);
        let tap = runtime_tap_ir_batch_v1(chain_type, chain_id, std::slice::from_ref(&tx), 0)
            .expect("tap native tx");
        assert_eq!(tap.accepted, 1);

        let nanos = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_nanos();
        let backend_path = PathBuf::from(format!(
            "artifacts/tests/native-ingest-{}-{}.rocksdb",
            std::process::id(),
            nanos
        ));
        let _ = fs::remove_dir_all(&backend_path);
        let backend = GatewayEthTxIndexStoreBackend::RocksDb {
            path: backend_path.clone(),
        };

        gateway_eth_native_persist_ingested_txs(
            &backend,
            chain_id,
            tx_hash,
            std::slice::from_ref(&tx),
            tap.accepted,
        );

        let indexed = backend
            .load_eth_tx(&tx_hash)
            .expect("load tx index")
            .expect("indexed tx");
        assert_eq!(indexed.tx_hash, tx_hash);
        assert_eq!(indexed.chain_id, chain_id);

        let status =
            gateway_eth_submit_status_by_tx(&backend, &tx_hash).expect("submit status exists");
        assert!(status.accepted);
        assert!(status.pending);
        assert!(!status.onchain);
        assert_eq!(status.chain_id, Some(chain_id));

        let _ = fs::remove_dir_all(&backend_path);
    }

    #[test]
    fn parse_native_peers_auto_routes_by_scheme_and_port() {
        let enode = "enode://0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef@127.0.0.1:30303?discport=30301";
        let raw = format!("2@127.0.0.1:39001,3@127.0.0.1:30304,{}", enode);
        let (supvm, plugin) = parse_gateway_eth_public_broadcast_native_peers(
            raw.as_str(),
            AdaptivePeerRoutePolicy::Auto,
            &[30303, 30304],
        );
        assert_eq!(supvm, vec![(NodeId(2), "127.0.0.1:39001".to_string())]);
        assert_eq!(plugin.len(), 4);
        assert!(plugin
            .iter()
            .any(|peer| peer.endpoint == "3@127.0.0.1:30304"));
        assert!(plugin
            .iter()
            .any(|peer| peer.endpoint == "3@127.0.0.1:30303"));
        assert!(plugin.iter().any(|peer| {
            peer.endpoint.starts_with("enode://") && peer.addr_hint == "127.0.0.1:30303"
        }));
        assert!(plugin.iter().any(|peer| {
            peer.endpoint.starts_with("enode://") && peer.addr_hint == "127.0.0.1:30304"
        }));
    }

    #[test]
    fn parse_native_peers_supvm_only_forces_supvm_route() {
        let enode = "enode://aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa@10.0.0.1:30303";
        let raw = format!("8@127.0.0.1:30303,{}", enode);
        let (supvm, plugin) = parse_gateway_eth_public_broadcast_native_peers(
            raw.as_str(),
            AdaptivePeerRoutePolicy::PrimaryOnly,
            &[30303],
        );
        assert_eq!(supvm.len(), 2);
        assert!(plugin.is_empty());
        assert!(supvm
            .iter()
            .any(|(node, addr)| *node == NodeId(8) && addr == "127.0.0.1:30303"));
    }

    #[test]
    fn parse_native_peers_plugin_only_forces_plugin_route() {
        let raw = "9@127.0.0.1:39001";
        let (supvm, plugin) = parse_gateway_eth_public_broadcast_native_peers(
            raw,
            AdaptivePeerRoutePolicy::PluginOnly,
            &[],
        );
        assert!(supvm.is_empty());
        assert_eq!(plugin.len(), 1);
        assert_eq!(plugin[0].node_hint, 9);
        assert_eq!(plugin[0].addr_hint, "127.0.0.1:39001");
    }

    #[test]
    fn parse_native_peers_adds_port_candidates_for_ipv6_addr_hints() {
        let raw = "11@[::1]:30303";
        let (_supvm, plugin) = parse_gateway_eth_public_broadcast_native_peers(
            raw,
            AdaptivePeerRoutePolicy::PluginOnly,
            &[30303, 30304],
        );
        assert_eq!(plugin.len(), 2);
        assert!(plugin.iter().any(|peer| peer.addr_hint == "[::1]:30303"));
        assert!(plugin.iter().any(|peer| peer.addr_hint == "[::1]:30304"));
    }

    #[test]
    fn builtin_bootnodes_cover_mainnet_and_sepolia() {
        assert!(gateway_eth_builtin_bootnodes_for_chain(1).len() >= 4);
        assert!(gateway_eth_builtin_bootnodes_for_chain(11_155_111).len() >= 5);
        assert!(gateway_eth_builtin_bootnodes_for_chain(999_999).is_empty());
    }

    #[test]
    fn plugin_probe_detects_reachable_listener() {
        let listener = std::net::TcpListener::bind("127.0.0.1:0").expect("bind probe listener");
        let addr = listener.local_addr().expect("listener local addr");
        let peers = vec![PluginPeerEndpoint {
            endpoint: format!("enode://probe@{}", addr),
            node_hint: 1,
            addr_hint: addr.to_string(),
        }];
        let outcome = probe_gateway_eth_plugin_peers_parallel(9_001, &peers, 300, 123);
        assert_eq!(outcome.total, 1);
        assert_eq!(outcome.reachable_count, 1);
        assert_eq!(outcome.unreachable_count, 0);
    }

    #[test]
    fn plugin_probe_reports_unreachable_listener() {
        let listener = std::net::TcpListener::bind("127.0.0.1:0").expect("bind ephemeral listener");
        let addr = listener.local_addr().expect("ephemeral local addr");
        drop(listener);

        let peers = vec![PluginPeerEndpoint {
            endpoint: format!("enode://probe@{}", addr),
            node_hint: 1,
            addr_hint: addr.to_string(),
        }];
        let outcome = probe_gateway_eth_plugin_peers_parallel(9_002, &peers, 150, 456);
        assert_eq!(outcome.total, 1);
        assert_eq!(outcome.reachable_count, 0);
        assert_eq!(outcome.unreachable_count, 1);
        assert!(!outcome.error_preview.is_empty());
    }

    #[test]
    fn plugin_probe_session_reaches_ready_when_peer_replies() {
        let listener =
            std::net::TcpListener::bind("127.0.0.1:0").expect("bind plugin probe listener");
        let addr = listener.local_addr().expect("listener local addr");
        let serve = std::thread::spawn(move || {
            use std::io::{Read, Write};
            let (mut socket, _) = listener.accept().expect("accept plugin probe");
            let mut recv = [0_u8; 256];
            let received = socket.read(&mut recv).expect("read plugin probe");
            assert!(received > 0);
            socket
                .write_all(b"novovm-plugin-probe-ack")
                .expect("write plugin probe ack");
        });

        let chain_id = 9_003_u64;
        let session_mode_key = format!(
            "NOVOVM_GATEWAY_ETH_PUBLIC_BROADCAST_PLUGIN_SESSION_PROBE_MODE_CHAIN_{}",
            chain_id
        );
        std::env::set_var(session_mode_key.as_str(), "all");
        let endpoint = format!("3@{}", addr);
        let peers = vec![PluginPeerEndpoint {
            endpoint: endpoint.clone(),
            node_hint: 1,
            addr_hint: addr.to_string(),
        }];
        let outcome = probe_gateway_eth_plugin_peers_parallel(
            chain_id,
            &peers,
            600,
            now_unix_millis() as u64,
        );
        serve.join().expect("join plugin probe server");
        std::env::remove_var(session_mode_key.as_str());
        assert_eq!(outcome.total, 1);
        assert_eq!(outcome.reachable_count, 1);

        let stats = gateway_eth_plugin_session_stage_stats(chain_id, peers.as_ref());
        assert_eq!(stats.ready, 1);
        assert_eq!(stats.disconnected, 0);
    }

    #[test]
    fn plugin_session_probe_mode_filters_endpoints() {
        assert!(gateway_eth_plugin_session_probe_enabled(
            GatewayEthPluginSessionProbeMode::EnodeOnly,
            "enode://abcdef@127.0.0.1:30303",
        ));
        assert!(!gateway_eth_plugin_session_probe_enabled(
            GatewayEthPluginSessionProbeMode::EnodeOnly,
            "3@127.0.0.1:30303",
        ));
        assert!(gateway_eth_plugin_session_probe_enabled(
            GatewayEthPluginSessionProbeMode::All,
            "3@127.0.0.1:30303",
        ));
        assert!(!gateway_eth_plugin_session_probe_enabled(
            GatewayEthPluginSessionProbeMode::Disabled,
            "enode://abcdef@127.0.0.1:30303",
        ));
    }

    #[test]
    fn plugin_session_report_ingest_updates_ready_stage() {
        let chain_id = 9_005_u64;
        let endpoint = "enode://report@127.0.0.1:30303";
        let report = serde_json::json!({
            "sessions": [
                {
                    "endpoint": endpoint,
                    "stage": "ready",
                }
            ]
        });
        let out = gateway_eth_public_broadcast_ingest_plugin_session_report(chain_id, &report)
            .expect("ingest plugin session report");
        assert_eq!(out["applied"], "0x1");
        let peers = vec![PluginPeerEndpoint {
            endpoint: endpoint.to_string(),
            node_hint: 1,
            addr_hint: "127.0.0.1:30303".to_string(),
        }];
        let stats = gateway_eth_plugin_session_stage_stats(chain_id, peers.as_ref());
        assert_eq!(stats.ready, 1);
        assert_eq!(stats.disconnected, 0);
    }

    #[test]
    fn plugin_probe_does_not_override_recent_reported_ready_stage() {
        let chain_id = 9_006_u64;
        let endpoint = "enode://reported-ready@127.0.0.1:9";
        let report = serde_json::json!({
            "sessions": [
                {
                    "endpoint": endpoint,
                    "stage": "ready",
                    "updated_ms": now_unix_millis() as u64,
                }
            ]
        });
        gateway_eth_public_broadcast_ingest_plugin_session_report(chain_id, &report)
            .expect("ingest reported ready stage");
        let peers = vec![PluginPeerEndpoint {
            endpoint: endpoint.to_string(),
            node_hint: 1,
            addr_hint: "127.0.0.1:9".to_string(),
        }];
        let outcome = probe_gateway_eth_plugin_peers_parallel(
            chain_id,
            &peers,
            100,
            now_unix_millis() as u64,
        );
        assert_eq!(outcome.unreachable_count, 1);
        let stats = gateway_eth_plugin_session_stage_stats(chain_id, peers.as_ref());
        assert_eq!(stats.ready, 1);
    }

    #[test]
    fn plugin_peers_json_exposes_learning_fields() {
        let chain_id = 9_007_u64;
        let endpoint = "enode://4aeb4ab6c14b23e2c4cfdce879c04b0748a20d8e9b59e25ded2a08143e265c6c25936e74cbc8e641e3312ca288673d91f2f93f8e277de3cfa444ecdaaf982052@127.0.0.1:30303";
        set_gateway_eth_plugin_session_stage(
            chain_id,
            endpoint,
            GatewayEthPluginSessionStage::Ready,
            now_unix_millis() as u64,
            None,
        );
        let worker_key = build_gateway_eth_plugin_rlpx_worker_key(chain_id, endpoint);
        update_gateway_eth_plugin_rlpx_worker_state(worker_key.as_str(), |state| {
            state.learning_score = 0x345;
            state.sessions_completed = 0x12;
            state.sessions_with_gossip = 0x7;
            state.total_new_pooled_hashes = 0x222;
            state.total_unique_new_pooled_hashes = 0x111;
            state.total_duplicate_new_pooled_hashes = 0x111;
            state.first_seen_hash_count = 0x111;
            state.total_pooled_txs_imported = 0x111;
            state.total_unique_pooled_txs = 0x22;
            state.total_duplicate_pooled_txs = 0x33;
            state.first_seen_tx_count = 0x22;
            state.last_new_pooled_ms = now_unix_millis() as u64;
            state.recent_new_pooled_hashes_total = 0x222;
            state.recent_new_pooled_hashes_window_start_ms = now_unix_millis() as u64;
            state.recent_unique_new_pooled_hashes_total = 0x111;
            state.recent_unique_pooled_txs_total = 0x22;
            state.recent_duplicate_new_pooled_hashes_total = 0x111;
            state.recent_duplicate_pooled_txs_total = 0x33;
            state.recent_dedup_window_start_ms = now_unix_millis() as u64;
            state.last_first_post_ready_code = 0x18;
        });

        let payload = gateway_eth_public_broadcast_plugin_peers_json(chain_id);
        let rows = payload["items"]
            .as_array()
            .expect("plugin peers items should be array");
        let row = rows
            .iter()
            .find(|item| item["endpoint"].as_str() == Some(endpoint))
            .expect("target endpoint row should exist");

        assert_eq!(row["tier"].as_str(), Some("core"));
        assert_eq!(row["ready_count"].as_str(), Some("0x12"));
        assert_eq!(row["new_pooled_count"].as_str(), Some("0x0"));
        assert_eq!(row["pooled_txs_total"].as_str(), Some("0x0"));
        assert_eq!(row["disconnect_count"].as_str(), Some("0x0"));
        assert_eq!(row["learning_score"].as_str(), Some("0x345"));
        assert_eq!(row["sessions_completed"].as_str(), Some("0x12"));
        assert_eq!(row["sessions_with_gossip"].as_str(), Some("0x7"));
        assert_eq!(row["total_new_pooled_hashes"].as_str(), Some("0x222"));
        assert_eq!(
            row["total_unique_new_pooled_hashes"].as_str(),
            Some("0x111")
        );
        assert_eq!(
            row["total_duplicate_new_pooled_hashes"].as_str(),
            Some("0x111")
        );
        assert!(row["last_new_pooled_ms"].as_str().is_some());
        assert_eq!(
            row["recent_new_pooled_hashes_total"].as_str(),
            Some("0x222")
        );
        assert_eq!(
            row["recent_unique_new_pooled_hashes_total"].as_str(),
            Some("0x111")
        );
        assert_eq!(row["recent_unique_pooled_txs_total"].as_str(), Some("0x22"));
        assert_eq!(
            row["recent_duplicate_new_pooled_hashes_total"].as_str(),
            Some("0x111")
        );
        assert_eq!(
            row["recent_duplicate_pooled_txs_total"].as_str(),
            Some("0x33")
        );
        assert_eq!(row["total_pooled_txs_imported"].as_str(), Some("0x111"));
        assert_eq!(row["total_unique_pooled_txs"].as_str(), Some("0x22"));
        assert_eq!(row["total_duplicate_pooled_txs"].as_str(), Some("0x33"));
        assert_eq!(row["first_seen_hash_count"].as_str(), Some("0x111"));
        assert_eq!(row["first_seen_tx_count"].as_str(), Some("0x22"));
        assert_eq!(row["last_first_post_ready_code"].as_str(), Some("0x18"));
    }

    #[test]
    fn rlpx_peer_profile_persist_roundtrip() {
        let chain_id = 9_007_101_u64;
        let endpoint = "enode://persist-roundtrip@127.0.0.1:30303";
        let worker_key = build_gateway_eth_plugin_rlpx_worker_key(chain_id, endpoint);
        let profile_path = std::env::temp_dir().join(format!(
            "novovm-evm-gateway-rlpx-profile-{}-{}.json",
            chain_id,
            now_unix_millis()
        ));
        let persist_key =
            format!("NOVOVM_GATEWAY_ETH_PLUGIN_RLPX_PROFILE_PERSIST_CHAIN_{chain_id}");
        let path_key = format!("NOVOVM_GATEWAY_ETH_PLUGIN_RLPX_PROFILE_PATH_CHAIN_{chain_id}");
        std::env::set_var(persist_key.as_str(), "true");
        std::env::set_var(path_key.as_str(), profile_path.display().to_string());

        let worker_map = gateway_eth_plugin_rlpx_worker_state_map();
        let prefix = format!("{chain_id}:");
        let stale_keys = worker_map
            .iter()
            .filter_map(|entry| {
                let key = entry.key();
                if key.starts_with(prefix.as_str()) {
                    Some(key.to_string())
                } else {
                    None
                }
            })
            .collect::<Vec<_>>();
        for key in stale_keys {
            worker_map.remove(key.as_str());
        }
        gateway_eth_plugin_rlpx_profile_persist_state_map().remove(&chain_id);
        if let Ok(mut guard) = gateway_eth_plugin_rlpx_profile_loaded_chains().lock() {
            guard.remove(&chain_id);
        }

        update_gateway_eth_plugin_rlpx_worker_state(worker_key.as_str(), |state| {
            state.learning_score = 777;
            state.last_sample_score = 321;
            state.sessions_completed = 6;
            state.sessions_with_gossip = 4;
            state.total_new_pooled_hashes = 250;
            state.total_unique_new_pooled_hashes = 200;
            state.total_duplicate_new_pooled_hashes = 50;
            state.total_pooled_txs_received = 18;
            state.total_unique_pooled_txs = 10;
            state.total_duplicate_pooled_txs = 8;
            state.total_pooled_txs_imported = 12;
            state.first_seen_hash_count = 200;
            state.first_seen_tx_count = 10;
            state.dial_attempt_count = 9;
            state.disconnect_count = 2;
            state.disconnect_too_many_count = 1;
            state.last_success_ms = 11_223_344;
            state.last_first_post_ready_code = 0x18;
        });
        let written = gateway_eth_plugin_rlpx_profile_flush_now(chain_id).expect("flush profile");
        assert_eq!(written, 1);
        assert!(profile_path.exists());

        worker_map.remove(worker_key.as_str());
        gateway_eth_plugin_rlpx_profile_persist_state_map().remove(&chain_id);
        if let Ok(mut guard) = gateway_eth_plugin_rlpx_profile_loaded_chains().lock() {
            guard.remove(&chain_id);
        }

        let loaded = gateway_eth_plugin_rlpx_profile_load_now(chain_id).expect("load profile");
        assert_eq!(loaded, 1);
        let reloaded = snapshot_gateway_eth_plugin_rlpx_worker_state(worker_key.as_str());
        assert_eq!(reloaded.learning_score, 777);
        assert_eq!(reloaded.last_sample_score, 321);
        assert_eq!(reloaded.sessions_completed, 6);
        assert_eq!(reloaded.sessions_with_gossip, 4);
        assert_eq!(reloaded.total_new_pooled_hashes, 250);
        assert_eq!(reloaded.total_unique_new_pooled_hashes, 200);
        assert_eq!(reloaded.total_duplicate_new_pooled_hashes, 50);
        assert_eq!(reloaded.total_pooled_txs_received, 18);
        assert_eq!(reloaded.total_unique_pooled_txs, 10);
        assert_eq!(reloaded.total_duplicate_pooled_txs, 8);
        assert_eq!(reloaded.total_pooled_txs_imported, 12);
        assert_eq!(reloaded.first_seen_hash_count, 200);
        assert_eq!(reloaded.first_seen_tx_count, 10);
        assert_eq!(reloaded.dial_attempt_count, 9);
        assert_eq!(reloaded.disconnect_count, 2);
        assert_eq!(reloaded.disconnect_too_many_count, 1);
        assert_eq!(reloaded.last_success_ms, 11_223_344);
        assert_eq!(reloaded.last_first_post_ready_code, 0x18);
        assert!(!reloaded.running);
        assert_eq!(reloaded.cooldown_until_ms, 0);

        worker_map.remove(worker_key.as_str());
        gateway_eth_plugin_rlpx_profile_persist_state_map().remove(&chain_id);
        if let Ok(mut guard) = gateway_eth_plugin_rlpx_profile_loaded_chains().lock() {
            guard.remove(&chain_id);
        }
        std::env::remove_var(persist_key.as_str());
        std::env::remove_var(path_key.as_str());
        let _ = std::fs::remove_file(profile_path);
    }

    #[test]
    fn plugin_session_stage_stats_tracks_cached_stage() {
        let chain_id = 9_004_u64;
        let peers = vec![PluginPeerEndpoint {
            endpoint: "enode://stage@127.0.0.1:30303".to_string(),
            node_hint: 3,
            addr_hint: "127.0.0.1:30303".to_string(),
        }];
        set_gateway_eth_plugin_session_stage(
            chain_id,
            peers[0].endpoint.as_str(),
            GatewayEthPluginSessionStage::TcpConnected,
            now_unix_millis() as u64,
            None,
        );
        let stats = gateway_eth_plugin_session_stage_stats(chain_id, peers.as_ref());
        assert_eq!(stats.tcp_connected, 1);
        assert_eq!(stats.disconnected, 0);
        assert_eq!(
            gateway_eth_plugin_session_stage_count(
                &stats,
                GatewayEthPluginSessionStage::TcpConnected,
            ),
            1
        );
    }

    #[test]
    fn peer_snapshot_merges_ready_session_cache_peers() {
        let chain_id = 9_004_100_u64;
        let endpoint = "enode://0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef@127.0.0.1:30303";
        set_gateway_eth_plugin_session_stage(
            chain_id,
            endpoint,
            GatewayEthPluginSessionStage::Ready,
            now_unix_millis() as u64,
            None,
        );
        let snapshot =
            gateway_eth_public_broadcast_native_peers_snapshot(chain_id).expect("snapshot exists");
        assert!(snapshot
            .plugin_peers
            .iter()
            .any(|peer| peer.endpoint.eq_ignore_ascii_case(endpoint)));
        assert!(snapshot.peer_source.contains("session_cache"));
    }

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
    fn sync_pull_phase_routes_to_expected_native_request() {
        assert!(matches!(
            gateway_eth_native_sync_pull_request(
                NodeId(1),
                1,
                gateway_eth_native_sync_pull_phase_tag("headers"),
                100,
                120
            ),
            ProtocolMessage::EvmNative(EvmNativeMessage::GetBlockHeaders { .. })
        ));
        assert!(matches!(
            gateway_eth_native_sync_pull_request(
                NodeId(1),
                1,
                gateway_eth_native_sync_pull_phase_tag("bodies"),
                100,
                120
            ),
            ProtocolMessage::EvmNative(EvmNativeMessage::GetBlockBodies { .. })
        ));
        assert!(matches!(
            gateway_eth_native_sync_pull_request(
                NodeId(1),
                1,
                gateway_eth_native_sync_pull_phase_tag("state"),
                100,
                120
            ),
            ProtocolMessage::EvmNative(EvmNativeMessage::SnapGetAccountRange { .. })
        ));
        assert!(matches!(
            gateway_eth_native_sync_pull_request(
                NodeId(1),
                1,
                gateway_eth_native_sync_pull_phase_tag("finalize"),
                100,
                120
            ),
            ProtocolMessage::EvmNative(EvmNativeMessage::GetBlockHeaders { .. })
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

    #[test]
    fn rlpx_learning_state_updates_and_smooths_scores() {
        let mut state = GatewayEthPluginRlpxWorkerState::default();
        let metrics1 = GatewayEthPluginRlpxSessionMetrics {
            first_post_ready_code: Some(
                GATEWAY_ETH_PLUGIN_RLPX_BASE_PROTOCOL_OFFSET
                    + GATEWAY_ETH_PLUGIN_RLPX_ETH_NEW_POOLED_HASHES_MSG,
            ),
            new_pooled_msgs: 2,
            new_pooled_hashes: 80,
            unique_new_pooled_hashes: 40,
            duplicate_new_pooled_hashes: 40,
            get_pooled_sent: 6,
            pooled_msgs: 2,
            pooled_txs_received: 12,
            unique_pooled_txs: 7,
            duplicate_pooled_txs: 5,
            pooled_txs_imported: 10,
            first_seen_hashes: 40,
            first_seen_txs: 7,
            swap_hits: 0,
            swap_v2_hits: 0,
            swap_v3_hits: 0,
            unique_swap_hits: 0,
            first_gossip_latency_ms: 0,
            first_swap_latency_ms: 0,
            txs_msgs: 1,
        };
        let sample1 = gateway_eth_plugin_rlpx_learning_sample_score(&metrics1);
        gateway_eth_plugin_rlpx_update_learning_state(&mut state, &metrics1);

        assert_eq!(state.sessions_completed, 1);
        assert_eq!(state.sessions_with_gossip, 1);
        assert_eq!(state.total_new_pooled_msgs, 2);
        assert_eq!(state.total_new_pooled_hashes, 80);
        assert_eq!(state.total_unique_new_pooled_hashes, 40);
        assert_eq!(state.total_duplicate_new_pooled_hashes, 40);
        assert_eq!(state.total_get_pooled_sent, 6);
        assert_eq!(state.total_pooled_msgs, 2);
        assert_eq!(state.total_pooled_txs_received, 12);
        assert_eq!(state.total_unique_pooled_txs, 7);
        assert_eq!(state.total_duplicate_pooled_txs, 5);
        assert_eq!(state.total_pooled_txs_imported, 10);
        assert_eq!(state.first_seen_hash_count, 40);
        assert_eq!(state.first_seen_tx_count, 7);
        assert_eq!(state.total_txs_msgs, 1);
        assert_eq!(
            state.last_first_post_ready_code,
            GATEWAY_ETH_PLUGIN_RLPX_BASE_PROTOCOL_OFFSET
                + GATEWAY_ETH_PLUGIN_RLPX_ETH_NEW_POOLED_HASHES_MSG
        );
        assert_eq!(state.last_sample_score, sample1);
        assert_eq!(state.learning_score, sample1);

        let metrics2 = GatewayEthPluginRlpxSessionMetrics::default();
        let sample2 = gateway_eth_plugin_rlpx_learning_sample_score(&metrics2);
        gateway_eth_plugin_rlpx_update_learning_state(&mut state, &metrics2);

        let expected_smoothed = sample1
            .saturating_mul(7)
            .saturating_add(sample2.saturating_mul(3))
            / 10;
        assert_eq!(state.sessions_completed, 2);
        assert_eq!(state.sessions_with_gossip, 1);
        assert_eq!(state.last_sample_score, sample2);
        assert_eq!(state.learning_score, expected_smoothed);
    }

    #[test]
    fn rlpx_seen_index_marks_unique_then_duplicate() {
        let chain_id = 9_901_020u64;
        clear_gateway_eth_plugin_rlpx_seen_index(chain_id);
        let now_ms = now_unix_millis() as u64;

        let h1 = [0x11u8; 32];
        assert!(gateway_eth_plugin_rlpx_mark_seen_hash(chain_id, h1, now_ms));
        assert!(!gateway_eth_plugin_rlpx_mark_seen_hash(
            chain_id,
            h1,
            now_ms.saturating_add(1)
        ));

        let t1 = [0x22u8; 32];
        assert!(gateway_eth_plugin_rlpx_mark_seen_tx(chain_id, t1, now_ms));
        assert!(!gateway_eth_plugin_rlpx_mark_seen_tx(
            chain_id,
            t1,
            now_ms.saturating_add(1)
        ));
    }

    #[test]
    fn rlpx_worker_sort_key_prioritizes_learned_peers() {
        let now = 10_000_u64;

        let learned_low = GatewayEthPluginRlpxWorkerState {
            learning_score: 120,
            sessions_completed: 1,
            ..Default::default()
        };
        let learned_high = GatewayEthPluginRlpxWorkerState {
            learning_score: 900,
            sessions_completed: 1,
            ..Default::default()
        };
        let recent_success = GatewayEthPluginRlpxWorkerState {
            last_success_ms: now.saturating_sub(10),
            ..Default::default()
        };
        let fresh = GatewayEthPluginRlpxWorkerState::default();
        let cooldown = GatewayEthPluginRlpxWorkerState {
            learning_score: 2_000,
            cooldown_until_ms: now + 500,
            ..Default::default()
        };

        let chain_id = 1_u64;
        let key_learned_low =
            gateway_eth_plugin_rlpx_worker_sort_key(chain_id, &learned_low, now, 0, 5, 0);
        let key_learned_high =
            gateway_eth_plugin_rlpx_worker_sort_key(chain_id, &learned_high, now, 1, 5, 0);
        let key_recent_success =
            gateway_eth_plugin_rlpx_worker_sort_key(chain_id, &recent_success, now, 2, 5, 0);
        let key_fresh = gateway_eth_plugin_rlpx_worker_sort_key(chain_id, &fresh, now, 3, 5, 0);
        let key_cooldown =
            gateway_eth_plugin_rlpx_worker_sort_key(chain_id, &cooldown, now, 4, 5, 0);

        assert_eq!(key_learned_low.0, 1);
        assert_eq!(key_learned_high.0, 1);
        assert!(key_learned_high < key_learned_low);
        assert_eq!(key_recent_success.0, 1);
        assert_eq!(key_fresh.0, 2);
        assert_eq!(key_cooldown.0, 9);

        assert!(key_learned_high < key_recent_success);
        assert!(key_recent_success < key_fresh);
        assert!(key_fresh < key_cooldown);
    }

    #[test]
    fn rlpx_worker_priority_signal_penalizes_too_many_peers_congestion() {
        let chain_id = 1_u64;
        let healthy = GatewayEthPluginRlpxWorkerState {
            learning_score: 25_000,
            sessions_with_gossip: 5,
            total_unique_new_pooled_hashes: 3_000,
            total_unique_pooled_txs: 2_000,
            recent_unique_new_pooled_hashes_total: 256,
            recent_unique_pooled_txs_total: 64,
            first_seen_hash_count: 2_000,
            first_seen_tx_count: 600,
            ..Default::default()
        };
        let mut congested = healthy.clone();
        congested.disconnect_too_many_count = 128;

        let healthy_signal = gateway_eth_plugin_rlpx_worker_priority_signal(chain_id, &healthy);
        let congested_signal = gateway_eth_plugin_rlpx_worker_priority_signal(chain_id, &congested);
        assert!(healthy_signal > congested_signal);

        let healthy_score = gateway_eth_plugin_rlpx_worker_score(chain_id, &healthy);
        let congested_score = gateway_eth_plugin_rlpx_worker_score(chain_id, &congested);
        assert!(healthy_score > congested_score);
    }

    #[test]
    fn rlpx_swap_priority_mode_prefers_swap_rich_low_latency_peer() {
        let chain_id = 9_901_111_u64;
        let enable_key = format!(
            "NOVOVM_GATEWAY_ETH_PLUGIN_MEMPOOL_INGEST_RLPX_SWAP_PRIORITY_ENABLE_CHAIN_{}",
            chain_id
        );
        let latency_key = format!(
            "NOVOVM_GATEWAY_ETH_PLUGIN_MEMPOOL_INGEST_RLPX_SWAP_PRIORITY_LATENCY_TARGET_MS_CHAIN_{}",
            chain_id
        );
        std::env::set_var(enable_key.as_str(), "1");
        std::env::set_var(latency_key.as_str(), "800");
        let base = GatewayEthPluginRlpxWorkerState {
            learning_score: 8_000,
            sessions_with_gossip: 3,
            recent_unique_new_pooled_hashes_total: 64,
            recent_unique_pooled_txs_total: 16,
            ..Default::default()
        };
        let mut swap_rich = base.clone();
        swap_rich.recent_swap_hits_total = 48;
        swap_rich.recent_unique_swap_hits_total = 32;
        swap_rich.total_swap_hits = 320;
        swap_rich.total_unique_swap_hits = 220;
        swap_rich.total_swap_v2_hits = 120;
        swap_rich.total_swap_v3_hits = 200;
        swap_rich.total_first_gossip_latency_ms = 250;
        swap_rich.first_gossip_latency_samples = 1;
        swap_rich.total_first_swap_latency_ms = 300;
        swap_rich.first_swap_latency_samples = 1;

        let base_signal = gateway_eth_plugin_rlpx_worker_priority_signal(chain_id, &base);
        let swap_signal = gateway_eth_plugin_rlpx_worker_priority_signal(chain_id, &swap_rich);
        assert!(swap_signal > base_signal);

        let base_score = gateway_eth_plugin_rlpx_worker_score(chain_id, &base);
        let swap_score = gateway_eth_plugin_rlpx_worker_score(chain_id, &swap_rich);
        assert!(swap_score > base_score);

        std::env::remove_var(enable_key.as_str());
        std::env::remove_var(latency_key.as_str());
    }

    #[test]
    fn rlpx_swap_priority_mode_requires_swap_signal_for_core_tier() {
        let chain_id = 9_901_112_u64;
        let enable_key = format!(
            "NOVOVM_GATEWAY_ETH_PLUGIN_MEMPOOL_INGEST_RLPX_SWAP_PRIORITY_ENABLE_CHAIN_{}",
            chain_id
        );
        std::env::set_var(enable_key.as_str(), "1");
        let now = now_unix_millis() as u64;
        let mut no_swap = GatewayEthPluginRlpxWorkerState {
            sessions_with_gossip: 2,
            sessions_completed: 2,
            last_success_ms: now,
            last_new_pooled_ms: now,
            recent_new_pooled_hashes_total: 128,
            recent_unique_new_pooled_hashes_total: 96,
            total_new_pooled_hashes: 512,
            total_unique_new_pooled_hashes: 384,
            ..Default::default()
        };
        assert_eq!(
            gateway_eth_plugin_rlpx_worker_tier_rank(chain_id, &no_swap, now),
            1
        );
        no_swap.recent_swap_hits_total = 2;
        no_swap.recent_unique_swap_hits_total = 1;
        no_swap.total_swap_hits = 2;
        no_swap.total_unique_swap_hits = 1;
        no_swap.last_swap_ms = now;
        assert_eq!(
            gateway_eth_plugin_rlpx_worker_tier_rank(chain_id, &no_swap, now),
            0
        );
        std::env::remove_var(enable_key.as_str());
    }

    #[test]
    fn rlpx_worker_backoff_is_tier_aware() {
        let now = now_unix_millis() as u64;
        let core_state = GatewayEthPluginRlpxWorkerState {
            sessions_with_gossip: 1,
            last_new_pooled_ms: now,
            ..Default::default()
        };
        let active_state = GatewayEthPluginRlpxWorkerState {
            sessions_completed: 1,
            last_success_ms: now,
            ..Default::default()
        };
        let candidate_state = GatewayEthPluginRlpxWorkerState::default();

        let chain_id = 1_u64;
        let core_rank = gateway_eth_plugin_rlpx_worker_tier_rank(chain_id, &core_state, now);
        let active_rank = gateway_eth_plugin_rlpx_worker_tier_rank(chain_id, &active_state, now);
        let candidate_rank =
            gateway_eth_plugin_rlpx_worker_tier_rank(chain_id, &candidate_state, now);
        assert_eq!(core_rank, 0);
        assert_eq!(active_rank, 1);
        assert_eq!(candidate_rank, 2);

        let failure1 = gateway_eth_plugin_rlpx_worker_fail_backoff_ms(1);
        let failure8 = gateway_eth_plugin_rlpx_worker_fail_backoff_ms(8);
        assert_eq!(failure1, 500);
        assert_eq!(failure8, 30_000);

        let (core_min, core_max) = gateway_eth_plugin_rlpx_worker_tier_backoff_bounds_ms(core_rank);
        let (active_min, active_max) =
            gateway_eth_plugin_rlpx_worker_tier_backoff_bounds_ms(active_rank);
        let (candidate_min, candidate_max) =
            gateway_eth_plugin_rlpx_worker_tier_backoff_bounds_ms(candidate_rank);

        assert_eq!(
            failure1.clamp(core_min, core_max),
            GATEWAY_ETH_PLUGIN_RLPX_WORKER_BACKOFF_CORE_MIN_MS
        );
        assert_eq!(
            failure1.clamp(active_min, active_max),
            GATEWAY_ETH_PLUGIN_RLPX_WORKER_BACKOFF_ACTIVE_MIN_MS
        );
        assert_eq!(
            failure1.clamp(candidate_min, candidate_max),
            GATEWAY_ETH_PLUGIN_RLPX_WORKER_BACKOFF_CANDIDATE_MIN_MS
        );
        assert_eq!(
            failure8.clamp(core_min, core_max),
            GATEWAY_ETH_PLUGIN_RLPX_WORKER_BACKOFF_CORE_MAX_MS
        );
        assert_eq!(
            failure8.clamp(active_min, active_max),
            GATEWAY_ETH_PLUGIN_RLPX_WORKER_BACKOFF_ACTIVE_MAX_MS
        );
        assert_eq!(
            failure8.clamp(candidate_min, candidate_max),
            GATEWAY_ETH_PLUGIN_RLPX_WORKER_BACKOFF_CANDIDATE_MAX_MS
        );

        assert_eq!(
            gateway_eth_plugin_rlpx_worker_timeout_backoff_floor_ms(core_rank),
            GATEWAY_ETH_PLUGIN_RLPX_WORKER_TIMEOUT_BACKOFF_CORE_MIN_MS
        );
        assert_eq!(
            gateway_eth_plugin_rlpx_worker_timeout_backoff_floor_ms(active_rank),
            GATEWAY_ETH_PLUGIN_RLPX_WORKER_TIMEOUT_BACKOFF_ACTIVE_MIN_MS
        );
        assert_eq!(
            gateway_eth_plugin_rlpx_worker_timeout_backoff_floor_ms(candidate_rank),
            GATEWAY_ETH_PLUGIN_RLPX_WORKER_TIMEOUT_BACKOFF_CANDIDATE_MIN_MS
        );
    }

    #[test]
    fn rlpx_core_lock_keeps_recently_successful_gossip_peer_in_core() {
        let now = now_unix_millis() as u64;
        let chain_id = 9_901_003u64;
        let core_window_key = format!(
            "NOVOVM_GATEWAY_ETH_PLUGIN_MEMPOOL_INGEST_RLPX_CORE_RECENT_GOSSIP_WINDOW_MS_CHAIN_{}",
            chain_id
        );
        let core_lock_key = format!(
            "NOVOVM_GATEWAY_ETH_PLUGIN_MEMPOOL_INGEST_RLPX_CORE_LOCK_MS_CHAIN_{}",
            chain_id
        );
        std::env::set_var(core_window_key.as_str(), "30000");
        std::env::set_var(core_lock_key.as_str(), "600000");
        let state = GatewayEthPluginRlpxWorkerState {
            sessions_with_gossip: 1,
            last_new_pooled_ms: now.saturating_sub(120_000),
            last_success_ms: now,
            ..Default::default()
        };
        let rank = gateway_eth_plugin_rlpx_worker_tier_rank(chain_id, &state, now);
        assert_eq!(rank, 0);
        std::env::remove_var(core_window_key.as_str());
        std::env::remove_var(core_lock_key.as_str());
    }

    #[test]
    fn rlpx_strong_unique_history_recent_ready_promotes_core() {
        let now = now_unix_millis() as u64;
        let chain_id = 9_901_023u64;
        let core_window_key = format!(
            "NOVOVM_GATEWAY_ETH_PLUGIN_MEMPOOL_INGEST_RLPX_CORE_RECENT_GOSSIP_WINDOW_MS_CHAIN_{}",
            chain_id
        );
        let active_window_key = format!(
            "NOVOVM_GATEWAY_ETH_PLUGIN_MEMPOOL_INGEST_RLPX_ACTIVE_RECENT_READY_WINDOW_MS_CHAIN_{}",
            chain_id
        );
        let recent_min_key = format!(
            "NOVOVM_GATEWAY_ETH_PLUGIN_MEMPOOL_INGEST_RLPX_RECENT_NEW_HASH_MIN_CHAIN_{}",
            chain_id
        );
        std::env::set_var(core_window_key.as_str(), "30000");
        std::env::set_var(active_window_key.as_str(), "30000");
        std::env::set_var(recent_min_key.as_str(), "64");
        let state = GatewayEthPluginRlpxWorkerState {
            sessions_with_gossip: 1,
            last_new_pooled_ms: now.saturating_sub(10 * 60_000),
            last_success_ms: now.saturating_sub(45_000),
            total_unique_new_pooled_hashes: 2048,
            first_seen_hash_count: 2048,
            ..Default::default()
        };
        let rank = gateway_eth_plugin_rlpx_worker_tier_rank(chain_id, &state, now);
        assert_eq!(rank, 0);
        std::env::remove_var(core_window_key.as_str());
        std::env::remove_var(active_window_key.as_str());
        std::env::remove_var(recent_min_key.as_str());
    }

    #[test]
    fn rlpx_elite_unique_history_promotes_core_without_recent_ready() {
        let now = now_unix_millis() as u64;
        let chain_id = 9_901_024u64;
        let core_window_key = format!(
            "NOVOVM_GATEWAY_ETH_PLUGIN_MEMPOOL_INGEST_RLPX_CORE_RECENT_GOSSIP_WINDOW_MS_CHAIN_{}",
            chain_id
        );
        let active_window_key = format!(
            "NOVOVM_GATEWAY_ETH_PLUGIN_MEMPOOL_INGEST_RLPX_ACTIVE_RECENT_READY_WINDOW_MS_CHAIN_{}",
            chain_id
        );
        let recent_min_key = format!(
            "NOVOVM_GATEWAY_ETH_PLUGIN_MEMPOOL_INGEST_RLPX_RECENT_NEW_HASH_MIN_CHAIN_{}",
            chain_id
        );
        std::env::set_var(core_window_key.as_str(), "30000");
        std::env::set_var(active_window_key.as_str(), "30000");
        std::env::set_var(recent_min_key.as_str(), "16");
        let state = GatewayEthPluginRlpxWorkerState {
            sessions_with_gossip: 1,
            last_new_pooled_ms: now.saturating_sub(20 * 60_000),
            last_success_ms: now.saturating_sub(20 * 60_000),
            total_unique_new_pooled_hashes: 700,
            first_seen_hash_count: 700,
            ..Default::default()
        };
        let rank = gateway_eth_plugin_rlpx_worker_tier_rank(chain_id, &state, now);
        assert_eq!(rank, 0);
        std::env::remove_var(core_window_key.as_str());
        std::env::remove_var(active_window_key.as_str());
        std::env::remove_var(recent_min_key.as_str());
    }

    #[test]
    fn rlpx_tier_budgets_shift_to_candidate_when_core_insufficient() {
        let (core_a, active_a, candidate_a) = gateway_eth_plugin_rlpx_tier_budgets(16, 1, 4, 8, 24);
        assert_eq!(core_a + active_a + candidate_a, 16);
        assert!(candidate_a >= 4);

        let (core_b, active_b, candidate_b) =
            gateway_eth_plugin_rlpx_tier_budgets(16, 12, 30, 8, 24);
        assert_eq!(core_b + active_b + candidate_b, 16);
        assert!(core_b >= active_b);
        assert!(candidate_b <= candidate_a);
    }

    #[test]
    fn rlpx_core_fallback_activates_after_low_core_streak() {
        let chain_id = 9_901_091u64;
        gateway_eth_plugin_rlpx_core_fallback_state_map().remove(&chain_id);
        let floor = 3usize;
        let trigger = 3u64;
        let hold = 4u64;

        let s1 =
            gateway_eth_plugin_rlpx_core_fallback_update_tick(chain_id, 1, floor, trigger, hold);
        assert!(!s1.active);
        let s2 =
            gateway_eth_plugin_rlpx_core_fallback_update_tick(chain_id, 2, floor, trigger, hold);
        assert!(!s2.active);
        let s3 =
            gateway_eth_plugin_rlpx_core_fallback_update_tick(chain_id, 1, floor, trigger, hold);
        assert!(s3.active);
        assert_eq!(s3.activation_count, 1);

        let s4 =
            gateway_eth_plugin_rlpx_core_fallback_update_tick(chain_id, 5, floor, trigger, hold);
        assert!(s4.active);
        let s5 =
            gateway_eth_plugin_rlpx_core_fallback_update_tick(chain_id, 5, floor, trigger, hold);
        assert!(s5.active);
        let s6 =
            gateway_eth_plugin_rlpx_core_fallback_update_tick(chain_id, 5, floor, trigger, hold);
        assert!(s6.active);
        let s7 =
            gateway_eth_plugin_rlpx_core_fallback_update_tick(chain_id, 5, floor, trigger, hold);
        assert!(!s7.active);
        gateway_eth_plugin_rlpx_core_fallback_state_map().remove(&chain_id);
    }

    #[test]
    fn rlpx_core_fallback_candidate_budget_has_floor() {
        let (core, active, candidate) =
            gateway_eth_plugin_rlpx_apply_core_fallback_candidate_budget(7, 7, 2, 16, 6);
        assert_eq!(core + active + candidate, 16);
        assert!(core >= 1);
        assert!(active >= 1);
        assert!(candidate >= 6);
    }

    #[test]
    fn rlpx_congested_peer_demotion_rule_prefers_unique_recent_sources() {
        let now_ms = 1_000_000u64;
        let stale_window_ms = 30_000u64;
        let too_many_threshold = 8u64;

        assert!(gateway_eth_plugin_rlpx_should_demote_congested_peer(
            0,
            12,
            0,
            0,
            64,
            0,
            now_ms.saturating_sub(stale_window_ms + 1),
            now_ms,
            stale_window_ms,
            too_many_threshold,
        ));
        assert!(!gateway_eth_plugin_rlpx_should_demote_congested_peer(
            0,
            12,
            16,
            4,
            8,
            0,
            now_ms.saturating_sub(stale_window_ms + 1),
            now_ms,
            stale_window_ms,
            too_many_threshold,
        ));
        assert!(!gateway_eth_plugin_rlpx_should_demote_congested_peer(
            2,
            100,
            0,
            0,
            200,
            100,
            0,
            now_ms,
            stale_window_ms,
            too_many_threshold,
        ));
    }

    #[test]
    fn rlpx_auto_priority_prefers_high_signal_entries() {
        let entries = vec![
            GatewayEthPluginRlpxPriorityAutoEntry {
                addr_hint: "a:30303".to_string(),
                tier_rank: 2,
                score: 5,
                learning_score: 1,
                priority_signal: 1,
                sessions_with_gossip: 0,
                total_new_pooled_hashes: 0,
                total_unique_new_pooled_hashes: 0,
                recent_unique_new_pooled_hashes_total: 0,
                recent_duplicate_new_pooled_hashes_total: 0,
                total_pooled_txs_received: 0,
                total_unique_pooled_txs: 0,
                recent_unique_pooled_txs_total: 0,
                recent_duplicate_pooled_txs_total: 0,
                total_pooled_txs_imported: 0,
                first_seen_hash_count: 0,
                first_seen_tx_count: 0,
            },
            GatewayEthPluginRlpxPriorityAutoEntry {
                addr_hint: "b:30303".to_string(),
                tier_rank: 1,
                score: 50,
                learning_score: 100,
                priority_signal: 12_000,
                sessions_with_gossip: 1,
                total_new_pooled_hashes: 100,
                total_unique_new_pooled_hashes: 70,
                recent_unique_new_pooled_hashes_total: 30,
                recent_duplicate_new_pooled_hashes_total: 10,
                total_pooled_txs_received: 20,
                total_unique_pooled_txs: 15,
                recent_unique_pooled_txs_total: 8,
                recent_duplicate_pooled_txs_total: 3,
                total_pooled_txs_imported: 20,
                first_seen_hash_count: 12,
                first_seen_tx_count: 4,
            },
            GatewayEthPluginRlpxPriorityAutoEntry {
                addr_hint: "c:30303".to_string(),
                tier_rank: 0,
                score: 80,
                learning_score: 200,
                priority_signal: 24_000,
                sessions_with_gossip: 2,
                total_new_pooled_hashes: 200,
                total_unique_new_pooled_hashes: 150,
                recent_unique_new_pooled_hashes_total: 64,
                recent_duplicate_new_pooled_hashes_total: 12,
                total_pooled_txs_received: 40,
                total_unique_pooled_txs: 30,
                recent_unique_pooled_txs_total: 16,
                recent_duplicate_pooled_txs_total: 5,
                total_pooled_txs_imported: 40,
                first_seen_hash_count: 32,
                first_seen_tx_count: 10,
            },
        ];
        let picked = gateway_eth_plugin_rlpx_select_auto_priority_addr_hints(entries.as_slice(), 2);
        assert_eq!(picked.len(), 2);
        assert!(picked.contains("c:30303"));
        assert!(picked.contains("b:30303"));
        assert!(!picked.contains("a:30303"));
    }

    #[test]
    fn rlpx_auto_priority_prefers_unique_and_first_seen_over_duplicate_volume() {
        let entries = vec![
            GatewayEthPluginRlpxPriorityAutoEntry {
                addr_hint: "dup-heavy:30303".to_string(),
                tier_rank: 0,
                score: 90,
                learning_score: 5_000,
                priority_signal: 10_000,
                sessions_with_gossip: 3,
                total_new_pooled_hashes: 10_000,
                total_unique_new_pooled_hashes: 100,
                recent_unique_new_pooled_hashes_total: 4,
                recent_duplicate_new_pooled_hashes_total: 1_200,
                total_pooled_txs_received: 500,
                total_unique_pooled_txs: 10,
                recent_unique_pooled_txs_total: 0,
                recent_duplicate_pooled_txs_total: 300,
                total_pooled_txs_imported: 200,
                first_seen_hash_count: 3,
                first_seen_tx_count: 0,
            },
            GatewayEthPluginRlpxPriorityAutoEntry {
                addr_hint: "unique-first:30303".to_string(),
                tier_rank: 0,
                score: 60,
                learning_score: 2_000,
                priority_signal: 20_000,
                sessions_with_gossip: 2,
                total_new_pooled_hashes: 900,
                total_unique_new_pooled_hashes: 600,
                recent_unique_new_pooled_hashes_total: 120,
                recent_duplicate_new_pooled_hashes_total: 40,
                total_pooled_txs_received: 120,
                total_unique_pooled_txs: 80,
                recent_unique_pooled_txs_total: 24,
                recent_duplicate_pooled_txs_total: 8,
                total_pooled_txs_imported: 110,
                first_seen_hash_count: 80,
                first_seen_tx_count: 20,
            },
        ];
        let picked = gateway_eth_plugin_rlpx_select_auto_priority_addr_hints(entries.as_slice(), 1);
        assert_eq!(picked.len(), 1);
        assert!(picked.contains("unique-first:30303"));
    }

    #[test]
    fn rlpx_priority_budget_grows_when_core_is_short() {
        let chain_id = 9_901_002u64;
        let key = format!(
            "NOVOVM_GATEWAY_ETH_PLUGIN_MEMPOOL_INGEST_RLPX_PRIORITY_BUDGET_CHAIN_{}",
            chain_id
        );
        std::env::set_var(key.as_str(), "0");
        let budget = gateway_eth_plugin_mempool_ingest_rlpx_priority_budget(chain_id, 16, 1, 8);
        assert!(budget >= 1);
        std::env::remove_var(key.as_str());
    }

    #[test]
    fn rlpx_priority_auto_pool_defaults_to_core_target_when_unset() {
        let chain_id = 9_901_022u64;
        let auto_key = format!(
            "NOVOVM_GATEWAY_ETH_PLUGIN_MEMPOOL_INGEST_RLPX_PRIORITY_AUTO_POOL_SIZE_CHAIN_{}",
            chain_id
        );
        let core_key = format!(
            "NOVOVM_GATEWAY_ETH_PLUGIN_MEMPOOL_INGEST_RLPX_CORE_TARGET_CHAIN_{}",
            chain_id
        );
        std::env::remove_var(auto_key.as_str());
        std::env::remove_var(core_key.as_str());
        assert_eq!(
            gateway_eth_plugin_mempool_ingest_rlpx_priority_auto_pool_size(chain_id, 16),
            8
        );
        std::env::set_var(core_key.as_str(), "5");
        assert_eq!(
            gateway_eth_plugin_mempool_ingest_rlpx_priority_auto_pool_size(chain_id, 16),
            5
        );
        std::env::remove_var(auto_key.as_str());
        std::env::remove_var(core_key.as_str());
    }

    #[test]
    fn rlpx_priority_addr_hint_normalization_handles_enode_and_socket_addr() {
        let from_enode =
            gateway_eth_plugin_normalize_priority_addr_hint(GATEWAY_ETH_MAINNET_BOOTNODES[2])
                .expect("enode should normalize");
        assert_eq!(from_enode, "65.108.70.101:30303");

        let from_addr = gateway_eth_plugin_normalize_priority_addr_hint("157.90.35.166:30303")
            .expect("socket addr should normalize");
        assert_eq!(from_addr, "157.90.35.166:30303");
    }

    #[test]
    fn rlpx_priority_addr_hints_are_loaded_from_chain_env() {
        let chain_id = 9_901_001u64;
        let key = format!(
            "NOVOVM_GATEWAY_ETH_PLUGIN_MEMPOOL_INGEST_RLPX_PRIORITY_ADDR_HINTS_CHAIN_{}",
            chain_id
        );
        std::env::set_var(
            key.as_str(),
            format!(
                "157.90.35.166:30303; {}; [::1]:30303",
                GATEWAY_ETH_MAINNET_BOOTNODES[2]
            ),
        );
        let parsed = gateway_eth_plugin_mempool_ingest_rlpx_priority_addr_hints(chain_id);
        assert!(parsed.contains("157.90.35.166:30303"));
        assert!(parsed.contains("65.108.70.101:30303"));
        assert!(parsed.contains("[::1]:30303"));
        std::env::remove_var(key.as_str());
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
