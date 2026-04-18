// Copyright (c) 2026 Xonovo Technology
// All rights reserved.
// Author: Xonovo Technology

use anyhow::{bail, Context, Result};
mod bincode_compat;
use ed25519_dalek::{Signature as Ed25519Signature, SigningKey, Verifier, VerifyingKey};
use novovm_adapter_api::{
    default_chain_id, AccountAuditEvent, AccountPolicy, AccountRole, BlockIR, ChainConfig,
    ChainType, NonceScope, PersonaAddress, PersonaType, ProtocolKind, RouteDecision, RouteRequest,
    SerializationFormat, StateIR, TxIR, TxType, Type4PolicyMode, KycPolicyMode, UnifiedAccountError,
    UnifiedAccountRouter,
};
use novovm_adapter_evm_core::{
    translate_raw_evm_tx_fields_m0, tx_ir_from_raw_fields_m0, EvmRawTxFieldsM0,
};
use novovm_adapter_novovm::{create_native_adapter, supports_native_chain};
use novovm_consensus::{
    AmmGovernanceParams, BFTConfig, BFTEngine, BFTError as ConsensusBftError, BondGovernanceParams,
    BuybackGovernanceParams, CdpGovernanceParams, Epoch as ConsensusEpoch, GovernanceAccessPolicy,
    GovernanceChainAuditEvent, GovernanceCouncilMember, GovernanceCouncilPolicy,
    GovernanceCouncilSeat, GovernanceOp, GovernanceProposal, GovernanceVote,
    GovernanceVoteVerificationInput, GovernanceVoteVerificationReport, GovernanceVoteVerifier,
    GovernanceVoteVerifierScheme, HotStuffProtocol, MarketGovernancePolicy, NavGovernanceParams,
    NetworkDosPolicy, NodeId as ConsensusNodeId, ReserveGovernanceParams, SlashMode, SlashPolicy,
    TokenEconomicsPolicy, ValidatorSet, Web30MarketEngineSnapshot,
};
use novovm_exec::{
    AoemExecFacade, AoemRuntimeConfig, AoemRuntimeVariant, ExecOpV2,
    SupervmEvmExecutionLogV1, SupervmEvmExecutionReceiptV1, SupervmEvmStateMirrorUpdateV1,
};
use novovm_network::{
    eth_rlpx_transaction_hash_v1,
    InMemoryTransport, Transport, UdpTransport,
};
use novovm_node::tx_ingress::{
    decode_eth_send_raw_hex_payload_v1, run_eth_send_raw_transaction_from_params_v1,
    get_nov_native_execution_receipt_by_hash_v1,
    get_nov_native_treasury_clearing_summary_v1, get_nov_native_treasury_settlement_summary_v1,
    has_nov_native_call_shape_v1,
    nov_native_module_info_v1, run_nov_native_call_from_params_v1,
    run_nov_send_raw_transaction_from_params_v1, run_nov_send_transaction_from_params_v1,
};
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
use rocksdb::{Options as RocksDbOptions, WriteBatch as RocksDbWriteBatch, DB as RocksDb};
use serde::{de::DeserializeOwned, Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::{HashMap, HashSet};
use std::fs::{self, OpenOptions};
use std::io::{BufRead, BufReader, Read, Write};
use std::net::{IpAddr, SocketAddr, TcpStream, ToSocketAddrs};
use std::path::{Component, Path, PathBuf};
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

fn string_list_env_nonempty(name: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut seen = HashSet::new();
    let Some(raw) = string_env_nonempty(name) else {
        return out;
    };
    for token in raw.split(',') {
        let trimmed = token.trim();
        if trimmed.is_empty() {
            continue;
        }
        let value = trimmed.to_string();
        if seen.insert(value.clone()) {
            out.push(value);
        }
    }
    out
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

#[derive(Debug, Clone, Default)]
struct CanonicalBatchArtifactsV1 {
    execution_receipts: Vec<SupervmEvmExecutionReceiptV1>,
    state_mirror_updates: Vec<SupervmEvmStateMirrorUpdateV1>,
}

#[derive(Debug, Clone)]
struct AdapterSignalSummary {
    backend: &'static str,
    chain: &'static str,
    chain_id: u64,
    txs: usize,
    verified: bool,
    applied: bool,
    accounts: usize,
    state_root: [u8; 32],
    canonical_artifacts: Option<CanonicalBatchArtifactsV1>,
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

#[derive(Debug, Clone, Copy, Default)]
struct UnifiedAccountExecGuardSummary {
    checked: usize,
    routed: usize,
    created_ucas: usize,
    added_bindings: usize,
    decision_fast_path: usize,
    decision_adapter: usize,
}

#[derive(Debug)]
struct UnifiedAccountStoreSnapshot {
    router: UnifiedAccountRouter,
    flushed_event_count: u64,
}

#[derive(Debug, Serialize, Deserialize)]
struct UnifiedAccountStoreEnvelopeV1 {
    version: u32,
    router: UnifiedAccountRouter,
    flushed_event_count: u64,
}

#[derive(Debug, Clone)]
enum UnifiedAccountStoreBackend {
    BincodeFile { path: PathBuf },
    RocksDb { path: PathBuf },
}

#[derive(Debug)]
struct UnifiedAccountRuntime {
    store: UnifiedAccountStoreBackend,
    snapshot: UnifiedAccountStoreSnapshot,
    audit_sink: UnifiedAccountAuditSinkBackend,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct UnifiedAccountAuditSinkRecord {
    at: u64,
    source: String,
    method: String,
    success: bool,
    router_changed: bool,
    event_cursor_from: u64,
    event_cursor_to: u64,
    router_events: Vec<AccountAuditEvent>,
    params: serde_json::Value,
    error: Option<String>,
}

#[derive(Debug, Clone)]
enum UnifiedAccountAuditSinkBackend {
    JsonlFile { path: PathBuf },
    RocksDb { path: PathBuf },
}

#[derive(Debug, Clone)]
struct D1PersistenceBinding {
    enforce: bool,
    variant: AoemRuntimeVariant,
    rocksdb_persistence: bool,
    persistence_root: PathBuf,
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

#[repr(C)]
#[derive(Debug, Clone, Copy, Default)]
struct PluginApplyOptionsV1 {
    flags: u64,
}

type PluginVersionFn = unsafe extern "C" fn() -> u32;
type PluginCapabilitiesFn = unsafe extern "C" fn() -> u64;
type PluginApplyV2Fn = unsafe extern "C" fn(
    chain_type_code: u32,
    chain_id: u64,
    tx_ir_ptr: *const u8,
    tx_ir_len: usize,
    options_ptr: *const PluginApplyOptionsV1,
    out_result: *mut PluginApplyResultV1,
) -> i32;
type PluginDrainBincodeFn = unsafe extern "C" fn(
    max_items: usize,
    out_ptr: *mut u8,
    out_cap: usize,
    out_len: *mut usize,
) -> i32;

const ADAPTER_PLUGIN_EXPECTED_ABI_DEFAULT: u32 = 1;
const ADAPTER_PLUGIN_CAP_APPLY_IR_V1: u64 = 0x1;
const ADAPTER_PLUGIN_CAP_UA_SELF_GUARD_V1: u64 = 0x2;
const ADAPTER_PLUGIN_APPLY_FLAG_UA_SELF_GUARD_V1: u64 = 0x1;
const ADAPTER_PLUGIN_APPLY_FLAG_MAINLINE_INGRESS_BYPASS_V1: u64 = 0x4;
const ADAPTER_PLUGIN_APPLY_FLAG_MAINLINE_RECEIPT_EXPORT_V1: u64 = 0x8;
const ADAPTER_PLUGIN_APPLY_FLAG_MAINLINE_RECEIPT_INGEST_V1: u64 = 0x10;
const ADAPTER_PLUGIN_REQUIRED_CAPS_DEFAULT: u64 = ADAPTER_PLUGIN_CAP_APPLY_IR_V1;
const ADAPTER_PLUGIN_REGISTRY_PATH_DEFAULT: &str =
    "..\\..\\config\\novovm-adapter-plugin-registry.json";
const ADAPTER_PLUGIN_REGISTRY_PATH_ALT: &str = "config\\novovm-adapter-plugin-registry.json";
const CONSENSUS_POLICY_PATH_DEFAULT: &str = "..\\..\\config\\novovm-consensus-policy.json";
const CONSENSUS_POLICY_PATH_ALT: &str = "config\\novovm-consensus-policy.json";
const FOREIGN_RATE_DEFAULT_SOURCE_NAME: &str = "market_policy_config_v1";
const NAV_VALUATION_DEFAULT_PRICE_BP: u32 = 10_000;
const NAV_VALUATION_MAX_PRICE_BP: u32 = 1_000_000;
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
struct NavFeedHttpEndpoint {
    host: String,
    host_header: String,
    port: u16,
    path_and_query: String,
}

#[derive(Debug, Clone)]
struct LoadedNavValuationSource {
    mode: String,
    source_name: String,
    configured_url: String,
    configured_sources: u32,
    strict: bool,
    timeout_ms: u64,
    fetched: bool,
    fetched_sources: u32,
    min_sources: u32,
    signature_required: bool,
    signature_verified: bool,
    fallback_to_deterministic: bool,
    price_bp: u32,
    reason_code: String,
}

#[derive(Debug, Clone)]
struct NavFeedFetchResult {
    price_bp: u32,
    signature_verified: bool,
}

#[derive(Debug, Clone)]
struct LoadedForeignRateSource {
    mode: String,
    source_name: String,
    configured_url: String,
    configured_sources: u32,
    strict: bool,
    timeout_ms: u64,
    fetched: bool,
    fetched_sources: u32,
    min_sources: u32,
    signature_required: bool,
    signature_verified: bool,
    quote_spec_applied: bool,
    fallback_to_deterministic: bool,
    reason_code: String,
}

#[derive(Debug, Clone)]
struct ForeignRateFeedFetchResult {
    quote_spec: String,
    signature_verified: bool,
}

#[derive(Debug, Clone)]
struct LocalBatch {
    id: u64,
    txs: Vec<LocalTx>,
    mapped_ops: u32,
    txs_digest: [u8; 32],
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
struct QueryExecutionLogRecord {
    tx_hash: String,
    block_height: u64,
    block_hash: String,
    emitter: String,
    topics: Vec<String>,
    data: String,
    tx_index: u32,
    log_index: u32,
    state_version: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct QueryReceiptRecord {
    tx_hash: String,
    block_height: u64,
    block_hash: String,
    success: bool,
    gas_used: u64,
    state_root: String,
    #[serde(default)]
    chain_type: String,
    #[serde(default)]
    chain_id: u64,
    #[serde(default)]
    tx_type: String,
    #[serde(default)]
    receipt_type: Option<u8>,
    #[serde(default)]
    cumulative_gas_used: u64,
    #[serde(default)]
    effective_gas_price: Option<u64>,
    #[serde(default)]
    log_bloom: String,
    #[serde(default)]
    revert_data: Option<String>,
    #[serde(default)]
    state_version: u64,
    #[serde(default)]
    contract_address: Option<String>,
    #[serde(default)]
    logs: Vec<QueryExecutionLogRecord>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct QueryStateMirrorRecord {
    block_height: u64,
    block_hash: String,
    chain_type: String,
    chain_id: u64,
    state_version: u64,
    state_root: String,
    receipt_count: u64,
    accepted_receipt_count: u64,
    tx_hashes: Vec<String>,
    imported_at_unix_ms: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
struct QueryStateDb {
    blocks: Vec<QueryBlockRecord>,
    txs: HashMap<String, QueryTxRecord>,
    receipts: HashMap<String, QueryReceiptRecord>,
    balances: HashMap<String, u64>,
    #[serde(default)]
    logs: Vec<QueryExecutionLogRecord>,
    #[serde(default)]
    state_mirror_updates: Vec<QueryStateMirrorRecord>,
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
static D1_PERSISTENCE_BINDING: OnceLock<Result<D1PersistenceBinding, String>> = OnceLock::new();

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
const LOCAL_TX_UCA_PRIMARY_KEY_DOMAIN: &[u8] = b"novovm_local_uca_primary_key_ref_v1";
const LOCAL_TX_UCA_ID_PREFIX: &str = "uca:local:";
const UNIFIED_ACCOUNT_STORE_ENVELOPE_VERSION: u32 = 1;
const UNIFIED_ACCOUNT_STORE_BACKEND_FILE: &str = "bincode_file";
const UNIFIED_ACCOUNT_STORE_BACKEND_ROCKSDB: &str = "rocksdb";
const UNIFIED_ACCOUNT_STORE_ROCKSDB_CF_STATE: &str = "ua_store_state_v2";
const UNIFIED_ACCOUNT_STORE_ROCKSDB_CF_AUDIT: &str = "ua_store_audit_v2";
const UNIFIED_ACCOUNT_STORE_ROCKSDB_KEY_SNAPSHOT: &[u8] = b"unified_account:snapshot:v1";
const UNIFIED_ACCOUNT_STORE_ROCKSDB_KEY_STATE_ROUTER: &[u8] = b"ua_store:state:router:v2";
const UNIFIED_ACCOUNT_STORE_ROCKSDB_KEY_AUDIT_CURSOR: &[u8] =
    b"ua_store:audit:flushed_event_count:v1";
const UNIFIED_ACCOUNT_AUDIT_BACKEND_JSONL: &str = "jsonl";
const UNIFIED_ACCOUNT_AUDIT_BACKEND_ROCKSDB: &str = "rocksdb";
const UNIFIED_ACCOUNT_AUDIT_LOG_NAME: &str = "ua-account-audit-events.jsonl";
const UNIFIED_ACCOUNT_AUDIT_DB_NAME: &str = "ua-account-audit-events.rocksdb";
const UNIFIED_ACCOUNT_AUDIT_ROCKSDB_KEY_SEQ: &[u8] = b"ua_audit:seq";
const UNIFIED_ACCOUNT_AUDIT_ROCKSDB_KEY_EVENT_PREFIX: &[u8] = b"ua_audit:event:";
const UNIFIED_ACCOUNT_PLUGIN_INGRESS_GUARD_ENV: &str =
    "NOVOVM_UNIFIED_ACCOUNT_PLUGIN_INGRESS_GUARD";
const UNIFIED_ACCOUNT_PLUGIN_PREFER_SELF_GUARD_ENV: &str =
    "NOVOVM_UNIFIED_ACCOUNT_PLUGIN_PREFER_SELF_GUARD";
const EVM_OVERLAP_ROUTER_P1_COMPARE_READY_ENV: &str = "NOVOVM_EVM_OVERLAP_P1_COMPARE_READY";
const EVM_OVERLAP_ROUTER_P2_FORCE_PLUGIN_ENV: &str = "NOVOVM_EVM_OVERLAP_P2_FORCE_PLUGIN";

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

fn verify_local_tx_signatures_batch(txs: &[LocalTx]) -> Vec<bool> {
    const MIN_WORKER_CHUNK: usize = 64;

    if txs.is_empty() {
        return Vec::new();
    }
    let cpu_parallelism = std::thread::available_parallelism()
        .map(|v| v.get())
        .unwrap_or(1);
    let load_limited_workers = txs.len().div_ceil(MIN_WORKER_CHUNK).max(1);
    let worker_count = cpu_parallelism.min(load_limited_workers).min(txs.len().max(1));
    if worker_count <= 1 || txs.len() <= 1 {
        return txs.iter().map(verify_local_tx_signature).collect();
    }

    let chunk_size = txs.len().div_ceil(worker_count);
    let mut results = vec![false; txs.len()];
    std::thread::scope(|scope| {
        let mut jobs = Vec::with_capacity(worker_count);
        for (out_chunk, tx_chunk) in results.chunks_mut(chunk_size).zip(txs.chunks(chunk_size)) {
            jobs.push(scope.spawn(move || {
                for (out, tx) in out_chunk.iter_mut().zip(tx_chunk.iter()) {
                    *out = verify_local_tx_signature(tx);
                }
            }));
        }
        for job in jobs {
            job.join().expect("local tx signature worker panicked");
        }
    });
    results
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
    let sig_results = verify_local_tx_signatures_batch(txs);

    for (idx, tx) in txs.iter().enumerate() {
        if tx.fee < fee_floor {
            rejected = rejected.saturating_add(1);
            continue;
        }
        if !sig_results[idx] {
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

    let accepted_len = accepted.len();
    Ok((
        accepted,
        MempoolAdmissionSummary {
            accepted: accepted_len,
            rejected,
            fee_floor,
            nonce_ok,
            sig_ok,
        },
    ))
}

fn admit_mempool_basic_owned(
    txs: Vec<LocalTx>,
    fee_floor: u64,
) -> Result<(Vec<LocalTx>, MempoolAdmissionSummary)> {
    let (accepted, summary, _) = admit_mempool_basic_owned_with_meta(txs, fee_floor)?;
    Ok((accepted, summary))
}

fn admit_mempool_basic_owned_with_meta(
    txs: Vec<LocalTx>,
    fee_floor: u64,
) -> Result<(Vec<LocalTx>, MempoolAdmissionSummary, TxMetaSummary)> {
    if txs.is_empty() {
        bail!("mempool admission requires at least one tx");
    }

    let sig_results = verify_local_tx_signatures_batch(&txs);
    let mut accepted = Vec::with_capacity(txs.len());
    let mut rejected = 0usize;
    let mut nonce_ok = true;
    let mut sig_ok = true;
    let mut next_nonce_by_account: HashMap<u64, u64> = HashMap::with_capacity(txs.len());
    let mut min_fee = u64::MAX;
    let mut max_fee = 0u64;

    for (tx, sig_valid) in txs.into_iter().zip(sig_results.into_iter()) {
        if tx.fee < fee_floor {
            rejected = rejected.saturating_add(1);
            continue;
        }
        if !sig_valid {
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
        min_fee = min_fee.min(tx.fee);
        max_fee = max_fee.max(tx.fee);
        accepted.push(tx);
    }

    if accepted.is_empty() {
        bail!("mempool rejected all transactions");
    }

    let accepted_len = accepted.len();
    Ok((
        accepted,
        MempoolAdmissionSummary {
            accepted: accepted_len,
            rejected,
            fee_floor,
            nonce_ok,
            sig_ok,
        },
        TxMetaSummary {
            accounts: next_nonce_by_account.len(),
            min_fee,
            max_fee,
            nonce_ok: true,
            sig_ok: true,
        },
    ))
}

fn validate_and_summarize_txs_with_mode(
    txs: &[LocalTx],
    assume_signatures_valid: bool,
) -> Result<TxMetaSummary> {
    if txs.is_empty() {
        bail!("tx set cannot be empty");
    }

    let mut next_nonce_by_account: HashMap<u64, u64> = HashMap::with_capacity(txs.len());
    let mut min_fee = u64::MAX;
    let mut max_fee = 0u64;
    if assume_signatures_valid {
        for tx in txs {
            if tx.fee == 0 {
                bail!(
                    "tx fee must be > 0 (account={}, nonce={})",
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
            min_fee = min_fee.min(tx.fee);
            max_fee = max_fee.max(tx.fee);
        }
    } else {
        let sig_results = verify_local_tx_signatures_batch(txs);
        for (tx, sig_valid) in txs.iter().zip(sig_results.iter().copied()) {
            if tx.fee == 0 {
                bail!(
                    "tx fee must be > 0 (account={}, nonce={})",
                    tx.account,
                    tx.nonce
                );
            }
            if !sig_valid {
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
            min_fee = min_fee.min(tx.fee);
            max_fee = max_fee.max(tx.fee);
        }
    }

    Ok(TxMetaSummary {
        accounts: next_nonce_by_account.len(),
        min_fee,
        max_fee,
        nonce_ok: true,
        sig_ok: true,
    })
}

fn validate_and_summarize_txs(txs: &[LocalTx]) -> Result<TxMetaSummary> {
    validate_and_summarize_txs_with_mode(txs, false)
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

fn split_plugin_dir_list(raw: &str) -> Vec<PathBuf> {
    raw.split([';', ','])
        .filter_map(|part| {
            let trimmed = part.trim();
            if trimmed.is_empty() {
                None
            } else {
                Some(PathBuf::from(trimmed))
            }
        })
        .collect()
}

fn path_like_eq(a: &Path, b: &Path) -> bool {
    if let (Ok(a_canon), Ok(b_canon)) = (a.canonicalize(), b.canonicalize()) {
        return a_canon == b_canon;
    }
    if cfg!(target_os = "windows") {
        a.to_string_lossy()
            .eq_ignore_ascii_case(&b.to_string_lossy())
    } else {
        a == b
    }
}

fn push_unique_path(paths: &mut Vec<PathBuf>, path: PathBuf) {
    if path.as_os_str().is_empty() {
        return;
    }
    if paths.iter().any(|existing| path_like_eq(existing, &path)) {
        return;
    }
    paths.push(path);
}

fn resolve_adapter_plugin_search_dirs(registry_path: Option<&Path>) -> Vec<PathBuf> {
    let mut dirs = Vec::new();
    for key in [
        "NOVOVM_ADAPTER_PLUGIN_DIRS",
        "NOVOVM_AOEM_PLUGIN_DIRS",
        "AOEM_FFI_PLUGIN_DIRS",
    ] {
        if let Some(raw) = string_env_nonempty(key) {
            for dir in split_plugin_dir_list(&raw) {
                push_unique_path(&mut dirs, dir);
            }
        }
    }
    for key in [
        "NOVOVM_ADAPTER_PLUGIN_DIR",
        "NOVOVM_AOEM_PLUGIN_DIR",
        "AOEM_FFI_PLUGIN_DIR",
    ] {
        if let Some(raw) = string_env_nonempty(key) {
            push_unique_path(&mut dirs, PathBuf::from(raw));
        }
    }
    if let Some(parent) = registry_path.and_then(Path::parent) {
        push_unique_path(&mut dirs, parent.to_path_buf());
    }
    if let Ok(cwd) = std::env::current_dir() {
        push_unique_path(&mut dirs, cwd);
    }
    dirs
}

fn file_mtime_nanos(path: &Path) -> u128 {
    fs::metadata(path)
        .ok()
        .and_then(|m| m.modified().ok())
        .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
        .map(|d| d.as_nanos())
        .unwrap_or(0)
}

fn collect_registry_entry_candidate_paths(
    entry: &AdapterPluginRegistryEntry,
    search_dirs: &[PathBuf],
) -> Vec<PathBuf> {
    let mut out = Vec::new();
    let raw_path = PathBuf::from(entry.path.trim());
    if raw_path.is_absolute() {
        if raw_path.exists() {
            push_unique_path(&mut out, raw_path);
        }
        return out;
    }

    if raw_path.exists() {
        push_unique_path(&mut out, raw_path.clone());
    }
    for dir in search_dirs {
        let candidate = dir.join(&raw_path);
        if candidate.exists() {
            push_unique_path(&mut out, candidate);
        }
    }
    out
}

fn pick_adapter_plugin_from_registry(
    registry: &AdapterPluginRegistryFile,
    chain: ChainType,
    expected_abi: u32,
    required_caps: u64,
    search_dirs: &[PathBuf],
) -> Result<Option<PathBuf>> {
    #[derive(Clone)]
    struct Candidate {
        path: PathBuf,
        chain_span: usize,
        chain_name_hint: bool,
        mtime_ns: u128,
    }

    let chain_name = chain.as_str().to_ascii_lowercase();
    let mut best: Option<Candidate> = None;
    for entry in &registry.plugins {
        if entry.enabled == Some(false) {
            continue;
        }
        if !chain_allowed_in_registry(entry, chain) {
            continue;
        }
        if entry.abi != expected_abi {
            continue;
        }
        let entry_required_caps =
            parse_u64_mask_str(&entry.required_caps, "registry.required_caps")?;
        if entry_required_caps != required_caps {
            continue;
        }

        let candidates = collect_registry_entry_candidate_paths(entry, search_dirs);
        if candidates.is_empty() {
            continue;
        }
        let chain_name_hint = entry.name.to_ascii_lowercase().contains(&chain_name)
            || entry.path.to_ascii_lowercase().contains(&chain_name);
        let chain_span = entry.chains.len();
        for path in candidates {
            let candidate = Candidate {
                mtime_ns: file_mtime_nanos(&path),
                path,
                chain_span,
                chain_name_hint,
            };
            match &best {
                None => best = Some(candidate),
                Some(current) => {
                    let better = if candidate.chain_name_hint != current.chain_name_hint {
                        candidate.chain_name_hint && !current.chain_name_hint
                    } else if candidate.chain_span != current.chain_span {
                        candidate.chain_span < current.chain_span
                    } else if candidate.mtime_ns != current.mtime_ns {
                        candidate.mtime_ns > current.mtime_ns
                    } else {
                        candidate.path.to_string_lossy() < current.path.to_string_lossy()
                    };
                    if better {
                        best = Some(candidate);
                    }
                }
            }
        }
    }
    Ok(best.map(|c| c.path))
}

fn resolve_adapter_plugin_path(
    chain: ChainType,
    expected_abi: u32,
    required_caps: u64,
) -> Result<Option<PathBuf>> {
    for key in ["NOVOVM_ADAPTER_PLUGIN_PATH", "NOVOVM_EVM_PLUGIN_PATH"] {
        if let Some(raw) = string_env_nonempty(key) {
            return Ok(Some(PathBuf::from(raw)));
        }
    }

    let registry_path = resolve_adapter_plugin_registry_path();
    let Some(registry_path) = registry_path.as_ref() else {
        return Ok(None);
    };
    let (registry, _sha256) = load_adapter_plugin_registry(registry_path)?;
    let search_dirs = resolve_adapter_plugin_search_dirs(Some(registry_path));
    pick_adapter_plugin_from_registry(
        &registry,
        chain,
        expected_abi,
        required_caps,
        &search_dirs,
    )
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
        ChainType::Polygon => Some(5),
        ChainType::BNB => Some(6),
        ChainType::Avalanche => Some(7),
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

fn tx_type_name(tx_type: TxType) -> &'static str {
    match tx_type {
        TxType::Transfer => "transfer",
        TxType::ContractCall => "contract_call",
        TxType::ContractDeploy => "contract_deploy",
        TxType::Privacy => "privacy",
        TxType::CrossShard => "cross_shard",
        TxType::CrossChainTransfer => "cross_chain_transfer",
        TxType::CrossChainCall => "cross_chain_call",
    }
}

fn canonical_tx_hash_hex_from_local_tx(tx: &LocalTx, chain_id: u64) -> String {
    let tx_ir = to_adapter_tx_ir(tx, chain_id);
    to_hex(&tx_ir.hash)
}

unsafe fn drain_plugin_bincode_items_v1<T>(
    lib: &libloading::Library,
    symbol: &[u8],
    max_items: usize,
) -> Result<Vec<T>>
where
    T: DeserializeOwned,
{
    let drain_fn: libloading::Symbol<PluginDrainBincodeFn> = lib
        .get(symbol)
        .with_context(|| format!("resolve {} failed", String::from_utf8_lossy(symbol)))?;
    let mut out_len = 0usize;
    let probe_rc = drain_fn(max_items, std::ptr::null_mut(), 0, &mut out_len as *mut usize);
    if probe_rc != 0 {
        bail!(
            "plugin bincode export probe failed: symbol={} rc={}",
            String::from_utf8_lossy(symbol),
            probe_rc
        );
    }
    if out_len == 0 {
        return Ok(Vec::new());
    }
    let mut payload = vec![0u8; out_len];
    let rc = drain_fn(
        max_items,
        payload.as_mut_ptr(),
        payload.len(),
        &mut out_len as *mut usize,
    );
    if rc != 0 {
        bail!(
            "plugin bincode export failed: symbol={} rc={}",
            String::from_utf8_lossy(symbol),
            rc
        );
    }
    payload.truncate(out_len);
    crate::bincode_compat::deserialize(payload.as_slice()).with_context(|| {
        format!(
            "decode plugin bincode export failed: symbol={}",
            String::from_utf8_lossy(symbol)
        )
    })
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
    let mut parsed_txs = Vec::with_capacity(tx_irs.len());
    for ir in tx_irs {
        let raw = ir
            .serialize(SerializationFormat::Bincode)
            .context("adapter tx serialize failed")?;
        let parsed = adapter
            .parse_transaction(&raw)
            .context("adapter parse_transaction failed")?;
        parsed_txs.push(parsed);
    }

    let gas_limit = parsed_txs
        .iter()
        .fold(0u64, |acc, tx| acc.saturating_add(tx.gas_limit))
        .max(21_000);
    let parsed_block = BlockIR {
        hash: vec![0u8; 32],
        parent_hash: vec![0u8; 32],
        number: 1,
        timestamp: 0,
        transactions: parsed_txs,
        state_root: vec![0u8; 32],
        transactions_root: vec![0u8; 32],
        receipts_root: vec![0u8; 32],
        miner: vec![0u8; 20],
        difficulty: 0,
        gas_used: 0,
        gas_limit,
    };

    if adapter
        .verify_block(&parsed_block)
        .context("adapter verify_block failed")?
    {
        for tx in &parsed_block.transactions {
            if let Err(e) = adapter.execute_transaction(tx, &mut state) {
                return Err(e).context("adapter execute_transaction failed");
            }
        }
    } else {
        verified = false;
        applied = false;
        for tx in &parsed_block.transactions {
            let tx_ok = adapter
                .verify_transaction(tx)
                .context("adapter verify_transaction failed")?;
            if tx_ok {
                if let Err(e) = adapter.execute_transaction(tx, &mut state) {
                    return Err(e).context("adapter execute_transaction failed");
                }
            }
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
        chain_id,
        txs: tx_irs.len(),
        verified,
        applied,
        accounts: state.accounts.len(),
        state_root,
        canonical_artifacts: None,
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
    prefer_plugin_self_guard: bool,
    registry: AdapterPluginRegistrySummary,
) -> Result<AdapterSignalSummary> {
    let chain_code = chain_type_to_plugin_code(chain).ok_or_else(|| {
        anyhow::anyhow!("plugin backend does not support chain={}", chain.as_str())
    })?;
    let tx_bytes = crate::bincode_compat::serialize(tx_irs).context("serialize tx_irs for plugin failed")?;

    let lib = unsafe { libloading::Library::new(plugin_path) }
        .with_context(|| format!("load adapter plugin failed: {}", plugin_path.display()))?;

    unsafe {
        let version_fn: libloading::Symbol<PluginVersionFn> = lib
            .get(b"novovm_adapter_plugin_version\0")
            .context("resolve novovm_adapter_plugin_version failed")?;
        let caps_fn: libloading::Symbol<PluginCapabilitiesFn> = lib
            .get(b"novovm_adapter_plugin_capabilities\0")
            .context("resolve novovm_adapter_plugin_capabilities failed")?;
        let apply_v2_fn: libloading::Symbol<PluginApplyV2Fn> = lib
            .get(b"novovm_adapter_plugin_apply_v2\0")
            .context("resolve novovm_adapter_plugin_apply_v2 failed")?;

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

        let _: Vec<SupervmEvmExecutionReceiptV1> = drain_plugin_bincode_items_v1(
            &lib,
            b"novovm_adapter_plugin_drain_execution_receipts_bincode_v1\0",
            usize::MAX,
        )?;
        let _: Vec<SupervmEvmStateMirrorUpdateV1> = drain_plugin_bincode_items_v1(
            &lib,
            b"novovm_adapter_plugin_drain_state_mirror_updates_bincode_v1\0",
            usize::MAX,
        )?;

        let mut flags = ADAPTER_PLUGIN_APPLY_FLAG_MAINLINE_RECEIPT_EXPORT_V1
            | ADAPTER_PLUGIN_APPLY_FLAG_MAINLINE_RECEIPT_INGEST_V1
            | ADAPTER_PLUGIN_APPLY_FLAG_MAINLINE_INGRESS_BYPASS_V1;
        if prefer_plugin_self_guard {
            flags |= ADAPTER_PLUGIN_APPLY_FLAG_UA_SELF_GUARD_V1;
        }
        let options = PluginApplyOptionsV1 { flags };
        let mut out = PluginApplyResultV1::default();
        let rc = apply_v2_fn(
            chain_code,
            chain_id,
            tx_bytes.as_ptr(),
            tx_bytes.len(),
            &options as *const PluginApplyOptionsV1,
            &mut out as *mut PluginApplyResultV1,
        );
        if rc != 0 {
            bail!("adapter plugin apply failed: rc={}", rc);
        }
        if out.error_code != 0 {
            bail!("adapter plugin returned error_code={}", out.error_code);
        }
        let execution_receipts: Vec<SupervmEvmExecutionReceiptV1> = drain_plugin_bincode_items_v1(
            &lib,
            b"novovm_adapter_plugin_drain_execution_receipts_bincode_v1\0",
            (out.txs as usize).max(1),
        )?;
        let state_mirror_updates: Vec<SupervmEvmStateMirrorUpdateV1> =
            drain_plugin_bincode_items_v1(
                &lib,
                b"novovm_adapter_plugin_drain_state_mirror_updates_bincode_v1\0",
                (out.txs as usize).max(1),
            )?;
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
            chain_id,
            txs: out.txs as usize,
            verified: out.verified != 0,
            applied: out.applied != 0,
            accounts: out.accounts as usize,
            state_root: out.state_root,
            canonical_artifacts: Some(CanonicalBatchArtifactsV1 {
                execution_receipts,
                state_mirror_updates,
            }),
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

fn unified_account_plugin_ingress_guard_enabled() -> bool {
    bool_env_default(UNIFIED_ACCOUNT_PLUGIN_INGRESS_GUARD_ENV, true)
}

fn unified_account_plugin_prefer_self_guard_enabled() -> bool {
    bool_env_default(UNIFIED_ACCOUNT_PLUGIN_PREFER_SELF_GUARD_ENV, false)
}

fn guard_plugin_adapter_ingress_via_unified_account(
    txs: &[LocalTx],
    chain_id: u64,
) -> Result<()> {
    let query_db_path = chain_query_db_path();
    let ua_store = resolve_unified_account_store(&query_db_path)?;
    let mut ua_snapshot = ua_store.load_snapshot()?;
    let signature_domain = string_env_nonempty("NOVOVM_UNIFIED_ACCOUNT_EXEC_SIGNATURE_DOMAIN")
        .unwrap_or_else(|| format!("evm:{chain_id}"));
    let auto_provision = bool_env_default("NOVOVM_UNIFIED_ACCOUNT_EXEC_AUTOPROVISION", true);
    let _ = route_local_txs_through_unified_account(
        txs,
        &mut ua_snapshot.router,
        chain_id,
        &signature_domain,
        now_unix_sec(),
        auto_provision,
    )?;
    ua_store.save_snapshot(&ua_snapshot)?;
    Ok(())
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum EvmOverlapClass {
    P0,
    P1,
    P2,
}

impl EvmOverlapClass {
    fn as_str(self) -> &'static str {
        match self {
            EvmOverlapClass::P0 => "p0",
            EvmOverlapClass::P1 => "p1",
            EvmOverlapClass::P2 => "p2",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum EvmOverlapAutoOrder {
    NativeFirst,
    PluginFirst,
}

#[derive(Debug, Clone, Copy)]
struct EvmOverlapAutoPolicy {
    class: EvmOverlapClass,
    order: EvmOverlapAutoOrder,
    policy: &'static str,
    p1_compare_ready: bool,
}

fn classify_evm_overlap_batch(chain: ChainType, tx_irs: &[TxIR]) -> Option<EvmOverlapClass> {
    if !matches!(
        chain,
        ChainType::EVM | ChainType::BNB | ChainType::Polygon | ChainType::Avalanche
    ) {
        return None;
    }
    if tx_irs.is_empty() {
        return Some(EvmOverlapClass::P0);
    }
    if tx_irs
        .iter()
        .any(|tx| tx.source_chain.is_some() || tx.target_chain.is_some())
    {
        return Some(EvmOverlapClass::P2);
    }
    if tx_irs.iter().any(|tx| {
        tx.tx_type != TxType::Transfer
            || tx.to.is_none()
            || !tx.data.is_empty()
            || tx.gas_limit > 21_000
    }) {
        return Some(EvmOverlapClass::P1);
    }
    Some(EvmOverlapClass::P0)
}

fn resolve_evm_overlap_auto_policy_with_flags(
    chain: ChainType,
    tx_irs: &[TxIR],
    plugin_available: bool,
    p1_compare_ready: bool,
    p2_force_plugin: bool,
) -> Option<EvmOverlapAutoPolicy> {
    let class = classify_evm_overlap_batch(chain, tx_irs)?;
    let (order, policy) = match class {
        EvmOverlapClass::P0 => (EvmOverlapAutoOrder::NativeFirst, "p0_supervm_first"),
        EvmOverlapClass::P1 => {
            if p1_compare_ready {
                (
                    EvmOverlapAutoOrder::NativeFirst,
                    "p1_compare_green_supervm_first",
                )
            } else if plugin_available {
                (
                    EvmOverlapAutoOrder::PluginFirst,
                    "p1_compare_pending_plugin_first",
                )
            } else {
                (
                    EvmOverlapAutoOrder::NativeFirst,
                    "p1_plugin_missing_native_fallback",
                )
            }
        }
        EvmOverlapClass::P2 => {
            if p2_force_plugin && plugin_available {
                (EvmOverlapAutoOrder::PluginFirst, "p2_plugin_only")
            } else {
                (EvmOverlapAutoOrder::NativeFirst, "p2_native_fallback")
            }
        }
    };
    Some(EvmOverlapAutoPolicy {
        class,
        order,
        policy,
        p1_compare_ready,
    })
}

fn resolve_evm_overlap_auto_policy(
    chain: ChainType,
    tx_irs: &[TxIR],
    plugin_available: bool,
) -> Option<EvmOverlapAutoPolicy> {
    let p1_compare_ready = bool_env_default(EVM_OVERLAP_ROUTER_P1_COMPARE_READY_ENV, false);
    let p2_force_plugin = bool_env_default(EVM_OVERLAP_ROUTER_P2_FORCE_PLUGIN_ENV, true);
    resolve_evm_overlap_auto_policy_with_flags(
        chain,
        tx_irs,
        plugin_available,
        p1_compare_ready,
        p2_force_plugin,
    )
}

fn run_adapter_bridge_signal_with_options(
    txs: &[LocalTx],
    plugin_ingress_pre_guarded: bool,
) -> Result<AdapterSignalSummary> {
    if txs.is_empty() {
        bail!("adapter bridge requires at least one tx");
    }

    let chain = resolve_adapter_chain()?;
    let chain_id = default_chain_id(chain);
    let backend_mode = resolve_adapter_backend_mode()?;
    let (plugin_expected_abi, mut plugin_required_caps) = resolve_adapter_plugin_requirements()?;
    let plugin_path = resolve_adapter_plugin_path(chain, plugin_expected_abi, plugin_required_caps)?;
    let prefer_plugin_self_guard = !plugin_ingress_pre_guarded
        && unified_account_plugin_prefer_self_guard_enabled();
    let host_plugin_guard_enabled = !plugin_ingress_pre_guarded
        && unified_account_plugin_ingress_guard_enabled()
        && !prefer_plugin_self_guard;
    if prefer_plugin_self_guard {
        // Performance mode: rely on plugin-side UA guard and avoid host duplicate guard.
        plugin_required_caps |= ADAPTER_PLUGIN_CAP_UA_SELF_GUARD_V1;
    }
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
    let overlap_auto_policy = if backend_mode == AdapterBackendMode::Auto {
        resolve_evm_overlap_auto_policy(chain, &tx_irs, plugin_path.is_some())
    } else {
        None
    };
    if let Some(policy) = overlap_auto_policy {
        println!(
            "overlap_router_out: chain={} class={} policy={} order={} p1_compare_ready={} plugin_available={} backend_mode=auto",
            chain.as_str(),
            policy.class.as_str(),
            policy.policy,
            match policy.order {
                EvmOverlapAutoOrder::NativeFirst => "native_first",
                EvmOverlapAutoOrder::PluginFirst => "plugin_first",
            },
            policy.p1_compare_ready,
            plugin_path.is_some()
        );
    }
    let run_plugin = |path: &Path| -> Result<AdapterSignalSummary> {
        let plugin_registry = resolve_adapter_plugin_registry_summary(
            chain,
            Some(path),
            plugin_expected_abi,
            plugin_required_caps,
        )?;
        if host_plugin_guard_enabled {
            guard_plugin_adapter_ingress_via_unified_account(txs, chain_id).context(
                "plugin adapter ingress unified account guard failed before plugin apply",
            )?;
        }
        run_plugin_adapter_signal(
            chain,
            chain_id,
            &tx_irs,
            path,
            plugin_expected_abi,
            plugin_required_caps,
            prefer_plugin_self_guard,
            plugin_registry,
        )
    };
    let run_native = || {
        run_native_adapter_signal(
            chain,
            chain_id,
            &tx_irs,
            plugin_expected_abi,
            plugin_required_caps,
            native_registry,
        )
    };

    match backend_mode {
        AdapterBackendMode::Native => run_native(),
        AdapterBackendMode::Plugin => {
            let path = plugin_path.as_deref().ok_or_else(|| {
                anyhow::anyhow!(
                    "NOVOVM_ADAPTER_BACKEND=plugin requires adapter plugin path (set NOVOVM_ADAPTER_PLUGIN_PATH/NOVOVM_EVM_PLUGIN_PATH or provide registry + plugin dirs)"
                )
            })?;
            run_plugin(path)
        }
        AdapterBackendMode::Auto => {
            let plugin_first = overlap_auto_policy
                .map(|policy| policy.order == EvmOverlapAutoOrder::PluginFirst)
                .unwrap_or(false);
            if plugin_first {
                if let Some(path) = plugin_path.as_deref() {
                    match run_plugin(path) {
                        Ok(signal) => Ok(signal),
                        Err(plugin_err) => {
                            if supports_native_chain(chain) {
                                eprintln!(
                                    "adapter_warn: plugin backend failed ({}), fallback native",
                                    plugin_err
                                );
                                run_native()
                            } else {
                                Err(plugin_err)
                            }
                        }
                    }
                } else if supports_native_chain(chain) {
                    run_native()
                } else {
                    bail!(
                        "adapter backend auto cannot resolve chain={} without plugin path",
                        chain.as_str()
                    )
                }
            } else {
                if supports_native_chain(chain) {
                    match run_native() {
                        Ok(signal) => Ok(signal),
                        Err(native_err) => {
                            if let Some(path) = plugin_path.as_deref() {
                                eprintln!(
                                    "adapter_warn: native backend failed ({}), fallback plugin={}",
                                    native_err,
                                    path.display()
                                );
                                run_plugin(path)
                            } else {
                                Err(native_err)
                            }
                        }
                    }
                } else if let Some(path) = plugin_path.as_deref() {
                    run_plugin(path)
                } else {
                    bail!(
                        "adapter backend auto cannot resolve chain={} without plugin path",
                        chain.as_str()
                    )
                }
            }
        }
    }
}

fn run_adapter_bridge_signal(txs: &[LocalTx]) -> Result<AdapterSignalSummary> {
    run_adapter_bridge_signal_with_options(txs, false)
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
    let (batches, _, _) = build_local_batches_and_ops_from_txs(txs, requested_batches);
    batches
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
    let (_, ops, _) = build_local_batches_and_ops_from_txs(txs, 1);
    ops
}

fn build_local_batches_and_ops_from_txs(
    txs: &[LocalTx],
    requested_batches: usize,
) -> (Vec<LocalBatch>, ExecBatchBuffer, usize) {
    if txs.is_empty() {
        return (
            Vec::new(),
            ExecBatchBuffer {
                _keys: Vec::new(),
                _values: Vec::new(),
                ops: Vec::new(),
            },
            0,
        );
    }

    let batch_count = requested_batches.max(1).min(txs.len());
    let base = txs.len() / batch_count;
    let rem = txs.len() % batch_count;

    let mut out = Vec::with_capacity(batch_count);
    let mut keys = vec![[0u8; 8]; txs.len()];
    let mut values = vec![[0u8; 8]; txs.len()];
    let mut ops = Vec::with_capacity(txs.len());
    let mut total_mapped_ops = 0usize;
    let mut cursor = 0usize;

    for i in 0..batch_count {
        let sz = base + usize::from(i < rem);
        let end = cursor + sz;
        let mut batch_txs = Vec::with_capacity(sz);
        let mut digest_hasher = Sha256::new();
        for idx in cursor..end {
            let tx = &txs[idx];
            let tx_hash = hash_local_tx(tx);
            digest_hasher.update(tx_hash);
            batch_txs.push(tx.clone());
            keys[idx] = tx.key.to_le_bytes();
            values[idx] = tx.value.to_le_bytes();
            ops.push(ExecOpV2 {
                opcode: 2,
                flags: 0,
                reserved: 0,
                key_ptr: keys[idx].as_mut_ptr(),
                key_len: keys[idx].len() as u32,
                value_ptr: values[idx].as_mut_ptr(),
                value_len: values[idx].len() as u32,
                delta: 0,
                expect_version: u64::MAX,
                plan_id: (tx.account << 32) | tx.nonce.saturating_add(1),
            });
        }

        let txs_digest: [u8; 32] = digest_hasher.finalize().into();
        total_mapped_ops = total_mapped_ops.saturating_add(batch_txs.len());
        out.push(LocalBatch {
            id: (i + 1) as u64,
            mapped_ops: batch_txs.len() as u32,
            txs: batch_txs,
            txs_digest,
        });
        cursor = end;
    }

    (
        out,
        ExecBatchBuffer {
            _keys: keys,
            _values: values,
            ops,
        },
        total_mapped_ops,
    )
}

fn build_local_batches_and_ops_from_txs_owned(
    txs: Vec<LocalTx>,
    requested_batches: usize,
) -> (Vec<LocalBatch>, ExecBatchBuffer, usize) {
    if txs.is_empty() {
        return (
            Vec::new(),
            ExecBatchBuffer {
                _keys: Vec::new(),
                _values: Vec::new(),
                ops: Vec::new(),
            },
            0,
        );
    }

    let total_txs = txs.len();
    let batch_count = requested_batches.max(1).min(total_txs);
    let base = total_txs / batch_count;
    let rem = total_txs % batch_count;

    let mut out = Vec::with_capacity(batch_count);
    let mut keys = vec![[0u8; 8]; total_txs];
    let mut values = vec![[0u8; 8]; total_txs];
    let mut ops = Vec::with_capacity(total_txs);
    let mut total_mapped_ops = 0usize;
    let mut op_cursor = 0usize;
    let mut tx_iter = txs.into_iter();

    for i in 0..batch_count {
        let sz = base + usize::from(i < rem);
        let mut batch_txs = Vec::with_capacity(sz);
        let mut digest_hasher = Sha256::new();
        for _ in 0..sz {
            let tx = tx_iter
                .next()
                .expect("tx iterator should yield enough items for computed batch size");
            let tx_hash = hash_local_tx(&tx);
            digest_hasher.update(tx_hash);
            keys[op_cursor] = tx.key.to_le_bytes();
            values[op_cursor] = tx.value.to_le_bytes();
            ops.push(ExecOpV2 {
                opcode: 2,
                flags: 0,
                reserved: 0,
                key_ptr: keys[op_cursor].as_mut_ptr(),
                key_len: keys[op_cursor].len() as u32,
                value_ptr: values[op_cursor].as_mut_ptr(),
                value_len: values[op_cursor].len() as u32,
                delta: 0,
                expect_version: u64::MAX,
                plan_id: (tx.account << 32) | tx.nonce.saturating_add(1),
            });
            op_cursor = op_cursor.saturating_add(1);
            batch_txs.push(tx);
        }

        let txs_digest: [u8; 32] = digest_hasher.finalize().into();
        total_mapped_ops = total_mapped_ops.saturating_add(batch_txs.len());
        out.push(LocalBatch {
            id: (i + 1) as u64,
            mapped_ops: batch_txs.len() as u32,
            txs: batch_txs,
            txs_digest,
        });
    }

    (
        out,
        ExecBatchBuffer {
            _keys: keys,
            _values: values,
            ops,
        },
        total_mapped_ops,
    )
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
    hasher.update(batch.txs_digest);
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
        hasher.update(batch.txs_digest);
    }
    hasher.finalize().into()
}

fn build_local_block_owned(closure: &BatchAClosureOutput, batches: Vec<LocalBatch>) -> LocalBlock {
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
    let block_hash = compute_block_hash(&header, &batches);
    LocalBlock {
        header,
        batches,
        proposal_hash: closure.proposal_hash,
        block_hash,
    }
}

fn build_local_block(closure: &BatchAClosureOutput, batches: &[LocalBatch]) -> LocalBlock {
    build_local_block_owned(closure, batches.to_vec())
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

fn d2d3_enforce_d1_persistence() -> bool {
    bool_env_default("NOVOVM_D2D3_ENFORCE_D1_PERSIST", true)
}

fn d2d3_storage_root_path() -> PathBuf {
    if let Some(custom) = string_env_nonempty("NOVOVM_D2D3_STORAGE_ROOT") {
        return PathBuf::from(custom);
    }
    if let Some(aoem_root) = string_env_nonempty("AOEM_PERSISTENCE_PATH")
        .or_else(|| string_env_nonempty("NOVOVM_AOEM_PERSISTENCE_PATH"))
    {
        return PathBuf::from(aoem_root).join("novovm-host");
    }
    PathBuf::from("artifacts")
}

fn normalize_lexical_path(path: &Path) -> PathBuf {
    let mut normalized = PathBuf::new();
    for component in path.components() {
        match component {
            Component::CurDir => {}
            Component::ParentDir => {
                let _ = normalized.pop();
            }
            _ => normalized.push(component.as_os_str()),
        }
    }
    normalized
}

fn absolute_normalized_path(path: &Path) -> Result<PathBuf> {
    let absolute = if path.is_absolute() {
        path.to_path_buf()
    } else {
        std::env::current_dir()
            .context("resolve current directory for D2/D3 persistence policy failed")?
            .join(path)
    };
    Ok(normalize_lexical_path(&absolute))
}

fn ensure_d2d3_path_under_d1_root(label: &str, path: &Path, root: &Path) -> Result<()> {
    let root_abs = absolute_normalized_path(root)?;
    let path_abs = absolute_normalized_path(path)?;
    if !path_abs.starts_with(&root_abs) {
        bail!(
            "D2/D3 persistence path {}={} is outside D1 root {}; keep D2/D3 persistence under D1",
            label,
            path_abs.display(),
            root_abs.display()
        );
    }
    Ok(())
}

fn ensure_d2d3_persistence_paths_under_d1_root(root: &Path, enforce: bool) -> Result<()> {
    if !enforce {
        return Ok(());
    }
    let query_db_path = chain_query_db_path();
    let governance_audit_path = governance_audit_db_path(&query_db_path);
    let governance_chain_audit_path = governance_chain_audit_db_path(&query_db_path);
    let unified_account_backend = unified_account_store_backend_kind();
    let unified_account_store_path =
        unified_account_store_path_for_backend(&query_db_path, &unified_account_backend);
    let unified_account_audit_backend = unified_account_audit_backend_kind();
    let unified_account_audit_path =
        unified_account_audit_path_for_backend(&query_db_path, &unified_account_audit_backend);
    ensure_d2d3_path_under_d1_root("chain_query_db", &query_db_path, root)?;
    ensure_d2d3_path_under_d1_root("governance_audit_db", &governance_audit_path, root)?;
    ensure_d2d3_path_under_d1_root(
        "governance_chain_audit_db",
        &governance_chain_audit_path,
        root,
    )?;
    ensure_d2d3_path_under_d1_root("unified_account_db", &unified_account_store_path, root)?;
    ensure_d2d3_path_under_d1_root(
        "unified_account_audit_log",
        &unified_account_audit_path,
        root,
    )?;
    Ok(())
}

fn ensure_d2d3_persistence_through_d1() -> Result<&'static D1PersistenceBinding> {
    let cached = D1_PERSISTENCE_BINDING.get_or_init(|| {
        let enforce = d2d3_enforce_d1_persistence();
        let runtime =
            AoemRuntimeConfig::from_env().map_err(|e| format!("resolve AOEM runtime failed: {e}"))?;
        let persistence_root = d2d3_storage_root_path();
        ensure_d2d3_persistence_paths_under_d1_root(&persistence_root, enforce)
            .map_err(|e| format!("validate D2/D3 persistence paths failed: {e}"))?;

        if !enforce {
            return Ok(D1PersistenceBinding {
                enforce,
                variant: runtime.variant,
                rocksdb_persistence: false,
                persistence_root,
            });
        }
        if runtime.variant != AoemRuntimeVariant::Persist {
            return Err(format!(
                "D2/D3 persistence requires AOEM persist variant through D1: set NOVOVM_AOEM_VARIANT=persist (current={})",
                runtime.variant.as_str()
            ));
        }
        let facade = AoemExecFacade::open_with_runtime(&runtime)
            .map_err(|e| format!("open AOEM runtime for D2/D3 persistence check failed: {e}"))?;
        let capability = facade
            .capability_contract()
            .map_err(|e| format!("load AOEM capability contract failed: {e}"))?;
        let rocksdb_persistence = capability
            .raw
            .get("rocksdb_persistence")
            .and_then(|v| v.as_bool())
            .unwrap_or(false);
        if !rocksdb_persistence {
            return Err(
                "D2/D3 persistence requires AOEM rocksdb_persistence=true on persist variant"
                    .to_string(),
            );
        }
        Ok(D1PersistenceBinding {
            enforce,
            variant: runtime.variant,
            rocksdb_persistence,
            persistence_root,
        })
    });

    match cached {
        Ok(binding) => Ok(binding),
        Err(msg) => bail!("{msg}"),
    }
}

fn chain_query_db_path() -> PathBuf {
    std::env::var("NOVOVM_CHAIN_QUERY_DB")
        .map(PathBuf::from)
        .unwrap_or_else(|_| d2d3_storage_root_path().join("novovm-chain-query-db.json"))
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

fn unified_account_store_path_for_backend(query_db_path: &Path, backend: &str) -> PathBuf {
    if let Some(custom) = string_env_nonempty("NOVOVM_UNIFIED_ACCOUNT_DB") {
        return PathBuf::from(custom);
    }
    let default_name = match backend {
        UNIFIED_ACCOUNT_STORE_BACKEND_ROCKSDB => "novovm-unified-account-router.rocksdb",
        _ => "novovm-unified-account-router.bin",
    };
    if let Some(parent) = query_db_path.parent() {
        if !parent.as_os_str().is_empty() {
            return parent.join(default_name);
        }
    }
    PathBuf::from("artifacts").join(default_name)
}

fn unified_account_store_backend_kind() -> String {
    string_env(
        "NOVOVM_UNIFIED_ACCOUNT_STORE_BACKEND",
        UNIFIED_ACCOUNT_STORE_BACKEND_ROCKSDB,
    )
    .trim()
    .to_ascii_lowercase()
}

fn resolve_unified_account_store(query_db_path: &Path) -> Result<UnifiedAccountStoreBackend> {
    let backend = unified_account_store_backend_kind();
    let path = unified_account_store_path_for_backend(query_db_path, &backend);
    match backend.as_str() {
        "rocksdb" => Ok(UnifiedAccountStoreBackend::RocksDb { path }),
        "bincode_file" | "file" | "bincode" => {
            if bool_env("NOVOVM_ALLOW_NON_PROD_UA_BACKEND", false) {
                Ok(UnifiedAccountStoreBackend::BincodeFile { path })
            } else {
                bail!(
                    "NOVOVM_UNIFIED_ACCOUNT_STORE_BACKEND={} is non-production; use rocksdb or set NOVOVM_ALLOW_NON_PROD_UA_BACKEND=1 for explicit override",
                    backend
                )
            }
        }
        _ => bail!(
            "invalid NOVOVM_UNIFIED_ACCOUNT_STORE_BACKEND={}; valid: rocksdb|bincode_file|file|bincode",
            backend
        ),
    }
}

impl UnifiedAccountStoreBackend {
    fn backend_name(&self) -> &'static str {
        match self {
            UnifiedAccountStoreBackend::BincodeFile { .. } => UNIFIED_ACCOUNT_STORE_BACKEND_FILE,
            UnifiedAccountStoreBackend::RocksDb { .. } => UNIFIED_ACCOUNT_STORE_BACKEND_ROCKSDB,
        }
    }

    fn path(&self) -> &Path {
        match self {
            UnifiedAccountStoreBackend::BincodeFile { path } => path.as_path(),
            UnifiedAccountStoreBackend::RocksDb { path } => path.as_path(),
        }
    }

    fn load_snapshot(&self) -> Result<UnifiedAccountStoreSnapshot> {
        match self {
            UnifiedAccountStoreBackend::BincodeFile { path } => {
                if !path.exists() {
                    return Ok(empty_unified_account_snapshot());
                }
                let raw = fs::read(path).with_context(|| {
                    format!("read unified account db failed: {}", path.display())
                })?;
                decode_unified_account_snapshot(&raw, path)
            }
            UnifiedAccountStoreBackend::RocksDb { path } => {
                let db = open_unified_account_rocksdb(path)?;
                let state_cf = db.cf_handle(UNIFIED_ACCOUNT_STORE_ROCKSDB_CF_STATE).ok_or_else(
                    || {
                        anyhow::anyhow!(
                            "missing unified account rocksdb column family '{}' for {}",
                            UNIFIED_ACCOUNT_STORE_ROCKSDB_CF_STATE,
                            path.display()
                        )
                    },
                )?;
                let audit_cf = db.cf_handle(UNIFIED_ACCOUNT_STORE_ROCKSDB_CF_AUDIT).ok_or_else(
                    || {
                        anyhow::anyhow!(
                            "missing unified account rocksdb column family '{}' for {}",
                            UNIFIED_ACCOUNT_STORE_ROCKSDB_CF_AUDIT,
                            path.display()
                        )
                    },
                )?;

                let mut router_raw = db
                    .get_cf(state_cf, UNIFIED_ACCOUNT_STORE_ROCKSDB_KEY_STATE_ROUTER)
                    .with_context(|| {
                        format!(
                            "read unified account rocksdb state key from cf '{}' failed: {}",
                            UNIFIED_ACCOUNT_STORE_ROCKSDB_CF_STATE,
                            path.display()
                        )
                    })?;
                let mut cursor_raw = db
                    .get_cf(audit_cf, UNIFIED_ACCOUNT_STORE_ROCKSDB_KEY_AUDIT_CURSOR)
                    .with_context(|| {
                        format!(
                            "read unified account rocksdb audit cursor key from cf '{}' failed: {}",
                            UNIFIED_ACCOUNT_STORE_ROCKSDB_CF_AUDIT,
                            path.display()
                        )
                    })?;

                if let Some(router_bytes) = router_raw {
                    let router: UnifiedAccountRouter =
                        crate::bincode_compat::deserialize(&router_bytes).with_context(|| {
                            format!(
                                "decode unified account rocksdb state router failed: {}",
                                path.display()
                            )
                        })?;
                    let flushed_event_count = match cursor_raw {
                        Some(bytes) => decode_u64_be(&bytes).with_context(|| {
                            format!(
                                "decode unified account rocksdb audit cursor failed: {}",
                                path.display()
                            )
                        })?,
                        None => router.events().len() as u64,
                    };
                    return Ok(UnifiedAccountStoreSnapshot {
                        router,
                        flushed_event_count,
                    });
                }
                if cursor_raw.is_some() {
                    bail!(
                        "invalid unified account rocksdb namespace: audit cursor exists but router state missing: {}",
                        path.display()
                    );
                }
                Ok(empty_unified_account_snapshot())
            }
        }
    }

    fn save_snapshot(&self, snapshot: &UnifiedAccountStoreSnapshot) -> Result<()> {
        match self {
            UnifiedAccountStoreBackend::BincodeFile { path } => {
                let encoded = encode_unified_account_snapshot(snapshot)?;
                ensure_parent_dir(path, "unified account db")?;
                fs::write(path, encoded).with_context(|| {
                    format!("write unified account db failed: {}", path.display())
                })?;
                Ok(())
            }
            UnifiedAccountStoreBackend::RocksDb { path } => {
                let db = open_unified_account_rocksdb(path)?;
                let state_cf = db.cf_handle(UNIFIED_ACCOUNT_STORE_ROCKSDB_CF_STATE).ok_or_else(
                    || {
                        anyhow::anyhow!(
                            "missing unified account rocksdb column family '{}' for {}",
                            UNIFIED_ACCOUNT_STORE_ROCKSDB_CF_STATE,
                            path.display()
                        )
                    },
                )?;
                let audit_cf = db.cf_handle(UNIFIED_ACCOUNT_STORE_ROCKSDB_CF_AUDIT).ok_or_else(
                    || {
                        anyhow::anyhow!(
                            "missing unified account rocksdb column family '{}' for {}",
                            UNIFIED_ACCOUNT_STORE_ROCKSDB_CF_AUDIT,
                            path.display()
                        )
                    },
                )?;
                let router_encoded = crate::bincode_compat::serialize(&snapshot.router).with_context(|| {
                    format!(
                        "serialize unified account rocksdb state router failed: {}",
                        path.display()
                    )
                })?;
                let mut batch = RocksDbWriteBatch::default();
                batch.put_cf(
                    state_cf,
                    UNIFIED_ACCOUNT_STORE_ROCKSDB_KEY_STATE_ROUTER,
                    router_encoded.as_slice(),
                );
                batch.put_cf(
                    audit_cf,
                    UNIFIED_ACCOUNT_STORE_ROCKSDB_KEY_AUDIT_CURSOR,
                    snapshot.flushed_event_count.to_be_bytes(),
                );
                db.write(batch).with_context(|| {
                    format!(
                        "write unified account rocksdb namespace batch failed: {}",
                        path.display()
                    )
                })?;
                Ok(())
            }
        }
    }
}

fn ensure_parent_dir(path: &Path, label: &str) -> Result<()> {
    if let Some(parent) = path.parent() {
        if !parent.as_os_str().is_empty() {
            fs::create_dir_all(parent).with_context(|| {
                format!("create {} parent dir failed: {}", label, parent.display())
            })?;
        }
    }
    Ok(())
}

fn open_unified_account_rocksdb(path: &Path) -> Result<RocksDb> {
    ensure_parent_dir(path, "unified account rocksdb")?;
    let mut opts = RocksDbOptions::default();
    opts.create_if_missing(true);
    opts.create_missing_column_families(true);

    let mut cf_names = match RocksDb::list_cf(&opts, path) {
        Ok(existing) => existing,
        Err(_) => vec!["default".to_string()],
    };
    if cf_names.is_empty() {
        cf_names.push("default".to_string());
    }
    for required in [
        UNIFIED_ACCOUNT_STORE_ROCKSDB_CF_STATE,
        UNIFIED_ACCOUNT_STORE_ROCKSDB_CF_AUDIT,
    ] {
        if !cf_names.iter().any(|name| name == required) {
            cf_names.push(required.to_string());
        }
    }

    RocksDb::open_cf(&opts, path, cf_names)
        .with_context(|| format!("open unified account rocksdb failed: {}", path.display()))
}

fn open_unified_account_audit_rocksdb(path: &Path) -> Result<RocksDb> {
    ensure_parent_dir(path, "unified account audit rocksdb")?;
    let mut opts = RocksDbOptions::default();
    opts.create_if_missing(true);
    RocksDb::open(&opts, path).with_context(|| {
        format!(
            "open unified account audit rocksdb failed: {}",
            path.display()
        )
    })
}

fn empty_unified_account_snapshot() -> UnifiedAccountStoreSnapshot {
    UnifiedAccountStoreSnapshot {
        router: UnifiedAccountRouter::new(),
        flushed_event_count: 0,
    }
}

fn decode_unified_account_snapshot(raw: &[u8], path: &Path) -> Result<UnifiedAccountStoreSnapshot> {
    if raw.is_empty() {
        return Ok(empty_unified_account_snapshot());
    }
    if let Ok(envelope) = crate::bincode_compat::deserialize::<UnifiedAccountStoreEnvelopeV1>(raw) {
        if envelope.version != UNIFIED_ACCOUNT_STORE_ENVELOPE_VERSION {
            bail!(
                "unsupported unified account db version {} at {}",
                envelope.version,
                path.display()
            );
        }
        return Ok(UnifiedAccountStoreSnapshot {
            router: envelope.router,
            flushed_event_count: envelope.flushed_event_count,
        });
    }
    // Backward compatibility: old format stored UnifiedAccountRouter directly.
    let legacy_router: UnifiedAccountRouter = crate::bincode_compat::deserialize(raw)
        .with_context(|| format!("parse unified account db failed: {}", path.display()))?;
    Ok(UnifiedAccountStoreSnapshot {
        flushed_event_count: legacy_router.events().len() as u64,
        router: legacy_router,
    })
}

fn encode_unified_account_snapshot(snapshot: &UnifiedAccountStoreSnapshot) -> Result<Vec<u8>> {
    #[derive(Serialize)]
    struct UnifiedAccountStoreEnvelopeRef<'a> {
        version: u32,
        router: &'a UnifiedAccountRouter,
        flushed_event_count: u64,
    }
    let envelope = UnifiedAccountStoreEnvelopeRef {
        version: UNIFIED_ACCOUNT_STORE_ENVELOPE_VERSION,
        router: &snapshot.router,
        flushed_event_count: snapshot.flushed_event_count,
    };
    crate::bincode_compat::serialize(&envelope).context("serialize unified account router failed")
}

fn unified_account_audit_log_path(query_db_path: &Path) -> PathBuf {
    if let Some(custom) = string_env_nonempty("NOVOVM_UNIFIED_ACCOUNT_AUDIT_LOG") {
        return PathBuf::from(custom);
    }
    if let Some(custom_dir) = string_env_nonempty("NOVOVM_UNIFIED_ACCOUNT_AUDIT_DIR") {
        return PathBuf::from(custom_dir).join(UNIFIED_ACCOUNT_AUDIT_LOG_NAME);
    }
    if let Some(parent) = query_db_path.parent() {
        if !parent.as_os_str().is_empty() {
            return parent
                .join("migration")
                .join("unifiedaccount")
                .join(UNIFIED_ACCOUNT_AUDIT_LOG_NAME);
        }
    }
    PathBuf::from("artifacts/migration/unifiedaccount").join(UNIFIED_ACCOUNT_AUDIT_LOG_NAME)
}

fn unified_account_audit_db_path(query_db_path: &Path) -> PathBuf {
    if let Some(custom) = string_env_nonempty("NOVOVM_UNIFIED_ACCOUNT_AUDIT_DB") {
        return PathBuf::from(custom);
    }
    if let Some(custom_dir) = string_env_nonempty("NOVOVM_UNIFIED_ACCOUNT_AUDIT_DIR") {
        return PathBuf::from(custom_dir).join(UNIFIED_ACCOUNT_AUDIT_DB_NAME);
    }
    if let Some(parent) = query_db_path.parent() {
        if !parent.as_os_str().is_empty() {
            return parent
                .join("migration")
                .join("unifiedaccount")
                .join(UNIFIED_ACCOUNT_AUDIT_DB_NAME);
        }
    }
    PathBuf::from("artifacts/migration/unifiedaccount").join(UNIFIED_ACCOUNT_AUDIT_DB_NAME)
}

fn unified_account_audit_backend_kind() -> String {
    string_env(
        "NOVOVM_UNIFIED_ACCOUNT_AUDIT_BACKEND",
        UNIFIED_ACCOUNT_AUDIT_BACKEND_ROCKSDB,
    )
    .trim()
    .to_ascii_lowercase()
}

fn unified_account_audit_path_for_backend(query_db_path: &Path, backend: &str) -> PathBuf {
    match backend {
        UNIFIED_ACCOUNT_AUDIT_BACKEND_ROCKSDB => unified_account_audit_db_path(query_db_path),
        _ => unified_account_audit_log_path(query_db_path),
    }
}

fn resolve_unified_account_audit_sink_with_backend(
    query_db_path: &Path,
    backend: &str,
) -> Result<UnifiedAccountAuditSinkBackend> {
    let path = unified_account_audit_path_for_backend(query_db_path, &backend);
    match backend {
        "rocksdb" => Ok(UnifiedAccountAuditSinkBackend::RocksDb { path }),
        "jsonl" | "file" => Ok(UnifiedAccountAuditSinkBackend::JsonlFile { path }),
        _ => bail!(
            "invalid NOVOVM_UNIFIED_ACCOUNT_AUDIT_BACKEND={}; valid: rocksdb|jsonl|file",
            backend
        ),
    }
}

fn resolve_unified_account_audit_sink(
    query_db_path: &Path,
) -> Result<UnifiedAccountAuditSinkBackend> {
    let backend = unified_account_audit_backend_kind();
    if backend != UNIFIED_ACCOUNT_AUDIT_BACKEND_ROCKSDB
        && !bool_env("NOVOVM_ALLOW_NON_PROD_UA_BACKEND", false)
    {
        bail!(
            "NOVOVM_UNIFIED_ACCOUNT_AUDIT_BACKEND={} is non-production; use rocksdb or set NOVOVM_ALLOW_NON_PROD_UA_BACKEND=1 for explicit override",
            backend
        );
    }
    resolve_unified_account_audit_sink_with_backend(query_db_path, &backend)
}

impl UnifiedAccountAuditSinkBackend {
    fn backend_name(&self) -> &'static str {
        match self {
            UnifiedAccountAuditSinkBackend::JsonlFile { .. } => UNIFIED_ACCOUNT_AUDIT_BACKEND_JSONL,
            UnifiedAccountAuditSinkBackend::RocksDb { .. } => UNIFIED_ACCOUNT_AUDIT_BACKEND_ROCKSDB,
        }
    }

    fn path(&self) -> &Path {
        match self {
            UnifiedAccountAuditSinkBackend::JsonlFile { path } => path.as_path(),
            UnifiedAccountAuditSinkBackend::RocksDb { path } => path.as_path(),
        }
    }

    fn append_record(&self, record: &UnifiedAccountAuditSinkRecord) -> Result<()> {
        match self {
            UnifiedAccountAuditSinkBackend::JsonlFile { path } => {
                append_unified_account_audit_record(path, record)
            }
            UnifiedAccountAuditSinkBackend::RocksDb { path } => {
                append_unified_account_audit_record_rocksdb(path, record)
            }
        }
    }
}

fn decode_unified_account_audit_record_json(
    raw: &[u8],
    path: &Path,
    seq: u64,
) -> Result<UnifiedAccountAuditSinkRecord> {
    serde_json::from_slice(raw).with_context(|| {
        format!(
            "decode unified account audit record failed: path={} seq={}",
            path.display(),
            seq
        )
    })
}

fn unified_account_audit_record_to_json(
    seq: u64,
    record: &UnifiedAccountAuditSinkRecord,
) -> Result<serde_json::Value> {
    let router_events_json = record
        .router_events
        .iter()
        .map(account_audit_event_to_json)
        .collect::<Result<Vec<_>>>()?;
    let mut value = serde_json::to_value(record)
        .context("serialize unified account audit record to json failed")?;
    if let serde_json::Value::Object(map) = &mut value {
        map.insert("seq".to_string(), serde_json::json!(seq));
        map.insert(
            "router_events".to_string(),
            serde_json::Value::Array(router_events_json),
        );
        return Ok(value);
    }
    Ok(serde_json::json!({
        "seq": seq,
        "router_events": router_events_json,
        "record": value,
    }))
}

fn unified_account_audit_rocksdb_head_seq(path: &Path) -> Result<u64> {
    let db = open_unified_account_audit_rocksdb(path)?;
    let raw = db
        .get(UNIFIED_ACCOUNT_AUDIT_ROCKSDB_KEY_SEQ)
        .with_context(|| {
            format!(
                "read unified account audit rocksdb sequence failed: {}",
                path.display()
            )
        })?;
    match raw {
        Some(bytes) => decode_u64_be(&bytes).with_context(|| {
            format!(
                "decode unified account audit rocksdb sequence failed: {}",
                path.display()
            )
        }),
        None => Ok(0),
    }
}

fn unified_account_audit_jsonl_head_seq(path: &Path) -> Result<u64> {
    if !path.exists() {
        return Ok(0);
    }
    let file = fs::File::open(path).with_context(|| {
        format!(
            "open unified account audit jsonl failed: {}",
            path.display()
        )
    })?;
    let reader = BufReader::new(file);
    let mut seq = 0u64;
    for line in reader.lines() {
        let line = line.with_context(|| {
            format!(
                "read unified account audit jsonl line failed: {}",
                path.display()
            )
        })?;
        if line.trim().is_empty() {
            continue;
        }
        seq = seq.saturating_add(1);
    }
    Ok(seq)
}

fn unified_account_audit_sink_head_seq(sink: &UnifiedAccountAuditSinkBackend) -> Result<u64> {
    match sink {
        UnifiedAccountAuditSinkBackend::JsonlFile { path } => {
            unified_account_audit_jsonl_head_seq(path)
        }
        UnifiedAccountAuditSinkBackend::RocksDb { path } => {
            unified_account_audit_rocksdb_head_seq(path)
        }
    }
}

fn load_unified_account_audit_records_all(
    sink: &UnifiedAccountAuditSinkBackend,
) -> Result<Vec<(u64, UnifiedAccountAuditSinkRecord)>> {
    match sink {
        UnifiedAccountAuditSinkBackend::JsonlFile { path } => {
            if !path.exists() {
                return Ok(Vec::new());
            }
            let file = fs::File::open(path).with_context(|| {
                format!(
                    "open unified account audit jsonl for full load failed: {}",
                    path.display()
                )
            })?;
            let reader = BufReader::new(file);
            let mut seq = 0u64;
            let mut out = Vec::new();
            for line in reader.lines() {
                let line = line.with_context(|| {
                    format!(
                        "read unified account audit jsonl line failed: {}",
                        path.display()
                    )
                })?;
                if line.trim().is_empty() {
                    continue;
                }
                seq = seq.saturating_add(1);
                let record = decode_unified_account_audit_record_json(line.as_bytes(), path, seq)?;
                out.push((seq, record));
            }
            Ok(out)
        }
        UnifiedAccountAuditSinkBackend::RocksDb { path } => {
            let db = open_unified_account_audit_rocksdb(path)?;
            let head_seq =
                match db
                    .get(UNIFIED_ACCOUNT_AUDIT_ROCKSDB_KEY_SEQ)
                    .with_context(|| {
                        format!(
                            "read unified account audit rocksdb sequence failed: {}",
                            path.display()
                        )
                    })? {
                    Some(bytes) => decode_u64_be(&bytes).with_context(|| {
                        format!(
                            "decode unified account audit rocksdb sequence failed: {}",
                            path.display()
                        )
                    })?,
                    None => 0,
                };
            if head_seq == 0 {
                return Ok(Vec::new());
            }
            let mut out = Vec::new();
            for seq in 1..=head_seq {
                let key = unified_account_audit_rocksdb_event_key(seq);
                if let Some(raw) = db.get(&key).with_context(|| {
                    format!(
                        "read unified account audit rocksdb event failed: {} seq={}",
                        path.display(),
                        seq
                    )
                })? {
                    let record = decode_unified_account_audit_record_json(&raw, path, seq)?;
                    out.push((seq, record));
                }
            }
            Ok(out)
        }
    }
}

#[derive(Debug, Clone, Default)]
struct UnifiedAccountAuditQueryFilter {
    method: Option<String>,
    source: Option<String>,
    success: Option<bool>,
    uca_id: Option<String>,
    event_kind: Option<String>,
    role: Option<AccountRole>,
    kyc_provided: Option<bool>,
    kyc_verified: Option<bool>,
}

impl UnifiedAccountAuditQueryFilter {
    fn from_rpc_params(params: &serde_json::Value) -> Result<Self> {
        let method = param_as_string(params, "filter_method")
            .map(|v| v.trim().to_string())
            .filter(|v| !v.is_empty());
        let source = param_as_string(params, "filter_source")
            .map(|v| v.trim().to_string())
            .filter(|v| !v.is_empty());
        let success = param_as_bool(params, "filter_success");
        let uca_id = param_as_string(params, "filter_uca_id")
            .map(|v| v.trim().to_string())
            .filter(|v| !v.is_empty());
        let event_kind = param_as_string(params, "filter_event")
            .or_else(|| param_as_string(params, "filter_event_kind"))
            .map(|v| normalize_audit_filter_token(v.trim()))
            .filter(|v| !v.is_empty());
        let role = match param_as_string(params, "filter_role")
            .map(|v| v.trim().to_ascii_lowercase())
            .filter(|v| !v.is_empty())
        {
            Some(raw) => Some(match raw.as_str() {
                "owner" => AccountRole::Owner,
                "delegate" => AccountRole::Delegate,
                "session" | "sessionkey" | "session_key" => AccountRole::SessionKey,
                _ => bail!(
                    "invalid filter_role: {}; valid: owner|delegate|session_key",
                    raw
                ),
            }),
            None => None,
        };
        let kyc_provided = param_as_bool(params, "filter_kyc_provided");
        let kyc_verified = param_as_bool(params, "filter_kyc_verified");
        Ok(Self {
            method,
            source,
            success,
            uca_id,
            event_kind,
            role,
            kyc_provided,
            kyc_verified,
        })
    }

    fn matches(&self, record: &UnifiedAccountAuditSinkRecord) -> bool {
        if let Some(method) = &self.method {
            if !record.method.eq_ignore_ascii_case(method) {
                return false;
            }
        }
        if let Some(source) = &self.source {
            if !record.source.eq_ignore_ascii_case(source) {
                return false;
            }
        }
        if let Some(success) = self.success {
            if record.success != success {
                return false;
            }
        }
        if self.requires_event_match()
            && !record
                .router_events
                .iter()
                .any(|event| self.matches_event(event))
        {
            return false;
        }
        true
    }

    fn requires_event_match(&self) -> bool {
        self.uca_id.is_some()
            || self.event_kind.is_some()
            || self.role.is_some()
            || self.kyc_provided.is_some()
            || self.kyc_verified.is_some()
    }

    fn matches_router_event(&self, event: &AccountAuditEvent) -> bool {
        self.matches_event(event)
    }

    fn matches_event(&self, event: &AccountAuditEvent) -> bool {
        if let Some(uca_id) = &self.uca_id {
            if !account_audit_event_matches_uca_id(event, uca_id) {
                return false;
            }
        }
        if let Some(kind) = &self.event_kind {
            if normalize_audit_filter_token(account_audit_event_kind(event)) != *kind {
                return false;
            }
        }
        if let Some(role) = self.role {
            if account_audit_event_role(event) != Some(role) {
                return false;
            }
        }
        if let Some(provided) = self.kyc_provided {
            if account_audit_event_kyc_provided(event) != Some(provided) {
                return false;
            }
        }
        if let Some(verified) = self.kyc_verified {
            if account_audit_event_kyc_verified(event) != Some(verified) {
                return false;
            }
        }
        true
    }

    fn to_json(&self) -> serde_json::Value {
        let role = self.role.map(|role| match role {
            AccountRole::Owner => "owner",
            AccountRole::Delegate => "delegate",
            AccountRole::SessionKey => "session_key",
        });
        serde_json::json!({
            "method": self.method,
            "source": self.source,
            "success": self.success,
            "uca_id": self.uca_id,
            "event_kind": self.event_kind,
            "role": role,
            "kyc_provided": self.kyc_provided,
            "kyc_verified": self.kyc_verified,
        })
    }
}

fn normalize_audit_filter_token(raw: &str) -> String {
    raw.chars()
        .filter(|ch| ch.is_ascii_alphanumeric())
        .map(|ch| ch.to_ascii_lowercase())
        .collect()
}

fn account_audit_event_kind(event: &AccountAuditEvent) -> &'static str {
    match event {
        AccountAuditEvent::UcaCreated { .. } => "uca_created",
        AccountAuditEvent::BindingAdded { .. } => "binding_added",
        AccountAuditEvent::BindingConflictRejected { .. } => "binding_conflict_rejected",
        AccountAuditEvent::BindingRevoked { .. } => "binding_revoked",
        AccountAuditEvent::NonceReplayRejected { .. } => "nonce_replay_rejected",
        AccountAuditEvent::DomainMismatchRejected { .. } => "domain_mismatch_rejected",
        AccountAuditEvent::PermissionDenied { .. } => "permission_denied",
        AccountAuditEvent::KeyRotated { .. } => "key_rotated",
        AccountAuditEvent::SessionKeyExpired { .. } => "session_key_expired",
        AccountAuditEvent::Type4PolicyRejected { .. } => "type4_policy_rejected",
        AccountAuditEvent::Type4PolicyDegraded { .. } => "type4_policy_degraded",
        AccountAuditEvent::KycAttestationObserved { .. } => "kyc_attestation_observed",
        AccountAuditEvent::KycPolicyRejected { .. } => "kyc_policy_rejected",
    }
}

fn account_audit_event_code(event: &AccountAuditEvent) -> &'static str {
    match event {
        AccountAuditEvent::UcaCreated { .. } => "UA_AUDIT_UCA_CREATED",
        AccountAuditEvent::BindingAdded { .. } => "UA_AUDIT_BINDING_ADDED",
        AccountAuditEvent::BindingConflictRejected { .. } => "UA_AUDIT_BINDING_CONFLICT_REJECTED",
        AccountAuditEvent::BindingRevoked { .. } => "UA_AUDIT_BINDING_REVOKED",
        AccountAuditEvent::NonceReplayRejected { .. } => "UA_AUDIT_NONCE_REPLAY_REJECTED",
        AccountAuditEvent::DomainMismatchRejected { .. } => "UA_AUDIT_DOMAIN_MISMATCH_REJECTED",
        AccountAuditEvent::PermissionDenied { .. } => "UA_AUDIT_PERMISSION_DENIED",
        AccountAuditEvent::KeyRotated { .. } => "UA_AUDIT_KEY_ROTATED",
        AccountAuditEvent::SessionKeyExpired { .. } => "UA_AUDIT_SESSION_KEY_EXPIRED",
        AccountAuditEvent::Type4PolicyRejected { .. } => "UA_AUDIT_TYPE4_POLICY_REJECTED",
        AccountAuditEvent::Type4PolicyDegraded { .. } => "UA_AUDIT_TYPE4_POLICY_DEGRADED",
        AccountAuditEvent::KycAttestationObserved { .. } => "UA_AUDIT_KYC_ATTESTATION_OBSERVED",
        AccountAuditEvent::KycPolicyRejected { .. } => "UA_AUDIT_KYC_POLICY_REJECTED",
    }
}

fn account_audit_event_to_json(event: &AccountAuditEvent) -> Result<serde_json::Value> {
    let mut value = serde_json::to_value(event)
        .context("serialize unified account audit event to json failed")?;
    if let serde_json::Value::Object(map) = &mut value {
        map.insert(
            "event_kind".to_string(),
            serde_json::json!(account_audit_event_kind(event)),
        );
        map.insert(
            "event_code".to_string(),
            serde_json::json!(account_audit_event_code(event)),
        );
        return Ok(value);
    }
    Ok(serde_json::json!({
        "event_kind": account_audit_event_kind(event),
        "event_code": account_audit_event_code(event),
        "event": value,
    }))
}

fn account_audit_event_matches_uca_id(event: &AccountAuditEvent, uca_id: &str) -> bool {
    match event {
        AccountAuditEvent::UcaCreated { uca_id: event_uca_id, .. } => {
            event_uca_id.eq_ignore_ascii_case(uca_id)
        }
        AccountAuditEvent::BindingAdded { uca_id: event_uca_id, .. } => {
            event_uca_id.eq_ignore_ascii_case(uca_id)
        }
        AccountAuditEvent::BindingConflictRejected {
            request_uca_id,
            existing_uca_id,
            ..
        } => {
            request_uca_id.eq_ignore_ascii_case(uca_id)
                || existing_uca_id.eq_ignore_ascii_case(uca_id)
        }
        AccountAuditEvent::BindingRevoked { uca_id: event_uca_id, .. } => {
            event_uca_id.eq_ignore_ascii_case(uca_id)
        }
        AccountAuditEvent::NonceReplayRejected { uca_id: event_uca_id, .. } => {
            event_uca_id.eq_ignore_ascii_case(uca_id)
        }
        AccountAuditEvent::DomainMismatchRejected { uca_id: event_uca_id, .. } => {
            event_uca_id.eq_ignore_ascii_case(uca_id)
        }
        AccountAuditEvent::PermissionDenied { uca_id: event_uca_id, .. } => {
            event_uca_id.eq_ignore_ascii_case(uca_id)
        }
        AccountAuditEvent::KeyRotated { uca_id: event_uca_id, .. } => {
            event_uca_id.eq_ignore_ascii_case(uca_id)
        }
        AccountAuditEvent::SessionKeyExpired { uca_id: event_uca_id, .. } => {
            event_uca_id.eq_ignore_ascii_case(uca_id)
        }
        AccountAuditEvent::Type4PolicyRejected { uca_id: event_uca_id, .. } => {
            event_uca_id.eq_ignore_ascii_case(uca_id)
        }
        AccountAuditEvent::Type4PolicyDegraded { uca_id: event_uca_id, .. } => {
            event_uca_id.eq_ignore_ascii_case(uca_id)
        }
        AccountAuditEvent::KycAttestationObserved { uca_id: event_uca_id, .. } => {
            event_uca_id.eq_ignore_ascii_case(uca_id)
        }
        AccountAuditEvent::KycPolicyRejected { uca_id: event_uca_id, .. } => {
            event_uca_id.eq_ignore_ascii_case(uca_id)
        }
    }
}

fn account_audit_event_role(event: &AccountAuditEvent) -> Option<AccountRole> {
    match event {
        AccountAuditEvent::PermissionDenied { role, .. }
        | AccountAuditEvent::Type4PolicyRejected { role, .. }
        | AccountAuditEvent::Type4PolicyDegraded { role, .. }
        | AccountAuditEvent::KycAttestationObserved { role, .. }
        | AccountAuditEvent::KycPolicyRejected { role, .. } => Some(*role),
        _ => None,
    }
}

fn account_audit_event_kyc_provided(event: &AccountAuditEvent) -> Option<bool> {
    match event {
        AccountAuditEvent::KycAttestationObserved { provided, .. }
        | AccountAuditEvent::KycPolicyRejected { provided, .. } => Some(*provided),
        _ => None,
    }
}

fn account_audit_event_kyc_verified(event: &AccountAuditEvent) -> Option<bool> {
    match event {
        AccountAuditEvent::KycAttestationObserved { verified, .. } => Some(*verified),
        AccountAuditEvent::KycPolicyRejected { .. } => Some(false),
        _ => None,
    }
}

fn load_unified_account_audit_records_for_rpc(
    sink: &UnifiedAccountAuditSinkBackend,
    since_seq: u64,
    limit: usize,
    filter: &UnifiedAccountAuditQueryFilter,
) -> Result<(u64, Vec<serde_json::Value>, u64, bool)> {
    let records = load_unified_account_audit_records_all(sink)?;
    let head_seq = records.last().map(|(seq, _)| *seq).unwrap_or(0);
    let mut filtered = Vec::new();
    for (seq, record) in records {
        if seq <= since_seq {
            continue;
        }
        if !filter.matches(&record) {
            continue;
        }
        filtered.push((seq, unified_account_audit_record_to_json(seq, &record)?));
    }
    let has_more = filtered.len() > limit;
    if has_more {
        filtered.truncate(limit);
    }
    let next_since_seq = filtered.last().map(|(seq, _)| *seq).unwrap_or(since_seq);
    let events = filtered.into_iter().map(|(_, event)| event).collect();
    Ok((head_seq, events, next_since_seq, has_more))
}

fn migrate_unified_account_audit_records(
    source: &UnifiedAccountAuditSinkBackend,
    target: &UnifiedAccountAuditSinkBackend,
) -> Result<(u64, u64, u64, u64)> {
    let source_norm = absolute_normalized_path(source.path())?;
    let target_norm = absolute_normalized_path(target.path())?;
    if source.backend_name() == target.backend_name() && source_norm == target_norm {
        bail!(
            "invalid ua audit migrate config: source and target are the same sink ({}, {})",
            source.backend_name(),
            source_norm.display()
        );
    }
    let source_records = load_unified_account_audit_records_all(source)?;
    let source_head = source_records.last().map(|(seq, _)| *seq).unwrap_or(0);
    let target_head_before = unified_account_audit_sink_head_seq(target)?;
    let mut appended = 0u64;
    for (seq, record) in source_records {
        if seq <= target_head_before {
            continue;
        }
        target.append_record(&record)?;
        appended = appended.saturating_add(1);
    }
    let target_head_after = unified_account_audit_sink_head_seq(target)?;
    Ok((source_head, target_head_before, appended, target_head_after))
}

fn is_unified_account_eth_route_method(method: &str) -> bool {
    method == "eth_sendRawTransaction" || method == "eth_sendTransaction"
}

fn is_unified_account_eth_persona_query_method(method: &str) -> bool {
    method == "eth_getTransactionCount"
}

fn is_evm_control_namespace(method: &str) -> bool {
    method.starts_with("engine_")
        || method.starts_with("admin_")
        || method.starts_with("debug_")
        || method.starts_with("miner_")
        || method.starts_with("personal_")
        || method.starts_with("clique_")
        || method.starts_with("parity_")
}

fn novovm_public_rpc_surface_map_json() -> serde_json::Value {
    serde_json::json!({
        "host_chain": "supervm_mainnet",
        "evm_plugin_enabled": true,
        "mode": "host_chain_plus_plugin",
        "domains": [
            {
                "domain": "novovm_mainnet",
                "scope": "native",
                "entry_methods": [
                    "novovm_getSurfaceMap",
                    "novovm_getMethodDomain",
                    "nov_getBlock",
                    "nov_getTransaction",
                    "nov_getReceipt",
                    "nov_getTransactionReceipt",
                    "nov_getBalance",
                    "nov_getAssetBalance",
                    "nov_getState",
                    "nov_getModuleInfo",
                    "nov_getTreasurySettlementSummary",
                    "nov_getTreasurySettlementJournal",
                    "nov_getTreasurySettlementPolicy",
                    "nov_call",
                    "nov_estimate",
                    "nov_estimateGas",
                    "nov_execute",
                    "nov_sendTransaction",
                    "nov_sendRawTransaction",
                    "getBlock",
                    "getTransaction",
                    "getReceipt",
                    "getBalance",
                    "ua_createUca",
                    "ua_rotatePrimaryKey",
                    "ua_bindPersona",
                    "ua_setPolicy",
                    "web30_sendTransaction",
                    "web30_sendRawTransaction"
                ]
            },
            {
                "domain": "evm_plugin",
                "scope": "compatibility",
                "entry_methods": [
                    "eth_chainId",
                    "net_version",
                    "web3_clientVersion",
                    "eth_blockNumber",
                    "eth_getBlockByNumber",
                    "eth_getBalance",
                    "eth_getCode",
                    "eth_getStorageAt",
                    "eth_call",
                    "eth_estimateGas",
                    "eth_gasPrice",
                    "eth_maxPriorityFeePerGas",
                    "eth_feeHistory",
                    "eth_getTransactionByHash",
                    "eth_getTransactionReceipt",
                    "eth_sendRawTransaction",
                    "eth_sendTransaction"
                ]
            }
        ],
        "notes": [
            "supervm mainnet remains the single host chain",
            "eth_* namespace is compatibility surface provided by evm plugin"
        ]
    })
}

fn novovm_public_rpc_method_domain(method: &str) -> &'static str {
    if method.starts_with("ua_")
        || method.starts_with("web30_")
        || method.starts_with("nov_")
        || method.starts_with("novovm_")
        || method.starts_with("governance_")
        || matches!(
            method,
            "getBlock" | "getTransaction" | "getReceipt" | "getBalance"
        )
    {
        "novovm_mainnet"
    } else if method.starts_with("eth_")
        || method.starts_with("evm_")
        || method.starts_with("txpool_")
        || method.starts_with("net_")
        || method.starts_with("web3_")
        || is_evm_control_namespace(method)
    {
        "evm_plugin"
    } else {
        "unknown"
    }
}

fn novovm_public_rpc_method_domain_json(method: &str) -> serde_json::Value {
    serde_json::json!({
        "host_chain": "supervm_mainnet",
        "method": method,
        "domain": novovm_public_rpc_method_domain(method),
        "control_namespace_disabled": is_evm_control_namespace(method),
    })
}

fn is_eth_plugin_allowed_method(method: &str) -> bool {
    is_unified_account_eth_route_method(method)
        || is_unified_account_eth_persona_query_method(method)
        || matches!(
            method,
            "eth_chainId"
                | "eth_getTransactionByHash"
                | "eth_getTransactionReceipt"
                | "eth_blockNumber"
                | "eth_getBlockByNumber"
                | "eth_getBalance"
                | "eth_getCode"
                | "eth_getStorageAt"
                | "eth_call"
                | "eth_estimateGas"
                | "eth_gasPrice"
                | "eth_maxPriorityFeePerGas"
                | "eth_feeHistory"
        )
}

fn is_unified_account_web30_route_method(method: &str) -> bool {
    method == "web30_sendTransaction" || method == "web30_sendRawTransaction"
}

fn is_unified_account_nov_route_method(method: &str) -> bool {
    method == "nov_sendTransaction" || method == "nov_sendRawTransaction" || method == "nov_execute"
}

fn is_eth_filter_or_reorg_method_m0(method: &str) -> bool {
    matches!(
        method,
        "eth_newFilter"
            | "eth_newBlockFilter"
            | "eth_newPendingTransactionFilter"
            | "eth_getFilterChanges"
            | "eth_getFilterLogs"
            | "eth_uninstallFilter"
            | "eth_getLogs"
            | "eth_subscribe"
            | "eth_unsubscribe"
    )
}

fn is_unified_account_method(method: &str) -> bool {
    method.starts_with("ua_")
        || is_unified_account_eth_route_method(method)
        || is_unified_account_eth_persona_query_method(method)
        || is_unified_account_web30_route_method(method)
        || is_unified_account_nov_route_method(method)
}

fn public_rpc_error_code_for_method(method: &str, message: &str) -> i64 {
    if is_unified_account_eth_route_method(method) {
        if message.contains("unsupported eth tx type: blob") {
            return -32031;
        }
        if message.contains("unsupported typed tx envelope")
            || message.contains("invalid tx envelope prefix")
            || message.contains("raw tx is empty")
        {
            return -32032;
        }
        if message.contains("chain_id mismatch")
            || message.contains("nonce mismatch")
            || message.contains("tx_type mismatch")
            || message.contains("kyc_verified")
            || message.contains("kyc_attestor_pubkey")
            || message.contains("kyc_attestation_sig")
            || message.contains("kyc attestation signature")
        {
            return -32033;
        }
        if message.contains("intrinsic gas too low") {
            return -32034;
        }
        if message.contains("type4 transaction cannot be used")
            || message.contains("ERR_UNSUPPORTED_TX_TYPE_4")
            || message.contains("ERR_TYPE4_ROLE_MIX_FORBIDDEN")
            || message.contains("ERR_KYC_POLICY_FORBIDDEN_NON_OWNER")
        {
            return -32035;
        }
    }
    if method.starts_with("eth_") && message.contains("unsupported eth filter/reorg method in M0")
    {
        return -32036;
    }
    if method.starts_with("eth_")
        && message.contains("eth method not enabled in supervm public rpc plugin scope")
    {
        return -32601;
    }
    -32602
}

fn unified_account_events_since(
    router: &UnifiedAccountRouter,
    cursor: u64,
) -> (Vec<AccountAuditEvent>, u64) {
    let events = router.events();
    let start = (cursor as usize).min(events.len());
    let out = events[start..].to_vec();
    (out, events.len() as u64)
}

fn append_unified_account_audit_record(
    path: &Path,
    record: &UnifiedAccountAuditSinkRecord,
) -> Result<()> {
    if let Some(parent) = path.parent() {
        if !parent.as_os_str().is_empty() {
            fs::create_dir_all(parent).with_context(|| {
                format!(
                    "create unified account audit parent dir failed: {}",
                    parent.display()
                )
            })?;
        }
    }
    let mut file = OpenOptions::new()
        .create(true)
        .append(true)
        .open(path)
        .with_context(|| format!("open unified account audit log failed: {}", path.display()))?;
    let encoded =
        serde_json::to_string(record).context("serialize unified account audit record")?;
    writeln!(file, "{encoded}")
        .with_context(|| format!("write unified account audit log failed: {}", path.display()))?;
    Ok(())
}

fn decode_u64_be(bytes: &[u8]) -> Result<u64> {
    if bytes.len() != 8 {
        bail!("invalid u64 bytes length: expected 8, got {}", bytes.len());
    }
    let mut out = [0u8; 8];
    out.copy_from_slice(bytes);
    Ok(u64::from_be_bytes(out))
}

fn unified_account_audit_rocksdb_event_key(seq: u64) -> Vec<u8> {
    let mut out = Vec::with_capacity(UNIFIED_ACCOUNT_AUDIT_ROCKSDB_KEY_EVENT_PREFIX.len() + 8);
    out.extend_from_slice(UNIFIED_ACCOUNT_AUDIT_ROCKSDB_KEY_EVENT_PREFIX);
    out.extend_from_slice(&seq.to_be_bytes());
    out
}

fn append_unified_account_audit_record_rocksdb(
    path: &Path,
    record: &UnifiedAccountAuditSinkRecord,
) -> Result<()> {
    let db = open_unified_account_audit_rocksdb(path)?;
    let current_seq = match db
        .get(UNIFIED_ACCOUNT_AUDIT_ROCKSDB_KEY_SEQ)
        .with_context(|| {
            format!(
                "read unified account audit rocksdb sequence failed: {}",
                path.display()
            )
        })? {
        Some(raw) => decode_u64_be(&raw).with_context(|| {
            format!(
                "decode unified account audit rocksdb sequence failed: {}",
                path.display()
            )
        })?,
        None => 0,
    };
    let next_seq = current_seq.saturating_add(1);
    let event_key = unified_account_audit_rocksdb_event_key(next_seq);
    let event_value = serde_json::to_vec(record)
        .context("serialize unified account audit record for rocksdb failed")?;
    let mut batch = RocksDbWriteBatch::default();
    batch.put(
        UNIFIED_ACCOUNT_AUDIT_ROCKSDB_KEY_SEQ,
        next_seq.to_be_bytes(),
    );
    batch.put(event_key, event_value);
    db.write(batch).with_context(|| {
        format!(
            "write unified account audit rocksdb batch failed: {}",
            path.display()
        )
    })?;
    Ok(())
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

fn local_tx_uca_id(account: u64) -> String {
    format!("{LOCAL_TX_UCA_ID_PREFIX}{account}")
}

fn local_tx_uca_primary_key_ref(account: u64) -> Vec<u8> {
    let mut hasher = Sha256::new();
    hasher.update(LOCAL_TX_UCA_PRIMARY_KEY_DOMAIN);
    hasher.update(account.to_le_bytes());
    hasher.finalize().to_vec()
}

fn local_tx_persona(tx: &LocalTx, chain_id: u64) -> PersonaAddress {
    PersonaAddress {
        persona_type: PersonaType::Evm,
        chain_id,
        external_address: encode_adapter_address(tx.account),
    }
}

fn route_local_txs_through_unified_account(
    txs: &[LocalTx],
    router: &mut UnifiedAccountRouter,
    chain_id: u64,
    signature_domain: &str,
    now: u64,
    auto_provision: bool,
) -> Result<UnifiedAccountExecGuardSummary> {
    if txs.is_empty() {
        bail!("unified account execution guard requires at least one tx");
    }

    let mut summary = UnifiedAccountExecGuardSummary::default();
    for tx in txs {
        summary.checked = summary.checked.saturating_add(1);
        let uca_id = local_tx_uca_id(tx.account);
        let persona = local_tx_persona(tx, chain_id);

        if auto_provision {
            match router.create_uca(
                uca_id.clone(),
                local_tx_uca_primary_key_ref(tx.account),
                now,
            ) {
                Ok(()) => {
                    summary.created_ucas = summary.created_ucas.saturating_add(1);
                }
                Err(UnifiedAccountError::UcaAlreadyExists { .. }) => {}
                Err(err) => {
                    bail!(
                        "unified account auto-provision create failed (account={}, uca_id={}): {}",
                        tx.account,
                        uca_id,
                        err
                    );
                }
            }
            match router.add_binding(&uca_id, AccountRole::Owner, persona.clone(), now) {
                Ok(()) => {
                    summary.added_bindings = summary.added_bindings.saturating_add(1);
                }
                Err(UnifiedAccountError::BindingAlreadyExists) => {}
                Err(err) => {
                    bail!(
                        "unified account auto-provision bind failed (account={}, uca_id={}, chain_id={}): {}",
                        tx.account,
                        uca_id,
                        chain_id,
                        err
                    );
                }
            }
        }

        let request = RouteRequest {
            uca_id,
            persona,
            role: AccountRole::Owner,
            protocol: ProtocolKind::Eth,
            signature_domain: signature_domain.to_string(),
            nonce: tx.nonce,
            kyc_attestation_provided: false,
            kyc_verified: false,
            wants_cross_chain_atomic: false,
            tx_type4: false,
            session_expires_at: None,
            now,
        };
        let decision = router.route(request).map_err(|err| {
            anyhow::anyhow!(
                "unified account execution route rejected (account={}, nonce={}, chain_id={}): {}",
                tx.account,
                tx.nonce,
                chain_id,
                err
            )
        })?;
        summary.routed = summary.routed.saturating_add(1);
        match decision {
            RouteDecision::FastPath => {
                summary.decision_fast_path = summary.decision_fast_path.saturating_add(1);
            }
            RouteDecision::Adapter { .. } => {
                summary.decision_adapter = summary.decision_adapter.saturating_add(1);
            }
        }
    }

    Ok(summary)
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
    let raw = fs::read_to_string(path).with_context(|| {
        format!(
            "read governance chain audit store failed: {}",
            path.display()
        )
    })?;
    let normalized = raw.trim_start_matches('\u{feff}');
    if normalized.trim().is_empty() {
        return Ok(GovernanceChainAuditStore::default());
    }
    let parsed: GovernanceChainAuditStore =
        serde_json::from_str(normalized).with_context(|| {
            format!(
                "parse governance chain audit store failed: {}",
                path.display()
            )
        })?;
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
    fs::write(path, serialized).with_context(|| {
        format!(
            "write governance chain audit store failed: {}",
            path.display()
        )
    })?;
    Ok(())
}

fn append_block_to_query_db(db: &mut QueryStateDb, block: &LocalBlock) -> (String, String) {
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
    (block_hash, state_root)
}

fn query_log_record_from_execution_log(
    block: &LocalBlock,
    block_hash: &str,
    tx_hash: &str,
    log: &SupervmEvmExecutionLogV1,
) -> QueryExecutionLogRecord {
    QueryExecutionLogRecord {
        tx_hash: tx_hash.to_string(),
        block_height: block.header.height,
        block_hash: block_hash.to_string(),
        emitter: to_hex(&log.emitter),
        topics: log.topics.iter().map(|topic| to_hex(topic)).collect(),
        data: to_hex(&log.data),
        tx_index: log.tx_index,
        log_index: log.log_index,
        state_version: log.state_version,
    }
}

fn query_receipt_record_from_execution_receipt(
    block: &LocalBlock,
    block_hash: &str,
    receipt: &SupervmEvmExecutionReceiptV1,
) -> QueryReceiptRecord {
    let tx_hash = to_hex(&receipt.tx_hash);
    let logs = receipt
        .logs
        .iter()
        .map(|log| query_log_record_from_execution_log(block, block_hash, tx_hash.as_str(), log))
        .collect();
    QueryReceiptRecord {
        tx_hash,
        block_height: block.header.height,
        block_hash: block_hash.to_string(),
        success: receipt.status_ok,
        gas_used: receipt.gas_used,
        state_root: to_hex(&receipt.state_root),
        chain_type: receipt.chain_type.as_str().to_string(),
        chain_id: receipt.chain_id,
        tx_type: tx_type_name(receipt.tx_type).to_string(),
        receipt_type: receipt.receipt_type,
        cumulative_gas_used: receipt.cumulative_gas_used,
        effective_gas_price: receipt.effective_gas_price,
        log_bloom: to_hex(&receipt.log_bloom),
        revert_data: receipt.revert_data.as_ref().map(|data| to_hex(data)),
        state_version: receipt.state_version,
        contract_address: receipt
            .contract_address
            .as_ref()
            .map(|address| to_hex(address)),
        logs,
    }
}

fn query_state_mirror_record_from_update(
    block: &LocalBlock,
    block_hash: &str,
    update: &SupervmEvmStateMirrorUpdateV1,
) -> QueryStateMirrorRecord {
    QueryStateMirrorRecord {
        block_height: block.header.height,
        block_hash: block_hash.to_string(),
        chain_type: update.chain_type.as_str().to_string(),
        chain_id: update.chain_id,
        state_version: update.state_version,
        state_root: to_hex(&update.state_root),
        receipt_count: update.receipt_count,
        accepted_receipt_count: update.accepted_receipt_count,
        tx_hashes: update.tx_hashes.iter().map(|tx_hash| to_hex(tx_hash)).collect(),
        imported_at_unix_ms: update.imported_at_unix_ms,
    }
}

fn ingest_canonical_batch_artifacts_v1(
    db: &mut QueryStateDb,
    block: &LocalBlock,
    chain_id: u64,
    artifacts: &CanonicalBatchArtifactsV1,
) {
    let block_hash = to_hex(&block.block_hash);
    let receipt_hashes: HashSet<String> = artifacts
        .execution_receipts
        .iter()
        .map(|receipt| to_hex(&receipt.tx_hash))
        .collect();
    if !receipt_hashes.is_empty() {
        db.logs
            .retain(|record| !receipt_hashes.contains(record.tx_hash.as_str()));
    }

    let flattened_txs: Vec<&LocalTx> = block
        .batches
        .iter()
        .flat_map(|batch| batch.txs.iter())
        .collect();
    let receipt_by_index: HashMap<u32, &SupervmEvmExecutionReceiptV1> = artifacts
        .execution_receipts
        .iter()
        .map(|receipt| (receipt.tx_index, receipt))
        .collect();

    for (idx, tx) in flattened_txs.iter().enumerate() {
        let receipt = receipt_by_index.get(&(idx as u32)).copied();
        let tx_hash = receipt
            .map(|item| to_hex(&item.tx_hash))
            .unwrap_or_else(|| canonical_tx_hash_hex_from_local_tx(tx, chain_id));
        db.balances.insert(tx.account.to_string(), tx.value);
        db.txs.insert(
            tx_hash.clone(),
            QueryTxRecord {
                tx_hash,
                block_height: block.header.height,
                block_hash: block_hash.clone(),
                account: tx.account,
                key: tx.key,
                value: tx.value,
                nonce: tx.nonce,
                fee: tx.fee,
                success: receipt.map(|item| item.status_ok).unwrap_or(true),
            },
        );
    }

    for receipt in &artifacts.execution_receipts {
        let query_receipt = query_receipt_record_from_execution_receipt(block, &block_hash, receipt);
        db.logs.extend(query_receipt.logs.clone());
        db.receipts
            .insert(query_receipt.tx_hash.clone(), query_receipt);
    }

    for update in &artifacts.state_mirror_updates {
        db.state_mirror_updates
            .push(query_state_mirror_record_from_update(block, &block_hash, update));
    }
}

fn apply_block_to_query_db_legacy(db: &mut QueryStateDb, block: &LocalBlock) {
    let (block_hash, state_root) = append_block_to_query_db(db, block);

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
                    chain_type: String::new(),
                    chain_id: 0,
                    tx_type: String::new(),
                    receipt_type: None,
                    cumulative_gas_used: 0,
                    effective_gas_price: None,
                    log_bloom: String::new(),
                    revert_data: None,
                    state_version: 0,
                    contract_address: None,
                    logs: Vec::new(),
                },
            );
        }
    }
}

fn apply_block_to_query_db_v1(
    db: &mut QueryStateDb,
    block: &LocalBlock,
    chain_id: u64,
    canonical_artifacts: Option<&CanonicalBatchArtifactsV1>,
) {
    if let Some(artifacts) = canonical_artifacts {
        let has_canonical = !artifacts.execution_receipts.is_empty()
            || !artifacts.state_mirror_updates.is_empty();
        if has_canonical {
            let _ = append_block_to_query_db(db, block);
            ingest_canonical_batch_artifacts_v1(db, block, chain_id, artifacts);
            return;
        }
    }
    apply_block_to_query_db_legacy(db, block);
}

fn apply_block_to_query_db(db: &mut QueryStateDb, block: &LocalBlock) {
    apply_block_to_query_db_v1(db, block, 1, None);
}

fn persist_query_state_for_block(
    block: &LocalBlock,
    chain_id: u64,
    canonical_artifacts: Option<&CanonicalBatchArtifactsV1>,
) -> Result<(PathBuf, QueryStateDb)> {
    let path = chain_query_db_path();
    let mut db = load_query_state_db(&path)?;
    apply_block_to_query_db_v1(&mut db, block, chain_id, canonical_artifacts);
    save_query_state_db(&path, &db)?;
    Ok((path, db))
}

fn run_batch_a_minimal_closure(
    batches: Vec<LocalBatch>,
    consensus_binding: ConsensusPluginBindingV1,
    execution_state_root: [u8; 32],
    slash_policy: &SlashPolicy,
) -> Result<LocalBlock> {
    // Single-validator closure for Batch A:
    // execute -> proposal -> vote -> QC -> commit.
    if batches.is_empty() {
        bail!("batch_a requires at least one batch");
    }
    let batch_layout = batch_layout_summary(&batches);
    let batch_mapped_ops_total = batches.iter().map(|b| b.mapped_ops as u64).sum::<u64>();
    let expected_txs = batches.iter().map(|b| b.txs.len() as u64).sum::<u64>();

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
    for batch in &batches {
        engine
            .add_batch(batch.id, batch.txs.len() as u64)
            .with_context(|| format!("add batch {} failed", batch.id))?;
    }

    let mut batch_results = HashMap::new();
    for batch in &batches {
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
        batch_layout,
        batch_mapped_ops_total
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
    if closure.txs != expected_txs {
        bail!(
            "batch_a tx mismatch: committed={} expected={}",
            closure.txs,
            expected_txs
        );
    }
    let block = build_local_block_owned(&closure, batches);
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
    chain_id: u64,
    canonical_artifacts: Option<&CanonicalBatchArtifactsV1>,
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

    let (query_db_path, query_db) =
        persist_query_state_for_block(&latest, chain_id, canonical_artifacts)?;
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
        "query_state_out: db={} blocks={} txs={} receipts={} logs={} state_mirror_updates={} balances={}",
        query_db_path.display(),
        query_db.blocks.len(),
        query_db.txs.len(),
        query_db.receipts.len(),
        query_db.logs.len(),
        query_db.state_mirror_updates.len(),
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
        serde_json::Value::String(s) => parse_u64_decimal_or_hex(s),
        _ => None,
    }
}

fn parse_u64_decimal_or_hex(raw: &str) -> Option<u64> {
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return None;
    }
    if let Some(hex) = trimmed
        .strip_prefix("0x")
        .or_else(|| trimmed.strip_prefix("0X"))
    {
        if hex.is_empty() {
            return Some(0);
        }
        u64::from_str_radix(hex, 16).ok()
    } else {
        trimmed.parse::<u64>().ok()
    }
}

fn value_to_u128(v: &serde_json::Value) -> Option<u128> {
    match v {
        serde_json::Value::Number(n) => n.as_u64().map(|v| v as u128),
        serde_json::Value::String(s) => parse_u128_decimal_or_hex(s),
        _ => None,
    }
}

fn parse_u128_decimal_or_hex(raw: &str) -> Option<u128> {
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return None;
    }
    if let Some(hex) = trimmed
        .strip_prefix("0x")
        .or_else(|| trimmed.strip_prefix("0X"))
    {
        if hex.is_empty() {
            return Some(0);
        }
        u128::from_str_radix(hex, 16).ok()
    } else {
        trimmed.parse::<u128>().ok()
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

fn param_as_u128(params: &serde_json::Value, key: &str) -> Option<u128> {
    match params {
        serde_json::Value::Object(map) => map.get(key).and_then(value_to_u128),
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

#[derive(Debug, Clone)]
struct EthRawTxTypeHint {
    raw_tx: Vec<u8>,
    fields: EvmRawTxFieldsM0,
}

fn extract_eth_raw_tx_param(params: &serde_json::Value) -> Option<String> {
    match params {
        serde_json::Value::Object(map) => {
            const CANDIDATE_KEYS: &[&str] = &[
                "raw_tx",
                "rawTransaction",
                "raw_transaction",
                "raw",
                "signed_tx",
            ];
            for key in CANDIDATE_KEYS {
                if let Some(value) = map.get(*key).and_then(value_to_string) {
                    let trimmed = value.trim().to_string();
                    if !trimmed.is_empty() {
                        return Some(trimmed);
                    }
                }
            }
            None
        }
        serde_json::Value::Array(arr) => arr.first().and_then(value_to_string),
        _ => None,
    }
}

fn extract_eth_persona_address_param(params: &serde_json::Value) -> Option<String> {
    match params {
        serde_json::Value::Object(map) => {
            const CANDIDATE_KEYS: &[&str] = &["external_address", "from", "address"];
            for key in CANDIDATE_KEYS {
                if let Some(value) = map.get(*key).and_then(value_to_string) {
                    let trimmed = value.trim().to_string();
                    if !trimmed.is_empty() {
                        return Some(trimmed);
                    }
                }
            }
            None
        }
        serde_json::Value::Array(arr) => arr.first().and_then(value_to_string),
        _ => None,
    }
}

fn infer_eth_raw_tx_type_hint(params: &serde_json::Value) -> Result<Option<EthRawTxTypeHint>> {
    let Some(raw_tx_hex) = extract_eth_raw_tx_param(params) else {
        return Ok(None);
    };
    let raw_tx = decode_eth_send_raw_hex_payload_v1(&raw_tx_hex, "raw_tx")?;
    let fields = translate_raw_evm_tx_fields_m0(&raw_tx)?;
    Ok(Some(EthRawTxTypeHint { raw_tx, fields }))
}

fn param_as_string_any(params: &serde_json::Value, keys: &[&str]) -> Option<String> {
    for key in keys {
        if let Some(value) = param_as_string(params, key) {
            return Some(value);
        }
    }
    None
}

fn param_as_u64_any(params: &serde_json::Value, keys: &[&str]) -> Option<u64> {
    for key in keys {
        if let Some(value) = param_as_u64(params, key) {
            return Some(value);
        }
    }
    None
}

fn param_as_bool_any(params: &serde_json::Value, keys: &[&str]) -> Option<bool> {
    for key in keys {
        if let Some(value) = param_as_bool(params, key) {
            return Some(value);
        }
    }
    None
}

fn param_as_u128_any(params: &serde_json::Value, keys: &[&str]) -> Option<u128> {
    for key in keys {
        if let Some(value) = param_as_u128(params, key) {
            return Some(value);
        }
    }
    None
}

fn ua_route_role_label(role: AccountRole) -> &'static str {
    match role {
        AccountRole::Owner => "owner",
        AccountRole::Delegate => "delegate",
        AccountRole::SessionKey => "session_key",
    }
}

fn ua_kyc_attestation_message(
    uca_id: &str,
    chain_id: u64,
    external_address: &[u8],
    role: AccountRole,
    nonce: u64,
) -> String {
    format!(
        "novovm.ua.kyc.v1|uca_id={}|chain_id={}|address=0x{}|role={}|nonce={}",
        uca_id,
        chain_id,
        to_hex(external_address),
        ua_route_role_label(role),
        nonce
    )
}

fn parse_ua_kyc_attestor_allowlist() -> Vec<[u8; 32]> {
    let Some(raw) = string_env_nonempty("NOVOVM_UA_KYC_ATTESTOR_PUBKEYS") else {
        return Vec::new();
    };
    raw.split(',')
        .filter_map(|token| {
            let trimmed = token.trim();
            if trimmed.is_empty() {
                return None;
            }
            let decoded = decode_hex_bytes(trimmed, "NOVOVM_UA_KYC_ATTESTOR_PUBKEYS").ok()?;
            if decoded.len() != 32 {
                return None;
            }
            let mut out = [0u8; 32];
            out.copy_from_slice(&decoded);
            Some(out)
        })
        .collect()
}

#[derive(Debug, Clone, Copy)]
struct NodeKycVerificationOutcome {
    provided: bool,
    verified: bool,
}

fn resolve_node_kyc_verification(
    params: &serde_json::Value,
    uca_id: &str,
    chain_id: u64,
    external_address: &[u8],
    role: AccountRole,
    nonce: u64,
) -> Result<NodeKycVerificationOutcome> {
    let explicit_bypass = param_as_bool_any(params, &["kyc_verified", "kycVerified"]).unwrap_or(false);
    let attestor_pubkey_hex = param_as_string_any(
        params,
        &[
            "kyc_attestor_pubkey",
            "kycAttestorPubkey",
            "kyc_proof_pubkey",
            "kycProofPubkey",
        ],
    );
    let attestation_sig_hex = param_as_string_any(
        params,
        &[
            "kyc_attestation_sig",
            "kycAttestationSig",
            "kyc_proof_sig",
            "kycProofSig",
        ],
    );
    if attestor_pubkey_hex.is_none() && attestation_sig_hex.is_none() {
        if explicit_bypass {
            bail!(
                "kyc_verified boolean bypass is disabled; provide kyc_attestor_pubkey and kyc_attestation_sig"
            );
        }
        return Ok(NodeKycVerificationOutcome {
            provided: false,
            verified: false,
        });
    }
    let Some(attestor_pubkey_hex) = attestor_pubkey_hex else {
        return Ok(NodeKycVerificationOutcome {
            provided: true,
            verified: false,
        });
    };
    let Some(attestation_sig_hex) = attestation_sig_hex else {
        return Ok(NodeKycVerificationOutcome {
            provided: true,
            verified: false,
        });
    };
    let Ok(attestor_pubkey) = decode_hex_bytes(&attestor_pubkey_hex, "kyc_attestor_pubkey") else {
        return Ok(NodeKycVerificationOutcome {
            provided: true,
            verified: false,
        });
    };
    if attestor_pubkey.len() != 32 {
        return Ok(NodeKycVerificationOutcome {
            provided: true,
            verified: false,
        });
    }
    let Ok(attestation_sig) = decode_hex_bytes(&attestation_sig_hex, "kyc_attestation_sig") else {
        return Ok(NodeKycVerificationOutcome {
            provided: true,
            verified: false,
        });
    };
    if attestation_sig.len() != 64 {
        return Ok(NodeKycVerificationOutcome {
            provided: true,
            verified: false,
        });
    }
    let mut attestor_pubkey_arr = [0u8; 32];
    attestor_pubkey_arr.copy_from_slice(&attestor_pubkey);
    let allowlist = parse_ua_kyc_attestor_allowlist();
    if !allowlist.is_empty() && !allowlist.iter().any(|allowed| allowed == &attestor_pubkey_arr) {
        return Ok(NodeKycVerificationOutcome {
            provided: true,
            verified: false,
        });
    }
    let Ok(verifying_key) = VerifyingKey::from_bytes(&attestor_pubkey_arr) else {
        return Ok(NodeKycVerificationOutcome {
            provided: true,
            verified: false,
        });
    };
    let Ok(signature) = Ed25519Signature::from_slice(&attestation_sig) else {
        return Ok(NodeKycVerificationOutcome {
            provided: true,
            verified: false,
        });
    };
    let message = ua_kyc_attestation_message(uca_id, chain_id, external_address, role, nonce);
    let verified = verifying_key
        .verify(message.as_bytes(), &signature)
        .is_ok();
    Ok(NodeKycVerificationOutcome {
        provided: true,
        verified,
    })
}

fn parse_eth_send_transaction_ir(
    params: &serde_json::Value,
    from: Vec<u8>,
    chain_id: u64,
    nonce: u64,
    tx_type4: bool,
) -> Result<TxIR> {
    let to = match param_as_string(params, "to") {
        Some(raw_to) => {
            let trimmed = raw_to.trim();
            if trimmed.is_empty() || trimmed.eq_ignore_ascii_case("0x") {
                None
            } else {
                Some(decode_hex_bytes(trimmed, "to")?)
            }
        }
        None => None,
    };
    let data = match param_as_string(params, "data").or_else(|| param_as_string(params, "input")) {
        Some(raw_data) => {
            let trimmed = raw_data.trim();
            if trimmed.is_empty() || trimmed.eq_ignore_ascii_case("0x") {
                Vec::new()
            } else {
                decode_hex_bytes(trimmed, "data")?
            }
        }
        None => Vec::new(),
    };
    let value = param_as_u128_any(params, &["value"]).unwrap_or(0);
    let gas_limit = param_as_u64_any(params, &["gas_limit", "gasLimit", "gas"]).unwrap_or(21_000);
    let gas_price = param_as_u64_any(
        params,
        &[
            "gas_price",
            "gasPrice",
            "max_fee_per_gas",
            "maxFeePerGas",
            "max_priority_fee_per_gas",
            "maxPriorityFeePerGas",
        ],
    )
    .unwrap_or(1);
    let tx_type = if tx_type4 {
        TxType::ContractCall
    } else if to.is_none() {
        TxType::ContractDeploy
    } else if data.is_empty() {
        TxType::Transfer
    } else {
        TxType::ContractCall
    };

    let mut tx = TxIR {
        hash: Vec::new(),
        from,
        to,
        value,
        gas_limit,
        gas_price,
        nonce,
        data,
        signature: Vec::new(),
        chain_id,
        tx_type,
        source_chain: None,
        target_chain: None,
    };
    tx.compute_hash();
    Ok(tx)
}

fn tx_type_label(tx_type: TxType) -> &'static str {
    match tx_type {
        TxType::Transfer => "transfer",
        TxType::ContractCall => "contract_call",
        TxType::ContractDeploy => "contract_deploy",
        TxType::Privacy => "privacy",
        TxType::CrossShard => "cross_shard",
        TxType::CrossChainTransfer => "cross_chain_transfer",
        TxType::CrossChainCall => "cross_chain_call",
    }
}

fn parse_account_role(params: &serde_json::Value) -> Result<AccountRole> {
    let raw = param_as_string(params, "role")
        .unwrap_or_else(|| "owner".to_string())
        .to_ascii_lowercase();
    match raw.as_str() {
        "owner" => Ok(AccountRole::Owner),
        "delegate" => Ok(AccountRole::Delegate),
        "session" | "sessionkey" | "session_key" => Ok(AccountRole::SessionKey),
        _ => bail!("invalid role: {}; valid: owner|delegate|session_key", raw),
    }
}

fn parse_persona_type(params: &serde_json::Value, key: &str) -> Result<PersonaType> {
    let raw = param_as_string(params, key)
        .ok_or_else(|| anyhow::anyhow!("{} is required", key))?
        .to_ascii_lowercase();
    Ok(match raw.as_str() {
        "web30" => PersonaType::Web30,
        "evm" => PersonaType::Evm,
        "bitcoin" | "btc" => PersonaType::Bitcoin,
        "solana" | "sol" => PersonaType::Solana,
        other => PersonaType::Other(other.to_string()),
    })
}

fn parse_external_address(params: &serde_json::Value, key: &str) -> Result<Vec<u8>> {
    let raw = param_as_string(params, key).ok_or_else(|| anyhow::anyhow!("{} is required", key))?;
    decode_hex_bytes(&raw, key)
}

fn parse_primary_key_ref(params: &serde_json::Value, uca_id: &str) -> Result<Vec<u8>> {
    if let Some(raw) = param_as_string(params, "primary_key_ref") {
        return decode_hex_bytes(&raw, "primary_key_ref");
    }
    let mut hasher = Sha256::new();
    hasher.update(b"uca-primary-key-ref-v1");
    hasher.update(uca_id.as_bytes());
    Ok(hasher.finalize().to_vec())
}

fn validate_uca_id_policy(uca_id_raw: &str) -> Result<String> {
    let uca_id = uca_id_raw.trim();
    if uca_id.is_empty() {
        bail!("uca_id must not be empty");
    }
    if uca_id.len() > 128 {
        bail!("uca_id too long: {} (max 128)", uca_id.len());
    }
    if uca_id.chars().all(|ch| ch.is_ascii_digit()) {
        let numeric = uca_id
            .parse::<u64>()
            .map_err(|_| anyhow::anyhow!("uca_id numeric segment parse failed: {}", uca_id))?;
        const UCA_BUSINESS_SEGMENT_START: u64 = 1_000_000;
        if numeric < UCA_BUSINESS_SEGMENT_START {
            bail!(
                "uca_id in reserved numeric segment: {} (business segment starts at {})",
                numeric,
                UCA_BUSINESS_SEGMENT_START
            );
        }
    }
    Ok(uca_id.to_string())
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
type AoemFreeFn = unsafe extern "C" fn(*mut u8, usize);
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
#[repr(C)]
#[derive(Clone, Copy)]
struct AoemMldsaVerifyItemV1 {
    level: u32,
    pubkey_ptr: *const u8,
    pubkey_len: usize,
    message_ptr: *const u8,
    message_len: usize,
    signature_ptr: *const u8,
    signature_len: usize,
}
type AoemMldsaVerifyBatchFn = unsafe extern "C" fn(
    items_ptr: *const AoemMldsaVerifyItemV1,
    item_count: usize,
    out_results_ptr: *mut *mut u8,
    out_results_len: *mut usize,
    out_valid_count: *mut u32,
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
    verify_batch_fn: Option<AoemMldsaVerifyBatchFn>,
    free_fn: Option<AoemFreeFn>,
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

    fn verify_batch_with_report(
        &self,
        inputs: &[GovernanceVoteVerificationInput<'_>],
    ) -> std::result::Result<Vec<GovernanceVoteVerificationReport>, ConsensusBftError> {
        if inputs.is_empty() {
            return Ok(Vec::new());
        }
        let Some(verify_batch_fn) = self.verify_batch_fn else {
            let mut out = Vec::with_capacity(inputs.len());
            for input in inputs {
                self.verify(input.vote, input.key)?;
                out.push(GovernanceVoteVerificationReport {
                    verifier_name: self.name(),
                    scheme: self.scheme(),
                });
            }
            return Ok(out);
        };
        let Some(free_fn) = self.free_fn else {
            let mut out = Vec::with_capacity(inputs.len());
            for input in inputs {
                self.verify(input.vote, input.key)?;
                out.push(GovernanceVoteVerificationReport {
                    verifier_name: self.name(),
                    scheme: self.scheme(),
                });
            }
            return Ok(out);
        };

        let mut pubkeys = Vec::with_capacity(inputs.len());
        let mut signatures = Vec::with_capacity(inputs.len());
        let mut messages = Vec::with_capacity(inputs.len());
        let mut items = Vec::with_capacity(inputs.len());
        let mut voter_ids = Vec::with_capacity(inputs.len());
        for input in inputs {
            let vote = input.vote;
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
            voter_ids.push(vote.voter_id);
            pubkeys.push(pubkey.to_vec());
            signatures.push(signature.to_vec());
            messages.push(governance_vote_message_bytes(vote));
        }
        for idx in 0..inputs.len() {
            items.push(AoemMldsaVerifyItemV1 {
                level: GOVERNANCE_MLDSA87_LEVEL,
                pubkey_ptr: pubkeys[idx].as_ptr(),
                pubkey_len: pubkeys[idx].len(),
                message_ptr: messages[idx].as_ptr(),
                message_len: messages[idx].len(),
                signature_ptr: signatures[idx].as_ptr(),
                signature_len: signatures[idx].len(),
            });
        }

        let mut out_results_ptr: *mut u8 = ptr::null_mut();
        let mut out_results_len: usize = 0;
        let mut out_valid_count: u32 = 0;
        let rc = unsafe {
            verify_batch_fn(
                items.as_ptr(),
                items.len(),
                &mut out_results_ptr as *mut *mut u8,
                &mut out_results_len as *mut usize,
                &mut out_valid_count as *mut u32,
            )
        };
        if rc != 0 {
            return Err(ConsensusBftError::InvalidSignature(format!(
                "aoem ffi mldsa verify batch failed: rc={}",
                rc
            )));
        }
        if out_results_ptr.is_null() {
            return Err(ConsensusBftError::InvalidSignature(
                "aoem ffi mldsa verify batch returned null results".to_string(),
            ));
        }
        let out_results = unsafe { std::slice::from_raw_parts(out_results_ptr, out_results_len) };
        if out_results_len != inputs.len() {
            unsafe { free_fn(out_results_ptr, out_results_len) };
            return Err(ConsensusBftError::InvalidSignature(format!(
                "aoem ffi mldsa verify batch result size mismatch: expected {} got {}",
                inputs.len(),
                out_results_len
            )));
        }
        if out_valid_count as usize > inputs.len() {
            unsafe { free_fn(out_results_ptr, out_results_len) };
            return Err(ConsensusBftError::InvalidSignature(format!(
                "aoem ffi mldsa verify batch valid_count out of range: {}",
                out_valid_count
            )));
        }
        for (idx, b) in out_results.iter().enumerate() {
            if *b == 1 {
                continue;
            }
            unsafe { free_fn(out_results_ptr, out_results_len) };
            return Err(ConsensusBftError::InvalidSignature(format!(
                "aoem ffi mldsa verify batch returned invalid for voter {}",
                voter_ids[idx]
            )));
        }
        unsafe { free_fn(out_results_ptr, out_results_len) };
        Ok(inputs
            .iter()
            .map(|_| GovernanceVoteVerificationReport {
                verifier_name: self.name(),
                scheme: self.scheme(),
            })
            .collect())
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

    let (
        abi_version_fn,
        supported_fn,
        pubkey_size_fn,
        signature_size_fn,
        verify_fn,
        verify_batch_fn,
        free_fn,
    ) = unsafe {
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
        let verify_batch_fn: Option<libloading::Symbol<AoemMldsaVerifyBatchFn>> =
            lib.get(b"aoem_mldsa_verify_batch_v1\0").ok();
        let free_fn: Option<libloading::Symbol<AoemFreeFn>> = lib.get(b"aoem_free\0").ok();
        (
            *abi_version_fn,
            *supported_fn,
            *pubkey_size_fn,
            *signature_size_fn,
            *verify_fn,
            verify_batch_fn.map(|f| *f),
            free_fn.map(|f| *f),
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
        verify_batch_fn,
        free_fn,
        expected_pubkey_size,
        expected_signature_size,
        voter_pubkeys,
    }))
}

fn governance_mldsa_mode() -> String {
    string_env_nonempty("NOVOVM_GOVERNANCE_MLDSA_MODE")
        .unwrap_or_else(|| "disabled".to_string())
        .trim()
        .to_ascii_lowercase()
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
            let mode = governance_mldsa_mode();
            match mode.as_str() {
                "aoem_ffi" => {
                    let custom = build_aoem_ffi_mldsa87_vote_verifier()?;
                    engine.set_governance_vote_verifier(custom);
                    Ok(())
                }
                "disabled" => bail!(
                    "unsupported governance vote verifier: mldsa87 (disabled-by-policy, set NOVOVM_GOVERNANCE_MLDSA_MODE=aoem_ffi + AOEM FFI library to enable)"
                ),
                other => bail!(
                    "invalid NOVOVM_GOVERNANCE_MLDSA_MODE={} (valid: disabled, aoem_ffi)",
                    other
                ),
            }
        }
    }
}

fn configure_governance_vote_verifier(engine: &BFTEngine) -> Result<()> {
    let verifier = load_governance_vote_verifier_config_from_env()?;
    apply_governance_vote_verifier(engine, verifier)?;
    let mode = governance_mldsa_mode();
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
            "unsupported signature scheme {} (policy-gated, current enabled: {})",
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

fn parse_nav_feed_http_endpoint(raw: &str) -> Result<NavFeedHttpEndpoint> {
    let trimmed = raw.trim();
    let rest = trimmed.strip_prefix("http://").ok_or_else(|| {
        anyhow::anyhow!("unsupported NAV feed url scheme (only http://): {}", raw)
    })?;
    if rest.is_empty() {
        bail!("NAV feed url is empty: {}", raw);
    }

    let (authority, path_tail) = match rest.find('/') {
        Some(pos) => (&rest[..pos], &rest[pos..]),
        None => (rest, "/"),
    };
    if authority.is_empty() {
        bail!("NAV feed url missing host: {}", raw);
    }

    let (host, port) = if authority.starts_with('[') {
        let end = authority
            .find(']')
            .ok_or_else(|| anyhow::anyhow!("NAV feed ipv6 host missing ']': {}", raw))?;
        let host_inner = &authority[1..end];
        if host_inner.is_empty() {
            bail!("NAV feed host cannot be empty: {}", raw);
        }
        let tail = &authority[end + 1..];
        let port = if tail.is_empty() {
            80u16
        } else if let Some(port_raw) = tail.strip_prefix(':') {
            port_raw
                .parse::<u16>()
                .with_context(|| format!("invalid NAV feed port in url: {}", raw))?
        } else {
            bail!("invalid NAV feed ipv6 authority: {}", raw);
        };
        (host_inner.to_string(), port)
    } else {
        match authority.rsplit_once(':') {
            Some((h, p))
                if !h.is_empty() && !h.contains(':') && p.chars().all(|ch| ch.is_ascii_digit()) =>
            {
                (
                    h.to_string(),
                    p.parse::<u16>()
                        .with_context(|| format!("invalid NAV feed port in url: {}", raw))?,
                )
            }
            _ => (authority.to_string(), 80u16),
        }
    };

    let path_and_query = if path_tail.is_empty() { "/" } else { path_tail };
    if !path_and_query.starts_with('/') {
        bail!("NAV feed url path must start with '/': {}", raw);
    }

    let host_header = if host.contains(':') {
        if port == 80 {
            format!("[{}]", host)
        } else {
            format!("[{}]:{}", host, port)
        }
    } else if port == 80 {
        host.clone()
    } else {
        format!("{}:{}", host, port)
    };

    Ok(NavFeedHttpEndpoint {
        host,
        host_header,
        port,
        path_and_query: path_and_query.to_string(),
    })
}

fn parse_nav_price_bp_json(payload: &serde_json::Value) -> Option<u64> {
    fn parse_scalar(value: &serde_json::Value) -> Option<u64> {
        match value {
            serde_json::Value::Number(num) => num.as_u64(),
            serde_json::Value::String(raw) => raw.trim().parse::<u64>().ok(),
            _ => None,
        }
    }

    if let Some(v) = payload.get("price_bp").and_then(parse_scalar) {
        return Some(v);
    }
    if let Some(v) = payload.get("priceBp").and_then(parse_scalar) {
        return Some(v);
    }
    if let Some(data) = payload.get("data") {
        if let Some(v) = data.get("price_bp").and_then(parse_scalar) {
            return Some(v);
        }
        if let Some(v) = data.get("priceBp").and_then(parse_scalar) {
            return Some(v);
        }
    }
    None
}

fn parse_feed_signature_sha256_json(payload: &serde_json::Value) -> Option<String> {
    fn parse_scalar(value: &serde_json::Value) -> Option<String> {
        match value {
            serde_json::Value::String(raw) => {
                let trimmed = raw.trim();
                if trimmed.is_empty() {
                    None
                } else {
                    Some(trimmed.to_string())
                }
            }
            _ => None,
        }
    }

    for key in ["signature_sha256", "signatureSha256", "signature"] {
        if let Some(sig) = payload.get(key).and_then(parse_scalar) {
            return Some(sig);
        }
    }
    if let Some(data) = payload.get("data") {
        for key in ["signature_sha256", "signatureSha256", "signature"] {
            if let Some(sig) = data.get(key).and_then(parse_scalar) {
                return Some(sig);
            }
        }
    }
    None
}

fn compute_feed_signature_sha256(domain: &str, message: &str, key: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(domain.as_bytes());
    hasher.update(b"|");
    hasher.update(message.as_bytes());
    hasher.update(b"|");
    hasher.update(key.as_bytes());
    to_hex(&hasher.finalize())
}

fn parse_feed_urls_with_compat(list_env: &str, single_env: &str) -> Vec<String> {
    let mut urls = string_list_env_nonempty(list_env);
    if urls.is_empty() {
        if let Some(single) = string_env_nonempty(single_env) {
            urls.push(single);
        }
    }
    urls
}

fn fetch_nav_price_bp_from_http_feed(
    url: &str,
    timeout_ms: u64,
    signature_required: bool,
    signature_key: Option<&str>,
) -> Result<NavFeedFetchResult> {
    let endpoint = parse_nav_feed_http_endpoint(url)?;
    let timeout = Duration::from_millis(timeout_ms.max(1));
    let addr = (endpoint.host.as_str(), endpoint.port)
        .to_socket_addrs()
        .with_context(|| format!("resolve NAV feed host failed: {}", url))?
        .next()
        .ok_or_else(|| anyhow::anyhow!("resolve NAV feed host returned empty set: {}", url))?;
    let mut stream = TcpStream::connect_timeout(&addr, timeout)
        .with_context(|| format!("connect NAV feed failed: {}", url))?;
    stream
        .set_read_timeout(Some(timeout))
        .context("set NAV feed read timeout failed")?;
    stream
        .set_write_timeout(Some(timeout))
        .context("set NAV feed write timeout failed")?;

    let request = format!(
        "GET {} HTTP/1.1\r\nHost: {}\r\nAccept: application/json\r\nConnection: close\r\nUser-Agent: novovm-node-nav-feed/1.0\r\n\r\n",
        endpoint.path_and_query, endpoint.host_header
    );
    stream
        .write_all(request.as_bytes())
        .context("write NAV feed request failed")?;
    stream.flush().context("flush NAV feed request failed")?;

    let mut bytes = Vec::new();
    stream
        .read_to_end(&mut bytes)
        .context("read NAV feed response failed")?;
    let response = String::from_utf8(bytes).context("NAV feed response is not valid utf-8")?;

    let (head, body) = if let Some(idx) = response.find("\r\n\r\n") {
        (&response[..idx], &response[idx + 4..])
    } else if let Some(idx) = response.find("\n\n") {
        (&response[..idx], &response[idx + 2..])
    } else {
        bail!("NAV feed malformed response (missing header separator)");
    };
    let status_line = head.lines().next().unwrap_or("");
    if !status_line.contains(" 200 ") {
        bail!("NAV feed non-200 response: {}", status_line);
    }

    let payload: serde_json::Value = serde_json::from_str(body)
        .context("NAV feed body is not valid json or cannot be parsed")?;
    let price_bp = parse_nav_price_bp_json(&payload)
        .ok_or_else(|| anyhow::anyhow!("NAV feed json missing price_bp field"))?;
    if price_bp == 0 || price_bp > u64::from(NAV_VALUATION_MAX_PRICE_BP) {
        bail!(
            "NAV feed price_bp must be in [1..{}], got {}",
            NAV_VALUATION_MAX_PRICE_BP,
            price_bp
        );
    }
    let signature_verified = if signature_required {
        let signature_key = signature_key
            .filter(|v| !v.is_empty())
            .ok_or_else(|| anyhow::anyhow!("NAV feed signature key is required"))?;
        let signature = parse_feed_signature_sha256_json(&payload)
            .ok_or_else(|| anyhow::anyhow!("NAV feed json missing signature_sha256 field"))?;
        let signature = normalize_sha256_hex(&signature, "NAV feed signature_sha256")?;
        let expected = compute_feed_signature_sha256(
            "nav_feed_v1",
            &format!("price_bp={}", price_bp),
            signature_key,
        );
        if signature != expected {
            bail!("NAV feed signature mismatch");
        }
        true
    } else {
        false
    };

    Ok(NavFeedFetchResult {
        price_bp: price_bp as u32,
        signature_verified,
    })
}

fn load_market_nav_valuation_source(engine: &BFTEngine) -> Result<LoadedNavValuationSource> {
    let mode_raw = string_env("NOVOVM_GOV_MARKET_NAV_VALUATION_MODE", "deterministic");
    let mode = mode_raw.trim().to_ascii_lowercase();
    if mode.is_empty() || mode == "deterministic" {
        return Ok(LoadedNavValuationSource {
            mode: "deterministic".to_string(),
            source_name: "deterministic_v1".to_string(),
            configured_url: String::new(),
            configured_sources: 0,
            strict: false,
            timeout_ms: 0,
            fetched: false,
            fetched_sources: 0,
            min_sources: 0,
            signature_required: false,
            signature_verified: false,
            fallback_to_deterministic: false,
            price_bp: NAV_VALUATION_DEFAULT_PRICE_BP,
            reason_code: "deterministic_default".to_string(),
        });
    }
    if mode != "external_feed" {
        bail!(
            "governance_policy_invalid: NOVOVM_GOV_MARKET_NAV_VALUATION_MODE unsupported value: {} (valid: deterministic|external_feed)",
            mode_raw
        );
    }

    let source_name = string_env_nonempty("NOVOVM_GOV_MARKET_NAV_VALUATION_SOURCE_NAME")
        .unwrap_or_else(|| "external_feed_v1".to_string());
    engine
        .set_market_nav_valuation_source_external(&source_name)
        .map_err(|e| {
            anyhow::anyhow!(
                "governance_policy_invalid: set market nav valuation source failed: {}",
                e
            )
        })?;

    let strict = bool_env_default("NOVOVM_GOV_MARKET_NAV_FEED_STRICT", false);
    let timeout_ms = u64_env("NOVOVM_GOV_MARKET_NAV_FEED_TIMEOUT_MS", 1500).max(1);
    let configured_urls = parse_feed_urls_with_compat(
        "NOVOVM_GOV_MARKET_NAV_FEED_URLS",
        "NOVOVM_GOV_MARKET_NAV_FEED_URL",
    );
    let configured_sources = configured_urls.len() as u32;
    let configured_url = configured_urls.join(",");
    let min_sources = if configured_sources == 0 {
        0
    } else {
        parse_u32_env("NOVOVM_GOV_MARKET_NAV_FEED_MIN_SOURCES", 1)?
    };
    if configured_sources > 0 && min_sources > configured_sources {
        bail!(
            "governance_policy_invalid: NOVOVM_GOV_MARKET_NAV_FEED_MIN_SOURCES={} exceeds configured sources={}",
            min_sources,
            configured_sources
        );
    }
    let signature_required =
        bool_env_default("NOVOVM_GOV_MARKET_NAV_FEED_SIGNATURE_REQUIRED", false);
    let signature_key = string_env_nonempty("NOVOVM_GOV_MARKET_NAV_FEED_SIGNATURE_KEY");
    if signature_required && signature_key.is_none() {
        bail!(
            "governance_policy_invalid: NOVOVM_GOV_MARKET_NAV_FEED_SIGNATURE_KEY is required when NOVOVM_GOV_MARKET_NAV_FEED_SIGNATURE_REQUIRED=1"
        );
    }
    let direct_price_raw = string_env_nonempty("NOVOVM_GOV_MARKET_NAV_EXTERNAL_PRICE_BP");

    if let Some(raw) = direct_price_raw {
        let direct_price = raw
            .parse::<u32>()
            .with_context(|| format!("invalid NOVOVM_GOV_MARKET_NAV_EXTERNAL_PRICE_BP: {}", raw))?;
        engine
            .set_market_nav_external_price_bp(direct_price)
            .map_err(|e| {
                anyhow::anyhow!(
                "governance_policy_invalid: invalid NOVOVM_GOV_MARKET_NAV_EXTERNAL_PRICE_BP: {}",
                e
            )
            })?;
        return Ok(LoadedNavValuationSource {
            mode: "external_feed".to_string(),
            source_name,
            configured_url,
            configured_sources,
            strict,
            timeout_ms,
            fetched: true,
            fetched_sources: 0,
            min_sources,
            signature_required,
            signature_verified: false,
            fallback_to_deterministic: false,
            price_bp: direct_price,
            reason_code: "direct_env_price".to_string(),
        });
    }

    if configured_sources > 0 {
        let mut fetched_prices = Vec::new();
        let mut fetch_errors = Vec::new();
        let mut signature_verified = true;
        for url in &configured_urls {
            match fetch_nav_price_bp_from_http_feed(
                url,
                timeout_ms,
                signature_required,
                signature_key.as_deref(),
            ) {
                Ok(result) => {
                    fetched_prices.push(result.price_bp);
                    if signature_required && !result.signature_verified {
                        signature_verified = false;
                    }
                }
                Err(err) => fetch_errors.push(format!("{} => {}", url, err)),
            }
        }

        let fetched_sources = fetched_prices.len() as u32;
        if fetched_sources >= min_sources && fetched_sources > 0 {
            fetched_prices.sort_unstable();
            let mid = fetched_prices.len() / 2;
            let price_bp = if fetched_prices.len() % 2 == 1 {
                fetched_prices[mid]
            } else {
                ((u64::from(fetched_prices[mid - 1]) + u64::from(fetched_prices[mid])) / 2) as u32
            };
            engine
                .set_market_nav_external_price_bp(price_bp)
                .map_err(|e| {
                    anyhow::anyhow!(
                        "governance_policy_invalid: nav feed quote out of range: {}",
                        e
                    )
                })?;
            let reason_code = if fetched_sources == configured_sources {
                "feed_quote_ok"
            } else {
                "feed_quote_partial_ok"
            };
            return Ok(LoadedNavValuationSource {
                mode: "external_feed".to_string(),
                source_name,
                configured_url,
                configured_sources,
                strict,
                timeout_ms,
                fetched: true,
                fetched_sources,
                min_sources,
                signature_required,
                signature_verified,
                fallback_to_deterministic: false,
                price_bp,
                reason_code: reason_code.to_string(),
            });
        }
        if strict {
            let detail = if fetch_errors.is_empty() {
                "none".to_string()
            } else {
                fetch_errors.join("; ")
            };
            bail!(
                "nav_feed_fetch_failed: insufficient valid sources fetched={} min_sources={} configured_sources={} errors={}",
                fetched_sources,
                min_sources,
                configured_sources,
                detail
            );
        }
        return Ok(LoadedNavValuationSource {
            mode: "external_feed".to_string(),
            source_name,
            configured_url,
            configured_sources,
            strict,
            timeout_ms,
            fetched: false,
            fetched_sources,
            min_sources,
            signature_required,
            signature_verified: false,
            fallback_to_deterministic: true,
            price_bp: NAV_VALUATION_DEFAULT_PRICE_BP,
            reason_code: "feed_quote_insufficient_sources_fallback".to_string(),
        });
    }

    if strict {
        bail!(
            "nav_feed_fetch_failed: strict mode requires NOVOVM_GOV_MARKET_NAV_FEED_URL(S) or NOVOVM_GOV_MARKET_NAV_EXTERNAL_PRICE_BP"
        );
    }
    Ok(LoadedNavValuationSource {
        mode: "external_feed".to_string(),
        source_name,
        configured_url,
        configured_sources,
        strict,
        timeout_ms,
        fetched: false,
        fetched_sources: 0,
        min_sources,
        signature_required,
        signature_verified: false,
        fallback_to_deterministic: true,
        price_bp: NAV_VALUATION_DEFAULT_PRICE_BP,
        reason_code: "external_mode_no_quote_fallback".to_string(),
    })
}

fn parse_foreign_quote_spec_json(payload: &serde_json::Value) -> Option<String> {
    fn parse_scalar(value: &serde_json::Value) -> Option<String> {
        match value {
            serde_json::Value::String(raw) => {
                let trimmed = raw.trim();
                if trimmed.is_empty() {
                    None
                } else {
                    Some(trimmed.to_string())
                }
            }
            _ => None,
        }
    }

    if let Some(spec) = payload.get("quote_spec").and_then(parse_scalar) {
        return Some(spec);
    }
    if let Some(spec) = payload.get("quoteSpec").and_then(parse_scalar) {
        return Some(spec);
    }
    if let Some(data) = payload.get("data") {
        if let Some(spec) = data.get("quote_spec").and_then(parse_scalar) {
            return Some(spec);
        }
        if let Some(spec) = data.get("quoteSpec").and_then(parse_scalar) {
            return Some(spec);
        }
    }
    None
}

fn fetch_foreign_quote_spec_from_http_feed(
    url: &str,
    timeout_ms: u64,
    signature_required: bool,
    signature_key: Option<&str>,
) -> Result<ForeignRateFeedFetchResult> {
    let endpoint = parse_nav_feed_http_endpoint(url)?;
    let timeout = Duration::from_millis(timeout_ms.max(1));
    let addr = (endpoint.host.as_str(), endpoint.port)
        .to_socket_addrs()
        .with_context(|| format!("resolve foreign rate feed host failed: {}", url))?
        .next()
        .ok_or_else(|| {
            anyhow::anyhow!("resolve foreign rate feed host returned empty set: {}", url)
        })?;
    let mut stream = TcpStream::connect_timeout(&addr, timeout)
        .with_context(|| format!("connect foreign rate feed failed: {}", url))?;
    stream
        .set_read_timeout(Some(timeout))
        .context("set foreign rate feed read timeout failed")?;
    stream
        .set_write_timeout(Some(timeout))
        .context("set foreign rate feed write timeout failed")?;

    let request = format!(
        "GET {} HTTP/1.1\r\nHost: {}\r\nAccept: application/json\r\nConnection: close\r\nUser-Agent: novovm-node-foreign-rate-feed/1.0\r\n\r\n",
        endpoint.path_and_query, endpoint.host_header
    );
    stream
        .write_all(request.as_bytes())
        .context("write foreign rate feed request failed")?;
    stream
        .flush()
        .context("flush foreign rate feed request failed")?;

    let mut bytes = Vec::new();
    stream
        .read_to_end(&mut bytes)
        .context("read foreign rate feed response failed")?;
    let response =
        String::from_utf8(bytes).context("foreign rate feed response is not valid utf-8")?;

    let (head, body) = if let Some(idx) = response.find("\r\n\r\n") {
        (&response[..idx], &response[idx + 4..])
    } else if let Some(idx) = response.find("\n\n") {
        (&response[..idx], &response[idx + 2..])
    } else {
        bail!("foreign rate feed malformed response (missing header separator)");
    };
    let status_line = head.lines().next().unwrap_or("");
    if !status_line.contains(" 200 ") {
        bail!("foreign rate feed non-200 response: {}", status_line);
    }

    let payload: serde_json::Value = serde_json::from_str(body)
        .context("foreign rate feed body is not valid json or cannot be parsed")?;
    let quote_spec = parse_foreign_quote_spec_json(&payload)
        .ok_or_else(|| anyhow::anyhow!("foreign rate feed json missing quote_spec field"))?;
    let signature_verified = if signature_required {
        let signature_key = signature_key
            .filter(|v| !v.is_empty())
            .ok_or_else(|| anyhow::anyhow!("foreign rate feed signature key is required"))?;
        let signature = parse_feed_signature_sha256_json(&payload).ok_or_else(|| {
            anyhow::anyhow!("foreign rate feed json missing signature_sha256 field")
        })?;
        let signature = normalize_sha256_hex(&signature, "foreign rate feed signature_sha256")?;
        let expected = compute_feed_signature_sha256(
            "foreign_rate_feed_v1",
            &format!("quote_spec={}", quote_spec),
            signature_key,
        );
        if signature != expected {
            bail!("foreign rate feed signature mismatch");
        }
        true
    } else {
        false
    };
    Ok(ForeignRateFeedFetchResult {
        quote_spec,
        signature_verified,
    })
}

fn aggregate_quote_spec_majority(specs: &[String]) -> Option<String> {
    let mut counts: HashMap<String, u32> = HashMap::new();
    for spec in specs {
        *counts.entry(spec.clone()).or_insert(0) += 1;
    }
    let mut best: Option<(String, u32)> = None;
    for (spec, count) in counts {
        match &best {
            None => best = Some((spec, count)),
            Some((best_spec, best_count)) => {
                if count > *best_count || (count == *best_count && spec < *best_spec) {
                    best = Some((spec, count));
                }
            }
        }
    }
    best.map(|(spec, _)| spec)
}

fn load_market_foreign_rate_source(engine: &BFTEngine) -> Result<LoadedForeignRateSource> {
    let mode_raw = string_env("NOVOVM_GOV_MARKET_FOREIGN_RATE_MODE", "deterministic");
    let mode = mode_raw.trim().to_ascii_lowercase();
    if mode.is_empty() || mode == "deterministic" {
        return Ok(LoadedForeignRateSource {
            mode: "deterministic".to_string(),
            source_name: FOREIGN_RATE_DEFAULT_SOURCE_NAME.to_string(),
            configured_url: String::new(),
            configured_sources: 0,
            strict: false,
            timeout_ms: 0,
            fetched: false,
            fetched_sources: 0,
            min_sources: 0,
            signature_required: false,
            signature_verified: false,
            quote_spec_applied: false,
            fallback_to_deterministic: false,
            reason_code: "deterministic_default".to_string(),
        });
    }
    if mode != "external_feed" {
        bail!(
            "governance_policy_invalid: NOVOVM_GOV_MARKET_FOREIGN_RATE_MODE unsupported value: {} (valid: deterministic|external_feed)",
            mode_raw
        );
    }

    let source_name = string_env_nonempty("NOVOVM_GOV_MARKET_FOREIGN_RATE_SOURCE_NAME")
        .unwrap_or_else(|| "foreign_external_feed_v1".to_string());
    engine
        .set_market_foreign_rate_source_name(&source_name)
        .map_err(|e| {
            anyhow::anyhow!(
                "governance_policy_invalid: set market foreign rate source failed: {}",
                e
            )
        })?;

    let strict = bool_env_default("NOVOVM_GOV_MARKET_FOREIGN_RATE_FEED_STRICT", false);
    let timeout_ms = u64_env("NOVOVM_GOV_MARKET_FOREIGN_RATE_FEED_TIMEOUT_MS", 1500).max(1);
    let configured_urls = parse_feed_urls_with_compat(
        "NOVOVM_GOV_MARKET_FOREIGN_RATE_FEED_URLS",
        "NOVOVM_GOV_MARKET_FOREIGN_RATE_FEED_URL",
    );
    let configured_sources = configured_urls.len() as u32;
    let configured_url = configured_urls.join(",");
    let min_sources = if configured_sources == 0 {
        0
    } else {
        parse_u32_env("NOVOVM_GOV_MARKET_FOREIGN_RATE_FEED_MIN_SOURCES", 1)?
    };
    if configured_sources > 0 && min_sources > configured_sources {
        bail!(
            "governance_policy_invalid: NOVOVM_GOV_MARKET_FOREIGN_RATE_FEED_MIN_SOURCES={} exceeds configured sources={}",
            min_sources,
            configured_sources
        );
    }
    let signature_required = bool_env_default(
        "NOVOVM_GOV_MARKET_FOREIGN_RATE_FEED_SIGNATURE_REQUIRED",
        false,
    );
    let signature_key = string_env_nonempty("NOVOVM_GOV_MARKET_FOREIGN_RATE_FEED_SIGNATURE_KEY");
    if signature_required && signature_key.is_none() {
        bail!(
            "governance_policy_invalid: NOVOVM_GOV_MARKET_FOREIGN_RATE_FEED_SIGNATURE_KEY is required when NOVOVM_GOV_MARKET_FOREIGN_RATE_FEED_SIGNATURE_REQUIRED=1"
        );
    }
    let direct_quote_spec = string_env_nonempty("NOVOVM_GOV_MARKET_FOREIGN_QUOTE_SPEC");

    if let Some(spec) = direct_quote_spec {
        engine.apply_market_foreign_quote_spec(&spec).map_err(|e| {
            anyhow::anyhow!(
                "governance_policy_invalid: invalid NOVOVM_GOV_MARKET_FOREIGN_QUOTE_SPEC: {}",
                e
            )
        })?;
        return Ok(LoadedForeignRateSource {
            mode: "external_feed".to_string(),
            source_name,
            configured_url,
            configured_sources,
            strict,
            timeout_ms,
            fetched: true,
            fetched_sources: 0,
            min_sources,
            signature_required,
            signature_verified: false,
            quote_spec_applied: true,
            fallback_to_deterministic: false,
            reason_code: "direct_env_quote_spec".to_string(),
        });
    }

    if configured_sources > 0 {
        let mut fetched_specs = Vec::new();
        let mut fetch_errors = Vec::new();
        let mut signature_verified = true;
        for url in &configured_urls {
            match fetch_foreign_quote_spec_from_http_feed(
                url,
                timeout_ms,
                signature_required,
                signature_key.as_deref(),
            ) {
                Ok(result) => {
                    fetched_specs.push(result.quote_spec);
                    if signature_required && !result.signature_verified {
                        signature_verified = false;
                    }
                }
                Err(err) => fetch_errors.push(format!("{} => {}", url, err)),
            }
        }
        let fetched_sources = fetched_specs.len() as u32;
        if fetched_sources >= min_sources && fetched_sources > 0 {
            let aggregated_spec = aggregate_quote_spec_majority(&fetched_specs)
                .ok_or_else(|| anyhow::anyhow!("foreign quote aggregation result is empty"))?;
            engine
                .apply_market_foreign_quote_spec(&aggregated_spec)
                .map_err(|e| {
                    anyhow::anyhow!(
                        "governance_policy_invalid: foreign quote spec invalid: {}",
                        e
                    )
                })?;
            let reason_code = if fetched_sources == configured_sources {
                "feed_quote_ok"
            } else {
                "feed_quote_partial_ok"
            };
            return Ok(LoadedForeignRateSource {
                mode: "external_feed".to_string(),
                source_name,
                configured_url,
                configured_sources,
                strict,
                timeout_ms,
                fetched: true,
                fetched_sources,
                min_sources,
                signature_required,
                signature_verified,
                quote_spec_applied: true,
                fallback_to_deterministic: false,
                reason_code: reason_code.to_string(),
            });
        }
        if strict {
            let detail = if fetch_errors.is_empty() {
                "none".to_string()
            } else {
                fetch_errors.join("; ")
            };
            bail!(
                "foreign_rate_feed_fetch_failed: insufficient valid sources fetched={} min_sources={} configured_sources={} errors={}",
                fetched_sources,
                min_sources,
                configured_sources,
                detail
            );
        }
        return Ok(LoadedForeignRateSource {
            mode: "external_feed".to_string(),
            source_name,
            configured_url,
            configured_sources,
            strict,
            timeout_ms,
            fetched: false,
            fetched_sources,
            min_sources,
            signature_required,
            signature_verified: false,
            quote_spec_applied: false,
            fallback_to_deterministic: true,
            reason_code: "feed_quote_insufficient_sources_fallback".to_string(),
        });
    }

    if strict {
        bail!(
            "foreign_rate_feed_fetch_failed: strict mode requires NOVOVM_GOV_MARKET_FOREIGN_RATE_FEED_URL(S) or NOVOVM_GOV_MARKET_FOREIGN_QUOTE_SPEC"
        );
    }
    Ok(LoadedForeignRateSource {
        mode: "external_feed".to_string(),
        source_name,
        configured_url,
        configured_sources,
        strict,
        timeout_ms,
        fetched: false,
        fetched_sources: 0,
        min_sources,
        signature_required,
        signature_verified: false,
        quote_spec_applied: false,
        fallback_to_deterministic: true,
        reason_code: "external_mode_no_quote_fallback".to_string(),
    })
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
    let mut out = serde_json::Map::new();
    macro_rules! put {
        ($k:literal, $v:expr) => {
            out.insert($k.to_string(), serde_json::json!($v));
        };
    }
    put!("amm_swap_fee_bp", snapshot.amm_swap_fee_bp);
    put!("amm_lp_fee_share_bp", snapshot.amm_lp_fee_share_bp);
    put!(
        "cdp_min_collateral_ratio_bp",
        snapshot.cdp_min_collateral_ratio_bp
    );
    put!(
        "cdp_liquidation_threshold_bp",
        snapshot.cdp_liquidation_threshold_bp
    );
    put!(
        "cdp_liquidation_penalty_bp",
        snapshot.cdp_liquidation_penalty_bp
    );
    put!("cdp_stability_fee_bp", snapshot.cdp_stability_fee_bp);
    put!("cdp_max_leverage_x100", snapshot.cdp_max_leverage_x100);
    put!("bond_one_year_coupon_bp", snapshot.bond_one_year_coupon_bp);
    put!(
        "bond_three_year_coupon_bp",
        snapshot.bond_three_year_coupon_bp
    );
    put!(
        "bond_five_year_coupon_bp",
        snapshot.bond_five_year_coupon_bp
    );
    put!(
        "bond_max_maturity_days_policy",
        snapshot.bond_max_maturity_days_policy
    );
    put!("bond_min_issue_price_bp", snapshot.bond_min_issue_price_bp);
    put!(
        "reserve_min_reserve_ratio_bp",
        snapshot.reserve_min_reserve_ratio_bp
    );
    put!(
        "reserve_redemption_fee_bp",
        snapshot.reserve_redemption_fee_bp
    );
    put!(
        "nav_settlement_delay_epochs",
        snapshot.nav_settlement_delay_epochs
    );
    put!(
        "nav_max_daily_redemption_bp",
        snapshot.nav_max_daily_redemption_bp
    );
    put!(
        "buyback_trigger_discount_bp",
        snapshot.buyback_trigger_discount_bp
    );
    put!(
        "buyback_max_treasury_budget_per_epoch",
        snapshot.buyback_max_treasury_budget_per_epoch
    );
    put!("buyback_burn_share_bp", snapshot.buyback_burn_share_bp);
    put!("treasury_main_balance", snapshot.treasury_main_balance);
    put!(
        "treasury_ecosystem_balance",
        snapshot.treasury_ecosystem_balance
    );
    put!(
        "treasury_risk_reserve_balance",
        snapshot.treasury_risk_reserve_balance
    );
    put!(
        "reserve_foreign_usdt_balance",
        snapshot.reserve_foreign_usdt_balance
    );
    put!("nav_soft_floor_value", snapshot.nav_soft_floor_value);
    put!(
        "buyback_last_spent_stable",
        snapshot.buyback_last_spent_stable
    );
    put!(
        "buyback_last_burned_token",
        snapshot.buyback_last_burned_token
    );
    put!("oracle_price_before", snapshot.oracle_price_before);
    put!("oracle_price_after", snapshot.oracle_price_after);
    put!(
        "cdp_liquidation_candidates",
        snapshot.cdp_liquidation_candidates
    );
    put!(
        "cdp_liquidations_executed",
        snapshot.cdp_liquidations_executed
    );
    put!(
        "cdp_liquidation_penalty_routed",
        snapshot.cdp_liquidation_penalty_routed
    );
    put!("nav_snapshot_day", snapshot.nav_snapshot_day);
    put!("nav_latest_value", snapshot.nav_latest_value);
    put!("nav_valuation_source", snapshot.nav_valuation_source);
    put!("nav_valuation_price_bp", snapshot.nav_valuation_price_bp);
    put!(
        "nav_valuation_fallback_used",
        snapshot.nav_valuation_fallback_used
    );
    put!(
        "nav_redemptions_submitted",
        snapshot.nav_redemptions_submitted
    );
    put!(
        "nav_redemptions_executed",
        snapshot.nav_redemptions_executed
    );
    put!(
        "nav_executed_stable_total",
        snapshot.nav_executed_stable_total
    );
    put!(
        "dividend_income_received",
        snapshot.dividend_income_received
    );
    put!(
        "dividend_runtime_balance_accounts",
        snapshot.dividend_runtime_balance_accounts
    );
    put!(
        "dividend_eligible_accounts",
        snapshot.dividend_eligible_accounts
    );
    put!(
        "dividend_snapshot_created",
        snapshot.dividend_snapshot_created
    );
    put!(
        "dividend_claims_executed",
        snapshot.dividend_claims_executed
    );
    put!("dividend_pool_balance", snapshot.dividend_pool_balance);
    put!(
        "foreign_payments_processed",
        snapshot.foreign_payments_processed
    );
    put!("foreign_rate_source", snapshot.foreign_rate_source);
    put!(
        "foreign_rate_quote_spec_applied",
        snapshot.foreign_rate_quote_spec_applied
    );
    put!(
        "foreign_rate_fallback_used",
        snapshot.foreign_rate_fallback_used
    );
    put!(
        "foreign_token_paid_total",
        snapshot.foreign_token_paid_total
    );
    put!("foreign_reserve_btc", snapshot.foreign_reserve_btc);
    put!("foreign_reserve_eth", snapshot.foreign_reserve_eth);
    put!(
        "foreign_payment_reserve_usdt",
        snapshot.foreign_payment_reserve_usdt
    );
    put!("foreign_swap_out_total", snapshot.foreign_swap_out_total);
    serde_json::Value::Object(out)
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
        restored_head_seq, restored_root_hex
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
    fn parse_block_selector_from_raw(raw: Option<String>) -> Result<Option<u64>> {
        let Some(raw) = raw else { return Ok(None) };
        let trimmed = raw.trim();
        if trimmed.is_empty()
            || trimmed.eq_ignore_ascii_case("latest")
            || trimmed.eq_ignore_ascii_case("pending")
        {
            return Ok(None);
        }
        if trimmed.eq_ignore_ascii_case("earliest") {
            return Ok(Some(0));
        }
        parse_u64_decimal_or_hex(trimmed)
            .map(Some)
            .ok_or_else(|| anyhow::anyhow!("invalid block selector: {}", trimmed))
    }

    fn parse_block_selector_u64(params: &serde_json::Value, default_latest: u64) -> Result<u64> {
        fn parse_at_array_index(
            params: &serde_json::Value,
            index: usize,
            default_latest: u64,
        ) -> Result<u64> {
            let raw = match params {
                serde_json::Value::Array(arr) => arr.get(index).and_then(value_to_string),
                _ => None,
            };
            let parsed = parse_block_selector_from_raw(raw)?;
            Ok(parsed.unwrap_or(default_latest))
        }

        if let serde_json::Value::Array(_) = params {
            return parse_at_array_index(params, 0, default_latest);
        }

        let object_selector = param_as_string(params, "height")
            .or_else(|| param_as_string(params, "block_number"))
            .or_else(|| param_as_string(params, "number"))
            .or_else(|| param_as_string(params, "block"));
        let parsed = parse_block_selector_from_raw(object_selector)?;
        Ok(parsed.unwrap_or(default_latest))
    }

    fn parse_block_selector_at_array_index_u64(
        params: &serde_json::Value,
        index: usize,
        default_latest: u64,
    ) -> Result<u64> {
        let object_selector = param_as_string(params, "height")
            .or_else(|| param_as_string(params, "block_number"))
            .or_else(|| param_as_string(params, "number"))
            .or_else(|| param_as_string(params, "block"));
        if let Some(height) = parse_block_selector_from_raw(object_selector)? {
            return Ok(height);
        }

        let raw = match params {
            serde_json::Value::Array(arr) => arr.get(index).and_then(value_to_string),
            _ => None,
        };
        let parsed = parse_block_selector_from_raw(raw)?;
        Ok(parsed.unwrap_or(default_latest))
    }

    fn parse_eth_call_object(params: &serde_json::Value) -> serde_json::Value {
        match params {
            serde_json::Value::Array(arr) => arr.first().cloned().unwrap_or(serde_json::Value::Null),
            serde_json::Value::Object(map) => map
                .get("call")
                .or_else(|| map.get("tx"))
                .or_else(|| map.get("transaction"))
                .cloned()
                .unwrap_or_else(|| params.clone()),
            _ => serde_json::Value::Null,
        }
    }

    fn parse_eth_storage_slot(params: &serde_json::Value) -> Result<u64> {
        let object_slot = param_as_string(params, "position")
            .or_else(|| param_as_string(params, "slot"))
            .or_else(|| param_as_string(params, "index"));
        if let Some(slot) = object_slot {
            return parse_u64_decimal_or_hex(&slot)
                .ok_or_else(|| anyhow::anyhow!("invalid storage slot selector: {}", slot));
        }

        if let serde_json::Value::Array(arr) = params {
            if let Some(raw) = arr.get(1).and_then(value_to_string) {
                return parse_u64_decimal_or_hex(&raw)
                    .ok_or_else(|| anyhow::anyhow!("invalid storage slot selector: {}", raw));
            }
        }
        Ok(0)
    }

    fn parse_eth_reward_percentile_count(params: &serde_json::Value) -> usize {
        let candidate = match params {
            serde_json::Value::Object(map) => map
                .get("reward_percentiles")
                .or_else(|| map.get("rewardPercentiles")),
            serde_json::Value::Array(arr) => arr.get(2),
            _ => None,
        };
        candidate
            .and_then(|value| value.as_array().map(|items| items.len()))
            .unwrap_or(0)
    }

    fn hex_to_data_len(data: &str) -> usize {
        let trimmed = data.trim();
        let hex = trimmed
            .strip_prefix("0x")
            .or_else(|| trimmed.strip_prefix("0X"))
            .unwrap_or(trimmed);
        if hex.is_empty() {
            0
        } else {
            hex.len().saturating_add(1) / 2
        }
    }

    fn parse_eth_compat_chain_id(params: &serde_json::Value, default_chain_id: u64) -> Result<u64> {
        let parse_selector = |raw: Option<String>| -> Result<Option<u64>> {
            let Some(raw) = raw else { return Ok(None) };
            let trimmed = raw.trim();
            if trimmed.is_empty() {
                return Ok(None);
            }
            parse_u64_decimal_or_hex(trimmed)
                .map(Some)
                .ok_or_else(|| anyhow::anyhow!("invalid chain id selector: {}", trimmed))
        };

        let object_chain_id = param_as_string(params, "chain_id")
            .or_else(|| param_as_string(params, "chainId"))
            .or_else(|| param_as_string(params, "network_id"))
            .or_else(|| param_as_string(params, "net_version"));
        if let Some(chain_id) = parse_selector(object_chain_id)? {
            return Ok(chain_id);
        }

        if let serde_json::Value::Array(arr) = params {
            let array_chain_id = arr.first().and_then(value_to_string);
            if let Some(chain_id) = parse_selector(array_chain_id)? {
                return Ok(chain_id);
            }
        }
        Ok(default_chain_id)
    }

    fn parse_eth_query_account(params: &serde_json::Value) -> Option<String> {
        param_as_string(params, "account")
            .or_else(|| param_as_string(params, "address"))
            .or_else(|| match params {
                serde_json::Value::Array(arr) => arr.first().and_then(value_to_string),
                _ => None,
            })
    }

    fn parse_eth_compat_chain_id(params: &serde_json::Value, default_chain_id: u64) -> Result<u64> {
        let parse_chain_id = |raw: Option<String>| -> Result<Option<u64>> {
            let Some(raw) = raw else { return Ok(None) };
            let trimmed = raw.trim();
            if trimmed.is_empty() {
                return Ok(None);
            }
            parse_u64_decimal_or_hex(trimmed)
                .map(Some)
                .ok_or_else(|| anyhow::anyhow!("invalid chain id selector: {}", trimmed))
        };

        let object_chain_id = param_as_string(params, "chain_id")
            .or_else(|| param_as_string(params, "chainId"))
            .or_else(|| param_as_string(params, "network_id"))
            .or_else(|| param_as_string(params, "net_version"));
        if let Some(chain_id) = parse_chain_id(object_chain_id)? {
            return Ok(chain_id);
        }

        if let serde_json::Value::Array(arr) = params {
            let array_chain_id = arr.first().and_then(value_to_string);
            if let Some(chain_id) = parse_chain_id(array_chain_id)? {
                return Ok(chain_id);
            }
        }
        Ok(default_chain_id)
    }

    let latest_height = db.blocks.last().map(|b| b.height).unwrap_or(0);
    let default_eth_chain_id =
        parse_u64_decimal_or_hex(&string_env("NOVOVM_ETH_COMPAT_CHAIN_ID", "1")).unwrap_or(1);
    let client_version = string_env(
        "NOVOVM_PUBLIC_CLIENT_VERSION",
        &format!("novovm-node/{} (supervm-host)", env!("CARGO_PKG_VERSION")),
    );
    let compat_gas_price =
        parse_u64_decimal_or_hex(&string_env("NOVOVM_ETH_COMPAT_GAS_PRICE", "1000000000"))
            .unwrap_or(1_000_000_000);
    let compat_priority_fee =
        parse_u64_decimal_or_hex(&string_env("NOVOVM_ETH_COMPAT_PRIORITY_FEE", "100000000"))
            .unwrap_or(100_000_000);
    let response = match method {
        "novovm_getSurfaceMap" | "novovm_get_surface_map" => novovm_public_rpc_surface_map_json(),
        "novovm_getMethodDomain" | "novovm_get_method_domain" => {
            let target_method = param_as_string(params, "method")
                .or_else(|| param_as_string(params, "rpc_method"))
                .or_else(|| param_as_string(params, "name"))
                .or_else(|| match params {
                    serde_json::Value::Array(arr) => arr.first().and_then(value_to_string),
                    _ => None,
                })
                .ok_or_else(|| anyhow::anyhow!("method is required for novovm_getMethodDomain"))?;
            novovm_public_rpc_method_domain_json(&target_method)
        }
        "eth_chainId" => {
            let chain_id = parse_eth_compat_chain_id(params, default_eth_chain_id)?;
            serde_json::json!({
                "method": "eth_chainId",
                "chain_id": chain_id,
                "chain_id_hex": format!("0x{:x}", chain_id),
            })
        }
        "net_version" => {
            let chain_id = parse_eth_compat_chain_id(params, default_eth_chain_id)?;
            serde_json::json!({
                "method": "net_version",
                "chain_id": chain_id,
                "net_version": chain_id.to_string(),
            })
        }
        "web3_clientVersion" => serde_json::json!({
            "method": "web3_clientVersion",
            "client_version": client_version,
        }),
        "getBlock" | "nov_getBlock" => {
            let height = param_as_u64(params, "height");
            let block = match height {
                Some(h) => db.blocks.iter().find(|b| b.height == h).cloned(),
                None => db.blocks.last().cloned(),
            };
            serde_json::json!({
                "method": method,
                "requested_height": height,
                "found": block.is_some(),
                "block": block,
            })
        }
        "getTransaction" | "nov_getTransaction" => {
            let tx_hash = param_as_string(params, "tx_hash")
                .or_else(|| param_as_string(params, "hash"))
                .unwrap_or_default()
                .to_ascii_lowercase();
            if tx_hash.is_empty() {
                bail!("tx_hash is required for {}", method);
            }
            let tx = db.txs.get(&tx_hash).cloned();
            serde_json::json!({
                "method": method,
                "tx_hash": tx_hash,
                "found": tx.is_some(),
                "transaction": tx,
            })
        }
        "eth_getTransactionByHash" => {
            let tx_hash = param_as_string(params, "tx_hash")
                .or_else(|| param_as_string(params, "hash"))
                .unwrap_or_default()
                .to_ascii_lowercase();
            if tx_hash.is_empty() {
                bail!("tx_hash is required for eth_getTransactionByHash");
            }
            let tx = db.txs.get(&tx_hash).cloned();
            serde_json::json!({
                "method": "eth_getTransactionByHash",
                "tx_hash": tx_hash,
                "found": tx.is_some(),
                "transaction": tx,
            })
        }
        "eth_blockNumber" => serde_json::json!({
            "method": "eth_blockNumber",
            "block_number": format!("0x{:x}", latest_height),
            "block_number_u64": latest_height,
        }),
        "eth_getBlockByNumber" => {
            let requested_height = parse_block_selector_u64(params, latest_height)?;
            let full_transactions = match params {
                serde_json::Value::Object(map) => map
                    .get("full_transactions")
                    .or_else(|| map.get("fullTransactions"))
                    .and_then(value_to_bool)
                    .unwrap_or(false),
                serde_json::Value::Array(arr) => {
                    arr.get(1).and_then(value_to_bool).unwrap_or(false)
                }
                _ => false,
            };
            let block = db
                .blocks
                .iter()
                .find(|b| b.height == requested_height)
                .cloned();
            serde_json::json!({
                "method": "eth_getBlockByNumber",
                "requested_height": requested_height,
                "requested_height_hex": format!("0x{:x}", requested_height),
                "full_transactions": full_transactions,
                "found": block.is_some(),
                "block": block,
            })
        }
        "getReceipt" | "nov_getReceipt" | "nov_getTransactionReceipt" => {
            let tx_hash = param_as_string(params, "tx_hash")
                .or_else(|| param_as_string(params, "hash"))
                .unwrap_or_default()
                .to_ascii_lowercase();
            if tx_hash.is_empty() {
                bail!("tx_hash is required for {}", method);
            }
            let receipt = if let Some(query_receipt) = db.receipts.get(&tx_hash).cloned() {
                Some(
                    serde_json::to_value(query_receipt)
                        .context("serialize query receipt for nov/getReceipt failed")?,
                )
            } else {
                get_nov_native_execution_receipt_by_hash_v1(tx_hash.as_str())?
                    .map(|native_receipt| {
                        let mut value = serde_json::to_value(native_receipt)
                            .context("serialize native nov receipt failed")?;
                        if let serde_json::Value::Object(map) = &mut value {
                            if let Ok(summary) = get_nov_native_treasury_settlement_summary_v1() {
                                map.insert("treasury_settlement".to_string(), summary);
                            }
                        }
                        Ok(value)
                    })
                    .transpose()?
            };
            serde_json::json!({
                "method": method,
                "tx_hash": tx_hash,
                "found": receipt.is_some(),
                "receipt": receipt,
            })
        }
        "eth_getTransactionReceipt" => {
            let tx_hash = param_as_string(params, "tx_hash")
                .or_else(|| param_as_string(params, "hash"))
                .unwrap_or_default()
                .to_ascii_lowercase();
            if tx_hash.is_empty() {
                bail!("tx_hash is required for eth_getTransactionReceipt");
            }
            let receipt = db.receipts.get(&tx_hash).cloned();
            serde_json::json!({
                "method": "eth_getTransactionReceipt",
                "tx_hash": tx_hash,
                "found": receipt.is_some(),
                "receipt": receipt,
            })
        }
        "getBalance" | "nov_getBalance" => {
            let account = param_as_string(params, "account").unwrap_or_default();
            let trimmed = account.trim();
            if trimmed.is_empty() {
                bail!("account is required for {}", method);
            }
            let balance = db.balances.get(trimmed).copied().unwrap_or(0);
            serde_json::json!({
                "method": method,
                "account": trimmed,
                "found": db.balances.contains_key(trimmed),
                "balance": balance,
            })
        }
        "nov_getAssetBalance" => {
            let account = param_as_string(params, "account").unwrap_or_default();
            let trimmed = account.trim();
            if trimmed.is_empty() {
                bail!("account is required for nov_getAssetBalance");
            }
            let asset = param_as_string(params, "asset")
                .or_else(|| param_as_string(params, "asset_id"))
                .unwrap_or_else(|| "NOV".to_string())
                .to_ascii_uppercase();
            let raw_balance = db.balances.get(trimmed).copied().unwrap_or(0);
            let balance = if asset == "NOV" { raw_balance } else { 0 };
            serde_json::json!({
                "method": "nov_getAssetBalance",
                "account": trimmed,
                "asset": asset,
                "found": db.balances.contains_key(trimmed),
                "balance": balance,
            })
        }
        "nov_getModuleInfo" => {
            let module = param_as_string(params, "module")
                .or_else(|| param_as_string(params, "name"))
                .unwrap_or_else(|| "treasury".to_string())
                .to_ascii_lowercase();
            let module_info = nov_native_module_info_v1(module.as_str());
            let found = module_info.is_some();
            serde_json::json!({
                "method": "nov_getModuleInfo",
                "module": module,
                "found": found,
                "module_info": module_info.unwrap_or(serde_json::Value::Null),
            })
        }
        "nov_getTreasurySettlementSummary" => {
            let summary = get_nov_native_treasury_settlement_summary_v1().ok();
            serde_json::json!({
                "method": "nov_getTreasurySettlementSummary",
                "found": summary.is_some(),
                "summary": summary.unwrap_or(serde_json::Value::Null),
            })
        }
        "nov_getTreasuryClearingSummary" => {
            let summary = get_nov_native_treasury_clearing_summary_v1().ok();
            serde_json::json!({
                "method": "nov_getTreasuryClearingSummary",
                "found": summary.is_some(),
                "summary": summary.unwrap_or(serde_json::Value::Null),
            })
        }
        "nov_getExecutionTrace" => {
            let tx_hash = param_as_string(params, "tx_hash")
                .or_else(|| param_as_string(params, "hash"))
                .unwrap_or_default();
            let method = if tx_hash.trim().is_empty() {
                "get_last_execution_trace"
            } else {
                "get_execution_trace_by_tx"
            };
            let args = if tx_hash.trim().is_empty() {
                serde_json::json!({})
            } else {
                serde_json::json!({ "tx_hash": tx_hash })
            };
            let out = run_nov_native_call_from_params_v1(&serde_json::json!({
                "target": {"kind": "native_module", "id": "treasury"},
                "method": method,
                "args": args,
            }))?;
            serde_json::json!({
                "method": "nov_getExecutionTrace",
                "found": out["found"].as_bool().unwrap_or(false),
                "trace": out["result"].clone(),
            })
        }
        "nov_getTreasuryClearingMetricsSummary" => {
            let out = run_nov_native_call_from_params_v1(&serde_json::json!({
                "target": {"kind": "native_module", "id": "treasury"},
                "method": "get_clearing_metrics_summary",
                "args": {},
            }))?;
            serde_json::json!({
                "method": "nov_getTreasuryClearingMetricsSummary",
                "found": out["found"].as_bool().unwrap_or(false),
                "summary": out["result"].clone(),
            })
        }
        "nov_getTreasuryPolicyMetricsSummary" => {
            let out = run_nov_native_call_from_params_v1(&serde_json::json!({
                "target": {"kind": "native_module", "id": "treasury"},
                "method": "get_policy_metrics_summary",
                "args": {},
            }))?;
            serde_json::json!({
                "method": "nov_getTreasuryPolicyMetricsSummary",
                "found": out["found"].as_bool().unwrap_or(false),
                "summary": out["result"].clone(),
            })
        }
        "nov_getTreasurySettlementPolicy" => {
            let out = run_nov_native_call_from_params_v1(&serde_json::json!({
                "target": {"kind": "native_module", "id": "treasury"},
                "method": "get_settlement_policy",
                "args": {},
            }))?;
            serde_json::json!({
                "method": "nov_getTreasurySettlementPolicy",
                "found": out["found"].as_bool().unwrap_or(false),
                "policy": out["result"].clone(),
            })
        }
        "nov_getTreasurySettlementJournal" => {
            let requested_limit = param_as_u64(params, "limit").unwrap_or(50);
            let out = run_nov_native_call_from_params_v1(&serde_json::json!({
                "target": {"kind": "native_module", "id": "treasury"},
                "method": "get_settlement_journal",
                "args": {
                    "limit": requested_limit,
                },
            }))?;
            serde_json::json!({
                "method": "nov_getTreasurySettlementJournal",
                "found": out["found"].as_bool().unwrap_or(false),
                "journal": out["result"].clone(),
            })
        }
        "nov_getState" => {
            let state = db.state_mirror_updates.last().cloned();
            serde_json::json!({
                "method": "nov_getState",
                "found": state.is_some(),
                "state": state,
            })
        }
        "eth_getBalance" => {
            let account = parse_eth_query_account(params).unwrap_or_default();
            let trimmed = account.trim();
            if trimmed.is_empty() {
                bail!("account/address is required for eth_getBalance");
            }
            let queried_height = parse_block_selector_u64(params, latest_height)?;
            let balance = db.balances.get(trimmed).copied().unwrap_or(0);
            serde_json::json!({
                "method": "eth_getBalance",
                "account": trimmed,
                "queried_height": queried_height,
                "queried_height_hex": format!("0x{:x}", queried_height),
                "found": db.balances.contains_key(trimmed),
                "balance": balance,
                "balance_hex": format!("0x{:x}", balance),
            })
        }
        "eth_getCode" => {
            let account = parse_eth_query_account(params).unwrap_or_default();
            let trimmed = account.trim();
            if trimmed.is_empty() {
                bail!("account/address is required for eth_getCode");
            }
            let queried_height = parse_block_selector_u64(params, latest_height)?;
            serde_json::json!({
                "method": "eth_getCode",
                "account": trimmed,
                "queried_height": queried_height,
                "queried_height_hex": format!("0x{:x}", queried_height),
                "found": false,
                "code": "0x",
            })
        }
        "eth_getStorageAt" => {
            let account = parse_eth_query_account(params).unwrap_or_default();
            let trimmed = account.trim();
            if trimmed.is_empty() {
                bail!("account/address is required for eth_getStorageAt");
            }
            let slot = parse_eth_storage_slot(params)?;
            let queried_height = parse_block_selector_at_array_index_u64(params, 2, latest_height)?;
            serde_json::json!({
                "method": "eth_getStorageAt",
                "account": trimmed,
                "slot": slot,
                "slot_hex": format!("0x{:x}", slot),
                "queried_height": queried_height,
                "queried_height_hex": format!("0x{:x}", queried_height),
                "value": format!("0x{:064x}", 0u64),
            })
        }
        "eth_call" => {
            let call_obj = parse_eth_call_object(params);
            let to = param_as_string(&call_obj, "to").unwrap_or_default();
            let from = param_as_string(&call_obj, "from").unwrap_or_default();
            let data = param_as_string(&call_obj, "data")
                .or_else(|| param_as_string(&call_obj, "input"))
                .unwrap_or_else(|| "0x".to_string());
            let value = param_as_string(&call_obj, "value").unwrap_or_else(|| "0x0".to_string());
            let queried_height = parse_block_selector_at_array_index_u64(params, 1, latest_height)?;
            serde_json::json!({
                "method": method,
                "to": to,
                "from": from,
                "data": data,
                "value": value,
                "queried_height": queried_height,
                "queried_height_hex": format!("0x{:x}", queried_height),
                "result": "0x",
            })
        }
        "nov_call" => {
            if has_nov_native_call_shape_v1(params) {
                run_nov_native_call_from_params_v1(params)?
            } else {
                let call_obj = parse_eth_call_object(params);
                let to = param_as_string(&call_obj, "to").unwrap_or_default();
                let from = param_as_string(&call_obj, "from").unwrap_or_default();
                let data = param_as_string(&call_obj, "data")
                    .or_else(|| param_as_string(&call_obj, "input"))
                    .unwrap_or_else(|| "0x".to_string());
                let value =
                    param_as_string(&call_obj, "value").unwrap_or_else(|| "0x0".to_string());
                let queried_height =
                    parse_block_selector_at_array_index_u64(params, 1, latest_height)?;
                serde_json::json!({
                    "method": method,
                    "to": to,
                    "from": from,
                    "data": data,
                    "value": value,
                    "queried_height": queried_height,
                    "queried_height_hex": format!("0x{:x}", queried_height),
                    "result": "0x",
                })
            }
        }
        "eth_estimateGas" | "nov_estimateGas" | "nov_estimate" => {
            let call_obj = parse_eth_call_object(params);
            let data = param_as_string(&call_obj, "data")
                .or_else(|| param_as_string(&call_obj, "input"))
                .unwrap_or_else(|| "0x".to_string());
            let data_len = hex_to_data_len(&data) as u64;
            let estimated = 21_000u64.saturating_add(data_len.saturating_mul(16));
            let queried_height = parse_block_selector_at_array_index_u64(params, 1, latest_height)?;
            serde_json::json!({
                "method": method,
                "queried_height": queried_height,
                "queried_height_hex": format!("0x{:x}", queried_height),
                "estimated_gas": estimated,
                "estimated_gas_hex": format!("0x{:x}", estimated),
            })
        }
        "eth_gasPrice" => serde_json::json!({
            "method": "eth_gasPrice",
            "gas_price": compat_gas_price,
            "gas_price_hex": format!("0x{:x}", compat_gas_price),
        }),
        "eth_maxPriorityFeePerGas" => serde_json::json!({
            "method": "eth_maxPriorityFeePerGas",
            "max_priority_fee_per_gas": compat_priority_fee,
            "max_priority_fee_per_gas_hex": format!("0x{:x}", compat_priority_fee),
        }),
        "eth_feeHistory" => {
            let block_count = match params {
                serde_json::Value::Object(map) => map
                    .get("block_count")
                    .or_else(|| map.get("blockCount"))
                    .and_then(value_to_u64)
                    .unwrap_or(1),
                serde_json::Value::Array(arr) => arr.first().and_then(value_to_u64).unwrap_or(1),
                _ => 1,
            }
            .clamp(1, 128);
            let newest = parse_block_selector_at_array_index_u64(params, 1, latest_height)?;
            let reward_percentiles = parse_eth_reward_percentile_count(params);
            let base_fee_per_gas: Vec<String> = (0..=block_count)
                .map(|_| format!("0x{:x}", compat_gas_price))
                .collect();
            let gas_used_ratio: Vec<f64> = (0..block_count).map(|_| 0.0).collect();
            let reward = if reward_percentiles > 0 {
                Some(
                    (0..block_count)
                        .map(|_| vec![format!("0x{:x}", compat_priority_fee); reward_percentiles])
                        .collect::<Vec<Vec<String>>>(),
                )
            } else {
                None
            };
            serde_json::json!({
                "method": "eth_feeHistory",
                "block_count": block_count,
                "oldest_block": newest.saturating_sub(block_count.saturating_sub(1)),
                "oldest_block_hex": format!("0x{:x}", newest.saturating_sub(block_count.saturating_sub(1))),
                "newest_block": newest,
                "newest_block_hex": format!("0x{:x}", newest),
                "baseFeePerGas": base_fee_per_gas,
                "gasUsedRatio": gas_used_ratio,
                "reward": reward,
            })
        }
        _ => bail!(
            "unknown method: {}; valid: novovm_getSurfaceMap|novovm_get_surface_map|novovm_getMethodDomain|novovm_get_method_domain|nov_getBlock|nov_getTransaction|nov_getReceipt|nov_getTransactionReceipt|nov_getBalance|nov_getAssetBalance|nov_getState|nov_getModuleInfo|nov_getTreasurySettlementSummary|nov_getTreasurySettlementJournal|nov_getTreasurySettlementPolicy|nov_call|nov_estimate|nov_estimateGas|getBlock|getTransaction|getReceipt|getBalance|eth_chainId|net_version|web3_clientVersion|eth_blockNumber|eth_getBlockByNumber|eth_getBalance|eth_getCode|eth_getStorageAt|eth_call|eth_estimateGas|eth_gasPrice|eth_maxPriorityFeePerGas|eth_feeHistory",
            method
        ),
    };
    Ok(response)
}

fn run_unified_account_rpc(
    router: &mut UnifiedAccountRouter,
    audit_sink: Option<&UnifiedAccountAuditSinkBackend>,
    method: &str,
    params: &serde_json::Value,
) -> Result<(serde_json::Value, bool)> {
    match method {
        "ua_createUca" => {
            let uca_id_raw = param_as_string(params, "uca_id")
                .ok_or_else(|| anyhow::anyhow!("uca_id is required for ua_createUca"))?;
            let uca_id = validate_uca_id_policy(&uca_id_raw)?;
            let now = param_as_u64(params, "now").unwrap_or_else(now_unix_sec);
            let primary_key_ref = parse_primary_key_ref(params, &uca_id)?;
            router.create_uca(uca_id.clone(), primary_key_ref, now)?;
            Ok((
                serde_json::json!({
                    "method": method,
                    "created": true,
                    "uca_id": uca_id,
                }),
                true,
            ))
        }
        "ua_rotatePrimaryKey" => {
            let uca_id = param_as_string(params, "uca_id")
                .ok_or_else(|| anyhow::anyhow!("uca_id is required for ua_rotatePrimaryKey"))?;
            let role = parse_account_role(params)?;
            let now = param_as_u64(params, "now").unwrap_or_else(now_unix_sec);
            let next_primary_key_ref = if let Some(raw) = param_as_string(params, "next_primary_key_ref")
            {
                decode_hex_bytes(&raw, "next_primary_key_ref")?
            } else {
                parse_primary_key_ref(params, &format!("{}:rotated:{}", uca_id, now))?
            };
            router.rotate_primary_key(&uca_id, role, next_primary_key_ref, now)?;
            Ok((
                serde_json::json!({
                    "method": method,
                    "rotated": true,
                    "uca_id": uca_id,
                }),
                true,
            ))
        }
        "ua_setPolicy" => {
            let uca_id = param_as_string(params, "uca_id")
                .ok_or_else(|| anyhow::anyhow!("uca_id is required for ua_setPolicy"))?;
            let role = parse_account_role(params)?;
            let nonce_scope = match param_as_string(params, "nonce_scope")
                .unwrap_or_else(|| "persona".to_string())
                .to_ascii_lowercase()
                .as_str()
            {
                "persona" => NonceScope::Persona,
                "chain" => NonceScope::Chain,
                "global" => NonceScope::Global,
                other => bail!("invalid nonce_scope: {}; valid: persona|chain|global", other),
            };
            let allow_type4_with_delegate_or_session =
                param_as_bool(params, "allow_type4_with_delegate_or_session").unwrap_or(false);
            let type4_policy_mode = if let Some(raw) = param_as_string(params, "type4_policy_mode")
            {
                match raw.trim().to_ascii_lowercase().as_str() {
                    "supported" => Type4PolicyMode::Supported,
                    "rejected" => Type4PolicyMode::Rejected,
                    "degraded" => Type4PolicyMode::Degraded,
                    other => bail!(
                        "invalid type4_policy_mode: {}; valid: supported|rejected|degraded",
                        other
                    ),
                }
            } else if allow_type4_with_delegate_or_session {
                Type4PolicyMode::Supported
            } else {
                Type4PolicyMode::Rejected
            };
            let kyc_policy_mode = if let Some(raw) = param_as_string(params, "kyc_policy_mode") {
                match raw.trim().to_ascii_lowercase().as_str() {
                    "disabled" => KycPolicyMode::Disabled,
                    "informational" => KycPolicyMode::Informational,
                    "required_non_owner" | "required-non-owner" | "requiredfornonowner" => {
                        KycPolicyMode::RequiredForNonOwner
                    }
                    other => bail!(
                        "invalid kyc_policy_mode: {}; valid: disabled|informational|required_non_owner",
                        other
                    ),
                }
            } else {
                KycPolicyMode::Disabled
            };
            let now = param_as_u64(params, "now").unwrap_or_else(now_unix_sec);
            let policy = AccountPolicy {
                nonce_scope,
                type4_policy_mode,
                allow_type4_with_delegate_or_session,
                kyc_policy_mode,
            };
            router.update_policy(&uca_id, role, policy, now)?;
            Ok((
                serde_json::json!({
                    "method": method,
                    "updated": true,
                    "uca_id": uca_id,
                    "nonce_scope": match nonce_scope {
                        NonceScope::Persona => "persona",
                        NonceScope::Chain => "chain",
                        NonceScope::Global => "global",
                    },
                    "type4_policy_mode": match type4_policy_mode {
                        Type4PolicyMode::Supported => "supported",
                        Type4PolicyMode::Rejected => "rejected",
                        Type4PolicyMode::Degraded => "degraded",
                    },
                    "kyc_policy_mode": match kyc_policy_mode {
                        KycPolicyMode::Disabled => "disabled",
                        KycPolicyMode::Informational => "informational",
                        KycPolicyMode::RequiredForNonOwner => "required_non_owner",
                    },
                    "allow_type4_with_delegate_or_session": allow_type4_with_delegate_or_session,
                }),
                true,
            ))
        }
        "ua_bindPersona" => {
            let uca_id = param_as_string(params, "uca_id")
                .ok_or_else(|| anyhow::anyhow!("uca_id is required for ua_bindPersona"))?;
            let role = parse_account_role(params)?;
            let persona_type = parse_persona_type(params, "persona_type")?;
            let chain_id = param_as_u64(params, "chain_id")
                .ok_or_else(|| anyhow::anyhow!("chain_id is required for ua_bindPersona"))?;
            let external_address = parse_external_address(params, "external_address")?;
            let now = param_as_u64(params, "now").unwrap_or_else(now_unix_sec);
            let persona = PersonaAddress {
                persona_type,
                chain_id,
                external_address,
            };
            router.add_binding(&uca_id, role, persona.clone(), now)?;
            Ok((
                serde_json::json!({
                    "method": method,
                    "bound": true,
                    "uca_id": uca_id,
                    "persona_type": persona.persona_type.as_str(),
                    "chain_id": persona.chain_id,
                }),
                true,
            ))
        }
        "ua_revokePersona" => {
            let uca_id = param_as_string(params, "uca_id")
                .ok_or_else(|| anyhow::anyhow!("uca_id is required for ua_revokePersona"))?;
            let role = parse_account_role(params)?;
            let persona_type = parse_persona_type(params, "persona_type")?;
            let chain_id = param_as_u64(params, "chain_id")
                .ok_or_else(|| anyhow::anyhow!("chain_id is required for ua_revokePersona"))?;
            let external_address = parse_external_address(params, "external_address")?;
            let cooldown_seconds = param_as_u64(params, "cooldown_seconds").unwrap_or(0);
            let now = param_as_u64(params, "now").unwrap_or_else(now_unix_sec);
            let persona = PersonaAddress {
                persona_type,
                chain_id,
                external_address,
            };
            router.revoke_binding(&uca_id, role, persona.clone(), cooldown_seconds, now)?;
            Ok((
                serde_json::json!({
                    "method": method,
                    "revoked": true,
                    "uca_id": uca_id,
                    "persona_type": persona.persona_type.as_str(),
                    "chain_id": persona.chain_id,
                    "cooldown_seconds": cooldown_seconds,
                }),
                true,
            ))
        }
        "ua_getBindingOwner" => {
            let persona_type = parse_persona_type(params, "persona_type")?;
            let chain_id = param_as_u64(params, "chain_id")
                .ok_or_else(|| anyhow::anyhow!("chain_id is required for ua_getBindingOwner"))?;
            let external_address = parse_external_address(params, "external_address")?;
            let persona = PersonaAddress {
                persona_type,
                chain_id,
                external_address,
            };
            let owner = router.resolve_binding_owner(&persona).map(str::to_string);
            Ok((
                serde_json::json!({
                    "method": method,
                    "found": owner.is_some(),
                    "owner_uca_id": owner,
                    "persona_type": persona.persona_type.as_str(),
                    "chain_id": persona.chain_id,
                }),
                false,
            ))
        }
        "ua_getAuditEvents" => {
            let source = param_as_string(params, "source")
                .unwrap_or_else(|| {
                    if audit_sink.is_some() {
                        "sink".to_string()
                    } else {
                        "router".to_string()
                    }
                })
                .to_ascii_lowercase();
            let limit = param_as_u64(params, "limit").unwrap_or(50).clamp(1, 500) as usize;
            let filter = UnifiedAccountAuditQueryFilter::from_rpc_params(params)?;
            if source == "router" {
                let clear = param_as_bool(params, "clear").unwrap_or(false);
                let mut events: Vec<_> = if clear {
                    router.take_events()
                } else {
                    router.events().to_vec()
                };
                if filter.requires_event_match() {
                    events.retain(|event| filter.matches_router_event(event));
                }
                if events.len() > limit {
                    let start = events.len().saturating_sub(limit);
                    events = events[start..].to_vec();
                }
                let events_json = events
                    .iter()
                    .map(account_audit_event_to_json)
                    .collect::<Result<Vec<_>>>()?;
                return Ok((
                    serde_json::json!({
                        "method": method,
                        "source": "router",
                        "count": events_json.len(),
                        "clear": clear,
                        "filter": filter.to_json(),
                        "events": events_json,
                    }),
                    clear,
                ));
            }
            if source == "sink" {
                let sink = audit_sink.ok_or_else(|| {
                    anyhow::anyhow!("audit sink is not available for ua_getAuditEvents")
                })?;
                let since_seq = param_as_u64(params, "since_seq").unwrap_or(0);
                let (head_seq, events, next_since_seq, has_more) =
                    load_unified_account_audit_records_for_rpc(sink, since_seq, limit, &filter)?;
                return Ok((
                    serde_json::json!({
                        "method": method,
                        "source": "sink",
                        "backend": sink.backend_name(),
                        "path": sink.path().display().to_string(),
                        "since_seq": since_seq,
                        "filter": filter.to_json(),
                        "head_seq": head_seq,
                        "next_since_seq": next_since_seq,
                        "has_more": has_more,
                        "cursor": next_since_seq,
                        "count": events.len(),
                        "events": events,
                    }),
                    false,
                ));
            }
            bail!(
                "invalid source for ua_getAuditEvents: {}; valid: router|sink",
                source
            );
        }
        "eth_getTransactionCount" => {
            let chain_id = param_as_u64(params, "chain_id").unwrap_or(1);
            let address_raw = extract_eth_persona_address_param(params)
                .ok_or_else(|| anyhow::anyhow!("address (or from/external_address) is required"))?;
            let external_address = decode_hex_bytes(&address_raw, "address")?;
            let persona = PersonaAddress {
                persona_type: PersonaType::Evm,
                chain_id,
                external_address: external_address.clone(),
            };
            let owner = router.resolve_binding_owner(&persona).map(str::to_string);
            let explicit_uca_id = param_as_string(params, "uca_id");
            let uca_id = match (explicit_uca_id, owner) {
                (Some(explicit), Some(owner_id)) => {
                    if explicit != owner_id {
                        bail!(
                            "uca_id mismatch for address binding: explicit={} binding_owner={}",
                            explicit,
                            owner_id
                        );
                    }
                    explicit
                }
                (Some(explicit), None) => {
                    bail!(
                        "binding not found for address on chain_id={} (uca_id={})",
                        chain_id,
                        explicit
                    );
                }
                (None, Some(owner_id)) => owner_id,
                (None, None) => {
                    bail!("binding not found for address on chain_id={}", chain_id);
                }
            };
            let nonce = router.next_nonce_for_persona(&uca_id, &persona)?;
            Ok((
                serde_json::json!({
                    "method": method,
                    "uca_id": uca_id,
                    "chain_id": chain_id,
                    "address": format!("0x{}", to_hex(&external_address)),
                    "nonce": nonce,
                    "nonce_hex": format!("0x{:x}", nonce),
                }),
                false,
            ))
        }
        m if m == "ua_route"
            || is_unified_account_eth_route_method(m)
            || is_unified_account_web30_route_method(m)
            || is_unified_account_nov_route_method(m) =>
        {
            let explicit_uca_id = param_as_string(params, "uca_id");
            let role = parse_account_role(params)?;
            let is_eth_alias = is_unified_account_eth_route_method(method);
            let is_eth_raw_alias = method == "eth_sendRawTransaction";
            let is_nov_alias = is_unified_account_nov_route_method(method);
            let is_nov_raw_alias = method == "nov_sendRawTransaction";
            let is_nov_execute_alias = method == "nov_execute";
            let default_eth_chain_id =
                parse_u64_decimal_or_hex(&string_env("NOVOVM_ETH_COMPAT_CHAIN_ID", "1"))
                    .unwrap_or(1);
            let default_nov_chain_id =
                parse_u64_decimal_or_hex(&string_env("NOVOVM_NOV_CHAIN_ID", "1"))
                    .unwrap_or(default_chain_id);
            let inferred_eth_tx_type = if is_eth_alias && is_eth_raw_alias {
                infer_eth_raw_tx_type_hint(params)?
            } else {
                None
            };
            let explicit_chain_id = param_as_u64(params, "chain_id");
            let inferred_chain_id = inferred_eth_tx_type
                .as_ref()
                .and_then(|hint| hint.fields.chain_id);
            if let (Some(explicit), Some(inferred)) = (explicit_chain_id, inferred_chain_id) {
                if explicit != inferred {
                    bail!(
                        "chain_id mismatch: explicit={} inferred_from_raw={}",
                        explicit,
                        inferred
                    );
                }
            }
            let chain_id = if is_eth_alias {
                explicit_chain_id
                    .or(inferred_chain_id)
                    .unwrap_or(default_eth_chain_id)
            } else if is_nov_alias {
                explicit_chain_id.or(inferred_chain_id).unwrap_or(default_nov_chain_id)
            } else {
                explicit_chain_id
                    .or(inferred_chain_id)
                    .ok_or_else(|| anyhow::anyhow!("chain_id is required for route methods"))?
            };
            let is_web30_alias = is_unified_account_web30_route_method(method);
            let persona_type = if is_eth_alias {
                PersonaType::Evm
            } else if is_web30_alias || is_nov_alias {
                PersonaType::Web30
            } else if param_as_string(params, "persona_type").is_some() {
                parse_persona_type(params, "persona_type")?
            } else {
                PersonaType::Other("unknown".to_string())
            };
            let external_address = if let Some(external_address_raw) = param_as_string(params, "external_address")
                .or_else(|| param_as_string(params, "from"))
            {
                decode_hex_bytes(&external_address_raw, "external_address")?
            } else if is_eth_alias && is_eth_raw_alias {
                inferred_eth_tx_type
                    .as_ref()
                    .map(|hint| hint.fields.from.clone())
                    .filter(|from| !from.is_empty())
                    .ok_or_else(|| {
                        anyhow::anyhow!(
                            "external_address (or from) is required for route methods"
                        )
                    })?
            } else {
                bail!("external_address (or from) is required for route methods");
            };
            let protocol = if is_eth_alias {
                ProtocolKind::Eth
            } else if is_web30_alias || is_nov_alias {
                ProtocolKind::Web30
            } else {
                match param_as_string(params, "protocol")
                    .unwrap_or_else(|| "other".to_string())
                    .to_ascii_lowercase()
                    .as_str()
                {
                    "eth" => ProtocolKind::Eth,
                    "web30" => ProtocolKind::Web30,
                    other => ProtocolKind::Other(other.to_string()),
                }
            };
            let signature_domain = param_as_string(params, "signature_domain").unwrap_or_else(|| {
                match protocol {
                    ProtocolKind::Eth => format!("evm:{}", chain_id),
                    ProtocolKind::Web30 => "web30:mainnet".to_string(),
                    ProtocolKind::Other(_) => format!("{}:{}", persona_type.as_str(), chain_id),
                }
            });
            let explicit_nonce = param_as_u64(params, "nonce");
            let inferred_nonce = inferred_eth_tx_type
                .as_ref()
                .and_then(|hint| hint.fields.nonce);
            if let (Some(explicit), Some(inferred)) = (explicit_nonce, inferred_nonce) {
                if explicit != inferred {
                    bail!(
                        "nonce mismatch: explicit={} inferred_from_raw={}",
                        explicit,
                        inferred
                    );
                }
            }
            let persona = PersonaAddress {
                persona_type: persona_type.clone(),
                chain_id,
                external_address: external_address.clone(),
            };
            let binding_owner = router.resolve_binding_owner(&persona).map(str::to_string);
            let uca_id = match (explicit_uca_id, binding_owner) {
                (Some(explicit), Some(owner_id)) => {
                    if explicit != owner_id {
                        bail!(
                            "uca_id mismatch for address binding: explicit={} binding_owner={}",
                            explicit,
                            owner_id
                        );
                    }
                    explicit
                }
                (Some(explicit), None) => explicit,
                (None, Some(owner_id)) if is_eth_alias => owner_id,
                (None, Some(_)) => {
                    bail!("uca_id is required for route methods (ua_route/eth/web30)")
                }
                (None, None) if is_eth_alias => {
                    bail!(
                        "uca_id is required or address binding must exist for route methods"
                    )
                }
                (None, None) => {
                    bail!("uca_id is required for route methods (ua_route/eth/web30)")
                }
            };
            let nonce = match explicit_nonce.or(inferred_nonce) {
                Some(nonce) => nonce,
                None if is_eth_alias || is_nov_alias => {
                    router.next_nonce_for_persona(&uca_id, &persona)?
                }
                None => bail!("nonce is required for route methods"),
            };
            let wants_cross_chain_atomic =
                param_as_bool(params, "wants_cross_chain_atomic").unwrap_or(false);
            let explicit_tx_type = param_as_u64(params, "tx_type");
            let inferred_tx_type = inferred_eth_tx_type
                .as_ref()
                .map(|hint| hint.fields.hint.tx_type_number as u64);
            if let (Some(explicit), Some(inferred)) = (explicit_tx_type, inferred_tx_type) {
                if explicit != inferred {
                    bail!(
                        "tx_type mismatch: explicit={} inferred_from_raw={}",
                        explicit,
                        inferred
                    );
                }
            }
            let tx_type = explicit_tx_type.or(inferred_tx_type);
            let gas_limit = param_as_u64_any(params, &["gas_limit", "gasLimit", "gas"])
                .or(inferred_eth_tx_type.as_ref().and_then(|hint| hint.fields.gas_limit));
            let tx_type4 = param_as_bool(params, "tx_type4").unwrap_or(false)
                || explicit_tx_type.map(|v| v == 4).unwrap_or(false)
                || inferred_eth_tx_type
                    .as_ref()
                    .map(|hint| hint.fields.hint.tx_type4)
                    .unwrap_or(false);
            let kyc = resolve_node_kyc_verification(
                params,
                &uca_id,
                chain_id,
                &external_address,
                role,
                nonce,
            )?;
            let inferred_eth_tx_ir = if is_eth_alias {
                if is_eth_raw_alias {
                    inferred_eth_tx_type.as_ref().map(|hint| {
                        tx_ir_from_raw_fields_m0(
                            &hint.fields,
                            &hint.raw_tx,
                            external_address.clone(),
                            chain_id,
                        )
                    })
                } else {
                    Some(parse_eth_send_transaction_ir(
                        params,
                        external_address.clone(),
                        chain_id,
                        nonce,
                        tx_type4,
                    )?)
                }
            } else {
                None
            };
            let inferred_eth_tx_ir_type = inferred_eth_tx_ir
                .as_ref()
                .map(|tx| tx_type_label(tx.tx_type));
            let inferred_eth_tx_ir_data_len = inferred_eth_tx_ir
                .as_ref()
                .map(|tx| tx.data.len() as u64);
            let session_expires_at = param_as_u64(params, "session_expires_at");
            let now = param_as_u64(params, "now").unwrap_or_else(now_unix_sec);

            let request = RouteRequest {
                uca_id: uca_id.clone(),
                persona,
                role,
                protocol,
                signature_domain: signature_domain.clone(),
                nonce,
                kyc_attestation_provided: kyc.provided,
                kyc_verified: kyc.verified,
                wants_cross_chain_atomic,
                tx_type4,
                session_expires_at,
                now,
            };
            let decision = router.route(request)?;
            let mut local_pending_tx_hash_hex: Option<String> = None;
            let mut local_pending_tx_ingress = false;
            let mut nov_execution_request: Option<serde_json::Value> = None;
            if is_eth_alias && is_eth_raw_alias {
                if let Some(hint) = inferred_eth_tx_type.as_ref() {
                    let raw_tx_hex = format!("0x{}", to_hex(hint.raw_tx.as_slice()));
                    let ingress = run_eth_send_raw_transaction_from_params_v1(
                        &serde_json::json!({
                            "raw_tx": raw_tx_hex,
                            "chain_id": chain_id,
                        }),
                    )?;
                    local_pending_tx_hash_hex = ingress
                        .get("pending_tx_hash")
                        .and_then(|value| value.as_str())
                        .map(str::to_string);
                    local_pending_tx_ingress = ingress
                        .get("pending_tx_local_ingress")
                        .and_then(|value| value.as_bool())
                        .unwrap_or(false);
                }
            } else if is_nov_alias {
                let ingress = if is_nov_raw_alias {
                    run_nov_send_raw_transaction_from_params_v1(params)?
                } else if is_nov_execute_alias {
                    novovm_node::tx_ingress::run_nov_execute_from_params_v1(params)?
                } else {
                    run_nov_send_transaction_from_params_v1(params)?
                };
                local_pending_tx_hash_hex = ingress
                    .get("pending_tx_hash")
                    .and_then(|value| value.as_str())
                    .map(str::to_string);
                local_pending_tx_ingress = ingress
                    .get("pending_tx_local_ingress")
                    .and_then(|value| value.as_bool())
                    .unwrap_or(false);
                nov_execution_request = ingress.get("execution_request").cloned();
            }
            Ok((
                serde_json::json!({
                    "method": method,
                    "accepted": true,
                    "uca_id": uca_id,
                    "decision": match decision {
                        RouteDecision::FastPath => serde_json::json!({"kind": "fast_path"}),
                        RouteDecision::Adapter { chain_id } => serde_json::json!({"kind": "adapter", "chain_id": chain_id}),
                    },
                    "signature_domain": signature_domain,
                    "nonce": nonce,
                    "gas_limit": gas_limit,
                    "tx_type": tx_type,
                    "tx_type4": tx_type4,
                    "kyc_attestation_provided": kyc.provided,
                    "kyc_verified": kyc.verified,
                    "tx_ir_type": inferred_eth_tx_ir_type,
                    "tx_ir_data_len": inferred_eth_tx_ir_data_len,
                    "session_expires_at": session_expires_at,
                    "pending_tx_local_ingress": local_pending_tx_ingress,
                    "pending_tx_hash": local_pending_tx_hash_hex,
                    "nov_execution_request": nov_execution_request,
                }),
                true,
            ))
        }
        _ => bail!(
            "unknown unified account method: {}; valid: ua_createUca|ua_rotatePrimaryKey|ua_setPolicy|ua_bindPersona|ua_revokePersona|ua_getBindingOwner|ua_getAuditEvents|ua_route|eth_sendRawTransaction|eth_sendTransaction|eth_getTransactionCount|web30_sendTransaction|web30_sendRawTransaction|nov_sendTransaction|nov_sendRawTransaction|nov_execute",
            method
        ),
    }
}

fn run_public_rpc(
    db: &QueryStateDb,
    router: &mut UnifiedAccountRouter,
    audit_sink: Option<&UnifiedAccountAuditSinkBackend>,
    method: &str,
    params: &serde_json::Value,
) -> Result<(serde_json::Value, bool)> {
    if is_unified_account_method(method) {
        return run_unified_account_rpc(router, audit_sink, method, params);
    }
    if is_eth_filter_or_reorg_method_m0(method) {
        bail!("unsupported eth filter/reorg method in M0: {}", method);
    }
    if method.starts_with("eth_") && !is_eth_plugin_allowed_method(method) {
        bail!(
            "eth method not enabled in supervm public rpc plugin scope: {}",
            method
        );
    }
    let out = run_chain_query(db, method, params)?;
    Ok((out, false))
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
    let public_db = match role {
        RpcServerRole::Public => Some(load_query_state_db(&db_path)?),
        RpcServerRole::Governance => None,
    };
    let mut public_unified_account_runtime = match role {
        RpcServerRole::Public => {
            let store = resolve_unified_account_store(&db_path)?;
            let snapshot = store.load_snapshot()?;
            let audit_sink = resolve_unified_account_audit_sink(&db_path)?;
            Some(UnifiedAccountRuntime {
                store,
                snapshot,
                audit_sink,
            })
        }
        RpcServerRole::Governance => None,
    };

    let governance_audit_db = governance_runtime
        .as_ref()
        .map(|runtime| runtime.audit_store_path.display().to_string())
        .unwrap_or_else(|| "-".to_string());
    let governance_chain_audit_db = governance_runtime
        .as_ref()
        .map(|runtime| runtime.chain_audit_store_path.display().to_string())
        .unwrap_or_else(|| "-".to_string());
    let ua_store_backend = public_unified_account_runtime
        .as_ref()
        .map(|runtime| runtime.store.backend_name().to_string())
        .unwrap_or_else(|| "-".to_string());
    let ua_db_display = public_unified_account_runtime
        .as_ref()
        .map(|runtime| runtime.store.path().display().to_string())
        .unwrap_or_else(|| "-".to_string());
    let ua_audit_backend = public_unified_account_runtime
        .as_ref()
        .map(|runtime| runtime.audit_sink.backend_name().to_string())
        .unwrap_or_else(|| "-".to_string());
    let ua_audit_display = public_unified_account_runtime
        .as_ref()
        .map(|runtime| runtime.audit_sink.path().display().to_string())
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
        "rpc_server_in: role={} bind={} db={} ua_store={} ua_db={} ua_audit_backend={} ua_audit={} max_body={} rate_limit_per_ip={} max_requests={} gov_allowlist_count={} governance_audit_db={} governance_chain_audit_db={} governance_execution_enabled={}",
        role.as_str(),
        bind,
        db_path.display(),
        ua_store_backend,
        ua_db_display,
        ua_audit_backend,
        ua_audit_display,
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
                let db = public_db
                    .as_ref()
                    .ok_or_else(|| anyhow::anyhow!("missing public query db"))?;
                let runtime = public_unified_account_runtime
                    .as_mut()
                    .ok_or_else(|| anyhow::anyhow!("missing unified account runtime"))?;

                let before_event_count = runtime.snapshot.router.events().len() as u64;
                let before_flushed_event_count = runtime.snapshot.flushed_event_count;
                let public_result = run_public_rpc(
                    db,
                    &mut runtime.snapshot.router,
                    Some(&runtime.audit_sink),
                    &method,
                    &params,
                );
                let after_event_count = runtime.snapshot.router.events().len() as u64;
                let mut router_changed = match &public_result {
                    Ok((_, changed)) => *changed,
                    Err(_) => false,
                };
                if after_event_count != before_event_count {
                    router_changed = true;
                }

                if is_unified_account_method(&method) {
                    let (router_events, next_cursor) = unified_account_events_since(
                        &runtime.snapshot.router,
                        runtime.snapshot.flushed_event_count,
                    );
                    let audit_record = UnifiedAccountAuditSinkRecord {
                        at: now_unix_sec(),
                        source: "public_rpc".to_string(),
                        method: method.clone(),
                        success: public_result.is_ok(),
                        router_changed,
                        event_cursor_from: runtime.snapshot.flushed_event_count,
                        event_cursor_to: next_cursor,
                        router_events,
                        params: params.clone(),
                        error: public_result.as_ref().err().map(|err| err.to_string()),
                    };
                    runtime.audit_sink.append_record(&audit_record)?;
                    runtime.snapshot.flushed_event_count = next_cursor;
                }

                if router_changed
                    || runtime.snapshot.flushed_event_count != before_flushed_event_count
                {
                    runtime.store.save_snapshot(&runtime.snapshot)?;
                }

                match public_result {
                    Ok((result, _)) => Ok(result),
                    Err(err) => Err(err),
                }
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
                    if let Err(persist_err) = save_governance_chain_audit_store(
                        &runtime.chain_audit_store_path,
                        &chain_events,
                        chain_root,
                    ) {
                        if governance_result.is_ok() {
                            governance_result =
                                Err(persist_err
                                    .context("persist governance chain audit store failed"));
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
            Err(e) => {
                let message = e.to_string();
                let code = if role == RpcServerRole::Public {
                    public_rpc_error_code_for_method(&method, &message)
                } else {
                    -32602
                };
                serde_json::json!({
                    "jsonrpc": "2.0",
                    "id": request_id,
                    "error": {
                        "code": code,
                        "message": message,
                    }
                })
            }
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

fn run_unified_account_audit_migrate_mode() -> Result<()> {
    let persistence = ensure_d2d3_persistence_through_d1()?;
    let query_db_path = chain_query_db_path();
    let from_backend = string_env(
        "NOVOVM_UA_AUDIT_MIGRATE_FROM",
        UNIFIED_ACCOUNT_AUDIT_BACKEND_JSONL,
    )
    .trim()
    .to_ascii_lowercase();
    let to_backend = string_env(
        "NOVOVM_UA_AUDIT_MIGRATE_TO",
        UNIFIED_ACCOUNT_AUDIT_BACKEND_ROCKSDB,
    )
    .trim()
    .to_ascii_lowercase();
    let source = resolve_unified_account_audit_sink_with_backend(&query_db_path, &from_backend)?;
    let target = resolve_unified_account_audit_sink_with_backend(&query_db_path, &to_backend)?;
    let (source_head, target_head_before, appended, target_head_after) =
        migrate_unified_account_audit_records(&source, &target)?;
    println!(
        "ua_audit_migrate_out: from_backend={} from_path={} from_head={} to_backend={} to_path={} target_head_before={} appended={} target_head_after={} d1_enforce={} d1_variant={} d1_rocksdb={} d1_root={}",
        source.backend_name(),
        source.path().display(),
        source_head,
        target.backend_name(),
        target.path().display(),
        target_head_before,
        appended,
        target_head_after,
        persistence.enforce,
        persistence.variant.as_str(),
        persistence.rocksdb_persistence,
        persistence.persistence_root.display()
    );
    Ok(())
}

fn run_chain_query_mode() -> Result<()> {
    let persistence = ensure_d2d3_persistence_through_d1()?;
    let db_path = chain_query_db_path();
    let db = load_query_state_db(&db_path)?;
    let method = string_env("NOVOVM_CHAIN_QUERY_METHOD", "")
        .trim()
        .to_string();
    if method.is_empty() {
        bail!(
            "missing NOVOVM_CHAIN_QUERY_METHOD; valid: novovm_getSurfaceMap|novovm_get_surface_map|novovm_getMethodDomain|novovm_get_method_domain|getBlock|getTransaction|getReceipt|getBalance|eth_chainId|net_version|web3_clientVersion|eth_blockNumber|eth_getBlockByNumber|eth_getBalance|eth_getCode|eth_getStorageAt|eth_call|eth_estimateGas|eth_gasPrice|eth_maxPriorityFeePerGas|eth_feeHistory"
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
        "chain_query_out: db={} method={} blocks={} txs={} receipts={} balances={} d1_enforce={} d1_variant={} d1_rocksdb={} d1_root={}",
        db_path.display(),
        method,
        db.blocks.len(),
        db.txs.len(),
        db.receipts.len(),
        db.balances.len(),
        persistence.enforce,
        persistence.variant.as_str(),
        persistence.rocksdb_persistence,
        persistence.persistence_root.display()
    );
    println!(
        "{}",
        serde_json::to_string_pretty(&response).context("serialize chain query response failed")?
    );
    Ok(())
}

fn run_chain_query_rpc_server_mode() -> Result<()> {
    let persistence = ensure_d2d3_persistence_through_d1()?;
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
        "rpc_server_mode: public_enabled={} public_bind={} public_max_requests={} gov_enabled={} gov_bind={} gov_max_requests={} gov_allowlist_count={} db={} d1_enforce={} d1_variant={} d1_rocksdb={} d1_root={}",
        enable_public,
        public_bind,
        public_max_requests,
        enable_gov,
        gov_bind,
        gov_max_requests,
        gov_allowlist.len(),
        db_path.display(),
        persistence.enforce,
        persistence.variant.as_str(),
        persistence.rocksdb_persistence,
        persistence.persistence_root.display()
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
    let foreign_rate_loaded = load_market_foreign_rate_source(&engine)
        .context("governance market policy probe: load foreign rate source failed")?;
    let foreign_feed_url_out = if foreign_rate_loaded.configured_url.is_empty() {
        "-"
    } else {
        foreign_rate_loaded.configured_url.as_str()
    };
    println!(
        "governance_market_foreign_source_in: mode={} source={} url={} strict={} timeout_ms={} configured_sources={} fetched={} fetched_sources={} min_sources={} signature_required={} signature_verified={} quote_spec_applied={} fallback_to_deterministic={} reason_code={}",
        foreign_rate_loaded.mode,
        foreign_rate_loaded.source_name,
        foreign_feed_url_out,
        foreign_rate_loaded.strict,
        foreign_rate_loaded.timeout_ms,
        foreign_rate_loaded.configured_sources,
        foreign_rate_loaded.fetched,
        foreign_rate_loaded.fetched_sources,
        foreign_rate_loaded.min_sources,
        foreign_rate_loaded.signature_required,
        foreign_rate_loaded.signature_verified,
        foreign_rate_loaded.quote_spec_applied,
        foreign_rate_loaded.fallback_to_deterministic,
        foreign_rate_loaded.reason_code
    );
    let nav_valuation_loaded = load_market_nav_valuation_source(&engine)
        .context("governance market policy probe: load nav valuation source failed")?;
    let nav_feed_url_out = if nav_valuation_loaded.configured_url.is_empty() {
        "-"
    } else {
        nav_valuation_loaded.configured_url.as_str()
    };
    println!(
        "governance_market_nav_source_in: mode={} source={} url={} strict={} timeout_ms={} configured_sources={} fetched={} fetched_sources={} min_sources={} signature_required={} signature_verified={} price_bp={} fallback_to_deterministic={} reason_code={}",
        nav_valuation_loaded.mode,
        nav_valuation_loaded.source_name,
        nav_feed_url_out,
        nav_valuation_loaded.strict,
        nav_valuation_loaded.timeout_ms,
        nav_valuation_loaded.configured_sources,
        nav_valuation_loaded.fetched,
        nav_valuation_loaded.fetched_sources,
        nav_valuation_loaded.min_sources,
        nav_valuation_loaded.signature_required,
        nav_valuation_loaded.signature_verified,
        nav_valuation_loaded.price_bp,
        nav_valuation_loaded.fallback_to_deterministic,
        nav_valuation_loaded.reason_code
    );

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
        && engine_snapshot.nav_valuation_source == nav_valuation_loaded.source_name
        && engine_snapshot.nav_valuation_price_bp == nav_valuation_loaded.price_bp
        && engine_snapshot.nav_valuation_fallback_used
            == nav_valuation_loaded.fallback_to_deterministic
        && engine_snapshot.nav_redemptions_submitted > 0
        && engine_snapshot.nav_redemptions_executed > 0
        && engine_snapshot.nav_executed_stable_total > 0
        && engine_snapshot.dividend_income_received > 0
        && engine_snapshot.dividend_snapshot_created > 0
        && engine_snapshot.dividend_claims_executed > 0
        && engine_snapshot.dividend_pool_balance > 0
        && engine_snapshot.foreign_payments_processed > 0
        && engine_snapshot.foreign_rate_source == foreign_rate_loaded.source_name
        && engine_snapshot.foreign_rate_quote_spec_applied
            == foreign_rate_loaded.quote_spec_applied
        && engine_snapshot.foreign_rate_fallback_used
            == foreign_rate_loaded.fallback_to_deterministic
        && engine_snapshot.foreign_token_paid_total > 0
        && engine_snapshot.foreign_reserve_btc > 0
        && engine_snapshot.foreign_reserve_eth > 0
        && engine_snapshot.foreign_payment_reserve_usdt > 0
        && engine_snapshot.foreign_swap_out_total > 0;
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
    let nav_source_applied = engine_snapshot.nav_valuation_source
        == nav_valuation_loaded.source_name
        && engine_snapshot.nav_valuation_price_bp == nav_valuation_loaded.price_bp
        && engine_snapshot.nav_valuation_fallback_used
            == nav_valuation_loaded.fallback_to_deterministic;
    println!(
        "governance_market_nav_source_out: proposal_id={} nav_source_applied={} source={} price_bp={} fallback_used={} fetched={} fetched_sources={} configured_sources={} min_sources={} signature_required={} signature_verified={} reason_code={} strict={} mode={}",
        proposal.proposal_id,
        nav_source_applied,
        engine_snapshot.nav_valuation_source,
        engine_snapshot.nav_valuation_price_bp,
        engine_snapshot.nav_valuation_fallback_used,
        nav_valuation_loaded.fetched,
        nav_valuation_loaded.fetched_sources,
        nav_valuation_loaded.configured_sources,
        nav_valuation_loaded.min_sources,
        nav_valuation_loaded.signature_required,
        nav_valuation_loaded.signature_verified,
        nav_valuation_loaded.reason_code,
        nav_valuation_loaded.strict,
        nav_valuation_loaded.mode
    );
    println!(
        "governance_market_dividend_out: proposal_id={} dividend_income_received={} dividend_snapshot_created={} dividend_claims_executed={} dividend_pool_balance={}",
        proposal.proposal_id,
        engine_snapshot.dividend_income_received,
        engine_snapshot.dividend_snapshot_created,
        engine_snapshot.dividend_claims_executed,
        engine_snapshot.dividend_pool_balance
    );
    println!(
        "governance_market_foreign_out: proposal_id={} foreign_payments_processed={} foreign_token_paid_total={} foreign_reserve_btc={} foreign_reserve_eth={} foreign_payment_reserve_usdt={} foreign_swap_out_total={}",
        proposal.proposal_id,
        engine_snapshot.foreign_payments_processed,
        engine_snapshot.foreign_token_paid_total,
        engine_snapshot.foreign_reserve_btc,
        engine_snapshot.foreign_reserve_eth,
        engine_snapshot.foreign_payment_reserve_usdt,
        engine_snapshot.foreign_swap_out_total
    );
    let foreign_source_applied = engine_snapshot.foreign_rate_source
        == foreign_rate_loaded.source_name
        && engine_snapshot.foreign_rate_quote_spec_applied
            == foreign_rate_loaded.quote_spec_applied
        && engine_snapshot.foreign_rate_fallback_used
            == foreign_rate_loaded.fallback_to_deterministic;
    println!(
        "governance_market_foreign_source_out: proposal_id={} foreign_source_applied={} source={} quote_spec_applied={} fallback_used={} fetched={} fetched_sources={} configured_sources={} min_sources={} signature_required={} signature_verified={} reason_code={} strict={} mode={}",
        proposal.proposal_id,
        foreign_source_applied,
        engine_snapshot.foreign_rate_source,
        engine_snapshot.foreign_rate_quote_spec_applied,
        engine_snapshot.foreign_rate_fallback_used,
        foreign_rate_loaded.fetched,
        foreign_rate_loaded.fetched_sources,
        foreign_rate_loaded.configured_sources,
        foreign_rate_loaded.min_sources,
        foreign_rate_loaded.signature_required,
        foreign_rate_loaded.signature_verified,
        foreign_rate_loaded.reason_code,
        foreign_rate_loaded.strict,
        foreign_rate_loaded.mode
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
    let persistence = ensure_d2d3_persistence_through_d1()?;
    println!(
        "d2d3_persistence_bind: enforce={} variant={} rocksdb_persistence={} root={}",
        persistence.enforce,
        persistence.variant.as_str(),
        persistence.rocksdb_persistence,
        persistence.persistence_root.display()
    );

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
    let (admitted_txs, mempool, tx_meta) =
        admit_mempool_basic_owned_with_meta(decoded_txs, fee_floor)?;
    let admitted_txs_count = admitted_txs.len();
    println!(
        "mempool_out: policy=basic accepted={} rejected={} fee_floor={} nonce_ok={} sig_ok={}",
        mempool.accepted, mempool.rejected, mempool.fee_floor, mempool.nonce_ok, mempool.sig_ok
    );

    println!(
        "tx_meta: accounts={} txs={} min_fee={} max_fee={} nonce_ok={} sig_ok={}",
        tx_meta.accounts,
        admitted_txs_count,
        tx_meta.min_fee,
        tx_meta.max_fee,
        tx_meta.nonce_ok,
        tx_meta.sig_ok
    );
    let ua_exec_guard_enabled = bool_env_default("NOVOVM_UNIFIED_ACCOUNT_EXEC_GUARD", true);
    if ua_exec_guard_enabled {
        let query_db_path = chain_query_db_path();
        let ua_store = resolve_unified_account_store(&query_db_path)?;
        let mut ua_snapshot = ua_store.load_snapshot()?;
        let ua_audit_sink = resolve_unified_account_audit_sink(&query_db_path)?;
        let chain_id = default_chain_id(resolve_adapter_chain()?);
        let signature_domain = string_env_nonempty("NOVOVM_UNIFIED_ACCOUNT_EXEC_SIGNATURE_DOMAIN")
            .unwrap_or_else(|| format!("evm:{}", chain_id));
        let auto_provision = bool_env_default("NOVOVM_UNIFIED_ACCOUNT_EXEC_AUTOPROVISION", true);
        let route_now = now_unix_sec();
        let before_flushed_event_count = ua_snapshot.flushed_event_count;
        let ua_summary_result = route_local_txs_through_unified_account(
            &admitted_txs,
            &mut ua_snapshot.router,
            chain_id,
            &signature_domain,
            route_now,
            auto_provision,
        );
        let (router_events, next_cursor) =
            unified_account_events_since(&ua_snapshot.router, ua_snapshot.flushed_event_count);
        let audit_record = UnifiedAccountAuditSinkRecord {
            at: route_now,
            source: "ffi_v2_exec_guard".to_string(),
            method: "ua_exec_guard".to_string(),
            success: ua_summary_result.is_ok(),
            router_changed: ua_summary_result.is_ok() || !router_events.is_empty(),
            event_cursor_from: ua_snapshot.flushed_event_count,
            event_cursor_to: next_cursor,
            router_events,
            params: serde_json::json!({
                "txs": admitted_txs.len(),
                "chain_id": chain_id,
                "signature_domain": signature_domain,
                "auto_provision": auto_provision,
            }),
            error: ua_summary_result.as_ref().err().map(|err| err.to_string()),
        };
        ua_audit_sink.append_record(&audit_record)?;
        ua_snapshot.flushed_event_count = next_cursor;
        if ua_summary_result.is_ok()
            || ua_snapshot.flushed_event_count != before_flushed_event_count
        {
            ua_store.save_snapshot(&ua_snapshot)?;
        }
        let ua_summary = ua_summary_result?;
        println!(
            "ua_exec_guard_out: enabled=true checked={} routed={} created_ucas={} added_bindings={} fast_path={} adapter={} chain_id={} signature_domain={} auto_provision={} ua_store={} ua_db={} ua_audit_backend={} ua_audit={}",
            ua_summary.checked,
            ua_summary.routed,
            ua_summary.created_ucas,
            ua_summary.added_bindings,
            ua_summary.decision_fast_path,
            ua_summary.decision_adapter,
            chain_id,
            signature_domain,
            auto_provision,
            ua_store.backend_name(),
            ua_store.path().display(),
            ua_audit_sink.backend_name(),
            ua_audit_sink.path().display()
        );
    } else {
        println!(
            "ua_exec_guard_out: enabled=false checked=0 routed=0 created_ucas=0 added_bindings=0 fast_path=0 adapter=0"
        );
    }
    let adapter_signal =
        run_adapter_bridge_signal_with_options(&admitted_txs, ua_exec_guard_enabled)?;
    println!(
        "adapter_out: backend={} chain={} chain_id={} txs={} verified={} applied={} accounts={} state_root={}",
        adapter_signal.backend,
        adapter_signal.chain,
        adapter_signal.chain_id,
        adapter_signal.txs,
        adapter_signal.verified,
        adapter_signal.applied,
        adapter_signal.accounts,
        to_hex(&adapter_signal.state_root)
    );
    println!(
        "adapter_canonical_out: receipts={} state_mirror_updates={}",
        adapter_signal
            .canonical_artifacts
            .as_ref()
            .map(|items| items.execution_receipts.len())
            .unwrap_or(0),
        adapter_signal
            .canonical_artifacts
            .as_ref()
            .map(|items| items.state_mirror_updates.len())
            .unwrap_or(0)
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
    let requested_batches = u32_env("NOVOVM_BATCH_A_BATCHES", 1) as usize;
    let (local_batches, batch, total_mapped_ops) =
        build_local_batches_and_ops_from_txs_owned(admitted_txs, requested_batches);
    if total_mapped_ops != batch.ops.len() {
        bail!(
            "batch mapping mismatch: mapped_ops={} encoded_ops={}",
            total_mapped_ops,
            batch.ops.len()
        );
    }
    println!(
        "tx_ingress: codec=novovm_local_tx_v1 accepted={} mapped_ops={}",
        admitted_txs_count,
        batch.ops.len()
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
        local_batches,
        consensus_binding,
        adapter_signal.state_root,
        &slash_policy_loaded.policy,
    ) {
        Ok(block) => {
            if let Err(e) = commit_block_in_memory(
                block,
                consensus_binding,
                adapter_signal.chain_id,
                adapter_signal.canonical_artifacts.as_ref(),
            ) {
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
    match run_network_smoke(admitted_txs_count as u64) {
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
    if node_mode.eq_ignore_ascii_case("ua_audit_migrate") {
        return run_unified_account_audit_migrate_mode();
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
        assert_eq!(default_chain_id(ChainType::Polygon), 137);
        assert_eq!(default_chain_id(ChainType::Avalanche), 43114);
        assert_eq!(default_chain_id(ChainType::Custom), 9_999_999);
    }

    #[test]
    fn adapter_plugin_chain_code_mapping_is_stable() {
        assert_eq!(chain_type_to_plugin_code(ChainType::NovoVM), Some(0));
        assert_eq!(chain_type_to_plugin_code(ChainType::EVM), Some(1));
        assert_eq!(chain_type_to_plugin_code(ChainType::Polygon), Some(5));
        assert_eq!(chain_type_to_plugin_code(ChainType::BNB), Some(6));
        assert_eq!(chain_type_to_plugin_code(ChainType::Avalanche), Some(7));
        assert_eq!(chain_type_to_plugin_code(ChainType::Custom), Some(13));
        assert_eq!(chain_type_to_plugin_code(ChainType::Solana), None);
    }

    #[test]
    fn adapter_plugin_auto_pick_prefers_chain_specialized_entry() {
        let mut dir = std::env::temp_dir();
        let nonce = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .expect("clock >= epoch")
            .as_nanos();
        dir.push(format!("novovm-adapter-plugin-auto-pick-{}", nonce));
        fs::create_dir_all(&dir).expect("create temp plugin dir");

        let sample_path = dir.join("sample-plugin.bin");
        let evm_path = dir.join("evm-plugin.bin");
        fs::write(&sample_path, b"sample").expect("write sample plugin file");
        std::thread::sleep(std::time::Duration::from_millis(5));
        fs::write(&evm_path, b"evm").expect("write evm plugin file");

        let registry = AdapterPluginRegistryFile {
            version: Some("novovm_adapter_registry_v1".to_string()),
            allowed_abi_versions: Some(vec![1]),
            plugins: vec![
                AdapterPluginRegistryEntry {
                    name: "sample".to_string(),
                    path: "sample-plugin.bin".to_string(),
                    abi: 1,
                    required_caps: "0x1".to_string(),
                    chains: vec![
                        "novovm".to_string(),
                        "evm".to_string(),
                        "polygon".to_string(),
                        "bnb".to_string(),
                        "avalanche".to_string(),
                        "custom".to_string(),
                    ],
                    enabled: Some(true),
                },
                AdapterPluginRegistryEntry {
                    name: "evm_specialized".to_string(),
                    path: "evm-plugin.bin".to_string(),
                    abi: 1,
                    required_caps: "0x1".to_string(),
                    chains: vec![
                        "evm".to_string(),
                        "polygon".to_string(),
                        "bnb".to_string(),
                        "avalanche".to_string(),
                    ],
                    enabled: Some(true),
                },
            ],
        };

        let picked = pick_adapter_plugin_from_registry(
            &registry,
            ChainType::EVM,
            1,
            ADAPTER_PLUGIN_CAP_APPLY_IR_V1,
            &[dir.clone()],
        )
        .expect("pick plugin from registry should succeed")
        .expect("plugin should be selected");
        assert_eq!(picked, evm_path);

        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn adapter_plugin_auto_pick_prefers_newer_binary_when_tie() {
        let mut dir = std::env::temp_dir();
        let nonce = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .expect("clock >= epoch")
            .as_nanos();
        dir.push(format!("novovm-adapter-plugin-auto-pick-tie-{}", nonce));
        fs::create_dir_all(&dir).expect("create temp plugin dir");

        let old_path = dir.join("plugin-old.bin");
        let new_path = dir.join("plugin-new.bin");
        fs::write(&old_path, b"old").expect("write old plugin file");
        std::thread::sleep(std::time::Duration::from_millis(5));
        fs::write(&new_path, b"new").expect("write new plugin file");

        let registry = AdapterPluginRegistryFile {
            version: Some("novovm_adapter_registry_v1".to_string()),
            allowed_abi_versions: Some(vec![1]),
            plugins: vec![
                AdapterPluginRegistryEntry {
                    name: "plugin_old".to_string(),
                    path: "plugin-old.bin".to_string(),
                    abi: 1,
                    required_caps: "0x1".to_string(),
                    chains: vec!["evm".to_string()],
                    enabled: Some(true),
                },
                AdapterPluginRegistryEntry {
                    name: "plugin_new".to_string(),
                    path: "plugin-new.bin".to_string(),
                    abi: 1,
                    required_caps: "0x1".to_string(),
                    chains: vec!["evm".to_string()],
                    enabled: Some(true),
                },
            ],
        };

        let picked = pick_adapter_plugin_from_registry(
            &registry,
            ChainType::EVM,
            1,
            ADAPTER_PLUGIN_CAP_APPLY_IR_V1,
            &[dir.clone()],
        )
        .expect("pick plugin from registry should succeed")
        .expect("plugin should be selected");
        assert_eq!(picked, new_path);

        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn evm_overlap_classifies_transfer_batch_as_p0() {
        let tx = test_tx(1000, 1, 10, 0, 2);
        let ir = to_adapter_tx_ir(&tx, 1);
        let class = classify_evm_overlap_batch(ChainType::EVM, &[ir]).expect("evm class");
        assert_eq!(class, EvmOverlapClass::P0);
    }

    #[test]
    fn evm_overlap_classifies_polygon_transfer_batch_as_p0() {
        let tx = test_tx(1000, 1, 10, 0, 2);
        let ir = to_adapter_tx_ir(&tx, 137);
        let class = classify_evm_overlap_batch(ChainType::Polygon, &[ir]).expect("polygon class");
        assert_eq!(class, EvmOverlapClass::P0);
    }

    #[test]
    fn evm_overlap_policy_prefers_plugin_for_p1_before_compare_green() {
        let tx = test_tx(1000, 1, 10, 0, 2);
        let mut ir = to_adapter_tx_ir(&tx, 1);
        ir.gas_limit = 30_000;
        let policy =
            resolve_evm_overlap_auto_policy_with_flags(ChainType::EVM, &[ir], true, false, true)
                .expect("policy");
        assert_eq!(policy.class, EvmOverlapClass::P1);
        assert_eq!(policy.order, EvmOverlapAutoOrder::PluginFirst);
        assert_eq!(policy.policy, "p1_compare_pending_plugin_first");
    }

    #[test]
    fn evm_overlap_policy_prefers_native_for_p1_after_compare_green() {
        let tx = test_tx(1000, 1, 10, 0, 2);
        let mut ir = to_adapter_tx_ir(&tx, 1);
        ir.gas_limit = 30_000;
        let policy =
            resolve_evm_overlap_auto_policy_with_flags(ChainType::EVM, &[ir], true, true, true)
                .expect("policy");
        assert_eq!(policy.class, EvmOverlapClass::P1);
        assert_eq!(policy.order, EvmOverlapAutoOrder::NativeFirst);
        assert_eq!(policy.policy, "p1_compare_green_supervm_first");
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
    fn canonical_artifact_ingest_uses_adapter_hash_and_persists_logs_and_state_mirror() {
        let chain_id = 1;
        let txs = vec![test_tx(1000, 1, 10, 0, 2)];
        let closure = BatchAClosureOutput {
            epoch_id: 3,
            height: 5,
            txs: 1,
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
        let tx_ir = to_adapter_tx_ir(&txs[0], chain_id);
        let canonical_hash = to_hex(&tx_ir.hash);
        let legacy_hash = to_hex(&local_tx_hash(&txs[0]));
        let artifacts = CanonicalBatchArtifactsV1 {
            execution_receipts: vec![SupervmEvmExecutionReceiptV1 {
                chain_type: ChainType::EVM,
                chain_id,
                tx_hash: tx_ir.hash.clone(),
                tx_index: 0,
                tx_type: TxType::Transfer,
                receipt_type: Some(2),
                status_ok: true,
                gas_used: 21_000,
                cumulative_gas_used: 21_000,
                effective_gas_price: Some(2),
                log_bloom: vec![0x11; 256],
                revert_data: None,
                state_root: [4u8; 32],
                state_version: 9,
                contract_address: None,
                logs: vec![SupervmEvmExecutionLogV1 {
                    emitter: vec![0xaa; 20],
                    topics: vec![[0xbb; 32]],
                    data: vec![0xcc; 4],
                    tx_index: 0,
                    log_index: 0,
                    state_version: 9,
                }],
            }],
            state_mirror_updates: vec![SupervmEvmStateMirrorUpdateV1 {
                chain_type: ChainType::EVM,
                chain_id,
                state_version: 9,
                state_root: [4u8; 32],
                receipt_count: 1,
                accepted_receipt_count: 1,
                tx_hashes: vec![tx_ir.hash.clone()],
                imported_at_unix_ms: 1234,
            }],
        };

        let mut db = QueryStateDb::default();
        apply_block_to_query_db_v1(&mut db, &block, chain_id, Some(&artifacts));

        assert_ne!(canonical_hash, legacy_hash);
        assert!(db.txs.contains_key(&canonical_hash));
        assert!(db.receipts.contains_key(&canonical_hash));
        assert!(!db.receipts.contains_key(&legacy_hash));
        assert_eq!(db.logs.len(), 1);
        assert_eq!(db.state_mirror_updates.len(), 1);
        assert_eq!(db.receipts[&canonical_hash].logs.len(), 1);
        assert_eq!(db.receipts[&canonical_hash].chain_type, "evm");
        assert_eq!(db.state_mirror_updates[0].tx_hashes, vec![canonical_hash]);
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
        let tx_resp_eth = run_chain_query(
            &db,
            "eth_getTransactionByHash",
            &serde_json::json!([tx_hash]),
        )
        .expect("eth_getTransactionByHash should succeed");
        assert_eq!(tx_resp_eth["found"].as_bool(), Some(true));

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
        let receipt_resp_eth = run_chain_query(
            &db,
            "eth_getTransactionReceipt",
            &serde_json::json!([receipt_hash]),
        )
        .expect("eth_getTransactionReceipt should succeed");
        assert_eq!(receipt_resp_eth["found"].as_bool(), Some(true));
        let receipt_resp_nov = run_chain_query(
            &db,
            "nov_getTransactionReceipt",
            &serde_json::json!({"tx_hash": receipt_hash}),
        )
        .expect("nov_getTransactionReceipt should succeed");
        assert_eq!(receipt_resp_nov["found"].as_bool(), Some(true));

        let balance_resp =
            run_chain_query(&db, "getBalance", &serde_json::json!({"account": "1001"}))
                .expect("getBalance should succeed");
        assert_eq!(balance_resp["found"].as_bool(), Some(true));
        assert_eq!(balance_resp["balance"].as_u64(), Some(20));

        let block_number_resp = run_chain_query(&db, "eth_blockNumber", &serde_json::json!([]))
            .expect("eth_blockNumber should succeed");
        assert_eq!(block_number_resp["block_number"].as_str(), Some("0x5"));
        assert_eq!(block_number_resp["block_number_u64"].as_u64(), Some(5));

        let chain_id_resp = run_chain_query(&db, "eth_chainId", &serde_json::json!({}))
            .expect("eth_chainId should succeed");
        assert_eq!(chain_id_resp["chain_id"].as_u64(), Some(1));
        assert_eq!(chain_id_resp["chain_id_hex"].as_str(), Some("0x1"));

        let net_version_resp =
            run_chain_query(&db, "net_version", &serde_json::json!({"chain_id": "0x89"}))
                .expect("net_version should accept chain selector");
        assert_eq!(net_version_resp["chain_id"].as_u64(), Some(137));
        assert_eq!(net_version_resp["net_version"].as_str(), Some("137"));

        let client_version_resp =
            run_chain_query(&db, "web3_clientVersion", &serde_json::json!({}))
                .expect("web3_clientVersion should succeed");
        assert!(
            client_version_resp["client_version"]
                .as_str()
                .is_some_and(|value| value.contains("novovm-node/"))
        );

        let surface_map_resp =
            run_chain_query(&db, "novovm_getSurfaceMap", &serde_json::json!({}))
                .expect("novovm_getSurfaceMap should succeed");
        assert_eq!(
            surface_map_resp["host_chain"].as_str(),
            Some("supervm_mainnet")
        );
        assert!(surface_map_resp["domains"]
            .as_array()
            .is_some_and(|domains| domains
                .iter()
                .any(|domain| domain["domain"].as_str() == Some("novovm_mainnet"))));
        assert!(surface_map_resp["domains"]
            .as_array()
            .is_some_and(|domains| domains
                .iter()
                .any(|domain| domain["domain"].as_str() == Some("evm_plugin"))));

        let method_domain_eth_resp = run_chain_query(
            &db,
            "novovm_getMethodDomain",
            &serde_json::json!({"method": "eth_getBalance"}),
        )
        .expect("novovm_getMethodDomain should return evm_plugin for eth method");
        assert_eq!(method_domain_eth_resp["domain"].as_str(), Some("evm_plugin"));
        assert_eq!(
            method_domain_eth_resp["control_namespace_disabled"].as_bool(),
            Some(false)
        );

        let method_domain_mainnet_resp = run_chain_query(
            &db,
            "novovm_getMethodDomain",
            &serde_json::json!(["ua_bindPersona"]),
        )
        .expect("novovm_getMethodDomain should return novovm_mainnet for ua_*");
        assert_eq!(
            method_domain_mainnet_resp["domain"].as_str(),
            Some("novovm_mainnet")
        );

        let method_domain_nov_resp = run_chain_query(
            &db,
            "novovm_getMethodDomain",
            &serde_json::json!({"method": "nov_sendRawTransaction"}),
        )
        .expect("novovm_getMethodDomain should return novovm_mainnet for nov_*");
        assert_eq!(
            method_domain_nov_resp["domain"].as_str(),
            Some("novovm_mainnet")
        );

        let method_domain_control_resp = run_chain_query(
            &db,
            "novovm_getMethodDomain",
            &serde_json::json!({"method": "debug_traceCall"}),
        )
        .expect("novovm_getMethodDomain should detect control namespace");
        assert_eq!(
            method_domain_control_resp["domain"].as_str(),
            Some("evm_plugin")
        );
        assert_eq!(
            method_domain_control_resp["control_namespace_disabled"].as_bool(),
            Some(true)
        );

        let block_by_number_latest = run_chain_query(
            &db,
            "eth_getBlockByNumber",
            &serde_json::json!(["latest", false]),
        )
        .expect("eth_getBlockByNumber latest should succeed");
        assert_eq!(block_by_number_latest["found"].as_bool(), Some(true));
        assert_eq!(
            block_by_number_latest["block"]["height"].as_u64(),
            Some(5)
        );

        let block_by_number_hex = run_chain_query(
            &db,
            "eth_getBlockByNumber",
            &serde_json::json!(["0x5", true]),
        )
        .expect("eth_getBlockByNumber hex should succeed");
        assert_eq!(block_by_number_hex["found"].as_bool(), Some(true));
        assert_eq!(
            block_by_number_hex["requested_height_hex"].as_str(),
            Some("0x5")
        );

        let eth_balance_resp = run_chain_query(
            &db,
            "eth_getBalance",
            &serde_json::json!(["1001", "latest"]),
        )
        .expect("eth_getBalance should succeed");
        assert_eq!(eth_balance_resp["found"].as_bool(), Some(true));
        assert_eq!(eth_balance_resp["balance"].as_u64(), Some(20));
        assert_eq!(eth_balance_resp["balance_hex"].as_str(), Some("0x14"));

        let nov_balance_resp = run_chain_query(
            &db,
            "nov_getBalance",
            &serde_json::json!({"account": "1001"}),
        )
        .expect("nov_getBalance should succeed");
        assert_eq!(nov_balance_resp["found"].as_bool(), Some(true));
        assert_eq!(nov_balance_resp["balance"].as_u64(), Some(20));

        let nov_asset_balance_resp = run_chain_query(
            &db,
            "nov_getAssetBalance",
            &serde_json::json!({"account": "1001", "asset": "NOV"}),
        )
        .expect("nov_getAssetBalance should succeed");
        assert_eq!(nov_asset_balance_resp["found"].as_bool(), Some(true));
        assert_eq!(nov_asset_balance_resp["asset"].as_str(), Some("NOV"));
        assert_eq!(nov_asset_balance_resp["balance"].as_u64(), Some(20));

        let nov_module_info_resp = run_chain_query(
            &db,
            "nov_getModuleInfo",
            &serde_json::json!({"module": "treasury"}),
        )
        .expect("nov_getModuleInfo should succeed");
        assert_eq!(nov_module_info_resp["found"].as_bool(), Some(true));
        assert_eq!(
            nov_module_info_resp["module_info"]["entry_kind"].as_str(),
            Some("native_module")
        );

        let nov_treasury_summary_resp = run_chain_query(
            &db,
            "nov_getTreasurySettlementSummary",
            &serde_json::json!({}),
        )
        .expect("nov_getTreasurySettlementSummary should succeed");
        assert_eq!(
            nov_treasury_summary_resp["method"].as_str(),
            Some("nov_getTreasurySettlementSummary")
        );
        assert!(nov_treasury_summary_resp["summary"].is_object());

        let nov_treasury_policy_resp = run_chain_query(
            &db,
            "nov_getTreasurySettlementPolicy",
            &serde_json::json!({}),
        )
        .expect("nov_getTreasurySettlementPolicy should succeed");
        assert_eq!(
            nov_treasury_policy_resp["method"].as_str(),
            Some("nov_getTreasurySettlementPolicy")
        );
        assert!(nov_treasury_policy_resp["policy"].is_object());

        let nov_treasury_journal_resp = run_chain_query(
            &db,
            "nov_getTreasurySettlementJournal",
            &serde_json::json!({"limit": 5}),
        )
        .expect("nov_getTreasurySettlementJournal should succeed");
        assert_eq!(
            nov_treasury_journal_resp["method"].as_str(),
            Some("nov_getTreasurySettlementJournal")
        );
        assert!(nov_treasury_journal_resp["journal"].is_object());

        let eth_code_resp = run_chain_query(
            &db,
            "eth_getCode",
            &serde_json::json!(["1001", "latest"]),
        )
        .expect("eth_getCode should succeed");
        assert_eq!(eth_code_resp["code"].as_str(), Some("0x"));

        let eth_storage_resp = run_chain_query(
            &db,
            "eth_getStorageAt",
            &serde_json::json!(["1001", "0x2", "latest"]),
        )
        .expect("eth_getStorageAt should succeed");
        assert_eq!(eth_storage_resp["slot_hex"].as_str(), Some("0x2"));
        assert_eq!(eth_storage_resp["value"].as_str().map(|v| v.len()), Some(66));

        let eth_call_resp = run_chain_query(
            &db,
            "eth_call",
            &serde_json::json!([{ "to": "0x1234", "data": "0x010203" }, "latest"]),
        )
        .expect("eth_call should succeed");
        assert_eq!(eth_call_resp["result"].as_str(), Some("0x"));

        let nov_call_resp = run_chain_query(
            &db,
            "nov_call",
            &serde_json::json!([{ "to": "0x1234", "data": "0x010203" }, "latest"]),
        )
        .expect("nov_call should succeed");
        assert_eq!(nov_call_resp["result"].as_str(), Some("0x"));

        let nov_state_resp = run_chain_query(&db, "nov_getState", &serde_json::json!({}))
            .expect("nov_getState should succeed");
        assert_eq!(nov_state_resp["method"].as_str(), Some("nov_getState"));
        assert!(nov_state_resp.get("found").is_some());

        let eth_estimate_gas_resp = run_chain_query(
            &db,
            "eth_estimateGas",
            &serde_json::json!([{ "to": "0x1234", "data": "0x010203" }, "latest"]),
        )
        .expect("eth_estimateGas should succeed");
        assert_eq!(
            eth_estimate_gas_resp["estimated_gas"].as_u64(),
            Some(21_048)
        );
        assert_eq!(
            eth_estimate_gas_resp["estimated_gas_hex"].as_str(),
            Some("0x5238")
        );

        let nov_estimate_gas_resp = run_chain_query(
            &db,
            "nov_estimateGas",
            &serde_json::json!([{ "to": "0x1234", "data": "0x010203" }, "latest"]),
        )
        .expect("nov_estimateGas should succeed");
        assert_eq!(
            nov_estimate_gas_resp["estimated_gas"].as_u64(),
            Some(21_048)
        );

        let nov_estimate_resp = run_chain_query(
            &db,
            "nov_estimate",
            &serde_json::json!([{ "to": "0x1234", "data": "0x010203" }, "latest"]),
        )
        .expect("nov_estimate should succeed");
        assert_eq!(nov_estimate_resp["estimated_gas"].as_u64(), Some(21_048));

        let eth_gas_price_resp = run_chain_query(&db, "eth_gasPrice", &serde_json::json!({}))
            .expect("eth_gasPrice should succeed");
        assert_eq!(eth_gas_price_resp["gas_price"].as_u64(), Some(1_000_000_000));
        assert_eq!(
            eth_gas_price_resp["gas_price_hex"].as_str(),
            Some("0x3b9aca00")
        );

        let eth_priority_fee_resp = run_chain_query(
            &db,
            "eth_maxPriorityFeePerGas",
            &serde_json::json!({}),
        )
        .expect("eth_maxPriorityFeePerGas should succeed");
        assert_eq!(
            eth_priority_fee_resp["max_priority_fee_per_gas"].as_u64(),
            Some(100_000_000)
        );
        assert_eq!(
            eth_priority_fee_resp["max_priority_fee_per_gas_hex"].as_str(),
            Some("0x5f5e100")
        );

        let eth_fee_history_resp = run_chain_query(
            &db,
            "eth_feeHistory",
            &serde_json::json!([2, "latest", [10, 20]]),
        )
        .expect("eth_feeHistory should succeed");
        assert_eq!(eth_fee_history_resp["block_count"].as_u64(), Some(2));
        assert_eq!(
            eth_fee_history_resp["baseFeePerGas"]
                .as_array()
                .map(|items| items.len()),
            Some(3)
        );
        assert_eq!(
            eth_fee_history_resp["gasUsedRatio"]
                .as_array()
                .map(|items| items.len()),
            Some(2)
        );
        assert_eq!(
            eth_fee_history_resp["reward"]
                .as_array()
                .map(|items| items.len()),
            Some(2)
        );
    }

    #[test]
    fn unified_account_public_rpc_flow_routes_and_rejects_replay() {
        let db = QueryStateDb::default();
        let mut router = UnifiedAccountRouter::new();
        let addr_hex = format!("0x{}", "11".repeat(20));

        let (create_resp, create_changed) = run_public_rpc(
            &db,
            &mut router,
            None,
            "ua_createUca",
            &serde_json::json!({
                "uca_id": "uca-test",
                "primary_key_ref": format!("0x{}", "22".repeat(32)),
                "now": 10,
            }),
        )
        .expect("ua_createUca should succeed");
        assert!(create_changed);
        assert_eq!(create_resp["created"].as_bool(), Some(true));

        let (bind_resp, bind_changed) = run_public_rpc(
            &db,
            &mut router,
            None,
            "ua_bindPersona",
            &serde_json::json!({
                "uca_id": "uca-test",
                "role": "owner",
                "persona_type": "evm",
                "chain_id": 1,
                "external_address": addr_hex,
                "now": 11,
            }),
        )
        .expect("ua_bindPersona should succeed");
        assert!(bind_changed);
        assert_eq!(bind_resp["bound"].as_bool(), Some(true));

        let (route_resp, route_changed) = run_public_rpc(
            &db,
            &mut router,
            None,
            "eth_sendRawTransaction",
            &serde_json::json!({
                "uca_id": "uca-test",
                "role": "owner",
                "chain_id": 1,
                "from": format!("0x{}", "11".repeat(20)),
                "nonce": 0,
                "tx_type4": false,
                "now": 12,
            }),
        )
        .expect("eth_sendRawTransaction should route");
        assert!(route_changed);
        assert_eq!(route_resp["accepted"].as_bool(), Some(true));
        assert_eq!(route_resp["decision"]["kind"].as_str(), Some("adapter"));
        assert_eq!(route_resp["decision"]["chain_id"].as_u64(), Some(1));

        let replay_err = run_public_rpc(
            &db,
            &mut router,
            None,
            "eth_sendRawTransaction",
            &serde_json::json!({
                "uca_id": "uca-test",
                "role": "owner",
                "chain_id": 1,
                "from": format!("0x{}", "11".repeat(20)),
                "nonce": 0,
                "tx_type4": false,
                "now": 13,
            }),
        )
        .expect_err("replay nonce should be rejected")
        .to_string();
        assert!(replay_err.contains("nonce rejected"));
    }

    #[test]
    fn unified_account_public_rpc_eth_send_transaction_alias_routes() {
        let db = QueryStateDb::default();
        let mut router = UnifiedAccountRouter::new();
        let addr_hex = format!("0x{}", "21".repeat(20));
        let to_hex = format!("0x{}", "22".repeat(20));

        run_public_rpc(
            &db,
            &mut router,
            None,
            "ua_createUca",
            &serde_json::json!({
                "uca_id": "uca-eth-alias",
                "primary_key_ref": format!("0x{}", "31".repeat(32)),
                "now": 10,
            }),
        )
        .expect("ua_createUca should succeed");

        run_public_rpc(
            &db,
            &mut router,
            None,
            "ua_bindPersona",
            &serde_json::json!({
                "uca_id": "uca-eth-alias",
                "role": "owner",
                "persona_type": "evm",
                "chain_id": 1,
                "external_address": addr_hex,
                "now": 11,
            }),
        )
        .expect("ua_bindPersona should succeed");

        let (route_resp, route_changed) = run_public_rpc(
            &db,
            &mut router,
            None,
            "eth_sendTransaction",
            &serde_json::json!({
                "uca_id": "uca-eth-alias",
                "role": "owner",
                "chain_id": 1,
                "from": format!("0x{}", "21".repeat(20)),
                "nonce": 0,
                "to": to_hex,
                "value": "0x01",
                "gas": "0x5208",
                "tx_type4": false,
                "now": 12,
            }),
        )
        .expect("eth_sendTransaction should route via unified account");
        assert!(route_changed);
        assert_eq!(route_resp["accepted"].as_bool(), Some(true));
        assert_eq!(route_resp["decision"]["kind"].as_str(), Some("adapter"));
        assert_eq!(route_resp["decision"]["chain_id"].as_u64(), Some(1));
        assert_eq!(route_resp["gas_limit"].as_u64(), Some(21_000));
        assert_eq!(route_resp["tx_ir_type"].as_str(), Some("transfer"));
        assert_eq!(route_resp["tx_ir_data_len"].as_u64(), Some(0));
    }

    #[test]
    fn unified_account_public_rpc_eth_send_transaction_alias_contract_call_ir() {
        let db = QueryStateDb::default();
        let mut router = UnifiedAccountRouter::new();
        let addr_hex = format!("0x{}", "23".repeat(20));
        let to_hex = format!("0x{}", "24".repeat(20));

        run_public_rpc(
            &db,
            &mut router,
            None,
            "ua_createUca",
            &serde_json::json!({
                "uca_id": "uca-eth-alias-call",
                "primary_key_ref": format!("0x{}", "33".repeat(32)),
                "now": 10,
            }),
        )
        .expect("ua_createUca should succeed");

        run_public_rpc(
            &db,
            &mut router,
            None,
            "ua_bindPersona",
            &serde_json::json!({
                "uca_id": "uca-eth-alias-call",
                "role": "owner",
                "persona_type": "evm",
                "chain_id": 1,
                "external_address": addr_hex,
                "now": 11,
            }),
        )
        .expect("ua_bindPersona should succeed");

        let (route_resp, route_changed) = run_public_rpc(
            &db,
            &mut router,
            None,
            "eth_sendTransaction",
            &serde_json::json!({
                "uca_id": "uca-eth-alias-call",
                "role": "owner",
                "chain_id": 1,
                "from": format!("0x{}", "23".repeat(20)),
                "nonce": "0x0",
                "to": to_hex,
                "data": "0xdeadbeef",
                "gasPrice": "0x2",
                "gas": "0x7530",
                "now": 12,
            }),
        )
        .expect("eth_sendTransaction call tx should route via unified account");
        assert!(route_changed);
        assert_eq!(route_resp["accepted"].as_bool(), Some(true));
        assert_eq!(route_resp["decision"]["kind"].as_str(), Some("adapter"));
        assert_eq!(route_resp["decision"]["chain_id"].as_u64(), Some(1));
        assert_eq!(route_resp["gas_limit"].as_u64(), Some(30_000));
        assert_eq!(route_resp["tx_ir_type"].as_str(), Some("contract_call"));
        assert_eq!(route_resp["tx_ir_data_len"].as_u64(), Some(4));
    }

    #[test]
    fn unified_account_public_rpc_eth_get_transaction_count_alias_tracks_next_nonce() {
        let db = QueryStateDb::default();
        let mut router = UnifiedAccountRouter::new();
        let addr_hex = format!("0x{}", "25".repeat(20));
        let to_hex = format!("0x{}", "26".repeat(20));

        run_public_rpc(
            &db,
            &mut router,
            None,
            "ua_createUca",
            &serde_json::json!({
                "uca_id": "uca-eth-nonce",
                "primary_key_ref": format!("0x{}", "35".repeat(32)),
                "now": 10,
            }),
        )
        .expect("ua_createUca should succeed");

        run_public_rpc(
            &db,
            &mut router,
            None,
            "ua_bindPersona",
            &serde_json::json!({
                "uca_id": "uca-eth-nonce",
                "role": "owner",
                "persona_type": "evm",
                "chain_id": 1,
                "external_address": addr_hex,
                "now": 11,
            }),
        )
        .expect("ua_bindPersona should succeed");

        run_public_rpc(
            &db,
            &mut router,
            None,
            "eth_sendTransaction",
            &serde_json::json!({
                "uca_id": "uca-eth-nonce",
                "role": "owner",
                "chain_id": 1,
                "from": format!("0x{}", "25".repeat(20)),
                "to": to_hex,
                "nonce": 0,
                "gas": "0x5208",
                "now": 12,
            }),
        )
        .expect("eth_sendTransaction should route");

        let (resp, changed) = run_public_rpc(
            &db,
            &mut router,
            None,
            "eth_getTransactionCount",
            &serde_json::json!({
                "address": format!("0x{}", "25".repeat(20)),
                "chain_id": 1,
            }),
        )
        .expect("eth_getTransactionCount should resolve nonce");
        assert!(!changed);
        assert_eq!(resp["uca_id"].as_str(), Some("uca-eth-nonce"));
        assert_eq!(resp["nonce"].as_u64(), Some(1));
        assert_eq!(resp["nonce_hex"].as_str(), Some("0x1"));
    }

    #[test]
    fn unified_account_public_rpc_eth_get_transaction_count_rejects_uca_mismatch() {
        let db = QueryStateDb::default();
        let mut router = UnifiedAccountRouter::new();
        let addr_hex = format!("0x{}", "27".repeat(20));

        run_public_rpc(
            &db,
            &mut router,
            None,
            "ua_createUca",
            &serde_json::json!({
                "uca_id": "uca-eth-owner",
                "primary_key_ref": format!("0x{}", "37".repeat(32)),
                "now": 10,
            }),
        )
        .expect("ua_createUca should succeed");
        run_public_rpc(
            &db,
            &mut router,
            None,
            "ua_bindPersona",
            &serde_json::json!({
                "uca_id": "uca-eth-owner",
                "role": "owner",
                "persona_type": "evm",
                "chain_id": 1,
                "external_address": addr_hex,
                "now": 11,
            }),
        )
        .expect("ua_bindPersona should succeed");

        let err = run_public_rpc(
            &db,
            &mut router,
            None,
            "eth_getTransactionCount",
            &serde_json::json!({
                "uca_id": "uca-other",
                "address": format!("0x{}", "27".repeat(20)),
                "chain_id": 1,
            }),
        )
        .expect_err("uca mismatch should reject")
        .to_string();
        assert!(err.contains("uca_id mismatch for address binding"));
    }

    #[test]
    fn unified_account_public_rpc_eth_get_transaction_count_array_params_supported() {
        let db = QueryStateDb::default();
        let mut router = UnifiedAccountRouter::new();
        let addr_hex = format!("0x{}", "28".repeat(20));

        run_public_rpc(
            &db,
            &mut router,
            None,
            "ua_createUca",
            &serde_json::json!({
                "uca_id": "uca-eth-array",
                "primary_key_ref": format!("0x{}", "38".repeat(32)),
                "now": 10,
            }),
        )
        .expect("ua_createUca should succeed");
        run_public_rpc(
            &db,
            &mut router,
            None,
            "ua_bindPersona",
            &serde_json::json!({
                "uca_id": "uca-eth-array",
                "role": "owner",
                "persona_type": "evm",
                "chain_id": 1,
                "external_address": addr_hex,
                "now": 11,
            }),
        )
        .expect("ua_bindPersona should succeed");

        let (resp, changed) = run_public_rpc(
            &db,
            &mut router,
            None,
            "eth_getTransactionCount",
            &serde_json::json!([
                format!("0x{}", "28".repeat(20)),
                "latest",
            ]),
        )
        .expect("eth_getTransactionCount array params should pass");
        assert!(!changed);
        assert_eq!(resp["uca_id"].as_str(), Some("uca-eth-array"));
        assert_eq!(resp["nonce"].as_u64(), Some(0));
    }

    #[test]
    fn unified_account_public_rpc_web30_send_raw_transaction_alias_routes() {
        let db = QueryStateDb::default();
        let mut router = UnifiedAccountRouter::new();
        let addr_hex = format!("0x{}", "41".repeat(20));

        run_public_rpc(
            &db,
            &mut router,
            None,
            "ua_createUca",
            &serde_json::json!({
                "uca_id": "uca-web30-alias",
                "primary_key_ref": format!("0x{}", "51".repeat(32)),
                "now": 10,
            }),
        )
        .expect("ua_createUca should succeed");

        run_public_rpc(
            &db,
            &mut router,
            None,
            "ua_bindPersona",
            &serde_json::json!({
                "uca_id": "uca-web30-alias",
                "role": "owner",
                "persona_type": "web30",
                "chain_id": 20260303,
                "external_address": addr_hex,
                "now": 11,
            }),
        )
        .expect("ua_bindPersona should succeed");

        let (route_resp, route_changed) = run_public_rpc(
            &db,
            &mut router,
            None,
            "web30_sendRawTransaction",
            &serde_json::json!({
                "uca_id": "uca-web30-alias",
                "role": "owner",
                "chain_id": 20260303,
                "from": format!("0x{}", "41".repeat(20)),
                "nonce": 0,
                "tx_type4": false,
                "now": 12,
            }),
        )
        .expect("web30_sendRawTransaction should route via unified account");
        assert!(route_changed);
        assert_eq!(route_resp["accepted"].as_bool(), Some(true));
        assert_eq!(route_resp["decision"]["kind"].as_str(), Some("fast_path"));
    }

    #[test]
    fn unified_account_public_rpc_nov_send_raw_transaction_alias_routes() {
        let db = QueryStateDb::default();
        let mut router = UnifiedAccountRouter::new();
        let addr_hex = format!("0x{}", "61".repeat(20));

        run_public_rpc(
            &db,
            &mut router,
            None,
            "ua_createUca",
            &serde_json::json!({
                "uca_id": "uca-nov-alias",
                "primary_key_ref": format!("0x{}", "71".repeat(32)),
                "now": 10,
            }),
        )
        .expect("ua_createUca should succeed");

        run_public_rpc(
            &db,
            &mut router,
            None,
            "ua_bindPersona",
            &serde_json::json!({
                "uca_id": "uca-nov-alias",
                "role": "owner",
                "persona_type": "web30",
                "chain_id": 20260417,
                "external_address": addr_hex,
                "now": 11,
            }),
        )
        .expect("ua_bindPersona should succeed");

        let (route_resp, route_changed) = run_public_rpc(
            &db,
            &mut router,
            None,
            "nov_sendRawTransaction",
            &serde_json::json!({
                "uca_id": "uca-nov-alias",
                "role": "owner",
                "chain_id": 20260417,
                "from": format!("0x{}", "61".repeat(20)),
                "nonce": 0,
                "tx_type4": false,
                "now": 12,
            }),
        )
        .expect("nov_sendRawTransaction should route via unified account");
        assert!(route_changed);
        assert_eq!(route_resp["accepted"].as_bool(), Some(true));
        assert_eq!(route_resp["decision"]["kind"].as_str(), Some("fast_path"));
    }

    #[test]
    fn unified_account_exec_guard_rejects_replay_before_adapter_execution() {
        let mut router = UnifiedAccountRouter::new();
        let txs = vec![test_tx(1000, 1, 10, 0, 2)];

        let first =
            route_local_txs_through_unified_account(&txs, &mut router, 1, "evm:1", 10, true)
                .expect("first execution guard routing should pass");
        assert_eq!(first.checked, 1);
        assert_eq!(first.routed, 1);
        assert_eq!(first.created_ucas, 1);
        assert_eq!(first.added_bindings, 1);
        assert_eq!(first.decision_adapter, 1);

        let replay_err =
            route_local_txs_through_unified_account(&txs, &mut router, 1, "evm:1", 11, true)
                .expect_err("execution guard should reject replay nonce")
                .to_string();
        assert!(replay_err.contains("nonce rejected"));
    }

    #[test]
    fn unified_account_exec_guard_rejects_domain_mismatch_before_adapter_execution() {
        let mut router = UnifiedAccountRouter::new();
        let txs = vec![test_tx(1000, 1, 10, 0, 2)];

        let err = route_local_txs_through_unified_account(
            &txs,
            &mut router,
            1,
            "web30:mainnet",
            10,
            true,
        )
        .expect_err("execution guard should reject mismatched signature domain")
        .to_string();
        assert!(err.contains("domain mismatch"));
    }

    #[test]
    fn unified_account_store_snapshot_roundtrip_and_legacy_decode() {
        let mut path = std::env::temp_dir();
        let nonce = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .expect("clock should be >= epoch")
            .as_nanos();
        path.push(format!("novovm-ua-store-{}.bin", nonce));
        let store = UnifiedAccountStoreBackend::BincodeFile { path: path.clone() };

        let mut router = UnifiedAccountRouter::new();
        router
            .create_uca("uca-roundtrip", vec![7u8; 32], 1)
            .expect("create uca for roundtrip");
        let expected_events = router.events().len();
        let legacy_raw = crate::bincode_compat::serialize(&router).expect("serialize legacy router");
        let snapshot = UnifiedAccountStoreSnapshot {
            router,
            flushed_event_count: 1,
        };
        store
            .save_snapshot(&snapshot)
            .expect("save ua snapshot should succeed");
        let loaded = store
            .load_snapshot()
            .expect("load ua snapshot should succeed");
        assert_eq!(loaded.flushed_event_count, 1);
        assert_eq!(loaded.router.events().len(), expected_events);

        fs::write(&path, legacy_raw).expect("write legacy router payload");
        let loaded_legacy = store
            .load_snapshot()
            .expect("load legacy router payload should succeed");
        assert_eq!(loaded_legacy.flushed_event_count, expected_events as u64);
        assert_eq!(loaded_legacy.router.events().len(), expected_events);

        let _ = fs::remove_file(&path);
    }

    #[test]
    fn unified_account_store_snapshot_roundtrip_rocksdb() {
        let mut path = std::env::temp_dir();
        let nonce = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .expect("clock should be >= epoch")
            .as_nanos();
        path.push(format!("novovm-ua-store-{}.rocksdb", nonce));
        let store = UnifiedAccountStoreBackend::RocksDb { path: path.clone() };

        let mut router = UnifiedAccountRouter::new();
        router
            .create_uca("uca-rocksdb", vec![9u8; 32], 1)
            .expect("create uca for rocksdb roundtrip");
        let expected_events = router.events().len();
        let snapshot = UnifiedAccountStoreSnapshot {
            router,
            flushed_event_count: 2,
        };
        store
            .save_snapshot(&snapshot)
            .expect("save ua rocksdb snapshot should succeed");
        let loaded = store
            .load_snapshot()
            .expect("load ua rocksdb snapshot should succeed");
        assert_eq!(loaded.flushed_event_count, 2);
        assert_eq!(loaded.router.events().len(), expected_events);

        let db = open_unified_account_rocksdb(&path).expect("open rocksdb store for verify");
        let state_cf = db
            .cf_handle(UNIFIED_ACCOUNT_STORE_ROCKSDB_CF_STATE)
            .expect("state cf should exist");
        let audit_cf = db
            .cf_handle(UNIFIED_ACCOUNT_STORE_ROCKSDB_CF_AUDIT)
            .expect("audit cf should exist");
        assert!(
            db.get_cf(state_cf, UNIFIED_ACCOUNT_STORE_ROCKSDB_KEY_STATE_ROUTER)
                .expect("read ua rocksdb state key from state cf")
                .is_some()
        );
        assert!(
            db.get_cf(audit_cf, UNIFIED_ACCOUNT_STORE_ROCKSDB_KEY_AUDIT_CURSOR)
                .expect("read ua rocksdb audit cursor key from audit cf")
                .is_some()
        );
        assert!(
            db.get(UNIFIED_ACCOUNT_STORE_ROCKSDB_KEY_STATE_ROUTER)
                .expect("read ua rocksdb state key from default cf")
                .is_some()
        );
        assert!(
            db.get(UNIFIED_ACCOUNT_STORE_ROCKSDB_KEY_AUDIT_CURSOR)
                .expect("read ua rocksdb audit cursor key from default cf")
                .is_some()
        );
        db.delete(UNIFIED_ACCOUNT_STORE_ROCKSDB_KEY_STATE_ROUTER)
            .expect("delete default-cf ua state key");
        db.delete(UNIFIED_ACCOUNT_STORE_ROCKSDB_KEY_AUDIT_CURSOR)
            .expect("delete default-cf ua audit cursor key");
        db.delete(UNIFIED_ACCOUNT_STORE_ROCKSDB_KEY_SNAPSHOT)
            .expect("delete legacy ua snapshot key");
        drop(db);

        let loaded_v2_only = store
            .load_snapshot()
            .expect("load ua rocksdb snapshot from split namespace should succeed");
        assert_eq!(loaded_v2_only.flushed_event_count, 2);
        assert_eq!(loaded_v2_only.router.events().len(), expected_events);

        let _ = fs::remove_dir_all(&path);
    }

    #[test]
    fn unified_account_store_snapshot_namespace_only_rocksdb_decode() {
        let mut path = std::env::temp_dir();
        let nonce = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .expect("clock should be >= epoch")
            .as_nanos();
        path.push(format!("novovm-ua-store-namespace-only-{}.rocksdb", nonce));
        let store = UnifiedAccountStoreBackend::RocksDb { path: path.clone() };

        let mut router = UnifiedAccountRouter::new();
        router
            .create_uca("uca-namespace-only", vec![7u8; 32], 1)
            .expect("create uca for namespace-only rocksdb decode");
        let expected_events = router.events().len();
        let router_encoded =
            crate::bincode_compat::serialize(&router).expect("serialize router for namespace-only write");
        let db =
            open_unified_account_rocksdb(&path).expect("open rocksdb store for namespace write");
        db.put(UNIFIED_ACCOUNT_STORE_ROCKSDB_KEY_STATE_ROUTER, router_encoded)
            .expect("write default-cf state key");
        db.put(
            UNIFIED_ACCOUNT_STORE_ROCKSDB_KEY_AUDIT_CURSOR,
            4u64.to_be_bytes(),
        )
        .expect("write default-cf audit cursor key");
        drop(db);

        let loaded = store
            .load_snapshot()
            .expect("namespace-only ua rocksdb decode should succeed");
        assert_eq!(loaded.flushed_event_count, 4);
        assert_eq!(loaded.router.events().len(), expected_events);

        let _ = fs::remove_dir_all(&path);
    }

    #[test]
    fn unified_account_store_snapshot_legacy_only_rocksdb_decode() {
        let mut path = std::env::temp_dir();
        let nonce = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .expect("clock should be >= epoch")
            .as_nanos();
        path.push(format!("novovm-ua-store-legacy-only-{}.rocksdb", nonce));
        let store = UnifiedAccountStoreBackend::RocksDb { path: path.clone() };

        let mut router = UnifiedAccountRouter::new();
        router
            .create_uca("uca-legacy-only", vec![8u8; 32], 1)
            .expect("create uca for legacy-only rocksdb decode");
        let expected_events = router.events().len();
        let snapshot = UnifiedAccountStoreSnapshot {
            router,
            flushed_event_count: 3,
        };
        let legacy_encoded = encode_unified_account_snapshot(&snapshot)
            .expect("encode legacy ua snapshot should succeed");
        let db = open_unified_account_rocksdb(&path).expect("open rocksdb store for legacy write");
        db.put(UNIFIED_ACCOUNT_STORE_ROCKSDB_KEY_SNAPSHOT, legacy_encoded)
            .expect("write legacy ua snapshot key");
        drop(db);

        let loaded = store
            .load_snapshot()
            .expect("legacy-only ua rocksdb decode should succeed");
        assert_eq!(loaded.flushed_event_count, 3);
        assert_eq!(loaded.router.events().len(), expected_events);

        let _ = fs::remove_dir_all(&path);
    }

    #[test]
    fn unified_account_audit_sink_appends_jsonl_records() {
        let mut path = std::env::temp_dir();
        let nonce = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .expect("clock should be >= epoch")
            .as_nanos();
        path.push(format!("novovm-ua-audit-{}.jsonl", nonce));
        let record = UnifiedAccountAuditSinkRecord {
            at: 1,
            source: "test".to_string(),
            method: "ua_route".to_string(),
            success: true,
            router_changed: true,
            event_cursor_from: 0,
            event_cursor_to: 1,
            router_events: vec![],
            params: serde_json::json!({"k":"v"}),
            error: None,
        };
        append_unified_account_audit_record(&path, &record).expect("append audit record");
        append_unified_account_audit_record(&path, &record).expect("append audit record again");

        let raw = fs::read_to_string(&path).expect("read audit file");
        let lines: Vec<_> = raw.lines().collect();
        assert_eq!(lines.len(), 2);
        assert!(lines[0].contains("\"method\":\"ua_route\""));
        assert!(lines[0].contains("\"source\":\"test\""));

        let _ = fs::remove_file(&path);
    }

    #[test]
    fn unified_account_audit_sink_appends_rocksdb_records() {
        let mut path = std::env::temp_dir();
        let nonce = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .expect("clock should be >= epoch")
            .as_nanos();
        path.push(format!("novovm-ua-audit-{}.rocksdb", nonce));
        let sink = UnifiedAccountAuditSinkBackend::RocksDb { path: path.clone() };
        let record = UnifiedAccountAuditSinkRecord {
            at: 2,
            source: "test".to_string(),
            method: "ua_route".to_string(),
            success: true,
            router_changed: true,
            event_cursor_from: 0,
            event_cursor_to: 1,
            router_events: vec![],
            params: serde_json::json!({"k":"v"}),
            error: None,
        };
        sink.append_record(&record)
            .expect("append rocksdb audit record");
        sink.append_record(&record)
            .expect("append rocksdb audit record again");

        let db = open_unified_account_audit_rocksdb(&path).expect("open rocksdb audit for verify");
        let seq_raw = db
            .get(UNIFIED_ACCOUNT_AUDIT_ROCKSDB_KEY_SEQ)
            .expect("read rocksdb audit seq")
            .expect("rocksdb audit seq should exist");
        let seq = decode_u64_be(&seq_raw).expect("decode rocksdb audit seq");
        assert_eq!(seq, 2);

        let event1_raw = db
            .get(unified_account_audit_rocksdb_event_key(1))
            .expect("read rocksdb audit event1")
            .expect("rocksdb audit event1 should exist");
        let event2_raw = db
            .get(unified_account_audit_rocksdb_event_key(2))
            .expect("read rocksdb audit event2")
            .expect("rocksdb audit event2 should exist");
        let event1: serde_json::Value =
            serde_json::from_slice(&event1_raw).expect("decode event1 json");
        let event2: serde_json::Value =
            serde_json::from_slice(&event2_raw).expect("decode event2 json");
        assert_eq!(event1["method"].as_str(), Some("ua_route"));
        assert_eq!(event2["source"].as_str(), Some("test"));

        drop(db);
        let _ = fs::remove_dir_all(&path);
    }

    #[test]
    fn unified_account_public_rpc_get_audit_events_from_sink() {
        let db = QueryStateDb::default();
        let mut router = UnifiedAccountRouter::new();
        let mut path = std::env::temp_dir();
        let nonce = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .expect("clock should be >= epoch")
            .as_nanos();
        path.push(format!("novovm-ua-audit-rpc-{}.rocksdb", nonce));
        let sink = UnifiedAccountAuditSinkBackend::RocksDb { path: path.clone() };
        let record = UnifiedAccountAuditSinkRecord {
            at: 3,
            source: "test".to_string(),
            method: "ua_route".to_string(),
            success: true,
            router_changed: true,
            event_cursor_from: 0,
            event_cursor_to: 1,
            router_events: vec![],
            params: serde_json::json!({"k":"v"}),
            error: None,
        };
        sink.append_record(&record)
            .expect("append rocksdb audit record");
        sink.append_record(&record)
            .expect("append rocksdb audit record again");

        let (resp, changed) = run_public_rpc(
            &db,
            &mut router,
            Some(&sink),
            "ua_getAuditEvents",
            &serde_json::json!({
                "source": "sink",
                "since_seq": 0,
                "limit": 10,
            }),
        )
        .expect("ua_getAuditEvents should read sink");
        assert!(!changed);
        assert_eq!(resp["source"].as_str(), Some("sink"));
        assert_eq!(resp["backend"].as_str(), Some("rocksdb"));
        assert_eq!(resp["count"].as_u64(), Some(2));
        assert_eq!(resp["events"][0]["seq"].as_u64(), Some(1));
        assert_eq!(resp["events"][1]["seq"].as_u64(), Some(2));

        let _ = fs::remove_dir_all(&path);
    }

    #[test]
    fn unified_account_public_rpc_get_audit_events_from_sink_supports_filters() {
        let db = QueryStateDb::default();
        let mut router = UnifiedAccountRouter::new();
        let mut path = std::env::temp_dir();
        let nonce = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .expect("clock should be >= epoch")
            .as_nanos();
        path.push(format!("novovm-ua-audit-rpc-filter-{}.rocksdb", nonce));
        let sink = UnifiedAccountAuditSinkBackend::RocksDb { path: path.clone() };
        sink.append_record(&UnifiedAccountAuditSinkRecord {
            at: 10,
            source: "public_rpc".to_string(),
            method: "ua_route".to_string(),
            success: true,
            router_changed: true,
            event_cursor_from: 0,
            event_cursor_to: 1,
            router_events: vec![],
            params: serde_json::json!({"idx":1}),
            error: None,
        })
        .expect("append filter record #1");
        sink.append_record(&UnifiedAccountAuditSinkRecord {
            at: 11,
            source: "ffi_v2_exec_guard".to_string(),
            method: "ua_exec_guard".to_string(),
            success: false,
            router_changed: false,
            event_cursor_from: 1,
            event_cursor_to: 1,
            router_events: vec![],
            params: serde_json::json!({"idx":2}),
            error: Some("x".to_string()),
        })
        .expect("append filter record #2");
        sink.append_record(&UnifiedAccountAuditSinkRecord {
            at: 12,
            source: "public_rpc".to_string(),
            method: "ua_route".to_string(),
            success: false,
            router_changed: false,
            event_cursor_from: 1,
            event_cursor_to: 2,
            router_events: vec![],
            params: serde_json::json!({"idx":3}),
            error: Some("y".to_string()),
        })
        .expect("append filter record #3");

        let (resp, changed) = run_public_rpc(
            &db,
            &mut router,
            Some(&sink),
            "ua_getAuditEvents",
            &serde_json::json!({
                "source": "sink",
                "since_seq": 0,
                "limit": 10,
                "filter_method": "ua_route",
                "filter_source": "public_rpc",
                "filter_success": true
            }),
        )
        .expect("ua_getAuditEvents sink filter should succeed");
        assert!(!changed);
        assert_eq!(resp["count"].as_u64(), Some(1));
        assert_eq!(resp["events"][0]["seq"].as_u64(), Some(1));
        assert_eq!(resp["events"][0]["method"].as_str(), Some("ua_route"));
        assert_eq!(resp["events"][0]["source"].as_str(), Some("public_rpc"));
        assert_eq!(resp["events"][0]["success"].as_bool(), Some(true));

        let _ = fs::remove_dir_all(&path);
    }

    #[test]
    fn unified_account_audit_migration_jsonl_to_rocksdb_is_incremental() {
        let mut src_path = std::env::temp_dir();
        let mut dst_path = std::env::temp_dir();
        let nonce = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .expect("clock should be >= epoch")
            .as_nanos();
        src_path.push(format!("novovm-ua-audit-src-{}.jsonl", nonce));
        dst_path.push(format!("novovm-ua-audit-dst-{}.rocksdb", nonce));
        let source = UnifiedAccountAuditSinkBackend::JsonlFile {
            path: src_path.clone(),
        };
        let target = UnifiedAccountAuditSinkBackend::RocksDb {
            path: dst_path.clone(),
        };
        let record = UnifiedAccountAuditSinkRecord {
            at: 4,
            source: "test".to_string(),
            method: "ua_route".to_string(),
            success: true,
            router_changed: true,
            event_cursor_from: 0,
            event_cursor_to: 1,
            router_events: vec![],
            params: serde_json::json!({"k":"v"}),
            error: None,
        };
        source
            .append_record(&record)
            .expect("append source audit record #1");
        source
            .append_record(&record)
            .expect("append source audit record #2");
        source
            .append_record(&record)
            .expect("append source audit record #3");
        target
            .append_record(&record)
            .expect("append target pre-existing audit record #1");

        let (source_head, target_head_before, appended, target_head_after) =
            migrate_unified_account_audit_records(&source, &target)
                .expect("migrate jsonl to rocksdb");
        assert_eq!(source_head, 3);
        assert_eq!(target_head_before, 1);
        assert_eq!(appended, 2);
        assert_eq!(target_head_after, 3);

        let (_, _, appended_again, target_head_after_again) =
            migrate_unified_account_audit_records(&source, &target)
                .expect("second migrate should be idempotent by seq");
        assert_eq!(appended_again, 0);
        assert_eq!(target_head_after_again, 3);

        let _ = fs::remove_file(&src_path);
        let _ = fs::remove_dir_all(&dst_path);
    }

    fn ua_hex(byte: u8, bytes: usize) -> String {
        format!("0x{}", format!("{:02x}", byte).repeat(bytes))
    }

    fn ua_create(db: &QueryStateDb, router: &mut UnifiedAccountRouter, uca_id: &str, now: u64) {
        let (resp, changed) = run_public_rpc(
            db,
            router,
            None,
            "ua_createUca",
            &serde_json::json!({
                "uca_id": uca_id,
                "now": now,
                "primary_key_ref": ua_hex(0x66, 32),
            }),
        )
        .expect("ua_createUca should succeed");
        assert!(changed);
        assert_eq!(resp["created"].as_bool(), Some(true));
        assert_eq!(resp["uca_id"].as_str(), Some(uca_id));
    }

    fn ua_bind(
        db: &QueryStateDb,
        router: &mut UnifiedAccountRouter,
        uca_id: &str,
        role: &str,
        persona_type: &str,
        chain_id: u64,
        external_address: &str,
        now: u64,
    ) {
        let (resp, changed) = run_public_rpc(
            db,
            router,
            None,
            "ua_bindPersona",
            &serde_json::json!({
                "uca_id": uca_id,
                "role": role,
                "persona_type": persona_type,
                "chain_id": chain_id,
                "external_address": external_address,
                "now": now,
            }),
        )
        .expect("ua_bindPersona should succeed");
        assert!(changed);
        assert_eq!(resp["bound"].as_bool(), Some(true));
    }

    #[test]
    fn unified_account_gate_ua_g01_mapping_bind_success() {
        let db = QueryStateDb::default();
        let mut router = UnifiedAccountRouter::new();
        let evm_addr = ua_hex(0x11, 20);
        ua_create(&db, &mut router, "uca-a", 10);
        ua_bind(&db, &mut router, "uca-a", "owner", "evm", 1, &evm_addr, 11);
        assert!(router.events().iter().any(|event| {
            matches!(
                event,
                AccountAuditEvent::BindingAdded { uca_id, .. } if uca_id == "uca-a"
            )
        }));
    }

    #[test]
    fn unified_account_gate_ua_g02_mapping_conflict_rejected() {
        let db = QueryStateDb::default();
        let mut router = UnifiedAccountRouter::new();
        let evm_addr = ua_hex(0x12, 20);
        ua_create(&db, &mut router, "uca-a", 10);
        ua_create(&db, &mut router, "uca-b", 10);
        ua_bind(&db, &mut router, "uca-a", "owner", "evm", 1, &evm_addr, 11);

        let err = run_public_rpc(
            &db,
            &mut router,
            None,
            "ua_bindPersona",
            &serde_json::json!({
                "uca_id": "uca-b",
                "role": "owner",
                "persona_type": "evm",
                "chain_id": 1,
                "external_address": evm_addr,
                "now": 12,
            }),
        )
        .expect_err("binding conflict should be rejected")
        .to_string();
        assert!(err.contains("binding conflict"));
        assert!(router.events().iter().any(|event| {
            matches!(
                event,
                AccountAuditEvent::BindingConflictRejected {
                    request_uca_id,
                    existing_uca_id,
                    ..
                } if request_uca_id == "uca-b" && existing_uca_id == "uca-a"
            )
        }));
    }

    #[test]
    fn unified_account_gate_ua_g03_mapping_cooldown_rejects_rebind() {
        let db = QueryStateDb::default();
        let mut router = UnifiedAccountRouter::new();
        let evm_addr = ua_hex(0x13, 20);
        ua_create(&db, &mut router, "uca-a", 10);
        ua_bind(&db, &mut router, "uca-a", "owner", "evm", 1, &evm_addr, 11);
        run_public_rpc(
            &db,
            &mut router,
            None,
            "ua_revokePersona",
            &serde_json::json!({
                "uca_id": "uca-a",
                "role": "owner",
                "persona_type": "evm",
                "chain_id": 1,
                "external_address": evm_addr,
                "cooldown_seconds": 60,
                "now": 20,
            }),
        )
        .expect("ua_revokePersona should succeed");

        let err = run_public_rpc(
            &db,
            &mut router,
            None,
            "ua_bindPersona",
            &serde_json::json!({
                "uca_id": "uca-a",
                "role": "owner",
                "persona_type": "evm",
                "chain_id": 1,
                "external_address": evm_addr,
                "now": 21,
            }),
        )
        .expect_err("cooldown window should reject")
        .to_string();
        assert!(err.contains("cooldown active"));
    }

    #[test]
    fn unified_account_gate_ua_g04_signature_domain_mismatch_rejected() {
        let db = QueryStateDb::default();
        let mut router = UnifiedAccountRouter::new();
        let evm_addr = ua_hex(0x14, 20);
        ua_create(&db, &mut router, "uca-a", 10);
        ua_bind(&db, &mut router, "uca-a", "owner", "evm", 1, &evm_addr, 11);

        let err = run_public_rpc(
            &db,
            &mut router,
            None,
            "eth_sendRawTransaction",
            &serde_json::json!({
                "uca_id": "uca-a",
                "role": "owner",
                "chain_id": 1,
                "from": evm_addr,
                "signature_domain": "web30:mainnet",
                "nonce": 0,
                "tx_type4": false,
                "now": 12,
            }),
        )
        .expect_err("domain mismatch should reject")
        .to_string();
        assert!(err.contains("domain mismatch"));
        assert!(router.events().iter().any(|event| {
            matches!(event, AccountAuditEvent::DomainMismatchRejected { uca_id, .. } if uca_id == "uca-a")
        }));
    }

    #[test]
    fn unified_account_gate_ua_g05_signature_domain_eip712_wrong_chain_rejected() {
        let db = QueryStateDb::default();
        let mut router = UnifiedAccountRouter::new();
        let evm_addr = ua_hex(0x15, 20);
        ua_create(&db, &mut router, "uca-a", 10);
        ua_bind(&db, &mut router, "uca-a", "owner", "evm", 1, &evm_addr, 11);

        let err = run_public_rpc(
            &db,
            &mut router,
            None,
            "eth_sendRawTransaction",
            &serde_json::json!({
                "uca_id": "uca-a",
                "role": "owner",
                "chain_id": 1,
                "from": evm_addr,
                "signature_domain": "eip712:10:demo-app",
                "nonce": 0,
                "tx_type4": false,
                "now": 12,
            }),
        )
        .expect_err("wrong chain eip712 domain should reject")
        .to_string();
        assert!(err.contains("domain mismatch"));
    }

    #[test]
    fn unified_account_gate_ua_g06_nonce_replay_rejected() {
        let db = QueryStateDb::default();
        let mut router = UnifiedAccountRouter::new();
        let evm_addr = ua_hex(0x16, 20);
        ua_create(&db, &mut router, "uca-a", 10);
        ua_bind(&db, &mut router, "uca-a", "owner", "evm", 1, &evm_addr, 11);

        run_public_rpc(
            &db,
            &mut router,
            None,
            "eth_sendRawTransaction",
            &serde_json::json!({
                "uca_id": "uca-a",
                "role": "owner",
                "chain_id": 1,
                "from": evm_addr,
                "nonce": 0,
                "tx_type4": false,
                "now": 12,
            }),
        )
        .expect("first nonce should pass");

        let err = run_public_rpc(
            &db,
            &mut router,
            None,
            "eth_sendRawTransaction",
            &serde_json::json!({
                "uca_id": "uca-a",
                "role": "owner",
                "chain_id": 1,
                "from": evm_addr,
                "nonce": 0,
                "tx_type4": false,
                "now": 13,
            }),
        )
        .expect_err("replay nonce should reject")
        .to_string();
        assert!(err.contains("nonce rejected"));
        assert!(router.events().iter().any(|event| {
            matches!(
                event,
                AccountAuditEvent::NonceReplayRejected { uca_id, .. } if uca_id == "uca-a"
            )
        }));
    }

    #[test]
    fn unified_account_gate_ua_g07_nonce_reverse_order_rejected() {
        let db = QueryStateDb::default();
        let mut router = UnifiedAccountRouter::new();
        let evm_addr = ua_hex(0x17, 20);
        ua_create(&db, &mut router, "uca-a", 10);
        ua_bind(&db, &mut router, "uca-a", "owner", "evm", 1, &evm_addr, 11);

        run_public_rpc(
            &db,
            &mut router,
            None,
            "eth_sendRawTransaction",
            &serde_json::json!({
                "uca_id": "uca-a",
                "role": "owner",
                "chain_id": 1,
                "from": evm_addr,
                "nonce": 0,
                "tx_type4": false,
                "now": 12,
            }),
        )
        .expect("nonce=0 should pass");
        run_public_rpc(
            &db,
            &mut router,
            None,
            "eth_sendRawTransaction",
            &serde_json::json!({
                "uca_id": "uca-a",
                "role": "owner",
                "chain_id": 1,
                "from": evm_addr,
                "nonce": 1,
                "tx_type4": false,
                "now": 13,
            }),
        )
        .expect("nonce=1 should pass");

        let err = run_public_rpc(
            &db,
            &mut router,
            None,
            "eth_sendRawTransaction",
            &serde_json::json!({
                "uca_id": "uca-a",
                "role": "owner",
                "chain_id": 1,
                "from": evm_addr,
                "nonce": 0,
                "tx_type4": false,
                "now": 14,
            }),
        )
        .expect_err("reverse nonce should reject")
        .to_string();
        assert!(err.contains("nonce rejected"));
    }

    #[test]
    fn unified_account_gate_ua_g08_permission_delegate_cannot_update_policy() {
        let db = QueryStateDb::default();
        let mut router = UnifiedAccountRouter::new();
        ua_create(&db, &mut router, "uca-a", 10);
        let err = run_public_rpc(
            &db,
            &mut router,
            None,
            "ua_setPolicy",
            &serde_json::json!({
                "uca_id": "uca-a",
                "role": "delegate",
                "nonce_scope": "global",
                "allow_type4_with_delegate_or_session": false,
                "now": 11,
            }),
        )
        .expect_err("delegate should not update policy")
        .to_string();
        assert!(err.contains("permission denied"));
        assert!(router.events().iter().any(|event| {
            matches!(
                event,
                AccountAuditEvent::PermissionDenied {
                    uca_id,
                    role: AccountRole::Delegate,
                    ..
                } if uca_id == "uca-a"
            )
        }));
    }

    #[test]
    fn unified_account_gate_ua_g09_permission_expired_session_key_rejected() {
        let db = QueryStateDb::default();
        let mut router = UnifiedAccountRouter::new();
        let evm_addr = ua_hex(0x19, 20);
        ua_create(&db, &mut router, "uca-a", 10);
        ua_bind(&db, &mut router, "uca-a", "owner", "evm", 1, &evm_addr, 11);

        let err = run_public_rpc(
            &db,
            &mut router,
            None,
            "eth_sendRawTransaction",
            &serde_json::json!({
                "uca_id": "uca-a",
                "role": "session_key",
                "chain_id": 1,
                "from": evm_addr,
                "nonce": 0,
                "tx_type4": false,
                "session_expires_at": 100,
                "now": 101,
            }),
        )
        .expect_err("expired session key should reject")
        .to_string();
        assert!(err.contains("session key expired"));
        assert!(router.events().iter().any(|event| {
            matches!(
                event,
                AccountAuditEvent::SessionKeyExpired {
                    uca_id,
                    expires_at,
                    now,
                    ..
                } if uca_id == "uca-a" && *expires_at == 100 && *now == 101
            )
        }));
    }

    #[test]
    fn unified_account_gate_ua_g10_boundary_eth_cross_chain_atomic_rejected() {
        let db = QueryStateDb::default();
        let mut router = UnifiedAccountRouter::new();
        let evm_addr = ua_hex(0x1A, 20);
        ua_create(&db, &mut router, "uca-a", 10);
        ua_bind(&db, &mut router, "uca-a", "owner", "evm", 1, &evm_addr, 11);

        let err = run_public_rpc(
            &db,
            &mut router,
            None,
            "eth_sendRawTransaction",
            &serde_json::json!({
                "uca_id": "uca-a",
                "role": "owner",
                "chain_id": 1,
                "from": evm_addr,
                "nonce": 0,
                "tx_type4": false,
                "wants_cross_chain_atomic": true,
                "now": 12,
            }),
        )
        .expect_err("eth cross-chain atomic should reject")
        .to_string();
        assert!(err.contains("cross-chain atomic"));
    }

    #[test]
    fn unified_account_gate_ua_g11_boundary_web30_single_chain_passes_without_eth_pollution() {
        let db = QueryStateDb::default();
        let mut router = UnifiedAccountRouter::new();
        let web30_addr = ua_hex(0x1B, 20);
        let evm_addr = ua_hex(0x2B, 20);
        ua_create(&db, &mut router, "uca-a", 10);
        ua_bind(
            &db,
            &mut router,
            "uca-a",
            "owner",
            "web30",
            100,
            &web30_addr,
            11,
        );
        ua_bind(&db, &mut router, "uca-a", "owner", "evm", 1, &evm_addr, 12);

        let (web30_resp, web30_changed) = run_public_rpc(
            &db,
            &mut router,
            None,
            "web30_sendTransaction",
            &serde_json::json!({
                "uca_id": "uca-a",
                "role": "owner",
                "chain_id": 100,
                "from": web30_addr,
                "nonce": 0,
                "tx_type4": false,
                "now": 13,
            }),
        )
        .expect("web30 route should pass");
        assert!(web30_changed);
        assert_eq!(web30_resp["accepted"].as_bool(), Some(true));
        assert_eq!(web30_resp["decision"]["kind"].as_str(), Some("fast_path"));

        let (eth_resp, eth_changed) = run_public_rpc(
            &db,
            &mut router,
            None,
            "eth_sendRawTransaction",
            &serde_json::json!({
                "uca_id": "uca-a",
                "role": "owner",
                "chain_id": 1,
                "from": evm_addr,
                "nonce": 0,
                "tx_type4": false,
                "now": 14,
            }),
        )
        .expect("eth route should still pass on its own persona nonce");
        assert!(eth_changed);
        assert_eq!(eth_resp["accepted"].as_bool(), Some(true));
        assert_eq!(eth_resp["decision"]["kind"].as_str(), Some("adapter"));
    }

    #[test]
    fn unified_account_gate_ua_g12_type4_supported_mode_passes() {
        let db = QueryStateDb::default();
        let mut router = UnifiedAccountRouter::new();
        let evm_addr = ua_hex(0x1C, 20);
        ua_create(&db, &mut router, "uca-a", 10);
        ua_bind(&db, &mut router, "uca-a", "owner", "evm", 1, &evm_addr, 11);
        run_public_rpc(
            &db,
            &mut router,
            None,
            "ua_setPolicy",
            &serde_json::json!({
                "uca_id": "uca-a",
                "role": "owner",
                "nonce_scope": "persona",
                "allow_type4_with_delegate_or_session": true,
                "now": 12,
            }),
        )
        .expect("owner should update policy");

        let (resp, changed) = run_public_rpc(
            &db,
            &mut router,
            None,
            "eth_sendRawTransaction",
            &serde_json::json!({
                "uca_id": "uca-a",
                "role": "delegate",
                "chain_id": 1,
                "from": evm_addr,
                "nonce": 0,
                "tx_type4": true,
                "now": 13,
            }),
        )
        .expect("type4 should pass in supported mode");
        assert!(changed);
        assert_eq!(resp["accepted"].as_bool(), Some(true));
        assert_eq!(resp["tx_type4"].as_bool(), Some(true));
    }

    #[test]
    fn unified_account_gate_ua_g13_type4_reject_mode_returns_fixed_error() {
        let db = QueryStateDb::default();
        let mut router = UnifiedAccountRouter::new();
        let evm_addr = ua_hex(0x1D, 20);
        ua_create(&db, &mut router, "uca-a", 10);
        ua_bind(&db, &mut router, "uca-a", "owner", "evm", 1, &evm_addr, 11);

        let err = run_public_rpc(
            &db,
            &mut router,
            None,
            "eth_sendRawTransaction",
            &serde_json::json!({
                "uca_id": "uca-a",
                "role": "delegate",
                "chain_id": 1,
                "from": evm_addr,
                "nonce": 0,
                "tx_type4": true,
                "now": 12,
            }),
        )
        .expect_err("type4 reject mode should fail")
        .to_string();
        assert!(
            err.contains("type4 transaction cannot be used with delegate/session role by policy")
        );
        assert!(router.events().iter().any(|event| {
            matches!(
                event,
                AccountAuditEvent::Type4PolicyRejected {
                    uca_id,
                    role: AccountRole::Delegate,
                    ..
                } if uca_id == "uca-a"
            )
        }));
    }

    #[test]
    fn unified_account_gate_ua_g14_type4_with_session_key_rejected_by_policy() {
        let db = QueryStateDb::default();
        let mut router = UnifiedAccountRouter::new();
        let evm_addr = ua_hex(0x1E, 20);
        ua_create(&db, &mut router, "uca-a", 10);
        ua_bind(&db, &mut router, "uca-a", "owner", "evm", 1, &evm_addr, 11);

        let err = run_public_rpc(
            &db,
            &mut router,
            None,
            "eth_sendRawTransaction",
            &serde_json::json!({
                "uca_id": "uca-a",
                "role": "session_key",
                "chain_id": 1,
                "from": evm_addr,
                "nonce": 0,
                "tx_type4": true,
                "session_expires_at": 9999,
                "now": 12,
            }),
        )
        .expect_err("type4 + session should reject by default policy")
        .to_string();
        assert!(
            err.contains("type4 transaction cannot be used with delegate/session role by policy")
        );
        assert!(router.events().iter().any(|event| {
            matches!(
                event,
                AccountAuditEvent::Type4PolicyRejected {
                    uca_id,
                    role: AccountRole::SessionKey,
                    ..
                } if uca_id == "uca-a"
            )
        }));
    }

    #[test]
    fn unified_account_public_rpc_eth_send_raw_blob_type3_rejected_in_m0() {
        let db = QueryStateDb::default();
        let mut router = UnifiedAccountRouter::new();
        let evm_addr = ua_hex(0x2E, 20);
        ua_create(&db, &mut router, "uca-a", 10);
        ua_bind(&db, &mut router, "uca-a", "owner", "evm", 1, &evm_addr, 11);

        let err = run_public_rpc(
            &db,
            &mut router,
            None,
            "eth_sendRawTransaction",
            &serde_json::json!({
                "uca_id": "uca-a",
                "role": "owner",
                "chain_id": 1,
                "from": evm_addr,
                "nonce": 0,
                "raw_tx": "0x03c0",
                "now": 12,
            }),
        )
        .expect_err("type3 blob write path should reject in M0")
        .to_string();
        assert!(err.contains("blob (type 3) write path disabled in M0"));
    }

    #[test]
    fn unified_account_public_rpc_eth_send_raw_type4_inferred_from_envelope() {
        let db = QueryStateDb::default();
        let mut router = UnifiedAccountRouter::new();
        let evm_addr = ua_hex(0x3E, 20);
        ua_create(&db, &mut router, "uca-a", 10);
        ua_bind(&db, &mut router, "uca-a", "owner", "evm", 1, &evm_addr, 11);

        let err = run_public_rpc(
            &db,
            &mut router,
            None,
            "eth_sendRawTransaction",
            &serde_json::json!({
                "uca_id": "uca-a",
                "role": "delegate",
                "chain_id": 1,
                "from": evm_addr,
                "nonce": 0,
                "raw_tx": "0x04c0",
                "now": 12,
            }),
        )
        .expect_err("type4 should be inferred from raw envelope")
        .to_string();
        assert!(err.contains("type4 transaction cannot be used with delegate/session role by policy"));
    }

    #[test]
    fn unified_account_public_rpc_eth_send_raw_type2_inferred_and_routes() {
        let db = QueryStateDb::default();
        let mut router = UnifiedAccountRouter::new();
        let evm_addr = ua_hex(0x4E, 20);
        ua_create(&db, &mut router, "uca-a", 10);
        ua_bind(&db, &mut router, "uca-a", "owner", "evm", 1, &evm_addr, 11);

        let (resp, changed) = run_public_rpc(
            &db,
            &mut router,
            None,
            "eth_sendRawTransaction",
            &serde_json::json!({
                "uca_id": "uca-a",
                "role": "owner",
                "chain_id": 1,
                "from": evm_addr,
                "raw_tx": "0x02e20180021e827530944e4e4e4e4e4e4e4e4e4e4e4e4e4e4e4e4e4e4e4e0480c0010101",
                "now": 12,
            }),
        )
        .expect("type2 raw tx should be inferred and routed");
        assert!(changed);
        assert_eq!(resp["accepted"].as_bool(), Some(true));
        assert_eq!(resp["nonce"].as_u64(), Some(0));
        assert_eq!(resp["gas_limit"].as_u64(), Some(30_000));
        assert_eq!(resp["tx_type"].as_u64(), Some(2));
        assert_eq!(resp["tx_type4"].as_bool(), Some(false));
        assert_eq!(resp["tx_ir_type"].as_str(), Some("transfer"));
        assert_eq!(resp["tx_ir_data_len"].as_u64(), Some(0));
    }

    #[test]
    fn unified_account_public_rpc_eth_send_raw_type2_contract_call_inferred_ir_type() {
        let db = QueryStateDb::default();
        let mut router = UnifiedAccountRouter::new();
        let evm_addr = ua_hex(0x5E, 20);
        ua_create(&db, &mut router, "uca-a", 10);
        ua_bind(&db, &mut router, "uca-a", "owner", "evm", 1, &evm_addr, 11);

        let (resp, changed) = run_public_rpc(
            &db,
            &mut router,
            None,
            "eth_sendRawTransaction",
            &serde_json::json!({
                "uca_id": "uca-a",
                "role": "owner",
                "chain_id": 1,
                "from": evm_addr,
                "raw_tx": "0x02e30180021e827530945e5e5e5e5e5e5e5e5e5e5e5e5e5e5e5e5e5e5e5e0481aac0010101",
                "now": 12,
            }),
        )
        .expect("type2 raw tx with call data should be inferred and routed");
        assert!(changed);
        assert_eq!(resp["accepted"].as_bool(), Some(true));
        assert_eq!(resp["nonce"].as_u64(), Some(0));
        assert_eq!(resp["tx_type"].as_u64(), Some(2));
        assert_eq!(resp["tx_ir_type"].as_str(), Some("contract_call"));
        assert_eq!(resp["tx_ir_data_len"].as_u64(), Some(1));
    }

    #[test]
    fn unified_account_public_rpc_eth_send_raw_routes_into_local_pending_ingress() {
        let db = QueryStateDb::default();
        let mut router = UnifiedAccountRouter::new();
        let evm_addr = ua_hex(0x6E, 20);
        ua_create(&db, &mut router, "uca-a", 10);
        ua_bind(&db, &mut router, "uca-a", "owner", "evm", 1, &evm_addr, 11);

        let raw_tx_hex =
            "0x02e20180021e827530946e6e6e6e6e6e6e6e6e6e6e6e6e6e6e6e6e6e6e6e0480c0010101";
        let raw_tx_bytes = decode_hex_bytes(raw_tx_hex, "raw_tx").expect("raw tx bytes");
        let expected_hash = eth_rlpx_transaction_hash_v1(raw_tx_bytes.as_slice());
        let expected_hash_hex = format!("0x{}", to_hex(&expected_hash));

        let (resp, changed) = run_public_rpc(
            &db,
            &mut router,
            None,
            "eth_sendRawTransaction",
            &serde_json::json!({
                "uca_id": "uca-a",
                "role": "owner",
                "chain_id": 1,
                "from": evm_addr,
                "raw_tx": raw_tx_hex,
                "now": 12,
            }),
        )
        .expect("raw tx should route");
        assert!(changed);
        assert_eq!(resp["accepted"].as_bool(), Some(true));
        assert_eq!(resp["pending_tx_local_ingress"].as_bool(), Some(true));
        assert_eq!(resp["pending_tx_hash"].as_str(), Some(expected_hash_hex.as_str()));

        let pending = novovm_network::get_network_runtime_native_pending_tx_v1(1, expected_hash)
            .expect("pending tx should be tracked");
        assert_eq!(
            pending.origin,
            novovm_network::NetworkRuntimeNativePendingTxOriginV1::Local
        );
        assert_eq!(
            pending.lifecycle_stage,
            novovm_network::NetworkRuntimeNativePendingTxLifecycleStageV1::Pending
        );
        assert!(pending.ingress_count >= 1);

        let candidates =
            novovm_network::snapshot_network_runtime_native_pending_tx_broadcast_candidates_v1(
                1, 64, 3,
            );
        assert!(candidates.iter().any(|item| {
            item.tx_hash == expected_hash
                && item.tx_payload_len > 0
                && !item.tx_payload.is_empty()
        }));
    }

    #[test]
    fn unified_account_public_rpc_eth_send_raw_type2_chain_id_mismatch_rejected() {
        let db = QueryStateDb::default();
        let mut router = UnifiedAccountRouter::new();
        let evm_addr = ua_hex(0x4E, 20);
        ua_create(&db, &mut router, "uca-a", 10);
        ua_bind(&db, &mut router, "uca-a", "owner", "evm", 1, &evm_addr, 11);

        let err = run_public_rpc(
            &db,
            &mut router,
            None,
            "eth_sendRawTransaction",
            &serde_json::json!({
                "uca_id": "uca-a",
                "role": "owner",
                "chain_id": 56,
                "from": evm_addr,
                "raw_tx": "0x02e20109021e827530944e4e4e4e4e4e4e4e4e4e4e4e4e4e4e4e4e4e4e4e0480c0010101",
                "now": 12,
            }),
        )
        .expect_err("chain_id mismatch should reject")
        .to_string();
        assert!(err.contains("chain_id mismatch"));
    }

    #[test]
    fn public_rpc_error_code_maps_eth_blob_and_mismatch_cases() {
        assert_eq!(
            public_rpc_error_code_for_method(
                "eth_sendRawTransaction",
                "unsupported eth tx type: blob (type 3) write path disabled in M0"
            ),
            -32031
        );
        assert_eq!(
            public_rpc_error_code_for_method(
                "eth_sendRawTransaction",
                "nonce mismatch: explicit=0 inferred_from_raw=9"
            ),
            -32033
        );
        assert_eq!(
            public_rpc_error_code_for_method("eth_sendRawTransaction", "intrinsic gas too low"),
            -32034
        );
        assert_eq!(
            public_rpc_error_code_for_method(
                "eth_getLogs",
                "unsupported eth filter/reorg method in M0: eth_getLogs"
            ),
            -32036
        );
        assert_eq!(
            public_rpc_error_code_for_method(
                "eth_getCode",
                "eth method not enabled in supervm public rpc plugin scope: eth_getCode"
            ),
            -32601
        );
    }

    #[test]
    fn unified_account_public_rpc_eth_filter_reorg_methods_rejected_in_m0() {
        let db = QueryStateDb::default();
        let mut router = UnifiedAccountRouter::new();
        let err = run_public_rpc(
            &db,
            &mut router,
            None,
            "eth_getLogs",
            &serde_json::json!({}),
        )
        .expect_err("eth_getLogs must be rejected in M0")
        .to_string();
        assert!(err.contains("unsupported eth filter/reorg method in M0"));
    }

    #[test]
    fn unified_account_public_rpc_eth_non_plugin_method_rejected() {
        let db = QueryStateDb::default();
        let mut router = UnifiedAccountRouter::new();
        let err = run_public_rpc(
            &db,
            &mut router,
            None,
            "eth_getCode",
            &serde_json::json!({}),
        )
        .expect_err("eth_getCode must be rejected in plugin-only scope")
        .to_string();
        assert!(err.contains("eth method not enabled in supervm public rpc plugin scope"));
    }

    #[test]
    fn unified_account_gate_ua_g15_uniqueness_conflict_signal_blocks_second_owner() {
        let db = QueryStateDb::default();
        let mut router = UnifiedAccountRouter::new();
        let evm_addr = ua_hex(0x1F, 20);
        ua_create(&db, &mut router, "uca-a", 10);
        ua_create(&db, &mut router, "uca-b", 10);
        ua_bind(&db, &mut router, "uca-a", "owner", "evm", 1, &evm_addr, 11);

        let err = run_public_rpc(
            &db,
            &mut router,
            None,
            "ua_bindPersona",
            &serde_json::json!({
                "uca_id": "uca-b",
                "role": "owner",
                "persona_type": "evm",
                "chain_id": 1,
                "external_address": evm_addr,
                "now": 12,
            }),
        )
        .expect_err("second owner bind should reject")
        .to_string();
        assert!(err.contains("binding conflict"));

        let (owner_resp, changed) = run_public_rpc(
            &db,
            &mut router,
            None,
            "ua_getBindingOwner",
            &serde_json::json!({
                "persona_type": "evm",
                "chain_id": 1,
                "external_address": evm_addr,
            }),
        )
        .expect("ua_getBindingOwner should succeed");
        assert!(!changed);
        assert_eq!(owner_resp["found"].as_bool(), Some(true));
        assert_eq!(owner_resp["owner_uca_id"].as_str(), Some("uca-a"));
    }

    #[test]
    fn unified_account_gate_ua_g16_recovery_rotate_then_revoke_emits_events() {
        let db = QueryStateDb::default();
        let mut router = UnifiedAccountRouter::new();
        let evm_addr = ua_hex(0x2A, 20);
        ua_create(&db, &mut router, "uca-a", 10);
        ua_bind(&db, &mut router, "uca-a", "owner", "evm", 1, &evm_addr, 11);

        let (rotate_resp, rotate_changed) = run_public_rpc(
            &db,
            &mut router,
            None,
            "ua_rotatePrimaryKey",
            &serde_json::json!({
                "uca_id": "uca-a",
                "role": "owner",
                "next_primary_key_ref": ua_hex(0x55, 32),
                "now": 12,
            }),
        )
        .expect("ua_rotatePrimaryKey should succeed");
        assert!(rotate_changed);
        assert_eq!(rotate_resp["rotated"].as_bool(), Some(true));

        let (revoke_resp, revoke_changed) = run_public_rpc(
            &db,
            &mut router,
            None,
            "ua_revokePersona",
            &serde_json::json!({
                "uca_id": "uca-a",
                "role": "owner",
                "persona_type": "evm",
                "chain_id": 1,
                "external_address": evm_addr,
                "cooldown_seconds": 30,
                "now": 13,
            }),
        )
        .expect("ua_revokePersona should succeed");
        assert!(revoke_changed);
        assert_eq!(revoke_resp["revoked"].as_bool(), Some(true));

        let (owner_resp, changed) = run_public_rpc(
            &db,
            &mut router,
            None,
            "ua_getBindingOwner",
            &serde_json::json!({
                "persona_type": "evm",
                "chain_id": 1,
                "external_address": evm_addr,
            }),
        )
        .expect("owner lookup should succeed");
        assert!(!changed);
        assert_eq!(owner_resp["found"].as_bool(), Some(false));

        assert!(router.events().iter().any(|event| {
            matches!(event, AccountAuditEvent::KeyRotated { uca_id, .. } if uca_id == "uca-a")
        }));
        assert!(router.events().iter().any(|event| {
            matches!(event, AccountAuditEvent::BindingRevoked { uca_id, .. } if uca_id == "uca-a")
        }));
    }

    #[test]
    fn d2d3_persistence_path_guard_accepts_path_under_root() {
        let nonce = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .expect("clock should be >= epoch")
            .as_nanos();
        let root = std::env::temp_dir().join(format!("novovm-d1-root-{}", nonce));
        let path = root.join("query").join("novovm-chain-query-db.json");
        ensure_d2d3_path_under_d1_root("chain_query_db", &path, &root)
            .expect("path under D1 root should pass");
    }

    #[test]
    fn d2d3_persistence_path_guard_rejects_path_outside_root() {
        let nonce = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .expect("clock should be >= epoch")
            .as_nanos();
        let root = std::env::temp_dir().join(format!("novovm-d1-root-{}", nonce));
        let outside = std::env::temp_dir()
            .join(format!("novovm-d1-outside-{}", nonce))
            .join("novovm-chain-query-db.json");
        let err = ensure_d2d3_path_under_d1_root("chain_query_db", &outside, &root)
            .expect_err("path outside D1 root should fail")
            .to_string();
        assert!(err.contains("outside D1 root"));
    }

    #[test]
    fn d2d3_persistence_path_guard_rejects_relative_escape() {
        let nonce = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .expect("clock should be >= epoch")
            .as_nanos();
        let root = std::env::temp_dir().join(format!("novovm-d1-root-{}", nonce));
        let escaped = root
            .join("query")
            .join("..")
            .join("..")
            .join("novovm-outside")
            .join("novovm-chain-query-db.json");
        let err = ensure_d2d3_path_under_d1_root("chain_query_db", &escaped, &root)
            .expect_err("relative escape should fail")
            .to_string();
        assert!(err.contains("outside D1 root"));
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
    fn apply_governance_vote_verifier_rejects_mldsa87_when_disabled_by_policy() {
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
        assert!(err.contains("disabled-by-policy"));
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


