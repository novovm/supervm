// Copyright (c) 2026 Xonovo Technology
// All rights reserved.
// Author: Xonovo Technology

use anyhow::{bail, Context, Result};
use ed25519_dalek::SigningKey;
use novovm_adapter_api::{
    default_chain_id, ChainConfig, ChainType, SerializationFormat, StateIR, TxIR, TxType,
};
use novovm_adapter_novovm::{create_native_adapter, supports_native_chain};
use novovm_consensus::{BFTEngine, BFTConfig, ValidatorSet};
use novovm_exec::{AoemExecOutput, AoemRuntimeConfig, ExecOpV2};
use novovm_network::{InMemoryTransport, Transport, UdpTransport};
use novovm_protocol::{
    decode_block_header_wire_v1, encode_block_header_wire_v1, BlockHeaderWireV1,
    BLOCK_HEADER_WIRE_V1_CODEC, LOCAL_PLUGIN_CLASS_CODE,
    decode_local_tx_wire_v1 as decode_tx_wire_v1, encode_local_tx_wire_v1 as encode_tx_wire_v1,
    plugin_class_name as protocol_plugin_class_name, verify_consensus_plugin_binding,
    ConsensusPluginBindingV1, CONSENSUS_PLUGIN_CLASS_CODE,
    protocol_catalog::distributed_occc::gossip::{
        GossipMessage as DistributedGossipMessage, MessageType as DistributedMessageType,
    },
    GossipMessage as ProtocolGossipMessage, LocalTxWireV1, NodeId, ProtocolMessage, ShardId,
    LOCAL_TX_WIRE_V1_CODEC,
};
use rand::rngs::OsRng;
use serde::Deserialize;
use sha2::{Digest, Sha256};
use std::collections::{HashMap, HashSet};
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::{Mutex, OnceLock};
use std::time::{Duration, Instant};

fn exec_path_mode() -> String {
    std::env::var("NOVOVM_EXEC_PATH")
        .or_else(|_| std::env::var("SUPERVM_EXEC_PATH"))
        .unwrap_or_else(|_| "ffi_v2".to_string())
}

fn bool_env(name: &str) -> bool {
    std::env::var(name)
        .map(|v| {
            let v = v.trim();
            v == "1"
                || v.eq_ignore_ascii_case("true")
                || v.eq_ignore_ascii_case("on")
                || v.eq_ignore_ascii_case("yes")
        })
        .unwrap_or(false)
}

fn u32_env(name: &str, default: u32) -> u32 {
    std::env::var(name)
        .ok()
        .and_then(|s| s.trim().parse::<u32>().ok())
        .filter(|v| *v > 0)
        .unwrap_or(default)
}

fn u64_env(name: &str, default: u64) -> u64 {
    std::env::var(name)
        .ok()
        .and_then(|s| s.trim().parse::<u64>().ok())
        .filter(|v| *v > 0)
        .unwrap_or(default)
}

fn string_env(name: &str, default: &str) -> String {
    std::env::var(name).unwrap_or_else(|_| default.to_string())
}

fn parse_u32_env(name: &str, default: u32) -> Result<u32> {
    match std::env::var(name) {
        Ok(raw) => {
            let trimmed = raw.trim();
            if trimmed.is_empty() {
                return Ok(default);
            }
            let parsed = trimmed
                .parse::<u32>()
                .with_context(|| format!("invalid {} value: {}", name, raw))?;
            if parsed == 0 {
                bail!("{} must be > 0", name);
            }
            Ok(parsed)
        }
        Err(_) => Ok(default),
    }
}

fn parse_u64_mask_env(name: &str, default: u64) -> Result<u64> {
    match std::env::var(name) {
        Ok(raw) => {
            let trimmed = raw.trim();
            if trimmed.is_empty() {
                return Ok(default);
            }
            if let Some(hex) = trimmed
                .strip_prefix("0x")
                .or_else(|| trimmed.strip_prefix("0X"))
            {
                let parsed = u64::from_str_radix(hex, 16)
                    .with_context(|| format!("invalid {} hex mask: {}", name, raw))?;
                return Ok(parsed);
            }
            let parsed = trimmed
                .parse::<u64>()
                .with_context(|| format!("invalid {} mask: {}", name, raw))?;
            Ok(parsed)
        }
        Err(_) => Ok(default),
    }
}

fn parse_u64_mask_str(raw: &str, field: &str) -> Result<u64> {
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        bail!("{} cannot be empty", field);
    }
    if let Some(hex) = trimmed
        .strip_prefix("0x")
        .or_else(|| trimmed.strip_prefix("0X"))
    {
        let parsed = u64::from_str_radix(hex, 16)
            .with_context(|| format!("invalid {} hex mask: {}", field, raw))?;
        return Ok(parsed);
    }
    let parsed = trimmed
        .parse::<u64>()
        .with_context(|| format!("invalid {} mask: {}", field, raw))?;
    Ok(parsed)
}

fn normalize_sha256_hex(raw: &str, field: &str) -> Result<String> {
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        bail!("{} cannot be empty", field);
    }
    let normalized = trimmed
        .strip_prefix("0x")
        .or_else(|| trimmed.strip_prefix("0X"))
        .unwrap_or(trimmed)
        .to_ascii_lowercase();
    if normalized.len() != 64 || !normalized.bytes().all(|b| b.is_ascii_hexdigit()) {
        bail!("{} must be 64 hex chars (sha256)", field);
    }
    Ok(normalized)
}

fn parse_sha256_env(name: &str) -> Result<Option<String>> {
    match std::env::var(name) {
        Ok(raw) => {
            let trimmed = raw.trim();
            if trimmed.is_empty() {
                Ok(None)
            } else {
                Ok(Some(normalize_sha256_hex(trimmed, name)?))
            }
        }
        Err(_) => Ok(None),
    }
}

fn parse_network_probe_peers(node_id: u64, fallback_peer_addr: &str) -> Result<Vec<(NodeId, String)>> {
    if let Ok(spec) = std::env::var("NOVOVM_NET_PEERS") {
        let spec = spec.trim();
        if !spec.is_empty() {
            let mut out = Vec::new();
            for item in spec.split(',') {
                let item = item.trim();
                if item.is_empty() {
                    continue;
                }
                let (id_raw, addr_raw) = item
                    .split_once('@')
                    .ok_or_else(|| anyhow::anyhow!("invalid NOVOVM_NET_PEERS item: {item} (expected id@addr)"))?;
                let peer_id = id_raw
                    .trim()
                    .parse::<u64>()
                    .with_context(|| format!("invalid peer id in NOVOVM_NET_PEERS: {id_raw}"))?;
                let addr = addr_raw.trim();
                if addr.is_empty() {
                    bail!("empty peer addr in NOVOVM_NET_PEERS item: {item}");
                }
                if peer_id == node_id {
                    continue;
                }
                out.push((NodeId(peer_id), addr.to_string()));
            }
            out.sort_by_key(|(id, _)| id.0);
            out.dedup_by_key(|(id, _)| id.0);
            if out.is_empty() {
                bail!("NOVOVM_NET_PEERS resolved to zero peers for node {}", node_id);
            }
            return Ok(out);
        }
    }

    let fallback_peer_id = if node_id == 0 { 1 } else { 0 };
    Ok(vec![(NodeId(fallback_peer_id), fallback_peer_addr.to_string())])
}

fn join_ids(ids: &[u64]) -> String {
    if ids.is_empty() {
        return "-".to_string();
    }
    ids.iter()
        .map(|id| id.to_string())
        .collect::<Vec<_>>()
        .join(",")
}

fn network_probe_consensus_binding() -> ConsensusPluginBindingV1 {
    let mut hasher = Sha256::new();
    hasher.update(ADAPTER_NATIVE_RULESET_ID.as_bytes());
    hasher.update(b":network_probe_binding_v1");
    ConsensusPluginBindingV1 {
        plugin_class_code: PLUGIN_CLASS_CONSENSUS,
        adapter_hash: hasher.finalize().into(),
    }
}

fn build_network_probe_block_wire_payload(
    from: NodeId,
    to: NodeId,
    consensus_binding: ConsensusPluginBindingV1,
    tamper_mode: &str,
) -> Vec<u8> {
    let mut parent_hash = [0u8; 32];
    parent_hash[..8].copy_from_slice(&from.0.to_le_bytes());
    parent_hash[8..16].copy_from_slice(&to.0.to_le_bytes());
    let mut root_hasher = Sha256::new();
    root_hasher.update(from.0.to_le_bytes());
    root_hasher.update(to.0.to_le_bytes());
    root_hasher.update(consensus_binding.adapter_hash);
    let state_root: [u8; 32] = root_hasher.finalize().into();
    let mut header = BlockHeaderWireV1 {
        height: from.0.saturating_add(1),
        epoch_id: 1,
        parent_hash,
        state_root,
        tx_count: 1,
        batch_count: 1,
        consensus_binding,
    };
    match tamper_mode {
        "class_mismatch" => {
            header.consensus_binding.plugin_class_code = if header
                .consensus_binding
                .plugin_class_code
                == CONSENSUS_PLUGIN_CLASS_CODE
            {
                LOCAL_PLUGIN_CLASS_CODE
            } else {
                CONSENSUS_PLUGIN_CLASS_CODE
            };
        }
        "hash_mismatch" => {
            header.consensus_binding.adapter_hash[0] ^= 0x80;
        }
        _ => {}
    }

    let mut wire = encode_block_header_wire_v1(&header);
    if tamper_mode == "codec_corrupt" && !wire.is_empty() {
        wire[0] ^= 0xff;
    }
    wire
}

#[derive(Debug, Clone)]
struct LocalTx {
    account: u64,
    key: u64,
    value: u64,
    nonce: u64,
    fee: u64,
    signature: [u8; 32],
}

#[derive(Debug, Clone, Copy)]
struct TxMetaSummary {
    accounts: usize,
    min_fee: u64,
    max_fee: u64,
    nonce_ok: bool,
    sig_ok: bool,
}

#[derive(Debug, Clone, Copy)]
struct TxCodecSummary {
    encoded: usize,
    decoded: usize,
    total_bytes: usize,
    pass: bool,
}

#[derive(Debug, Clone, Copy)]
struct MempoolAdmissionSummary {
    accepted: usize,
    rejected: usize,
    fee_floor: u64,
    nonce_ok: bool,
    sig_ok: bool,
}

#[derive(Debug, Clone, Copy)]
struct AdapterSignalSummary {
    backend: &'static str,
    chain: &'static str,
    txs: usize,
    verified: bool,
    applied: bool,
    accounts: usize,
    state_root: [u8; 32],
    plugin_abi_enabled: bool,
    plugin_abi_version: u32,
    plugin_abi_expected: u32,
    plugin_capabilities: u64,
    plugin_required_capabilities: u64,
    plugin_abi_compatible: bool,
    plugin_registry_enabled: bool,
    plugin_registry_strict: bool,
    plugin_registry_matched: bool,
    plugin_registry_chain_allowed: bool,
    plugin_registry_entry_abi: u32,
    plugin_registry_entry_required_caps: u64,
    plugin_registry_hash_check_enabled: bool,
    plugin_registry_hash_match: bool,
    plugin_registry_whitelist_present: bool,
    plugin_registry_whitelist_match: bool,
    plugin_class_code: u8,
    plugin_class: &'static str,
    consensus_adapter_hash: [u8; 32],
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum AdapterBackendMode {
    Auto,
    Native,
    Plugin,
}

#[repr(C)]
#[derive(Debug, Clone, Copy, Default)]
struct PluginApplyResultV1 {
    verified: u8,
    applied: u8,
    txs: u64,
    accounts: u64,
    state_root: [u8; 32],
    error_code: i32,
}

type PluginVersionFn = unsafe extern "C" fn() -> u32;
type PluginCapabilitiesFn = unsafe extern "C" fn() -> u64;
type PluginApplyFn = unsafe extern "C" fn(
    chain_type_code: u32,
    chain_id: u64,
    tx_ir_ptr: *const u8,
    tx_ir_len: usize,
    out_result: *mut PluginApplyResultV1,
) -> i32;

const ADAPTER_PLUGIN_EXPECTED_ABI_DEFAULT: u32 = 1;
const ADAPTER_PLUGIN_REQUIRED_CAPS_DEFAULT: u64 = 0x1;
const ADAPTER_PLUGIN_REGISTRY_PATH_DEFAULT: &str = "..\\..\\config\\novovm-adapter-plugin-registry.json";
const ADAPTER_PLUGIN_REGISTRY_PATH_ALT: &str = "config\\novovm-adapter-plugin-registry.json";
const PLUGIN_CLASS_CONSENSUS: u8 = CONSENSUS_PLUGIN_CLASS_CODE;
const ADAPTER_NATIVE_RULESET_ID: &str = "novovm_adapter_native_ruleset_v1";

#[derive(Debug, Clone, Deserialize)]
struct AdapterPluginRegistryFile {
    version: Option<String>,
    allowed_abi_versions: Option<Vec<u32>>,
    plugins: Vec<AdapterPluginRegistryEntry>,
}

#[derive(Debug, Clone, Deserialize)]
struct AdapterPluginRegistryEntry {
    name: String,
    path: String,
    abi: u32,
    required_caps: String,
    chains: Vec<String>,
    enabled: Option<bool>,
}

#[derive(Debug, Clone, Copy)]
struct AdapterPluginRegistrySummary {
    enabled: bool,
    strict: bool,
    matched: bool,
    chain_allowed: bool,
    entry_abi: u32,
    entry_required_caps: u64,
    hash_check_enabled: bool,
    hash_match: bool,
    whitelist_present: bool,
    whitelist_match: bool,
}

#[derive(Debug, Clone)]
struct LocalBatch {
    id: u64,
    txs: Vec<LocalTx>,
    mapped_ops: u32,
}

#[derive(Debug, Clone)]
struct LocalBlockHeader {
    height: u64,
    epoch_id: u64,
    parent_hash: [u8; 32],
    state_root: [u8; 32],
    tx_count: u64,
    batch_count: u32,
    consensus_binding: ConsensusPluginBindingV1,
}

#[derive(Debug, Clone)]
struct LocalBlock {
    header: LocalBlockHeader,
    batches: Vec<LocalBatch>,
    proposal_hash: [u8; 32],
    block_hash: [u8; 32],
}

#[derive(Debug, Clone)]
struct BatchAClosureOutput {
    epoch_id: u64,
    height: u64,
    txs: u64,
    state_root: [u8; 32],
    proposal_hash: [u8; 32],
    consensus_binding: ConsensusPluginBindingV1,
}

#[derive(Debug, Clone)]
struct NetworkSignal {
    transport: &'static str,
    from: u64,
    to: u64,
    nodes: u64,
    sent: u64,
    received: u64,
    msg_kind: &'static str,
    discovery: bool,
    gossip: bool,
    sync: bool,
}

#[derive(Debug, Default)]
struct InMemoryBlockStore {
    blocks: Vec<LocalBlock>,
}

impl InMemoryBlockStore {
    fn commit_block(&mut self, block: LocalBlock) -> Result<()> {
        if let Some(prev) = self.blocks.last() {
            let expected_height = prev.header.height.saturating_add(1);
            if block.header.height != expected_height {
                bail!(
                    "block height not monotonic: expected {}, got {}",
                    expected_height,
                    block.header.height
                );
            }
            if block.header.parent_hash != prev.block_hash {
                bail!("block parent hash mismatch");
            }
        } else if block.header.height != 0 {
            bail!("genesis block height must be 0");
        } else if block.header.parent_hash != [0u8; 32] {
            bail!("genesis parent hash must be zero");
        }

        self.blocks.push(block);
        Ok(())
    }

    fn latest(&self) -> Option<&LocalBlock> {
        self.blocks.last()
    }

    fn total_blocks(&self) -> usize {
        self.blocks.len()
    }
}

static BLOCK_STORE: OnceLock<Mutex<InMemoryBlockStore>> = OnceLock::new();

fn global_block_store() -> &'static Mutex<InMemoryBlockStore> {
    BLOCK_STORE.get_or_init(|| Mutex::new(InMemoryBlockStore::default()))
}

struct ExecBatchBuffer {
    _keys: Vec<[u8; 8]>,
    _values: Vec<[u8; 8]>,
    ops: Vec<ExecOpV2>,
}

const LOCAL_TX_SIG_DOMAIN: &[u8] = b"novovm_local_tx_v1";

fn compute_local_tx_signature_parts(
    account: u64,
    key: u64,
    value: u64,
    nonce: u64,
    fee: u64,
) -> [u8; 32] {
    let mut hasher = Sha256::new();
    hasher.update(LOCAL_TX_SIG_DOMAIN);
    hasher.update(account.to_le_bytes());
    hasher.update(key.to_le_bytes());
    hasher.update(value.to_le_bytes());
    hasher.update(nonce.to_le_bytes());
    hasher.update(fee.to_le_bytes());
    hasher.finalize().into()
}

fn build_local_tx(account: u64, key: u64, value: u64, nonce: u64, fee: u64) -> LocalTx {
    LocalTx {
        account,
        key,
        value,
        nonce,
        fee,
        signature: compute_local_tx_signature_parts(account, key, value, nonce, fee),
    }
}

fn verify_local_tx_signature(tx: &LocalTx) -> bool {
    tx.signature
        == compute_local_tx_signature_parts(tx.account, tx.key, tx.value, tx.nonce, tx.fee)
}

fn to_tx_wire_v1(tx: &LocalTx) -> LocalTxWireV1 {
    LocalTxWireV1 {
        account: tx.account,
        key: tx.key,
        value: tx.value,
        nonce: tx.nonce,
        fee: tx.fee,
        signature: tx.signature,
    }
}

fn from_tx_wire_v1(wire: &LocalTxWireV1) -> LocalTx {
    LocalTx {
        account: wire.account,
        key: wire.key,
        value: wire.value,
        nonce: wire.nonce,
        fee: wire.fee,
        signature: wire.signature,
    }
}

fn roundtrip_local_tx_codec_v1(txs: &[LocalTx]) -> Result<(Vec<LocalTx>, TxCodecSummary)> {
    if txs.is_empty() {
        bail!("tx wire roundtrip requires at least one tx");
    }

    let mut encoded_total = 0usize;
    let mut decoded = Vec::with_capacity(txs.len());
    let mut pass = true;
    for tx in txs {
        let wire = encode_tx_wire_v1(&to_tx_wire_v1(tx));
        encoded_total = encoded_total.saturating_add(wire.len());
        let decoded_wire = decode_tx_wire_v1(&wire).context("decode tx wire failed")?;
        let decoded_tx = from_tx_wire_v1(&decoded_wire);
        if decoded_tx.account != tx.account
            || decoded_tx.key != tx.key
            || decoded_tx.value != tx.value
            || decoded_tx.nonce != tx.nonce
            || decoded_tx.fee != tx.fee
            || decoded_tx.signature != tx.signature
        {
            pass = false;
        }
        decoded.push(decoded_tx);
    }

    Ok((
        decoded,
        TxCodecSummary {
            encoded: txs.len(),
            decoded: txs.len(),
            total_bytes: encoded_total,
            pass,
        },
    ))
}

fn admit_mempool_basic(
    txs: &[LocalTx],
    fee_floor: u64,
) -> Result<(Vec<LocalTx>, MempoolAdmissionSummary)> {
    if txs.is_empty() {
        bail!("mempool admission requires at least one tx");
    }

    let mut accepted = Vec::with_capacity(txs.len());
    let mut rejected = 0usize;
    let mut nonce_ok = true;
    let mut sig_ok = true;
    let mut next_nonce_by_account: HashMap<u64, u64> = HashMap::new();

    for tx in txs {
        if tx.fee < fee_floor {
            rejected = rejected.saturating_add(1);
            continue;
        }
        if !verify_local_tx_signature(tx) {
            sig_ok = false;
            rejected = rejected.saturating_add(1);
            continue;
        }
        let expected_nonce = next_nonce_by_account.entry(tx.account).or_insert(0);
        if tx.nonce != *expected_nonce {
            nonce_ok = false;
            rejected = rejected.saturating_add(1);
            continue;
        }
        *expected_nonce = expected_nonce.saturating_add(1);
        accepted.push(tx.clone());
    }

    if accepted.is_empty() {
        bail!("mempool rejected all transactions");
    }

    Ok((
        accepted.clone(),
        MempoolAdmissionSummary {
            accepted: accepted.len(),
            rejected,
            fee_floor,
            nonce_ok,
            sig_ok,
        },
    ))
}

fn validate_and_summarize_txs(txs: &[LocalTx]) -> Result<TxMetaSummary> {
    if txs.is_empty() {
        bail!("tx set cannot be empty");
    }

    let mut next_nonce_by_account: HashMap<u64, u64> = HashMap::new();
    let mut accounts: HashSet<u64> = HashSet::new();
    let mut min_fee = u64::MAX;
    let mut max_fee = 0u64;

    for tx in txs {
        if tx.fee == 0 {
            bail!("tx fee must be > 0 (account={}, nonce={})", tx.account, tx.nonce);
        }
        if !verify_local_tx_signature(tx) {
            bail!(
                "tx signature invalid (account={}, nonce={})",
                tx.account,
                tx.nonce
            );
        }
        let expected_nonce = next_nonce_by_account.entry(tx.account).or_insert(0);
        if tx.nonce != *expected_nonce {
            bail!(
                "nonce sequence invalid for account {}: expected {}, got {}",
                tx.account,
                *expected_nonce,
                tx.nonce
            );
        }
        *expected_nonce = expected_nonce.saturating_add(1);
        accounts.insert(tx.account);
        min_fee = min_fee.min(tx.fee);
        max_fee = max_fee.max(tx.fee);
    }

    Ok(TxMetaSummary {
        accounts: accounts.len(),
        min_fee,
        max_fee,
        nonce_ok: true,
        sig_ok: true,
    })
}

fn encode_adapter_address(seed: u64) -> Vec<u8> {
    let mut out = vec![0u8; 20];
    out[12..20].copy_from_slice(&seed.to_be_bytes());
    out
}

fn to_adapter_tx_ir(tx: &LocalTx, chain_id: u64) -> TxIR {
    let mut ir = TxIR {
        hash: Vec::new(),
        from: encode_adapter_address(tx.account),
        to: Some(encode_adapter_address(tx.key)),
        value: tx.value as u128,
        gas_limit: 21_000,
        gas_price: tx.fee,
        nonce: tx.nonce,
        data: Vec::new(),
        signature: tx.signature.to_vec(),
        chain_id,
        tx_type: TxType::Transfer,
        source_chain: None,
        target_chain: None,
    };
    ir.compute_hash();
    ir
}

fn resolve_adapter_chain() -> Result<ChainType> {
    let chain = string_env("NOVOVM_ADAPTER_CHAIN", "novovm");
    ChainType::parse(chain.trim())
}

fn resolve_adapter_backend_mode() -> Result<AdapterBackendMode> {
    let backend = string_env("NOVOVM_ADAPTER_BACKEND", "auto").to_ascii_lowercase();
    match backend.as_str() {
        "auto" => Ok(AdapterBackendMode::Auto),
        "native" => Ok(AdapterBackendMode::Native),
        "plugin" => Ok(AdapterBackendMode::Plugin),
        _ => bail!(
            "invalid NOVOVM_ADAPTER_BACKEND={}, valid: auto|native|plugin",
            backend
        ),
    }
}

fn resolve_adapter_plugin_path() -> Option<PathBuf> {
    std::env::var("NOVOVM_ADAPTER_PLUGIN_PATH")
        .ok()
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
        .map(PathBuf::from)
}

fn resolve_adapter_plugin_requirements() -> Result<(u32, u64)> {
    let expected_abi = parse_u32_env(
        "NOVOVM_ADAPTER_PLUGIN_EXPECT_ABI",
        ADAPTER_PLUGIN_EXPECTED_ABI_DEFAULT,
    )?;
    let required_caps = parse_u64_mask_env(
        "NOVOVM_ADAPTER_PLUGIN_REQUIRE_CAPS",
        ADAPTER_PLUGIN_REQUIRED_CAPS_DEFAULT,
    )?;
    Ok((expected_abi, required_caps))
}

fn plugin_class_name(code: u8) -> &'static str {
    protocol_plugin_class_name(code)
}

fn compute_consensus_adapter_hash(
    backend: &'static str,
    chain: ChainType,
    expected_abi: u32,
    required_caps: u64,
    plugin_path: Option<&Path>,
) -> Result<[u8; 32]> {
    let mut hasher = Sha256::new();
    hasher.update(b"novovm_consensus_adapter_hash_v1");
    hasher.update(backend.as_bytes());
    hasher.update(chain.as_str().as_bytes());
    hasher.update(expected_abi.to_le_bytes());
    hasher.update(required_caps.to_le_bytes());

    if backend == "plugin" {
        let path = plugin_path.ok_or_else(|| anyhow::anyhow!("plugin backend requires plugin path"))?;
        let plugin_bytes = fs::read(path)
            .with_context(|| format!("read adapter plugin bytes failed: {}", path.display()))?;
        let plugin_bin_hash = Sha256::digest(plugin_bytes);
        hasher.update(plugin_bin_hash);
    } else {
        hasher.update(ADAPTER_NATIVE_RULESET_ID.as_bytes());
    }

    Ok(hasher.finalize().into())
}

fn resolve_adapter_plugin_registry_path() -> Option<PathBuf> {
    if let Ok(raw) = std::env::var("NOVOVM_ADAPTER_PLUGIN_REGISTRY_PATH") {
        let trimmed = raw.trim();
        if !trimmed.is_empty() {
            return Some(PathBuf::from(trimmed));
        }
    }
    let default = PathBuf::from(ADAPTER_PLUGIN_REGISTRY_PATH_DEFAULT);
    if default.exists() {
        return Some(default);
    }
    let alt = PathBuf::from(ADAPTER_PLUGIN_REGISTRY_PATH_ALT);
    if alt.exists() {
        return Some(alt);
    }
    None
}

fn resolve_adapter_plugin_registry_strict() -> bool {
    bool_env("NOVOVM_ADAPTER_PLUGIN_REGISTRY_STRICT")
}

fn resolve_adapter_plugin_registry_expected_sha256() -> Result<Option<String>> {
    parse_sha256_env("NOVOVM_ADAPTER_PLUGIN_REGISTRY_SHA256")
}

fn load_adapter_plugin_registry(path: &Path) -> Result<(AdapterPluginRegistryFile, String)> {
    let bytes = fs::read(path)
        .with_context(|| format!("read plugin registry failed: {}", path.display()))?;
    let registry_sha256 = to_hex(&Sha256::digest(&bytes));
    let text = String::from_utf8(bytes)
        .with_context(|| format!("plugin registry is not valid utf-8: {}", path.display()))?;
    let parsed: AdapterPluginRegistryFile = serde_json::from_str(&text)
        .with_context(|| format!("parse plugin registry json failed: {}", path.display()))?;
    if parsed.plugins.is_empty() {
        bail!("adapter plugin registry has zero plugins: {}", path.display());
    }
    Ok((parsed, registry_sha256))
}

fn path_matches_registry_entry(entry_path: &str, actual_path: &Path) -> bool {
    let entry_trimmed = entry_path.trim();
    if entry_trimmed.is_empty() {
        return false;
    }
    let entry = Path::new(entry_trimmed);
    if entry.is_absolute() {
        if let (Ok(entry_canon), Ok(actual_canon)) = (entry.canonicalize(), actual_path.canonicalize()) {
            return entry_canon == actual_canon;
        }
        return entry == actual_path;
    }
    if let Some(entry_name) = entry.file_name() {
        if let Some(actual_name) = actual_path.file_name() {
            return entry_name
                .to_string_lossy()
                .eq_ignore_ascii_case(&actual_name.to_string_lossy());
        }
    }
    false
}

fn chain_allowed_in_registry(entry: &AdapterPluginRegistryEntry, chain: ChainType) -> bool {
    let chain_name = chain.as_str();
    entry
        .chains
        .iter()
        .any(|c| c.trim().eq_ignore_ascii_case(chain_name))
}

fn resolve_adapter_plugin_registry_summary(
    chain: ChainType,
    plugin_path: Option<&Path>,
    expected_abi: u32,
    required_caps: u64,
) -> Result<AdapterPluginRegistrySummary> {
    let strict = resolve_adapter_plugin_registry_strict();
    let expected_registry_hash = resolve_adapter_plugin_registry_expected_sha256()?;
    let hash_check_enabled = expected_registry_hash.is_some();
    let registry_path = resolve_adapter_plugin_registry_path();
    if registry_path.is_none() {
        if strict && plugin_path.is_some() {
            bail!("adapter plugin registry strict mode enabled but registry path is missing");
        }
        if strict && hash_check_enabled {
            bail!("adapter plugin registry strict mode enabled but registry hash check is configured without registry file");
        }
        return Ok(AdapterPluginRegistrySummary {
            enabled: false,
            strict,
            matched: true,
            chain_allowed: true,
            entry_abi: expected_abi,
            entry_required_caps: required_caps,
            hash_check_enabled,
            hash_match: !hash_check_enabled,
            whitelist_present: false,
            whitelist_match: true,
        });
    }

    let path = registry_path.unwrap();
    let (registry, actual_registry_sha256) = load_adapter_plugin_registry(&path)?;
    let hash_match = expected_registry_hash
        .as_ref()
        .map(|expected| expected == &actual_registry_sha256)
        .unwrap_or(true);
    if strict && !hash_match {
        let expected = expected_registry_hash.as_deref().unwrap_or("-");
        bail!(
            "adapter plugin registry hash mismatch: expected={} actual={} path={}",
            expected,
            actual_registry_sha256,
            path.display()
        );
    }
    let whitelist = registry.allowed_abi_versions.clone().unwrap_or_default();
    let whitelist_present = !whitelist.is_empty();
    let whitelist_match = if whitelist_present {
        whitelist.iter().any(|v| *v == expected_abi)
    } else {
        true
    };
    if strict && !whitelist_match {
        bail!(
            "adapter plugin registry abi whitelist mismatch: expected_abi={} allowed={:?} path={}",
            expected_abi,
            whitelist,
            path.display()
        );
    }
    let _registry_version = registry.version.clone().unwrap_or_default();
    if plugin_path.is_none() {
        return Ok(AdapterPluginRegistrySummary {
            enabled: true,
            strict,
            matched: true,
            chain_allowed: true,
            entry_abi: expected_abi,
            entry_required_caps: required_caps,
            hash_check_enabled,
            hash_match,
            whitelist_present,
            whitelist_match,
        });
    }

    let plugin_path = plugin_path.unwrap();
    let mut matched_entry: Option<&AdapterPluginRegistryEntry> = None;
    for entry in &registry.plugins {
        if entry.enabled == Some(false) {
            continue;
        }
        if path_matches_registry_entry(&entry.path, plugin_path) {
            matched_entry = Some(entry);
            break;
        }
    }

    let entry = if let Some(v) = matched_entry {
        v
    } else {
        if strict {
            bail!(
                "adapter plugin registry mismatch: plugin path not registered ({})",
                plugin_path.display()
            );
        }
        return Ok(AdapterPluginRegistrySummary {
            enabled: true,
            strict,
            matched: false,
            chain_allowed: false,
            entry_abi: 0,
            entry_required_caps: 0,
            hash_check_enabled,
            hash_match,
            whitelist_present,
            whitelist_match,
        });
    };

    let chain_allowed = chain_allowed_in_registry(entry, chain);
    let entry_required_caps = parse_u64_mask_str(&entry.required_caps, "registry.required_caps")?;
    let abi_match = entry.abi == expected_abi;
    let caps_match = entry_required_caps == required_caps;
    let matched = chain_allowed && abi_match && caps_match;
    if strict && !matched {
        bail!(
            "adapter plugin registry mismatch: name={} chain_allowed={} abi(entry={}, expected={}) caps(entry=0x{:x}, expected=0x{:x})",
            entry.name,
            chain_allowed,
            entry.abi,
            expected_abi,
            entry_required_caps,
            required_caps
        );
    }

    Ok(AdapterPluginRegistrySummary {
        enabled: true,
        strict,
        matched,
        chain_allowed,
        entry_abi: entry.abi,
        entry_required_caps,
        hash_check_enabled,
        hash_match,
        whitelist_present,
        whitelist_match,
    })
}

fn chain_type_to_plugin_code(chain: ChainType) -> Option<u32> {
    match chain {
        ChainType::NovoVM => Some(0),
        ChainType::Custom => Some(13),
        _ => None,
    }
}

fn normalize_root32(root: &[u8]) -> [u8; 32] {
    if root.len() == 32 {
        let mut out = [0u8; 32];
        out.copy_from_slice(root);
        return out;
    }
    let mut hasher = Sha256::new();
    hasher.update(root);
    hasher.finalize().into()
}

fn run_native_adapter_signal(
    chain: ChainType,
    chain_id: u64,
    tx_irs: &[TxIR],
    expected_abi: u32,
    required_caps: u64,
    registry: AdapterPluginRegistrySummary,
) -> Result<AdapterSignalSummary> {
    let config = ChainConfig {
        chain_type: chain,
        chain_id: chain_id,
        name: format!("{}-native", chain.as_str()),
        enabled: true,
        custom_config: None,
    };
    let mut adapter = create_native_adapter(config)?;
    adapter.initialize()?;
    let mut state = StateIR::new();
    let mut verified = true;
    let mut applied = true;

    for ir in tx_irs {
        let raw = ir
            .serialize(SerializationFormat::Bincode)
            .context("adapter tx serialize failed")?;
        let parsed = adapter
            .parse_transaction(&raw)
            .context("adapter parse_transaction failed")?;
        let tx_ok = adapter
            .verify_transaction(&parsed)
            .context("adapter verify_transaction failed")?;
        verified = verified && tx_ok;
        if tx_ok {
            if let Err(e) = adapter.execute_transaction(&parsed, &mut state) {
                return Err(e).context("adapter execute_transaction failed");
            }
        } else {
            applied = false;
        }
    }

    let state_root_raw = adapter.state_root().context("adapter state_root failed")?;
    let state_root = normalize_root32(&state_root_raw);
    state.state_root = state_root.to_vec();
    adapter.shutdown().context("adapter shutdown failed")?;
    let consensus_adapter_hash =
        compute_consensus_adapter_hash("native", chain, expected_abi, required_caps, None)?;

    Ok(AdapterSignalSummary {
        backend: "native",
        chain: chain.as_str(),
        txs: tx_irs.len(),
        verified,
        applied,
        accounts: state.accounts.len(),
        state_root,
        plugin_abi_enabled: false,
        plugin_abi_version: 0,
        plugin_abi_expected: expected_abi,
        plugin_capabilities: 0,
        plugin_required_capabilities: required_caps,
        plugin_abi_compatible: true,
        plugin_registry_enabled: registry.enabled,
        plugin_registry_strict: registry.strict,
        plugin_registry_matched: registry.matched,
        plugin_registry_chain_allowed: registry.chain_allowed,
        plugin_registry_entry_abi: registry.entry_abi,
        plugin_registry_entry_required_caps: registry.entry_required_caps,
        plugin_registry_hash_check_enabled: registry.hash_check_enabled,
        plugin_registry_hash_match: registry.hash_match,
        plugin_registry_whitelist_present: registry.whitelist_present,
        plugin_registry_whitelist_match: registry.whitelist_match,
        plugin_class_code: PLUGIN_CLASS_CONSENSUS,
        plugin_class: plugin_class_name(PLUGIN_CLASS_CONSENSUS),
        consensus_adapter_hash,
    })
}

fn run_plugin_adapter_signal(
    chain: ChainType,
    chain_id: u64,
    tx_irs: &[TxIR],
    plugin_path: &Path,
    expected_abi: u32,
    required_caps: u64,
    registry: AdapterPluginRegistrySummary,
) -> Result<AdapterSignalSummary> {
    let chain_code = chain_type_to_plugin_code(chain)
        .ok_or_else(|| anyhow::anyhow!("plugin backend does not support chain={}", chain.as_str()))?;
    let tx_bytes = bincode::serialize(tx_irs).context("serialize tx_irs for plugin failed")?;

    let lib = unsafe { libloading::Library::new(plugin_path) }
        .with_context(|| format!("load adapter plugin failed: {}", plugin_path.display()))?;

    unsafe {
        let version_fn: libloading::Symbol<PluginVersionFn> = lib
            .get(b"novovm_adapter_plugin_version\0")
            .context("resolve novovm_adapter_plugin_version failed")?;
        let caps_fn: libloading::Symbol<PluginCapabilitiesFn> = lib
            .get(b"novovm_adapter_plugin_capabilities\0")
            .context("resolve novovm_adapter_plugin_capabilities failed")?;
        let apply_fn: libloading::Symbol<PluginApplyFn> = lib
            .get(b"novovm_adapter_plugin_apply_v1\0")
            .context("resolve novovm_adapter_plugin_apply_v1 failed")?;

        let abi_version = version_fn();
        if abi_version != expected_abi {
            bail!(
                "adapter plugin ABI version mismatch: expected {}, got {}",
                expected_abi,
                abi_version
            );
        }
        let caps = caps_fn();
        if caps & required_caps != required_caps {
            bail!(
                "adapter plugin capability mismatch: required=0x{:x}, got=0x{:x}",
                required_caps,
                caps
            );
        }

        let mut out = PluginApplyResultV1::default();
        let rc = apply_fn(
            chain_code,
            chain_id,
            tx_bytes.as_ptr(),
            tx_bytes.len(),
            &mut out as *mut PluginApplyResultV1,
        );
        if rc != 0 {
            bail!("adapter plugin apply failed: rc={}", rc);
        }
        if out.error_code != 0 {
            bail!("adapter plugin returned error_code={}", out.error_code);
        }
        let consensus_adapter_hash = compute_consensus_adapter_hash(
            "plugin",
            chain,
            expected_abi,
            required_caps,
            Some(plugin_path),
        )?;

        Ok(AdapterSignalSummary {
            backend: "plugin",
            chain: chain.as_str(),
            txs: out.txs as usize,
            verified: out.verified != 0,
            applied: out.applied != 0,
            accounts: out.accounts as usize,
            state_root: out.state_root,
            plugin_abi_enabled: true,
            plugin_abi_version: abi_version,
            plugin_abi_expected: expected_abi,
            plugin_capabilities: caps,
            plugin_required_capabilities: required_caps,
            plugin_abi_compatible: true,
            plugin_registry_enabled: registry.enabled,
            plugin_registry_strict: registry.strict,
            plugin_registry_matched: registry.matched,
            plugin_registry_chain_allowed: registry.chain_allowed,
            plugin_registry_entry_abi: registry.entry_abi,
            plugin_registry_entry_required_caps: registry.entry_required_caps,
            plugin_registry_hash_check_enabled: registry.hash_check_enabled,
            plugin_registry_hash_match: registry.hash_match,
            plugin_registry_whitelist_present: registry.whitelist_present,
            plugin_registry_whitelist_match: registry.whitelist_match,
            plugin_class_code: PLUGIN_CLASS_CONSENSUS,
            plugin_class: plugin_class_name(PLUGIN_CLASS_CONSENSUS),
            consensus_adapter_hash,
        })
    }
}

fn run_adapter_bridge_signal(txs: &[LocalTx]) -> Result<AdapterSignalSummary> {
    if txs.is_empty() {
        bail!("adapter bridge requires at least one tx");
    }

    let chain = resolve_adapter_chain()?;
    let chain_id = default_chain_id(chain);
    let backend_mode = resolve_adapter_backend_mode()?;
    let plugin_path = resolve_adapter_plugin_path();
    let (plugin_expected_abi, plugin_required_caps) = resolve_adapter_plugin_requirements()?;
    let native_registry = resolve_adapter_plugin_registry_summary(
        chain,
        None,
        plugin_expected_abi,
        plugin_required_caps,
    )?;
    let tx_irs: Vec<TxIR> = txs.iter().map(|tx| to_adapter_tx_ir(tx, chain_id)).collect();

    match backend_mode {
        AdapterBackendMode::Native => run_native_adapter_signal(
            chain,
            chain_id,
            &tx_irs,
            plugin_expected_abi,
            plugin_required_caps,
            native_registry,
        ),
        AdapterBackendMode::Plugin => {
            let path = plugin_path.as_deref().ok_or_else(|| {
                anyhow::anyhow!(
                    "NOVOVM_ADAPTER_BACKEND=plugin requires NOVOVM_ADAPTER_PLUGIN_PATH"
                )
            })?;
            let plugin_registry = resolve_adapter_plugin_registry_summary(
                chain,
                Some(path),
                plugin_expected_abi,
                plugin_required_caps,
            )?;
            run_plugin_adapter_signal(
                chain,
                chain_id,
                &tx_irs,
                path,
                plugin_expected_abi,
                plugin_required_caps,
                plugin_registry,
            )
        }
        AdapterBackendMode::Auto => {
            if supports_native_chain(chain) {
                match run_native_adapter_signal(
                    chain,
                    chain_id,
                    &tx_irs,
                    plugin_expected_abi,
                    plugin_required_caps,
                    native_registry,
                ) {
                    Ok(signal) => Ok(signal),
                    Err(native_err) => {
                        if let Some(path) = plugin_path.as_deref() {
                            let plugin_registry = resolve_adapter_plugin_registry_summary(
                                chain,
                                Some(path),
                                plugin_expected_abi,
                                plugin_required_caps,
                            )?;
                            eprintln!(
                                "adapter_warn: native backend failed ({}), fallback plugin={}",
                                native_err,
                                path.display()
                            );
                            run_plugin_adapter_signal(
                                chain,
                                chain_id,
                                &tx_irs,
                                path,
                                plugin_expected_abi,
                                plugin_required_caps,
                                plugin_registry,
                            )
                        } else {
                            Err(native_err)
                        }
                    }
                }
            } else if let Some(path) = plugin_path.as_deref() {
                let plugin_registry = resolve_adapter_plugin_registry_summary(
                    chain,
                    Some(path),
                    plugin_expected_abi,
                    plugin_required_caps,
                )?;
                run_plugin_adapter_signal(
                    chain,
                    chain_id,
                    &tx_irs,
                    path,
                    plugin_expected_abi,
                    plugin_required_caps,
                    plugin_registry,
                )
            } else {
                bail!(
                    "adapter backend auto cannot resolve chain={} without plugin path",
                    chain.as_str()
                )
            }
        }
    }
}

fn build_demo_txs() -> Vec<LocalTx> {
    let count = u32_env("NOVOVM_DEMO_TXS", 1);
    let account_count = u32_env("NOVOVM_DEMO_ACCOUNTS", 2).max(1);
    (0..count)
        .map(|i| {
            let account_idx = i % account_count;
            let account = 1000 + account_idx as u64;
            let nonce = (i / account_count) as u64;
            let fee = 1 + (i % 5) as u64;
            build_local_tx(account, 42 + i as u64, 7 + i as u64, nonce, fee)
        })
        .collect()
}

fn build_local_batches_from_txs(txs: &[LocalTx], requested_batches: usize) -> Vec<LocalBatch> {
    if txs.is_empty() {
        return Vec::new();
    }

    let batch_count = requested_batches.max(1).min(txs.len());
    let base = txs.len() / batch_count;
    let rem = txs.len() % batch_count;
    let mut out = Vec::with_capacity(batch_count);
    let mut cursor = 0usize;

    for i in 0..batch_count {
        let sz = base + usize::from(i < rem);
        let end = cursor + sz;
        let batch_txs = txs[cursor..end].to_vec();
        out.push(LocalBatch {
            id: (i + 1) as u64,
            mapped_ops: batch_txs.len() as u32,
            txs: batch_txs,
        });
        cursor = end;
    }

    out
}

fn batch_layout_summary(batches: &[LocalBatch]) -> String {
    if batches.is_empty() {
        return "-".to_string();
    }
    batches
        .iter()
        .map(|b| format!("{}:{}", b.id, b.txs.len()))
        .collect::<Vec<_>>()
        .join(",")
}

fn encode_ops_v2_buffer(txs: &[LocalTx]) -> ExecBatchBuffer {
    let mut keys = vec![[0u8; 8]; txs.len()];
    let mut values = vec![[0u8; 8]; txs.len()];
    let mut ops = Vec::with_capacity(txs.len());

    for (i, tx) in txs.iter().enumerate() {
        keys[i] = tx.key.to_le_bytes();
        values[i] = tx.value.to_le_bytes();
        ops.push(ExecOpV2 {
            opcode: 2,
            flags: 0,
            reserved: 0,
            key_ptr: keys[i].as_mut_ptr(),
            key_len: keys[i].len() as u32,
            value_ptr: values[i].as_mut_ptr(),
            value_len: values[i].len() as u32,
            delta: 0,
            expect_version: u64::MAX,
            plan_id: (tx.account << 32) | tx.nonce.saturating_add(1),
        });
    }

    ExecBatchBuffer {
        _keys: keys,
        _values: values,
        ops,
    }
}

fn build_proxy_state_root(out: &AoemExecOutput) -> [u8; 32] {
    let mut hasher = Sha256::new();
    hasher.update(out.result.processed.to_le_bytes());
    hasher.update(out.result.success.to_le_bytes());
    hasher.update(out.result.failed_index.to_le_bytes());
    hasher.update(out.result.total_writes.to_le_bytes());
    hasher.update(out.metrics.submitted_ops.to_le_bytes());
    hasher.update(out.metrics.return_code.to_le_bytes());
    hasher.finalize().into()
}

fn build_batch_proxy_state_root(out: &AoemExecOutput, batch: &LocalBatch) -> [u8; 32] {
    let base = build_proxy_state_root(out);
    let mut hasher = Sha256::new();
    hasher.update(base);
    hasher.update(batch.id.to_le_bytes());
    hasher.update((batch.txs.len() as u64).to_le_bytes());
    for tx in &batch.txs {
        hasher.update(hash_local_tx(tx));
    }
    hasher.finalize().into()
}

fn build_epoch_proxy_state_root(
    out: &AoemExecOutput,
    batches: &[LocalBatch],
    batch_results: &HashMap<u64, [u8; 32]>,
) -> Result<[u8; 32]> {
    let base = build_proxy_state_root(out);
    let mut hasher = Sha256::new();
    hasher.update(base);
    for batch in batches {
        let root = batch_results
            .get(&batch.id)
            .ok_or_else(|| anyhow::anyhow!("missing batch proxy root: {}", batch.id))?;
        hasher.update(batch.id.to_le_bytes());
        hasher.update(root);
    }
    Ok(hasher.finalize().into())
}

fn hash_local_tx(tx: &LocalTx) -> [u8; 32] {
    let mut hasher = Sha256::new();
    hasher.update(tx.account.to_le_bytes());
    hasher.update(tx.key.to_le_bytes());
    hasher.update(tx.value.to_le_bytes());
    hasher.update(tx.nonce.to_le_bytes());
    hasher.update(tx.fee.to_le_bytes());
    hasher.update(tx.signature);
    hasher.finalize().into()
}

fn compute_block_hash(header: &LocalBlockHeader, batches: &[LocalBatch]) -> [u8; 32] {
    let mut hasher = Sha256::new();
    hasher.update(header.height.to_le_bytes());
    hasher.update(header.epoch_id.to_le_bytes());
    hasher.update(header.parent_hash);
    hasher.update(header.state_root);
    hasher.update(header.tx_count.to_le_bytes());
    hasher.update(header.batch_count.to_le_bytes());
    hasher.update(header.consensus_binding.plugin_class_code.to_le_bytes());
    hasher.update(header.consensus_binding.adapter_hash);
    for batch in batches {
        hasher.update(batch.id.to_le_bytes());
        hasher.update(batch.mapped_ops.to_le_bytes());
        hasher.update((batch.txs.len() as u64).to_le_bytes());
        for tx in &batch.txs {
            hasher.update(hash_local_tx(tx));
        }
    }
    hasher.finalize().into()
}

fn build_local_block(closure: &BatchAClosureOutput, batches: &[LocalBatch]) -> LocalBlock {
    let tx_count = batches.iter().map(|b| b.txs.len() as u64).sum();
    let header = LocalBlockHeader {
        height: closure.height,
        epoch_id: closure.epoch_id,
        parent_hash: [0u8; 32],
        state_root: closure.state_root,
        tx_count,
        batch_count: batches.len() as u32,
        consensus_binding: closure.consensus_binding,
    };
    let batches = batches.to_vec();
    let block_hash = compute_block_hash(&header, &batches);
    LocalBlock {
        header,
        batches,
        proposal_hash: closure.proposal_hash,
        block_hash,
    }
}

fn to_block_header_wire_v1(header: &LocalBlockHeader) -> BlockHeaderWireV1 {
    BlockHeaderWireV1 {
        height: header.height,
        epoch_id: header.epoch_id,
        parent_hash: header.parent_hash,
        state_root: header.state_root,
        tx_count: header.tx_count,
        batch_count: header.batch_count,
        consensus_binding: header.consensus_binding,
    }
}

fn to_hex(bytes: &[u8]) -> String {
    let mut out = String::with_capacity(bytes.len() * 2);
    for b in bytes {
        use std::fmt::Write as _;
        let _ = write!(out, "{:02x}", b);
    }
    out
}

fn run_batch_a_minimal_closure(
    out: &AoemExecOutput,
    batches: &[LocalBatch],
    consensus_binding: ConsensusPluginBindingV1,
) -> Result<LocalBlock> {
    // Single-validator closure for Batch A:
    // execute -> proposal -> vote -> QC -> commit.
    if batches.is_empty() {
        bail!("batch_a requires at least one batch");
    }

    let validator_set = ValidatorSet::new_equal_weight(vec![0]);
    let signing_key = SigningKey::generate(&mut OsRng);
    let mut public_keys = HashMap::new();
    public_keys.insert(0, signing_key.verifying_key());

    let mut engine = BFTEngine::new(
        BFTConfig::default(),
        0,
        signing_key,
        validator_set,
        public_keys,
    )
    .context("init novovm-consensus engine failed")?;

    engine.start_epoch().context("start epoch failed")?;
    for batch in batches {
        engine
            .add_batch(batch.id, batch.txs.len() as u64)
            .with_context(|| format!("add batch {} failed", batch.id))?;
    }

    let mut batch_results = HashMap::new();
    for batch in batches {
        batch_results.insert(batch.id, build_batch_proxy_state_root(out, batch));
    }
    let proxy_state_root = build_epoch_proxy_state_root(out, batches, &batch_results)?;

    let proposal = engine
        .propose_epoch_with_state_root(&batch_results, proxy_state_root)
        .context("propose epoch failed")?;

    let vote = engine
        .vote_for_proposal(&proposal)
        .context("vote proposal failed")?;
    let qc = engine
        .collect_vote(vote)
        .context("collect vote failed")?
        .ok_or_else(|| anyhow::anyhow!("qc not formed"))?;
    let committed = engine.commit_qc(qc).context("commit qc failed")?;

    let committed_root = engine.last_committed_state_root().unwrap_or([0u8; 32]);
    let proposal_hash = proposal.hash();
    println!(
        "batch_a: epoch={} height={} committed=true txs={} state_root={} proposal_hash={}",
        committed.epoch.id,
        committed.epoch.height,
        committed.epoch.total_txs,
        to_hex(&committed_root),
        to_hex(&proposal_hash)
    );
    println!(
        "batch_a_batches: count={} layout={} mapped_ops={}",
        batches.len(),
        batch_layout_summary(batches),
        batches.iter().map(|b| b.mapped_ops as u64).sum::<u64>()
    );

    let closure = BatchAClosureOutput {
        epoch_id: committed.epoch.id,
        height: committed.epoch.height,
        txs: committed.epoch.total_txs,
        state_root: committed_root,
        proposal_hash,
        consensus_binding,
    };
    let expected_txs = batches.iter().map(|b| b.txs.len() as u64).sum::<u64>();
    if closure.txs != expected_txs {
        bail!(
            "batch_a tx mismatch: committed={} expected={}",
            closure.txs,
            expected_txs
        );
    }
    let block = build_local_block(&closure, batches);
    println!(
        "block_out: height={} epoch={} batches={} txs={} block_hash={} state_root={} proposal_hash={}",
        block.header.height,
        block.header.epoch_id,
        block.batches.len(),
        block.header.tx_count,
        to_hex(&block.block_hash),
        to_hex(&block.header.state_root),
        to_hex(&block.proposal_hash)
    );
    println!(
        "block_consensus: plugin_class={} plugin_hash={}",
        plugin_class_name(block.header.consensus_binding.plugin_class_code),
        to_hex(&block.header.consensus_binding.adapter_hash)
    );
    Ok(block)
}

fn commit_block_in_memory(
    block: LocalBlock,
    expected_binding: ConsensusPluginBindingV1,
) -> Result<()> {
    let wire = encode_block_header_wire_v1(&to_block_header_wire_v1(&block.header));
    let decoded_header = decode_block_header_wire_v1(&wire)
        .context("decode block header wire failed before commit validation")?;
    verify_consensus_plugin_binding(expected_binding, decoded_header.consensus_binding)
        .map_err(|e| anyhow::anyhow!("consensus plugin binding verification failed: {e}"))?;
    println!(
        "block_wire: codec={} bytes={} pass=true",
        BLOCK_HEADER_WIRE_V1_CODEC,
        wire.len()
    );

    let mut store = global_block_store()
        .lock()
        .map_err(|_| anyhow::anyhow!("block store mutex poisoned"))?;
    store.commit_block(block)?;
    let latest = store
        .latest()
        .ok_or_else(|| anyhow::anyhow!("missing latest block after commit"))?;
    println!(
        "commit_out: store=in_memory committed=true height={} total_blocks={} block_hash={} state_root={}",
        latest.header.height,
        store.total_blocks(),
        to_hex(&latest.block_hash),
        to_hex(&latest.header.state_root)
    );
    println!(
        "commit_consensus: plugin_class={} plugin_hash={} pass=true",
        plugin_class_name(latest.header.consensus_binding.plugin_class_code),
        to_hex(&latest.header.consensus_binding.adapter_hash)
    );
    Ok(())
}

fn run_network_smoke(tx_count: u64) -> Result<NetworkSignal> {
    let transport = InMemoryTransport::new(64);
    let from = NodeId(0);
    let to = NodeId(1);
    transport.register(from);
    transport.register(to);

    // discovery: exchange peer lists
    let discovery_a = ProtocolMessage::Gossip(ProtocolGossipMessage::PeerList {
        from,
        peers: vec![to],
    });
    let discovery_b = ProtocolMessage::Gossip(ProtocolGossipMessage::PeerList {
        from: to,
        peers: vec![from],
    });
    transport
        .send(to, discovery_a)
        .context("network discovery send A->B failed")?;
    transport
        .send(from, discovery_b)
        .context("network discovery send B->A failed")?;

    // gossip: heartbeat
    let heartbeat = ProtocolMessage::Gossip(ProtocolGossipMessage::Heartbeat {
        from,
        shard: ShardId((tx_count as u32) % 1024),
    });
    transport
        .send(to, heartbeat)
        .context("network gossip send failed")?;

    // sync: distributed occc state sync message
    let sync = ProtocolMessage::DistributedOcccGossip(DistributedGossipMessage {
        from: to.0 as u32,
        to: from.0 as u32,
        msg_type: DistributedMessageType::StateSync,
        payload: tx_count.to_le_bytes().to_vec(),
        timestamp: 0,
        seq: tx_count,
    });
    transport
        .send(from, sync)
        .context("network sync send failed")?;

    let recv_b_1 = transport
        .try_recv(to)
        .context("network recv on node B (slot 1) failed")?;
    let recv_a_1 = transport
        .try_recv(from)
        .context("network recv on node A (slot 1) failed")?;
    let recv_b_2 = transport
        .try_recv(to)
        .context("network recv on node B (slot 2) failed")?;
    let recv_a_2 = transport
        .try_recv(from)
        .context("network recv on node A (slot 2) failed")?;

    let discovery = matches!(
        recv_b_1.as_ref(),
        Some(ProtocolMessage::Gossip(ProtocolGossipMessage::PeerList { .. }))
    ) && matches!(
        recv_a_1.as_ref(),
        Some(ProtocolMessage::Gossip(ProtocolGossipMessage::PeerList { .. }))
    );
    let gossip = matches!(
        recv_b_2.as_ref(),
        Some(ProtocolMessage::Gossip(ProtocolGossipMessage::Heartbeat { .. }))
    );
    let sync = matches!(
        recv_a_2.as_ref(),
        Some(ProtocolMessage::DistributedOcccGossip(DistributedGossipMessage {
            msg_type: DistributedMessageType::StateSync,
            ..
        }))
    );

    let received = [recv_b_1, recv_a_1, recv_b_2, recv_a_2]
        .iter()
        .filter(|m| m.is_some())
        .count() as u64;

    Ok(NetworkSignal {
        transport: "in_memory",
        from: from.0,
        to: to.0,
        nodes: 2,
        sent: 4,
        received,
        msg_kind: "two_node_discovery_gossip_sync",
        discovery,
        gossip,
        sync,
    })
}

fn run_network_probe_mode() -> Result<()> {
    let node_id = u32_env("NOVOVM_NET_NODE_ID", 0) as u64;
    let default_listen = if node_id == 0 {
        "127.0.0.1:39100"
    } else {
        "127.0.0.1:39101"
    };
    let default_peer = if node_id == 0 {
        "127.0.0.1:39101"
    } else {
        "127.0.0.1:39100"
    };
    let listen = string_env("NOVOVM_NET_LISTEN", default_listen);
    let peer = string_env("NOVOVM_NET_PEER", default_peer);
    let timeout_ms = u64_env("NOVOVM_NET_TIMEOUT_MS", 2_500);
    let resend_ms = u64_env("NOVOVM_NET_RESEND_MS", 120);
    let min_runtime_ms = u64_env("NOVOVM_NET_MIN_RUNTIME_MS", 800);

    let from = NodeId(node_id);
    let peers = parse_network_probe_peers(node_id, &peer)?;
    let expected_peer_ids: Vec<NodeId> = peers.iter().map(|(id, _)| *id).collect();
    let expected_set: HashSet<u64> = expected_peer_ids.iter().map(|id| id.0).collect();
    let expected_count = expected_set.len();
    let tamper_mode = string_env("NOVOVM_NET_TAMPER_BLOCK_WIRE", "").to_ascii_lowercase();
    if !tamper_mode.is_empty()
        && tamper_mode != "class_mismatch"
        && tamper_mode != "hash_mismatch"
        && tamper_mode != "codec_corrupt"
    {
        bail!(
            "invalid NOVOVM_NET_TAMPER_BLOCK_WIRE mode: {} (valid: class_mismatch|hash_mismatch|codec_corrupt)",
            tamper_mode
        );
    }
    let expected_consensus_binding = network_probe_consensus_binding();
    let mut sync_payloads: HashMap<u64, Vec<u8>> = HashMap::new();
    for (peer_id, _) in &peers {
        sync_payloads.insert(
            peer_id.0,
            build_network_probe_block_wire_payload(
                from,
                *peer_id,
                expected_consensus_binding,
                &tamper_mode,
            ),
        );
    }
    let peer_label = if peers.len() == 1 {
        peers[0].1.clone()
    } else {
        format!("mesh:{}", peers.len())
    };
    let transport = UdpTransport::bind(from, &listen)
        .with_context(|| format!("udp transport bind failed: {listen}"))?;
    for (peer_id, peer_addr) in &peers {
        transport
            .register_peer(*peer_id, peer_addr)
            .with_context(|| format!("udp transport register peer failed: {peer_addr}"))?;
    }

    let discovery = ProtocolMessage::Gossip(ProtocolGossipMessage::PeerList {
        from,
        peers: expected_peer_ids.clone(),
    });

    let heartbeat = ProtocolMessage::Gossip(ProtocolGossipMessage::Heartbeat {
        from,
        shard: ShardId(((node_id + 1) as u32) % 1024),
    });

    let mut discovery_from: HashSet<u64> = HashSet::new();
    let mut gossip_from: HashSet<u64> = HashSet::new();
    let mut sync_from: HashSet<u64> = HashSet::new();
    let mut block_wire_from: HashSet<u64> = HashSet::new();
    let mut block_wire_min_bytes = usize::MAX;
    let mut block_wire_max_bytes = 0usize;
    let mut received = 0u64;
    let mut sent = 0u64;
    let started_at = Instant::now();
    let deadline = Duration::from_millis(timeout_ms);
    let resend_every = Duration::from_millis(resend_ms);
    let mut last_send = Instant::now()
        .checked_sub(resend_every)
        .unwrap_or_else(Instant::now);

    while started_at.elapsed() < deadline {
        if last_send.elapsed() >= resend_every {
            for (to, _) in &peers {
                let payload = sync_payloads
                    .get(&to.0)
                    .cloned()
                    .ok_or_else(|| anyhow::anyhow!("missing sync payload for peer {}", to.0))?;
                let sync = ProtocolMessage::DistributedOcccGossip(DistributedGossipMessage {
                    from: from.0 as u32,
                    to: to.0 as u32,
                    msg_type: DistributedMessageType::StateSync,
                    payload,
                    timestamp: 0,
                    seq: node_id,
                });
                transport
                    .send(*to, discovery.clone())
                    .context("udp transport send discovery failed")?;
                transport
                    .send(*to, heartbeat.clone())
                    .context("udp transport send gossip failed")?;
                transport
                    .send(*to, sync)
                    .context("udp transport send sync failed")?;
                sent = sent.saturating_add(3);
            }
            last_send = Instant::now();
        }

        match transport
            .try_recv(from)
            .context("udp transport recv failed")?
        {
            Some(msg) => {
                received = received.saturating_add(1);
                match msg {
                    ProtocolMessage::Gossip(ProtocolGossipMessage::PeerList { from: src, .. }) => {
                        if expected_set.contains(&src.0) {
                            discovery_from.insert(src.0);
                        }
                    }
                    ProtocolMessage::Gossip(ProtocolGossipMessage::Heartbeat { from: src, .. }) => {
                        if expected_set.contains(&src.0) {
                            gossip_from.insert(src.0);
                        }
                    }
                    ProtocolMessage::DistributedOcccGossip(DistributedGossipMessage {
                        from: src,
                        msg_type: DistributedMessageType::StateSync,
                        payload,
                        ..
                    }) => {
                        let src_id = src as u64;
                        if expected_set.contains(&src_id) {
                            sync_from.insert(src_id);
                            if let Ok(header) = decode_block_header_wire_v1(&payload) {
                                if verify_consensus_plugin_binding(
                                    expected_consensus_binding,
                                    header.consensus_binding,
                                )
                                .is_ok()
                                {
                                    block_wire_from.insert(src_id);
                                    let payload_len = payload.len();
                                    block_wire_min_bytes = block_wire_min_bytes.min(payload_len);
                                    block_wire_max_bytes = block_wire_max_bytes.max(payload_len);
                                }
                            }
                        }
                    }
                    _ => {}
                }
            }
            None => {
                std::thread::sleep(Duration::from_millis(2));
            }
        }

        if discovery_from.len() == expected_count
            && gossip_from.len() == expected_count
            && sync_from.len() == expected_count
            && block_wire_from.len() == expected_count
            && started_at.elapsed() >= Duration::from_millis(min_runtime_ms)
        {
            break;
        }
    }

    let mut edge_ok_ids: Vec<u64> = Vec::new();
    let mut edge_down_ids: Vec<u64> = Vec::new();
    for id in expected_set.iter() {
        if discovery_from.contains(id)
            && gossip_from.contains(id)
            && sync_from.contains(id)
            && block_wire_from.contains(id)
        {
            edge_ok_ids.push(*id);
        } else {
            edge_down_ids.push(*id);
        }
    }
    edge_ok_ids.sort_unstable();
    edge_down_ids.sort_unstable();
    let got_discovery = discovery_from.len() == expected_count;
    let got_gossip = gossip_from.len() == expected_count;
    let got_sync = sync_from.len() == expected_count;
    let got_block_wire = block_wire_from.len() == expected_count;
    let block_wire_min = if block_wire_min_bytes == usize::MAX {
        0
    } else {
        block_wire_min_bytes
    };

    println!(
        "network_probe_out: transport=udp node={} listen={} peer={} sent={} received={} discovery={} gossip={} sync={}",
        node_id, listen, peer_label, sent, received, got_discovery, got_gossip, got_sync
    );
    println!(
        "network_probe_graph: node={} peers={} discovery_ok={}/{} gossip_ok={}/{} sync_ok={}/{} edge_ok={}/{}",
        node_id,
        expected_count,
        discovery_from.len(),
        expected_count,
        gossip_from.len(),
        expected_count,
        sync_from.len(),
        expected_count,
        edge_ok_ids.len(),
        expected_count
    );
    println!(
        "network_probe_edges: node={} up={} down={}",
        node_id,
        join_ids(&edge_ok_ids),
        join_ids(&edge_down_ids)
    );
    println!(
        "network_probe_tamper: node={} mode={}",
        node_id,
        if tamper_mode.is_empty() {
            "none"
        } else {
            tamper_mode.as_str()
        }
    );
    println!(
        "network_block_wire: codec={} node={} peers={} verified={}/{} expected_class={} expected_hash={} pass={} bytes_min={} bytes_max={}",
        BLOCK_HEADER_WIRE_V1_CODEC,
        node_id,
        expected_count,
        block_wire_from.len(),
        expected_count,
        plugin_class_name(expected_consensus_binding.plugin_class_code),
        to_hex(&expected_consensus_binding.adapter_hash),
        got_block_wire,
        block_wire_min,
        block_wire_max_bytes
    );

    if bool_env("NOVOVM_NETWORK_STRICT")
        && (!got_discovery || !got_gossip || !got_sync || !got_block_wire)
    {
        bail!(
            "network probe closure incomplete: discovery={} gossip={} sync={} block_wire={} node={}",
            got_discovery,
            got_gossip,
            got_sync,
            got_block_wire,
            node_id
        );
    }

    Ok(())
}

fn run_ffi_v2() -> Result<()> {
    let runtime = AoemRuntimeConfig::from_env()?;
    let facade = novovm_exec::AoemExecFacade::open_with_runtime(&runtime)?;
    let session = facade.create_session()?;

    let source_txs = build_demo_txs();
    let (decoded_txs, tx_codec) = roundtrip_local_tx_codec_v1(&source_txs)?;
    println!(
        "tx_codec: codec={} encoded={} decoded={} bytes={} pass={}",
        LOCAL_TX_WIRE_V1_CODEC,
        tx_codec.encoded,
        tx_codec.decoded,
        tx_codec.total_bytes,
        tx_codec.pass
    );

    let fee_floor = u64_env("NOVOVM_MEMPOOL_FEE_FLOOR", 1);
    let (admitted_txs, mempool) = admit_mempool_basic(&decoded_txs, fee_floor)?;
    println!(
        "mempool_out: policy=basic accepted={} rejected={} fee_floor={} nonce_ok={} sig_ok={}",
        mempool.accepted,
        mempool.rejected,
        mempool.fee_floor,
        mempool.nonce_ok,
        mempool.sig_ok
    );

    let tx_meta = validate_and_summarize_txs(&admitted_txs)?;
    let requested_batches = u32_env("NOVOVM_BATCH_A_BATCHES", 1) as usize;
    let local_batches = build_local_batches_from_txs(&admitted_txs, requested_batches);
    let total_mapped_ops = local_batches
        .iter()
        .map(|b| b.mapped_ops as usize)
        .sum::<usize>();
    let batch = encode_ops_v2_buffer(&admitted_txs);
    if total_mapped_ops != batch.ops.len() {
        bail!(
            "batch mapping mismatch: mapped_ops={} encoded_ops={}",
            total_mapped_ops,
            batch.ops.len()
        );
    }
    println!(
        "tx_ingress: codec=novovm_local_tx_v1 accepted={} mapped_ops={}",
        admitted_txs.len(),
        batch.ops.len()
    );
    println!(
        "tx_meta: accounts={} txs={} min_fee={} max_fee={} nonce_ok={} sig_ok={}",
        tx_meta.accounts,
        admitted_txs.len(),
        tx_meta.min_fee,
        tx_meta.max_fee,
        tx_meta.nonce_ok,
        tx_meta.sig_ok
    );
    let adapter_signal = run_adapter_bridge_signal(&admitted_txs)?;
    println!(
        "adapter_out: backend={} chain={} txs={} verified={} applied={} accounts={} state_root={}",
        adapter_signal.backend,
        adapter_signal.chain,
        adapter_signal.txs,
        adapter_signal.verified,
        adapter_signal.applied,
        adapter_signal.accounts,
        to_hex(&adapter_signal.state_root)
    );
    println!(
        "adapter_plugin_abi: enabled={} version={} expected={} caps=0x{:x} required=0x{:x} compatible={}",
        adapter_signal.plugin_abi_enabled,
        adapter_signal.plugin_abi_version,
        adapter_signal.plugin_abi_expected,
        adapter_signal.plugin_capabilities,
        adapter_signal.plugin_required_capabilities,
        adapter_signal.plugin_abi_compatible
    );
    println!(
        "adapter_plugin_registry: enabled={} strict={} matched={} chain_allowed={} entry_abi={} entry_required=0x{:x} hash_check={} hash_match={} abi_whitelist={} abi_allowed={}",
        adapter_signal.plugin_registry_enabled,
        adapter_signal.plugin_registry_strict,
        adapter_signal.plugin_registry_matched,
        adapter_signal.plugin_registry_chain_allowed,
        adapter_signal.plugin_registry_entry_abi,
        adapter_signal.plugin_registry_entry_required_caps,
        adapter_signal.plugin_registry_hash_check_enabled,
        adapter_signal.plugin_registry_hash_match,
        adapter_signal.plugin_registry_whitelist_present,
        adapter_signal.plugin_registry_whitelist_match
    );
    println!(
        "adapter_consensus: plugin_class={} plugin_class_code={} consensus_adapter_hash={} backend={}",
        adapter_signal.plugin_class,
        adapter_signal.plugin_class_code,
        to_hex(&adapter_signal.consensus_adapter_hash),
        adapter_signal.backend
    );
    println!(
        "batch_ingress: batches={} layout={} mapped_ops={}",
        local_batches.len(),
        batch_layout_summary(&local_batches),
        total_mapped_ops
    );
    let report = session.submit_ops_report(&batch.ops);
    if !report.ok {
        let err = report
            .error
            .as_ref()
            .map(|e| e.message.as_str())
            .unwrap_or("unknown");
        bail!(
            "mode=ffi_v2 rc={}({}) err={}",
            report.return_code,
            report.return_code_name,
            err
        );
    }

    let out = report
        .output
        .as_ref()
        .ok_or_else(|| anyhow::anyhow!("missing output on success report"))?;

    let strict_batch_a = bool_env("NOVOVM_BATCH_A_STRICT");
    let consensus_binding = ConsensusPluginBindingV1 {
        plugin_class_code: adapter_signal.plugin_class_code,
        adapter_hash: adapter_signal.consensus_adapter_hash,
    };
    match run_batch_a_minimal_closure(
        out,
        &local_batches,
        consensus_binding,
    ) {
        Ok(block) => {
            if let Err(e) = commit_block_in_memory(block, consensus_binding) {
                if strict_batch_a {
                    return Err(e).context("batch_a commit failed with strict mode");
                }
                eprintln!("batch_a_warn: {}", e);
            }
        }
        Err(e) => {
            if strict_batch_a {
                return Err(e).context("batch_a closure failed with strict mode");
            }
            eprintln!("batch_a_warn: {}", e);
        }
    }

    let strict_network = bool_env("NOVOVM_NETWORK_STRICT");
    match run_network_smoke(admitted_txs.len() as u64) {
        Ok(signal) => {
            println!(
                "network_out: transport={} from={} to={} sent={} received={} msg_kind={}",
                signal.transport,
                signal.from,
                signal.to,
                signal.sent,
                signal.received,
                signal.msg_kind
            );
            println!(
                "network_closure: nodes={} discovery={} gossip={} sync={}",
                signal.nodes,
                signal.discovery,
                signal.gossip,
                signal.sync
            );
            if strict_network && (!signal.discovery || !signal.gossip || !signal.sync) {
                bail!(
                    "network closure incomplete: discovery={} gossip={} sync={}",
                    signal.discovery,
                    signal.gossip,
                    signal.sync
                );
            }
        }
        Err(e) => {
            if strict_network {
                return Err(e).context("network smoke failed with strict mode");
            }
            eprintln!("network_warn: {}", e);
        }
    }

    // Keep this line stable for migration scripts that parse host report.
    println!(
        "mode=ffi_v2 variant={} dll={} rc={}({}) submitted={} processed={} success={} writes={} elapsed_us={}",
        runtime.variant.as_str(),
        runtime.dll_path.display(),
        report.return_code,
        report.return_code_name,
        out.metrics.submitted_ops,
        out.metrics.processed_ops,
        out.metrics.success_ops,
        out.metrics.total_writes,
        out.metrics.elapsed_us
    );
    drop(session);
    std::mem::forget(facade);
    Ok(())
}

fn run_legacy_compat() -> Result<()> {
    println!("mode=legacy_compat route=ffi_v2");
    run_ffi_v2()
}

fn main() -> Result<()> {
    let node_mode = std::env::var("NOVOVM_NODE_MODE").unwrap_or_else(|_| "full".to_string());
    if node_mode.eq_ignore_ascii_case("network_probe") {
        return run_network_probe_mode();
    }

    let mode = exec_path_mode();
    match mode.as_str() {
        "ffi_v2" => run_ffi_v2(),
        "legacy" => run_legacy_compat(),
        _ => bail!("unknown exec path mode: {mode}; valid: ffi_v2|legacy"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use aoem_bindings::AoemExecV2Result;
    use novovm_exec::{AoemExecMetrics, AoemExecReturnCode};

    fn test_tx(account: u64, key: u64, value: u64, nonce: u64, fee: u64) -> LocalTx {
        build_local_tx(account, key, value, nonce, fee)
    }

    #[test]
    fn proxy_state_root_is_deterministic() {
        let out = AoemExecOutput {
            result: AoemExecV2Result {
                processed: 10,
                success: 10,
                failed_index: u32::MAX,
                total_writes: 20,
            },
            metrics: AoemExecMetrics {
                submitted_ops: 10,
                return_code: AoemExecReturnCode::Ok.as_u32(),
                ..AoemExecMetrics::default()
            },
        };
        let a = build_proxy_state_root(&out);
        let b = build_proxy_state_root(&out);
        assert_eq!(a, b);
    }

    #[test]
    fn encode_ops_matches_tx_count() {
        let txs = vec![test_tx(0, 1, 2, 0, 1), test_tx(0, 3, 4, 1, 1)];
        let batch = encode_ops_v2_buffer(&txs);
        assert_eq!(batch.ops.len(), 2);
        assert_eq!(batch._keys.len(), 2);
        assert_eq!(batch._values.len(), 2);
        assert_eq!(batch.ops[0].plan_id, 1);
        assert_eq!(batch.ops[1].plan_id, 2);
    }

    #[test]
    fn block_hash_is_deterministic() {
        let txs = vec![test_tx(0, 11, 22, 0, 1), test_tx(0, 33, 44, 1, 1)];
        let closure = BatchAClosureOutput {
            epoch_id: 7,
            height: 9,
            txs: 2,
            state_root: [3u8; 32],
            proposal_hash: [9u8; 32],
            consensus_binding: ConsensusPluginBindingV1 {
                plugin_class_code: PLUGIN_CLASS_CONSENSUS,
                adapter_hash: [8u8; 32],
            },
        };
        let batches = build_local_batches_from_txs(&txs, 2);
        let a = build_local_block(&closure, &batches);
        let b = build_local_block(&closure, &batches);
        assert_eq!(a.block_hash, b.block_hash);
    }

    #[test]
    fn local_batch_partition_is_stable() {
        let txs = vec![
            test_tx(0, 1, 10, 0, 1),
            test_tx(0, 2, 20, 1, 1),
            test_tx(0, 3, 30, 2, 1),
            test_tx(0, 4, 40, 3, 1),
            test_tx(0, 5, 50, 4, 1),
        ];

        let batches = build_local_batches_from_txs(&txs, 3);
        assert_eq!(batches.len(), 3);
        assert_eq!(batches[0].txs.len(), 2);
        assert_eq!(batches[1].txs.len(), 2);
        assert_eq!(batches[2].txs.len(), 1);
        assert_eq!(batch_layout_summary(&batches), "1:2,2:2,3:1");
    }

    #[test]
    fn tx_metadata_validation_passes() {
        let txs = vec![
            test_tx(1000, 1, 10, 0, 2),
            test_tx(1001, 2, 20, 0, 3),
            test_tx(1000, 3, 30, 1, 2),
            test_tx(1001, 4, 40, 1, 4),
        ];
        let summary = validate_and_summarize_txs(&txs).expect("tx metadata should be valid");
        assert_eq!(summary.accounts, 2);
        assert_eq!(summary.min_fee, 2);
        assert_eq!(summary.max_fee, 4);
        assert!(summary.nonce_ok);
        assert!(summary.sig_ok);
    }

    #[test]
    fn tx_metadata_validation_rejects_bad_signature() {
        let mut tx = test_tx(1000, 1, 10, 0, 1);
        tx.signature = [7u8; 32];
        let err = validate_and_summarize_txs(&[tx]).unwrap_err().to_string();
        assert!(err.contains("signature invalid"));
    }

    #[test]
    fn adapter_tx_ir_mapping_is_stable() {
        let tx = test_tx(1000, 7, 11, 2, 3);
        let ir = to_adapter_tx_ir(&tx, 20260303);
        assert_eq!(ir.chain_id, 20260303);
        assert_eq!(ir.tx_type, TxType::Transfer);
        assert_eq!(ir.value, 11);
        assert_eq!(ir.nonce, 2);
        assert_eq!(ir.gas_price, 3);
        assert_eq!(ir.signature, tx.signature.to_vec());
        assert!(ir.to.is_some());
        assert!(!ir.hash.is_empty());
    }

    #[test]
    fn adapter_bridge_signal_is_deterministic() {
        let txs = vec![test_tx(1000, 1, 10, 0, 2), test_tx(1001, 2, 20, 0, 3)];
        let a = run_adapter_bridge_signal(&txs).expect("adapter bridge should pass");
        let b = run_adapter_bridge_signal(&txs).expect("adapter bridge should pass");
        assert_eq!(a.backend, "native");
        assert_eq!(a.chain, "novovm");
        assert_eq!(a.txs, 2);
        assert!(a.verified);
        assert!(a.applied);
        assert_eq!(a.accounts, 4);
        assert_eq!(a.state_root, b.state_root);
        assert_eq!(a.plugin_class_code, PLUGIN_CLASS_CONSENSUS);
        assert_eq!(a.plugin_class, plugin_class_name(PLUGIN_CLASS_CONSENSUS));
        assert_eq!(a.consensus_adapter_hash, b.consensus_adapter_hash);
    }

    #[test]
    fn adapter_chain_id_mapping_is_stable() {
        assert_eq!(default_chain_id(ChainType::NovoVM), 20260303);
        assert_eq!(default_chain_id(ChainType::EVM), 1);
        assert_eq!(default_chain_id(ChainType::BNB), 56);
        assert_eq!(default_chain_id(ChainType::Custom), 9_999_999);
    }

    #[test]
    fn tx_wire_codec_roundtrip_passes() {
        let txs = vec![test_tx(1000, 1, 10, 0, 1), test_tx(1001, 2, 20, 0, 2)];
        let (decoded, summary) =
            roundtrip_local_tx_codec_v1(&txs).expect("codec roundtrip should succeed");
        assert_eq!(decoded.len(), txs.len());
        assert_eq!(summary.encoded, 2);
        assert_eq!(summary.decoded, 2);
        assert!(summary.total_bytes > 0);
        assert!(summary.pass);
    }

    #[test]
    fn mempool_admission_applies_fee_floor() {
        let txs = vec![
            test_tx(1001, 2, 20, 0, 2),
            test_tx(1000, 1, 10, 0, 1),
            test_tx(1001, 3, 30, 1, 3),
        ];
        let (accepted, summary) = admit_mempool_basic(&txs, 2).expect("mempool should admit some");
        assert_eq!(accepted.len(), 2);
        assert_eq!(summary.accepted, 2);
        assert_eq!(summary.rejected, 1);
        assert_eq!(summary.fee_floor, 2);
        assert!(summary.nonce_ok);
        assert!(summary.sig_ok);
    }

    #[test]
    fn in_memory_store_accepts_genesis_and_next() {
        let mut store = InMemoryBlockStore::default();
        let txs = vec![test_tx(0, 1, 2, 0, 1)];
        let c0 = BatchAClosureOutput {
            epoch_id: 0,
            height: 0,
            txs: 1,
            state_root: [1u8; 32],
            proposal_hash: [2u8; 32],
            consensus_binding: ConsensusPluginBindingV1 {
                plugin_class_code: PLUGIN_CLASS_CONSENSUS,
                adapter_hash: [8u8; 32],
            },
        };
        let batches0 = build_local_batches_from_txs(&txs, 1);
        let mut b0 = build_local_block(&c0, &batches0);
        b0.header.parent_hash = [0u8; 32];
        store.commit_block(b0.clone()).unwrap();

        let c1 = BatchAClosureOutput {
            epoch_id: 1,
            height: 1,
            txs: 1,
            state_root: [3u8; 32],
            proposal_hash: [4u8; 32],
            consensus_binding: ConsensusPluginBindingV1 {
                plugin_class_code: PLUGIN_CLASS_CONSENSUS,
                adapter_hash: [8u8; 32],
            },
        };
        let batches1 = build_local_batches_from_txs(&txs, 1);
        let mut b1 = build_local_block(&c1, &batches1);
        b1.header.parent_hash = b0.block_hash;
        store.commit_block(b1).unwrap();
        assert_eq!(store.total_blocks(), 2);
    }

    #[test]
    fn in_memory_store_rejects_bad_parent() {
        let mut store = InMemoryBlockStore::default();
        let txs = vec![test_tx(0, 1, 2, 0, 1)];
        let c0 = BatchAClosureOutput {
            epoch_id: 0,
            height: 0,
            txs: 1,
            state_root: [1u8; 32],
            proposal_hash: [2u8; 32],
            consensus_binding: ConsensusPluginBindingV1 {
                plugin_class_code: PLUGIN_CLASS_CONSENSUS,
                adapter_hash: [8u8; 32],
            },
        };
        let batches0 = build_local_batches_from_txs(&txs, 1);
        let mut b0 = build_local_block(&c0, &batches0);
        b0.header.parent_hash = [0u8; 32];
        store.commit_block(b0).unwrap();

        let c1 = BatchAClosureOutput {
            epoch_id: 1,
            height: 1,
            txs: 1,
            state_root: [3u8; 32],
            proposal_hash: [4u8; 32],
            consensus_binding: ConsensusPluginBindingV1 {
                plugin_class_code: PLUGIN_CLASS_CONSENSUS,
                adapter_hash: [8u8; 32],
            },
        };
        let batches1 = build_local_batches_from_txs(&txs, 1);
        let mut b1 = build_local_block(&c1, &batches1);
        b1.header.parent_hash = [9u8; 32];
        let err = store.commit_block(b1).unwrap_err().to_string();
        assert!(err.contains("parent hash mismatch"));
    }

    #[test]
    fn network_smoke_closes_two_node_flow() {
        let signal = run_network_smoke(3).expect("network smoke should succeed");
        assert_eq!(signal.transport, "in_memory");
        assert_eq!(signal.nodes, 2);
        assert_eq!(signal.sent, 4);
        assert_eq!(signal.received, 4);
        assert_eq!(signal.msg_kind, "two_node_discovery_gossip_sync");
        assert!(signal.discovery);
        assert!(signal.gossip);
        assert!(signal.sync);
    }
}
