#![forbid(unsafe_code)]

use anyhow::{bail, Context, Result};
use ed25519_dalek::{Signature as Ed25519Signature, Verifier, VerifyingKey};
mod bincode_compat;
use novovm_adapter_api::{
    AccountPolicy, AccountRole, AtomicBroadcastReadyV1, AtomicIntentReceiptV1, AtomicIntentStatus,
    EvmFeePayoutInstructionV1, EvmFeeSettlementRecordV1, EvmMempoolIngressFrameV1, KycPolicyMode,
    NonceScope, PersonaAddress, PersonaType, ProtocolKind, RouteDecision, RouteRequest,
    SerializationFormat, TxIR, TxType, Type4PolicyMode, UnifiedAccountRouter,
};
#[cfg(test)]
use novovm_adapter_evm_core::{
    estimate_access_list_intrinsic_extra_gas_m0, estimate_intrinsic_gas_m0,
    estimate_intrinsic_gas_with_access_list_m0,
};
use novovm_adapter_evm_core::{
    estimate_intrinsic_gas_with_envelope_extras_m0, recover_raw_evm_tx_sender_m0,
    resolve_evm_chain_type_from_chain_id, resolve_evm_profile, resolve_raw_evm_tx_route_hint_m0,
    translate_raw_evm_tx_fields_m0, tx_ir_from_raw_fields_m0, validate_tx_semantics_m0,
    EvmRawTxEnvelopeType, EvmRawTxFieldsM0,
};
use novovm_adapter_evm_plugin::{
    apply_ir_batch_v1, drain_atomic_broadcast_ready_for_host, drain_atomic_receipts_for_host,
    drain_executable_ingress_frames_for_host, drain_payout_instructions_for_host,
    drain_pending_ingress_frames_for_host, drain_settlement_records_for_host,
    evict_stale_ingress_frames_for_host, runtime_tap_ir_batch_v1,
    snapshot_executable_ingress_frames_for_host, snapshot_pending_ingress_frames_for_host,
    snapshot_pending_sender_buckets_for_host, EvmPendingSenderBucketV1,
    NovovmAdapterPluginApplyResultV1, NOVOVM_ADAPTER_PLUGIN_APPLY_FLAG_ATOMIC_INTENT_GUARD_V1,
};
use novovm_adapter_novovm::{
    build_privacy_tx_ir_signed_from_raw_v1, PrivacyTxRawEnvelopeV1, PrivacyTxRawSignerV1,
};
use novovm_exec::{OpsWireOp, OpsWireV1Builder};
use novovm_network::{
    get_network_runtime_native_sync_status, get_network_runtime_sync_status,
    network_runtime_native_sync_is_active, observe_network_runtime_local_head_max,
    plan_network_runtime_sync_pull_window,
};
use rocksdb::{
    ColumnFamilyDescriptor, Direction, IteratorMode, Options as RocksDbOptions, DB as RocksDb,
    DEFAULT_COLUMN_FAMILY_NAME,
};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use sha3::Keccak256;
use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::fs;
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::{
    atomic::{AtomicU64, Ordering},
    Mutex, OnceLock,
};
use std::thread;
use std::time::{Duration, SystemTime};

mod rpc_eth_txpool;
use rpc_eth_txpool::*;
mod rpc_eth_sync;
use rpc_eth_sync::*;
mod rpc_params_utils;
use rpc_params_utils::*;
mod rpc_eth_query_helpers;
use rpc_eth_query_helpers::*;
mod rpc_eth_upstream;
use rpc_eth_upstream::*;
mod rpc_error_http;
use rpc_error_http::*;
mod rpc_eth_receipts;
use rpc_eth_receipts::*;
mod rpc_eth_state;
use rpc_eth_state::*;
mod rpc_gateway_ops;
use rpc_gateway_ops::*;
mod rpc_gateway_exec_cfg;
use rpc_gateway_exec_cfg::*;

const GATEWAY_UA_STORE_ENVELOPE_VERSION: u32 = 1;
const GATEWAY_UA_STORE_BACKEND_FILE: &str = "bincode_file";
const GATEWAY_UA_STORE_BACKEND_ROCKSDB: &str = "rocksdb";
const GATEWAY_UA_STORE_ROCKSDB_CF_STATE: &str = "ua_gateway_state_v1";
const GATEWAY_UA_STORE_ROCKSDB_KEY_ROUTER: &[u8] = b"ua_gateway:router:v1";
const GATEWAY_ETH_TX_INDEX_RECORD_VERSION: u32 = 1;
const GATEWAY_EVM_SETTLEMENT_INDEX_RECORD_VERSION: u32 = 1;
const GATEWAY_EVM_PAYOUT_PENDING_RECORD_VERSION: u32 = 1;
const GATEWAY_EVM_ATOMIC_READY_INDEX_RECORD_VERSION: u32 = 1;
const GATEWAY_EVM_ATOMIC_READY_PENDING_RECORD_VERSION: u32 = 1;
const GATEWAY_EVM_ATOMIC_BROADCAST_PENDING_RECORD_VERSION: u32 = 1;
const GATEWAY_ETH_PUBLIC_BROADCAST_PENDING_RECORD_VERSION: u32 = 1;
const GATEWAY_ETH_BROADCAST_STATUS_RECORD_VERSION: u32 = 1;
const GATEWAY_ETH_SUBMIT_STATUS_RECORD_VERSION: u32 = 1;
const GATEWAY_ETH_TX_INDEX_BACKEND_MEMORY: &str = "memory";
const GATEWAY_ETH_TX_INDEX_BACKEND_ROCKSDB: &str = "rocksdb";
const GATEWAY_ALLOW_NON_PROD_BACKEND_ENV: &str = "NOVOVM_ALLOW_NON_PROD_GATEWAY_BACKEND";
const GATEWAY_ETH_TX_INDEX_ROCKSDB_CF_STATE: &str = "eth_tx_index_state_v1";
const GATEWAY_ETH_TX_INDEX_ROCKSDB_KEY_PREFIX: &[u8] = b"gateway:eth:tx:index:v1:";
const GATEWAY_ETH_BROADCAST_STATUS_ROCKSDB_KEY_PREFIX: &[u8] = b"gateway:eth:broadcast_status:v1:";
const GATEWAY_ETH_SUBMIT_STATUS_ROCKSDB_KEY_PREFIX: &[u8] = b"gateway:eth:submit_status:v1:";
const GATEWAY_ETH_TX_BLOCK_INDEX_ROCKSDB_KEY_PREFIX: &[u8] = b"gateway:eth:tx:block_index:v1:";
const GATEWAY_ETH_BLOCK_HASH_INDEX_ROCKSDB_KEY_BY_HASH_PREFIX: &[u8] =
    b"gateway:eth:block_hash:index:by_hash:v1:";
const GATEWAY_EVM_SETTLEMENT_INDEX_ROCKSDB_KEY_BY_ID_PREFIX: &[u8] =
    b"gateway:evm:settlement:index:by_id:v1:";
const GATEWAY_EVM_SETTLEMENT_INDEX_ROCKSDB_KEY_BY_TX_PREFIX: &[u8] =
    b"gateway:evm:settlement:index:by_tx:v1:";
const GATEWAY_EVM_PAYOUT_PENDING_ROCKSDB_KEY_PREFIX: &[u8] = b"gateway:evm:payout:pending:v1:";
const GATEWAY_EVM_ATOMIC_READY_INDEX_ROCKSDB_KEY_BY_INTENT_PREFIX: &[u8] =
    b"gateway:evm:atomic_ready:index:by_intent:v1:";
const GATEWAY_EVM_ATOMIC_READY_PENDING_ROCKSDB_KEY_PREFIX: &[u8] =
    b"gateway:evm:atomic_ready:pending:v1:";
const GATEWAY_EVM_ATOMIC_BROADCAST_PENDING_ROCKSDB_KEY_PREFIX: &[u8] =
    b"gateway:evm:atomic_broadcast:pending:v1:";
const GATEWAY_EVM_ATOMIC_BROADCAST_PAYLOAD_ROCKSDB_KEY_PREFIX: &[u8] =
    b"gateway:evm:atomic_broadcast:payload:v1:";
const GATEWAY_ETH_PUBLIC_BROADCAST_PENDING_ROCKSDB_KEY_PREFIX: &[u8] =
    b"gateway:eth:public_broadcast:pending:v1:";
const GATEWAY_UA_PRIMARY_KEY_DOMAIN: &[u8] = b"novovm_gateway_uca_primary_key_ref_v1";
const GATEWAY_INGRESS_RECORD_VERSION: u16 = 1;
const GATEWAY_INGRESS_PROTOCOL_ETH: u8 = 1;
const GATEWAY_INGRESS_PROTOCOL_WEB30: u8 = 2;
const GATEWAY_INGRESS_PROTOCOL_EVM_PAYOUT: u8 = 3;
const GATEWAY_EVM_RUNTIME_DRAIN_MAX: usize = 256;
const GATEWAY_EVM_ATOMIC_BROADCAST_EXEC_RETRY_DEFAULT: u64 = 1;
const GATEWAY_EVM_ATOMIC_BROADCAST_EXEC_TIMEOUT_MS_DEFAULT: u64 = 5_000;
const GATEWAY_EVM_ATOMIC_BROADCAST_EXEC_RETRY_BACKOFF_MS_DEFAULT: u64 = 25;
const GATEWAY_BATCH_WORKERS_DEFAULT_MAX: usize = 8;
const GATEWAY_EVM_ATOMIC_BROADCAST_EXEC_BATCH_DEFAULT: u64 = 64;
const GATEWAY_EVM_ATOMIC_BROADCAST_EXEC_BATCH_HARD_MAX: u64 = 1024;
const GATEWAY_ETH_PUBLIC_BROADCAST_EXEC_RETRY_DEFAULT: u64 = 1;
const GATEWAY_ETH_PUBLIC_BROADCAST_EXEC_TIMEOUT_MS_DEFAULT: u64 = 5_000;
const GATEWAY_ETH_PUBLIC_BROADCAST_EXEC_RETRY_BACKOFF_MS_DEFAULT: u64 = 25;
const GATEWAY_ETH_QUERY_SCAN_MAX_DEFAULT: usize = 8_192;
const GATEWAY_ETH_EMPTY_UNCLES_HASH: &str =
    "0x1dcc4de8dec75d7aab85b567b6ccd41ad312451b948a7413f0a142fd40d49347";
const GATEWAY_ETH_EMPTY_TRIE_ROOT: &str =
    "0x56e81f171bcc55a6ff8345e692c0f86e5b48e01b996cadc001622fb5e363b421";
const EVM_SETTLEMENT_LEDGER_RECORD_KEY_PREFIX: &[u8] = b"ledger:evm:settlement:v1:";
const EVM_SETTLEMENT_LEDGER_RESERVE_DELTA_KEY_PREFIX: &[u8] =
    b"ledger:evm:settlement_reserve_delta:v1:";
const EVM_SETTLEMENT_LEDGER_PAYOUT_DELTA_KEY_PREFIX: &[u8] =
    b"ledger:evm:settlement_payout_delta:v1:";
const EVM_SETTLEMENT_LEDGER_STATUS_KEY_PREFIX: &[u8] = b"ledger:evm:settlement_status:v1:";
const EVM_PAYOUT_LEDGER_RECORD_KEY_PREFIX: &[u8] = b"ledger:evm:payout:v1:";
const EVM_PAYOUT_LEDGER_RESERVE_DELTA_KEY_PREFIX: &[u8] = b"ledger:evm:reserve_delta:v1:";
const EVM_PAYOUT_LEDGER_PAYOUT_DELTA_KEY_PREFIX: &[u8] = b"ledger:evm:payout_delta:v1:";
const EVM_PAYOUT_LEDGER_STATUS_KEY_PREFIX: &[u8] = b"ledger:evm:payout_status:v1:";
const EVM_PAYOUT_LEDGER_RESERVE_DEBIT_KEY_PREFIX: &[u8] = b"ledger:evm:reserve_debit:v1:";
const EVM_PAYOUT_LEDGER_PAYOUT_CREDIT_KEY_PREFIX: &[u8] = b"ledger:evm:payout_credit:v1:";
const EVM_ATOMIC_READY_LEDGER_KEY_PREFIX: &[u8] = b"ledger:evm:atomic_ready:v1:";
const EVM_ATOMIC_BROADCAST_QUEUE_LEDGER_KEY_PREFIX: &[u8] =
    b"ledger:evm:atomic_broadcast_queue:v1:";
const EVM_SETTLEMENT_STATUS_SETTLED_V1: &[u8] = b"settled_v1";
const EVM_PAYOUT_STATUS_APPLIED_V1: &[u8] = b"applied_v1";
const EVM_SETTLEMENT_STATUS_PAYOUT_SPOOLED_V1: &str = "payout_spooled_v1";
const EVM_SETTLEMENT_STATUS_COMPENSATE_PENDING_V1: &str = "compensate_pending_v1";
const EVM_SETTLEMENT_STATUS_COMPENSATED_V1: &str = "compensated_v1";
const EVM_ATOMIC_READY_STATUS_SPOOLED_V1: &str = "spooled_v1";
const EVM_ATOMIC_READY_STATUS_BROADCAST_QUEUED_V1: &str = "broadcast_queued_v1";
const EVM_ATOMIC_READY_STATUS_BROADCASTED_V1: &str = "broadcasted_v1";
const EVM_ATOMIC_READY_STATUS_BROADCAST_FAILED_V1: &str = "broadcast_failed_v1";
const EVM_ATOMIC_READY_STATUS_COMPENSATE_PENDING_V1: &str = "compensate_pending_v1";
const EVM_ATOMIC_READY_STATUS_COMPENSATED_V1: &str = "compensated_v1";

static SPOOL_SEQ: AtomicU64 = AtomicU64::new(0);
static GATEWAY_ETH_BROADCAST_STATUS_BY_TX: OnceLock<
    Mutex<HashMap<[u8; 32], GatewayEthBroadcastStatus>>,
> = OnceLock::new();
static GATEWAY_ETH_SUBMIT_STATUS_BY_TX: OnceLock<Mutex<HashMap<[u8; 32], GatewayEthSubmitStatus>>> =
    OnceLock::new();

macro_rules! gateway_warn {
    ($($arg:tt)*) => {{
        if gateway_warn_enabled() {
            eprintln!($($arg)*);
        }
    }};
}

macro_rules! gateway_summary {
    ($($arg:tt)*) => {{
        if gateway_summary_enabled() {
            println!($($arg)*);
        }
    }};
}

#[derive(Debug, Deserialize)]
struct GatewayUaStoreEnvelopeV1 {
    version: u32,
    router: UnifiedAccountRouter,
}

#[derive(Debug, Clone)]
enum GatewayUaStoreBackend {
    BincodeFile { path: PathBuf },
    RocksDb { path: PathBuf },
}

#[derive(Debug, Clone)]
enum GatewayEthTxIndexStoreBackend {
    Memory,
    RocksDb { path: PathBuf },
}

#[derive(Debug, Serialize, Deserialize)]
struct GatewayIngressEthRecordV1 {
    version: u16,
    protocol: u8,
    uca_id: String,
    chain_id: u64,
    nonce: u64,
    tx_type: u8,
    tx_type4: bool,
    from: Vec<u8>,
    to: Option<Vec<u8>>,
    value: u128,
    gas_limit: u64,
    gas_price: u64,
    data: Vec<u8>,
    signature: Vec<u8>,
    tx_hash: [u8; 32],
    signature_domain: String,
    #[serde(default)]
    overlay_node_id: String,
    #[serde(default)]
    overlay_session_id: String,
}

#[derive(Debug, Serialize, Deserialize)]
struct GatewayIngressWeb30RecordV1 {
    version: u16,
    protocol: u8,
    uca_id: String,
    chain_id: u64,
    nonce: u64,
    from: Vec<u8>,
    payload: Vec<u8>,
    is_raw: bool,
    signature_domain: String,
    wants_cross_chain_atomic: bool,
    tx_hash: [u8; 32],
    #[serde(default)]
    overlay_node_id: String,
    #[serde(default)]
    overlay_session_id: String,
}

struct GatewayWeb30TxHashInput<'a> {
    uca_id: &'a str,
    chain_id: u64,
    nonce: u64,
    from: &'a [u8],
    payload: &'a [u8],
    signature_domain: &'a str,
    is_raw: bool,
    wants_cross_chain_atomic: bool,
}

#[derive(Debug, Clone)]
struct GatewayWeb30PrivacyTxPlan {
    value: u128,
    gas_limit: u64,
    gas_price: u64,
    stealth_view_key: [u8; 32],
    stealth_spend_key: [u8; 32],
    ring_members: Vec<[u8; 32]>,
    signer_index: usize,
    private_key: [u8; 32],
}

struct GatewayEthTxHashInput<'a> {
    uca_id: &'a str,
    chain_id: u64,
    nonce: u64,
    tx_type: u8,
    tx_type4: bool,
    from: &'a [u8],
    to: Option<&'a [u8]>,
    value: u128,
    gas_limit: u64,
    gas_price: u64,
    max_priority_fee_per_gas: u64,
    data: &'a [u8],
    signature: &'a [u8],
    access_list_address_count: u64,
    access_list_storage_key_count: u64,
    max_fee_per_blob_gas: u64,
    blob_hash_count: u64,
    signature_domain: &'a str,
    wants_cross_chain_atomic: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct GatewayEthTxIndexEntry {
    tx_hash: [u8; 32],
    uca_id: String,
    chain_id: u64,
    nonce: u64,
    tx_type: u8,
    from: Vec<u8>,
    to: Option<Vec<u8>>,
    value: u128,
    gas_limit: u64,
    gas_price: u64,
    input: Vec<u8>,
}

#[derive(Debug, Serialize, Deserialize)]
struct GatewayEthTxIndexRecordV1 {
    version: u32,
    entry: GatewayEthTxIndexEntry,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct GatewayEvmSettlementIndexEntry {
    settlement_id: String,
    chain_id: u64,
    income_tx_hash: [u8; 32],
    reserve_delta_wei: u128,
    payout_delta_units: u128,
    settled_at_unix_ms: u64,
    status: String,
}

#[derive(Debug, Serialize, Deserialize)]
struct GatewayEvmSettlementIndexRecordV1 {
    version: u32,
    entry: GatewayEvmSettlementIndexEntry,
}

#[derive(Debug, Serialize, Deserialize)]
struct GatewayEvmSettlementTxRefRecordV1 {
    version: u32,
    settlement_id: String,
}

#[derive(Debug, Serialize, Deserialize)]
struct GatewayEvmPayoutPendingRecordV1 {
    version: u32,
    instruction: EvmFeePayoutInstructionV1,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct GatewayEvmAtomicReadyIndexEntry {
    intent_id: String,
    chain_id: u64,
    tx_hash: [u8; 32],
    ready_at_unix_ms: u64,
    status: String,
}

#[derive(Debug, Serialize, Deserialize)]
struct GatewayEvmAtomicReadyIndexRecordV1 {
    version: u32,
    entry: GatewayEvmAtomicReadyIndexEntry,
}

#[derive(Debug, Serialize, Deserialize)]
struct GatewayEvmAtomicReadyPendingRecordV1 {
    version: u32,
    ready_item: AtomicBroadcastReadyV1,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct GatewayEvmAtomicBroadcastTicketV1 {
    intent_id: String,
    chain_id: u64,
    tx_hash: [u8; 32],
    ready_at_unix_ms: u64,
}

#[derive(Debug, Serialize, Deserialize)]
struct GatewayEvmAtomicBroadcastPendingRecordV1 {
    version: u32,
    ticket: GatewayEvmAtomicBroadcastTicketV1,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct GatewayEthPublicBroadcastPendingTicketV1 {
    chain_id: u64,
    tx_hash: [u8; 32],
    queued_at_unix_ms: u128,
}

#[derive(Debug, Serialize, Deserialize)]
struct GatewayEthPublicBroadcastPendingRecordV1 {
    version: u32,
    ticket: GatewayEthPublicBroadcastPendingTicketV1,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
struct GatewaySettlementTxKey {
    chain_id: u64,
    tx_hash: [u8; 32],
}

#[derive(Debug)]
struct GatewayRuntime {
    bind: String,
    spool_dir: PathBuf,
    max_body_bytes: usize,
    max_requests: u32,
    evm_payout_autoreplay_max: usize,
    evm_payout_autoreplay_cooldown_ms: u64,
    evm_payout_pending_warn_threshold: usize,
    evm_payout_last_autoreplay_at_ms: u128,
    evm_payout_last_warn_at_ms: u128,
    evm_atomic_broadcast_autoreplay_max: usize,
    evm_atomic_broadcast_autoreplay_cooldown_ms: u64,
    evm_atomic_broadcast_pending_warn_threshold: usize,
    evm_atomic_broadcast_autoreplay_use_external_executor: bool,
    evm_atomic_broadcast_last_autoreplay_at_ms: u128,
    evm_atomic_broadcast_last_warn_at_ms: u128,
    eth_public_broadcast_autoreplay_max: usize,
    eth_public_broadcast_autoreplay_cooldown_ms: u64,
    eth_public_broadcast_pending_warn_threshold: usize,
    eth_public_broadcast_last_autoreplay_at_ms: u128,
    eth_public_broadcast_last_warn_at_ms: u128,
    eth_default_chain_id: u64,
    ua_store: GatewayUaStoreBackend,
    eth_tx_index_store: GatewayEthTxIndexStoreBackend,
    eth_tx_index: HashMap<[u8; 32], GatewayEthTxIndexEntry>,
    eth_filters: GatewayEthFilterState,
    evm_settlement_index_by_id: HashMap<String, GatewayEvmSettlementIndexEntry>,
    evm_settlement_index_by_tx: HashMap<GatewaySettlementTxKey, String>,
    evm_pending_payout_by_settlement: HashMap<String, EvmFeePayoutInstructionV1>,
    router: UnifiedAccountRouter,
}

struct GatewayMethodContext<'a> {
    eth_tx_index_store: &'a GatewayEthTxIndexStoreBackend,
    eth_default_chain_id: u64,
    spool_dir: &'a Path,
    overlay_node_id: String,
    overlay_session_id: String,
    eth_filters: &'a mut GatewayEthFilterState,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct GatewayEthBroadcastStatus {
    mode: String,
    attempts: Option<u64>,
    executor: Option<String>,
    executor_output: Option<String>,
    updated_at_unix_ms: u128,
}

#[derive(Debug, Serialize, Deserialize)]
struct GatewayEthBroadcastStatusRecordV1 {
    version: u32,
    status: GatewayEthBroadcastStatus,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct GatewayEthSubmitStatus {
    chain_id: Option<u64>,
    accepted: bool,
    pending: bool,
    onchain: bool,
    error_code: Option<String>,
    error_reason: Option<String>,
    updated_at_unix_ms: u128,
}

#[derive(Debug, Serialize, Deserialize)]
struct GatewayEthSubmitStatusRecordV1 {
    version: u32,
    status: GatewayEthSubmitStatus,
}

#[derive(Debug, Clone, Copy)]
struct GatewayEthSyncStatusV1 {
    peer_count: u64,
    starting_block: u64,
    current_block: u64,
    highest_block: u64,
    local_current_block: u64,
}

struct GatewayEvmRuntimeTapDrain {
    settlement_records: Vec<EvmFeeSettlementRecordV1>,
    payout_instructions: Vec<EvmFeePayoutInstructionV1>,
    atomic_ready_items: Vec<AtomicBroadcastReadyV1>,
}

type GatewayResolvedBlock = (u64, [u8; 32], Vec<GatewayEthTxIndexEntry>);
type GatewayEthTopicFilterSlots = Vec<Option<Vec<[u8; 32]>>>;

#[derive(Debug, Clone)]
struct GatewayEthLogsQuery {
    address_filters: Option<Vec<Vec<u8>>>,
    topic_filters: Option<GatewayEthTopicFilterSlots>,
    block_hash: Option<[u8; 32]>,
    from_block: Option<u64>,
    to_block: Option<u64>,
    include_pending_block: bool,
}

#[derive(Debug, Clone)]
struct GatewayEthLogsFilter {
    chain_id: u64,
    query: GatewayEthLogsQuery,
    next_block: u64,
    block_hash_drained: bool,
}

#[derive(Debug, Clone)]
enum GatewayEthFilterKind {
    Logs(GatewayEthLogsFilter),
    Blocks {
        chain_id: u64,
        last_seen_block: u64,
    },
    PendingTransactions {
        chain_id: u64,
        last_seen_hashes: BTreeSet<[u8; 32]>,
    },
}

#[derive(Debug, Default)]
struct GatewayEthFilterState {
    next_filter_id: u64,
    filters: HashMap<u64, GatewayEthFilterKind>,
}

impl GatewayEthFilterState {
    fn insert(&mut self, kind: GatewayEthFilterKind) -> u64 {
        let id = if self.next_filter_id == 0 {
            1
        } else {
            self.next_filter_id
        };
        self.next_filter_id = id.saturating_add(1);
        self.filters.insert(id, kind);
        id
    }
}

#[derive(Debug, Clone)]
struct GatewayEmbeddedReconcileConfig {
    sender_address: String,
    rpc_endpoint: String,
    interval_seconds: u64,
    restart_delay_seconds: u64,
    replay_max_per_payout: u64,
    replay_cooldown_sec: u64,
    dispatch_index_file: PathBuf,
    submitted_index_file: PathBuf,
    address_map_file: PathBuf,
    output_dir: PathBuf,
    reconcile_index_file: PathBuf,
    state_file: PathBuf,
    cursor_file: PathBuf,
    confirm_method: String,
    submit_method: String,
    wei_per_reward_unit: u64,
    gas_limit: u64,
    max_fee_per_gas_wei: u64,
    max_priority_fee_per_gas_wei: u64,
    rpc_timeout_sec: u64,
    full_replay_first_cycle: bool,
}

#[derive(Debug, Clone)]
struct GatewayEmbeddedReconcileCycleResult {
    processed_submitted_records: u64,
    payout_state_size: usize,
    confirmed_count: u64,
    replayed_count: u64,
    pending_count: u64,
    error_count: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(default)]
struct GatewayReconcileDispatchInstruction {
    payout_id: String,
    node_id: String,
    payout_account: String,
    reward_units: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(default)]
struct GatewayReconcileDispatchRecordLine {
    voucher_id: String,
    payout_instructions: Vec<GatewayReconcileDispatchInstruction>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(default)]
struct GatewayReconcileSubmittedItem {
    payout_id: String,
    voucher_id: String,
    node_id: String,
    payout_account: String,
    reward_units: u64,
    status: String,
    tx_hash_hex: String,
    submitted_at_unix_ms: u64,
    error: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(default)]
struct GatewayReconcileSubmittedRecordLine {
    dispatch_created_at_unix_ms: u64,
    payout_submissions: Vec<GatewayReconcileSubmittedItem>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
struct GatewayReconcilePayoutStateEntry {
    version: u32,
    payout_id: String,
    voucher_id: String,
    node_id: String,
    payout_account: String,
    reward_units: u64,
    status: String,
    tx_hash_hex: Option<String>,
    confirm_block_number_hex: Option<String>,
    submit_count: u64,
    replay_count: u64,
    last_submit_at_unix_ms: u64,
    last_confirm_at_unix_ms: u64,
    last_error: Option<String>,
}

impl Default for GatewayReconcilePayoutStateEntry {
    fn default() -> Self {
        Self {
            version: 1,
            payout_id: String::new(),
            voucher_id: String::new(),
            node_id: String::new(),
            payout_account: String::new(),
            reward_units: 0,
            status: "new_v1".to_string(),
            tx_hash_hex: None,
            confirm_block_number_hex: None,
            submit_count: 0,
            replay_count: 0,
            last_submit_at_unix_ms: 0,
            last_confirm_at_unix_ms: 0,
            last_error: None,
        }
    }
}

impl GatewayReconcilePayoutStateEntry {
    fn new(
        payout_id: &str,
        voucher_id: &str,
        node_id: &str,
        payout_account: &str,
        reward_units: u64,
    ) -> Self {
        Self {
            payout_id: payout_id.to_string(),
            voucher_id: voucher_id.to_string(),
            node_id: node_id.to_string(),
            payout_account: payout_account.to_string(),
            reward_units,
            ..Default::default()
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
struct GatewayReconcileStateRoot {
    version: u32,
    updated_at_unix_ms: u64,
    payouts: BTreeMap<String, GatewayReconcilePayoutStateEntry>,
}

impl Default for GatewayReconcileStateRoot {
    fn default() -> Self {
        Self {
            version: 1,
            updated_at_unix_ms: 0,
            payouts: BTreeMap::new(),
        }
    }
}

#[derive(Debug, Clone, Serialize)]
struct GatewayReconcileChangedAction {
    payout_id: String,
    action: String,
    tx_hash_hex: String,
    status: String,
}

fn load_gateway_embedded_reconcile_config() -> Result<Option<GatewayEmbeddedReconcileConfig>> {
    fn env_present(name: &str) -> bool {
        std::env::var(name)
            .map(|v| !v.trim().is_empty())
            .unwrap_or(false)
    }
    fn string_env_prefer(primary: &str, fallback: &str, default: &str) -> String {
        if env_present(primary) {
            string_env(primary, default)
        } else if env_present(fallback) {
            string_env(fallback, default)
        } else {
            default.to_string()
        }
    }
    fn u64_env_prefer(primary: &str, fallback: &str, default: u64) -> u64 {
        if env_present(primary) {
            u64_env(primary, default)
        } else if env_present(fallback) {
            u64_env(fallback, default)
        } else {
            default
        }
    }
    fn bool_env_prefer(primary: &str, fallback: &str, default: bool) -> bool {
        if env_present(primary) {
            bool_env(primary, default)
        } else if env_present(fallback) {
            bool_env(fallback, default)
        } else {
            default
        }
    }

    let enabled = bool_env_prefer(
        "NOVOVM_GATEWAY_EMBED_RECONCILE_DAEMON",
        "NOVOVM_RECONCILE_ENABLED",
        false,
    );
    if !enabled {
        return Ok(None);
    }
    let sender_address = string_env_prefer(
        "NOVOVM_GATEWAY_RECONCILE_SENDER_ADDRESS",
        "NOVOVM_RECONCILE_SENDER_ADDRESS",
        "",
    );
    if sender_address.trim().is_empty() {
        bail!("reconcile sender address is required when embedded reconcile daemon is enabled");
    }
    let rpc_endpoint = string_env_prefer(
        "NOVOVM_GATEWAY_RECONCILE_RPC_ENDPOINT",
        "NOVOVM_RECONCILE_RPC_ENDPOINT",
        "http://127.0.0.1:9899",
    );
    let interval_seconds = u64_env_prefer(
        "NOVOVM_GATEWAY_RECONCILE_INTERVAL_SECONDS",
        "NOVOVM_RECONCILE_INTERVAL_SECONDS",
        15,
    );
    let restart_delay_seconds = u64_env_prefer(
        "NOVOVM_GATEWAY_RECONCILE_RESTART_DELAY_SECONDS",
        "NOVOVM_RECONCILE_RESTART_DELAY_SECONDS",
        3,
    );
    let replay_max_per_payout = u64_env_prefer(
        "NOVOVM_GATEWAY_RECONCILE_REPLAY_MAX_PER_PAYOUT",
        "NOVOVM_RECONCILE_REPLAY_MAX_PER_PAYOUT",
        3,
    );
    let replay_cooldown_sec = u64_env_prefer(
        "NOVOVM_GATEWAY_RECONCILE_REPLAY_COOLDOWN_SEC",
        "NOVOVM_RECONCILE_REPLAY_COOLDOWN_SEC",
        30,
    );
    let dispatch_index_file = PathBuf::from(string_env_prefer(
        "NOVOVM_GATEWAY_RECONCILE_DISPATCH_INDEX_FILE",
        "NOVOVM_RECONCILE_DISPATCH_INDEX_FILE",
        "artifacts/l1/l1l4-payout-dispatch.jsonl",
    ));
    let submitted_index_file = PathBuf::from(string_env_prefer(
        "NOVOVM_GATEWAY_RECONCILE_SUBMITTED_INDEX_FILE",
        "NOVOVM_RECONCILE_SUBMITTED_INDEX_FILE",
        "artifacts/l1/l1l4-payout-submitted.jsonl",
    ));
    let address_map_file = PathBuf::from(string_env_prefer(
        "NOVOVM_GATEWAY_RECONCILE_ADDRESS_MAP_FILE",
        "NOVOVM_RECONCILE_ADDRESS_MAP_FILE",
        "artifacts/l1/payout-address-map.json",
    ));
    let output_dir = PathBuf::from(string_env_prefer(
        "NOVOVM_GATEWAY_RECONCILE_OUTPUT_DIR",
        "NOVOVM_RECONCILE_OUTPUT_DIR",
        "artifacts/l1/payout-reconcile",
    ));
    let reconcile_index_file = PathBuf::from(string_env_prefer(
        "NOVOVM_GATEWAY_RECONCILE_INDEX_FILE",
        "NOVOVM_RECONCILE_INDEX_FILE",
        "artifacts/l1/l1l4-payout-reconcile.jsonl",
    ));
    let state_file = PathBuf::from(string_env_prefer(
        "NOVOVM_GATEWAY_RECONCILE_STATE_FILE",
        "NOVOVM_RECONCILE_STATE_FILE",
        "artifacts/l1/l1l4-payout-state.json",
    ));
    let cursor_file = PathBuf::from(string_env_prefer(
        "NOVOVM_GATEWAY_RECONCILE_CURSOR_FILE",
        "NOVOVM_RECONCILE_CURSOR_FILE",
        "artifacts/l1/l1l4-payout-reconcile.cursor",
    ));
    let confirm_method = string_env_prefer(
        "NOVOVM_GATEWAY_RECONCILE_CONFIRM_METHOD",
        "NOVOVM_RECONCILE_CONFIRM_METHOD",
        "eth_getTransactionReceipt",
    );
    let submit_method = string_env_prefer(
        "NOVOVM_GATEWAY_RECONCILE_SUBMIT_METHOD",
        "NOVOVM_RECONCILE_SUBMIT_METHOD",
        "eth_sendTransaction",
    );
    let wei_per_reward_unit = u64_env_prefer(
        "NOVOVM_GATEWAY_RECONCILE_WEI_PER_REWARD_UNIT",
        "NOVOVM_RECONCILE_WEI_PER_REWARD_UNIT",
        1,
    );
    let gas_limit = u64_env_prefer(
        "NOVOVM_GATEWAY_RECONCILE_GAS_LIMIT",
        "NOVOVM_RECONCILE_GAS_LIMIT",
        21_000,
    );
    let max_fee_per_gas_wei = u64_env_prefer(
        "NOVOVM_GATEWAY_RECONCILE_MAX_FEE_PER_GAS_WEI",
        "NOVOVM_RECONCILE_MAX_FEE_PER_GAS_WEI",
        0,
    );
    let max_priority_fee_per_gas_wei = u64_env_prefer(
        "NOVOVM_GATEWAY_RECONCILE_MAX_PRIORITY_FEE_PER_GAS_WEI",
        "NOVOVM_RECONCILE_MAX_PRIORITY_FEE_PER_GAS_WEI",
        0,
    );
    let rpc_timeout_sec = u64_env_prefer(
        "NOVOVM_GATEWAY_RECONCILE_RPC_TIMEOUT_SEC",
        "NOVOVM_RECONCILE_RPC_TIMEOUT_SEC",
        15,
    );
    let full_replay_first_cycle = bool_env_prefer(
        "NOVOVM_GATEWAY_RECONCILE_FULL_REPLAY_FIRST_CYCLE",
        "NOVOVM_RECONCILE_FULL_REPLAY_FIRST_CYCLE",
        false,
    );
    Ok(Some(GatewayEmbeddedReconcileConfig {
        sender_address,
        rpc_endpoint,
        interval_seconds,
        restart_delay_seconds,
        replay_max_per_payout,
        replay_cooldown_sec,
        dispatch_index_file,
        submitted_index_file,
        address_map_file,
        output_dir,
        reconcile_index_file,
        state_file,
        cursor_file,
        confirm_method,
        submit_method,
        wei_per_reward_unit,
        gas_limit,
        max_fee_per_gas_wei,
        max_priority_fee_per_gas_wei,
        rpc_timeout_sec,
        full_replay_first_cycle,
    }))
}

fn gateway_reconcile_normalize_evm_address(address: &str) -> Option<String> {
    let trimmed = address.trim();
    if trimmed.is_empty() {
        return None;
    }
    let no_prefix = trimmed
        .strip_prefix("0x")
        .or_else(|| trimmed.strip_prefix("0X"))
        .unwrap_or(trimmed);
    if no_prefix.len() != 40 || !no_prefix.chars().all(|c| c.is_ascii_hexdigit()) {
        return None;
    }
    Some(format!("0x{}", no_prefix.to_ascii_lowercase()))
}

fn gateway_reconcile_normalize_tx_hash(tx_hash_hex: &str) -> Option<String> {
    let trimmed = tx_hash_hex.trim();
    if trimmed.is_empty() {
        return None;
    }
    let no_prefix = trimmed
        .strip_prefix("0x")
        .or_else(|| trimmed.strip_prefix("0X"))
        .unwrap_or(trimmed);
    if no_prefix.len() != 64 || !no_prefix.chars().all(|c| c.is_ascii_hexdigit()) {
        return None;
    }
    Some(format!("0x{}", no_prefix.to_ascii_lowercase()))
}

fn gateway_reconcile_hex_qty_u64(value: u64) -> String {
    format!("0x{value:x}")
}

fn gateway_reconcile_hex_qty_u128(value: u128) -> String {
    format!("0x{value:x}")
}

fn gateway_reconcile_read_cursor(path: &Path, full_replay: bool) -> Result<u64> {
    if full_replay || !path.exists() {
        return Ok(0);
    }
    let raw = fs::read_to_string(path)
        .with_context(|| format!("read reconcile cursor failed: {}", path.display()))?;
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return Ok(0);
    }
    let value = trimmed.parse::<u64>().with_context(|| {
        format!(
            "invalid reconcile cursor value at {}: {}",
            path.display(),
            trimmed
        )
    })?;
    Ok(value)
}

fn gateway_reconcile_load_address_map(path: &Path) -> Result<HashMap<String, String>> {
    if !path.exists() {
        return Ok(HashMap::new());
    }
    let raw = fs::read_to_string(path)
        .with_context(|| format!("read reconcile address map failed: {}", path.display()))?;
    if raw.trim().is_empty() {
        return Ok(HashMap::new());
    }
    let json: serde_json::Value = serde_json::from_str(&raw)
        .with_context(|| format!("parse reconcile address map failed: {}", path.display()))?;
    let mut table = HashMap::new();
    if let Some(obj) = json.as_object() {
        for (k, v) in obj {
            if let Some(s) = v.as_str() {
                table.insert(k.clone(), s.to_string());
            }
        }
    }
    Ok(table)
}

fn gateway_reconcile_resolve_payout_address(
    node_id: &str,
    payout_account: &str,
    address_map: &HashMap<String, String>,
) -> Option<String> {
    gateway_reconcile_normalize_evm_address(payout_account)
        .or_else(|| {
            address_map
                .get(payout_account)
                .and_then(|v| gateway_reconcile_normalize_evm_address(v))
        })
        .or_else(|| {
            address_map
                .get(node_id)
                .and_then(|v| gateway_reconcile_normalize_evm_address(v))
        })
}

fn gateway_reconcile_rpc_call(
    cfg: &GatewayEmbeddedReconcileConfig,
    method: &str,
    params: serde_json::Value,
) -> Result<serde_json::Value> {
    let timeout = Duration::from_secs(cfg.rpc_timeout_sec.max(1));
    let payload = serde_json::json!({
        "jsonrpc": "2.0",
        "id": 1u64,
        "method": method,
        "params": params,
    });
    let response = ureq::post(&cfg.rpc_endpoint)
        .timeout(timeout)
        .send_json(payload)
        .with_context(|| {
            format!(
                "reconcile rpc call failed: endpoint={} method={}",
                cfg.rpc_endpoint, method
            )
        })?;
    let value: serde_json::Value = response.into_json().with_context(|| {
        format!(
            "reconcile rpc decode failed: endpoint={} method={}",
            cfg.rpc_endpoint, method
        )
    })?;
    if let Some(err) = value.get("error") {
        bail!(
            "reconcile rpc returned error: endpoint={} method={} error={}",
            cfg.rpc_endpoint,
            method,
            err
        );
    }
    Ok(value
        .get("result")
        .cloned()
        .unwrap_or(serde_json::Value::Null))
}

fn run_gateway_embedded_reconcile_cycle(
    cfg: &GatewayEmbeddedReconcileConfig,
    full_replay: bool,
) -> Result<GatewayEmbeddedReconcileCycleResult> {
    if !cfg.dispatch_index_file.exists() {
        bail!(
            "reconcile dispatch index file not found: {}",
            cfg.dispatch_index_file.display()
        );
    }
    if !cfg.submitted_index_file.exists() {
        bail!(
            "reconcile submitted index file not found: {}",
            cfg.submitted_index_file.display()
        );
    }

    let sender = gateway_reconcile_normalize_evm_address(&cfg.sender_address).ok_or_else(|| {
        anyhow::anyhow!("invalid reconcile sender address: {}", cfg.sender_address)
    })?;
    let cursor_dispatch_at = gateway_reconcile_read_cursor(&cfg.cursor_file, full_replay)?;
    let address_map = gateway_reconcile_load_address_map(&cfg.address_map_file)?;

    let dispatch_raw = fs::read_to_string(&cfg.dispatch_index_file).with_context(|| {
        format!(
            "read reconcile dispatch index failed: {}",
            cfg.dispatch_index_file.display()
        )
    })?;
    let mut dispatch_map = HashMap::<String, GatewayReconcileDispatchInstruction>::new();
    for (idx, line) in dispatch_raw.lines().enumerate() {
        if line.trim().is_empty() {
            continue;
        }
        let row: GatewayReconcileDispatchRecordLine =
            serde_json::from_str(line).with_context(|| {
                format!(
                    "parse reconcile dispatch index line failed: path={} line={}",
                    cfg.dispatch_index_file.display(),
                    idx + 1
                )
            })?;
        let voucher_id = row.voucher_id;
        for mut item in row.payout_instructions {
            if item.payout_id.trim().is_empty() {
                continue;
            }
            if item.node_id.trim().is_empty() {
                item.node_id = String::new();
            }
            if item.payout_account.trim().is_empty() {
                item.payout_account = String::new();
            }
            let key = item.payout_id.clone();
            if !voucher_id.trim().is_empty() && item.node_id.is_empty() {
                item.node_id = voucher_id.clone();
            }
            dispatch_map.insert(key, item);
        }
    }

    let mut state = if cfg.state_file.exists() {
        let raw = fs::read_to_string(&cfg.state_file).with_context(|| {
            format!(
                "read reconcile state file failed: {}",
                cfg.state_file.display()
            )
        })?;
        if raw.trim().is_empty() {
            GatewayReconcileStateRoot::default()
        } else {
            serde_json::from_str::<GatewayReconcileStateRoot>(&raw).with_context(|| {
                format!(
                    "parse reconcile state file failed: {}",
                    cfg.state_file.display()
                )
            })?
        }
    } else {
        GatewayReconcileStateRoot::default()
    };

    let submitted_raw = fs::read_to_string(&cfg.submitted_index_file).with_context(|| {
        format!(
            "read reconcile submitted index failed: {}",
            cfg.submitted_index_file.display()
        )
    })?;
    let mut submit_rows = Vec::<GatewayReconcileSubmittedItem>::new();
    let mut processed_submitted_records = 0u64;
    let mut last_dispatch_at = cursor_dispatch_at;
    for (idx, line) in submitted_raw.lines().enumerate() {
        if line.trim().is_empty() {
            continue;
        }
        let row: GatewayReconcileSubmittedRecordLine =
            serde_json::from_str(line).with_context(|| {
                format!(
                    "parse reconcile submitted index line failed: path={} line={}",
                    cfg.submitted_index_file.display(),
                    idx + 1
                )
            })?;
        if !full_replay && row.dispatch_created_at_unix_ms <= cursor_dispatch_at {
            continue;
        }
        processed_submitted_records = processed_submitted_records.saturating_add(1);
        if row.dispatch_created_at_unix_ms > last_dispatch_at {
            last_dispatch_at = row.dispatch_created_at_unix_ms;
        }
        submit_rows.extend(row.payout_submissions.into_iter());
    }

    for row in submit_rows {
        let payout_id = row.payout_id.trim().to_string();
        if payout_id.is_empty() {
            continue;
        }
        let mut voucher_id = row.voucher_id.trim().to_string();
        let mut node_id = row.node_id.trim().to_string();
        let mut payout_account = row.payout_account.trim().to_string();
        let mut reward_units = row.reward_units;
        if let Some(dispatch) = dispatch_map.get(&payout_id) {
            if voucher_id.is_empty() {
                voucher_id = dispatch.payout_id.clone();
            }
            if node_id.is_empty() {
                node_id = dispatch.node_id.clone();
            }
            if payout_account.is_empty() {
                payout_account = dispatch.payout_account.clone();
            }
            if reward_units == 0 {
                reward_units = dispatch.reward_units;
            }
        }
        let entry = state.payouts.entry(payout_id.clone()).or_insert_with(|| {
            GatewayReconcilePayoutStateEntry::new(
                &payout_id,
                &voucher_id,
                &node_id,
                &payout_account,
                reward_units,
            )
        });
        if !voucher_id.is_empty() {
            entry.voucher_id = voucher_id;
        }
        if !node_id.is_empty() {
            entry.node_id = node_id;
        }
        if !payout_account.is_empty() {
            entry.payout_account = payout_account;
        }
        if reward_units > 0 {
            entry.reward_units = reward_units;
        }
        if !row.status.trim().is_empty() {
            entry.status = row.status.trim().to_string();
        }
        entry.tx_hash_hex = gateway_reconcile_normalize_tx_hash(&row.tx_hash_hex);
        if row.submitted_at_unix_ms > 0 {
            entry.last_submit_at_unix_ms = row.submitted_at_unix_ms;
        }
        entry.last_error = if row.error.trim().is_empty() {
            None
        } else {
            Some(row.error)
        };
        if entry.status == "submitted_v1" {
            entry.submit_count = entry.submit_count.saturating_add(1);
        }
    }

    let now_ms = now_unix_millis() as u64;
    let cooldown_ms = cfg.replay_cooldown_sec.saturating_mul(1000);
    let mut confirmed_count = 0u64;
    let mut replayed_count = 0u64;
    let mut pending_count = 0u64;
    let mut error_count = 0u64;
    let mut changed = Vec::<GatewayReconcileChangedAction>::new();

    let payout_ids: Vec<String> = state.payouts.keys().cloned().collect();
    for payout_id in payout_ids {
        let Some(entry) = state.payouts.get_mut(&payout_id) else {
            continue;
        };
        if entry.status == "confirmed_v1" {
            continue;
        }
        let normalized_tx_hash = entry
            .tx_hash_hex
            .as_deref()
            .and_then(gateway_reconcile_normalize_tx_hash);
        let mut confirmed = false;
        if let Some(tx_hash) = normalized_tx_hash.as_ref() {
            match gateway_reconcile_rpc_call(cfg, &cfg.confirm_method, serde_json::json!([tx_hash]))
            {
                Ok(result) => {
                    if !result.is_null() {
                        let block_number = result
                            .get("blockNumber")
                            .and_then(|v| v.as_str())
                            .unwrap_or_default()
                            .to_string();
                        let transaction_hash = result
                            .get("transactionHash")
                            .and_then(|v| v.as_str())
                            .unwrap_or(tx_hash)
                            .to_string();
                        entry.status = "confirmed_v1".to_string();
                        entry.confirm_block_number_hex = if block_number.is_empty() {
                            None
                        } else {
                            Some(block_number)
                        };
                        entry.last_confirm_at_unix_ms = now_ms;
                        entry.tx_hash_hex = Some(transaction_hash.clone());
                        entry.last_error = None;
                        confirmed = true;
                        confirmed_count = confirmed_count.saturating_add(1);
                        changed.push(GatewayReconcileChangedAction {
                            payout_id: payout_id.clone(),
                            action: "confirm".to_string(),
                            tx_hash_hex: transaction_hash,
                            status: entry.status.clone(),
                        });
                    }
                }
                Err(e) => {
                    entry.last_error = Some(e.to_string());
                    error_count = error_count.saturating_add(1);
                }
            }
        }
        if confirmed {
            continue;
        }

        let due = cooldown_ms == 0
            || entry.last_submit_at_unix_ms == 0
            || now_ms.saturating_sub(entry.last_submit_at_unix_ms) >= cooldown_ms;
        let can_replay = entry.replay_count < cfg.replay_max_per_payout && due;
        if !can_replay {
            pending_count = pending_count.saturating_add(1);
            continue;
        }

        let Some(to_address) = gateway_reconcile_resolve_payout_address(
            &entry.node_id,
            &entry.payout_account,
            &address_map,
        ) else {
            entry.status = "replay_error_v1".to_string();
            entry.last_error = Some("target address unresolved in replay".to_string());
            error_count = error_count.saturating_add(1);
            continue;
        };

        let wei_amount =
            (entry.reward_units as u128).saturating_mul(cfg.wei_per_reward_unit as u128);
        let mut tx = serde_json::Map::new();
        tx.insert(
            "from".to_string(),
            serde_json::Value::String(sender.clone()),
        );
        tx.insert("to".to_string(), serde_json::Value::String(to_address));
        tx.insert(
            "value".to_string(),
            serde_json::Value::String(gateway_reconcile_hex_qty_u128(wei_amount)),
        );
        if cfg.gas_limit > 0 {
            tx.insert(
                "gas".to_string(),
                serde_json::Value::String(gateway_reconcile_hex_qty_u64(cfg.gas_limit)),
            );
        }
        if cfg.max_fee_per_gas_wei > 0 {
            tx.insert(
                "maxFeePerGas".to_string(),
                serde_json::Value::String(gateway_reconcile_hex_qty_u64(cfg.max_fee_per_gas_wei)),
            );
        }
        if cfg.max_priority_fee_per_gas_wei > 0 {
            tx.insert(
                "maxPriorityFeePerGas".to_string(),
                serde_json::Value::String(gateway_reconcile_hex_qty_u64(
                    cfg.max_priority_fee_per_gas_wei,
                )),
            );
        }

        match gateway_reconcile_rpc_call(
            cfg,
            &cfg.submit_method,
            serde_json::json!([serde_json::Value::Object(tx)]),
        ) {
            Ok(result) => {
                let tx_hash = result.as_str().unwrap_or_default().to_string();
                if let Some(norm_hash) = gateway_reconcile_normalize_tx_hash(&tx_hash) {
                    entry.tx_hash_hex = Some(norm_hash.clone());
                    entry.status = "submitted_v1".to_string();
                    entry.last_submit_at_unix_ms = now_ms;
                    entry.submit_count = entry.submit_count.saturating_add(1);
                    entry.replay_count = entry.replay_count.saturating_add(1);
                    entry.last_error = None;
                    replayed_count = replayed_count.saturating_add(1);
                    changed.push(GatewayReconcileChangedAction {
                        payout_id: payout_id.clone(),
                        action: "replay_submit".to_string(),
                        tx_hash_hex: norm_hash,
                        status: entry.status.clone(),
                    });
                } else {
                    entry.status = "replay_error_v1".to_string();
                    entry.replay_count = entry.replay_count.saturating_add(1);
                    entry.last_error = Some("empty replay rpc result".to_string());
                    error_count = error_count.saturating_add(1);
                }
            }
            Err(e) => {
                entry.status = "replay_error_v1".to_string();
                entry.replay_count = entry.replay_count.saturating_add(1);
                entry.last_error = Some(e.to_string());
                error_count = error_count.saturating_add(1);
            }
        }
    }

    state.updated_at_unix_ms = now_ms;
    if let Some(parent) = cfg.state_file.parent() {
        if !parent.as_os_str().is_empty() {
            fs::create_dir_all(parent).with_context(|| {
                format!(
                    "create reconcile state parent dir failed: {}",
                    parent.display()
                )
            })?;
        }
    }
    let state_bytes = serde_json::to_vec_pretty(&state)
        .with_context(|| "encode reconcile state json failed".to_string())?;
    fs::write(&cfg.state_file, state_bytes).with_context(|| {
        format!(
            "write reconcile state file failed: {}",
            cfg.state_file.display()
        )
    })?;

    fs::create_dir_all(&cfg.output_dir).with_context(|| {
        format!(
            "create reconcile output dir failed: {}",
            cfg.output_dir.display()
        )
    })?;
    if let Some(parent) = cfg.reconcile_index_file.parent() {
        if !parent.as_os_str().is_empty() {
            fs::create_dir_all(parent).with_context(|| {
                format!(
                    "create reconcile index parent dir failed: {}",
                    parent.display()
                )
            })?;
        }
    }
    if let Some(parent) = cfg.cursor_file.parent() {
        if !parent.as_os_str().is_empty() {
            fs::create_dir_all(parent).with_context(|| {
                format!(
                    "create reconcile cursor parent dir failed: {}",
                    parent.display()
                )
            })?;
        }
    }

    let reconcile = serde_json::json!({
        "version": 1u32,
        "created_at_unix_ms": now_ms,
        "rpc_endpoint": cfg.rpc_endpoint,
        "confirm_method": cfg.confirm_method,
        "submit_method": cfg.submit_method,
        "processed_submitted_records": processed_submitted_records,
        "payout_state_size": state.payouts.len() as u64,
        "confirmed_count": confirmed_count,
        "replayed_count": replayed_count,
        "pending_count": pending_count,
        "error_count": error_count,
        "changed": changed,
    });

    let snapshot_name = format!("reconcile-{now_ms}.json");
    let snapshot_path = cfg.output_dir.join(snapshot_name);
    let snapshot_bytes = serde_json::to_vec_pretty(&reconcile)
        .with_context(|| "encode reconcile snapshot failed".to_string())?;
    fs::write(&snapshot_path, snapshot_bytes).with_context(|| {
        format!(
            "write reconcile snapshot failed: {}",
            snapshot_path.display()
        )
    })?;

    let mut line_bytes = serde_json::to_vec(&reconcile)
        .with_context(|| "encode reconcile index line failed".to_string())?;
    line_bytes.push(b'\n');
    let mut index_writer = fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(&cfg.reconcile_index_file)
        .with_context(|| {
            format!(
                "open reconcile index file failed: {}",
                cfg.reconcile_index_file.display()
            )
        })?;
    index_writer.write_all(&line_bytes).with_context(|| {
        format!(
            "append reconcile index failed: {}",
            cfg.reconcile_index_file.display()
        )
    })?;

    fs::write(&cfg.cursor_file, last_dispatch_at.to_string()).with_context(|| {
        format!(
            "write reconcile cursor failed: {}",
            cfg.cursor_file.display()
        )
    })?;

    Ok(GatewayEmbeddedReconcileCycleResult {
        processed_submitted_records,
        payout_state_size: state.payouts.len(),
        confirmed_count,
        replayed_count,
        pending_count,
        error_count,
    })
}

fn main() -> Result<()> {
    let mut runtime = GatewayRuntime::from_env()?;
    let reconcile_cfg = load_gateway_embedded_reconcile_config()?;
    if let Some(cfg) = reconcile_cfg.as_ref() {
        gateway_summary!(
            "gateway_reconcile_in: enabled=true sender={} rpc_endpoint={} interval_seconds={} replay_max_per_payout={} replay_cooldown_sec={} dispatch_index={} submitted_index={} state_file={} reconcile_index={} cursor_file={} output_dir={} full_replay_first_cycle={}",
            cfg.sender_address,
            cfg.rpc_endpoint,
            cfg.interval_seconds,
            cfg.replay_max_per_payout,
            cfg.replay_cooldown_sec,
            cfg.dispatch_index_file.display(),
            cfg.submitted_index_file.display(),
            cfg.state_file.display(),
            cfg.reconcile_index_file.display(),
            cfg.cursor_file.display(),
            cfg.output_dir.display(),
            cfg.full_replay_first_cycle
        );
    }
    let mut reconcile_next_run_at_unix_ms = 0u128;
    let mut reconcile_first_cycle = true;
    gateway_summary!(
        "gateway_in: bind={} spool_dir={} max_body={} max_requests={} evm_payout_autoreplay_max={} evm_payout_autoreplay_cooldown_ms={} evm_payout_pending_warn_threshold={} evm_atomic_broadcast_autoreplay_max={} evm_atomic_broadcast_autoreplay_cooldown_ms={} evm_atomic_broadcast_pending_warn_threshold={} evm_atomic_broadcast_autoreplay_use_external_executor={} eth_public_broadcast_autoreplay_max={} eth_public_broadcast_autoreplay_cooldown_ms={} eth_public_broadcast_pending_warn_threshold={} eth_default_chain_id={} ua_store_backend={} ua_store_path={} eth_tx_index_backend={} eth_tx_index_path={} internal_ingress=ops_wire_v1",
        runtime.bind,
        runtime.spool_dir.display(),
        runtime.max_body_bytes,
        runtime.max_requests,
        runtime.evm_payout_autoreplay_max,
        runtime.evm_payout_autoreplay_cooldown_ms,
        runtime.evm_payout_pending_warn_threshold,
        runtime.evm_atomic_broadcast_autoreplay_max,
        runtime.evm_atomic_broadcast_autoreplay_cooldown_ms,
        runtime.evm_atomic_broadcast_pending_warn_threshold,
        runtime.evm_atomic_broadcast_autoreplay_use_external_executor,
        runtime.eth_public_broadcast_autoreplay_max,
        runtime.eth_public_broadcast_autoreplay_cooldown_ms,
        runtime.eth_public_broadcast_pending_warn_threshold,
        runtime.eth_default_chain_id,
        runtime.ua_store.backend_name(),
        runtime.ua_store.path().display(),
        runtime.eth_tx_index_store.backend_name(),
        runtime.eth_tx_index_store.path().display(),
    );
    auto_replay_pending_payouts(&mut runtime);
    auto_replay_pending_atomic_broadcasts(&mut runtime);
    auto_replay_pending_public_broadcasts(&mut runtime);
    ensure_gateway_eth_plugin_mempool_ingest_runtime(runtime.eth_default_chain_id);

    let run_result = (|| -> Result<()> {
        let server = tiny_http::Server::http(&runtime.bind).map_err(|e| {
            anyhow::anyhow!("start gateway server failed on {}: {}", runtime.bind, e)
        })?;
        let mut processed = 0u32;
        loop {
            if let Some(cfg) = reconcile_cfg.as_ref() {
                let now_ms = now_unix_millis();
                if now_ms >= reconcile_next_run_at_unix_ms {
                    let full_replay_this_cycle =
                        reconcile_first_cycle && cfg.full_replay_first_cycle;
                    reconcile_first_cycle = false;
                    match run_gateway_embedded_reconcile_cycle(cfg, full_replay_this_cycle) {
                        Ok(cycle) => {
                            gateway_summary!(
                                "gateway_reconcile_cycle_out: processed_submitted={} state_size={} confirmed={} replayed={} pending={} errors={}",
                                cycle.processed_submitted_records,
                                cycle.payout_state_size,
                                cycle.confirmed_count,
                                cycle.replayed_count,
                                cycle.pending_count,
                                cycle.error_count
                            );
                            let interval_ms =
                                (cfg.interval_seconds.max(1) as u128).saturating_mul(1000);
                            reconcile_next_run_at_unix_ms = now_ms.saturating_add(interval_ms);
                        }
                        Err(e) => {
                            gateway_warn!("gateway_warn: embedded reconcile cycle failed: {}", e);
                            let retry_ms =
                                (cfg.restart_delay_seconds.max(1) as u128).saturating_mul(1000);
                            reconcile_next_run_at_unix_ms = now_ms.saturating_add(retry_ms);
                        }
                    }
                }
            }

            match server.recv_timeout(Duration::from_millis(250)) {
                Ok(Some(request)) => {
                    handle_gateway_request(&mut runtime, request)?;
                    processed = processed.saturating_add(1);
                    if runtime.max_requests > 0 && processed >= runtime.max_requests {
                        break;
                    }
                }
                Ok(None) => {}
                Err(e) => {
                    bail!("gateway receive request failed: {}", e);
                }
            }
        }
        gateway_summary!(
            "gateway_out: bind={} processed={} max_requests={}",
            runtime.bind,
            processed,
            runtime.max_requests
        );
        Ok(())
    })();
    run_result
}

impl GatewayRuntime {
    fn from_env() -> Result<Self> {
        let bind = string_env("NOVOVM_GATEWAY_BIND", "127.0.0.1:9899");
        let spool_dir = PathBuf::from(string_env(
            "NOVOVM_GATEWAY_SPOOL_DIR",
            "artifacts/ingress/spool",
        ));
        let max_body_bytes = u64_env("NOVOVM_GATEWAY_MAX_BODY_BYTES", 64 * 1024) as usize;
        let max_requests = u32_env_allow_zero("NOVOVM_GATEWAY_MAX_REQUESTS", 0);
        let evm_payout_autoreplay_max =
            u32_env_allow_zero("NOVOVM_GATEWAY_EVM_PAYOUT_AUTOREPLAY_MAX", 0) as usize;
        let evm_payout_autoreplay_cooldown_ms =
            u64_env("NOVOVM_GATEWAY_EVM_PAYOUT_AUTOREPLAY_COOLDOWN_MS", 500);
        let evm_payout_pending_warn_threshold =
            u32_env_allow_zero("NOVOVM_GATEWAY_EVM_PAYOUT_PENDING_WARN_THRESHOLD", 512) as usize;
        let evm_payout_pending_hydrate_max =
            u32_env_allow_zero("NOVOVM_GATEWAY_EVM_PAYOUT_PENDING_HYDRATE_MAX", 1024) as usize;
        let evm_atomic_broadcast_autoreplay_max =
            u32_env_allow_zero("NOVOVM_GATEWAY_EVM_ATOMIC_BROADCAST_AUTOREPLAY_MAX", 0) as usize;
        let evm_atomic_broadcast_autoreplay_cooldown_ms = u64_env(
            "NOVOVM_GATEWAY_EVM_ATOMIC_BROADCAST_AUTOREPLAY_COOLDOWN_MS",
            500,
        );
        let evm_atomic_broadcast_pending_warn_threshold = u32_env_allow_zero(
            "NOVOVM_GATEWAY_EVM_ATOMIC_BROADCAST_PENDING_WARN_THRESHOLD",
            512,
        ) as usize;
        let evm_atomic_broadcast_autoreplay_use_external_executor = bool_env(
            "NOVOVM_GATEWAY_EVM_ATOMIC_BROADCAST_AUTOREPLAY_USE_EXTERNAL_EXECUTOR",
            false,
        );
        let eth_public_broadcast_autoreplay_max =
            u32_env_allow_zero("NOVOVM_GATEWAY_ETH_PUBLIC_BROADCAST_AUTOREPLAY_MAX", 0) as usize;
        let eth_public_broadcast_autoreplay_cooldown_ms = u64_env(
            "NOVOVM_GATEWAY_ETH_PUBLIC_BROADCAST_AUTOREPLAY_COOLDOWN_MS",
            500,
        );
        let eth_public_broadcast_pending_warn_threshold = u32_env_allow_zero(
            "NOVOVM_GATEWAY_ETH_PUBLIC_BROADCAST_PENDING_WARN_THRESHOLD",
            512,
        ) as usize;
        let eth_default_chain_id = u64_env("NOVOVM_GATEWAY_ETH_DEFAULT_CHAIN_ID", 1);
        let ua_store = resolve_gateway_ua_store_backend()?;
        let eth_tx_index_store = resolve_gateway_eth_tx_index_store_backend()?;
        let router = ua_store.load_router()?;
        ua_store
            .save_router(&router)
            .context("bootstrap gateway unified-account store writable check failed")?;
        let mut evm_pending_payout_by_settlement = HashMap::new();
        if evm_payout_pending_hydrate_max > 0 {
            match eth_tx_index_store
                .load_pending_payout_instructions(evm_payout_pending_hydrate_max)
            {
                Ok(items) => {
                    for item in items {
                        evm_pending_payout_by_settlement.insert(item.settlement_id.clone(), item);
                    }
                }
                Err(e) => {
                    gateway_warn!(
                        "gateway_warn: hydrate pending payouts failed: backend={} err={}",
                        eth_tx_index_store.backend_name(),
                        e
                    );
                }
            }
        }
        Ok(Self {
            bind,
            spool_dir,
            max_body_bytes,
            max_requests,
            evm_payout_autoreplay_max,
            evm_payout_autoreplay_cooldown_ms,
            evm_payout_pending_warn_threshold,
            evm_payout_last_autoreplay_at_ms: 0,
            evm_payout_last_warn_at_ms: 0,
            evm_atomic_broadcast_autoreplay_max,
            evm_atomic_broadcast_autoreplay_cooldown_ms,
            evm_atomic_broadcast_pending_warn_threshold,
            evm_atomic_broadcast_autoreplay_use_external_executor,
            evm_atomic_broadcast_last_autoreplay_at_ms: 0,
            evm_atomic_broadcast_last_warn_at_ms: 0,
            eth_public_broadcast_autoreplay_max,
            eth_public_broadcast_autoreplay_cooldown_ms,
            eth_public_broadcast_pending_warn_threshold,
            eth_public_broadcast_last_autoreplay_at_ms: 0,
            eth_public_broadcast_last_warn_at_ms: 0,
            eth_default_chain_id,
            ua_store,
            eth_tx_index_store,
            eth_tx_index: HashMap::new(),
            eth_filters: GatewayEthFilterState::default(),
            evm_settlement_index_by_id: HashMap::new(),
            evm_settlement_index_by_tx: HashMap::new(),
            evm_pending_payout_by_settlement,
            router,
        })
    }
}

fn evm_chain_type_for_gateway(chain_id: u64) -> novovm_adapter_api::ChainType {
    resolve_evm_chain_type_from_chain_id(chain_id)
}

fn atomic_ready_matches_tx_hash(item: &AtomicBroadcastReadyV1, tx_hash: &[u8]) -> bool {
    item.intent
        .legs
        .iter()
        .any(|leg| leg.hash.as_slice() == tx_hash)
}

fn atomic_reject_reasons_csv(receipts: &[AtomicIntentReceiptV1]) -> String {
    let reasons: Vec<String> = receipts
        .iter()
        .filter_map(|receipt| receipt.reason.as_deref())
        .map(str::trim)
        .filter(|reason| !reason.is_empty())
        .map(ToOwned::to_owned)
        .collect();
    if reasons.is_empty() {
        "rejected".to_string()
    } else {
        reasons.join(",")
    }
}

fn gateway_batch_worker_count(batch_len: usize) -> usize {
    if batch_len <= 1 {
        return 1;
    }
    let configured = std::env::var("NOVOVM_GATEWAY_BATCH_WORKERS")
        .ok()
        .and_then(|raw| raw.trim().parse::<usize>().ok())
        .unwrap_or(0);
    let auto = std::thread::available_parallelism()
        .map(|v| v.get())
        .unwrap_or(1)
        .clamp(1, GATEWAY_BATCH_WORKERS_DEFAULT_MAX);
    let workers = if configured == 0 { auto } else { configured };
    workers.clamp(1, batch_len)
}

fn gateway_eth_public_broadcast_status_item_json(
    eth_tx_index: &HashMap<[u8; 32], GatewayEthTxIndexEntry>,
    eth_tx_index_store: &GatewayEthTxIndexStoreBackend,
    tx_hash: [u8; 32],
    chain_hint: Option<u64>,
) -> serde_json::Value {
    let mut chain_id: Option<u64> = None;
    let mut known = false;
    if let Some(entry) = eth_tx_index.get(&tx_hash) {
        chain_id = Some(entry.chain_id);
        known = true;
    } else if let Ok(Some(entry)) = eth_tx_index_store.load_eth_tx(&tx_hash) {
        chain_id = Some(entry.chain_id);
        known = true;
    } else if let Some(tx) = find_gateway_eth_runtime_tx_by_hash(tx_hash, chain_hint) {
        let runtime_entry = gateway_eth_tx_index_entry_from_ir(tx);
        chain_id = Some(runtime_entry.chain_id);
        known = true;
    }
    if let (Some(actual_chain_id), Some(chain_hint)) = (chain_id, chain_hint) {
        if actual_chain_id != chain_hint {
            return serde_json::json!({
                "known": false,
                "tx_hash": format!("0x{}", to_hex(&tx_hash)),
                "chain_id": format!("0x{:x}", actual_chain_id),
                "has_status": false,
                "broadcast": serde_json::Value::Null,
            });
        }
    }
    let broadcast = gateway_eth_broadcast_status_json_by_tx(eth_tx_index_store, &tx_hash);
    serde_json::json!({
        "known": known,
        "tx_hash": format!("0x{}", to_hex(&tx_hash)),
        "chain_id": chain_id.map(|v| format!("0x{:x}", v)),
        "has_status": !broadcast.is_null(),
        "broadcast": broadcast,
    })
}

fn gateway_eth_tx_by_hash_item_json(
    eth_tx_index: &HashMap<[u8; 32], GatewayEthTxIndexEntry>,
    eth_tx_index_store: &GatewayEthTxIndexStoreBackend,
    tx_hash: [u8; 32],
    chain_hint: Option<u64>,
) -> Result<(serde_json::Value, Option<GatewayEthTxIndexEntry>)> {
    if let Some(entry) = eth_tx_index.get(&tx_hash) {
        if chain_hint.is_some_and(|chain_id| entry.chain_id != chain_id) {
            return Ok((serde_json::Value::Null, None));
        }
        return Ok((
            gateway_eth_tx_by_hash_query_json(entry, eth_tx_index, eth_tx_index_store)?,
            None,
        ));
    }
    if let Some(entry) = eth_tx_index_store.load_eth_tx(&tx_hash)? {
        if chain_hint.is_some_and(|chain_id| entry.chain_id != chain_id) {
            return Ok((serde_json::Value::Null, None));
        }
        return Ok((
            gateway_eth_tx_by_hash_query_json(&entry, eth_tx_index, eth_tx_index_store)?,
            Some(entry),
        ));
    }
    if let Some(tx) = find_gateway_eth_runtime_tx_by_hash(tx_hash, chain_hint) {
        let pending_entry = gateway_eth_tx_index_entry_from_ir(tx);
        let chain_entries = collect_gateway_eth_chain_entries(
            eth_tx_index,
            eth_tx_index_store,
            pending_entry.chain_id,
            gateway_eth_query_scan_max(),
        )?;
        let latest_block_number = resolve_gateway_eth_latest_block_number(
            pending_entry.chain_id,
            &chain_entries,
            eth_tx_index_store,
        )?;
        let mut pending_view_index = eth_tx_index.clone();
        if let Some((pending_block_number, pending_block_hash, pending_entries)) =
            resolve_gateway_eth_pending_block_for_runtime_view(
                pending_entry.chain_id,
                latest_block_number,
                &mut pending_view_index,
                eth_tx_index_store,
            )?
        {
            if let Some(tx_index) = pending_entries
                .iter()
                .position(|entry| entry.tx_hash == tx_hash)
            {
                return Ok((
                    gateway_eth_tx_pending_with_block_json(
                        &pending_entries[tx_index],
                        pending_block_number,
                        tx_index,
                        &pending_block_hash,
                    ),
                    None,
                ));
            }
        }
        return Ok((gateway_eth_tx_by_hash_json(&pending_entry), None));
    }
    Ok((serde_json::Value::Null, None))
}

fn gateway_eth_tx_receipt_item_json(
    eth_tx_index: &HashMap<[u8; 32], GatewayEthTxIndexEntry>,
    eth_tx_index_store: &GatewayEthTxIndexStoreBackend,
    tx_hash: [u8; 32],
    chain_hint: Option<u64>,
) -> Result<(serde_json::Value, Option<GatewayEthTxIndexEntry>)> {
    if let Some(entry) = eth_tx_index.get(&tx_hash) {
        if chain_hint.is_some_and(|chain_id| entry.chain_id != chain_id) {
            return Ok((serde_json::Value::Null, None));
        }
        return Ok((
            gateway_eth_tx_receipt_query_json(entry, eth_tx_index, eth_tx_index_store)?,
            None,
        ));
    }
    if let Some(entry) = eth_tx_index_store.load_eth_tx(&tx_hash)? {
        if chain_hint.is_some_and(|chain_id| entry.chain_id != chain_id) {
            return Ok((serde_json::Value::Null, None));
        }
        return Ok((
            gateway_eth_tx_receipt_query_json(&entry, eth_tx_index, eth_tx_index_store)?,
            Some(entry),
        ));
    }
    if let Some(tx) = find_gateway_eth_runtime_tx_by_hash(tx_hash, chain_hint) {
        let pending_entry = gateway_eth_tx_index_entry_from_ir(tx);
        let chain_entries = collect_gateway_eth_chain_entries(
            eth_tx_index,
            eth_tx_index_store,
            pending_entry.chain_id,
            gateway_eth_query_scan_max(),
        )?;
        let latest_block_number = resolve_gateway_eth_latest_block_number(
            pending_entry.chain_id,
            &chain_entries,
            eth_tx_index_store,
        )?;
        let mut pending_view_index = eth_tx_index.clone();
        if let Some((pending_block_number, pending_block_hash, pending_entries)) =
            resolve_gateway_eth_pending_block_for_runtime_view(
                pending_entry.chain_id,
                latest_block_number,
                &mut pending_view_index,
                eth_tx_index_store,
            )?
        {
            if let Some(receipt) = gateway_eth_tx_receipt_pending_query_json_by_hash(
                pending_block_number,
                &pending_block_hash,
                &pending_entries,
                &tx_hash,
            ) {
                return Ok((receipt, None));
            }
        }
        return Ok((gateway_eth_tx_receipt_json(&pending_entry), None));
    }
    Ok((serde_json::Value::Null, None))
}

fn gateway_eth_replay_public_broadcast_item_json(
    eth_tx_index: &HashMap<[u8; 32], GatewayEthTxIndexEntry>,
    eth_tx_index_store: &GatewayEthTxIndexStoreBackend,
    tx_hash_raw: &str,
    chain_hint: Option<u64>,
) -> Result<(serde_json::Value, Option<GatewayEthTxIndexEntry>)> {
    let tx_hash_bytes = decode_hex_bytes(tx_hash_raw, "tx_hash")?;
    let tx_hash = vec_to_32(&tx_hash_bytes, "tx_hash")?;

    let mut cache_entry = None;
    let mut entry = eth_tx_index.get(&tx_hash).cloned();
    if entry.is_none() {
        entry = eth_tx_index_store.load_eth_tx(&tx_hash)?;
        if let Some(cached) = entry.as_ref() {
            cache_entry = Some(cached.clone());
        }
    }
    if entry.is_none() {
        if let Some(tx) = find_gateway_eth_runtime_tx_by_hash(tx_hash, chain_hint) {
            entry = Some(gateway_eth_tx_index_entry_from_ir(tx));
        }
    }
    let Some(entry) = entry else {
        return Ok((serde_json::Value::Null, cache_entry));
    };
    if chain_hint.is_some_and(|chain_id| entry.chain_id != chain_id) {
        return Ok((serde_json::Value::Null, cache_entry));
    }

    let tx_ir = gateway_eth_tx_ir_from_index_entry(&entry);
    let tx_ir_bincode = tx_ir
        .serialize(SerializationFormat::Bincode)
        .context("serialize eth tx ir bincode for replay public broadcast failed")?;
    let broadcast_result = maybe_execute_gateway_eth_public_broadcast(
        entry.chain_id,
        &entry.tx_hash,
        GatewayEthPublicBroadcastPayload {
            raw_tx: None,
            tx_ir_bincode: Some(tx_ir_bincode.as_slice()),
        },
        true,
    )?;
    upsert_gateway_eth_broadcast_status(
        eth_tx_index_store,
        entry.chain_id,
        entry.tx_hash,
        &broadcast_result,
    );
    let broadcast_json = match broadcast_result {
        Some((output, attempts, executor)) => serde_json::json!({
            "mode": "external",
            "attempts": attempts,
            "executor": executor,
            "executor_output": output,
        }),
        None => serde_json::json!({
            "mode": "none",
        }),
    };
    Ok((
        serde_json::json!({
            "replayed": true,
            "tx_hash": format!("0x{}", to_hex(&entry.tx_hash)),
            "chain_id": format!("0x{:x}", entry.chain_id),
            "broadcast": broadcast_json,
        }),
        cache_entry,
    ))
}

fn gateway_eth_tx_lifecycle_item_json(
    eth_tx_index: &HashMap<[u8; 32], GatewayEthTxIndexEntry>,
    eth_tx_index_store: &GatewayEthTxIndexStoreBackend,
    tx_hash: [u8; 32],
    chain_hint: Option<u64>,
) -> Result<(serde_json::Value, Option<GatewayEthTxIndexEntry>)> {
    let broadcast = gateway_eth_broadcast_status_json_by_tx(eth_tx_index_store, &tx_hash);

    if let Some(tx) = find_gateway_eth_runtime_tx_by_hash(tx_hash, chain_hint) {
        let pending_entry = gateway_eth_tx_index_entry_from_ir(tx);
        let chain_entries = collect_gateway_eth_chain_entries(
            eth_tx_index,
            eth_tx_index_store,
            pending_entry.chain_id,
            gateway_eth_query_scan_max(),
        )?;
        let latest_block_number = resolve_gateway_eth_latest_block_number(
            pending_entry.chain_id,
            &chain_entries,
            eth_tx_index_store,
        )?;
        let mut pending_view_index = eth_tx_index.clone();
        let receipt = if let Some((pending_block_number, pending_block_hash, pending_entries)) =
            resolve_gateway_eth_pending_block_for_runtime_view(
                pending_entry.chain_id,
                latest_block_number,
                &mut pending_view_index,
                eth_tx_index_store,
            )? {
            gateway_eth_tx_receipt_pending_query_json_by_hash(
                pending_block_number,
                &pending_block_hash,
                &pending_entries,
                &tx_hash,
            )
            .unwrap_or_else(|| gateway_eth_tx_receipt_json(&pending_entry))
        } else {
            gateway_eth_tx_receipt_json(&pending_entry)
        };
        persist_gateway_eth_submit_success_status(
            eth_tx_index_store,
            tx_hash,
            pending_entry.chain_id,
            true,
            false,
        );
        return Ok((
            serde_json::json!({
                "accepted": true,
                "pending": true,
                "onchain": false,
                "stage": "pending",
                "terminal": false,
                "failed": false,
                "tx_hash": format!("0x{}", to_hex(&tx_hash)),
                "chain_id": format!("0x{:x}", pending_entry.chain_id),
                "receipt_pending": receipt.get("pending").cloned().unwrap_or(serde_json::Value::Null),
                "receipt_status": receipt.get("status").cloned().unwrap_or(serde_json::Value::Null),
                "broadcast_mode": broadcast.get("mode").cloned().unwrap_or(serde_json::Value::Null),
                "receipt": receipt,
                "broadcast": broadcast,
                "error_code": serde_json::Value::Null,
                "error_reason": serde_json::Value::Null,
            }),
            None,
        ));
    }

    let mut cache_entry = None;
    let mut entry = eth_tx_index.get(&tx_hash).cloned();
    if entry.is_none() {
        entry = eth_tx_index_store.load_eth_tx(&tx_hash)?;
        if let Some(cached) = entry.as_ref() {
            cache_entry = Some(cached.clone());
        }
    }
    let submit_status = gateway_eth_submit_status_by_tx(eth_tx_index_store, &tx_hash);
    let Some(entry) = entry else {
        if let Some(status) = submit_status {
            if chain_hint
                .zip(status.chain_id)
                .is_some_and(|(hint, status_chain)| hint != status_chain)
            {
                return Ok((
                    serde_json::json!({
                        "accepted": false,
                        "pending": false,
                        "onchain": false,
                        "stage": "rejected",
                        "terminal": true,
                        "failed": true,
                        "tx_hash": format!("0x{}", to_hex(&tx_hash)),
                        "chain_id": status.chain_id.map(|value| format!("0x{:x}", value)),
                        "receipt_pending": serde_json::Value::Null,
                        "receipt_status": serde_json::Value::Null,
                        "broadcast_mode": broadcast.get("mode").cloned().unwrap_or(serde_json::Value::Null),
                        "receipt": serde_json::Value::Null,
                        "broadcast": broadcast,
                        "error_code": "CHAIN_MISMATCH",
                        "error_reason": "transaction exists but chain_id mismatch",
                        "updated_at_unix_ms": status.updated_at_unix_ms,
                    }),
                    cache_entry,
                ));
            }
            let stage = if status.pending {
                "pending"
            } else if status.onchain && status.error_code.as_deref() == Some("ONCHAIN_FAILED") {
                "onchain_failed"
            } else if status.onchain {
                "onchain"
            } else if status.accepted {
                "accepted"
            } else if status.error_code.is_some() {
                "failed"
            } else {
                "rejected"
            };
            let terminal = !matches!(stage, "pending" | "accepted");
            let failed = matches!(stage, "onchain_failed" | "failed" | "rejected");
            let (error_code, error_reason) = if failed {
                let fallback_code = if stage == "onchain_failed" {
                    "ONCHAIN_FAILED"
                } else if stage == "failed" {
                    "SUBMIT_FAILED"
                } else {
                    "SUBMIT_REJECTED"
                };
                let fallback_reason = if stage == "onchain_failed" {
                    "transaction failed onchain"
                } else if stage == "failed" {
                    "submit failed"
                } else {
                    "submit rejected"
                };
                (
                    status
                        .error_code
                        .clone()
                        .unwrap_or_else(|| fallback_code.to_string()),
                    status
                        .error_reason
                        .clone()
                        .unwrap_or_else(|| fallback_reason.to_string()),
                )
            } else {
                (String::new(), String::new())
            };
            return Ok((
                serde_json::json!({
                    "accepted": status.accepted,
                    "pending": status.pending,
                    "onchain": status.onchain,
                    "stage": stage,
                    "terminal": terminal,
                    "failed": failed,
                    "tx_hash": format!("0x{}", to_hex(&tx_hash)),
                    "chain_id": status.chain_id.map(|value| format!("0x{:x}", value)),
                    "receipt_pending": serde_json::Value::Null,
                    "receipt_status": serde_json::Value::Null,
                    "broadcast_mode": broadcast.get("mode").cloned().unwrap_or(serde_json::Value::Null),
                    "receipt": serde_json::Value::Null,
                    "broadcast": broadcast,
                    "error_code": if failed { serde_json::Value::String(error_code) } else { serde_json::Value::Null },
                    "error_reason": if failed { serde_json::Value::String(error_reason) } else { serde_json::Value::Null },
                    "updated_at_unix_ms": status.updated_at_unix_ms,
                }),
                cache_entry,
            ));
        }
        return Ok((
            serde_json::json!({
                "accepted": false,
                "pending": false,
                "onchain": false,
                "stage": "not_found",
                "terminal": true,
                "failed": true,
                "tx_hash": format!("0x{}", to_hex(&tx_hash)),
                "chain_id": serde_json::Value::Null,
                "receipt_pending": serde_json::Value::Null,
                "receipt_status": serde_json::Value::Null,
                "broadcast_mode": broadcast.get("mode").cloned().unwrap_or(serde_json::Value::Null),
                "receipt": serde_json::Value::Null,
                "broadcast": broadcast,
                "error_code": "TX_NOT_FOUND",
                "error_reason": "transaction not found",
            }),
            cache_entry,
        ));
    };
    if chain_hint.is_some_and(|chain_id| entry.chain_id != chain_id) {
        return Ok((
            serde_json::json!({
                "accepted": false,
                "pending": false,
                "onchain": false,
                "stage": "rejected",
                "terminal": true,
                "failed": true,
                "tx_hash": format!("0x{}", to_hex(&tx_hash)),
                "chain_id": format!("0x{:x}", entry.chain_id),
                "receipt_pending": serde_json::Value::Null,
                "receipt_status": serde_json::Value::Null,
                "broadcast_mode": broadcast.get("mode").cloned().unwrap_or(serde_json::Value::Null),
                "receipt": serde_json::Value::Null,
                "broadcast": broadcast,
                "error_code": "CHAIN_MISMATCH",
                "error_reason": "transaction exists but chain_id mismatch",
            }),
            cache_entry,
        ));
    }
    let receipt = gateway_eth_tx_receipt_query_json(&entry, eth_tx_index, eth_tx_index_store)?;
    let pending = receipt
        .get("pending")
        .and_then(serde_json::Value::as_bool)
        .unwrap_or(false);
    let stage = if pending {
        "pending"
    } else if receipt.get("status").and_then(serde_json::Value::as_str) == Some("0x0") {
        "onchain_failed"
    } else {
        "onchain"
    };
    let failed = stage == "onchain_failed";
    if pending {
        persist_gateway_eth_submit_success_status(
            eth_tx_index_store,
            tx_hash,
            entry.chain_id,
            true,
            false,
        );
    } else {
        persist_gateway_eth_submit_onchain_status(
            eth_tx_index_store,
            tx_hash,
            entry.chain_id,
            failed,
        );
    }
    Ok((
        serde_json::json!({
            "accepted": true,
            "pending": pending,
            "onchain": !pending,
            "stage": stage,
            "terminal": !pending,
            "failed": failed,
            "tx_hash": format!("0x{}", to_hex(&tx_hash)),
            "chain_id": format!("0x{:x}", entry.chain_id),
            "receipt_pending": receipt.get("pending").cloned().unwrap_or(serde_json::Value::Null),
            "receipt_status": receipt.get("status").cloned().unwrap_or(serde_json::Value::Null),
            "broadcast_mode": broadcast.get("mode").cloned().unwrap_or(serde_json::Value::Null),
            "receipt": receipt,
            "broadcast": broadcast,
            "error_code": serde_json::Value::Null,
            "error_reason": serde_json::Value::Null,
        }),
        cache_entry,
    ))
}

#[derive(Clone, Copy)]
struct GatewayEthUpstreamTxStatusInclude {
    lifecycle: bool,
    receipts: bool,
    broadcast: bool,
}

type GatewayEthUpstreamTxStatusChunk =
    (Vec<(usize, serde_json::Value)>, Vec<GatewayEthTxIndexEntry>);

fn gateway_eth_upstream_tx_status_item_json(
    eth_tx_index: &HashMap<[u8; 32], GatewayEthTxIndexEntry>,
    eth_tx_index_store: &GatewayEthTxIndexStoreBackend,
    chain_id: u64,
    tx_hash_raw: &str,
    tx_hash: [u8; 32],
    include: GatewayEthUpstreamTxStatusInclude,
) -> Result<(serde_json::Value, Vec<GatewayEthTxIndexEntry>)> {
    let chain_hint = Some(chain_id);
    let mut cache_entries = Vec::with_capacity(2);
    let lifecycle = if include.lifecycle {
        let (item, cache_entry) = gateway_eth_tx_lifecycle_item_json(
            eth_tx_index,
            eth_tx_index_store,
            tx_hash,
            chain_hint,
        )?;
        if let Some(entry) = cache_entry {
            cache_entries.push(entry);
        }
        item
    } else {
        serde_json::Value::Null
    };
    let receipt = if include.receipts {
        let (item, cache_entry) = gateway_eth_tx_receipt_item_json(
            eth_tx_index,
            eth_tx_index_store,
            tx_hash,
            chain_hint,
        )?;
        if let Some(entry) = cache_entry {
            cache_entries.push(entry);
        }
        item
    } else {
        serde_json::Value::Null
    };
    let broadcast = if include.broadcast {
        gateway_eth_public_broadcast_status_item_json(
            eth_tx_index,
            eth_tx_index_store,
            tx_hash,
            chain_hint,
        )
    } else {
        serde_json::Value::Null
    };

    let accepted = lifecycle
        .get("accepted")
        .cloned()
        .unwrap_or(serde_json::Value::Null);
    let pending = lifecycle
        .get("pending")
        .cloned()
        .unwrap_or(serde_json::Value::Null);
    let onchain = lifecycle
        .get("onchain")
        .cloned()
        .unwrap_or(serde_json::Value::Null);
    let error_code = lifecycle
        .get("error_code")
        .cloned()
        .unwrap_or(serde_json::Value::Null);
    let error_reason = lifecycle
        .get("error_reason")
        .cloned()
        .unwrap_or(serde_json::Value::Null);
    let receipt_pending = receipt
        .get("pending")
        .cloned()
        .unwrap_or(serde_json::Value::Null);
    let receipt_status = receipt
        .get("status")
        .cloned()
        .unwrap_or(serde_json::Value::Null);
    let broadcast_mode = broadcast
        .get("mode")
        .cloned()
        .unwrap_or(serde_json::Value::Null);
    let accepted_bool = accepted.as_bool();
    let pending_bool = pending.as_bool();
    let onchain_bool = onchain.as_bool();
    let error_code_str = error_code.as_str();
    let stage = if onchain_bool == Some(true) {
        if receipt_status.as_str() == Some("0x0") {
            "onchain_failed"
        } else {
            "onchain"
        }
    } else if pending_bool == Some(true) {
        "pending"
    } else if accepted_bool == Some(true) {
        "accepted"
    } else if accepted_bool == Some(false) {
        if error_code_str.is_some() {
            "failed"
        } else {
            "rejected"
        }
    } else {
        "unknown"
    };
    let terminal = matches!(stage, "onchain" | "onchain_failed" | "failed" | "rejected");
    let failed = matches!(stage, "onchain_failed" | "failed" | "rejected");
    Ok((
        serde_json::json!({
            "tx_hash": tx_hash_raw,
            "chain_id": format!("0x{:x}", chain_id),
            "accepted": accepted,
            "pending": pending,
            "onchain": onchain,
            "error_code": error_code,
            "error_reason": error_reason,
            "stage": stage,
            "terminal": terminal,
            "failed": failed,
            "receipt_pending": receipt_pending,
            "receipt_status": receipt_status,
            "broadcast_mode": broadcast_mode,
            "lifecycle": lifecycle,
            "receipt": receipt,
            "broadcast": broadcast,
        }),
        cache_entries,
    ))
}

fn gateway_eth_logs_item_json(
    eth_tx_index: &HashMap<[u8; 32], GatewayEthTxIndexEntry>,
    eth_tx_index_store: &GatewayEthTxIndexStoreBackend,
    eth_default_chain_id: u64,
    params: &serde_json::Value,
) -> Result<serde_json::Value> {
    let chain_id =
        param_as_u64_any_with_tx(params, &["chain_id", "chainId"]).unwrap_or(eth_default_chain_id);
    let entries = collect_gateway_eth_chain_entries(
        eth_tx_index,
        eth_tx_index_store,
        chain_id,
        gateway_eth_query_scan_max(),
    )?;
    let latest = resolve_gateway_eth_latest_block_number(chain_id, &entries, eth_tx_index_store)?;
    let query = parse_eth_logs_query_from_params(params, latest)?;
    let logs = collect_gateway_eth_logs_with_query(
        chain_id,
        entries,
        &query,
        eth_tx_index,
        eth_tx_index_store,
    )?;
    Ok(serde_json::Value::Array(logs))
}

fn gateway_eth_filter_logs_item_json(
    eth_tx_index: &HashMap<[u8; 32], GatewayEthTxIndexEntry>,
    eth_tx_index_store: &GatewayEthTxIndexStoreBackend,
    filters: &HashMap<u64, GatewayEthFilterKind>,
    params: &serde_json::Value,
) -> Result<serde_json::Value> {
    let filter_id = parse_eth_filter_id(params)
        .ok_or_else(|| anyhow::anyhow!("filter_id (or id) is required"))?;
    let Some(filter) = filters.get(&filter_id).cloned() else {
        bail!("filter not found: 0x{:x}", filter_id);
    };
    match filter {
        GatewayEthFilterKind::Logs(log_filter) => {
            let entries = collect_gateway_eth_chain_entries(
                eth_tx_index,
                eth_tx_index_store,
                log_filter.chain_id,
                gateway_eth_query_scan_max(),
            )?;
            let logs = collect_gateway_eth_logs_with_query(
                log_filter.chain_id,
                entries,
                &log_filter.query,
                eth_tx_index,
                eth_tx_index_store,
            )?;
            Ok(serde_json::Value::Array(logs))
        }
        GatewayEthFilterKind::Blocks { .. } | GatewayEthFilterKind::PendingTransactions { .. } => {
            bail!("filter does not support logs: 0x{:x}", filter_id)
        }
    }
}

fn gateway_eth_filter_changes_item_json(
    eth_tx_index: &mut HashMap<[u8; 32], GatewayEthTxIndexEntry>,
    eth_tx_index_store: &GatewayEthTxIndexStoreBackend,
    eth_filters: &mut GatewayEthFilterState,
    filter_id: u64,
) -> Result<serde_json::Value> {
    let Some(mut filter) = eth_filters.filters.get(&filter_id).cloned() else {
        bail!("filter not found: 0x{:x}", filter_id);
    };
    let response = match &mut filter {
        GatewayEthFilterKind::Logs(log_filter) => {
            let entries = collect_gateway_eth_chain_entries(
                eth_tx_index,
                eth_tx_index_store,
                log_filter.chain_id,
                gateway_eth_query_scan_max(),
            )?;
            if log_filter.query.block_hash.is_some() {
                if log_filter.block_hash_drained {
                    serde_json::Value::Array(Vec::new())
                } else {
                    log_filter.block_hash_drained = true;
                    serde_json::Value::Array(collect_gateway_eth_logs_with_query(
                        log_filter.chain_id,
                        entries,
                        &log_filter.query,
                        eth_tx_index,
                        eth_tx_index_store,
                    )?)
                }
            } else {
                let latest = resolve_gateway_eth_latest_block_number(
                    log_filter.chain_id,
                    &entries,
                    eth_tx_index_store,
                )?;
                let has_runtime_pending = log_filter.query.include_pending_block
                    && resolve_gateway_eth_pending_block_for_runtime_view(
                        log_filter.chain_id,
                        latest,
                        eth_tx_index,
                        eth_tx_index_store,
                    )?
                    .is_some();
                let max_visible_block = if has_runtime_pending {
                    latest.saturating_add(1)
                } else {
                    latest
                };
                let from = log_filter
                    .query
                    .from_block
                    .unwrap_or(0)
                    .max(log_filter.next_block);
                let requested_to = log_filter.query.to_block.unwrap_or(max_visible_block);
                let to = requested_to.min(max_visible_block);
                if from > to {
                    serde_json::Value::Array(Vec::new())
                } else {
                    let mut delta_query = log_filter.query.clone();
                    delta_query.from_block = Some(from);
                    delta_query.to_block = Some(to);
                    log_filter.next_block = to.saturating_add(1);
                    serde_json::Value::Array(collect_gateway_eth_logs_with_query(
                        log_filter.chain_id,
                        entries,
                        &delta_query,
                        eth_tx_index,
                        eth_tx_index_store,
                    )?)
                }
            }
        }
        GatewayEthFilterKind::Blocks {
            chain_id,
            last_seen_block,
        } => {
            let entries = collect_gateway_eth_chain_entries(
                eth_tx_index,
                eth_tx_index_store,
                *chain_id,
                gateway_eth_query_scan_max(),
            )?;
            let latest =
                resolve_gateway_eth_latest_block_number(*chain_id, &entries, eth_tx_index_store)?;
            if latest <= *last_seen_block {
                serde_json::Value::Array(Vec::new())
            } else {
                let mut emitted_blocks = BTreeSet::<u64>::new();
                let mut out = Vec::new();
                for (block_number, block_txs) in gateway_eth_group_entries_by_block(entries) {
                    if block_number <= *last_seen_block {
                        continue;
                    }
                    let block_hash =
                        gateway_eth_block_hash_for_txs(*chain_id, block_number, &block_txs);
                    out.push(serde_json::Value::String(format!(
                        "0x{}",
                        to_hex(&block_hash)
                    )));
                    emitted_blocks.insert(block_number);
                }

                let mut recover_start = last_seen_block.saturating_add(1);
                let recover_cap = gateway_eth_query_scan_max() as u64;
                let span = latest.saturating_sub(recover_start).saturating_add(1);
                if span > recover_cap {
                    recover_start = latest.saturating_sub(recover_cap.saturating_sub(1));
                }
                for block_number in recover_start..=latest {
                    if emitted_blocks.contains(&block_number) {
                        continue;
                    }
                    let precise_block_txs = collect_gateway_eth_block_entries_precise(
                        eth_tx_index,
                        eth_tx_index_store,
                        *chain_id,
                        block_number,
                        gateway_eth_query_scan_max(),
                    )?;
                    if precise_block_txs.is_empty() {
                        continue;
                    }
                    let block_hash =
                        gateway_eth_block_hash_for_txs(*chain_id, block_number, &precise_block_txs);
                    out.push(serde_json::Value::String(format!(
                        "0x{}",
                        to_hex(&block_hash)
                    )));
                }

                *last_seen_block = latest.max(*last_seen_block);
                serde_json::Value::Array(out)
            }
        }
        GatewayEthFilterKind::PendingTransactions {
            chain_id,
            last_seen_hashes,
        } => {
            let current_hashes = collect_gateway_eth_pending_hashes_runtime(*chain_id);
            let hashes = current_hashes
                .difference(last_seen_hashes)
                .map(|hash| serde_json::Value::String(format!("0x{}", to_hex(hash))))
                .collect::<Vec<serde_json::Value>>();
            *last_seen_hashes = current_hashes;
            serde_json::Value::Array(hashes)
        }
    };
    eth_filters.filters.insert(filter_id, filter);
    Ok(response)
}

fn gateway_eth_pending_transactions_json(chain_id: u64) -> serde_json::Value {
    let (pending_txs, queued_txs) = collect_gateway_eth_txpool_runtime_txs(chain_id);
    if pending_txs.is_empty() && queued_txs.is_empty() {
        return serde_json::Value::Array(Vec::new());
    }
    let mut pending = Vec::<serde_json::Value>::with_capacity(pending_txs.len() + queued_txs.len());
    pending.extend(
        pending_txs
            .iter()
            .map(gateway_eth_pending_tx_by_hash_json_from_ir),
    );
    pending.extend(
        queued_txs
            .iter()
            .map(gateway_eth_pending_tx_by_hash_json_from_ir),
    );
    serde_json::Value::Array(pending)
}

fn gateway_eth_txpool_status_json(chain_id: u64) -> serde_json::Value {
    let (pending_txs, queued_txs) = collect_gateway_eth_txpool_runtime_txs(chain_id);
    if pending_txs.is_empty() && queued_txs.is_empty() {
        return serde_json::json!({
            "pending": "0x0",
            "queued": "0x0",
        });
    }
    build_gateway_eth_txpool_status_from_ir(&pending_txs, &queued_txs)
}

fn gateway_eth_upstream_status_arrays_json(
    eth_tx_index: &mut HashMap<[u8; 32], GatewayEthTxIndexEntry>,
    eth_tx_index_store: &GatewayEthTxIndexStoreBackend,
    chain_id: u64,
    tx_hashes: &[String],
    include: GatewayEthUpstreamTxStatusInclude,
) -> Result<(serde_json::Value, serde_json::Value, serde_json::Value)> {
    if tx_hashes.is_empty() || (!include.lifecycle && !include.receipts && !include.broadcast) {
        return Ok((
            serde_json::Value::Array(Vec::new()),
            serde_json::Value::Array(Vec::new()),
            serde_json::Value::Array(Vec::new()),
        ));
    }

    let mut indexed_hashes: Vec<(usize, String, [u8; 32])> = Vec::with_capacity(tx_hashes.len());
    for (idx, tx_hash) in tx_hashes.iter().enumerate() {
        let tx_hash_bytes = decode_hex_bytes(tx_hash, "tx_hash")?;
        indexed_hashes.push((idx, tx_hash.clone(), vec_to_32(&tx_hash_bytes, "tx_hash")?));
    }
    let workers = gateway_batch_worker_count(indexed_hashes.len());
    let eth_tx_index_snapshot = eth_tx_index.clone();
    let mut ordered_items: Vec<(usize, serde_json::Value)> =
        Vec::with_capacity(indexed_hashes.len());
    let mut cache_entries: Vec<GatewayEthTxIndexEntry> = Vec::new();
    if workers <= 1 || indexed_hashes.len() <= 1 {
        for (idx, tx_hash, tx_hash_bytes) in indexed_hashes {
            let (item, mut local_cache_entries) = gateway_eth_upstream_tx_status_item_json(
                &eth_tx_index_snapshot,
                eth_tx_index_store,
                chain_id,
                &tx_hash,
                tx_hash_bytes,
                include,
            )?;
            ordered_items.push((idx, item));
            cache_entries.append(&mut local_cache_entries);
        }
    } else {
        let chunk_size = indexed_hashes.len().div_ceil(workers).max(1);
        let mut chunk_results: Vec<GatewayEthUpstreamTxStatusChunk> = Vec::new();
        std::thread::scope(|scope| -> Result<()> {
            let mut handles = Vec::new();
            for chunk in indexed_hashes.chunks(chunk_size) {
                let chunk_items = chunk.to_vec();
                let snapshot = &eth_tx_index_snapshot;
                let store = eth_tx_index_store;
                handles.push(scope.spawn(move || {
                    let mut local_items = Vec::with_capacity(chunk_items.len());
                    let mut local_cache_entries = Vec::new();
                    for (idx, tx_hash, tx_hash_bytes) in chunk_items {
                        let (item, mut entry_cache) = gateway_eth_upstream_tx_status_item_json(
                            snapshot,
                            store,
                            chain_id,
                            &tx_hash,
                            tx_hash_bytes,
                            include,
                        )?;
                        local_items.push((idx, item));
                        local_cache_entries.append(&mut entry_cache);
                    }
                    Ok::<_, anyhow::Error>((local_items, local_cache_entries))
                }));
            }
            for handle in handles {
                let local = handle.join().map_err(|_| {
                    anyhow::anyhow!("gateway upstream snapshot worker thread panicked")
                })??;
                chunk_results.push(local);
            }
            Ok(())
        })?;
        for (mut local_items, mut local_cache_entries) in chunk_results {
            ordered_items.append(&mut local_items);
            cache_entries.append(&mut local_cache_entries);
        }
    }
    for entry in cache_entries {
        eth_tx_index.insert(entry.tx_hash, entry);
    }
    ordered_items.sort_by_key(|(idx, _)| *idx);

    let mut lifecycle_items = Vec::new();
    let mut receipt_items = Vec::new();
    let mut broadcast_items = Vec::new();
    for (_, item) in ordered_items {
        if include.lifecycle {
            lifecycle_items.push(
                item.get("lifecycle")
                    .cloned()
                    .unwrap_or(serde_json::Value::Null),
            );
        }
        if include.receipts {
            receipt_items.push(
                item.get("receipt")
                    .cloned()
                    .unwrap_or(serde_json::Value::Null),
            );
        }
        if include.broadcast {
            broadcast_items.push(
                item.get("broadcast")
                    .cloned()
                    .unwrap_or(serde_json::Value::Null),
            );
        }
    }
    Ok((
        serde_json::Value::Array(lifecycle_items),
        serde_json::Value::Array(receipt_items),
        serde_json::Value::Array(broadcast_items),
    ))
}

fn gateway_evm_upstream_snapshot_json(
    eth_tx_index: &mut HashMap<[u8; 32], GatewayEthTxIndexEntry>,
    eth_tx_index_store: &GatewayEthTxIndexStoreBackend,
    eth_default_chain_id: u64,
    chain_id: u64,
    params: &serde_json::Value,
) -> Result<serde_json::Value> {
    let tx_hashes = extract_eth_tx_hashes_query_params(params);
    let include_pending = param_as_bool(params, "include_pending")
        .or_else(|| param_as_bool(params, "includePending"))
        .unwrap_or(true);
    let include_txpool = param_as_bool(params, "include_txpool")
        .or_else(|| param_as_bool(params, "includeTxpool"))
        .unwrap_or(true);
    let include_logs = param_as_bool(params, "include_logs")
        .or_else(|| param_as_bool(params, "includeLogs"))
        .unwrap_or(true);
    let include_lifecycle = param_as_bool(params, "include_lifecycle")
        .or_else(|| param_as_bool(params, "includeLifecycle"))
        .unwrap_or(!tx_hashes.is_empty());
    let include_receipts = param_as_bool(params, "include_receipts")
        .or_else(|| param_as_bool(params, "includeReceipts"))
        .unwrap_or(!tx_hashes.is_empty());
    let include_broadcast = param_as_bool(params, "include_broadcast")
        .or_else(|| param_as_bool(params, "includeBroadcast"))
        .unwrap_or(!tx_hashes.is_empty());

    let pending_transactions = if include_pending {
        gateway_eth_pending_transactions_json(chain_id)
    } else {
        serde_json::json!([])
    };
    let txpool_status = if include_txpool {
        gateway_eth_txpool_status_json(chain_id)
    } else {
        serde_json::json!({
            "pending": "0x0",
            "queued": "0x0",
        })
    };
    let logs = if include_logs {
        let mut logs_params = params.clone();
        if logs_params.get("chain_id").is_none() && logs_params.get("chainId").is_none() {
            if let Some(obj) = logs_params.as_object_mut() {
                obj.insert("chain_id".to_owned(), serde_json::Value::from(chain_id));
            }
        }
        gateway_eth_logs_item_json(
            eth_tx_index,
            eth_tx_index_store,
            eth_default_chain_id,
            &logs_params,
        )?
    } else {
        serde_json::json!([])
    };

    let include_status = GatewayEthUpstreamTxStatusInclude {
        lifecycle: include_lifecycle,
        receipts: include_receipts,
        broadcast: include_broadcast,
    };
    let (lifecycle, receipts, broadcast) = gateway_eth_upstream_status_arrays_json(
        eth_tx_index,
        eth_tx_index_store,
        chain_id,
        &tx_hashes,
        include_status,
    )?;

    Ok(serde_json::json!({
        "chain_id": format!("0x{:x}", chain_id),
        "tx_hashes": tx_hashes,
        "pending_transactions": pending_transactions,
        "txpool_status": txpool_status,
        "logs": logs,
        "lifecycle": lifecycle,
        "receipts": receipts,
        "broadcast": broadcast,
    }))
}

fn gateway_evm_upstream_runtime_bundle_json(
    eth_tx_index: &mut HashMap<[u8; 32], GatewayEthTxIndexEntry>,
    eth_tx_index_store: &GatewayEthTxIndexStoreBackend,
    eth_default_chain_id: u64,
    chain_id: u64,
    max_items: u64,
    include_ingress: bool,
    params: &serde_json::Value,
) -> Result<serde_json::Value> {
    let upstream = gateway_evm_upstream_snapshot_json(
        eth_tx_index,
        eth_tx_index_store,
        eth_default_chain_id,
        chain_id,
        params,
    )?;

    let ingress = if include_ingress {
        let max_items_usize = max_items.clamp(1, 8_192) as usize;
        let include_raw = true;
        let include_parsed = true;

        let mut executable_frames = snapshot_executable_ingress_frames_for_host(max_items_usize);
        executable_frames.retain(|frame| frame.chain_id == chain_id);
        let executable_ingress: Vec<serde_json::Value> = executable_frames
            .iter()
            .map(|frame| gateway_evm_ingress_frame_json(frame, include_raw, include_parsed))
            .collect();

        let mut pending_frames = snapshot_pending_ingress_frames_for_host(max_items_usize);
        pending_frames.retain(|frame| frame.chain_id == chain_id);
        let pending_ingress: Vec<serde_json::Value> = pending_frames
            .iter()
            .map(|frame| gateway_evm_ingress_frame_json(frame, include_raw, include_parsed))
            .collect();

        let mut sender_buckets = snapshot_pending_sender_buckets_for_host(256, 64);
        sender_buckets.retain(|bucket| bucket.chain_id == chain_id);
        let pending_sender_buckets: Vec<serde_json::Value> = sender_buckets
            .iter()
            .map(gateway_evm_pending_sender_bucket_json)
            .collect();

        serde_json::json!({
            "max_items": max_items,
            "executable_ingress": executable_ingress,
            "pending_ingress": pending_ingress,
            "pending_sender_buckets": pending_sender_buckets,
        })
    } else {
        serde_json::json!({
            "max_items": max_items,
            "executable_ingress": [],
            "pending_ingress": [],
            "pending_sender_buckets": [],
        })
    };

    Ok(serde_json::json!({
        "chain_id": format!("0x{:x}", chain_id),
        "upstream": upstream,
        "ingress": ingress,
    }))
}

fn gateway_evm_parse_filter_ids(input: &serde_json::Value) -> Vec<String> {
    let mut out = Vec::new();
    let sources = [
        input.get("filter_ids"),
        input.get("filterIds"),
        input.get("filters"),
        input.get("ids"),
    ];
    for source in sources {
        let Some(source) = source else {
            continue;
        };
        if let Some(value) = source.as_str() {
            out.push(value.to_owned());
        }
        if let Some(values) = source.as_array() {
            for value in values {
                if let Some(v) = value.as_str() {
                    out.push(v.to_owned());
                }
            }
        }
        if !out.is_empty() {
            break;
        }
    }
    out
}

#[derive(Copy, Clone)]
struct GatewayEvmUpstreamTxStatusBundleOptions {
    chain_id: u64,
    max_items: usize,
    auto_from_pending: bool,
    include_lifecycle: bool,
    include_receipts: bool,
    include_broadcast: bool,
}

fn gateway_evm_upstream_tx_status_bundle_json(
    eth_tx_index: &mut HashMap<[u8; 32], GatewayEthTxIndexEntry>,
    eth_tx_index_store: &GatewayEthTxIndexStoreBackend,
    options: GatewayEvmUpstreamTxStatusBundleOptions,
    params: &serde_json::Value,
) -> Result<serde_json::Value> {
    let mut tx_hashes = extract_eth_tx_hashes_query_params(params);
    if tx_hashes.len() > options.max_items {
        tx_hashes.truncate(options.max_items);
    }
    if tx_hashes.is_empty() && options.auto_from_pending {
        let pending_transactions = gateway_eth_pending_transactions_json(options.chain_id);
        let mut seen: BTreeSet<String> = BTreeSet::new();
        if let Some(items) = pending_transactions.as_array() {
            for item in items {
                let Some(tx_hash) = item.get("hash").and_then(|v| v.as_str()) else {
                    continue;
                };
                if seen.insert(tx_hash.to_ascii_lowercase()) {
                    tx_hashes.push(tx_hash.to_owned());
                    if tx_hashes.len() >= options.max_items {
                        break;
                    }
                }
            }
        }
    }
    if tx_hashes.is_empty() {
        bail!("tx_hashes (or txHashes/hashes/txs) is required; or set auto_from_pending=true");
    }

    let mut indexed_hashes: Vec<(usize, String, [u8; 32])> = Vec::with_capacity(tx_hashes.len());
    for (idx, tx_hash) in tx_hashes.into_iter().enumerate() {
        let tx_hash_bytes = decode_hex_bytes(&tx_hash, "tx_hash")?;
        indexed_hashes.push((idx, tx_hash, vec_to_32(&tx_hash_bytes, "tx_hash")?));
    }
    let include_flags = GatewayEthUpstreamTxStatusInclude {
        lifecycle: options.include_lifecycle,
        receipts: options.include_receipts,
        broadcast: options.include_broadcast,
    };
    let workers = gateway_batch_worker_count(indexed_hashes.len());
    let eth_tx_index_snapshot = eth_tx_index.clone();
    let mut ordered_items: Vec<(usize, serde_json::Value)> =
        Vec::with_capacity(indexed_hashes.len());
    let mut cache_entries: Vec<GatewayEthTxIndexEntry> = Vec::new();
    if workers <= 1 || indexed_hashes.len() <= 1 {
        for (idx, tx_hash, tx_hash_bytes) in indexed_hashes {
            let (item, mut local_cache_entries) = gateway_eth_upstream_tx_status_item_json(
                &eth_tx_index_snapshot,
                eth_tx_index_store,
                options.chain_id,
                &tx_hash,
                tx_hash_bytes,
                include_flags,
            )?;
            ordered_items.push((idx, item));
            cache_entries.append(&mut local_cache_entries);
        }
    } else {
        let chunk_size = indexed_hashes.len().div_ceil(workers).max(1);
        let mut chunk_results: Vec<GatewayEthUpstreamTxStatusChunk> = Vec::new();
        std::thread::scope(|scope| -> Result<()> {
            let mut handles = Vec::new();
            for chunk in indexed_hashes.chunks(chunk_size) {
                let chunk_items = chunk.to_vec();
                let snapshot = &eth_tx_index_snapshot;
                let store = eth_tx_index_store;
                handles.push(scope.spawn(move || {
                    let mut local_items = Vec::with_capacity(chunk_items.len());
                    let mut local_cache_entries = Vec::new();
                    for (idx, tx_hash, tx_hash_bytes) in chunk_items {
                        let (item, mut entry_cache) = gateway_eth_upstream_tx_status_item_json(
                            snapshot,
                            store,
                            options.chain_id,
                            &tx_hash,
                            tx_hash_bytes,
                            include_flags,
                        )?;
                        local_items.push((idx, item));
                        local_cache_entries.append(&mut entry_cache);
                    }
                    Ok::<_, anyhow::Error>((local_items, local_cache_entries))
                }));
            }
            for handle in handles {
                let local = handle.join().map_err(|_| {
                    anyhow::anyhow!("gateway upstream tx-status bundle worker thread panicked")
                })??;
                chunk_results.push(local);
            }
            Ok(())
        })?;
        for (mut local_items, mut local_cache_entries) in chunk_results {
            ordered_items.append(&mut local_items);
            cache_entries.append(&mut local_cache_entries);
        }
    }
    for entry in cache_entries {
        eth_tx_index.insert(entry.tx_hash, entry);
    }
    ordered_items.sort_by_key(|(idx, _)| *idx);
    let items: Vec<serde_json::Value> = ordered_items.into_iter().map(|(_, item)| item).collect();

    Ok(serde_json::json!({
        "chain_id": format!("0x{:x}", options.chain_id),
        "count": items.len(),
        "items": items,
    }))
}

#[derive(Copy, Clone)]
struct GatewayEvmUpstreamEventBundleOptions {
    chain_id: u64,
    eth_default_chain_id: u64,
    include_logs: bool,
    include_filter_changes: bool,
    include_filter_logs: bool,
}

fn gateway_evm_upstream_event_bundle_json(
    eth_tx_index: &mut HashMap<[u8; 32], GatewayEthTxIndexEntry>,
    eth_tx_index_store: &GatewayEthTxIndexStoreBackend,
    eth_filters: &mut GatewayEthFilterState,
    options: GatewayEvmUpstreamEventBundleOptions,
    params: &serde_json::Value,
) -> Result<serde_json::Value> {
    let mut logs_params = params.clone();
    if logs_params.get("chain_id").is_none() && logs_params.get("chainId").is_none() {
        if let Some(obj) = logs_params.as_object_mut() {
            obj.insert(
                "chain_id".to_owned(),
                serde_json::Value::from(options.chain_id),
            );
        }
    }

    let logs = if options.include_logs {
        gateway_eth_logs_item_json(
            eth_tx_index,
            eth_tx_index_store,
            options.eth_default_chain_id,
            &logs_params,
        )?
    } else {
        serde_json::json!([])
    };

    let filter_ids = gateway_evm_parse_filter_ids(params);
    let mut filter_changes = Vec::new();
    let mut filter_logs = Vec::new();
    if !filter_ids.is_empty() {
        for filter_id in filter_ids {
            if options.include_filter_changes {
                let forwarded = serde_json::json!({
                    "chain_id": options.chain_id,
                    "filter_id": filter_id,
                });
                let parsed_filter_id = parse_eth_filter_id(&forwarded)
                    .ok_or_else(|| anyhow::anyhow!("filter_id (or id) is required"))?;
                let changes = gateway_eth_filter_changes_item_json(
                    eth_tx_index,
                    eth_tx_index_store,
                    eth_filters,
                    parsed_filter_id,
                )?;
                filter_changes.push(serde_json::json!({
                    "filter_id": forwarded["filter_id"],
                    "changes": changes,
                }));
            }
            if options.include_filter_logs {
                let forwarded = serde_json::json!({
                    "chain_id": options.chain_id,
                    "filter_id": filter_id,
                });
                let items = gateway_eth_filter_logs_item_json(
                    eth_tx_index,
                    eth_tx_index_store,
                    &eth_filters.filters,
                    &forwarded,
                )?;
                filter_logs.push(serde_json::json!({
                    "filter_id": forwarded["filter_id"],
                    "logs": items,
                }));
            }
        }
    }

    Ok(serde_json::json!({
        "chain_id": format!("0x{:x}", options.chain_id),
        "logs": logs,
        "filter_changes": filter_changes,
        "filter_logs": filter_logs,
    }))
}

fn gateway_evm_runtime_native_sync_status_json(chain_id: u64) -> serde_json::Value {
    let Some(status) = get_network_runtime_native_sync_status(chain_id) else {
        return serde_json::Value::Null;
    };
    let active = network_runtime_native_sync_is_active(&status);
    serde_json::json!({
        "chain_id": format!("0x{:x}", chain_id),
        "phase": status.phase.as_str(),
        "active": active,
        "peer_count": status.peer_count,
        "starting_block": status.starting_block,
        "current_block": status.current_block,
        "highest_block": status.highest_block,
        "updated_at_unix_millis": status.updated_at_unix_millis,
        "peerCount": format!("0x{:x}", status.peer_count),
        "startingBlock": format!("0x{:x}", status.starting_block),
        "currentBlock": format!("0x{:x}", status.current_block),
        "highestBlock": format!("0x{:x}", status.highest_block),
    })
}

fn gateway_evm_runtime_sync_pull_window_json(chain_id: u64) -> serde_json::Value {
    let Some(window) = plan_network_runtime_sync_pull_window(chain_id) else {
        return serde_json::Value::Null;
    };
    serde_json::json!({
        "chain_id": format!("0x{:x}", window.chain_id),
        "phase": window.phase.as_str(),
        "peer_count": window.peer_count,
        "current_block": window.current_block,
        "highest_block": window.highest_block,
        "from_block": window.from_block,
        "to_block": window.to_block,
        "peerCount": format!("0x{:x}", window.peer_count),
        "currentBlock": format!("0x{:x}", window.current_block),
        "highestBlock": format!("0x{:x}", window.highest_block),
        "fromBlock": format!("0x{:x}", window.from_block),
        "toBlock": format!("0x{:x}", window.to_block),
    })
}

fn gateway_evm_runtime_status_bundle_json(
    chain_id: u64,
    eth_tx_index: &mut HashMap<[u8; 32], GatewayEthTxIndexEntry>,
    eth_tx_index_store: &GatewayEthTxIndexStoreBackend,
) -> Result<serde_json::Value> {
    let sync_status = resolve_gateway_eth_sync_status(chain_id, eth_tx_index, eth_tx_index_store)?;
    let peer_count = sync_status.peer_count;
    let block_number = sync_status
        .current_block
        .max(sync_status.local_current_block);
    let syncing = gateway_eth_syncing_json(sync_status, None);
    Ok(serde_json::json!({
        "syncing": syncing,
        "peer_count": format!("0x{:x}", peer_count),
        "block_number": format!("0x{:x}", block_number),
        "public_broadcast": gateway_eth_public_broadcast_capability_json(chain_id),
        "native_sync": gateway_evm_runtime_native_sync_status_json(chain_id),
        "sync_pull_window": gateway_evm_runtime_sync_pull_window_json(chain_id),
    }))
}

#[derive(Copy, Clone)]
struct GatewayEvmUpstreamFullBundleOptions {
    chain_id: u64,
    max_items: u64,
    include_ingress: bool,
    include_logs: bool,
    include_filter_changes: bool,
    include_filter_logs: bool,
    include_lifecycle: bool,
    include_receipts: bool,
    include_broadcast: bool,
    auto_from_pending: bool,
}

fn gateway_evm_upstream_full_bundle_json(
    eth_tx_index: &mut HashMap<[u8; 32], GatewayEthTxIndexEntry>,
    eth_tx_index_store: &GatewayEthTxIndexStoreBackend,
    eth_default_chain_id: u64,
    eth_filters: &mut GatewayEthFilterState,
    options: GatewayEvmUpstreamFullBundleOptions,
    params: &serde_json::Value,
) -> Result<serde_json::Value> {
    let runtime_status =
        gateway_evm_runtime_status_bundle_json(options.chain_id, eth_tx_index, eth_tx_index_store)?;

    let mut bundle_forwarded = params.clone();
    if let Some(obj) = bundle_forwarded.as_object_mut() {
        obj.insert(
            "chain_id".to_owned(),
            serde_json::Value::from(options.chain_id),
        );
        obj.insert(
            "max_items".to_owned(),
            serde_json::Value::from(options.max_items),
        );
        obj.insert(
            "include_ingress".to_owned(),
            serde_json::Value::from(options.include_ingress),
        );
        obj.insert(
            "include_logs".to_owned(),
            serde_json::Value::from(options.include_logs),
        );
        obj.insert(
            "include_lifecycle".to_owned(),
            serde_json::Value::from(options.include_lifecycle),
        );
        obj.insert(
            "include_receipts".to_owned(),
            serde_json::Value::from(options.include_receipts),
        );
        obj.insert(
            "include_broadcast".to_owned(),
            serde_json::Value::from(options.include_broadcast),
        );
    }
    let runtime_bundle = gateway_evm_upstream_runtime_bundle_json(
        eth_tx_index,
        eth_tx_index_store,
        eth_default_chain_id,
        options.chain_id,
        options.max_items,
        options.include_ingress,
        &bundle_forwarded,
    )?;

    let mut tx_status_forwarded = serde_json::json!({
        "chain_id": options.chain_id,
        "max_items": options.max_items,
        "auto_from_pending": options.auto_from_pending,
        "include_lifecycle": options.include_lifecycle,
        "include_receipts": options.include_receipts,
        "include_broadcast": options.include_broadcast,
    });
    let tx_hashes = runtime_bundle
        .get("upstream")
        .and_then(|v| v.get("tx_hashes"))
        .and_then(|v| v.as_array())
        .cloned()
        .unwrap_or_default();
    if !tx_hashes.is_empty() {
        if let Some(obj) = tx_status_forwarded.as_object_mut() {
            obj.insert("tx_hashes".to_owned(), serde_json::Value::Array(tx_hashes));
        }
    }
    let tx_status_bundle = gateway_evm_upstream_tx_status_bundle_json(
        eth_tx_index,
        eth_tx_index_store,
        GatewayEvmUpstreamTxStatusBundleOptions {
            chain_id: options.chain_id,
            max_items: options.max_items as usize,
            auto_from_pending: options.auto_from_pending,
            include_lifecycle: options.include_lifecycle,
            include_receipts: options.include_receipts,
            include_broadcast: options.include_broadcast,
        },
        &tx_status_forwarded,
    )?;

    let event_forwarded = serde_json::json!({
        "chain_id": options.chain_id,
        "include_logs": options.include_logs,
        "include_filter_changes": options.include_filter_changes,
        "include_filter_logs": options.include_filter_logs,
        "filter_ids": params.get("filter_ids")
            .or_else(|| params.get("filterIds"))
            .or_else(|| params.get("filters"))
            .or_else(|| params.get("ids"))
            .cloned()
            .unwrap_or(serde_json::Value::Array(vec![])),
    });
    let event_bundle = gateway_evm_upstream_event_bundle_json(
        eth_tx_index,
        eth_tx_index_store,
        eth_filters,
        GatewayEvmUpstreamEventBundleOptions {
            chain_id: options.chain_id,
            eth_default_chain_id,
            include_logs: options.include_logs,
            include_filter_changes: options.include_filter_changes,
            include_filter_logs: options.include_filter_logs,
        },
        &event_forwarded,
    )?;

    Ok(serde_json::json!({
        "chain_id": format!("0x{:x}", options.chain_id),
        "runtime_status": {
            "syncing": runtime_status["syncing"].clone(),
            "peer_count": runtime_status["peer_count"].clone(),
            "block_number": runtime_status["block_number"].clone(),
            "public_broadcast": runtime_status["public_broadcast"].clone(),
            "native_sync": runtime_status["native_sync"].clone(),
            "sync_pull_window": runtime_status["sync_pull_window"].clone(),
        },
        "runtime_bundle": runtime_bundle,
        "tx_status_bundle": tx_status_bundle,
        "event_bundle": event_bundle,
    }))
}

fn enforce_gateway_atomic_gate(
    chain_type: novovm_adapter_api::ChainType,
    chain_id: u64,
    tx_hash: &[u8],
    receipts: &[AtomicIntentReceiptV1],
    ready_items: &[AtomicBroadcastReadyV1],
) -> Result<()> {
    let rejected: Vec<AtomicIntentReceiptV1> = receipts
        .iter()
        .filter(|receipt| matches!(receipt.status, AtomicIntentStatus::Rejected))
        .cloned()
        .collect();
    if !rejected.is_empty() {
        bail!(
            "plugin_atomic_gate_rejected: rejected_receipts={} chain={} chain_id={} tx_hash=0x{} reasons={}",
            rejected.len(),
            chain_type.as_str(),
            chain_id,
            to_hex(tx_hash),
            atomic_reject_reasons_csv(&rejected)
        );
    }
    let matched_ready = ready_items
        .iter()
        .filter(|item| atomic_ready_matches_tx_hash(item, tx_hash))
        .count();
    if matched_ready == 0 {
        bail!(
            "plugin_atomic_gate_not_ready: ready_items={} matched_ready={} chain={} chain_id={} tx_hash=0x{}",
            ready_items.len(),
            matched_ready,
            chain_type.as_str(),
            chain_id,
            to_hex(tx_hash)
        );
    }
    Ok(())
}

fn apply_gateway_evm_runtime_tap(
    tx_ir: &TxIR,
    wants_cross_chain_atomic: bool,
) -> Result<GatewayEvmRuntimeTapDrain> {
    let chain_type = evm_chain_type_for_gateway(tx_ir.chain_id);
    let mut flags = 0u64;
    if wants_cross_chain_atomic {
        flags |= NOVOVM_ADAPTER_PLUGIN_APPLY_FLAG_ATOMIC_INTENT_GUARD_V1;
    }
    let txs = vec![tx_ir.clone()];
    let tap = match runtime_tap_ir_batch_v1(chain_type, tx_ir.chain_id, &txs, flags) {
        Ok(v) => v,
        Err(rc) => {
            bail!(
                "gateway evm runtime tap failed: rc={} chain={} chain_id={} tx_hash=0x{}",
                rc,
                chain_type.as_str(),
                tx_ir.chain_id,
                to_hex(&tx_ir.hash)
            );
        }
    };
    if tap.accepted < txs.len() {
        let reason = tap
            .primary_reject_reason
            .map(|value| value.as_str())
            .unwrap_or("rejected");
        let reasons = if tap.reject_reasons.is_empty() {
            reason.to_string()
        } else {
            tap.reject_reasons
                .iter()
                .map(|value| value.as_str())
                .collect::<Vec<_>>()
                .join(",")
        };
        bail!(
            "gateway evm txpool rejected tx: chain={} chain_id={} tx_hash=0x{} reason={} reasons={} requested={} accepted={} dropped={} dropped_underpriced={} dropped_nonce_gap={} dropped_nonce_too_low={} dropped_over_capacity={}",
            chain_type.as_str(),
            tx_ir.chain_id,
            to_hex(&tx_ir.hash),
            reason,
            reasons,
            tap.requested,
            tap.accepted,
            tap.dropped,
            tap.dropped_underpriced,
            tap.dropped_nonce_gap,
            tap.dropped_nonce_too_low,
            tap.dropped_over_capacity
        );
    }

    let drain_max = GATEWAY_EVM_RUNTIME_DRAIN_MAX;
    let _ = drain_executable_ingress_frames_for_host(drain_max);
    let _ = drain_pending_ingress_frames_for_host(drain_max);
    let settlement_records = drain_settlement_records_for_host(drain_max);
    let payout_instructions = drain_payout_instructions_for_host(drain_max);
    let mut atomic_ready_items = Vec::new();
    if wants_cross_chain_atomic {
        let ready_items = drain_atomic_broadcast_ready_for_host(drain_max);
        let drained_receipts = drain_atomic_receipts_for_host(drain_max);
        enforce_gateway_atomic_gate(
            chain_type,
            tx_ir.chain_id,
            tx_ir.hash.as_slice(),
            &drained_receipts,
            &ready_items,
        )?;
        atomic_ready_items = ready_items
            .into_iter()
            .filter(|item| atomic_ready_matches_tx_hash(item, tx_ir.hash.as_slice()))
            .collect();
    }
    Ok(GatewayEvmRuntimeTapDrain {
        settlement_records,
        payout_instructions,
        atomic_ready_items,
    })
}

fn execute_gateway_evm_executable_ingress_frames(
    chain_id: u64,
    frames: &[EvmMempoolIngressFrameV1],
) -> Result<(Vec<TxIR>, Vec<serde_json::Value>, usize)> {
    let mut txs = Vec::new();
    let mut sampled_hashes = Vec::new();
    let mut dropped_unparsed = 0usize;
    for frame in frames {
        let Some(tx) = frame.parsed_tx.clone() else {
            dropped_unparsed = dropped_unparsed.saturating_add(1);
            continue;
        };
        if tx.chain_id != chain_id {
            continue;
        }
        let hash_bytes = if !frame.tx_hash.is_empty() {
            frame.tx_hash.as_slice()
        } else {
            tx.hash.as_slice()
        };
        sampled_hashes.push(serde_json::Value::String(format!(
            "0x{}",
            to_hex(hash_bytes)
        )));
        txs.push(tx);
    }
    if txs.is_empty() {
        bail!(
            "no parsed ingress transactions for chain_id={} (dropped_unparsed={})",
            chain_id,
            dropped_unparsed
        );
    }
    Ok((txs, sampled_hashes, dropped_unparsed))
}

impl GatewayUaStoreBackend {
    fn backend_name(&self) -> &'static str {
        match self {
            GatewayUaStoreBackend::BincodeFile { .. } => GATEWAY_UA_STORE_BACKEND_FILE,
            GatewayUaStoreBackend::RocksDb { .. } => GATEWAY_UA_STORE_BACKEND_ROCKSDB,
        }
    }

    fn path(&self) -> &Path {
        match self {
            GatewayUaStoreBackend::BincodeFile { path } => path.as_path(),
            GatewayUaStoreBackend::RocksDb { path } => path.as_path(),
        }
    }

    fn load_router(&self) -> Result<UnifiedAccountRouter> {
        match self {
            GatewayUaStoreBackend::BincodeFile { path } => {
                if !path.exists() {
                    return Ok(UnifiedAccountRouter::new());
                }
                let raw = fs::read(path)
                    .with_context(|| format!("read gateway ua store failed: {}", path.display()))?;
                if raw.is_empty() {
                    return Ok(UnifiedAccountRouter::new());
                }
                if let Ok(envelope) =
                    crate::bincode_compat::deserialize::<GatewayUaStoreEnvelopeV1>(&raw)
                {
                    if envelope.version != GATEWAY_UA_STORE_ENVELOPE_VERSION {
                        bail!(
                            "unsupported gateway ua store version {} at {}",
                            envelope.version,
                            path.display()
                        );
                    }
                    return Ok(envelope.router);
                }
                let router: UnifiedAccountRouter = crate::bincode_compat::deserialize(&raw)
                    .with_context(|| {
                        format!("decode legacy gateway ua store failed: {}", path.display())
                    })?;
                Ok(router)
            }
            GatewayUaStoreBackend::RocksDb { path } => {
                let db = open_gateway_ua_rocksdb(path)?;
                let state_cf =
                    db.cf_handle(GATEWAY_UA_STORE_ROCKSDB_CF_STATE)
                        .ok_or_else(|| {
                            anyhow::anyhow!(
                                "missing gateway ua rocksdb column family '{}' for {}",
                                GATEWAY_UA_STORE_ROCKSDB_CF_STATE,
                                path.display()
                            )
                        })?;
                let mut raw = db
                    .get_cf(state_cf, GATEWAY_UA_STORE_ROCKSDB_KEY_ROUTER)
                    .with_context(|| {
                        format!(
                            "read gateway ua router key from cf '{}' failed: {}",
                            GATEWAY_UA_STORE_ROCKSDB_CF_STATE,
                            path.display()
                        )
                    })?;
                let Some(raw) = raw else {
                    return Ok(UnifiedAccountRouter::new());
                };
                if raw.is_empty() {
                    return Ok(UnifiedAccountRouter::new());
                }
                if let Ok(envelope) =
                    crate::bincode_compat::deserialize::<GatewayUaStoreEnvelopeV1>(&raw)
                {
                    if envelope.version != GATEWAY_UA_STORE_ENVELOPE_VERSION {
                        bail!(
                            "unsupported gateway ua store version {} at {}",
                            envelope.version,
                            path.display()
                        );
                    }
                    return Ok(envelope.router);
                }
                let router: UnifiedAccountRouter = crate::bincode_compat::deserialize(&raw)
                    .with_context(|| {
                        format!(
                            "decode legacy gateway ua rocksdb state failed: {}",
                            path.display()
                        )
                    })?;
                Ok(router)
            }
        }
    }

    fn save_router(&self, router: &UnifiedAccountRouter) -> Result<()> {
        #[derive(Serialize)]
        struct GatewayUaStoreEnvelopeRef<'a> {
            version: u32,
            router: &'a UnifiedAccountRouter,
        }
        let envelope = GatewayUaStoreEnvelopeRef {
            version: GATEWAY_UA_STORE_ENVELOPE_VERSION,
            router,
        };
        let encoded = crate::bincode_compat::serialize(&envelope)
            .context("serialize gateway ua store envelope failed")?;
        match self {
            GatewayUaStoreBackend::BincodeFile { path } => {
                ensure_parent_dir(path, "gateway ua store")?;
                fs::write(path, encoded).with_context(|| {
                    format!("write gateway ua store failed: {}", path.display())
                })?;
                Ok(())
            }
            GatewayUaStoreBackend::RocksDb { path } => {
                let db = open_gateway_ua_rocksdb(path)?;
                let state_cf =
                    db.cf_handle(GATEWAY_UA_STORE_ROCKSDB_CF_STATE)
                        .ok_or_else(|| {
                            anyhow::anyhow!(
                                "missing gateway ua rocksdb column family '{}' for {}",
                                GATEWAY_UA_STORE_ROCKSDB_CF_STATE,
                                path.display()
                            )
                        })?;
                db.put_cf(state_cf, GATEWAY_UA_STORE_ROCKSDB_KEY_ROUTER, encoded)
                    .with_context(|| {
                        format!(
                            "write gateway ua router key into cf '{}' failed: {}",
                            GATEWAY_UA_STORE_ROCKSDB_CF_STATE,
                            path.display()
                        )
                    })?;
                Ok(())
            }
        }
    }
}

impl GatewayEthTxIndexStoreBackend {
    fn backend_name(&self) -> &'static str {
        match self {
            GatewayEthTxIndexStoreBackend::Memory => GATEWAY_ETH_TX_INDEX_BACKEND_MEMORY,
            GatewayEthTxIndexStoreBackend::RocksDb { .. } => GATEWAY_ETH_TX_INDEX_BACKEND_ROCKSDB,
        }
    }

    fn path(&self) -> &Path {
        match self {
            GatewayEthTxIndexStoreBackend::Memory => Path::new("memory"),
            GatewayEthTxIndexStoreBackend::RocksDb { path } => path.as_path(),
        }
    }

    fn load_eth_tx(&self, tx_hash: &[u8; 32]) -> Result<Option<GatewayEthTxIndexEntry>> {
        match self {
            GatewayEthTxIndexStoreBackend::Memory => Ok(None),
            GatewayEthTxIndexStoreBackend::RocksDb { path } => {
                let db = open_gateway_eth_tx_index_rocksdb(path)?;
                let cf = db
                    .cf_handle(GATEWAY_ETH_TX_INDEX_ROCKSDB_CF_STATE)
                    .ok_or_else(|| {
                        anyhow::anyhow!(
                            "missing eth tx index rocksdb column family '{}' for {}",
                            GATEWAY_ETH_TX_INDEX_ROCKSDB_CF_STATE,
                            path.display()
                        )
                    })?;
                let key = gateway_eth_tx_index_key(tx_hash);
                let raw = db.get_cf(cf, &key).with_context(|| {
                    format!(
                        "read eth tx index from cf '{}' failed: {}",
                        GATEWAY_ETH_TX_INDEX_ROCKSDB_CF_STATE,
                        path.display()
                    )
                })?;
                let Some(raw) = raw else {
                    return Ok(None);
                };
                if raw.is_empty() {
                    return Ok(None);
                }
                if let Ok(record) =
                    crate::bincode_compat::deserialize::<GatewayEthTxIndexRecordV1>(&raw)
                {
                    if record.version != GATEWAY_ETH_TX_INDEX_RECORD_VERSION {
                        bail!(
                            "unsupported eth tx index record version {} at {}",
                            record.version,
                            path.display()
                        );
                    }
                    return Ok(Some(record.entry));
                }
                let legacy_entry: GatewayEthTxIndexEntry = crate::bincode_compat::deserialize(&raw)
                    .with_context(|| {
                        format!(
                            "decode legacy eth tx index record failed: {}",
                            path.display()
                        )
                    })?;
                Ok(Some(legacy_entry))
            }
        }
    }

    fn save_eth_tx(&self, entry: &GatewayEthTxIndexEntry) -> Result<()> {
        match self {
            GatewayEthTxIndexStoreBackend::Memory => Ok(()),
            GatewayEthTxIndexStoreBackend::RocksDb { path } => {
                let db = open_gateway_eth_tx_index_rocksdb(path)?;
                let cf = db
                    .cf_handle(GATEWAY_ETH_TX_INDEX_ROCKSDB_CF_STATE)
                    .ok_or_else(|| {
                        anyhow::anyhow!(
                            "missing eth tx index rocksdb column family '{}' for {}",
                            GATEWAY_ETH_TX_INDEX_ROCKSDB_CF_STATE,
                            path.display()
                        )
                    })?;
                let key = gateway_eth_tx_index_key(&entry.tx_hash);
                let value = crate::bincode_compat::serialize(&GatewayEthTxIndexRecordV1 {
                    version: GATEWAY_ETH_TX_INDEX_RECORD_VERSION,
                    entry: entry.clone(),
                })
                .context("serialize eth tx index record failed")?;
                db.put_cf(cf, key, value).with_context(|| {
                    format!(
                        "write eth tx index into cf '{}' failed: {}",
                        GATEWAY_ETH_TX_INDEX_ROCKSDB_CF_STATE,
                        path.display()
                    )
                })?;
                let block_index_key =
                    gateway_eth_tx_block_index_key(entry.chain_id, entry.nonce, &entry.tx_hash);
                db.put_cf(cf, block_index_key, entry.tx_hash)
                    .with_context(|| {
                        format!(
                            "write eth tx block index into cf '{}' failed: {}",
                            GATEWAY_ETH_TX_INDEX_ROCKSDB_CF_STATE,
                            path.display()
                        )
                    })?;
                let prefix = gateway_eth_tx_block_index_prefix(entry.chain_id, entry.nonce);
                let mut block_txs = Vec::new();
                let iter =
                    db.iterator_cf(cf, IteratorMode::From(&prefix, rocksdb::Direction::Forward));
                for item in iter {
                    let (key, tx_hash_raw) = item.with_context(|| {
                        format!(
                            "iterate eth tx block index from cf '{}' failed: {}",
                            GATEWAY_ETH_TX_INDEX_ROCKSDB_CF_STATE,
                            path.display()
                        )
                    })?;
                    if !key.starts_with(&prefix) {
                        break;
                    }
                    if tx_hash_raw.len() < 32 {
                        continue;
                    }
                    let mut tx_hash = [0u8; 32];
                    tx_hash.copy_from_slice(&tx_hash_raw[..32]);
                    let Some(tx_raw) = db
                        .get_cf(cf, gateway_eth_tx_index_key(&tx_hash))
                        .with_context(|| {
                            format!(
                                "read eth tx index by hash from cf '{}' failed: {}",
                                GATEWAY_ETH_TX_INDEX_ROCKSDB_CF_STATE,
                                path.display()
                            )
                        })?
                    else {
                        continue;
                    };
                    let tx_entry = if let Ok(record) =
                        crate::bincode_compat::deserialize::<GatewayEthTxIndexRecordV1>(&tx_raw)
                    {
                        if record.version != GATEWAY_ETH_TX_INDEX_RECORD_VERSION {
                            continue;
                        }
                        record.entry
                    } else if let Ok(legacy) =
                        crate::bincode_compat::deserialize::<GatewayEthTxIndexEntry>(&tx_raw)
                    {
                        legacy
                    } else {
                        continue;
                    };
                    if tx_entry.chain_id != entry.chain_id || tx_entry.nonce != entry.nonce {
                        continue;
                    }
                    block_txs.push(tx_entry);
                }
                if !block_txs.is_empty() {
                    sort_gateway_eth_block_txs(&mut block_txs);
                    let block_hash =
                        gateway_eth_block_hash_for_txs(entry.chain_id, entry.nonce, &block_txs);
                    let block_hash_key =
                        gateway_eth_block_hash_index_key_by_hash(entry.chain_id, &block_hash);
                    db.put_cf(cf, block_hash_key, entry.nonce.to_be_bytes())
                        .with_context(|| {
                            format!(
                                "write eth block hash index into cf '{}' failed: {}",
                                GATEWAY_ETH_TX_INDEX_ROCKSDB_CF_STATE,
                                path.display()
                            )
                        })?;
                }
                Ok(())
            }
        }
    }

    fn load_eth_broadcast_status(
        &self,
        tx_hash: &[u8; 32],
    ) -> Result<Option<GatewayEthBroadcastStatus>> {
        match self {
            GatewayEthTxIndexStoreBackend::Memory => Ok(None),
            GatewayEthTxIndexStoreBackend::RocksDb { path } => {
                let db = open_gateway_eth_tx_index_rocksdb(path)?;
                let cf = db
                    .cf_handle(GATEWAY_ETH_TX_INDEX_ROCKSDB_CF_STATE)
                    .ok_or_else(|| {
                        anyhow::anyhow!(
                            "missing eth tx index rocksdb column family '{}' for {}",
                            GATEWAY_ETH_TX_INDEX_ROCKSDB_CF_STATE,
                            path.display()
                        )
                    })?;
                let key = gateway_eth_broadcast_status_key(tx_hash);
                let raw = db.get_cf(cf, &key).with_context(|| {
                    format!(
                        "read eth broadcast status from cf '{}' failed: {}",
                        GATEWAY_ETH_TX_INDEX_ROCKSDB_CF_STATE,
                        path.display()
                    )
                })?;
                let Some(raw) = raw else {
                    return Ok(None);
                };
                if raw.is_empty() {
                    return Ok(None);
                }
                if let Ok(record) =
                    crate::bincode_compat::deserialize::<GatewayEthBroadcastStatusRecordV1>(&raw)
                {
                    if record.version != GATEWAY_ETH_BROADCAST_STATUS_RECORD_VERSION {
                        bail!(
                            "unsupported eth broadcast-status record version {} at {}",
                            record.version,
                            path.display()
                        );
                    }
                    return Ok(Some(record.status));
                }
                let legacy_status: GatewayEthBroadcastStatus =
                    crate::bincode_compat::deserialize(&raw).with_context(|| {
                        format!(
                            "decode legacy eth broadcast-status record failed: {}",
                            path.display()
                        )
                    })?;
                Ok(Some(legacy_status))
            }
        }
    }

    fn save_eth_broadcast_status(
        &self,
        tx_hash: &[u8; 32],
        status: &GatewayEthBroadcastStatus,
    ) -> Result<()> {
        match self {
            GatewayEthTxIndexStoreBackend::Memory => Ok(()),
            GatewayEthTxIndexStoreBackend::RocksDb { path } => {
                let db = open_gateway_eth_tx_index_rocksdb(path)?;
                let cf = db
                    .cf_handle(GATEWAY_ETH_TX_INDEX_ROCKSDB_CF_STATE)
                    .ok_or_else(|| {
                        anyhow::anyhow!(
                            "missing eth tx index rocksdb column family '{}' for {}",
                            GATEWAY_ETH_TX_INDEX_ROCKSDB_CF_STATE,
                            path.display()
                        )
                    })?;
                let key = gateway_eth_broadcast_status_key(tx_hash);
                let value = crate::bincode_compat::serialize(&GatewayEthBroadcastStatusRecordV1 {
                    version: GATEWAY_ETH_BROADCAST_STATUS_RECORD_VERSION,
                    status: status.clone(),
                })
                .context("serialize eth broadcast-status record failed")?;
                db.put_cf(cf, key, value).with_context(|| {
                    format!(
                        "write eth broadcast-status into cf '{}' failed: {}",
                        GATEWAY_ETH_TX_INDEX_ROCKSDB_CF_STATE,
                        path.display()
                    )
                })?;
                Ok(())
            }
        }
    }

    fn save_pending_eth_public_broadcast_ticket(
        &self,
        ticket: &GatewayEthPublicBroadcastPendingTicketV1,
    ) -> Result<()> {
        match self {
            GatewayEthTxIndexStoreBackend::Memory => Ok(()),
            GatewayEthTxIndexStoreBackend::RocksDb { path } => {
                let db = open_gateway_eth_tx_index_rocksdb(path)?;
                let cf = db
                    .cf_handle(GATEWAY_ETH_TX_INDEX_ROCKSDB_CF_STATE)
                    .ok_or_else(|| {
                        anyhow::anyhow!(
                            "missing eth tx index rocksdb column family '{}' for {}",
                            GATEWAY_ETH_TX_INDEX_ROCKSDB_CF_STATE,
                            path.display()
                        )
                    })?;
                let key = gateway_eth_public_broadcast_pending_key(&ticket.tx_hash);
                let value =
                    crate::bincode_compat::serialize(&GatewayEthPublicBroadcastPendingRecordV1 {
                        version: GATEWAY_ETH_PUBLIC_BROADCAST_PENDING_RECORD_VERSION,
                        ticket: ticket.clone(),
                    })
                    .context("serialize eth pending public-broadcast ticket failed")?;
                db.put_cf(cf, key, value).with_context(|| {
                    format!(
                        "write eth pending public-broadcast ticket into cf '{}' failed: {}",
                        GATEWAY_ETH_TX_INDEX_ROCKSDB_CF_STATE,
                        path.display()
                    )
                })?;
                Ok(())
            }
        }
    }

    fn delete_pending_eth_public_broadcast_ticket(&self, tx_hash: &[u8; 32]) -> Result<()> {
        match self {
            GatewayEthTxIndexStoreBackend::Memory => Ok(()),
            GatewayEthTxIndexStoreBackend::RocksDb { path } => {
                let db = open_gateway_eth_tx_index_rocksdb(path)?;
                let cf = db
                    .cf_handle(GATEWAY_ETH_TX_INDEX_ROCKSDB_CF_STATE)
                    .ok_or_else(|| {
                        anyhow::anyhow!(
                            "missing eth tx index rocksdb column family '{}' for {}",
                            GATEWAY_ETH_TX_INDEX_ROCKSDB_CF_STATE,
                            path.display()
                        )
                    })?;
                let key = gateway_eth_public_broadcast_pending_key(tx_hash);
                db.delete_cf(cf, key).with_context(|| {
                    format!(
                        "delete eth pending public-broadcast ticket from cf '{}' failed: {}",
                        GATEWAY_ETH_TX_INDEX_ROCKSDB_CF_STATE,
                        path.display()
                    )
                })?;
                Ok(())
            }
        }
    }

    fn load_pending_eth_public_broadcast_tickets(
        &self,
        max_items: usize,
    ) -> Result<Vec<GatewayEthPublicBroadcastPendingTicketV1>> {
        if max_items == 0 {
            return Ok(Vec::new());
        }
        match self {
            GatewayEthTxIndexStoreBackend::Memory => Ok(Vec::new()),
            GatewayEthTxIndexStoreBackend::RocksDb { path } => {
                let db = open_gateway_eth_tx_index_rocksdb(path)?;
                let cf = db
                    .cf_handle(GATEWAY_ETH_TX_INDEX_ROCKSDB_CF_STATE)
                    .ok_or_else(|| {
                        anyhow::anyhow!(
                            "missing eth tx index rocksdb column family '{}' for {}",
                            GATEWAY_ETH_TX_INDEX_ROCKSDB_CF_STATE,
                            path.display()
                        )
                    })?;
                let mut out = Vec::new();
                let iter = db.iterator_cf(cf, IteratorMode::Start);
                for entry in iter {
                    let (key, raw) = entry.with_context(|| {
                        format!(
                            "iterate pending public-broadcast tickets from cf '{}' failed: {}",
                            GATEWAY_ETH_TX_INDEX_ROCKSDB_CF_STATE,
                            path.display()
                        )
                    })?;
                    if !key.starts_with(GATEWAY_ETH_PUBLIC_BROADCAST_PENDING_ROCKSDB_KEY_PREFIX) {
                        continue;
                    }
                    if raw.is_empty() {
                        continue;
                    }
                    let ticket = if let Ok(record) = crate::bincode_compat::deserialize::<
                        GatewayEthPublicBroadcastPendingRecordV1,
                    >(&raw)
                    {
                        if record.version != GATEWAY_ETH_PUBLIC_BROADCAST_PENDING_RECORD_VERSION {
                            continue;
                        }
                        record.ticket
                    } else if let Ok(legacy) = crate::bincode_compat::deserialize::<
                        GatewayEthPublicBroadcastPendingTicketV1,
                    >(&raw)
                    {
                        legacy
                    } else {
                        continue;
                    };
                    out.push(ticket);
                    if out.len() >= max_items {
                        break;
                    }
                }
                Ok(out)
            }
        }
    }

    fn load_eth_submit_status(&self, tx_hash: &[u8; 32]) -> Result<Option<GatewayEthSubmitStatus>> {
        match self {
            GatewayEthTxIndexStoreBackend::Memory => Ok(None),
            GatewayEthTxIndexStoreBackend::RocksDb { path } => {
                let db = open_gateway_eth_tx_index_rocksdb(path)?;
                let cf = db
                    .cf_handle(GATEWAY_ETH_TX_INDEX_ROCKSDB_CF_STATE)
                    .ok_or_else(|| {
                        anyhow::anyhow!(
                            "missing eth tx index rocksdb column family '{}' for {}",
                            GATEWAY_ETH_TX_INDEX_ROCKSDB_CF_STATE,
                            path.display()
                        )
                    })?;
                let key = gateway_eth_submit_status_key(tx_hash);
                let raw = db.get_cf(cf, &key).with_context(|| {
                    format!(
                        "read eth submit status from cf '{}' failed: {}",
                        GATEWAY_ETH_TX_INDEX_ROCKSDB_CF_STATE,
                        path.display()
                    )
                })?;
                let Some(raw) = raw else {
                    return Ok(None);
                };
                if raw.is_empty() {
                    return Ok(None);
                }
                if let Ok(record) =
                    crate::bincode_compat::deserialize::<GatewayEthSubmitStatusRecordV1>(&raw)
                {
                    if record.version != GATEWAY_ETH_SUBMIT_STATUS_RECORD_VERSION {
                        bail!(
                            "unsupported eth submit-status record version {} at {}",
                            record.version,
                            path.display()
                        );
                    }
                    return Ok(Some(record.status));
                }
                let legacy_status: GatewayEthSubmitStatus =
                    crate::bincode_compat::deserialize(&raw).with_context(|| {
                        format!(
                            "decode legacy eth submit-status record failed: {}",
                            path.display()
                        )
                    })?;
                Ok(Some(legacy_status))
            }
        }
    }

    fn save_eth_submit_status(
        &self,
        tx_hash: &[u8; 32],
        status: &GatewayEthSubmitStatus,
    ) -> Result<()> {
        match self {
            GatewayEthTxIndexStoreBackend::Memory => Ok(()),
            GatewayEthTxIndexStoreBackend::RocksDb { path } => {
                let db = open_gateway_eth_tx_index_rocksdb(path)?;
                let cf = db
                    .cf_handle(GATEWAY_ETH_TX_INDEX_ROCKSDB_CF_STATE)
                    .ok_or_else(|| {
                        anyhow::anyhow!(
                            "missing eth tx index rocksdb column family '{}' for {}",
                            GATEWAY_ETH_TX_INDEX_ROCKSDB_CF_STATE,
                            path.display()
                        )
                    })?;
                let key = gateway_eth_submit_status_key(tx_hash);
                let value = crate::bincode_compat::serialize(&GatewayEthSubmitStatusRecordV1 {
                    version: GATEWAY_ETH_SUBMIT_STATUS_RECORD_VERSION,
                    status: status.clone(),
                })
                .context("serialize eth submit-status record failed")?;
                db.put_cf(cf, key, value).with_context(|| {
                    format!(
                        "write eth submit-status into cf '{}' failed: {}",
                        GATEWAY_ETH_TX_INDEX_ROCKSDB_CF_STATE,
                        path.display()
                    )
                })?;
                Ok(())
            }
        }
    }

    fn load_eth_txs_by_chain(
        &self,
        chain_id: u64,
        max_items: usize,
    ) -> Result<Vec<GatewayEthTxIndexEntry>> {
        if max_items == 0 {
            return Ok(Vec::new());
        }
        match self {
            GatewayEthTxIndexStoreBackend::Memory => Ok(Vec::new()),
            GatewayEthTxIndexStoreBackend::RocksDb { path } => {
                let db = open_gateway_eth_tx_index_rocksdb(path)?;
                let cf = db
                    .cf_handle(GATEWAY_ETH_TX_INDEX_ROCKSDB_CF_STATE)
                    .ok_or_else(|| {
                        anyhow::anyhow!(
                            "missing eth tx index rocksdb column family '{}' for {}",
                            GATEWAY_ETH_TX_INDEX_ROCKSDB_CF_STATE,
                            path.display()
                        )
                    })?;
                let mut out = Vec::new();
                let mut seen = BTreeSet::<[u8; 32]>::new();
                let chain_prefix = gateway_eth_tx_block_index_chain_prefix(chain_id);
                let seek_upper = gateway_eth_tx_block_index_prefix(chain_id, u64::MAX);
                let iter = db.iterator_cf(
                    cf,
                    IteratorMode::From(seek_upper.as_slice(), Direction::Reverse),
                );
                for item in iter {
                    let (key, raw) = item.with_context(|| {
                        format!(
                            "iterate eth tx block-index records from cf '{}' failed: {}",
                            GATEWAY_ETH_TX_INDEX_ROCKSDB_CF_STATE,
                            path.display()
                        )
                    })?;
                    if !key.starts_with(chain_prefix.as_slice()) {
                        if key.as_ref() < chain_prefix.as_slice() {
                            break;
                        }
                        continue;
                    }
                    let tx_hash = if raw.len() >= 32 {
                        let mut hash = [0u8; 32];
                        hash.copy_from_slice(&raw[..32]);
                        hash
                    } else if key.len() >= chain_prefix.len() + 8 + 1 + 32 {
                        let mut hash = [0u8; 32];
                        let offset = chain_prefix.len() + 8 + 1;
                        hash.copy_from_slice(&key[offset..(offset + 32)]);
                        hash
                    } else {
                        continue;
                    };
                    if !seen.insert(tx_hash) {
                        continue;
                    }
                    let tx_key = gateway_eth_tx_index_key(&tx_hash);
                    let Some(tx_raw) = db.get_cf(cf, tx_key).with_context(|| {
                        format!(
                            "read eth tx by chain block-index from cf '{}' failed: {}",
                            GATEWAY_ETH_TX_INDEX_ROCKSDB_CF_STATE,
                            path.display()
                        )
                    })?
                    else {
                        continue;
                    };
                    if tx_raw.is_empty() {
                        continue;
                    }
                    let entry = if let Ok(record) =
                        crate::bincode_compat::deserialize::<GatewayEthTxIndexRecordV1>(&tx_raw)
                    {
                        if record.version != GATEWAY_ETH_TX_INDEX_RECORD_VERSION {
                            continue;
                        }
                        record.entry
                    } else if let Ok(legacy) =
                        crate::bincode_compat::deserialize::<GatewayEthTxIndexEntry>(&tx_raw)
                    {
                        legacy
                    } else {
                        continue;
                    };
                    if entry.chain_id != chain_id {
                        continue;
                    }
                    out.push(entry);
                    if out.len() >= max_items {
                        break;
                    }
                }
                out.sort_by(|a, b| {
                    a.nonce
                        .cmp(&b.nonce)
                        .then_with(|| a.tx_hash.cmp(&b.tx_hash))
                });
                Ok(out)
            }
        }
    }

    fn load_eth_latest_block_number(&self, chain_id: u64) -> Result<Option<u64>> {
        match self {
            GatewayEthTxIndexStoreBackend::Memory => Ok(None),
            GatewayEthTxIndexStoreBackend::RocksDb { path } => {
                let db = open_gateway_eth_tx_index_rocksdb(path)?;
                let cf = db
                    .cf_handle(GATEWAY_ETH_TX_INDEX_ROCKSDB_CF_STATE)
                    .ok_or_else(|| {
                        anyhow::anyhow!(
                            "missing eth tx index rocksdb column family '{}' for {}",
                            GATEWAY_ETH_TX_INDEX_ROCKSDB_CF_STATE,
                            path.display()
                        )
                    })?;
                let chain_prefix = gateway_eth_tx_block_index_chain_prefix(chain_id);
                let seek_upper = gateway_eth_tx_block_index_prefix(chain_id, u64::MAX);
                let iter = db.iterator_cf(
                    cf,
                    IteratorMode::From(seek_upper.as_slice(), Direction::Reverse),
                );
                for item in iter {
                    let (key, _raw) = item.with_context(|| {
                        format!(
                            "iterate eth latest block-index records from cf '{}' failed: {}",
                            GATEWAY_ETH_TX_INDEX_ROCKSDB_CF_STATE,
                            path.display()
                        )
                    })?;
                    if !key.starts_with(chain_prefix.as_slice()) {
                        if key.as_ref() < chain_prefix.as_slice() {
                            break;
                        }
                        continue;
                    }
                    if key.len() < chain_prefix.len() + 8 {
                        continue;
                    }
                    let mut block_bytes = [0u8; 8];
                    block_bytes.copy_from_slice(&key[chain_prefix.len()..(chain_prefix.len() + 8)]);
                    return Ok(Some(u64::from_be_bytes(block_bytes)));
                }
                Ok(None)
            }
        }
    }

    fn load_eth_txs_by_block(
        &self,
        chain_id: u64,
        block_number: u64,
        max_items: usize,
    ) -> Result<Vec<GatewayEthTxIndexEntry>> {
        if max_items == 0 {
            return Ok(Vec::new());
        }
        match self {
            GatewayEthTxIndexStoreBackend::Memory => Ok(Vec::new()),
            GatewayEthTxIndexStoreBackend::RocksDb { path } => {
                let db = open_gateway_eth_tx_index_rocksdb(path)?;
                let cf = db
                    .cf_handle(GATEWAY_ETH_TX_INDEX_ROCKSDB_CF_STATE)
                    .ok_or_else(|| {
                        anyhow::anyhow!(
                            "missing eth tx index rocksdb column family '{}' for {}",
                            GATEWAY_ETH_TX_INDEX_ROCKSDB_CF_STATE,
                            path.display()
                        )
                    })?;
                let prefix = gateway_eth_tx_block_index_prefix(chain_id, block_number);
                let mut out = Vec::new();
                let iter = db.iterator_cf(
                    cf,
                    IteratorMode::From(prefix.as_slice(), Direction::Forward),
                );
                for item in iter {
                    let (key, raw) = item.with_context(|| {
                        format!(
                            "iterate eth tx block index records from cf '{}' failed: {}",
                            GATEWAY_ETH_TX_INDEX_ROCKSDB_CF_STATE,
                            path.display()
                        )
                    })?;
                    if !key.starts_with(prefix.as_slice()) {
                        break;
                    }
                    let tx_hash = if raw.len() >= 32 {
                        let mut hash = [0u8; 32];
                        hash.copy_from_slice(&raw[..32]);
                        hash
                    } else if key.len() >= prefix.len() + 32 {
                        let mut hash = [0u8; 32];
                        hash.copy_from_slice(&key[prefix.len()..(prefix.len() + 32)]);
                        hash
                    } else {
                        continue;
                    };
                    let tx_key = gateway_eth_tx_index_key(&tx_hash);
                    let Some(tx_raw) = db.get_cf(cf, tx_key).with_context(|| {
                        format!(
                            "read eth tx by block index from cf '{}' failed: {}",
                            GATEWAY_ETH_TX_INDEX_ROCKSDB_CF_STATE,
                            path.display()
                        )
                    })?
                    else {
                        continue;
                    };
                    if tx_raw.is_empty() {
                        continue;
                    }
                    let entry = if let Ok(record) =
                        crate::bincode_compat::deserialize::<GatewayEthTxIndexRecordV1>(&tx_raw)
                    {
                        if record.version != GATEWAY_ETH_TX_INDEX_RECORD_VERSION {
                            continue;
                        }
                        record.entry
                    } else if let Ok(legacy) =
                        crate::bincode_compat::deserialize::<GatewayEthTxIndexEntry>(&tx_raw)
                    {
                        legacy
                    } else {
                        continue;
                    };
                    if entry.chain_id != chain_id || entry.nonce != block_number {
                        continue;
                    }
                    out.push(entry);
                    if out.len() >= max_items {
                        break;
                    }
                }
                Ok(out)
            }
        }
    }

    fn load_eth_block_number_by_hash(
        &self,
        chain_id: u64,
        block_hash: &[u8; 32],
    ) -> Result<Option<u64>> {
        match self {
            GatewayEthTxIndexStoreBackend::Memory => Ok(None),
            GatewayEthTxIndexStoreBackend::RocksDb { path } => {
                let db = open_gateway_eth_tx_index_rocksdb(path)?;
                let cf = db
                    .cf_handle(GATEWAY_ETH_TX_INDEX_ROCKSDB_CF_STATE)
                    .ok_or_else(|| {
                        anyhow::anyhow!(
                            "missing eth tx index rocksdb column family '{}' for {}",
                            GATEWAY_ETH_TX_INDEX_ROCKSDB_CF_STATE,
                            path.display()
                        )
                    })?;
                let key = gateway_eth_block_hash_index_key_by_hash(chain_id, block_hash);
                let Some(raw) = db.get_cf(cf, key).with_context(|| {
                    format!(
                        "read eth block hash index from cf '{}' failed: {}",
                        GATEWAY_ETH_TX_INDEX_ROCKSDB_CF_STATE,
                        path.display()
                    )
                })?
                else {
                    return Ok(None);
                };
                if raw.len() < 8 {
                    return Ok(None);
                }
                let mut bytes = [0u8; 8];
                bytes.copy_from_slice(&raw[..8]);
                Ok(Some(u64::from_be_bytes(bytes)))
            }
        }
    }

    fn load_evm_settlement_by_id(
        &self,
        settlement_id: &str,
    ) -> Result<Option<GatewayEvmSettlementIndexEntry>> {
        match self {
            GatewayEthTxIndexStoreBackend::Memory => Ok(None),
            GatewayEthTxIndexStoreBackend::RocksDb { path } => {
                let db = open_gateway_eth_tx_index_rocksdb(path)?;
                let cf = db
                    .cf_handle(GATEWAY_ETH_TX_INDEX_ROCKSDB_CF_STATE)
                    .ok_or_else(|| {
                        anyhow::anyhow!(
                            "missing eth tx index rocksdb column family '{}' for {}",
                            GATEWAY_ETH_TX_INDEX_ROCKSDB_CF_STATE,
                            path.display()
                        )
                    })?;
                let key = gateway_evm_settlement_index_key_by_id(settlement_id);
                let raw = db.get_cf(cf, &key).with_context(|| {
                    format!(
                        "read evm settlement index by id from cf '{}' failed: {}",
                        GATEWAY_ETH_TX_INDEX_ROCKSDB_CF_STATE,
                        path.display()
                    )
                })?;
                let Some(raw) = raw else {
                    return Ok(None);
                };
                if raw.is_empty() {
                    return Ok(None);
                }
                if let Ok(record) =
                    crate::bincode_compat::deserialize::<GatewayEvmSettlementIndexRecordV1>(&raw)
                {
                    if record.version != GATEWAY_EVM_SETTLEMENT_INDEX_RECORD_VERSION {
                        bail!(
                            "unsupported evm settlement index record version {} at {}",
                            record.version,
                            path.display()
                        );
                    }
                    return Ok(Some(record.entry));
                }
                let legacy_entry: GatewayEvmSettlementIndexEntry =
                    crate::bincode_compat::deserialize(&raw).with_context(|| {
                        format!(
                            "decode legacy evm settlement index record by id failed: {}",
                            path.display()
                        )
                    })?;
                Ok(Some(legacy_entry))
            }
        }
    }

    fn load_evm_settlement_by_tx_hash(
        &self,
        chain_id: u64,
        tx_hash: &[u8; 32],
    ) -> Result<Option<GatewayEvmSettlementIndexEntry>> {
        match self {
            GatewayEthTxIndexStoreBackend::Memory => Ok(None),
            GatewayEthTxIndexStoreBackend::RocksDb { path } => {
                let db = open_gateway_eth_tx_index_rocksdb(path)?;
                let cf = db
                    .cf_handle(GATEWAY_ETH_TX_INDEX_ROCKSDB_CF_STATE)
                    .ok_or_else(|| {
                        anyhow::anyhow!(
                            "missing eth tx index rocksdb column family '{}' for {}",
                            GATEWAY_ETH_TX_INDEX_ROCKSDB_CF_STATE,
                            path.display()
                        )
                    })?;
                let key = gateway_evm_settlement_index_key_by_tx(chain_id, tx_hash);
                let raw = db.get_cf(cf, &key).with_context(|| {
                    format!(
                        "read evm settlement tx-ref from cf '{}' failed: {}",
                        GATEWAY_ETH_TX_INDEX_ROCKSDB_CF_STATE,
                        path.display()
                    )
                })?;
                let Some(raw) = raw else {
                    return Ok(None);
                };
                if raw.is_empty() {
                    return Ok(None);
                }
                let settlement_id = if let Ok(ref_record) =
                    crate::bincode_compat::deserialize::<GatewayEvmSettlementTxRefRecordV1>(&raw)
                {
                    if ref_record.version != GATEWAY_EVM_SETTLEMENT_INDEX_RECORD_VERSION {
                        bail!(
                            "unsupported evm settlement tx-ref record version {} at {}",
                            ref_record.version,
                            path.display()
                        );
                    }
                    ref_record.settlement_id
                } else {
                    String::from_utf8(raw.to_vec()).with_context(|| {
                        format!(
                            "decode legacy evm settlement tx-ref failed: {}",
                            path.display()
                        )
                    })?
                };
                self.load_evm_settlement_by_id(&settlement_id)
            }
        }
    }

    fn save_evm_settlement(&self, entry: &GatewayEvmSettlementIndexEntry) -> Result<()> {
        match self {
            GatewayEthTxIndexStoreBackend::Memory => Ok(()),
            GatewayEthTxIndexStoreBackend::RocksDb { path } => {
                let db = open_gateway_eth_tx_index_rocksdb(path)?;
                let cf = db
                    .cf_handle(GATEWAY_ETH_TX_INDEX_ROCKSDB_CF_STATE)
                    .ok_or_else(|| {
                        anyhow::anyhow!(
                            "missing eth tx index rocksdb column family '{}' for {}",
                            GATEWAY_ETH_TX_INDEX_ROCKSDB_CF_STATE,
                            path.display()
                        )
                    })?;
                let key_by_id = gateway_evm_settlement_index_key_by_id(&entry.settlement_id);
                let value_by_id =
                    crate::bincode_compat::serialize(&GatewayEvmSettlementIndexRecordV1 {
                        version: GATEWAY_EVM_SETTLEMENT_INDEX_RECORD_VERSION,
                        entry: entry.clone(),
                    })
                    .context("serialize evm settlement index record failed")?;
                db.put_cf(cf, key_by_id, value_by_id).with_context(|| {
                    format!(
                        "write evm settlement index by id into cf '{}' failed: {}",
                        GATEWAY_ETH_TX_INDEX_ROCKSDB_CF_STATE,
                        path.display()
                    )
                })?;

                let key_by_tx =
                    gateway_evm_settlement_index_key_by_tx(entry.chain_id, &entry.income_tx_hash);
                let value_by_tx =
                    crate::bincode_compat::serialize(&GatewayEvmSettlementTxRefRecordV1 {
                        version: GATEWAY_EVM_SETTLEMENT_INDEX_RECORD_VERSION,
                        settlement_id: entry.settlement_id.clone(),
                    })
                    .context("serialize evm settlement tx-ref record failed")?;
                db.put_cf(cf, key_by_tx, value_by_tx).with_context(|| {
                    format!(
                        "write evm settlement tx-ref into cf '{}' failed: {}",
                        GATEWAY_ETH_TX_INDEX_ROCKSDB_CF_STATE,
                        path.display()
                    )
                })?;
                Ok(())
            }
        }
    }

    fn load_pending_payout_instruction(
        &self,
        settlement_id: &str,
    ) -> Result<Option<EvmFeePayoutInstructionV1>> {
        match self {
            GatewayEthTxIndexStoreBackend::Memory => Ok(None),
            GatewayEthTxIndexStoreBackend::RocksDb { path } => {
                let db = open_gateway_eth_tx_index_rocksdb(path)?;
                let cf = db
                    .cf_handle(GATEWAY_ETH_TX_INDEX_ROCKSDB_CF_STATE)
                    .ok_or_else(|| {
                        anyhow::anyhow!(
                            "missing eth tx index rocksdb column family '{}' for {}",
                            GATEWAY_ETH_TX_INDEX_ROCKSDB_CF_STATE,
                            path.display()
                        )
                    })?;
                let key = gateway_evm_payout_pending_key(settlement_id);
                let raw = db.get_cf(cf, &key).with_context(|| {
                    format!(
                        "read evm pending payout from cf '{}' failed: {}",
                        GATEWAY_ETH_TX_INDEX_ROCKSDB_CF_STATE,
                        path.display()
                    )
                })?;
                let Some(raw) = raw else {
                    return Ok(None);
                };
                if raw.is_empty() {
                    return Ok(None);
                }
                if let Ok(record) =
                    crate::bincode_compat::deserialize::<GatewayEvmPayoutPendingRecordV1>(&raw)
                {
                    if record.version != GATEWAY_EVM_PAYOUT_PENDING_RECORD_VERSION {
                        bail!(
                            "unsupported evm pending payout record version {} at {}",
                            record.version,
                            path.display()
                        );
                    }
                    return Ok(Some(record.instruction));
                }
                let legacy_instruction: EvmFeePayoutInstructionV1 =
                    crate::bincode_compat::deserialize(&raw).with_context(|| {
                        format!(
                            "decode legacy evm pending payout record failed: {}",
                            path.display()
                        )
                    })?;
                Ok(Some(legacy_instruction))
            }
        }
    }

    fn save_pending_payout_instruction(
        &self,
        instruction: &EvmFeePayoutInstructionV1,
    ) -> Result<()> {
        match self {
            GatewayEthTxIndexStoreBackend::Memory => Ok(()),
            GatewayEthTxIndexStoreBackend::RocksDb { path } => {
                let db = open_gateway_eth_tx_index_rocksdb(path)?;
                let cf = db
                    .cf_handle(GATEWAY_ETH_TX_INDEX_ROCKSDB_CF_STATE)
                    .ok_or_else(|| {
                        anyhow::anyhow!(
                            "missing eth tx index rocksdb column family '{}' for {}",
                            GATEWAY_ETH_TX_INDEX_ROCKSDB_CF_STATE,
                            path.display()
                        )
                    })?;
                let key = gateway_evm_payout_pending_key(&instruction.settlement_id);
                let value = crate::bincode_compat::serialize(&GatewayEvmPayoutPendingRecordV1 {
                    version: GATEWAY_EVM_PAYOUT_PENDING_RECORD_VERSION,
                    instruction: instruction.clone(),
                })
                .context("serialize evm pending payout record failed")?;
                db.put_cf(cf, key, value).with_context(|| {
                    format!(
                        "write evm pending payout into cf '{}' failed: {}",
                        GATEWAY_ETH_TX_INDEX_ROCKSDB_CF_STATE,
                        path.display()
                    )
                })?;
                Ok(())
            }
        }
    }

    fn delete_pending_payout_instruction(&self, settlement_id: &str) -> Result<()> {
        match self {
            GatewayEthTxIndexStoreBackend::Memory => Ok(()),
            GatewayEthTxIndexStoreBackend::RocksDb { path } => {
                let db = open_gateway_eth_tx_index_rocksdb(path)?;
                let cf = db
                    .cf_handle(GATEWAY_ETH_TX_INDEX_ROCKSDB_CF_STATE)
                    .ok_or_else(|| {
                        anyhow::anyhow!(
                            "missing eth tx index rocksdb column family '{}' for {}",
                            GATEWAY_ETH_TX_INDEX_ROCKSDB_CF_STATE,
                            path.display()
                        )
                    })?;
                let key = gateway_evm_payout_pending_key(settlement_id);
                db.delete_cf(cf, key).with_context(|| {
                    format!(
                        "delete evm pending payout from cf '{}' failed: {}",
                        GATEWAY_ETH_TX_INDEX_ROCKSDB_CF_STATE,
                        path.display()
                    )
                })?;
                Ok(())
            }
        }
    }

    fn load_pending_payout_instructions(
        &self,
        max_items: usize,
    ) -> Result<Vec<EvmFeePayoutInstructionV1>> {
        if max_items == 0 {
            return Ok(Vec::new());
        }
        match self {
            GatewayEthTxIndexStoreBackend::Memory => Ok(Vec::new()),
            GatewayEthTxIndexStoreBackend::RocksDb { path } => {
                let db = open_gateway_eth_tx_index_rocksdb(path)?;
                let cf = db
                    .cf_handle(GATEWAY_ETH_TX_INDEX_ROCKSDB_CF_STATE)
                    .ok_or_else(|| {
                        anyhow::anyhow!(
                            "missing eth tx index rocksdb column family '{}' for {}",
                            GATEWAY_ETH_TX_INDEX_ROCKSDB_CF_STATE,
                            path.display()
                        )
                    })?;
                let mut out = Vec::new();
                let iter = db.iterator_cf(cf, IteratorMode::Start);
                for entry in iter {
                    let (key, raw) = entry.with_context(|| {
                        format!(
                            "iterate pending payout records from cf '{}' failed: {}",
                            GATEWAY_ETH_TX_INDEX_ROCKSDB_CF_STATE,
                            path.display()
                        )
                    })?;
                    if !key.starts_with(GATEWAY_EVM_PAYOUT_PENDING_ROCKSDB_KEY_PREFIX) {
                        continue;
                    }
                    if raw.is_empty() {
                        continue;
                    }
                    let instruction = if let Ok(record) =
                        crate::bincode_compat::deserialize::<GatewayEvmPayoutPendingRecordV1>(&raw)
                    {
                        if record.version != GATEWAY_EVM_PAYOUT_PENDING_RECORD_VERSION {
                            continue;
                        }
                        record.instruction
                    } else if let Ok(legacy) =
                        crate::bincode_compat::deserialize::<EvmFeePayoutInstructionV1>(&raw)
                    {
                        legacy
                    } else {
                        continue;
                    };
                    out.push(instruction);
                    if out.len() >= max_items {
                        break;
                    }
                }
                Ok(out)
            }
        }
    }

    fn load_evm_atomic_ready_by_intent(
        &self,
        intent_id: &str,
    ) -> Result<Option<GatewayEvmAtomicReadyIndexEntry>> {
        match self {
            GatewayEthTxIndexStoreBackend::Memory => Ok(None),
            GatewayEthTxIndexStoreBackend::RocksDb { path } => {
                let db = open_gateway_eth_tx_index_rocksdb(path)?;
                let cf = db
                    .cf_handle(GATEWAY_ETH_TX_INDEX_ROCKSDB_CF_STATE)
                    .ok_or_else(|| {
                        anyhow::anyhow!(
                            "missing eth tx index rocksdb column family '{}' for {}",
                            GATEWAY_ETH_TX_INDEX_ROCKSDB_CF_STATE,
                            path.display()
                        )
                    })?;
                let key = gateway_evm_atomic_ready_index_key_by_intent(intent_id);
                let raw = db.get_cf(cf, &key).with_context(|| {
                    format!(
                        "read evm atomic-ready index by intent from cf '{}' failed: {}",
                        GATEWAY_ETH_TX_INDEX_ROCKSDB_CF_STATE,
                        path.display()
                    )
                })?;
                let Some(raw) = raw else {
                    return Ok(None);
                };
                if raw.is_empty() {
                    return Ok(None);
                }
                if let Ok(record) =
                    crate::bincode_compat::deserialize::<GatewayEvmAtomicReadyIndexRecordV1>(&raw)
                {
                    if record.version != GATEWAY_EVM_ATOMIC_READY_INDEX_RECORD_VERSION {
                        bail!(
                            "unsupported evm atomic-ready index record version {} at {}",
                            record.version,
                            path.display()
                        );
                    }
                    return Ok(Some(record.entry));
                }
                let legacy_entry: GatewayEvmAtomicReadyIndexEntry =
                    crate::bincode_compat::deserialize(&raw).with_context(|| {
                        format!(
                            "decode legacy evm atomic-ready index by intent failed: {}",
                            path.display()
                        )
                    })?;
                Ok(Some(legacy_entry))
            }
        }
    }

    fn save_evm_atomic_ready(&self, entry: &GatewayEvmAtomicReadyIndexEntry) -> Result<()> {
        match self {
            GatewayEthTxIndexStoreBackend::Memory => Ok(()),
            GatewayEthTxIndexStoreBackend::RocksDb { path } => {
                let db = open_gateway_eth_tx_index_rocksdb(path)?;
                let cf = db
                    .cf_handle(GATEWAY_ETH_TX_INDEX_ROCKSDB_CF_STATE)
                    .ok_or_else(|| {
                        anyhow::anyhow!(
                            "missing eth tx index rocksdb column family '{}' for {}",
                            GATEWAY_ETH_TX_INDEX_ROCKSDB_CF_STATE,
                            path.display()
                        )
                    })?;
                let key = gateway_evm_atomic_ready_index_key_by_intent(&entry.intent_id);
                let value = crate::bincode_compat::serialize(&GatewayEvmAtomicReadyIndexRecordV1 {
                    version: GATEWAY_EVM_ATOMIC_READY_INDEX_RECORD_VERSION,
                    entry: entry.clone(),
                })
                .context("serialize evm atomic-ready index record failed")?;
                db.put_cf(cf, key, value).with_context(|| {
                    format!(
                        "write evm atomic-ready index into cf '{}' failed: {}",
                        GATEWAY_ETH_TX_INDEX_ROCKSDB_CF_STATE,
                        path.display()
                    )
                })?;
                Ok(())
            }
        }
    }

    fn load_pending_atomic_ready(&self, intent_id: &str) -> Result<Option<AtomicBroadcastReadyV1>> {
        match self {
            GatewayEthTxIndexStoreBackend::Memory => Ok(None),
            GatewayEthTxIndexStoreBackend::RocksDb { path } => {
                let db = open_gateway_eth_tx_index_rocksdb(path)?;
                let cf = db
                    .cf_handle(GATEWAY_ETH_TX_INDEX_ROCKSDB_CF_STATE)
                    .ok_or_else(|| {
                        anyhow::anyhow!(
                            "missing eth tx index rocksdb column family '{}' for {}",
                            GATEWAY_ETH_TX_INDEX_ROCKSDB_CF_STATE,
                            path.display()
                        )
                    })?;
                let key = gateway_evm_atomic_ready_pending_key(intent_id);
                let raw = db.get_cf(cf, &key).with_context(|| {
                    format!(
                        "read evm pending atomic-ready from cf '{}' failed: {}",
                        GATEWAY_ETH_TX_INDEX_ROCKSDB_CF_STATE,
                        path.display()
                    )
                })?;
                let Some(raw) = raw else {
                    return Ok(None);
                };
                if raw.is_empty() {
                    return Ok(None);
                }
                if let Ok(record) =
                    crate::bincode_compat::deserialize::<GatewayEvmAtomicReadyPendingRecordV1>(&raw)
                {
                    if record.version != GATEWAY_EVM_ATOMIC_READY_PENDING_RECORD_VERSION {
                        bail!(
                            "unsupported evm pending atomic-ready record version {} at {}",
                            record.version,
                            path.display()
                        );
                    }
                    return Ok(Some(record.ready_item));
                }
                let legacy_item: AtomicBroadcastReadyV1 = crate::bincode_compat::deserialize(&raw)
                    .with_context(|| {
                        format!(
                            "decode legacy evm pending atomic-ready record failed: {}",
                            path.display()
                        )
                    })?;
                Ok(Some(legacy_item))
            }
        }
    }

    fn load_pending_atomic_readies(&self, max_items: usize) -> Result<Vec<AtomicBroadcastReadyV1>> {
        if max_items == 0 {
            return Ok(Vec::new());
        }
        match self {
            GatewayEthTxIndexStoreBackend::Memory => Ok(Vec::new()),
            GatewayEthTxIndexStoreBackend::RocksDb { path } => {
                let db = open_gateway_eth_tx_index_rocksdb(path)?;
                let cf = db
                    .cf_handle(GATEWAY_ETH_TX_INDEX_ROCKSDB_CF_STATE)
                    .ok_or_else(|| {
                        anyhow::anyhow!(
                            "missing eth tx index rocksdb column family '{}' for {}",
                            GATEWAY_ETH_TX_INDEX_ROCKSDB_CF_STATE,
                            path.display()
                        )
                    })?;
                let mut out = Vec::new();
                let iter = db.iterator_cf(cf, IteratorMode::Start);
                for entry in iter {
                    let (key, raw) = entry.with_context(|| {
                        format!(
                            "iterate pending atomic-ready from cf '{}' failed: {}",
                            GATEWAY_ETH_TX_INDEX_ROCKSDB_CF_STATE,
                            path.display()
                        )
                    })?;
                    if !key.starts_with(GATEWAY_EVM_ATOMIC_READY_PENDING_ROCKSDB_KEY_PREFIX) {
                        continue;
                    }
                    if raw.is_empty() {
                        continue;
                    }
                    let item = if let Ok(record) = crate::bincode_compat::deserialize::<
                        GatewayEvmAtomicReadyPendingRecordV1,
                    >(&raw)
                    {
                        if record.version != GATEWAY_EVM_ATOMIC_READY_PENDING_RECORD_VERSION {
                            continue;
                        }
                        record.ready_item
                    } else if let Ok(legacy_item) =
                        crate::bincode_compat::deserialize::<AtomicBroadcastReadyV1>(&raw)
                    {
                        legacy_item
                    } else {
                        continue;
                    };
                    out.push(item);
                    if out.len() >= max_items {
                        break;
                    }
                }
                Ok(out)
            }
        }
    }

    fn save_pending_atomic_ready(&self, item: &AtomicBroadcastReadyV1) -> Result<()> {
        match self {
            GatewayEthTxIndexStoreBackend::Memory => Ok(()),
            GatewayEthTxIndexStoreBackend::RocksDb { path } => {
                let db = open_gateway_eth_tx_index_rocksdb(path)?;
                let cf = db
                    .cf_handle(GATEWAY_ETH_TX_INDEX_ROCKSDB_CF_STATE)
                    .ok_or_else(|| {
                        anyhow::anyhow!(
                            "missing eth tx index rocksdb column family '{}' for {}",
                            GATEWAY_ETH_TX_INDEX_ROCKSDB_CF_STATE,
                            path.display()
                        )
                    })?;
                let key = gateway_evm_atomic_ready_pending_key(&item.intent.intent_id);
                let value =
                    crate::bincode_compat::serialize(&GatewayEvmAtomicReadyPendingRecordV1 {
                        version: GATEWAY_EVM_ATOMIC_READY_PENDING_RECORD_VERSION,
                        ready_item: item.clone(),
                    })
                    .context("serialize evm pending atomic-ready record failed")?;
                db.put_cf(cf, key, value).with_context(|| {
                    format!(
                        "write evm pending atomic-ready into cf '{}' failed: {}",
                        GATEWAY_ETH_TX_INDEX_ROCKSDB_CF_STATE,
                        path.display()
                    )
                })?;
                Ok(())
            }
        }
    }

    fn delete_pending_atomic_ready(&self, intent_id: &str) -> Result<()> {
        match self {
            GatewayEthTxIndexStoreBackend::Memory => Ok(()),
            GatewayEthTxIndexStoreBackend::RocksDb { path } => {
                let db = open_gateway_eth_tx_index_rocksdb(path)?;
                let cf = db
                    .cf_handle(GATEWAY_ETH_TX_INDEX_ROCKSDB_CF_STATE)
                    .ok_or_else(|| {
                        anyhow::anyhow!(
                            "missing eth tx index rocksdb column family '{}' for {}",
                            GATEWAY_ETH_TX_INDEX_ROCKSDB_CF_STATE,
                            path.display()
                        )
                    })?;
                let key = gateway_evm_atomic_ready_pending_key(intent_id);
                db.delete_cf(cf, key).with_context(|| {
                    format!(
                        "delete evm pending atomic-ready from cf '{}' failed: {}",
                        GATEWAY_ETH_TX_INDEX_ROCKSDB_CF_STATE,
                        path.display()
                    )
                })?;
                Ok(())
            }
        }
    }

    fn load_pending_atomic_broadcast_ticket(
        &self,
        intent_id: &str,
    ) -> Result<Option<GatewayEvmAtomicBroadcastTicketV1>> {
        match self {
            GatewayEthTxIndexStoreBackend::Memory => Ok(None),
            GatewayEthTxIndexStoreBackend::RocksDb { path } => {
                let db = open_gateway_eth_tx_index_rocksdb(path)?;
                let cf = db
                    .cf_handle(GATEWAY_ETH_TX_INDEX_ROCKSDB_CF_STATE)
                    .ok_or_else(|| {
                        anyhow::anyhow!(
                            "missing eth tx index rocksdb column family '{}' for {}",
                            GATEWAY_ETH_TX_INDEX_ROCKSDB_CF_STATE,
                            path.display()
                        )
                    })?;
                let key = gateway_evm_atomic_broadcast_pending_key(intent_id);
                let raw = db.get_cf(cf, &key).with_context(|| {
                    format!(
                        "read evm pending atomic-broadcast ticket from cf '{}' failed: {}",
                        GATEWAY_ETH_TX_INDEX_ROCKSDB_CF_STATE,
                        path.display()
                    )
                })?;
                let Some(raw) = raw else {
                    return Ok(None);
                };
                if raw.is_empty() {
                    return Ok(None);
                }
                if let Ok(record) = crate::bincode_compat::deserialize::<
                    GatewayEvmAtomicBroadcastPendingRecordV1,
                >(&raw)
                {
                    if record.version != GATEWAY_EVM_ATOMIC_BROADCAST_PENDING_RECORD_VERSION {
                        bail!(
                            "unsupported evm pending atomic-broadcast ticket record version {} at {}",
                            record.version,
                            path.display()
                        );
                    }
                    return Ok(Some(record.ticket));
                }
                let legacy_ticket: GatewayEvmAtomicBroadcastTicketV1 =
                    crate::bincode_compat::deserialize(&raw).with_context(|| {
                        format!(
                            "decode legacy evm pending atomic-broadcast ticket failed: {}",
                            path.display()
                        )
                    })?;
                Ok(Some(legacy_ticket))
            }
        }
    }

    fn save_pending_atomic_broadcast_ticket(
        &self,
        ticket: &GatewayEvmAtomicBroadcastTicketV1,
    ) -> Result<()> {
        match self {
            GatewayEthTxIndexStoreBackend::Memory => Ok(()),
            GatewayEthTxIndexStoreBackend::RocksDb { path } => {
                let db = open_gateway_eth_tx_index_rocksdb(path)?;
                let cf = db
                    .cf_handle(GATEWAY_ETH_TX_INDEX_ROCKSDB_CF_STATE)
                    .ok_or_else(|| {
                        anyhow::anyhow!(
                            "missing eth tx index rocksdb column family '{}' for {}",
                            GATEWAY_ETH_TX_INDEX_ROCKSDB_CF_STATE,
                            path.display()
                        )
                    })?;
                let key = gateway_evm_atomic_broadcast_pending_key(&ticket.intent_id);
                let value =
                    crate::bincode_compat::serialize(&GatewayEvmAtomicBroadcastPendingRecordV1 {
                        version: GATEWAY_EVM_ATOMIC_BROADCAST_PENDING_RECORD_VERSION,
                        ticket: ticket.clone(),
                    })
                    .context("serialize evm pending atomic-broadcast ticket record failed")?;
                db.put_cf(cf, key, value).with_context(|| {
                    format!(
                        "write evm pending atomic-broadcast ticket into cf '{}' failed: {}",
                        GATEWAY_ETH_TX_INDEX_ROCKSDB_CF_STATE,
                        path.display()
                    )
                })?;
                Ok(())
            }
        }
    }

    fn delete_pending_atomic_broadcast_ticket(&self, intent_id: &str) -> Result<()> {
        match self {
            GatewayEthTxIndexStoreBackend::Memory => Ok(()),
            GatewayEthTxIndexStoreBackend::RocksDb { path } => {
                let db = open_gateway_eth_tx_index_rocksdb(path)?;
                let cf = db
                    .cf_handle(GATEWAY_ETH_TX_INDEX_ROCKSDB_CF_STATE)
                    .ok_or_else(|| {
                        anyhow::anyhow!(
                            "missing eth tx index rocksdb column family '{}' for {}",
                            GATEWAY_ETH_TX_INDEX_ROCKSDB_CF_STATE,
                            path.display()
                        )
                    })?;
                let key = gateway_evm_atomic_broadcast_pending_key(intent_id);
                db.delete_cf(cf, key).with_context(|| {
                    format!(
                        "delete evm pending atomic-broadcast ticket from cf '{}' failed: {}",
                        GATEWAY_ETH_TX_INDEX_ROCKSDB_CF_STATE,
                        path.display()
                    )
                })?;
                Ok(())
            }
        }
    }

    fn load_atomic_broadcast_payload(&self, intent_id: &str) -> Result<Option<Vec<u8>>> {
        match self {
            GatewayEthTxIndexStoreBackend::Memory => Ok(None),
            GatewayEthTxIndexStoreBackend::RocksDb { path } => {
                let db = open_gateway_eth_tx_index_rocksdb(path)?;
                let cf = db
                    .cf_handle(GATEWAY_ETH_TX_INDEX_ROCKSDB_CF_STATE)
                    .ok_or_else(|| {
                        anyhow::anyhow!(
                            "missing eth tx index rocksdb column family '{}' for {}",
                            GATEWAY_ETH_TX_INDEX_ROCKSDB_CF_STATE,
                            path.display()
                        )
                    })?;
                let key = gateway_evm_atomic_broadcast_payload_key(intent_id);
                let raw = db.get_cf(cf, &key).with_context(|| {
                    format!(
                        "read evm atomic-broadcast payload from cf '{}' failed: {}",
                        GATEWAY_ETH_TX_INDEX_ROCKSDB_CF_STATE,
                        path.display()
                    )
                })?;
                Ok(raw)
            }
        }
    }

    fn save_atomic_broadcast_payload(&self, intent_id: &str, payload: &[u8]) -> Result<()> {
        match self {
            GatewayEthTxIndexStoreBackend::Memory => Ok(()),
            GatewayEthTxIndexStoreBackend::RocksDb { path } => {
                let db = open_gateway_eth_tx_index_rocksdb(path)?;
                let cf = db
                    .cf_handle(GATEWAY_ETH_TX_INDEX_ROCKSDB_CF_STATE)
                    .ok_or_else(|| {
                        anyhow::anyhow!(
                            "missing eth tx index rocksdb column family '{}' for {}",
                            GATEWAY_ETH_TX_INDEX_ROCKSDB_CF_STATE,
                            path.display()
                        )
                    })?;
                let key = gateway_evm_atomic_broadcast_payload_key(intent_id);
                db.put_cf(cf, key, payload).with_context(|| {
                    format!(
                        "write evm atomic-broadcast payload into cf '{}' failed: {}",
                        GATEWAY_ETH_TX_INDEX_ROCKSDB_CF_STATE,
                        path.display()
                    )
                })?;
                Ok(())
            }
        }
    }

    fn delete_atomic_broadcast_payload(&self, intent_id: &str) -> Result<()> {
        match self {
            GatewayEthTxIndexStoreBackend::Memory => Ok(()),
            GatewayEthTxIndexStoreBackend::RocksDb { path } => {
                let db = open_gateway_eth_tx_index_rocksdb(path)?;
                let cf = db
                    .cf_handle(GATEWAY_ETH_TX_INDEX_ROCKSDB_CF_STATE)
                    .ok_or_else(|| {
                        anyhow::anyhow!(
                            "missing eth tx index rocksdb column family '{}' for {}",
                            GATEWAY_ETH_TX_INDEX_ROCKSDB_CF_STATE,
                            path.display()
                        )
                    })?;
                let key = gateway_evm_atomic_broadcast_payload_key(intent_id);
                db.delete_cf(cf, key).with_context(|| {
                    format!(
                        "delete evm atomic-broadcast payload from cf '{}' failed: {}",
                        GATEWAY_ETH_TX_INDEX_ROCKSDB_CF_STATE,
                        path.display()
                    )
                })?;
                Ok(())
            }
        }
    }

    fn load_pending_atomic_broadcast_tickets(
        &self,
        max_items: usize,
    ) -> Result<Vec<GatewayEvmAtomicBroadcastTicketV1>> {
        if max_items == 0 {
            return Ok(Vec::new());
        }
        match self {
            GatewayEthTxIndexStoreBackend::Memory => Ok(Vec::new()),
            GatewayEthTxIndexStoreBackend::RocksDb { path } => {
                let db = open_gateway_eth_tx_index_rocksdb(path)?;
                let cf = db
                    .cf_handle(GATEWAY_ETH_TX_INDEX_ROCKSDB_CF_STATE)
                    .ok_or_else(|| {
                        anyhow::anyhow!(
                            "missing eth tx index rocksdb column family '{}' for {}",
                            GATEWAY_ETH_TX_INDEX_ROCKSDB_CF_STATE,
                            path.display()
                        )
                    })?;
                let mut out = Vec::new();
                let iter = db.iterator_cf(cf, IteratorMode::Start);
                for entry in iter {
                    let (key, raw) = entry.with_context(|| {
                        format!(
                            "iterate pending atomic-broadcast tickets from cf '{}' failed: {}",
                            GATEWAY_ETH_TX_INDEX_ROCKSDB_CF_STATE,
                            path.display()
                        )
                    })?;
                    if !key.starts_with(GATEWAY_EVM_ATOMIC_BROADCAST_PENDING_ROCKSDB_KEY_PREFIX) {
                        continue;
                    }
                    if raw.is_empty() {
                        continue;
                    }
                    let ticket = if let Ok(record) = crate::bincode_compat::deserialize::<
                        GatewayEvmAtomicBroadcastPendingRecordV1,
                    >(&raw)
                    {
                        if record.version != GATEWAY_EVM_ATOMIC_BROADCAST_PENDING_RECORD_VERSION {
                            continue;
                        }
                        record.ticket
                    } else if let Ok(legacy) = crate::bincode_compat::deserialize::<
                        GatewayEvmAtomicBroadcastTicketV1,
                    >(&raw)
                    {
                        legacy
                    } else {
                        continue;
                    };
                    out.push(ticket);
                    if out.len() >= max_items {
                        break;
                    }
                }
                Ok(out)
            }
        }
    }
}

fn resolve_gateway_ua_store_backend() -> Result<GatewayUaStoreBackend> {
    let backend = string_env(
        "NOVOVM_GATEWAY_UA_STORE_BACKEND",
        GATEWAY_UA_STORE_BACKEND_ROCKSDB,
    )
    .trim()
    .to_ascii_lowercase();
    let path = if let Some(custom) = string_env_nonempty("NOVOVM_GATEWAY_UA_STORE_PATH") {
        PathBuf::from(custom)
    } else {
        match backend.as_str() {
            GATEWAY_UA_STORE_BACKEND_ROCKSDB => {
                PathBuf::from("artifacts/gateway/unified-account-router.rocksdb")
            }
            _ => PathBuf::from("artifacts/gateway/unified-account-router.bin"),
        }
    };
    match backend.as_str() {
        "rocksdb" => Ok(GatewayUaStoreBackend::RocksDb { path }),
        "bincode_file" | "file" | "bincode" => {
            if bool_env("NOVOVM_ALLOW_NON_PROD_UA_BACKEND", false) {
                Ok(GatewayUaStoreBackend::BincodeFile { path })
            } else {
                bail!(
                    "NOVOVM_GATEWAY_UA_STORE_BACKEND={} is non-production; use rocksdb or set NOVOVM_ALLOW_NON_PROD_UA_BACKEND=1 for explicit override",
                    backend
                )
            }
        }
        _ => bail!(
            "invalid NOVOVM_GATEWAY_UA_STORE_BACKEND={}; valid: rocksdb|bincode_file|file|bincode",
            backend
        ),
    }
}

fn resolve_gateway_eth_tx_index_store_backend() -> Result<GatewayEthTxIndexStoreBackend> {
    let backend = string_env(
        "NOVOVM_GATEWAY_ETH_TX_INDEX_BACKEND",
        GATEWAY_ETH_TX_INDEX_BACKEND_ROCKSDB,
    )
    .trim()
    .to_ascii_lowercase();
    match backend.as_str() {
        "memory" => {
            if bool_env(GATEWAY_ALLOW_NON_PROD_BACKEND_ENV, false) {
                Ok(GatewayEthTxIndexStoreBackend::Memory)
            } else {
                bail!(
                    "NOVOVM_GATEWAY_ETH_TX_INDEX_BACKEND=memory is non-production; use rocksdb or set {}=1 for explicit override",
                    GATEWAY_ALLOW_NON_PROD_BACKEND_ENV
                )
            }
        }
        "rocksdb" => {
            let path = if let Some(custom) = string_env_nonempty("NOVOVM_GATEWAY_ETH_TX_INDEX_PATH")
            {
                PathBuf::from(custom)
            } else {
                PathBuf::from("artifacts/gateway/eth-tx-index.rocksdb")
            };
            Ok(GatewayEthTxIndexStoreBackend::RocksDb { path })
        }
        _ => bail!(
            "invalid NOVOVM_GATEWAY_ETH_TX_INDEX_BACKEND={}; valid: memory|rocksdb",
            backend
        ),
    }
}

fn open_gateway_ua_rocksdb(path: &Path) -> Result<RocksDb> {
    ensure_parent_dir(path, "gateway ua rocksdb")?;
    let mut opts = RocksDbOptions::default();
    opts.create_if_missing(true);
    opts.create_missing_column_families(true);
    let cf_descriptors = vec![
        ColumnFamilyDescriptor::new(DEFAULT_COLUMN_FAMILY_NAME, RocksDbOptions::default()),
        ColumnFamilyDescriptor::new(GATEWAY_UA_STORE_ROCKSDB_CF_STATE, RocksDbOptions::default()),
    ];
    RocksDb::open_cf_descriptors(&opts, path, cf_descriptors)
        .with_context(|| format!("open gateway ua rocksdb failed: {}", path.display()))
}

fn open_gateway_eth_tx_index_rocksdb(path: &Path) -> Result<RocksDb> {
    ensure_parent_dir(path, "gateway eth tx index rocksdb")?;
    let mut opts = RocksDbOptions::default();
    opts.create_if_missing(true);
    opts.create_missing_column_families(true);
    let cf_descriptors = vec![
        ColumnFamilyDescriptor::new(DEFAULT_COLUMN_FAMILY_NAME, RocksDbOptions::default()),
        ColumnFamilyDescriptor::new(
            GATEWAY_ETH_TX_INDEX_ROCKSDB_CF_STATE,
            RocksDbOptions::default(),
        ),
    ];
    RocksDb::open_cf_descriptors(&opts, path, cf_descriptors).with_context(|| {
        format!(
            "open gateway eth tx index rocksdb failed: {}",
            path.display()
        )
    })
}

fn handle_gateway_request(
    runtime: &mut GatewayRuntime,
    mut request: tiny_http::Request,
) -> Result<()> {
    if request.method() != &tiny_http::Method::Post {
        let body = rpc_error_body(
            serde_json::Value::Null,
            -32600,
            "only HTTP POST is supported on gateway endpoint",
        );
        respond_json_http(request, 405, &body)?;
        return Ok(());
    }

    let mut body_bytes = Vec::new();
    request
        .as_reader()
        .take((runtime.max_body_bytes as u64).saturating_add(1))
        .read_to_end(&mut body_bytes)
        .context("read gateway request body failed")?;
    if body_bytes.is_empty() {
        let body = rpc_error_body(serde_json::Value::Null, -32600, "request body is empty");
        respond_json_http(request, 400, &body)?;
        return Ok(());
    }
    if body_bytes.len() > runtime.max_body_bytes {
        let body = rpc_error_body(serde_json::Value::Null, -32600, "request body too large");
        respond_json_http(request, 413, &body)?;
        return Ok(());
    }

    let payload: serde_json::Value = match serde_json::from_slice(&body_bytes) {
        Ok(v) => v,
        Err(e) => {
            let body = rpc_error_body(
                serde_json::Value::Null,
                -32700,
                &format!("invalid JSON payload: {e}"),
            );
            respond_json_http(request, 400, &body)?;
            return Ok(());
        }
    };
    let id = payload
        .get("id")
        .cloned()
        .unwrap_or(serde_json::Value::Null);
    let method = match payload.get("method").and_then(|v| v.as_str()) {
        Some(v) if !v.trim().is_empty() => v.trim(),
        _ => {
            let body = rpc_error_body(id, -32600, "missing jsonrpc method");
            respond_json_http(request, 400, &body)?;
            return Ok(());
        }
    };
    let params = payload
        .get("params")
        .cloned()
        .unwrap_or_else(|| serde_json::json!({}));
    let overlay_node_id = resolve_gateway_overlay_node_id(&params);
    let overlay_session_id = resolve_gateway_overlay_session_id(&params, &overlay_node_id);

    let mut ctx = GatewayMethodContext {
        eth_tx_index_store: &runtime.eth_tx_index_store,
        eth_default_chain_id: runtime.eth_default_chain_id,
        spool_dir: &runtime.spool_dir,
        overlay_node_id,
        overlay_session_id,
        eth_filters: &mut runtime.eth_filters,
    };
    match run_gateway_method(
        &mut runtime.router,
        &mut runtime.eth_tx_index,
        &mut runtime.evm_settlement_index_by_id,
        &mut runtime.evm_settlement_index_by_tx,
        &mut runtime.evm_pending_payout_by_settlement,
        &mut ctx,
        method,
        &params,
    ) {
        Ok((result, changed)) => {
            if changed {
                runtime.ua_store.save_router(&runtime.router)?;
            }
            let body = serde_json::json!({
                "jsonrpc": "2.0",
                "id": id,
                "result": result,
            });
            respond_json_http(request, 200, &body)?;
        }
        Err(e) => {
            // Keep state consistency if any runtime step failed after mutation.
            if let Ok(restored) = runtime.ua_store.load_router() {
                runtime.router = restored;
            }
            let raw_message = e.to_string();
            let code = gateway_error_code_for_method(method, &raw_message);
            let message = gateway_error_message_for_method(method, code, &raw_message);
            let data = gateway_error_data_for_method(method, code, &raw_message);
            persist_gateway_eth_submit_failure_status_from_error(
                &runtime.eth_tx_index_store,
                Some(&runtime.router),
                method,
                &params,
                &raw_message,
                code,
                &message,
                runtime.eth_default_chain_id,
            );
            let body = rpc_error_body_with_data(id, code, &message, data);
            respond_json_http(request, 200, &body)?;
        }
    }
    auto_replay_pending_payouts(runtime);
    auto_replay_pending_atomic_broadcasts(runtime);
    auto_replay_pending_public_broadcasts(runtime);
    Ok(())
}

fn sanitize_gateway_overlay_id(raw: &str, fallback: &str) -> String {
    let mut normalized = String::new();
    for ch in raw.trim().chars() {
        if ch.is_ascii_alphanumeric() || matches!(ch, '_' | '-' | ':' | '.') {
            normalized.push(ch);
            if normalized.len() >= 96 {
                break;
            }
        }
    }
    if normalized.is_empty() {
        fallback.to_string()
    } else {
        normalized
    }
}

fn resolve_gateway_overlay_node_id(params: &serde_json::Value) -> String {
    let raw = param_as_string_any_with_tx(
        params,
        &["overlay_node_id", "overlayNodeId", "node_id", "nodeId"],
    )
    .or_else(|| string_env_nonempty("NOVOVM_OVERLAY_NODE_ID"))
    .or_else(|| string_env_nonempty("NOVOVM_NODE_ID"))
    .or_else(|| string_env_nonempty("HOSTNAME"))
    .or_else(|| string_env_nonempty("COMPUTERNAME"))
    .unwrap_or_else(|| "overlay-local".to_string());
    sanitize_gateway_overlay_id(&raw, "overlay-local")
}

fn resolve_gateway_overlay_session_id(params: &serde_json::Value, node_id: &str) -> String {
    let fallback = format!(
        "{}-{:x}-{:x}",
        node_id,
        now_unix_millis(),
        SPOOL_SEQ.fetch_add(1, Ordering::Relaxed)
    );
    let raw = param_as_string_any_with_tx(
        params,
        &[
            "overlay_session_id",
            "overlaySessionId",
            "session_id",
            "sessionId",
        ],
    )
    .or_else(|| string_env_nonempty("NOVOVM_OVERLAY_SESSION_ID"))
    .unwrap_or(fallback.clone());
    sanitize_gateway_overlay_id(&raw, &fallback)
}

fn resolve_gateway_eth_pending_block_for_runtime_view(
    chain_id: u64,
    local_latest_block: u64,
    eth_tx_index: &mut HashMap<[u8; 32], GatewayEthTxIndexEntry>,
    eth_tx_index_store: &GatewayEthTxIndexStoreBackend,
) -> Result<Option<GatewayResolvedBlock>> {
    let sync_status = resolve_gateway_eth_sync_status(chain_id, eth_tx_index, eth_tx_index_store)?;
    let pending_latest = sync_status
        .current_block
        .max(sync_status.local_current_block)
        .max(local_latest_block);
    Ok(gateway_eth_pending_block_from_runtime(
        chain_id,
        pending_latest,
        false,
    ))
}

fn is_gateway_standalone_evm_control_namespace(method: &str) -> bool {
    method.starts_with("engine_")
        || method.starts_with("admin_")
        || method.starts_with("debug_")
        || method.starts_with("miner_")
        || method.starts_with("personal_")
        || method.starts_with("clique_")
        || method.starts_with("parity_")
}

fn gateway_runtime_surface_map_json() -> serde_json::Value {
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
                    "eth_blockNumber",
                    "eth_getBalance",
                    "eth_getBlockByNumber",
                    "eth_getTransactionByHash",
                    "eth_getTransactionReceipt",
                    "eth_sendRawTransaction",
                    "eth_getLogs",
                    "txpool_content"
                ]
            }
        ],
        "notes": [
            "supervm mainnet remains the single host chain",
            "eth_* namespace is compatibility surface provided by evm plugin gateway"
        ]
    })
}

fn gateway_runtime_method_domain(method: &str) -> &'static str {
    if method.starts_with("ua_") || method.starts_with("web30_") || method.starts_with("novovm_") {
        "novovm_mainnet"
    } else if method.starts_with("eth_")
        || method.starts_with("evm_")
        || method.starts_with("txpool_")
        || method.starts_with("net_")
        || method.starts_with("web3_")
        || is_gateway_standalone_evm_control_namespace(method)
    {
        "evm_plugin"
    } else {
        "unknown"
    }
}

fn gateway_runtime_method_domain_json(method: &str) -> serde_json::Value {
    serde_json::json!({
        "host_chain": "supervm_mainnet",
        "method": method,
        "domain": gateway_runtime_method_domain(method),
        "control_namespace_disabled": is_gateway_standalone_evm_control_namespace(method),
    })
}

#[allow(clippy::too_many_arguments)]
fn run_gateway_method(
    router: &mut UnifiedAccountRouter,
    eth_tx_index: &mut HashMap<[u8; 32], GatewayEthTxIndexEntry>,
    evm_settlement_index_by_id: &mut HashMap<String, GatewayEvmSettlementIndexEntry>,
    evm_settlement_index_by_tx: &mut HashMap<GatewaySettlementTxKey, String>,
    evm_pending_payout_by_settlement: &mut HashMap<String, EvmFeePayoutInstructionV1>,
    ctx: &mut GatewayMethodContext<'_>,
    method: &str,
    params: &serde_json::Value,
) -> Result<(serde_json::Value, bool)> {
    if is_gateway_standalone_evm_control_namespace(method) {
        bail!(
            "standalone evm control namespace disabled on supervm host mode: {}",
            method
        );
    }
    match method {
        "novovm_getSurfaceMap" | "novovm_get_surface_map" => {
            Ok((gateway_runtime_surface_map_json(), false))
        }
        "novovm_getMethodDomain" | "novovm_get_method_domain" => {
            let target_method = param_as_string(params, "method")
                .or_else(|| param_as_string(params, "rpc_method"))
                .or_else(|| param_as_string(params, "name"))
                .or_else(|| first_scalar_param_string(params))
                .ok_or_else(|| {
                    anyhow::anyhow!("method is required for novovm_getMethodDomain")
                })?;
            Ok((gateway_runtime_method_domain_json(&target_method), false))
        }
        "evm_sendRawTransaction"
        | "evm_send_raw_transaction"
        | "evm_publicSendRawTransaction"
        | "evm_public_send_raw_transaction" => {
            let mut forwarded = params.clone();
            force_evm_send_public_broadcast_detail(&mut forwarded);
            run_gateway_method(
                router,
                eth_tx_index,
                evm_settlement_index_by_id,
                evm_settlement_index_by_tx,
                evm_pending_payout_by_settlement,
                ctx,
                "eth_sendRawTransaction",
                &forwarded,
            )
        }
        "evm_sendTransaction"
        | "evm_send_transaction"
        | "evm_publicSendTransaction"
        | "evm_public_send_transaction" => {
            let mut forwarded = params.clone();
            force_evm_send_public_broadcast_detail(&mut forwarded);
            run_gateway_method(
                router,
                eth_tx_index,
                evm_settlement_index_by_id,
                evm_settlement_index_by_tx,
                evm_pending_payout_by_settlement,
                ctx,
                "eth_sendTransaction",
                &forwarded,
            )
        }
        "evm_getLogs" | "evm_get_logs" => run_gateway_method(
            router,
            eth_tx_index,
            evm_settlement_index_by_id,
            evm_settlement_index_by_tx,
            evm_pending_payout_by_settlement,
            ctx,
            "eth_getLogs",
            params,
        ),
        "evm_getTransactionReceipt" | "evm_get_transaction_receipt" => run_gateway_method(
            router,
            eth_tx_index,
            evm_settlement_index_by_id,
            evm_settlement_index_by_tx,
            evm_pending_payout_by_settlement,
            ctx,
            "eth_getTransactionReceipt",
            params,
        ),
        "evm_subscribe" => run_gateway_method(
            router,
            eth_tx_index,
            evm_settlement_index_by_id,
            evm_settlement_index_by_tx,
            evm_pending_payout_by_settlement,
            ctx,
            "eth_subscribe",
            params,
        ),
        "evm_unsubscribe" => run_gateway_method(
            router,
            eth_tx_index,
            evm_settlement_index_by_id,
            evm_settlement_index_by_tx,
            evm_pending_payout_by_settlement,
            ctx,
            "eth_unsubscribe",
            params,
        ),
        "evm_newFilter" | "evm_new_filter" => run_gateway_method(
            router,
            eth_tx_index,
            evm_settlement_index_by_id,
            evm_settlement_index_by_tx,
            evm_pending_payout_by_settlement,
            ctx,
            "eth_newFilter",
            params,
        ),
        "evm_newBlockFilter" | "evm_new_block_filter" => run_gateway_method(
            router,
            eth_tx_index,
            evm_settlement_index_by_id,
            evm_settlement_index_by_tx,
            evm_pending_payout_by_settlement,
            ctx,
            "eth_newBlockFilter",
            params,
        ),
        "evm_newPendingTransactionFilter" | "evm_new_pending_transaction_filter" => run_gateway_method(
            router,
            eth_tx_index,
            evm_settlement_index_by_id,
            evm_settlement_index_by_tx,
            evm_pending_payout_by_settlement,
            ctx,
            "eth_newPendingTransactionFilter",
            params,
        ),
        "evm_getFilterChanges" | "evm_get_filter_changes" => run_gateway_method(
            router,
            eth_tx_index,
            evm_settlement_index_by_id,
            evm_settlement_index_by_tx,
            evm_pending_payout_by_settlement,
            ctx,
            "eth_getFilterChanges",
            params,
        ),
        "evm_getFilterLogs" | "evm_get_filter_logs" => run_gateway_method(
            router,
            eth_tx_index,
            evm_settlement_index_by_id,
            evm_settlement_index_by_tx,
            evm_pending_payout_by_settlement,
            ctx,
            "eth_getFilterLogs",
            params,
        ),
        "evm_uninstallFilter" | "evm_uninstall_filter" => run_gateway_method(
            router,
            eth_tx_index,
            evm_settlement_index_by_id,
            evm_settlement_index_by_tx,
            evm_pending_payout_by_settlement,
            ctx,
            "eth_uninstallFilter",
            params,
        ),
        "evm_chainId" | "evm_chain_id" => run_gateway_method(
            router,
            eth_tx_index,
            evm_settlement_index_by_id,
            evm_settlement_index_by_tx,
            evm_pending_payout_by_settlement,
            ctx,
            "eth_chainId",
            params,
        ),
        "evm_clientVersion" | "evm_client_version" => run_gateway_method(
            router,
            eth_tx_index,
            evm_settlement_index_by_id,
            evm_settlement_index_by_tx,
            evm_pending_payout_by_settlement,
            ctx,
            "web3_clientVersion",
            params,
        ),
        "evm_sha3" => run_gateway_method(
            router,
            eth_tx_index,
            evm_settlement_index_by_id,
            evm_settlement_index_by_tx,
            evm_pending_payout_by_settlement,
            ctx,
            "web3_sha3",
            params,
        ),
        "evm_protocolVersion" | "evm_protocol_version" => run_gateway_method(
            router,
            eth_tx_index,
            evm_settlement_index_by_id,
            evm_settlement_index_by_tx,
            evm_pending_payout_by_settlement,
            ctx,
            "eth_protocolVersion",
            params,
        ),
        "evm_listening" => run_gateway_method(
            router,
            eth_tx_index,
            evm_settlement_index_by_id,
            evm_settlement_index_by_tx,
            evm_pending_payout_by_settlement,
            ctx,
            "net_listening",
            params,
        ),
        "evm_peerCount" | "evm_peer_count" => run_gateway_method(
            router,
            eth_tx_index,
            evm_settlement_index_by_id,
            evm_settlement_index_by_tx,
            evm_pending_payout_by_settlement,
            ctx,
            "net_peerCount",
            params,
        ),
        "evm_accounts" => run_gateway_method(
            router,
            eth_tx_index,
            evm_settlement_index_by_id,
            evm_settlement_index_by_tx,
            evm_pending_payout_by_settlement,
            ctx,
            "eth_accounts",
            params,
        ),
        "evm_coinbase" => run_gateway_method(
            router,
            eth_tx_index,
            evm_settlement_index_by_id,
            evm_settlement_index_by_tx,
            evm_pending_payout_by_settlement,
            ctx,
            "eth_coinbase",
            params,
        ),
        "evm_mining" => run_gateway_method(
            router,
            eth_tx_index,
            evm_settlement_index_by_id,
            evm_settlement_index_by_tx,
            evm_pending_payout_by_settlement,
            ctx,
            "eth_mining",
            params,
        ),
        "evm_hashrate" => run_gateway_method(
            router,
            eth_tx_index,
            evm_settlement_index_by_id,
            evm_settlement_index_by_tx,
            evm_pending_payout_by_settlement,
            ctx,
            "eth_hashrate",
            params,
        ),
        "evm_netVersion" | "evm_net_version" => run_gateway_method(
            router,
            eth_tx_index,
            evm_settlement_index_by_id,
            evm_settlement_index_by_tx,
            evm_pending_payout_by_settlement,
            ctx,
            "net_version",
            params,
        ),
        "evm_syncing" => run_gateway_method(
            router,
            eth_tx_index,
            evm_settlement_index_by_id,
            evm_settlement_index_by_tx,
            evm_pending_payout_by_settlement,
            ctx,
            "eth_syncing",
            params,
        ),
        "evm_blockNumber" | "evm_block_number" => run_gateway_method(
            router,
            eth_tx_index,
            evm_settlement_index_by_id,
            evm_settlement_index_by_tx,
            evm_pending_payout_by_settlement,
            ctx,
            "eth_blockNumber",
            params,
        ),
        "evm_getBalance" | "evm_get_balance" => run_gateway_method(
            router,
            eth_tx_index,
            evm_settlement_index_by_id,
            evm_settlement_index_by_tx,
            evm_pending_payout_by_settlement,
            ctx,
            "eth_getBalance",
            params,
        ),
        "evm_getBlockByNumber" | "evm_get_block_by_number" => run_gateway_method(
            router,
            eth_tx_index,
            evm_settlement_index_by_id,
            evm_settlement_index_by_tx,
            evm_pending_payout_by_settlement,
            ctx,
            "eth_getBlockByNumber",
            params,
        ),
        "evm_getBlockByHash" | "evm_get_block_by_hash" => run_gateway_method(
            router,
            eth_tx_index,
            evm_settlement_index_by_id,
            evm_settlement_index_by_tx,
            evm_pending_payout_by_settlement,
            ctx,
            "eth_getBlockByHash",
            params,
        ),
        "evm_getBlockReceipts" | "evm_get_block_receipts" => run_gateway_method(
            router,
            eth_tx_index,
            evm_settlement_index_by_id,
            evm_settlement_index_by_tx,
            evm_pending_payout_by_settlement,
            ctx,
            "eth_getBlockReceipts",
            params,
        ),
        "evm_getTransactionByHash" | "evm_get_transaction_by_hash" => run_gateway_method(
            router,
            eth_tx_index,
            evm_settlement_index_by_id,
            evm_settlement_index_by_tx,
            evm_pending_payout_by_settlement,
            ctx,
            "eth_getTransactionByHash",
            params,
        ),
        "evm_getTransactionCount" | "evm_get_transaction_count" => run_gateway_method(
            router,
            eth_tx_index,
            evm_settlement_index_by_id,
            evm_settlement_index_by_tx,
            evm_pending_payout_by_settlement,
            ctx,
            "eth_getTransactionCount",
            params,
        ),
        "evm_gasPrice" | "evm_gas_price" => run_gateway_method(
            router,
            eth_tx_index,
            evm_settlement_index_by_id,
            evm_settlement_index_by_tx,
            evm_pending_payout_by_settlement,
            ctx,
            "eth_gasPrice",
            params,
        ),
        "evm_call" => run_gateway_method(
            router,
            eth_tx_index,
            evm_settlement_index_by_id,
            evm_settlement_index_by_tx,
            evm_pending_payout_by_settlement,
            ctx,
            "eth_call",
            params,
        ),
        "evm_estimateGas" | "evm_estimate_gas" => run_gateway_method(
            router,
            eth_tx_index,
            evm_settlement_index_by_id,
            evm_settlement_index_by_tx,
            evm_pending_payout_by_settlement,
            ctx,
            "eth_estimateGas",
            params,
        ),
        "evm_getCode" | "evm_get_code" => run_gateway_method(
            router,
            eth_tx_index,
            evm_settlement_index_by_id,
            evm_settlement_index_by_tx,
            evm_pending_payout_by_settlement,
            ctx,
            "eth_getCode",
            params,
        ),
        "evm_getStorageAt" | "evm_get_storage_at" => run_gateway_method(
            router,
            eth_tx_index,
            evm_settlement_index_by_id,
            evm_settlement_index_by_tx,
            evm_pending_payout_by_settlement,
            ctx,
            "eth_getStorageAt",
            params,
        ),
        "evm_getProof" | "evm_get_proof" => run_gateway_method(
            router,
            eth_tx_index,
            evm_settlement_index_by_id,
            evm_settlement_index_by_tx,
            evm_pending_payout_by_settlement,
            ctx,
            "eth_getProof",
            params,
        ),
        "evm_maxPriorityFeePerGas" | "evm_max_priority_fee_per_gas" => run_gateway_method(
            router,
            eth_tx_index,
            evm_settlement_index_by_id,
            evm_settlement_index_by_tx,
            evm_pending_payout_by_settlement,
            ctx,
            "eth_maxPriorityFeePerGas",
            params,
        ),
        "evm_feeHistory" | "evm_fee_history" => run_gateway_method(
            router,
            eth_tx_index,
            evm_settlement_index_by_id,
            evm_settlement_index_by_tx,
            evm_pending_payout_by_settlement,
            ctx,
            "eth_feeHistory",
            params,
        ),
        "evm_getTransactionByBlockNumberAndIndex"
        | "evm_get_transaction_by_block_number_and_index" => run_gateway_method(
            router,
            eth_tx_index,
            evm_settlement_index_by_id,
            evm_settlement_index_by_tx,
            evm_pending_payout_by_settlement,
            ctx,
            "eth_getTransactionByBlockNumberAndIndex",
            params,
        ),
        "evm_getTransactionByBlockHashAndIndex"
        | "evm_get_transaction_by_block_hash_and_index" => run_gateway_method(
            router,
            eth_tx_index,
            evm_settlement_index_by_id,
            evm_settlement_index_by_tx,
            evm_pending_payout_by_settlement,
            ctx,
            "eth_getTransactionByBlockHashAndIndex",
            params,
        ),
        "evm_getBlockTransactionCountByNumber"
        | "evm_get_block_transaction_count_by_number" => run_gateway_method(
            router,
            eth_tx_index,
            evm_settlement_index_by_id,
            evm_settlement_index_by_tx,
            evm_pending_payout_by_settlement,
            ctx,
            "eth_getBlockTransactionCountByNumber",
            params,
        ),
        "evm_getBlockTransactionCountByHash" | "evm_get_block_transaction_count_by_hash" => {
            run_gateway_method(
                router,
                eth_tx_index,
                evm_settlement_index_by_id,
                evm_settlement_index_by_tx,
                evm_pending_payout_by_settlement,
                ctx,
                "eth_getBlockTransactionCountByHash",
                params,
            )
        }
        "evm_getUncleCountByBlockNumber" | "evm_get_uncle_count_by_block_number" => {
            run_gateway_method(
                router,
                eth_tx_index,
                evm_settlement_index_by_id,
                evm_settlement_index_by_tx,
                evm_pending_payout_by_settlement,
                ctx,
                "eth_getUncleCountByBlockNumber",
                params,
            )
        }
        "evm_getUncleCountByBlockHash" | "evm_get_uncle_count_by_block_hash" => run_gateway_method(
            router,
            eth_tx_index,
            evm_settlement_index_by_id,
            evm_settlement_index_by_tx,
            evm_pending_payout_by_settlement,
            ctx,
            "eth_getUncleCountByBlockHash",
            params,
        ),
        "evm_getUncleByBlockNumberAndIndex" | "evm_get_uncle_by_block_number_and_index" => {
            run_gateway_method(
                router,
                eth_tx_index,
                evm_settlement_index_by_id,
                evm_settlement_index_by_tx,
                evm_pending_payout_by_settlement,
                ctx,
                "eth_getUncleByBlockNumberAndIndex",
                params,
            )
        }
        "evm_getUncleByBlockHashAndIndex" | "evm_get_uncle_by_block_hash_and_index" => {
            run_gateway_method(
                router,
                eth_tx_index,
                evm_settlement_index_by_id,
                evm_settlement_index_by_tx,
                evm_pending_payout_by_settlement,
                ctx,
                "eth_getUncleByBlockHashAndIndex",
                params,
            )
        }
        "evm_pendingTransactions" | "evm_pending_transactions" => run_gateway_method(
            router,
            eth_tx_index,
            evm_settlement_index_by_id,
            evm_settlement_index_by_tx,
            evm_pending_payout_by_settlement,
            ctx,
            "eth_pendingTransactions",
            params,
        ),
        "evm_txpoolContent" | "evm_txpool_content" => run_gateway_method(
            router,
            eth_tx_index,
            evm_settlement_index_by_id,
            evm_settlement_index_by_tx,
            evm_pending_payout_by_settlement,
            ctx,
            "txpool_content",
            params,
        ),
        "evm_txpoolContentFrom" | "evm_txpool_contentFrom" | "evm_txpool_content_from" => run_gateway_method(
            router,
            eth_tx_index,
            evm_settlement_index_by_id,
            evm_settlement_index_by_tx,
            evm_pending_payout_by_settlement,
            ctx,
            "txpool_contentFrom",
            params,
        ),
        "evm_txpoolInspect" | "evm_txpool_inspect" => run_gateway_method(
            router,
            eth_tx_index,
            evm_settlement_index_by_id,
            evm_settlement_index_by_tx,
            evm_pending_payout_by_settlement,
            ctx,
            "txpool_inspect",
            params,
        ),
        "evm_txpoolInspectFrom" | "evm_txpool_inspectFrom" | "evm_txpool_inspect_from" => run_gateway_method(
            router,
            eth_tx_index,
            evm_settlement_index_by_id,
            evm_settlement_index_by_tx,
            evm_pending_payout_by_settlement,
            ctx,
            "txpool_inspectFrom",
            params,
        ),
        "evm_txpoolStatus" | "evm_txpool_status" => run_gateway_method(
            router,
            eth_tx_index,
            evm_settlement_index_by_id,
            evm_settlement_index_by_tx,
            evm_pending_payout_by_settlement,
            ctx,
            "txpool_status",
            params,
        ),
        "evm_txpoolStatusFrom" | "evm_txpool_statusFrom" | "evm_txpool_status_from" => run_gateway_method(
            router,
            eth_tx_index,
            evm_settlement_index_by_id,
            evm_settlement_index_by_tx,
            evm_pending_payout_by_settlement,
            ctx,
            "txpool_statusFrom",
            params,
        ),
        "evm_snapshotPendingIngress" | "evm_snapshot_pending_ingress" => {
            let max_items = param_as_u64(params, "max_items")
                .or_else(|| param_as_u64(params, "max"))
                .unwrap_or(512)
                .clamp(1, 8_192) as usize;
            let chain_filter = param_as_u64_any_with_tx(params, &["chain_id", "chainId"]);
            let include_raw =
                param_as_bool_any_with_tx(params, &["include_raw", "includeRaw"]).unwrap_or(true);
            let include_parsed =
                param_as_bool_any_with_tx(params, &["include_parsed", "includeParsed"])
                    .unwrap_or(true);
            let mut frames = snapshot_pending_ingress_frames_for_host(max_items);
            if let Some(chain_id) = chain_filter {
                frames.retain(|frame| frame.chain_id == chain_id);
            }
            let items: Vec<serde_json::Value> = frames
                .iter()
                .map(|frame| gateway_evm_ingress_frame_json(frame, include_raw, include_parsed))
                .collect();
            Ok((
                serde_json::json!({
                    "count": items.len(),
                    "items": items,
                }),
                false,
            ))
        }
        "evm_snapshotExecutableIngress" | "evm_snapshot_executable_ingress" => {
            let max_items = param_as_u64(params, "max_items")
                .or_else(|| param_as_u64(params, "max"))
                .unwrap_or(512)
                .clamp(1, 8_192) as usize;
            let chain_filter =
                param_as_u64_any_with_tx(params, &["chain_id", "chainId"]);
            let include_raw =
                param_as_bool_any_with_tx(params, &["include_raw", "includeRaw"]).unwrap_or(true);
            let include_parsed =
                param_as_bool_any_with_tx(params, &["include_parsed", "includeParsed"])
                    .unwrap_or(true);
            let mut frames = snapshot_executable_ingress_frames_for_host(max_items);
            if let Some(chain_id) = chain_filter {
                frames.retain(|frame| frame.chain_id == chain_id);
            }
            let items: Vec<serde_json::Value> = frames
                .iter()
                .map(|frame| gateway_evm_ingress_frame_json(frame, include_raw, include_parsed))
                .collect();
            Ok((
                serde_json::json!({
                    "count": items.len(),
                    "items": items,
                }),
                false,
            ))
        }
        "evm_drainExecutableIngress" | "evm_drain_executable_ingress" => {
            let max_items = param_as_u64(params, "max_items")
                .or_else(|| param_as_u64(params, "max"))
                .unwrap_or(512)
                .clamp(1, 8_192) as usize;
            let chain_filter =
                param_as_u64_any_with_tx(params, &["chain_id", "chainId"]);
            let include_raw =
                param_as_bool_any_with_tx(params, &["include_raw", "includeRaw"]).unwrap_or(true);
            let include_parsed =
                param_as_bool_any_with_tx(params, &["include_parsed", "includeParsed"])
                    .unwrap_or(true);
            let mut frames = drain_executable_ingress_frames_for_host(max_items);
            if let Some(chain_id) = chain_filter {
                frames.retain(|frame| frame.chain_id == chain_id);
            }
            let items: Vec<serde_json::Value> = frames
                .iter()
                .map(|frame| gateway_evm_ingress_frame_json(frame, include_raw, include_parsed))
                .collect();
            Ok((
                serde_json::json!({
                    "count": items.len(),
                    "items": items,
                }),
                false,
            ))
        }
        "evm_executeExecutableIngressSample" | "evm_execute_executable_ingress_sample" => {
            let chain_id = param_as_u64_any_with_tx(params, &["chain_id", "chainId"])
                .unwrap_or(ctx.eth_default_chain_id);
            let max_items = param_as_u64(params, "max_items")
                .or_else(|| param_as_u64(params, "max"))
                .unwrap_or(16)
                .clamp(1, 2_048) as usize;
            let use_drain =
                param_as_bool_any_with_tx(params, &["drain", "use_drain", "useDrain"])
                    .unwrap_or(true);
            let fallback_pending = param_as_bool_any_with_tx(
                params,
                &["fallback_pending", "fallbackPending", "fallback"],
            )
            .unwrap_or(true);
            let single_tx_fallback = param_as_bool_any_with_tx(
                params,
                &["single_tx_fallback", "singleTxFallback", "single_fallback"],
            )
            .unwrap_or(true);
            let mut source = if use_drain {
                "drain_executable_ingress"
            } else {
                "snapshot_executable_ingress"
            };
            let mut frames = if use_drain {
                drain_executable_ingress_frames_for_host(max_items)
            } else {
                snapshot_executable_ingress_frames_for_host(max_items)
            };
            frames.retain(|frame| frame.chain_id == chain_id);
            let mut sampled_count = frames.len();
            let mut used_pending_fallback = false;
            let (txs, mut sampled_hashes, dropped_unparsed) =
                match execute_gateway_evm_executable_ingress_frames(chain_id, &frames) {
                    Ok(result) => result,
                    Err(exec_err) => {
                        if !fallback_pending {
                            return Err(exec_err);
                        }
                        let mut pending_frames = if use_drain {
                            drain_pending_ingress_frames_for_host(max_items)
                        } else {
                            snapshot_pending_ingress_frames_for_host(max_items)
                        };
                        pending_frames.retain(|frame| frame.chain_id == chain_id);
                        sampled_count = pending_frames.len();
                        match execute_gateway_evm_executable_ingress_frames(
                            chain_id,
                            &pending_frames,
                        ) {
                            Ok(result) => {
                                used_pending_fallback = true;
                                source = if use_drain {
                                    "drain_pending_ingress_fallback"
                                } else {
                                    "snapshot_pending_ingress_fallback"
                                };
                                result
                            }
                            Err(pending_err) => {
                                return Err(anyhow::anyhow!(
                                    "executable ingress sample failed ({}) and pending fallback failed ({})",
                                    exec_err,
                                    pending_err
                                ));
                            }
                        }
                    }
                };
            let chain_type = evm_chain_type_for_gateway(chain_id);
            let candidate_txs = txs.len();
            let mut batch_error_code = None::<i32>;
            let mut result = match apply_ir_batch_v1(chain_type, chain_id, &txs) {
                Ok(v) => v,
                Err(rc) => {
                    batch_error_code = Some(rc);
                    NovovmAdapterPluginApplyResultV1 {
                        verified: 0,
                        applied: 0,
                        txs: txs.len() as u64,
                        accounts: 0,
                        state_root: [0u8; 32],
                        error_code: rc,
                    }
                }
            };
            let mut selection_mode = "batch";
            let mut single_attempts = 0usize;
            let mut single_error_codes: Vec<i32> = Vec::new();
            let mut selected_single_index: Option<usize> = None;
            if single_tx_fallback
                && txs.len() > 1
                && !(result.verified == 1 && result.applied == 1)
            {
                for (idx, tx) in txs.iter().enumerate() {
                    single_attempts = single_attempts.saturating_add(1);
                    match apply_ir_batch_v1(chain_type, chain_id, std::slice::from_ref(tx)) {
                        Ok(single_result) => {
                            if single_result.verified == 1 && single_result.applied == 1 {
                                result = single_result;
                                sampled_hashes = vec![sampled_hashes[idx].clone()];
                                sampled_count = 1;
                                selection_mode = "single_tx_fallback";
                                selected_single_index = Some(idx);
                                break;
                            }
                        }
                        Err(rc) => single_error_codes.push(rc),
                    }
                }
            }
            let local_exec_head_before = get_network_runtime_sync_status(chain_id)
                .map(|status| status.current_block)
                .unwrap_or(0);
            let mut local_exec_head_after = local_exec_head_before;
            let mut local_exec_block_hash: Option<String> = None;
            let mut local_exec_indexed_txs = 0usize;
            if result.verified == 1 && result.applied == 1 {
                local_exec_head_after = local_exec_head_before.saturating_add(1);
                let _ = observe_network_runtime_local_head_max(chain_id, local_exec_head_after);
                let selected_txs: Vec<TxIR> = if let Some(idx) = selected_single_index {
                    txs.get(idx).cloned().into_iter().collect()
                } else {
                    txs.clone()
                };
                let mut sealed_entries = Vec::<GatewayEthTxIndexEntry>::new();
                for tx in selected_txs {
                    let normalized = gateway_eth_tx_ir_with_hash(tx);
                    if normalized.hash.len() != 32 {
                        continue;
                    }
                    let mut tx_hash = [0u8; 32];
                    tx_hash.copy_from_slice(&normalized.hash);
                    let tx_type = gateway_eth_tx_type_number_from_ir(normalized.tx_type);
                    let tx_type4 = resolve_raw_evm_tx_route_hint_m0(&normalized.signature)
                        .map(|hint| hint.tx_type4)
                        .unwrap_or(false);
                    let record = GatewayIngressEthRecordV1 {
                        version: GATEWAY_INGRESS_RECORD_VERSION,
                        protocol: GATEWAY_INGRESS_PROTOCOL_ETH,
                        uca_id: "runtime:execute_ingress".to_string(),
                        chain_id,
                        nonce: local_exec_head_after,
                        tx_type,
                        tx_type4,
                        from: normalized.from.clone(),
                        to: normalized.to.clone(),
                        value: normalized.value,
                        gas_limit: normalized.gas_limit,
                        gas_price: normalized.gas_price,
                        data: normalized.data.clone(),
                        signature: normalized.signature.clone(),
                        tx_hash,
                        signature_domain: format!("evm:{chain_id}:runtime_execute"),
                        overlay_node_id: ctx.overlay_node_id.clone(),
                        overlay_session_id: ctx.overlay_session_id.clone(),
                    };
                    upsert_gateway_eth_tx_index(eth_tx_index, ctx.eth_tx_index_store, &record);
                    persist_gateway_eth_submit_onchain_status(
                        ctx.eth_tx_index_store,
                        tx_hash,
                        chain_id,
                        false,
                    );
                    sealed_entries.push(GatewayEthTxIndexEntry {
                        tx_hash,
                        uca_id: record.uca_id,
                        chain_id,
                        nonce: local_exec_head_after,
                        tx_type: record.tx_type,
                        from: record.from,
                        to: record.to,
                        value: record.value,
                        gas_limit: record.gas_limit,
                        gas_price: record.gas_price,
                        input: record.data,
                    });
                }
                if !sealed_entries.is_empty() {
                    sort_gateway_eth_block_txs(&mut sealed_entries);
                    let block_hash =
                        gateway_eth_block_hash_for_txs(chain_id, local_exec_head_after, &sealed_entries);
                    local_exec_block_hash = Some(format!("0x{}", to_hex(&block_hash)));
                    local_exec_indexed_txs = sealed_entries.len();
                }
            }
            Ok((
                serde_json::json!({
                    "chain_id": format!("0x{:x}", chain_id),
                    "source": source,
                    "sampled_frames": sampled_count,
                    "fallback_pending_enabled": fallback_pending,
                    "used_pending_fallback": used_pending_fallback,
                    "single_tx_fallback_enabled": single_tx_fallback,
                    "selection_mode": selection_mode,
                    "candidate_txs": candidate_txs,
                    "batch_error_code": batch_error_code,
                    "single_attempts": single_attempts,
                    "single_error_codes": single_error_codes,
                    "sampled_tx_hashes": sampled_hashes,
                    "dropped_unparsed": dropped_unparsed,
                    "local_exec_head_before": format!("0x{:x}", local_exec_head_before),
                    "local_exec_head_after": format!("0x{:x}", local_exec_head_after),
                    "local_exec_sealed": result.verified == 1 && result.applied == 1 && local_exec_head_after > local_exec_head_before,
                    "local_exec_block_number": format!("0x{:x}", local_exec_head_after),
                    "local_exec_block_hash": local_exec_block_hash,
                    "local_exec_indexed_txs": local_exec_indexed_txs,
                    "apply": {
                        "verified": result.verified == 1,
                        "applied": result.applied == 1,
                        "txs": result.txs,
                        "accounts": result.accounts,
                        "state_root": format!("0x{}", to_hex(&result.state_root)),
                        "error_code": result.error_code,
                    }
                }),
                false,
            ))
        }
        "evm_drainPendingIngress" | "evm_drain_pending_ingress" => {
            let max_items = param_as_u64(params, "max_items")
                .or_else(|| param_as_u64(params, "max"))
                .unwrap_or(512)
                .clamp(1, 8_192) as usize;
            let chain_filter =
                param_as_u64_any_with_tx(params, &["chain_id", "chainId"]);
            let include_raw =
                param_as_bool_any_with_tx(params, &["include_raw", "includeRaw"]).unwrap_or(true);
            let include_parsed =
                param_as_bool_any_with_tx(params, &["include_parsed", "includeParsed"])
                    .unwrap_or(true);
            let mut frames = drain_pending_ingress_frames_for_host(max_items);
            if let Some(chain_id) = chain_filter {
                frames.retain(|frame| frame.chain_id == chain_id);
            }
            let items: Vec<serde_json::Value> = frames
                .iter()
                .map(|frame| gateway_evm_ingress_frame_json(frame, include_raw, include_parsed))
                .collect();
            Ok((
                serde_json::json!({
                    "count": items.len(),
                    "items": items,
                }),
                false,
            ))
        }
        "evm_snapshotPendingSenderBuckets" | "evm_snapshot_pending_sender_buckets" => {
            let max_senders = param_as_u64(params, "max_senders")
                .or_else(|| param_as_u64(params, "maxSenders"))
                .unwrap_or(256)
                .clamp(1, 4_096) as usize;
            let max_txs_per_sender = param_as_u64(params, "max_txs_per_sender")
                .or_else(|| param_as_u64(params, "maxTxsPerSender"))
                .unwrap_or(64)
                .clamp(1, 1_024) as usize;
            let chain_filter =
                param_as_u64_any_with_tx(params, &["chain_id", "chainId"]);
            let mut buckets =
                snapshot_pending_sender_buckets_for_host(max_senders, max_txs_per_sender);
            if let Some(chain_id) = chain_filter {
                buckets.retain(|bucket| bucket.chain_id == chain_id);
            }
            let items: Vec<serde_json::Value> = buckets
                .iter()
                .map(gateway_evm_pending_sender_bucket_json)
                .collect();
            Ok((
                serde_json::json!({
                    "count": items.len(),
                    "items": items,
                }),
                false,
            ))
        }
        "eth_chainId" => {
            let chain_id =
                resolve_chain_id_with_tx_consistency(params, ctx.eth_default_chain_id)?;
            Ok((serde_json::Value::String(format!("0x{:x}", chain_id)), false))
        }
        "net_version" => {
            let chain_id = param_as_u64_any_with_tx(params, &["chain_id", "chainId"])
                .unwrap_or(ctx.eth_default_chain_id);
            Ok((serde_json::Value::String(chain_id.to_string()), false))
        }
        "web3_clientVersion" => Ok((
            serde_json::Value::String(gateway_web3_client_version_from_env()),
            false,
        )),
        "web3_sha3" => {
            let raw_data = extract_web3_sha3_input_hex(params)
                .ok_or_else(|| anyhow::anyhow!("data is required for web3_sha3"))?;
            let data = decode_hex_bytes(&raw_data, "data")?;
            let mut hasher = Keccak256::new();
            hasher.update(&data);
            let digest = hasher.finalize();
            Ok((
                serde_json::Value::String(format!("0x{}", to_hex(&digest))),
                false,
            ))
        }
        "eth_protocolVersion" => Ok((
            serde_json::Value::String(gateway_eth_protocol_version_from_env()),
            false,
        )),
        "net_listening" => Ok((serde_json::Value::Bool(true), false)),
        "net_peerCount" => {
            let chain_id = param_as_u64_any_with_tx(params, &["chain_id", "chainId"])
                .unwrap_or(ctx.eth_default_chain_id);
            let sync_status =
                resolve_gateway_eth_sync_status(chain_id, eth_tx_index, ctx.eth_tx_index_store)?;
            Ok((
                serde_json::Value::String(format!("0x{:x}", sync_status.peer_count)),
                false,
            ))
        }
        "evm_getRuntimeSyncStatus" | "evm_get_runtime_sync_status" => {
            let chain_id = param_as_u64_any_with_tx(params, &["chain_id", "chainId"])
                .unwrap_or(ctx.eth_default_chain_id);
            let Some(status) = get_network_runtime_sync_status(chain_id) else {
                return Ok((serde_json::Value::Null, false));
            };
            Ok((
                serde_json::json!({
                    "chain_id": format!("0x{:x}", chain_id),
                    "peer_count": status.peer_count,
                    "starting_block": status.starting_block,
                    "current_block": status.current_block,
                    "highest_block": status.highest_block,
                    "peerCount": format!("0x{:x}", status.peer_count),
                    "startingBlock": format!("0x{:x}", status.starting_block),
                    "currentBlock": format!("0x{:x}", status.current_block),
                    "highestBlock": format!("0x{:x}", status.highest_block),
                }),
                false,
            ))
        }
        "evm_getRuntimeNativeSyncStatus" | "evm_get_runtime_native_sync_status" => {
            let chain_id = param_as_u64_any_with_tx(params, &["chain_id", "chainId"])
                .unwrap_or(ctx.eth_default_chain_id);
            let Some(status) = get_network_runtime_native_sync_status(chain_id) else {
                return Ok((serde_json::Value::Null, false));
            };
            let active = network_runtime_native_sync_is_active(&status);
            Ok((
                serde_json::json!({
                    "chain_id": format!("0x{:x}", chain_id),
                    "phase": status.phase.as_str(),
                    "active": active,
                    "peer_count": status.peer_count,
                    "starting_block": status.starting_block,
                    "current_block": status.current_block,
                    "highest_block": status.highest_block,
                    "updated_at_unix_millis": status.updated_at_unix_millis,
                    "peerCount": format!("0x{:x}", status.peer_count),
                    "startingBlock": format!("0x{:x}", status.starting_block),
                    "currentBlock": format!("0x{:x}", status.current_block),
                    "highestBlock": format!("0x{:x}", status.highest_block),
                }),
                false,
            ))
        }
        "evm_getRuntimeSyncPullWindow" | "evm_get_runtime_sync_pull_window" => {
            let chain_id = param_as_u64_any_with_tx(params, &["chain_id", "chainId"])
                .unwrap_or(ctx.eth_default_chain_id);
            let Some(window) = plan_network_runtime_sync_pull_window(chain_id) else {
                return Ok((serde_json::Value::Null, false));
            };
            Ok((
                serde_json::json!({
                    "chain_id": format!("0x{:x}", window.chain_id),
                    "phase": window.phase.as_str(),
                    "peer_count": window.peer_count,
                    "current_block": window.current_block,
                    "highest_block": window.highest_block,
                    "from_block": window.from_block,
                    "to_block": window.to_block,
                    "peerCount": format!("0x{:x}", window.peer_count),
                    "currentBlock": format!("0x{:x}", window.current_block),
                    "highestBlock": format!("0x{:x}", window.highest_block),
                    "fromBlock": format!("0x{:x}", window.from_block),
                    "toBlock": format!("0x{:x}", window.to_block),
                }),
                false,
            ))
        }
        "eth_accounts" => Ok((
            serde_json::Value::Array(
                gateway_eth_accounts_from_env()?
                    .into_iter()
                    .map(|addr| serde_json::Value::String(format!("0x{}", to_hex(&addr))))
                    .collect(),
            ),
            false,
        )),
        "eth_coinbase" => Ok((
            serde_json::Value::String(format!(
                "0x{}",
                to_hex(&gateway_eth_coinbase_from_env()?)
            )),
            false,
        )),
        "eth_mining" => Ok((serde_json::Value::Bool(false), false)),
        "eth_hashrate" => Ok((serde_json::Value::String("0x0".to_string()), false)),
        "eth_maxPriorityFeePerGas" => {
            let chain_id = param_as_u64_any_with_tx(params, &["chain_id", "chainId"])
                .unwrap_or(ctx.eth_default_chain_id);
            if let Some(upstream) =
                maybe_gateway_eth_upstream_read(chain_id, "eth_maxPriorityFeePerGas", params)?
            {
                return Ok((upstream, false));
            }
            Ok((
                serde_json::Value::String(format!(
                    "0x{:x}",
                    gateway_eth_default_max_priority_fee_per_gas_wei(chain_id)
                )),
                false,
            ))
        }
        "eth_feeHistory" => {
            let chain_id = param_as_u64_any_with_tx(params, &["chain_id", "chainId"])
                .unwrap_or(ctx.eth_default_chain_id);
            if let Some(upstream) =
                maybe_gateway_eth_upstream_read(chain_id, "eth_feeHistory", params)?
            {
                return Ok((upstream, false));
            }
            let base_fee_per_gas_wei = gateway_eth_base_fee_per_gas_wei(chain_id);
            let default_priority_fee_wei =
                gateway_eth_default_max_priority_fee_per_gas_wei(chain_id);
            let entries = collect_gateway_eth_chain_entries(
                eth_tx_index,
                ctx.eth_tx_index_store,
                chain_id,
                gateway_eth_query_scan_max(),
            )?;
            let latest =
                resolve_gateway_eth_latest_block_number(chain_id, &entries, ctx.eth_tx_index_store)?;
            let block_count = parse_eth_fee_history_block_count(params)
                .ok_or_else(|| anyhow::anyhow!("block_count (or blockCount) is required"))?
                .clamp(1, 1024);
            let blocks = gateway_eth_group_entries_by_block(entries);
            let pending_block = resolve_gateway_eth_pending_block_for_runtime_view(
                chain_id,
                latest,
                eth_tx_index,
                ctx.eth_tx_index_store,
            )?
            .map(|(number, _hash, txs)| {
                let mut sorted = txs;
                sort_gateway_eth_block_txs(&mut sorted);
                (number, sorted)
            });
            let newest_tag = parse_eth_fee_history_newest_block_tag(params)
                .unwrap_or_else(|| "latest".to_string());
            let Some(newest_block) = parse_eth_fee_history_newest_block_number(
                &newest_tag,
                latest,
                pending_block.as_ref().map(|(number, _)| *number),
            )? else {
                return Ok((serde_json::Value::Null, false));
            };
            let oldest_block = newest_block.saturating_sub(block_count.saturating_sub(1));
            let mut base_fee_per_gas: Vec<String> = Vec::with_capacity((block_count + 1) as usize);
            let mut gas_used_ratio: Vec<serde_json::Value> = Vec::with_capacity(block_count as usize);
            let reward_percentiles = parse_eth_fee_history_reward_percentiles(params)?;
            let mut reward_rows: Vec<serde_json::Value> = Vec::with_capacity(block_count as usize);
            for block_number in oldest_block..=newest_block {
                let txs: std::borrow::Cow<'_, [GatewayEthTxIndexEntry]> = if pending_block
                    .as_ref()
                    .is_some_and(|(pending_number, _)| *pending_number == block_number)
                {
                    std::borrow::Cow::Borrowed(
                        pending_block
                            .as_ref()
                            .map(|(_, txs)| txs.as_slice())
                            .unwrap_or(&[]),
                    )
                } else if let Some(block_txs) = blocks.get(&block_number) {
                    std::borrow::Cow::Borrowed(block_txs.as_slice())
                } else if block_number <= latest {
                    std::borrow::Cow::Owned(collect_gateway_eth_block_entries_precise(
                        eth_tx_index,
                        ctx.eth_tx_index_store,
                        chain_id,
                        block_number,
                        gateway_eth_query_scan_max(),
                    )?)
                } else {
                    std::borrow::Cow::Borrowed(&[])
                };
                let txs = txs.as_ref();
                let gas_used: u128 = txs.iter().map(|tx| tx.gas_limit as u128).sum();
                let gas_used_ratio_value = if gateway_eth_fee_history_block_gas_limit() == 0 {
                    0.0
                } else {
                    (gas_used as f64 / gateway_eth_fee_history_block_gas_limit() as f64).min(1.0)
                };
                gas_used_ratio.push(serde_json::json!(gas_used_ratio_value));
                base_fee_per_gas.push(format!("0x{:x}", base_fee_per_gas_wei));
                if let Some(percentiles) = reward_percentiles.as_ref() {
                    reward_rows.push(serde_json::Value::Array(
                        gateway_eth_fee_history_reward_row_hex(
                            txs,
                            percentiles,
                            default_priority_fee_wei as u128,
                        )
                        .into_iter()
                        .map(serde_json::Value::String)
                        .collect(),
                    ));
                }
            }
            base_fee_per_gas.push(format!("0x{:x}", base_fee_per_gas_wei));
            let mut obj = serde_json::Map::new();
            obj.insert(
                "oldestBlock".to_string(),
                serde_json::Value::String(format!("0x{:x}", oldest_block)),
            );
            obj.insert(
                "baseFeePerGas".to_string(),
                serde_json::Value::Array(
                    base_fee_per_gas
                        .into_iter()
                        .map(serde_json::Value::String)
                        .collect(),
                ),
            );
            obj.insert("gasUsedRatio".to_string(), serde_json::Value::Array(gas_used_ratio));
            if reward_percentiles.is_some() {
                obj.insert("reward".to_string(), serde_json::Value::Array(reward_rows));
            }
            Ok((serde_json::Value::Object(obj), false))
        }
        "eth_syncing" => {
            let chain_id = param_as_u64_any_with_tx(params, &["chain_id", "chainId"])
                .unwrap_or(ctx.eth_default_chain_id);
            if let Some(upstream) =
                maybe_gateway_eth_upstream_read(chain_id, "eth_syncing", params)?
            {
                return Ok((upstream, false));
            }
            let sync_status =
                resolve_gateway_eth_sync_status(chain_id, eth_tx_index, ctx.eth_tx_index_store)?;
            Ok((gateway_eth_syncing_json(sync_status, None), false))
        }
        "eth_pendingTransactions" => {
            let chain_id = param_as_u64_any_with_tx(params, &["chain_id", "chainId"])
                .unwrap_or(ctx.eth_default_chain_id);
            poll_gateway_eth_public_broadcast_native_runtime(chain_id, 64);
            let (pending_txs, queued_txs) = collect_gateway_eth_txpool_txs_with_index_fallback(
                chain_id,
                eth_tx_index,
                ctx.eth_tx_index_store,
            )?;
            if !pending_txs.is_empty() || !queued_txs.is_empty() {
                let mut pending =
                    Vec::<serde_json::Value>::with_capacity(pending_txs.len() + queued_txs.len());
                pending.extend(
                    pending_txs
                        .iter()
                        .map(gateway_eth_pending_tx_by_hash_json_from_ir),
                );
                pending.extend(
                    queued_txs
                        .iter()
                        .map(gateway_eth_pending_tx_by_hash_json_from_ir),
                );
                return Ok((serde_json::Value::Array(pending), false));
            }
            Ok((serde_json::Value::Array(Vec::new()), false))
        }
        "eth_blockNumber" => {
            let chain_id = param_as_u64_any_with_tx(params, &["chain_id", "chainId"])
                .unwrap_or(ctx.eth_default_chain_id);
            if let Some(upstream) =
                maybe_gateway_eth_upstream_read(chain_id, "eth_blockNumber", params)?
            {
                return Ok((upstream, false));
            }
            let sync_status =
                resolve_gateway_eth_sync_status(chain_id, eth_tx_index, ctx.eth_tx_index_store)?;
            Ok((
                serde_json::Value::String(format!(
                    "0x{:x}",
                    sync_status.current_block.max(sync_status.local_current_block)
                )),
                false,
            ))
        }
        "eth_getBalance" => {
            let chain_id = param_as_u64_any_with_tx(params, &["chain_id", "chainId"])
                .unwrap_or(ctx.eth_default_chain_id);
            if let Some(upstream) =
                maybe_gateway_eth_upstream_read(chain_id, "eth_getBalance", params)?
            {
                return Ok((upstream, false));
            }
            let address_raw = extract_eth_persona_address_param(params)
                .ok_or_else(|| anyhow::anyhow!("address (or from/external_address) is required"))?;
            let address = decode_hex_bytes(&address_raw, "address")?;
            let entries = collect_gateway_eth_chain_entries(
                eth_tx_index,
                ctx.eth_tx_index_store,
                chain_id,
                gateway_eth_query_scan_max(),
            )?;
            let latest =
                resolve_gateway_eth_latest_block_number(chain_id, &entries, ctx.eth_tx_index_store)?;
            let block_tag =
                parse_eth_tx_count_block_tag(params).unwrap_or_else(|| "latest".to_string());
            let Some(view_entries) =
                resolve_gateway_eth_get_proof_entries(chain_id, entries, &block_tag, latest)?
            else {
                return Ok((serde_json::Value::Null, false));
            };
            let balance = gateway_eth_balance_from_entries(&view_entries, &address);
            Ok((serde_json::Value::String(format!("0x{:x}", balance)), false))
        }
        "eth_getBlockByNumber" => {
            let chain_id = param_as_u64_any_with_tx(params, &["chain_id", "chainId"])
                .unwrap_or(ctx.eth_default_chain_id);
            if let Some(upstream) =
                maybe_gateway_eth_upstream_read(chain_id, "eth_getBlockByNumber", params)?
            {
                return Ok((upstream, false));
            }
            let full_transactions = parse_eth_block_query_full_transactions(params);
            let block_tag = parse_eth_block_query_tag(params).unwrap_or_else(|| "latest".to_string());
            let normalized_tag = block_tag.trim().trim_matches('"');
            let entries = collect_gateway_eth_chain_entries(
                eth_tx_index,
                ctx.eth_tx_index_store,
                chain_id,
                gateway_eth_query_scan_max(),
            )?;
            let latest =
                resolve_gateway_eth_latest_block_number(chain_id, &entries, ctx.eth_tx_index_store)?;
            let resolve_state_root_for_block = |target_block: u64| -> Result<Option<[u8; 32]>> {
                let block_tag_for_root = format!("0x{:x}", target_block);
                Ok(resolve_gateway_eth_get_proof_entries(
                    chain_id,
                    entries.clone(),
                    &block_tag_for_root,
                    latest,
                )?
                .map(|state_entries| gateway_eth_state_root_from_entries(&state_entries)))
            };
            if normalized_tag.eq_ignore_ascii_case("pending") {
                let Some((pending_block_number, _pending_block_hash, pending_entries)) =
                    resolve_gateway_eth_pending_block_for_runtime_view(
                        chain_id,
                        latest,
                        eth_tx_index,
                        ctx.eth_tx_index_store,
                    )?
                else {
                    return Ok((serde_json::Value::Null, false));
                };
                let mut pending_state_entries = entries.clone();
                pending_state_entries.extend(pending_entries.clone());
                return Ok((
                    gateway_eth_block_by_number_json(
                        chain_id,
                        pending_block_number,
                        &pending_entries,
                        full_transactions,
                        true,
                        Some(gateway_eth_state_root_from_entries(&pending_state_entries)),
                    ),
                    false,
                ));
            }
            let Some(block_number) = parse_eth_block_number_from_tag(&block_tag, latest)? else {
                return Ok((serde_json::Value::Null, false));
            };
            let mut block_txs: Vec<GatewayEthTxIndexEntry> = entries
                .iter()
                .filter(|entry| entry.nonce == block_number)
                .cloned()
                .collect();
            if block_txs.is_empty() {
                block_txs = collect_gateway_eth_block_entries_precise(
                    eth_tx_index,
                    ctx.eth_tx_index_store,
                    chain_id,
                    block_number,
                    gateway_eth_query_scan_max(),
                )?;
            }
            if block_txs.is_empty() {
                if block_number <= latest {
                    return Ok((
                        gateway_eth_block_by_number_json(
                            chain_id,
                            block_number,
                            &[],
                            full_transactions,
                            false,
                            resolve_state_root_for_block(block_number)?,
                        ),
                        false,
                    ));
                }
                return Ok((serde_json::Value::Null, false));
            }
            Ok((
                gateway_eth_block_by_number_json(
                    chain_id,
                    block_number,
                    &block_txs,
                    full_transactions,
                    false,
                    resolve_state_root_for_block(block_number)?,
                ),
                false,
            ))
        }
        "eth_getBlockByHash" => {
            let chain_id = param_as_u64_any_with_tx(params, &["chain_id", "chainId"])
                .unwrap_or(ctx.eth_default_chain_id);
            if let Some(upstream) =
                maybe_gateway_eth_upstream_read(chain_id, "eth_getBlockByHash", params)?
            {
                return Ok((upstream, false));
            }
            let full_transactions = parse_eth_block_query_full_transactions(params);
            let block_hash_raw = extract_eth_block_hash_param(params)
                .ok_or_else(|| anyhow::anyhow!("block_hash (or blockHash/hash) is required"))?;
            let block_hash = parse_hex32_from_string(&block_hash_raw, "block_hash")?;
            let entries = collect_gateway_eth_chain_entries(
                eth_tx_index,
                ctx.eth_tx_index_store,
                chain_id,
                gateway_eth_query_scan_max(),
            )?;
            let latest =
                resolve_gateway_eth_latest_block_number(chain_id, &entries, ctx.eth_tx_index_store)?;
            let resolve_state_root_for_block = |target_block: u64| -> Result<Option<[u8; 32]>> {
                let block_tag_for_root = format!("0x{:x}", target_block);
                Ok(resolve_gateway_eth_get_proof_entries(
                    chain_id,
                    entries.clone(),
                    &block_tag_for_root,
                    latest,
                )?
                .map(|state_entries| gateway_eth_state_root_from_entries(&state_entries)))
            };
            if !entries.is_empty() {
                let blocks = gateway_eth_group_entries_by_block(entries.clone());
                for (block_number, block_txs) in blocks {
                    let candidate = gateway_eth_block_hash_for_txs(chain_id, block_number, &block_txs);
                    if candidate != block_hash {
                        continue;
                    }
                    return Ok((
                        gateway_eth_block_by_number_json(
                            chain_id,
                            block_number,
                            &block_txs,
                            full_transactions,
                            false,
                            resolve_state_root_for_block(block_number)?,
                        ),
                        false,
                    ));
                }
            }
            if let Some((pending_block_number, pending_hash, pending_entries)) =
                resolve_gateway_eth_pending_block_for_runtime_view(
                    chain_id,
                    latest,
                    eth_tx_index,
                    ctx.eth_tx_index_store,
                )?
            {
                if pending_hash == block_hash {
                    let mut pending_state_entries = entries.clone();
                    pending_state_entries.extend(pending_entries.clone());
                    return Ok((
                        gateway_eth_block_by_number_json(
                            chain_id,
                            pending_block_number,
                            &pending_entries,
                            full_transactions,
                            true,
                            Some(gateway_eth_state_root_from_entries(&pending_state_entries)),
                        ),
                        false,
                    ));
                }
            }
            if let Some((block_number, block_txs)) = collect_gateway_eth_block_entries_by_hash_precise(
                eth_tx_index,
                ctx.eth_tx_index_store,
                chain_id,
                &block_hash,
                gateway_eth_query_scan_max(),
            )? {
                return Ok((
                    gateway_eth_block_by_number_json(
                        chain_id,
                        block_number,
                        &block_txs,
                        full_transactions,
                        false,
                        resolve_state_root_for_block(block_number)?,
                    ),
                    false,
                ));
            }
            Ok((serde_json::Value::Null, false))
        }
        "eth_getTransactionByBlockNumberAndIndex" => {
            let chain_id = param_as_u64_any_with_tx(params, &["chain_id", "chainId"])
                .unwrap_or(ctx.eth_default_chain_id);
            if let Some(upstream) = maybe_gateway_eth_upstream_read(
                chain_id,
                "eth_getTransactionByBlockNumberAndIndex",
                params,
            )? {
                return Ok((upstream, false));
            }
            let tx_index = parse_eth_block_query_tx_index(params).ok_or_else(|| {
                anyhow::anyhow!("transaction_index (or transactionIndex/index) is required")
            })?;
            let entries = collect_gateway_eth_chain_entries(
                eth_tx_index,
                ctx.eth_tx_index_store,
                chain_id,
                gateway_eth_query_scan_max(),
            )?;
            let latest =
                resolve_gateway_eth_latest_block_number(chain_id, &entries, ctx.eth_tx_index_store)?;
            let block_tag =
                parse_eth_block_query_tag(params).unwrap_or_else(|| "latest".to_string());
            let normalized_tag = block_tag.trim().trim_matches('"');
            if normalized_tag.eq_ignore_ascii_case("pending") {
                let Some((pending_block_number, pending_hash, pending_entries)) =
                    resolve_gateway_eth_pending_block_for_runtime_view(
                        chain_id,
                        latest,
                        eth_tx_index,
                        ctx.eth_tx_index_store,
                    )?
                else {
                    return Ok((serde_json::Value::Null, false));
                };
                let tx_index = tx_index as usize;
                let Some(entry) = pending_entries.get(tx_index) else {
                    return Ok((serde_json::Value::Null, false));
                };
                return Ok((
                    gateway_eth_tx_pending_with_block_json(
                        entry,
                        pending_block_number,
                        tx_index,
                        &pending_hash,
                    ),
                    false,
                ));
            }
            let Some(block_number) = parse_eth_block_number_from_tag(&block_tag, latest)? else {
                return Ok((serde_json::Value::Null, false));
            };
            let mut block_txs: Vec<GatewayEthTxIndexEntry> = entries
                .into_iter()
                .filter(|entry| entry.nonce == block_number)
                .collect();
            if block_txs.is_empty() {
                block_txs = collect_gateway_eth_block_entries_precise(
                    eth_tx_index,
                    ctx.eth_tx_index_store,
                    chain_id,
                    block_number,
                    gateway_eth_query_scan_max(),
                )?;
            }
            if block_txs.is_empty() {
                return Ok((serde_json::Value::Null, false));
            }
            sort_gateway_eth_block_txs(&mut block_txs);
            let tx_index = tx_index as usize;
            let Some(entry) = block_txs.get(tx_index) else {
                return Ok((serde_json::Value::Null, false));
            };
            let block_hash = gateway_eth_block_hash_for_txs(chain_id, block_number, &block_txs);
            Ok((
                gateway_eth_tx_with_block_json(entry, block_number, tx_index, &block_hash),
                false,
            ))
        }
        "eth_getTransactionByBlockHashAndIndex" => {
            let chain_id = param_as_u64_any_with_tx(params, &["chain_id", "chainId"])
                .unwrap_or(ctx.eth_default_chain_id);
            if let Some(upstream) = maybe_gateway_eth_upstream_read(
                chain_id,
                "eth_getTransactionByBlockHashAndIndex",
                params,
            )? {
                return Ok((upstream, false));
            }
            let tx_index = parse_eth_block_query_tx_index(params).ok_or_else(|| {
                anyhow::anyhow!("transaction_index (or transactionIndex/index) is required")
            })?;
            let block_hash_raw = extract_eth_block_hash_param(params)
                .ok_or_else(|| anyhow::anyhow!("block_hash (or blockHash/hash) is required"))?;
            let block_hash = parse_hex32_from_string(&block_hash_raw, "block_hash")?;
            let entries = collect_gateway_eth_chain_entries(
                eth_tx_index,
                ctx.eth_tx_index_store,
                chain_id,
                gateway_eth_query_scan_max(),
            )?;
            let latest =
                resolve_gateway_eth_latest_block_number(chain_id, &entries, ctx.eth_tx_index_store)?;
            let tx_index = tx_index as usize;
            if !entries.is_empty() {
                let blocks = gateway_eth_group_entries_by_block(entries.clone());
                for (block_number, block_txs) in blocks {
                    let candidate =
                        gateway_eth_block_hash_for_txs(chain_id, block_number, &block_txs);
                    if candidate != block_hash {
                        continue;
                    }
                    let Some(entry) = block_txs.get(tx_index) else {
                        return Ok((serde_json::Value::Null, false));
                    };
                    return Ok((
                        gateway_eth_tx_with_block_json(entry, block_number, tx_index, &candidate),
                        false,
                    ));
                }
            }
            if let Some((pending_block_number, pending_hash, pending_entries)) =
                resolve_gateway_eth_pending_block_for_runtime_view(
                    chain_id,
                    latest,
                    eth_tx_index,
                    ctx.eth_tx_index_store,
                )?
            {
                if pending_hash == block_hash {
                    let Some(entry) = pending_entries.get(tx_index) else {
                        return Ok((serde_json::Value::Null, false));
                    };
                    return Ok((
                        gateway_eth_tx_pending_with_block_json(
                            entry,
                            pending_block_number,
                            tx_index,
                            &pending_hash,
                        ),
                        false,
                    ));
                }
            }
            if let Some((block_number, block_txs)) = collect_gateway_eth_block_entries_by_hash_precise(
                eth_tx_index,
                ctx.eth_tx_index_store,
                chain_id,
                &block_hash,
                gateway_eth_query_scan_max(),
            )? {
                let Some(entry) = block_txs.get(tx_index) else {
                    return Ok((serde_json::Value::Null, false));
                };
                return Ok((
                    gateway_eth_tx_with_block_json(entry, block_number, tx_index, &block_hash),
                    false,
                ));
            }
            Ok((serde_json::Value::Null, false))
        }
        "eth_getBlockTransactionCountByNumber" => {
            let chain_id = param_as_u64_any_with_tx(params, &["chain_id", "chainId"])
                .unwrap_or(ctx.eth_default_chain_id);
            if let Some(upstream) = maybe_gateway_eth_upstream_read(
                chain_id,
                "eth_getBlockTransactionCountByNumber",
                params,
            )? {
                return Ok((upstream, false));
            }
            let block_tag =
                parse_eth_block_query_tag(params).unwrap_or_else(|| "latest".to_string());
            let normalized_tag = block_tag.trim().trim_matches('"');
            if normalized_tag.eq_ignore_ascii_case("pending") {
                let (pending_txs, queued_txs) = collect_gateway_eth_txpool_runtime_txs(chain_id);
                if pending_txs.is_empty() && queued_txs.is_empty() {
                    return Ok((serde_json::Value::Null, false));
                }
                let pending_count = pending_txs.len().saturating_add(queued_txs.len());
                return Ok((
                    serde_json::Value::String(format!("0x{:x}", pending_count)),
                    false,
                ));
            }
            let entries = collect_gateway_eth_chain_entries(
                eth_tx_index,
                ctx.eth_tx_index_store,
                chain_id,
                gateway_eth_query_scan_max(),
            )?;
            let latest =
                resolve_gateway_eth_latest_block_number(chain_id, &entries, ctx.eth_tx_index_store)?;
            let Some(block_number) = parse_eth_block_number_from_tag(&block_tag, latest)? else {
                return Ok((serde_json::Value::Null, false));
            };
            let mut count = entries
                .iter()
                .filter(|entry| entry.nonce == block_number)
                .count();
            if count == 0 {
                let precise_block_txs = collect_gateway_eth_block_entries_precise(
                    eth_tx_index,
                    ctx.eth_tx_index_store,
                    chain_id,
                    block_number,
                    gateway_eth_query_scan_max(),
                )?;
                if !precise_block_txs.is_empty() {
                    count = precise_block_txs.len();
                }
            }
            if count == 0 {
                if block_number <= latest {
                    return Ok((serde_json::Value::String("0x0".to_string()), false));
                }
                return Ok((serde_json::Value::Null, false));
            }
            Ok((serde_json::Value::String(format!("0x{:x}", count)), false))
        }
        "eth_getBlockTransactionCountByHash" => {
            let chain_id = param_as_u64_any_with_tx(params, &["chain_id", "chainId"])
                .unwrap_or(ctx.eth_default_chain_id);
            if let Some(upstream) = maybe_gateway_eth_upstream_read(
                chain_id,
                "eth_getBlockTransactionCountByHash",
                params,
            )? {
                return Ok((upstream, false));
            }
            let block_hash_raw = extract_eth_block_hash_param(params)
                .ok_or_else(|| anyhow::anyhow!("block_hash (or blockHash/hash) is required"))?;
            let block_hash = parse_hex32_from_string(&block_hash_raw, "block_hash")?;
            let entries = collect_gateway_eth_chain_entries(
                eth_tx_index,
                ctx.eth_tx_index_store,
                chain_id,
                gateway_eth_query_scan_max(),
            )?;
            let latest =
                resolve_gateway_eth_latest_block_number(chain_id, &entries, ctx.eth_tx_index_store)?;
            if !entries.is_empty() {
                let blocks = gateway_eth_group_entries_by_block(entries.clone());
                for (block_number, block_txs) in blocks {
                    let candidate =
                        gateway_eth_block_hash_for_txs(chain_id, block_number, &block_txs);
                    if candidate == block_hash {
                        return Ok((
                            serde_json::Value::String(format!("0x{:x}", block_txs.len())),
                            false,
                        ));
                    }
                }
            }
            if let Some((_pending_block_number, pending_hash, pending_entries)) =
                resolve_gateway_eth_pending_block_for_runtime_view(
                    chain_id,
                    latest,
                    eth_tx_index,
                    ctx.eth_tx_index_store,
                )?
            {
                if pending_hash == block_hash {
                    return Ok((
                        serde_json::Value::String(format!("0x{:x}", pending_entries.len())),
                        false,
                    ));
                }
            }
            if let Some((_block_number, block_txs)) = collect_gateway_eth_block_entries_by_hash_precise(
                eth_tx_index,
                ctx.eth_tx_index_store,
                chain_id,
                &block_hash,
                gateway_eth_query_scan_max(),
            )? {
                return Ok((
                    serde_json::Value::String(format!("0x{:x}", block_txs.len())),
                    false,
                ));
            }
            Ok((serde_json::Value::Null, false))
        }
        "eth_getBlockReceipts" => {
            let chain_id = param_as_u64_any_with_tx(params, &["chain_id", "chainId"])
                .unwrap_or(ctx.eth_default_chain_id);
            if let Some(upstream) =
                maybe_gateway_eth_upstream_read(chain_id, "eth_getBlockReceipts", params)?
            {
                return Ok((upstream, false));
            }
            let entries = collect_gateway_eth_chain_entries(
                eth_tx_index,
                ctx.eth_tx_index_store,
                chain_id,
                gateway_eth_query_scan_max(),
            )?;
            let latest =
                resolve_gateway_eth_latest_block_number(chain_id, &entries, ctx.eth_tx_index_store)?;
            let requested_hash = parse_eth_block_hash_from_params(params)?;
            let block_tag = if requested_hash.is_none() {
                parse_eth_block_query_tag(params).unwrap_or_else(|| "latest".to_string())
            } else {
                String::new()
            };
            let normalized_block_tag = block_tag.trim().trim_matches('"');
            let pending_block = resolve_gateway_eth_pending_block_for_runtime_view(
                chain_id,
                latest,
                eth_tx_index,
                ctx.eth_tx_index_store,
            )?;
            if requested_hash.is_none() && normalized_block_tag.eq_ignore_ascii_case("pending") {
                let Some((pending_block_number, pending_block_hash, pending_block_txs)) =
                    pending_block.as_ref()
                else {
                    return Ok((serde_json::Value::Null, false));
                };
                let receipts = gateway_eth_tx_receipts_pending_with_block_json(
                    *pending_block_number,
                    pending_block_hash,
                    pending_block_txs,
                );
                return Ok((serde_json::Value::Array(receipts), false));
            }
            if let Some(hash) = requested_hash {
                if !entries.is_empty() {
                    for (block_number, block_txs) in gateway_eth_group_entries_by_block(entries.clone()) {
                        let candidate = gateway_eth_block_hash_for_txs(chain_id, block_number, &block_txs);
                        if candidate != hash {
                            continue;
                        }
                        let receipts = block_txs
                            .iter()
                            .enumerate()
                            .map(|(idx, entry)| {
                                let cumulative_gas_used =
                                    gateway_eth_block_cumulative_gas_used(&block_txs, idx);
                                gateway_eth_tx_receipt_with_block_json(
                                    entry,
                                    block_number,
                                    idx,
                                    &candidate,
                                    cumulative_gas_used,
                                )
                            })
                            .collect();
                        return Ok((serde_json::Value::Array(receipts), false));
                    }
                }
                if let Some((pending_block_number, pending_block_hash, pending_block_txs)) =
                    pending_block.as_ref()
                {
                    if *pending_block_hash == hash {
                        let receipts = gateway_eth_tx_receipts_pending_with_block_json(
                            *pending_block_number,
                            pending_block_hash,
                            pending_block_txs,
                        );
                        return Ok((serde_json::Value::Array(receipts), false));
                    }
                }
                if let Some((block_number, block_txs)) = collect_gateway_eth_block_entries_by_hash_precise(
                    eth_tx_index,
                    ctx.eth_tx_index_store,
                    chain_id,
                    &hash,
                    gateway_eth_query_scan_max(),
                )? {
                    let receipts = block_txs
                        .iter()
                        .enumerate()
                        .map(|(idx, entry)| {
                            let cumulative_gas_used =
                                gateway_eth_block_cumulative_gas_used(&block_txs, idx);
                            gateway_eth_tx_receipt_with_block_json(
                                entry,
                                block_number,
                                idx,
                                &hash,
                                cumulative_gas_used,
                            )
                        })
                        .collect();
                    return Ok((serde_json::Value::Array(receipts), false));
                }
                return Ok((serde_json::Value::Null, false));
            }
            let requested_block_number = if requested_hash.is_none() {
                parse_eth_block_number_from_tag(&block_tag, latest)?
            } else {
                None
            };
            if let Some(number) = requested_block_number {
                if let Some((pending_block_number, pending_block_hash, pending_block_txs)) =
                    pending_block.as_ref()
                {
                    if number == *pending_block_number {
                        let receipts = gateway_eth_tx_receipts_pending_with_block_json(
                            *pending_block_number,
                            pending_block_hash,
                            pending_block_txs,
                        );
                        return Ok((serde_json::Value::Array(receipts), false));
                    }
                }
            }
            let Some((block_number, block_hash, block_txs)) =
                resolve_gateway_eth_block_txs(chain_id, params, entries)?
            else {
                if requested_hash.is_some() {
                    return Ok((serde_json::Value::Null, false));
                }
                if let Some(number) = requested_block_number {
                    let precise_block_txs = collect_gateway_eth_block_entries_precise(
                        eth_tx_index,
                        ctx.eth_tx_index_store,
                        chain_id,
                        number,
                        gateway_eth_query_scan_max(),
                    )?;
                    if !precise_block_txs.is_empty() {
                        let block_hash =
                            gateway_eth_block_hash_for_txs(chain_id, number, &precise_block_txs);
                        let receipts: Vec<serde_json::Value> = precise_block_txs
                            .iter()
                            .enumerate()
                            .map(|(idx, entry)| {
                                let cumulative_gas_used =
                                    gateway_eth_block_cumulative_gas_used(&precise_block_txs, idx);
                                gateway_eth_tx_receipt_with_block_json(
                                    entry,
                                    number,
                                    idx,
                                    &block_hash,
                                    cumulative_gas_used,
                                )
                            })
                            .collect();
                        return Ok((serde_json::Value::Array(receipts), false));
                    }
                    if number <= latest {
                        return Ok((serde_json::Value::Array(Vec::new()), false));
                    }
                }
                return Ok((serde_json::Value::Null, false));
            };
            let receipts: Vec<serde_json::Value> = block_txs
                .iter()
                .enumerate()
                .map(|(idx, entry)| {
                    let cumulative_gas_used = gateway_eth_block_cumulative_gas_used(&block_txs, idx);
                    gateway_eth_tx_receipt_with_block_json(
                        entry,
                        block_number,
                        idx,
                        &block_hash,
                        cumulative_gas_used,
                    )
                })
                .collect();
            Ok((serde_json::Value::Array(receipts), false))
        }
        "eth_getUncleCountByBlockNumber" => {
            let chain_id = param_as_u64_any_with_tx(params, &["chain_id", "chainId"])
                .unwrap_or(ctx.eth_default_chain_id);
            if let Some(upstream) = maybe_gateway_eth_upstream_read(
                chain_id,
                "eth_getUncleCountByBlockNumber",
                params,
            )? {
                return Ok((upstream, false));
            }
            let entries = collect_gateway_eth_chain_entries(
                eth_tx_index,
                ctx.eth_tx_index_store,
                chain_id,
                gateway_eth_query_scan_max(),
            )?;
            let latest =
                resolve_gateway_eth_latest_block_number(chain_id, &entries, ctx.eth_tx_index_store)?;
            let block_tag = parse_eth_block_query_tag(params).unwrap_or_else(|| "latest".to_string());
            let normalized_block_tag = block_tag.trim().trim_matches('"');
            if normalized_block_tag.eq_ignore_ascii_case("pending") {
                if resolve_gateway_eth_pending_block_for_runtime_view(
                    chain_id,
                    latest,
                    eth_tx_index,
                    ctx.eth_tx_index_store,
                )?
                .is_some()
                {
                    return Ok((serde_json::Value::String("0x0".to_string()), false));
                }
                return Ok((serde_json::Value::Null, false));
            }
            let Some(block_number) = parse_eth_block_number_from_tag(&block_tag, latest)? else {
                return Ok((serde_json::Value::Null, false));
            };
            if block_number <= latest {
                return Ok((serde_json::Value::String("0x0".to_string()), false));
            }
            let precise_block_txs = collect_gateway_eth_block_entries_precise(
                eth_tx_index,
                ctx.eth_tx_index_store,
                chain_id,
                block_number,
                gateway_eth_query_scan_max(),
            )?;
            if !precise_block_txs.is_empty() {
                return Ok((serde_json::Value::String("0x0".to_string()), false));
            }
            Ok((serde_json::Value::Null, false))
        }
        "eth_getUncleCountByBlockHash" => {
            let chain_id = param_as_u64_any_with_tx(params, &["chain_id", "chainId"])
                .unwrap_or(ctx.eth_default_chain_id);
            if let Some(upstream) = maybe_gateway_eth_upstream_read(
                chain_id,
                "eth_getUncleCountByBlockHash",
                params,
            )? {
                return Ok((upstream, false));
            }
            let requested_hash = parse_eth_block_hash_from_params(params)?;
            let entries = collect_gateway_eth_chain_entries(
                eth_tx_index,
                ctx.eth_tx_index_store,
                chain_id,
                gateway_eth_query_scan_max(),
            )?;
            let latest =
                resolve_gateway_eth_latest_block_number(chain_id, &entries, ctx.eth_tx_index_store)?;
            if let Some(hash) = requested_hash {
                if !entries.is_empty() {
                    for (block_number, block_txs) in gateway_eth_group_entries_by_block(entries.clone())
                    {
                        let candidate =
                            gateway_eth_block_hash_for_txs(chain_id, block_number, &block_txs);
                        if candidate == hash {
                            return Ok((serde_json::Value::String("0x0".to_string()), false));
                        }
                    }
                }
                if let Some((_pending_block_number, pending_block_hash, _pending_block_txs)) =
                    resolve_gateway_eth_pending_block_for_runtime_view(
                        chain_id,
                        latest,
                        eth_tx_index,
                        ctx.eth_tx_index_store,
                    )?
                {
                    if pending_block_hash == hash {
                        return Ok((serde_json::Value::String("0x0".to_string()), false));
                    }
                }
                if collect_gateway_eth_block_entries_by_hash_precise(
                    eth_tx_index,
                    ctx.eth_tx_index_store,
                    chain_id,
                    &hash,
                    gateway_eth_query_scan_max(),
                )?
                .is_some()
                {
                    return Ok((serde_json::Value::String("0x0".to_string()), false));
                }
                return Ok((serde_json::Value::Null, false));
            }
            let Some((_block_number, _block_hash, _block_txs)) =
                resolve_gateway_eth_block_txs(chain_id, params, entries)?
            else {
                return Ok((serde_json::Value::Null, false));
            };
            Ok((serde_json::Value::String("0x0".to_string()), false))
        }
        "eth_getUncleByBlockNumberAndIndex" => {
            let chain_id = param_as_u64_any_with_tx(params, &["chain_id", "chainId"])
                .unwrap_or(ctx.eth_default_chain_id);
            let tx_index = parse_eth_block_query_tx_index(params).ok_or_else(|| {
                anyhow::anyhow!("transaction_index (or transactionIndex/index) is required")
            })?;
            let entries = collect_gateway_eth_chain_entries(
                eth_tx_index,
                ctx.eth_tx_index_store,
                chain_id,
                gateway_eth_query_scan_max(),
            )?;
            let Some((_block_number, _block_hash, _block_txs)) =
                resolve_gateway_eth_block_txs(chain_id, params, entries)?
            else {
                return Ok((serde_json::Value::Null, false));
            };
            let _ = tx_index; // no uncle data yet in minimal mirror mode
            Ok((serde_json::Value::Null, false))
        }
        "eth_getUncleByBlockHashAndIndex" => {
            let chain_id = param_as_u64_any_with_tx(params, &["chain_id", "chainId"])
                .unwrap_or(ctx.eth_default_chain_id);
            let tx_index = parse_eth_block_query_tx_index(params).ok_or_else(|| {
                anyhow::anyhow!("transaction_index (or transactionIndex/index) is required")
            })?;
            let entries = collect_gateway_eth_chain_entries(
                eth_tx_index,
                ctx.eth_tx_index_store,
                chain_id,
                gateway_eth_query_scan_max(),
            )?;
            let Some((_block_number, _block_hash, _block_txs)) =
                resolve_gateway_eth_block_txs(chain_id, params, entries)?
            else {
                return Ok((serde_json::Value::Null, false));
            };
            let _ = tx_index; // no uncle data yet in minimal mirror mode
            Ok((serde_json::Value::Null, false))
        }
        "eth_getLogs" => {
            let chain_id = param_as_u64_any_with_tx(params, &["chain_id", "chainId"])
                .unwrap_or(ctx.eth_default_chain_id);
            if let Some(upstream) =
                maybe_gateway_eth_upstream_read(chain_id, "eth_getLogs", params)?
            {
                return Ok((upstream, false));
            }
            let entries = collect_gateway_eth_chain_entries(
                eth_tx_index,
                ctx.eth_tx_index_store,
                chain_id,
                gateway_eth_query_scan_max(),
            )?;
            let latest =
                resolve_gateway_eth_latest_block_number(chain_id, &entries, ctx.eth_tx_index_store)?;
            let query = parse_eth_logs_query_from_params(params, latest)?;
            let logs = collect_gateway_eth_logs_with_query(
                chain_id,
                entries,
                &query,
                eth_tx_index,
                ctx.eth_tx_index_store,
            )?;
            Ok((serde_json::Value::Array(logs), false))
        }
        "eth_subscribe" => {
            let sub_kind_raw = parse_eth_subscribe_kind(params)
                .ok_or_else(|| anyhow::anyhow!("subscription kind is required"))?;
            let sub_kind = sub_kind_raw.trim().to_ascii_lowercase();
            match sub_kind.as_str() {
                "newheads" => {
                    let chain_id = param_as_u64_any_with_tx(params, &["chain_id", "chainId"])
                        .unwrap_or(ctx.eth_default_chain_id);
                    let entries = collect_gateway_eth_chain_entries(
                        eth_tx_index,
                        ctx.eth_tx_index_store,
                        chain_id,
                        gateway_eth_query_scan_max(),
                    )?;
                    let last_seen_block = resolve_gateway_eth_latest_block_number(
                        chain_id,
                        &entries,
                        ctx.eth_tx_index_store,
                    )?;
                    let filter_id = ctx.eth_filters.insert(GatewayEthFilterKind::Blocks {
                        chain_id,
                        last_seen_block,
                    });
                    Ok((serde_json::Value::String(format!("0x{:x}", filter_id)), false))
                }
                "newpendingtransactions" => {
                    let chain_id = param_as_u64_any_with_tx(params, &["chain_id", "chainId"])
                        .unwrap_or(ctx.eth_default_chain_id);
                    let last_seen_hashes = collect_gateway_eth_pending_hashes_runtime(chain_id);
                    let filter_id =
                        ctx.eth_filters
                            .insert(GatewayEthFilterKind::PendingTransactions {
                                chain_id,
                                last_seen_hashes,
                            });
                    Ok((serde_json::Value::String(format!("0x{:x}", filter_id)), false))
                }
                "logs" => {
                    let chain_id = param_as_u64_any_with_tx(params, &["chain_id", "chainId"])
                        .unwrap_or(ctx.eth_default_chain_id);
                    let entries = collect_gateway_eth_chain_entries(
                        eth_tx_index,
                        ctx.eth_tx_index_store,
                        chain_id,
                        gateway_eth_query_scan_max(),
                    )?;
                    let latest = resolve_gateway_eth_latest_block_number(
                        chain_id,
                        &entries,
                        ctx.eth_tx_index_store,
                    )?;
                    let query = parse_eth_logs_query_from_params(params, latest)?;
                    let filter = GatewayEthLogsFilter {
                        chain_id,
                        next_block: query.from_block.unwrap_or(0),
                        query,
                        block_hash_drained: false,
                    };
                    let filter_id = ctx.eth_filters.insert(GatewayEthFilterKind::Logs(filter));
                    Ok((serde_json::Value::String(format!("0x{:x}", filter_id)), false))
                }
                _ => bail!(
                    "unsupported eth_subscribe kind: {} (supported: newHeads|newPendingTransactions|logs)",
                    sub_kind_raw
                ),
            }
        }
        "eth_newFilter" => {
            let chain_id = param_as_u64_any_with_tx(params, &["chain_id", "chainId"])
                .unwrap_or(ctx.eth_default_chain_id);
            let entries = collect_gateway_eth_chain_entries(
                eth_tx_index,
                ctx.eth_tx_index_store,
                chain_id,
                gateway_eth_query_scan_max(),
            )?;
            let latest = resolve_gateway_eth_latest_block_number(
                chain_id,
                &entries,
                ctx.eth_tx_index_store,
            )?;
            let query = parse_eth_logs_query_from_params(params, latest)?;
            let filter = GatewayEthLogsFilter {
                chain_id,
                next_block: query.from_block.unwrap_or(0),
                query,
                block_hash_drained: false,
            };
            let filter_id = ctx.eth_filters.insert(GatewayEthFilterKind::Logs(filter));
            Ok((
                serde_json::Value::String(format!("0x{:x}", filter_id)),
                false,
            ))
        }
        "eth_newBlockFilter" => {
            let chain_id = param_as_u64_any_with_tx(params, &["chain_id", "chainId"])
                .unwrap_or(ctx.eth_default_chain_id);
            let entries = collect_gateway_eth_chain_entries(
                eth_tx_index,
                ctx.eth_tx_index_store,
                chain_id,
                gateway_eth_query_scan_max(),
            )?;
            let last_seen_block =
                resolve_gateway_eth_latest_block_number(chain_id, &entries, ctx.eth_tx_index_store)?;
            let filter_id = ctx.eth_filters.insert(GatewayEthFilterKind::Blocks {
                chain_id,
                last_seen_block,
            });
            Ok((
                serde_json::Value::String(format!("0x{:x}", filter_id)),
                false,
            ))
        }
        "eth_newPendingTransactionFilter" => {
            let chain_id = param_as_u64_any_with_tx(params, &["chain_id", "chainId"])
                .unwrap_or(ctx.eth_default_chain_id);
            let last_seen_hashes = collect_gateway_eth_pending_hashes_runtime(chain_id);
            let filter_id = ctx
                .eth_filters
                .insert(GatewayEthFilterKind::PendingTransactions {
                    chain_id,
                    last_seen_hashes,
                });
            Ok((
                serde_json::Value::String(format!("0x{:x}", filter_id)),
                false,
            ))
        }
        "eth_unsubscribe" => {
            let filter_id = parse_eth_filter_id(params)
                .ok_or_else(|| anyhow::anyhow!("subscription id is required"))?;
            Ok((
                serde_json::Value::Bool(ctx.eth_filters.filters.remove(&filter_id).is_some()),
                false,
            ))
        }
        "eth_uninstallFilter" => {
            let filter_id = parse_eth_filter_id(params)
                .ok_or_else(|| anyhow::anyhow!("filter_id (or id) is required"))?;
            Ok((serde_json::Value::Bool(ctx.eth_filters.filters.remove(&filter_id).is_some()), false))
        }
        "eth_getFilterLogs" => {
            let filter_id = parse_eth_filter_id(params)
                .ok_or_else(|| anyhow::anyhow!("filter_id (or id) is required"))?;
            let Some(filter) = ctx.eth_filters.filters.get(&filter_id).cloned() else {
                bail!("filter not found: 0x{:x}", filter_id);
            };
            match filter {
                GatewayEthFilterKind::Logs(log_filter) => {
                    let entries = collect_gateway_eth_chain_entries(
                        eth_tx_index,
                        ctx.eth_tx_index_store,
                        log_filter.chain_id,
                        gateway_eth_query_scan_max(),
                    )?;
                    let logs = collect_gateway_eth_logs_with_query(
                        log_filter.chain_id,
                        entries,
                        &log_filter.query,
                        eth_tx_index,
                        ctx.eth_tx_index_store,
                    )?;
                    Ok((serde_json::Value::Array(logs), false))
                }
                GatewayEthFilterKind::Blocks { .. }
                | GatewayEthFilterKind::PendingTransactions { .. } => {
                    bail!("filter does not support logs: 0x{:x}", filter_id)
                }
            }
        }
        "eth_getFilterChanges" => {
            let filter_id = parse_eth_filter_id(params)
                .ok_or_else(|| anyhow::anyhow!("filter_id (or id) is required"))?;
            let response = gateway_eth_filter_changes_item_json(
                eth_tx_index,
                ctx.eth_tx_index_store,
                ctx.eth_filters,
                filter_id,
            )?;
            Ok((response, false))
        }
        "txpool_content" => {
            let chain_id = param_as_u64_any_with_tx(params, &["chain_id", "chainId"])
                .unwrap_or(ctx.eth_default_chain_id);
            poll_gateway_eth_public_broadcast_native_runtime(chain_id, 64);
            let (pending_txs, queued_txs) = collect_gateway_eth_txpool_txs_with_index_fallback(
                chain_id,
                eth_tx_index,
                ctx.eth_tx_index_store,
            )?;
            if !pending_txs.is_empty() || !queued_txs.is_empty() {
                return Ok((build_gateway_eth_txpool_content_from_ir(pending_txs, queued_txs), false));
            }
            Ok((build_gateway_eth_txpool_content(Vec::new()), false))
        }
        "txpool_contentFrom" => {
            let chain_id = param_as_u64_any_with_tx(params, &["chain_id", "chainId"])
                .unwrap_or(ctx.eth_default_chain_id);
            poll_gateway_eth_public_broadcast_native_runtime(chain_id, 64);
            let address_raw = extract_eth_persona_address_param(params)
                .ok_or_else(|| anyhow::anyhow!("address (or from/external_address) is required"))?;
            let address = decode_hex_bytes(&address_raw, "address")?;
            if address.len() != 20 {
                bail!("address must be 20 bytes");
            }
            let (pending_txs, queued_txs) = collect_gateway_eth_txpool_txs_with_index_fallback(
                chain_id,
                eth_tx_index,
                ctx.eth_tx_index_store,
            )?;
            Ok((
                build_gateway_eth_txpool_content_from_ir_for_sender(
                    pending_txs,
                    queued_txs,
                    &address,
                ),
                false,
            ))
        }
        "txpool_inspect" => {
            let chain_id = param_as_u64_any_with_tx(params, &["chain_id", "chainId"])
                .unwrap_or(ctx.eth_default_chain_id);
            poll_gateway_eth_public_broadcast_native_runtime(chain_id, 64);
            let (pending_txs, queued_txs) = collect_gateway_eth_txpool_txs_with_index_fallback(
                chain_id,
                eth_tx_index,
                ctx.eth_tx_index_store,
            )?;
            if !pending_txs.is_empty() || !queued_txs.is_empty() {
                return Ok((build_gateway_eth_txpool_inspect_from_ir(pending_txs, queued_txs), false));
            }
            Ok((build_gateway_eth_txpool_inspect(Vec::new()), false))
        }
        "txpool_inspectFrom" => {
            let chain_id = param_as_u64_any_with_tx(params, &["chain_id", "chainId"])
                .unwrap_or(ctx.eth_default_chain_id);
            poll_gateway_eth_public_broadcast_native_runtime(chain_id, 64);
            let address_raw = extract_eth_persona_address_param(params)
                .ok_or_else(|| anyhow::anyhow!("address (or from/external_address) is required"))?;
            let address = decode_hex_bytes(&address_raw, "address")?;
            if address.len() != 20 {
                bail!("address must be 20 bytes");
            }
            let (pending_txs, queued_txs) = collect_gateway_eth_txpool_txs_with_index_fallback(
                chain_id,
                eth_tx_index,
                ctx.eth_tx_index_store,
            )?;
            Ok((
                build_gateway_eth_txpool_inspect_from_ir_for_sender(
                    pending_txs,
                    queued_txs,
                    &address,
                ),
                false,
            ))
        }
        "txpool_status" => {
            let chain_id = param_as_u64_any_with_tx(params, &["chain_id", "chainId"])
                .unwrap_or(ctx.eth_default_chain_id);
            poll_gateway_eth_public_broadcast_native_runtime(chain_id, 64);
            let (pending_txs, queued_txs) = collect_gateway_eth_txpool_txs_with_index_fallback(
                chain_id,
                eth_tx_index,
                ctx.eth_tx_index_store,
            )?;
            if !pending_txs.is_empty() || !queued_txs.is_empty() {
                return Ok((build_gateway_eth_txpool_status_from_ir(&pending_txs, &queued_txs), false));
            }
            Ok((
                serde_json::json!({
                    "pending": "0x0",
                    "queued": "0x0",
                }),
                false,
            ))
        }
        "txpool_statusFrom" => {
            let chain_id = param_as_u64_any_with_tx(params, &["chain_id", "chainId"])
                .unwrap_or(ctx.eth_default_chain_id);
            poll_gateway_eth_public_broadcast_native_runtime(chain_id, 64);
            let address_raw = extract_eth_persona_address_param(params)
                .ok_or_else(|| anyhow::anyhow!("address (or from/external_address) is required"))?;
            let address = decode_hex_bytes(&address_raw, "address")?;
            if address.len() != 20 {
                bail!("address must be 20 bytes");
            }
            let (pending_txs, queued_txs) = collect_gateway_eth_txpool_txs_with_index_fallback(
                chain_id,
                eth_tx_index,
                ctx.eth_tx_index_store,
            )?;
            Ok((
                build_gateway_eth_txpool_status_from_ir_for_sender(
                    &pending_txs,
                    &queued_txs,
                    &address,
                ),
                false,
            ))
        }
        "eth_gasPrice" => {
            let chain_id = param_as_u64_any_with_tx(params, &["chain_id", "chainId"])
                .unwrap_or(ctx.eth_default_chain_id);
            if let Some(upstream) =
                maybe_gateway_eth_upstream_read(chain_id, "eth_gasPrice", params)?
            {
                return Ok((upstream, false));
            }
            let suggested = gateway_eth_suggest_gas_price_wei(
                chain_id,
                eth_tx_index,
                ctx.eth_tx_index_store,
                gateway_eth_default_gas_price_wei(chain_id),
            )?;
            Ok((
                serde_json::Value::String(format!("0x{:x}", suggested)),
                false,
            ))
        }
        "eth_call" => {
            let chain_id = param_as_u64_any_with_tx(params, &["chain_id", "chainId"])
                .unwrap_or(ctx.eth_default_chain_id);
            if let Some(upstream) =
                maybe_gateway_eth_upstream_read(chain_id, "eth_call", params)?
            {
                return Ok((upstream, false));
            }
            let from = match extract_eth_persona_address_param(params) {
                Some(raw_from) => {
                    let from = decode_hex_bytes(&raw_from, "from")?;
                    if from.len() != 20 {
                        bail!("from must be 20 bytes");
                    }
                    Some(from)
                }
                None => None,
            };
            let to = match param_as_string_any_with_tx(params, &["to"]) {
                Some(raw_to) => {
                    let trimmed = raw_to.trim();
                    if trimmed.is_empty() || trimmed.eq_ignore_ascii_case("0x") {
                        None
                    } else {
                        let decoded = decode_hex_bytes(trimmed, "to")?;
                        if decoded.len() != 20 {
                            bail!("to must be 20 bytes");
                        }
                        Some(decoded)
                    }
                }
                None => None,
            };
            let value = param_as_u128_any_with_tx(params, &["value"]).unwrap_or(0);
            let call_data = if let Some(raw_data) = param_as_string_any_with_tx(params, &["data", "input"]) {
                let trimmed = raw_data.trim();
                if !trimmed.is_empty() && !trimmed.eq_ignore_ascii_case("0x") {
                    decode_hex_bytes(trimmed, "data")?
                } else {
                    Vec::new()
                }
            } else {
                Vec::new()
            };
            let entries = collect_gateway_eth_chain_entries(
                eth_tx_index,
                ctx.eth_tx_index_store,
                chain_id,
                gateway_eth_query_scan_max(),
            )?;
            let latest =
                resolve_gateway_eth_latest_block_number(chain_id, &entries, ctx.eth_tx_index_store)?;
            let block_tag =
                parse_eth_tx_count_block_tag(params).unwrap_or_else(|| "latest".to_string());
            let Some(view_entries) =
                resolve_gateway_eth_get_proof_entries(chain_id, entries, &block_tag, latest)?
            else {
                return Ok((serde_json::Value::Null, false));
            };
            if let Some(from_addr) = from.as_ref() {
                let balance = gateway_eth_balance_from_entries(&view_entries, from_addr);
                if balance < value {
                    bail!(
                        "insufficient funds for eth_call value transfer: balance={} value={}",
                        balance,
                        value
                    );
                }
            }
            let Some(to) = to else {
                // Contract-creation call path (to=null) is accepted, but current gateway
                // projection has no initcode execution VM; keep deterministic empty return.
                return Ok((serde_json::Value::String("0x".to_string()), false));
            };

            // Standard ERC20 balanceOf(address) selector.
            const ERC20_BALANCE_OF_SELECTOR: [u8; 4] = [0x70, 0xa0, 0x82, 0x31];
            const ERC20_TOTAL_SUPPLY_SELECTOR: [u8; 4] = [0x18, 0x16, 0x0d, 0xdd];
            const ERC20_DECIMALS_SELECTOR: [u8; 4] = [0x31, 0x3c, 0xe5, 0x67];
            const ERC20_ALLOWANCE_SELECTOR: [u8; 4] = [0xdd, 0x62, 0xed, 0x3e];
            if call_data.len() == 36
                && call_data[0..4] == ERC20_BALANCE_OF_SELECTOR
                && view_entries.iter().any(|entry| entry.chain_id == chain_id)
            {
                let query_addr = call_data[16..36].to_vec();
                let balance = gateway_eth_balance_from_entries(&view_entries, &query_addr);
                return Ok((
                    serde_json::Value::String(format!("0x{:064x}", balance)),
                    false,
                ));
            }
            if call_data.len() == 4 && call_data[0..4] == ERC20_TOTAL_SUPPLY_SELECTOR {
                let total_supply = gateway_eth_total_supply_from_entries(&view_entries);
                return Ok((
                    serde_json::Value::String(format!("0x{:064x}", total_supply)),
                    false,
                ));
            }
            if call_data.len() == 4 && call_data[0..4] == ERC20_DECIMALS_SELECTOR {
                return Ok((
                    serde_json::Value::String(format!("0x{:064x}", 18u8)),
                    false,
                ));
            }
            if call_data.len() == 68 && call_data[0..4] == ERC20_ALLOWANCE_SELECTOR {
                return Ok((
                    serde_json::Value::String(format!("0x{:064x}", 0u8)),
                    false,
                ));
            }

            // Minimal read-only convention: when calldata is exactly one 32-byte slot, reuse
            // the same slot resolver as eth_getStorageAt for deterministic local reads.
            if call_data.len() == 32 {
                let mut slot_key = [0u8; 32];
                slot_key.copy_from_slice(&call_data[..32]);
                let storage =
                    gateway_eth_resolve_storage_word_from_entries_by_key(&view_entries, &to, slot_key)
                        .unwrap_or([0u8; 32]);
                return Ok((
                    serde_json::Value::String(format!("0x{}", to_hex(&storage))),
                    false,
                ));
            }

            // Minimal mirror fallback: empty calldata returns the known deployed code bytes.
            if call_data.is_empty() {
                let code = gateway_eth_resolve_code_from_entries(&view_entries, &to)
                    .unwrap_or_default();
                return Ok((
                    serde_json::Value::String(format!("0x{}", to_hex(&code))),
                    false,
                ));
            }

            if !gateway_eth_has_code_for_address(&view_entries, &to) {
                return Ok((serde_json::Value::String("0x".to_string()), false));
            }
            Ok((serde_json::Value::String("0x".to_string()), false))
        }
        "eth_estimateGas" => {
            let chain_id =
                resolve_chain_id_with_tx_consistency(params, ctx.eth_default_chain_id)?;
            if let Some(upstream) =
                maybe_gateway_eth_upstream_read(chain_id, "eth_estimateGas", params)?
            {
                return Ok((upstream, false));
            }
            let eth_default_gas_price = gateway_eth_default_gas_price_wei(chain_id);
            let (access_list_address_count, access_list_storage_key_count) =
                parse_eth_access_list_intrinsic_counts(params)?;
            let (max_fee_per_blob_gas, blob_hash_count) =
                parse_eth_blob_intrinsic_fields(params)?;
            let from = match extract_eth_persona_address_param(params) {
                Some(raw_from) => decode_hex_bytes(&raw_from, "from")?,
                None => vec![0u8; 20],
            };
            if from.len() != 20 {
                bail!("from must be 20 bytes");
            }
            let to = match param_as_string_any_with_tx(params, &["to"]) {
                Some(raw_to) => {
                    let trimmed = raw_to.trim();
                    if trimmed.is_empty() || trimmed.eq_ignore_ascii_case("0x") {
                        None
                    } else {
                        let decoded = decode_hex_bytes(trimmed, "to")?;
                        if decoded.len() != 20 {
                            bail!("to must be 20 bytes");
                        }
                        Some(decoded)
                    }
                }
                None => None,
            };
            let data = match param_as_string_any_with_tx(params, &["data", "input"]) {
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
            let value = param_as_u128_any_with_tx(params, &["value"]).unwrap_or(0);
            let has_access_list_intrinsic =
                access_list_address_count > 0 || access_list_storage_key_count > 0;
            let explicit_tx_type = param_as_u64_any_with_tx(params, &["tx_type", "txType", "type"]);
            let max_fee_per_gas_param =
                param_as_u64_any_with_tx(params, &["max_fee_per_gas", "maxFeePerGas"]);
            let max_priority_fee_per_gas_param = param_as_u64_any_with_tx(
                params,
                &["max_priority_fee_per_gas", "maxPriorityFeePerGas"],
            );
            let has_eip1559_fee_fields = param_as_u64_any_with_tx(
                params,
                &[
                    "max_fee_per_gas",
                    "maxFeePerGas",
                    "max_priority_fee_per_gas",
                    "maxPriorityFeePerGas",
                ],
            )
            .is_some();
            let envelope_tx_type = resolve_gateway_eth_write_tx_type(
                chain_id,
                explicit_tx_type,
                has_eip1559_fee_fields,
                has_access_list_intrinsic,
                max_fee_per_blob_gas,
                blob_hash_count,
            )?;
            let tx_type = if to.is_some() {
                if data.is_empty() {
                    TxType::Transfer
                } else {
                    TxType::ContractCall
                }
            } else {
                TxType::ContractDeploy
            };
            let tx_ir = TxIR {
                hash: Vec::new(),
                from: from.clone(),
                to: to.clone(),
                value,
                gas_limit: u64::MAX,
                gas_price: eth_default_gas_price,
                nonce: 0,
                data: data.clone(),
                signature: Vec::new(),
                chain_id,
                tx_type,
                source_chain: None,
                target_chain: None,
            };
            let intrinsic = estimate_intrinsic_gas_with_envelope_extras_m0(
                &tx_ir,
                access_list_address_count,
                access_list_storage_key_count,
                if envelope_tx_type == 3 {
                    blob_hash_count
                } else {
                    0
                },
            );
            let entries = collect_gateway_eth_chain_entries(
                eth_tx_index,
                ctx.eth_tx_index_store,
                chain_id,
                gateway_eth_query_scan_max(),
            )?;
            let latest =
                resolve_gateway_eth_latest_block_number(chain_id, &entries, ctx.eth_tx_index_store)?;
            let pending_block_number = latest.saturating_add(1);
            validate_gateway_eth_tx_type_fork_activation(
                chain_id,
                envelope_tx_type,
                pending_block_number,
            )?;
            if to.is_none() {
                validate_gateway_eth_contract_deploy_initcode_size(
                    chain_id,
                    pending_block_number,
                    data.len(),
                )?;
            }
            let block_tag =
                parse_eth_tx_count_block_tag(params).unwrap_or_else(|| "latest".to_string());
            let view_entries =
                resolve_gateway_eth_get_proof_entries(chain_id, entries, &block_tag, latest)?
                    .unwrap_or_default();
            let execution_surcharge = if let Some(contract) = to.as_ref() {
                if !data.is_empty() && gateway_eth_has_code_for_address(&view_entries, contract) {
                    u64_env("NOVOVM_GATEWAY_ETH_ESTIMATE_EXEC_CALL_EXTRA", 25_000)
                } else {
                    0
                }
            } else {
                0
            };
            let estimated = intrinsic.saturating_add(execution_surcharge);
            if envelope_tx_type == 2 || envelope_tx_type == 3 {
                if max_fee_per_gas_param.is_none() {
                    bail!("eth_estimateGas maxFeePerGas is required for type2/type3 transactions");
                }
                let max_fee_per_gas = max_fee_per_gas_param.unwrap_or(eth_default_gas_price);
                let max_priority_fee_per_gas = max_priority_fee_per_gas_param.unwrap_or(
                    gateway_eth_default_max_priority_fee_per_gas_wei(chain_id).min(max_fee_per_gas),
                );
                if max_priority_fee_per_gas > max_fee_per_gas {
                    bail!(
                        "eth_estimateGas maxPriorityFeePerGas exceeds maxFeePerGas: max_priority_fee_per_gas={} max_fee_per_gas={}",
                        max_priority_fee_per_gas,
                        max_fee_per_gas
                    );
                }
                let base_fee_per_gas = gateway_eth_base_fee_per_gas_wei(chain_id);
                if max_fee_per_gas < base_fee_per_gas {
                    bail!(
                        "eth_estimateGas maxFeePerGas below current base fee: max_fee_per_gas={} base_fee_per_gas={}",
                        max_fee_per_gas,
                        base_fee_per_gas
                    );
                }
            }
            if let Some(gas_cap) = param_as_u64_any_with_tx(params, &["gas", "gas_limit", "gasLimit"]) {
                if gas_cap < estimated {
                    bail!(
                        "eth_estimateGas required gas exceeds allowance: required={} allowance={}",
                        estimated,
                        gas_cap
                    );
                }
            }
            let from_balance = gateway_eth_balance_from_entries(&view_entries, &from);
            if from_balance < value {
                bail!(
                    "insufficient funds for eth_estimateGas value transfer: balance={} value={}",
                    from_balance,
                    value
                );
            }
            Ok((serde_json::Value::String(format!("0x{:x}", estimated)), false))
        }
        "eth_getCode" => {
            let chain_id = param_as_u64_any_with_tx(params, &["chain_id", "chainId"])
                .unwrap_or(ctx.eth_default_chain_id);
            if let Some(upstream) =
                maybe_gateway_eth_upstream_read(chain_id, "eth_getCode", params)?
            {
                return Ok((upstream, false));
            }
            let address_raw = extract_eth_persona_address_param(params)
                .ok_or_else(|| anyhow::anyhow!("address is required for eth_getCode"))?;
            let address = decode_hex_bytes(&address_raw, "address")?;
            let entries = collect_gateway_eth_chain_entries(
                eth_tx_index,
                ctx.eth_tx_index_store,
                chain_id,
                gateway_eth_query_scan_max(),
            )?;
            let latest =
                resolve_gateway_eth_latest_block_number(chain_id, &entries, ctx.eth_tx_index_store)?;
            let block_tag =
                parse_eth_tx_count_block_tag(params).unwrap_or_else(|| "latest".to_string());
            let Some(view_entries) =
                resolve_gateway_eth_get_proof_entries(chain_id, entries, &block_tag, latest)?
            else {
                return Ok((serde_json::Value::Null, false));
            };
            let code = gateway_eth_resolve_code_from_entries(&view_entries, &address)
                .unwrap_or_default();
            Ok((
                serde_json::Value::String(format!("0x{}", to_hex(&code))),
                false,
            ))
        }
        "eth_getStorageAt" => {
            let chain_id = param_as_u64_any_with_tx(params, &["chain_id", "chainId"])
                .unwrap_or(ctx.eth_default_chain_id);
            if let Some(upstream) =
                maybe_gateway_eth_upstream_read(chain_id, "eth_getStorageAt", params)?
            {
                return Ok((upstream, false));
            }
            let address_raw = extract_eth_persona_address_param(params)
                .ok_or_else(|| anyhow::anyhow!("address is required for eth_getStorageAt"))?;
            let address = decode_hex_bytes(&address_raw, "address")?;
            let slot_raw = extract_eth_storage_slot_param(params)
                .ok_or_else(|| anyhow::anyhow!("slot/position is required for eth_getStorageAt"))?;
            let Some(slot_key) = parse_storage_key_32(&slot_raw) else {
                bail!("invalid slot/position for eth_getStorageAt: {}", slot_raw);
            };
            let entries = collect_gateway_eth_chain_entries(
                eth_tx_index,
                ctx.eth_tx_index_store,
                chain_id,
                gateway_eth_query_scan_max(),
            )?;
            let latest =
                resolve_gateway_eth_latest_block_number(chain_id, &entries, ctx.eth_tx_index_store)?;
            let block_tag =
                parse_eth_get_proof_block_tag(params).unwrap_or_else(|| "latest".to_string());
            let Some(view_entries) =
                resolve_gateway_eth_get_proof_entries(chain_id, entries, &block_tag, latest)?
            else {
                return Ok((serde_json::Value::Null, false));
            };
            let storage = gateway_eth_resolve_storage_word_from_entries_by_key(
                &view_entries,
                &address,
                slot_key,
            )
            .unwrap_or([0u8; 32]);
            Ok((
                serde_json::Value::String(format!("0x{}", to_hex(&storage))),
                false,
            ))
        }
        "eth_getProof" => {
            let chain_id = param_as_u64_any_with_tx(params, &["chain_id", "chainId"])
                .unwrap_or(ctx.eth_default_chain_id);
            if let Some(upstream) =
                maybe_gateway_eth_upstream_read(chain_id, "eth_getProof", params)?
            {
                return Ok((upstream, false));
            }
            let address_raw = extract_eth_persona_address_param(params)
                .ok_or_else(|| anyhow::anyhow!("address is required for eth_getProof"))?;
            let address = decode_hex_bytes(&address_raw, "address")?;
            let storage_keys = parse_eth_get_proof_storage_keys(params)?;
            let all_entries = collect_gateway_eth_chain_entries(
                eth_tx_index,
                ctx.eth_tx_index_store,
                chain_id,
                gateway_eth_query_scan_max(),
            )?;
            let latest = resolve_gateway_eth_latest_block_number(
                chain_id,
                &all_entries,
                ctx.eth_tx_index_store,
            )?;
            let block_tag =
                parse_eth_get_proof_block_tag(params).unwrap_or_else(|| "latest".to_string());
            let Some(entries) =
                resolve_gateway_eth_get_proof_entries(chain_id, all_entries, &block_tag, latest)?
            else {
                return Ok((serde_json::Value::Null, false));
            };
            let account_view = gateway_eth_resolve_account_proof_view(&entries, &address);
            let mut requested_slots = Vec::<[u8; 32]>::with_capacity(storage_keys.len());
            for raw_key in storage_keys {
                let Some(slot) = parse_storage_key_32(&raw_key) else {
                    bail!("invalid storage key for eth_getProof: {}", raw_key);
                };
                requested_slots.push(slot);
            }
            let (storage_root, storage_items_with_proof) =
                gateway_eth_storage_proof_for_slots(&account_view.storage_items, &requested_slots);
            let account_proof = account_view
                .account_proof
                .iter()
                .map(|node| serde_json::Value::String(format!("0x{}", to_hex(node))))
                .collect::<Vec<serde_json::Value>>();
            let storage_proof = storage_items_with_proof
                .iter()
                .map(|(slot, value, proof_nodes)| {
                    let proof = proof_nodes
                        .iter()
                        .map(|node| serde_json::Value::String(format!("0x{}", to_hex(node))))
                        .collect::<Vec<serde_json::Value>>();
                    serde_json::json!({
                        "key": format!("0x{}", to_hex(slot)),
                        "value": format!("0x{}", to_hex(value)),
                        "proof": proof,
                    })
                })
                .collect::<Vec<serde_json::Value>>();

            Ok((
                serde_json::json!({
                    "address": format!("0x{}", to_hex(&address)),
                    "accountProof": account_proof,
                    "balance": format!("0x{:x}", account_view.balance),
                    "codeHash": format!("0x{}", to_hex(&account_view.code_hash)),
                    "nonce": format!("0x{:x}", account_view.nonce),
                    "storageHash": format!("0x{}", to_hex(&storage_root)),
                    "storageProof": storage_proof,
                }),
                false,
            ))
        }
        "evm_verifyProof" | "evm_verify_proof" => {
            let chain_id = param_as_u64_any_with_tx(params, &["chain_id", "chainId"])
                .unwrap_or(ctx.eth_default_chain_id);
            let address_raw = extract_eth_persona_address_param(params)
                .ok_or_else(|| anyhow::anyhow!("address is required for evm_verifyProof"))?;
            let storage_keys = parse_eth_get_proof_storage_keys(params)?;
            let block_tag =
                parse_eth_get_proof_block_tag(params).unwrap_or_else(|| "latest".to_string());
            let provided_proof = param_value_from_params(params, "proof")
                .or_else(|| param_value_from_params(params, "proof_obj"))
                .or_else(|| param_value_from_params(params, "proofObject"))
                .cloned()
                .ok_or_else(|| anyhow::anyhow!("proof/proof_obj/proofObject is required"))?;
            let address = decode_hex_bytes(&address_raw, "address")?;
            let all_entries = collect_gateway_eth_chain_entries(
                eth_tx_index,
                ctx.eth_tx_index_store,
                chain_id,
                gateway_eth_query_scan_max(),
            )?;
            let latest =
                resolve_gateway_eth_latest_block_number(chain_id, &all_entries, ctx.eth_tx_index_store)?;
            let Some(entries) =
                resolve_gateway_eth_get_proof_entries(chain_id, all_entries, &block_tag, latest)?
            else {
                return Ok((
                    serde_json::json!({
                        "valid": false,
                        "chain_id": format!("0x{:x}", chain_id),
                        "reason": "expected_proof_unavailable",
                        "mismatch_fields": ["proof"],
                    }),
                    false,
                ));
            };

            let account_view = gateway_eth_resolve_account_proof_view(&entries, &address);
            let expected_state_root = gateway_eth_state_root_from_entries(&entries);
            let expected_account_present = gateway_eth_account_exists_in_entries(&entries, &address);
            let mut requested_slots = Vec::<[u8; 32]>::with_capacity(storage_keys.len());
            for raw_key in storage_keys {
                let Some(slot) = parse_storage_key_32(&raw_key) else {
                    bail!("invalid storage key for evm_verifyProof: {}", raw_key);
                };
                requested_slots.push(slot);
            }
            let (expected_storage_root, expected_storage_with_proof) =
                gateway_eth_storage_proof_for_slots(&account_view.storage_items, &requested_slots);
            let expected_account_payload = gateway_eth_account_trie_payload_for_verify(
                account_view.nonce,
                account_view.balance,
                expected_storage_root,
                account_view.code_hash,
            );
            let expected_address_hex = format!("0x{}", to_hex(&address));
            let expected_code_hash_hex = format!("0x{}", to_hex(&account_view.code_hash));
            let expected_storage_root_hex = format!("0x{}", to_hex(&expected_storage_root));

            let Some(provided_obj) = provided_proof.as_object() else {
                return Ok((
                    serde_json::json!({
                        "valid": false,
                        "chain_id": format!("0x{:x}", chain_id),
                        "address": expected_address_hex,
                        "block_tag": block_tag,
                        "mismatch_fields": ["proof"],
                        "reason": "invalid_proof_format",
                        "expected_storage_hash": expected_storage_root_hex,
                        "expected_state_root": format!("0x{}", to_hex(&expected_state_root)),
                    }),
                    false,
                ));
            };

            let mut mismatch_fields = BTreeSet::<String>::new();
            let provided_address_ok = provided_obj
                .get("address")
                .and_then(serde_json::Value::as_str)
                .is_some_and(|v| {
                    decode_hex_bytes(v, "proof.address")
                        .ok()
                        .is_some_and(|decoded| decoded == address)
                });
            if !provided_address_ok {
                mismatch_fields.insert("address".to_string());
            }
            if provided_obj
                .get("balance")
                .and_then(value_to_u128)
                .is_none_or(|v| v != account_view.balance)
            {
                mismatch_fields.insert("balance".to_string());
            }
            if provided_obj
                .get("nonce")
                .and_then(value_to_u64)
                .is_none_or(|v| v != account_view.nonce)
            {
                mismatch_fields.insert("nonce".to_string());
            }
            let provided_code_hash_ok = provided_obj
                .get("codeHash")
                .and_then(serde_json::Value::as_str)
                .is_some_and(|v| {
                    decode_hex_bytes(v, "proof.codeHash")
                        .ok()
                        .is_some_and(|decoded| decoded == account_view.code_hash)
                });
            if !provided_code_hash_ok {
                mismatch_fields.insert("codeHash".to_string());
            }
            let provided_storage_hash_ok = provided_obj
                .get("storageHash")
                .and_then(serde_json::Value::as_str)
                .is_some_and(|v| {
                    decode_hex_bytes(v, "proof.storageHash")
                        .ok()
                        .is_some_and(|decoded| decoded == expected_storage_root)
                });
            if !provided_storage_hash_ok {
                mismatch_fields.insert("storageHash".to_string());
            }

            let account_proof_nodes = gateway_eth_parse_proof_nodes_for_verify(
                provided_obj.get("accountProof"),
            );
            let account_proof_valid = if expected_account_present {
                gateway_eth_verify_mpt_proof_value_for_verify(
                    &gateway_eth_keccak256_bytes_for_verify(&address),
                    expected_state_root,
                    Some(&expected_account_payload),
                    &account_proof_nodes,
                )
            } else {
                gateway_eth_verify_mpt_proof_value_for_verify(
                    &gateway_eth_keccak256_bytes_for_verify(&address),
                    expected_state_root,
                    None,
                    &account_proof_nodes,
                )
            };
            if !account_proof_valid {
                mismatch_fields.insert("accountProof".to_string());
            }

            let mut expected_storage_by_slot = BTreeMap::<[u8; 32], [u8; 32]>::new();
            for (slot, value, _) in &expected_storage_with_proof {
                expected_storage_by_slot.insert(*slot, *value);
            }
            let expected_storage_present = account_view
                .storage_items
                .iter()
                .map(|(slot, _)| *slot)
                .collect::<BTreeSet<[u8; 32]>>();
            let provided_storage = gateway_eth_parse_storage_proof_map_for_verify(
                provided_obj.get("storageProof"),
            );
            if requested_slots.is_empty() {
                if !provided_storage.is_empty() {
                    mismatch_fields.insert("storageProof".to_string());
                }
            } else {
                for slot in &requested_slots {
                    let expected_value = expected_storage_by_slot.get(slot).copied().unwrap_or([0u8; 32]);
                    let Some(provided_slot) = provided_storage.get(slot) else {
                        mismatch_fields.insert("storageProof".to_string());
                        continue;
                    };
                    if provided_slot.value != expected_value {
                        mismatch_fields.insert("storageProof".to_string());
                    }
                    let expected_payload =
                        gateway_eth_storage_trie_payload_for_verify(expected_value);
                    let key_hash = gateway_eth_keccak256_bytes_for_verify(slot);
                    let member_ok = gateway_eth_verify_mpt_proof_value_for_verify(
                        &key_hash,
                        expected_storage_root,
                        Some(&expected_payload),
                        &provided_slot.proof_nodes,
                    );
                    let proof_ok = if expected_storage_present.contains(slot) {
                        member_ok
                    } else {
                        member_ok
                            || gateway_eth_verify_mpt_proof_value_for_verify(
                                &key_hash,
                                expected_storage_root,
                                None,
                                &provided_slot.proof_nodes,
                            )
                    };
                    if !proof_ok {
                        mismatch_fields.insert("storageProof".to_string());
                    }
                }
            }

            let mismatch_fields = mismatch_fields.into_iter().collect::<Vec<String>>();
            let valid = mismatch_fields.is_empty();
            Ok((
                serde_json::json!({
                    "valid": valid,
                    "chain_id": format!("0x{:x}", chain_id),
                    "address": expected_address_hex,
                    "block_tag": block_tag,
                    "mismatch_fields": mismatch_fields,
                    "expected_storage_hash": expected_storage_root_hex,
                    "expected_state_root": format!("0x{}", to_hex(&expected_state_root)),
                    "expected_code_hash": expected_code_hash_hex,
                }),
                false,
            ))
        }
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
            let next_primary_key_ref =
                if let Some(raw) = param_as_string(params, "next_primary_key_ref") {
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
        "eth_getTransactionCount" => {
            let chain_id = param_as_u64_any_with_tx(params, &["chain_id", "chainId"])
                .unwrap_or(ctx.eth_default_chain_id);
            if let Some(upstream) =
                maybe_gateway_eth_upstream_read(chain_id, "eth_getTransactionCount", params)?
            {
                return Ok((upstream, false));
            }
            let address_raw = extract_eth_persona_address_param(params)
                .ok_or_else(|| anyhow::anyhow!("address (or from/external_address) is required"))?;
            let external_address = decode_hex_bytes(&address_raw, "address")?;
            let persona = PersonaAddress {
                persona_type: PersonaType::Evm,
                chain_id,
                external_address: external_address.clone(),
            };
            let explicit_uca_id = param_as_string(params, "uca_id");
            let binding_owner = router.resolve_binding_owner(&persona).map(str::to_string);
            if let (Some(explicit), Some(owner_id)) =
                (explicit_uca_id.as_ref(), binding_owner.as_ref())
            {
                if explicit != owner_id {
                    bail!(
                        "uca_id mismatch for address binding: explicit={} binding_owner={}",
                        explicit,
                        owner_id
                    );
                }
            }

            let entries = collect_gateway_eth_chain_entries(
                eth_tx_index,
                ctx.eth_tx_index_store,
                chain_id,
                gateway_eth_query_scan_max(),
            )?;
            let latest_block = resolve_gateway_eth_latest_block_number(
                chain_id,
                &entries,
                ctx.eth_tx_index_store,
            )?;
            let latest_nonce = entries
                .iter()
                .filter(|entry| entry.from == external_address)
                .map(|entry| entry.nonce.saturating_add(1))
                .max()
                .unwrap_or(0);
            let block_tag =
                parse_eth_tx_count_block_tag(params).unwrap_or_else(|| "latest".to_string());
            let normalized_tag = block_tag.trim().trim_matches('"');
            let nonce = if normalized_tag.eq_ignore_ascii_case("pending") {
                let pending_nonce_from_router = explicit_uca_id
                    .clone()
                    .or(binding_owner)
                    .and_then(|uca_id| router.next_nonce_for_persona(&uca_id, &persona).ok());
                let pending_nonce = pending_nonce_from_router
                    .map(|nonce| nonce.max(latest_nonce))
                    .unwrap_or(latest_nonce);
                let pending_nonce_from_runtime =
                    gateway_eth_pending_nonce_from_runtime(chain_id, &external_address);
                pending_nonce_from_runtime
                    .map(|nonce| nonce.max(pending_nonce))
                    .unwrap_or(pending_nonce)
            } else if normalized_tag.eq_ignore_ascii_case("earliest") {
                0
            } else if normalized_tag.is_empty()
                || normalized_tag.eq_ignore_ascii_case("latest")
                || normalized_tag.eq_ignore_ascii_case("safe")
                || normalized_tag.eq_ignore_ascii_case("finalized")
            {
                latest_nonce
            } else if let Some(block_number) = parse_u64_decimal_or_hex(normalized_tag) {
                // Historical block-tag view: evaluate sender nonce against entries up to block number.
                // Align with other state-read methods: future block number returns null.
                if block_number > latest_block {
                    return Ok((serde_json::Value::Null, false));
                }
                let oldest_block = entries.iter().map(|entry| entry.nonce).min().unwrap_or(latest_block);
                let likely_scan_truncated = !entries.is_empty()
                    && entries.len() >= gateway_eth_query_scan_max()
                    && oldest_block > 0;
                // If requested historical block is older than current query window, return null
                // instead of guessing with incomplete in-memory sample.
                if likely_scan_truncated && block_number < oldest_block {
                    return Ok((serde_json::Value::Null, false));
                }
                entries
                    .iter()
                    .filter(|entry| entry.from == external_address && entry.nonce <= block_number)
                    .map(|entry| entry.nonce.saturating_add(1))
                    .max()
                    .unwrap_or(0)
            } else {
                bail!("invalid block number/tag: {}", block_tag);
            };
            Ok((serde_json::Value::String(format!("0x{:x}", nonce)), false))
        }
        "eth_getTransactionByHash" => {
            let chain_hint = param_as_u64_any_with_tx(params, &["chain_id", "chainId"])
                .or(Some(ctx.eth_default_chain_id));
            if let Some(chain_id) = chain_hint {
                if let Some(upstream) =
                    maybe_gateway_eth_upstream_read(chain_id, "eth_getTransactionByHash", params)?
                {
                    return Ok((upstream, false));
                }
            }
            let tx_hash_raw = extract_eth_tx_hash_query_param(params)
                .ok_or_else(|| anyhow::anyhow!("tx_hash (or hash) is required"))?;
            let tx_hash_bytes = decode_hex_bytes(&tx_hash_raw, "tx_hash")?;
            let tx_hash = vec_to_32(&tx_hash_bytes, "tx_hash")?;
            if let Some(entry) = eth_tx_index.get(&tx_hash) {
                if chain_hint.is_some_and(|chain_id| entry.chain_id != chain_id) {
                    return Ok((serde_json::Value::Null, false));
                }
                Ok((
                    gateway_eth_tx_by_hash_query_json(entry, eth_tx_index, ctx.eth_tx_index_store)?,
                    false,
                ))
            } else if let Ok(Some(entry)) = ctx.eth_tx_index_store.load_eth_tx(&tx_hash) {
                if chain_hint.is_some_and(|chain_id| entry.chain_id != chain_id) {
                    return Ok((serde_json::Value::Null, false));
                }
                let response =
                    gateway_eth_tx_by_hash_query_json(&entry, eth_tx_index, ctx.eth_tx_index_store)?;
                eth_tx_index.insert(tx_hash, entry);
                Ok((response, false))
            } else if let Some(tx) = find_gateway_eth_runtime_tx_by_hash(tx_hash, chain_hint) {
                let pending_entry = gateway_eth_tx_index_entry_from_ir(tx);
                let chain_entries = collect_gateway_eth_chain_entries(
                    eth_tx_index,
                    ctx.eth_tx_index_store,
                    pending_entry.chain_id,
                    gateway_eth_query_scan_max(),
                )?;
                let latest_block_number = resolve_gateway_eth_latest_block_number(
                    pending_entry.chain_id,
                    &chain_entries,
                    ctx.eth_tx_index_store,
                )?;
                if let Some((pending_block_number, pending_block_hash, pending_entries)) =
                    resolve_gateway_eth_pending_block_for_runtime_view(
                        pending_entry.chain_id,
                        latest_block_number,
                        eth_tx_index,
                        ctx.eth_tx_index_store,
                    )?
                {
                    if let Some(tx_index) =
                        pending_entries.iter().position(|entry| entry.tx_hash == tx_hash)
                    {
                        return Ok((
                            gateway_eth_tx_pending_with_block_json(
                                &pending_entries[tx_index],
                                pending_block_number,
                                tx_index,
                                &pending_block_hash,
                            ),
                            false,
                        ));
                    }
                }
                Ok((gateway_eth_tx_by_hash_json(&pending_entry), false))
            } else {
                Ok((serde_json::Value::Null, false))
            }
        }
        "eth_getTransactionReceipt" => {
            let chain_hint = param_as_u64_any_with_tx(params, &["chain_id", "chainId"])
                .or(Some(ctx.eth_default_chain_id));
            if let Some(chain_id) = chain_hint {
                if let Some(upstream) =
                    maybe_gateway_eth_upstream_read(chain_id, "eth_getTransactionReceipt", params)?
                {
                    return Ok((upstream, false));
                }
            }
            let tx_hash_raw = extract_eth_tx_hash_query_param(params)
                .ok_or_else(|| anyhow::anyhow!("tx_hash (or hash) is required"))?;
            let tx_hash_bytes = decode_hex_bytes(&tx_hash_raw, "tx_hash")?;
            let tx_hash = vec_to_32(&tx_hash_bytes, "tx_hash")?;
            if let Some(entry) = eth_tx_index.get(&tx_hash) {
                if chain_hint.is_some_and(|chain_id| entry.chain_id != chain_id) {
                    return Ok((serde_json::Value::Null, false));
                }
                Ok((
                    gateway_eth_tx_receipt_query_json(
                        entry,
                        eth_tx_index,
                        ctx.eth_tx_index_store,
                    )?,
                    false,
                ))
            } else if let Ok(Some(entry)) = ctx.eth_tx_index_store.load_eth_tx(&tx_hash) {
                if chain_hint.is_some_and(|chain_id| entry.chain_id != chain_id) {
                    return Ok((serde_json::Value::Null, false));
                }
                let response =
                    gateway_eth_tx_receipt_query_json(&entry, eth_tx_index, ctx.eth_tx_index_store)?;
                eth_tx_index.insert(tx_hash, entry);
                Ok((response, false))
            } else if let Some(tx) = find_gateway_eth_runtime_tx_by_hash(
                tx_hash,
                chain_hint,
            ) {
                let pending_entry = gateway_eth_tx_index_entry_from_ir(tx);
                let chain_entries = collect_gateway_eth_chain_entries(
                    eth_tx_index,
                    ctx.eth_tx_index_store,
                    pending_entry.chain_id,
                    gateway_eth_query_scan_max(),
                )?;
                let latest_block_number = resolve_gateway_eth_latest_block_number(
                    pending_entry.chain_id,
                    &chain_entries,
                    ctx.eth_tx_index_store,
                )?;
                if let Some((pending_block_number, pending_block_hash, pending_entries)) =
                    resolve_gateway_eth_pending_block_for_runtime_view(
                        pending_entry.chain_id,
                        latest_block_number,
                        eth_tx_index,
                        ctx.eth_tx_index_store,
                    )?
                {
                    if let Some(receipt) = gateway_eth_tx_receipt_pending_query_json_by_hash(
                        pending_block_number,
                        &pending_block_hash,
                        &pending_entries,
                        &tx_hash,
                    ) {
                        return Ok((
                            receipt,
                            false,
                        ));
                    }
                }
                Ok((gateway_eth_tx_receipt_json(&pending_entry), false))
            } else {
                Ok((serde_json::Value::Null, false))
            }
        }
        "evm_getLogsBatch" | "evm_get_logs_batch" => {
            let queries = extract_gateway_batch_items(params, &["queries", "filters", "items"]);
            if queries.is_empty() {
                bail!("queries (or filters/items) is required");
            }
            let chain_id = param_as_u64_any_with_tx(params, &["chain_id", "chainId"]);
            let mut indexed_queries: Vec<(usize, serde_json::Value)> = Vec::with_capacity(queries.len());
            for (idx, query) in queries.into_iter().enumerate() {
                let forwarded = if let Some(chain_id) = chain_id {
                    match query {
                        serde_json::Value::Object(mut map) => {
                            if !map.contains_key("chain_id") && !map.contains_key("chainId") {
                                map.insert("chain_id".to_string(), serde_json::json!(chain_id));
                            }
                            serde_json::Value::Object(map)
                        }
                        other => serde_json::json!([
                            {
                                "chain_id": chain_id
                            },
                            other
                        ]),
                    }
                } else {
                    query
                };
                indexed_queries.push((idx, forwarded));
            }
            let workers = gateway_batch_worker_count(indexed_queries.len());
            let eth_tx_index_snapshot = eth_tx_index.clone();
            let mut ordered_items: Vec<(usize, serde_json::Value)> =
                Vec::with_capacity(indexed_queries.len());
            if workers <= 1 {
                for (idx, query) in indexed_queries {
                    let item = gateway_eth_logs_item_json(
                        &eth_tx_index_snapshot,
                        ctx.eth_tx_index_store,
                        ctx.eth_default_chain_id,
                        &query,
                    )?;
                    ordered_items.push((idx, item));
                }
            } else {
                let chunk_size = indexed_queries.len().div_ceil(workers).max(1);
                let mut chunk_results: Vec<Vec<(usize, serde_json::Value)>> = Vec::new();
                std::thread::scope(|scope| -> Result<()> {
                    let mut handles = Vec::new();
                    for chunk in indexed_queries.chunks(chunk_size) {
                        let chunk_items = chunk.to_vec();
                        let snapshot = &eth_tx_index_snapshot;
                        let store = ctx.eth_tx_index_store;
                        let default_chain_id = ctx.eth_default_chain_id;
                        handles.push(scope.spawn(move || {
                            let mut local = Vec::with_capacity(chunk_items.len());
                            for (idx, query) in chunk_items {
                                let item =
                                    gateway_eth_logs_item_json(snapshot, store, default_chain_id, &query)?;
                                local.push((idx, item));
                            }
                            Ok::<_, anyhow::Error>(local)
                        }));
                    }
                    for handle in handles {
                        let local = handle
                            .join()
                            .map_err(|_| anyhow::anyhow!("gateway logs batch worker thread panicked"))??;
                        chunk_results.push(local);
                    }
                    Ok(())
                })?;
                for chunk in chunk_results {
                    for item in chunk {
                        ordered_items.push(item);
                    }
                }
            }
            ordered_items.sort_by_key(|(idx, _)| *idx);
            let out: Vec<serde_json::Value> = ordered_items
                .into_iter()
                .map(|(_, item)| item)
                .collect();
            Ok((serde_json::Value::Array(out), false))
        }
        "evm_getFilterChangesBatch" | "evm_get_filter_changes_batch" => {
            let filters = extract_gateway_batch_items(
                params,
                &["filter_ids", "filterIds", "subscriptions", "ids", "items"],
            );
            if filters.is_empty() {
                bail!("filter_ids (or filterIds/subscriptions/ids/items) is required");
            }
            let mut out = Vec::with_capacity(filters.len());
            for filter in filters {
                let forwarded = match filter {
                    serde_json::Value::Object(map) => serde_json::Value::Object(map),
                    other => serde_json::json!({ "filter_id": other }),
                };
                let filter_id = parse_eth_filter_id(&forwarded)
                    .ok_or_else(|| anyhow::anyhow!("filter_id (or id) is required"))?;
                let item = gateway_eth_filter_changes_item_json(
                    eth_tx_index,
                    ctx.eth_tx_index_store,
                    ctx.eth_filters,
                    filter_id,
                )?;
                out.push(item);
            }
            Ok((serde_json::Value::Array(out), false))
        }
        "evm_getFilterLogsBatch" | "evm_get_filter_logs_batch" => {
            let filters = extract_gateway_batch_items(
                params,
                &["filter_ids", "filterIds", "subscriptions", "ids", "items"],
            );
            if filters.is_empty() {
                bail!("filter_ids (or filterIds/subscriptions/ids/items) is required");
            }
            let mut indexed_filters: Vec<(usize, serde_json::Value)> =
                Vec::with_capacity(filters.len());
            for (idx, filter) in filters.into_iter().enumerate() {
                let forwarded = match filter {
                    serde_json::Value::Object(map) => serde_json::Value::Object(map),
                    other => serde_json::json!({ "filter_id": other }),
                };
                indexed_filters.push((idx, forwarded));
            }
            let workers = gateway_batch_worker_count(indexed_filters.len());
            let eth_tx_index_snapshot = eth_tx_index.clone();
            let filters_snapshot = ctx.eth_filters.filters.clone();
            let mut ordered_items: Vec<(usize, serde_json::Value)> =
                Vec::with_capacity(indexed_filters.len());
            if workers <= 1 {
                for (idx, forwarded) in indexed_filters {
                    let item = gateway_eth_filter_logs_item_json(
                        &eth_tx_index_snapshot,
                        ctx.eth_tx_index_store,
                        &filters_snapshot,
                        &forwarded,
                    )?;
                    ordered_items.push((idx, item));
                }
            } else {
                let chunk_size = indexed_filters.len().div_ceil(workers).max(1);
                let mut chunk_results: Vec<Vec<(usize, serde_json::Value)>> = Vec::new();
                std::thread::scope(|scope| -> Result<()> {
                    let mut handles = Vec::new();
                    for chunk in indexed_filters.chunks(chunk_size) {
                        let chunk_items = chunk.to_vec();
                        let snapshot = &eth_tx_index_snapshot;
                        let store = ctx.eth_tx_index_store;
                        let filters_map = &filters_snapshot;
                        handles.push(scope.spawn(move || {
                            let mut local = Vec::with_capacity(chunk_items.len());
                            for (idx, forwarded) in chunk_items {
                                let item = gateway_eth_filter_logs_item_json(
                                    snapshot,
                                    store,
                                    filters_map,
                                    &forwarded,
                                )?;
                                local.push((idx, item));
                            }
                            Ok::<_, anyhow::Error>(local)
                        }));
                    }
                    for handle in handles {
                        let local = handle
                            .join()
                            .map_err(|_| anyhow::anyhow!("gateway filter-logs batch worker thread panicked"))??;
                        chunk_results.push(local);
                    }
                    Ok(())
                })?;
                for chunk in chunk_results {
                    for item in chunk {
                        ordered_items.push(item);
                    }
                }
            }
            ordered_items.sort_by_key(|(idx, _)| *idx);
            let out: Vec<serde_json::Value> = ordered_items
                .into_iter()
                .map(|(_, item)| item)
                .collect();
            Ok((serde_json::Value::Array(out), false))
        }
        "evm_publicSendRawTransactionBatch" | "evm_public_send_raw_transaction_batch" => {
            let items = extract_gateway_batch_items(
                params,
                &["raw_txs", "rawTxs", "txs", "transactions", "items"],
            );
            if items.is_empty() {
                bail!("raw_txs (or rawTxs/txs/transactions/items) is required");
            }
            let chain_id = param_as_u64_any_with_tx(params, &["chain_id", "chainId"]);
            let mut accepted = 0usize;
            let mut rejected = 0usize;
            let mut results = Vec::with_capacity(items.len());
            for (idx, item) in items.into_iter().enumerate() {
                let mut forwarded = match item {
                    serde_json::Value::Object(map) => serde_json::Value::Object(map),
                    other => serde_json::json!({ "raw_tx": other }),
                };
                if let Some(chain_id) = chain_id {
                    if let Some(map) = forwarded.as_object_mut() {
                        if !map.contains_key("chain_id") && !map.contains_key("chainId") {
                            map.insert("chain_id".to_string(), serde_json::json!(chain_id));
                        }
                    }
                }
                force_evm_send_public_broadcast_detail(&mut forwarded);
                match run_gateway_method(
                    router,
                    eth_tx_index,
                    evm_settlement_index_by_id,
                    evm_settlement_index_by_tx,
                    evm_pending_payout_by_settlement,
                    ctx,
                    "eth_sendRawTransaction",
                    &forwarded,
                ) {
                    Ok((result, _)) => {
                        accepted = accepted.saturating_add(1);
                        results.push(serde_json::json!({
                            "index": idx,
                            "ok": true,
                            "result": result,
                        }));
                    }
                    Err(err) => {
                        rejected = rejected.saturating_add(1);
                        let raw_message = err.to_string();
                        let code =
                            gateway_error_code_for_method("eth_sendRawTransaction", &raw_message);
                        let message = gateway_error_message_for_method(
                            "eth_sendRawTransaction",
                            code,
                            &raw_message,
                        );
                        let data =
                            gateway_error_data_for_method("eth_sendRawTransaction", code, &raw_message);
                        persist_gateway_eth_submit_failure_status_from_error(
                            ctx.eth_tx_index_store,
                            Some(router),
                            "evm_publicSendRawTransaction",
                            &forwarded,
                            &raw_message,
                            code,
                            &message,
                            ctx.eth_default_chain_id,
                        );
                        results.push(serde_json::json!({
                            "index": idx,
                            "ok": false,
                            "error_code": code,
                            "error_message": message,
                            "error_data": data,
                        }));
                    }
                }
            }
            Ok((
                serde_json::json!({
                    "total": accepted.saturating_add(rejected),
                    "accepted": accepted,
                    "rejected": rejected,
                    "results": results,
                }),
                false,
            ))
        }
        "evm_publicSendTransactionBatch" | "evm_public_send_transaction_batch" => {
            let items = extract_gateway_batch_items(params, &["txs", "transactions", "items"]);
            if items.is_empty() {
                bail!("txs (or transactions/items) is required");
            }
            let chain_id = param_as_u64_any_with_tx(params, &["chain_id", "chainId"]);
            let mut accepted = 0usize;
            let mut rejected = 0usize;
            let mut results = Vec::with_capacity(items.len());
            for (idx, item) in items.into_iter().enumerate() {
                let mut forwarded = match item {
                    serde_json::Value::Object(map) => serde_json::Value::Object(map),
                    other => serde_json::json!({ "tx": other }),
                };
                if let Some(chain_id) = chain_id {
                    if let Some(map) = forwarded.as_object_mut() {
                        if !map.contains_key("chain_id") && !map.contains_key("chainId") {
                            map.insert("chain_id".to_string(), serde_json::json!(chain_id));
                        }
                    }
                }
                force_evm_send_public_broadcast_detail(&mut forwarded);
                match run_gateway_method(
                    router,
                    eth_tx_index,
                    evm_settlement_index_by_id,
                    evm_settlement_index_by_tx,
                    evm_pending_payout_by_settlement,
                    ctx,
                    "eth_sendTransaction",
                    &forwarded,
                ) {
                    Ok((result, _)) => {
                        accepted = accepted.saturating_add(1);
                        results.push(serde_json::json!({
                            "index": idx,
                            "ok": true,
                            "result": result,
                        }));
                    }
                    Err(err) => {
                        rejected = rejected.saturating_add(1);
                        let raw_message = err.to_string();
                        let code =
                            gateway_error_code_for_method("eth_sendTransaction", &raw_message);
                        let message = gateway_error_message_for_method(
                            "eth_sendTransaction",
                            code,
                            &raw_message,
                        );
                        let data =
                            gateway_error_data_for_method("eth_sendTransaction", code, &raw_message);
                        persist_gateway_eth_submit_failure_status_from_error(
                            ctx.eth_tx_index_store,
                            Some(router),
                            "evm_publicSendTransaction",
                            &forwarded,
                            &raw_message,
                            code,
                            &message,
                            ctx.eth_default_chain_id,
                        );
                        results.push(serde_json::json!({
                            "index": idx,
                            "ok": false,
                            "error_code": code,
                            "error_message": message,
                            "error_data": data,
                        }));
                    }
                }
            }
            Ok((
                serde_json::json!({
                    "total": accepted.saturating_add(rejected),
                    "accepted": accepted,
                    "rejected": rejected,
                    "results": results,
                }),
                false,
            ))
        }
        "evm_getPublicBroadcastStatus"
        | "evm_get_public_broadcast_status"
        | "evm_getBroadcastStatus"
        | "evm_get_broadcast_status" => {
            let tx_hash_raw = extract_eth_tx_hash_query_param(params)
                .or_else(|| param_as_string(params, "txHash"))
                .ok_or_else(|| anyhow::anyhow!("tx_hash (or txHash/hash) is required"))?;
            let tx_hash_bytes = decode_hex_bytes(&tx_hash_raw, "tx_hash")?;
            let tx_hash = vec_to_32(&tx_hash_bytes, "tx_hash")?;
            let chain_hint = param_as_u64_any_with_tx(params, &["chain_id", "chainId"])
                .or(Some(ctx.eth_default_chain_id));

            let mut chain_id: Option<u64> = None;
            let mut known = false;
            if let Some(entry) = eth_tx_index.get(&tx_hash) {
                chain_id = Some(entry.chain_id);
                known = true;
            } else if let Ok(Some(entry)) = ctx.eth_tx_index_store.load_eth_tx(&tx_hash) {
                chain_id = Some(entry.chain_id);
                known = true;
                eth_tx_index.insert(tx_hash, entry);
            } else if let Some(tx) = find_gateway_eth_runtime_tx_by_hash(tx_hash, chain_hint) {
                let runtime_entry = gateway_eth_tx_index_entry_from_ir(tx);
                chain_id = Some(runtime_entry.chain_id);
                known = true;
            }
            if let (Some(chain_id), Some(chain_hint)) = (chain_id, chain_hint) {
                if chain_id != chain_hint {
                    return Ok((
                        serde_json::json!({
                            "known": false,
                            "tx_hash": format!("0x{}", to_hex(&tx_hash)),
                            "chain_id": format!("0x{:x}", chain_id),
                            "has_status": false,
                            "broadcast": serde_json::Value::Null,
                        }),
                        false,
                    ));
                }
            }
            let broadcast = gateway_eth_broadcast_status_json_by_tx(ctx.eth_tx_index_store, &tx_hash);
            Ok((
                serde_json::json!({
                    "known": known,
                    "tx_hash": format!("0x{}", to_hex(&tx_hash)),
                    "chain_id": chain_id.map(|v| format!("0x{:x}", v)),
                    "has_status": !broadcast.is_null(),
                    "broadcast": broadcast,
                }),
                false,
            ))
        }
        "evm_getPublicBroadcastStatusBatch"
        | "evm_get_public_broadcast_status_batch"
        | "evm_getBroadcastStatusBatch"
        | "evm_get_broadcast_status_batch" => {
            let tx_hashes = extract_eth_tx_hashes_query_params(params);
            if tx_hashes.is_empty() {
                bail!("tx_hashes (or txHashes/hashes/txs) is required");
            }
            let chain_hint = param_as_u64_any_with_tx(params, &["chain_id", "chainId"])
                .or(Some(ctx.eth_default_chain_id));
            let mut decoded_hashes = Vec::with_capacity(tx_hashes.len());
            for tx_hash in tx_hashes {
                let tx_hash_bytes = decode_hex_bytes(&tx_hash, "tx_hash")?;
                decoded_hashes.push(vec_to_32(&tx_hash_bytes, "tx_hash")?);
            }
            let workers = gateway_batch_worker_count(decoded_hashes.len());
            let mut out = Vec::with_capacity(decoded_hashes.len());
            if workers <= 1 || decoded_hashes.len() <= 1 {
                for tx_hash in decoded_hashes {
                    out.push(gateway_eth_public_broadcast_status_item_json(
                        eth_tx_index,
                        ctx.eth_tx_index_store,
                        tx_hash,
                        chain_hint,
                    ));
                }
            } else {
                let chunk_size = decoded_hashes.len().div_ceil(workers);
                let chunks = std::thread::scope(|scope| -> Result<Vec<Vec<serde_json::Value>>> {
                    let mut jobs = Vec::with_capacity(workers);
                    for chunk in decoded_hashes.chunks(chunk_size) {
                        let hashes = chunk.to_vec();
                        let eth_tx_index = &*eth_tx_index;
                        let eth_tx_index_store = ctx.eth_tx_index_store;
                        jobs.push(scope.spawn(move || {
                            let mut partial = Vec::with_capacity(hashes.len());
                            for tx_hash in hashes {
                                partial.push(gateway_eth_public_broadcast_status_item_json(
                                    eth_tx_index,
                                    eth_tx_index_store,
                                    tx_hash,
                                    chain_hint,
                                ));
                            }
                            partial
                        }));
                    }
                    let mut out = Vec::with_capacity(jobs.len());
                    for job in jobs {
                        let partial = job.join().map_err(|_| {
                            anyhow::anyhow!(
                                "gateway public-broadcast status batch worker thread panicked"
                            )
                        })?;
                        out.push(partial);
                    }
                    Ok(out)
                })?;
                for partial in chunks {
                    out.extend(partial);
                }
            }
            Ok((serde_json::Value::Array(out), false))
        }
        "evm_getPublicBroadcastCapability"
        | "evm_get_public_broadcast_capability"
        | "evm_getBroadcastCapability"
        | "evm_get_broadcast_capability" => {
            let chain_id = param_as_u64_any_with_tx(params, &["chain_id", "chainId"])
                .unwrap_or(ctx.eth_default_chain_id);
            Ok((gateway_eth_public_broadcast_capability_json(chain_id), false))
        }
        "evm_getPublicBroadcastPluginPeers"
        | "evm_get_public_broadcast_plugin_peers"
        | "evm_getPluginPeers"
        | "evm_get_plugin_peers" => {
            let chain_id = param_as_u64_any_with_tx(params, &["chain_id", "chainId"])
                .unwrap_or(ctx.eth_default_chain_id);
            Ok((gateway_eth_public_broadcast_plugin_peers_json(chain_id), false))
        }
        "evm_reportPublicBroadcastPluginSession"
        | "evm_report_public_broadcast_plugin_session"
        | "evm_reportPluginSession"
        | "evm_report_plugin_session" => {
            let chain_id = param_as_u64_any_with_tx(params, &["chain_id", "chainId"])
                .unwrap_or(ctx.eth_default_chain_id);
            Ok((
                gateway_eth_public_broadcast_ingest_plugin_session_report(chain_id, params)?,
                false,
            ))
        }
        "evm_getUpstreamSnapshot"
        | "evm_get_upstream_snapshot"
        | "evm_getRuntimeSnapshot"
        | "evm_get_runtime_snapshot" => {
            let chain_id = param_as_u64_any_with_tx(params, &["chain_id", "chainId"])
                .unwrap_or(ctx.eth_default_chain_id);
            Ok((
                gateway_evm_upstream_snapshot_json(
                    eth_tx_index,
                    ctx.eth_tx_index_store,
                    ctx.eth_default_chain_id,
                    chain_id,
                    params,
                )?,
                false,
            ))
        }
        "evm_getUpstreamRuntimeBundle" | "evm_get_upstream_runtime_bundle" => {
            let chain_id = param_as_u64_any_with_tx(params, &["chain_id", "chainId"])
                .unwrap_or(ctx.eth_default_chain_id);
            let max_items =
                param_as_u64(params, "max_items")
                    .or_else(|| param_as_u64(params, "maxItems"))
                    .or_else(|| param_as_u64(params, "max"))
                    .unwrap_or(256)
                    .clamp(1, 4096);
            let include_ingress = param_as_bool(params, "include_ingress")
                .or_else(|| param_as_bool(params, "includeIngress"))
                .unwrap_or(true);
            Ok((
                gateway_evm_upstream_runtime_bundle_json(
                    eth_tx_index,
                    ctx.eth_tx_index_store,
                    ctx.eth_default_chain_id,
                    chain_id,
                    max_items,
                    include_ingress,
                    params,
                )?,
                false,
            ))
        }
        "evm_getUpstreamTxStatusBundle" | "evm_get_upstream_tx_status_bundle" => {
            let chain_id = param_as_u64_any_with_tx(params, &["chain_id", "chainId"])
                .unwrap_or(ctx.eth_default_chain_id);
            let max_items =
                param_as_u64(params, "max_items")
                    .or_else(|| param_as_u64(params, "maxItems"))
                    .or_else(|| param_as_u64(params, "max"))
                    .unwrap_or(256)
                    .clamp(1, 4096) as usize;
            let auto_from_pending = param_as_bool(params, "auto_from_pending")
                .or_else(|| param_as_bool(params, "autoFromPending"))
                .or_else(|| param_as_bool(params, "from_pending"))
                .or_else(|| param_as_bool(params, "fromPending"))
                .unwrap_or(true);
            let include_lifecycle = param_as_bool(params, "include_lifecycle")
                .or_else(|| param_as_bool(params, "includeLifecycle"))
                .unwrap_or(true);
            let include_receipts = param_as_bool(params, "include_receipts")
                .or_else(|| param_as_bool(params, "includeReceipts"))
                .unwrap_or(true);
            let include_broadcast = param_as_bool(params, "include_broadcast")
                .or_else(|| param_as_bool(params, "includeBroadcast"))
                .unwrap_or(true);
            Ok((
                gateway_evm_upstream_tx_status_bundle_json(
                    eth_tx_index,
                    ctx.eth_tx_index_store,
                    GatewayEvmUpstreamTxStatusBundleOptions {
                        chain_id,
                        max_items,
                        auto_from_pending,
                        include_lifecycle,
                        include_receipts,
                        include_broadcast,
                    },
                    params,
                )?,
                false,
            ))
        }
        "evm_getUpstreamEventBundle" | "evm_get_upstream_event_bundle" => {
            let chain_id = param_as_u64_any_with_tx(params, &["chain_id", "chainId"])
                .unwrap_or(ctx.eth_default_chain_id);
            let include_logs = param_as_bool(params, "include_logs")
                .or_else(|| param_as_bool(params, "includeLogs"))
                .unwrap_or(true);
            let include_filter_changes = param_as_bool(params, "include_filter_changes")
                .or_else(|| param_as_bool(params, "includeFilterChanges"))
                .unwrap_or(true);
            let include_filter_logs = param_as_bool(params, "include_filter_logs")
                .or_else(|| param_as_bool(params, "includeFilterLogs"))
                .unwrap_or(false);
            Ok((
                gateway_evm_upstream_event_bundle_json(
                    eth_tx_index,
                    ctx.eth_tx_index_store,
                    ctx.eth_filters,
                    GatewayEvmUpstreamEventBundleOptions {
                        chain_id,
                        eth_default_chain_id: ctx.eth_default_chain_id,
                        include_logs,
                        include_filter_changes,
                        include_filter_logs,
                    },
                    params,
                )?,
                false,
            ))
        }
        "evm_getUpstreamFullBundle" | "evm_get_upstream_full_bundle" => {
            let chain_id = param_as_u64_any_with_tx(params, &["chain_id", "chainId"])
                .unwrap_or(ctx.eth_default_chain_id);
            let max_items =
                param_as_u64(params, "max_items")
                    .or_else(|| param_as_u64(params, "maxItems"))
                    .or_else(|| param_as_u64(params, "max"))
                    .unwrap_or(256)
                    .clamp(1, 4096);
            let include_ingress = param_as_bool(params, "include_ingress")
                .or_else(|| param_as_bool(params, "includeIngress"))
                .unwrap_or(true);
            let include_logs = param_as_bool(params, "include_logs")
                .or_else(|| param_as_bool(params, "includeLogs"))
                .unwrap_or(true);
            let include_filter_changes = param_as_bool(params, "include_filter_changes")
                .or_else(|| param_as_bool(params, "includeFilterChanges"))
                .unwrap_or(false);
            let include_filter_logs = param_as_bool(params, "include_filter_logs")
                .or_else(|| param_as_bool(params, "includeFilterLogs"))
                .unwrap_or(false);
            let include_lifecycle = param_as_bool(params, "include_lifecycle")
                .or_else(|| param_as_bool(params, "includeLifecycle"))
                .unwrap_or(true);
            let include_receipts = param_as_bool(params, "include_receipts")
                .or_else(|| param_as_bool(params, "includeReceipts"))
                .unwrap_or(true);
            let include_broadcast = param_as_bool(params, "include_broadcast")
                .or_else(|| param_as_bool(params, "includeBroadcast"))
                .unwrap_or(true);
            let auto_from_pending = param_as_bool(params, "auto_from_pending")
                .or_else(|| param_as_bool(params, "autoFromPending"))
                .or_else(|| param_as_bool(params, "from_pending"))
                .or_else(|| param_as_bool(params, "fromPending"))
                .unwrap_or(true);

            Ok((
                gateway_evm_upstream_full_bundle_json(
                    eth_tx_index,
                    ctx.eth_tx_index_store,
                    ctx.eth_default_chain_id,
                    ctx.eth_filters,
                    GatewayEvmUpstreamFullBundleOptions {
                        chain_id,
                        max_items,
                        include_ingress,
                        include_logs,
                        include_filter_changes,
                        include_filter_logs,
                        include_lifecycle,
                        include_receipts,
                        include_broadcast,
                        auto_from_pending,
                    },
                    params,
                )?,
                false,
            ))
        }
        "evm_getUpstreamConsumerBundle" | "evm_get_upstream_consumer_bundle" => {
            let chain_id = param_as_u64_any_with_tx(params, &["chain_id", "chainId"])
                .unwrap_or(ctx.eth_default_chain_id);
            let max_items =
                param_as_u64(params, "max_items")
                    .or_else(|| param_as_u64(params, "maxItems"))
                    .or_else(|| param_as_u64(params, "max"))
                    .unwrap_or(256)
                    .clamp(1, 4096);

            let mut forwarded = params.clone();
            if let Some(obj) = forwarded.as_object_mut() {
                obj.insert("chain_id".to_owned(), serde_json::Value::from(chain_id));
                obj.insert("max_items".to_owned(), serde_json::Value::from(max_items));
                obj.insert("include_ingress".to_owned(), serde_json::Value::from(true));
                obj.insert("include_logs".to_owned(), serde_json::Value::from(true));
                obj.insert(
                    "include_filter_changes".to_owned(),
                    serde_json::Value::from(true),
                );
                obj.insert("include_filter_logs".to_owned(), serde_json::Value::from(false));
                obj.insert("include_lifecycle".to_owned(), serde_json::Value::from(true));
                obj.insert("include_receipts".to_owned(), serde_json::Value::from(true));
                obj.insert("include_broadcast".to_owned(), serde_json::Value::from(true));
                obj.insert("auto_from_pending".to_owned(), serde_json::Value::from(true));
            }

            let full_bundle = gateway_evm_upstream_full_bundle_json(
                eth_tx_index,
                ctx.eth_tx_index_store,
                ctx.eth_default_chain_id,
                ctx.eth_filters,
                GatewayEvmUpstreamFullBundleOptions {
                    chain_id,
                    max_items,
                    include_ingress: true,
                    include_logs: true,
                    include_filter_changes: true,
                    include_filter_logs: false,
                    include_lifecycle: true,
                    include_receipts: true,
                    include_broadcast: true,
                    auto_from_pending: true,
                },
                &forwarded,
            )?;

            let public_broadcast_ready = full_bundle
                .get("runtime_status")
                .and_then(|v| v.get("public_broadcast"))
                .and_then(|v| v.get("ready"))
                .and_then(serde_json::Value::as_bool)
                .unwrap_or(false);
            let tx_status_items = full_bundle
                .get("tx_status_bundle")
                .and_then(|v| v.get("items"))
                .and_then(serde_json::Value::as_array)
                .cloned()
                .unwrap_or_default();
            let tx_status_total = tx_status_items.len();
            let lifecycle_resolved = tx_status_items
                .iter()
                .filter(|item| item.get("lifecycle").is_some_and(|v| !v.is_null()))
                .count();
            let receipts_resolved = tx_status_items
                .iter()
                .filter(|item| item.get("receipt").is_some_and(|v| !v.is_null()))
                .count();
            let broadcast_resolved = tx_status_items
                .iter()
                .filter(|item| item.get("broadcast").is_some_and(|v| !v.is_null()))
                .count();
            let stage_resolved = tx_status_items
                .iter()
                .filter(|item| {
                    item.get("stage")
                        .and_then(serde_json::Value::as_str)
                        .is_some_and(|stage| stage != "unknown")
                })
                .count();
            let unresolved_lifecycle_tx_hashes: Vec<serde_json::Value> = tx_status_items
                .iter()
                .filter(|item| item.get("lifecycle").is_none_or(|v| v.is_null()))
                .filter_map(|item| {
                    item.get("tx_hash")
                        .or_else(|| item.get("txHash"))
                        .or_else(|| item.get("hash"))
                        .cloned()
                })
                .collect();
            let unresolved_receipt_tx_hashes: Vec<serde_json::Value> = tx_status_items
                .iter()
                .filter(|item| item.get("receipt").is_none_or(|v| v.is_null()))
                .filter_map(|item| {
                    item.get("tx_hash")
                        .or_else(|| item.get("txHash"))
                        .or_else(|| item.get("hash"))
                        .cloned()
                })
                .collect();
            let unresolved_broadcast_tx_hashes: Vec<serde_json::Value> = tx_status_items
                .iter()
                .filter(|item| item.get("broadcast").is_none_or(|v| v.is_null()))
                .filter_map(|item| {
                    item.get("tx_hash")
                        .or_else(|| item.get("txHash"))
                        .or_else(|| item.get("hash"))
                        .cloned()
                })
                .collect();
            let unresolved_stage_tx_hashes: Vec<serde_json::Value> = tx_status_items
                .iter()
                .filter(|item| {
                    item.get("stage")
                        .and_then(serde_json::Value::as_str)
                        .is_none_or(|stage| stage == "unknown")
                })
                .filter_map(|item| {
                    item.get("tx_hash")
                        .or_else(|| item.get("txHash"))
                        .or_else(|| item.get("hash"))
                        .cloned()
                })
                .collect();
            let logs = full_bundle
                .get("event_bundle")
                .and_then(|v| v.get("logs"))
                .and_then(serde_json::Value::as_array)
                .cloned()
                .unwrap_or_default();
            let filter_changes = full_bundle
                .get("event_bundle")
                .and_then(|v| v.get("filter_changes"))
                .and_then(serde_json::Value::as_array)
                .cloned()
                .unwrap_or_default();
            let filter_logs = full_bundle
                .get("event_bundle")
                .and_then(|v| v.get("filter_logs"))
                .and_then(serde_json::Value::as_array)
                .cloned()
                .unwrap_or_default();
            let logs_count = logs.len();
            let filter_changes_count = filter_changes.len();
            let tx_status_ready = tx_status_total == 0
                || (unresolved_lifecycle_tx_hashes.is_empty()
                    && unresolved_receipt_tx_hashes.is_empty()
                    && unresolved_broadcast_tx_hashes.is_empty()
                    && unresolved_stage_tx_hashes.is_empty());
            let events_ready = full_bundle
                .get("event_bundle")
                .and_then(serde_json::Value::as_object)
                .is_some_and(|event| {
                    event
                        .get("logs")
                        .and_then(serde_json::Value::as_array)
                        .is_some()
                        && event
                            .get("filter_changes")
                        .and_then(serde_json::Value::as_array)
                        .is_some()
                });
            let consumer_ready = public_broadcast_ready && tx_status_ready && events_ready;
            let public_broadcast = full_bundle
                .get("runtime_status")
                .and_then(|v| v.get("public_broadcast"))
                .cloned()
                .unwrap_or(serde_json::Value::Null);

            Ok((
                serde_json::json!({
                    "chain_id": format!("0x{:x}", chain_id),
                    "ready": consumer_ready,
                    "ready_details": {
                        "public_broadcast_ready": public_broadcast_ready,
                        "tx_status_ready": tx_status_ready,
                        "tx_status_stage_ready": unresolved_stage_tx_hashes.is_empty(),
                        "events_ready": events_ready,
                    },
                    "counts": {
                        "tx_status_items": tx_status_total,
                        "lifecycle_resolved": lifecycle_resolved,
                        "receipts_resolved": receipts_resolved,
                        "broadcast_resolved": broadcast_resolved,
                        "stage_resolved": stage_resolved,
                        "lifecycle_unresolved": unresolved_lifecycle_tx_hashes.len(),
                        "receipts_unresolved": unresolved_receipt_tx_hashes.len(),
                        "broadcast_unresolved": unresolved_broadcast_tx_hashes.len(),
                        "stage_unresolved": unresolved_stage_tx_hashes.len(),
                        "logs": logs_count,
                        "filter_changes": filter_changes_count,
                    },
                    "unresolved": {
                        "lifecycle_tx_hashes": unresolved_lifecycle_tx_hashes,
                        "receipt_tx_hashes": unresolved_receipt_tx_hashes,
                        "broadcast_tx_hashes": unresolved_broadcast_tx_hashes,
                        "stage_tx_hashes": unresolved_stage_tx_hashes,
                    },
                    "consumer": {
                        "public_broadcast": public_broadcast,
                        "tx_status": tx_status_items,
                        "events": {
                            "logs": logs,
                            "filter_changes": filter_changes,
                            "filter_logs": filter_logs,
                        }
                    },
                    "bundle": full_bundle,
                }),
                false,
            ))
        }
        "evm_getTransactionLifecycleBatch"
        | "evm_get_transaction_lifecycle_batch"
        | "evm_getTxSubmitStatusBatch"
        | "evm_get_tx_submit_status_batch" => {
            let tx_hashes = extract_eth_tx_hashes_query_params(params);
            if tx_hashes.is_empty() {
                bail!("tx_hashes (or txHashes/hashes/txs) is required");
            }
            let chain_hint = param_as_u64_any_with_tx(params, &["chain_id", "chainId"])
                .or(Some(ctx.eth_default_chain_id));
            let mut decoded_hashes = Vec::with_capacity(tx_hashes.len());
            for (idx, tx_hash) in tx_hashes.into_iter().enumerate() {
                let tx_hash_bytes = decode_hex_bytes(&tx_hash, &format!("tx_hashes[{idx}]"))?;
                decoded_hashes.push(vec_to_32(&tx_hash_bytes, &format!("tx_hashes[{idx}]"))?);
            }
            let indexed_hashes: Vec<(usize, [u8; 32])> =
                decoded_hashes.into_iter().enumerate().collect();
            let workers = gateway_batch_worker_count(indexed_hashes.len());
            let eth_tx_index_snapshot = eth_tx_index.clone();
            let mut cache_updates: Vec<GatewayEthTxIndexEntry> = Vec::new();
            let mut ordered_items: Vec<(usize, serde_json::Value)> =
                Vec::with_capacity(indexed_hashes.len());
            if workers <= 1 {
                for (idx, tx_hash) in indexed_hashes {
                    let (item, cache_entry) = gateway_eth_tx_lifecycle_item_json(
                        &eth_tx_index_snapshot,
                        ctx.eth_tx_index_store,
                        tx_hash,
                        chain_hint,
                    )?;
                    if let Some(entry) = cache_entry {
                        cache_updates.push(entry);
                    }
                    ordered_items.push((idx, item));
                }
            } else {
                let chunk_size = indexed_hashes.len().div_ceil(workers).max(1);
                let mut chunk_results: Vec<
                    Vec<(usize, serde_json::Value, Option<GatewayEthTxIndexEntry>)>,
                > = Vec::new();
                std::thread::scope(|scope| -> Result<()> {
                    let mut handles = Vec::new();
                    for chunk in indexed_hashes.chunks(chunk_size) {
                        let chunk_items = chunk.to_vec();
                        let snapshot = &eth_tx_index_snapshot;
                        let store = ctx.eth_tx_index_store;
                        handles.push(scope.spawn(move || {
                            let mut local = Vec::with_capacity(chunk_items.len());
                            for (idx, tx_hash) in chunk_items {
                                let (item, cache_entry) = gateway_eth_tx_lifecycle_item_json(
                                    snapshot, store, tx_hash, chain_hint,
                                )?;
                                local.push((idx, item, cache_entry));
                            }
                            Ok::<_, anyhow::Error>(local)
                        }));
                    }
                    for handle in handles {
                        let local = handle
                            .join()
                            .map_err(|_| anyhow::anyhow!("gateway tx-lifecycle batch worker thread panicked"))??;
                        chunk_results.push(local);
                    }
                    Ok(())
                })?;
                for chunk in chunk_results {
                    for (idx, item, cache_entry) in chunk {
                        if let Some(entry) = cache_entry {
                            cache_updates.push(entry);
                        }
                        ordered_items.push((idx, item));
                    }
                }
            }
            ordered_items.sort_by_key(|(idx, _)| *idx);
            for entry in cache_updates {
                eth_tx_index.insert(entry.tx_hash, entry);
            }
            let out: Vec<serde_json::Value> =
                ordered_items.into_iter().map(|(_, item)| item).collect();
            Ok((serde_json::Value::Array(out), false))
        }
        "evm_replayPublicBroadcast" | "evm_replay_public_broadcast" => {
            let tx_hash_raw = extract_eth_tx_hash_query_param(params)
                .or_else(|| param_as_string(params, "txHash"))
                .ok_or_else(|| anyhow::anyhow!("tx_hash (or txHash/hash) is required"))?;
            let chain_hint = param_as_u64_any_with_tx(params, &["chain_id", "chainId"]);
            let eth_tx_index_snapshot = eth_tx_index.clone();
            let (item, cache_entry) = gateway_eth_replay_public_broadcast_item_json(
                &eth_tx_index_snapshot,
                ctx.eth_tx_index_store,
                &tx_hash_raw,
                chain_hint,
            )?;
            if let Some(entry) = cache_entry {
                eth_tx_index.insert(entry.tx_hash, entry);
            }
            Ok((item, false))
        }
        "evm_replayPublicBroadcastBatch" | "evm_replay_public_broadcast_batch" => {
            let tx_hashes = extract_eth_tx_hashes_query_params(params);
            if tx_hashes.is_empty() {
                bail!("tx_hashes (or txHashes/hashes/txs) is required");
            }
            let chain_hint = param_as_u64_any_with_tx(params, &["chain_id", "chainId"]);
            let indexed_hashes: Vec<(usize, String)> = tx_hashes.into_iter().enumerate().collect();
            let workers = gateway_batch_worker_count(indexed_hashes.len());
            let eth_tx_index_snapshot = eth_tx_index.clone();
            let mut ordered_items: Vec<(usize, serde_json::Value, bool)> =
                Vec::with_capacity(indexed_hashes.len());
            let mut cache_updates: Vec<GatewayEthTxIndexEntry> = Vec::new();
            if workers <= 1 {
                for (idx, tx_hash_raw) in indexed_hashes {
                    match gateway_eth_replay_public_broadcast_item_json(
                        &eth_tx_index_snapshot,
                        ctx.eth_tx_index_store,
                        &tx_hash_raw,
                        chain_hint,
                    ) {
                        Ok((item, cache_entry)) => {
                            if let Some(entry) = cache_entry {
                                cache_updates.push(entry);
                            }
                            ordered_items.push((idx, item, false));
                        }
                        Err(e) => {
                            ordered_items.push((
                                idx,
                                serde_json::json!({
                                    "replayed": false,
                                    "tx_hash": tx_hash_raw,
                                    "error": e.to_string(),
                                }),
                                true,
                            ));
                        }
                    }
                }
            } else {
                let chunk_size = indexed_hashes.len().div_ceil(workers).max(1);
                type ReplayBatchItem =
                    (usize, serde_json::Value, bool, Option<GatewayEthTxIndexEntry>);
                let mut chunk_results: Vec<Vec<ReplayBatchItem>> = Vec::new();
                std::thread::scope(|scope| -> Result<()> {
                    let mut handles = Vec::new();
                    for chunk in indexed_hashes.chunks(chunk_size) {
                        let chunk_items = chunk.to_vec();
                        let snapshot = &eth_tx_index_snapshot;
                        let store = ctx.eth_tx_index_store;
                        handles.push(scope.spawn(move || {
                            let mut local = Vec::with_capacity(chunk_items.len());
                            for (idx, tx_hash_raw) in chunk_items {
                                match gateway_eth_replay_public_broadcast_item_json(
                                    snapshot,
                                    store,
                                    &tx_hash_raw,
                                    chain_hint,
                                ) {
                                    Ok((item, cache_entry)) => {
                                        local.push((idx, item, false, cache_entry));
                                    }
                                    Err(e) => {
                                        local.push((
                                            idx,
                                            serde_json::json!({
                                                "replayed": false,
                                                "tx_hash": tx_hash_raw,
                                                "error": e.to_string(),
                                            }),
                                            true,
                                            None,
                                        ));
                                    }
                                }
                            }
                            Ok::<_, anyhow::Error>(local)
                        }));
                    }
                    for handle in handles {
                        let local = handle
                            .join()
                            .map_err(|_| anyhow::anyhow!("gateway replay-public-broadcast batch worker thread panicked"))??;
                        chunk_results.push(local);
                    }
                    Ok(())
                })?;
                for chunk in chunk_results {
                    for (idx, item, is_failed, cache_entry) in chunk {
                        if let Some(entry) = cache_entry {
                            cache_updates.push(entry);
                        }
                        ordered_items.push((idx, item, is_failed));
                    }
                }
            }
            ordered_items.sort_by_key(|(idx, _, _)| *idx);
            for entry in cache_updates {
                eth_tx_index.insert(entry.tx_hash, entry);
            }
            let mut replayed = 0usize;
            let mut failed = 0usize;
            let mut results = Vec::with_capacity(ordered_items.len());
            for (_, item, is_failed) in ordered_items {
                if is_failed {
                    failed = failed.saturating_add(1);
                } else {
                    replayed = replayed.saturating_add(1);
                }
                results.push(item);
            }
            Ok((
                serde_json::json!({
                    "total": replayed.saturating_add(failed),
                    "replayed": replayed,
                    "failed": failed,
                    "results": results,
                }),
                false,
            ))
        }
        "evm_getTransactionReceiptBatch" | "evm_get_transaction_receipt_batch" => {
            let tx_hashes = extract_eth_tx_hashes_query_params(params);
            if tx_hashes.is_empty() {
                bail!("tx_hashes (or txHashes/hashes/txs) is required");
            }
            let chain_hint = param_as_u64_any_with_tx(params, &["chain_id", "chainId"])
                .or(Some(ctx.eth_default_chain_id));
            let mut decoded_hashes = Vec::with_capacity(tx_hashes.len());
            for (idx, tx_hash) in tx_hashes.into_iter().enumerate() {
                let tx_hash_bytes = decode_hex_bytes(&tx_hash, &format!("tx_hashes[{idx}]"))?;
                decoded_hashes.push(vec_to_32(&tx_hash_bytes, &format!("tx_hashes[{idx}]"))?);
            }
            let indexed_hashes: Vec<(usize, [u8; 32])> =
                decoded_hashes.into_iter().enumerate().collect();
            let workers = gateway_batch_worker_count(indexed_hashes.len());
            let eth_tx_index_snapshot = eth_tx_index.clone();
            let mut cache_updates: Vec<GatewayEthTxIndexEntry> = Vec::new();
            let mut ordered_items: Vec<(usize, serde_json::Value)> =
                Vec::with_capacity(indexed_hashes.len());
            if workers <= 1 {
                for (idx, tx_hash) in indexed_hashes {
                    let (item, cache_entry) = gateway_eth_tx_receipt_item_json(
                        &eth_tx_index_snapshot,
                        ctx.eth_tx_index_store,
                        tx_hash,
                        chain_hint,
                    )?;
                    if let Some(entry) = cache_entry {
                        cache_updates.push(entry);
                    }
                    ordered_items.push((idx, item));
                }
            } else {
                let chunk_size = indexed_hashes.len().div_ceil(workers).max(1);
                let mut chunk_results: Vec<Vec<(usize, serde_json::Value, Option<GatewayEthTxIndexEntry>)>> =
                    Vec::new();
                std::thread::scope(|scope| -> Result<()> {
                    let mut handles = Vec::new();
                    for chunk in indexed_hashes.chunks(chunk_size) {
                        let chunk_items = chunk.to_vec();
                        let snapshot = &eth_tx_index_snapshot;
                        let store = ctx.eth_tx_index_store;
                        handles.push(scope.spawn(move || {
                            let mut local = Vec::with_capacity(chunk_items.len());
                            for (idx, tx_hash) in chunk_items {
                                let (item, cache_entry) =
                                    gateway_eth_tx_receipt_item_json(
                                        snapshot, store, tx_hash, chain_hint,
                                    )?;
                                local.push((idx, item, cache_entry));
                            }
                            Ok::<_, anyhow::Error>(local)
                        }));
                    }
                    for handle in handles {
                        let local = handle
                            .join()
                            .map_err(|_| anyhow::anyhow!("gateway receipt batch worker thread panicked"))??;
                        chunk_results.push(local);
                    }
                    Ok(())
                })?;
                for chunk in chunk_results {
                    for (idx, item, cache_entry) in chunk {
                        if let Some(entry) = cache_entry {
                            cache_updates.push(entry);
                        }
                        ordered_items.push((idx, item));
                    }
                }
            }
            ordered_items.sort_by_key(|(idx, _)| *idx);
            for entry in cache_updates {
                eth_tx_index.entry(entry.tx_hash).or_insert(entry);
            }
            let out: Vec<serde_json::Value> = ordered_items
                .into_iter()
                .map(|(_, item)| item)
                .collect();
            Ok((serde_json::Value::Array(out), false))
        }
        "evm_getTransactionByHashBatch" | "evm_get_transaction_by_hash_batch" => {
            let tx_hashes = extract_eth_tx_hashes_query_params(params);
            if tx_hashes.is_empty() {
                bail!("tx_hashes (or txHashes/hashes/txs) is required");
            }
            let chain_hint = param_as_u64_any_with_tx(params, &["chain_id", "chainId"])
                .or(Some(ctx.eth_default_chain_id));
            let mut decoded_hashes = Vec::with_capacity(tx_hashes.len());
            for (idx, tx_hash) in tx_hashes.into_iter().enumerate() {
                let tx_hash_bytes = decode_hex_bytes(&tx_hash, &format!("tx_hashes[{idx}]"))?;
                decoded_hashes.push(vec_to_32(&tx_hash_bytes, &format!("tx_hashes[{idx}]"))?);
            }
            let indexed_hashes: Vec<(usize, [u8; 32])> =
                decoded_hashes.into_iter().enumerate().collect();
            let workers = gateway_batch_worker_count(indexed_hashes.len());
            let eth_tx_index_snapshot = eth_tx_index.clone();
            let mut cache_updates: Vec<GatewayEthTxIndexEntry> = Vec::new();
            let mut ordered_items: Vec<(usize, serde_json::Value)> =
                Vec::with_capacity(indexed_hashes.len());
            if workers <= 1 {
                for (idx, tx_hash) in indexed_hashes {
                    let (item, cache_entry) = gateway_eth_tx_by_hash_item_json(
                        &eth_tx_index_snapshot,
                        ctx.eth_tx_index_store,
                        tx_hash,
                        chain_hint,
                    )?;
                    if let Some(entry) = cache_entry {
                        cache_updates.push(entry);
                    }
                    ordered_items.push((idx, item));
                }
            } else {
                let chunk_size = indexed_hashes.len().div_ceil(workers).max(1);
                let mut chunk_results: Vec<Vec<(usize, serde_json::Value, Option<GatewayEthTxIndexEntry>)>> =
                    Vec::new();
                std::thread::scope(|scope| -> Result<()> {
                    let mut handles = Vec::new();
                    for chunk in indexed_hashes.chunks(chunk_size) {
                        let chunk_items = chunk.to_vec();
                        let snapshot = &eth_tx_index_snapshot;
                        let store = ctx.eth_tx_index_store;
                        handles.push(scope.spawn(move || {
                            let mut local = Vec::with_capacity(chunk_items.len());
                            for (idx, tx_hash) in chunk_items {
                                let (item, cache_entry) = gateway_eth_tx_by_hash_item_json(
                                    snapshot, store, tx_hash, chain_hint,
                                )?;
                                local.push((idx, item, cache_entry));
                            }
                            Ok::<_, anyhow::Error>(local)
                        }));
                    }
                    for handle in handles {
                        let local = handle
                            .join()
                            .map_err(|_| anyhow::anyhow!("gateway tx-by-hash batch worker thread panicked"))??;
                        chunk_results.push(local);
                    }
                    Ok(())
                })?;
                for chunk in chunk_results {
                    for (idx, item, cache_entry) in chunk {
                        if let Some(entry) = cache_entry {
                            cache_updates.push(entry);
                        }
                        ordered_items.push((idx, item));
                    }
                }
            }
            ordered_items.sort_by_key(|(idx, _)| *idx);
            for entry in cache_updates {
                eth_tx_index.entry(entry.tx_hash).or_insert(entry);
            }
            let out: Vec<serde_json::Value> = ordered_items
                .into_iter()
                .map(|(_, item)| item)
                .collect();
            Ok((serde_json::Value::Array(out), false))
        }
        "evm_getTransactionLifecycle"
        | "evm_get_transaction_lifecycle"
        | "evm_getTxSubmitStatus"
        | "evm_get_tx_submit_status" => {
            let tx_hash_raw = extract_eth_tx_hash_query_param(params)
                .or_else(|| param_as_string(params, "txHash"))
                .ok_or_else(|| anyhow::anyhow!("tx_hash (or txHash/hash) is required"))?;
            let tx_hash_bytes = decode_hex_bytes(&tx_hash_raw, "tx_hash")?;
            let tx_hash = vec_to_32(&tx_hash_bytes, "tx_hash")?;
            let chain_hint = param_as_u64_any_with_tx(params, &["chain_id", "chainId"])
                .or(Some(ctx.eth_default_chain_id));
            let eth_tx_index_snapshot = eth_tx_index.clone();
            let (item, cache_entry) = gateway_eth_tx_lifecycle_item_json(
                &eth_tx_index_snapshot,
                ctx.eth_tx_index_store,
                tx_hash,
                chain_hint,
            )?;
            if let Some(entry) = cache_entry {
                eth_tx_index.insert(entry.tx_hash, entry);
            }
            Ok((item, false))
        }
        "evm_getSettlementById" | "evm_get_settlement_by_id" => {
            let settlement_id = param_as_string(params, "settlement_id")
                .or_else(|| param_as_string(params, "settlementId"))
                .ok_or_else(|| anyhow::anyhow!("settlement_id (or settlementId) is required"))?;
            if let Some(entry) = evm_settlement_index_by_id.get(&settlement_id) {
                Ok((gateway_evm_settlement_json(entry), false))
            } else if let Ok(Some(entry)) = ctx
                .eth_tx_index_store
                .load_evm_settlement_by_id(&settlement_id)
            {
                let tx_key = GatewaySettlementTxKey {
                    chain_id: entry.chain_id,
                    tx_hash: entry.income_tx_hash,
                };
                evm_settlement_index_by_tx.insert(tx_key, entry.settlement_id.clone());
                evm_settlement_index_by_id.insert(entry.settlement_id.clone(), entry.clone());
                Ok((gateway_evm_settlement_json(&entry), false))
            } else {
                Ok((serde_json::Value::Null, false))
            }
        }
        "evm_getSettlementByTxHash" | "evm_get_settlement_by_tx_hash" => {
            let chain_id = param_as_u64(params, "chain_id")
                .or_else(|| param_as_u64(params, "chainId"))
                .unwrap_or(ctx.eth_default_chain_id);
            let tx_hash_raw = extract_eth_tx_hash_query_param(params)
                .or_else(|| param_as_string(params, "txHash"))
                .ok_or_else(|| anyhow::anyhow!("tx_hash (or txHash/hash) is required"))?;
            let tx_hash_bytes = decode_hex_bytes(&tx_hash_raw, "tx_hash")?;
            let tx_hash = vec_to_32(&tx_hash_bytes, "tx_hash")?;
            let tx_key = GatewaySettlementTxKey { chain_id, tx_hash };
            if let Some(settlement_id) = evm_settlement_index_by_tx.get(&tx_key) {
                if let Some(entry) = evm_settlement_index_by_id.get(settlement_id) {
                    return Ok((gateway_evm_settlement_json(entry), false));
                }
            }
            if let Ok(Some(entry)) = ctx
                .eth_tx_index_store
                .load_evm_settlement_by_tx_hash(chain_id, &tx_hash)
            {
                let tx_key = GatewaySettlementTxKey {
                    chain_id: entry.chain_id,
                    tx_hash: entry.income_tx_hash,
                };
                evm_settlement_index_by_tx.insert(tx_key, entry.settlement_id.clone());
                evm_settlement_index_by_id.insert(entry.settlement_id.clone(), entry.clone());
                Ok((gateway_evm_settlement_json(&entry), false))
            } else {
                Ok((serde_json::Value::Null, false))
            }
        }
        "evm_replaySettlementPayout" | "evm_replay_settlement_payout" => {
            let settlement_id = param_as_string(params, "settlement_id")
                .or_else(|| param_as_string(params, "settlementId"))
                .ok_or_else(|| anyhow::anyhow!("settlement_id (or settlementId) is required"))?;
            let pending = if let Some(in_mem) = evm_pending_payout_by_settlement.get(&settlement_id)
            {
                Some(in_mem.clone())
            } else if let Ok(Some(from_store)) = ctx
                .eth_tx_index_store
                .load_pending_payout_instruction(&settlement_id)
            {
                evm_pending_payout_by_settlement.insert(settlement_id.clone(), from_store.clone());
                Some(from_store)
            } else {
                None
            };
            let Some(instruction) = pending else {
                return Ok((serde_json::Value::Null, false));
            };
            persist_gateway_evm_payout_instructions(
                ctx.spool_dir,
                std::slice::from_ref(&instruction),
            )?;
            clear_gateway_pending_payout(
                evm_pending_payout_by_settlement,
                ctx.eth_tx_index_store,
                &instruction.settlement_id,
            );
            set_gateway_evm_settlement_status(
                evm_settlement_index_by_id,
                ctx.eth_tx_index_store,
                &instruction.settlement_id,
                EVM_SETTLEMENT_STATUS_COMPENSATED_V1,
            );
            Ok((
                serde_json::json!({
                    "settlement_id": instruction.settlement_id,
                    "replayed": true,
                    "chain_id": format!("0x{:x}", instruction.chain_id),
                    "income_tx_hash": format!("0x{}", to_hex(&instruction.income_tx_hash)),
                }),
                false,
            ))
        }
        "evm_getAtomicReadyByIntentId" | "evm_get_atomic_ready_by_intent_id" => {
            let intent_id = param_as_string(params, "intent_id")
                .or_else(|| param_as_string(params, "intentId"))
                .ok_or_else(|| anyhow::anyhow!("intent_id (or intentId) is required"))?;
            if let Ok(Some(entry)) = ctx
                .eth_tx_index_store
                .load_evm_atomic_ready_by_intent(&intent_id)
            {
                Ok((gateway_evm_atomic_ready_json(&entry), false))
            } else {
                Ok((serde_json::Value::Null, false))
            }
        }
        "evm_replayAtomicReady" | "evm_replay_atomic_ready" => {
            let intent_id = param_as_string(params, "intent_id")
                .or_else(|| param_as_string(params, "intentId"))
                .ok_or_else(|| anyhow::anyhow!("intent_id (or intentId) is required"))?;
            let Some(item) = ctx.eth_tx_index_store.load_pending_atomic_ready(&intent_id)? else {
                return Ok((serde_json::Value::Null, false));
            };
            persist_gateway_evm_atomic_ready(ctx.spool_dir, std::slice::from_ref(&item))?;
            clear_gateway_pending_atomic_ready(ctx.eth_tx_index_store, &intent_id);
            upsert_gateway_evm_atomic_ready_index(
                ctx.eth_tx_index_store,
                &item,
                EVM_ATOMIC_READY_STATUS_COMPENSATED_V1,
                None,
                None,
            );
            Ok((
                serde_json::json!({
                    "intent_id": intent_id,
                    "replayed": true,
                }),
                false,
            ))
        }
        "evm_queueAtomicBroadcast" | "evm_queue_atomic_broadcast" => {
            let intent_id = param_as_string(params, "intent_id")
                .or_else(|| param_as_string(params, "intentId"))
                .ok_or_else(|| anyhow::anyhow!("intent_id (or intentId) is required"))?;
            let Some(entry) = ctx
                .eth_tx_index_store
                .load_evm_atomic_ready_by_intent(&intent_id)?
            else {
                return Ok((serde_json::Value::Null, false));
            };
            let ticket = atomic_broadcast_ticket_from_index_entry(&entry);
            let _ = load_atomic_broadcast_tx_ir_bincode_from_pending_ready(
                ctx.eth_tx_index_store,
                &ticket.intent_id,
                ticket.chain_id,
                &ticket.tx_hash,
            );
            match persist_gateway_evm_atomic_broadcast_queue(
                ctx.spool_dir,
                std::slice::from_ref(&ticket),
            ) {
                Ok(spool_file) => {
                    mark_gateway_pending_atomic_broadcast_ticket(ctx.eth_tx_index_store, &ticket);
                    let _ = set_gateway_evm_atomic_ready_status(
                        ctx.eth_tx_index_store,
                        &intent_id,
                        EVM_ATOMIC_READY_STATUS_BROADCAST_QUEUED_V1,
                    );
                    Ok((
                        serde_json::json!({
                            "intent_id": intent_id,
                            "queued": true,
                            "chain_id": format!("0x{:x}", ticket.chain_id),
                            "tx_hash": format!("0x{}", to_hex(&ticket.tx_hash)),
                            "spool_file": spool_file.display().to_string(),
                        }),
                        false,
                    ))
                }
                Err(e) => {
                    mark_gateway_pending_atomic_broadcast_ticket(ctx.eth_tx_index_store, &ticket);
                    let _ = set_gateway_evm_atomic_ready_status(
                        ctx.eth_tx_index_store,
                        &intent_id,
                        EVM_ATOMIC_READY_STATUS_BROADCAST_FAILED_V1,
                    );
                    bail!(
                        "queue atomic broadcast failed: intent_id={} chain_id={} tx_hash=0x{} err={}",
                        intent_id,
                        ticket.chain_id,
                        to_hex(&ticket.tx_hash),
                        e
                    );
                }
            }
        }
        "evm_replayAtomicBroadcastQueue" | "evm_replay_atomic_broadcast_queue" => {
            let intent_id = param_as_string(params, "intent_id")
                .or_else(|| param_as_string(params, "intentId"))
                .ok_or_else(|| anyhow::anyhow!("intent_id (or intentId) is required"))?;
            let Some(ticket) = ctx
                .eth_tx_index_store
                .load_pending_atomic_broadcast_ticket(&intent_id)?
            else {
                return Ok((serde_json::Value::Null, false));
            };
            let spool_file = persist_gateway_evm_atomic_broadcast_queue(
                ctx.spool_dir,
                std::slice::from_ref(&ticket),
            )?;
            mark_gateway_pending_atomic_broadcast_ticket(ctx.eth_tx_index_store, &ticket);
            let _ = set_gateway_evm_atomic_ready_status(
                ctx.eth_tx_index_store,
                &intent_id,
                EVM_ATOMIC_READY_STATUS_BROADCAST_QUEUED_V1,
            );
            Ok((
                serde_json::json!({
                    "intent_id": intent_id,
                    "replayed": true,
                    "spool_file": spool_file.display().to_string(),
                }),
                false,
            ))
        }
        "evm_markAtomicBroadcastFailed" | "evm_mark_atomic_broadcast_failed" => {
            let intent_id = param_as_string(params, "intent_id")
                .or_else(|| param_as_string(params, "intentId"))
                .ok_or_else(|| anyhow::anyhow!("intent_id (or intentId) is required"))?;
            let Some(entry) = ctx
                .eth_tx_index_store
                .load_evm_atomic_ready_by_intent(&intent_id)?
            else {
                return Ok((serde_json::Value::Null, false));
            };
            let ticket = atomic_broadcast_ticket_from_index_entry(&entry);
            let _ = load_atomic_broadcast_tx_ir_bincode_from_pending_ready(
                ctx.eth_tx_index_store,
                &ticket.intent_id,
                ticket.chain_id,
                &ticket.tx_hash,
            );
            mark_gateway_pending_atomic_broadcast_ticket(ctx.eth_tx_index_store, &ticket);
            let _ = set_gateway_evm_atomic_ready_status(
                ctx.eth_tx_index_store,
                &intent_id,
                EVM_ATOMIC_READY_STATUS_BROADCAST_FAILED_V1,
            );
            Ok((
                serde_json::json!({
                    "intent_id": intent_id,
                    "failed": true,
                    "chain_id": format!("0x{:x}", ticket.chain_id),
                    "tx_hash": format!("0x{}", to_hex(&ticket.tx_hash)),
                }),
                false,
            ))
        }
        "evm_markAtomicBroadcasted" | "evm_mark_atomic_broadcasted" => {
            let intent_id = param_as_string(params, "intent_id")
                .or_else(|| param_as_string(params, "intentId"))
                .ok_or_else(|| anyhow::anyhow!("intent_id (or intentId) is required"))?;
            let updated = set_gateway_evm_atomic_ready_status(
                ctx.eth_tx_index_store,
                &intent_id,
                EVM_ATOMIC_READY_STATUS_BROADCASTED_V1,
            );
            if !updated {
                return Ok((serde_json::Value::Null, false));
            }
            clear_gateway_pending_atomic_broadcast_ticket(ctx.eth_tx_index_store, &intent_id);
            clear_gateway_pending_atomic_broadcast_payload(ctx.eth_tx_index_store, &intent_id);
            Ok((
                serde_json::json!({
                    "intent_id": intent_id,
                    "broadcasted": true,
                }),
                false,
            ))
        }
        "evm_executeAtomicBroadcast" | "evm_execute_atomic_broadcast" => {
            let intent_id = param_as_string(params, "intent_id")
                .or_else(|| param_as_string(params, "intentId"))
                .ok_or_else(|| anyhow::anyhow!("intent_id (or intentId) is required"))?;
            let force_native = gateway_atomic_broadcast_force_native(params);
            let use_external_executor = !force_native
                && gateway_atomic_broadcast_use_external_executor(params);
            let retry_override = param_as_u64(params, "retry")
                .or_else(|| param_as_u64(params, "max_retry"))
                .map(|v| v.min(16));
            let timeout_ms_override = param_as_u64(params, "timeout_ms")
                .or_else(|| param_as_u64(params, "exec_timeout_ms"))
                .map(|v| v.min(300_000));
            let retry_backoff_ms_override = param_as_u64(params, "retry_backoff_ms")
                .or_else(|| param_as_u64(params, "backoff_ms"))
                .map(|v| v.min(10_000));
            let ticket = if let Some(ticket) = ctx
                .eth_tx_index_store
                .load_pending_atomic_broadcast_ticket(&intent_id)?
            {
                ticket
            } else {
                let Some(entry) = ctx
                    .eth_tx_index_store
                    .load_evm_atomic_ready_by_intent(&intent_id)?
                else {
                    return Ok((serde_json::Value::Null, false));
                };
                atomic_broadcast_ticket_from_index_entry(&entry)
            };
            let retry = retry_override
                .unwrap_or_else(|| gateway_evm_atomic_broadcast_exec_retry_default(ticket.chain_id));
            let timeout_ms = timeout_ms_override.unwrap_or_else(|| {
                gateway_evm_atomic_broadcast_exec_timeout_ms_default(ticket.chain_id)
            });
            let retry_backoff_ms = retry_backoff_ms_override.unwrap_or_else(|| {
                gateway_evm_atomic_broadcast_exec_retry_backoff_ms_default(ticket.chain_id)
            });
            let exec_path = if use_external_executor {
                gateway_evm_atomic_broadcast_exec_path(ticket.chain_id)
            } else {
                None
            };
            if use_external_executor && exec_path.is_none() {
                mark_gateway_pending_atomic_broadcast_ticket(ctx.eth_tx_index_store, &ticket);
                let _ = set_gateway_evm_atomic_ready_status(
                    ctx.eth_tx_index_store,
                    &intent_id,
                    EVM_ATOMIC_READY_STATUS_BROADCAST_FAILED_V1,
                );
                bail!(
                    "evm atomic-broadcast executor requested but not configured; set NOVOVM_GATEWAY_EVM_ATOMIC_BROADCAST_EXEC"
                );
            }
            let tx_ir_bincode = load_atomic_broadcast_tx_ir_bincode_from_pending_ready(
                ctx.eth_tx_index_store,
                &ticket.intent_id,
                ticket.chain_id,
                &ticket.tx_hash,
            );
            if let Some(exec_path) = exec_path.as_ref() {
                match execute_gateway_atomic_broadcast_ticket_with_retry(
                    exec_path.as_path(),
                    &ticket,
                    retry,
                    timeout_ms,
                    retry_backoff_ms,
                    tx_ir_bincode.as_deref(),
                ) {
                    Ok((exec_output, attempts)) => {
                        clear_gateway_pending_atomic_broadcast_ticket(
                            ctx.eth_tx_index_store,
                            &intent_id,
                        );
                        clear_gateway_pending_atomic_broadcast_payload(
                            ctx.eth_tx_index_store,
                            &intent_id,
                        );
                        let _ = set_gateway_evm_atomic_ready_status(
                            ctx.eth_tx_index_store,
                            &intent_id,
                            EVM_ATOMIC_READY_STATUS_BROADCASTED_V1,
                        );
                        Ok((
                            serde_json::json!({
                                "intent_id": intent_id,
                                "broadcasted": true,
                                "chain_id": format!("0x{:x}", ticket.chain_id),
                                "tx_hash": format!("0x{}", to_hex(&ticket.tx_hash)),
                            "executor": exec_path.display().to_string(),
                                "retry": retry,
                                "timeout_ms": timeout_ms,
                                "retry_backoff_ms": retry_backoff_ms,
                                "attempts": attempts,
                                "executor_output": exec_output,
                            }),
                            false,
                        ))
                    }
                    Err((e, attempts)) => {
                        mark_gateway_pending_atomic_broadcast_ticket(ctx.eth_tx_index_store, &ticket);
                        let _ = set_gateway_evm_atomic_ready_status(
                            ctx.eth_tx_index_store,
                            &intent_id,
                            EVM_ATOMIC_READY_STATUS_BROADCAST_FAILED_V1,
                        );
                        bail!(
                            "execute atomic broadcast failed: intent_id={} chain_id={} tx_hash=0x{} attempts={} err={}",
                            intent_id,
                            ticket.chain_id,
                            to_hex(&ticket.tx_hash),
                            attempts,
                            e
                        );
                    }
                }
            } else {
                match execute_gateway_atomic_broadcast_ticket_native(
                    eth_tx_index,
                    evm_settlement_index_by_id,
                    evm_settlement_index_by_tx,
                    evm_pending_payout_by_settlement,
                    ctx,
                    &ticket,
                    tx_ir_bincode.as_deref(),
                ) {
                    Ok(spool_file) => {
                        clear_gateway_pending_atomic_broadcast_ticket(
                            ctx.eth_tx_index_store,
                            &intent_id,
                        );
                        clear_gateway_pending_atomic_broadcast_payload(
                            ctx.eth_tx_index_store,
                            &intent_id,
                        );
                        let _ = set_gateway_evm_atomic_ready_status(
                            ctx.eth_tx_index_store,
                            &intent_id,
                            EVM_ATOMIC_READY_STATUS_BROADCASTED_V1,
                        );
                        Ok((
                            serde_json::json!({
                                "intent_id": intent_id,
                                "broadcasted": true,
                                "chain_id": format!("0x{:x}", ticket.chain_id),
                                "tx_hash": format!("0x{}", to_hex(&ticket.tx_hash)),
                                "executor": "native",
                                "retry": 0,
                                "timeout_ms": 0,
                                "retry_backoff_ms": 0,
                                "attempts": 1,
                                "spool_file": spool_file.display().to_string(),
                            }),
                            false,
                        ))
                    }
                    Err(e) => {
                        mark_gateway_pending_atomic_broadcast_ticket(ctx.eth_tx_index_store, &ticket);
                        let _ = set_gateway_evm_atomic_ready_status(
                            ctx.eth_tx_index_store,
                            &intent_id,
                            EVM_ATOMIC_READY_STATUS_BROADCAST_FAILED_V1,
                        );
                        bail!(
                            "execute atomic broadcast (native) failed: intent_id={} chain_id={} tx_hash=0x{} err={}",
                            intent_id,
                            ticket.chain_id,
                            to_hex(&ticket.tx_hash),
                            e
                        );
                    }
                }
            }
        }
        "evm_executePendingAtomicBroadcasts" | "evm_execute_pending_atomic_broadcasts" => {
            let force_native = gateway_atomic_broadcast_force_native(params);
            let use_external_executor = !force_native
                && gateway_atomic_broadcast_use_external_executor(params);
            let requested_max_items = param_as_u64(params, "max_items")
                .or_else(|| param_as_u64(params, "max"))
                .unwrap_or_else(gateway_evm_atomic_broadcast_exec_batch_default);
            let hard_max_items = gateway_evm_atomic_broadcast_exec_batch_hard_max();
            let max_items = requested_max_items.clamp(1, hard_max_items) as usize;
            let retry_override = param_as_u64(params, "retry")
                .or_else(|| param_as_u64(params, "max_retry"))
                .map(|v| v.min(16));
            let timeout_ms_override = param_as_u64(params, "timeout_ms")
                .or_else(|| param_as_u64(params, "exec_timeout_ms"))
                .map(|v| v.min(300_000));
            let retry_backoff_ms_override = param_as_u64(params, "retry_backoff_ms")
                .or_else(|| param_as_u64(params, "backoff_ms"))
                .map(|v| v.min(10_000));
            let summary_chain_id =
                param_as_u64_any_with_tx(params, &["chain_id", "chainId"]).unwrap_or(
                    ctx.eth_default_chain_id,
                );
            let summary_retry = retry_override
                .unwrap_or_else(|| gateway_evm_atomic_broadcast_exec_retry_default(summary_chain_id));
            let summary_timeout_ms = timeout_ms_override.unwrap_or_else(|| {
                gateway_evm_atomic_broadcast_exec_timeout_ms_default(summary_chain_id)
            });
            let summary_retry_backoff_ms = retry_backoff_ms_override.unwrap_or_else(|| {
                gateway_evm_atomic_broadcast_exec_retry_backoff_ms_default(summary_chain_id)
            });
            let tickets = ctx
                .eth_tx_index_store
                .load_pending_atomic_broadcast_tickets(max_items)?;
            let executor_name = if use_external_executor {
                "external(per-chain)".to_string()
            } else {
                "native".to_string()
            };
            if tickets.is_empty() {
                return Ok((
                    serde_json::json!({
                        "total": 0,
                        "executed": 0,
                        "failed": 0,
                        "executor": executor_name,
                        "retry": summary_retry,
                        "timeout_ms": summary_timeout_ms,
                        "retry_backoff_ms": summary_retry_backoff_ms,
                        "requested_max_items": requested_max_items,
                        "max_items": max_items,
                        "hard_max_items": hard_max_items,
                    }),
                    false,
                ));
            }
            let mut executed = 0usize;
            let mut failed = 0usize;
            let mut total_attempts = 0u64;
            let mut failed_intent_ids = Vec::new();
            for ticket in tickets {
                let retry = retry_override
                    .unwrap_or_else(|| gateway_evm_atomic_broadcast_exec_retry_default(ticket.chain_id));
                let timeout_ms = timeout_ms_override.unwrap_or_else(|| {
                    gateway_evm_atomic_broadcast_exec_timeout_ms_default(ticket.chain_id)
                });
                let retry_backoff_ms = retry_backoff_ms_override.unwrap_or_else(|| {
                    gateway_evm_atomic_broadcast_exec_retry_backoff_ms_default(ticket.chain_id)
                });
                let tx_ir_bincode = load_atomic_broadcast_tx_ir_bincode_from_pending_ready(
                    ctx.eth_tx_index_store,
                    &ticket.intent_id,
                    ticket.chain_id,
                    &ticket.tx_hash,
                );
                let exec_result = if use_external_executor {
                    let Some(exec_path) = gateway_evm_atomic_broadcast_exec_path(ticket.chain_id) else {
                        failed = failed.saturating_add(1);
                        failed_intent_ids.push(ticket.intent_id.clone());
                        mark_gateway_pending_atomic_broadcast_ticket(ctx.eth_tx_index_store, &ticket);
                        let _ = set_gateway_evm_atomic_ready_status(
                            ctx.eth_tx_index_store,
                            &ticket.intent_id,
                            EVM_ATOMIC_READY_STATUS_BROADCAST_FAILED_V1,
                        );
                        continue;
                    };
                    execute_gateway_atomic_broadcast_ticket_with_retry(
                        exec_path.as_path(),
                        &ticket,
                        retry,
                        timeout_ms,
                        retry_backoff_ms,
                        tx_ir_bincode.as_deref(),
                    )
                    .map(|(_, attempts)| attempts)
                    .map_err(|(_, attempts)| attempts)
                } else {
                    execute_gateway_atomic_broadcast_ticket_native(
                        eth_tx_index,
                        evm_settlement_index_by_id,
                        evm_settlement_index_by_tx,
                        evm_pending_payout_by_settlement,
                        ctx,
                        &ticket,
                        tx_ir_bincode.as_deref(),
                    )
                    .map(|_| 1u64)
                    .map_err(|_| 1u64)
                };
                match exec_result {
                    Ok(attempts) => {
                        total_attempts = total_attempts.saturating_add(attempts);
                        clear_gateway_pending_atomic_broadcast_ticket(
                            ctx.eth_tx_index_store,
                            &ticket.intent_id,
                        );
                        clear_gateway_pending_atomic_broadcast_payload(
                            ctx.eth_tx_index_store,
                            &ticket.intent_id,
                        );
                        let _ = set_gateway_evm_atomic_ready_status(
                            ctx.eth_tx_index_store,
                            &ticket.intent_id,
                            EVM_ATOMIC_READY_STATUS_BROADCASTED_V1,
                        );
                        executed = executed.saturating_add(1);
                    }
                    Err(attempts) => {
                        total_attempts = total_attempts.saturating_add(attempts);
                        mark_gateway_pending_atomic_broadcast_ticket(ctx.eth_tx_index_store, &ticket);
                        let _ = set_gateway_evm_atomic_ready_status(
                            ctx.eth_tx_index_store,
                            &ticket.intent_id,
                            EVM_ATOMIC_READY_STATUS_BROADCAST_FAILED_V1,
                        );
                        failed = failed.saturating_add(1);
                        failed_intent_ids.push(ticket.intent_id.clone());
                    }
                }
            }
            Ok((
                serde_json::json!({
                    "total": executed + failed,
                    "executed": executed,
                    "failed": failed,
                    "failed_intent_ids": failed_intent_ids,
                    "executor": executor_name,
                    "retry": summary_retry,
                    "timeout_ms": summary_timeout_ms,
                    "retry_backoff_ms": summary_retry_backoff_ms,
                    "requested_max_items": requested_max_items,
                    "max_items": max_items,
                    "hard_max_items": hard_max_items,
                    "total_attempts": total_attempts,
                }),
                false,
            ))
        }
        "ua_setPolicy" => {
            let uca_id = param_as_string(params, "uca_id")
                .ok_or_else(|| anyhow::anyhow!("uca_id is required for ua_setPolicy"))?;
            let role = parse_account_role(params)?;
            let now = param_as_u64(params, "now").unwrap_or_else(now_unix_sec);
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
            router.update_policy(
                &uca_id,
                role,
                AccountPolicy {
                    nonce_scope,
                    type4_policy_mode,
                    allow_type4_with_delegate_or_session,
                    kyc_policy_mode,
                },
                now,
            )?;
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
        "eth_sendRawTransaction" => {
            let explicit_uca_id = param_as_string(params, "uca_id");
            let role = parse_account_role(params)?;
            let raw_tx_hex = extract_eth_raw_tx_param(params)
                .ok_or_else(|| anyhow::anyhow!("raw_tx is required for eth_sendRawTransaction"))?;
            let raw_tx = decode_hex_bytes(&raw_tx_hex, "raw_tx")?;
            let fields = translate_raw_evm_tx_fields_m0(&raw_tx)?;
            let explicit_tx_type = param_as_u64_any_with_tx(params, &["tx_type", "type"]);
            if let Some(explicit) = explicit_tx_type {
                if explicit > u8::MAX as u64 {
                    bail!("tx_type out of range: {}", explicit);
                }
                let inferred = fields.hint.tx_type_number as u64;
                if explicit != inferred {
                    bail!(
                        "tx_type mismatch: explicit={} inferred_from_raw={}",
                        explicit,
                        inferred
                    );
                }
            }

            let explicit_chain_id_present = param_as_u64(params, "chain_id").is_some()
                || param_as_u64(params, "chainId").is_some()
                || param_tx_object(params)
                    .and_then(|tx| {
                        param_as_u64(tx, "chain_id").or_else(|| param_as_u64(tx, "chainId"))
                    })
                    .is_some();
            let explicit_chain_id = if explicit_chain_id_present {
                Some(resolve_chain_id_with_tx_consistency(params, 0)?)
            } else {
                None
            };
            if let (Some(explicit), Some(inferred)) = (explicit_chain_id, fields.chain_id) {
                if explicit != inferred {
                    bail!(
                        "chain_id mismatch: explicit={} inferred_from_raw={}",
                        explicit,
                        inferred
                    );
                }
            }
            let chain_id = explicit_chain_id.or(fields.chain_id).ok_or_else(|| {
                anyhow::anyhow!("chain_id is required for eth_sendRawTransaction")
            })?;
            let has_access_list_intrinsic = fields.access_list_address_count.unwrap_or(0) > 0
                || fields.access_list_storage_key_count.unwrap_or(0) > 0;
            let tx_type = resolve_gateway_eth_write_tx_type(
                chain_id,
                explicit_tx_type.or(Some(fields.hint.tx_type_number as u64)),
                false,
                has_access_list_intrinsic,
                fields.max_fee_per_blob_gas.unwrap_or(0),
                fields.blob_hash_count.unwrap_or(0),
            )
            .context("eth_sendRawTransaction write tx type validation failed")?;
            let chain_entries = collect_gateway_eth_chain_entries(
                eth_tx_index,
                ctx.eth_tx_index_store,
                chain_id,
                gateway_eth_query_scan_max(),
            )?;
            let latest_block_number = resolve_gateway_eth_latest_block_number(
                chain_id,
                &chain_entries,
                ctx.eth_tx_index_store,
            )?;
            let pending_block_number = latest_block_number.saturating_add(1);
            validate_gateway_eth_tx_type_fork_activation(
                chain_id,
                fields.hint.tx_type_number,
                pending_block_number,
            )?;

            let explicit_nonce = param_as_u64_any_with_tx(params, &["nonce"]);
            if let (Some(explicit), Some(inferred)) = (explicit_nonce, fields.nonce) {
                if explicit != inferred {
                    bail!(
                        "nonce mismatch: explicit={} inferred_from_raw={}",
                        explicit,
                        inferred
                    );
                }
            }
            let nonce = explicit_nonce
                .or(fields.nonce)
                .ok_or_else(|| anyhow::anyhow!("nonce is required for eth_sendRawTransaction"))?;

            let explicit_from = extract_eth_persona_address_param(params)
                .map(|raw| decode_hex_bytes(&raw, "from"))
                .transpose()?;
            let recovered_from = recover_raw_evm_tx_sender_m0(&raw_tx)?;
            let from = match (explicit_from, recovered_from) {
                (Some(explicit), Some(recovered)) => {
                    if explicit != recovered {
                        bail!(
                            "from mismatch: explicit=0x{} recovered_from_raw=0x{}",
                            to_hex(&explicit),
                            to_hex(&recovered)
                        );
                    }
                    recovered
                }
                (Some(explicit), None) => explicit,
                (None, Some(recovered)) => recovered,
                (None, None) => {
                    bail!(
                        "from (or external_address) is required for eth_sendRawTransaction when raw sender recovery is unavailable"
                    )
                }
            };
            let persona = PersonaAddress {
                persona_type: PersonaType::Evm,
                chain_id,
                external_address: from.clone(),
            };
            let binding_owner = router.resolve_binding_owner(&persona).map(str::to_string);
            let uca_id = match (explicit_uca_id, binding_owner) {
                (Some(explicit), Some(owner)) => {
                    if explicit != owner {
                        bail!(
                            "uca_id mismatch for address binding: explicit={} binding_owner={}",
                            explicit,
                            owner
                        );
                    }
                    explicit
                }
                (Some(explicit), None) => explicit,
                (None, Some(owner)) => owner,
                (None, None) => {
                    bail!(
                        "uca_id is required for eth_sendRawTransaction when from is not bound"
                    )
                }
            };
            let signature_domain = param_as_string(params, "signature_domain")
                .or_else(|| {
                    param_as_string_any_with_tx(params, &["signature_domain", "signatureDomain"])
                })
                .unwrap_or_else(|| format!("evm:{chain_id}"));
            let wants_cross_chain_atomic =
                param_as_bool_any_with_tx(params, &["wants_cross_chain_atomic", "wantsCrossChainAtomic"])
                    .unwrap_or(false);
            let kyc =
                resolve_gateway_kyc_verified(params, &uca_id, chain_id, &from, role, nonce, true)?;
            let session_expires_at =
                param_as_u64_any_with_tx(params, &["session_expires_at", "sessionExpiresAt"]);
            let now = param_as_u64(params, "now").unwrap_or_else(now_unix_sec);

            let _decision = router.route(RouteRequest {
                uca_id: uca_id.clone(),
                persona,
                role,
                protocol: ProtocolKind::Eth,
                signature_domain: signature_domain.clone(),
                nonce,
                kyc_attestation_provided: kyc.provided,
                kyc_verified: kyc.verified,
                wants_cross_chain_atomic,
                tx_type4: fields.hint.tx_type4,
                session_expires_at,
                now,
            })?;
            let tx_ir = tx_ir_from_raw_fields_m0(&fields, &raw_tx, from.clone(), chain_id);
            if tx_ir.to.is_none() {
                validate_gateway_eth_contract_deploy_initcode_size(
                    chain_id,
                    pending_block_number,
                    tx_ir.data.len(),
                )?;
            }
            let chain_type = resolve_evm_chain_type_from_chain_id(chain_id);
            let profile = resolve_evm_profile(chain_type, chain_id)?;
            validate_tx_semantics_m0(&profile, &tx_ir)
                .context("eth_sendRawTransaction semantic validation failed")?;
            if matches!(
                fields.hint.envelope,
                EvmRawTxEnvelopeType::Type2DynamicFee | EvmRawTxEnvelopeType::Type3Blob
            ) {
                let base_fee_per_gas = gateway_eth_base_fee_per_gas_wei(chain_id);
                if tx_ir.gas_price < base_fee_per_gas {
                    bail!(
                        "eth_sendRawTransaction maxFeePerGas below current base fee: max_fee_per_gas={} base_fee_per_gas={}",
                        tx_ir.gas_price,
                        base_fee_per_gas
                    );
                }
            }
            let tap_drain =
                apply_gateway_evm_runtime_tap(&tx_ir, wants_cross_chain_atomic)?;
            let tx_hash = vec_to_32(&tx_ir.hash, "tx_hash")?;
            let record = GatewayIngressEthRecordV1 {
                version: GATEWAY_INGRESS_RECORD_VERSION,
                protocol: GATEWAY_INGRESS_PROTOCOL_ETH,
                uca_id: uca_id.clone(),
                chain_id,
                nonce,
                tx_type,
                tx_type4: fields.hint.tx_type4,
                from,
                to: tx_ir.to.clone(),
                value: tx_ir.value,
                gas_limit: tx_ir.gas_limit,
                gas_price: tx_ir.gas_price,
                data: tx_ir.data.clone(),
                signature: raw_tx,
                tx_hash,
                signature_domain: signature_domain.clone(),
                overlay_node_id: ctx.overlay_node_id.clone(),
                overlay_session_id: ctx.overlay_session_id.clone(),
            };
            let tx_ir_bincode = if gateway_eth_public_broadcast_exec_path(chain_id).is_some() {
                Some(
                    tx_ir
                        .serialize(SerializationFormat::Bincode)
                        .context("serialize eth tx ir bincode for public broadcast failed")?,
                )
            } else {
                None
            };
            let require_public_broadcast = param_as_bool_any_with_tx(
                params,
                &["require_public_broadcast", "requirePublicBroadcast"],
            )
            .unwrap_or(false);
            let broadcast_result = maybe_execute_gateway_eth_public_broadcast(
                chain_id,
                &record.tx_hash,
                GatewayEthPublicBroadcastPayload {
                    raw_tx: Some(record.signature.as_slice()),
                    tx_ir_bincode: tx_ir_bincode.as_deref(),
                },
                require_public_broadcast,
            )?;
            upsert_gateway_eth_broadcast_status(
                ctx.eth_tx_index_store,
                chain_id,
                record.tx_hash,
                &broadcast_result,
            );
            let wire = encode_gateway_ingress_ops_wire_v1_eth(&record)?;
            write_spool_ops_wire_v1(ctx.spool_dir, &wire)?;
            upsert_gateway_eth_tx_index(eth_tx_index, ctx.eth_tx_index_store, &record);
            persist_gateway_eth_submit_success_status(
                ctx.eth_tx_index_store,
                record.tx_hash,
                record.chain_id,
                true,
                false,
            );
            for settlement in &tap_drain.settlement_records {
                if let Err(e) = upsert_gateway_evm_settlement_index(
                    evm_settlement_index_by_id,
                    evm_settlement_index_by_tx,
                    ctx.eth_tx_index_store,
                    settlement,
                ) {
                    gateway_warn!(
                        "gateway_warn: upsert evm settlement index failed: chain_id={} tx_hash=0x{} settlement_id={} err={}",
                        settlement.income.chain_id,
                        to_hex(&settlement.income.tx_hash),
                        settlement.result.settlement_id,
                        e
                    );
                }
            }
            if let Err(e) =
                persist_gateway_evm_settlement_records(ctx.spool_dir, &tap_drain.settlement_records)
            {
                gateway_warn!(
                    "gateway_warn: persist evm settlement records failed: chain_id={} tx_hash=0x{} count={} err={}",
                    chain_id,
                    to_hex(&record.tx_hash),
                    tap_drain.settlement_records.len(),
                    e
                );
            }
            persist_gateway_payout_with_compensation(
                ctx.spool_dir,
                chain_id,
                &record.tx_hash,
                &tap_drain.payout_instructions,
                evm_settlement_index_by_id,
                evm_pending_payout_by_settlement,
                ctx.eth_tx_index_store,
            );
            persist_gateway_atomic_ready_with_compensation(
                ctx.spool_dir,
                chain_id,
                &record.tx_hash,
                &tap_drain.atomic_ready_items,
                ctx.eth_tx_index_store,
            );
            let return_detail =
                param_as_bool_any_with_tx(params, &["return_detail", "returnDetail"]).unwrap_or(false);
            if !return_detail {
                return Ok((
                    serde_json::Value::String(format!("0x{}", to_hex(&tx_hash))),
                    true,
                ));
            }
            let broadcast_json = match broadcast_result {
                Some((output, attempts, executor)) => serde_json::json!({
                    "mode": "external",
                    "attempts": attempts,
                    "executor": executor,
                    "executor_output": output,
                }),
                None => serde_json::json!({
                    "mode": "none",
                }),
            };
            Ok((
                serde_json::json!({
                    "accepted": true,
                    "tx_hash": format!("0x{}", to_hex(&tx_hash)),
                    "chain_id": format!("0x{:x}", chain_id),
                    "pending": true,
                    "onchain": false,
                    "broadcast": broadcast_json,
                    "overlay_node_id": record.overlay_node_id,
                    "overlay_session_id": record.overlay_session_id,
                }),
                true,
            ))
        }
        "eth_sendTransaction" => {
            let explicit_uca_id = param_as_string(params, "uca_id");
            let role = parse_account_role(params)?;
            let chain_id =
                resolve_chain_id_with_tx_consistency(params, ctx.eth_default_chain_id)?;
            let from_raw = extract_eth_persona_address_param(params).ok_or_else(|| {
                anyhow::anyhow!("from (or external_address) is required for eth_sendTransaction")
            })?;
            let explicit_from = decode_hex_bytes(&from_raw, "from")?;
            let recovered_from = recover_gateway_eth_sender_from_signature_param(params)?;
            let from = match (explicit_from, recovered_from) {
                (explicit, Some(recovered)) => {
                    if explicit != recovered {
                        bail!(
                            "from mismatch: explicit=0x{} recovered_from_signature=0x{}",
                            to_hex(&explicit),
                            to_hex(&recovered)
                        );
                    }
                    recovered
                }
                (explicit, None) => explicit,
            };
            let persona = PersonaAddress {
                persona_type: PersonaType::Evm,
                chain_id,
                external_address: from.clone(),
            };
            let binding_owner = router.resolve_binding_owner(&persona).map(str::to_string);
            let uca_id = match (explicit_uca_id, binding_owner) {
                (Some(explicit), Some(owner)) => {
                    if explicit != owner {
                        bail!(
                            "uca_id mismatch for address binding: explicit={} binding_owner={}",
                            explicit,
                            owner
                        );
                    }
                    explicit
                }
                (Some(explicit), None) => explicit,
                (None, Some(owner)) => owner,
                (None, None) => {
                    bail!("uca_id is required for eth_sendTransaction when from is not bound")
                }
            };
            let chain_entries = collect_gateway_eth_chain_entries(
                eth_tx_index,
                ctx.eth_tx_index_store,
                chain_id,
                gateway_eth_query_scan_max(),
            )?;
            let latest_block_number = resolve_gateway_eth_latest_block_number(
                chain_id,
                &chain_entries,
                ctx.eth_tx_index_store,
            )?;
            let nonce = if let Some(explicit_nonce) = param_as_u64_any_with_tx(params, &["nonce"]) {
                explicit_nonce
            } else {
                let latest_nonce = chain_entries
                    .iter()
                    .filter(|entry| entry.from == from)
                    .map(|entry| entry.nonce.saturating_add(1))
                    .max()
                    .unwrap_or(0);
                let pending_nonce_from_router = router.next_nonce_for_persona(&uca_id, &persona).ok();
                let pending_nonce = pending_nonce_from_router
                    .map(|value| value.max(latest_nonce))
                    .unwrap_or(latest_nonce);
                let pending_nonce_from_runtime =
                    gateway_eth_pending_nonce_from_runtime(chain_id, &from);
                pending_nonce_from_runtime
                    .map(|value| value.max(pending_nonce))
                    .unwrap_or(pending_nonce)
            };
            let to = match param_as_string_any_with_tx(params, &["to"]) {
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
            let data = match param_as_string_any_with_tx(params, &["data", "input"]) {
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
            let value = param_as_u128_any_with_tx(params, &["value"]).unwrap_or(0);
            let (access_list_address_count, access_list_storage_key_count) =
                parse_eth_access_list_intrinsic_counts(params)?;
            let (max_fee_per_blob_gas, blob_hash_count) =
                parse_eth_blob_intrinsic_fields(params)?;
            let has_access_list_intrinsic =
                access_list_address_count > 0 || access_list_storage_key_count > 0;
            let gas_limit = param_as_u64_any_with_tx(params, &["gas_limit", "gasLimit", "gas"])
                .unwrap_or(21_000);
            let max_fee_per_gas_param = param_as_u64_any_with_tx(
                params,
                &["max_fee_per_gas", "maxFeePerGas"],
            );
            let max_priority_fee_per_gas_param = param_as_u64_any_with_tx(
                params,
                &["max_priority_fee_per_gas", "maxPriorityFeePerGas"],
            );
            let legacy_gas_price_param =
                param_as_u64_any_with_tx(params, &["gas_price", "gasPrice"]);
            let explicit_tx_type = param_as_u64_any_with_tx(params, &["tx_type", "txType", "type"]);
            let has_eip1559_fee_fields = param_as_u64_any_with_tx(
                params,
                &[
                    "max_fee_per_gas",
                    "maxFeePerGas",
                    "max_priority_fee_per_gas",
                    "maxPriorityFeePerGas",
                ],
            )
            .is_some();
            let tx_type = resolve_gateway_eth_write_tx_type(
                chain_id,
                explicit_tx_type,
                has_eip1559_fee_fields,
                has_access_list_intrinsic,
                max_fee_per_blob_gas,
                blob_hash_count,
            )?;
            let pending_block_number = latest_block_number.saturating_add(1);
            validate_gateway_eth_tx_type_fork_activation(
                chain_id,
                tx_type,
                pending_block_number,
            )?;
            if to.is_none() {
                validate_gateway_eth_contract_deploy_initcode_size(
                    chain_id,
                    pending_block_number,
                    data.len(),
                )?;
            }
            let (gas_price, max_priority_fee_per_gas) = if tx_type == 2 || tx_type == 3 {
                if max_fee_per_gas_param.is_none() {
                    bail!(
                        "eth_sendTransaction maxFeePerGas is required for type2/type3 transactions"
                    );
                }
                let max_fee_per_gas = max_fee_per_gas_param.unwrap_or_default();
                let max_priority_fee_per_gas = max_priority_fee_per_gas_param.unwrap_or(
                    gateway_eth_default_max_priority_fee_per_gas_wei(chain_id).min(max_fee_per_gas),
                );
                if max_priority_fee_per_gas > max_fee_per_gas {
                    bail!(
                        "eth_sendTransaction maxPriorityFeePerGas exceeds maxFeePerGas: max_priority_fee_per_gas={} max_fee_per_gas={}",
                        max_priority_fee_per_gas,
                        max_fee_per_gas
                    );
                }
                let base_fee_per_gas = gateway_eth_base_fee_per_gas_wei(chain_id);
                if max_fee_per_gas < base_fee_per_gas {
                    bail!(
                        "eth_sendTransaction maxFeePerGas below current base fee: max_fee_per_gas={} base_fee_per_gas={}",
                        max_fee_per_gas,
                        base_fee_per_gas
                    );
                }
                (max_fee_per_gas, max_priority_fee_per_gas)
            } else {
                (
                    legacy_gas_price_param.unwrap_or(1),
                    max_priority_fee_per_gas_param.unwrap_or_default(),
                )
            };
            let tx_type4 =
                param_as_bool_any_with_tx(params, &["tx_type4"]).unwrap_or(false) || tx_type == 4;
            let signature = match param_as_string_any_with_tx(
                params,
                &["signature", "raw_signature", "signed_tx"],
            ) {
                Some(raw_sig) => decode_hex_bytes(&raw_sig, "signature")?,
                None => Vec::new(),
            };
            let signature_domain = param_as_string(params, "signature_domain")
                .or_else(|| {
                    param_as_string_any_with_tx(params, &["signature_domain", "signatureDomain"])
                })
                .unwrap_or_else(|| format!("evm:{chain_id}"));
            let wants_cross_chain_atomic =
                param_as_bool_any_with_tx(params, &["wants_cross_chain_atomic", "wantsCrossChainAtomic"])
                    .unwrap_or(false);
            let kyc =
                resolve_gateway_kyc_verified(params, &uca_id, chain_id, &from, role, nonce, true)?;
            let session_expires_at =
                param_as_u64_any_with_tx(params, &["session_expires_at", "sessionExpiresAt"]);
            let now = param_as_u64(params, "now").unwrap_or_else(now_unix_sec);
            let tx_hash_input = GatewayEthTxHashInput {
                uca_id: &uca_id,
                chain_id,
                nonce,
                tx_type,
                tx_type4,
                from: &from,
                to: to.as_deref(),
                value,
                gas_limit,
                gas_price,
                max_priority_fee_per_gas,
                data: &data,
                signature: &signature,
                access_list_address_count,
                access_list_storage_key_count,
                max_fee_per_blob_gas,
                blob_hash_count,
                signature_domain: &signature_domain,
                wants_cross_chain_atomic,
            };

            validate_gateway_eth_send_tx_signature_consistency(
                params,
                &tx_hash_input,
                tx_type,
                tx_type4,
            )?;

            let _decision = router.route(RouteRequest {
                uca_id: uca_id.clone(),
                persona,
                role,
                protocol: ProtocolKind::Eth,
                signature_domain: signature_domain.clone(),
                nonce,
                kyc_attestation_provided: kyc.provided,
                kyc_verified: kyc.verified,
                wants_cross_chain_atomic,
                tx_type4,
                session_expires_at,
                now,
            })?;
            let tx_hash = compute_gateway_eth_tx_hash(&tx_hash_input);
            let tx_ir_type = if to.is_none() {
                TxType::ContractDeploy
            } else if data.is_empty() {
                TxType::Transfer
            } else {
                TxType::ContractCall
            };
            let tx_ir = TxIR {
                hash: tx_hash.to_vec(),
                from: from.clone(),
                to: to.clone(),
                value,
                gas_limit,
                gas_price,
                nonce,
                data: data.clone(),
                signature: signature.clone(),
                chain_id,
                tx_type: tx_ir_type,
                source_chain: None,
                target_chain: None,
            };
            let required_intrinsic = estimate_intrinsic_gas_with_envelope_extras_m0(
                &tx_ir,
                access_list_address_count,
                access_list_storage_key_count,
                if tx_type == 3 { blob_hash_count } else { 0 },
            );
            if tx_ir.gas_limit < required_intrinsic {
                bail!(
                    "eth_sendTransaction gas too low for intrinsic cost: gas_limit={} required_intrinsic={} access_list_addresses={} access_list_storage_keys={} blob_hash_count={}",
                    tx_ir.gas_limit,
                    required_intrinsic,
                    access_list_address_count,
                    access_list_storage_key_count,
                    if tx_type == 3 { blob_hash_count } else { 0 }
                );
            }
            let tap_drain =
                apply_gateway_evm_runtime_tap(&tx_ir, wants_cross_chain_atomic)?;
            let record = GatewayIngressEthRecordV1 {
                version: GATEWAY_INGRESS_RECORD_VERSION,
                protocol: GATEWAY_INGRESS_PROTOCOL_ETH,
                uca_id: uca_id.clone(),
                chain_id,
                nonce,
                tx_type,
                tx_type4,
                from,
                to: to.clone(),
                value,
                gas_limit,
                gas_price,
                data: data.clone(),
                signature,
                tx_hash,
                signature_domain: signature_domain.clone(),
                overlay_node_id: ctx.overlay_node_id.clone(),
                overlay_session_id: ctx.overlay_session_id.clone(),
            };
            let tx_ir_bincode = if gateway_eth_public_broadcast_exec_path(chain_id).is_some() {
                Some(
                    tx_ir
                        .serialize(SerializationFormat::Bincode)
                        .context("serialize eth tx ir bincode for public broadcast failed")?,
                )
            } else {
                None
            };
            let raw_tx_for_public_broadcast = if record.signature.is_empty() {
                None
            } else {
                Some(record.signature.as_slice())
            };
            let require_public_broadcast = param_as_bool_any_with_tx(
                params,
                &["require_public_broadcast", "requirePublicBroadcast"],
            )
            .unwrap_or(false);
            let broadcast_result = maybe_execute_gateway_eth_public_broadcast(
                chain_id,
                &record.tx_hash,
                GatewayEthPublicBroadcastPayload {
                    raw_tx: raw_tx_for_public_broadcast,
                    tx_ir_bincode: tx_ir_bincode.as_deref(),
                },
                require_public_broadcast,
            )?;
            upsert_gateway_eth_broadcast_status(
                ctx.eth_tx_index_store,
                chain_id,
                record.tx_hash,
                &broadcast_result,
            );
            let wire = encode_gateway_ingress_ops_wire_v1_eth(&record)?;
            write_spool_ops_wire_v1(ctx.spool_dir, &wire)?;
            upsert_gateway_eth_tx_index(eth_tx_index, ctx.eth_tx_index_store, &record);
            persist_gateway_eth_submit_success_status(
                ctx.eth_tx_index_store,
                record.tx_hash,
                record.chain_id,
                true,
                false,
            );
            for settlement in &tap_drain.settlement_records {
                if let Err(e) = upsert_gateway_evm_settlement_index(
                    evm_settlement_index_by_id,
                    evm_settlement_index_by_tx,
                    ctx.eth_tx_index_store,
                    settlement,
                ) {
                    gateway_warn!(
                        "gateway_warn: upsert evm settlement index failed: chain_id={} tx_hash=0x{} settlement_id={} err={}",
                        settlement.income.chain_id,
                        to_hex(&settlement.income.tx_hash),
                        settlement.result.settlement_id,
                        e
                    );
                }
            }
            if let Err(e) =
                persist_gateway_evm_settlement_records(ctx.spool_dir, &tap_drain.settlement_records)
            {
                gateway_warn!(
                    "gateway_warn: persist evm settlement records failed: chain_id={} tx_hash=0x{} count={} err={}",
                    chain_id,
                    to_hex(&record.tx_hash),
                    tap_drain.settlement_records.len(),
                    e
                );
            }
            persist_gateway_payout_with_compensation(
                ctx.spool_dir,
                chain_id,
                &record.tx_hash,
                &tap_drain.payout_instructions,
                evm_settlement_index_by_id,
                evm_pending_payout_by_settlement,
                ctx.eth_tx_index_store,
            );
            persist_gateway_atomic_ready_with_compensation(
                ctx.spool_dir,
                chain_id,
                &record.tx_hash,
                &tap_drain.atomic_ready_items,
                ctx.eth_tx_index_store,
            );
            let return_detail =
                param_as_bool_any_with_tx(params, &["return_detail", "returnDetail"]).unwrap_or(false);
            if !return_detail {
                return Ok((
                    serde_json::Value::String(format!("0x{}", to_hex(&record.tx_hash))),
                    true,
                ));
            }
            let broadcast_json = match broadcast_result {
                Some((output, attempts, executor)) => serde_json::json!({
                    "mode": "external",
                    "attempts": attempts,
                    "executor": executor,
                    "executor_output": output,
                }),
                None => serde_json::json!({
                    "mode": "none",
                }),
            };
            Ok((
                serde_json::json!({
                    "accepted": true,
                    "tx_hash": format!("0x{}", to_hex(&record.tx_hash)),
                    "chain_id": format!("0x{:x}", chain_id),
                    "pending": true,
                    "onchain": false,
                    "broadcast": broadcast_json,
                    "overlay_node_id": record.overlay_node_id,
                    "overlay_session_id": record.overlay_session_id,
                }),
                true,
            ))
        }
        "web30_sendRawTransaction" => {
            let uca_id = param_as_string(params, "uca_id")
                .ok_or_else(|| anyhow::anyhow!("uca_id is required for web30_sendRawTransaction"))?;
            let role = parse_account_role(params)?;
            let chain_id = param_as_u64(params, "chain_id")
                .ok_or_else(|| anyhow::anyhow!("chain_id is required for web30_sendRawTransaction"))?;
            let nonce = param_as_u64(params, "nonce")
                .ok_or_else(|| anyhow::anyhow!("nonce is required for web30_sendRawTransaction"))?;
            let from_raw = extract_eth_persona_address_param(params).ok_or_else(|| {
                anyhow::anyhow!(
                    "external_address (or from/address) is required for web30_sendRawTransaction"
                )
            })?;
            let from = decode_hex_bytes(&from_raw, "external_address")?;
            let raw_payload = extract_web30_raw_payload_param(params).ok_or_else(|| {
                anyhow::anyhow!("raw_tx/raw_transaction/raw/payload_hex is required for web30_sendRawTransaction")
            })?;
            let signature_domain = param_as_string(params, "signature_domain")
                .unwrap_or_else(|| "web30:mainnet".to_string());
            let wants_cross_chain_atomic =
                param_as_bool(params, "wants_cross_chain_atomic").unwrap_or(false);
            let kyc =
                resolve_gateway_kyc_verified(params, &uca_id, chain_id, &from, role, nonce, false)?;
            let session_expires_at = param_as_u64(params, "session_expires_at");
            let now = param_as_u64(params, "now").unwrap_or_else(now_unix_sec);

            let decision = router.route(RouteRequest {
                uca_id: uca_id.clone(),
                persona: PersonaAddress {
                    persona_type: PersonaType::Web30,
                    chain_id,
                    external_address: from.clone(),
                },
                role,
                protocol: ProtocolKind::Web30,
                signature_domain: signature_domain.clone(),
                nonce,
                kyc_attestation_provided: kyc.provided,
                kyc_verified: kyc.verified,
                wants_cross_chain_atomic,
                tx_type4: false,
                session_expires_at,
                now,
            })?;
            let tx_hash = compute_gateway_web30_tx_hash(&GatewayWeb30TxHashInput {
                uca_id: &uca_id,
                chain_id,
                nonce,
                from: &from,
                payload: &raw_payload,
                signature_domain: &signature_domain,
                is_raw: true,
                wants_cross_chain_atomic,
            });
            let record = GatewayIngressWeb30RecordV1 {
                version: GATEWAY_INGRESS_RECORD_VERSION,
                protocol: GATEWAY_INGRESS_PROTOCOL_WEB30,
                uca_id: uca_id.clone(),
                chain_id,
                nonce,
                from,
                payload: raw_payload,
                is_raw: true,
                signature_domain: signature_domain.clone(),
                wants_cross_chain_atomic,
                tx_hash,
                overlay_node_id: ctx.overlay_node_id.clone(),
                overlay_session_id: ctx.overlay_session_id.clone(),
            };
            let wire = encode_gateway_ingress_ops_wire_v1_web30(&record)?;
            let spool_file = write_spool_ops_wire_v1(ctx.spool_dir, &wire)?;

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
                    "tx_hash": format!("0x{}", to_hex(&record.tx_hash)),
                    "spool_file": spool_file.display().to_string(),
                    "ingress_codec": "ops_wire_v1",
                    "overlay_node_id": record.overlay_node_id,
                    "overlay_session_id": record.overlay_session_id,
                }),
                true,
            ))
        }
        "web30_sendTransaction" => {
            let uca_id = param_as_string(params, "uca_id")
                .ok_or_else(|| anyhow::anyhow!("uca_id is required for web30_sendTransaction"))?;
            let role = parse_account_role(params)?;
            let chain_id = param_as_u64(params, "chain_id")
                .ok_or_else(|| anyhow::anyhow!("chain_id is required for web30_sendTransaction"))?;
            let nonce = param_as_u64(params, "nonce")
                .ok_or_else(|| anyhow::anyhow!("nonce is required for web30_sendTransaction"))?;
            let from_raw = extract_eth_persona_address_param(params).ok_or_else(|| {
                anyhow::anyhow!("external_address (or from/address) is required for web30_sendTransaction")
            })?;
            let from = decode_hex_bytes(&from_raw, "external_address")?;
            let privacy_plan = parse_gateway_web30_privacy_plan(params)?;
            let signature_domain = param_as_string(params, "signature_domain")
                .unwrap_or_else(|| "web30:mainnet".to_string());
            let wants_cross_chain_atomic =
                param_as_bool(params, "wants_cross_chain_atomic").unwrap_or(false);
            let kyc =
                resolve_gateway_kyc_verified(params, &uca_id, chain_id, &from, role, nonce, false)?;
            let session_expires_at = param_as_u64(params, "session_expires_at");
            let now = param_as_u64(params, "now").unwrap_or_else(now_unix_sec);

            let decision = router.route(RouteRequest {
                uca_id: uca_id.clone(),
                persona: PersonaAddress {
                    persona_type: PersonaType::Web30,
                    chain_id,
                    external_address: from.clone(),
                },
                role,
                protocol: ProtocolKind::Web30,
                signature_domain: signature_domain.clone(),
                nonce,
                kyc_attestation_provided: kyc.provided,
                kyc_verified: kyc.verified,
                wants_cross_chain_atomic,
                tx_type4: false,
                session_expires_at,
                now,
            })?;
            let mut tap_drain = GatewayEvmRuntimeTapDrain {
                settlement_records: Vec::new(),
                payout_instructions: Vec::new(),
                atomic_ready_items: Vec::new(),
            };
            let (payload, tx_hash, payload_kind, tx_ir_type) = if let Some(plan) = privacy_plan {
                if from.len() != 32 {
                    bail!(
                        "privacy web30_sendTransaction requires 32-byte from/external_address, got {}",
                        from.len()
                    );
                }
                if !plan
                    .ring_members
                    .iter()
                    .any(|member| member.as_slice() == from.as_slice())
                {
                    bail!("privacy.ring_members must include from/external_address");
                }
                let tx_ir = build_privacy_tx_ir_signed_from_raw_v1(
                    &PrivacyTxRawEnvelopeV1 {
                        from: from.clone(),
                        stealth_view_key: plan.stealth_view_key,
                        stealth_spend_key: plan.stealth_spend_key,
                        value: plan.value,
                        nonce,
                        chain_id,
                        gas_limit: plan.gas_limit,
                        gas_price: plan.gas_price,
                    },
                    PrivacyTxRawSignerV1 {
                        ring_members: &plan.ring_members,
                        signer_index: plan.signer_index,
                        private_key: plan.private_key,
                    },
                )?;
                tap_drain =
                    apply_gateway_evm_runtime_tap(&tx_ir, wants_cross_chain_atomic)?;
                let payload = tx_ir
                    .serialize(SerializationFormat::Bincode)
                    .context("serialize privacy tx ir payload failed")?;
                let tx_hash = vec_to_32(&tx_ir.hash, "privacy_tx_hash")?;
                (
                    payload,
                    tx_hash,
                    "signed_privacy_tx_ir_bincode_v1",
                    Some("privacy"),
                )
            } else {
                let payload = extract_web30_tx_payload(params)?;
                let tx_hash = compute_gateway_web30_tx_hash(&GatewayWeb30TxHashInput {
                    uca_id: &uca_id,
                    chain_id,
                    nonce,
                    from: &from,
                    payload: &payload,
                    signature_domain: &signature_domain,
                    is_raw: false,
                    wants_cross_chain_atomic,
                });
                (payload, tx_hash, "generic_web30_payload_v1", None)
            };
            let record = GatewayIngressWeb30RecordV1 {
                version: GATEWAY_INGRESS_RECORD_VERSION,
                protocol: GATEWAY_INGRESS_PROTOCOL_WEB30,
                uca_id: uca_id.clone(),
                chain_id,
                nonce,
                from,
                payload,
                is_raw: false,
                signature_domain: signature_domain.clone(),
                wants_cross_chain_atomic,
                tx_hash,
                overlay_node_id: ctx.overlay_node_id.clone(),
                overlay_session_id: ctx.overlay_session_id.clone(),
            };
            let wire = encode_gateway_ingress_ops_wire_v1_web30(&record)?;
            let spool_file = write_spool_ops_wire_v1(ctx.spool_dir, &wire)?;
            for settlement in &tap_drain.settlement_records {
                if let Err(e) = upsert_gateway_evm_settlement_index(
                    evm_settlement_index_by_id,
                    evm_settlement_index_by_tx,
                    ctx.eth_tx_index_store,
                    settlement,
                ) {
                    gateway_warn!(
                        "gateway_warn: upsert evm settlement index failed: chain_id={} tx_hash=0x{} settlement_id={} err={}",
                        settlement.income.chain_id,
                        to_hex(&settlement.income.tx_hash),
                        settlement.result.settlement_id,
                        e
                    );
                }
            }
            if let Err(e) =
                persist_gateway_evm_settlement_records(ctx.spool_dir, &tap_drain.settlement_records)
            {
                gateway_warn!(
                    "gateway_warn: persist evm settlement records failed: chain_id={} tx_hash=0x{} count={} err={}",
                    chain_id,
                    to_hex(&record.tx_hash),
                    tap_drain.settlement_records.len(),
                    e
                );
            }
            persist_gateway_payout_with_compensation(
                ctx.spool_dir,
                chain_id,
                &record.tx_hash,
                &tap_drain.payout_instructions,
                evm_settlement_index_by_id,
                evm_pending_payout_by_settlement,
                ctx.eth_tx_index_store,
            );
            persist_gateway_atomic_ready_with_compensation(
                ctx.spool_dir,
                chain_id,
                &record.tx_hash,
                &tap_drain.atomic_ready_items,
                ctx.eth_tx_index_store,
            );

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
                    "tx_hash": format!("0x{}", to_hex(&record.tx_hash)),
                    "spool_file": spool_file.display().to_string(),
                    "ingress_codec": "ops_wire_v1",
                    "payload_kind": payload_kind,
                    "tx_ir_type": tx_ir_type,
                    "overlay_node_id": record.overlay_node_id,
                    "overlay_session_id": record.overlay_session_id,
                }),
                true,
            ))
        }
        _ => bail!(
            "unknown method: {}; valid: novovm_getSurfaceMap|novovm_get_surface_map|novovm_getMethodDomain|novovm_get_method_domain|ua_createUca|ua_rotatePrimaryKey|ua_bindPersona|ua_revokePersona|ua_getBindingOwner|ua_setPolicy|eth_chainId|net_version|web3_clientVersion|web3_sha3|eth_protocolVersion|net_listening|net_peerCount|eth_accounts|eth_coinbase|eth_mining|eth_hashrate|eth_maxPriorityFeePerGas|eth_feeHistory|eth_syncing|eth_pendingTransactions|eth_blockNumber|eth_getBalance|eth_getBlockByNumber|eth_getBlockByHash|eth_getTransactionByBlockNumberAndIndex|eth_getTransactionByBlockHashAndIndex|eth_getBlockTransactionCountByNumber|eth_getBlockTransactionCountByHash|eth_getBlockReceipts|eth_getUncleCountByBlockNumber|eth_getUncleCountByBlockHash|eth_getUncleByBlockNumberAndIndex|eth_getUncleByBlockHashAndIndex|eth_getLogs|eth_subscribe|eth_unsubscribe|eth_newFilter|eth_newBlockFilter|eth_newPendingTransactionFilter|eth_getFilterChanges|eth_getFilterLogs|eth_uninstallFilter|txpool_content|txpool_contentFrom|txpool_inspect|txpool_inspectFrom|txpool_status|txpool_statusFrom|eth_gasPrice|eth_call|eth_estimateGas|eth_getCode|eth_getStorageAt|eth_getProof|eth_sendRawTransaction|eth_sendTransaction|eth_getTransactionCount|eth_getTransactionByHash|eth_getTransactionReceipt|evm_sendRawTransaction|evm_send_raw_transaction|evm_sendTransaction|evm_send_transaction|evm_publicSendRawTransaction|evm_public_send_raw_transaction|evm_publicSendRawTransactionBatch|evm_public_send_raw_transaction_batch|evm_publicSendTransaction|evm_public_send_transaction|evm_publicSendTransactionBatch|evm_public_send_transaction_batch|evm_getLogs|evm_get_logs|evm_getLogsBatch|evm_get_logs_batch|evm_getTransactionReceipt|evm_get_transaction_receipt|evm_getTransactionReceiptBatch|evm_get_transaction_receipt_batch|evm_getTransactionByHashBatch|evm_get_transaction_by_hash_batch|evm_subscribe|evm_unsubscribe|evm_newFilter|evm_new_filter|evm_newBlockFilter|evm_new_block_filter|evm_newPendingTransactionFilter|evm_new_pending_transaction_filter|evm_getFilterChanges|evm_get_filter_changes|evm_getFilterChangesBatch|evm_get_filter_changes_batch|evm_getFilterLogs|evm_get_filter_logs|evm_getFilterLogsBatch|evm_get_filter_logs_batch|evm_uninstallFilter|evm_uninstall_filter|evm_chainId|evm_chain_id|evm_clientVersion|evm_client_version|evm_sha3|evm_protocolVersion|evm_protocol_version|evm_listening|evm_peerCount|evm_peer_count|evm_accounts|evm_coinbase|evm_mining|evm_hashrate|evm_netVersion|evm_net_version|evm_syncing|evm_blockNumber|evm_block_number|evm_getBalance|evm_get_balance|evm_getBlockByNumber|evm_get_block_by_number|evm_getBlockByHash|evm_get_block_by_hash|evm_getBlockReceipts|evm_get_block_receipts|evm_getTransactionByHash|evm_get_transaction_by_hash|evm_getTransactionCount|evm_get_transaction_count|evm_gasPrice|evm_gas_price|evm_call|evm_estimateGas|evm_estimate_gas|evm_getCode|evm_get_code|evm_getStorageAt|evm_get_storage_at|evm_getProof|evm_get_proof|evm_verifyProof|evm_verify_proof|evm_maxPriorityFeePerGas|evm_max_priority_fee_per_gas|evm_feeHistory|evm_fee_history|evm_getTransactionByBlockNumberAndIndex|evm_get_transaction_by_block_number_and_index|evm_getTransactionByBlockHashAndIndex|evm_get_transaction_by_block_hash_and_index|evm_getBlockTransactionCountByNumber|evm_get_block_transaction_count_by_number|evm_getBlockTransactionCountByHash|evm_get_block_transaction_count_by_hash|evm_getUncleCountByBlockNumber|evm_get_uncle_count_by_block_number|evm_getUncleCountByBlockHash|evm_get_uncle_count_by_block_hash|evm_getUncleByBlockNumberAndIndex|evm_get_uncle_by_block_number_and_index|evm_getUncleByBlockHashAndIndex|evm_get_uncle_by_block_hash_and_index|evm_pendingTransactions|evm_pending_transactions|evm_txpoolContent|evm_txpool_content|evm_txpoolContentFrom|evm_txpool_contentFrom|evm_txpool_content_from|evm_txpoolInspect|evm_txpool_inspect|evm_txpoolInspectFrom|evm_txpool_inspectFrom|evm_txpool_inspect_from|evm_txpoolStatus|evm_txpool_status|evm_txpoolStatusFrom|evm_txpool_statusFrom|evm_txpool_status_from|evm_snapshotPendingIngress|evm_snapshot_pending_ingress|evm_snapshotExecutableIngress|evm_snapshot_executable_ingress|evm_drainExecutableIngress|evm_drain_executable_ingress|evm_drainPendingIngress|evm_drain_pending_ingress|evm_snapshotPendingSenderBuckets|evm_snapshot_pending_sender_buckets|evm_getPublicBroadcastStatus|evm_get_public_broadcast_status|evm_getBroadcastStatus|evm_get_broadcast_status|evm_getPublicBroadcastStatusBatch|evm_get_public_broadcast_status_batch|evm_getBroadcastStatusBatch|evm_get_broadcast_status_batch|evm_getUpstreamConsumerBundle|evm_get_upstream_consumer_bundle|evm_getTransactionLifecycleBatch|evm_get_transaction_lifecycle_batch|evm_getTxSubmitStatusBatch|evm_get_tx_submit_status_batch|evm_replayPublicBroadcast|evm_replay_public_broadcast|evm_replayPublicBroadcastBatch|evm_replay_public_broadcast_batch|evm_getTransactionLifecycle|evm_get_transaction_lifecycle|evm_getTxSubmitStatus|evm_get_tx_submit_status|evm_getSettlementById|evm_get_settlement_by_id|evm_getSettlementByTxHash|evm_get_settlement_by_tx_hash|evm_replaySettlementPayout|evm_replay_settlement_payout|evm_getAtomicReadyByIntentId|evm_get_atomic_ready_by_intent_id|evm_replayAtomicReady|evm_replay_atomic_ready|evm_queueAtomicBroadcast|evm_queue_atomic_broadcast|evm_replayAtomicBroadcastQueue|evm_replay_atomic_broadcast_queue|evm_markAtomicBroadcastFailed|evm_mark_atomic_broadcast_failed|evm_markAtomicBroadcasted|evm_mark_atomic_broadcasted|evm_executeAtomicBroadcast|evm_execute_atomic_broadcast|evm_executePendingAtomicBroadcasts|evm_execute_pending_atomic_broadcasts|web30_sendRawTransaction|web30_sendTransaction",
            method
        ),
    }
}

fn force_evm_send_public_broadcast_detail(params: &mut serde_json::Value) {
    match params {
        serde_json::Value::Object(map) => {
            map.insert(
                "require_public_broadcast".to_string(),
                serde_json::Value::Bool(true),
            );
            map.insert("return_detail".to_string(), serde_json::Value::Bool(true));
        }
        serde_json::Value::Array(arr) => {
            if let Some(map) = arr.iter_mut().find_map(serde_json::Value::as_object_mut) {
                map.insert(
                    "require_public_broadcast".to_string(),
                    serde_json::Value::Bool(true),
                );
                map.insert("return_detail".to_string(), serde_json::Value::Bool(true));
            } else {
                arr.insert(
                    0,
                    serde_json::json!({
                        "require_public_broadcast": true,
                        "return_detail": true,
                    }),
                );
            }
        }
        _ => {
            *params = serde_json::json!({
                "require_public_broadcast": true,
                "return_detail": true,
            });
        }
    }
}

#[derive(Debug, Clone)]
struct GatewayVerifyStorageProofItem {
    value: [u8; 32],
    proof_nodes: Vec<Vec<u8>>,
}

#[derive(Debug, Clone)]
struct GatewayVerifyRlpItem {
    raw: Vec<u8>,
    is_list: bool,
    bytes: Option<Vec<u8>>,
}

fn gateway_eth_parse_proof_nodes_for_verify(value: Option<&serde_json::Value>) -> Vec<Vec<u8>> {
    value
        .and_then(serde_json::Value::as_array)
        .map(|items| {
            items
                .iter()
                .filter_map(serde_json::Value::as_str)
                .filter_map(|text| decode_hex_bytes(text, "proof_nodes").ok())
                .collect::<Vec<Vec<u8>>>()
        })
        .unwrap_or_default()
}

fn gateway_eth_parse_storage_proof_map_for_verify(
    value: Option<&serde_json::Value>,
) -> BTreeMap<[u8; 32], GatewayVerifyStorageProofItem> {
    let mut out = BTreeMap::<[u8; 32], GatewayVerifyStorageProofItem>::new();
    let Some(items) = value.and_then(serde_json::Value::as_array) else {
        return out;
    };
    for item in items {
        let Some(map) = item.as_object() else {
            continue;
        };
        let Some(slot) = map
            .get("key")
            .and_then(serde_json::Value::as_str)
            .and_then(parse_storage_key_32)
        else {
            continue;
        };
        let Some(value_word) = map
            .get("value")
            .and_then(serde_json::Value::as_str)
            .and_then(parse_storage_key_32)
        else {
            continue;
        };
        let proof_nodes = gateway_eth_parse_proof_nodes_for_verify(map.get("proof"));
        out.insert(
            slot,
            GatewayVerifyStorageProofItem {
                value: value_word,
                proof_nodes,
            },
        );
    }
    out
}

fn gateway_eth_keccak256_bytes_for_verify(bytes: &[u8]) -> [u8; 32] {
    Keccak256::digest(bytes).into()
}

fn gateway_eth_account_exists_in_entries(
    entries: &[GatewayEthTxIndexEntry],
    address: &[u8],
) -> bool {
    entries.iter().any(|entry| {
        entry.from == address
            || entry.to.as_deref().is_some_and(|to| to == address)
            || (entry.to.is_none()
                && !entry.input.is_empty()
                && gateway_eth_derive_contract_address(&entry.from, entry.nonce) == address)
    })
}

fn gateway_eth_rlp_encode_bytes_for_verify(bytes: &[u8]) -> Vec<u8> {
    if bytes.is_empty() {
        return vec![0x80];
    }
    if bytes.len() == 1 && bytes[0] < 0x80 {
        return vec![bytes[0]];
    }
    if bytes.len() <= 55 {
        let mut out = Vec::with_capacity(1 + bytes.len());
        out.push(0x80 + bytes.len() as u8);
        out.extend_from_slice(bytes);
        return out;
    }
    let len_bytes = gateway_eth_rlp_length_bytes_for_verify(bytes.len());
    let mut out = Vec::with_capacity(1 + len_bytes.len() + bytes.len());
    out.push(0xb7 + len_bytes.len() as u8);
    out.extend_from_slice(&len_bytes);
    out.extend_from_slice(bytes);
    out
}

fn gateway_eth_rlp_length_bytes_for_verify(mut value: usize) -> Vec<u8> {
    let mut out = Vec::new();
    while value > 0 {
        out.push((value & 0xff) as u8);
        value >>= 8;
    }
    if out.is_empty() {
        out.push(0);
    }
    out.reverse();
    out
}

fn gateway_eth_rlp_encode_u64_for_verify(value: u64) -> Vec<u8> {
    if value == 0 {
        return gateway_eth_rlp_encode_bytes_for_verify(&[]);
    }
    let bytes = value.to_be_bytes();
    let first_non_zero = bytes
        .iter()
        .position(|b| *b != 0)
        .unwrap_or(bytes.len() - 1);
    gateway_eth_rlp_encode_bytes_for_verify(&bytes[first_non_zero..])
}

fn gateway_eth_rlp_encode_u128_for_verify(value: u128) -> Vec<u8> {
    if value == 0 {
        return gateway_eth_rlp_encode_bytes_for_verify(&[]);
    }
    let bytes = value.to_be_bytes();
    let first_non_zero = bytes
        .iter()
        .position(|b| *b != 0)
        .unwrap_or(bytes.len() - 1);
    gateway_eth_rlp_encode_bytes_for_verify(&bytes[first_non_zero..])
}

fn gateway_eth_rlp_encode_list_for_verify(items: &[Vec<u8>]) -> Vec<u8> {
    let payload_len = items.iter().map(Vec::len).sum::<usize>();
    let mut out = Vec::with_capacity(payload_len + 8);
    if payload_len <= 55 {
        out.push(0xc0 + payload_len as u8);
    } else {
        let len_bytes = gateway_eth_rlp_length_bytes_for_verify(payload_len);
        out.push(0xf7 + len_bytes.len() as u8);
        out.extend_from_slice(&len_bytes);
    }
    for item in items {
        out.extend_from_slice(item);
    }
    out
}

fn gateway_eth_storage_trie_payload_for_verify(value: [u8; 32]) -> Vec<u8> {
    if value.iter().all(|b| *b == 0) {
        return gateway_eth_rlp_encode_bytes_for_verify(&[]);
    }
    let first_non_zero = value
        .iter()
        .position(|b| *b != 0)
        .unwrap_or(value.len() - 1);
    gateway_eth_rlp_encode_bytes_for_verify(&value[first_non_zero..])
}

fn gateway_eth_account_trie_payload_for_verify(
    nonce: u64,
    balance: u128,
    storage_root: [u8; 32],
    code_hash: [u8; 32],
) -> Vec<u8> {
    gateway_eth_rlp_encode_list_for_verify(&[
        gateway_eth_rlp_encode_u64_for_verify(nonce),
        gateway_eth_rlp_encode_u128_for_verify(balance),
        gateway_eth_rlp_encode_bytes_for_verify(&storage_root),
        gateway_eth_rlp_encode_bytes_for_verify(&code_hash),
    ])
}

fn gateway_eth_parse_be_len_for_verify(bytes: &[u8]) -> Option<usize> {
    let mut out = 0usize;
    for byte in bytes {
        out = out.checked_mul(256)?;
        out = out.checked_add(*byte as usize)?;
    }
    Some(out)
}

fn gateway_eth_rlp_item_header_for_verify(input: &[u8]) -> Option<(usize, usize, bool)> {
    let first = *input.first()?;
    if first <= 0x7f {
        return Some((1, 0, false));
    }
    if first <= 0xb7 {
        let len = (first - 0x80) as usize;
        let total = 1usize.checked_add(len)?;
        if input.len() < total {
            return None;
        }
        return Some((total, 1, false));
    }
    if first <= 0xbf {
        let len_of_len = (first - 0xb7) as usize;
        if input.len() < 1 + len_of_len {
            return None;
        }
        let len = gateway_eth_parse_be_len_for_verify(&input[1..1 + len_of_len])?;
        let total = 1usize.checked_add(len_of_len)?.checked_add(len)?;
        if input.len() < total {
            return None;
        }
        return Some((total, 1 + len_of_len, false));
    }
    if first <= 0xf7 {
        let len = (first - 0xc0) as usize;
        let total = 1usize.checked_add(len)?;
        if input.len() < total {
            return None;
        }
        return Some((total, 1, true));
    }
    let len_of_len = (first - 0xf7) as usize;
    if input.len() < 1 + len_of_len {
        return None;
    }
    let len = gateway_eth_parse_be_len_for_verify(&input[1..1 + len_of_len])?;
    let total = 1usize.checked_add(len_of_len)?.checked_add(len)?;
    if input.len() < total {
        return None;
    }
    Some((total, 1 + len_of_len, true))
}

fn gateway_eth_rlp_decode_list_items_for_verify(raw: &[u8]) -> Option<Vec<GatewayVerifyRlpItem>> {
    let (total, payload_offset, is_list) = gateway_eth_rlp_item_header_for_verify(raw)?;
    if !is_list || total != raw.len() {
        return None;
    }
    let mut cursor = payload_offset;
    let end = total;
    let mut out = Vec::<GatewayVerifyRlpItem>::new();
    while cursor < end {
        let (item_total, item_payload_offset, item_is_list) =
            gateway_eth_rlp_item_header_for_verify(&raw[cursor..end])?;
        let raw_item = raw[cursor..cursor + item_total].to_vec();
        let bytes = if item_is_list {
            None
        } else if item_payload_offset == 0 {
            Some(vec![raw_item[0]])
        } else {
            Some(raw_item[item_payload_offset..].to_vec())
        };
        out.push(GatewayVerifyRlpItem {
            raw: raw_item,
            is_list: item_is_list,
            bytes,
        });
        cursor += item_total;
    }
    if cursor != end {
        return None;
    }
    Some(out)
}

fn gateway_eth_mpt_nibbles_from_key_for_verify(key: &[u8]) -> Vec<u8> {
    let mut out = Vec::with_capacity(key.len() * 2);
    for byte in key {
        out.push(byte >> 4);
        out.push(byte & 0x0f);
    }
    out
}

fn gateway_eth_mpt_hex_prefix_decode_for_verify(compact: &[u8]) -> Option<(bool, Vec<u8>)> {
    let first = *compact.first()?;
    let flag = first >> 4;
    let is_leaf = (flag & 0b10) != 0;
    let odd = (flag & 0b1) != 0;
    let mut out = Vec::<u8>::new();
    if odd {
        out.push(first & 0x0f);
    }
    for byte in compact.iter().skip(1) {
        out.push(byte >> 4);
        out.push(byte & 0x0f);
    }
    Some((is_leaf, out))
}

fn gateway_eth_empty_trie_root_for_verify() -> [u8; 32] {
    decode_hex_bytes(GATEWAY_ETH_EMPTY_TRIE_ROOT, "empty_trie_root")
        .ok()
        .and_then(|bytes| bytes.try_into().ok())
        .unwrap_or([0u8; 32])
}

fn gateway_eth_verify_mpt_proof_value_for_verify(
    key: &[u8],
    expected_root: [u8; 32],
    expected_value: Option<&[u8]>,
    proof_nodes: &[Vec<u8>],
) -> bool {
    if proof_nodes.is_empty() {
        return expected_value.is_none()
            && expected_root == gateway_eth_empty_trie_root_for_verify();
    }
    let root_hash: [u8; 32] = Keccak256::digest(&proof_nodes[0]).into();
    if root_hash != expected_root {
        return false;
    }
    let key_nibbles = gateway_eth_mpt_nibbles_from_key_for_verify(key);
    let mut rest = key_nibbles.as_slice();
    let mut node_idx = 0usize;

    loop {
        let Some(items) = gateway_eth_rlp_decode_list_items_for_verify(&proof_nodes[node_idx])
        else {
            return false;
        };
        if items.len() == 17 {
            if rest.is_empty() {
                let Some(value) = items[16].bytes.as_deref() else {
                    return false;
                };
                return expected_value
                    .map_or_else(|| value.is_empty(), |expected| value == expected);
            }
            let branch_idx = rest[0] as usize;
            rest = &rest[1..];
            let child = &items[branch_idx];
            if child.is_list {
                node_idx += 1;
                if node_idx >= proof_nodes.len() {
                    return false;
                }
                if proof_nodes[node_idx] != child.raw {
                    return false;
                }
                continue;
            }
            let Some(child_bytes) = child.bytes.as_deref() else {
                return false;
            };
            if child_bytes.is_empty() {
                return expected_value.is_none() && node_idx + 1 == proof_nodes.len();
            }
            if child_bytes.len() != 32 {
                return false;
            }
            node_idx += 1;
            if node_idx >= proof_nodes.len() {
                return false;
            }
            let next_hash: [u8; 32] = Keccak256::digest(&proof_nodes[node_idx]).into();
            if next_hash.as_slice() != child_bytes {
                return false;
            }
            continue;
        }
        if items.len() != 2 {
            return false;
        }
        let Some(path_compact) = items[0].bytes.as_deref() else {
            return false;
        };
        let Some((is_leaf, path_nibbles)) =
            gateway_eth_mpt_hex_prefix_decode_for_verify(path_compact)
        else {
            return false;
        };
        if is_leaf {
            if rest != path_nibbles.as_slice() {
                return expected_value.is_none() && node_idx + 1 == proof_nodes.len();
            }
            let Some(value) = items[1].bytes.as_deref() else {
                return false;
            };
            return expected_value.map_or_else(|| value.is_empty(), |expected| value == expected);
        }
        if !rest.starts_with(&path_nibbles) {
            return expected_value.is_none() && node_idx + 1 == proof_nodes.len();
        }
        rest = &rest[path_nibbles.len()..];
        let child = &items[1];
        if child.is_list {
            node_idx += 1;
            if node_idx >= proof_nodes.len() {
                return false;
            }
            if proof_nodes[node_idx] != child.raw {
                return false;
            }
            continue;
        }
        let Some(child_bytes) = child.bytes.as_deref() else {
            return false;
        };
        if child_bytes.is_empty() || child_bytes.len() != 32 {
            return false;
        }
        node_idx += 1;
        if node_idx >= proof_nodes.len() {
            return false;
        }
        let next_hash: [u8; 32] = Keccak256::digest(&proof_nodes[node_idx]).into();
        if next_hash.as_slice() != child_bytes {
            return false;
        }
    }
}

fn gateway_evm_ingress_frame_json(
    frame: &EvmMempoolIngressFrameV1,
    include_raw: bool,
    include_parsed: bool,
) -> serde_json::Value {
    let mut obj = serde_json::Map::new();
    obj.insert(
        "chain_id".to_string(),
        serde_json::Value::String(format!("0x{:x}", frame.chain_id)),
    );
    obj.insert(
        "tx_hash".to_string(),
        serde_json::Value::String(format!("0x{}", to_hex(&frame.tx_hash))),
    );
    obj.insert(
        "observed_at_unix_ms".to_string(),
        serde_json::Value::from(frame.observed_at_unix_ms),
    );
    if include_raw {
        obj.insert(
            "raw_tx".to_string(),
            serde_json::Value::String(format!("0x{}", to_hex(&frame.raw_tx))),
        );
    }
    if include_parsed {
        let parsed = serde_json::to_value(&frame.parsed_tx).unwrap_or(serde_json::Value::Null);
        obj.insert("parsed_tx".to_string(), parsed);
    }
    serde_json::Value::Object(obj)
}

fn gateway_evm_pending_sender_bucket_json(bucket: &EvmPendingSenderBucketV1) -> serde_json::Value {
    serde_json::json!({
        "chain_id": format!("0x{:x}", bucket.chain_id),
        "sender": format!("0x{}", to_hex(&bucket.sender)),
        "txs": bucket.txs,
    })
}

fn gateway_eth_broadcast_status_store(
) -> &'static Mutex<HashMap<[u8; 32], GatewayEthBroadcastStatus>> {
    GATEWAY_ETH_BROADCAST_STATUS_BY_TX.get_or_init(|| Mutex::new(HashMap::new()))
}

fn gateway_eth_submit_status_store() -> &'static Mutex<HashMap<[u8; 32], GatewayEthSubmitStatus>> {
    GATEWAY_ETH_SUBMIT_STATUS_BY_TX.get_or_init(|| Mutex::new(HashMap::new()))
}

fn upsert_gateway_eth_submit_status(
    eth_tx_index_store: &GatewayEthTxIndexStoreBackend,
    tx_hash: [u8; 32],
    status: GatewayEthSubmitStatus,
) {
    if let Ok(mut map) = gateway_eth_submit_status_store().lock() {
        map.insert(tx_hash, status.clone());
    }
    if let Err(e) = eth_tx_index_store.save_eth_submit_status(&tx_hash, &status) {
        gateway_warn!(
            "gateway_warn: persist eth submit-status failed: tx_hash=0x{} backend={} err={}",
            to_hex(&tx_hash),
            eth_tx_index_store.backend_name(),
            e
        );
    }
}

fn persist_gateway_eth_submit_success_status(
    eth_tx_index_store: &GatewayEthTxIndexStoreBackend,
    tx_hash: [u8; 32],
    chain_id: u64,
    pending: bool,
    onchain: bool,
) {
    let status = GatewayEthSubmitStatus {
        chain_id: Some(chain_id),
        accepted: true,
        pending,
        onchain,
        error_code: None,
        error_reason: None,
        updated_at_unix_ms: now_unix_millis(),
    };
    upsert_gateway_eth_submit_status(eth_tx_index_store, tx_hash, status);
}

fn persist_gateway_eth_submit_onchain_status(
    eth_tx_index_store: &GatewayEthTxIndexStoreBackend,
    tx_hash: [u8; 32],
    chain_id: u64,
    onchain_failed: bool,
) {
    let (error_code, error_reason) = if onchain_failed {
        (
            Some("ONCHAIN_FAILED".to_string()),
            Some("transaction failed onchain".to_string()),
        )
    } else {
        (None, None)
    };
    let status = GatewayEthSubmitStatus {
        chain_id: Some(chain_id),
        accepted: true,
        pending: false,
        onchain: true,
        error_code,
        error_reason,
        updated_at_unix_ms: now_unix_millis(),
    };
    upsert_gateway_eth_submit_status(eth_tx_index_store, tx_hash, status);
}

fn gateway_eth_submit_status_by_tx(
    eth_tx_index_store: &GatewayEthTxIndexStoreBackend,
    tx_hash: &[u8; 32],
) -> Option<GatewayEthSubmitStatus> {
    let mut status = if let Ok(map) = gateway_eth_submit_status_store().lock() {
        map.get(tx_hash).cloned()
    } else {
        None
    };
    if status.is_none() {
        if let Ok(loaded) = eth_tx_index_store.load_eth_submit_status(tx_hash) {
            status = loaded;
        }
        if let Some(loaded_status) = status.clone() {
            if let Ok(mut map) = gateway_eth_submit_status_store().lock() {
                map.insert(*tx_hash, loaded_status);
            }
        }
    }
    status
}

fn gateway_eth_broadcast_status_by_tx(
    eth_tx_index_store: &GatewayEthTxIndexStoreBackend,
    tx_hash: &[u8; 32],
) -> Option<GatewayEthBroadcastStatus> {
    let mut status = if let Ok(map) = gateway_eth_broadcast_status_store().lock() {
        map.get(tx_hash).cloned()
    } else {
        None
    };
    if status.is_none() {
        if let Ok(loaded) = eth_tx_index_store.load_eth_broadcast_status(tx_hash) {
            status = loaded;
        }
        if let Some(loaded_status) = status.clone() {
            if let Ok(mut map) = gateway_eth_broadcast_status_store().lock() {
                map.insert(*tx_hash, loaded_status);
            }
        }
    }
    status
}

fn parse_u64_numeric_token(raw: &str) -> Option<u64> {
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

fn parse_named_error_u64_any(message: &str, name: &str) -> Option<u64> {
    parse_named_error_counter(message, name).or_else(|| {
        parse_named_error_token(message, name).and_then(|raw| parse_u64_numeric_token(&raw))
    })
}

fn parse_named_error_hash32(message: &str, name: &str) -> Option<[u8; 32]> {
    let raw = parse_named_error_token(message, name)?;
    let decoded = decode_hex_bytes(&raw, name).ok()?;
    if decoded.len() != 32 {
        return None;
    }
    let mut out = [0u8; 32];
    out.copy_from_slice(&decoded);
    Some(out)
}

fn parse_gateway_eth_tx_hash_from_error(raw_message: &str) -> Option<[u8; 32]> {
    parse_named_error_hash32(raw_message, "tx_hash")
        .or_else(|| parse_named_error_hash32(raw_message, "txHash"))
}

fn parse_gateway_eth_tx_hash_from_params(params: &serde_json::Value) -> Option<[u8; 32]> {
    extract_eth_tx_hash_query_param(params)
        .or_else(|| param_as_string_any_with_tx(params, &["tx_hash", "txHash", "hash"]))
        .and_then(|raw| decode_hex_bytes(&raw, "tx_hash").ok())
        .and_then(|bytes| vec_to_32(&bytes, "tx_hash").ok())
}

fn is_gateway_eth_raw_write_method(method: &str) -> bool {
    method == "eth_sendRawTransaction"
        || method == "evm_sendRawTransaction"
        || method == "evm_send_raw_transaction"
        || method == "evm_publicSendRawTransaction"
        || method == "evm_public_send_raw_transaction"
}

fn is_gateway_eth_send_tx_write_method(method: &str) -> bool {
    method == "eth_sendTransaction"
        || method == "evm_sendTransaction"
        || method == "evm_send_transaction"
        || method == "evm_publicSendTransaction"
        || method == "evm_public_send_transaction"
}

fn gateway_param_as_bool_any(params: &serde_json::Value, keys: &[&str]) -> Option<bool> {
    for key in keys {
        if let Some(value) = param_as_bool(params, key) {
            return Some(value);
        }
    }
    None
}

fn gateway_param_as_string_any(params: &serde_json::Value, keys: &[&str]) -> Option<String> {
    for key in keys {
        if let Some(value) = param_as_string(params, key) {
            return Some(value);
        }
    }
    None
}

fn gateway_ua_route_role_label(role: AccountRole) -> &'static str {
    match role {
        AccountRole::Owner => "owner",
        AccountRole::Delegate => "delegate",
        AccountRole::SessionKey => "session_key",
    }
}

fn gateway_ua_kyc_attestation_message(
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
        gateway_ua_route_role_label(role),
        nonce
    )
}

#[derive(Debug, Clone, Copy)]
struct GatewayKycVerificationOutcome {
    provided: bool,
    verified: bool,
}

fn parse_gateway_ua_kyc_attestor_allowlist() -> Vec<[u8; 32]> {
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

fn resolve_gateway_kyc_verified(
    params: &serde_json::Value,
    uca_id: &str,
    chain_id: u64,
    external_address: &[u8],
    role: AccountRole,
    nonce: u64,
    include_tx_object: bool,
) -> Result<GatewayKycVerificationOutcome> {
    let explicit_bypass = if include_tx_object {
        param_as_bool_any_with_tx(params, &["kyc_verified", "kycVerified"])
    } else {
        gateway_param_as_bool_any(params, &["kyc_verified", "kycVerified"])
    };
    let attestor_pubkey_hex = if include_tx_object {
        param_as_string_any_with_tx(
            params,
            &[
                "kyc_attestor_pubkey",
                "kycAttestorPubkey",
                "kyc_proof_pubkey",
                "kycProofPubkey",
            ],
        )
    } else {
        gateway_param_as_string_any(
            params,
            &[
                "kyc_attestor_pubkey",
                "kycAttestorPubkey",
                "kyc_proof_pubkey",
                "kycProofPubkey",
            ],
        )
    };
    let attestation_sig_hex = if include_tx_object {
        param_as_string_any_with_tx(
            params,
            &[
                "kyc_attestation_sig",
                "kycAttestationSig",
                "kyc_proof_sig",
                "kycProofSig",
            ],
        )
    } else {
        gateway_param_as_string_any(
            params,
            &[
                "kyc_attestation_sig",
                "kycAttestationSig",
                "kyc_proof_sig",
                "kycProofSig",
            ],
        )
    };
    if attestor_pubkey_hex.is_none() && attestation_sig_hex.is_none() {
        if explicit_bypass.unwrap_or(false) {
            bail!(
                "kyc_verified boolean bypass is disabled; provide kyc_attestor_pubkey and kyc_attestation_sig"
            );
        }
        return Ok(GatewayKycVerificationOutcome {
            provided: false,
            verified: false,
        });
    }
    let Some(attestor_pubkey_hex) = attestor_pubkey_hex else {
        return Ok(GatewayKycVerificationOutcome {
            provided: true,
            verified: false,
        });
    };
    let Some(attestation_sig_hex) = attestation_sig_hex else {
        return Ok(GatewayKycVerificationOutcome {
            provided: true,
            verified: false,
        });
    };
    let Ok(attestor_pubkey) = decode_hex_bytes(&attestor_pubkey_hex, "kyc_attestor_pubkey") else {
        return Ok(GatewayKycVerificationOutcome {
            provided: true,
            verified: false,
        });
    };
    if attestor_pubkey.len() != 32 {
        return Ok(GatewayKycVerificationOutcome {
            provided: true,
            verified: false,
        });
    }
    let Ok(attestation_sig) = decode_hex_bytes(&attestation_sig_hex, "kyc_attestation_sig") else {
        return Ok(GatewayKycVerificationOutcome {
            provided: true,
            verified: false,
        });
    };
    if attestation_sig.len() != 64 {
        return Ok(GatewayKycVerificationOutcome {
            provided: true,
            verified: false,
        });
    }
    let mut attestor_pubkey_arr = [0u8; 32];
    attestor_pubkey_arr.copy_from_slice(&attestor_pubkey);
    let allowlist = parse_gateway_ua_kyc_attestor_allowlist();
    if !allowlist.is_empty()
        && !allowlist
            .iter()
            .any(|allowed| allowed == &attestor_pubkey_arr)
    {
        return Ok(GatewayKycVerificationOutcome {
            provided: true,
            verified: false,
        });
    }
    let Ok(verifying_key) = VerifyingKey::from_bytes(&attestor_pubkey_arr) else {
        return Ok(GatewayKycVerificationOutcome {
            provided: true,
            verified: false,
        });
    };
    let Ok(signature) = Ed25519Signature::from_slice(&attestation_sig) else {
        return Ok(GatewayKycVerificationOutcome {
            provided: true,
            verified: false,
        });
    };
    let message =
        gateway_ua_kyc_attestation_message(uca_id, chain_id, external_address, role, nonce);
    let verified = verifying_key.verify(message.as_bytes(), &signature).is_ok();
    Ok(GatewayKycVerificationOutcome {
        provided: true,
        verified,
    })
}

fn infer_gateway_eth_send_tx_uca_id(
    router: Option<&UnifiedAccountRouter>,
    params: &serde_json::Value,
    chain_id: u64,
    from: &[u8],
) -> Option<String> {
    let explicit_uca_id = param_as_string(params, "uca_id");
    if let Some(explicit) = explicit_uca_id {
        if let Some(router) = router {
            let persona = PersonaAddress {
                persona_type: PersonaType::Evm,
                chain_id,
                external_address: from.to_vec(),
            };
            if let Some(owner) = router.resolve_binding_owner(&persona) {
                if owner != explicit {
                    return None;
                }
            }
        }
        return Some(explicit);
    }
    let router = router?;
    let persona = PersonaAddress {
        persona_type: PersonaType::Evm,
        chain_id,
        external_address: from.to_vec(),
    };
    router.resolve_binding_owner(&persona).map(str::to_string)
}

fn recover_gateway_eth_sender_from_signature_param(
    params: &serde_json::Value,
) -> anyhow::Result<Option<Vec<u8>>> {
    let signature =
        match param_as_string_any_with_tx(params, &["signature", "raw_signature", "signed_tx"]) {
            Some(raw_sig) => decode_hex_bytes(&raw_sig, "signature")?,
            None => return Ok(None),
        };
    if signature.is_empty() {
        return Ok(None);
    }
    recover_raw_evm_tx_sender_m0(&signature)
}

fn recover_gateway_eth_signature_raw_fields(
    params: &serde_json::Value,
) -> anyhow::Result<Option<EvmRawTxFieldsM0>> {
    let signature =
        match param_as_string_any_with_tx(params, &["signature", "raw_signature", "signed_tx"]) {
            Some(raw_sig) => decode_hex_bytes(&raw_sig, "signature")?,
            None => return Ok(None),
        };
    if signature.is_empty() {
        return Ok(None);
    }
    let Some(_) = recover_raw_evm_tx_sender_m0(&signature)? else {
        return Ok(None);
    };
    Ok(translate_raw_evm_tx_fields_m0(&signature).ok())
}

fn validate_gateway_eth_send_tx_signature_consistency(
    params: &serde_json::Value,
    effective: &GatewayEthTxHashInput<'_>,
    tx_type: u8,
    tx_type4: bool,
) -> anyhow::Result<()> {
    let Some(fields) = recover_gateway_eth_signature_raw_fields(params)? else {
        return Ok(());
    };
    if let Some(raw_chain_id) = fields.chain_id {
        if raw_chain_id != effective.chain_id {
            bail!(
                "signature chain_id mismatch: effective={} recovered={}",
                effective.chain_id,
                raw_chain_id
            );
        }
    }
    if fields.hint.tx_type_number != tx_type || fields.hint.tx_type4 != tx_type4 {
        bail!(
            "signature tx_type mismatch: effective={} recovered={}",
            tx_type,
            fields.hint.tx_type_number
        );
    }
    if fields.nonce.unwrap_or_default() != effective.nonce {
        bail!(
            "signature nonce mismatch: effective={} recovered={}",
            effective.nonce,
            fields.nonce.unwrap_or_default()
        );
    }
    if fields.gas_limit.unwrap_or_default() != effective.gas_limit {
        bail!(
            "signature gas_limit mismatch: effective={} recovered={}",
            effective.gas_limit,
            fields.gas_limit.unwrap_or_default()
        );
    }
    if fields.gas_price.unwrap_or_default() != effective.gas_price {
        bail!(
            "signature gas_price mismatch: effective={} recovered={}",
            effective.gas_price,
            fields.gas_price.unwrap_or_default()
        );
    }
    if fields.max_priority_fee_per_gas.unwrap_or_default() != effective.max_priority_fee_per_gas {
        bail!(
            "signature max_priority_fee_per_gas mismatch: effective={} recovered={}",
            effective.max_priority_fee_per_gas,
            fields.max_priority_fee_per_gas.unwrap_or_default()
        );
    }
    if fields.access_list_address_count.unwrap_or_default() != effective.access_list_address_count {
        bail!(
            "signature access_list_address_count mismatch: effective={} recovered={}",
            effective.access_list_address_count,
            fields.access_list_address_count.unwrap_or_default()
        );
    }
    if fields.access_list_storage_key_count.unwrap_or_default()
        != effective.access_list_storage_key_count
    {
        bail!(
            "signature access_list_storage_key_count mismatch: effective={} recovered={}",
            effective.access_list_storage_key_count,
            fields.access_list_storage_key_count.unwrap_or_default()
        );
    }
    if fields.max_fee_per_blob_gas.unwrap_or_default() != effective.max_fee_per_blob_gas {
        bail!(
            "signature max_fee_per_blob_gas mismatch: effective={} recovered={}",
            effective.max_fee_per_blob_gas,
            fields.max_fee_per_blob_gas.unwrap_or_default()
        );
    }
    if fields.blob_hash_count.unwrap_or_default() != effective.blob_hash_count {
        bail!(
            "signature blob_hash_count mismatch: effective={} recovered={}",
            effective.blob_hash_count,
            fields.blob_hash_count.unwrap_or_default()
        );
    }
    if fields.value.unwrap_or_default() != effective.value {
        bail!(
            "signature value mismatch: effective={} recovered={}",
            effective.value,
            fields.value.unwrap_or_default()
        );
    }
    if fields.to.as_deref() != effective.to {
        bail!("signature to mismatch with effective request fields");
    }
    if fields.data.as_deref().unwrap_or_default() != effective.data {
        bail!("signature data mismatch with effective request fields");
    }
    Ok(())
}

fn infer_gateway_eth_tx_hash_from_write_params(
    method: &str,
    params: &serde_json::Value,
    default_chain_id: u64,
) -> Option<[u8; 32]> {
    if let Some(tx_hash) = parse_gateway_eth_tx_hash_from_params(params) {
        return Some(tx_hash);
    }
    if !is_gateway_eth_raw_write_method(method) {
        return None;
    }
    let raw_tx_hex = extract_eth_raw_tx_param(params)?;
    let raw_tx = decode_hex_bytes(&raw_tx_hex, "raw_tx").ok()?;
    let fields = translate_raw_evm_tx_fields_m0(&raw_tx).ok()?;
    let explicit_chain_id_present = param_as_u64(params, "chain_id").is_some()
        || param_as_u64(params, "chainId").is_some()
        || param_tx_object(params)
            .and_then(|tx| param_as_u64(tx, "chain_id").or_else(|| param_as_u64(tx, "chainId")))
            .is_some();
    let explicit_chain_id = if explicit_chain_id_present {
        Some(resolve_chain_id_with_tx_consistency(params, default_chain_id).ok()?)
    } else {
        None
    };
    if let (Some(explicit), Some(inferred)) = (explicit_chain_id, fields.chain_id) {
        if explicit != inferred {
            return None;
        }
    }
    let chain_id = explicit_chain_id
        .or(fields.chain_id)
        .unwrap_or(default_chain_id);
    let explicit_from = extract_eth_persona_address_param(params)
        .map(|raw| decode_hex_bytes(&raw, "from"))
        .transpose()
        .ok()?;
    let recovered_from = recover_raw_evm_tx_sender_m0(&raw_tx).ok().flatten();
    let from = match (explicit_from, recovered_from) {
        (Some(explicit), Some(recovered)) => {
            if explicit != recovered {
                return None;
            }
            recovered
        }
        (Some(explicit), None) => explicit,
        (None, Some(recovered)) => recovered,
        (None, None) => return None,
    };
    let tx_ir = tx_ir_from_raw_fields_m0(&fields, &raw_tx, from, chain_id);
    vec_to_32(&tx_ir.hash, "tx_hash").ok()
}

fn infer_gateway_eth_send_tx_hash_from_params(
    router: Option<&UnifiedAccountRouter>,
    params: &serde_json::Value,
    default_chain_id: u64,
) -> Option<[u8; 32]> {
    let chain_id = resolve_chain_id_with_tx_consistency(params, default_chain_id).ok()?;
    let from_raw = extract_eth_persona_address_param(params)?;
    let explicit_from = decode_hex_bytes(&from_raw, "from").ok()?;
    let recovered_from = recover_gateway_eth_sender_from_signature_param(params)
        .ok()
        .flatten();
    let from = match (explicit_from, recovered_from) {
        (explicit, Some(recovered)) => {
            if explicit != recovered {
                return None;
            }
            recovered
        }
        (explicit, None) => explicit,
    };
    let uca_id = infer_gateway_eth_send_tx_uca_id(router, params, chain_id, &from)?;
    let nonce = param_as_u64_any_with_tx(params, &["nonce"])?;
    let to = match param_as_string_any_with_tx(params, &["to"]) {
        Some(raw_to) => {
            let trimmed = raw_to.trim();
            if trimmed.is_empty() || trimmed.eq_ignore_ascii_case("0x") {
                None
            } else {
                Some(decode_hex_bytes(trimmed, "to").ok()?)
            }
        }
        None => None,
    };
    let data = match param_as_string_any_with_tx(params, &["data", "input"]) {
        Some(raw_data) => {
            let trimmed = raw_data.trim();
            if trimmed.is_empty() || trimmed.eq_ignore_ascii_case("0x") {
                Vec::new()
            } else {
                decode_hex_bytes(trimmed, "data").ok()?
            }
        }
        None => Vec::new(),
    };
    let value = param_as_u128_any_with_tx(params, &["value"]).unwrap_or(0);
    let (access_list_address_count, access_list_storage_key_count) =
        parse_eth_access_list_intrinsic_counts(params).ok()?;
    let (max_fee_per_blob_gas, blob_hash_count) = parse_eth_blob_intrinsic_fields(params).ok()?;
    let has_access_list_intrinsic =
        access_list_address_count > 0 || access_list_storage_key_count > 0;
    let gas_limit =
        param_as_u64_any_with_tx(params, &["gas_limit", "gasLimit", "gas"]).unwrap_or(21_000);
    let legacy_gas_price_param = param_as_u64_any_with_tx(params, &["gas_price", "gasPrice"]);
    let max_fee_per_gas_param =
        param_as_u64_any_with_tx(params, &["max_fee_per_gas", "maxFeePerGas"]);
    let max_priority_fee_per_gas_param = param_as_u64_any_with_tx(
        params,
        &["max_priority_fee_per_gas", "maxPriorityFeePerGas"],
    );
    let explicit_tx_type = param_as_u64_any_with_tx(params, &["tx_type", "txType", "type"]);
    let has_eip1559_fee_fields = param_as_u64_any_with_tx(
        params,
        &[
            "max_fee_per_gas",
            "maxFeePerGas",
            "max_priority_fee_per_gas",
            "maxPriorityFeePerGas",
        ],
    )
    .is_some();
    let tx_type = resolve_gateway_eth_write_tx_type(
        chain_id,
        explicit_tx_type,
        has_eip1559_fee_fields,
        has_access_list_intrinsic,
        max_fee_per_blob_gas,
        blob_hash_count,
    )
    .ok()?;
    let (gas_price, max_priority_fee_per_gas) = if tx_type == 2 || tx_type == 3 {
        let max_fee_per_gas = max_fee_per_gas_param?;
        let max_priority_fee_per_gas = max_priority_fee_per_gas_param.unwrap_or(
            gateway_eth_default_max_priority_fee_per_gas_wei(chain_id).min(max_fee_per_gas),
        );
        if max_priority_fee_per_gas > max_fee_per_gas {
            return None;
        }
        if max_fee_per_gas < gateway_eth_base_fee_per_gas_wei(chain_id) {
            return None;
        }
        (max_fee_per_gas, max_priority_fee_per_gas)
    } else {
        (
            legacy_gas_price_param.unwrap_or(1),
            max_priority_fee_per_gas_param.unwrap_or_default(),
        )
    };
    let tx_type4 =
        param_as_bool_any_with_tx(params, &["tx_type4"]).unwrap_or(false) || tx_type == 4;
    let signature =
        match param_as_string_any_with_tx(params, &["signature", "raw_signature", "signed_tx"]) {
            Some(raw_sig) => decode_hex_bytes(&raw_sig, "signature").ok()?,
            None => Vec::new(),
        };
    let signature_domain = param_as_string(params, "signature_domain")
        .or_else(|| param_as_string_any_with_tx(params, &["signature_domain", "signatureDomain"]))
        .unwrap_or_else(|| format!("evm:{chain_id}"));
    let wants_cross_chain_atomic = param_as_bool_any_with_tx(
        params,
        &["wants_cross_chain_atomic", "wantsCrossChainAtomic"],
    )
    .unwrap_or(false);
    let tx_hash_input = GatewayEthTxHashInput {
        uca_id: &uca_id,
        chain_id,
        nonce,
        tx_type,
        tx_type4,
        from: &from,
        to: to.as_deref(),
        value,
        gas_limit,
        gas_price,
        max_priority_fee_per_gas,
        data: &data,
        signature: &signature,
        access_list_address_count,
        access_list_storage_key_count,
        max_fee_per_blob_gas,
        blob_hash_count,
        signature_domain: &signature_domain,
        wants_cross_chain_atomic,
    };
    validate_gateway_eth_send_tx_signature_consistency(params, &tx_hash_input, tx_type, tx_type4)
        .ok()?;
    Some(compute_gateway_eth_tx_hash(&tx_hash_input))
}

fn gateway_eth_submit_error_code_label(code: i64) -> &'static str {
    match code {
        -32034 => "REPLACEMENT_UNDERPRICED",
        -32035 => "NONCE_TOO_LOW",
        -32036 => "ATOMIC_INTENT_REJECTED",
        -32037 => "NONCE_TOO_HIGH",
        -32038 => "TXPOOL_FULL",
        -32039 => "ATOMIC_INTENT_NOT_READY",
        -32040 => "PUBLIC_BROADCAST_FAILED",
        -32033 => "INVALID_PARAMS",
        -32031 => "TX_TYPE_UNSUPPORTED",
        -32030 => "TX_REJECTED",
        _ => "SUBMIT_FAILED",
    }
}

fn is_gateway_eth_write_method(method: &str) -> bool {
    method == "eth_sendRawTransaction"
        || method == "eth_sendTransaction"
        || method == "evm_sendRawTransaction"
        || method == "evm_send_raw_transaction"
        || method == "evm_sendTransaction"
        || method == "evm_send_transaction"
        || method == "evm_publicSendRawTransaction"
        || method == "evm_public_send_raw_transaction"
        || method == "evm_publicSendTransaction"
        || method == "evm_public_send_transaction"
}

#[allow(clippy::too_many_arguments)]
fn persist_gateway_eth_submit_failure_status_from_error(
    eth_tx_index_store: &GatewayEthTxIndexStoreBackend,
    router: Option<&UnifiedAccountRouter>,
    method: &str,
    params: &serde_json::Value,
    raw_message: &str,
    code: i64,
    message: &str,
    default_chain_id: u64,
) {
    if !is_gateway_eth_write_method(method) {
        return;
    }
    let Some(tx_hash) = parse_gateway_eth_tx_hash_from_error(raw_message)
        .or_else(|| infer_gateway_eth_tx_hash_from_write_params(method, params, default_chain_id))
        .or_else(|| {
            if is_gateway_eth_send_tx_write_method(method) {
                infer_gateway_eth_send_tx_hash_from_params(router, params, default_chain_id)
            } else {
                None
            }
        })
    else {
        return;
    };
    let chain_id = param_as_u64_any_with_tx(params, &["chain_id", "chainId"])
        .or_else(|| parse_named_error_u64_any(raw_message, "chain_id"))
        .or(Some(default_chain_id));
    let status = GatewayEthSubmitStatus {
        chain_id,
        accepted: false,
        pending: false,
        onchain: false,
        error_code: Some(gateway_eth_submit_error_code_label(code).to_string()),
        error_reason: Some(message.to_string()),
        updated_at_unix_ms: now_unix_millis(),
    };
    upsert_gateway_eth_submit_status(eth_tx_index_store, tx_hash, status);
}

fn mark_gateway_pending_eth_public_broadcast(
    eth_tx_index_store: &GatewayEthTxIndexStoreBackend,
    chain_id: u64,
    tx_hash: [u8; 32],
) {
    let ticket = GatewayEthPublicBroadcastPendingTicketV1 {
        chain_id,
        tx_hash,
        queued_at_unix_ms: now_unix_millis(),
    };
    if let Err(e) = eth_tx_index_store.save_pending_eth_public_broadcast_ticket(&ticket) {
        gateway_warn!(
            "gateway_warn: persist pending public-broadcast ticket failed: chain_id={} tx_hash=0x{} backend={} err={}",
            chain_id,
            to_hex(&tx_hash),
            eth_tx_index_store.backend_name(),
            e
        );
    }
}

fn clear_gateway_pending_eth_public_broadcast(
    eth_tx_index_store: &GatewayEthTxIndexStoreBackend,
    tx_hash: &[u8; 32],
) {
    if let Err(e) = eth_tx_index_store.delete_pending_eth_public_broadcast_ticket(tx_hash) {
        gateway_warn!(
            "gateway_warn: delete pending public-broadcast ticket failed: tx_hash=0x{} backend={} err={}",
            to_hex(tx_hash),
            eth_tx_index_store.backend_name(),
            e
        );
    }
}

fn upsert_gateway_eth_broadcast_status(
    eth_tx_index_store: &GatewayEthTxIndexStoreBackend,
    chain_id: u64,
    tx_hash: [u8; 32],
    broadcast_result: &Option<(String, u64, String)>,
) {
    let status = match broadcast_result {
        Some((output, attempts, executor)) => {
            let mode = if executor.starts_with("native:") {
                "native"
            } else {
                "external"
            };
            GatewayEthBroadcastStatus {
                mode: mode.to_string(),
                attempts: Some(*attempts),
                executor: Some(executor.clone()),
                executor_output: Some(output.clone()),
                updated_at_unix_ms: now_unix_millis(),
            }
        }
        None => GatewayEthBroadcastStatus {
            mode: "none".to_string(),
            attempts: None,
            executor: None,
            executor_output: None,
            updated_at_unix_ms: now_unix_millis(),
        },
    };
    if let Ok(mut map) = gateway_eth_broadcast_status_store().lock() {
        map.insert(tx_hash, status.clone());
    }
    if let Err(e) = eth_tx_index_store.save_eth_broadcast_status(&tx_hash, &status) {
        gateway_warn!(
            "gateway_warn: persist eth broadcast-status failed: tx_hash=0x{} backend={} err={}",
            to_hex(&tx_hash),
            eth_tx_index_store.backend_name(),
            e
        );
    }
    if status.mode == "none" {
        mark_gateway_pending_eth_public_broadcast(eth_tx_index_store, chain_id, tx_hash);
    } else {
        clear_gateway_pending_eth_public_broadcast(eth_tx_index_store, &tx_hash);
    }
}

fn gateway_eth_broadcast_status_json_by_tx(
    eth_tx_index_store: &GatewayEthTxIndexStoreBackend,
    tx_hash: &[u8; 32],
) -> serde_json::Value {
    let mut status = if let Ok(map) = gateway_eth_broadcast_status_store().lock() {
        map.get(tx_hash).cloned()
    } else {
        None
    };
    if status.is_none() {
        if let Ok(loaded) = eth_tx_index_store.load_eth_broadcast_status(tx_hash) {
            status = loaded;
        }
        if let Some(loaded_status) = status.clone() {
            if let Ok(mut map) = gateway_eth_broadcast_status_store().lock() {
                map.insert(*tx_hash, loaded_status);
            }
        }
    }
    gateway_eth_broadcast_status_json(status.as_ref())
}

fn gateway_eth_broadcast_status_json(
    status: Option<&GatewayEthBroadcastStatus>,
) -> serde_json::Value {
    let Some(status) = status else {
        return serde_json::Value::Null;
    };
    serde_json::json!({
        "mode": status.mode,
        "attempts": status.attempts,
        "executor": status.executor,
        "executor_output": status.executor_output,
        "updated_at_unix_ms": status.updated_at_unix_ms,
    })
}

fn upsert_gateway_evm_settlement_index(
    by_id: &mut HashMap<String, GatewayEvmSettlementIndexEntry>,
    by_tx: &mut HashMap<GatewaySettlementTxKey, String>,
    eth_tx_index_store: &GatewayEthTxIndexStoreBackend,
    record: &EvmFeeSettlementRecordV1,
) -> Result<()> {
    let entry = settlement_index_entry_from_record(record)?;
    let key = GatewaySettlementTxKey {
        chain_id: entry.chain_id,
        tx_hash: entry.income_tx_hash,
    };
    by_tx.insert(key, entry.settlement_id.clone());
    by_id.insert(entry.settlement_id.clone(), entry.clone());
    if let Err(e) = eth_tx_index_store.save_evm_settlement(&entry) {
        gateway_warn!(
            "gateway_warn: persist evm settlement index failed for settlement_id={} chain_id={} tx_hash=0x{} backend={} err={}",
            entry.settlement_id,
            entry.chain_id,
            to_hex(&entry.income_tx_hash),
            eth_tx_index_store.backend_name(),
            e
        );
    }
    Ok(())
}

fn set_gateway_evm_settlement_status(
    by_id: &mut HashMap<String, GatewayEvmSettlementIndexEntry>,
    eth_tx_index_store: &GatewayEthTxIndexStoreBackend,
    settlement_id: &str,
    status: &str,
) {
    let mut updated = if let Some(current) = by_id.get(settlement_id) {
        current.clone()
    } else if let Ok(Some(from_store)) = eth_tx_index_store.load_evm_settlement_by_id(settlement_id)
    {
        from_store
    } else {
        return;
    };
    updated.status = status.to_string();
    by_id.insert(settlement_id.to_string(), updated.clone());
    if let Err(e) = eth_tx_index_store.save_evm_settlement(&updated) {
        gateway_warn!(
            "gateway_warn: persist evm settlement status failed: settlement_id={} status={} backend={} err={}",
            settlement_id,
            status,
            eth_tx_index_store.backend_name(),
            e
        );
    }
}

fn select_atomic_ready_leg<'a>(
    item: &'a AtomicBroadcastReadyV1,
    preferred_chain_id: Option<u64>,
    preferred_tx_hash: Option<&[u8; 32]>,
) -> Option<&'a TxIR> {
    if let Some(preferred_tx_hash) = preferred_tx_hash {
        if let Some(leg) = item.intent.legs.iter().find(|leg| {
            vec_to_32(&leg.hash, "atomic_ready_tx_hash").ok() == Some(*preferred_tx_hash)
        }) {
            return Some(leg);
        }
    }
    if let Some(preferred_chain_id) = preferred_chain_id {
        if let Some(leg) = item
            .intent
            .legs
            .iter()
            .find(|leg| leg.chain_id == preferred_chain_id)
        {
            return Some(leg);
        }
    }
    item.intent.legs.first()
}

fn atomic_ready_tx_ir_bincode_from_item(
    item: &AtomicBroadcastReadyV1,
    preferred_chain_id: Option<u64>,
    preferred_tx_hash: Option<&[u8; 32]>,
) -> Vec<u8> {
    select_atomic_ready_leg(item, preferred_chain_id, preferred_tx_hash)
        .and_then(|leg| leg.serialize(SerializationFormat::Bincode).ok())
        .unwrap_or_default()
}

fn atomic_ready_index_entry_from_item(
    item: &AtomicBroadcastReadyV1,
    status: &str,
    preferred_chain_id: Option<u64>,
    preferred_tx_hash: Option<&[u8; 32]>,
) -> GatewayEvmAtomicReadyIndexEntry {
    let (chain_id, tx_hash) =
        if let Some(leg) = select_atomic_ready_leg(item, preferred_chain_id, preferred_tx_hash) {
            (
                leg.chain_id,
                vec_to_32(&leg.hash, "atomic_ready_tx_hash").unwrap_or([0u8; 32]),
            )
        } else {
            (0, [0u8; 32])
        };
    GatewayEvmAtomicReadyIndexEntry {
        intent_id: item.intent.intent_id.clone(),
        chain_id,
        tx_hash,
        ready_at_unix_ms: item.ready_at_unix_ms,
        status: status.to_string(),
    }
}

fn upsert_gateway_evm_atomic_ready_index(
    eth_tx_index_store: &GatewayEthTxIndexStoreBackend,
    item: &AtomicBroadcastReadyV1,
    status: &str,
    preferred_chain_id: Option<u64>,
    preferred_tx_hash: Option<&[u8; 32]>,
) {
    let entry =
        atomic_ready_index_entry_from_item(item, status, preferred_chain_id, preferred_tx_hash);
    if let Err(e) = eth_tx_index_store.save_evm_atomic_ready(&entry) {
        gateway_warn!(
            "gateway_warn: persist evm atomic-ready index failed: intent_id={} chain_id={} tx_hash=0x{} status={} backend={} err={}",
            entry.intent_id,
            entry.chain_id,
            to_hex(&entry.tx_hash),
            status,
            eth_tx_index_store.backend_name(),
            e
        );
    }
}

fn set_gateway_evm_atomic_ready_status(
    eth_tx_index_store: &GatewayEthTxIndexStoreBackend,
    intent_id: &str,
    status: &str,
) -> bool {
    let Ok(Some(mut entry)) = eth_tx_index_store.load_evm_atomic_ready_by_intent(intent_id) else {
        return false;
    };
    entry.status = status.to_string();
    if let Err(e) = eth_tx_index_store.save_evm_atomic_ready(&entry) {
        gateway_warn!(
            "gateway_warn: persist evm atomic-ready status failed: intent_id={} status={} backend={} err={}",
            intent_id,
            status,
            eth_tx_index_store.backend_name(),
            e
        );
    }
    true
}

fn atomic_broadcast_ticket_from_index_entry(
    entry: &GatewayEvmAtomicReadyIndexEntry,
) -> GatewayEvmAtomicBroadcastTicketV1 {
    GatewayEvmAtomicBroadcastTicketV1 {
        intent_id: entry.intent_id.clone(),
        chain_id: entry.chain_id,
        tx_hash: entry.tx_hash,
        ready_at_unix_ms: entry.ready_at_unix_ms,
    }
}

fn load_atomic_broadcast_tx_ir_bincode_from_pending_ready(
    eth_tx_index_store: &GatewayEthTxIndexStoreBackend,
    intent_id: &str,
    chain_id: u64,
    tx_hash: &[u8; 32],
) -> Option<Vec<u8>> {
    if let Ok(Some(payload)) = eth_tx_index_store.load_atomic_broadcast_payload(intent_id) {
        if !payload.is_empty() {
            return Some(payload);
        }
    }
    let item = eth_tx_index_store
        .load_pending_atomic_ready(intent_id)
        .ok()
        .flatten()?;
    let payload = atomic_ready_tx_ir_bincode_from_item(&item, Some(chain_id), Some(tx_hash));
    if payload.is_empty() {
        None
    } else {
        mark_gateway_pending_atomic_broadcast_payload(eth_tx_index_store, intent_id, &payload);
        Some(payload)
    }
}

fn mark_gateway_pending_atomic_ready(
    eth_tx_index_store: &GatewayEthTxIndexStoreBackend,
    item: &AtomicBroadcastReadyV1,
) {
    if let Err(e) = eth_tx_index_store.save_pending_atomic_ready(item) {
        gateway_warn!(
            "gateway_warn: persist pending atomic-ready failed: intent_id={} backend={} err={}",
            item.intent.intent_id,
            eth_tx_index_store.backend_name(),
            e
        );
    }
}

fn clear_gateway_pending_atomic_ready(
    eth_tx_index_store: &GatewayEthTxIndexStoreBackend,
    intent_id: &str,
) {
    if let Err(e) = eth_tx_index_store.delete_pending_atomic_ready(intent_id) {
        gateway_warn!(
            "gateway_warn: delete pending atomic-ready failed: intent_id={} backend={} err={}",
            intent_id,
            eth_tx_index_store.backend_name(),
            e
        );
    }
}

fn mark_gateway_pending_atomic_broadcast_ticket(
    eth_tx_index_store: &GatewayEthTxIndexStoreBackend,
    ticket: &GatewayEvmAtomicBroadcastTicketV1,
) {
    if let Err(e) = eth_tx_index_store.save_pending_atomic_broadcast_ticket(ticket) {
        gateway_warn!(
            "gateway_warn: persist pending atomic-broadcast ticket failed: intent_id={} backend={} err={}",
            ticket.intent_id,
            eth_tx_index_store.backend_name(),
            e
        );
    }
}

fn clear_gateway_pending_atomic_broadcast_ticket(
    eth_tx_index_store: &GatewayEthTxIndexStoreBackend,
    intent_id: &str,
) {
    if let Err(e) = eth_tx_index_store.delete_pending_atomic_broadcast_ticket(intent_id) {
        gateway_warn!(
            "gateway_warn: delete pending atomic-broadcast ticket failed: intent_id={} backend={} err={}",
            intent_id,
            eth_tx_index_store.backend_name(),
            e
        );
    }
}

fn mark_gateway_pending_atomic_broadcast_payload(
    eth_tx_index_store: &GatewayEthTxIndexStoreBackend,
    intent_id: &str,
    payload: &[u8],
) {
    if payload.is_empty() {
        return;
    }
    if let Err(e) = eth_tx_index_store.save_atomic_broadcast_payload(intent_id, payload) {
        gateway_warn!(
            "gateway_warn: persist atomic-broadcast payload failed: intent_id={} payload_bytes={} backend={} err={}",
            intent_id,
            payload.len(),
            eth_tx_index_store.backend_name(),
            e
        );
    }
}

fn clear_gateway_pending_atomic_broadcast_payload(
    eth_tx_index_store: &GatewayEthTxIndexStoreBackend,
    intent_id: &str,
) {
    if let Err(e) = eth_tx_index_store.delete_atomic_broadcast_payload(intent_id) {
        gateway_warn!(
            "gateway_warn: delete atomic-broadcast payload failed: intent_id={} backend={} err={}",
            intent_id,
            eth_tx_index_store.backend_name(),
            e
        );
    }
}

fn execute_gateway_atomic_broadcast_ticket_native(
    eth_tx_index: &mut HashMap<[u8; 32], GatewayEthTxIndexEntry>,
    evm_settlement_index_by_id: &mut HashMap<String, GatewayEvmSettlementIndexEntry>,
    evm_settlement_index_by_tx: &mut HashMap<GatewaySettlementTxKey, String>,
    evm_pending_payout_by_settlement: &mut HashMap<String, EvmFeePayoutInstructionV1>,
    ctx: &GatewayMethodContext<'_>,
    ticket: &GatewayEvmAtomicBroadcastTicketV1,
    tx_ir_bincode: Option<&[u8]>,
) -> Result<PathBuf> {
    let payload = tx_ir_bincode
        .filter(|v| !v.is_empty())
        .ok_or_else(|| anyhow::anyhow!("missing tx_ir_bincode for native atomic-broadcast"))?;
    let mut tx_ir = decode_gateway_atomic_broadcast_tx_ir_bincode(payload)
        .context("decode native atomic-broadcast tx_ir_bincode failed")?;
    if tx_ir.chain_id == 0 {
        tx_ir.chain_id = ticket.chain_id;
    }
    if tx_ir.chain_id != ticket.chain_id {
        bail!(
            "native atomic-broadcast chain_id mismatch: expected={} actual={}",
            ticket.chain_id,
            tx_ir.chain_id
        );
    }
    if tx_ir.hash.is_empty() {
        tx_ir.hash = ticket.tx_hash.to_vec();
    }
    let tx_hash = vec_to_32(&tx_ir.hash, "native_atomic_broadcast.tx_hash")
        .context("decode native atomic-broadcast tx hash failed")?;
    if tx_hash != ticket.tx_hash {
        bail!(
            "native atomic-broadcast tx_hash mismatch: expected=0x{} actual=0x{}",
            to_hex(&ticket.tx_hash),
            to_hex(&tx_hash)
        );
    }
    if tx_ir.from.is_empty() {
        bail!("native atomic-broadcast tx.from is empty");
    }

    let tap_drain = apply_gateway_evm_runtime_tap(&tx_ir, false)?;
    let record = GatewayIngressEthRecordV1 {
        version: GATEWAY_INGRESS_RECORD_VERSION,
        protocol: GATEWAY_INGRESS_PROTOCOL_ETH,
        uca_id: format!("atomic:{}", ticket.intent_id),
        chain_id: tx_ir.chain_id,
        nonce: tx_ir.nonce,
        tx_type: 0,
        tx_type4: false,
        from: tx_ir.from.clone(),
        to: tx_ir.to.clone(),
        value: tx_ir.value,
        gas_limit: tx_ir.gas_limit,
        gas_price: tx_ir.gas_price,
        data: tx_ir.data.clone(),
        signature: tx_ir.signature.clone(),
        tx_hash,
        signature_domain: "evm:atomic_broadcast".to_string(),
        overlay_node_id: ctx.overlay_node_id.clone(),
        overlay_session_id: ctx.overlay_session_id.clone(),
    };
    let wire = encode_gateway_ingress_ops_wire_v1_eth(&record)?;
    let spool_file = write_spool_ops_wire_v1(ctx.spool_dir, &wire)?;
    upsert_gateway_eth_tx_index(eth_tx_index, ctx.eth_tx_index_store, &record);
    for settlement in &tap_drain.settlement_records {
        if let Err(e) = upsert_gateway_evm_settlement_index(
            evm_settlement_index_by_id,
            evm_settlement_index_by_tx,
            ctx.eth_tx_index_store,
            settlement,
        ) {
            gateway_warn!(
                "gateway_warn: upsert evm settlement index failed: chain_id={} tx_hash=0x{} settlement_id={} err={}",
                settlement.income.chain_id,
                to_hex(&settlement.income.tx_hash),
                settlement.result.settlement_id,
                e
            );
        }
    }
    if let Err(e) =
        persist_gateway_evm_settlement_records(ctx.spool_dir, &tap_drain.settlement_records)
    {
        gateway_warn!(
            "gateway_warn: persist evm settlement records failed: chain_id={} tx_hash=0x{} count={} err={}",
            tx_ir.chain_id,
            to_hex(&record.tx_hash),
            tap_drain.settlement_records.len(),
            e
        );
    }
    persist_gateway_payout_with_compensation(
        ctx.spool_dir,
        tx_ir.chain_id,
        &record.tx_hash,
        &tap_drain.payout_instructions,
        evm_settlement_index_by_id,
        evm_pending_payout_by_settlement,
        ctx.eth_tx_index_store,
    );
    Ok(spool_file)
}

fn execute_gateway_atomic_broadcast_ticket(
    exec_path: &Path,
    ticket: &GatewayEvmAtomicBroadcastTicketV1,
    timeout_ms: u64,
    tx_ir_bincode: Option<&[u8]>,
) -> Result<String> {
    let req = build_gateway_atomic_broadcast_executor_request(ticket, tx_ir_bincode);
    let req_body = serde_json::to_vec(&req)
        .context("serialize evm atomic-broadcast executor request failed")?;
    let mut child = Command::new(exec_path)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .with_context(|| {
            format!(
                "spawn evm atomic-broadcast executor failed: {}",
                exec_path.display()
            )
        })?;
    if let Some(stdin) = child.stdin.as_mut() {
        stdin.write_all(&req_body).with_context(|| {
            format!(
                "write evm atomic-broadcast request into executor stdin failed: {}",
                exec_path.display()
            )
        })?;
    }
    let output = if timeout_ms == 0 {
        child.wait_with_output().with_context(|| {
            format!(
                "wait evm atomic-broadcast executor output failed: {}",
                exec_path.display()
            )
        })?
    } else {
        let timeout = Duration::from_millis(timeout_ms);
        let start = SystemTime::now();
        loop {
            match child.try_wait().with_context(|| {
                format!(
                    "poll evm atomic-broadcast executor failed: {}",
                    exec_path.display()
                )
            })? {
                Some(_) => {
                    break child.wait_with_output().with_context(|| {
                        format!(
                            "read evm atomic-broadcast executor output failed: {}",
                            exec_path.display()
                        )
                    })?;
                }
                None => {
                    if start.elapsed().unwrap_or_else(|_| Duration::from_millis(0)) >= timeout {
                        let _ = child.kill();
                        let timed_out_output = child.wait_with_output().with_context(|| {
                            format!(
                                "read timed-out evm atomic-broadcast executor output failed: {}",
                                exec_path.display()
                            )
                        })?;
                        let stderr = String::from_utf8_lossy(&timed_out_output.stderr);
                        bail!(
                            "evm atomic-broadcast executor timed out: timeout_ms={} stderr={}",
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
            "evm atomic-broadcast executor exit={} stderr={}",
            output.status,
            stderr.trim()
        );
    }
    let stdout = String::from_utf8_lossy(&output.stdout).trim().to_string();
    validate_gateway_atomic_broadcast_executor_output(&stdout, ticket)?;
    Ok(stdout)
}

fn execute_gateway_atomic_broadcast_ticket_with_retry(
    exec_path: &Path,
    ticket: &GatewayEvmAtomicBroadcastTicketV1,
    retry: u64,
    timeout_ms: u64,
    retry_backoff_ms: u64,
    tx_ir_bincode: Option<&[u8]>,
) -> std::result::Result<(String, u64), (anyhow::Error, u64)> {
    let mut attempts = 0u64;
    loop {
        attempts = attempts.saturating_add(1);
        match execute_gateway_atomic_broadcast_ticket(exec_path, ticket, timeout_ms, tx_ir_bincode)
        {
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

fn mark_gateway_pending_payout(
    pending: &mut HashMap<String, EvmFeePayoutInstructionV1>,
    eth_tx_index_store: &GatewayEthTxIndexStoreBackend,
    instruction: &EvmFeePayoutInstructionV1,
) {
    pending.insert(instruction.settlement_id.clone(), instruction.clone());
    if let Err(e) = eth_tx_index_store.save_pending_payout_instruction(instruction) {
        gateway_warn!(
            "gateway_warn: persist pending payout failed: settlement_id={} chain_id={} backend={} err={}",
            instruction.settlement_id,
            instruction.chain_id,
            eth_tx_index_store.backend_name(),
            e
        );
    }
}

fn clear_gateway_pending_payout(
    pending: &mut HashMap<String, EvmFeePayoutInstructionV1>,
    eth_tx_index_store: &GatewayEthTxIndexStoreBackend,
    settlement_id: &str,
) {
    pending.remove(settlement_id);
    if let Err(e) = eth_tx_index_store.delete_pending_payout_instruction(settlement_id) {
        gateway_warn!(
            "gateway_warn: delete pending payout failed: settlement_id={} backend={} err={}",
            settlement_id,
            eth_tx_index_store.backend_name(),
            e
        );
    }
}

fn auto_replay_pending_payouts(runtime: &mut GatewayRuntime) {
    if runtime.evm_payout_autoreplay_max == 0 || runtime.evm_pending_payout_by_settlement.is_empty()
    {
        return;
    }
    let now_ms = now_unix_millis();
    if runtime.evm_payout_pending_warn_threshold > 0
        && runtime.evm_pending_payout_by_settlement.len()
            >= runtime.evm_payout_pending_warn_threshold
    {
        let due_warn = runtime.evm_payout_last_warn_at_ms == 0
            || now_ms
                >= runtime
                    .evm_payout_last_warn_at_ms
                    .saturating_add(runtime.evm_payout_autoreplay_cooldown_ms as u128);
        if due_warn {
            runtime.evm_payout_last_warn_at_ms = now_ms;
            gateway_warn!(
                "gateway_warn: pending payout backlog reached threshold: pending_total={} threshold={} autoreplay_max={} cooldown_ms={}",
                runtime.evm_pending_payout_by_settlement.len(),
                runtime.evm_payout_pending_warn_threshold,
                runtime.evm_payout_autoreplay_max,
                runtime.evm_payout_autoreplay_cooldown_ms
            );
        }
    }
    if runtime.evm_payout_autoreplay_cooldown_ms > 0
        && runtime.evm_payout_last_autoreplay_at_ms > 0
        && now_ms
            < runtime
                .evm_payout_last_autoreplay_at_ms
                .saturating_add(runtime.evm_payout_autoreplay_cooldown_ms as u128)
    {
        return;
    }
    runtime.evm_payout_last_autoreplay_at_ms = now_ms;
    let mut candidates: Vec<EvmFeePayoutInstructionV1> = runtime
        .evm_pending_payout_by_settlement
        .values()
        .cloned()
        .collect();
    candidates.sort_by(|a, b| {
        a.generated_at_unix_ms
            .cmp(&b.generated_at_unix_ms)
            .then_with(|| a.chain_id.cmp(&b.chain_id))
            .then_with(|| a.settlement_id.cmp(&b.settlement_id))
    });
    let max = runtime.evm_payout_autoreplay_max.min(candidates.len());
    let mut replayed = 0usize;
    let mut failed = 0usize;
    for instruction in candidates.into_iter().take(max) {
        let persisted = persist_gateway_evm_payout_instructions(
            runtime.spool_dir.as_path(),
            std::slice::from_ref(&instruction),
        );
        if persisted.is_ok() {
            clear_gateway_pending_payout(
                &mut runtime.evm_pending_payout_by_settlement,
                &runtime.eth_tx_index_store,
                &instruction.settlement_id,
            );
            set_gateway_evm_settlement_status(
                &mut runtime.evm_settlement_index_by_id,
                &runtime.eth_tx_index_store,
                &instruction.settlement_id,
                EVM_SETTLEMENT_STATUS_COMPENSATED_V1,
            );
            replayed = replayed.saturating_add(1);
        } else {
            failed = failed.saturating_add(1);
            break;
        }
    }
    if failed > 0 {
        gateway_warn!(
            "gateway_warn: auto replay pending payouts partially failed: replayed={} failed={} pending_total={} max={}",
            replayed,
            failed,
            runtime.evm_pending_payout_by_settlement.len(),
            runtime.evm_payout_autoreplay_max
        );
    }
}

fn auto_replay_pending_atomic_broadcasts(runtime: &mut GatewayRuntime) {
    if runtime.evm_atomic_broadcast_autoreplay_max == 0 {
        return;
    }
    let now_ms = now_unix_millis();
    if runtime.evm_atomic_broadcast_autoreplay_cooldown_ms > 0
        && runtime.evm_atomic_broadcast_last_autoreplay_at_ms > 0
        && now_ms
            < runtime
                .evm_atomic_broadcast_last_autoreplay_at_ms
                .saturating_add(runtime.evm_atomic_broadcast_autoreplay_cooldown_ms as u128)
    {
        return;
    }
    let hard_cap = gateway_evm_atomic_broadcast_exec_batch_hard_max() as usize;
    let scan_limit = runtime
        .evm_atomic_broadcast_autoreplay_max
        .max(runtime.evm_atomic_broadcast_pending_warn_threshold.max(1))
        .min(hard_cap);
    auto_replay_pending_atomic_ready_items(runtime, scan_limit);
    let tickets = match runtime
        .eth_tx_index_store
        .load_pending_atomic_broadcast_tickets(scan_limit)
    {
        Ok(items) => items,
        Err(e) => {
            gateway_warn!(
                "gateway_warn: auto replay pending atomic-broadcast tickets load failed: backend={} err={}",
                runtime.eth_tx_index_store.backend_name(),
                e
            );
            return;
        }
    };
    if tickets.is_empty() {
        return;
    }
    runtime.evm_atomic_broadcast_last_autoreplay_at_ms = now_ms;
    if runtime.evm_atomic_broadcast_pending_warn_threshold > 0
        && tickets.len() >= runtime.evm_atomic_broadcast_pending_warn_threshold
    {
        let due_warn = runtime.evm_atomic_broadcast_last_warn_at_ms == 0
            || now_ms
                >= runtime
                    .evm_atomic_broadcast_last_warn_at_ms
                    .saturating_add(runtime.evm_atomic_broadcast_autoreplay_cooldown_ms as u128);
        if due_warn {
            runtime.evm_atomic_broadcast_last_warn_at_ms = now_ms;
            gateway_warn!(
                "gateway_warn: pending atomic-broadcast backlog reached threshold: pending_total={} threshold={} autoreplay_max={} cooldown_ms={}",
                tickets.len(),
                runtime.evm_atomic_broadcast_pending_warn_threshold,
                runtime.evm_atomic_broadcast_autoreplay_max,
                runtime.evm_atomic_broadcast_autoreplay_cooldown_ms
            );
        }
    }
    let mut tickets = tickets;
    tickets.sort_by(|a, b| {
        a.ready_at_unix_ms
            .cmp(&b.ready_at_unix_ms)
            .then_with(|| a.chain_id.cmp(&b.chain_id))
            .then_with(|| a.intent_id.cmp(&b.intent_id))
    });
    let max = runtime
        .evm_atomic_broadcast_autoreplay_max
        .min(tickets.len());
    let mut executed = 0usize;
    let mut failed = 0usize;
    let mut total_attempts = 0u64;
    for ticket in tickets.into_iter().take(max) {
        let tx_ir_bincode = load_atomic_broadcast_tx_ir_bincode_from_pending_ready(
            &runtime.eth_tx_index_store,
            &ticket.intent_id,
            ticket.chain_id,
            &ticket.tx_hash,
        );
        let exec_result = if runtime.evm_atomic_broadcast_autoreplay_use_external_executor {
            let Some(exec_path) = gateway_evm_atomic_broadcast_exec_path(ticket.chain_id) else {
                mark_gateway_pending_atomic_broadcast_ticket(&runtime.eth_tx_index_store, &ticket);
                let _ = set_gateway_evm_atomic_ready_status(
                    &runtime.eth_tx_index_store,
                    &ticket.intent_id,
                    EVM_ATOMIC_READY_STATUS_BROADCAST_FAILED_V1,
                );
                failed = failed.saturating_add(1);
                continue;
            };
            let retry = gateway_evm_atomic_broadcast_exec_retry_default(ticket.chain_id);
            let timeout_ms = gateway_evm_atomic_broadcast_exec_timeout_ms_default(ticket.chain_id);
            let retry_backoff_ms =
                gateway_evm_atomic_broadcast_exec_retry_backoff_ms_default(ticket.chain_id);
            execute_gateway_atomic_broadcast_ticket_with_retry(
                exec_path.as_path(),
                &ticket,
                retry,
                timeout_ms,
                retry_backoff_ms,
                tx_ir_bincode.as_deref(),
            )
            .map(|(_, attempts)| attempts)
            .map_err(|(_, attempts)| attempts)
        } else {
            let mut dummy_filters = GatewayEthFilterState::default();
            let ctx = GatewayMethodContext {
                eth_tx_index_store: &runtime.eth_tx_index_store,
                eth_default_chain_id: runtime.eth_default_chain_id,
                spool_dir: runtime.spool_dir.as_path(),
                overlay_node_id: "reconcile:auto".to_string(),
                overlay_session_id: format!("reconcile-{}", now_unix_millis()),
                eth_filters: &mut dummy_filters,
            };
            execute_gateway_atomic_broadcast_ticket_native(
                &mut runtime.eth_tx_index,
                &mut runtime.evm_settlement_index_by_id,
                &mut runtime.evm_settlement_index_by_tx,
                &mut runtime.evm_pending_payout_by_settlement,
                &ctx,
                &ticket,
                tx_ir_bincode.as_deref(),
            )
            .map(|_| 1u64)
            .map_err(|_| 1u64)
        };
        match exec_result {
            Ok(attempts) => {
                total_attempts = total_attempts.saturating_add(attempts);
                clear_gateway_pending_atomic_broadcast_ticket(
                    &runtime.eth_tx_index_store,
                    &ticket.intent_id,
                );
                clear_gateway_pending_atomic_broadcast_payload(
                    &runtime.eth_tx_index_store,
                    &ticket.intent_id,
                );
                let _ = set_gateway_evm_atomic_ready_status(
                    &runtime.eth_tx_index_store,
                    &ticket.intent_id,
                    EVM_ATOMIC_READY_STATUS_BROADCASTED_V1,
                );
                executed = executed.saturating_add(1);
            }
            Err(attempts) => {
                total_attempts = total_attempts.saturating_add(attempts);
                mark_gateway_pending_atomic_broadcast_ticket(&runtime.eth_tx_index_store, &ticket);
                let _ = set_gateway_evm_atomic_ready_status(
                    &runtime.eth_tx_index_store,
                    &ticket.intent_id,
                    EVM_ATOMIC_READY_STATUS_BROADCAST_FAILED_V1,
                );
                failed = failed.saturating_add(1);
            }
        }
    }
    if failed > 0 {
        gateway_warn!(
            "gateway_warn: auto replay pending atomic-broadcasts partially failed: executed={} failed={} max={} attempts={}",
            executed,
            failed,
            runtime.evm_atomic_broadcast_autoreplay_max,
            total_attempts
        );
    }
}

fn auto_replay_pending_atomic_ready_items(runtime: &mut GatewayRuntime, max_items: usize) {
    if max_items == 0 {
        return;
    }
    let items = match runtime
        .eth_tx_index_store
        .load_pending_atomic_readies(max_items)
    {
        Ok(v) => v,
        Err(e) => {
            gateway_warn!(
                "gateway_warn: auto replay pending atomic-ready load failed: backend={} err={}",
                runtime.eth_tx_index_store.backend_name(),
                e
            );
            return;
        }
    };
    if items.is_empty() {
        return;
    }
    let mut replayed = 0usize;
    let mut failed = 0usize;
    for item in items {
        let entry = atomic_ready_index_entry_from_item(
            &item,
            EVM_ATOMIC_READY_STATUS_COMPENSATE_PENDING_V1,
            None,
            None,
        );
        if entry.chain_id == 0 || entry.tx_hash == [0u8; 32] {
            failed = failed.saturating_add(1);
            continue;
        }
        persist_gateway_atomic_ready_with_compensation(
            runtime.spool_dir.as_path(),
            entry.chain_id,
            &entry.tx_hash,
            std::slice::from_ref(&item),
            &runtime.eth_tx_index_store,
        );
        match runtime
            .eth_tx_index_store
            .load_pending_atomic_ready(&entry.intent_id)
        {
            Ok(None) => replayed = replayed.saturating_add(1),
            _ => failed = failed.saturating_add(1),
        }
    }
    if failed > 0 {
        gateway_warn!(
            "gateway_warn: auto replay pending atomic-ready partially failed: replayed={} failed={} max={}",
            replayed,
            failed,
            max_items
        );
    }
}

fn auto_replay_pending_public_broadcasts(runtime: &mut GatewayRuntime) {
    if runtime.eth_public_broadcast_autoreplay_max == 0 {
        return;
    }
    let now_ms = now_unix_millis();
    if runtime.eth_public_broadcast_autoreplay_cooldown_ms > 0
        && runtime.eth_public_broadcast_last_autoreplay_at_ms > 0
        && now_ms
            < runtime
                .eth_public_broadcast_last_autoreplay_at_ms
                .saturating_add(runtime.eth_public_broadcast_autoreplay_cooldown_ms as u128)
    {
        return;
    }

    let scan_limit = runtime
        .eth_public_broadcast_autoreplay_max
        .max(runtime.eth_public_broadcast_pending_warn_threshold.max(1))
        .min(4096);
    let mut seen_hashes = BTreeSet::<[u8; 32]>::new();
    let mut candidates = Vec::<GatewayEthTxIndexEntry>::new();
    // Prefer explicit pending queue to avoid scanning the whole tx-index on every replay tick.
    let pending_tickets = match runtime
        .eth_tx_index_store
        .load_pending_eth_public_broadcast_tickets(scan_limit)
    {
        Ok(items) => items,
        Err(e) => {
            gateway_warn!(
                "gateway_warn: load pending public-broadcast tickets failed: backend={} err={}",
                runtime.eth_tx_index_store.backend_name(),
                e
            );
            Vec::new()
        }
    };
    for ticket in pending_tickets {
        if !seen_hashes.insert(ticket.tx_hash) {
            continue;
        }
        let still_pending = match gateway_eth_broadcast_status_by_tx(
            &runtime.eth_tx_index_store,
            &ticket.tx_hash,
        ) {
            None => true,
            Some(status) => status.mode == "none",
        };
        if !still_pending {
            clear_gateway_pending_eth_public_broadcast(
                &runtime.eth_tx_index_store,
                &ticket.tx_hash,
            );
            continue;
        }
        let entry = runtime
            .eth_tx_index
            .get(&ticket.tx_hash)
            .cloned()
            .or_else(|| {
                runtime
                    .eth_tx_index_store
                    .load_eth_tx(&ticket.tx_hash)
                    .ok()
                    .flatten()
            });
        if let Some(entry) = entry {
            candidates.push(entry);
        }
        if candidates.len() >= scan_limit {
            break;
        }
    }

    // Compatibility fallback: migrate legacy "broadcast_status=none" records into explicit queue.
    if candidates.len() < scan_limit {
        let mut chain_ids = BTreeSet::<u64>::new();
        chain_ids.insert(runtime.eth_default_chain_id);
        for entry in runtime.eth_tx_index.values() {
            chain_ids.insert(entry.chain_id);
        }
        for chain_id in chain_ids {
            let entries = match runtime
                .eth_tx_index_store
                .load_eth_txs_by_chain(chain_id, scan_limit)
            {
                Ok(items) => items,
                Err(e) => {
                    gateway_warn!(
                        "gateway_warn: auto replay public-broadcast tx load failed: chain_id={} backend={} err={}",
                        chain_id,
                        runtime.eth_tx_index_store.backend_name(),
                        e
                    );
                    continue;
                }
            };
            for entry in entries {
                if !seen_hashes.insert(entry.tx_hash) {
                    continue;
                }
                let pending = match gateway_eth_broadcast_status_by_tx(
                    &runtime.eth_tx_index_store,
                    &entry.tx_hash,
                ) {
                    None => true,
                    Some(status) => status.mode == "none",
                };
                if !pending {
                    continue;
                }
                mark_gateway_pending_eth_public_broadcast(
                    &runtime.eth_tx_index_store,
                    entry.chain_id,
                    entry.tx_hash,
                );
                candidates.push(entry);
                if candidates.len() >= scan_limit {
                    break;
                }
            }
            if candidates.len() >= scan_limit {
                break;
            }
        }
    }
    if candidates.is_empty() {
        return;
    }

    runtime.eth_public_broadcast_last_autoreplay_at_ms = now_ms;
    if runtime.eth_public_broadcast_pending_warn_threshold > 0
        && candidates.len() >= runtime.eth_public_broadcast_pending_warn_threshold
    {
        let due_warn = runtime.eth_public_broadcast_last_warn_at_ms == 0
            || now_ms
                >= runtime
                    .eth_public_broadcast_last_warn_at_ms
                    .saturating_add(runtime.eth_public_broadcast_autoreplay_cooldown_ms as u128);
        if due_warn {
            runtime.eth_public_broadcast_last_warn_at_ms = now_ms;
            gateway_warn!(
                "gateway_warn: pending public-broadcast backlog reached threshold: pending_total={} threshold={} autoreplay_max={} cooldown_ms={}",
                candidates.len(),
                runtime.eth_public_broadcast_pending_warn_threshold,
                runtime.eth_public_broadcast_autoreplay_max,
                runtime.eth_public_broadcast_autoreplay_cooldown_ms
            );
        }
    }

    candidates.sort_by(|a, b| {
        a.chain_id
            .cmp(&b.chain_id)
            .then_with(|| a.nonce.cmp(&b.nonce))
            .then_with(|| a.tx_hash.cmp(&b.tx_hash))
    });
    let max = runtime
        .eth_public_broadcast_autoreplay_max
        .min(candidates.len());
    let mut replayed = 0usize;
    let mut failed = 0usize;
    let mut unavailable = 0usize;
    for entry in candidates.into_iter().take(max) {
        let tx_ir = gateway_eth_tx_ir_from_index_entry(&entry);
        let tx_ir_bincode = match tx_ir.serialize(SerializationFormat::Bincode) {
            Ok(v) => v,
            Err(_) => {
                failed = failed.saturating_add(1);
                continue;
            }
        };
        match maybe_execute_gateway_eth_public_broadcast(
            entry.chain_id,
            &entry.tx_hash,
            GatewayEthPublicBroadcastPayload {
                raw_tx: None,
                tx_ir_bincode: Some(tx_ir_bincode.as_slice()),
            },
            false,
        ) {
            Ok(result) => {
                upsert_gateway_eth_broadcast_status(
                    &runtime.eth_tx_index_store,
                    entry.chain_id,
                    entry.tx_hash,
                    &result,
                );
                if result.is_some() {
                    replayed = replayed.saturating_add(1);
                } else {
                    unavailable = unavailable.saturating_add(1);
                }
            }
            Err(_) => {
                failed = failed.saturating_add(1);
            }
        }
    }
    if failed > 0 || unavailable > 0 {
        gateway_warn!(
            "gateway_warn: auto replay pending public-broadcast partially failed: replayed={} unavailable={} failed={} max={}",
            replayed,
            unavailable,
            failed,
            runtime.eth_public_broadcast_autoreplay_max
        );
    }
}

fn persist_gateway_payout_with_compensation(
    spool_dir: &Path,
    chain_id: u64,
    tx_hash: &[u8; 32],
    instructions: &[EvmFeePayoutInstructionV1],
    settlement_index_by_id: &mut HashMap<String, GatewayEvmSettlementIndexEntry>,
    pending_payout_by_settlement: &mut HashMap<String, EvmFeePayoutInstructionV1>,
    eth_tx_index_store: &GatewayEthTxIndexStoreBackend,
) {
    if instructions.is_empty() {
        return;
    }
    match persist_gateway_evm_payout_instructions(spool_dir, instructions) {
        Ok(()) => {
            for instruction in instructions {
                clear_gateway_pending_payout(
                    pending_payout_by_settlement,
                    eth_tx_index_store,
                    &instruction.settlement_id,
                );
                set_gateway_evm_settlement_status(
                    settlement_index_by_id,
                    eth_tx_index_store,
                    &instruction.settlement_id,
                    EVM_SETTLEMENT_STATUS_PAYOUT_SPOOLED_V1,
                );
            }
        }
        Err(e) => {
            gateway_warn!(
                "gateway_warn: persist evm payout instructions failed: chain_id={} tx_hash=0x{} count={} err={}",
                chain_id,
                to_hex(tx_hash),
                instructions.len(),
                e
            );
            for instruction in instructions {
                mark_gateway_pending_payout(
                    pending_payout_by_settlement,
                    eth_tx_index_store,
                    instruction,
                );
                set_gateway_evm_settlement_status(
                    settlement_index_by_id,
                    eth_tx_index_store,
                    &instruction.settlement_id,
                    EVM_SETTLEMENT_STATUS_COMPENSATE_PENDING_V1,
                );
            }
        }
    }
}

fn persist_gateway_atomic_ready_with_compensation(
    spool_dir: &Path,
    chain_id: u64,
    tx_hash: &[u8; 32],
    ready_items: &[AtomicBroadcastReadyV1],
    eth_tx_index_store: &GatewayEthTxIndexStoreBackend,
) {
    if ready_items.is_empty() {
        return;
    }
    match persist_gateway_evm_atomic_ready(spool_dir, ready_items) {
        Ok(()) => {
            let mut tickets = Vec::with_capacity(ready_items.len());
            for item in ready_items {
                clear_gateway_pending_atomic_ready(eth_tx_index_store, &item.intent.intent_id);
                clear_gateway_pending_atomic_broadcast_ticket(
                    eth_tx_index_store,
                    &item.intent.intent_id,
                );
                upsert_gateway_evm_atomic_ready_index(
                    eth_tx_index_store,
                    item,
                    EVM_ATOMIC_READY_STATUS_SPOOLED_V1,
                    Some(chain_id),
                    Some(tx_hash),
                );
                let indexed = atomic_ready_index_entry_from_item(
                    item,
                    EVM_ATOMIC_READY_STATUS_SPOOLED_V1,
                    Some(chain_id),
                    Some(tx_hash),
                );
                tickets.push(atomic_broadcast_ticket_from_index_entry(&indexed));
                let tx_ir_bincode =
                    atomic_ready_tx_ir_bincode_from_item(item, Some(chain_id), Some(tx_hash));
                mark_gateway_pending_atomic_broadcast_payload(
                    eth_tx_index_store,
                    &indexed.intent_id,
                    &tx_ir_bincode,
                );
            }
            if let Err(e) = persist_gateway_evm_atomic_broadcast_queue(spool_dir, &tickets) {
                gateway_warn!(
                    "gateway_warn: persist evm atomic-broadcast queue failed: chain_id={} tx_hash=0x{} count={} err={}",
                    chain_id,
                    to_hex(tx_hash),
                    tickets.len(),
                    e
                );
                for ticket in &tickets {
                    mark_gateway_pending_atomic_broadcast_ticket(eth_tx_index_store, ticket);
                }
                for item in ready_items {
                    upsert_gateway_evm_atomic_ready_index(
                        eth_tx_index_store,
                        item,
                        EVM_ATOMIC_READY_STATUS_BROADCAST_FAILED_V1,
                        Some(chain_id),
                        Some(tx_hash),
                    );
                }
            } else {
                for ticket in &tickets {
                    let _ = set_gateway_evm_atomic_ready_status(
                        eth_tx_index_store,
                        &ticket.intent_id,
                        EVM_ATOMIC_READY_STATUS_BROADCAST_QUEUED_V1,
                    );
                    mark_gateway_pending_atomic_broadcast_ticket(eth_tx_index_store, ticket);
                }
            }
        }
        Err(e) => {
            gateway_warn!(
                "gateway_warn: persist evm atomic-ready records failed: chain_id={} tx_hash=0x{} count={} err={}",
                chain_id,
                to_hex(tx_hash),
                ready_items.len(),
                e
            );
            for item in ready_items {
                mark_gateway_pending_atomic_ready(eth_tx_index_store, item);
                upsert_gateway_evm_atomic_ready_index(
                    eth_tx_index_store,
                    item,
                    EVM_ATOMIC_READY_STATUS_COMPENSATE_PENDING_V1,
                    Some(chain_id),
                    Some(tx_hash),
                );
            }
        }
    }
}

fn write_spool_ops_wire_v1(spool_dir: &Path, bytes: &[u8]) -> Result<PathBuf> {
    ensure_dir(spool_dir, "gateway spool dir")?;
    let now_ms = now_unix_millis();
    let seq = SPOOL_SEQ.fetch_add(1, Ordering::Relaxed);
    let base = format!("ingress-{now_ms}-{seq}");
    let tmp_path = spool_dir.join(format!("{base}.opsw1.tmp"));
    let out_path = spool_dir.join(format!("{base}.opsw1"));
    fs::write(&tmp_path, bytes).with_context(|| {
        format!(
            "write gateway spool temp file failed: {}",
            tmp_path.display()
        )
    })?;
    fs::rename(&tmp_path, &out_path).with_context(|| {
        format!(
            "atomic rename gateway spool file failed: {} -> {}",
            tmp_path.display(),
            out_path.display()
        )
    })?;
    Ok(out_path)
}

#[cfg(test)]
mod main_tests;
