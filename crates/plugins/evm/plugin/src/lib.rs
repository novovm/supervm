mod bincode_compat;
use novovm_adapter_api::ChainAdapter;
use novovm_adapter_api::{
    AccountAuditEvent, AccountRole, AtomicBroadcastReadyV1, AtomicCrossChainIntentV1,
    AtomicIntentReceiptV1, AtomicIntentStatus, ChainConfig, ChainType, EvmFeeIncomeRecordV1,
    EvmFeePayoutInstructionV1, EvmFeeSettlementPolicyV1, EvmFeeSettlementRecordV1,
    EvmFeeSettlementResultV1, EvmFeeSettlementSnapshotV1, EvmMempoolIngressFrameV1, PersonaAddress,
    PersonaType, ProtocolKind, RouteRequest, StateIR, TxIR, TxType, UnifiedAccountError,
    UnifiedAccountRouter,
};
use novovm_adapter_evm_core::{
    active_precompile_set_m0, resolve_evm_chain_type_from_chain_id, resolve_evm_profile,
    validate_tx_semantics_m0,
};
use novovm_adapter_novovm::NovoVmAdapter;
use novovm_exec::{
    project_tx_execution_artifacts_v1, AoemBatchExecutionArtifactsV1, AoemEventLogV1,
    AoemExecFacade, AoemExecOutput, AoemProjectedTxExecutionV1, AoemRuntimeConfig,
    AoemTxExecutionArtifactV1, ExecOpV2, AOEM_LOG_BLOOM_BYTES_V1,
};
pub use novovm_exec::{
    SupervmEvmExecutionLogV1, SupervmEvmExecutionReceiptV1, SupervmEvmStateMirrorUpdateV1,
};
use rocksdb::Options as RocksDbOptions;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::{BTreeMap, HashMap, HashSet, VecDeque};
use std::fs::{self, OpenOptions};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, AtomicUsize, Ordering};
use std::sync::{Mutex, OnceLock};

#[cfg(test)]
use novovm_exec::AoemTxExecutionAnchorV1;

pub const NOVOVM_ADAPTER_PLUGIN_ABI_V1: u32 = 1;
pub const NOVOVM_ADAPTER_PLUGIN_CAP_APPLY_IR_V1: u64 = 0x1;
pub const NOVOVM_ADAPTER_PLUGIN_CAP_UA_SELF_GUARD_V1: u64 = 0x2;
pub const NOVOVM_ADAPTER_PLUGIN_CAP_EVM_RUNTIME_V1: u64 = 0x4;
pub const NOVOVM_ADAPTER_PLUGIN_APPLY_FLAG_UA_SELF_GUARD_V1: u64 = 0x1;
pub const NOVOVM_ADAPTER_PLUGIN_APPLY_FLAG_ATOMIC_INTENT_GUARD_V1: u64 = 0x2;
pub const NOVOVM_ADAPTER_PLUGIN_APPLY_FLAG_MAINLINE_INGRESS_BYPASS_V1: u64 = 0x4;
pub const NOVOVM_ADAPTER_PLUGIN_APPLY_FLAG_MAINLINE_RECEIPT_EXPORT_V1: u64 = 0x8;
pub const NOVOVM_ADAPTER_PLUGIN_APPLY_FLAG_MAINLINE_RECEIPT_INGEST_V1: u64 = 0x10;
// Stable FFI return codes (external contract):
//  0=ok, -1=invalid_arg, -2=unsupported_chain, -3=decode_failed,
// -4=empty_batch, -5=unsupported_tx_type, -6=apply_failed,
// -7=ua_self_guard_failed, -8=payload_too_large, -9=batch_too_large,
// -10=buffer_too_small.
pub const NOVOVM_ADAPTER_PLUGIN_RC_OK: i32 = 0;
pub const NOVOVM_ADAPTER_PLUGIN_RC_INVALID_ARG: i32 = -1;
pub const NOVOVM_ADAPTER_PLUGIN_RC_UNSUPPORTED_CHAIN: i32 = -2;
pub const NOVOVM_ADAPTER_PLUGIN_RC_DECODE_FAILED: i32 = -3;
pub const NOVOVM_ADAPTER_PLUGIN_RC_EMPTY_BATCH: i32 = -4;
pub const NOVOVM_ADAPTER_PLUGIN_RC_UNSUPPORTED_TX_TYPE: i32 = -5;
pub const NOVOVM_ADAPTER_PLUGIN_RC_APPLY_FAILED: i32 = -6;
pub const NOVOVM_ADAPTER_PLUGIN_RC_UA_SELF_GUARD_FAILED: i32 = -7;
pub const NOVOVM_ADAPTER_PLUGIN_RC_PAYLOAD_TOO_LARGE: i32 = -8;
pub const NOVOVM_ADAPTER_PLUGIN_RC_BATCH_TOO_LARGE: i32 = -9;
pub const NOVOVM_ADAPTER_PLUGIN_RC_BUFFER_TOO_SMALL: i32 = -10;

const UA_PLUGIN_STORE_VERSION_V1: u32 = 1;
const UA_PLUGIN_STORE_KEY_V1: &[u8] = b"ua_plugin:store:router:v1";
const UA_PLUGIN_AUDIT_HEAD_KEY_V1: &[u8] = b"ua_plugin:audit:head:v1";
const UA_PLUGIN_AUDIT_SEQ_KEY_PREFIX_V1: &str = "ua_plugin:audit:seq:v1:";
const UA_PLUGIN_ARTIFACTS_SUBDIR: &str = "artifacts/migration/unifiedaccount";
const UA_PLUGIN_ALLOW_NON_PROD_BACKEND_ENV: &str = "NOVOVM_ALLOW_NON_PROD_PLUGIN_BACKEND";
const MAX_PLUGIN_TX_IR_BYTES: usize = 16 * 1024 * 1024;
const MAX_PLUGIN_TX_COUNT: usize = 100_000;
const DEFAULT_INGRESS_QUEUE_MAX: usize = 4096;
const DEFAULT_ATOMIC_RECEIPT_QUEUE_MAX: usize = 2048;
const DEFAULT_SETTLEMENT_RECORD_QUEUE_MAX: usize = 4096;
const DEFAULT_PAYOUT_INSTRUCTION_QUEUE_MAX: usize = 4096;
const DEFAULT_ATOMIC_BROADCAST_QUEUE_MAX: usize = 2048;
const DEFAULT_EXECUTION_RECEIPT_QUEUE_MAX: usize = 4096;
const DEFAULT_STATE_MIRROR_UPDATE_QUEUE_MAX: usize = 2048;
const DEFAULT_SETTLEMENT_CONVERT_NUM: u128 = 1;
const DEFAULT_SETTLEMENT_CONVERT_DEN: u128 = 1;
const DEFAULT_TXPOOL_PRICE_BUMP_PCT: u64 = 10;
const DEFAULT_TXPOOL_MAX_PENDING_PER_SENDER: usize = 64;
const DEFAULT_TXPOOL_MAX_NONCE_GAP: u64 = 1024;
const DEFAULT_TXPOOL_EXECUTABLE_QUEUE_MAX: usize = 4096;
const DEFAULT_EVM_RUNTIME_SHARD_COUNT: usize = 16;
const MAX_EVM_RUNTIME_SHARD_COUNT: usize = 128;
const DEFAULT_EVM_TXPOOL_SHARD_COUNT: usize = 16;
const MAX_EVM_TXPOOL_SHARD_COUNT: usize = 128;

#[derive(Debug)]
struct EvmRuntimeConfig {
    ingress_queue_max: usize,
    atomic_receipt_queue_max: usize,
    settlement_record_queue_max: usize,
    payout_instruction_queue_max: usize,
    atomic_broadcast_queue_max: usize,
    execution_receipt_queue_max: usize,
    state_mirror_update_queue_max: usize,
    convert_num: u128,
    convert_den: u128,
    txpool_price_bump_pct: u64,
    txpool_max_pending_per_sender: usize,
    txpool_max_nonce_gap: u64,
    txpool_executable_queue_max: usize,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
struct IngressNonceKey {
    chain_id: u64,
    from: Vec<u8>,
    nonce: u64,
}

#[derive(Debug, Clone)]
struct IngressNonceMeta {
    tx_hash: Vec<u8>,
    gas_price: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EvmPendingSenderBucketV1 {
    pub chain_id: u64,
    pub sender: Vec<u8>,
    pub txs: Vec<TxIR>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EvmTxpoolRejectReasonV1 {
    ReplacementUnderpriced,
    NonceTooLow,
    NonceTooHigh,
    PoolFull,
    Rejected,
}

impl EvmTxpoolRejectReasonV1 {
    pub fn as_str(self) -> &'static str {
        match self {
            EvmTxpoolRejectReasonV1::ReplacementUnderpriced => "replacement_underpriced",
            EvmTxpoolRejectReasonV1::NonceTooLow => "nonce_too_low",
            EvmTxpoolRejectReasonV1::NonceTooHigh => "nonce_too_high",
            EvmTxpoolRejectReasonV1::PoolFull => "pool_full",
            EvmTxpoolRejectReasonV1::Rejected => "rejected",
        }
    }
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct EvmRuntimeTapSummaryV1 {
    pub requested: usize,
    pub accepted: usize,
    pub dropped: usize,
    pub replaced_pending: usize,
    pub replaced_executable: usize,
    pub dropped_underpriced: usize,
    pub dropped_nonce_gap: usize,
    pub dropped_nonce_too_low: usize,
    pub dropped_over_capacity: usize,
    pub primary_reject_reason: Option<EvmTxpoolRejectReasonV1>,
    pub reject_reasons: Vec<EvmTxpoolRejectReasonV1>,
}

#[derive(Debug, Clone, Default)]
struct EvmIngressPushOutcome {
    summary: EvmRuntimeTapSummaryV1,
    accepted_txs: Vec<TxIR>,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
struct IngressSenderKey {
    chain_id: u64,
    from: Vec<u8>,
}

#[derive(Debug, Default)]
struct EvmRuntimeState {
    settlement_policy: EvmFeeSettlementPolicyV1,
    settlement_seq: u64,
    reserve_total_wei: u128,
    payout_total_units: u128,
    atomic_receipts: VecDeque<AtomicIntentReceiptV1>,
    settlement_records: VecDeque<EvmFeeSettlementRecordV1>,
    payout_instructions: VecDeque<EvmFeePayoutInstructionV1>,
    atomic_broadcast_ready: VecDeque<AtomicBroadcastReadyV1>,
    execution_receipts: VecDeque<SupervmEvmExecutionReceiptV1>,
    state_mirror_updates: VecDeque<SupervmEvmStateMirrorUpdateV1>,
}

#[derive(Debug, Default)]
struct EvmTxpoolState {
    ingress_frames: VecDeque<EvmMempoolIngressFrameV1>,
    executable_ingress_frames: VecDeque<EvmMempoolIngressFrameV1>,
    pending_by_nonce: HashMap<IngressNonceKey, IngressNonceMeta>,
    pending_by_sender: HashMap<IngressSenderKey, BTreeMap<u64, Vec<u8>>>,
    next_nonce_by_sender: HashMap<IngressSenderKey, u64>,
}

#[repr(C)]
#[derive(Clone, Copy, Debug, Default, Serialize, Deserialize)]
pub struct NovovmAdapterPluginApplyResultV1 {
    pub verified: u8,
    pub applied: u8,
    pub txs: u64,
    pub accounts: u64,
    pub state_root: [u8; 32],
    pub error_code: i32,
}

#[repr(C)]
#[derive(Clone, Copy, Debug, Default, Serialize, Deserialize)]
pub struct NovovmAdapterPluginApplyOptionsV1 {
    pub flags: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SupervmEvmNativeSubmitReportV1 {
    pub chain_type: ChainType,
    pub chain_id: u64,
    pub tx_count: u64,
    pub tap_summary: EvmRuntimeTapSummaryV1,
    pub apply_result: NovovmAdapterPluginApplyResultV1,
    pub exported_receipt_count: u64,
    pub mirrored_receipt_count: u64,
    pub state_version: u64,
    pub ingress_bypassed: bool,
    pub atomic_guard_enabled: bool,
}

#[derive(Debug)]
struct ApplyBatchArtifacts {
    result: NovovmAdapterPluginApplyResultV1,
    execution_receipts: Vec<SupervmEvmExecutionReceiptV1>,
    state_mirror_update: Option<SupervmEvmStateMirrorUpdateV1>,
    state_version: u64,
}

#[derive(Debug)]
struct AoemMainlineOpsBatch {
    _keys: Vec<[u8; 32]>,
    _values: Vec<[u8; 32]>,
    ops: Vec<ExecOpV2>,
    projected_txs: Vec<AoemProjectedTxExecutionV1>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum UaPluginStoreBackend {
    Memory,
    BincodeFile,
    Rocksdb,
}

impl UaPluginStoreBackend {
    fn as_str(self) -> &'static str {
        match self {
            UaPluginStoreBackend::Memory => "memory",
            UaPluginStoreBackend::BincodeFile => "bincode_file",
            UaPluginStoreBackend::Rocksdb => "rocksdb",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum UaPluginAuditBackend {
    None,
    Jsonl,
    Rocksdb,
}

impl UaPluginAuditBackend {
    fn as_str(self) -> &'static str {
        match self {
            UaPluginAuditBackend::None => "none",
            UaPluginAuditBackend::Jsonl => "jsonl",
            UaPluginAuditBackend::Rocksdb => "rocksdb",
        }
    }
}

#[derive(Debug)]
struct UaPluginStandaloneConfig {
    store_backend: UaPluginStoreBackend,
    store_path: PathBuf,
    audit_backend: UaPluginAuditBackend,
    audit_path: PathBuf,
}

#[derive(Debug, Default)]
struct UaPluginRuntime {
    router: UnifiedAccountRouter,
    audit_seq: u64,
}

#[derive(Debug, Deserialize)]
struct UaPluginStoreEnvelopeV1 {
    version: u32,
    router: UnifiedAccountRouter,
    audit_seq: u64,
}

#[derive(Debug, Serialize)]
struct UaPluginStoreEnvelopeRefV1<'a> {
    version: u32,
    router: &'a UnifiedAccountRouter,
    audit_seq: u64,
}

#[derive(Debug, Serialize)]
struct UaPluginAuditRecordV1 {
    seq: u64,
    at: u64,
    source: String,
    chain_id: u64,
    tx_count: usize,
    success: bool,
    error: Option<String>,
    store_backend: String,
    audit_backend: String,
    overlay_route_id: String,
    overlay_route_epoch: u64,
    overlay_route_mask_bits: u8,
    overlay_route_mode: String,
    overlay_route_region: String,
    overlay_route_relay_bucket: u16,
    overlay_route_relay_set_size: u8,
    overlay_route_relay_round: u64,
    overlay_route_relay_index: u8,
    overlay_route_relay_id: String,
    overlay_route_strategy: String,
    overlay_route_hop_count: u8,
    events: Vec<AccountAuditEvent>,
}

static UA_PLUGIN_RUNTIME: OnceLock<Mutex<UaPluginRuntime>> = OnceLock::new();
static UA_PLUGIN_STANDALONE_CONFIG: OnceLock<UaPluginStandaloneConfig> = OnceLock::new();
static EVM_RUNTIME_CONFIG: OnceLock<EvmRuntimeConfig> = OnceLock::new();
static EVM_RUNTIME_SHARDS: OnceLock<Vec<Mutex<EvmRuntimeState>>> = OnceLock::new();
static EVM_RUNTIME_SHARD_CURSOR: AtomicUsize = AtomicUsize::new(0);
static EVM_RUNTIME_SETTLEMENT_SEQ: AtomicU64 = AtomicU64::new(0);
static EVM_EXECUTION_STATE_VERSION: AtomicU64 = AtomicU64::new(0);
static EVM_TXPOOL_SHARDS: OnceLock<Vec<Mutex<EvmTxpoolState>>> = OnceLock::new();
static EVM_TXPOOL_SHARD_CURSOR: AtomicUsize = AtomicUsize::new(0);

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

fn now_unix_sec() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_else(|_| std::time::Duration::from_secs(0))
        .as_secs()
}

fn now_unix_ms() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_else(|_| std::time::Duration::from_secs(0))
        .as_millis() as u64
}

fn next_execution_state_version() -> u64 {
    let now = now_unix_ms().max(1);
    let mut current = EVM_EXECUTION_STATE_VERSION.load(Ordering::Relaxed);
    loop {
        let next = now.max(current.saturating_add(1));
        match EVM_EXECUTION_STATE_VERSION.compare_exchange(
            current,
            next,
            Ordering::SeqCst,
            Ordering::Relaxed,
        ) {
            Ok(_) => return next,
            Err(actual) => current = actual,
        }
    }
}

fn to_lower_hex(bytes: &[u8]) -> String {
    const HEX: &[u8; 16] = b"0123456789abcdef";
    let mut out = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        out.push(HEX[(byte >> 4) as usize] as char);
        out.push(HEX[(byte & 0x0f) as usize] as char);
    }
    out
}

fn derive_primary_key_ref(uca_id: &str) -> Vec<u8> {
    let mut hasher = Sha256::new();
    hasher.update(b"ua-plugin-self-guard-primary-key-ref-v1");
    hasher.update(uca_id.as_bytes());
    hasher.finalize().to_vec()
}

fn decode_hex_bytes(input: &str) -> Option<Vec<u8>> {
    let trimmed = input.trim();
    if trimmed.is_empty() {
        return Some(Vec::new());
    }
    let raw = trimmed
        .strip_prefix("0x")
        .or_else(|| trimmed.strip_prefix("0X"))
        .unwrap_or(trimmed);
    if !raw.len().is_multiple_of(2) {
        return None;
    }
    let mut out = Vec::with_capacity(raw.len() / 2);
    let bytes = raw.as_bytes();
    for idx in (0..bytes.len()).step_by(2) {
        let hi = (bytes[idx] as char).to_digit(16)?;
        let lo = (bytes[idx + 1] as char).to_digit(16)?;
        out.push(((hi << 4) | lo) as u8);
    }
    Some(out)
}

fn parse_env_u128(name: &str, fallback: u128) -> u128 {
    std::env::var(name)
        .ok()
        .and_then(|raw| raw.trim().parse::<u128>().ok())
        .unwrap_or(fallback)
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

fn parse_env_bool(name: &str, fallback: bool) -> bool {
    match std::env::var(name) {
        Ok(raw) => {
            let v = raw.trim();
            v == "1"
                || v.eq_ignore_ascii_case("true")
                || v.eq_ignore_ascii_case("on")
                || v.eq_ignore_ascii_case("yes")
        }
        Err(_) => fallback,
    }
}

#[derive(Debug, Clone)]
struct PluginOverlayRouteMeta {
    overlay_route_id: String,
    overlay_route_epoch: u64,
    overlay_route_mask_bits: u8,
    overlay_route_mode: String,
    overlay_route_region: String,
    overlay_route_relay_bucket: u16,
    overlay_route_relay_set_size: u8,
    overlay_route_relay_round: u64,
    overlay_route_relay_index: u8,
    overlay_route_relay_id: String,
    overlay_route_strategy: String,
    overlay_route_hop_count: u8,
}

fn env_nonempty(name: &str) -> Option<String> {
    std::env::var(name)
        .ok()
        .map(|v| v.trim().to_string())
        .filter(|v| !v.is_empty())
}

fn resolve_plugin_overlay_route_strategy() -> String {
    if let Some(mode) = env_nonempty("NOVOVM_OVERLAY_ROUTE_MODE").map(|v| v.to_ascii_lowercase()) {
        if mode == "fast" {
            return "direct".to_string();
        }
        if mode == "secure" {
            return "multi_hop".to_string();
        }
    }
    if parse_env_bool("NOVOVM_OVERLAY_ROUTE_ENFORCE_MULTI_HOP", false) {
        return "multi_hop".to_string();
    }
    let raw = env_nonempty("NOVOVM_OVERLAY_ROUTE_STRATEGY").unwrap_or_else(|| "direct".to_string());
    let strategy = raw.trim().to_ascii_lowercase();
    if strategy == "multi_hop" {
        "multi_hop".to_string()
    } else {
        "direct".to_string()
    }
}

fn resolve_plugin_overlay_route_hop_count(strategy: &str, min_hops: u8) -> u8 {
    let default_hops = if strategy == "multi_hop" { 3 } else { 1 };
    let raw = parse_env_u64("NOVOVM_OVERLAY_ROUTE_HOP_COUNT", default_hops);
    let hops = raw.clamp(1, 16) as u8;
    if strategy == "multi_hop" {
        hops.max(min_hops.max(1))
    } else {
        1
    }
}

fn build_plugin_overlay_route_id(
    seed: &str,
    node_id: &str,
    session_id: &str,
    route_epoch: u64,
    route_hop_slot: u64,
    mask_bits: u8,
) -> String {
    let material = format!("{seed}|{node_id}|{session_id}|{route_epoch}|{route_hop_slot}");
    let digest = Sha256::digest(material.as_bytes());
    let mut value_bytes = [0u8; 8];
    value_bytes.copy_from_slice(&digest[..8]);
    let mut value = u64::from_be_bytes(value_bytes);
    let keep = mask_bits.min(64);
    if keep == 0 {
        value = 0;
    } else if keep < 64 {
        value &= u64::MAX << (64 - keep);
    }
    format!("ovr{:016x}", value)
}

fn resolve_plugin_overlay_route_relay_bucket(
    route_id: &str,
    region: &str,
    relay_buckets: u16,
) -> u16 {
    if relay_buckets <= 1 {
        return 0;
    }
    let material = format!("{route_id}|{region}");
    let digest = Sha256::digest(material.as_bytes());
    let bucket_seed = u64::from_be_bytes([
        digest[0], digest[1], digest[2], digest[3], digest[4], digest[5], digest[6], digest[7],
    ]);
    (bucket_seed % relay_buckets as u64) as u16
}

fn parse_plugin_overlay_route_relay_candidates() -> Vec<String> {
    env_nonempty("NOVOVM_OVERLAY_ROUTE_RELAY_CANDIDATES")
        .map(|raw| {
            raw.split([',', ';'])
                .map(str::trim)
                .filter(|v| !v.is_empty())
                .map(|v| v.to_string())
                .collect::<Vec<_>>()
        })
        .unwrap_or_default()
}

fn resolve_plugin_overlay_route_relay_selection(
    route_id: &str,
    region: &str,
    relay_bucket: u16,
    route_mode: &str,
    now_unix_sec: u64,
) -> (u8, u64, u8, String) {
    let default_set_size = if route_mode == "secure" { 3 } else { 1 };
    let relay_candidates = parse_plugin_overlay_route_relay_candidates();
    let relay_set_size_raw =
        parse_env_u64("NOVOVM_OVERLAY_ROUTE_RELAY_SET_SIZE", default_set_size).clamp(1, 64) as u8;
    let relay_candidate_cap = relay_candidates.len().min(u8::MAX as usize) as u8;
    let relay_set_size = if relay_candidate_cap == 0 {
        relay_set_size_raw.max(1)
    } else {
        relay_set_size_raw.max(1).min(relay_candidate_cap)
    };
    let default_rotate_seconds = if route_mode == "secure" { 60 } else { 300 };
    let relay_rotate_seconds = parse_env_u64(
        "NOVOVM_OVERLAY_ROUTE_RELAY_ROTATE_SECONDS",
        default_rotate_seconds,
    )
    .clamp(1, 86_400);
    let relay_round = now_unix_sec.saturating_div(relay_rotate_seconds.max(1));
    let relay_index = if relay_set_size <= 1 {
        0u8
    } else {
        let material = format!("{route_id}|{region}|{relay_bucket}|{relay_round}");
        let digest = Sha256::digest(material.as_bytes());
        let pick_seed = u64::from_be_bytes([
            digest[0], digest[1], digest[2], digest[3], digest[4], digest[5], digest[6], digest[7],
        ]);
        (pick_seed % relay_set_size as u64) as u8
    };
    let relay_id = relay_candidates
        .get(relay_index as usize)
        .cloned()
        .unwrap_or_else(|| format!("rly:{}:{}:{}", region, relay_bucket, relay_index));
    (relay_set_size, relay_round, relay_index, relay_id)
}

fn resolve_plugin_overlay_route_meta(now_unix_sec: u64) -> PluginOverlayRouteMeta {
    let node_id = env_nonempty("NOVOVM_OVERLAY_NODE_ID")
        .or_else(|| env_nonempty("NOVOVM_NODE_ID"))
        .unwrap_or_else(|| "plugin".to_string());
    let session_id = env_nonempty("NOVOVM_OVERLAY_SESSION_ID")
        .or_else(|| env_nonempty("NOVOVM_OVERLAY_SESSION"))
        .unwrap_or_else(|| "plugin".to_string());
    let overlay_mode = env_nonempty("NOVOVM_OVERLAY_ROUTE_MODE")
        .map(|v| v.to_ascii_lowercase())
        .filter(|v| v == "secure" || v == "fast")
        .unwrap_or_default();
    let epoch_seconds = parse_env_u64("NOVOVM_OVERLAY_ROUTE_EPOCH_SECONDS", 300).clamp(1, 86_400);
    let route_epoch = now_unix_sec.saturating_div(epoch_seconds.max(1));
    let route_hop_slot_default = if overlay_mode == "fast" { 300 } else { 30 };
    let route_hop_slot_seconds = parse_env_u64(
        "NOVOVM_OVERLAY_ROUTE_HOP_SLOT_SECONDS",
        route_hop_slot_default,
    )
    .clamp(1, 86_400);
    let route_strategy = resolve_plugin_overlay_route_strategy();
    let route_mode = if overlay_mode == "secure" || overlay_mode == "fast" {
        overlay_mode.clone()
    } else if route_strategy == "multi_hop" {
        "secure".to_string()
    } else {
        "fast".to_string()
    };
    let overlay_region =
        env_nonempty("NOVOVM_OVERLAY_ROUTE_REGION").unwrap_or_else(|| "global".to_string());
    let route_hop_slot = if route_strategy == "multi_hop" {
        now_unix_sec.saturating_div(route_hop_slot_seconds.max(1))
    } else {
        0
    };
    let route_mask_bits = parse_env_u64("NOVOVM_OVERLAY_ROUTE_MASK_BITS", 40).clamp(0, 64) as u8;
    let route_id = env_nonempty("NOVOVM_OVERLAY_ROUTE_ID").unwrap_or_else(|| {
        let seed = env_nonempty("NOVOVM_OVERLAY_ROUTE_SEED").unwrap_or_else(|| node_id.clone());
        build_plugin_overlay_route_id(
            &seed,
            &node_id,
            &session_id,
            route_epoch,
            route_hop_slot,
            route_mask_bits,
        )
    });
    let route_relay_buckets_default = if route_mode == "secure" { 8 } else { 1 };
    let route_relay_buckets = parse_env_u64(
        "NOVOVM_OVERLAY_ROUTE_RELAY_BUCKETS",
        route_relay_buckets_default,
    )
    .clamp(1, 1024) as u16;
    let route_relay_bucket =
        resolve_plugin_overlay_route_relay_bucket(&route_id, &overlay_region, route_relay_buckets);
    let (route_relay_set_size, route_relay_round, route_relay_index, route_relay_id) =
        resolve_plugin_overlay_route_relay_selection(
            &route_id,
            &overlay_region,
            route_relay_bucket,
            &route_mode,
            now_unix_sec,
        );
    let route_min_hops_default = if route_strategy == "multi_hop" { 2 } else { 1 };
    let route_min_hops =
        parse_env_u64("NOVOVM_OVERLAY_ROUTE_MIN_HOPS", route_min_hops_default).clamp(1, 16) as u8;
    let route_hop_count = resolve_plugin_overlay_route_hop_count(&route_strategy, route_min_hops);
    PluginOverlayRouteMeta {
        overlay_route_id: route_id,
        overlay_route_epoch: route_epoch,
        overlay_route_mask_bits: route_mask_bits,
        overlay_route_mode: route_mode,
        overlay_route_region: overlay_region,
        overlay_route_relay_bucket: route_relay_bucket,
        overlay_route_relay_set_size: route_relay_set_size,
        overlay_route_relay_round: route_relay_round,
        overlay_route_relay_index: route_relay_index,
        overlay_route_relay_id: route_relay_id,
        overlay_route_strategy: route_strategy,
        overlay_route_hop_count: route_hop_count,
    }
}

fn min_replacement_gas_price(current: u64, price_bump_pct: u64) -> u64 {
    let bump_num = 100u128.saturating_add(price_bump_pct as u128);
    let current = current as u128;
    let required = current
        .saturating_mul(bump_num)
        .saturating_add(99)
        .saturating_div(100);
    required.min(u64::MAX as u128) as u64
}

fn resolve_evm_runtime_config() -> &'static EvmRuntimeConfig {
    EVM_RUNTIME_CONFIG.get_or_init(|| {
        let ingress_queue_max = parse_env_usize(
            "NOVOVM_ADAPTER_PLUGIN_EVM_INGRESS_QUEUE_MAX",
            DEFAULT_INGRESS_QUEUE_MAX,
        )
        .max(1);
        let atomic_receipt_queue_max = parse_env_usize(
            "NOVOVM_ADAPTER_PLUGIN_EVM_ATOMIC_RECEIPT_QUEUE_MAX",
            DEFAULT_ATOMIC_RECEIPT_QUEUE_MAX,
        )
        .max(1);
        let settlement_record_queue_max = parse_env_usize(
            "NOVOVM_ADAPTER_PLUGIN_EVM_SETTLEMENT_RECORD_QUEUE_MAX",
            DEFAULT_SETTLEMENT_RECORD_QUEUE_MAX,
        )
        .max(1);
        let payout_instruction_queue_max = parse_env_usize(
            "NOVOVM_ADAPTER_PLUGIN_EVM_PAYOUT_INSTRUCTION_QUEUE_MAX",
            DEFAULT_PAYOUT_INSTRUCTION_QUEUE_MAX,
        )
        .max(1);
        let atomic_broadcast_queue_max = parse_env_usize(
            "NOVOVM_ADAPTER_PLUGIN_EVM_ATOMIC_BROADCAST_QUEUE_MAX",
            DEFAULT_ATOMIC_BROADCAST_QUEUE_MAX,
        )
        .max(1);
        let execution_receipt_queue_max = parse_env_usize(
            "NOVOVM_ADAPTER_PLUGIN_EVM_EXECUTION_RECEIPT_QUEUE_MAX",
            DEFAULT_EXECUTION_RECEIPT_QUEUE_MAX,
        )
        .max(1);
        let state_mirror_update_queue_max = parse_env_usize(
            "NOVOVM_ADAPTER_PLUGIN_EVM_STATE_MIRROR_UPDATE_QUEUE_MAX",
            DEFAULT_STATE_MIRROR_UPDATE_QUEUE_MAX,
        )
        .max(1);
        let convert_num = parse_env_u128(
            "NOVOVM_ADAPTER_PLUGIN_EVM_FEE_CONVERT_NUM",
            DEFAULT_SETTLEMENT_CONVERT_NUM,
        );
        let convert_den = parse_env_u128(
            "NOVOVM_ADAPTER_PLUGIN_EVM_FEE_CONVERT_DEN",
            DEFAULT_SETTLEMENT_CONVERT_DEN,
        )
        .max(1);
        let txpool_price_bump_pct = parse_env_u64(
            "NOVOVM_ADAPTER_PLUGIN_EVM_TXPOOL_PRICE_BUMP_PCT",
            DEFAULT_TXPOOL_PRICE_BUMP_PCT,
        );
        let txpool_max_pending_per_sender = parse_env_usize(
            "NOVOVM_ADAPTER_PLUGIN_EVM_TXPOOL_MAX_PENDING_PER_SENDER",
            DEFAULT_TXPOOL_MAX_PENDING_PER_SENDER,
        )
        .max(1);
        let txpool_max_nonce_gap = parse_env_u64(
            "NOVOVM_ADAPTER_PLUGIN_EVM_TXPOOL_MAX_NONCE_GAP",
            DEFAULT_TXPOOL_MAX_NONCE_GAP,
        );
        let txpool_executable_queue_max = parse_env_usize(
            "NOVOVM_ADAPTER_PLUGIN_EVM_TXPOOL_EXECUTABLE_QUEUE_MAX",
            DEFAULT_TXPOOL_EXECUTABLE_QUEUE_MAX,
        )
        .max(1);
        EvmRuntimeConfig {
            ingress_queue_max,
            atomic_receipt_queue_max,
            settlement_record_queue_max,
            payout_instruction_queue_max,
            atomic_broadcast_queue_max,
            execution_receipt_queue_max,
            state_mirror_update_queue_max,
            convert_num,
            convert_den,
            txpool_price_bump_pct,
            txpool_max_pending_per_sender,
            txpool_max_nonce_gap,
            txpool_executable_queue_max,
        }
    })
}

fn default_settlement_policy_from_env() -> EvmFeeSettlementPolicyV1 {
    let mut policy = EvmFeeSettlementPolicyV1::default();
    if let Ok(raw) = std::env::var("NOVOVM_ADAPTER_PLUGIN_EVM_RESERVE_CCY") {
        let trimmed = raw.trim();
        if !trimmed.is_empty() {
            policy.reserve_currency_code = trimmed.to_string();
        }
    }
    if let Ok(raw) = std::env::var("NOVOVM_ADAPTER_PLUGIN_EVM_PAYOUT_TOKEN") {
        let trimmed = raw.trim();
        if !trimmed.is_empty() {
            policy.payout_token_code = trimmed.to_string();
        }
    }
    if let Ok(raw) = std::env::var("NOVOVM_ADAPTER_PLUGIN_EVM_RESERVE_ACCOUNT_HEX") {
        if let Some(addr) = decode_hex_bytes(&raw) {
            policy.reserve_account = addr;
        }
    }
    if let Ok(raw) = std::env::var("NOVOVM_ADAPTER_PLUGIN_EVM_PAYOUT_ACCOUNT_HEX") {
        if let Some(addr) = decode_hex_bytes(&raw) {
            policy.payout_account = addr;
        }
    }
    policy
}

fn resolve_evm_runtime_shards() -> &'static [Mutex<EvmRuntimeState>] {
    EVM_RUNTIME_SHARDS
        .get_or_init(|| {
            let shard_count = parse_env_usize(
                "NOVOVM_ADAPTER_PLUGIN_EVM_RUNTIME_SHARDS",
                DEFAULT_EVM_RUNTIME_SHARD_COUNT,
            )
            .clamp(1, MAX_EVM_RUNTIME_SHARD_COUNT);
            let policy = default_settlement_policy_from_env();
            (0..shard_count)
                .map(|_| {
                    let state = EvmRuntimeState {
                        settlement_policy: policy.clone(),
                        ..EvmRuntimeState::default()
                    };
                    Mutex::new(state)
                })
                .collect()
        })
        .as_slice()
}

fn runtime_shard_start_index(shard_count: usize) -> usize {
    if shard_count <= 1 {
        0
    } else {
        EVM_RUNTIME_SHARD_CURSOR.fetch_add(1, Ordering::Relaxed) % shard_count
    }
}

fn resolve_evm_runtime_state_for_chain(chain_id: u64) -> &'static Mutex<EvmRuntimeState> {
    let shards = resolve_evm_runtime_shards();
    &shards[txpool_shard_index(chain_id, shards.len())]
}

fn resolve_evm_txpool_shards() -> &'static [Mutex<EvmTxpoolState>] {
    EVM_TXPOOL_SHARDS
        .get_or_init(|| {
            let shard_count = parse_env_usize(
                "NOVOVM_ADAPTER_PLUGIN_EVM_TXPOOL_SHARDS",
                DEFAULT_EVM_TXPOOL_SHARD_COUNT,
            )
            .clamp(1, MAX_EVM_TXPOOL_SHARD_COUNT);
            (0..shard_count)
                .map(|_| Mutex::new(EvmTxpoolState::default()))
                .collect()
        })
        .as_slice()
}

fn txpool_shard_index(chain_id: u64, shard_count: usize) -> usize {
    let mixed = chain_id ^ (chain_id >> 32);
    (mixed as usize) % shard_count.max(1)
}

fn txpool_shard_index_for_sender(chain_id: u64, sender: &[u8], shard_count: usize) -> usize {
    if sender.is_empty() {
        return txpool_shard_index(chain_id, shard_count);
    }
    let mut mixed = chain_id ^ (chain_id >> 32);
    for byte in sender {
        mixed = mixed
            .wrapping_mul(1099511628211u64)
            .wrapping_add((*byte as u64).saturating_add(1));
    }
    (mixed as usize) % shard_count.max(1)
}

fn txpool_shard_index_for_tx(chain_id: u64, tx: &TxIR, shard_count: usize) -> usize {
    txpool_shard_index_for_sender(chain_id, tx.from.as_slice(), shard_count)
}

fn txpool_shard_start_index(shard_count: usize) -> usize {
    if shard_count <= 1 {
        0
    } else {
        EVM_TXPOOL_SHARD_CURSOR.fetch_add(1, Ordering::Relaxed) % shard_count
    }
}

fn infer_chain_type_from_chain_id(chain_id: u64) -> ChainType {
    resolve_evm_chain_type_from_chain_id(chain_id)
}

fn ensure_tx_hash(mut tx: TxIR) -> TxIR {
    if tx.hash.is_empty() {
        tx.compute_hash();
    }
    tx
}

fn tx_hash_or_compute(tx: &TxIR) -> Vec<u8> {
    if tx.hash.is_empty() {
        let mut cloned = tx.clone();
        cloned.compute_hash();
        cloned.hash
    } else {
        tx.hash.clone()
    }
}

fn tx_hash32_or_compute(tx: &TxIR) -> [u8; 32] {
    normalize_root32(tx_hash_or_compute(tx).as_slice())
}

fn derive_contract_address_for_receipt(from: &[u8], nonce: u64) -> Vec<u8> {
    let mut hasher = Sha256::new();
    hasher.update(b"novovm_contract_address_v1");
    hasher.update(from);
    hasher.update(nonce.to_le_bytes());
    let digest: [u8; 32] = hasher.finalize().into();
    digest[12..32].to_vec()
}

fn aoem_mainline_enabled() -> bool {
    parse_env_bool("NOVOVM_ADAPTER_PLUGIN_AOEM_ENABLED", true)
}

fn aoem_mainline_required() -> bool {
    parse_env_bool("NOVOVM_ADAPTER_PLUGIN_AOEM_REQUIRED", false)
}

fn tx_type_code(tx_type: &TxType) -> u8 {
    match tx_type {
        TxType::Transfer => 1,
        TxType::ContractCall => 2,
        TxType::ContractDeploy => 3,
        TxType::Privacy => 4,
        TxType::CrossShard => 5,
        TxType::CrossChainTransfer => 6,
        TxType::CrossChainCall => 7,
    }
}

fn build_aoem_mainline_value_digest(chain_type: ChainType, chain_id: u64, tx: &TxIR) -> [u8; 32] {
    let mut hasher = Sha256::new();
    hasher.update(b"supervm-evm-aoem-mainline-op-v1");
    hasher.update(chain_type.as_str().as_bytes());
    hasher.update(chain_id.to_le_bytes());
    hasher.update(tx.from.as_slice());
    hasher.update(tx.to.as_deref().unwrap_or(&[]));
    hasher.update(tx.value.to_le_bytes());
    hasher.update(tx.gas_limit.to_le_bytes());
    hasher.update(tx.gas_price.to_le_bytes());
    hasher.update(tx.nonce.to_le_bytes());
    hasher.update([tx_type_code(&tx.tx_type)]);
    hasher.update(tx.data.as_slice());
    if let Some(source_chain) = tx.source_chain {
        hasher.update(source_chain.to_le_bytes());
    }
    if let Some(target_chain) = tx.target_chain {
        hasher.update(target_chain.to_le_bytes());
    }
    hasher.finalize().into()
}

fn build_aoem_plan_id(tx: &TxIR, tx_index: usize) -> u64 {
    let mut hasher = Sha256::new();
    hasher.update(b"supervm-evm-aoem-plan-id-v1");
    hasher.update(tx_hash32_or_compute(tx));
    hasher.update((tx_index as u64).to_le_bytes());
    let digest: [u8; 32] = hasher.finalize().into();
    let mut out = [0u8; 8];
    out.copy_from_slice(&digest[..8]);
    u64::from_le_bytes(out)
}

fn build_aoem_mainline_ops_batch(
    chain_type: ChainType,
    chain_id: u64,
    txs: &[TxIR],
    verify_results: &[bool],
) -> AoemMainlineOpsBatch {
    let op_count = verify_results.iter().filter(|ok| **ok).count();
    let mut keys = vec![[0u8; 32]; op_count];
    let mut values = vec![[0u8; 32]; op_count];
    let mut ops = Vec::with_capacity(op_count);
    let mut projected_txs = Vec::with_capacity(op_count);
    let mut op_index = 0usize;
    for (tx_index, (tx, tx_ok)) in txs.iter().zip(verify_results.iter().copied()).enumerate() {
        if !tx_ok {
            continue;
        }
        keys[op_index] = tx_hash32_or_compute(tx);
        values[op_index] = build_aoem_mainline_value_digest(chain_type, chain_id, tx);
        ops.push(ExecOpV2 {
            opcode: 2,
            flags: 0,
            reserved: 0,
            key_ptr: keys[op_index].as_ptr(),
            key_len: keys[op_index].len() as u32,
            value_ptr: values[op_index].as_ptr(),
            value_len: values[op_index].len() as u32,
            delta: 0,
            expect_version: u64::MAX,
            plan_id: build_aoem_plan_id(tx, tx_index),
        });
        projected_txs.push(AoemProjectedTxExecutionV1 {
            tx_index: tx_index as u32,
            op_index: Some(op_index as u32),
            tx_hash: tx_hash_or_compute(tx),
            gas_limit: tx.gas_limit,
            contract_address: if tx.tx_type == TxType::ContractDeploy {
                Some(derive_contract_address_for_receipt(&tx.from, tx.nonce))
            } else {
                None
            },
            log_emitter: tx.to.clone().or_else(|| Some(tx.from.clone())),
            event_logs: Vec::new(),
            receipt_type: None,
            effective_gas_price: Some(tx.gas_price),
            runtime_code: None,
            runtime_code_hash: None,
            revert_data: None,
        });
        op_index = op_index.saturating_add(1);
    }
    AoemMainlineOpsBatch {
        _keys: keys,
        _values: values,
        ops,
        projected_txs,
    }
}

fn aoem_batch_state_root(
    chain_type: ChainType,
    chain_id: u64,
    adapter_state_root: [u8; 32],
    txs: &[TxIR],
    output: &AoemExecOutput,
) -> [u8; 32] {
    let mut hasher = Sha256::new();
    hasher.update(b"supervm-evm-mainline-state-root-v1");
    hasher.update(chain_type.as_str().as_bytes());
    hasher.update(chain_id.to_le_bytes());
    hasher.update(adapter_state_root);
    hasher.update(output.result.processed.to_le_bytes());
    hasher.update(output.result.success.to_le_bytes());
    hasher.update(output.result.failed_index.to_le_bytes());
    hasher.update(output.result.total_writes.to_le_bytes());
    hasher.update(output.metrics.elapsed_us.to_le_bytes());
    hasher.update(output.metrics.return_code.to_le_bytes());
    for tx in txs {
        hasher.update(tx_hash32_or_compute(tx));
    }
    hasher.finalize().into()
}

fn try_execute_mainline_batch_via_aoem(
    chain_type: ChainType,
    chain_id: u64,
    txs: &[TxIR],
    verify_results: &[bool],
    adapter_state_root: [u8; 32],
    _state_version: u64,
) -> anyhow::Result<Option<AoemBatchExecutionArtifactsV1>> {
    if !aoem_mainline_enabled() {
        return Ok(None);
    }
    let ops_batch = build_aoem_mainline_ops_batch(chain_type, chain_id, txs, verify_results);
    if ops_batch.ops.is_empty() {
        return Ok(None);
    }
    let runtime = AoemRuntimeConfig::from_env()?;
    if !runtime.dll_path.exists() {
        return Ok(None);
    }
    let facade = AoemExecFacade::open_with_runtime(&runtime)?;
    let capability_contract = facade.capability_contract()?;
    if !capability_contract.execute_ops_v2 {
        return Ok(None);
    }
    let session = facade.create_session()?;
    let output = session.submit_ops(ops_batch.ops.as_slice())?;
    let state_root = aoem_batch_state_root(chain_type, chain_id, adapter_state_root, txs, &output);
    Ok(Some(project_tx_execution_artifacts_v1(
        txs.len(),
        ops_batch.projected_txs.as_slice(),
        state_root,
        &output,
    )))
}

fn parallel_worker_count(item_count: usize) -> usize {
    std::thread::available_parallelism()
        .map(|v| v.get())
        .unwrap_or(1)
        .min(item_count.max(1))
}

fn prepare_txs_with_hashes(txs: &[TxIR]) -> Vec<TxIR> {
    if txs.is_empty() {
        return Vec::new();
    }
    let workers = parallel_worker_count(txs.len());
    // Avoid thread fan-out for tiny batches; keep hot path deterministic.
    if workers <= 1 || txs.len() < 32 {
        return txs.iter().cloned().map(ensure_tx_hash).collect();
    }

    let chunk_size = txs.len().div_ceil(workers);
    let mut chunked: Vec<Vec<TxIR>> = Vec::with_capacity(workers);
    let mut panicked = false;
    std::thread::scope(|scope| {
        let mut jobs = Vec::with_capacity(workers);
        for chunk in txs.chunks(chunk_size) {
            jobs.push(scope.spawn(move || -> Vec<TxIR> {
                chunk.iter().cloned().map(ensure_tx_hash).collect()
            }));
        }
        for job in jobs {
            match job.join() {
                Ok(v) => chunked.push(v),
                Err(_) => {
                    panicked = true;
                }
            }
        }
    });
    if panicked {
        return txs.iter().cloned().map(ensure_tx_hash).collect();
    }

    let mut out = Vec::with_capacity(txs.len());
    for chunk in chunked {
        out.extend(chunk);
    }
    out
}

fn nonce_key_from_tx(chain_id: u64, tx: &TxIR) -> Option<IngressNonceKey> {
    if tx.from.is_empty() {
        return None;
    }
    Some(IngressNonceKey {
        chain_id,
        from: tx.from.clone(),
        nonce: tx.nonce,
    })
}

fn sender_key_from_tx(chain_id: u64, tx: &TxIR) -> Option<IngressSenderKey> {
    if tx.from.is_empty() {
        return None;
    }
    Some(IngressSenderKey {
        chain_id,
        from: tx.from.clone(),
    })
}

fn nonce_key_from_frame(frame: &EvmMempoolIngressFrameV1) -> Option<IngressNonceKey> {
    let tx = frame.parsed_tx.as_ref()?;
    nonce_key_from_tx(frame.chain_id, tx)
}

fn sender_key_from_nonce_key(key: &IngressNonceKey) -> IngressSenderKey {
    IngressSenderKey {
        chain_id: key.chain_id,
        from: key.from.clone(),
    }
}

fn remove_nonce_index_for_frame(runtime: &mut EvmTxpoolState, frame: &EvmMempoolIngressFrameV1) {
    let Some(key) = nonce_key_from_frame(frame) else {
        return;
    };
    let should_remove = runtime
        .pending_by_nonce
        .get(&key)
        .map(|meta| meta.tx_hash == frame.tx_hash)
        .unwrap_or(false);
    if should_remove {
        let _ = runtime.pending_by_nonce.remove(&key);
    }

    let sender_key = sender_key_from_nonce_key(&key);
    let mut sender_map_empty = false;
    if let Some(sender_map) = runtime.pending_by_sender.get_mut(&sender_key) {
        let remove_sender_nonce = sender_map
            .get(&key.nonce)
            .map(|hash| *hash == frame.tx_hash)
            .unwrap_or(false);
        if remove_sender_nonce {
            let _ = sender_map.remove(&key.nonce);
        }
        sender_map_empty = sender_map.is_empty();
    }
    if sender_map_empty {
        let _ = runtime.pending_by_sender.remove(&sender_key);
        let _ = runtime.next_nonce_by_sender.remove(&sender_key);
    }
}

fn remove_ingress_frame_by_hash(
    runtime: &mut EvmTxpoolState,
    chain_id: u64,
    tx_hash: &[u8],
) -> Option<EvmMempoolIngressFrameV1> {
    let pos = runtime
        .ingress_frames
        .iter()
        .position(|frame| frame.chain_id == chain_id && frame.tx_hash.as_slice() == tx_hash)?;
    runtime.ingress_frames.remove(pos)
}

fn remove_executable_ingress_frame_by_hash(
    runtime: &mut EvmTxpoolState,
    chain_id: u64,
    tx_hash: &[u8],
) -> Option<EvmMempoolIngressFrameV1> {
    let pos = runtime
        .executable_ingress_frames
        .iter()
        .position(|frame| frame.chain_id == chain_id && frame.tx_hash.as_slice() == tx_hash)?;
    runtime.executable_ingress_frames.remove(pos)
}

fn tx_present_in_runtime(runtime: &EvmTxpoolState, chain_id: u64, tx_hash: &[u8]) -> bool {
    runtime
        .executable_ingress_frames
        .iter()
        .any(|frame| frame.chain_id == chain_id && frame.tx_hash.as_slice() == tx_hash)
        || runtime
            .ingress_frames
            .iter()
            .any(|frame| frame.chain_id == chain_id && frame.tx_hash.as_slice() == tx_hash)
}

fn tx_matches_runtime_frame(frame: &EvmMempoolIngressFrameV1, chain_id: u64, tx: &TxIR) -> bool {
    if frame.chain_id != chain_id {
        return false;
    }
    let Some(parsed) = frame.parsed_tx.as_ref() else {
        return false;
    };
    parsed.hash == tx.hash
        && parsed.from == tx.from
        && parsed.to == tx.to
        && parsed.value == tx.value
        && parsed.gas_limit == tx.gas_limit
        && parsed.gas_price == tx.gas_price
        && parsed.nonce == tx.nonce
        && parsed.data == tx.data
        && parsed.signature == tx.signature
        && parsed.chain_id == tx.chain_id
        && parsed.tx_type == tx.tx_type
        && parsed.source_chain == tx.source_chain
        && parsed.target_chain == tx.target_chain
}

fn tx_present_exact_in_runtime(runtime: &EvmTxpoolState, chain_id: u64, tx: &TxIR) -> bool {
    runtime
        .executable_ingress_frames
        .iter()
        .any(|frame| tx_matches_runtime_frame(frame, chain_id, tx))
        || runtime
            .ingress_frames
            .iter()
            .any(|frame| tx_matches_runtime_frame(frame, chain_id, tx))
}

fn derive_primary_reject_reason(
    summary: &EvmRuntimeTapSummaryV1,
) -> Option<EvmTxpoolRejectReasonV1> {
    if summary.dropped_underpriced > 0 {
        return Some(EvmTxpoolRejectReasonV1::ReplacementUnderpriced);
    }
    if summary.dropped_nonce_too_low > 0 {
        return Some(EvmTxpoolRejectReasonV1::NonceTooLow);
    }
    if summary.dropped_nonce_gap > 0 {
        return Some(EvmTxpoolRejectReasonV1::NonceTooHigh);
    }
    if summary.dropped_over_capacity > 0 {
        return Some(EvmTxpoolRejectReasonV1::PoolFull);
    }
    if summary.dropped > 0 {
        return Some(EvmTxpoolRejectReasonV1::Rejected);
    }
    None
}

fn derive_reject_reasons(summary: &EvmRuntimeTapSummaryV1) -> Vec<EvmTxpoolRejectReasonV1> {
    let mut out = Vec::with_capacity(4);
    if summary.dropped_underpriced > 0 {
        out.push(EvmTxpoolRejectReasonV1::ReplacementUnderpriced);
    }
    if summary.dropped_nonce_too_low > 0 {
        out.push(EvmTxpoolRejectReasonV1::NonceTooLow);
    }
    if summary.dropped_nonce_gap > 0 {
        out.push(EvmTxpoolRejectReasonV1::NonceTooHigh);
    }
    if summary.dropped_over_capacity > 0 {
        out.push(EvmTxpoolRejectReasonV1::PoolFull);
    }
    if out.is_empty() && summary.dropped > 0 {
        out.push(EvmTxpoolRejectReasonV1::Rejected);
    }
    out
}

fn pop_next_pending_frame_for_sender(
    runtime: &mut EvmTxpoolState,
    sender_key: &IngressSenderKey,
) -> Option<EvmMempoolIngressFrameV1> {
    let tx_hash = runtime
        .pending_by_sender
        .get(sender_key)
        .and_then(|pending| pending.first_key_value().map(|(_, hash)| hash.clone()))?;
    if let Some(frame) = remove_ingress_frame_by_hash(runtime, sender_key.chain_id, &tx_hash) {
        remove_nonce_index_for_frame(runtime, &frame);
        return Some(frame);
    }
    let remove_sender = if let Some(sender_map) = runtime.pending_by_sender.get_mut(sender_key) {
        if let Some((&nonce, _)) = sender_map.first_key_value() {
            let _ = sender_map.remove(&nonce);
        }
        sender_map.is_empty()
    } else {
        false
    };
    if remove_sender {
        let _ = runtime.pending_by_sender.remove(sender_key);
        let _ = runtime.next_nonce_by_sender.remove(sender_key);
    }
    None
}

fn drain_pending_frames_round_robin_across_shards(
    max_items: usize,
) -> Vec<EvmMempoolIngressFrameV1> {
    if max_items == 0 {
        return Vec::new();
    }
    let shards = resolve_evm_txpool_shards();
    if shards.is_empty() {
        return Vec::new();
    }
    let mut sender_keys: Vec<IngressSenderKey> = Vec::new();
    for shard in shards {
        let runtime = match shard.lock() {
            Ok(v) => v,
            Err(_) => continue,
        };
        sender_keys.extend(runtime.pending_by_sender.keys().cloned());
    }
    if sender_keys.is_empty() {
        return Vec::new();
    }
    sender_keys.sort_by(|left, right| match left.chain_id.cmp(&right.chain_id) {
        std::cmp::Ordering::Equal => left.from.cmp(&right.from),
        ord => ord,
    });
    sender_keys.dedup();

    let mut out: Vec<EvmMempoolIngressFrameV1> = Vec::with_capacity(max_items);
    let mut cursor = 0usize;
    while out.len() < max_items && !sender_keys.is_empty() {
        if cursor >= sender_keys.len() {
            cursor = 0;
        }
        let sender_key = sender_keys[cursor].clone();
        let shard_index = txpool_shard_index_for_sender(
            sender_key.chain_id,
            sender_key.from.as_slice(),
            shards.len(),
        );
        let mut runtime = match shards[shard_index].lock() {
            Ok(v) => v,
            Err(_) => {
                sender_keys.remove(cursor);
                continue;
            }
        };
        if let Some(frame) = pop_next_pending_frame_for_sender(&mut runtime, &sender_key) {
            out.push(frame);
        }
        let has_more = runtime
            .pending_by_sender
            .get(&sender_key)
            .map(|sender_map| !sender_map.is_empty())
            .unwrap_or(false);
        if has_more {
            cursor = cursor.saturating_add(1);
        } else {
            sender_keys.remove(cursor);
        }
    }
    out
}

fn find_executable_frame_pos_by_sender_nonce(
    runtime: &EvmTxpoolState,
    chain_id: u64,
    from: &[u8],
    nonce: u64,
) -> Option<usize> {
    runtime.executable_ingress_frames.iter().position(|frame| {
        if frame.chain_id != chain_id {
            return false;
        }
        frame
            .parsed_tx
            .as_ref()
            .map(|tx| tx.nonce == nonce && tx.from.as_slice() == from)
            .unwrap_or(false)
    })
}

fn sender_nonce_gap_exceeded(
    runtime: &EvmTxpoolState,
    sender_key: &IngressSenderKey,
    nonce: u64,
    max_gap: u64,
) -> bool {
    if let Some(next_nonce) = sender_next_nonce_hint(runtime, sender_key) {
        return nonce > next_nonce.saturating_add(max_gap);
    }
    let Some(sender_map) = runtime.pending_by_sender.get(sender_key) else {
        return false;
    };
    let Some((&min_nonce, _)) = sender_map.first_key_value() else {
        return false;
    };
    nonce > min_nonce.saturating_add(max_gap)
}

fn sender_next_nonce_hint(runtime: &EvmTxpoolState, sender_key: &IngressSenderKey) -> Option<u64> {
    runtime.next_nonce_by_sender.get(sender_key).copied()
}

fn resolve_sender_expected_nonce(
    runtime: &EvmTxpoolState,
    sender_key: &IngressSenderKey,
) -> Option<u64> {
    if let Some(next) = sender_next_nonce_hint(runtime, sender_key) {
        return Some(next);
    }
    runtime
        .pending_by_sender
        .get(sender_key)
        .and_then(|m| m.first_key_value().map(|(&nonce, _)| nonce))
}

fn enforce_sender_pending_cap(
    runtime: &mut EvmTxpoolState,
    sender_key: &IngressSenderKey,
    max_pending_per_sender: usize,
) {
    loop {
        let should_evict = runtime
            .pending_by_sender
            .get(sender_key)
            .map(|m| m.len() > max_pending_per_sender)
            .unwrap_or(false);
        if !should_evict {
            break;
        }
        let evicted = {
            let sender_map = match runtime.pending_by_sender.get_mut(sender_key) {
                Some(v) => v,
                None => break,
            };
            let Some((&evict_nonce, evict_hash)) = sender_map.last_key_value() else {
                break;
            };
            let evict_hash = evict_hash.clone();
            let _ = sender_map.remove(&evict_nonce);
            (evict_nonce, evict_hash, sender_map.is_empty())
        };
        let (evict_nonce, evict_hash, sender_map_empty) = evicted;
        let nonce_key = IngressNonceKey {
            chain_id: sender_key.chain_id,
            from: sender_key.from.clone(),
            nonce: evict_nonce,
        };
        let should_remove_nonce = runtime
            .pending_by_nonce
            .get(&nonce_key)
            .map(|meta| meta.tx_hash == evict_hash)
            .unwrap_or(false);
        if should_remove_nonce {
            let _ = runtime.pending_by_nonce.remove(&nonce_key);
        }
        if let Some(removed) =
            remove_ingress_frame_by_hash(runtime, sender_key.chain_id, evict_hash.as_slice())
        {
            remove_nonce_index_for_frame(runtime, &removed);
        }
        if sender_map_empty {
            let _ = runtime.pending_by_sender.remove(sender_key);
            let _ = runtime.next_nonce_by_sender.remove(sender_key);
            break;
        }
    }
}

fn promote_sender_executable_frames(
    runtime: &mut EvmTxpoolState,
    config: &EvmRuntimeConfig,
    sender_key: &IngressSenderKey,
) {
    let Some(mut expected_nonce) = resolve_sender_expected_nonce(runtime, sender_key) else {
        return;
    };
    loop {
        let maybe_hash = runtime
            .pending_by_sender
            .get(sender_key)
            .and_then(|m| m.get(&expected_nonce).cloned());
        let Some(tx_hash) = maybe_hash else {
            break;
        };
        let sender_map_empty =
            if let Some(sender_map) = runtime.pending_by_sender.get_mut(sender_key) {
                let _ = sender_map.remove(&expected_nonce);
                sender_map.is_empty()
            } else {
                false
            };
        if sender_map_empty {
            let _ = runtime.pending_by_sender.remove(sender_key);
        }
        let nonce_key = IngressNonceKey {
            chain_id: sender_key.chain_id,
            from: sender_key.from.clone(),
            nonce: expected_nonce,
        };
        let _ = runtime.pending_by_nonce.remove(&nonce_key);
        if let Some(frame) =
            remove_ingress_frame_by_hash(runtime, sender_key.chain_id, tx_hash.as_slice())
        {
            runtime.executable_ingress_frames.push_back(frame);
        }
        while runtime.executable_ingress_frames.len() > config.txpool_executable_queue_max {
            let _ = runtime.executable_ingress_frames.pop_front();
        }
        expected_nonce = expected_nonce.saturating_add(1);
    }
    runtime
        .next_nonce_by_sender
        .insert(sender_key.clone(), expected_nonce);
}

fn push_ingress_frames_prepared(chain_id: u64, prepared_txs: &[TxIR]) -> EvmIngressPushOutcome {
    if prepared_txs.is_empty() {
        return EvmIngressPushOutcome::default();
    }
    let mut outcome = EvmIngressPushOutcome {
        summary: EvmRuntimeTapSummaryV1 {
            requested: prepared_txs.len(),
            ..EvmRuntimeTapSummaryV1::default()
        },
        accepted_txs: Vec::with_capacity(prepared_txs.len()),
    };
    let config = resolve_evm_runtime_config();
    let shards = resolve_evm_txpool_shards();
    let shard_count = shards.len().max(1);
    let observed_at = now_unix_ms();
    for tx in prepared_txs.iter().cloned() {
        let shard_index = txpool_shard_index_for_tx(chain_id, &tx, shard_count);
        let runtime = &shards[shard_index];
        let mut runtime = match runtime.lock() {
            Ok(v) => v,
            Err(_) => {
                outcome.summary.dropped = outcome.summary.dropped.saturating_add(1);
                outcome.summary.dropped_over_capacity =
                    outcome.summary.dropped_over_capacity.saturating_add(1);
                continue;
            }
        };
        let tx_hash = tx.hash.clone();
        if tx_present_exact_in_runtime(&runtime, chain_id, &tx) {
            outcome.summary.accepted = outcome.summary.accepted.saturating_add(1);
            outcome.accepted_txs.push(tx);
            continue;
        }
        let sender_key = sender_key_from_tx(chain_id, &tx);
        if let Some(sender_key) = sender_key.as_ref() {
            if let Some(next_nonce) = sender_next_nonce_hint(&runtime, sender_key) {
                if tx.nonce < next_nonce {
                    if tx.nonce.saturating_add(1) == next_nonce {
                        if let Some(pos) = find_executable_frame_pos_by_sender_nonce(
                            &runtime,
                            chain_id,
                            tx.from.as_slice(),
                            tx.nonce,
                        ) {
                            let existing_gas_price = runtime.executable_ingress_frames[pos]
                                .parsed_tx
                                .as_ref()
                                .map(|v| v.gas_price)
                                .unwrap_or_default();
                            let required = min_replacement_gas_price(
                                existing_gas_price,
                                config.txpool_price_bump_pct,
                            );
                            if tx.gas_price >= required {
                                let raw_tx =
                                    crate::bincode_compat::serialize(&tx).unwrap_or_default();
                                runtime.executable_ingress_frames[pos] = EvmMempoolIngressFrameV1 {
                                    chain_id,
                                    tx_hash: tx.hash.clone(),
                                    raw_tx,
                                    parsed_tx: Some(tx.clone()),
                                    observed_at_unix_ms: observed_at,
                                    overlay_route_id: String::new(),
                                    overlay_route_epoch: 0,
                                    overlay_route_mask_bits: 0,
                                    overlay_route_mode: "fast".to_string(),
                                    overlay_route_region: "global".to_string(),
                                    overlay_route_relay_bucket: 0,
                                    overlay_route_relay_set_size: 1,
                                    overlay_route_relay_round: 0,
                                    overlay_route_relay_index: 0,
                                    overlay_route_relay_id: "rly:global:0:0".to_string(),
                                    overlay_route_strategy: "direct".to_string(),
                                    overlay_route_hop_count: 1,
                                };
                                outcome.summary.replaced_executable =
                                    outcome.summary.replaced_executable.saturating_add(1);
                                if tx_present_in_runtime(&runtime, chain_id, &tx_hash) {
                                    outcome.summary.accepted =
                                        outcome.summary.accepted.saturating_add(1);
                                    outcome.accepted_txs.push(tx);
                                } else {
                                    outcome.summary.dropped =
                                        outcome.summary.dropped.saturating_add(1);
                                    outcome.summary.dropped_over_capacity =
                                        outcome.summary.dropped_over_capacity.saturating_add(1);
                                }
                            } else {
                                outcome.summary.dropped = outcome.summary.dropped.saturating_add(1);
                                outcome.summary.dropped_underpriced =
                                    outcome.summary.dropped_underpriced.saturating_add(1);
                            }
                        } else {
                            outcome.summary.dropped = outcome.summary.dropped.saturating_add(1);
                            outcome.summary.dropped_nonce_too_low =
                                outcome.summary.dropped_nonce_too_low.saturating_add(1);
                        }
                    } else {
                        outcome.summary.dropped = outcome.summary.dropped.saturating_add(1);
                        outcome.summary.dropped_nonce_too_low =
                            outcome.summary.dropped_nonce_too_low.saturating_add(1);
                    }
                    continue;
                }
            }
            if sender_nonce_gap_exceeded(
                &runtime,
                sender_key,
                tx.nonce,
                config.txpool_max_nonce_gap,
            ) {
                outcome.summary.dropped = outcome.summary.dropped.saturating_add(1);
                outcome.summary.dropped_nonce_gap =
                    outcome.summary.dropped_nonce_gap.saturating_add(1);
                continue;
            }
        }
        let mut replaced_pending = false;
        if let Some(nonce_key) = nonce_key_from_tx(chain_id, &tx) {
            if let Some(existing) = runtime.pending_by_nonce.get(&nonce_key) {
                let existing_gas_price = existing.gas_price;
                let existing_hash = existing.tx_hash.clone();
                let required =
                    min_replacement_gas_price(existing_gas_price, config.txpool_price_bump_pct);
                if tx.gas_price < required {
                    outcome.summary.dropped = outcome.summary.dropped.saturating_add(1);
                    outcome.summary.dropped_underpriced =
                        outcome.summary.dropped_underpriced.saturating_add(1);
                    continue;
                }
                if let Some(removed) =
                    remove_ingress_frame_by_hash(&mut runtime, chain_id, existing_hash.as_slice())
                {
                    remove_nonce_index_for_frame(&mut runtime, &removed);
                }
                replaced_pending = true;
            }
            runtime.pending_by_nonce.insert(
                nonce_key,
                IngressNonceMeta {
                    tx_hash: tx.hash.clone(),
                    gas_price: tx.gas_price,
                },
            );
        }
        let sender_key_for_cap = sender_key.clone();
        if let Some(sender_key) = sender_key {
            runtime
                .pending_by_sender
                .entry(sender_key)
                .or_default()
                .insert(tx.nonce, tx.hash.clone());
        }
        let raw_tx = crate::bincode_compat::serialize(&tx).unwrap_or_default();
        let frame = EvmMempoolIngressFrameV1 {
            chain_id,
            tx_hash: tx.hash.clone(),
            raw_tx,
            parsed_tx: Some(tx.clone()),
            observed_at_unix_ms: observed_at,
            overlay_route_id: String::new(),
            overlay_route_epoch: 0,
            overlay_route_mask_bits: 0,
            overlay_route_mode: "fast".to_string(),
            overlay_route_region: "global".to_string(),
            overlay_route_relay_bucket: 0,
            overlay_route_relay_set_size: 1,
            overlay_route_relay_round: 0,
            overlay_route_relay_index: 0,
            overlay_route_relay_id: "rly:global:0:0".to_string(),
            overlay_route_strategy: "direct".to_string(),
            overlay_route_hop_count: 1,
        };
        runtime.ingress_frames.push_back(frame);
        if let Some(sender_key) = sender_key_for_cap {
            enforce_sender_pending_cap(
                &mut runtime,
                &sender_key,
                config.txpool_max_pending_per_sender,
            );
            promote_sender_executable_frames(&mut runtime, config, &sender_key);
        }
        if tx_present_in_runtime(&runtime, chain_id, &tx_hash) {
            outcome.summary.accepted = outcome.summary.accepted.saturating_add(1);
            if replaced_pending {
                outcome.summary.replaced_pending =
                    outcome.summary.replaced_pending.saturating_add(1);
            }
            outcome.accepted_txs.push(tx);
        } else {
            outcome.summary.dropped = outcome.summary.dropped.saturating_add(1);
            outcome.summary.dropped_over_capacity =
                outcome.summary.dropped_over_capacity.saturating_add(1);
        }
        while runtime.ingress_frames.len() > config.ingress_queue_max {
            if let Some(removed) = runtime.ingress_frames.pop_front() {
                remove_nonce_index_for_frame(&mut runtime, &removed);
            }
        }
    }
    outcome.summary.reject_reasons = derive_reject_reasons(&outcome.summary);
    outcome.summary.primary_reject_reason = outcome.summary.reject_reasons.first().copied();
    if outcome.summary.primary_reject_reason.is_none() {
        outcome.summary.primary_reject_reason = derive_primary_reject_reason(&outcome.summary);
    }
    outcome
}

#[cfg(test)]
fn push_ingress_frames(chain_id: u64, txs: &[TxIR]) -> EvmIngressPushOutcome {
    let prepared_txs = prepare_txs_with_hashes(txs);
    push_ingress_frames_prepared(chain_id, &prepared_txs)
}

fn settle_fee_income_record(
    runtime: &mut EvmRuntimeState,
    convert_num: u128,
    convert_den: u128,
    income: &EvmFeeIncomeRecordV1,
) -> EvmFeeSettlementResultV1 {
    let settlement_seq = EVM_RUNTIME_SETTLEMENT_SEQ
        .fetch_add(1, Ordering::Relaxed)
        .saturating_add(1);
    runtime.settlement_seq = runtime.settlement_seq.max(settlement_seq);
    let reserve_delta = income.fee_amount_wei;
    let payout_delta = reserve_delta
        .saturating_mul(convert_num)
        .saturating_div(convert_den);
    runtime.reserve_total_wei = runtime.reserve_total_wei.saturating_add(reserve_delta);
    runtime.payout_total_units = runtime.payout_total_units.saturating_add(payout_delta);
    EvmFeeSettlementResultV1 {
        reserve_delta,
        payout_delta,
        settlement_id: format!("evm-settlement-{:020}", settlement_seq),
    }
}

fn build_payout_instruction_from_record(
    policy: &EvmFeeSettlementPolicyV1,
    chain_id: u64,
    record: &EvmFeeSettlementRecordV1,
) -> EvmFeePayoutInstructionV1 {
    EvmFeePayoutInstructionV1 {
        settlement_id: record.result.settlement_id.clone(),
        chain_id,
        income_tx_hash: record.income.tx_hash.clone(),
        reserve_currency_code: policy.reserve_currency_code.clone(),
        payout_token_code: policy.payout_token_code.clone(),
        reserve_delta_wei: record.result.reserve_delta,
        payout_delta_units: record.result.payout_delta,
        reserve_account: policy.reserve_account.clone(),
        payout_account: policy.payout_account.clone(),
        generated_at_unix_ms: record.settled_at_unix_ms,
    }
}

fn settle_fee_income_for_batch(chain_id: u64, txs: &[TxIR]) {
    if txs.is_empty() {
        return;
    }
    let config = resolve_evm_runtime_config();
    let runtime = resolve_evm_runtime_state_for_chain(chain_id);
    let mut runtime = match runtime.lock() {
        Ok(v) => v,
        Err(_) => return,
    };
    for tx in txs {
        let fee_amount_wei = (tx.gas_limit as u128).saturating_mul(tx.gas_price as u128);
        let income = EvmFeeIncomeRecordV1 {
            chain_id,
            tx_hash: tx_hash_or_compute(tx),
            fee_amount_wei,
            collector_address: tx.from.clone(),
        };
        let result = settle_fee_income_record(
            &mut runtime,
            config.convert_num,
            config.convert_den,
            &income,
        );
        let record = EvmFeeSettlementRecordV1 {
            income,
            result,
            settled_at_unix_ms: now_unix_ms(),
        };
        let payout_instruction =
            build_payout_instruction_from_record(&runtime.settlement_policy, chain_id, &record);
        runtime.settlement_records.push_back(record);
        while runtime.settlement_records.len() > config.settlement_record_queue_max {
            let _ = runtime.settlement_records.pop_front();
        }
        runtime.payout_instructions.push_back(payout_instruction);
        while runtime.payout_instructions.len() > config.payout_instruction_queue_max {
            let _ = runtime.payout_instructions.pop_front();
        }
    }
}

fn enqueue_atomic_receipt(chain_id: u64, receipt: AtomicIntentReceiptV1) {
    let config = resolve_evm_runtime_config();
    let runtime = resolve_evm_runtime_state_for_chain(chain_id);
    let mut runtime = match runtime.lock() {
        Ok(v) => v,
        Err(_) => return,
    };
    runtime.atomic_receipts.push_back(receipt);
    while runtime.atomic_receipts.len() > config.atomic_receipt_queue_max {
        let _ = runtime.atomic_receipts.pop_front();
    }
}

fn enqueue_atomic_broadcast_ready(chain_id: u64, item: AtomicBroadcastReadyV1) {
    let config = resolve_evm_runtime_config();
    let runtime = resolve_evm_runtime_state_for_chain(chain_id);
    let mut runtime = match runtime.lock() {
        Ok(v) => v,
        Err(_) => return,
    };
    runtime.atomic_broadcast_ready.push_back(item);
    while runtime.atomic_broadcast_ready.len() > config.atomic_broadcast_queue_max {
        let _ = runtime.atomic_broadcast_ready.pop_front();
    }
}

fn enqueue_execution_receipts(chain_id: u64, receipts: &[SupervmEvmExecutionReceiptV1]) {
    if receipts.is_empty() {
        return;
    }
    let config = resolve_evm_runtime_config();
    let runtime = resolve_evm_runtime_state_for_chain(chain_id);
    let mut runtime = match runtime.lock() {
        Ok(v) => v,
        Err(_) => return,
    };
    for receipt in receipts {
        runtime.execution_receipts.push_back(receipt.clone());
    }
    while runtime.execution_receipts.len() > config.execution_receipt_queue_max {
        let _ = runtime.execution_receipts.pop_front();
    }
}

fn enqueue_state_mirror_update(chain_id: u64, update: SupervmEvmStateMirrorUpdateV1) {
    let config = resolve_evm_runtime_config();
    let runtime = resolve_evm_runtime_state_for_chain(chain_id);
    let mut runtime = match runtime.lock() {
        Ok(v) => v,
        Err(_) => return,
    };
    runtime.state_mirror_updates.push_back(update);
    while runtime.state_mirror_updates.len() > config.state_mirror_update_queue_max {
        let _ = runtime.state_mirror_updates.pop_front();
    }
}

fn build_supervm_logs_from_aoem_event_logs(
    tx_index: usize,
    state_version: u64,
    logs: &[AoemEventLogV1],
) -> Vec<SupervmEvmExecutionLogV1> {
    logs.iter()
        .enumerate()
        .map(|(position, log)| SupervmEvmExecutionLogV1 {
            emitter: log.emitter.clone(),
            topics: log.topics.clone(),
            data: log.data.clone(),
            tx_index: tx_index as u32,
            log_index: log.log_index.max(position as u32),
            state_version,
        })
        .collect()
}

fn default_tx_execution_artifact_for_receipt(
    tx: &TxIR,
    tx_index: usize,
    verified: bool,
    state_root: [u8; 32],
) -> AoemTxExecutionArtifactV1 {
    let status_ok = verified;
    AoemTxExecutionArtifactV1 {
        tx_index: tx_index as u32,
        tx_hash: tx_hash_or_compute(tx),
        status_ok,
        gas_used: if status_ok { tx.gas_limit } else { 0 },
        cumulative_gas_used: 0,
        state_root,
        contract_address: if status_ok && tx.tx_type == TxType::ContractDeploy {
            Some(derive_contract_address_for_receipt(&tx.from, tx.nonce))
        } else {
            None
        },
        receipt_type: None,
        effective_gas_price: Some(tx.gas_price),
        runtime_code: None,
        runtime_code_hash: None,
        event_logs: Vec::new(),
        log_bloom: vec![0u8; AOEM_LOG_BLOOM_BYTES_V1],
        revert_data: None,
        anchor: None,
    }
}

fn materialize_batch_execution_artifacts(
    txs: &[TxIR],
    verify_results: &[bool],
    state_root: [u8; 32],
    resolved_artifacts: Vec<Option<AoemTxExecutionArtifactV1>>,
    aoem_artifacts: Option<AoemBatchExecutionArtifactsV1>,
) -> Option<AoemBatchExecutionArtifactsV1> {
    let has_resolved = resolved_artifacts.iter().any(|artifact| artifact.is_some());
    let mut batch = match aoem_artifacts {
        Some(batch) => batch,
        None if has_resolved => AoemBatchExecutionArtifactsV1 {
            state_root,
            processed_ops: 0,
            success_ops: 0,
            failed_index: None,
            total_writes: 0,
            tx_artifacts: Vec::with_capacity(txs.len()),
        },
        None => return None,
    };

    if batch.tx_artifacts.len() < txs.len() {
        for idx in batch.tx_artifacts.len()..txs.len() {
            batch
                .tx_artifacts
                .push(default_tx_execution_artifact_for_receipt(
                    &txs[idx],
                    idx,
                    verify_results.get(idx).copied().unwrap_or(false),
                    state_root,
                ));
        }
    }

    for (idx, tx) in txs.iter().enumerate() {
        let replacement = resolved_artifacts
            .get(idx)
            .and_then(|artifact| artifact.clone())
            .unwrap_or_else(|| {
                batch.tx_artifacts.get(idx).cloned().unwrap_or_else(|| {
                    default_tx_execution_artifact_for_receipt(
                        tx,
                        idx,
                        verify_results.get(idx).copied().unwrap_or(false),
                        state_root,
                    )
                })
            });
        if idx < batch.tx_artifacts.len() {
            batch.tx_artifacts[idx] = replacement;
        } else {
            batch.tx_artifacts.push(replacement);
        }
    }

    let mut cumulative_gas_used = 0u64;
    let mut first_failure = None;
    let mut success_ops = 0u32;
    for (idx, artifact) in batch.tx_artifacts.iter_mut().enumerate() {
        cumulative_gas_used = cumulative_gas_used.saturating_add(artifact.gas_used);
        artifact.cumulative_gas_used = cumulative_gas_used;
        artifact.state_root = state_root;
        let verified = verify_results.get(idx).copied().unwrap_or(false);
        if verified && artifact.status_ok {
            success_ops = success_ops.saturating_add(1);
        } else if verified && first_failure.is_none() {
            first_failure = Some(artifact.tx_index);
        }
    }
    batch.state_root = state_root;
    batch.processed_ops = verify_results.iter().filter(|ok| **ok).count() as u32;
    batch.success_ops = success_ops;
    batch.failed_index = first_failure;
    Some(batch)
}

fn build_execution_receipts_from_apply_batch(
    chain_type: ChainType,
    chain_id: u64,
    txs: &[TxIR],
    verify_results: &[bool],
    result: &NovovmAdapterPluginApplyResultV1,
    state_version: u64,
    aoem_artifacts: Option<&AoemBatchExecutionArtifactsV1>,
) -> Vec<SupervmEvmExecutionReceiptV1> {
    let mut out: Vec<SupervmEvmExecutionReceiptV1> = Vec::with_capacity(txs.len());
    let final_state_root = aoem_artifacts
        .map(|artifacts| artifacts.state_root)
        .unwrap_or(result.state_root);
    for (idx, tx) in txs.iter().enumerate() {
        let aoem_artifact = aoem_artifacts.and_then(|artifacts| artifacts.tx_artifacts.get(idx));
        let status_ok = aoem_artifact
            .map(|artifact| artifact.status_ok)
            .unwrap_or_else(|| verify_results.get(idx).copied().unwrap_or(false));
        let gas_used = aoem_artifact
            .map(|artifact| artifact.gas_used)
            .unwrap_or_else(|| if status_ok { tx.gas_limit } else { 0 });
        let cumulative_gas_used = aoem_artifact
            .map(|artifact| artifact.cumulative_gas_used)
            .unwrap_or_else(|| {
                out.last()
                    .map(|receipt| receipt.cumulative_gas_used)
                    .unwrap_or(0)
                    .saturating_add(gas_used)
            });
        out.push(SupervmEvmExecutionReceiptV1 {
            chain_type,
            chain_id,
            tx_hash: tx_hash_or_compute(tx),
            tx_index: idx as u32,
            tx_type: tx.tx_type.clone(),
            receipt_type: aoem_artifact.and_then(|artifact| artifact.receipt_type),
            status_ok,
            gas_used,
            cumulative_gas_used,
            effective_gas_price: aoem_artifact
                .and_then(|artifact| artifact.effective_gas_price)
                .or(Some(tx.gas_price)),
            log_bloom: aoem_artifact
                .map(|artifact| artifact.log_bloom.clone())
                .unwrap_or_else(|| vec![0u8; AOEM_LOG_BLOOM_BYTES_V1]),
            revert_data: aoem_artifact.and_then(|artifact| artifact.revert_data.clone()),
            state_root: aoem_artifact
                .map(|artifact| artifact.state_root)
                .unwrap_or(final_state_root),
            state_version,
            contract_address: aoem_artifact
                .and_then(|artifact| artifact.contract_address.clone())
                .or_else(|| {
                    if status_ok && tx.tx_type == TxType::ContractDeploy {
                        Some(derive_contract_address_for_receipt(&tx.from, tx.nonce))
                    } else {
                        None
                    }
                }),
            logs: aoem_artifact
                .map(|artifact| {
                    build_supervm_logs_from_aoem_event_logs(
                        idx,
                        state_version,
                        artifact.event_logs.as_slice(),
                    )
                })
                .unwrap_or_default(),
        });
    }
    out
}

pub fn ingest_execution_receipts_for_host(
    chain_id: u64,
    state_version: u64,
    receipts: &[SupervmEvmExecutionReceiptV1],
) -> anyhow::Result<SupervmEvmStateMirrorUpdateV1> {
    let update = preview_execution_receipts_ingest_for_host(chain_id, state_version, receipts)?;
    enqueue_state_mirror_update(chain_id, update.clone());
    Ok(update)
}

fn preview_execution_receipts_ingest_for_host(
    chain_id: u64,
    state_version: u64,
    receipts: &[SupervmEvmExecutionReceiptV1],
) -> anyhow::Result<SupervmEvmStateMirrorUpdateV1> {
    if receipts.is_empty() {
        anyhow::bail!("execution receipts must not be empty");
    }
    for receipt in receipts {
        if receipt.chain_id != chain_id {
            anyhow::bail!(
                "execution receipt chain mismatch: expected={} actual={}",
                chain_id,
                receipt.chain_id
            );
        }
    }
    let resolved_state_version = state_version.max(1);
    let accepted_receipt_count = receipts.iter().filter(|receipt| receipt.status_ok).count() as u64;
    let update = SupervmEvmStateMirrorUpdateV1 {
        chain_type: receipts[0].chain_type,
        chain_id,
        state_version: resolved_state_version,
        state_root: receipts
            .last()
            .map(|receipt| receipt.state_root)
            .unwrap_or([0u8; 32]),
        receipt_count: receipts.len() as u64,
        accepted_receipt_count,
        tx_hashes: receipts
            .iter()
            .map(|receipt| receipt.tx_hash.clone())
            .collect::<Vec<_>>(),
        imported_at_unix_ms: now_unix_ms(),
    };
    Ok(update)
}

pub fn ingest_execution_receipts_bincode_for_host(
    chain_id: u64,
    state_version: u64,
    payload: &[u8],
) -> anyhow::Result<SupervmEvmStateMirrorUpdateV1> {
    if payload.is_empty() {
        anyhow::bail!("execution receipt payload must not be empty");
    }
    let receipts: Vec<SupervmEvmExecutionReceiptV1> =
        crate::bincode_compat::deserialize(payload)
            .map_err(|e| anyhow::anyhow!("decode execution receipt payload failed: {e}"))?;
    ingest_execution_receipts_for_host(chain_id, state_version, receipts.as_slice())
}

#[cfg(test)]
fn remove_path_if_present(path: &Path) {
    if path.as_os_str().is_empty() || !path.exists() {
        return;
    }
    let result = if path.is_dir() {
        fs::remove_dir_all(path)
    } else {
        fs::remove_file(path)
    };
    let _ = result;
}

fn build_atomic_intent_id(chain_id: u64, txs: &[TxIR]) -> String {
    let mut hasher = Sha256::new();
    hasher.update(b"evm-plugin-atomic-intent-v1");
    hasher.update(chain_id.to_be_bytes());
    for tx in txs {
        let tx_hash = tx_hash_or_compute(tx);
        hasher.update(tx_hash);
        hasher.update(tx.nonce.to_be_bytes());
    }
    let digest = hasher.finalize();
    format!("atomic-{}", to_lower_hex(&digest))
}

fn maybe_build_atomic_intent(
    chain_type: ChainType,
    chain_id: u64,
    txs: &[TxIR],
) -> Option<AtomicCrossChainIntentV1> {
    if !txs
        .iter()
        .any(|tx| match (tx.source_chain, tx.target_chain) {
            (Some(source), Some(target)) => source != target,
            (None, Some(target)) => target != chain_id,
            _ => false,
        })
    {
        return None;
    }

    let destination_chain = txs
        .iter()
        .find_map(|tx| tx.target_chain)
        .map(infer_chain_type_from_chain_id)
        .unwrap_or(chain_type);
    Some(AtomicCrossChainIntentV1 {
        intent_id: build_atomic_intent_id(chain_id, txs),
        source_chain: chain_type,
        destination_chain,
        ttl_unix_ms: now_unix_ms().saturating_add(30_000),
        legs: txs.to_vec(),
    })
}

fn validate_atomic_intent_local(
    chain_id: u64,
    intent: &AtomicCrossChainIntentV1,
) -> anyhow::Result<()> {
    if intent.legs.is_empty() {
        anyhow::bail!("atomic intent must include at least one leg");
    }
    let mut nonce_by_sender: HashMap<Vec<u8>, u64> = HashMap::new();
    for tx in &intent.legs {
        if tx.from.is_empty() {
            anyhow::bail!("atomic intent leg requires non-empty tx.from");
        }
        if let Some(source_chain) = tx.source_chain {
            if source_chain != chain_id {
                anyhow::bail!(
                    "atomic intent leg source_chain mismatch: expected={} actual={}",
                    chain_id,
                    source_chain
                );
            }
        }
        if let Some(target_chain) = tx.target_chain {
            if target_chain == chain_id {
                anyhow::bail!("atomic intent leg target_chain must differ from source chain");
            }
        }
        match nonce_by_sender.get(&tx.from) {
            Some(prev_nonce) if tx.nonce <= *prev_nonce => {
                anyhow::bail!(
                    "atomic intent nonce must be strictly increasing per sender: nonce={} prev={}",
                    tx.nonce,
                    prev_nonce
                );
            }
            _ => {
                nonce_by_sender.insert(tx.from.clone(), tx.nonce);
            }
        }
    }
    Ok(())
}

fn prepare_atomic_intent_guard(
    chain_type: ChainType,
    chain_id: u64,
    txs: &[TxIR],
) -> anyhow::Result<Option<AtomicCrossChainIntentV1>> {
    let Some(intent) = maybe_build_atomic_intent(chain_type, chain_id, txs) else {
        return Ok(None);
    };
    let intent_id = intent.intent_id.clone();
    match validate_atomic_intent_local(chain_id, &intent) {
        Ok(()) => {
            enqueue_atomic_receipt(
                chain_id,
                AtomicIntentReceiptV1 {
                    intent_id: intent_id.clone(),
                    status: AtomicIntentStatus::Accepted,
                    reason: None,
                },
            );
            Ok(Some(intent))
        }
        Err(err) => {
            enqueue_atomic_receipt(
                chain_id,
                AtomicIntentReceiptV1 {
                    intent_id: intent_id.clone(),
                    status: AtomicIntentStatus::Rejected,
                    reason: Some(err.to_string()),
                },
            );
            Err(err)
        }
    }
}

fn mark_atomic_intent_executed(chain_id: u64, intent: &AtomicCrossChainIntentV1) {
    enqueue_atomic_receipt(
        chain_id,
        AtomicIntentReceiptV1 {
            intent_id: intent.intent_id.clone(),
            status: AtomicIntentStatus::Executed,
            reason: None,
        },
    );
    enqueue_atomic_broadcast_ready(
        chain_id,
        AtomicBroadcastReadyV1 {
            intent: intent.clone(),
            ready_at_unix_ms: now_unix_ms(),
        },
    );
}

fn mark_atomic_intent_apply_failed(chain_id: u64, intent_id: &str, error: &str) {
    enqueue_atomic_receipt(
        chain_id,
        AtomicIntentReceiptV1 {
            intent_id: intent_id.to_string(),
            status: AtomicIntentStatus::Rejected,
            reason: Some(error.to_string()),
        },
    );
}

pub fn drain_execution_receipts_for_host(max_items: usize) -> Vec<SupervmEvmExecutionReceiptV1> {
    if max_items == 0 {
        return Vec::new();
    }
    let shards = resolve_evm_runtime_shards();
    let shard_count = shards.len();
    let start = runtime_shard_start_index(shard_count);
    let mut out = Vec::with_capacity(max_items);
    for offset in 0..shard_count {
        if out.len() >= max_items {
            break;
        }
        let shard = &shards[(start + offset) % shard_count];
        let mut runtime = match shard.lock() {
            Ok(v) => v,
            Err(_) => continue,
        };
        let take = max_items
            .saturating_sub(out.len())
            .min(runtime.execution_receipts.len());
        for _ in 0..take {
            if let Some(item) = runtime.execution_receipts.pop_front() {
                out.push(item);
            }
        }
    }
    out
}

pub fn drain_state_mirror_updates_for_host(max_items: usize) -> Vec<SupervmEvmStateMirrorUpdateV1> {
    if max_items == 0 {
        return Vec::new();
    }
    let shards = resolve_evm_runtime_shards();
    let shard_count = shards.len();
    let start = runtime_shard_start_index(shard_count);
    let mut out = Vec::with_capacity(max_items);
    for offset in 0..shard_count {
        if out.len() >= max_items {
            break;
        }
        let shard = &shards[(start + offset) % shard_count];
        let mut runtime = match shard.lock() {
            Ok(v) => v,
            Err(_) => continue,
        };
        let take = max_items
            .saturating_sub(out.len())
            .min(runtime.state_mirror_updates.len());
        for _ in 0..take {
            if let Some(item) = runtime.state_mirror_updates.pop_front() {
                out.push(item);
            }
        }
    }
    out
}

pub fn drain_plugin_ingress_frames_for_host(max_items: usize) -> Vec<EvmMempoolIngressFrameV1> {
    if max_items == 0 {
        return Vec::new();
    }
    let shards = resolve_evm_txpool_shards();
    let shard_count = shards.len();
    let start = txpool_shard_start_index(shard_count);
    let mut out = Vec::with_capacity(max_items);
    for offset in 0..shard_count {
        if out.len() >= max_items {
            break;
        }
        let shard = &shards[(start + offset) % shard_count];
        let mut runtime = match shard.lock() {
            Ok(v) => v,
            Err(_) => continue,
        };
        let remaining = max_items.saturating_sub(out.len());
        let total = runtime
            .executable_ingress_frames
            .len()
            .saturating_add(runtime.ingress_frames.len());
        let take = remaining.min(total);
        let mut drained = 0usize;
        while drained < take {
            if let Some(frame) = runtime.executable_ingress_frames.pop_front() {
                out.push(frame);
                drained = drained.saturating_add(1);
                continue;
            }
            break;
        }
    }
    if out.len() < max_items {
        let pending_take = max_items.saturating_sub(out.len());
        let pending = drain_pending_frames_round_robin_across_shards(pending_take);
        out.extend(pending);
    }
    out
}

pub fn snapshot_executable_ingress_frames_for_host(
    max_items: usize,
) -> Vec<EvmMempoolIngressFrameV1> {
    if max_items == 0 {
        return Vec::new();
    }
    let shards = resolve_evm_txpool_shards();
    let shard_count = shards.len();
    let start = txpool_shard_start_index(shard_count);
    let mut out = Vec::with_capacity(max_items);
    for offset in 0..shard_count {
        if out.len() >= max_items {
            break;
        }
        let shard = &shards[(start + offset) % shard_count];
        let runtime = match shard.lock() {
            Ok(v) => v,
            Err(_) => continue,
        };
        let remaining = max_items.saturating_sub(out.len());
        out.extend(
            runtime
                .executable_ingress_frames
                .iter()
                .take(remaining)
                .cloned(),
        );
    }
    out
}

pub fn drain_executable_ingress_frames_for_host(max_items: usize) -> Vec<EvmMempoolIngressFrameV1> {
    if max_items == 0 {
        return Vec::new();
    }
    let shards = resolve_evm_txpool_shards();
    let shard_count = shards.len();
    let start = txpool_shard_start_index(shard_count);
    let mut out = Vec::with_capacity(max_items);
    for offset in 0..shard_count {
        if out.len() >= max_items {
            break;
        }
        let shard = &shards[(start + offset) % shard_count];
        let mut runtime = match shard.lock() {
            Ok(v) => v,
            Err(_) => continue,
        };
        let remaining = max_items.saturating_sub(out.len());
        let take = remaining.min(runtime.executable_ingress_frames.len());
        for _ in 0..take {
            if let Some(frame) = runtime.executable_ingress_frames.pop_front() {
                out.push(frame);
            }
        }
    }
    out
}

pub fn snapshot_pending_ingress_frames_for_host(max_items: usize) -> Vec<EvmMempoolIngressFrameV1> {
    if max_items == 0 {
        return Vec::new();
    }
    let shards = resolve_evm_txpool_shards();
    let shard_count = shards.len();
    let start = txpool_shard_start_index(shard_count);
    let mut out = Vec::with_capacity(max_items);
    for offset in 0..shard_count {
        if out.len() >= max_items {
            break;
        }
        let shard = &shards[(start + offset) % shard_count];
        let runtime = match shard.lock() {
            Ok(v) => v,
            Err(_) => continue,
        };
        let remaining = max_items.saturating_sub(out.len());
        out.extend(runtime.ingress_frames.iter().take(remaining).cloned());
    }
    out
}

pub fn drain_pending_ingress_frames_for_host(max_items: usize) -> Vec<EvmMempoolIngressFrameV1> {
    if max_items == 0 {
        return Vec::new();
    }
    let shards = resolve_evm_txpool_shards();
    let shard_count = shards.len();
    let start = txpool_shard_start_index(shard_count);
    let mut out = Vec::with_capacity(max_items);
    for offset in 0..shard_count {
        if out.len() >= max_items {
            break;
        }
        let shard = &shards[(start + offset) % shard_count];
        let mut runtime = match shard.lock() {
            Ok(v) => v,
            Err(_) => continue,
        };
        let remaining = max_items.saturating_sub(out.len());
        let take = remaining.min(runtime.ingress_frames.len());
        for _ in 0..take {
            if let Some(frame) = runtime.ingress_frames.pop_front() {
                remove_nonce_index_for_frame(&mut runtime, &frame);
                out.push(frame);
            }
        }
    }
    out
}

pub fn evict_ingress_tx_hashes_for_host(chain_id: u64, tx_hashes: &[[u8; 32]]) -> usize {
    if tx_hashes.is_empty() {
        return 0;
    }
    let wanted = tx_hashes.iter().copied().collect::<HashSet<[u8; 32]>>();
    if wanted.is_empty() {
        return 0;
    }
    let shards = resolve_evm_txpool_shards();
    let mut removed = 0usize;
    for shard in shards {
        let mut runtime = match shard.lock() {
            Ok(v) => v,
            Err(_) => continue,
        };
        loop {
            let next_hash = runtime.executable_ingress_frames.iter().find_map(|frame| {
                if frame.chain_id != chain_id || frame.tx_hash.len() != 32 {
                    return None;
                }
                let mut hash = [0u8; 32];
                hash.copy_from_slice(frame.tx_hash.as_slice());
                wanted.contains(&hash).then_some(hash)
            });
            let Some(tx_hash) = next_hash else {
                break;
            };
            if remove_executable_ingress_frame_by_hash(&mut runtime, chain_id, tx_hash.as_slice())
                .is_some()
            {
                removed = removed.saturating_add(1);
            } else {
                break;
            }
        }

        loop {
            let next_hash = runtime.ingress_frames.iter().find_map(|frame| {
                if frame.chain_id != chain_id || frame.tx_hash.len() != 32 {
                    return None;
                }
                let mut hash = [0u8; 32];
                hash.copy_from_slice(frame.tx_hash.as_slice());
                wanted.contains(&hash).then_some(hash)
            });
            let Some(tx_hash) = next_hash else {
                break;
            };
            if let Some(frame) = remove_ingress_frame_by_hash(&mut runtime, chain_id, &tx_hash) {
                remove_nonce_index_for_frame(&mut runtime, &frame);
                removed = removed.saturating_add(1);
            } else {
                break;
            }
        }
    }
    removed
}

pub fn evict_stale_ingress_frames_for_host(chain_id: u64, observed_before_unix_ms: u64) -> usize {
    if observed_before_unix_ms == 0 {
        return 0;
    }
    let shards = resolve_evm_txpool_shards();
    let mut removed = 0usize;
    for shard in shards {
        let mut runtime = match shard.lock() {
            Ok(v) => v,
            Err(_) => continue,
        };
        let mut executable_idx = 0usize;
        while executable_idx < runtime.executable_ingress_frames.len() {
            let should_remove = runtime
                .executable_ingress_frames
                .get(executable_idx)
                .map(|frame| {
                    frame.chain_id == chain_id
                        && frame.observed_at_unix_ms <= observed_before_unix_ms
                })
                .unwrap_or(false);
            if should_remove {
                let _ = runtime.executable_ingress_frames.remove(executable_idx);
                removed = removed.saturating_add(1);
            } else {
                executable_idx = executable_idx.saturating_add(1);
            }
        }
        let mut pending_idx = 0usize;
        while pending_idx < runtime.ingress_frames.len() {
            let should_remove = runtime
                .ingress_frames
                .get(pending_idx)
                .map(|frame| {
                    frame.chain_id == chain_id
                        && frame.observed_at_unix_ms <= observed_before_unix_ms
                })
                .unwrap_or(false);
            if should_remove {
                if let Some(frame) = runtime.ingress_frames.remove(pending_idx) {
                    remove_nonce_index_for_frame(&mut runtime, &frame);
                    removed = removed.saturating_add(1);
                }
            } else {
                pending_idx = pending_idx.saturating_add(1);
            }
        }
    }
    removed
}

pub fn snapshot_pending_sender_buckets_for_host(
    max_senders: usize,
    max_txs_per_sender: usize,
) -> Vec<EvmPendingSenderBucketV1> {
    if max_senders == 0 || max_txs_per_sender == 0 {
        return Vec::new();
    }
    #[derive(Debug)]
    struct SenderPendingHashes {
        sender_key: IngressSenderKey,
        tx_hashes: Vec<Vec<u8>>,
        shard_index: usize,
    }

    let shards = resolve_evm_txpool_shards();
    let shard_count = shards.len();
    let start = txpool_shard_start_index(shard_count);
    let mut sender_entries: Vec<SenderPendingHashes> = Vec::new();
    for offset in 0..shard_count {
        let shard_index = (start + offset) % shard_count;
        let shard = &shards[shard_index];
        let runtime = match shard.lock() {
            Ok(v) => v,
            Err(_) => continue,
        };
        sender_entries.extend(runtime.pending_by_sender.iter().map(|(key, map)| {
            SenderPendingHashes {
                sender_key: key.clone(),
                tx_hashes: map.values().take(max_txs_per_sender).cloned().collect(),
                shard_index,
            }
        }));
    }
    sender_entries.sort_by(
        |SenderPendingHashes {
             sender_key: left_key,
             ..
         },
         SenderPendingHashes {
             sender_key: right_key,
             ..
         }| match left_key.chain_id.cmp(&right_key.chain_id) {
            std::cmp::Ordering::Equal => left_key.from.cmp(&right_key.from),
            ord => ord,
        },
    );
    sender_entries.truncate(max_senders);

    let mut needed_hashes_by_shard: HashMap<usize, HashSet<Vec<u8>>> = HashMap::new();
    for entry in &sender_entries {
        let needed_hashes = needed_hashes_by_shard.entry(entry.shard_index).or_default();
        for tx_hash in &entry.tx_hashes {
            needed_hashes.insert(tx_hash.clone());
        }
    }

    let mut ingress_tx_by_hash_by_shard: HashMap<usize, HashMap<Vec<u8>, TxIR>> = HashMap::new();
    for (shard_index, needed_hashes) in needed_hashes_by_shard {
        if needed_hashes.is_empty() {
            continue;
        }
        let shard = &shards[shard_index];
        let runtime = match shard.lock() {
            Ok(v) => v,
            Err(_) => continue,
        };
        let mut tx_by_hash: HashMap<Vec<u8>, TxIR> = HashMap::with_capacity(needed_hashes.len());
        for frame in runtime.ingress_frames.iter() {
            if !needed_hashes.contains(&frame.tx_hash) {
                continue;
            }
            if let Some(tx) = frame.parsed_tx.as_ref() {
                tx_by_hash
                    .entry(frame.tx_hash.clone())
                    .or_insert_with(|| tx.clone());
            }
            if tx_by_hash.len() >= needed_hashes.len() {
                break;
            }
        }
        ingress_tx_by_hash_by_shard.insert(shard_index, tx_by_hash);
    }

    let mut out = Vec::with_capacity(sender_entries.len());
    for SenderPendingHashes {
        sender_key,
        tx_hashes,
        shard_index,
    } in sender_entries
    {
        let mut txs = Vec::with_capacity(max_txs_per_sender.min(tx_hashes.len()));
        if let Some(ingress_tx_by_hash) = ingress_tx_by_hash_by_shard.get(&shard_index) {
            for tx_hash in tx_hashes.iter().take(max_txs_per_sender) {
                if let Some(tx) = ingress_tx_by_hash.get(tx_hash) {
                    txs.push(tx.clone());
                }
            }
        }
        if txs.is_empty() {
            continue;
        }
        out.push(EvmPendingSenderBucketV1 {
            chain_id: sender_key.chain_id,
            sender: sender_key.from.clone(),
            txs,
        });
    }
    out
}

pub fn plugin_settlement_totals_for_host() -> (u128, u128) {
    let shards = resolve_evm_runtime_shards();
    let mut reserve_total_wei = 0u128;
    let mut payout_total_units = 0u128;
    for shard in shards {
        let runtime = match shard.lock() {
            Ok(v) => v,
            Err(_) => continue,
        };
        reserve_total_wei = reserve_total_wei.saturating_add(runtime.reserve_total_wei);
        payout_total_units = payout_total_units.saturating_add(runtime.payout_total_units);
    }
    (reserve_total_wei, payout_total_units)
}

pub fn settlement_snapshot_for_host() -> EvmFeeSettlementSnapshotV1 {
    let shards = resolve_evm_runtime_shards();
    let mut policy: Option<EvmFeeSettlementPolicyV1> = None;
    let mut reserve_total_wei = 0u128;
    let mut payout_total_units = 0u128;
    for shard in shards {
        let runtime = match shard.lock() {
            Ok(v) => v,
            Err(_) => continue,
        };
        if policy.is_none() {
            policy = Some(runtime.settlement_policy.clone());
        }
        reserve_total_wei = reserve_total_wei.saturating_add(runtime.reserve_total_wei);
        payout_total_units = payout_total_units.saturating_add(runtime.payout_total_units);
    }
    EvmFeeSettlementSnapshotV1 {
        policy: policy.unwrap_or_default(),
        settlement_seq: EVM_RUNTIME_SETTLEMENT_SEQ.load(Ordering::Relaxed),
        reserve_total_wei,
        payout_total_units,
    }
}

pub fn drain_atomic_receipts_for_host(max_items: usize) -> Vec<AtomicIntentReceiptV1> {
    if max_items == 0 {
        return Vec::new();
    }
    let shards = resolve_evm_runtime_shards();
    let shard_count = shards.len();
    let start = runtime_shard_start_index(shard_count);
    let mut out = Vec::with_capacity(max_items);
    for offset in 0..shard_count {
        if out.len() >= max_items {
            break;
        }
        let shard = &shards[(start + offset) % shard_count];
        let mut runtime = match shard.lock() {
            Ok(v) => v,
            Err(_) => continue,
        };
        let take = max_items
            .saturating_sub(out.len())
            .min(runtime.atomic_receipts.len());
        for _ in 0..take {
            if let Some(receipt) = runtime.atomic_receipts.pop_front() {
                out.push(receipt);
            }
        }
    }
    out
}

pub fn drain_settlement_records_for_host(max_items: usize) -> Vec<EvmFeeSettlementRecordV1> {
    if max_items == 0 {
        return Vec::new();
    }
    let shards = resolve_evm_runtime_shards();
    let shard_count = shards.len();
    let start = runtime_shard_start_index(shard_count);
    let mut out = Vec::with_capacity(max_items);
    for offset in 0..shard_count {
        if out.len() >= max_items {
            break;
        }
        let shard = &shards[(start + offset) % shard_count];
        let mut runtime = match shard.lock() {
            Ok(v) => v,
            Err(_) => continue,
        };
        let take = max_items
            .saturating_sub(out.len())
            .min(runtime.settlement_records.len());
        for _ in 0..take {
            if let Some(item) = runtime.settlement_records.pop_front() {
                out.push(item);
            }
        }
    }
    out
}

pub fn drain_payout_instructions_for_host(max_items: usize) -> Vec<EvmFeePayoutInstructionV1> {
    if max_items == 0 {
        return Vec::new();
    }
    let shards = resolve_evm_runtime_shards();
    let shard_count = shards.len();
    let start = runtime_shard_start_index(shard_count);
    let mut out = Vec::with_capacity(max_items);
    for offset in 0..shard_count {
        if out.len() >= max_items {
            break;
        }
        let shard = &shards[(start + offset) % shard_count];
        let mut runtime = match shard.lock() {
            Ok(v) => v,
            Err(_) => continue,
        };
        let take = max_items
            .saturating_sub(out.len())
            .min(runtime.payout_instructions.len());
        for _ in 0..take {
            if let Some(item) = runtime.payout_instructions.pop_front() {
                out.push(item);
            }
        }
    }
    out
}

pub fn drain_atomic_broadcast_ready_for_host(max_items: usize) -> Vec<AtomicBroadcastReadyV1> {
    if max_items == 0 {
        return Vec::new();
    }
    let shards = resolve_evm_runtime_shards();
    let shard_count = shards.len();
    let start = runtime_shard_start_index(shard_count);
    let mut out = Vec::with_capacity(max_items);
    for offset in 0..shard_count {
        if out.len() >= max_items {
            break;
        }
        let shard = &shards[(start + offset) % shard_count];
        let mut runtime = match shard.lock() {
            Ok(v) => v,
            Err(_) => continue,
        };
        let take = max_items
            .saturating_sub(out.len())
            .min(runtime.atomic_broadcast_ready.len());
        for _ in 0..take {
            if let Some(item) = runtime.atomic_broadcast_ready.pop_front() {
                out.push(item);
            }
        }
    }
    out
}

fn current_workdir_or_dot() -> PathBuf {
    std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."))
}

unsafe fn write_bincode_blob_to_out(
    payload: &[u8],
    out_ptr: *mut u8,
    out_cap: usize,
    out_len: *mut usize,
) -> i32 {
    if out_len.is_null() {
        return NOVOVM_ADAPTER_PLUGIN_RC_INVALID_ARG;
    }
    *out_len = payload.len();
    if payload.len() > MAX_PLUGIN_TX_IR_BYTES {
        return NOVOVM_ADAPTER_PLUGIN_RC_PAYLOAD_TOO_LARGE;
    }
    if out_ptr.is_null() {
        return if out_cap == 0 {
            NOVOVM_ADAPTER_PLUGIN_RC_OK
        } else {
            NOVOVM_ADAPTER_PLUGIN_RC_INVALID_ARG
        };
    }
    if out_cap < payload.len() {
        return NOVOVM_ADAPTER_PLUGIN_RC_BUFFER_TOO_SMALL;
    }
    std::ptr::copy_nonoverlapping(payload.as_ptr(), out_ptr, payload.len());
    NOVOVM_ADAPTER_PLUGIN_RC_OK
}

fn serialize_bincode_export_blob<T: Serialize>(value: &T) -> Result<Vec<u8>, i32> {
    let payload =
        bincode_compat::serialize(value).map_err(|_| NOVOVM_ADAPTER_PLUGIN_RC_APPLY_FAILED)?;
    if payload.len() > MAX_PLUGIN_TX_IR_BYTES {
        return Err(NOVOVM_ADAPTER_PLUGIN_RC_PAYLOAD_TOO_LARGE);
    }
    Ok(payload)
}

fn default_plugin_store_path(backend: UaPluginStoreBackend) -> PathBuf {
    let base = current_workdir_or_dot().join(UA_PLUGIN_ARTIFACTS_SUBDIR);
    match backend {
        UaPluginStoreBackend::Memory => PathBuf::new(),
        UaPluginStoreBackend::BincodeFile => base.join("ua-plugin-self-guard-router.bin"),
        UaPluginStoreBackend::Rocksdb => base.join("ua-plugin-self-guard-router.rocksdb"),
    }
}

fn default_plugin_audit_path(backend: UaPluginAuditBackend) -> PathBuf {
    let base = current_workdir_or_dot().join(UA_PLUGIN_ARTIFACTS_SUBDIR);
    match backend {
        UaPluginAuditBackend::None => PathBuf::new(),
        UaPluginAuditBackend::Jsonl => base.join("ua-plugin-self-guard-audit.jsonl"),
        UaPluginAuditBackend::Rocksdb => base.join("ua-plugin-self-guard-audit.rocksdb"),
    }
}

fn ua_plugin_allow_non_prod_backend() -> bool {
    std::env::var(UA_PLUGIN_ALLOW_NON_PROD_BACKEND_ENV)
        .ok()
        .map(|raw| raw.trim().to_ascii_lowercase())
        .map(|raw| matches!(raw.as_str(), "1" | "true" | "yes" | "on"))
        .unwrap_or(false)
}

fn parse_store_backend(raw: &str) -> UaPluginStoreBackend {
    let allow_non_prod = ua_plugin_allow_non_prod_backend();
    let normalized = raw.trim().to_ascii_lowercase();
    match normalized.as_str() {
        "" | "rocksdb" => UaPluginStoreBackend::Rocksdb,
        "memory" => {
            if allow_non_prod {
                UaPluginStoreBackend::Memory
            } else {
                panic!(
                    "NOVOVM_ADAPTER_PLUGIN_UA_STORE_BACKEND=memory is non-production; set {}=1 for explicit override",
                    UA_PLUGIN_ALLOW_NON_PROD_BACKEND_ENV
                )
            }
        }
        "bincode_file" | "bincode" | "file" => {
            if allow_non_prod {
                UaPluginStoreBackend::BincodeFile
            } else {
                panic!(
                    "NOVOVM_ADAPTER_PLUGIN_UA_STORE_BACKEND={} is non-production; set {}=1 for explicit override",
                    normalized,
                    UA_PLUGIN_ALLOW_NON_PROD_BACKEND_ENV
                )
            }
        }
        _ => panic!(
            "invalid NOVOVM_ADAPTER_PLUGIN_UA_STORE_BACKEND={}; valid: rocksdb|memory|bincode_file|bincode|file",
            normalized
        ),
    }
}

fn parse_audit_backend(raw: &str) -> UaPluginAuditBackend {
    let allow_non_prod = ua_plugin_allow_non_prod_backend();
    let normalized = raw.trim().to_ascii_lowercase();
    match normalized.as_str() {
        "" | "rocksdb" => UaPluginAuditBackend::Rocksdb,
        "none" => {
            if allow_non_prod {
                UaPluginAuditBackend::None
            } else {
                panic!(
                    "NOVOVM_ADAPTER_PLUGIN_UA_AUDIT_BACKEND=none is non-production; set {}=1 for explicit override",
                    UA_PLUGIN_ALLOW_NON_PROD_BACKEND_ENV
                )
            }
        }
        "jsonl" => {
            if allow_non_prod {
                UaPluginAuditBackend::Jsonl
            } else {
                panic!(
                    "NOVOVM_ADAPTER_PLUGIN_UA_AUDIT_BACKEND=jsonl is non-production; set {}=1 for explicit override",
                    UA_PLUGIN_ALLOW_NON_PROD_BACKEND_ENV
                )
            }
        }
        _ => panic!(
            "invalid NOVOVM_ADAPTER_PLUGIN_UA_AUDIT_BACKEND={}; valid: rocksdb|none|jsonl",
            normalized
        ),
    }
}

fn resolve_ua_plugin_standalone_config() -> &'static UaPluginStandaloneConfig {
    UA_PLUGIN_STANDALONE_CONFIG.get_or_init(|| {
        let store_backend = parse_store_backend(
            &std::env::var("NOVOVM_ADAPTER_PLUGIN_UA_STORE_BACKEND")
                .unwrap_or_else(|_| "rocksdb".to_string()),
        );
        let store_path = std::env::var("NOVOVM_ADAPTER_PLUGIN_UA_STORE_PATH")
            .ok()
            .map(|raw| raw.trim().to_string())
            .filter(|raw| !raw.is_empty())
            .map(PathBuf::from)
            .unwrap_or_else(|| default_plugin_store_path(store_backend));

        let audit_backend = parse_audit_backend(
            &std::env::var("NOVOVM_ADAPTER_PLUGIN_UA_AUDIT_BACKEND")
                .unwrap_or_else(|_| "rocksdb".to_string()),
        );
        let audit_path = std::env::var("NOVOVM_ADAPTER_PLUGIN_UA_AUDIT_PATH")
            .ok()
            .map(|raw| raw.trim().to_string())
            .filter(|raw| !raw.is_empty())
            .map(PathBuf::from)
            .unwrap_or_else(|| default_plugin_audit_path(audit_backend));

        UaPluginStandaloneConfig {
            store_backend,
            store_path,
            audit_backend,
            audit_path,
        }
    })
}

fn open_rocksdb(path: &Path) -> anyhow::Result<rocksdb::DB> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)
            .map_err(|e| anyhow::anyhow!("create rocksdb parent dir failed: {e}"))?;
    }
    let mut opts = RocksDbOptions::default();
    opts.create_if_missing(true);
    rocksdb::DB::open(&opts, path)
        .map_err(|e| anyhow::anyhow!("open rocksdb failed: {} ({})", path.display(), e))
}

fn decode_store_envelope(raw: &[u8]) -> anyhow::Result<UaPluginStoreEnvelopeV1> {
    if let Ok(envelope) = crate::bincode_compat::deserialize::<UaPluginStoreEnvelopeV1>(raw) {
        if envelope.version == UA_PLUGIN_STORE_VERSION_V1 {
            return Ok(envelope);
        }
        anyhow::bail!(
            "unsupported ua plugin store envelope version={}",
            envelope.version
        );
    }

    // Backward compatibility: older payload persisted router directly.
    let router: UnifiedAccountRouter = crate::bincode_compat::deserialize(raw).map_err(|e| {
        anyhow::anyhow!("decode ua plugin store envelope failed (router fallback): {e}")
    })?;
    Ok(UaPluginStoreEnvelopeV1 {
        version: 0,
        router,
        audit_seq: 0,
    })
}

fn load_runtime_from_bincode_file(path: &Path) -> anyhow::Result<Option<UaPluginRuntime>> {
    if !path.exists() {
        return Ok(None);
    }
    let raw = fs::read(path).map_err(|e| {
        anyhow::anyhow!(
            "read ua plugin store file failed: {} ({})",
            path.display(),
            e
        )
    })?;
    let envelope = decode_store_envelope(&raw)?;
    Ok(Some(UaPluginRuntime {
        router: envelope.router,
        audit_seq: envelope.audit_seq,
    }))
}

fn load_runtime_from_rocksdb(path: &Path) -> anyhow::Result<Option<UaPluginRuntime>> {
    let db = open_rocksdb(path)?;
    let raw = db
        .get(UA_PLUGIN_STORE_KEY_V1)
        .map_err(|e| anyhow::anyhow!("rocksdb read ua plugin store failed: {}", e))?;
    let Some(raw) = raw else {
        return Ok(None);
    };
    let envelope = decode_store_envelope(&raw)?;
    Ok(Some(UaPluginRuntime {
        router: envelope.router,
        audit_seq: envelope.audit_seq,
    }))
}

fn load_runtime_from_store(config: &UaPluginStandaloneConfig) -> anyhow::Result<UaPluginRuntime> {
    let runtime = match config.store_backend {
        UaPluginStoreBackend::Memory => None,
        UaPluginStoreBackend::BincodeFile => load_runtime_from_bincode_file(&config.store_path)?,
        UaPluginStoreBackend::Rocksdb => load_runtime_from_rocksdb(&config.store_path)?,
    };
    Ok(runtime.unwrap_or_default())
}

fn save_runtime_to_bincode_file(path: &Path, runtime: &UaPluginRuntime) -> anyhow::Result<()> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)
            .map_err(|e| anyhow::anyhow!("create ua plugin store dir failed: {e}"))?;
    }
    let envelope = UaPluginStoreEnvelopeRefV1 {
        version: UA_PLUGIN_STORE_VERSION_V1,
        router: &runtime.router,
        audit_seq: runtime.audit_seq,
    };
    let payload = crate::bincode_compat::serialize(&envelope)
        .map_err(|e| anyhow::anyhow!("encode ua plugin store payload failed: {e}"))?;
    fs::write(path, payload).map_err(|e| {
        anyhow::anyhow!(
            "write ua plugin store file failed: {} ({})",
            path.display(),
            e
        )
    })
}

fn save_runtime_to_rocksdb(path: &Path, runtime: &UaPluginRuntime) -> anyhow::Result<()> {
    let db = open_rocksdb(path)?;
    let envelope = UaPluginStoreEnvelopeRefV1 {
        version: UA_PLUGIN_STORE_VERSION_V1,
        router: &runtime.router,
        audit_seq: runtime.audit_seq,
    };
    let payload = crate::bincode_compat::serialize(&envelope)
        .map_err(|e| anyhow::anyhow!("encode ua plugin store payload failed: {e}"))?;
    db.put(UA_PLUGIN_STORE_KEY_V1, payload)
        .map_err(|e| anyhow::anyhow!("rocksdb write ua plugin store failed: {}", e))
}

fn save_runtime_to_store(
    config: &UaPluginStandaloneConfig,
    runtime: &UaPluginRuntime,
) -> anyhow::Result<()> {
    match config.store_backend {
        UaPluginStoreBackend::Memory => Ok(()),
        UaPluginStoreBackend::BincodeFile => {
            save_runtime_to_bincode_file(&config.store_path, runtime)
        }
        UaPluginStoreBackend::Rocksdb => save_runtime_to_rocksdb(&config.store_path, runtime),
    }
}

fn parse_u64_be(raw: &[u8]) -> Option<u64> {
    if raw.len() != 8 {
        return None;
    }
    let mut buf = [0u8; 8];
    buf.copy_from_slice(raw);
    Some(u64::from_be_bytes(buf))
}

fn append_audit_jsonl(path: &Path, record: &UaPluginAuditRecordV1) -> anyhow::Result<()> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)
            .map_err(|e| anyhow::anyhow!("create ua plugin audit dir failed: {e}"))?;
    }
    let payload = serde_json::to_string(record)
        .map_err(|e| anyhow::anyhow!("serialize ua plugin audit record failed: {e}"))?;
    let mut writer = OpenOptions::new()
        .create(true)
        .append(true)
        .open(path)
        .map_err(|e| {
            anyhow::anyhow!(
                "open ua plugin audit jsonl failed: {} ({})",
                path.display(),
                e
            )
        })?;
    writer
        .write_all(payload.as_bytes())
        .and_then(|_| writer.write_all(b"\n"))
        .map_err(|e| {
            anyhow::anyhow!(
                "append ua plugin audit jsonl failed: {} ({})",
                path.display(),
                e
            )
        })
}

fn append_audit_rocksdb(path: &Path, record: &UaPluginAuditRecordV1) -> anyhow::Result<()> {
    let db = open_rocksdb(path)?;
    let key = format!("{}{:020}", UA_PLUGIN_AUDIT_SEQ_KEY_PREFIX_V1, record.seq);
    let payload = serde_json::to_vec(record)
        .map_err(|e| anyhow::anyhow!("serialize ua plugin audit record failed: {e}"))?;
    db.put(key.as_bytes(), payload)
        .map_err(|e| anyhow::anyhow!("rocksdb write ua plugin audit record failed: {}", e))?;
    db.put(UA_PLUGIN_AUDIT_HEAD_KEY_V1, record.seq.to_be_bytes())
        .map_err(|e| anyhow::anyhow!("rocksdb write ua plugin audit head failed: {}", e))
}

fn append_plugin_audit_record(
    config: &UaPluginStandaloneConfig,
    runtime: &mut UaPluginRuntime,
    chain_id: u64,
    tx_count: usize,
    success: bool,
    error: Option<&str>,
    events: Vec<AccountAuditEvent>,
) -> anyhow::Result<()> {
    if config.audit_backend == UaPluginAuditBackend::None {
        return Ok(());
    }

    let route_meta = resolve_plugin_overlay_route_meta(now_unix_sec());
    runtime.audit_seq = runtime.audit_seq.saturating_add(1);
    let record = UaPluginAuditRecordV1 {
        seq: runtime.audit_seq,
        at: now_unix_sec(),
        source: "plugin_self_guard".to_string(),
        chain_id,
        tx_count,
        success,
        error: error.map(ToOwned::to_owned),
        store_backend: config.store_backend.as_str().to_string(),
        audit_backend: config.audit_backend.as_str().to_string(),
        overlay_route_id: route_meta.overlay_route_id,
        overlay_route_epoch: route_meta.overlay_route_epoch,
        overlay_route_mask_bits: route_meta.overlay_route_mask_bits,
        overlay_route_mode: route_meta.overlay_route_mode,
        overlay_route_region: route_meta.overlay_route_region,
        overlay_route_relay_bucket: route_meta.overlay_route_relay_bucket,
        overlay_route_relay_set_size: route_meta.overlay_route_relay_set_size,
        overlay_route_relay_round: route_meta.overlay_route_relay_round,
        overlay_route_relay_index: route_meta.overlay_route_relay_index,
        overlay_route_relay_id: route_meta.overlay_route_relay_id,
        overlay_route_strategy: route_meta.overlay_route_strategy,
        overlay_route_hop_count: route_meta.overlay_route_hop_count,
        events,
    };

    match config.audit_backend {
        UaPluginAuditBackend::None => Ok(()),
        UaPluginAuditBackend::Jsonl => append_audit_jsonl(&config.audit_path, &record),
        UaPluginAuditBackend::Rocksdb => append_audit_rocksdb(&config.audit_path, &record),
    }
}

fn reconcile_audit_seq_from_backend(
    config: &UaPluginStandaloneConfig,
    runtime: &mut UaPluginRuntime,
) -> anyhow::Result<()> {
    if runtime.audit_seq != 0 || config.audit_backend != UaPluginAuditBackend::Rocksdb {
        return Ok(());
    }
    let db = open_rocksdb(&config.audit_path)?;
    let head = db
        .get(UA_PLUGIN_AUDIT_HEAD_KEY_V1)
        .map_err(|e| anyhow::anyhow!("rocksdb read ua plugin audit head failed: {}", e))?;
    if let Some(head) = head.and_then(|raw| parse_u64_be(&raw)) {
        runtime.audit_seq = head;
    }
    Ok(())
}

fn ua_plugin_runtime(
    config: &UaPluginStandaloneConfig,
) -> anyhow::Result<&'static Mutex<UaPluginRuntime>> {
    if let Some(runtime) = UA_PLUGIN_RUNTIME.get() {
        return Ok(runtime);
    }
    let mut runtime = load_runtime_from_store(config)?;
    reconcile_audit_seq_from_backend(config, &mut runtime)?;
    let _ = UA_PLUGIN_RUNTIME.set(Mutex::new(runtime));
    UA_PLUGIN_RUNTIME
        .get()
        .ok_or_else(|| anyhow::anyhow!("initialize ua plugin runtime failed"))
}

fn map_create_uca_error(err: UnifiedAccountError) -> anyhow::Result<()> {
    match err {
        UnifiedAccountError::UcaAlreadyExists { .. } => Ok(()),
        other => Err(anyhow::anyhow!(
            "plugin ua self-guard create_uca failed: {}",
            other
        )),
    }
}

fn route_txs_via_plugin_ua_self_guard(chain_id: u64, txs: &[TxIR]) -> anyhow::Result<()> {
    let config = resolve_ua_plugin_standalone_config();
    let runtime = ua_plugin_runtime(config)?;
    let mut runtime = runtime
        .lock()
        .map_err(|_| anyhow::anyhow!("plugin ua self-guard mutex poisoned"))?;
    let base_now = now_unix_sec();
    let mut route_error: Option<anyhow::Error> = None;

    for (idx, tx) in txs.iter().enumerate() {
        if tx.from.is_empty() {
            route_error = Some(anyhow::anyhow!(
                "plugin ua self-guard requires non-empty tx.from"
            ));
            break;
        }
        let now = base_now.saturating_add(idx as u64);
        let persona = PersonaAddress {
            persona_type: PersonaType::Evm,
            chain_id,
            external_address: tx.from.clone(),
        };
        let uca_id = format!("uca:plugin:{}:{}", chain_id, to_lower_hex(&tx.from));
        if let Err(err) =
            runtime
                .router
                .create_uca(uca_id.clone(), derive_primary_key_ref(&uca_id), now)
        {
            if let Err(mapped) = map_create_uca_error(err) {
                route_error = Some(mapped);
                break;
            }
        }

        match runtime.router.resolve_binding_owner(&persona) {
            Some(owner) if owner == uca_id => {}
            Some(owner) => {
                route_error = Some(anyhow::anyhow!(
                    "plugin ua self-guard binding conflict: owner={} expected={}",
                    owner,
                    uca_id
                ));
                break;
            }
            None => {
                if let Err(err) =
                    runtime
                        .router
                        .add_binding(&uca_id, AccountRole::Owner, persona.clone(), now)
                {
                    route_error = Some(anyhow::anyhow!(
                        "plugin ua self-guard add_binding failed: {}",
                        err
                    ));
                    break;
                }
            }
        }

        let request = RouteRequest {
            uca_id,
            persona,
            role: AccountRole::Owner,
            protocol: ProtocolKind::Eth,
            signature_domain: format!("evm:{}", chain_id),
            nonce: tx.nonce,
            kyc_attestation_provided: false,
            kyc_verified: false,
            wants_cross_chain_atomic: false,
            tx_type4: false,
            session_expires_at: None,
            now,
        };
        if let Err(err) = runtime.router.route(request) {
            route_error = Some(anyhow::anyhow!(
                "plugin ua self-guard route failed: {}",
                err
            ));
            break;
        }
    }

    let events = runtime.router.take_events();
    let success = route_error.is_none();
    let error_text = route_error.as_ref().map(|err| err.to_string());
    append_plugin_audit_record(
        config,
        &mut runtime,
        chain_id,
        txs.len(),
        success,
        error_text.as_deref(),
        events,
    )?;
    save_runtime_to_store(config, &runtime)?;

    if let Some(err) = route_error {
        return Err(err);
    }
    Ok(())
}

fn chain_type_from_code(code: u32) -> Option<ChainType> {
    Some(match code {
        1 => ChainType::EVM,
        5 => ChainType::Polygon,
        6 => ChainType::BNB,
        7 => ChainType::Avalanche,
        _ => return None,
    })
}

fn validate_plugin_tx_batch(txs: &[TxIR]) -> Result<(), i32> {
    if txs.is_empty() {
        return Err(NOVOVM_ADAPTER_PLUGIN_RC_EMPTY_BATCH);
    }
    if txs.len() > MAX_PLUGIN_TX_COUNT {
        return Err(NOVOVM_ADAPTER_PLUGIN_RC_BATCH_TOO_LARGE);
    }
    if !txs.iter().all(|tx| {
        matches!(
            tx.tx_type,
            TxType::Transfer | TxType::ContractCall | TxType::ContractDeploy
        )
    }) {
        return Err(NOVOVM_ADAPTER_PLUGIN_RC_UNSUPPORTED_TX_TYPE);
    }
    Ok(())
}

fn decode_plugin_apply_inputs(
    chain_type_code: u32,
    tx_ir_ptr: *const u8,
    tx_ir_len: usize,
) -> Result<(ChainType, Vec<TxIR>), i32> {
    if tx_ir_ptr.is_null() || tx_ir_len == 0 {
        return Err(NOVOVM_ADAPTER_PLUGIN_RC_INVALID_ARG);
    }
    if tx_ir_len > MAX_PLUGIN_TX_IR_BYTES || tx_ir_len > isize::MAX as usize {
        return Err(NOVOVM_ADAPTER_PLUGIN_RC_PAYLOAD_TOO_LARGE);
    }

    let chain_type = match chain_type_from_code(chain_type_code) {
        Some(v) => v,
        None => return Err(NOVOVM_ADAPTER_PLUGIN_RC_UNSUPPORTED_CHAIN),
    };

    let tx_bytes = unsafe { std::slice::from_raw_parts(tx_ir_ptr, tx_ir_len) };
    if tx_bytes.len() > MAX_PLUGIN_TX_IR_BYTES {
        return Err(NOVOVM_ADAPTER_PLUGIN_RC_PAYLOAD_TOO_LARGE);
    }
    let txs: Vec<TxIR> =
        match crate::bincode_compat::deserialize_with_remainder::<Vec<TxIR>>(tx_bytes) {
            Ok((v, _)) => v,
            Err(_) => return Err(NOVOVM_ADAPTER_PLUGIN_RC_DECODE_FAILED),
        };
    validate_plugin_tx_batch(&txs)?;
    Ok((chain_type, txs))
}

pub fn runtime_tap_ir_batch_v1(
    chain_type: ChainType,
    chain_id: u64,
    txs: &[TxIR],
    flags: u64,
) -> Result<EvmRuntimeTapSummaryV1, i32> {
    validate_plugin_tx_batch(txs)?;
    let prepared_txs = prepare_txs_with_hashes(txs);
    if flags & NOVOVM_ADAPTER_PLUGIN_APPLY_FLAG_UA_SELF_GUARD_V1 != 0
        && route_txs_via_plugin_ua_self_guard(chain_id, &prepared_txs).is_err()
    {
        return Err(NOVOVM_ADAPTER_PLUGIN_RC_UA_SELF_GUARD_FAILED);
    }
    let ingress = push_ingress_frames_prepared(chain_id, &prepared_txs);
    let atomic_guard_requested =
        flags & NOVOVM_ADAPTER_PLUGIN_APPLY_FLAG_ATOMIC_INTENT_GUARD_V1 != 0;
    if atomic_guard_requested {
        let intent = prepare_atomic_intent_guard(chain_type, chain_id, &ingress.accepted_txs)
            .map_err(|_| NOVOVM_ADAPTER_PLUGIN_RC_APPLY_FAILED)?;
        if let Some(intent) = intent {
            enqueue_atomic_broadcast_ready(
                chain_id,
                AtomicBroadcastReadyV1 {
                    intent,
                    ready_at_unix_ms: now_unix_ms(),
                },
            );
        }
    }
    settle_fee_income_for_batch(chain_id, &ingress.accepted_txs);
    Ok(ingress.summary)
}

pub fn apply_ir_batch_v1(
    chain_type: ChainType,
    chain_id: u64,
    txs: &[TxIR],
) -> Result<NovovmAdapterPluginApplyResultV1, i32> {
    validate_plugin_tx_batch(txs)?;
    let prepared_txs = prepare_txs_with_hashes(txs);
    apply_ir_batch(chain_type, chain_id, &prepared_txs)
        .map_err(|_| NOVOVM_ADAPTER_PLUGIN_RC_APPLY_FAILED)
}

fn apply_ir_batch(
    chain_type: ChainType,
    chain_id: u64,
    txs: &[TxIR],
) -> anyhow::Result<NovovmAdapterPluginApplyResultV1> {
    Ok(apply_ir_batch_with_artifacts(chain_type, chain_id, txs, 0)?.result)
}

fn apply_ir_batch_with_artifacts(
    chain_type: ChainType,
    chain_id: u64,
    txs: &[TxIR],
    flags: u64,
) -> anyhow::Result<ApplyBatchArtifacts> {
    let profile = resolve_evm_profile(chain_type, chain_id)?;
    let _active_precompiles = active_precompile_set_m0(&profile);

    let config = ChainConfig {
        chain_type,
        chain_id,
        name: format!("evm-plugin-{}", chain_type.as_str()),
        enabled: true,
        custom_config: None,
    };

    let mut adapter = NovoVmAdapter::new(config);
    adapter.initialize()?;

    // Keep state transition deterministic: verify in parallel, execute in order.
    let apply_result = (|| -> anyhow::Result<(
        NovovmAdapterPluginApplyResultV1,
        Vec<bool>,
        Option<AoemBatchExecutionArtifactsV1>,
        u64,
    )> {
        let verify_results = validate_and_verify_txs_for_apply_batch(&adapter, &profile, txs)?;
        let mut state = StateIR::new();
        let mut verified = true;
        let mut applied = true;
        let pre_state_root = normalize_root32(&adapter.state_root()?);
        let state_version = next_execution_state_version();
        let mut aoem_artifacts = match try_execute_mainline_batch_via_aoem(
            chain_type,
            chain_id,
            txs,
            verify_results.as_slice(),
            pre_state_root,
            state_version,
        ) {
            Ok(v) => v,
            Err(err) => {
                if aoem_mainline_required() {
                    return Err(err.context("aoem mainline batch execution failed"));
                }
                None
            }
        };
        let mut resolved_artifacts = vec![None; txs.len()];
        for (idx, (tx, tx_ok)) in txs.iter().zip(verify_results.iter().copied()).enumerate() {
            verified = verified && tx_ok;
            if !tx_ok {
                applied = false;
                continue;
            }
            let artifact = aoem_artifacts
                .as_ref()
                .and_then(|artifacts| artifacts.tx_artifacts.get(idx));
            let resolved_artifact = adapter.execute_transaction_with_artifact(tx, &mut state, artifact)?;
            applied = applied && resolved_artifact.status_ok;
            resolved_artifacts[idx] = Some(resolved_artifact);
        }
        let final_state_root = normalize_root32(&adapter.state_root()?);
        aoem_artifacts = materialize_batch_execution_artifacts(
            txs,
            verify_results.as_slice(),
            final_state_root,
            resolved_artifacts,
            aoem_artifacts,
        );
        let accounts = state.accounts.len() as u64;
        Ok((
            NovovmAdapterPluginApplyResultV1 {
                verified: u8::from(verified),
                applied: u8::from(applied),
                txs: txs.len() as u64,
                accounts,
                state_root: final_state_root,
                error_code: 0,
            },
            verify_results,
            aoem_artifacts,
            state_version,
        ))
    })();

    let shutdown_result = adapter.shutdown();
    match (apply_result, shutdown_result) {
        (Ok((result, verify_results, aoem_artifacts, state_version)), Ok(())) => {
            let execution_receipts = build_execution_receipts_from_apply_batch(
                chain_type,
                chain_id,
                txs,
                verify_results.as_slice(),
                &result,
                state_version,
                aoem_artifacts.as_ref(),
            );
            if flags & NOVOVM_ADAPTER_PLUGIN_APPLY_FLAG_MAINLINE_RECEIPT_EXPORT_V1 != 0 {
                enqueue_execution_receipts(chain_id, execution_receipts.as_slice());
            }
            let state_mirror_update =
                if flags & NOVOVM_ADAPTER_PLUGIN_APPLY_FLAG_MAINLINE_RECEIPT_INGEST_V1 != 0 {
                    Some(ingest_execution_receipts_for_host(
                        chain_id,
                        state_version,
                        execution_receipts.as_slice(),
                    )?)
                } else {
                    None
                };
            Ok(ApplyBatchArtifacts {
                result,
                execution_receipts,
                state_mirror_update,
                state_version,
            })
        }
        (Ok(_), Err(err)) => Err(err),
        (Err(err), Ok(())) => Err(err),
        (Err(err), Err(_)) => Err(err),
    }
}

pub fn submit_internal_batch_to_mainline_v1(
    chain_type: ChainType,
    chain_id: u64,
    txs: &[TxIR],
    atomic_guard_enabled: bool,
) -> anyhow::Result<SupervmEvmNativeSubmitReportV1> {
    validate_plugin_tx_batch(txs)
        .map_err(|rc| anyhow::anyhow!("validate plugin tx batch failed: rc={rc}"))?;
    let prepared_txs = prepare_txs_with_hashes(txs);
    let mut flags = NOVOVM_ADAPTER_PLUGIN_APPLY_FLAG_MAINLINE_INGRESS_BYPASS_V1
        | NOVOVM_ADAPTER_PLUGIN_APPLY_FLAG_MAINLINE_RECEIPT_EXPORT_V1
        | NOVOVM_ADAPTER_PLUGIN_APPLY_FLAG_MAINLINE_RECEIPT_INGEST_V1;
    if atomic_guard_enabled {
        flags |= NOVOVM_ADAPTER_PLUGIN_APPLY_FLAG_ATOMIC_INTENT_GUARD_V1;
    }
    let tap_summary = runtime_tap_ir_batch_v1(chain_type, chain_id, &prepared_txs, flags)
        .map_err(|rc| anyhow::anyhow!("runtime tap failed: rc={rc}"))?;
    if tap_summary.accepted < prepared_txs.len() {
        let reason = tap_summary
            .primary_reject_reason
            .map(|value| value.as_str())
            .unwrap_or("rejected");
        anyhow::bail!(
            "mainline batch tap rejected txs: requested={} accepted={} dropped={} reason={}",
            tap_summary.requested,
            tap_summary.accepted,
            tap_summary.dropped,
            reason
        );
    }
    let artifacts = apply_ir_batch_with_artifacts(chain_type, chain_id, &prepared_txs, flags)?;
    Ok(SupervmEvmNativeSubmitReportV1 {
        chain_type,
        chain_id,
        tx_count: prepared_txs.len() as u64,
        tap_summary,
        apply_result: artifacts.result,
        exported_receipt_count: artifacts.execution_receipts.len() as u64,
        mirrored_receipt_count: artifacts
            .state_mirror_update
            .as_ref()
            .map(|update| update.receipt_count)
            .unwrap_or(0),
        state_version: artifacts.state_version,
        ingress_bypassed: true,
        atomic_guard_enabled,
    })
}

fn validate_and_verify_txs_for_apply_batch(
    adapter: &NovoVmAdapter,
    profile: &novovm_adapter_evm_core::EvmChainProfile,
    txs: &[TxIR],
) -> anyhow::Result<Vec<bool>> {
    if txs.is_empty() {
        return Ok(Vec::new());
    }
    let workers = std::thread::available_parallelism()
        .map(|v| v.get())
        .unwrap_or(1)
        .min(txs.len().max(1));
    if workers > 1 && txs.len() > 1 {
        let chunk_size = txs.len().div_ceil(workers);
        std::thread::scope(|scope| -> anyhow::Result<()> {
            let mut jobs = Vec::with_capacity(workers);
            for chunk in txs.chunks(chunk_size) {
                let profile = profile.clone();
                jobs.push(scope.spawn(move || -> anyhow::Result<()> {
                    for tx in chunk {
                        validate_tx_semantics_m0(&profile, tx)?;
                    }
                    Ok(())
                }));
            }
            for job in jobs {
                job.join().map_err(|_| {
                    anyhow::anyhow!("parallel transaction verification thread panicked")
                })??;
            }
            Ok(())
        })?;
        return adapter.verify_transactions_batch(txs);
    }
    for tx in txs {
        validate_tx_semantics_m0(profile, tx)?;
    }
    adapter.verify_transactions_batch(txs)
}

#[no_mangle]
pub extern "C" fn novovm_adapter_plugin_version() -> u32 {
    NOVOVM_ADAPTER_PLUGIN_ABI_V1
}

#[no_mangle]
pub extern "C" fn novovm_adapter_plugin_capabilities() -> u64 {
    NOVOVM_ADAPTER_PLUGIN_CAP_APPLY_IR_V1
        | NOVOVM_ADAPTER_PLUGIN_CAP_UA_SELF_GUARD_V1
        | NOVOVM_ADAPTER_PLUGIN_CAP_EVM_RUNTIME_V1
}

#[no_mangle]
/// Apply a serialized TxIR batch through the EVM adapter plugin (ABI v1).
///
/// # Safety
/// - `tx_ir_ptr` must be valid for reads of `tx_ir_len` bytes for the duration of this call.
/// - `out_result` must be a valid, writable pointer to `NovovmAdapterPluginApplyResultV1`.
/// - The memory behind `tx_ir_ptr` must contain a bincode-encoded `Vec<TxIR>`.
pub unsafe extern "C" fn novovm_adapter_plugin_apply_v1(
    chain_type_code: u32,
    chain_id: u64,
    tx_ir_ptr: *const u8,
    tx_ir_len: usize,
    out_result: *mut NovovmAdapterPluginApplyResultV1,
) -> i32 {
    if out_result.is_null() {
        return NOVOVM_ADAPTER_PLUGIN_RC_INVALID_ARG;
    }
    let (chain_type, txs) = match decode_plugin_apply_inputs(chain_type_code, tx_ir_ptr, tx_ir_len)
    {
        Ok(v) => v,
        Err(rc) => return rc,
    };
    let prepared_txs = prepare_txs_with_hashes(&txs);

    push_ingress_frames_prepared(chain_id, &prepared_txs);
    let atomic_intent = match prepare_atomic_intent_guard(chain_type, chain_id, &prepared_txs) {
        Ok(v) => v,
        Err(_) => return NOVOVM_ADAPTER_PLUGIN_RC_APPLY_FAILED,
    };
    let result = match apply_ir_batch(chain_type, chain_id, &prepared_txs) {
        Ok(v) => {
            if let Some(intent) = atomic_intent.as_ref() {
                mark_atomic_intent_executed(chain_id, intent);
            }
            settle_fee_income_for_batch(chain_id, &prepared_txs);
            v
        }
        Err(err) => {
            if let Some(intent) = atomic_intent.as_ref() {
                mark_atomic_intent_apply_failed(chain_id, &intent.intent_id, &err.to_string());
            }
            return NOVOVM_ADAPTER_PLUGIN_RC_APPLY_FAILED;
        }
    };

    *out_result = result;
    NOVOVM_ADAPTER_PLUGIN_RC_OK
}

#[no_mangle]
/// Apply a serialized TxIR batch through the EVM adapter plugin (ABI v2 with options).
///
/// # Safety
/// - `tx_ir_ptr` must be valid for reads of `tx_ir_len` bytes for the duration of this call.
/// - `options_ptr` may be null; when non-null it must point to a valid `NovovmAdapterPluginApplyOptionsV1`.
/// - `out_result` must be a valid, writable pointer to `NovovmAdapterPluginApplyResultV1`.
/// - The memory behind `tx_ir_ptr` must contain a bincode-encoded `Vec<TxIR>`.
pub unsafe extern "C" fn novovm_adapter_plugin_apply_v2(
    chain_type_code: u32,
    chain_id: u64,
    tx_ir_ptr: *const u8,
    tx_ir_len: usize,
    options_ptr: *const NovovmAdapterPluginApplyOptionsV1,
    out_result: *mut NovovmAdapterPluginApplyResultV1,
) -> i32 {
    if out_result.is_null() {
        return NOVOVM_ADAPTER_PLUGIN_RC_INVALID_ARG;
    }
    let (chain_type, txs) = match decode_plugin_apply_inputs(chain_type_code, tx_ir_ptr, tx_ir_len)
    {
        Ok(v) => v,
        Err(rc) => return rc,
    };
    let prepared_txs = prepare_txs_with_hashes(&txs);
    let flags = if options_ptr.is_null() {
        0
    } else {
        (*options_ptr).flags
    };
    if flags & NOVOVM_ADAPTER_PLUGIN_APPLY_FLAG_UA_SELF_GUARD_V1 != 0
        && route_txs_via_plugin_ua_self_guard(chain_id, &prepared_txs).is_err()
    {
        return NOVOVM_ADAPTER_PLUGIN_RC_UA_SELF_GUARD_FAILED;
    }
    let atomic_guard_requested =
        flags & NOVOVM_ADAPTER_PLUGIN_APPLY_FLAG_ATOMIC_INTENT_GUARD_V1 != 0;

    if flags & NOVOVM_ADAPTER_PLUGIN_APPLY_FLAG_MAINLINE_INGRESS_BYPASS_V1 == 0 {
        push_ingress_frames_prepared(chain_id, &prepared_txs);
    }
    let atomic_intent = if atomic_guard_requested {
        match prepare_atomic_intent_guard(chain_type, chain_id, &prepared_txs) {
            Ok(v) => v,
            Err(_) => return NOVOVM_ADAPTER_PLUGIN_RC_APPLY_FAILED,
        }
    } else {
        None
    };

    let result = match apply_ir_batch_with_artifacts(chain_type, chain_id, &prepared_txs, flags) {
        Ok(v) => {
            if let Some(intent) = atomic_intent.as_ref() {
                mark_atomic_intent_executed(chain_id, intent);
            }
            settle_fee_income_for_batch(chain_id, &prepared_txs);
            v.result
        }
        Err(err) => {
            if let Some(intent) = atomic_intent.as_ref() {
                mark_atomic_intent_apply_failed(chain_id, &intent.intent_id, &err.to_string());
            }
            return NOVOVM_ADAPTER_PLUGIN_RC_APPLY_FAILED;
        }
    };

    *out_result = result;
    NOVOVM_ADAPTER_PLUGIN_RC_OK
}

#[no_mangle]
/// Runtime-side tap for EVM mirror data-plane:
/// ingest TxIR batch into ingress / settlement / atomic-broadcast-ready queues
/// without executing chain state apply.
///
/// # Safety
/// - `tx_ir_ptr` must be valid for reads of `tx_ir_len` bytes for the duration of this call.
/// - The memory behind `tx_ir_ptr` must contain a bincode-encoded `Vec<TxIR>`.
pub unsafe extern "C" fn novovm_adapter_plugin_runtime_tap_v1(
    chain_type_code: u32,
    chain_id: u64,
    tx_ir_ptr: *const u8,
    tx_ir_len: usize,
    flags: u64,
) -> i32 {
    let (chain_type, txs) = match decode_plugin_apply_inputs(chain_type_code, tx_ir_ptr, tx_ir_len)
    {
        Ok(v) => v,
        Err(rc) => return rc,
    };
    match runtime_tap_ir_batch_v1(chain_type, chain_id, &txs, flags) {
        Ok(_) => NOVOVM_ADAPTER_PLUGIN_RC_OK,
        Err(rc) => rc,
    }
}

#[no_mangle]
/// Drain ingress frames and return bincode bytes through caller-provided buffer.
///
/// # Safety
/// - `out_len` must be a valid writable pointer.
/// - `out_ptr` may be null only when `out_cap == 0` (size-probe mode).
/// - When non-null, `out_ptr` must be writable for `out_cap` bytes.
pub unsafe extern "C" fn novovm_adapter_plugin_drain_ingress_bincode_v1(
    max_items: usize,
    out_ptr: *mut u8,
    out_cap: usize,
    out_len: *mut usize,
) -> i32 {
    let frames = drain_plugin_ingress_frames_for_host(max_items);
    let payload = match serialize_bincode_export_blob(&frames) {
        Ok(v) => v,
        Err(rc) => return rc,
    };
    write_bincode_blob_to_out(&payload, out_ptr, out_cap, out_len)
}

#[no_mangle]
/// Drain only nonce-contiguous executable ingress frames and return bincode bytes.
///
/// # Safety
/// - `out_len` must be a valid writable pointer.
/// - `out_ptr` may be null only when `out_cap == 0` (size-probe mode).
/// - When non-null, `out_ptr` must be writable for `out_cap` bytes.
pub unsafe extern "C" fn novovm_adapter_plugin_drain_executable_ingress_bincode_v1(
    max_items: usize,
    out_ptr: *mut u8,
    out_cap: usize,
    out_len: *mut usize,
) -> i32 {
    let frames = drain_executable_ingress_frames_for_host(max_items);
    let payload = match serialize_bincode_export_blob(&frames) {
        Ok(v) => v,
        Err(rc) => return rc,
    };
    write_bincode_blob_to_out(&payload, out_ptr, out_cap, out_len)
}

#[no_mangle]
/// Drain only pending (non-executable) ingress frames and return bincode bytes.
///
/// # Safety
/// - `out_len` must be a valid writable pointer.
/// - `out_ptr` may be null only when `out_cap == 0` (size-probe mode).
/// - When non-null, `out_ptr` must be writable for `out_cap` bytes.
pub unsafe extern "C" fn novovm_adapter_plugin_drain_pending_ingress_bincode_v1(
    max_items: usize,
    out_ptr: *mut u8,
    out_cap: usize,
    out_len: *mut usize,
) -> i32 {
    let frames = drain_pending_ingress_frames_for_host(max_items);
    let payload = match serialize_bincode_export_blob(&frames) {
        Ok(v) => v,
        Err(rc) => return rc,
    };
    write_bincode_blob_to_out(&payload, out_ptr, out_cap, out_len)
}

#[no_mangle]
/// Snapshot pending txs grouped by sender and return bincode bytes.
///
/// # Safety
/// - `out_len` must be a valid writable pointer.
/// - `out_ptr` may be null only when `out_cap == 0` (size-probe mode).
/// - When non-null, `out_ptr` must be writable for `out_cap` bytes.
pub unsafe extern "C" fn novovm_adapter_plugin_snapshot_pending_sender_buckets_bincode_v1(
    max_senders: usize,
    max_txs_per_sender: usize,
    out_ptr: *mut u8,
    out_cap: usize,
    out_len: *mut usize,
) -> i32 {
    let buckets = snapshot_pending_sender_buckets_for_host(max_senders, max_txs_per_sender);
    let payload = match serialize_bincode_export_blob(&buckets) {
        Ok(v) => v,
        Err(rc) => return rc,
    };
    write_bincode_blob_to_out(&payload, out_ptr, out_cap, out_len)
}

#[no_mangle]
/// Drain atomic intent receipts and return bincode bytes through caller-provided buffer.
///
/// # Safety
/// - `out_len` must be a valid writable pointer.
/// - `out_ptr` may be null only when `out_cap == 0` (size-probe mode).
/// - When non-null, `out_ptr` must be writable for `out_cap` bytes.
pub unsafe extern "C" fn novovm_adapter_plugin_drain_atomic_receipts_bincode_v1(
    max_items: usize,
    out_ptr: *mut u8,
    out_cap: usize,
    out_len: *mut usize,
) -> i32 {
    let receipts = drain_atomic_receipts_for_host(max_items);
    let payload = match serialize_bincode_export_blob(&receipts) {
        Ok(v) => v,
        Err(rc) => return rc,
    };
    write_bincode_blob_to_out(&payload, out_ptr, out_cap, out_len)
}

#[no_mangle]
/// Drain fee-settlement records and return bincode bytes through caller-provided buffer.
///
/// # Safety
/// - `out_len` must be a valid writable pointer.
/// - `out_ptr` may be null only when `out_cap == 0` (size-probe mode).
/// - When non-null, `out_ptr` must be writable for `out_cap` bytes.
pub unsafe extern "C" fn novovm_adapter_plugin_drain_settlement_records_bincode_v1(
    max_items: usize,
    out_ptr: *mut u8,
    out_cap: usize,
    out_len: *mut usize,
) -> i32 {
    let records = drain_settlement_records_for_host(max_items);
    let payload = match serialize_bincode_export_blob(&records) {
        Ok(v) => v,
        Err(rc) => return rc,
    };
    write_bincode_blob_to_out(&payload, out_ptr, out_cap, out_len)
}

#[no_mangle]
/// Drain payout instructions generated from settlement records.
///
/// # Safety
/// - `out_len` must be a valid writable pointer.
/// - `out_ptr` may be null only when `out_cap == 0` (size-probe mode).
/// - When non-null, `out_ptr` must be writable for `out_cap` bytes.
pub unsafe extern "C" fn novovm_adapter_plugin_drain_payout_instructions_bincode_v1(
    max_items: usize,
    out_ptr: *mut u8,
    out_cap: usize,
    out_len: *mut usize,
) -> i32 {
    let records = drain_payout_instructions_for_host(max_items);
    let payload = match serialize_bincode_export_blob(&records) {
        Ok(v) => v,
        Err(rc) => return rc,
    };
    write_bincode_blob_to_out(&payload, out_ptr, out_cap, out_len)
}

#[no_mangle]
/// Drain atomic intents that passed local checks and are ready for broadcast.
///
/// # Safety
/// - `out_len` must be a valid writable pointer.
/// - `out_ptr` may be null only when `out_cap == 0` (size-probe mode).
/// - When non-null, `out_ptr` must be writable for `out_cap` bytes.
pub unsafe extern "C" fn novovm_adapter_plugin_drain_atomic_broadcast_ready_bincode_v1(
    max_items: usize,
    out_ptr: *mut u8,
    out_cap: usize,
    out_len: *mut usize,
) -> i32 {
    let items = drain_atomic_broadcast_ready_for_host(max_items);
    let payload = match serialize_bincode_export_blob(&items) {
        Ok(v) => v,
        Err(rc) => return rc,
    };
    write_bincode_blob_to_out(&payload, out_ptr, out_cap, out_len)
}

#[no_mangle]
/// Export current fee-settlement snapshot as bincode bytes.
///
/// # Safety
/// - `out_len` must be a valid writable pointer.
/// - `out_ptr` may be null only when `out_cap == 0` (size-probe mode).
/// - When non-null, `out_ptr` must be writable for `out_cap` bytes.
pub unsafe extern "C" fn novovm_adapter_plugin_get_settlement_snapshot_bincode_v1(
    out_ptr: *mut u8,
    out_cap: usize,
    out_len: *mut usize,
) -> i32 {
    let snapshot = settlement_snapshot_for_host();
    let payload = match serialize_bincode_export_blob(&snapshot) {
        Ok(v) => v,
        Err(rc) => return rc,
    };
    write_bincode_blob_to_out(&payload, out_ptr, out_cap, out_len)
}

#[no_mangle]
/// Drain execution receipts and return bincode bytes through caller-provided buffer.
///
/// # Safety
/// - `out_len` must be a valid writable pointer.
/// - `out_ptr` may be null only when `out_cap == 0` (size-probe mode).
/// - When non-null, `out_ptr` must be writable for `out_cap` bytes.
pub unsafe extern "C" fn novovm_adapter_plugin_drain_execution_receipts_bincode_v1(
    max_items: usize,
    out_ptr: *mut u8,
    out_cap: usize,
    out_len: *mut usize,
) -> i32 {
    let items = drain_execution_receipts_for_host(max_items);
    let payload = match serialize_bincode_export_blob(&items) {
        Ok(v) => v,
        Err(rc) => return rc,
    };
    write_bincode_blob_to_out(&payload, out_ptr, out_cap, out_len)
}

#[no_mangle]
/// Drain state mirror updates and return bincode bytes through caller-provided buffer.
///
/// # Safety
/// - `out_len` must be a valid writable pointer.
/// - `out_ptr` may be null only when `out_cap == 0` (size-probe mode).
/// - When non-null, `out_ptr` must be writable for `out_cap` bytes.
pub unsafe extern "C" fn novovm_adapter_plugin_drain_state_mirror_updates_bincode_v1(
    max_items: usize,
    out_ptr: *mut u8,
    out_cap: usize,
    out_len: *mut usize,
) -> i32 {
    let items = drain_state_mirror_updates_for_host(max_items);
    let payload = match serialize_bincode_export_blob(&items) {
        Ok(v) => v,
        Err(rc) => return rc,
    };
    write_bincode_blob_to_out(&payload, out_ptr, out_cap, out_len)
}

#[no_mangle]
/// Ingest serialized execution receipts and emit a state mirror update report.
///
/// # Safety
/// - `payload_ptr` must be valid for reads of `payload_len` bytes.
/// - `out_len` must be a valid writable pointer.
/// - `out_ptr` may be null only when `out_cap == 0` (size-probe mode).
/// - When non-null, `out_ptr` must be writable for `out_cap` bytes.
pub unsafe extern "C" fn novovm_adapter_plugin_ingest_execution_receipts_bincode_v1(
    chain_id: u64,
    state_version: u64,
    payload_ptr: *const u8,
    payload_len: usize,
    out_ptr: *mut u8,
    out_cap: usize,
    out_len: *mut usize,
) -> i32 {
    if payload_ptr.is_null() || payload_len == 0 {
        return NOVOVM_ADAPTER_PLUGIN_RC_INVALID_ARG;
    }
    if payload_len > MAX_PLUGIN_TX_IR_BYTES || payload_len > isize::MAX as usize {
        return NOVOVM_ADAPTER_PLUGIN_RC_PAYLOAD_TOO_LARGE;
    }
    let payload = std::slice::from_raw_parts(payload_ptr, payload_len);
    let receipts: Vec<SupervmEvmExecutionReceiptV1> =
        match crate::bincode_compat::deserialize(payload) {
            Ok(v) => v,
            Err(_) => return NOVOVM_ADAPTER_PLUGIN_RC_APPLY_FAILED,
        };
    let update = match preview_execution_receipts_ingest_for_host(
        chain_id,
        state_version,
        receipts.as_slice(),
    ) {
        Ok(v) => v,
        Err(_) => return NOVOVM_ADAPTER_PLUGIN_RC_APPLY_FAILED,
    };
    let encoded = match serialize_bincode_export_blob(&update) {
        Ok(v) => v,
        Err(rc) => return rc,
    };
    let rc = write_bincode_blob_to_out(&encoded, out_ptr, out_cap, out_len);
    if rc != NOVOVM_ADAPTER_PLUGIN_RC_OK {
        return rc;
    }
    if out_ptr.is_null() {
        return NOVOVM_ADAPTER_PLUGIN_RC_OK;
    }
    match ingest_execution_receipts_for_host(chain_id, state_version, receipts.as_slice()) {
        Ok(_) => NOVOVM_ADAPTER_PLUGIN_RC_OK,
        Err(_) => NOVOVM_ADAPTER_PLUGIN_RC_APPLY_FAILED,
    }
}

#[no_mangle]
/// Submit an internal mainline batch through the same-process execution bridge.
///
/// # Safety
/// - `tx_ir_ptr` must be valid for reads of `tx_ir_len` bytes for the duration of this call.
/// - `out_len` must be a valid writable pointer.
/// - `out_ptr` may be null only when `out_cap == 0` (size-probe mode).
/// - When non-null, `out_ptr` must be writable for `out_cap` bytes.
pub unsafe extern "C" fn novovm_adapter_plugin_submit_internal_batch_bincode_v1(
    chain_type_code: u32,
    chain_id: u64,
    tx_ir_ptr: *const u8,
    tx_ir_len: usize,
    atomic_guard_enabled: u8,
    out_ptr: *mut u8,
    out_cap: usize,
    out_len: *mut usize,
) -> i32 {
    let (chain_type, txs) = match decode_plugin_apply_inputs(chain_type_code, tx_ir_ptr, tx_ir_len)
    {
        Ok(v) => v,
        Err(rc) => return rc,
    };
    let report = match submit_internal_batch_to_mainline_v1(
        chain_type,
        chain_id,
        &txs,
        atomic_guard_enabled != 0,
    ) {
        Ok(v) => v,
        Err(_) => return NOVOVM_ADAPTER_PLUGIN_RC_APPLY_FAILED,
    };
    let payload = match serialize_bincode_export_blob(&report) {
        Ok(v) => v,
        Err(rc) => return rc,
    };
    write_bincode_blob_to_out(&payload, out_ptr, out_cap, out_len)
}

#[cfg(test)]
mod tests {
    use super::*;
    use novovm_adapter_novovm::{address_from_seed_v1, signature_payload_with_seed_v1};
    use std::sync::{Mutex, MutexGuard, OnceLock};
    const TEST_SIGN_SEED: [u8; 32] = [13u8; 32];

    fn runtime_queue_test_guard() -> MutexGuard<'static, ()> {
        static LOCK: OnceLock<Mutex<()>> = OnceLock::new();
        LOCK.get_or_init(|| Mutex::new(()))
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
    }

    fn reset_runtime_queues_for_test() {
        EVM_RUNTIME_SETTLEMENT_SEQ.store(0, Ordering::Relaxed);
        EVM_EXECUTION_STATE_VERSION.store(0, Ordering::Relaxed);
        for shard in resolve_evm_runtime_shards() {
            let mut runtime = shard
                .lock()
                .unwrap_or_else(|poisoned| poisoned.into_inner());
            runtime.settlement_seq = 0;
            runtime.reserve_total_wei = 0;
            runtime.payout_total_units = 0;
            runtime.atomic_receipts.clear();
            runtime.settlement_records.clear();
            runtime.payout_instructions.clear();
            runtime.atomic_broadcast_ready.clear();
            runtime.execution_receipts.clear();
            runtime.state_mirror_updates.clear();
        }
        for shard in resolve_evm_txpool_shards() {
            let mut txpool = shard
                .lock()
                .unwrap_or_else(|poisoned| poisoned.into_inner());
            txpool.ingress_frames.clear();
            txpool.executable_ingress_frames.clear();
            txpool.pending_by_nonce.clear();
            txpool.pending_by_sender.clear();
            txpool.next_nonce_by_sender.clear();
        }
        if let Some(runtime) = UA_PLUGIN_RUNTIME.get() {
            let mut runtime = runtime
                .lock()
                .unwrap_or_else(|poisoned| poisoned.into_inner());
            *runtime = UaPluginRuntime::default();
        }
        let config = resolve_ua_plugin_standalone_config();
        remove_path_if_present(&config.store_path);
        remove_path_if_present(&config.audit_path);
    }

    fn encode_address(seed: u64) -> Vec<u8> {
        let mut out = vec![0u8; 20];
        out[12..20].copy_from_slice(&seed.to_be_bytes());
        out
    }

    fn sample_tx(chain_id: u64, nonce: u64) -> TxIR {
        let mut tx = TxIR {
            hash: Vec::new(),
            from: address_from_seed_v1(TEST_SIGN_SEED),
            to: Some(encode_address(2000)),
            value: 5,
            gas_limit: 21_000,
            gas_price: 1,
            nonce,
            data: Vec::new(),
            signature: Vec::new(),
            chain_id,
            tx_type: TxType::Transfer,
            source_chain: None,
            target_chain: None,
        };
        tx.compute_hash();
        tx.signature = signature_payload_with_seed_v1(&tx, TEST_SIGN_SEED);
        tx
    }

    fn sample_contract_call_tx(chain_id: u64, nonce: u64) -> TxIR {
        let mut tx = sample_tx(chain_id, nonce);
        tx.tx_type = TxType::ContractCall;
        tx.data = vec![0xab, 0xcd, 0xef];
        tx.gas_limit = 24_000;
        tx.compute_hash();
        tx.signature = signature_payload_with_seed_v1(&tx, TEST_SIGN_SEED);
        tx
    }

    fn sample_contract_deploy_tx(chain_id: u64, nonce: u64) -> TxIR {
        let mut tx = sample_tx(chain_id, nonce);
        tx.tx_type = TxType::ContractDeploy;
        tx.to = None;
        tx.data = vec![0x60, 0x00, 0x60, 0x00];
        tx.gas_limit = 60_000;
        tx.compute_hash();
        tx.signature = signature_payload_with_seed_v1(&tx, TEST_SIGN_SEED);
        tx
    }

    fn resign_tx(mut tx: TxIR) -> TxIR {
        tx.compute_hash();
        tx.signature = signature_payload_with_seed_v1(&tx, TEST_SIGN_SEED);
        tx
    }

    fn with_gas_price(mut tx: TxIR, gas_price: u64) -> TxIR {
        tx.gas_price = gas_price;
        resign_tx(tx)
    }

    #[test]
    fn apply_ir_batch_smoke_for_evm_chain() {
        let txs = vec![sample_tx(1, 0), sample_tx(1, 1)];
        let result = apply_ir_batch(ChainType::EVM, 1, &txs).expect("apply should pass");
        assert_eq!(result.verified, 1);
        assert_eq!(result.applied, 1);
        assert_eq!(result.txs, 2);
        assert!(result.accounts >= 2);
    }

    #[test]
    fn apply_ir_batch_rejects_intrinsic_gas_too_low() {
        let mut tx = sample_tx(1, 0);
        tx.gas_limit = 20_999;
        let err = apply_ir_batch(ChainType::EVM, 1, &[tx]).expect_err("must reject low gas");
        assert!(err.to_string().contains("intrinsic gas too low"));
    }

    #[test]
    fn apply_ir_batch_accepts_contract_call_and_deploy() {
        let txs = vec![
            sample_contract_call_tx(1, 0),
            sample_contract_deploy_tx(1, 1),
        ];
        let result = apply_ir_batch(ChainType::EVM, 1, &txs).expect("apply should pass");
        assert_eq!(result.verified, 1);
        assert_eq!(result.applied, 1);
        assert_eq!(result.txs, 2);
        assert!(result.accounts >= 2);
    }

    #[test]
    fn chain_code_mapping_supports_only_evm_family() {
        assert_eq!(chain_type_from_code(1), Some(ChainType::EVM));
        assert_eq!(chain_type_from_code(5), Some(ChainType::Polygon));
        assert_eq!(chain_type_from_code(6), Some(ChainType::BNB));
        assert_eq!(chain_type_from_code(7), Some(ChainType::Avalanche));
        assert_eq!(chain_type_from_code(0), None);
        assert_eq!(chain_type_from_code(13), None);
    }

    #[test]
    fn plugin_capabilities_include_ua_self_guard_contract_bit() {
        let caps = novovm_adapter_plugin_capabilities();
        assert!(caps & NOVOVM_ADAPTER_PLUGIN_CAP_APPLY_IR_V1 != 0);
        assert!(caps & NOVOVM_ADAPTER_PLUGIN_CAP_UA_SELF_GUARD_V1 != 0);
        assert!(caps & NOVOVM_ADAPTER_PLUGIN_CAP_EVM_RUNTIME_V1 != 0);
    }

    #[test]
    fn plugin_return_codes_are_stable_contract() {
        assert_eq!(NOVOVM_ADAPTER_PLUGIN_RC_OK, 0);
        assert_eq!(NOVOVM_ADAPTER_PLUGIN_RC_INVALID_ARG, -1);
        assert_eq!(NOVOVM_ADAPTER_PLUGIN_RC_UNSUPPORTED_CHAIN, -2);
        assert_eq!(NOVOVM_ADAPTER_PLUGIN_RC_DECODE_FAILED, -3);
        assert_eq!(NOVOVM_ADAPTER_PLUGIN_RC_EMPTY_BATCH, -4);
        assert_eq!(NOVOVM_ADAPTER_PLUGIN_RC_UNSUPPORTED_TX_TYPE, -5);
        assert_eq!(NOVOVM_ADAPTER_PLUGIN_RC_APPLY_FAILED, -6);
        assert_eq!(NOVOVM_ADAPTER_PLUGIN_RC_UA_SELF_GUARD_FAILED, -7);
        assert_eq!(NOVOVM_ADAPTER_PLUGIN_RC_PAYLOAD_TOO_LARGE, -8);
        assert_eq!(NOVOVM_ADAPTER_PLUGIN_RC_BATCH_TOO_LARGE, -9);
        assert_eq!(NOVOVM_ADAPTER_PLUGIN_RC_BUFFER_TOO_SMALL, -10);
    }

    #[test]
    fn plugin_apply_v1_rejects_invalid_chain_type() {
        let txs = vec![sample_tx(1, 0)];
        let tx_bytes = crate::bincode_compat::serialize(&txs).expect("tx encode");
        let mut out = NovovmAdapterPluginApplyResultV1::default();
        let rc = unsafe {
            novovm_adapter_plugin_apply_v1(
                0,
                1,
                tx_bytes.as_ptr(),
                tx_bytes.len(),
                &mut out as *mut NovovmAdapterPluginApplyResultV1,
            )
        };
        assert_eq!(rc, NOVOVM_ADAPTER_PLUGIN_RC_UNSUPPORTED_CHAIN);
    }

    #[test]
    fn plugin_apply_v1_rejects_malformed_payload() {
        let payload = [1u8, 2u8, 3u8];
        let mut out = NovovmAdapterPluginApplyResultV1::default();
        let rc = unsafe {
            novovm_adapter_plugin_apply_v1(
                1,
                1,
                payload.as_ptr(),
                payload.len(),
                &mut out as *mut NovovmAdapterPluginApplyResultV1,
            )
        };
        assert_eq!(rc, NOVOVM_ADAPTER_PLUGIN_RC_DECODE_FAILED);
    }

    #[test]
    fn plugin_apply_v1_rejects_empty_batch() {
        let txs: Vec<TxIR> = Vec::new();
        let tx_bytes = crate::bincode_compat::serialize(&txs).expect("tx encode");
        let mut out = NovovmAdapterPluginApplyResultV1::default();
        let rc = unsafe {
            novovm_adapter_plugin_apply_v1(
                1,
                1,
                tx_bytes.as_ptr(),
                tx_bytes.len(),
                &mut out as *mut NovovmAdapterPluginApplyResultV1,
            )
        };
        assert_eq!(rc, NOVOVM_ADAPTER_PLUGIN_RC_EMPTY_BATCH);
    }

    #[test]
    fn plugin_apply_v1_rejects_unsupported_tx_type() {
        let mut tx = sample_tx(1, 0);
        tx.tx_type = TxType::Privacy;
        let tx_bytes = crate::bincode_compat::serialize(&vec![tx]).expect("tx encode");
        let mut out = NovovmAdapterPluginApplyResultV1::default();
        let rc = unsafe {
            novovm_adapter_plugin_apply_v1(
                1,
                1,
                tx_bytes.as_ptr(),
                tx_bytes.len(),
                &mut out as *mut NovovmAdapterPluginApplyResultV1,
            )
        };
        assert_eq!(rc, NOVOVM_ADAPTER_PLUGIN_RC_UNSUPPORTED_TX_TYPE);
    }

    #[test]
    fn plugin_apply_v1_maps_engine_failure_to_apply_failed() {
        let mut tx = sample_tx(1, 0);
        tx.gas_limit = 20_999;
        let tx_bytes = crate::bincode_compat::serialize(&vec![tx]).expect("tx encode");
        let mut out = NovovmAdapterPluginApplyResultV1::default();
        let rc = unsafe {
            novovm_adapter_plugin_apply_v1(
                1,
                1,
                tx_bytes.as_ptr(),
                tx_bytes.len(),
                &mut out as *mut NovovmAdapterPluginApplyResultV1,
            )
        };
        assert_eq!(rc, NOVOVM_ADAPTER_PLUGIN_RC_APPLY_FAILED);
    }

    #[test]
    fn plugin_apply_v2_self_guard_rejects_replay_nonce() {
        let _guard = runtime_queue_test_guard();
        reset_runtime_queues_for_test();
        let txs = vec![sample_tx(1, 0)];
        let tx_bytes = crate::bincode_compat::serialize(&txs).expect("tx encode");
        let options = NovovmAdapterPluginApplyOptionsV1 {
            flags: NOVOVM_ADAPTER_PLUGIN_APPLY_FLAG_UA_SELF_GUARD_V1,
        };
        let mut out = NovovmAdapterPluginApplyResultV1::default();

        let rc_first = unsafe {
            novovm_adapter_plugin_apply_v2(
                1,
                1,
                tx_bytes.as_ptr(),
                tx_bytes.len(),
                &options as *const NovovmAdapterPluginApplyOptionsV1,
                &mut out as *mut NovovmAdapterPluginApplyResultV1,
            )
        };
        assert_eq!(rc_first, NOVOVM_ADAPTER_PLUGIN_RC_OK);

        let rc_second = unsafe {
            novovm_adapter_plugin_apply_v2(
                1,
                1,
                tx_bytes.as_ptr(),
                tx_bytes.len(),
                &options as *const NovovmAdapterPluginApplyOptionsV1,
                &mut out as *mut NovovmAdapterPluginApplyResultV1,
            )
        };
        assert_eq!(rc_second, NOVOVM_ADAPTER_PLUGIN_RC_UA_SELF_GUARD_FAILED);
    }

    #[test]
    fn plugin_apply_v1_populates_ingress_and_settlement_totals() {
        let _guard = runtime_queue_test_guard();
        reset_runtime_queues_for_test();
        let (reserve_before, payout_before) = plugin_settlement_totals_for_host();
        let txs = vec![sample_tx(1, 0), sample_contract_call_tx(1, 1)];
        let tx_bytes = crate::bincode_compat::serialize(&txs).expect("tx encode");
        let mut out = NovovmAdapterPluginApplyResultV1::default();
        let rc = unsafe {
            novovm_adapter_plugin_apply_v1(
                1,
                1,
                tx_bytes.as_ptr(),
                tx_bytes.len(),
                &mut out as *mut NovovmAdapterPluginApplyResultV1,
            )
        };
        assert_eq!(rc, NOVOVM_ADAPTER_PLUGIN_RC_OK);
        let frames = drain_plugin_ingress_frames_for_host(16);
        assert!(frames.len() >= 2);
        assert!(frames.iter().all(|f| f.chain_id == 1));
        let records = drain_settlement_records_for_host(16);
        assert!(records.len() >= 2);
        assert!(records.iter().all(|r| r.income.chain_id == 1));
        let payout_instructions = drain_payout_instructions_for_host(16);
        assert!(payout_instructions.len() >= 2);
        assert!(payout_instructions.iter().all(|item| item.chain_id == 1));
        assert!(payout_instructions
            .iter()
            .all(|item| !item.settlement_id.is_empty()));
        let expected_fee = 21_000u128 + 24_000u128;
        let (reserve_after, payout_after) = plugin_settlement_totals_for_host();
        assert!(reserve_after >= reserve_before.saturating_add(expected_fee));
        assert!(payout_after >= payout_before.saturating_add(expected_fee));
    }

    #[test]
    fn ingress_txpool_replacement_requires_price_bump() {
        let _guard = runtime_queue_test_guard();
        reset_runtime_queues_for_test();

        let from = address_from_seed_v1(TEST_SIGN_SEED);
        let nonce = 77u64;
        let cfg = resolve_evm_runtime_config();
        let base_price = 100u64;
        let required = min_replacement_gas_price(base_price, cfg.txpool_price_bump_pct);
        let below_required = required.saturating_sub(1);

        let low = with_gas_price(sample_tx(1, nonce), base_price);
        let mut not_enough = with_gas_price(sample_tx(1, nonce), below_required);
        not_enough.from = from.clone();
        not_enough = resign_tx(not_enough);

        push_ingress_frames(1, std::slice::from_ref(&low));
        push_ingress_frames(1, std::slice::from_ref(&not_enough));
        let frames = drain_plugin_ingress_frames_for_host(16);
        let matched: Vec<_> = frames
            .iter()
            .filter(|frame| {
                frame
                    .parsed_tx
                    .as_ref()
                    .map(|tx| tx.from == from && tx.nonce == nonce)
                    .unwrap_or(false)
            })
            .collect();
        assert_eq!(matched.len(), 1);
        let gas_price = matched[0]
            .parsed_tx
            .as_ref()
            .map(|tx| tx.gas_price)
            .unwrap_or_default();
        assert_eq!(gas_price, base_price);
    }

    #[test]
    fn ingress_txpool_replacement_accepts_required_price_bump() {
        let _guard = runtime_queue_test_guard();
        reset_runtime_queues_for_test();

        let from = address_from_seed_v1(TEST_SIGN_SEED);
        let nonce = 88u64;
        let cfg = resolve_evm_runtime_config();
        let base_price = 100u64;
        let required = min_replacement_gas_price(base_price, cfg.txpool_price_bump_pct);

        let low = with_gas_price(sample_tx(1, nonce), base_price);
        let mut replacement = with_gas_price(sample_tx(1, nonce), required);
        replacement.from = from.clone();
        replacement = resign_tx(replacement);

        push_ingress_frames(1, &[low]);
        push_ingress_frames(1, &[replacement.clone()]);
        let frames = drain_plugin_ingress_frames_for_host(16);
        let matched: Vec<_> = frames
            .iter()
            .filter(|frame| {
                frame
                    .parsed_tx
                    .as_ref()
                    .map(|tx| tx.from == from && tx.nonce == nonce)
                    .unwrap_or(false)
            })
            .collect();
        assert_eq!(matched.len(), 1);
        let gas_price = matched[0]
            .parsed_tx
            .as_ref()
            .map(|tx| tx.gas_price)
            .unwrap_or_default();
        assert_eq!(gas_price, required);
    }

    #[test]
    fn ingress_txpool_duplicate_tx_is_idempotent_accepted() {
        let _guard = runtime_queue_test_guard();
        reset_runtime_queues_for_test();

        let tx = sample_tx(1, 99);
        let first = runtime_tap_ir_batch_v1(ChainType::EVM, 1, std::slice::from_ref(&tx), 0)
            .expect("first tap ok");
        assert_eq!(first.requested, 1);
        assert_eq!(first.accepted, 1);
        assert_eq!(first.dropped, 0);

        let second = runtime_tap_ir_batch_v1(ChainType::EVM, 1, std::slice::from_ref(&tx), 0)
            .expect("second tap ok");
        assert_eq!(second.requested, 1);
        assert_eq!(second.accepted, 1);
        assert_eq!(second.dropped, 0);
        assert!(second.reject_reasons.is_empty());
        assert_eq!(second.primary_reject_reason, None);

        let frames = drain_plugin_ingress_frames_for_host(16);
        let same_hash_count = frames
            .iter()
            .filter(|frame| frame.tx_hash == tx.hash)
            .count();
        assert_eq!(same_hash_count, 1);
    }

    #[test]
    fn ingress_txpool_rejects_nonce_gap_beyond_threshold() {
        let _guard = runtime_queue_test_guard();
        reset_runtime_queues_for_test();

        let from = address_from_seed_v1(TEST_SIGN_SEED);
        let base = sample_tx(1, 0);
        let mut gap_tx = sample_tx(1, 2_000);
        gap_tx.from = from.clone();
        gap_tx = resign_tx(gap_tx);

        push_ingress_frames(1, &[base, gap_tx]);
        let frames = drain_plugin_ingress_frames_for_host(16);
        let mut nonces: Vec<u64> = frames
            .iter()
            .filter_map(|frame| {
                frame
                    .parsed_tx
                    .as_ref()
                    .filter(|tx| tx.from == from)
                    .map(|tx| tx.nonce)
            })
            .collect();
        nonces.sort_unstable();
        assert_eq!(nonces, vec![0]);
    }

    #[test]
    fn ingress_txpool_enforces_per_sender_pending_cap() {
        let _guard = runtime_queue_test_guard();
        reset_runtime_queues_for_test();

        let from = address_from_seed_v1(TEST_SIGN_SEED);
        let cfg = resolve_evm_runtime_config();
        let cap = cfg.txpool_max_pending_per_sender;
        let first = {
            let mut tx = sample_tx(1, 0);
            tx.from = from.clone();
            resign_tx(tx)
        };
        push_ingress_frames(1, &[first]);
        let _ = drain_executable_ingress_frames_for_host(16);

        let total = cap + 8;
        let mut batch = Vec::with_capacity(total);
        for offset in 0..(total as u64) {
            let nonce = 100 + offset;
            let mut tx = sample_tx(1, nonce);
            tx.from = from.clone();
            batch.push(resign_tx(tx));
        }
        push_ingress_frames(1, &batch);
        let frames = drain_plugin_ingress_frames_for_host(total + 32);
        let mut nonces: Vec<u64> = frames
            .iter()
            .filter_map(|frame| {
                frame
                    .parsed_tx
                    .as_ref()
                    .filter(|tx| tx.from == from)
                    .map(|tx| tx.nonce)
            })
            .collect();
        nonces.sort_unstable();
        assert_eq!(nonces.len(), cap);
        assert_eq!(nonces.first().copied().unwrap_or_default(), 100);
        assert_eq!(
            nonces.last().copied().unwrap_or_default(),
            (100 + cap - 1) as u64
        );
    }

    #[test]
    fn ingress_executable_queue_only_emits_contiguous_nonce_sequence() {
        let _guard = runtime_queue_test_guard();
        reset_runtime_queues_for_test();

        let from = address_from_seed_v1(TEST_SIGN_SEED);
        let mut tx0 = sample_tx(1, 0);
        tx0.from = from.clone();
        let mut tx2 = sample_tx(1, 2);
        tx2.from = from.clone();
        push_ingress_frames(1, &[resign_tx(tx0), resign_tx(tx2)]);

        let executable = drain_executable_ingress_frames_for_host(16);
        let executable_nonces: Vec<u64> = executable
            .iter()
            .filter_map(|frame| frame.parsed_tx.as_ref().map(|tx| tx.nonce))
            .collect();
        assert_eq!(executable_nonces, vec![0]);

        let pending = drain_plugin_ingress_frames_for_host(16);
        let pending_nonces: Vec<u64> = pending
            .iter()
            .filter_map(|frame| frame.parsed_tx.as_ref().map(|tx| tx.nonce))
            .collect();
        assert_eq!(pending_nonces, vec![2]);
    }

    #[test]
    fn ingress_pending_drain_and_sender_bucket_snapshot_are_explicit() {
        let _guard = runtime_queue_test_guard();
        reset_runtime_queues_for_test();

        let sender = address_from_seed_v1(TEST_SIGN_SEED);
        let mut seed = sample_tx(1, 0);
        seed.from = sender.clone();
        push_ingress_frames(1, &[resign_tx(seed)]);
        let _ = drain_executable_ingress_frames_for_host(16);

        let mut tx2 = sample_tx(1, 2);
        tx2.from = sender.clone();
        let mut tx3 = sample_tx(1, 3);
        tx3.from = sender.clone();
        push_ingress_frames(1, &[resign_tx(tx2), resign_tx(tx3)]);

        let executable = drain_executable_ingress_frames_for_host(16);
        assert!(executable.is_empty());

        let buckets = snapshot_pending_sender_buckets_for_host(8, 8);
        assert_eq!(buckets.len(), 1);
        assert_eq!(buckets[0].sender, sender);
        let bucket_nonces: Vec<u64> = buckets[0].txs.iter().map(|tx| tx.nonce).collect();
        assert_eq!(bucket_nonces, vec![2, 3]);

        let pending = drain_pending_ingress_frames_for_host(16);
        let pending_nonces: Vec<u64> = pending
            .iter()
            .filter_map(|frame| frame.parsed_tx.as_ref().map(|tx| tx.nonce))
            .collect();
        assert_eq!(pending_nonces, vec![2, 3]);
    }

    #[test]
    fn evict_ingress_tx_hashes_removes_executable_and_pending_frames() {
        let _guard = runtime_queue_test_guard();
        reset_runtime_queues_for_test();

        let sender = address_from_seed_v1(TEST_SIGN_SEED);
        let mut tx0 = sample_tx(1, 0);
        tx0.from = sender.clone();
        let mut tx2 = sample_tx(1, 2);
        tx2.from = sender.clone();
        push_ingress_frames(1, &[resign_tx(tx0), resign_tx(tx2)]);

        let executable = snapshot_executable_ingress_frames_for_host(16);
        let pending = snapshot_pending_ingress_frames_for_host(16);
        assert_eq!(executable.len(), 1);
        assert_eq!(pending.len(), 1);

        let mut hashes = Vec::<[u8; 32]>::new();
        for frame in executable.into_iter().chain(pending) {
            if frame.tx_hash.len() != 32 {
                continue;
            }
            let mut hash = [0u8; 32];
            hash.copy_from_slice(frame.tx_hash.as_slice());
            hashes.push(hash);
        }
        let removed = evict_ingress_tx_hashes_for_host(1, hashes.as_slice());
        assert_eq!(removed, 2);
        assert!(snapshot_executable_ingress_frames_for_host(16).is_empty());
        assert!(snapshot_pending_ingress_frames_for_host(16).is_empty());
    }

    #[test]
    fn evict_stale_ingress_frames_removes_old_runtime_frames() {
        let _guard = runtime_queue_test_guard();
        reset_runtime_queues_for_test();

        let sender = address_from_seed_v1(TEST_SIGN_SEED);
        let mut tx0 = sample_tx(1, 0);
        tx0.from = sender.clone();
        let mut tx2 = sample_tx(1, 2);
        tx2.from = sender;
        push_ingress_frames(1, &[resign_tx(tx0), resign_tx(tx2)]);

        let removed = evict_stale_ingress_frames_for_host(1, u64::MAX);
        assert_eq!(removed, 2);
        assert!(snapshot_executable_ingress_frames_for_host(16).is_empty());
        assert!(snapshot_pending_ingress_frames_for_host(16).is_empty());
    }

    #[test]
    fn ingress_drain_uses_sender_round_robin_for_pending_queue() {
        let _guard = runtime_queue_test_guard();
        reset_runtime_queues_for_test();

        let sender_a = encode_address(100);
        let sender_b = encode_address(200);

        let mut a0 = sample_tx(1, 0);
        a0.from = sender_a.clone();
        let mut b0 = sample_tx(1, 0);
        b0.from = sender_b.clone();
        push_ingress_frames(1, &[resign_tx(a0), resign_tx(b0)]);
        let _ = drain_executable_ingress_frames_for_host(16);

        let mut a2 = sample_tx(1, 2);
        a2.from = sender_a.clone();
        let mut a3 = sample_tx(1, 3);
        a3.from = sender_a.clone();
        let mut b2 = sample_tx(1, 2);
        b2.from = sender_b.clone();
        push_ingress_frames(1, &[resign_tx(a2), resign_tx(a3), resign_tx(b2)]);

        let drained = drain_plugin_ingress_frames_for_host(3);
        let sender_nonce_pairs: Vec<(Vec<u8>, u64)> = drained
            .iter()
            .filter_map(|frame| {
                frame
                    .parsed_tx
                    .as_ref()
                    .map(|tx| (tx.from.clone(), tx.nonce))
            })
            .collect();
        assert_eq!(
            sender_nonce_pairs,
            vec![(sender_a.clone(), 2), (sender_b.clone(), 2), (sender_a, 3),]
        );
    }

    #[test]
    fn runtime_tap_summary_reports_txpool_drop_reason() {
        let _guard = runtime_queue_test_guard();
        reset_runtime_queues_for_test();

        let seed = sample_tx(1, 0);
        let first = runtime_tap_ir_batch_v1(ChainType::EVM, 1, &[seed], 0).expect("tap ok");
        assert_eq!(first.requested, 1);
        assert_eq!(first.accepted, 1);
        assert_eq!(first.dropped, 0);

        let mut low_replace = sample_tx(1, 0);
        low_replace.to = Some(encode_address(2001));
        low_replace = resign_tx(low_replace);
        let second = runtime_tap_ir_batch_v1(ChainType::EVM, 1, &[low_replace], 0).expect("tap ok");
        assert_eq!(second.requested, 1);
        assert_eq!(second.accepted, 0);
        assert_eq!(second.dropped, 1);
        assert_eq!(second.dropped_underpriced, 1);
        assert_eq!(
            second.primary_reject_reason,
            Some(EvmTxpoolRejectReasonV1::ReplacementUnderpriced)
        );
        assert_eq!(
            second.reject_reasons,
            vec![EvmTxpoolRejectReasonV1::ReplacementUnderpriced]
        );
    }

    #[test]
    fn plugin_apply_v2_atomic_guard_enforces_and_executes() {
        let _guard = runtime_queue_test_guard();
        reset_runtime_queues_for_test();
        let options = NovovmAdapterPluginApplyOptionsV1 {
            flags: NOVOVM_ADAPTER_PLUGIN_APPLY_FLAG_ATOMIC_INTENT_GUARD_V1,
        };
        let mut out = NovovmAdapterPluginApplyResultV1::default();

        let mut invalid_tx = sample_tx(1, 0);
        invalid_tx.source_chain = Some(2);
        invalid_tx.target_chain = Some(137);
        let invalid_tx = resign_tx(invalid_tx);
        let invalid_bytes = crate::bincode_compat::serialize(&vec![invalid_tx]).expect("tx encode");
        let rc_invalid = unsafe {
            novovm_adapter_plugin_apply_v2(
                1,
                1,
                invalid_bytes.as_ptr(),
                invalid_bytes.len(),
                &options as *const NovovmAdapterPluginApplyOptionsV1,
                &mut out as *mut NovovmAdapterPluginApplyResultV1,
            )
        };
        assert_eq!(rc_invalid, NOVOVM_ADAPTER_PLUGIN_RC_APPLY_FAILED);

        let mut valid_tx = sample_tx(1, 0);
        valid_tx.source_chain = Some(1);
        valid_tx.target_chain = Some(137);
        let valid_tx = resign_tx(valid_tx);
        let valid_bytes = crate::bincode_compat::serialize(&vec![valid_tx]).expect("tx encode");
        let rc_valid = unsafe {
            novovm_adapter_plugin_apply_v2(
                1,
                1,
                valid_bytes.as_ptr(),
                valid_bytes.len(),
                &options as *const NovovmAdapterPluginApplyOptionsV1,
                &mut out as *mut NovovmAdapterPluginApplyResultV1,
            )
        };
        assert_eq!(rc_valid, NOVOVM_ADAPTER_PLUGIN_RC_OK);

        let receipts = drain_atomic_receipts_for_host(32);
        assert!(receipts
            .iter()
            .any(|r| r.status == AtomicIntentStatus::Rejected));
        assert!(receipts
            .iter()
            .any(|r| r.status == AtomicIntentStatus::Executed));
        let ready = drain_atomic_broadcast_ready_for_host(32);
        assert!(ready
            .iter()
            .any(|item| item.intent.source_chain == ChainType::EVM));
    }

    #[test]
    fn plugin_runtime_tap_v1_populates_runtime_queues_without_state_apply() {
        let _guard = runtime_queue_test_guard();
        reset_runtime_queues_for_test();

        let mut tx = sample_contract_call_tx(1, 42);
        tx.source_chain = Some(1);
        tx.target_chain = Some(137);
        let tx = resign_tx(tx);
        let tx_bytes = crate::bincode_compat::serialize(&vec![tx]).expect("tx encode");
        let rc = unsafe {
            novovm_adapter_plugin_runtime_tap_v1(
                1,
                1,
                tx_bytes.as_ptr(),
                tx_bytes.len(),
                NOVOVM_ADAPTER_PLUGIN_APPLY_FLAG_ATOMIC_INTENT_GUARD_V1,
            )
        };
        assert_eq!(rc, NOVOVM_ADAPTER_PLUGIN_RC_OK);

        let ingress = drain_plugin_ingress_frames_for_host(16);
        assert!(!ingress.is_empty());
        let settlement = drain_settlement_records_for_host(16);
        assert!(!settlement.is_empty());
        let receipts = drain_atomic_receipts_for_host(16);
        assert!(receipts
            .iter()
            .any(|r| r.status == AtomicIntentStatus::Accepted));
        let ready = drain_atomic_broadcast_ready_for_host(16);
        assert!(!ready.is_empty());
    }

    #[test]
    fn plugin_bincode_export_supports_size_probe_and_decode() {
        let mut size = 0usize;
        let rc_probe = unsafe {
            novovm_adapter_plugin_get_settlement_snapshot_bincode_v1(
                std::ptr::null_mut(),
                0,
                &mut size as *mut usize,
            )
        };
        assert_eq!(rc_probe, NOVOVM_ADAPTER_PLUGIN_RC_OK);
        assert!(size > 0);

        let mut too_small_len = 0usize;
        let mut too_small = vec![0u8; size.saturating_sub(1)];
        let rc_too_small = unsafe {
            novovm_adapter_plugin_get_settlement_snapshot_bincode_v1(
                too_small.as_mut_ptr(),
                too_small.len(),
                &mut too_small_len as *mut usize,
            )
        };
        assert_eq!(rc_too_small, NOVOVM_ADAPTER_PLUGIN_RC_BUFFER_TOO_SMALL);
        assert_eq!(too_small_len, size);

        let mut out_len = 0usize;
        let mut out = vec![0u8; size];
        let rc = unsafe {
            novovm_adapter_plugin_get_settlement_snapshot_bincode_v1(
                out.as_mut_ptr(),
                out.len(),
                &mut out_len as *mut usize,
            )
        };
        assert_eq!(rc, NOVOVM_ADAPTER_PLUGIN_RC_OK);
        out.truncate(out_len);
        let snapshot: EvmFeeSettlementSnapshotV1 =
            crate::bincode_compat::deserialize(&out).expect("decode settlement snapshot");
        assert!(!snapshot.policy.reserve_currency_code.is_empty());
        assert!(!snapshot.policy.payout_token_code.is_empty());
    }

    #[test]
    fn plugin_bincode_drain_exports_return_empty_for_zero_max_items() {
        let _guard = runtime_queue_test_guard();
        reset_runtime_queues_for_test();
        let mut ingress_len = 0usize;
        let rc_ingress_probe = unsafe {
            novovm_adapter_plugin_drain_ingress_bincode_v1(
                0,
                std::ptr::null_mut(),
                0,
                &mut ingress_len as *mut usize,
            )
        };
        assert_eq!(rc_ingress_probe, NOVOVM_ADAPTER_PLUGIN_RC_OK);
        let mut ingress_buf = vec![0u8; ingress_len];
        let rc_ingress = unsafe {
            novovm_adapter_plugin_drain_ingress_bincode_v1(
                0,
                ingress_buf.as_mut_ptr(),
                ingress_buf.len(),
                &mut ingress_len as *mut usize,
            )
        };
        assert_eq!(rc_ingress, NOVOVM_ADAPTER_PLUGIN_RC_OK);
        ingress_buf.truncate(ingress_len);
        let ingress: Vec<EvmMempoolIngressFrameV1> =
            crate::bincode_compat::deserialize(&ingress_buf).expect("decode ingress");
        assert!(ingress.is_empty());

        let mut pending_len = 0usize;
        let rc_pending_probe = unsafe {
            novovm_adapter_plugin_drain_pending_ingress_bincode_v1(
                0,
                std::ptr::null_mut(),
                0,
                &mut pending_len as *mut usize,
            )
        };
        assert_eq!(rc_pending_probe, NOVOVM_ADAPTER_PLUGIN_RC_OK);
        let mut pending_buf = vec![0u8; pending_len];
        let rc_pending = unsafe {
            novovm_adapter_plugin_drain_pending_ingress_bincode_v1(
                0,
                pending_buf.as_mut_ptr(),
                pending_buf.len(),
                &mut pending_len as *mut usize,
            )
        };
        assert_eq!(rc_pending, NOVOVM_ADAPTER_PLUGIN_RC_OK);
        pending_buf.truncate(pending_len);
        let pending: Vec<EvmMempoolIngressFrameV1> =
            crate::bincode_compat::deserialize(&pending_buf).expect("decode pending ingress");
        assert!(pending.is_empty());

        let mut pending_bucket_len = 0usize;
        let rc_bucket_probe = unsafe {
            novovm_adapter_plugin_snapshot_pending_sender_buckets_bincode_v1(
                0,
                0,
                std::ptr::null_mut(),
                0,
                &mut pending_bucket_len as *mut usize,
            )
        };
        assert_eq!(rc_bucket_probe, NOVOVM_ADAPTER_PLUGIN_RC_OK);
        let mut pending_bucket_buf = vec![0u8; pending_bucket_len];
        let rc_bucket = unsafe {
            novovm_adapter_plugin_snapshot_pending_sender_buckets_bincode_v1(
                0,
                0,
                pending_bucket_buf.as_mut_ptr(),
                pending_bucket_buf.len(),
                &mut pending_bucket_len as *mut usize,
            )
        };
        assert_eq!(rc_bucket, NOVOVM_ADAPTER_PLUGIN_RC_OK);
        pending_bucket_buf.truncate(pending_bucket_len);
        let pending_buckets: Vec<EvmPendingSenderBucketV1> =
            crate::bincode_compat::deserialize(&pending_bucket_buf)
                .expect("decode pending sender buckets");
        assert!(pending_buckets.is_empty());

        let mut receipts_len = 0usize;
        let rc_receipts_probe = unsafe {
            novovm_adapter_plugin_drain_atomic_receipts_bincode_v1(
                0,
                std::ptr::null_mut(),
                0,
                &mut receipts_len as *mut usize,
            )
        };
        assert_eq!(rc_receipts_probe, NOVOVM_ADAPTER_PLUGIN_RC_OK);
        let mut receipts_buf = vec![0u8; receipts_len];
        let rc_receipts = unsafe {
            novovm_adapter_plugin_drain_atomic_receipts_bincode_v1(
                0,
                receipts_buf.as_mut_ptr(),
                receipts_buf.len(),
                &mut receipts_len as *mut usize,
            )
        };
        assert_eq!(rc_receipts, NOVOVM_ADAPTER_PLUGIN_RC_OK);
        receipts_buf.truncate(receipts_len);
        let receipts: Vec<AtomicIntentReceiptV1> =
            crate::bincode_compat::deserialize(&receipts_buf).expect("decode receipts");
        assert!(receipts.is_empty());

        let mut settlement_len = 0usize;
        let rc_settlement_probe = unsafe {
            novovm_adapter_plugin_drain_settlement_records_bincode_v1(
                0,
                std::ptr::null_mut(),
                0,
                &mut settlement_len as *mut usize,
            )
        };
        assert_eq!(rc_settlement_probe, NOVOVM_ADAPTER_PLUGIN_RC_OK);
        let mut settlement_buf = vec![0u8; settlement_len];
        let rc_settlement = unsafe {
            novovm_adapter_plugin_drain_settlement_records_bincode_v1(
                0,
                settlement_buf.as_mut_ptr(),
                settlement_buf.len(),
                &mut settlement_len as *mut usize,
            )
        };
        assert_eq!(rc_settlement, NOVOVM_ADAPTER_PLUGIN_RC_OK);
        settlement_buf.truncate(settlement_len);
        let settlement: Vec<EvmFeeSettlementRecordV1> =
            crate::bincode_compat::deserialize(&settlement_buf).expect("decode settlement records");
        assert!(settlement.is_empty());

        let mut payout_len = 0usize;
        let rc_payout_probe = unsafe {
            novovm_adapter_plugin_drain_payout_instructions_bincode_v1(
                0,
                std::ptr::null_mut(),
                0,
                &mut payout_len as *mut usize,
            )
        };
        assert_eq!(rc_payout_probe, NOVOVM_ADAPTER_PLUGIN_RC_OK);
        let mut payout_buf = vec![0u8; payout_len];
        let rc_payout = unsafe {
            novovm_adapter_plugin_drain_payout_instructions_bincode_v1(
                0,
                payout_buf.as_mut_ptr(),
                payout_buf.len(),
                &mut payout_len as *mut usize,
            )
        };
        assert_eq!(rc_payout, NOVOVM_ADAPTER_PLUGIN_RC_OK);
        payout_buf.truncate(payout_len);
        let payouts: Vec<EvmFeePayoutInstructionV1> =
            crate::bincode_compat::deserialize(&payout_buf).expect("decode payout instructions");
        assert!(payouts.is_empty());

        let mut ready_len = 0usize;
        let rc_ready_probe = unsafe {
            novovm_adapter_plugin_drain_atomic_broadcast_ready_bincode_v1(
                0,
                std::ptr::null_mut(),
                0,
                &mut ready_len as *mut usize,
            )
        };
        assert_eq!(rc_ready_probe, NOVOVM_ADAPTER_PLUGIN_RC_OK);
        let mut ready_buf = vec![0u8; ready_len];
        let rc_ready = unsafe {
            novovm_adapter_plugin_drain_atomic_broadcast_ready_bincode_v1(
                0,
                ready_buf.as_mut_ptr(),
                ready_buf.len(),
                &mut ready_len as *mut usize,
            )
        };
        assert_eq!(rc_ready, NOVOVM_ADAPTER_PLUGIN_RC_OK);
        ready_buf.truncate(ready_len);
        let ready: Vec<AtomicBroadcastReadyV1> =
            crate::bincode_compat::deserialize(&ready_buf).expect("decode atomic ready records");
        assert!(ready.is_empty());

        let mut exec_receipts_len = 0usize;
        let rc_exec_receipts_probe = unsafe {
            novovm_adapter_plugin_drain_execution_receipts_bincode_v1(
                0,
                std::ptr::null_mut(),
                0,
                &mut exec_receipts_len as *mut usize,
            )
        };
        assert_eq!(rc_exec_receipts_probe, NOVOVM_ADAPTER_PLUGIN_RC_OK);
        let mut exec_receipts_buf = vec![0u8; exec_receipts_len];
        let rc_exec_receipts = unsafe {
            novovm_adapter_plugin_drain_execution_receipts_bincode_v1(
                0,
                exec_receipts_buf.as_mut_ptr(),
                exec_receipts_buf.len(),
                &mut exec_receipts_len as *mut usize,
            )
        };
        assert_eq!(rc_exec_receipts, NOVOVM_ADAPTER_PLUGIN_RC_OK);
        exec_receipts_buf.truncate(exec_receipts_len);
        let execution_receipts: Vec<SupervmEvmExecutionReceiptV1> =
            crate::bincode_compat::deserialize(&exec_receipts_buf)
                .expect("decode execution receipts");
        assert!(execution_receipts.is_empty());

        let mut mirror_len = 0usize;
        let rc_mirror_probe = unsafe {
            novovm_adapter_plugin_drain_state_mirror_updates_bincode_v1(
                0,
                std::ptr::null_mut(),
                0,
                &mut mirror_len as *mut usize,
            )
        };
        assert_eq!(rc_mirror_probe, NOVOVM_ADAPTER_PLUGIN_RC_OK);
        let mut mirror_buf = vec![0u8; mirror_len];
        let rc_mirror = unsafe {
            novovm_adapter_plugin_drain_state_mirror_updates_bincode_v1(
                0,
                mirror_buf.as_mut_ptr(),
                mirror_buf.len(),
                &mut mirror_len as *mut usize,
            )
        };
        assert_eq!(rc_mirror, NOVOVM_ADAPTER_PLUGIN_RC_OK);
        mirror_buf.truncate(mirror_len);
        let mirror_updates: Vec<SupervmEvmStateMirrorUpdateV1> =
            crate::bincode_compat::deserialize(&mirror_buf).expect("decode mirror updates");
        assert!(mirror_updates.is_empty());
    }

    #[test]
    fn plugin_apply_v2_can_export_and_ingest_execution_receipts() {
        let _guard = runtime_queue_test_guard();
        reset_runtime_queues_for_test();
        let txs = vec![
            sample_contract_call_tx(1, 0),
            sample_contract_deploy_tx(1, 1),
        ];
        let tx_bytes = crate::bincode_compat::serialize(&txs).expect("tx encode");
        let options = NovovmAdapterPluginApplyOptionsV1 {
            flags: NOVOVM_ADAPTER_PLUGIN_APPLY_FLAG_MAINLINE_RECEIPT_EXPORT_V1
                | NOVOVM_ADAPTER_PLUGIN_APPLY_FLAG_MAINLINE_RECEIPT_INGEST_V1,
        };
        let mut out = NovovmAdapterPluginApplyResultV1::default();
        let rc = unsafe {
            novovm_adapter_plugin_apply_v2(
                1,
                1,
                tx_bytes.as_ptr(),
                tx_bytes.len(),
                &options as *const NovovmAdapterPluginApplyOptionsV1,
                &mut out as *mut NovovmAdapterPluginApplyResultV1,
            )
        };
        assert_eq!(rc, NOVOVM_ADAPTER_PLUGIN_RC_OK);
        let receipts = drain_execution_receipts_for_host(16);
        assert_eq!(receipts.len(), 2);
        assert!(receipts.iter().all(|receipt| receipt.chain_id == 1));
        assert!(receipts.iter().all(|receipt| receipt.state_version > 0));
        assert!(receipts[1].contract_address.is_some());
        let mirror_updates = drain_state_mirror_updates_for_host(16);
        assert_eq!(mirror_updates.len(), 1);
        assert_eq!(mirror_updates[0].receipt_count, 2);
        assert_eq!(mirror_updates[0].accepted_receipt_count, 2);
    }

    #[test]
    fn execution_receipt_builder_prefers_aoem_contract() {
        let txs = vec![
            sample_contract_call_tx(1, 0),
            sample_contract_deploy_tx(1, 1),
        ];
        let verify_results = vec![true, true];
        let result = NovovmAdapterPluginApplyResultV1 {
            verified: 1,
            applied: 1,
            txs: 2,
            accounts: 2,
            state_root: [0x11; 32],
            error_code: 0,
        };
        let aoem_artifacts = AoemBatchExecutionArtifactsV1 {
            state_root: [0x33; 32],
            processed_ops: 2,
            success_ops: 1,
            failed_index: Some(1),
            total_writes: 4,
            tx_artifacts: vec![
                novovm_exec::AoemTxExecutionArtifactV1 {
                    tx_index: 0,
                    tx_hash: tx_hash_or_compute(&txs[0]),
                    status_ok: true,
                    gas_used: 21_000,
                    cumulative_gas_used: 21_000,
                    state_root: [0x33; 32],
                    contract_address: None,
                    receipt_type: Some(2),
                    effective_gas_price: Some(7),
                    runtime_code: None,
                    runtime_code_hash: None,
                    event_logs: vec![novovm_exec::AoemEventLogV1 {
                        emitter: txs[0].to.clone().expect("call target"),
                        topics: vec![[0xabu8; 32]],
                        data: vec![0x01, 0x02],
                        log_index: 0,
                    }],
                    log_bloom: vec![0x55; AOEM_LOG_BLOOM_BYTES_V1],
                    revert_data: None,
                    anchor: Some(AoemTxExecutionAnchorV1 {
                        op_index: Some(0),
                        processed_ops: 2,
                        success_ops: 1,
                        failed_index: Some(1),
                        total_writes: 4,
                        elapsed_us: 9,
                        return_code: 0,
                        return_code_name: "ok".to_string(),
                    }),
                },
                novovm_exec::AoemTxExecutionArtifactV1 {
                    tx_index: 1,
                    tx_hash: tx_hash_or_compute(&txs[1]),
                    status_ok: false,
                    gas_used: 0,
                    cumulative_gas_used: 21_000,
                    state_root: [0x33; 32],
                    contract_address: None,
                    receipt_type: Some(3),
                    effective_gas_price: Some(9),
                    runtime_code: None,
                    runtime_code_hash: None,
                    event_logs: Vec::new(),
                    log_bloom: vec![0u8; AOEM_LOG_BLOOM_BYTES_V1],
                    revert_data: Some(vec![0xde, 0xad]),
                    anchor: Some(AoemTxExecutionAnchorV1 {
                        op_index: Some(1),
                        processed_ops: 2,
                        success_ops: 1,
                        failed_index: Some(1),
                        total_writes: 4,
                        elapsed_us: 9,
                        return_code: 0,
                        return_code_name: "ok".to_string(),
                    }),
                },
            ],
        };
        let receipts = build_execution_receipts_from_apply_batch(
            ChainType::EVM,
            1,
            txs.as_slice(),
            verify_results.as_slice(),
            &result,
            9,
            Some(&aoem_artifacts),
        );
        assert_eq!(receipts.len(), 2);
        assert_eq!(receipts[0].state_root, [0x33; 32]);
        assert_eq!(receipts[0].logs.len(), 1);
        assert_eq!(receipts[0].logs[0].topics, vec![[0xabu8; 32]]);
        assert_eq!(receipts[0].receipt_type, Some(2));
        assert_eq!(receipts[0].effective_gas_price, Some(7));
        assert_eq!(receipts[0].log_bloom, vec![0x55; AOEM_LOG_BLOOM_BYTES_V1]);
        assert!(receipts[0].status_ok);
        assert!(!receipts[1].status_ok);
        assert_eq!(receipts[1].receipt_type, Some(3));
        assert_eq!(receipts[1].revert_data, Some(vec![0xde, 0xad]));
        assert!(receipts[1].contract_address.is_none());
    }

    #[test]
    fn plugin_receipt_ingest_bincode_returns_state_mirror_update() {
        let _guard = runtime_queue_test_guard();
        reset_runtime_queues_for_test();
        let receipt = SupervmEvmExecutionReceiptV1 {
            chain_type: ChainType::EVM,
            chain_id: 1,
            tx_hash: vec![0x11; 32],
            tx_index: 0,
            tx_type: TxType::Transfer,
            receipt_type: Some(0),
            status_ok: true,
            gas_used: 21_000,
            cumulative_gas_used: 21_000,
            effective_gas_price: Some(1),
            log_bloom: vec![0u8; AOEM_LOG_BLOOM_BYTES_V1],
            revert_data: None,
            state_root: [0x22; 32],
            state_version: 77,
            contract_address: None,
            logs: Vec::new(),
        };
        let payload = crate::bincode_compat::serialize(&vec![receipt]).expect("receipt encode");
        let mut out_len = 0usize;
        let rc_probe = unsafe {
            novovm_adapter_plugin_ingest_execution_receipts_bincode_v1(
                1,
                77,
                payload.as_ptr(),
                payload.len(),
                std::ptr::null_mut(),
                0,
                &mut out_len as *mut usize,
            )
        };
        assert_eq!(rc_probe, NOVOVM_ADAPTER_PLUGIN_RC_OK);
        let mut out = vec![0u8; out_len];
        let rc = unsafe {
            novovm_adapter_plugin_ingest_execution_receipts_bincode_v1(
                1,
                77,
                payload.as_ptr(),
                payload.len(),
                out.as_mut_ptr(),
                out.len(),
                &mut out_len as *mut usize,
            )
        };
        assert_eq!(rc, NOVOVM_ADAPTER_PLUGIN_RC_OK);
        out.truncate(out_len);
        let update: SupervmEvmStateMirrorUpdateV1 =
            crate::bincode_compat::deserialize(&out).expect("decode mirror update");
        assert_eq!(update.chain_id, 1);
        assert_eq!(update.receipt_count, 1);
        assert_eq!(update.state_version, 77);
        let drained = drain_state_mirror_updates_for_host(16);
        assert_eq!(drained.len(), 1);
        assert_eq!(drained[0].state_root, [0x22; 32]);
    }

    #[test]
    fn submit_internal_batch_to_mainline_avoids_duplicate_ingress() {
        let _guard = runtime_queue_test_guard();
        reset_runtime_queues_for_test();
        let txs = vec![sample_tx(1, 0), sample_tx(1, 1)];
        let report = submit_internal_batch_to_mainline_v1(ChainType::EVM, 1, &txs, false)
            .expect("submit ok");
        assert_eq!(report.tx_count, 2);
        assert_eq!(report.exported_receipt_count, 2);
        assert_eq!(report.mirrored_receipt_count, 2);
        assert!(report.ingress_bypassed);
        let ingress = drain_plugin_ingress_frames_for_host(16);
        assert_eq!(ingress.len(), 2);
        let receipts = drain_execution_receipts_for_host(16);
        assert_eq!(receipts.len(), 2);
        let mirror_updates = drain_state_mirror_updates_for_host(16);
        assert_eq!(mirror_updates.len(), 1);
    }

    #[test]
    fn plugin_apply_v1_rejects_oversized_payload_before_decode() {
        let mut out = NovovmAdapterPluginApplyResultV1::default();
        let marker = [0u8; 1];
        let rc = unsafe {
            novovm_adapter_plugin_apply_v1(
                1,
                1,
                marker.as_ptr(),
                MAX_PLUGIN_TX_IR_BYTES + 1,
                &mut out as *mut NovovmAdapterPluginApplyResultV1,
            )
        };
        assert_eq!(rc, NOVOVM_ADAPTER_PLUGIN_RC_PAYLOAD_TOO_LARGE);
    }

    #[test]
    fn plugin_apply_v1_rejects_decode_size_limit_payload() {
        let mut out = NovovmAdapterPluginApplyResultV1::default();
        let mut payload = Vec::new();
        // Vec<TxIR> length prefix set to a huge value; decode must hard-fail (never succeed).
        payload.extend_from_slice(&u64::MAX.to_le_bytes());
        let rc = unsafe {
            novovm_adapter_plugin_apply_v1(
                1,
                1,
                payload.as_ptr(),
                payload.len(),
                &mut out as *mut NovovmAdapterPluginApplyResultV1,
            )
        };
        assert!(
            rc == NOVOVM_ADAPTER_PLUGIN_RC_PAYLOAD_TOO_LARGE
                || rc == NOVOVM_ADAPTER_PLUGIN_RC_DECODE_FAILED
        );
    }

    #[test]
    fn bincode_export_rejects_payload_over_size_limit() {
        let payload = vec![0u8; MAX_PLUGIN_TX_IR_BYTES + 1];
        let mut out_len = 0usize;
        let rc =
            unsafe { write_bincode_blob_to_out(&payload, std::ptr::null_mut(), 0, &mut out_len) };
        assert_eq!(out_len, payload.len());
        assert_eq!(rc, NOVOVM_ADAPTER_PLUGIN_RC_PAYLOAD_TOO_LARGE);
    }
}
