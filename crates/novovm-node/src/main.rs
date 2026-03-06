// Copyright (c) 2026 Xonovo Technology
// All rights reserved.
// Author: Xonovo Technology

use anyhow::{bail, Context, Result};
use ed25519_dalek::SigningKey;
use novovm_adapter_api::{
    default_chain_id, ChainConfig, ChainType, SerializationFormat, StateIR, TxIR, TxType,
};
use novovm_adapter_novovm::{create_native_adapter, supports_native_chain};
use novovm_consensus::{
    AmmGovernanceParams, BFTConfig, BFTEngine, BFTError as ConsensusBftError, BondGovernanceParams,
    BuybackGovernanceParams, CdpGovernanceParams, Epoch as ConsensusEpoch, GovernanceAccessPolicy,
    GovernanceChainAuditEvent, GovernanceCouncilMember, GovernanceCouncilPolicy,
    GovernanceCouncilSeat, GovernanceOp, GovernanceProposal, GovernanceVote, GovernanceVoteVerifier,
    GovernanceVoteVerifierScheme, HotStuffProtocol, MarketGovernancePolicy, NavGovernanceParams,
    NetworkDosPolicy,
    NodeId as ConsensusNodeId, ReserveGovernanceParams, SlashMode, SlashPolicy,
    TokenEconomicsPolicy, ValidatorSet, Web30MarketEngineSnapshot,
};
use novovm_exec::{AoemRuntimeConfig, ExecOpV2};
use novovm_network::{InMemoryTransport, Transport, UdpTransport};
use novovm_protocol::{
    decode_block_header_wire_v1, decode_local_tx_wire_v1 as decode_tx_wire_v1,
    encode_block_header_wire_v1, encode_local_tx_wire_v1 as encode_tx_wire_v1,
    plugin_class_name as protocol_plugin_class_name,
    protocol_catalog::distributed_occc::gossip::{
        GossipMessage as DistributedGossipMessage, MessageType as DistributedMessageType,
    },
    verify_consensus_plugin_binding, BlockHeaderWireV1, ConsensusPluginBindingV1,
    GossipMessage as ProtocolGossipMessage, LocalTxWireV1, NodeId,
    PacemakerMessage as ProtocolPacemakerMessage, ProtocolMessage, ShardId,
    BLOCK_HEADER_WIRE_V1_CODEC, CONSENSUS_PLUGIN_CLASS_CODE, LOCAL_PLUGIN_CLASS_CODE,
    LOCAL_TX_WIRE_V1_CODEC,
};
use rand::rngs::OsRng;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::{HashMap, HashSet};
use std::fs;
use std::io::Read;
use std::net::{IpAddr, SocketAddr};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, OnceLock};
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

fn bool_env_default(name: &str, default: bool) -> bool {
    match std::env::var(name) {
        Ok(v) => {
            let v = v.trim();
            v == "1"
                || v.eq_ignore_ascii_case("true")
                || v.eq_ignore_ascii_case("on")
                || v.eq_ignore_ascii_case("yes")
        }
        Err(_) => default,
    }
}

fn string_env_nonempty(name: &str) -> Option<String> {
    std::env::var(name).ok().and_then(|raw| {
        let trimmed = raw.trim();
        if trimmed.is_empty() {
            None
        } else {
            Some(trimmed.to_string())
        }
    })
}

fn u32_env_allow_zero(name: &str, default: u32) -> u32 {
    std::env::var(name)
        .ok()
        .and_then(|s| s.trim().parse::<u32>().ok())
        .unwrap_or(default)
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

fn resolve_consensus_policy_path() -> (Option<PathBuf>, bool) {
    if let Ok(raw) = std::env::var("NOVOVM_CONSENSUS_POLICY_PATH") {
        let trimmed = raw.trim();
        if !trimmed.is_empty() {
            return (Some(PathBuf::from(trimmed)), true);
        }
    }

    let default = PathBuf::from(CONSENSUS_POLICY_PATH_DEFAULT);
    if default.exists() {
        return (Some(default), false);
    }
    let alt = PathBuf::from(CONSENSUS_POLICY_PATH_ALT);
    if alt.exists() {
        return (Some(alt), false);
    }
    (None, false)
}

fn parse_slash_mode(raw: &str) -> Result<SlashMode> {
    let normalized = raw.trim().to_ascii_lowercase();
    match normalized.as_str() {
        "enforce" => Ok(SlashMode::Enforce),
        "observe_only" => Ok(SlashMode::ObserveOnly),
        _ => bail!(
            "policy_invalid: unsupported mode={}, valid: enforce|observe_only",
            raw
        ),
    }
}

fn parse_consensus_policy_file(path: &Path) -> Result<LoadedSlashPolicy> {
    let bytes = fs::read(path).with_context(|| {
        format!(
            "policy_parse_failed: read slash policy failed: {}",
            path.display()
        )
    })?;
    let text = String::from_utf8(bytes).with_context(|| {
        format!(
            "policy_parse_failed: slash policy is not valid utf-8: {}",
            path.display()
        )
    })?;
    let normalized = text.trim_start_matches('\u{feff}');
    let parsed: ConsensusPolicyFile = serde_json::from_str(normalized).with_context(|| {
        format!(
            "policy_parse_failed: parse slash policy json failed: {}",
            path.display()
        )
    })?;

    let mode_raw = parsed
        .mode
        .as_deref()
        .ok_or_else(|| anyhow::anyhow!("policy_invalid: missing mode"))?;
    let equivocation_threshold = parsed
        .equivocation_threshold
        .ok_or_else(|| anyhow::anyhow!("policy_invalid: missing equivocation_threshold"))?;
    let min_active_validators = parsed
        .min_active_validators
        .ok_or_else(|| anyhow::anyhow!("policy_invalid: missing min_active_validators"))?;
    let mode = parse_slash_mode(mode_raw)?;
    let policy = SlashPolicy {
        mode,
        equivocation_threshold,
        min_active_validators,
        cooldown_epochs: parsed.cooldown_epochs.unwrap_or(0),
    };
    policy
        .validate()
        .map_err(|e| anyhow::anyhow!("policy_invalid: {}", e))?;

    Ok(LoadedSlashPolicy {
        source: "file",
        path: Some(path.to_path_buf()),
        cooldown_epochs: policy.cooldown_epochs,
        policy,
    })
}

fn load_consensus_slash_policy() -> Result<LoadedSlashPolicy> {
    let (path, explicit_path) = resolve_consensus_policy_path();
    if let Some(path) = path {
        if explicit_path && !path.exists() {
            bail!(
                "policy_parse_failed: slash policy path not found: {}",
                path.display()
            );
        }
        return parse_consensus_policy_file(&path);
    }

    let default_policy = SlashPolicy::default();
    Ok(LoadedSlashPolicy {
        source: "default",
        path: None,
        cooldown_epochs: default_policy.cooldown_epochs,
        policy: default_policy,
    })
}

fn emit_slash_policy_in_signal(loaded: &LoadedSlashPolicy) {
    let path = loaded
        .path
        .as_ref()
        .map(|p| p.display().to_string())
        .unwrap_or_else(|| "-".to_string());
    println!(
        "slash_policy_in: source={} path={} mode={} threshold={} min_validators={} cooldown_epochs={}",
        loaded.source,
        path,
        loaded.policy.mode.as_str(),
        loaded.policy.equivocation_threshold,
        loaded.policy.min_active_validators,
        loaded.cooldown_epochs
    );
}

fn parse_network_probe_peers(
    node_id: u64,
    fallback_peer_addr: &str,
) -> Result<Vec<(NodeId, String)>> {
    if let Ok(spec) = std::env::var("NOVOVM_NET_PEERS") {
        let spec = spec.trim();
        if !spec.is_empty() {
            let mut out = Vec::new();
            for item in spec.split(',') {
                let item = item.trim();
                if item.is_empty() {
                    continue;
                }
                let (id_raw, addr_raw) = item.split_once('@').ok_or_else(|| {
                    anyhow::anyhow!("invalid NOVOVM_NET_PEERS item: {item} (expected id@addr)")
                })?;
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
                bail!(
                    "NOVOVM_NET_PEERS resolved to zero peers for node {}",
                    node_id
                );
            }
            return Ok(out);
        }
    }

    let fallback_peer_id = if node_id == 0 { 1 } else { 0 };
    Ok(vec![(
        NodeId(fallback_peer_id),
        fallback_peer_addr.to_string(),
    )])
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
    let mut governance_root_hasher = Sha256::new();
    governance_root_hasher.update(b"novovm_network_probe_governance_chain_audit_root_v1");
    governance_root_hasher.update(from.0.to_le_bytes());
    governance_root_hasher.update(to.0.to_le_bytes());
    governance_root_hasher.update(consensus_binding.adapter_hash);
    let governance_chain_audit_root: [u8; 32] = governance_root_hasher.finalize().into();
    let mut header = BlockHeaderWireV1 {
        height: from.0.saturating_add(1),
        epoch_id: 1,
        parent_hash,
        state_root,
        governance_chain_audit_root,
        tx_count: 1,
        batch_count: 1,
        consensus_binding,
    };
    match tamper_mode {
        "class_mismatch" => {
            header.consensus_binding.plugin_class_code =
                if header.consensus_binding.plugin_class_code == CONSENSUS_PLUGIN_CLASS_CODE {
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
const ADAPTER_PLUGIN_REGISTRY_PATH_DEFAULT: &str =
    "..\\..\\config\\novovm-adapter-plugin-registry.json";
const ADAPTER_PLUGIN_REGISTRY_PATH_ALT: &str = "config\\novovm-adapter-plugin-registry.json";
const CONSENSUS_POLICY_PATH_DEFAULT: &str = "..\\..\\config\\novovm-consensus-policy.json";
const CONSENSUS_POLICY_PATH_ALT: &str = "config\\novovm-consensus-policy.json";
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

#[derive(Debug, Clone, Deserialize)]
struct ConsensusPolicyFile {
    mode: Option<String>,
    equivocation_threshold: Option<u32>,
    min_active_validators: Option<u32>,
    cooldown_epochs: Option<u64>,
}

#[derive(Debug, Clone)]
struct LoadedSlashPolicy {
    source: &'static str,
    path: Option<PathBuf>,
    policy: SlashPolicy,
    cooldown_epochs: u64,
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
    governance_chain_audit_root: [u8; 32],
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

#[derive(Debug, Clone, Serialize, Deserialize)]
struct QueryBlockRecord {
    height: u64,
    epoch_id: u64,
    parent_hash: String,
    state_root: String,
    #[serde(default)]
    governance_chain_audit_root: String,
    tx_count: u64,
    batch_count: u32,
    proposal_hash: String,
    block_hash: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct QueryTxRecord {
    tx_hash: String,
    block_height: u64,
    block_hash: String,
    account: u64,
    key: u64,
    value: u64,
    nonce: u64,
    fee: u64,
    success: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct QueryReceiptRecord {
    tx_hash: String,
    block_height: u64,
    block_hash: String,
    success: bool,
    gas_used: u64,
    state_root: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
struct QueryStateDb {
    blocks: Vec<QueryBlockRecord>,
    txs: HashMap<String, QueryTxRecord>,
    receipts: HashMap<String, QueryReceiptRecord>,
    balances: HashMap<String, u64>,
}

#[derive(Debug, Clone)]
struct BatchAClosureOutput {
    epoch_id: u64,
    height: u64,
    txs: u64,
    state_root: [u8; 32],
    governance_chain_audit_root: [u8; 32],
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
    view_sync: bool,
    new_view: bool,
}

#[derive(Debug, Clone)]
struct HeaderSyncSignal {
    mode: &'static str,
    codec: &'static str,
    remote_tip: u64,
    local_tip_before: u64,
    fetched: u64,
    applied: u64,
    local_tip_after: u64,
    complete: bool,
    pass: bool,
    tamper_at: u64,
    reason: String,
}

#[derive(Debug, Clone)]
struct HeaderChainEntry {
    header: BlockHeaderWireV1,
    block_hash: [u8; 32],
}

#[derive(Debug, Clone)]
struct FastStateSyncSignal {
    mode: &'static str,
    codec: &'static str,
    remote_tip: u64,
    local_tip_before: u64,
    fetched_headers: u64,
    applied_headers: u64,
    local_tip_after: u64,
    fast_complete: bool,
    snapshot_height: u64,
    snapshot_accounts: u64,
    snapshot_verified: bool,
    state_complete: bool,
    pass: bool,
    tamper_snapshot_at: u64,
    reason: String,
}

#[derive(Debug, Clone)]
struct NetworkDosSignal {
    mode: &'static str,
    codec: &'static str,
    peers: u64,
    invalid_peers: u64,
    invalid_burst: u64,
    ban_after: u64,
    invalid_detected: u64,
    bans: u64,
    storm_rejected: u64,
    healthy_accepts: u64,
    pass: bool,
    reason: String,
}

#[derive(Debug, Clone)]
struct PacemakerFailoverSignal {
    mode: &'static str,
    transport: &'static str,
    nodes: u64,
    failed_leader: u64,
    initial_view: u64,
    next_view: u64,
    next_leader: u64,
    timeout_votes: u64,
    timeout_quorum: u64,
    timeout_cert: bool,
    local_view_advanced: u64,
    view_sync_votes: u64,
    new_view_votes: u64,
    qc_formed: bool,
    committed: bool,
    committed_height: u64,
    pass: bool,
    reason: String,
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
const LOCAL_TX_HASH_DOMAIN: &[u8] = b"novovm_local_tx_hash_v1";

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
    tx.signature == compute_local_tx_signature_parts(tx.account, tx.key, tx.value, tx.nonce, tx.fee)
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
            bail!(
                "tx fee must be > 0 (account={}, nonce={})",
                tx.account,
                tx.nonce
            );
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
        let path =
            plugin_path.ok_or_else(|| anyhow::anyhow!("plugin backend requires plugin path"))?;
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
        bail!(
            "adapter plugin registry has zero plugins: {}",
            path.display()
        );
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
        if let (Ok(entry_canon), Ok(actual_canon)) =
            (entry.canonicalize(), actual_path.canonicalize())
        {
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
        ChainType::EVM => Some(1),
        ChainType::BNB => Some(6),
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
    let chain_code = chain_type_to_plugin_code(chain).ok_or_else(|| {
        anyhow::anyhow!("plugin backend does not support chain={}", chain.as_str())
    })?;
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
    let tx_irs: Vec<TxIR> = txs
        .iter()
        .map(|tx| to_adapter_tx_ir(tx, chain_id))
        .collect();

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
                anyhow::anyhow!("NOVOVM_ADAPTER_BACKEND=plugin requires NOVOVM_ADAPTER_PLUGIN_PATH")
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

#[cfg(test)]
fn build_proxy_state_root(out: &novovm_exec::AoemExecOutput) -> [u8; 32] {
    let mut hasher = Sha256::new();
    hasher.update(out.result.processed.to_le_bytes());
    hasher.update(out.result.success.to_le_bytes());
    hasher.update(out.result.failed_index.to_le_bytes());
    hasher.update(out.result.total_writes.to_le_bytes());
    hasher.update(out.metrics.submitted_ops.to_le_bytes());
    hasher.update(out.metrics.return_code.to_le_bytes());
    hasher.finalize().into()
}

fn build_batch_state_root(base: [u8; 32], batch: &LocalBatch) -> [u8; 32] {
    let mut hasher = Sha256::new();
    hasher.update(base);
    hasher.update(batch.id.to_le_bytes());
    hasher.update((batch.txs.len() as u64).to_le_bytes());
    for tx in &batch.txs {
        hasher.update(hash_local_tx(tx));
    }
    hasher.finalize().into()
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
    hasher.update(header.governance_chain_audit_root);
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
        governance_chain_audit_root: closure.governance_chain_audit_root,
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
        governance_chain_audit_root: header.governance_chain_audit_root,
        tx_count: header.tx_count,
        batch_count: header.batch_count,
        consensus_binding: header.consensus_binding,
    }
}

fn synthetic_state_root(height: u64) -> [u8; 32] {
    let mut hasher = Sha256::new();
    hasher.update(b"novovm_header_sync_state_root_v1");
    hasher.update(height.to_le_bytes());
    hasher.finalize().into()
}

fn synthetic_governance_chain_audit_root(height: u64) -> [u8; 32] {
    let mut hasher = Sha256::new();
    hasher.update(b"novovm_header_sync_governance_chain_audit_root_v1");
    hasher.update(height.to_le_bytes());
    hasher.finalize().into()
}

fn compute_header_wire_hash(header: &BlockHeaderWireV1) -> [u8; 32] {
    let mut hasher = Sha256::new();
    hasher.update(b"novovm_header_sync_hash_v1");
    hasher.update(header.height.to_le_bytes());
    hasher.update(header.epoch_id.to_le_bytes());
    hasher.update(header.parent_hash);
    hasher.update(header.state_root);
    hasher.update(header.governance_chain_audit_root);
    hasher.update(header.tx_count.to_le_bytes());
    hasher.update(header.batch_count.to_le_bytes());
    hasher.update(header.consensus_binding.plugin_class_code.to_le_bytes());
    hasher.update(header.consensus_binding.adapter_hash);
    hasher.finalize().into()
}

fn build_synthetic_header_chain(
    total_headers: u64,
    binding: ConsensusPluginBindingV1,
) -> Vec<HeaderChainEntry> {
    let mut chain = Vec::with_capacity(total_headers as usize);
    let mut parent_hash = [0u8; 32];
    for height in 0..total_headers {
        let header = BlockHeaderWireV1 {
            height,
            epoch_id: 1,
            parent_hash,
            state_root: synthetic_state_root(height),
            governance_chain_audit_root: synthetic_governance_chain_audit_root(height),
            tx_count: height.saturating_add(1),
            batch_count: 1,
            consensus_binding: binding,
        };
        let block_hash = compute_header_wire_hash(&header);
        parent_hash = block_hash;
        chain.push(HeaderChainEntry { header, block_hash });
    }
    chain
}

fn run_header_sync_probe(
    remote_headers: u64,
    local_headers: u64,
    fetch_limit: u64,
    tamper_parent_at: u64,
) -> Result<HeaderSyncSignal> {
    if remote_headers < 1 {
        bail!("remote_headers must be >= 1");
    }
    if local_headers < 1 {
        bail!("local_headers must be >= 1");
    }
    if local_headers > remote_headers {
        bail!(
            "local_headers ({}) cannot exceed remote_headers ({})",
            local_headers,
            remote_headers
        );
    }
    if fetch_limit < 1 {
        bail!("fetch_limit must be >= 1");
    }

    let binding = network_probe_consensus_binding();
    let remote_chain = build_synthetic_header_chain(remote_headers, binding);
    let remote_tip = remote_headers.saturating_sub(1);
    let local_tip_before = local_headers.saturating_sub(1);

    let mut local_tip_height = local_tip_before;
    let mut local_tip_hash = remote_chain[local_tip_height as usize].block_hash;
    let mut fetched = 0u64;
    let mut applied = 0u64;
    let mut reason = "ok".to_string();

    let start = local_headers;
    let end_exclusive = std::cmp::min(remote_headers, start.saturating_add(fetch_limit));
    for idx in start..end_exclusive {
        let entry = &remote_chain[idx as usize];
        let mut outbound = entry.header;
        if tamper_parent_at > 0 && outbound.height == tamper_parent_at {
            outbound.parent_hash[0] ^= 0xA5;
        }

        fetched = fetched.saturating_add(1);
        let wire = encode_block_header_wire_v1(&outbound);
        let decoded = decode_block_header_wire_v1(&wire).with_context(|| {
            format!("decode synced header failed at height {}", outbound.height)
        })?;

        let expected_height = local_tip_height.saturating_add(1);
        if decoded.height != expected_height {
            reason = format!(
                "height_mismatch_at_{}_expected_{}",
                decoded.height, expected_height
            );
            break;
        }
        if decoded.parent_hash != local_tip_hash {
            reason = format!("parent_hash_mismatch_at_{}", decoded.height);
            break;
        }
        if let Err(err) = verify_consensus_plugin_binding(binding, decoded.consensus_binding) {
            reason = format!("consensus_binding_mismatch_at_{}_{}", decoded.height, err);
            break;
        }

        local_tip_height = decoded.height;
        local_tip_hash = compute_header_wire_hash(&decoded);
        applied = applied.saturating_add(1);
    }

    let complete = local_tip_height == remote_tip;
    let pass = complete;
    if pass {
        reason = "ok".to_string();
    } else if reason == "ok" {
        reason = "incomplete_sync".to_string();
    }

    Ok(HeaderSyncSignal {
        mode: "headers_first",
        codec: BLOCK_HEADER_WIRE_V1_CODEC,
        remote_tip,
        local_tip_before,
        fetched,
        applied,
        local_tip_after: local_tip_height,
        complete,
        pass,
        tamper_at: tamper_parent_at,
        reason,
    })
}

fn build_synthetic_snapshot_state(height: u64) -> Vec<(u64, u64)> {
    vec![
        (1001, 10 + height),
        (1002, 20 + height.saturating_mul(2)),
        (1003, 30 + height.saturating_mul(3)),
        (1004, 40 + height.saturating_mul(4)),
    ]
}

fn compute_snapshot_root(entries: &[(u64, u64)]) -> [u8; 32] {
    let mut sorted = entries.to_vec();
    sorted.sort_by_key(|(k, _)| *k);
    let mut hasher = Sha256::new();
    hasher.update(b"novovm_state_sync_snapshot_root_v1");
    for (k, v) in sorted {
        hasher.update(k.to_le_bytes());
        hasher.update(v.to_le_bytes());
    }
    hasher.finalize().into()
}

fn build_snapshot_header_chain(
    total_headers: u64,
    binding: ConsensusPluginBindingV1,
) -> Vec<HeaderChainEntry> {
    let mut chain = Vec::with_capacity(total_headers as usize);
    let mut parent_hash = [0u8; 32];
    for height in 0..total_headers {
        let snapshot = build_synthetic_snapshot_state(height);
        let header = BlockHeaderWireV1 {
            height,
            epoch_id: 1,
            parent_hash,
            state_root: compute_snapshot_root(&snapshot),
            governance_chain_audit_root: synthetic_governance_chain_audit_root(height),
            tx_count: height.saturating_add(1),
            batch_count: 1,
            consensus_binding: binding,
        };
        let block_hash = compute_header_wire_hash(&header);
        parent_hash = block_hash;
        chain.push(HeaderChainEntry { header, block_hash });
    }
    chain
}

fn run_fast_state_sync_probe(
    remote_headers: u64,
    local_headers: u64,
    fetch_limit: u64,
    tamper_snapshot_at: u64,
) -> Result<FastStateSyncSignal> {
    if remote_headers < 1 {
        bail!("remote_headers must be >= 1");
    }
    if local_headers < 1 {
        bail!("local_headers must be >= 1");
    }
    if local_headers > remote_headers {
        bail!(
            "local_headers ({}) cannot exceed remote_headers ({})",
            local_headers,
            remote_headers
        );
    }
    if fetch_limit < 1 {
        bail!("fetch_limit must be >= 1");
    }

    let binding = network_probe_consensus_binding();
    let remote_chain = build_snapshot_header_chain(remote_headers, binding);
    let remote_tip = remote_headers.saturating_sub(1);
    let local_tip_before = local_headers.saturating_sub(1);

    let mut local_tip_height = local_tip_before;
    let mut local_tip_hash = remote_chain[local_tip_height as usize].block_hash;
    let mut fetched_headers = 0u64;
    let mut applied_headers = 0u64;
    let mut reason = "ok".to_string();

    let start = local_headers;
    let end_exclusive = std::cmp::min(remote_headers, start.saturating_add(fetch_limit));
    for idx in start..end_exclusive {
        let entry = &remote_chain[idx as usize];
        fetched_headers = fetched_headers.saturating_add(1);
        let wire = encode_block_header_wire_v1(&entry.header);
        let decoded = decode_block_header_wire_v1(&wire).with_context(|| {
            format!(
                "decode fast-sync header failed at height {}",
                entry.header.height
            )
        })?;

        let expected_height = local_tip_height.saturating_add(1);
        if decoded.height != expected_height {
            reason = format!(
                "height_mismatch_at_{}_expected_{}",
                decoded.height, expected_height
            );
            break;
        }
        if decoded.parent_hash != local_tip_hash {
            reason = format!("parent_hash_mismatch_at_{}", decoded.height);
            break;
        }
        if let Err(err) = verify_consensus_plugin_binding(binding, decoded.consensus_binding) {
            reason = format!("consensus_binding_mismatch_at_{}_{}", decoded.height, err);
            break;
        }

        local_tip_height = decoded.height;
        local_tip_hash = compute_header_wire_hash(&decoded);
        applied_headers = applied_headers.saturating_add(1);
    }

    let fast_complete = local_tip_height == remote_tip;
    if !fast_complete && reason == "ok" {
        reason = "fast_sync_incomplete".to_string();
    }

    let snapshot_height = local_tip_height;
    let mut snapshot = build_synthetic_snapshot_state(snapshot_height);
    if tamper_snapshot_at > 0 && snapshot_height == tamper_snapshot_at && !snapshot.is_empty() {
        snapshot[0].1 = snapshot[0].1.saturating_add(999);
    }
    let snapshot_accounts = snapshot.len() as u64;
    let snapshot_root = compute_snapshot_root(&snapshot);
    let expected_root = remote_chain[snapshot_height as usize].header.state_root;
    let snapshot_verified = snapshot_root == expected_root;
    if !snapshot_verified && reason == "ok" {
        reason = format!("snapshot_root_mismatch_at_{}", snapshot_height);
    }

    let state_complete = fast_complete && snapshot_verified;
    let pass = state_complete;
    if pass {
        reason = "ok".to_string();
    }

    Ok(FastStateSyncSignal {
        mode: "fast_state_sync",
        codec: BLOCK_HEADER_WIRE_V1_CODEC,
        remote_tip,
        local_tip_before,
        fetched_headers,
        applied_headers,
        local_tip_after: local_tip_height,
        fast_complete,
        snapshot_height,
        snapshot_accounts,
        snapshot_verified,
        state_complete,
        pass,
        tamper_snapshot_at,
        reason,
    })
}

fn run_network_dos_probe(
    invalid_peers: u64,
    invalid_burst: u64,
    ban_after: u64,
) -> Result<NetworkDosSignal> {
    if invalid_peers < 1 {
        bail!("invalid_peers must be >= 1");
    }
    if invalid_burst < 1 {
        bail!("invalid_burst must be >= 1");
    }
    if ban_after < 1 {
        bail!("ban_after must be >= 1");
    }

    #[derive(Clone, Copy)]
    struct PeerState {
        offenses: u64,
        banned: bool,
    }

    let local = NodeId(0);
    let total_peers = invalid_peers.saturating_add(1); // one healthy peer
    let healthy_peer_id = total_peers;
    let binding = network_probe_consensus_binding();
    let mut peer_state: HashMap<u64, PeerState> = HashMap::new();
    for pid in 1..=total_peers {
        peer_state.insert(
            pid,
            PeerState {
                offenses: 0,
                banned: false,
            },
        );
    }

    let mut invalid_detected = 0u64;
    let mut bans = 0u64;
    let mut storm_rejected = 0u64;
    let mut healthy_accepts = 0u64;
    let mut reason = "ok".to_string();

    // invalid-block-storm from invalid peers
    for pid in 1..=invalid_peers {
        for _ in 0..invalid_burst {
            let state = peer_state
                .get_mut(&pid)
                .ok_or_else(|| anyhow::anyhow!("missing peer state: {}", pid))?;
            if state.banned {
                // banned peer traffic is rejected before decode/verify
                storm_rejected = storm_rejected.saturating_add(1);
                continue;
            }

            let payload = build_network_probe_block_wire_payload(
                NodeId(pid),
                local,
                binding,
                "hash_mismatch",
            );
            if let Ok(header) = decode_block_header_wire_v1(&payload) {
                if verify_consensus_plugin_binding(binding, header.consensus_binding).is_err() {
                    invalid_detected = invalid_detected.saturating_add(1);
                    state.offenses = state.offenses.saturating_add(1);
                    if state.offenses >= ban_after {
                        state.banned = true;
                        bans = bans.saturating_add(1);
                    }
                }
            }
        }

        // try one valid payload after storm; banned peers should be rejected.
        let state = peer_state
            .get(&pid)
            .ok_or_else(|| anyhow::anyhow!("missing peer state: {}", pid))?;
        if state.banned {
            storm_rejected = storm_rejected.saturating_add(1);
        }
    }

    // healthy peer sends valid payload and must pass
    let valid_payload =
        build_network_probe_block_wire_payload(NodeId(healthy_peer_id), local, binding, "");
    let valid_header = decode_block_header_wire_v1(&valid_payload)
        .context("decode valid block wire in dos probe failed")?;
    if verify_consensus_plugin_binding(binding, valid_header.consensus_binding).is_ok() {
        healthy_accepts = healthy_accepts.saturating_add(1);
    } else {
        reason = "healthy_binding_failed".to_string();
    }

    let pass = bans == invalid_peers
        && healthy_accepts >= 1
        && storm_rejected >= invalid_peers
        && invalid_detected >= invalid_peers.saturating_mul(ban_after);
    if !pass && reason == "ok" {
        reason = "dos_gate_assertion_failed".to_string();
    }

    Ok(NetworkDosSignal {
        mode: "peer_score_ban",
        codec: BLOCK_HEADER_WIRE_V1_CODEC,
        peers: total_peers,
        invalid_peers,
        invalid_burst,
        ban_after,
        invalid_detected,
        bans,
        storm_rejected,
        healthy_accepts,
        pass,
        reason,
    })
}

fn run_pacemaker_failover_probe(nodes: u64, failed_leader: u64) -> Result<PacemakerFailoverSignal> {
    if nodes < 4 {
        bail!(
            "pacemaker failover requires at least 4 validators (got {})",
            nodes
        );
    }
    if nodes > (u32::MAX as u64).saturating_add(1) {
        bail!("nodes exceeds u32 range: {}", nodes);
    }
    if failed_leader >= nodes {
        bail!(
            "failed_leader ({}) must be in [0, {})",
            failed_leader,
            nodes
        );
    }
    // Current probe assumes initial leader is validator-0 (protocol default).
    if failed_leader != 0 {
        bail!(
            "pacemaker failover probe currently supports failed_leader=0 only (got {})",
            failed_leader
        );
    }

    let failed_leader_u32 = failed_leader as u32;
    let validator_ids: Vec<u32> = (0..nodes).map(|id| id as u32).collect();
    let active_ids: Vec<u32> = validator_ids
        .iter()
        .copied()
        .filter(|id| *id != failed_leader_u32)
        .collect();
    if active_ids.len() < 3 {
        bail!("active validator count must be >= 3 after failover");
    }
    let active_set: HashSet<u32> = active_ids.iter().copied().collect();
    let validator_set = ValidatorSet::new_equal_weight(validator_ids.clone());
    let timeout_quorum = validator_set.quorum_size();
    if (active_ids.len() as u64) < timeout_quorum {
        bail!(
            "active validators ({}) cannot form quorum ({}) after leader failure",
            active_ids.len(),
            timeout_quorum
        );
    }

    let mut signing_keys = Vec::with_capacity(nodes as usize);
    for _ in 0..nodes {
        signing_keys.push(SigningKey::generate(&mut OsRng));
    }

    let initial_view = 0u64;
    let mut bootstrap = HotStuffProtocol::new(validator_set.clone(), 0u32)
        .context("create bootstrap protocol failed")?;
    let next_leader = bootstrap
        .trigger_view_change()
        .context("bootstrap view-change failed")?;
    let next_view = bootstrap.current_view();
    if next_view != initial_view.saturating_add(1) {
        bail!(
            "unexpected next view: expected {}, got {}",
            initial_view.saturating_add(1),
            next_view
        );
    }
    if next_leader == failed_leader_u32 {
        bail!("view-change selected failed leader again: {}", next_leader);
    }

    let transport = InMemoryTransport::new(512);
    for id in &validator_ids {
        transport.register(NodeId(u64::from(*id)));
    }

    let mut local_view_advanced = 0u64;
    let mut reason = "ok".to_string();

    // Active validators timeout on failed leader and locally advance to next view/leader.
    for id in &active_ids {
        let mut protocol = HotStuffProtocol::new(validator_set.clone(), *id)
            .with_context(|| format!("create protocol failed for validator {}", id))?;
        let rotated = protocol
            .trigger_view_change()
            .with_context(|| format!("trigger view-change failed for validator {}", id))?;
        let state = protocol.get_state();
        if rotated == next_leader && state.view == next_view && state.leader_id == next_leader {
            local_view_advanced = local_view_advanced.saturating_add(1);
        }

        let view_sync = ProtocolMessage::Pacemaker(ProtocolPacemakerMessage::ViewSync {
            from: NodeId(u64::from(*id)),
            height: state.height,
            view: state.view,
            leader: NodeId(u64::from(state.leader_id)),
        });
        transport
            .send(NodeId(u64::from(next_leader)), view_sync)
            .with_context(|| format!("send view-sync failed from validator {}", id))?;

        let new_view = ProtocolMessage::Pacemaker(ProtocolPacemakerMessage::NewView {
            from: NodeId(u64::from(*id)),
            height: state.height,
            view: state.view,
            high_qc_height: state.height,
        });
        transport
            .send(NodeId(u64::from(next_leader)), new_view)
            .with_context(|| format!("send new-view failed from validator {}", id))?;
    }

    let mut view_sync_from: HashSet<u32> = HashSet::new();
    let mut new_view_from: HashSet<u32> = HashSet::new();
    let mut timeout_votes = 0u64;
    loop {
        let msg = transport
            .try_recv(NodeId(u64::from(next_leader)))
            .context("receive pacemaker failover message failed")?;
        let Some(msg) = msg else {
            break;
        };
        match msg {
            ProtocolMessage::Pacemaker(ProtocolPacemakerMessage::ViewSync {
                from,
                height,
                view,
                leader,
            }) => {
                if let Ok(src) = u32::try_from(from.0) {
                    if active_set.contains(&src)
                        && height == 0
                        && view == next_view
                        && leader == NodeId(u64::from(next_leader))
                        && view_sync_from.insert(src)
                    {
                        timeout_votes = timeout_votes.saturating_add(1);
                    }
                }
            }
            ProtocolMessage::Pacemaker(ProtocolPacemakerMessage::NewView {
                from,
                height,
                view,
                high_qc_height,
            }) => {
                if let Ok(src) = u32::try_from(from.0) {
                    if active_set.contains(&src)
                        && height == 0
                        && view == next_view
                        && high_qc_height <= height
                    {
                        new_view_from.insert(src);
                    }
                }
            }
            _ => {}
        }
    }

    let timeout_cert = timeout_votes >= timeout_quorum;
    let view_sync_votes = view_sync_from.len() as u64;
    let new_view_votes = new_view_from.len() as u64;

    let mut qc_formed = false;
    let mut committed = false;
    let mut committed_height = 0u64;

    if timeout_cert
        && local_view_advanced >= timeout_quorum
        && view_sync_votes >= timeout_quorum
        && new_view_votes >= timeout_quorum
    {
        let mut leader_protocol = HotStuffProtocol::new(validator_set.clone(), next_leader)
            .context("create next-leader protocol failed")?;
        let rotated = leader_protocol
            .trigger_view_change()
            .context("next-leader trigger view-change failed")?;
        if rotated != next_leader {
            reason = "next_leader_rotation_mismatch".to_string();
        } else {
            let mut epoch = ConsensusEpoch::new(0, leader_protocol.current_height(), next_leader);
            epoch.add_batch(1, 64);
            let mut batch_results = HashMap::new();
            batch_results.insert(1, synthetic_state_root(1));
            epoch
                .compute_state_root(&batch_results)
                .context("compute epoch state root in failover probe failed")?;

            let proposal = leader_protocol
                .propose(&epoch)
                .context("failover probe proposal failed")?;
            let leader_state = leader_protocol.get_state();

            let mut qc_opt = None;
            for voter_id in &active_ids {
                let mut voter = HotStuffProtocol::new(validator_set.clone(), *voter_id)
                    .with_context(|| {
                        format!("create voter protocol failed for validator {}", voter_id)
                    })?;
                let _ = voter
                    .trigger_view_change()
                    .with_context(|| format!("voter {} trigger view-change failed", voter_id))?;
                voter.sync_state(leader_state.clone());
                let vote = voter
                    .vote(&proposal, &signing_keys[*voter_id as usize])
                    .with_context(|| format!("voter {} vote failed", voter_id))?;
                if let Some(qc) = leader_protocol
                    .collect_vote(vote)
                    .with_context(|| format!("collect vote from {} failed", voter_id))?
                {
                    qc_opt = Some(qc);
                    break;
                }
            }

            if let Some(qc) = qc_opt {
                qc_formed = true;
                leader_protocol
                    .pre_commit(&qc)
                    .context("failover probe pre-commit failed")?;
                leader_protocol
                    .commit()
                    .context("failover probe commit failed")?;
                committed = true;
                committed_height = leader_protocol.current_height();
            }
        }
    }

    let pass = timeout_cert
        && local_view_advanced >= timeout_quorum
        && view_sync_votes >= timeout_quorum
        && new_view_votes >= timeout_quorum
        && qc_formed
        && committed
        && committed_height >= 1
        && u64::from(next_leader) != failed_leader;

    if !pass && reason == "ok" {
        reason = if !timeout_cert {
            "timeout_cert_not_formed".to_string()
        } else if local_view_advanced < timeout_quorum {
            "local_view_change_not_quorum".to_string()
        } else if view_sync_votes < timeout_quorum {
            "view_sync_quorum_not_met".to_string()
        } else if new_view_votes < timeout_quorum {
            "new_view_quorum_not_met".to_string()
        } else if !qc_formed {
            "qc_not_formed_after_failover".to_string()
        } else if !committed || committed_height < 1 {
            "post_failover_commit_failed".to_string()
        } else {
            "pacemaker_failover_assertion_failed".to_string()
        };
    }

    Ok(PacemakerFailoverSignal {
        mode: "timeout_view_sync_new_view_failover",
        transport: "in_memory",
        nodes,
        failed_leader,
        initial_view,
        next_view,
        next_leader: u64::from(next_leader),
        timeout_votes,
        timeout_quorum,
        timeout_cert,
        local_view_advanced,
        view_sync_votes,
        new_view_votes,
        qc_formed,
        committed,
        committed_height,
        pass,
        reason,
    })
}

fn to_hex(bytes: &[u8]) -> String {
    let mut out = String::with_capacity(bytes.len() * 2);
    for b in bytes {
        use std::fmt::Write as _;
        let _ = write!(out, "{:02x}", b);
    }
    out
}

fn chain_query_db_path() -> PathBuf {
    std::env::var("NOVOVM_CHAIN_QUERY_DB")
        .map(PathBuf::from)
        .unwrap_or_else(|_| PathBuf::from("artifacts/novovm-chain-query-db.json"))
}

fn governance_audit_db_path(query_db_path: &Path) -> PathBuf {
    if let Some(custom) = string_env_nonempty("NOVOVM_GOVERNANCE_AUDIT_DB") {
        return PathBuf::from(custom);
    }
    if let Some(parent) = query_db_path.parent() {
        if !parent.as_os_str().is_empty() {
            return parent.join("governance-audit-events.json");
        }
    }
    PathBuf::from("artifacts/novovm-governance-audit-events.json")
}

fn governance_chain_audit_db_path(query_db_path: &Path) -> PathBuf {
    if let Some(custom) = string_env_nonempty("NOVOVM_GOVERNANCE_CHAIN_AUDIT_DB") {
        return PathBuf::from(custom);
    }
    if let Some(parent) = query_db_path.parent() {
        if !parent.as_os_str().is_empty() {
            return parent.join("governance-chain-audit-events.json");
        }
    }
    PathBuf::from("artifacts/novovm-governance-chain-audit-events.json")
}

fn local_tx_hash(tx: &LocalTx) -> [u8; 32] {
    let mut hasher = Sha256::new();
    hasher.update(LOCAL_TX_HASH_DOMAIN);
    hasher.update(tx.account.to_le_bytes());
    hasher.update(tx.key.to_le_bytes());
    hasher.update(tx.value.to_le_bytes());
    hasher.update(tx.nonce.to_le_bytes());
    hasher.update(tx.fee.to_le_bytes());
    hasher.update(tx.signature);
    hasher.finalize().into()
}

fn load_query_state_db(path: &Path) -> Result<QueryStateDb> {
    if !path.exists() {
        return Ok(QueryStateDb::default());
    }
    let raw = fs::read_to_string(path)
        .with_context(|| format!("read query db failed: {}", path.display()))?;
    let normalized = raw.trim_start_matches('\u{feff}');
    if normalized.trim().is_empty() {
        return Ok(QueryStateDb::default());
    }
    let parsed: QueryStateDb = serde_json::from_str(normalized)
        .with_context(|| format!("parse query db failed: {}", path.display()))?;
    Ok(parsed)
}

fn save_query_state_db(path: &Path, db: &QueryStateDb) -> Result<()> {
    if let Some(parent) = path.parent() {
        if !parent.as_os_str().is_empty() {
            fs::create_dir_all(parent).with_context(|| {
                format!("create query db parent dir failed: {}", parent.display())
            })?;
        }
    }
    let serialized = serde_json::to_string_pretty(db).context("serialize query db json failed")?;
    fs::write(path, serialized)
        .with_context(|| format!("write query db failed: {}", path.display()))?;
    Ok(())
}

fn load_governance_audit_store(path: &Path) -> Result<GovernanceRpcAuditStore> {
    if !path.exists() {
        return Ok(GovernanceRpcAuditStore::default());
    }
    let raw = fs::read_to_string(path)
        .with_context(|| format!("read governance audit store failed: {}", path.display()))?;
    let normalized = raw.trim_start_matches('\u{feff}');
    if normalized.trim().is_empty() {
        return Ok(GovernanceRpcAuditStore::default());
    }
    let parsed: GovernanceRpcAuditStore = serde_json::from_str(normalized)
        .with_context(|| format!("parse governance audit store failed: {}", path.display()))?;
    Ok(parsed)
}

fn save_governance_audit_store(
    path: &Path,
    next_seq: u64,
    events: &[GovernanceRpcAuditEvent],
) -> Result<()> {
    if let Some(parent) = path.parent() {
        if !parent.as_os_str().is_empty() {
            fs::create_dir_all(parent).with_context(|| {
                format!(
                    "create governance audit store parent dir failed: {}",
                    parent.display()
                )
            })?;
        }
    }
    let store = GovernanceRpcAuditStore {
        next_seq,
        events: events.to_vec(),
    };
    let serialized =
        serde_json::to_string_pretty(&store).context("serialize governance audit store failed")?;
    fs::write(path, serialized)
        .with_context(|| format!("write governance audit store failed: {}", path.display()))?;
    Ok(())
}

fn load_governance_chain_audit_store(path: &Path) -> Result<GovernanceChainAuditStore> {
    if !path.exists() {
        return Ok(GovernanceChainAuditStore::default());
    }
    let raw = fs::read_to_string(path)
        .with_context(|| format!("read governance chain audit store failed: {}", path.display()))?;
    let normalized = raw.trim_start_matches('\u{feff}');
    if normalized.trim().is_empty() {
        return Ok(GovernanceChainAuditStore::default());
    }
    let parsed: GovernanceChainAuditStore = serde_json::from_str(normalized)
        .with_context(|| format!("parse governance chain audit store failed: {}", path.display()))?;
    Ok(parsed)
}

fn save_governance_chain_audit_store(
    path: &Path,
    events: &[GovernanceChainAuditEvent],
    root: [u8; 32],
) -> Result<()> {
    if let Some(parent) = path.parent() {
        if !parent.as_os_str().is_empty() {
            fs::create_dir_all(parent).with_context(|| {
                format!(
                    "create governance chain audit store parent dir failed: {}",
                    parent.display()
                )
            })?;
        }
    }
    let head_seq = events.last().map(|event| event.seq).unwrap_or(0);
    let store = GovernanceChainAuditStore {
        events: events.to_vec(),
        head_seq,
        root_hex: to_hex(&root),
    };
    let serialized = serde_json::to_string_pretty(&store)
        .context("serialize governance chain audit store failed")?;
    fs::write(path, serialized)
        .with_context(|| format!("write governance chain audit store failed: {}", path.display()))?;
    Ok(())
}

fn apply_block_to_query_db(db: &mut QueryStateDb, block: &LocalBlock) {
    let block_hash = to_hex(&block.block_hash);
    let state_root = to_hex(&block.header.state_root);
    let governance_chain_audit_root = to_hex(&block.header.governance_chain_audit_root);
    let block_record = QueryBlockRecord {
        height: block.header.height,
        epoch_id: block.header.epoch_id,
        parent_hash: to_hex(&block.header.parent_hash),
        state_root: state_root.clone(),
        governance_chain_audit_root,
        tx_count: block.header.tx_count,
        batch_count: block.header.batch_count,
        proposal_hash: to_hex(&block.proposal_hash),
        block_hash: block_hash.clone(),
    };
    db.blocks.push(block_record);

    for batch in &block.batches {
        for tx in &batch.txs {
            let tx_hash = to_hex(&local_tx_hash(tx));
            let account = tx.account.to_string();
            db.balances.insert(account, tx.value);

            db.txs.insert(
                tx_hash.clone(),
                QueryTxRecord {
                    tx_hash: tx_hash.clone(),
                    block_height: block.header.height,
                    block_hash: block_hash.clone(),
                    account: tx.account,
                    key: tx.key,
                    value: tx.value,
                    nonce: tx.nonce,
                    fee: tx.fee,
                    success: true,
                },
            );

            db.receipts.insert(
                tx_hash.clone(),
                QueryReceiptRecord {
                    tx_hash,
                    block_height: block.header.height,
                    block_hash: block_hash.clone(),
                    success: true,
                    gas_used: tx.fee,
                    state_root: state_root.clone(),
                },
            );
        }
    }
}

fn persist_query_state_for_block(block: &LocalBlock) -> Result<(PathBuf, QueryStateDb)> {
    let path = chain_query_db_path();
    let mut db = load_query_state_db(&path)?;
    apply_block_to_query_db(&mut db, block);
    save_query_state_db(&path, &db)?;
    Ok((path, db))
}

fn run_batch_a_minimal_closure(
    batches: &[LocalBatch],
    consensus_binding: ConsensusPluginBindingV1,
    execution_state_root: [u8; 32],
    slash_policy: &SlashPolicy,
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
    engine
        .set_slash_policy(slash_policy.clone())
        .context("set slash policy failed")?;
    let applied_policy = engine.slash_policy();
    println!(
        "slash_policy_bind: mode={} threshold={} min_validators={}",
        applied_policy.mode.as_str(),
        applied_policy.equivocation_threshold,
        applied_policy.min_active_validators
    );

    engine.start_epoch().context("start epoch failed")?;
    for batch in batches {
        engine
            .add_batch(batch.id, batch.txs.len() as u64)
            .with_context(|| format!("add batch {} failed", batch.id))?;
    }

    let mut batch_results = HashMap::new();
    for batch in batches {
        batch_results.insert(
            batch.id,
            build_batch_state_root(execution_state_root, batch),
        );
    }

    let proposal = engine
        .propose_epoch_with_state_root(&batch_results, execution_state_root)
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
    let governance_chain_audit_root = engine.governance_chain_audit_root();
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
        governance_chain_audit_root,
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
        "block_out: height={} epoch={} batches={} txs={} block_hash={} state_root={} governance_chain_audit_root={} proposal_hash={}",
        block.header.height,
        block.header.epoch_id,
        block.batches.len(),
        block.header.tx_count,
        to_hex(&block.block_hash),
        to_hex(&block.header.state_root),
        to_hex(&block.header.governance_chain_audit_root),
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
        .cloned()
        .ok_or_else(|| anyhow::anyhow!("missing latest block after commit"))?;
    let total_blocks = store.total_blocks();
    drop(store);

    let (query_db_path, query_db) = persist_query_state_for_block(&latest)?;
    println!(
        "commit_out: store=in_memory committed=true height={} total_blocks={} block_hash={} state_root={} governance_chain_audit_root={}",
        latest.header.height,
        total_blocks,
        to_hex(&latest.block_hash),
        to_hex(&latest.header.state_root),
        to_hex(&latest.header.governance_chain_audit_root)
    );
    println!(
        "commit_consensus: plugin_class={} plugin_hash={} pass=true",
        plugin_class_name(latest.header.consensus_binding.plugin_class_code),
        to_hex(&latest.header.consensus_binding.adapter_hash)
    );
    println!(
        "query_state_out: db={} blocks={} txs={} receipts={} balances={}",
        query_db_path.display(),
        query_db.blocks.len(),
        query_db.txs.len(),
        query_db.receipts.len(),
        query_db.balances.len()
    );
    Ok(())
}

#[derive(Debug, Clone, Copy)]
struct RpcRateCounter {
    window_sec: u64,
    count: u32,
}

#[derive(Debug, Clone, Serialize)]
struct GovernanceRpcProposalView {
    proposal_id: u64,
    proposer: u32,
    created_height: u64,
    proposal_digest: String,
    op: String,
    payload: serde_json::Value,
    votes_collected: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct GovernanceRpcAuditEvent {
    seq: u64,
    ts_sec: u64,
    action: String,
    proposal_id: u64,
    actor: Option<u32>,
    outcome: String,
    detail: String,
}

struct GovernanceRpcRuntime {
    engine: BFTEngine,
    signers: HashMap<ConsensusNodeId, SigningKey>,
    votes: HashMap<u64, Vec<GovernanceVote>>,
    signed_votes: HashMap<(u64, ConsensusNodeId, bool), Vec<u8>>,
    proposer_allowlist: HashSet<ConsensusNodeId>,
    executor_allowlist: HashSet<ConsensusNodeId>,
    audit_events: Vec<GovernanceRpcAuditEvent>,
    next_audit_seq: u64,
    audit_store_path: PathBuf,
    chain_audit_store_path: PathBuf,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
struct GovernanceRpcAuditStore {
    next_seq: u64,
    events: Vec<GovernanceRpcAuditEvent>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
struct GovernanceChainAuditStore {
    #[serde(default)]
    events: Vec<GovernanceChainAuditEvent>,
    #[serde(default)]
    head_seq: u64,
    #[serde(default)]
    root_hex: String,
}

fn value_to_u64(v: &serde_json::Value) -> Option<u64> {
    match v {
        serde_json::Value::Number(n) => n.as_u64(),
        serde_json::Value::String(s) => s.trim().parse::<u64>().ok(),
        _ => None,
    }
}

fn value_to_i64(v: &serde_json::Value) -> Option<i64> {
    match v {
        serde_json::Value::Number(n) => n.as_i64(),
        serde_json::Value::String(s) => s.trim().parse::<i64>().ok(),
        _ => None,
    }
}

fn param_as_u64(params: &serde_json::Value, key: &str) -> Option<u64> {
    match params {
        serde_json::Value::Object(map) => map.get(key).and_then(value_to_u64),
        serde_json::Value::Array(arr) => {
            if key == "height" {
                arr.first().and_then(value_to_u64)
            } else {
                None
            }
        }
        _ => None,
    }
}

fn param_as_i64(params: &serde_json::Value, key: &str) -> Option<i64> {
    match params {
        serde_json::Value::Object(map) => map.get(key).and_then(value_to_i64),
        _ => None,
    }
}

fn value_to_string(v: &serde_json::Value) -> Option<String> {
    match v {
        serde_json::Value::String(s) => Some(s.trim().to_string()),
        serde_json::Value::Number(n) => Some(n.to_string()),
        _ => None,
    }
}

fn param_as_string(params: &serde_json::Value, key: &str) -> Option<String> {
    match params {
        serde_json::Value::Object(map) => map.get(key).and_then(value_to_string),
        serde_json::Value::Array(arr) => {
            if key == "tx_hash" || key == "account" {
                arr.first().and_then(value_to_string)
            } else {
                None
            }
        }
        _ => None,
    }
}

fn value_to_bool(v: &serde_json::Value) -> Option<bool> {
    match v {
        serde_json::Value::Bool(b) => Some(*b),
        serde_json::Value::String(s) => {
            let t = s.trim();
            if t.eq_ignore_ascii_case("true") || t == "1" {
                Some(true)
            } else if t.eq_ignore_ascii_case("false") || t == "0" {
                Some(false)
            } else {
                None
            }
        }
        serde_json::Value::Number(n) => n.as_u64().map(|v| v != 0),
        _ => None,
    }
}

fn param_as_bool(params: &serde_json::Value, key: &str) -> Option<bool> {
    match params {
        serde_json::Value::Object(map) => map.get(key).and_then(value_to_bool),
        serde_json::Value::Array(arr) => {
            if key == "support" {
                arr.first().and_then(value_to_bool)
            } else {
                None
            }
        }
        _ => None,
    }
}

fn param_as_u64_list(params: &serde_json::Value, key: &str) -> Option<Vec<u64>> {
    match params {
        serde_json::Value::Object(map) => map.get(key).and_then(|v| match v {
            serde_json::Value::Array(items) => {
                let mut out = Vec::with_capacity(items.len());
                for item in items {
                    let value = value_to_u64(item)?;
                    out.push(value);
                }
                Some(out)
            }
            serde_json::Value::String(s) => {
                let mut out = Vec::new();
                for token in s.split(',') {
                    let t = token.trim();
                    if t.is_empty() {
                        continue;
                    }
                    let parsed = t.parse::<u64>().ok()?;
                    out.push(parsed);
                }
                if out.is_empty() {
                    None
                } else {
                    Some(out)
                }
            }
            _ => None,
        }),
        _ => None,
    }
}

fn parse_governance_signature_scheme(
    params: &serde_json::Value,
) -> Result<GovernanceVoteVerifierScheme> {
    let raw = param_as_string(params, "signature_scheme")
        .or_else(|| param_as_string(params, "sig_alg"))
        .or_else(|| param_as_string(params, "scheme"));
    match raw {
        Some(value) => GovernanceVoteVerifierScheme::parse(&value).ok_or_else(|| {
            anyhow::anyhow!(
                "unsupported governance signature scheme: {} (valid: ed25519, mldsa87)",
                value
            )
        }),
        None => Ok(GovernanceVoteVerifierScheme::Ed25519),
    }
}

fn parse_governance_vote_verifier_config(
    raw: Option<&str>,
) -> Result<GovernanceVoteVerifierScheme> {
    match raw {
        Some(value) => GovernanceVoteVerifierScheme::parse(value).ok_or_else(|| {
            anyhow::anyhow!(
                "unsupported NOVOVM_GOVERNANCE_VOTE_VERIFIER: {} (valid: ed25519, mldsa87)",
                value
            )
        }),
        None => Ok(GovernanceVoteVerifierScheme::Ed25519),
    }
}

fn load_governance_vote_verifier_config_from_env() -> Result<GovernanceVoteVerifierScheme> {
    let configured = string_env_nonempty("NOVOVM_GOVERNANCE_VOTE_VERIFIER");
    parse_governance_vote_verifier_config(configured.as_deref())
}

const GOVERNANCE_MLDSA87_ENVELOPE_MAGIC: &[u8] = b"MLDSA87\0";
const GOVERNANCE_MLDSA87_LEVEL: u32 = 87;
const GOVERNANCE_AOEM_FFI_ABI_VERSION: u32 = 1;

type AoemAbiVersionFn = unsafe extern "C" fn() -> u32;
type AoemMldsaSupportedFn = unsafe extern "C" fn() -> u32;
type AoemMldsaPubkeySizeFn = unsafe extern "C" fn(level: u32) -> u32;
type AoemMldsaSignatureSizeFn = unsafe extern "C" fn(level: u32) -> u32;
type AoemMldsaVerifyFn = unsafe extern "C" fn(
    level: u32,
    pubkey_ptr: *const u8,
    pubkey_len: usize,
    message_ptr: *const u8,
    message_len: usize,
    signature_ptr: *const u8,
    signature_len: usize,
    out_valid: *mut u32,
) -> i32;

fn governance_vote_message_bytes(vote: &GovernanceVote) -> Vec<u8> {
    let mut message = Vec::with_capacity(8 + 8 + 8 + 32 + 1);
    message.extend_from_slice(b"GOV_VOTE_V1:");
    message.extend_from_slice(&vote.proposal_id.to_le_bytes());
    message.extend_from_slice(&vote.proposal_height.to_le_bytes());
    message.extend_from_slice(&vote.proposal_digest);
    message.push(if vote.support { 1 } else { 0 });
    message
}

fn encode_mldsa87_vote_signature_envelope(pubkey: &[u8], signature: &[u8]) -> Result<Vec<u8>> {
    if pubkey.is_empty() {
        bail!("mldsa_pubkey is empty");
    }
    if signature.is_empty() {
        bail!("signature is empty");
    }
    if pubkey.len() > u16::MAX as usize {
        bail!("mldsa_pubkey too large: {}", pubkey.len());
    }
    if signature.len() > u16::MAX as usize {
        bail!("signature too large: {}", signature.len());
    }
    let mut out = Vec::with_capacity(
        GOVERNANCE_MLDSA87_ENVELOPE_MAGIC.len() + 2 + 2 + pubkey.len() + signature.len(),
    );
    out.extend_from_slice(GOVERNANCE_MLDSA87_ENVELOPE_MAGIC);
    out.extend_from_slice(&(pubkey.len() as u16).to_le_bytes());
    out.extend_from_slice(&(signature.len() as u16).to_le_bytes());
    out.extend_from_slice(pubkey);
    out.extend_from_slice(signature);
    Ok(out)
}

fn decode_mldsa87_vote_signature_envelope(raw: &[u8]) -> Result<(&[u8], &[u8])> {
    let min = GOVERNANCE_MLDSA87_ENVELOPE_MAGIC.len() + 2 + 2;
    if raw.len() < min {
        bail!("mldsa87 signature envelope too short");
    }
    if &raw[..GOVERNANCE_MLDSA87_ENVELOPE_MAGIC.len()] != GOVERNANCE_MLDSA87_ENVELOPE_MAGIC {
        bail!("invalid mldsa87 signature envelope magic");
    }
    let mut offset = GOVERNANCE_MLDSA87_ENVELOPE_MAGIC.len();
    let pubkey_len = u16::from_le_bytes([raw[offset], raw[offset + 1]]) as usize;
    offset += 2;
    let signature_len = u16::from_le_bytes([raw[offset], raw[offset + 1]]) as usize;
    offset += 2;
    if pubkey_len == 0 || signature_len == 0 {
        bail!("mldsa87 signature envelope has empty pubkey or signature");
    }
    if raw.len() != offset + pubkey_len + signature_len {
        bail!("mldsa87 signature envelope length mismatch");
    }
    let pubkey = &raw[offset..offset + pubkey_len];
    let signature = &raw[offset + pubkey_len..];
    Ok((pubkey, signature))
}

fn parse_governance_mldsa87_pubkeys_from_env() -> Result<HashMap<ConsensusNodeId, Vec<u8>>> {
    let raw = std::env::var("NOVOVM_GOVERNANCE_MLDSA87_PUBKEYS").map_err(|_| {
        anyhow::anyhow!(
            "NOVOVM_GOVERNANCE_MLDSA87_PUBKEYS is required when NOVOVM_GOVERNANCE_VOTE_VERIFIER=mldsa87 and NOVOVM_GOVERNANCE_MLDSA_MODE=aoem_ffi"
        )
    })?;
    let mut out = HashMap::new();
    for token in raw.split(',') {
        let entry = token.trim();
        if entry.is_empty() {
            continue;
        }
        let mut parts = entry.splitn(2, ':');
        let id_raw = parts
            .next()
            .ok_or_else(|| anyhow::anyhow!("invalid mldsa pubkey mapping entry: {}", entry))?;
        let pubkey_hex = parts
            .next()
            .ok_or_else(|| anyhow::anyhow!("invalid mldsa pubkey mapping entry: {}", entry))?;
        let voter_id = id_raw
            .trim()
            .parse::<ConsensusNodeId>()
            .with_context(|| format!("invalid mldsa voter id in mapping: {}", id_raw.trim()))?;
        let pubkey = decode_hex_bytes(pubkey_hex.trim(), "mldsa_pubkey")?;
        out.insert(voter_id, pubkey);
    }
    if out.is_empty() {
        bail!("NOVOVM_GOVERNANCE_MLDSA87_PUBKEYS resolved to empty mapping");
    }
    Ok(out)
}

fn default_aoem_ffi_library_name() -> &'static str {
    #[cfg(target_os = "windows")]
    {
        return "aoem_ffi.dll";
    }
    #[cfg(target_os = "macos")]
    {
        return "libaoem_ffi.dylib";
    }
    #[cfg(all(not(target_os = "windows"), not(target_os = "macos")))]
    {
        "libaoem_ffi.so"
    }
}

fn resolve_aoem_ffi_library_path() -> PathBuf {
    string_env_nonempty("NOVOVM_AOEM_FFI_LIB_PATH")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from(default_aoem_ffi_library_name()))
}

struct AoemFfiMldsa87GovernanceVoteVerifier {
    verify_fn: AoemMldsaVerifyFn,
    expected_pubkey_size: usize,
    expected_signature_size: usize,
    voter_pubkeys: HashMap<ConsensusNodeId, Vec<u8>>,
}

impl GovernanceVoteVerifier for AoemFfiMldsa87GovernanceVoteVerifier {
    fn name(&self) -> &'static str {
        "mldsa87_aoem_ffi"
    }

    fn scheme(&self) -> GovernanceVoteVerifierScheme {
        GovernanceVoteVerifierScheme::MlDsa87
    }

    fn verify(
        &self,
        vote: &GovernanceVote,
        _key: &ed25519_dalek::VerifyingKey,
    ) -> std::result::Result<(), ConsensusBftError> {
        let (pubkey, signature) =
            decode_mldsa87_vote_signature_envelope(&vote.signature).map_err(|e| {
                ConsensusBftError::InvalidSignature(format!("invalid mldsa87 envelope: {}", e))
            })?;
        if pubkey.len() != self.expected_pubkey_size {
            return Err(ConsensusBftError::InvalidSignature(format!(
                "mldsa87 pubkey size mismatch: expected {} got {}",
                self.expected_pubkey_size,
                pubkey.len()
            )));
        }
        if signature.len() != self.expected_signature_size {
            return Err(ConsensusBftError::InvalidSignature(format!(
                "mldsa87 signature size mismatch: expected {} got {}",
                self.expected_signature_size,
                signature.len()
            )));
        }
        let expected_pubkey = self.voter_pubkeys.get(&vote.voter_id).ok_or_else(|| {
            ConsensusBftError::InvalidSignature(format!(
                "missing registered mldsa87 pubkey for voter {}",
                vote.voter_id
            ))
        })?;
        if expected_pubkey.as_slice() != pubkey {
            return Err(ConsensusBftError::InvalidSignature(format!(
                "mldsa87 pubkey mismatch for voter {}",
                vote.voter_id
            )));
        }

        let message = governance_vote_message_bytes(vote);
        let mut out_valid = 0u32;
        let rc = unsafe {
            (self.verify_fn)(
                GOVERNANCE_MLDSA87_LEVEL,
                pubkey.as_ptr(),
                pubkey.len(),
                message.as_ptr(),
                message.len(),
                signature.as_ptr(),
                signature.len(),
                &mut out_valid as *mut u32,
            )
        };
        if rc != 0 {
            return Err(ConsensusBftError::InvalidSignature(format!(
                "aoem ffi mldsa verify failed: rc={}",
                rc
            )));
        }
        if out_valid != 1 {
            return Err(ConsensusBftError::InvalidSignature(
                "aoem ffi mldsa verify returned invalid".to_string(),
            ));
        }
        Ok(())
    }
}

fn build_aoem_ffi_mldsa87_vote_verifier() -> Result<Arc<dyn GovernanceVoteVerifier>> {
    let lib_path = resolve_aoem_ffi_library_path();
    let lib = unsafe { libloading::Library::new(&lib_path) }.with_context(|| {
        format!(
            "load AOEM FFI library failed: {} (set NOVOVM_AOEM_FFI_LIB_PATH or ensure library name on PATH/LD_LIBRARY_PATH/DYLD_LIBRARY_PATH)",
            lib_path.display()
        )
    })?;
    let lib = Box::leak(Box::new(lib));

    let (abi_version_fn, supported_fn, pubkey_size_fn, signature_size_fn, verify_fn) = unsafe {
        let abi_version_fn: libloading::Symbol<AoemAbiVersionFn> =
            lib.get(b"aoem_abi_version\0")
                .context("resolve aoem_abi_version failed")?;
        let supported_fn: libloading::Symbol<AoemMldsaSupportedFn> = lib
            .get(b"aoem_mldsa_supported\0")
            .context("resolve aoem_mldsa_supported failed")?;
        let pubkey_size_fn: libloading::Symbol<AoemMldsaPubkeySizeFn> = lib
            .get(b"aoem_mldsa_pubkey_size\0")
            .context("resolve aoem_mldsa_pubkey_size failed")?;
        let signature_size_fn: libloading::Symbol<AoemMldsaSignatureSizeFn> = lib
            .get(b"aoem_mldsa_signature_size\0")
            .context("resolve aoem_mldsa_signature_size failed")?;
        let verify_fn: libloading::Symbol<AoemMldsaVerifyFn> = lib
            .get(b"aoem_mldsa_verify\0")
            .context("resolve aoem_mldsa_verify failed")?;
        (
            *abi_version_fn,
            *supported_fn,
            *pubkey_size_fn,
            *signature_size_fn,
            *verify_fn,
        )
    };

    let abi_version = unsafe { abi_version_fn() };
    if abi_version != GOVERNANCE_AOEM_FFI_ABI_VERSION {
        bail!(
            "unsupported AOEM FFI ABI version: expected {} got {}",
            GOVERNANCE_AOEM_FFI_ABI_VERSION,
            abi_version
        );
    }
    let mldsa_supported = unsafe { supported_fn() };
    if mldsa_supported != 1 {
        bail!("AOEM FFI library reports mldsa capability disabled");
    }

    let expected_pubkey_size = unsafe { pubkey_size_fn(GOVERNANCE_MLDSA87_LEVEL) } as usize;
    let expected_signature_size = unsafe { signature_size_fn(GOVERNANCE_MLDSA87_LEVEL) } as usize;
    if expected_pubkey_size == 0 || expected_signature_size == 0 {
        bail!(
            "AOEM FFI returned invalid mldsa87 sizes: pubkey_size={} signature_size={}",
            expected_pubkey_size,
            expected_signature_size
        );
    }

    let mut voter_pubkeys = parse_governance_mldsa87_pubkeys_from_env()?;
    for (voter, pubkey) in voter_pubkeys.iter() {
        if pubkey.len() != expected_pubkey_size {
            bail!(
                "registered mldsa87 pubkey size mismatch for voter {}: expected {} got {}",
                voter,
                expected_pubkey_size,
                pubkey.len()
            );
        }
    }
    // Keep deterministic behavior for logs/debug.
    voter_pubkeys.shrink_to_fit();

    Ok(Arc::new(AoemFfiMldsa87GovernanceVoteVerifier {
        verify_fn,
        expected_pubkey_size,
        expected_signature_size,
        voter_pubkeys,
    }))
}

fn apply_governance_vote_verifier(
    engine: &BFTEngine,
    verifier: GovernanceVoteVerifierScheme,
) -> Result<()> {
    match verifier {
        GovernanceVoteVerifierScheme::Ed25519 => engine
            .set_governance_vote_verifier_by_scheme(verifier)
            .map_err(|e| anyhow::anyhow!("{}", e)),
        GovernanceVoteVerifierScheme::MlDsa87 => {
            let mode = string_env_nonempty("NOVOVM_GOVERNANCE_MLDSA_MODE")
                .unwrap_or_else(|| "staged".to_string());
            if mode.eq_ignore_ascii_case("aoem_ffi") {
                let custom = build_aoem_ffi_mldsa87_vote_verifier()?;
                engine.set_governance_vote_verifier(custom);
                Ok(())
            } else {
                bail!(
                    "unsupported governance vote verifier: mldsa87 (staged-only, set NOVOVM_GOVERNANCE_MLDSA_MODE=aoem_ffi + AOEM FFI library to enable)"
                );
            }
        }
    }
}

fn configure_governance_vote_verifier(engine: &BFTEngine) -> Result<()> {
    let verifier = load_governance_vote_verifier_config_from_env()?;
    apply_governance_vote_verifier(engine, verifier)?;
    let mode =
        string_env_nonempty("NOVOVM_GOVERNANCE_MLDSA_MODE").unwrap_or_else(|| "staged".to_string());
    println!(
        "governance_vote_verifier_in: source=env key=NOVOVM_GOVERNANCE_VOTE_VERIFIER configured={} active={} active_scheme={} mldsa_mode={}",
        verifier.as_str(),
        engine.governance_vote_verifier_name(),
        engine.governance_vote_verifier_scheme().as_str(),
        mode
    );
    Ok(())
}

fn ensure_governance_signature_scheme_supported(
    runtime: &mut GovernanceRpcRuntime,
    action: &str,
    proposal_id: u64,
    actor: ConsensusNodeId,
    requested_scheme: GovernanceVoteVerifierScheme,
) -> Result<()> {
    if runtime
        .engine
        .governance_signature_scheme_supported(requested_scheme)
    {
        return Ok(());
    }
    let active_scheme = runtime.engine.governance_vote_verifier_scheme();
    push_governance_audit_event(
        runtime,
        action,
        proposal_id,
        Some(actor),
        "reject",
        format!(
            "unsupported signature scheme {} (staged-only, current enabled: {})",
            requested_scheme.as_str(),
            active_scheme.as_str()
        ),
    )?;
    bail!(
        "unsupported governance signature scheme: {} (current enabled: {})",
        requested_scheme.as_str(),
        active_scheme.as_str()
    );
}

fn parse_node_id_allowlist_env(
    name: &str,
    default_ids: &[ConsensusNodeId],
) -> Result<HashSet<ConsensusNodeId>> {
    match std::env::var(name) {
        Ok(raw) => {
            let trimmed = raw.trim();
            if trimmed.is_empty() {
                return Ok(default_ids.iter().copied().collect());
            }
            let mut out = HashSet::new();
            for part in trimmed.split(',') {
                let token = part.trim();
                if token.is_empty() {
                    continue;
                }
                let parsed = token
                    .parse::<u32>()
                    .with_context(|| format!("invalid {} node id: {}", name, token))?;
                out.insert(parsed);
            }
            if out.is_empty() {
                bail!("{} resolved to empty allowlist", name);
            }
            Ok(out)
        }
        Err(_) => Ok(default_ids.iter().copied().collect()),
    }
}

fn parse_ip_allowlist_env(name: &str) -> Result<HashSet<IpAddr>> {
    match std::env::var(name) {
        Ok(raw) => {
            let trimmed = raw.trim();
            if trimmed.is_empty() {
                return Ok(HashSet::new());
            }
            let mut out = HashSet::new();
            for part in trimmed.split(',') {
                let token = part.trim();
                if token.is_empty() {
                    continue;
                }
                let ip = token
                    .parse::<IpAddr>()
                    .with_context(|| format!("invalid {} ip: {}", name, token))?;
                out.insert(ip);
            }
            Ok(out)
        }
        Err(_) => Ok(HashSet::new()),
    }
}

fn bind_is_loopback(bind: &str) -> bool {
    if let Ok(addr) = bind.parse::<SocketAddr>() {
        return addr.ip().is_loopback();
    }
    if let Some((host_raw, _)) = bind.rsplit_once(':') {
        let host = host_raw
            .trim()
            .trim_start_matches('[')
            .trim_end_matches(']');
        return host.eq_ignore_ascii_case("localhost")
            || host.eq_ignore_ascii_case("::1")
            || host.starts_with("127.");
    }
    false
}

fn binds_conflict(lhs: &str, rhs: &str) -> bool {
    let a = lhs.trim();
    let b = rhs.trim();
    if a.eq_ignore_ascii_case(b) {
        return true;
    }
    match (a.parse::<SocketAddr>(), b.parse::<SocketAddr>()) {
        (Ok(x), Ok(y)) => x == y,
        _ => false,
    }
}

fn decode_hex_bytes(raw: &str, field: &str) -> Result<Vec<u8>> {
    let normalized = raw.trim().strip_prefix("0x").unwrap_or(raw.trim());
    if normalized.is_empty() {
        bail!("{} is empty", field);
    }
    if normalized.len() % 2 != 0 {
        bail!("{} must have even hex length", field);
    }
    if !normalized.bytes().all(|b| b.is_ascii_hexdigit()) {
        bail!("{} must be hex", field);
    }
    let mut out = Vec::with_capacity(normalized.len() / 2);
    let bytes = normalized.as_bytes();
    let mut idx = 0usize;
    while idx < bytes.len() {
        let pair = std::str::from_utf8(&bytes[idx..idx + 2])
            .with_context(|| format!("{} contains invalid utf8", field))?;
        let v = u8::from_str_radix(pair, 16)
            .with_context(|| format!("{} contains invalid hex byte {}", field, pair))?;
        out.push(v);
        idx += 2;
    }
    Ok(out)
}

fn push_governance_audit_event(
    runtime: &mut GovernanceRpcRuntime,
    action: &str,
    proposal_id: u64,
    actor: Option<ConsensusNodeId>,
    outcome: &str,
    detail: impl Into<String>,
) -> Result<()> {
    runtime.next_audit_seq = runtime.next_audit_seq.saturating_add(1);
    runtime.audit_events.push(GovernanceRpcAuditEvent {
        seq: runtime.next_audit_seq,
        ts_sec: now_unix_sec(),
        action: action.to_string(),
        proposal_id,
        actor,
        outcome: outcome.to_string(),
        detail: detail.into(),
    });
    save_governance_audit_store(
        &runtime.audit_store_path,
        runtime.next_audit_seq,
        &runtime.audit_events,
    )?;
    Ok(())
}

fn governance_council_policy_to_json(policy: &GovernanceCouncilPolicy) -> serde_json::Value {
    let members: Vec<_> = policy
        .members
        .iter()
        .map(|m| {
            let seat = match m.seat {
                GovernanceCouncilSeat::Founder => "founder".to_string(),
                GovernanceCouncilSeat::TopHolder(idx) => format!("top_holder_{}", idx),
                GovernanceCouncilSeat::Team(idx) => format!("team_{}", idx),
                GovernanceCouncilSeat::Independent => "independent".to_string(),
            };
            serde_json::json!({
                "seat": seat,
                "node_id": m.node_id,
            })
        })
        .collect();
    serde_json::json!({
        "enabled": policy.enabled,
        "members": members,
        "parameter_change_threshold_bp": policy.parameter_change_threshold_bp,
        "treasury_spend_threshold_bp": policy.treasury_spend_threshold_bp,
        "protocol_upgrade_threshold_bp": policy.protocol_upgrade_threshold_bp,
        "emergency_freeze_threshold_bp": policy.emergency_freeze_threshold_bp,
        "emergency_min_categories": policy.emergency_min_categories,
    })
}

fn market_governance_policy_to_json(policy: &MarketGovernancePolicy) -> serde_json::Value {
    serde_json::json!({
        "amm": {
            "swap_fee_bp": policy.amm.swap_fee_bp,
            "lp_fee_share_bp": policy.amm.lp_fee_share_bp,
        },
        "cdp": {
            "min_collateral_ratio_bp": policy.cdp.min_collateral_ratio_bp,
            "liquidation_threshold_bp": policy.cdp.liquidation_threshold_bp,
            "liquidation_penalty_bp": policy.cdp.liquidation_penalty_bp,
            "stability_fee_bp": policy.cdp.stability_fee_bp,
            "max_leverage_x100": policy.cdp.max_leverage_x100,
        },
        "bond": {
            "coupon_rate_bp": policy.bond.coupon_rate_bp,
            "max_maturity_days": policy.bond.max_maturity_days,
            "min_issue_price_bp": policy.bond.min_issue_price_bp,
        },
        "reserve": {
            "min_reserve_ratio_bp": policy.reserve.min_reserve_ratio_bp,
            "redemption_fee_bp": policy.reserve.redemption_fee_bp,
        },
        "nav": {
            "settlement_delay_epochs": policy.nav.settlement_delay_epochs,
            "max_daily_redemption_bp": policy.nav.max_daily_redemption_bp,
        },
        "buyback": {
            "trigger_discount_bp": policy.buyback.trigger_discount_bp,
            "max_treasury_budget_per_epoch": policy.buyback.max_treasury_budget_per_epoch,
            "burn_share_bp": policy.buyback.burn_share_bp,
        },
    })
}

fn market_engine_snapshot_to_json(snapshot: &Web30MarketEngineSnapshot) -> serde_json::Value {
    serde_json::json!({
        "amm_swap_fee_bp": snapshot.amm_swap_fee_bp,
        "amm_lp_fee_share_bp": snapshot.amm_lp_fee_share_bp,
        "cdp_min_collateral_ratio_bp": snapshot.cdp_min_collateral_ratio_bp,
        "cdp_liquidation_threshold_bp": snapshot.cdp_liquidation_threshold_bp,
        "cdp_liquidation_penalty_bp": snapshot.cdp_liquidation_penalty_bp,
        "cdp_stability_fee_bp": snapshot.cdp_stability_fee_bp,
        "cdp_max_leverage_x100": snapshot.cdp_max_leverage_x100,
        "bond_one_year_coupon_bp": snapshot.bond_one_year_coupon_bp,
        "bond_three_year_coupon_bp": snapshot.bond_three_year_coupon_bp,
        "bond_five_year_coupon_bp": snapshot.bond_five_year_coupon_bp,
        "bond_max_maturity_days_policy": snapshot.bond_max_maturity_days_policy,
        "bond_min_issue_price_bp": snapshot.bond_min_issue_price_bp,
        "reserve_min_reserve_ratio_bp": snapshot.reserve_min_reserve_ratio_bp,
        "reserve_redemption_fee_bp": snapshot.reserve_redemption_fee_bp,
        "nav_settlement_delay_epochs": snapshot.nav_settlement_delay_epochs,
        "nav_max_daily_redemption_bp": snapshot.nav_max_daily_redemption_bp,
        "buyback_trigger_discount_bp": snapshot.buyback_trigger_discount_bp,
        "buyback_max_treasury_budget_per_epoch": snapshot.buyback_max_treasury_budget_per_epoch,
        "buyback_burn_share_bp": snapshot.buyback_burn_share_bp,
        "treasury_main_balance": snapshot.treasury_main_balance,
        "treasury_ecosystem_balance": snapshot.treasury_ecosystem_balance,
        "treasury_risk_reserve_balance": snapshot.treasury_risk_reserve_balance,
        "reserve_foreign_usdt_balance": snapshot.reserve_foreign_usdt_balance,
        "nav_soft_floor_value": snapshot.nav_soft_floor_value,
        "buyback_last_spent_stable": snapshot.buyback_last_spent_stable,
        "buyback_last_burned_token": snapshot.buyback_last_burned_token,
        "oracle_price_before": snapshot.oracle_price_before,
        "oracle_price_after": snapshot.oracle_price_after,
        "cdp_liquidation_candidates": snapshot.cdp_liquidation_candidates,
        "cdp_liquidations_executed": snapshot.cdp_liquidations_executed,
        "cdp_liquidation_penalty_routed": snapshot.cdp_liquidation_penalty_routed,
        "nav_snapshot_day": snapshot.nav_snapshot_day,
        "nav_latest_value": snapshot.nav_latest_value,
        "nav_redemptions_submitted": snapshot.nav_redemptions_submitted,
        "nav_redemptions_executed": snapshot.nav_redemptions_executed,
        "nav_executed_stable_total": snapshot.nav_executed_stable_total,
    })
}

fn governance_op_to_view(op: &GovernanceOp) -> (String, serde_json::Value) {
    match op {
        GovernanceOp::UpdateSlashPolicy { policy } => (
            "update_slash_policy".to_string(),
            serde_json::json!({
                "mode": policy.mode.as_str(),
                "equivocation_threshold": policy.equivocation_threshold,
                "min_active_validators": policy.min_active_validators,
                "cooldown_epochs": policy.cooldown_epochs,
            }),
        ),
        GovernanceOp::UpdateMempoolFeeFloor { fee_floor } => (
            "update_mempool_fee_floor".to_string(),
            serde_json::json!({
                "fee_floor": fee_floor,
            }),
        ),
        GovernanceOp::UpdateNetworkDosPolicy { policy } => (
            "update_network_dos_policy".to_string(),
            serde_json::json!({
                "rpc_rate_limit_per_ip": policy.rpc_rate_limit_per_ip,
                "peer_ban_threshold": policy.peer_ban_threshold,
            }),
        ),
        GovernanceOp::UpdateTokenEconomicsPolicy { policy } => (
            "update_token_economics_policy".to_string(),
            serde_json::json!({
                "max_supply": policy.max_supply,
                "locked_supply": policy.locked_supply,
                "fee_split": {
                    "gas_base_burn_bp": policy.fee_split.gas_base_burn_bp,
                    "gas_to_node_bp": policy.fee_split.gas_to_node_bp,
                    "service_burn_bp": policy.fee_split.service_burn_bp,
                    "service_to_provider_bp": policy.fee_split.service_to_provider_bp,
                },
            }),
        ),
        GovernanceOp::UpdateMarketGovernancePolicy { policy } => (
            "update_market_governance_policy".to_string(),
            market_governance_policy_to_json(policy),
        ),
        GovernanceOp::UpdateGovernanceAccessPolicy { policy } => (
            "update_governance_access_policy".to_string(),
            serde_json::json!({
                "proposer_committee": policy.proposer_committee,
                "proposer_threshold": policy.proposer_threshold,
                "executor_committee": policy.executor_committee,
                "executor_threshold": policy.executor_threshold,
                "timelock_epochs": policy.timelock_epochs,
            }),
        ),
        GovernanceOp::UpdateGovernanceCouncilPolicy { policy } => (
            "update_governance_council_policy".to_string(),
            governance_council_policy_to_json(policy),
        ),
        GovernanceOp::TreasurySpend { to, amount, reason } => (
            "treasury_spend".to_string(),
            serde_json::json!({
                "to": to,
                "amount": amount,
                "reason": reason,
            }),
        ),
    }
}

fn proposal_to_view(
    proposal: &GovernanceProposal,
    votes_collected: usize,
) -> GovernanceRpcProposalView {
    let (op, payload) = governance_op_to_view(&proposal.op);
    GovernanceRpcProposalView {
        proposal_id: proposal.proposal_id,
        proposer: proposal.proposer,
        created_height: proposal.created_height,
        proposal_digest: to_hex(&proposal.digest()),
        op,
        payload,
        votes_collected,
    }
}

fn parse_council_seat(raw: &str) -> Result<GovernanceCouncilSeat> {
    let seat = raw.trim().to_ascii_lowercase();
    if seat == "founder" {
        return Ok(GovernanceCouncilSeat::Founder);
    }
    if seat == "independent" {
        return Ok(GovernanceCouncilSeat::Independent);
    }
    if let Some(rest) = seat.strip_prefix("top_holder_") {
        let idx = rest
            .parse::<u8>()
            .with_context(|| format!("invalid top_holder index: {}", rest))?;
        return Ok(GovernanceCouncilSeat::TopHolder(idx));
    }
    if let Some(rest) = seat.strip_prefix("team_") {
        let idx = rest
            .parse::<u8>()
            .with_context(|| format!("invalid team index: {}", rest))?;
        return Ok(GovernanceCouncilSeat::Team(idx));
    }
    bail!(
        "unsupported council seat: {} (expected founder|top_holder_0..4|team_0..1|independent)",
        raw
    )
}

fn parse_governance_council_members(
    params: &serde_json::Value,
    field: &str,
) -> Result<Vec<GovernanceCouncilMember>> {
    let arr = params
        .get(field)
        .and_then(|v| v.as_array())
        .ok_or_else(|| anyhow::anyhow!("{} must be an array", field))?;
    let mut out = Vec::with_capacity(arr.len());
    for (idx, item) in arr.iter().enumerate() {
        let seat_raw = item
            .get("seat")
            .and_then(|v| v.as_str())
            .ok_or_else(|| anyhow::anyhow!("{}.{}: seat is required", field, idx))?;
        let node_id_raw = item
            .get("node_id")
            .and_then(|v| v.as_u64())
            .ok_or_else(|| anyhow::anyhow!("{}.{}: node_id is required", field, idx))?;
        let node_id = u32::try_from(node_id_raw).map_err(|_| {
            anyhow::anyhow!("{}.{}: node_id out of range: {}", field, idx, node_id_raw)
        })?;
        out.push(GovernanceCouncilMember {
            seat: parse_council_seat(seat_raw)?,
            node_id,
        });
    }
    Ok(out)
}

fn parse_governance_op(params: &serde_json::Value) -> Result<GovernanceOp> {
    let op = param_as_string(params, "op")
        .unwrap_or_default()
        .to_ascii_lowercase();
    if op.is_empty() {
        bail!("op is required for governance_submitProposal");
    }
    match op.as_str() {
        "update_slash_policy" => {
            let mode_raw =
                param_as_string(params, "mode").unwrap_or_else(|| "observe_only".to_string());
            let mode = parse_slash_mode(&mode_raw)?;
            let equivocation_threshold =
                param_as_u64(params, "equivocation_threshold").ok_or_else(|| {
                    anyhow::anyhow!("equivocation_threshold is required for update_slash_policy")
                })? as u32;
            let min_active_validators =
                param_as_u64(params, "min_active_validators").ok_or_else(|| {
                    anyhow::anyhow!("min_active_validators is required for update_slash_policy")
                })? as u32;
            let cooldown_epochs = param_as_u64(params, "cooldown_epochs").unwrap_or(0);
            let policy = SlashPolicy {
                mode,
                equivocation_threshold,
                min_active_validators,
                cooldown_epochs,
            };
            policy
                .validate()
                .map_err(|e| anyhow::anyhow!("governance_policy_invalid: {}", e))?;
            Ok(GovernanceOp::UpdateSlashPolicy { policy })
        }
        "update_mempool_fee_floor" => {
            let fee_floor = param_as_u64(params, "fee_floor").ok_or_else(|| {
                anyhow::anyhow!("fee_floor is required for update_mempool_fee_floor")
            })?;
            if fee_floor == 0 {
                bail!("fee_floor must be > 0");
            }
            Ok(GovernanceOp::UpdateMempoolFeeFloor { fee_floor })
        }
        "update_network_dos_policy" => {
            let rpc_rate_limit_per_ip =
                param_as_u64(params, "rpc_rate_limit_per_ip").ok_or_else(|| {
                    anyhow::anyhow!(
                        "rpc_rate_limit_per_ip is required for update_network_dos_policy"
                    )
                })? as u32;
            let peer_ban_threshold_raw =
                param_as_i64(params, "peer_ban_threshold").ok_or_else(|| {
                    anyhow::anyhow!("peer_ban_threshold is required for update_network_dos_policy")
                })?;
            let peer_ban_threshold = i32::try_from(peer_ban_threshold_raw)
                .map_err(|_| anyhow::anyhow!("peer_ban_threshold is out of i32 range"))?;
            let policy = NetworkDosPolicy {
                rpc_rate_limit_per_ip,
                peer_ban_threshold,
            };
            policy
                .validate()
                .map_err(|e| anyhow::anyhow!("governance_policy_invalid: {}", e))?;
            Ok(GovernanceOp::UpdateNetworkDosPolicy { policy })
        }
        "update_token_economics_policy" => {
            let max_supply = param_as_u64(params, "max_supply").ok_or_else(|| {
                anyhow::anyhow!("max_supply is required for update_token_economics_policy")
            })?;
            let locked_supply = param_as_u64(params, "locked_supply").ok_or_else(|| {
                anyhow::anyhow!("locked_supply is required for update_token_economics_policy")
            })?;
            let gas_base_burn_bp_raw =
                param_as_u64(params, "gas_base_burn_bp").ok_or_else(|| {
                    anyhow::anyhow!(
                        "gas_base_burn_bp is required for update_token_economics_policy"
                    )
                })?;
            let gas_base_burn_bp = u16::try_from(gas_base_burn_bp_raw)
                .map_err(|_| anyhow::anyhow!("gas_base_burn_bp is out of u16 range"))?;
            let gas_to_node_bp_raw = param_as_u64(params, "gas_to_node_bp").ok_or_else(|| {
                anyhow::anyhow!("gas_to_node_bp is required for update_token_economics_policy")
            })?;
            let gas_to_node_bp = u16::try_from(gas_to_node_bp_raw)
                .map_err(|_| anyhow::anyhow!("gas_to_node_bp is out of u16 range"))?;
            let service_burn_bp_raw = param_as_u64(params, "service_burn_bp").ok_or_else(|| {
                anyhow::anyhow!("service_burn_bp is required for update_token_economics_policy")
            })?;
            let service_burn_bp = u16::try_from(service_burn_bp_raw)
                .map_err(|_| anyhow::anyhow!("service_burn_bp is out of u16 range"))?;
            let service_to_provider_bp_raw = param_as_u64(params, "service_to_provider_bp")
                .ok_or_else(|| {
                    anyhow::anyhow!(
                        "service_to_provider_bp is required for update_token_economics_policy"
                    )
                })?;
            let service_to_provider_bp = u16::try_from(service_to_provider_bp_raw)
                .map_err(|_| anyhow::anyhow!("service_to_provider_bp is out of u16 range"))?;
            let policy = TokenEconomicsPolicy {
                max_supply,
                locked_supply,
                fee_split: novovm_consensus::FeeSplit {
                    gas_base_burn_bp,
                    gas_to_node_bp,
                    service_burn_bp,
                    service_to_provider_bp,
                },
            };
            policy
                .validate()
                .map_err(|e| anyhow::anyhow!("governance_policy_invalid: {}", e))?;
            Ok(GovernanceOp::UpdateTokenEconomicsPolicy { policy })
        }
        "update_market_governance_policy" => {
            let amm_swap_fee_bp_raw = param_as_u64(params, "amm_swap_fee_bp").ok_or_else(|| {
                anyhow::anyhow!("amm_swap_fee_bp is required for update_market_governance_policy")
            })?;
            let amm_swap_fee_bp = u16::try_from(amm_swap_fee_bp_raw)
                .map_err(|_| anyhow::anyhow!("amm_swap_fee_bp is out of u16 range"))?;
            let amm_lp_fee_share_bp_raw =
                param_as_u64(params, "amm_lp_fee_share_bp").ok_or_else(|| {
                    anyhow::anyhow!(
                        "amm_lp_fee_share_bp is required for update_market_governance_policy"
                    )
                })?;
            let amm_lp_fee_share_bp = u16::try_from(amm_lp_fee_share_bp_raw)
                .map_err(|_| anyhow::anyhow!("amm_lp_fee_share_bp is out of u16 range"))?;
            let cdp_min_collateral_ratio_bp_raw =
                param_as_u64(params, "cdp_min_collateral_ratio_bp").ok_or_else(|| {
                    anyhow::anyhow!(
                    "cdp_min_collateral_ratio_bp is required for update_market_governance_policy"
                )
                })?;
            let cdp_min_collateral_ratio_bp = u16::try_from(cdp_min_collateral_ratio_bp_raw)
                .map_err(|_| anyhow::anyhow!("cdp_min_collateral_ratio_bp is out of u16 range"))?;
            let cdp_liquidation_threshold_bp_raw =
                param_as_u64(params, "cdp_liquidation_threshold_bp").ok_or_else(|| {
                    anyhow::anyhow!(
                    "cdp_liquidation_threshold_bp is required for update_market_governance_policy"
                )
                })?;
            let cdp_liquidation_threshold_bp = u16::try_from(cdp_liquidation_threshold_bp_raw)
                .map_err(|_| anyhow::anyhow!("cdp_liquidation_threshold_bp is out of u16 range"))?;
            let cdp_liquidation_penalty_bp_raw = param_as_u64(params, "cdp_liquidation_penalty_bp")
                .ok_or_else(|| {
                    anyhow::anyhow!(
                        "cdp_liquidation_penalty_bp is required for update_market_governance_policy"
                    )
                })?;
            let cdp_liquidation_penalty_bp = u16::try_from(cdp_liquidation_penalty_bp_raw)
                .map_err(|_| anyhow::anyhow!("cdp_liquidation_penalty_bp is out of u16 range"))?;
            let cdp_stability_fee_bp_raw = param_as_u64(params, "cdp_stability_fee_bp")
                .ok_or_else(|| {
                    anyhow::anyhow!(
                        "cdp_stability_fee_bp is required for update_market_governance_policy"
                    )
                })?;
            let cdp_stability_fee_bp = u16::try_from(cdp_stability_fee_bp_raw)
                .map_err(|_| anyhow::anyhow!("cdp_stability_fee_bp is out of u16 range"))?;
            let cdp_max_leverage_x100_raw = param_as_u64(params, "cdp_max_leverage_x100")
                .ok_or_else(|| {
                    anyhow::anyhow!(
                        "cdp_max_leverage_x100 is required for update_market_governance_policy"
                    )
                })?;
            let cdp_max_leverage_x100 = u16::try_from(cdp_max_leverage_x100_raw)
                .map_err(|_| anyhow::anyhow!("cdp_max_leverage_x100 is out of u16 range"))?;
            let bond_coupon_rate_bp_raw =
                param_as_u64(params, "bond_coupon_rate_bp").ok_or_else(|| {
                    anyhow::anyhow!(
                        "bond_coupon_rate_bp is required for update_market_governance_policy"
                    )
                })?;
            let bond_coupon_rate_bp = u16::try_from(bond_coupon_rate_bp_raw)
                .map_err(|_| anyhow::anyhow!("bond_coupon_rate_bp is out of u16 range"))?;
            let bond_max_maturity_days_raw = param_as_u64(params, "bond_max_maturity_days")
                .ok_or_else(|| {
                    anyhow::anyhow!(
                        "bond_max_maturity_days is required for update_market_governance_policy"
                    )
                })?;
            let bond_max_maturity_days = u16::try_from(bond_max_maturity_days_raw)
                .map_err(|_| anyhow::anyhow!("bond_max_maturity_days is out of u16 range"))?;
            let bond_min_issue_price_bp_raw = param_as_u64(params, "bond_min_issue_price_bp")
                .ok_or_else(|| {
                    anyhow::anyhow!(
                        "bond_min_issue_price_bp is required for update_market_governance_policy"
                    )
                })?;
            let bond_min_issue_price_bp = u16::try_from(bond_min_issue_price_bp_raw)
                .map_err(|_| anyhow::anyhow!("bond_min_issue_price_bp is out of u16 range"))?;
            let reserve_min_reserve_ratio_bp_raw =
                param_as_u64(params, "reserve_min_reserve_ratio_bp").ok_or_else(|| {
                    anyhow::anyhow!(
                    "reserve_min_reserve_ratio_bp is required for update_market_governance_policy"
                )
                })?;
            let reserve_min_reserve_ratio_bp = u16::try_from(reserve_min_reserve_ratio_bp_raw)
                .map_err(|_| anyhow::anyhow!("reserve_min_reserve_ratio_bp is out of u16 range"))?;
            let reserve_redemption_fee_bp_raw = param_as_u64(params, "reserve_redemption_fee_bp")
                .ok_or_else(|| {
                anyhow::anyhow!(
                    "reserve_redemption_fee_bp is required for update_market_governance_policy"
                )
            })?;
            let reserve_redemption_fee_bp = u16::try_from(reserve_redemption_fee_bp_raw)
                .map_err(|_| anyhow::anyhow!("reserve_redemption_fee_bp is out of u16 range"))?;
            let nav_settlement_delay_epochs = param_as_u64(params, "nav_settlement_delay_epochs")
                .ok_or_else(|| {
                anyhow::anyhow!(
                    "nav_settlement_delay_epochs is required for update_market_governance_policy"
                )
            })?;
            let nav_settlement_delay_epochs = u16::try_from(nav_settlement_delay_epochs)
                .map_err(|_| anyhow::anyhow!("nav_settlement_delay_epochs is out of u16 range"))?;
            let nav_max_daily_redemption_bp_raw =
                param_as_u64(params, "nav_max_daily_redemption_bp").ok_or_else(|| {
                    anyhow::anyhow!(
                    "nav_max_daily_redemption_bp is required for update_market_governance_policy"
                )
                })?;
            let nav_max_daily_redemption_bp = u16::try_from(nav_max_daily_redemption_bp_raw)
                .map_err(|_| anyhow::anyhow!("nav_max_daily_redemption_bp is out of u16 range"))?;
            let buyback_trigger_discount_bp_raw =
                param_as_u64(params, "buyback_trigger_discount_bp").ok_or_else(|| {
                    anyhow::anyhow!(
                    "buyback_trigger_discount_bp is required for update_market_governance_policy"
                )
                })?;
            let buyback_trigger_discount_bp = u16::try_from(buyback_trigger_discount_bp_raw)
                .map_err(|_| anyhow::anyhow!("buyback_trigger_discount_bp is out of u16 range"))?;
            let buyback_max_treasury_budget_per_epoch =
                param_as_u64(params, "buyback_max_treasury_budget_per_epoch").ok_or_else(
                    || {
                        anyhow::anyhow!(
                            "buyback_max_treasury_budget_per_epoch is required for update_market_governance_policy"
                        )
                    },
                )?;
            let buyback_burn_share_bp_raw = param_as_u64(params, "buyback_burn_share_bp")
                .ok_or_else(|| {
                    anyhow::anyhow!(
                        "buyback_burn_share_bp is required for update_market_governance_policy"
                    )
                })?;
            let buyback_burn_share_bp = u16::try_from(buyback_burn_share_bp_raw)
                .map_err(|_| anyhow::anyhow!("buyback_burn_share_bp is out of u16 range"))?;

            let policy = MarketGovernancePolicy {
                amm: AmmGovernanceParams {
                    swap_fee_bp: amm_swap_fee_bp,
                    lp_fee_share_bp: amm_lp_fee_share_bp,
                },
                cdp: CdpGovernanceParams {
                    min_collateral_ratio_bp: cdp_min_collateral_ratio_bp,
                    liquidation_threshold_bp: cdp_liquidation_threshold_bp,
                    liquidation_penalty_bp: cdp_liquidation_penalty_bp,
                    stability_fee_bp: cdp_stability_fee_bp,
                    max_leverage_x100: cdp_max_leverage_x100,
                },
                bond: BondGovernanceParams {
                    coupon_rate_bp: bond_coupon_rate_bp,
                    max_maturity_days: bond_max_maturity_days,
                    min_issue_price_bp: bond_min_issue_price_bp,
                },
                reserve: ReserveGovernanceParams {
                    min_reserve_ratio_bp: reserve_min_reserve_ratio_bp,
                    redemption_fee_bp: reserve_redemption_fee_bp,
                },
                nav: NavGovernanceParams {
                    settlement_delay_epochs: nav_settlement_delay_epochs,
                    max_daily_redemption_bp: nav_max_daily_redemption_bp,
                },
                buyback: BuybackGovernanceParams {
                    trigger_discount_bp: buyback_trigger_discount_bp,
                    max_treasury_budget_per_epoch: buyback_max_treasury_budget_per_epoch,
                    burn_share_bp: buyback_burn_share_bp,
                },
            };
            policy
                .validate()
                .map_err(|e| anyhow::anyhow!("governance_policy_invalid: {}", e))?;
            Ok(GovernanceOp::UpdateMarketGovernancePolicy { policy })
        }
        "update_governance_access_policy" => {
            let proposer_committee_raw = param_as_u64_list(params, "proposer_committee")
                .ok_or_else(|| {
                    anyhow::anyhow!(
                        "proposer_committee is required for update_governance_access_policy"
                    )
                })?;
            let executor_committee_raw = param_as_u64_list(params, "executor_committee")
                .ok_or_else(|| {
                    anyhow::anyhow!(
                        "executor_committee is required for update_governance_access_policy"
                    )
                })?;
            let proposer_threshold =
                param_as_u64(params, "proposer_threshold").ok_or_else(|| {
                    anyhow::anyhow!(
                        "proposer_threshold is required for update_governance_access_policy"
                    )
                })? as u32;
            let executor_threshold =
                param_as_u64(params, "executor_threshold").ok_or_else(|| {
                    anyhow::anyhow!(
                        "executor_threshold is required for update_governance_access_policy"
                    )
                })? as u32;
            let timelock_epochs = param_as_u64(params, "timelock_epochs").unwrap_or(0);
            let proposer_committee: Vec<ConsensusNodeId> = proposer_committee_raw
                .into_iter()
                .map(|id| {
                    u32::try_from(id)
                        .map_err(|_| anyhow::anyhow!("proposer committee id out of range: {}", id))
                })
                .collect::<Result<Vec<_>>>()?;
            let executor_committee: Vec<ConsensusNodeId> = executor_committee_raw
                .into_iter()
                .map(|id| {
                    u32::try_from(id)
                        .map_err(|_| anyhow::anyhow!("executor committee id out of range: {}", id))
                })
                .collect::<Result<Vec<_>>>()?;
            let policy = GovernanceAccessPolicy {
                proposer_committee,
                proposer_threshold,
                executor_committee,
                executor_threshold,
                timelock_epochs,
            };
            policy
                .validate()
                .map_err(|e| anyhow::anyhow!("governance_policy_invalid: {}", e))?;
            Ok(GovernanceOp::UpdateGovernanceAccessPolicy { policy })
        }
        "update_governance_council_policy" => {
            let enabled = params
                .get("enabled")
                .and_then(|v| v.as_bool())
                .unwrap_or(true);
            let defaults = GovernanceCouncilPolicy::disabled();
            let members = if enabled {
                parse_governance_council_members(params, "members")?
            } else {
                params
                    .get("members")
                    .and_then(|v| v.as_array())
                    .map(|_| parse_governance_council_members(params, "members"))
                    .transpose()?
                    .unwrap_or_default()
            };
            let parameter_change_threshold_bp =
                match param_as_u64(params, "parameter_change_threshold_bp") {
                    Some(v) => u16::try_from(v).map_err(|_| {
                        anyhow::anyhow!("parameter_change_threshold_bp out of u16 range")
                    })?,
                    None => defaults.parameter_change_threshold_bp,
                };
            let treasury_spend_threshold_bp =
                match param_as_u64(params, "treasury_spend_threshold_bp") {
                    Some(v) => u16::try_from(v).map_err(|_| {
                        anyhow::anyhow!("treasury_spend_threshold_bp out of u16 range")
                    })?,
                    None => defaults.treasury_spend_threshold_bp,
                };
            let protocol_upgrade_threshold_bp =
                match param_as_u64(params, "protocol_upgrade_threshold_bp") {
                    Some(v) => u16::try_from(v).map_err(|_| {
                        anyhow::anyhow!("protocol_upgrade_threshold_bp out of u16 range")
                    })?,
                    None => defaults.protocol_upgrade_threshold_bp,
                };
            let emergency_freeze_threshold_bp =
                match param_as_u64(params, "emergency_freeze_threshold_bp") {
                    Some(v) => u16::try_from(v).map_err(|_| {
                        anyhow::anyhow!("emergency_freeze_threshold_bp out of u16 range")
                    })?,
                    None => defaults.emergency_freeze_threshold_bp,
                };
            let emergency_min_categories = match param_as_u64(params, "emergency_min_categories") {
                Some(v) => u8::try_from(v)
                    .map_err(|_| anyhow::anyhow!("emergency_min_categories out of u8 range"))?,
                None => defaults.emergency_min_categories,
            };

            let policy = GovernanceCouncilPolicy {
                enabled,
                members,
                parameter_change_threshold_bp,
                treasury_spend_threshold_bp,
                protocol_upgrade_threshold_bp,
                emergency_freeze_threshold_bp,
                emergency_min_categories,
            };
            policy
                .validate()
                .map_err(|e| anyhow::anyhow!("governance_policy_invalid: {}", e))?;
            Ok(GovernanceOp::UpdateGovernanceCouncilPolicy { policy })
        }
        "treasury_spend" => {
            let to = param_as_u64(params, "to")
                .ok_or_else(|| anyhow::anyhow!("to is required for treasury_spend"))?
                as ConsensusNodeId;
            let amount = param_as_u64(params, "amount")
                .ok_or_else(|| anyhow::anyhow!("amount is required for treasury_spend"))?;
            let reason = param_as_string(params, "reason").unwrap_or_default();
            let reason = reason.trim().to_string();
            if amount == 0 {
                bail!("governance_policy_invalid: treasury spend amount must be > 0");
            }
            if reason.is_empty() {
                bail!("governance_policy_invalid: treasury spend reason cannot be empty");
            }
            if reason.len() > 128 {
                bail!("governance_policy_invalid: treasury spend reason too long (max 128)");
            }
            Ok(GovernanceOp::TreasurySpend { to, amount, reason })
        }
        _ => bail!("unsupported governance op: {}", op),
    }
}

fn init_governance_rpc_runtime(
    slash_policy: &SlashPolicy,
    audit_store_path: PathBuf,
    chain_audit_store_path: PathBuf,
) -> Result<GovernanceRpcRuntime> {
    let validator_ids: Vec<ConsensusNodeId> = vec![0, 1, 2];
    let proposer_allowlist =
        parse_node_id_allowlist_env("NOVOVM_GOVERNANCE_PROPOSER_ALLOWLIST", &validator_ids)?;
    let executor_allowlist =
        parse_node_id_allowlist_env("NOVOVM_GOVERNANCE_EXECUTOR_ALLOWLIST", &validator_ids)?;
    let validator_set = ValidatorSet::new_equal_weight(validator_ids.clone());
    let signing_keys: Vec<_> = (0..validator_ids.len())
        .map(|_| SigningKey::generate(&mut OsRng))
        .collect();
    let mut public_keys = HashMap::new();
    let mut signers = HashMap::new();
    for (idx, node_id) in validator_ids.iter().enumerate() {
        public_keys.insert(*node_id, signing_keys[idx].verifying_key());
        signers.insert(*node_id, signing_keys[idx].clone());
    }

    let engine = BFTEngine::new(
        BFTConfig::default(),
        0,
        signing_keys[0].clone(),
        validator_set,
        public_keys,
    )
    .context("init governance rpc consensus engine failed")?;
    configure_governance_vote_verifier(&engine)
        .context("configure governance vote verifier failed")?;
    engine
        .set_slash_policy(slash_policy.clone())
        .context("set governance rpc slash policy failed")?;
    engine.set_governance_execution_enabled(true);

    let audit_store = load_governance_audit_store(&audit_store_path)?;
    println!(
        "governance_audit_store_in: path={} events={} next_seq={}",
        audit_store_path.display(),
        audit_store.events.len(),
        audit_store.next_seq
    );
    let chain_audit_store = load_governance_chain_audit_store(&chain_audit_store_path)?;
    let chain_head_seq = chain_audit_store
        .events
        .last()
        .map(|event| event.seq)
        .unwrap_or(0);
    let persisted_head_seq = chain_audit_store.head_seq;
    let persisted_root_hex = chain_audit_store.root_hex.trim().to_ascii_lowercase();
    println!(
        "governance_chain_audit_store_in: path={} events={} head_seq={} persisted_head_seq={} persisted_root={}",
        chain_audit_store_path.display(),
        chain_audit_store.events.len(),
        chain_head_seq,
        persisted_head_seq,
        if persisted_root_hex.is_empty() {
            "-"
        } else {
            persisted_root_hex.as_str()
        }
    );
    engine.restore_governance_chain_audit_events(chain_audit_store.events.clone());
    let restored_root_hex = to_hex(&engine.governance_chain_audit_root());
    let restored_head_seq = engine
        .governance_chain_audit_events()
        .last()
        .map(|event| event.seq)
        .unwrap_or(0);
    if persisted_head_seq > 0 && persisted_head_seq != restored_head_seq {
        bail!(
            "governance chain audit store head_seq mismatch: persisted={} restored={}",
            persisted_head_seq,
            restored_head_seq
        );
    }
    if !persisted_root_hex.is_empty() && persisted_root_hex != restored_root_hex {
        bail!(
            "governance chain audit store root mismatch: persisted={} restored={}",
            persisted_root_hex,
            restored_root_hex
        );
    }
    println!(
        "governance_chain_audit_store_restore_out: head_seq={} root={}",
        restored_head_seq,
        restored_root_hex
    );

    Ok(GovernanceRpcRuntime {
        engine,
        signers,
        votes: HashMap::new(),
        signed_votes: HashMap::new(),
        proposer_allowlist,
        executor_allowlist,
        audit_events: audit_store.events,
        next_audit_seq: audit_store.next_seq,
        audit_store_path,
        chain_audit_store_path,
    })
}

fn run_governance_rpc(
    runtime: &mut GovernanceRpcRuntime,
    method: &str,
    params: &serde_json::Value,
) -> Result<serde_json::Value> {
    match method {
        "governance_submitProposal" => {
            let proposer = param_as_u64(params, "proposer")
                .ok_or_else(|| anyhow::anyhow!("proposer is required for governance_submitProposal"))?
                as ConsensusNodeId;
            if !runtime.proposer_allowlist.contains(&proposer) {
                push_governance_audit_event(
                    runtime,
                    "submit",
                    0,
                    Some(proposer),
                    "reject",
                    format!("unauthorized proposer {}", proposer),
                )?;
                bail!("unauthorized proposer: {}", proposer);
            }
            let op = parse_governance_op(params)?;
            let proposer_approvals_raw =
                param_as_u64_list(params, "proposer_approvals").unwrap_or_else(|| vec![proposer as u64]);
            let proposer_approvals: Vec<ConsensusNodeId> = proposer_approvals_raw
                .into_iter()
                .map(|id| {
                    u32::try_from(id)
                        .map_err(|_| anyhow::anyhow!("proposer_approvals id out of range: {}", id))
                })
                .collect::<Result<Vec<_>>>()?;
            let proposal = runtime
                .engine
                .submit_governance_proposal_with_approvals(proposer, &proposer_approvals, op)?;
            let view = proposal_to_view(&proposal, 0);
            push_governance_audit_event(
                runtime,
                "submit",
                proposal.proposal_id,
                Some(proposer),
                "ok",
                format!("{} proposer_approvals={}", view.op, proposer_approvals.len()),
            )?;
            Ok(serde_json::json!({
                "method": method,
                "submitted": true,
                "proposer_approvals": proposer_approvals,
                "proposal": view,
            }))
        }
        "governance_sign" => {
            let proposal_id = param_as_u64(params, "proposal_id")
                .ok_or_else(|| anyhow::anyhow!("proposal_id is required for governance_sign"))?;
            let signer_id = param_as_u64(params, "signer_id")
                .ok_or_else(|| anyhow::anyhow!("signer_id is required for governance_sign"))?
                as ConsensusNodeId;
            let support = param_as_bool(params, "support").unwrap_or(true);
            let signature_scheme = parse_governance_signature_scheme(params)?;
            if signature_scheme == GovernanceVoteVerifierScheme::MlDsa87 {
                ensure_governance_signature_scheme_supported(
                    runtime,
                    "sign",
                    proposal_id,
                    signer_id,
                    signature_scheme,
                )?;
                push_governance_audit_event(
                    runtime,
                    "sign",
                    proposal_id,
                    Some(signer_id),
                    "reject",
                    "mldsa87 local signing is not supported; provide external mldsa signature via governance_vote(signature,mldsa_pubkey)",
                )?;
                bail!(
                    "governance_sign does not support local mldsa87 signing; use governance_vote with external signature and mldsa_pubkey"
                );
            }
            ensure_governance_signature_scheme_supported(
                runtime,
                "sign",
                proposal_id,
                signer_id,
                signature_scheme,
            )?;
            let signer = runtime
                .signers
                .get(&signer_id)
                .ok_or_else(|| anyhow::anyhow!("unknown signer_id: {}", signer_id))?;
            let proposal = runtime
                .engine
                .governance_pending_proposal(proposal_id)
                .ok_or_else(|| anyhow::anyhow!("proposal not found: {}", proposal_id))?;
            let vote = GovernanceVote::new(&proposal, signer_id, support, signer);
            let signature_hex = to_hex(&vote.signature);
            runtime
                .signed_votes
                .insert((proposal_id, signer_id, support), vote.signature);
            push_governance_audit_event(
                runtime,
                "sign",
                proposal_id,
                Some(signer_id),
                "ok",
                format!(
                    "support={} signature_scheme={}",
                    support,
                    signature_scheme.as_str()
                ),
            )?;
            Ok(serde_json::json!({
                "method": method,
                "proposal_id": proposal_id,
                "signer_id": signer_id,
                "support": support,
                "signature_scheme": signature_scheme.as_str(),
                "signature": signature_hex,
            }))
        }
        "governance_vote" => {
            let proposal_id = param_as_u64(params, "proposal_id")
                .ok_or_else(|| anyhow::anyhow!("proposal_id is required for governance_vote"))?;
            let voter_id = param_as_u64(params, "voter_id")
                .ok_or_else(|| anyhow::anyhow!("voter_id is required for governance_vote"))?
                as ConsensusNodeId;
            let support = param_as_bool(params, "support").unwrap_or(true);
            let signature_scheme = parse_governance_signature_scheme(params)?;
            ensure_governance_signature_scheme_supported(
                runtime,
                "vote",
                proposal_id,
                voter_id,
                signature_scheme,
            )?;
            let signer = runtime
                .signers
                .get(&voter_id)
                .ok_or_else(|| anyhow::anyhow!("unknown voter_id: {}", voter_id))?;
            let proposal = runtime
                .engine
                .governance_pending_proposal(proposal_id)
                .ok_or_else(|| anyhow::anyhow!("proposal not found: {}", proposal_id))?;

            let duplicate_vote = runtime
                .votes
                .get(&proposal_id)
                .map(|entry| entry.iter().any(|v| v.voter_id == voter_id))
                .unwrap_or(false);
            if duplicate_vote {
                push_governance_audit_event(
                    runtime,
                    "vote",
                    proposal_id,
                    Some(voter_id),
                    "reject",
                    "duplicate governance vote",
                )?;
                bail!("duplicate governance vote from voter {}", voter_id);
            }
            let signature = if signature_scheme == GovernanceVoteVerifierScheme::MlDsa87 {
                let signature_hex = param_as_string(params, "signature").ok_or_else(|| {
                    anyhow::anyhow!(
                        "signature is required for governance_vote when signature_scheme=mldsa87"
                    )
                })?;
                let mldsa_signature = decode_hex_bytes(&signature_hex, "signature")?;
                let pubkey_hex = param_as_string(params, "mldsa_pubkey")
                    .or_else(|| param_as_string(params, "pubkey"))
                    .ok_or_else(|| {
                        anyhow::anyhow!(
                            "mldsa_pubkey is required for governance_vote when signature_scheme=mldsa87"
                        )
                    })?;
                let mldsa_pubkey = decode_hex_bytes(&pubkey_hex, "mldsa_pubkey")?;
                encode_mldsa87_vote_signature_envelope(&mldsa_pubkey, &mldsa_signature)?
            } else if let Some(signature_hex) = param_as_string(params, "signature") {
                decode_hex_bytes(&signature_hex, "signature")?
            } else if let Some(sig) = runtime
                .signed_votes
                .remove(&(proposal_id, voter_id, support))
            {
                sig
            } else {
                GovernanceVote::new(&proposal, voter_id, support, signer).signature
            };
            let vote = GovernanceVote {
                proposal_id,
                proposal_height: proposal.created_height,
                proposal_digest: proposal.digest(),
                voter_id,
                support,
                signature,
            };
            let votes_collected = {
                let entry = runtime.votes.entry(proposal_id).or_default();
                entry.push(vote);
                entry.len()
            };
            push_governance_audit_event(
                runtime,
                "vote",
                proposal_id,
                Some(voter_id),
                "ok",
                format!(
                    "support={} votes_collected={} signature_scheme={}",
                    support,
                    votes_collected,
                    signature_scheme.as_str()
                ),
            )?;
            Ok(serde_json::json!({
                "method": method,
                "proposal_id": proposal_id,
                "voter_id": voter_id,
                "support": support,
                "signature_scheme": signature_scheme.as_str(),
                "votes_collected": votes_collected,
            }))
        }
        "governance_execute" => {
            let proposal_id = param_as_u64(params, "proposal_id")
                .ok_or_else(|| anyhow::anyhow!("proposal_id is required for governance_execute"))?;
            let executor = param_as_u64(params, "executor")
                .ok_or_else(|| anyhow::anyhow!("executor is required for governance_execute"))?
                as ConsensusNodeId;
            if !runtime.executor_allowlist.contains(&executor) {
                push_governance_audit_event(
                    runtime,
                    "execute",
                    proposal_id,
                    Some(executor),
                    "reject",
                    format!("unauthorized executor {}", executor),
                )?;
                bail!("unauthorized executor: {}", executor);
            }
            let votes = runtime
                .votes
                .get(&proposal_id)
                .cloned()
                .unwrap_or_default();
            let executors_raw =
                param_as_u64_list(params, "executor_approvals").unwrap_or_else(|| vec![executor as u64]);
            let executor_approvals: Vec<ConsensusNodeId> = executors_raw
                .into_iter()
                .map(|id| {
                    u32::try_from(id)
                        .map_err(|_| anyhow::anyhow!("executor_approvals id out of range: {}", id))
                })
                .collect::<Result<Vec<_>>>()?;
            let executed = runtime
                .engine
                .execute_governance_proposal_with_executor_approvals(
                    proposal_id,
                    &votes,
                    &executor_approvals,
                )?;
            if executed {
                runtime.votes.remove(&proposal_id);
            }
            let slash = runtime.engine.slash_policy();
            let dos = runtime.engine.governance_network_dos_policy();
            let token = runtime.engine.governance_token_economics_policy();
            let market = runtime.engine.governance_market_policy();
            let market_engine = runtime.engine.governance_market_engine_snapshot();
            let access = runtime.engine.governance_access_policy();
            let council = runtime.engine.governance_council_policy();
            let vote_verifier_name = runtime.engine.governance_vote_verifier_name();
            let vote_verifier_scheme = runtime.engine.governance_vote_verifier_scheme();
            let treasury_balance = runtime.engine.token_treasury_balance();
            let treasury_spent_total = runtime.engine.token_treasury_spent_total();
            push_governance_audit_event(
                runtime,
                "execute",
                proposal_id,
                Some(executor),
                "ok",
                format!(
                    "executed={} executor_approvals={} verifier={} signature_scheme={}",
                    executed,
                    executor_approvals.len(),
                    vote_verifier_name,
                    vote_verifier_scheme.as_str()
                ),
            )?;
            Ok(serde_json::json!({
                "method": method,
                "proposal_id": proposal_id,
                "executor": executor,
                "executor_approvals": executor_approvals,
                "executed": executed,
                "vote_verifier": {
                    "name": vote_verifier_name,
                    "signature_scheme": vote_verifier_scheme.as_str(),
                },
                "slash_policy": {
                    "mode": slash.mode.as_str(),
                    "equivocation_threshold": slash.equivocation_threshold,
                    "min_active_validators": slash.min_active_validators,
                    "cooldown_epochs": slash.cooldown_epochs,
                },
                "mempool_fee_floor": runtime.engine.governance_mempool_fee_floor(),
                "network_dos_policy": {
                    "rpc_rate_limit_per_ip": dos.rpc_rate_limit_per_ip,
                    "peer_ban_threshold": dos.peer_ban_threshold,
                },
                "governance_access_policy": {
                    "proposer_committee": access.proposer_committee,
                    "proposer_threshold": access.proposer_threshold,
                    "executor_committee": access.executor_committee,
                    "executor_threshold": access.executor_threshold,
                    "timelock_epochs": access.timelock_epochs,
                },
                "governance_council_policy": governance_council_policy_to_json(&council),
                "token_economics_policy": {
                    "max_supply": token.max_supply,
                    "locked_supply": token.locked_supply,
                    "fee_split": {
                        "gas_base_burn_bp": token.fee_split.gas_base_burn_bp,
                        "gas_to_node_bp": token.fee_split.gas_to_node_bp,
                        "service_burn_bp": token.fee_split.service_burn_bp,
                        "service_to_provider_bp": token.fee_split.service_to_provider_bp,
                    },
                },
                "market_governance_policy": market_governance_policy_to_json(&market),
                "market_engine_snapshot": market_engine_snapshot_to_json(&market_engine),
                "market_runtime_snapshot": market_engine_snapshot_to_json(&market_engine),
                "treasury": {
                    "balance": treasury_balance,
                    "spent_total": treasury_spent_total,
                },
            }))
        }
        "governance_getProposal" => {
            let proposal_id = param_as_u64(params, "proposal_id")
                .ok_or_else(|| anyhow::anyhow!("proposal_id is required for governance_getProposal"))?;
            let proposal = runtime.engine.governance_pending_proposal(proposal_id);
            let votes_collected = runtime
                .votes
                .get(&proposal_id)
                .map(|v| v.len())
                .unwrap_or(0);
            Ok(serde_json::json!({
                "method": method,
                "proposal_id": proposal_id,
                "found": proposal.is_some(),
                "proposal": proposal.map(|p| proposal_to_view(&p, votes_collected)),
            }))
        }
        "governance_listProposals" => {
            let proposals: Vec<_> = runtime
                .engine
                .governance_pending_proposals()
                .into_iter()
                .map(|p| {
                    let votes_collected = runtime.votes.get(&p.proposal_id).map(|v| v.len()).unwrap_or(0);
                    proposal_to_view(&p, votes_collected)
                })
                .collect();
            Ok(serde_json::json!({
                "method": method,
                "count": proposals.len(),
                "proposals": proposals,
            }))
        }
        "governance_listAuditEvents" => {
            let proposal_id_filter = param_as_u64(params, "proposal_id");
            let limit = param_as_u64(params, "limit").unwrap_or(50).clamp(1, 200) as usize;
            let mut events: Vec<GovernanceRpcAuditEvent> = runtime
                .audit_events
                .iter()
                .filter(|event| proposal_id_filter.map(|id| event.proposal_id == id).unwrap_or(true))
                .cloned()
                .collect();
            if events.len() > limit {
                let start = events.len().saturating_sub(limit);
                events = events[start..].to_vec();
            }
            Ok(serde_json::json!({
                "method": method,
                "count": events.len(),
                "proposal_id_filter": proposal_id_filter,
                "events": events,
            }))
        }
        "governance_listChainAuditEvents" => {
            let proposal_id_filter = param_as_u64(params, "proposal_id");
            let since_seq = param_as_u64(params, "since_seq").unwrap_or(0);
            let limit = param_as_u64(params, "limit").unwrap_or(50).clamp(1, 200) as usize;
            let chain_audit_root = to_hex(&runtime.engine.governance_chain_audit_root());
            let mut all_events = runtime.engine.governance_chain_audit_events();
            let head_seq = all_events.last().map(|event| event.seq).unwrap_or(0);
            all_events.retain(|event| {
                event.seq > since_seq
                    && proposal_id_filter
                        .map(|proposal_id| event.proposal_id == proposal_id)
                        .unwrap_or(true)
            });
            if all_events.len() > limit {
                let start = all_events.len().saturating_sub(limit);
                all_events = all_events[start..].to_vec();
            }
            Ok(serde_json::json!({
                "method": method,
                "count": all_events.len(),
                "proposal_id_filter": proposal_id_filter,
                "since_seq": since_seq,
                "head_seq": head_seq,
                "root": chain_audit_root,
                "events": all_events,
            }))
        }
        "governance_getPolicy" => {
            let slash = runtime.engine.slash_policy();
            let dos = runtime.engine.governance_network_dos_policy();
            let token = runtime.engine.governance_token_economics_policy();
            let market = runtime.engine.governance_market_policy();
            let market_engine = runtime.engine.governance_market_engine_snapshot();
            let access = runtime.engine.governance_access_policy();
            let council = runtime.engine.governance_council_policy();
            let treasury_balance = runtime.engine.token_treasury_balance();
            let treasury_spent_total = runtime.engine.token_treasury_spent_total();
            let chain_audit_events = runtime.engine.governance_chain_audit_events();
            let chain_audit_head_seq = chain_audit_events.last().map(|event| event.seq).unwrap_or(0);
            let chain_audit_root = to_hex(&runtime.engine.governance_chain_audit_root());
            Ok(serde_json::json!({
                "method": method,
                "slash_policy": {
                    "mode": slash.mode.as_str(),
                    "equivocation_threshold": slash.equivocation_threshold,
                    "min_active_validators": slash.min_active_validators,
                    "cooldown_epochs": slash.cooldown_epochs,
                },
                "mempool_fee_floor": runtime.engine.governance_mempool_fee_floor(),
                "network_dos_policy": {
                    "rpc_rate_limit_per_ip": dos.rpc_rate_limit_per_ip,
                    "peer_ban_threshold": dos.peer_ban_threshold,
                },
                "governance_access_policy": {
                    "proposer_committee": access.proposer_committee,
                    "proposer_threshold": access.proposer_threshold,
                    "executor_committee": access.executor_committee,
                    "executor_threshold": access.executor_threshold,
                    "timelock_epochs": access.timelock_epochs,
                },
                "governance_council_policy": governance_council_policy_to_json(&council),
                "token_economics_policy": {
                    "max_supply": token.max_supply,
                    "locked_supply": token.locked_supply,
                    "fee_split": {
                        "gas_base_burn_bp": token.fee_split.gas_base_burn_bp,
                        "gas_to_node_bp": token.fee_split.gas_to_node_bp,
                        "service_burn_bp": token.fee_split.service_burn_bp,
                        "service_to_provider_bp": token.fee_split.service_to_provider_bp,
                    },
                },
                "market_governance_policy": market_governance_policy_to_json(&market),
                "market_engine_snapshot": market_engine_snapshot_to_json(&market_engine),
                "market_runtime_snapshot": market_engine_snapshot_to_json(&market_engine),
                "treasury": {
                    "balance": treasury_balance,
                    "spent_total": treasury_spent_total,
                },
                "governance_chain_audit": {
                    "count": chain_audit_events.len(),
                    "head_seq": chain_audit_head_seq,
                    "root": chain_audit_root,
                },
                "governance_execution_enabled": runtime.engine.governance_execution_enabled(),
            }))
        }
        _ => bail!(
            "unknown governance method: {}; valid: governance_submitProposal|governance_sign|governance_vote|governance_execute|governance_getProposal|governance_listProposals|governance_listAuditEvents|governance_listChainAuditEvents|governance_getPolicy",
            method
        ),
    }
}

fn run_chain_query(
    db: &QueryStateDb,
    method: &str,
    params: &serde_json::Value,
) -> Result<serde_json::Value> {
    let response = match method {
        "getBlock" => {
            let height = param_as_u64(params, "height");
            let block = match height {
                Some(h) => db.blocks.iter().find(|b| b.height == h).cloned(),
                None => db.blocks.last().cloned(),
            };
            serde_json::json!({
                "method": "getBlock",
                "requested_height": height,
                "found": block.is_some(),
                "block": block,
            })
        }
        "getTransaction" => {
            let tx_hash = param_as_string(params, "tx_hash")
                .or_else(|| param_as_string(params, "hash"))
                .unwrap_or_default()
                .to_ascii_lowercase();
            if tx_hash.is_empty() {
                bail!("tx_hash is required for getTransaction");
            }
            let tx = db.txs.get(&tx_hash).cloned();
            serde_json::json!({
                "method": "getTransaction",
                "tx_hash": tx_hash,
                "found": tx.is_some(),
                "transaction": tx,
            })
        }
        "getReceipt" => {
            let tx_hash = param_as_string(params, "tx_hash")
                .or_else(|| param_as_string(params, "hash"))
                .unwrap_or_default()
                .to_ascii_lowercase();
            if tx_hash.is_empty() {
                bail!("tx_hash is required for getReceipt");
            }
            let receipt = db.receipts.get(&tx_hash).cloned();
            serde_json::json!({
                "method": "getReceipt",
                "tx_hash": tx_hash,
                "found": receipt.is_some(),
                "receipt": receipt,
            })
        }
        "getBalance" => {
            let account = param_as_string(params, "account").unwrap_or_default();
            let trimmed = account.trim();
            if trimmed.is_empty() {
                bail!("account is required for getBalance");
            }
            let balance = db.balances.get(trimmed).copied().unwrap_or(0);
            serde_json::json!({
                "method": "getBalance",
                "account": trimmed,
                "found": db.balances.contains_key(trimmed),
                "balance": balance,
            })
        }
        _ => bail!(
            "unknown method: {}; valid: getBlock|getTransaction|getReceipt|getBalance",
            method
        ),
    };
    Ok(response)
}

fn now_unix_sec() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_else(|_| Duration::from_secs(0))
        .as_secs()
}

fn is_rate_limited(
    counters: &mut HashMap<String, RpcRateCounter>,
    remote: &str,
    max_per_sec: u32,
) -> bool {
    if max_per_sec == 0 {
        return false;
    }
    let now_sec = now_unix_sec();
    let entry = counters
        .entry(remote.to_string())
        .or_insert(RpcRateCounter {
            window_sec: now_sec,
            count: 0,
        });
    if entry.window_sec != now_sec {
        entry.window_sec = now_sec;
        entry.count = 0;
    }
    if entry.count >= max_per_sec {
        return true;
    }
    entry.count = entry.count.saturating_add(1);
    false
}

fn respond_json_http(
    request: tiny_http::Request,
    status: u16,
    body: &serde_json::Value,
) -> Result<()> {
    let payload = serde_json::to_string(body).context("serialize rpc response json failed")?;
    let mut response =
        tiny_http::Response::from_string(payload).with_status_code(tiny_http::StatusCode(status));
    if let Ok(header) =
        tiny_http::Header::from_bytes(b"Content-Type".to_vec(), b"application/json".to_vec())
    {
        response = response.with_header(header);
    }
    request
        .respond(response)
        .map_err(|e| anyhow::anyhow!("rpc response send failed: {e}"))?;
    Ok(())
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum RpcServerRole {
    Public,
    Governance,
}

impl RpcServerRole {
    fn as_str(self) -> &'static str {
        match self {
            RpcServerRole::Public => "public",
            RpcServerRole::Governance => "governance",
        }
    }
}

#[derive(Clone, Debug)]
struct RpcServerExit {
    role: RpcServerRole,
    bind: String,
    processed: u32,
    max_requests: u32,
}

fn run_rpc_server_instance(
    role: RpcServerRole,
    bind: String,
    db_path: PathBuf,
    max_body_bytes: usize,
    rate_limit_per_ip: u32,
    max_requests: u32,
    gov_allowlist: HashSet<IpAddr>,
    governance_slash_policy: Option<SlashPolicy>,
) -> Result<RpcServerExit> {
    let mut governance_runtime = match role {
        RpcServerRole::Governance => {
            let slash_policy = governance_slash_policy
                .ok_or_else(|| anyhow::anyhow!("missing governance slash policy"))?;
            let audit_store_path = governance_audit_db_path(&db_path);
            let chain_audit_store_path = governance_chain_audit_db_path(&db_path);
            Some(init_governance_rpc_runtime(
                &slash_policy,
                audit_store_path,
                chain_audit_store_path,
            )?)
        }
        RpcServerRole::Public => None,
    };

    let governance_audit_db = governance_runtime
        .as_ref()
        .map(|runtime| runtime.audit_store_path.display().to_string())
        .unwrap_or_else(|| "-".to_string());
    let governance_chain_audit_db = governance_runtime
        .as_ref()
        .map(|runtime| runtime.chain_audit_store_path.display().to_string())
        .unwrap_or_else(|| "-".to_string());

    let server = tiny_http::Server::http(&bind).map_err(|e| {
        anyhow::anyhow!(
            "start {} rpc server failed on {}: {}",
            role.as_str(),
            bind,
            e
        )
    })?;
    println!(
        "rpc_server_in: role={} bind={} db={} max_body={} rate_limit_per_ip={} max_requests={} gov_allowlist_count={} governance_audit_db={} governance_chain_audit_db={} governance_execution_enabled={}",
        role.as_str(),
        bind,
        db_path.display(),
        max_body_bytes,
        rate_limit_per_ip,
        max_requests,
        gov_allowlist.len(),
        governance_audit_db,
        governance_chain_audit_db,
        governance_runtime
            .as_ref()
            .map(|runtime| runtime.engine.governance_execution_enabled())
            .unwrap_or(false)
    );

    let mut processed = 0u32;
    let mut rate_counters: HashMap<String, RpcRateCounter> = HashMap::new();
    loop {
        let mut request = server
            .recv()
            .map_err(|e| anyhow::anyhow!("{} rpc recv failed: {e}", role.as_str()))?;
        let remote_addr = request.remote_addr();
        let remote = remote_addr
            .map(|addr| addr.ip().to_string())
            .unwrap_or_else(|| "unknown".to_string());

        if role == RpcServerRole::Governance && !gov_allowlist.is_empty() {
            let allowed = remote_addr
                .map(|addr| gov_allowlist.contains(&addr.ip()))
                .unwrap_or(false);
            if !allowed {
                let resp = serde_json::json!({
                    "jsonrpc": "2.0",
                    "id": serde_json::Value::Null,
                    "error": {
                        "code": -32023,
                        "message": format!("governance rpc caller ip not in allowlist: {}", remote),
                    }
                });
                respond_json_http(request, 403, &resp)?;
                processed = processed.saturating_add(1);
                if max_requests > 0 && processed >= max_requests {
                    break;
                }
                continue;
            }
        }

        if is_rate_limited(&mut rate_counters, &remote, rate_limit_per_ip) {
            let resp = serde_json::json!({
                "jsonrpc": "2.0",
                "id": serde_json::Value::Null,
                "error": {
                    "code": -32029,
                    "message": format!("rate limit exceeded for {}", remote),
                }
            });
            respond_json_http(request, 429, &resp)?;
            processed = processed.saturating_add(1);
            if max_requests > 0 && processed >= max_requests {
                break;
            }
            continue;
        }

        if request.method() != &tiny_http::Method::Post {
            let resp = serde_json::json!({
                "jsonrpc": "2.0",
                "id": serde_json::Value::Null,
                "error": {
                    "code": -32600,
                    "message": "only POST is supported",
                }
            });
            respond_json_http(request, 405, &resp)?;
            processed = processed.saturating_add(1);
            if max_requests > 0 && processed >= max_requests {
                break;
            }
            continue;
        }

        let path = request.url().to_string();
        if path != "/" && path != "/rpc" {
            let resp = serde_json::json!({
                "jsonrpc": "2.0",
                "id": serde_json::Value::Null,
                "error": {
                    "code": -32601,
                    "message": format!("unsupported path: {}", path),
                }
            });
            respond_json_http(request, 404, &resp)?;
            processed = processed.saturating_add(1);
            if max_requests > 0 && processed >= max_requests {
                break;
            }
            continue;
        }

        let mut body = String::new();
        request
            .as_reader()
            .take(max_body_bytes as u64 + 1)
            .read_to_string(&mut body)
            .context("read rpc request body failed")?;

        if body.len() > max_body_bytes {
            let resp = serde_json::json!({
                "jsonrpc": "2.0",
                "id": serde_json::Value::Null,
                "error": {
                    "code": -32600,
                    "message": format!("request body exceeds {} bytes", max_body_bytes),
                }
            });
            respond_json_http(request, 413, &resp)?;
            processed = processed.saturating_add(1);
            if max_requests > 0 && processed >= max_requests {
                break;
            }
            continue;
        }

        let payload: serde_json::Value = match serde_json::from_str(&body) {
            Ok(v) => v,
            Err(e) => {
                let resp = serde_json::json!({
                    "jsonrpc": "2.0",
                    "id": serde_json::Value::Null,
                    "error": {
                        "code": -32700,
                        "message": format!("parse error: {}", e),
                    }
                });
                respond_json_http(request, 400, &resp)?;
                processed = processed.saturating_add(1);
                if max_requests > 0 && processed >= max_requests {
                    break;
                }
                continue;
            }
        };

        let request_id = payload
            .get("id")
            .cloned()
            .unwrap_or(serde_json::Value::Null);
        let method = payload
            .get("method")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .trim()
            .to_string();
        let params = payload
            .get("params")
            .cloned()
            .unwrap_or_else(|| serde_json::json!({}));

        if method.is_empty() {
            let resp = serde_json::json!({
                "jsonrpc": "2.0",
                "id": request_id,
                "error": {
                    "code": -32600,
                    "message": "missing method",
                }
            });
            respond_json_http(request, 400, &resp)?;
            processed = processed.saturating_add(1);
            if max_requests > 0 && processed >= max_requests {
                break;
            }
            continue;
        }

        if role == RpcServerRole::Public && method.starts_with("governance_") {
            let resp = serde_json::json!({
                "jsonrpc": "2.0",
                "id": request_id,
                "error": {
                    "code": -32601,
                    "message": format!("method not found on public rpc: {}", method),
                }
            });
            respond_json_http(request, 200, &resp)?;
            processed = processed.saturating_add(1);
            if max_requests > 0 && processed >= max_requests {
                break;
            }
            continue;
        }

        if role == RpcServerRole::Governance && !method.starts_with("governance_") {
            let resp = serde_json::json!({
                "jsonrpc": "2.0",
                "id": request_id,
                "error": {
                    "code": -32601,
                    "message": format!("method not found on governance rpc: {}", method),
                }
            });
            respond_json_http(request, 200, &resp)?;
            processed = processed.saturating_add(1);
            if max_requests > 0 && processed >= max_requests {
                break;
            }
            continue;
        }

        let result = match role {
            RpcServerRole::Public => {
                let db = load_query_state_db(&db_path)?;
                run_chain_query(&db, &method, &params)
            }
            RpcServerRole::Governance => {
                let runtime = governance_runtime
                    .as_mut()
                    .ok_or_else(|| anyhow::anyhow!("missing governance runtime"))?;
                let before_head_seq = runtime
                    .engine
                    .governance_chain_audit_events()
                    .last()
                    .map(|event| event.seq)
                    .unwrap_or(0);
                let mut governance_result = run_governance_rpc(runtime, &method, &params);
                let chain_events = runtime.engine.governance_chain_audit_events();
                let chain_root = runtime.engine.governance_chain_audit_root();
                let after_head_seq = chain_events.last().map(|event| event.seq).unwrap_or(0);
                if after_head_seq > before_head_seq {
                    if let Err(persist_err) =
                        save_governance_chain_audit_store(
                            &runtime.chain_audit_store_path,
                            &chain_events,
                            chain_root,
                        )
                    {
                        if governance_result.is_ok() {
                            governance_result =
                                Err(persist_err.context("persist governance chain audit store failed"));
                        }
                    }
                }
                governance_result
            }
        };
        let response = match result {
            Ok(out) => serde_json::json!({
                "jsonrpc": "2.0",
                "id": request_id,
                "result": out,
            }),
            Err(e) => serde_json::json!({
                "jsonrpc": "2.0",
                "id": request_id,
                "error": {
                    "code": -32602,
                    "message": e.to_string(),
                }
            }),
        };
        respond_json_http(request, 200, &response)?;

        processed = processed.saturating_add(1);
        if max_requests > 0 && processed >= max_requests {
            break;
        }
    }

    println!(
        "rpc_server_out: role={} bind={} processed={} max_requests={}",
        role.as_str(),
        bind,
        processed,
        max_requests
    );
    if role == RpcServerRole::Public {
        println!(
            "chain_query_rpc_server_out: bind={} processed={} max_requests={}",
            bind, processed, max_requests
        );
    } else {
        println!(
            "governance_rpc_server_out: bind={} processed={} max_requests={}",
            bind, processed, max_requests
        );
    }

    Ok(RpcServerExit {
        role,
        bind,
        processed,
        max_requests,
    })
}

fn run_chain_query_mode() -> Result<()> {
    let db_path = chain_query_db_path();
    let db = load_query_state_db(&db_path)?;
    let method = string_env("NOVOVM_CHAIN_QUERY_METHOD", "")
        .trim()
        .to_string();
    if method.is_empty() {
        bail!(
            "missing NOVOVM_CHAIN_QUERY_METHOD; valid: getBlock|getTransaction|getReceipt|getBalance"
        );
    }

    let mut params = serde_json::Map::new();
    if let Ok(v) = std::env::var("NOVOVM_CHAIN_QUERY_HEIGHT") {
        if !v.trim().is_empty() {
            params.insert("height".to_string(), serde_json::Value::String(v));
        }
    }
    if let Ok(v) = std::env::var("NOVOVM_CHAIN_QUERY_TX_HASH") {
        if !v.trim().is_empty() {
            params.insert("tx_hash".to_string(), serde_json::Value::String(v));
        }
    }
    if let Ok(v) = std::env::var("NOVOVM_CHAIN_QUERY_ACCOUNT") {
        if !v.trim().is_empty() {
            params.insert("account".to_string(), serde_json::Value::String(v));
        }
    }

    let response = run_chain_query(&db, &method, &serde_json::Value::Object(params))?;
    println!(
        "chain_query_out: db={} method={} blocks={} txs={} receipts={} balances={}",
        db_path.display(),
        method,
        db.blocks.len(),
        db.txs.len(),
        db.receipts.len(),
        db.balances.len()
    );
    println!(
        "{}",
        serde_json::to_string_pretty(&response).context("serialize chain query response failed")?
    );
    Ok(())
}

fn run_chain_query_rpc_server_mode() -> Result<()> {
    let legacy_bind = string_env("NOVOVM_RPC_BIND", "127.0.0.1:8899");
    let public_bind =
        string_env_nonempty("NOVOVM_PUBLIC_RPC_BIND").unwrap_or_else(|| legacy_bind.clone());
    let gov_bind =
        string_env_nonempty("NOVOVM_GOV_RPC_BIND").unwrap_or_else(|| "127.0.0.1:8901".to_string());

    let enable_public = bool_env_default("NOVOVM_ENABLE_PUBLIC_RPC", true);
    let enable_gov = bool_env_default("NOVOVM_ENABLE_GOV_RPC", false);
    if !enable_public && !enable_gov {
        bail!("rpc server invalid config: both public and governance rpc are disabled");
    }
    if enable_public && enable_gov && binds_conflict(&public_bind, &gov_bind) {
        bail!(
            "rpc server invalid config: public and governance binds conflict ({})",
            public_bind
        );
    }

    let gov_allowlist = parse_ip_allowlist_env("NOVOVM_GOV_RPC_ALLOWLIST")?;
    if enable_gov && !bind_is_loopback(&gov_bind) && gov_allowlist.is_empty() {
        bail!(
            "rpc server invalid config: governance rpc bind {} is non-loopback and NOVOVM_GOV_RPC_ALLOWLIST is empty",
            gov_bind
        );
    }

    let default_max_body_bytes = u64_env("NOVOVM_RPC_MAX_BODY_BYTES", 64 * 1024);
    let public_max_body_bytes =
        u64_env("NOVOVM_PUBLIC_RPC_MAX_BODY_BYTES", default_max_body_bytes) as usize;
    let gov_max_body_bytes =
        u64_env("NOVOVM_GOV_RPC_MAX_BODY_BYTES", default_max_body_bytes) as usize;

    let default_rate_limit_per_ip = u32_env("NOVOVM_RPC_RATE_LIMIT_PER_IP", 30);
    let public_rate_limit_per_ip = u32_env(
        "NOVOVM_PUBLIC_RPC_RATE_LIMIT_PER_IP",
        default_rate_limit_per_ip,
    );
    let gov_rate_limit_per_ip = u32_env(
        "NOVOVM_GOV_RPC_RATE_LIMIT_PER_IP",
        default_rate_limit_per_ip,
    );

    let legacy_max_requests = u32_env_allow_zero("NOVOVM_RPC_MAX_REQUESTS", 0);
    let public_max_requests =
        u32_env_allow_zero("NOVOVM_PUBLIC_RPC_MAX_REQUESTS", legacy_max_requests);
    let gov_max_requests = u32_env_allow_zero("NOVOVM_GOV_RPC_MAX_REQUESTS", legacy_max_requests);

    let db_path = chain_query_db_path();
    let governance_slash_policy = if enable_gov {
        Some(load_consensus_slash_policy()?.policy)
    } else {
        None
    };

    println!(
        "rpc_server_mode: public_enabled={} public_bind={} public_max_requests={} gov_enabled={} gov_bind={} gov_max_requests={} gov_allowlist_count={} db={}",
        enable_public,
        public_bind,
        public_max_requests,
        enable_gov,
        gov_bind,
        gov_max_requests,
        gov_allowlist.len(),
        db_path.display()
    );

    let mut handles = Vec::new();
    if enable_public {
        let public_db_path = db_path.clone();
        let public_bind_cloned = public_bind.clone();
        handles.push((
            RpcServerRole::Public,
            std::thread::spawn(move || {
                run_rpc_server_instance(
                    RpcServerRole::Public,
                    public_bind_cloned,
                    public_db_path,
                    public_max_body_bytes,
                    public_rate_limit_per_ip,
                    public_max_requests,
                    HashSet::new(),
                    None,
                )
            }),
        ));
    }

    if enable_gov {
        let gov_db_path = db_path.clone();
        let gov_bind_cloned = gov_bind.clone();
        let gov_allowlist_cloned = gov_allowlist.clone();
        let slash_policy = governance_slash_policy
            .clone()
            .ok_or_else(|| anyhow::anyhow!("missing governance slash policy"))?;
        handles.push((
            RpcServerRole::Governance,
            std::thread::spawn(move || {
                run_rpc_server_instance(
                    RpcServerRole::Governance,
                    gov_bind_cloned,
                    gov_db_path,
                    gov_max_body_bytes,
                    gov_rate_limit_per_ip,
                    gov_max_requests,
                    gov_allowlist_cloned,
                    Some(slash_policy),
                )
            }),
        ));
    }

    let mut exits: Vec<RpcServerExit> = Vec::new();
    for (role, handle) in handles {
        let joined = handle
            .join()
            .map_err(|_| anyhow::anyhow!("{} rpc server thread panicked", role.as_str()))?;
        let exit = joined.with_context(|| format!("{} rpc server failed", role.as_str()))?;
        exits.push(exit);
    }

    let public_processed = exits
        .iter()
        .find(|entry| entry.role == RpcServerRole::Public)
        .map(|entry| entry.processed)
        .unwrap_or(0);
    let gov_processed = exits
        .iter()
        .find(|entry| entry.role == RpcServerRole::Governance)
        .map(|entry| entry.processed)
        .unwrap_or(0);
    let public_bind_out = exits
        .iter()
        .find(|entry| entry.role == RpcServerRole::Public)
        .map(|entry| entry.bind.as_str())
        .unwrap_or("-");
    let gov_bind_out = exits
        .iter()
        .find(|entry| entry.role == RpcServerRole::Governance)
        .map(|entry| entry.bind.as_str())
        .unwrap_or("-");
    let public_max_requests_out = exits
        .iter()
        .find(|entry| entry.role == RpcServerRole::Public)
        .map(|entry| entry.max_requests)
        .unwrap_or(0);
    let gov_max_requests_out = exits
        .iter()
        .find(|entry| entry.role == RpcServerRole::Governance)
        .map(|entry| entry.max_requests)
        .unwrap_or(0);
    println!(
        "rpc_server_mode_out: public_bind={} public_processed={} public_max_requests={} gov_bind={} gov_processed={} gov_max_requests={}",
        public_bind_out,
        public_processed,
        public_max_requests_out,
        gov_bind_out,
        gov_processed,
        gov_max_requests_out
    );

    Ok(())
}

fn run_header_sync_probe_mode() -> Result<()> {
    let remote_headers = parse_u32_env("NOVOVM_HEADER_SYNC_REMOTE_HEADERS", 8)? as u64;
    let local_headers = parse_u32_env("NOVOVM_HEADER_SYNC_LOCAL_HEADERS", 3)? as u64;
    let fetch_limit = parse_u32_env("NOVOVM_HEADER_SYNC_FETCH_LIMIT", 64)? as u64;
    let tamper_parent_at = std::env::var("NOVOVM_HEADER_SYNC_TAMPER_PARENT_AT")
        .ok()
        .map(|raw| raw.trim().to_string())
        .filter(|raw| !raw.is_empty())
        .map(|raw| {
            raw.parse::<u64>()
                .with_context(|| format!("invalid NOVOVM_HEADER_SYNC_TAMPER_PARENT_AT: {}", raw))
        })
        .transpose()?
        .unwrap_or(0);

    let signal =
        run_header_sync_probe(remote_headers, local_headers, fetch_limit, tamper_parent_at)?;
    println!(
        "header_sync_out: mode={} codec={} remote_tip={} local_tip_before={} fetched={} applied={} local_tip_after={} complete={} pass={} tamper_at={} reason={}",
        signal.mode,
        signal.codec,
        signal.remote_tip,
        signal.local_tip_before,
        signal.fetched,
        signal.applied,
        signal.local_tip_after,
        signal.complete,
        signal.pass,
        signal.tamper_at,
        signal.reason
    );
    Ok(())
}

fn run_fast_state_sync_probe_mode() -> Result<()> {
    let remote_headers = parse_u32_env("NOVOVM_FAST_SYNC_REMOTE_HEADERS", 16)? as u64;
    let local_headers = parse_u32_env("NOVOVM_FAST_SYNC_LOCAL_HEADERS", 3)? as u64;
    let fetch_limit = parse_u32_env("NOVOVM_FAST_SYNC_FETCH_LIMIT", 128)? as u64;
    let tamper_snapshot_at = std::env::var("NOVOVM_STATE_SYNC_TAMPER_SNAPSHOT_AT")
        .ok()
        .map(|raw| raw.trim().to_string())
        .filter(|raw| !raw.is_empty())
        .map(|raw| {
            raw.parse::<u64>()
                .with_context(|| format!("invalid NOVOVM_STATE_SYNC_TAMPER_SNAPSHOT_AT: {}", raw))
        })
        .transpose()?
        .unwrap_or(0);

    let signal = run_fast_state_sync_probe(
        remote_headers,
        local_headers,
        fetch_limit,
        tamper_snapshot_at,
    )?;
    println!(
        "fast_state_sync_out: mode={} codec={} remote_tip={} local_tip_before={} fetched_headers={} applied_headers={} local_tip_after={} fast_complete={} snapshot_height={} snapshot_accounts={} snapshot_verified={} state_complete={} pass={} tamper_snapshot_at={} reason={}",
        signal.mode,
        signal.codec,
        signal.remote_tip,
        signal.local_tip_before,
        signal.fetched_headers,
        signal.applied_headers,
        signal.local_tip_after,
        signal.fast_complete,
        signal.snapshot_height,
        signal.snapshot_accounts,
        signal.snapshot_verified,
        signal.state_complete,
        signal.pass,
        signal.tamper_snapshot_at,
        signal.reason
    );
    Ok(())
}

fn run_network_dos_probe_mode() -> Result<()> {
    let invalid_peers = parse_u32_env("NOVOVM_NET_DOS_INVALID_PEERS", 2)? as u64;
    let invalid_burst = parse_u32_env("NOVOVM_NET_DOS_INVALID_BURST", 6)? as u64;
    let ban_after = parse_u32_env("NOVOVM_NET_DOS_BAN_AFTER", 3)? as u64;

    let signal = run_network_dos_probe(invalid_peers, invalid_burst, ban_after)?;
    println!(
        "network_dos_out: mode={} codec={} peers={} invalid_peers={} invalid_burst={} ban_after={} invalid_detected={} bans={} storm_rejected={} healthy_accepts={} pass={} reason={}",
        signal.mode,
        signal.codec,
        signal.peers,
        signal.invalid_peers,
        signal.invalid_burst,
        signal.ban_after,
        signal.invalid_detected,
        signal.bans,
        signal.storm_rejected,
        signal.healthy_accepts,
        signal.pass,
        signal.reason
    );
    Ok(())
}

fn run_pacemaker_failover_probe_mode() -> Result<()> {
    let nodes = parse_u32_env("NOVOVM_PACEMAKER_NODES", 4)? as u64;
    let failed_leader = std::env::var("NOVOVM_PACEMAKER_FAILED_LEADER")
        .ok()
        .map(|raw| raw.trim().to_string())
        .filter(|raw| !raw.is_empty())
        .map(|raw| {
            raw.parse::<u64>()
                .with_context(|| format!("invalid NOVOVM_PACEMAKER_FAILED_LEADER: {}", raw))
        })
        .transpose()?
        .unwrap_or(0);
    let signal = run_pacemaker_failover_probe(nodes, failed_leader)?;
    println!(
        "pacemaker_failover_out: mode={} transport={} nodes={} failed_leader={} initial_view={} next_view={} next_leader={} timeout_votes={} timeout_quorum={} timeout_cert={} local_view_advanced={} view_sync_votes={} new_view_votes={} qc_formed={} committed={} committed_height={} pass={} reason={}",
        signal.mode,
        signal.transport,
        signal.nodes,
        signal.failed_leader,
        signal.initial_view,
        signal.next_view,
        signal.next_leader,
        signal.timeout_votes,
        signal.timeout_quorum,
        signal.timeout_cert,
        signal.local_view_advanced,
        signal.view_sync_votes,
        signal.new_view_votes,
        signal.qc_formed,
        signal.committed,
        signal.committed_height,
        signal.pass,
        signal.reason
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

    // gossip: heartbeat (both directions)
    let heartbeat_a = ProtocolMessage::Gossip(ProtocolGossipMessage::Heartbeat {
        from,
        shard: ShardId((tx_count as u32) % 1024),
    });
    let heartbeat_b = ProtocolMessage::Gossip(ProtocolGossipMessage::Heartbeat {
        from: to,
        shard: ShardId(((tx_count as u32) + 1) % 1024),
    });
    transport
        .send(to, heartbeat_a)
        .context("network gossip send A->B failed")?;
    transport
        .send(from, heartbeat_b)
        .context("network gossip send B->A failed")?;

    // sync: distributed occc state sync message (both directions)
    let sync_a = ProtocolMessage::DistributedOcccGossip(DistributedGossipMessage {
        from: from.0 as u32,
        to: to.0 as u32,
        msg_type: DistributedMessageType::StateSync,
        payload: tx_count.to_le_bytes().to_vec(),
        timestamp: 0,
        seq: tx_count,
    });
    let sync_b = ProtocolMessage::DistributedOcccGossip(DistributedGossipMessage {
        from: to.0 as u32,
        to: from.0 as u32,
        msg_type: DistributedMessageType::StateSync,
        payload: tx_count.to_le_bytes().to_vec(),
        timestamp: 0,
        seq: tx_count,
    });
    transport
        .send(to, sync_a)
        .context("network sync send A->B failed")?;
    transport
        .send(from, sync_b)
        .context("network sync send B->A failed")?;

    // pacemaker: view-sync + new-view
    let view_sync_a = ProtocolMessage::Pacemaker(ProtocolPacemakerMessage::ViewSync {
        from,
        height: tx_count.saturating_add(1),
        view: 1,
        leader: to,
    });
    let view_sync_b = ProtocolMessage::Pacemaker(ProtocolPacemakerMessage::ViewSync {
        from: to,
        height: tx_count.saturating_add(1),
        view: 1,
        leader: to,
    });
    let new_view_a = ProtocolMessage::Pacemaker(ProtocolPacemakerMessage::NewView {
        from,
        height: tx_count.saturating_add(1),
        view: 2,
        high_qc_height: tx_count,
    });
    let new_view_b = ProtocolMessage::Pacemaker(ProtocolPacemakerMessage::NewView {
        from: to,
        height: tx_count.saturating_add(1),
        view: 2,
        high_qc_height: tx_count,
    });
    transport
        .send(to, view_sync_a)
        .context("network pacemaker view-sync send A->B failed")?;
    transport
        .send(from, view_sync_b)
        .context("network pacemaker view-sync send B->A failed")?;
    transport
        .send(to, new_view_a)
        .context("network pacemaker new-view send A->B failed")?;
    transport
        .send(from, new_view_b)
        .context("network pacemaker new-view send B->A failed")?;

    let mut discovery_from_a = false;
    let mut discovery_from_b = false;
    let mut gossip_from_a = false;
    let mut gossip_from_b = false;
    let mut sync_from_a = false;
    let mut sync_from_b = false;
    let mut view_sync_from_a = false;
    let mut view_sync_from_b = false;
    let mut new_view_from_a = false;
    let mut new_view_from_b = false;
    let mut received = 0u64;

    loop {
        let mut progressed = false;

        if let Some(msg) = transport
            .try_recv(to)
            .context("network recv on node B failed")?
        {
            progressed = true;
            received = received.saturating_add(1);
            match msg {
                ProtocolMessage::Gossip(ProtocolGossipMessage::PeerList { from: src, .. }) => {
                    if src == from {
                        discovery_from_a = true;
                    }
                }
                ProtocolMessage::Gossip(ProtocolGossipMessage::Heartbeat { from: src, .. }) => {
                    if src == from {
                        gossip_from_a = true;
                    }
                }
                ProtocolMessage::DistributedOcccGossip(DistributedGossipMessage {
                    from: src,
                    msg_type: DistributedMessageType::StateSync,
                    ..
                }) => {
                    if src as u64 == from.0 {
                        sync_from_a = true;
                    }
                }
                ProtocolMessage::Pacemaker(ProtocolPacemakerMessage::ViewSync {
                    from: src,
                    ..
                }) => {
                    if src == from {
                        view_sync_from_a = true;
                    }
                }
                ProtocolMessage::Pacemaker(ProtocolPacemakerMessage::NewView {
                    from: src, ..
                }) => {
                    if src == from {
                        new_view_from_a = true;
                    }
                }
                _ => {}
            }
        }

        if let Some(msg) = transport
            .try_recv(from)
            .context("network recv on node A failed")?
        {
            progressed = true;
            received = received.saturating_add(1);
            match msg {
                ProtocolMessage::Gossip(ProtocolGossipMessage::PeerList { from: src, .. }) => {
                    if src == to {
                        discovery_from_b = true;
                    }
                }
                ProtocolMessage::Gossip(ProtocolGossipMessage::Heartbeat { from: src, .. }) => {
                    if src == to {
                        gossip_from_b = true;
                    }
                }
                ProtocolMessage::DistributedOcccGossip(DistributedGossipMessage {
                    from: src,
                    msg_type: DistributedMessageType::StateSync,
                    ..
                }) => {
                    if src as u64 == to.0 {
                        sync_from_b = true;
                    }
                }
                ProtocolMessage::Pacemaker(ProtocolPacemakerMessage::ViewSync {
                    from: src,
                    ..
                }) => {
                    if src == to {
                        view_sync_from_b = true;
                    }
                }
                ProtocolMessage::Pacemaker(ProtocolPacemakerMessage::NewView {
                    from: src, ..
                }) => {
                    if src == to {
                        new_view_from_b = true;
                    }
                }
                _ => {}
            }
        }

        if !progressed {
            break;
        }
    }

    let discovery = discovery_from_a && discovery_from_b;
    let gossip = gossip_from_a && gossip_from_b;
    let sync = sync_from_a && sync_from_b;
    let view_sync = view_sync_from_a && view_sync_from_b;
    let new_view = new_view_from_a && new_view_from_b;

    Ok(NetworkSignal {
        transport: "in_memory",
        from: from.0,
        to: to.0,
        nodes: 2,
        sent: 10,
        received,
        msg_kind: "two_node_discovery_gossip_sync_pacemaker",
        discovery,
        gossip,
        sync,
        view_sync,
        new_view,
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
    let mut view_roster = expected_peer_ids.clone();
    view_roster.push(from);
    view_roster.sort_by_key(|id| id.0);
    view_roster.dedup_by_key(|id| id.0);
    if view_roster.is_empty() {
        bail!("network probe has empty view roster");
    }
    let view_sync_height = 1u64;
    let view_sync_view = 1u64;
    let new_view_view = 2u64;
    let view_sync_leader = view_roster[(view_sync_view as usize) % view_roster.len()];
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
    let mut view_sync_from: HashSet<u64> = HashSet::new();
    let mut new_view_from: HashSet<u64> = HashSet::new();
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
                let view_sync = ProtocolMessage::Pacemaker(ProtocolPacemakerMessage::ViewSync {
                    from,
                    height: view_sync_height,
                    view: view_sync_view,
                    leader: view_sync_leader,
                });
                transport
                    .send(*to, view_sync)
                    .context("udp transport send pacemaker view-sync failed")?;
                let new_view = ProtocolMessage::Pacemaker(ProtocolPacemakerMessage::NewView {
                    from,
                    height: view_sync_height,
                    view: new_view_view,
                    high_qc_height: 0,
                });
                transport
                    .send(*to, new_view)
                    .context("udp transport send pacemaker new-view failed")?;
                sent = sent.saturating_add(5);
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
                    ProtocolMessage::Gossip(ProtocolGossipMessage::PeerList {
                        from: src, ..
                    }) => {
                        if expected_set.contains(&src.0) {
                            discovery_from.insert(src.0);
                        }
                    }
                    ProtocolMessage::Gossip(ProtocolGossipMessage::Heartbeat {
                        from: src, ..
                    }) => {
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
                    ProtocolMessage::Pacemaker(ProtocolPacemakerMessage::ViewSync {
                        from: src,
                        height,
                        view,
                        leader,
                    }) => {
                        let src_id = src.0;
                        if expected_set.contains(&src_id)
                            && height == view_sync_height
                            && view == view_sync_view
                            && leader == view_sync_leader
                        {
                            view_sync_from.insert(src_id);
                        }
                    }
                    ProtocolMessage::Pacemaker(ProtocolPacemakerMessage::NewView {
                        from: src,
                        height,
                        view,
                        high_qc_height,
                    }) => {
                        let src_id = src.0;
                        if expected_set.contains(&src_id)
                            && height == view_sync_height
                            && view == new_view_view
                            && high_qc_height <= height
                        {
                            new_view_from.insert(src_id);
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
            && view_sync_from.len() == expected_count
            && new_view_from.len() == expected_count
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
            && view_sync_from.contains(id)
            && new_view_from.contains(id)
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
    let got_view_sync = view_sync_from.len() == expected_count;
    let got_new_view = new_view_from.len() == expected_count;
    let got_block_wire = block_wire_from.len() == expected_count;
    let block_wire_min = if block_wire_min_bytes == usize::MAX {
        0
    } else {
        block_wire_min_bytes
    };

    println!(
        "network_probe_out: transport=udp node={} listen={} peer={} sent={} received={} discovery={} gossip={} sync={} view_sync={} new_view={}",
        node_id, listen, peer_label, sent, received, got_discovery, got_gossip, got_sync, got_view_sync, got_new_view
    );
    println!(
        "network_probe_graph: node={} peers={} discovery_ok={}/{} gossip_ok={}/{} sync_ok={}/{} view_sync_ok={}/{} new_view_ok={}/{} edge_ok={}/{}",
        node_id,
        expected_count,
        discovery_from.len(),
        expected_count,
        gossip_from.len(),
        expected_count,
        sync_from.len(),
        expected_count,
        view_sync_from.len(),
        expected_count,
        new_view_from.len(),
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
        && (!got_discovery
            || !got_gossip
            || !got_sync
            || !got_view_sync
            || !got_new_view
            || !got_block_wire)
    {
        bail!(
            "network probe closure incomplete: discovery={} gossip={} sync={} view_sync={} new_view={} block_wire={} node={}",
            got_discovery,
            got_gossip,
            got_sync,
            got_view_sync,
            got_new_view,
            got_block_wire,
            node_id
        );
    }

    Ok(())
}

fn run_slash_policy_probe_mode() -> Result<()> {
    let loaded = load_consensus_slash_policy()?;
    emit_slash_policy_in_signal(&loaded);

    // Probe mode only checks policy injection into consensus, without AOEM runtime dependency.
    let validator_set = ValidatorSet::new_equal_weight(vec![0, 1]);
    let signing_key = SigningKey::generate(&mut OsRng);
    let peer_signing_key = SigningKey::generate(&mut OsRng);
    let mut public_keys = HashMap::new();
    public_keys.insert(0, signing_key.verifying_key());
    public_keys.insert(1, peer_signing_key.verifying_key());
    let engine = BFTEngine::new(
        BFTConfig::default(),
        0,
        signing_key,
        validator_set,
        public_keys,
    )
    .context("slash policy probe: init novovm-consensus engine failed")?;
    engine
        .set_slash_policy(loaded.policy.clone())
        .context("slash policy probe: set slash policy failed")?;
    let applied = engine.slash_policy();
    let injected = applied == loaded.policy;
    println!(
        "slash_policy_probe_out: injected={} mode={} threshold={} min_validators={} cooldown_epochs={}",
        injected,
        applied.mode.as_str(),
        applied.equivocation_threshold,
        applied.min_active_validators,
        loaded.cooldown_epochs
    );
    if !injected {
        bail!(
            "slash policy probe mismatch: expect mode={} threshold={} min_validators={}, got mode={} threshold={} min_validators={}",
            loaded.policy.mode.as_str(),
            loaded.policy.equivocation_threshold,
            loaded.policy.min_active_validators,
            applied.mode.as_str(),
            applied.equivocation_threshold,
            applied.min_active_validators
        );
    }

    Ok(())
}

fn load_governance_update_slash_policy() -> Result<SlashPolicy> {
    let mode_raw =
        std::env::var("NOVOVM_GOV_SLASH_MODE").unwrap_or_else(|_| "observe_only".to_string());
    let mode = parse_slash_mode(&mode_raw)?;
    let policy = SlashPolicy {
        mode,
        equivocation_threshold: u32_env("NOVOVM_GOV_SLASH_THRESHOLD", 3),
        min_active_validators: u32_env("NOVOVM_GOV_SLASH_MIN_VALIDATORS", 2),
        cooldown_epochs: u64_env("NOVOVM_GOV_SLASH_COOLDOWN_EPOCHS", 6),
    };
    policy
        .validate()
        .map_err(|e| anyhow::anyhow!("governance_policy_invalid: {}", e))?;
    Ok(policy)
}

fn load_governance_mempool_fee_floor() -> Result<u64> {
    let fee_floor = u64_env("NOVOVM_GOV_MEMPOOL_FEE_FLOOR", 9);
    if fee_floor == 0 {
        bail!("governance_policy_invalid: mempool fee floor must be > 0");
    }
    Ok(fee_floor)
}

fn load_governance_network_dos_policy() -> Result<NetworkDosPolicy> {
    let rpc_rate_limit_per_ip = u32_env("NOVOVM_GOV_NETWORK_DOS_RATE_LIMIT_PER_IP", 96);
    let peer_ban_threshold_raw = std::env::var("NOVOVM_GOV_NETWORK_DOS_PEER_BAN_THRESHOLD")
        .ok()
        .and_then(|v| v.trim().parse::<i32>().ok())
        .unwrap_or(-6);
    let policy = NetworkDosPolicy {
        rpc_rate_limit_per_ip,
        peer_ban_threshold: peer_ban_threshold_raw,
    };
    policy
        .validate()
        .map_err(|e| anyhow::anyhow!("governance_policy_invalid: {}", e))?;
    Ok(policy)
}

fn load_governance_token_economics_policy() -> Result<TokenEconomicsPolicy> {
    fn parse_bp(name: &str, default: u16) -> Result<u16> {
        match std::env::var(name) {
            Ok(raw) => {
                let trimmed = raw.trim();
                if trimmed.is_empty() {
                    return Ok(default);
                }
                let parsed = trimmed
                    .parse::<u16>()
                    .with_context(|| format!("invalid {} value: {}", name, raw))?;
                Ok(parsed)
            }
            Err(_) => Ok(default),
        }
    }

    let policy = TokenEconomicsPolicy {
        max_supply: u64_env("NOVOVM_GOV_TOKEN_MAX_SUPPLY", 1_000_000),
        locked_supply: u64_env("NOVOVM_GOV_TOKEN_LOCKED_SUPPLY", 300_000),
        fee_split: novovm_consensus::FeeSplit {
            gas_base_burn_bp: parse_bp("NOVOVM_GOV_TOKEN_GAS_BASE_BURN_BP", 2_000)?,
            gas_to_node_bp: parse_bp("NOVOVM_GOV_TOKEN_GAS_TO_NODE_BP", 3_000)?,
            service_burn_bp: parse_bp("NOVOVM_GOV_TOKEN_SERVICE_BURN_BP", 1_000)?,
            service_to_provider_bp: parse_bp("NOVOVM_GOV_TOKEN_SERVICE_TO_PROVIDER_BP", 4_000)?,
        },
    };
    policy
        .validate()
        .map_err(|e| anyhow::anyhow!("governance_policy_invalid: {}", e))?;
    Ok(policy)
}

fn load_governance_market_policy() -> Result<MarketGovernancePolicy> {
    fn parse_u16_env(name: &str, default: u16) -> Result<u16> {
        match std::env::var(name) {
            Ok(raw) => {
                let trimmed = raw.trim();
                if trimmed.is_empty() {
                    return Ok(default);
                }
                trimmed
                    .parse::<u16>()
                    .with_context(|| format!("invalid {} value: {}", name, raw))
            }
            Err(_) => Ok(default),
        }
    }

    let defaults = MarketGovernancePolicy::default();
    let policy = MarketGovernancePolicy {
        amm: AmmGovernanceParams {
            swap_fee_bp: parse_u16_env(
                "NOVOVM_GOV_MARKET_AMM_SWAP_FEE_BP",
                defaults.amm.swap_fee_bp,
            )?,
            lp_fee_share_bp: parse_u16_env(
                "NOVOVM_GOV_MARKET_AMM_LP_FEE_SHARE_BP",
                defaults.amm.lp_fee_share_bp,
            )?,
        },
        cdp: CdpGovernanceParams {
            min_collateral_ratio_bp: parse_u16_env(
                "NOVOVM_GOV_MARKET_CDP_MIN_COLLATERAL_RATIO_BP",
                defaults.cdp.min_collateral_ratio_bp,
            )?,
            liquidation_threshold_bp: parse_u16_env(
                "NOVOVM_GOV_MARKET_CDP_LIQUIDATION_THRESHOLD_BP",
                defaults.cdp.liquidation_threshold_bp,
            )?,
            liquidation_penalty_bp: parse_u16_env(
                "NOVOVM_GOV_MARKET_CDP_LIQUIDATION_PENALTY_BP",
                defaults.cdp.liquidation_penalty_bp,
            )?,
            stability_fee_bp: parse_u16_env(
                "NOVOVM_GOV_MARKET_CDP_STABILITY_FEE_BP",
                defaults.cdp.stability_fee_bp,
            )?,
            max_leverage_x100: parse_u16_env(
                "NOVOVM_GOV_MARKET_CDP_MAX_LEVERAGE_X100",
                defaults.cdp.max_leverage_x100,
            )?,
        },
        bond: BondGovernanceParams {
            coupon_rate_bp: parse_u16_env(
                "NOVOVM_GOV_MARKET_BOND_COUPON_RATE_BP",
                defaults.bond.coupon_rate_bp,
            )?,
            max_maturity_days: parse_u16_env(
                "NOVOVM_GOV_MARKET_BOND_MAX_MATURITY_DAYS",
                defaults.bond.max_maturity_days,
            )?,
            min_issue_price_bp: parse_u16_env(
                "NOVOVM_GOV_MARKET_BOND_MIN_ISSUE_PRICE_BP",
                defaults.bond.min_issue_price_bp,
            )?,
        },
        reserve: ReserveGovernanceParams {
            min_reserve_ratio_bp: parse_u16_env(
                "NOVOVM_GOV_MARKET_RESERVE_MIN_RESERVE_RATIO_BP",
                defaults.reserve.min_reserve_ratio_bp,
            )?,
            redemption_fee_bp: parse_u16_env(
                "NOVOVM_GOV_MARKET_RESERVE_REDEMPTION_FEE_BP",
                defaults.reserve.redemption_fee_bp,
            )?,
        },
        nav: NavGovernanceParams {
            settlement_delay_epochs: parse_u16_env(
                "NOVOVM_GOV_MARKET_NAV_SETTLEMENT_DELAY_EPOCHS",
                defaults.nav.settlement_delay_epochs,
            )?,
            max_daily_redemption_bp: parse_u16_env(
                "NOVOVM_GOV_MARKET_NAV_MAX_DAILY_REDEMPTION_BP",
                defaults.nav.max_daily_redemption_bp,
            )?,
        },
        buyback: BuybackGovernanceParams {
            trigger_discount_bp: parse_u16_env(
                "NOVOVM_GOV_MARKET_BUYBACK_TRIGGER_DISCOUNT_BP",
                defaults.buyback.trigger_discount_bp,
            )?,
            max_treasury_budget_per_epoch: u64_env(
                "NOVOVM_GOV_MARKET_BUYBACK_MAX_TREASURY_BUDGET_PER_EPOCH",
                defaults.buyback.max_treasury_budget_per_epoch,
            ),
            burn_share_bp: parse_u16_env(
                "NOVOVM_GOV_MARKET_BUYBACK_BURN_SHARE_BP",
                defaults.buyback.burn_share_bp,
            )?,
        },
    };
    policy
        .validate()
        .map_err(|e| anyhow::anyhow!("governance_policy_invalid: {}", e))?;
    Ok(policy)
}

fn parse_node_id_csv(name: &str, raw: &str) -> Result<Vec<ConsensusNodeId>> {
    let mut out = Vec::new();
    for part in raw.split(',') {
        let token = part.trim();
        if token.is_empty() {
            continue;
        }
        let parsed = token
            .parse::<u32>()
            .with_context(|| format!("invalid {} node id: {}", name, token))?;
        out.push(parsed);
    }
    if out.is_empty() {
        bail!("{} resolved to empty list", name);
    }
    Ok(out)
}

fn load_governance_access_policy() -> Result<GovernanceAccessPolicy> {
    let proposer_raw =
        std::env::var("NOVOVM_GOV_ACCESS_PROPOSER_COMMITTEE").unwrap_or_else(|_| "0,1".to_string());
    let executor_raw =
        std::env::var("NOVOVM_GOV_ACCESS_EXECUTOR_COMMITTEE").unwrap_or_else(|_| "1,2".to_string());
    let proposer_committee =
        parse_node_id_csv("NOVOVM_GOV_ACCESS_PROPOSER_COMMITTEE", &proposer_raw)?;
    let executor_committee =
        parse_node_id_csv("NOVOVM_GOV_ACCESS_EXECUTOR_COMMITTEE", &executor_raw)?;
    let policy = GovernanceAccessPolicy {
        proposer_committee,
        proposer_threshold: u32_env("NOVOVM_GOV_ACCESS_PROPOSER_THRESHOLD", 2),
        executor_committee,
        executor_threshold: u32_env("NOVOVM_GOV_ACCESS_EXECUTOR_THRESHOLD", 2),
        timelock_epochs: u64_env("NOVOVM_GOV_ACCESS_TIMELOCK_EPOCHS", 1),
    };
    policy
        .validate()
        .map_err(|e| anyhow::anyhow!("governance_policy_invalid: {}", e))?;
    Ok(policy)
}

fn load_governance_treasury_spend() -> Result<(ConsensusNodeId, u64, String)> {
    let to = u64_env("NOVOVM_GOV_TREASURY_TO", 7) as ConsensusNodeId;
    let amount = u64_env("NOVOVM_GOV_TREASURY_AMOUNT", 60);
    let reason = std::env::var("NOVOVM_GOV_TREASURY_REASON")
        .unwrap_or_else(|_| "ecosystem_grant".to_string());
    let reason = reason.trim().to_string();
    if amount == 0 {
        bail!("governance_policy_invalid: treasury spend amount must be > 0");
    }
    if reason.is_empty() {
        bail!("governance_policy_invalid: treasury spend reason cannot be empty");
    }
    if reason.len() > 128 {
        bail!("governance_policy_invalid: treasury spend reason too long (max 128)");
    }
    Ok((to, amount, reason))
}

fn run_governance_hook_probe_mode() -> Result<()> {
    let loaded = load_consensus_slash_policy()?;
    emit_slash_policy_in_signal(&loaded);
    let governance_policy = load_governance_update_slash_policy()?;
    println!(
        "governance_op_in: op=update_slash_policy mode={} threshold={} min_validators={} cooldown_epochs={}",
        governance_policy.mode.as_str(),
        governance_policy.equivocation_threshold,
        governance_policy.min_active_validators,
        governance_policy.cooldown_epochs
    );

    // Probe mode only verifies governance hook behavior, without enabling governance execution.
    let validator_set = ValidatorSet::new_equal_weight(vec![0, 1]);
    let signing_key = SigningKey::generate(&mut OsRng);
    let peer_signing_key = SigningKey::generate(&mut OsRng);
    let mut public_keys = HashMap::new();
    public_keys.insert(0, signing_key.verifying_key());
    public_keys.insert(1, peer_signing_key.verifying_key());
    let engine = BFTEngine::new(
        BFTConfig::default(),
        0,
        signing_key,
        validator_set,
        public_keys,
    )
    .context("governance hook probe: init novovm-consensus engine failed")?;
    engine
        .set_slash_policy(loaded.policy.clone())
        .context("governance hook probe: set baseline slash policy failed")?;
    let policy_before = engine.slash_policy();

    let staged_result = engine.stage_governance_op(GovernanceOp::UpdateSlashPolicy {
        policy: governance_policy.clone(),
    });
    let reason_code = match &staged_result {
        Err(err) => {
            let msg = err.to_string().to_ascii_lowercase();
            if msg.contains("governance not enabled") {
                "governance_not_enabled"
            } else {
                "governance_hook_error"
            }
        }
        Ok(_) => "executed_unexpectedly",
    };

    let policy_after = engine.slash_policy();
    let policy_unchanged = policy_after == policy_before;
    let staged_ops = engine.staged_governance_ops();
    let staged_match = staged_ops
        .last()
        .map(|op| {
            matches!(
                op,
                GovernanceOp::UpdateSlashPolicy { policy } if policy == &governance_policy
            )
        })
        .unwrap_or(false);
    let staged = !staged_ops.is_empty();
    let executed = staged_result.is_ok() || !policy_unchanged;
    println!(
        "governance_op_hook_out: staged={} executed={} reason_code={} policy_unchanged={} staged_ops={} staged_match={}",
        staged,
        executed,
        reason_code,
        policy_unchanged,
        staged_ops.len(),
        staged_match
    );

    if !staged
        || executed
        || !policy_unchanged
        || !staged_match
        || reason_code != "governance_not_enabled"
    {
        bail!(
            "governance hook probe failed: staged={} executed={} reason_code={} policy_unchanged={} staged_match={} staged_ops={}",
            staged,
            executed,
            reason_code,
            policy_unchanged,
            staged_match,
            staged_ops.len()
        );
    }
    Ok(())
}

fn run_governance_execute_probe_mode() -> Result<()> {
    let loaded = load_consensus_slash_policy()?;
    emit_slash_policy_in_signal(&loaded);
    let governance_policy = load_governance_update_slash_policy()?;

    let validator_set = ValidatorSet::new_equal_weight(vec![0, 1, 2]);
    let required_quorum = validator_set.quorum_size();
    let signing_keys: Vec<_> = (0..3).map(|_| SigningKey::generate(&mut OsRng)).collect();
    let mut public_keys = HashMap::new();
    public_keys.insert(0, signing_keys[0].verifying_key());
    public_keys.insert(1, signing_keys[1].verifying_key());
    public_keys.insert(2, signing_keys[2].verifying_key());

    let engine = BFTEngine::new(
        BFTConfig::default(),
        0,
        signing_keys[0].clone(),
        validator_set,
        public_keys,
    )
    .context("governance execute probe: init novovm-consensus engine failed")?;
    engine
        .set_slash_policy(loaded.policy.clone())
        .context("governance execute probe: set baseline slash policy failed")?;
    engine.set_governance_execution_enabled(true);

    let proposal = engine
        .submit_governance_proposal(
            0,
            GovernanceOp::UpdateSlashPolicy {
                policy: governance_policy.clone(),
            },
        )
        .context("governance execute probe: submit proposal failed")?;
    let votes = vec![
        GovernanceVote::new(&proposal, 0, true, &signing_keys[0]),
        GovernanceVote::new(&proposal, 1, true, &signing_keys[1]),
    ];
    println!(
        "governance_execute_in: proposal_id={} op=update_slash_policy mode={} threshold={} min_validators={} cooldown_epochs={} votes={} quorum={}",
        proposal.proposal_id,
        governance_policy.mode.as_str(),
        governance_policy.equivocation_threshold,
        governance_policy.min_active_validators,
        governance_policy.cooldown_epochs,
        votes.len(),
        required_quorum
    );

    let exec_result = engine.execute_governance_proposal(proposal.proposal_id, &votes);
    let reason_code = match &exec_result {
        Ok(_) => "ok",
        Err(err) => {
            let msg = err.to_string().to_ascii_lowercase();
            if msg.contains("insufficient votes") {
                "insufficient_votes"
            } else if msg.contains("invalid signature") {
                "invalid_signature"
            } else if msg.contains("governance not enabled") {
                "governance_not_enabled"
            } else {
                "governance_execution_error"
            }
        }
    };
    let executed = exec_result.is_ok();
    let applied = engine.slash_policy() == governance_policy;
    println!(
        "governance_execute_out: proposal_id={} executed={} reason_code={} policy_applied={} mode={} threshold={} min_validators={} cooldown_epochs={}",
        proposal.proposal_id,
        executed,
        reason_code,
        applied,
        engine.slash_policy().mode.as_str(),
        engine.slash_policy().equivocation_threshold,
        engine.slash_policy().min_active_validators,
        engine.slash_policy().cooldown_epochs
    );

    if !executed || !applied || reason_code != "ok" {
        bail!(
            "governance execute probe failed: executed={} applied={} reason_code={}",
            executed,
            applied,
            reason_code
        );
    }
    Ok(())
}

fn run_governance_negative_probe_mode() -> Result<()> {
    let loaded = load_consensus_slash_policy()?;
    emit_slash_policy_in_signal(&loaded);
    let governance_policy = load_governance_update_slash_policy()?;

    let validator_set = ValidatorSet::new_equal_weight(vec![0, 1, 2]);
    let signing_keys: Vec<_> = (0..3).map(|_| SigningKey::generate(&mut OsRng)).collect();
    let mut public_keys = HashMap::new();
    public_keys.insert(0, signing_keys[0].verifying_key());
    public_keys.insert(1, signing_keys[1].verifying_key());
    public_keys.insert(2, signing_keys[2].verifying_key());

    let engine = BFTEngine::new(
        BFTConfig::default(),
        0,
        signing_keys[0].clone(),
        validator_set,
        public_keys,
    )
    .context("governance negative probe: init novovm-consensus engine failed")?;
    engine
        .set_slash_policy(loaded.policy.clone())
        .context("governance negative probe: set baseline slash policy failed")?;
    engine.set_governance_execution_enabled(true);

    let unauthorized_submit = matches!(
        engine.submit_governance_proposal(
            99,
            GovernanceOp::UpdateSlashPolicy {
                policy: governance_policy.clone(),
            }
        ),
        Err(novovm_consensus::BFTError::NotValidator(99))
    );

    let invalid_sig_proposal = engine
        .submit_governance_proposal(
            0,
            GovernanceOp::UpdateSlashPolicy {
                policy: governance_policy.clone(),
            },
        )
        .context("governance negative probe: submit invalid-signature proposal failed")?;
    let invalid_sig_votes = vec![
        GovernanceVote::new(&invalid_sig_proposal, 0, true, &signing_keys[0]),
        // voter_id=1 but signed by key[2], should fail verification.
        GovernanceVote::new(&invalid_sig_proposal, 1, true, &signing_keys[2]),
    ];
    let invalid_signature = matches!(
        engine.execute_governance_proposal(invalid_sig_proposal.proposal_id, &invalid_sig_votes),
        Err(novovm_consensus::BFTError::InvalidSignature(_))
    );

    let duplicate_vote_proposal = engine
        .submit_governance_proposal(
            0,
            GovernanceOp::UpdateSlashPolicy {
                policy: governance_policy.clone(),
            },
        )
        .context("governance negative probe: submit duplicate-vote proposal failed")?;
    let duplicate_vote_votes = vec![
        GovernanceVote::new(&duplicate_vote_proposal, 0, true, &signing_keys[0]),
        GovernanceVote::new(&duplicate_vote_proposal, 0, true, &signing_keys[0]),
    ];
    let duplicate_vote = matches!(
        engine.execute_governance_proposal(
            duplicate_vote_proposal.proposal_id,
            &duplicate_vote_votes
        ),
        Err(novovm_consensus::BFTError::DuplicateVote(0))
    );

    let insufficient_votes_proposal = engine
        .submit_governance_proposal(
            0,
            GovernanceOp::UpdateSlashPolicy {
                policy: governance_policy.clone(),
            },
        )
        .context("governance negative probe: submit insufficient-votes proposal failed")?;
    let insufficient_votes_set = vec![GovernanceVote::new(
        &insufficient_votes_proposal,
        0,
        true,
        &signing_keys[0],
    )];
    let insufficient_votes = matches!(
        engine.execute_governance_proposal(
            insufficient_votes_proposal.proposal_id,
            &insufficient_votes_set
        ),
        Err(novovm_consensus::BFTError::InsufficientVotes { .. })
    );

    let replay_proposal = engine
        .submit_governance_proposal(
            0,
            GovernanceOp::UpdateSlashPolicy {
                policy: governance_policy.clone(),
            },
        )
        .context("governance negative probe: submit replay proposal failed")?;
    let replay_votes = vec![
        GovernanceVote::new(&replay_proposal, 0, true, &signing_keys[0]),
        GovernanceVote::new(&replay_proposal, 1, true, &signing_keys[1]),
    ];
    let first_exec_ok = engine
        .execute_governance_proposal(replay_proposal.proposal_id, &replay_votes)
        .is_ok();
    let replay_execute =
        match engine.execute_governance_proposal(replay_proposal.proposal_id, &replay_votes) {
            Err(novovm_consensus::BFTError::Internal(msg)) => {
                msg.to_ascii_lowercase().contains("not found")
            }
            _ => false,
        };

    println!(
        "governance_negative_out: unauthorized_submit={} invalid_signature={} duplicate_vote={} insufficient_votes={} replay_execute={} first_exec_ok={}",
        unauthorized_submit,
        invalid_signature,
        duplicate_vote,
        insufficient_votes,
        replay_execute,
        first_exec_ok
    );

    if !unauthorized_submit
        || !invalid_signature
        || !duplicate_vote
        || !insufficient_votes
        || !first_exec_ok
        || !replay_execute
    {
        bail!(
            "governance negative probe failed: unauthorized_submit={} invalid_signature={} duplicate_vote={} insufficient_votes={} replay_execute={} first_exec_ok={}",
            unauthorized_submit,
            invalid_signature,
            duplicate_vote,
            insufficient_votes,
            replay_execute,
            first_exec_ok
        );
    }

    Ok(())
}

fn run_governance_param2_probe_mode() -> Result<()> {
    let loaded = load_consensus_slash_policy()?;
    emit_slash_policy_in_signal(&loaded);
    let mempool_fee_floor = load_governance_mempool_fee_floor()?;

    let validator_set = ValidatorSet::new_equal_weight(vec![0, 1, 2]);
    let required_quorum = validator_set.quorum_size();
    let signing_keys: Vec<_> = (0..3).map(|_| SigningKey::generate(&mut OsRng)).collect();
    let mut public_keys = HashMap::new();
    public_keys.insert(0, signing_keys[0].verifying_key());
    public_keys.insert(1, signing_keys[1].verifying_key());
    public_keys.insert(2, signing_keys[2].verifying_key());
    let engine = BFTEngine::new(
        BFTConfig::default(),
        0,
        signing_keys[0].clone(),
        validator_set,
        public_keys,
    )
    .context("governance param2 probe: init novovm-consensus engine failed")?;
    engine
        .set_slash_policy(loaded.policy.clone())
        .context("governance param2 probe: set baseline slash policy failed")?;
    engine.set_governance_execution_enabled(true);

    let proposal = engine
        .submit_governance_proposal(
            0,
            GovernanceOp::UpdateMempoolFeeFloor {
                fee_floor: mempool_fee_floor,
            },
        )
        .context("governance param2 probe: submit proposal failed")?;
    let votes = vec![
        GovernanceVote::new(&proposal, 0, true, &signing_keys[0]),
        GovernanceVote::new(&proposal, 1, true, &signing_keys[1]),
    ];
    println!(
        "governance_param2_in: proposal_id={} op=update_mempool_fee_floor fee_floor={} votes={} quorum={}",
        proposal.proposal_id,
        mempool_fee_floor,
        votes.len(),
        required_quorum
    );
    let exec_result = engine.execute_governance_proposal(proposal.proposal_id, &votes);
    let reason_code = match &exec_result {
        Ok(_) => "ok",
        Err(err) => {
            let msg = err.to_string().to_ascii_lowercase();
            if msg.contains("insufficient votes") {
                "insufficient_votes"
            } else if msg.contains("invalid signature") {
                "invalid_signature"
            } else if msg.contains("governance not enabled") {
                "governance_not_enabled"
            } else {
                "governance_execution_error"
            }
        }
    };
    let executed = exec_result.is_ok();
    let applied = engine.governance_mempool_fee_floor() == mempool_fee_floor;
    println!(
        "governance_param2_out: proposal_id={} executed={} reason_code={} fee_floor_applied={} fee_floor={}",
        proposal.proposal_id,
        executed,
        reason_code,
        applied,
        engine.governance_mempool_fee_floor()
    );

    if !executed || !applied || reason_code != "ok" {
        bail!(
            "governance param2 probe failed: executed={} applied={} reason_code={} fee_floor={}",
            executed,
            applied,
            reason_code,
            engine.governance_mempool_fee_floor()
        );
    }
    Ok(())
}

fn run_governance_param3_probe_mode() -> Result<()> {
    let loaded = load_consensus_slash_policy()?;
    emit_slash_policy_in_signal(&loaded);
    let network_dos_policy = load_governance_network_dos_policy()?;

    let validator_set = ValidatorSet::new_equal_weight(vec![0, 1, 2]);
    let required_quorum = validator_set.quorum_size();
    let signing_keys: Vec<_> = (0..3).map(|_| SigningKey::generate(&mut OsRng)).collect();
    let mut public_keys = HashMap::new();
    public_keys.insert(0, signing_keys[0].verifying_key());
    public_keys.insert(1, signing_keys[1].verifying_key());
    public_keys.insert(2, signing_keys[2].verifying_key());
    let engine = BFTEngine::new(
        BFTConfig::default(),
        0,
        signing_keys[0].clone(),
        validator_set,
        public_keys,
    )
    .context("governance param3 probe: init novovm-consensus engine failed")?;
    engine
        .set_slash_policy(loaded.policy.clone())
        .context("governance param3 probe: set baseline slash policy failed")?;
    engine.set_governance_execution_enabled(true);

    let proposal = engine
        .submit_governance_proposal(
            0,
            GovernanceOp::UpdateNetworkDosPolicy {
                policy: network_dos_policy.clone(),
            },
        )
        .context("governance param3 probe: submit proposal failed")?;
    let votes = vec![
        GovernanceVote::new(&proposal, 0, true, &signing_keys[0]),
        GovernanceVote::new(&proposal, 1, true, &signing_keys[1]),
    ];
    println!(
        "governance_param3_in: proposal_id={} op=update_network_dos_policy rpc_rate_limit_per_ip={} peer_ban_threshold={} votes={} quorum={}",
        proposal.proposal_id,
        network_dos_policy.rpc_rate_limit_per_ip,
        network_dos_policy.peer_ban_threshold,
        votes.len(),
        required_quorum
    );
    let exec_result = engine.execute_governance_proposal(proposal.proposal_id, &votes);
    let reason_code = match &exec_result {
        Ok(_) => "ok",
        Err(err) => {
            let msg = err.to_string().to_ascii_lowercase();
            if msg.contains("insufficient votes") {
                "insufficient_votes"
            } else if msg.contains("invalid signature") {
                "invalid_signature"
            } else if msg.contains("governance not enabled") {
                "governance_not_enabled"
            } else {
                "governance_execution_error"
            }
        }
    };
    let executed = exec_result.is_ok();
    let applied = engine.governance_network_dos_policy() == network_dos_policy;
    let applied_policy = engine.governance_network_dos_policy();
    println!(
        "governance_param3_out: proposal_id={} executed={} reason_code={} policy_applied={} rpc_rate_limit_per_ip={} peer_ban_threshold={}",
        proposal.proposal_id,
        executed,
        reason_code,
        applied,
        applied_policy.rpc_rate_limit_per_ip,
        applied_policy.peer_ban_threshold
    );

    if !executed || !applied || reason_code != "ok" {
        bail!(
            "governance param3 probe failed: executed={} applied={} reason_code={} rpc_rate_limit_per_ip={} peer_ban_threshold={}",
            executed,
            applied,
            reason_code,
            applied_policy.rpc_rate_limit_per_ip,
            applied_policy.peer_ban_threshold
        );
    }
    Ok(())
}

fn run_governance_token_economics_probe_mode() -> Result<()> {
    let loaded = load_consensus_slash_policy()?;
    emit_slash_policy_in_signal(&loaded);
    let token_policy = load_governance_token_economics_policy()?;

    let validator_set = ValidatorSet::new_equal_weight(vec![0, 1, 2]);
    let required_quorum = validator_set.quorum_size();
    let signing_keys: Vec<_> = (0..3).map(|_| SigningKey::generate(&mut OsRng)).collect();
    let mut public_keys = HashMap::new();
    public_keys.insert(0, signing_keys[0].verifying_key());
    public_keys.insert(1, signing_keys[1].verifying_key());
    public_keys.insert(2, signing_keys[2].verifying_key());
    let engine = BFTEngine::new(
        BFTConfig::default(),
        0,
        signing_keys[0].clone(),
        validator_set,
        public_keys,
    )
    .context("governance token probe: init novovm-consensus engine failed")?;
    engine
        .set_slash_policy(loaded.policy.clone())
        .context("governance token probe: set baseline slash policy failed")?;
    engine.set_governance_execution_enabled(true);

    let proposal = engine
        .submit_governance_proposal(
            0,
            GovernanceOp::UpdateTokenEconomicsPolicy {
                policy: token_policy.clone(),
            },
        )
        .context("governance token probe: submit proposal failed")?;
    let votes = vec![
        GovernanceVote::new(&proposal, 0, true, &signing_keys[0]),
        GovernanceVote::new(&proposal, 1, true, &signing_keys[1]),
    ];
    println!(
        "governance_token_in: proposal_id={} op=update_token_economics_policy max_supply={} locked_supply={} gas_base_burn_bp={} gas_to_node_bp={} service_burn_bp={} service_to_provider_bp={} votes={} quorum={}",
        proposal.proposal_id,
        token_policy.max_supply,
        token_policy.locked_supply,
        token_policy.fee_split.gas_base_burn_bp,
        token_policy.fee_split.gas_to_node_bp,
        token_policy.fee_split.service_burn_bp,
        token_policy.fee_split.service_to_provider_bp,
        votes.len(),
        required_quorum
    );

    let exec_result = engine.execute_governance_proposal(proposal.proposal_id, &votes);
    let reason_code = match &exec_result {
        Ok(_) => "ok",
        Err(err) => {
            let msg = err.to_string().to_ascii_lowercase();
            if msg.contains("insufficient votes") {
                "insufficient_votes"
            } else if msg.contains("invalid signature") {
                "invalid_signature"
            } else if msg.contains("governance not enabled") {
                "governance_not_enabled"
            } else {
                "governance_execution_error"
            }
        }
    };
    let executed = exec_result.is_ok();
    let applied_policy = engine.governance_token_economics_policy();
    let policy_applied = applied_policy == token_policy;
    println!(
        "governance_token_out: proposal_id={} executed={} reason_code={} policy_applied={} max_supply={} locked_supply={}",
        proposal.proposal_id,
        executed,
        reason_code,
        policy_applied,
        applied_policy.max_supply,
        applied_policy.locked_supply
    );
    if !executed || !policy_applied || reason_code != "ok" {
        bail!(
            "governance token probe failed at governance apply: executed={} policy_applied={} reason_code={}",
            executed,
            policy_applied,
            reason_code
        );
    }

    let account: ConsensusNodeId = 42;
    let mint_amount = std::cmp::max(100, token_policy.locked_supply / 5);
    let gas_fee = std::cmp::max(10, mint_amount / 5);
    let service_fee = std::cmp::max(5, mint_amount / 10);
    let burn_amount = std::cmp::max(1, mint_amount / 20);

    engine
        .mint_tokens(account, mint_amount)
        .context("governance token probe: mint failed")?;
    let mint_zero_reject = engine.mint_tokens(account, 0).is_err();
    let mint_locked_reject = engine
        .mint_tokens(account, token_policy.locked_supply)
        .is_err();

    let gas_out = engine
        .charge_gas_fee(account, gas_fee)
        .context("governance token probe: gas fee routing failed")?;
    let service_out = engine
        .charge_service_fee(account, service_fee)
        .context("governance token probe: service fee routing failed")?;
    let burn_overdraft_reject = engine.burn_tokens(account, mint_amount).is_err();
    engine
        .burn_tokens(account, burn_amount)
        .context("governance token probe: burn failed")?;

    let expected_total_supply = mint_amount
        .saturating_sub(gas_out.burn_amount)
        .saturating_sub(service_out.burn_amount)
        .saturating_sub(burn_amount);
    let expected_balance = mint_amount
        .saturating_sub(gas_fee)
        .saturating_sub(service_fee)
        .saturating_sub(burn_amount);
    let expected_treasury = gas_out
        .treasury_amount
        .saturating_add(service_out.treasury_amount);
    let expected_burned = gas_out
        .burn_amount
        .saturating_add(service_out.burn_amount)
        .saturating_add(burn_amount);

    let total_supply = engine.token_total_supply();
    let balance = engine.token_balance(account);
    let treasury = engine.token_treasury_balance();
    let burned = engine.token_burned_total();
    let gas_provider_pool = engine.token_gas_provider_fee_pool();
    let service_provider_pool = engine.token_service_provider_fee_pool();

    println!(
        "token_econ_out: account={} mint={} gas_fee={} service_fee={} burn={} total_supply={} balance={} treasury={} burned={} gas_provider_pool={} service_provider_pool={} mint_zero_reject={} mint_locked_reject={} burn_overdraft_reject={} expected_total_supply={} expected_balance={} expected_treasury={} expected_burned={}",
        account,
        mint_amount,
        gas_fee,
        service_fee,
        burn_amount,
        total_supply,
        balance,
        treasury,
        burned,
        gas_provider_pool,
        service_provider_pool,
        mint_zero_reject,
        mint_locked_reject,
        burn_overdraft_reject,
        expected_total_supply,
        expected_balance,
        expected_treasury,
        expected_burned
    );

    if !mint_zero_reject || !mint_locked_reject || !burn_overdraft_reject {
        bail!(
            "governance token probe negative checks failed: mint_zero_reject={} mint_locked_reject={} burn_overdraft_reject={}",
            mint_zero_reject,
            mint_locked_reject,
            burn_overdraft_reject
        );
    }
    if total_supply != expected_total_supply
        || balance != expected_balance
        || treasury != expected_treasury
        || burned != expected_burned
    {
        bail!(
            "governance token probe accounting mismatch: total_supply={}/{} balance={}/{} treasury={}/{} burned={}/{}",
            total_supply,
            expected_total_supply,
            balance,
            expected_balance,
            treasury,
            expected_treasury,
            burned,
            expected_burned
        );
    }

    Ok(())
}

fn run_governance_market_policy_probe_mode() -> Result<()> {
    let loaded = load_consensus_slash_policy()?;
    emit_slash_policy_in_signal(&loaded);
    let market_policy = load_governance_market_policy()?;

    let validator_set = ValidatorSet::new_equal_weight(vec![0, 1, 2]);
    let required_quorum = validator_set.quorum_size();
    let signing_keys: Vec<_> = (0..3).map(|_| SigningKey::generate(&mut OsRng)).collect();
    let mut public_keys = HashMap::new();
    public_keys.insert(0, signing_keys[0].verifying_key());
    public_keys.insert(1, signing_keys[1].verifying_key());
    public_keys.insert(2, signing_keys[2].verifying_key());
    let engine = BFTEngine::new(
        BFTConfig::default(),
        0,
        signing_keys[0].clone(),
        validator_set,
        public_keys,
    )
    .context("governance market policy probe: init novovm-consensus engine failed")?;
    engine
        .set_slash_policy(loaded.policy.clone())
        .context("governance market policy probe: set baseline slash policy failed")?;
    engine.set_governance_execution_enabled(true);

    let proposal = engine
        .submit_governance_proposal(
            0,
            GovernanceOp::UpdateMarketGovernancePolicy {
                policy: market_policy.clone(),
            },
        )
        .context("governance market policy probe: submit proposal failed")?;
    let votes = vec![
        GovernanceVote::new(&proposal, 0, true, &signing_keys[0]),
        GovernanceVote::new(&proposal, 1, true, &signing_keys[1]),
    ];
    println!(
        "governance_market_in: proposal_id={} op=update_market_governance_policy amm_swap_fee_bp={} cdp_min_collateral_ratio_bp={} bond_coupon_rate_bp={} reserve_min_reserve_ratio_bp={} nav_settlement_delay_epochs={} buyback_trigger_discount_bp={} votes={} quorum={}",
        proposal.proposal_id,
        market_policy.amm.swap_fee_bp,
        market_policy.cdp.min_collateral_ratio_bp,
        market_policy.bond.coupon_rate_bp,
        market_policy.reserve.min_reserve_ratio_bp,
        market_policy.nav.settlement_delay_epochs,
        market_policy.buyback.trigger_discount_bp,
        votes.len(),
        required_quorum
    );

    let exec_result = engine.execute_governance_proposal(proposal.proposal_id, &votes);
    let reason_code = match &exec_result {
        Ok(_) => "ok",
        Err(err) => {
            let msg = err.to_string().to_ascii_lowercase();
            if msg.contains("insufficient votes") {
                "insufficient_votes"
            } else if msg.contains("invalid signature") {
                "invalid_signature"
            } else if msg.contains("governance not enabled") {
                "governance_not_enabled"
            } else {
                "governance_execution_error"
            }
        }
    };
    let executed = exec_result.is_ok();
    let applied_policy = engine.governance_market_policy();
    let policy_applied = applied_policy == market_policy;
    let engine_snapshot = engine.governance_market_engine_snapshot();
    let engine_applied = engine_snapshot.amm_swap_fee_bp == market_policy.amm.swap_fee_bp
        && engine_snapshot.cdp_min_collateral_ratio_bp == market_policy.cdp.min_collateral_ratio_bp
        && engine_snapshot.cdp_liquidation_threshold_bp
            == market_policy.cdp.liquidation_threshold_bp
        && engine_snapshot.bond_one_year_coupon_bp == market_policy.bond.coupon_rate_bp
        && engine_snapshot.reserve_min_reserve_ratio_bp
            == market_policy.reserve.min_reserve_ratio_bp
        && engine_snapshot.nav_settlement_delay_epochs == market_policy.nav.settlement_delay_epochs
        && engine_snapshot.buyback_trigger_discount_bp == market_policy.buyback.trigger_discount_bp
        && engine_snapshot.treasury_main_balance > 0
        && engine_snapshot.treasury_risk_reserve_balance > 0
        && engine_snapshot.reserve_foreign_usdt_balance > 0
        && engine_snapshot.nav_soft_floor_value > 0
        && engine_snapshot.oracle_price_before > engine_snapshot.oracle_price_after
        && engine_snapshot.cdp_liquidation_candidates > 0
        && engine_snapshot.cdp_liquidations_executed > 0
        && engine_snapshot.cdp_liquidation_penalty_routed > 0
        && engine_snapshot.nav_snapshot_day > 0
        && engine_snapshot.nav_latest_value > 0
        && engine_snapshot.nav_redemptions_submitted > 0
        && engine_snapshot.nav_redemptions_executed > 0
        && engine_snapshot.nav_executed_stable_total > 0;
    println!(
        "governance_market_out: proposal_id={} executed={} reason_code={} policy_applied={} amm_swap_fee_bp={} cdp_min_collateral_ratio_bp={} bond_coupon_rate_bp={} reserve_min_reserve_ratio_bp={} nav_settlement_delay_epochs={} buyback_trigger_discount_bp={}",
        proposal.proposal_id,
        executed,
        reason_code,
        policy_applied,
        applied_policy.amm.swap_fee_bp,
        applied_policy.cdp.min_collateral_ratio_bp,
        applied_policy.bond.coupon_rate_bp,
        applied_policy.reserve.min_reserve_ratio_bp,
        applied_policy.nav.settlement_delay_epochs,
        applied_policy.buyback.trigger_discount_bp
    );
    println!(
        "governance_market_engine_out: proposal_id={} engine_applied={} cdp_liquidation_threshold_bp={} bond_one_year_coupon_bp={} nav_max_daily_redemption_bp={}",
        proposal.proposal_id,
        engine_applied,
        engine_snapshot.cdp_liquidation_threshold_bp,
        engine_snapshot.bond_one_year_coupon_bp,
        engine_snapshot.nav_max_daily_redemption_bp
    );
    println!(
        "governance_market_runtime_out: proposal_id={} runtime_applied={} cdp_liquidation_threshold_bp={} bond_one_year_coupon_bp={} nav_max_daily_redemption_bp={}",
        proposal.proposal_id,
        engine_applied,
        engine_snapshot.cdp_liquidation_threshold_bp,
        engine_snapshot.bond_one_year_coupon_bp,
        engine_snapshot.nav_max_daily_redemption_bp
    );
    println!(
        "governance_market_treasury_out: proposal_id={} treasury_main_balance={} treasury_risk_reserve_balance={} reserve_foreign_usdt_balance={} nav_soft_floor_value={} buyback_last_spent_stable={} buyback_last_burned_token={}",
        proposal.proposal_id,
        engine_snapshot.treasury_main_balance,
        engine_snapshot.treasury_risk_reserve_balance,
        engine_snapshot.reserve_foreign_usdt_balance,
        engine_snapshot.nav_soft_floor_value,
        engine_snapshot.buyback_last_spent_stable,
        engine_snapshot.buyback_last_burned_token
    );
    println!(
        "governance_market_orchestration_out: proposal_id={} oracle_price_before={} oracle_price_after={} cdp_liquidation_candidates={} cdp_liquidations_executed={} cdp_liquidation_penalty_routed={} nav_snapshot_day={} nav_latest_value={} nav_redemptions_submitted={} nav_redemptions_executed={} nav_executed_stable_total={}",
        proposal.proposal_id,
        engine_snapshot.oracle_price_before,
        engine_snapshot.oracle_price_after,
        engine_snapshot.cdp_liquidation_candidates,
        engine_snapshot.cdp_liquidations_executed,
        engine_snapshot.cdp_liquidation_penalty_routed,
        engine_snapshot.nav_snapshot_day,
        engine_snapshot.nav_latest_value,
        engine_snapshot.nav_redemptions_submitted,
        engine_snapshot.nav_redemptions_executed,
        engine_snapshot.nav_executed_stable_total
    );

    if !executed || !policy_applied || !engine_applied || reason_code != "ok" {
        bail!(
            "governance market policy probe failed: executed={} policy_applied={} engine_applied={} reason_code={}",
            executed,
            policy_applied,
            engine_applied,
            reason_code
        );
    }
    Ok(())
}

fn run_governance_access_policy_probe_mode() -> Result<()> {
    let loaded = load_consensus_slash_policy()?;
    emit_slash_policy_in_signal(&loaded);
    let access_policy = load_governance_access_policy()?;

    let validator_set = ValidatorSet::new_equal_weight(vec![0, 1, 2]);
    let required_quorum = validator_set.quorum_size();
    let signing_keys: Vec<_> = (0..3).map(|_| SigningKey::generate(&mut OsRng)).collect();
    let mut public_keys = HashMap::new();
    public_keys.insert(0, signing_keys[0].verifying_key());
    public_keys.insert(1, signing_keys[1].verifying_key());
    public_keys.insert(2, signing_keys[2].verifying_key());
    let engine = BFTEngine::new(
        BFTConfig::default(),
        0,
        signing_keys[0].clone(),
        validator_set,
        public_keys,
    )
    .context("governance access probe: init novovm-consensus engine failed")?;
    engine
        .set_slash_policy(loaded.policy.clone())
        .context("governance access probe: set baseline slash policy failed")?;
    engine.set_governance_execution_enabled(true);

    // 1) Apply governance access policy via governance op.
    let policy_proposal = engine
        .submit_governance_proposal(
            0,
            GovernanceOp::UpdateGovernanceAccessPolicy {
                policy: access_policy.clone(),
            },
        )
        .context("governance access probe: submit access policy proposal failed")?;
    let policy_votes = vec![
        GovernanceVote::new(&policy_proposal, 0, true, &signing_keys[0]),
        GovernanceVote::new(&policy_proposal, 1, true, &signing_keys[1]),
    ];
    println!(
        "governance_access_in: proposal_id={} op=update_governance_access_policy proposer_threshold={} executor_threshold={} timelock_epochs={} votes={} quorum={}",
        policy_proposal.proposal_id,
        access_policy.proposer_threshold,
        access_policy.executor_threshold,
        access_policy.timelock_epochs,
        policy_votes.len(),
        required_quorum
    );
    engine
        .execute_governance_proposal(policy_proposal.proposal_id, &policy_votes)
        .context("governance access probe: execute access policy proposal failed")?;
    let policy_applied = engine.governance_access_policy() == access_policy;

    // 2) Proposer multisig reject and pass.
    let submit_reject = engine
        .submit_governance_proposal_with_approvals(
            0,
            &[0],
            GovernanceOp::UpdateMempoolFeeFloor { fee_floor: 17 },
        )
        .is_err();
    let proposal = engine
        .submit_governance_proposal_with_approvals(
            0,
            &[0, 1],
            GovernanceOp::UpdateMempoolFeeFloor { fee_floor: 17 },
        )
        .context("governance access probe: submit protected proposal failed")?;
    let votes = vec![
        GovernanceVote::new(&proposal, 0, true, &signing_keys[0]),
        GovernanceVote::new(&proposal, 1, true, &signing_keys[1]),
    ];

    // 3) Timelock must reject first execute.
    let timelock_reject = engine
        .execute_governance_proposal_with_executor_approvals(proposal.proposal_id, &votes, &[1, 2])
        .is_err();

    // 4) Bypass timelock for probe to verify executor multisig behavior.
    let no_timelock_policy = GovernanceAccessPolicy {
        timelock_epochs: 0,
        ..access_policy.clone()
    };
    engine
        .set_governance_access_policy(no_timelock_policy)
        .context("governance access probe: set no-timelock policy failed")?;
    let executor_threshold_reject = engine
        .execute_governance_proposal_with_executor_approvals(proposal.proposal_id, &votes, &[1])
        .is_err();
    let execute_ok = engine
        .execute_governance_proposal_with_executor_approvals(proposal.proposal_id, &votes, &[1, 2])
        .is_ok();
    let mempool_fee_floor = engine.governance_mempool_fee_floor();

    println!(
        "governance_access_out: proposal_id={} policy_applied={} submit_reject={} timelock_reject={} executor_threshold_reject={} execute_ok={} mempool_fee_floor={}",
        proposal.proposal_id,
        policy_applied,
        submit_reject,
        timelock_reject,
        executor_threshold_reject,
        execute_ok,
        mempool_fee_floor
    );

    if !policy_applied
        || !submit_reject
        || !timelock_reject
        || !executor_threshold_reject
        || !execute_ok
        || mempool_fee_floor != 17
    {
        bail!(
            "governance access probe failed: policy_applied={} submit_reject={} timelock_reject={} executor_threshold_reject={} execute_ok={} mempool_fee_floor={}",
            policy_applied,
            submit_reject,
            timelock_reject,
            executor_threshold_reject,
            execute_ok,
            mempool_fee_floor
        );
    }

    Ok(())
}

fn run_governance_council_policy_probe_mode() -> Result<()> {
    let loaded = load_consensus_slash_policy()?;
    emit_slash_policy_in_signal(&loaded);

    let validator_ids: Vec<ConsensusNodeId> = (0..9).collect();
    let validator_set = ValidatorSet::new_equal_weight(validator_ids.clone());
    let required_quorum = validator_set.quorum_size();
    let signing_keys: Vec<_> = (0..validator_ids.len())
        .map(|_| SigningKey::generate(&mut OsRng))
        .collect();
    let mut public_keys = HashMap::new();
    for (idx, node_id) in validator_ids.iter().enumerate() {
        public_keys.insert(*node_id, signing_keys[idx].verifying_key());
    }
    let engine = BFTEngine::new(
        BFTConfig::default(),
        0,
        signing_keys[0].clone(),
        validator_set,
        public_keys,
    )
    .context("governance council probe: init novovm-consensus engine failed")?;
    engine
        .set_slash_policy(loaded.policy.clone())
        .context("governance council probe: set baseline slash policy failed")?;
    engine.set_governance_execution_enabled(true);
    engine
        .set_governance_access_policy(GovernanceAccessPolicy {
            proposer_committee: validator_ids.clone(),
            proposer_threshold: 1,
            executor_committee: validator_ids,
            executor_threshold: 1,
            timelock_epochs: 0,
        })
        .context("governance council probe: set permissive governance access policy failed")?;

    let council_policy = GovernanceCouncilPolicy {
        enabled: true,
        members: vec![
            GovernanceCouncilMember {
                seat: GovernanceCouncilSeat::Founder,
                node_id: 0,
            },
            GovernanceCouncilMember {
                seat: GovernanceCouncilSeat::TopHolder(0),
                node_id: 1,
            },
            GovernanceCouncilMember {
                seat: GovernanceCouncilSeat::TopHolder(1),
                node_id: 2,
            },
            GovernanceCouncilMember {
                seat: GovernanceCouncilSeat::TopHolder(2),
                node_id: 3,
            },
            GovernanceCouncilMember {
                seat: GovernanceCouncilSeat::TopHolder(3),
                node_id: 4,
            },
            GovernanceCouncilMember {
                seat: GovernanceCouncilSeat::TopHolder(4),
                node_id: 5,
            },
            GovernanceCouncilMember {
                seat: GovernanceCouncilSeat::Team(0),
                node_id: 6,
            },
            GovernanceCouncilMember {
                seat: GovernanceCouncilSeat::Team(1),
                node_id: 7,
            },
            GovernanceCouncilMember {
                seat: GovernanceCouncilSeat::Independent,
                node_id: 8,
            },
        ],
        parameter_change_threshold_bp: 5000,
        treasury_spend_threshold_bp: 6600,
        protocol_upgrade_threshold_bp: 7500,
        emergency_freeze_threshold_bp: 5000,
        emergency_min_categories: 3,
    };

    // 1) Enable council policy via governance op (still evaluated by validator quorum before policy is active).
    let policy_proposal = engine
        .submit_governance_proposal(
            0,
            GovernanceOp::UpdateGovernanceCouncilPolicy {
                policy: council_policy.clone(),
            },
        )
        .context("governance council probe: submit council policy proposal failed")?;
    let policy_votes = vec![
        GovernanceVote::new(&policy_proposal, 0, true, &signing_keys[0]),
        GovernanceVote::new(&policy_proposal, 1, true, &signing_keys[1]),
        GovernanceVote::new(&policy_proposal, 2, true, &signing_keys[2]),
        GovernanceVote::new(&policy_proposal, 3, true, &signing_keys[3]),
        GovernanceVote::new(&policy_proposal, 4, true, &signing_keys[4]),
        GovernanceVote::new(&policy_proposal, 5, true, &signing_keys[5]),
    ];
    println!(
        "governance_council_in: proposal_id={} op=update_governance_council_policy members={} parameter_threshold_bp={} protocol_upgrade_threshold_bp={} apply_votes={} quorum={}",
        policy_proposal.proposal_id,
        council_policy.members.len(),
        council_policy.parameter_change_threshold_bp,
        council_policy.protocol_upgrade_threshold_bp,
        policy_votes.len(),
        required_quorum
    );
    engine
        .execute_governance_proposal(policy_proposal.proposal_id, &policy_votes)
        .context("governance council probe: execute council policy proposal failed")?;
    let policy_applied = engine.governance_council_policy() == council_policy;

    // 2) ParameterChange threshold (>5000): fail at 4500, pass at 5500.
    let parameter_proposal = engine
        .submit_governance_proposal(0, GovernanceOp::UpdateMempoolFeeFloor { fee_floor: 19 })
        .context("governance council probe: submit parameter proposal failed")?;
    let parameter_low_votes = vec![
        GovernanceVote::new(&parameter_proposal, 0, true, &signing_keys[0]), // founder 3500
        GovernanceVote::new(&parameter_proposal, 1, true, &signing_keys[1]), // +1000 => 4500
    ];
    let parameter_reject = engine
        .execute_governance_proposal(parameter_proposal.proposal_id, &parameter_low_votes)
        .is_err();
    let parameter_ok_votes = vec![
        GovernanceVote::new(&parameter_proposal, 0, true, &signing_keys[0]), // 3500
        GovernanceVote::new(&parameter_proposal, 1, true, &signing_keys[1]), // +1000
        GovernanceVote::new(&parameter_proposal, 2, true, &signing_keys[2]), // +1000 => 5500
    ];
    let parameter_execute_ok = engine
        .execute_governance_proposal(parameter_proposal.proposal_id, &parameter_ok_votes)
        .is_ok();
    let mempool_fee_floor = engine.governance_mempool_fee_floor();

    // 3) ProtocolUpgrade threshold (>7500): fail at 6500, pass at 8500.
    let target_access = GovernanceAccessPolicy {
        proposer_committee: vec![0, 1],
        proposer_threshold: 2,
        executor_committee: vec![0, 1, 2],
        executor_threshold: 2,
        timelock_epochs: 1,
    };
    let protocol_upgrade_proposal = engine
        .submit_governance_proposal(
            0,
            GovernanceOp::UpdateGovernanceAccessPolicy {
                policy: target_access.clone(),
            },
        )
        .context("governance council probe: submit protocol-upgrade proposal failed")?;
    let protocol_low_votes = vec![
        GovernanceVote::new(&protocol_upgrade_proposal, 0, true, &signing_keys[0]), // 3500
        GovernanceVote::new(&protocol_upgrade_proposal, 1, true, &signing_keys[1]), // +1000
        GovernanceVote::new(&protocol_upgrade_proposal, 2, true, &signing_keys[2]), // +1000
        GovernanceVote::new(&protocol_upgrade_proposal, 3, true, &signing_keys[3]), // +1000 => 6500
    ];
    let protocol_reject = engine
        .execute_governance_proposal(protocol_upgrade_proposal.proposal_id, &protocol_low_votes)
        .is_err();
    let protocol_ok_votes = vec![
        GovernanceVote::new(&protocol_upgrade_proposal, 0, true, &signing_keys[0]), // 3500
        GovernanceVote::new(&protocol_upgrade_proposal, 1, true, &signing_keys[1]), // +1000
        GovernanceVote::new(&protocol_upgrade_proposal, 2, true, &signing_keys[2]), // +1000
        GovernanceVote::new(&protocol_upgrade_proposal, 3, true, &signing_keys[3]), // +1000
        GovernanceVote::new(&protocol_upgrade_proposal, 4, true, &signing_keys[4]), // +1000
        GovernanceVote::new(&protocol_upgrade_proposal, 5, true, &signing_keys[5]), // +1000 => 8500
    ];
    let protocol_execute_ok = engine
        .execute_governance_proposal(protocol_upgrade_proposal.proposal_id, &protocol_ok_votes)
        .is_ok();
    let proposer_threshold = engine.governance_access_policy().proposer_threshold;

    println!(
        "governance_council_out: policy_applied={} parameter_reject={} parameter_execute_ok={} protocol_reject={} protocol_execute_ok={} mempool_fee_floor={} proposer_threshold={}",
        policy_applied,
        parameter_reject,
        parameter_execute_ok,
        protocol_reject,
        protocol_execute_ok,
        mempool_fee_floor,
        proposer_threshold
    );

    if !policy_applied
        || !parameter_reject
        || !parameter_execute_ok
        || !protocol_reject
        || !protocol_execute_ok
        || mempool_fee_floor != 19
        || proposer_threshold != 2
    {
        bail!(
            "governance council probe failed: policy_applied={} parameter_reject={} parameter_execute_ok={} protocol_reject={} protocol_execute_ok={} mempool_fee_floor={} proposer_threshold={}",
            policy_applied,
            parameter_reject,
            parameter_execute_ok,
            protocol_reject,
            protocol_execute_ok,
            mempool_fee_floor,
            proposer_threshold
        );
    }

    Ok(())
}

fn run_governance_treasury_spend_probe_mode() -> Result<()> {
    let loaded = load_consensus_slash_policy()?;
    emit_slash_policy_in_signal(&loaded);
    let token_policy = load_governance_token_economics_policy()?;
    let (treasury_to, treasury_amount_requested, treasury_reason) =
        load_governance_treasury_spend()?;

    let validator_set = ValidatorSet::new_equal_weight(vec![0, 1, 2]);
    let required_quorum = validator_set.quorum_size();
    let signing_keys: Vec<_> = (0..3).map(|_| SigningKey::generate(&mut OsRng)).collect();
    let mut public_keys = HashMap::new();
    public_keys.insert(0, signing_keys[0].verifying_key());
    public_keys.insert(1, signing_keys[1].verifying_key());
    public_keys.insert(2, signing_keys[2].verifying_key());
    let engine = BFTEngine::new(
        BFTConfig::default(),
        0,
        signing_keys[0].clone(),
        validator_set,
        public_keys,
    )
    .context("governance treasury probe: init novovm-consensus engine failed")?;
    engine
        .set_slash_policy(loaded.policy.clone())
        .context("governance treasury probe: set baseline slash policy failed")?;
    engine
        .set_token_economics_policy(token_policy)
        .context("governance treasury probe: set token economics policy failed")?;
    engine.set_governance_execution_enabled(true);

    // Build treasury balance first: mint -> gas fee -> service fee.
    let payer: ConsensusNodeId = 42;
    let mint_amount = 500u64;
    let gas_fee = 100u64;
    let service_fee = 100u64;
    engine
        .mint_tokens(payer, mint_amount)
        .context("governance treasury probe: mint failed")?;
    engine
        .charge_gas_fee(payer, gas_fee)
        .context("governance treasury probe: gas fee routing failed")?;
    engine
        .charge_service_fee(payer, service_fee)
        .context("governance treasury probe: service fee routing failed")?;

    let treasury_before = engine.token_treasury_balance();
    if treasury_before == 0 {
        bail!("governance treasury probe failed: treasury_before is zero");
    }
    let treasury_amount = treasury_amount_requested.max(1).min(treasury_before);
    let recipient_before = engine.token_balance(treasury_to);

    let proposal = engine
        .submit_governance_proposal(
            0,
            GovernanceOp::TreasurySpend {
                to: treasury_to,
                amount: treasury_amount,
                reason: treasury_reason.clone(),
            },
        )
        .context("governance treasury probe: submit proposal failed")?;
    let votes = vec![
        GovernanceVote::new(&proposal, 0, true, &signing_keys[0]),
        GovernanceVote::new(&proposal, 1, true, &signing_keys[1]),
    ];
    println!(
        "governance_treasury_in: proposal_id={} op=treasury_spend to={} amount={} reason={} votes={} quorum={} treasury_before={} recipient_before={}",
        proposal.proposal_id,
        treasury_to,
        treasury_amount,
        treasury_reason,
        votes.len(),
        required_quorum,
        treasury_before,
        recipient_before
    );

    let exec_result = engine.execute_governance_proposal(proposal.proposal_id, &votes);
    let reason_code = match &exec_result {
        Ok(_) => "ok",
        Err(err) => {
            let msg = err.to_string().to_ascii_lowercase();
            if msg.contains("insufficient votes") {
                "insufficient_votes"
            } else if msg.contains("invalid signature") {
                "invalid_signature"
            } else if msg.contains("governance not enabled") {
                "governance_not_enabled"
            } else {
                "governance_execution_error"
            }
        }
    };
    let executed = exec_result.is_ok();
    let treasury_after = engine.token_treasury_balance();
    let recipient_after = engine.token_balance(treasury_to);
    let spent_total = engine.token_treasury_spent_total();
    let spend_applied = treasury_after
        .saturating_add(treasury_amount)
        .eq(&treasury_before)
        && recipient_after.eq(&recipient_before.saturating_add(treasury_amount));

    // Negative: overspend must be rejected.
    let overspend_amount = treasury_after.saturating_add(1);
    let overspend_reject = if overspend_amount == 0 {
        true
    } else {
        let overspend = engine
            .submit_governance_proposal(
                0,
                GovernanceOp::TreasurySpend {
                    to: treasury_to,
                    amount: overspend_amount,
                    reason: "overspend_reject".to_string(),
                },
            )
            .context("governance treasury probe: submit overspend proposal failed")?;
        let overspend_votes = vec![
            GovernanceVote::new(&overspend, 0, true, &signing_keys[0]),
            GovernanceVote::new(&overspend, 1, true, &signing_keys[1]),
        ];
        engine
            .execute_governance_proposal(overspend.proposal_id, &overspend_votes)
            .is_err()
    };

    println!(
        "governance_treasury_out: proposal_id={} executed={} reason_code={} spend_applied={} treasury_before={} treasury_after={} recipient_before={} recipient_after={} spent_total={} overspend_reject={}",
        proposal.proposal_id,
        executed,
        reason_code,
        spend_applied,
        treasury_before,
        treasury_after,
        recipient_before,
        recipient_after,
        spent_total,
        overspend_reject
    );

    if !executed || reason_code != "ok" || !spend_applied || !overspend_reject {
        bail!(
            "governance treasury probe failed: executed={} reason_code={} spend_applied={} overspend_reject={} treasury_before={} treasury_after={} recipient_before={} recipient_after={} spent_total={}",
            executed,
            reason_code,
            spend_applied,
            overspend_reject,
            treasury_before,
            treasury_after,
            recipient_before,
            recipient_after,
            spent_total
        );
    }

    Ok(())
}

fn run_ffi_v2() -> Result<()> {
    let slash_policy_loaded = load_consensus_slash_policy()?;
    emit_slash_policy_in_signal(&slash_policy_loaded);

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
        mempool.accepted, mempool.rejected, mempool.fee_floor, mempool.nonce_ok, mempool.sig_ok
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
        &local_batches,
        consensus_binding,
        adapter_signal.state_root,
        &slash_policy_loaded.policy,
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
                signal.nodes, signal.discovery, signal.gossip, signal.sync
            );
            println!(
                "network_pacemaker: view_sync={} new_view={}",
                signal.view_sync, signal.new_view
            );
            if strict_network
                && (!signal.discovery
                    || !signal.gossip
                    || !signal.sync
                    || !signal.view_sync
                    || !signal.new_view)
            {
                bail!(
                    "network closure incomplete: discovery={} gossip={} sync={} view_sync={} new_view={}",
                    signal.discovery,
                    signal.gossip,
                    signal.sync,
                    signal.view_sync,
                    signal.new_view
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
    if node_mode.eq_ignore_ascii_case("header_sync_probe") {
        return run_header_sync_probe_mode();
    }
    if node_mode.eq_ignore_ascii_case("fast_state_sync_probe") {
        return run_fast_state_sync_probe_mode();
    }
    if node_mode.eq_ignore_ascii_case("network_dos_probe") {
        return run_network_dos_probe_mode();
    }
    if node_mode.eq_ignore_ascii_case("pacemaker_failover_probe") {
        return run_pacemaker_failover_probe_mode();
    }
    if node_mode.eq_ignore_ascii_case("slash_policy_probe") {
        return run_slash_policy_probe_mode();
    }
    if node_mode.eq_ignore_ascii_case("governance_hook_probe") {
        return run_governance_hook_probe_mode();
    }
    if node_mode.eq_ignore_ascii_case("governance_execute_probe") {
        return run_governance_execute_probe_mode();
    }
    if node_mode.eq_ignore_ascii_case("governance_negative_probe") {
        return run_governance_negative_probe_mode();
    }
    if node_mode.eq_ignore_ascii_case("governance_param2_probe") {
        return run_governance_param2_probe_mode();
    }
    if node_mode.eq_ignore_ascii_case("governance_param3_probe") {
        return run_governance_param3_probe_mode();
    }
    if node_mode.eq_ignore_ascii_case("governance_token_economics_probe") {
        return run_governance_token_economics_probe_mode();
    }
    if node_mode.eq_ignore_ascii_case("governance_market_policy_probe") {
        return run_governance_market_policy_probe_mode();
    }
    if node_mode.eq_ignore_ascii_case("governance_access_policy_probe") {
        return run_governance_access_policy_probe_mode();
    }
    if node_mode.eq_ignore_ascii_case("governance_council_policy_probe") {
        return run_governance_council_policy_probe_mode();
    }
    if node_mode.eq_ignore_ascii_case("governance_treasury_spend_probe") {
        return run_governance_treasury_spend_probe_mode();
    }
    if node_mode.eq_ignore_ascii_case("chain_query") {
        return run_chain_query_mode();
    }
    if node_mode.eq_ignore_ascii_case("rpc_server") {
        return run_chain_query_rpc_server_mode();
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
    use novovm_exec::{AoemExecMetrics, AoemExecOutput, AoemExecReturnCode};
    use std::collections::HashMap;

    fn test_tx(account: u64, key: u64, value: u64, nonce: u64, fee: u64) -> LocalTx {
        build_local_tx(account, key, value, nonce, fee)
    }

    fn build_test_governance_engine() -> BFTEngine {
        let validator_ids: Vec<ConsensusNodeId> = vec![0, 1, 2];
        let validator_set = ValidatorSet::new_equal_weight(validator_ids.clone());
        let signing_keys: Vec<_> = (0..validator_ids.len())
            .map(|_| SigningKey::generate(&mut OsRng))
            .collect();
        let mut public_keys: HashMap<ConsensusNodeId, ed25519_dalek::VerifyingKey> = HashMap::new();
        for (idx, node_id) in validator_ids.iter().enumerate() {
            public_keys.insert(*node_id, signing_keys[idx].verifying_key());
        }
        BFTEngine::new(
            BFTConfig::default(),
            0,
            signing_keys[0].clone(),
            validator_set,
            public_keys,
        )
        .expect("build test governance engine")
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
            governance_chain_audit_root: [6u8; 32],
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
    fn adapter_plugin_chain_code_mapping_is_stable() {
        assert_eq!(chain_type_to_plugin_code(ChainType::NovoVM), Some(0));
        assert_eq!(chain_type_to_plugin_code(ChainType::EVM), Some(1));
        assert_eq!(chain_type_to_plugin_code(ChainType::BNB), Some(6));
        assert_eq!(chain_type_to_plugin_code(ChainType::Custom), Some(13));
        assert_eq!(chain_type_to_plugin_code(ChainType::Solana), None);
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
            governance_chain_audit_root: [5u8; 32],
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
            governance_chain_audit_root: [6u8; 32],
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
            governance_chain_audit_root: [5u8; 32],
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
            governance_chain_audit_root: [6u8; 32],
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
    fn local_tx_hash_is_deterministic() {
        let tx = test_tx(1000, 7, 11, 2, 3);
        let a = local_tx_hash(&tx);
        let b = local_tx_hash(&tx);
        assert_eq!(a, b);
    }

    #[test]
    fn query_state_db_tracks_block_tx_receipt_balance() {
        let txs = vec![test_tx(1000, 1, 10, 0, 2), test_tx(1001, 2, 20, 0, 3)];
        let closure = BatchAClosureOutput {
            epoch_id: 3,
            height: 5,
            txs: 2,
            state_root: [4u8; 32],
            governance_chain_audit_root: [7u8; 32],
            proposal_hash: [5u8; 32],
            consensus_binding: ConsensusPluginBindingV1 {
                plugin_class_code: PLUGIN_CLASS_CONSENSUS,
                adapter_hash: [9u8; 32],
            },
        };
        let batches = build_local_batches_from_txs(&txs, 1);
        let block = build_local_block(&closure, &batches);

        let mut db = QueryStateDb::default();
        apply_block_to_query_db(&mut db, &block);

        assert_eq!(db.blocks.len(), 1);
        assert_eq!(db.txs.len(), 2);
        assert_eq!(db.receipts.len(), 2);
        assert_eq!(db.balances.get("1000"), Some(&10));
        assert_eq!(db.balances.get("1001"), Some(&20));
    }

    #[test]
    fn chain_query_methods_return_expected_records() {
        let txs = vec![test_tx(1000, 1, 10, 0, 2), test_tx(1001, 2, 20, 0, 3)];
        let closure = BatchAClosureOutput {
            epoch_id: 3,
            height: 5,
            txs: 2,
            state_root: [4u8; 32],
            governance_chain_audit_root: [7u8; 32],
            proposal_hash: [5u8; 32],
            consensus_binding: ConsensusPluginBindingV1 {
                plugin_class_code: PLUGIN_CLASS_CONSENSUS,
                adapter_hash: [9u8; 32],
            },
        };
        let batches = build_local_batches_from_txs(&txs, 1);
        let block = build_local_block(&closure, &batches);
        let mut db = QueryStateDb::default();
        apply_block_to_query_db(&mut db, &block);

        let block_resp = run_chain_query(&db, "getBlock", &serde_json::json!({"height": 5}))
            .expect("getBlock should succeed");
        assert_eq!(block_resp["found"].as_bool(), Some(true));
        assert_eq!(block_resp["block"]["height"].as_u64(), Some(5));

        let tx_hash = db
            .txs
            .keys()
            .next()
            .expect("tx hash should exist")
            .to_string();
        let tx_resp = run_chain_query(
            &db,
            "getTransaction",
            &serde_json::json!({"tx_hash": tx_hash}),
        )
        .expect("getTransaction should succeed");
        assert_eq!(tx_resp["found"].as_bool(), Some(true));

        let receipt_hash = db
            .receipts
            .keys()
            .next()
            .expect("receipt hash should exist")
            .to_string();
        let receipt_resp = run_chain_query(
            &db,
            "getReceipt",
            &serde_json::json!({"tx_hash": receipt_hash}),
        )
        .expect("getReceipt should succeed");
        assert_eq!(receipt_resp["found"].as_bool(), Some(true));

        let balance_resp =
            run_chain_query(&db, "getBalance", &serde_json::json!({"account": "1001"}))
                .expect("getBalance should succeed");
        assert_eq!(balance_resp["found"].as_bool(), Some(true));
        assert_eq!(balance_resp["balance"].as_u64(), Some(20));
    }

    #[test]
    fn network_smoke_closes_two_node_flow() {
        let signal = run_network_smoke(3).expect("network smoke should succeed");
        assert_eq!(signal.transport, "in_memory");
        assert_eq!(signal.nodes, 2);
        assert_eq!(signal.sent, 10);
        assert_eq!(signal.received, 10);
        assert_eq!(signal.msg_kind, "two_node_discovery_gossip_sync_pacemaker");
        assert!(signal.discovery);
        assert!(signal.gossip);
        assert!(signal.sync);
        assert!(signal.view_sync);
        assert!(signal.new_view);
    }

    #[test]
    fn header_sync_probe_completes_headers_first() {
        let signal = run_header_sync_probe(8, 3, 64, 0).expect("header sync probe should succeed");
        assert!(signal.pass);
        assert!(signal.complete);
        assert_eq!(signal.local_tip_before, 2);
        assert_eq!(signal.local_tip_after, 7);
        assert_eq!(signal.fetched, 5);
        assert_eq!(signal.applied, 5);
        assert_eq!(signal.reason, "ok");
    }

    #[test]
    fn header_sync_probe_detects_tampered_parent_hash() {
        let signal = run_header_sync_probe(8, 3, 64, 5).expect("header sync probe should run");
        assert!(!signal.pass);
        assert!(!signal.complete);
        assert!(signal.reason.starts_with("parent_hash_mismatch_at_"));
        assert_eq!(signal.local_tip_before, 2);
        assert_eq!(signal.local_tip_after, 4);
        assert_eq!(signal.applied, 2);
        assert_eq!(signal.fetched, 3);
    }

    #[test]
    fn fast_state_sync_probe_completes_and_verifies_snapshot() {
        let signal =
            run_fast_state_sync_probe(16, 3, 128, 0).expect("fast/state sync probe should succeed");
        assert!(signal.pass);
        assert!(signal.fast_complete);
        assert!(signal.snapshot_verified);
        assert!(signal.state_complete);
        assert_eq!(signal.local_tip_before, 2);
        assert_eq!(signal.local_tip_after, 15);
        assert_eq!(signal.snapshot_height, 15);
        assert_eq!(signal.reason, "ok");
    }

    #[test]
    fn fast_state_sync_probe_detects_tampered_snapshot() {
        let signal =
            run_fast_state_sync_probe(16, 3, 128, 15).expect("fast/state sync probe should run");
        assert!(!signal.pass);
        assert!(signal.fast_complete);
        assert!(!signal.snapshot_verified);
        assert!(!signal.state_complete);
        assert!(signal.reason.starts_with("snapshot_root_mismatch_at_"));
    }

    #[test]
    fn network_dos_probe_bans_invalid_peers_and_keeps_healthy_peer() {
        let signal = run_network_dos_probe(2, 6, 3).expect("network dos probe should run");
        assert!(signal.pass);
        assert_eq!(signal.invalid_peers, 2);
        assert_eq!(signal.bans, 2);
        assert!(signal.invalid_detected >= 6);
        assert!(signal.storm_rejected >= 2);
        assert!(signal.healthy_accepts >= 1);
        assert_eq!(signal.reason, "ok");
    }

    #[test]
    fn pacemaker_failover_probe_rotates_and_commits_after_timeout() {
        let signal =
            run_pacemaker_failover_probe(4, 0).expect("pacemaker failover probe should run");
        assert_eq!(signal.mode, "timeout_view_sync_new_view_failover");
        assert_eq!(signal.nodes, 4);
        assert_eq!(signal.failed_leader, 0);
        assert_eq!(signal.initial_view, 0);
        assert_eq!(signal.next_view, 1);
        assert_eq!(signal.next_leader, 1);
        assert!(signal.timeout_cert);
        assert!(signal.local_view_advanced >= signal.timeout_quorum);
        assert!(signal.view_sync_votes >= signal.timeout_quorum);
        assert!(signal.new_view_votes >= signal.timeout_quorum);
        assert!(signal.qc_formed);
        assert!(signal.committed);
        assert!(signal.committed_height >= 1);
        assert!(signal.pass);
        assert_eq!(signal.reason, "ok");
    }

    #[test]
    fn slash_mode_parser_accepts_supported_values() {
        assert_eq!(
            parse_slash_mode("enforce").expect("parse enforce").as_str(),
            "enforce"
        );
        assert_eq!(
            parse_slash_mode("observe_only")
                .expect("parse observe_only")
                .as_str(),
            "observe_only"
        );
        assert!(parse_slash_mode("bad_mode").is_err());
    }

    #[test]
    fn parse_consensus_policy_file_accepts_utf8_bom() {
        let mut path = std::env::temp_dir();
        let nonce = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .expect("clock should be >= epoch")
            .as_nanos();
        path.push(format!("novovm-consensus-policy-bom-{}.json", nonce));

        let mut bytes = vec![0xEF, 0xBB, 0xBF];
        bytes.extend_from_slice(
            br#"{"mode":"enforce","equivocation_threshold":2,"min_active_validators":1,"cooldown_epochs":9}"#,
        );
        fs::write(&path, bytes).expect("write policy file");

        let loaded = parse_consensus_policy_file(&path).expect("parse policy with bom");
        assert_eq!(loaded.policy.mode.as_str(), "enforce");
        assert_eq!(loaded.policy.equivocation_threshold, 2);
        assert_eq!(loaded.policy.min_active_validators, 1);
        assert_eq!(loaded.cooldown_epochs, 9);

        let _ = fs::remove_file(&path);
    }

    #[test]
    fn governance_vote_verifier_config_parser_accepts_supported_values() {
        assert_eq!(
            parse_governance_vote_verifier_config(None).expect("default verifier"),
            GovernanceVoteVerifierScheme::Ed25519
        );
        assert_eq!(
            parse_governance_vote_verifier_config(Some("ed25519")).expect("ed25519"),
            GovernanceVoteVerifierScheme::Ed25519
        );
        assert_eq!(
            parse_governance_vote_verifier_config(Some("ml-dsa-87")).expect("ml-dsa alias"),
            GovernanceVoteVerifierScheme::MlDsa87
        );
        assert!(parse_governance_vote_verifier_config(Some("bad-scheme")).is_err());
    }

    #[test]
    fn apply_governance_vote_verifier_rejects_mldsa87_staged_only() {
        std::env::remove_var("NOVOVM_GOVERNANCE_MLDSA_MODE");
        std::env::remove_var("NOVOVM_GOVERNANCE_MLDSA87_PUBKEYS");
        std::env::remove_var("NOVOVM_AOEM_FFI_LIB_PATH");
        let engine = build_test_governance_engine();
        assert_eq!(engine.governance_vote_verifier_name(), "ed25519");
        apply_governance_vote_verifier(&engine, GovernanceVoteVerifierScheme::Ed25519)
            .expect("ed25519 should apply");
        let err = apply_governance_vote_verifier(&engine, GovernanceVoteVerifierScheme::MlDsa87)
            .unwrap_err()
            .to_string();
        assert!(err.contains("staged-only"));
        assert_eq!(engine.governance_vote_verifier_name(), "ed25519");
    }

    #[test]
    fn mldsa87_signature_envelope_roundtrip() {
        let pubkey = vec![0x11; 12];
        let signature = vec![0x22; 20];
        let encoded = encode_mldsa87_vote_signature_envelope(&pubkey, &signature)
            .expect("encode mldsa envelope");
        let (decoded_pubkey, decoded_signature) =
            decode_mldsa87_vote_signature_envelope(&encoded).expect("decode mldsa envelope");
        assert_eq!(decoded_pubkey, pubkey.as_slice());
        assert_eq!(decoded_signature, signature.as_slice());
    }
}
