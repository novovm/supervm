#![forbid(unsafe_code)]

use anyhow::{bail, Context, Result};
use novovm_adapter_api::{
    AccountPolicy, AccountRole, AtomicBroadcastReadyV1, AtomicIntentReceiptV1, AtomicIntentStatus,
    EvmFeePayoutInstructionV1, EvmFeeSettlementRecordV1, EvmMempoolIngressFrameV1, NonceScope,
    PersonaAddress, PersonaType, ProtocolKind, RouteDecision, RouteRequest, SerializationFormat,
    TxIR, TxType, UnifiedAccountRouter,
};
use novovm_adapter_evm_core::{
    estimate_access_list_intrinsic_extra_gas_m0, estimate_intrinsic_gas_m0,
    estimate_intrinsic_gas_with_access_list_m0, resolve_raw_evm_tx_route_hint_m0,
    translate_raw_evm_tx_fields_m0, tx_ir_from_raw_fields_m0, EvmRawTxEnvelopeType,
};
use novovm_adapter_evm_plugin::{
    drain_atomic_broadcast_ready_for_host, drain_atomic_receipts_for_host,
    drain_executable_ingress_frames_for_host, drain_payout_instructions_for_host,
    drain_pending_ingress_frames_for_host, drain_settlement_records_for_host,
    runtime_tap_ir_batch_v1, snapshot_executable_ingress_frames_for_host,
    snapshot_pending_ingress_frames_for_host, snapshot_pending_sender_buckets_for_host,
    EvmPendingSenderBucketV1, NOVOVM_ADAPTER_PLUGIN_APPLY_FLAG_ATOMIC_INTENT_GUARD_V1,
};
use novovm_adapter_novovm::{
    build_privacy_tx_ir_signed_from_raw_v1, PrivacyTxRawEnvelopeV1, PrivacyTxRawSignerV1,
};
use novovm_exec::{OpsWireOp, OpsWireV1Builder};
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
const GATEWAY_ETH_TX_INDEX_BACKEND_MEMORY: &str = "memory";
const GATEWAY_ETH_TX_INDEX_BACKEND_ROCKSDB: &str = "rocksdb";
const GATEWAY_ETH_TX_INDEX_ROCKSDB_CF_STATE: &str = "eth_tx_index_state_v1";
const GATEWAY_ETH_TX_INDEX_ROCKSDB_KEY_PREFIX: &[u8] = b"gateway:eth:tx:index:v1:";
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
const GATEWAY_UA_PRIMARY_KEY_DOMAIN: &[u8] = b"novovm_gateway_uca_primary_key_ref_v1";
const GATEWAY_INGRESS_RECORD_VERSION: u16 = 1;
const GATEWAY_INGRESS_PROTOCOL_ETH: u8 = 1;
const GATEWAY_INGRESS_PROTOCOL_WEB30: u8 = 2;
const GATEWAY_INGRESS_PROTOCOL_EVM_PAYOUT: u8 = 3;
const GATEWAY_EVM_RUNTIME_DRAIN_MAX: usize = 256;
const GATEWAY_EVM_ATOMIC_BROADCAST_EXEC_RETRY_DEFAULT: u64 = 1;
const GATEWAY_EVM_ATOMIC_BROADCAST_EXEC_TIMEOUT_MS_DEFAULT: u64 = 5_000;
const GATEWAY_EVM_ATOMIC_BROADCAST_EXEC_RETRY_BACKOFF_MS_DEFAULT: u64 = 25;
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
    data: &'a [u8],
    signature: &'a [u8],
    access_list_address_count: u64,
    access_list_storage_key_count: u64,
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

fn main() -> Result<()> {
    let mut runtime = GatewayRuntime::from_env()?;
    gateway_summary!(
        "gateway_in: bind={} spool_dir={} max_body={} max_requests={} evm_payout_autoreplay_max={} evm_payout_autoreplay_cooldown_ms={} evm_payout_pending_warn_threshold={} eth_default_chain_id={} ua_store_backend={} ua_store_path={} eth_tx_index_backend={} eth_tx_index_path={} internal_ingress=ops_wire_v1",
        runtime.bind,
        runtime.spool_dir.display(),
        runtime.max_body_bytes,
        runtime.max_requests,
        runtime.evm_payout_autoreplay_max,
        runtime.evm_payout_autoreplay_cooldown_ms,
        runtime.evm_payout_pending_warn_threshold,
        runtime.eth_default_chain_id,
        runtime.ua_store.backend_name(),
        runtime.ua_store.path().display(),
        runtime.eth_tx_index_store.backend_name(),
        runtime.eth_tx_index_store.path().display(),
    );

    let server = tiny_http::Server::http(&runtime.bind)
        .map_err(|e| anyhow::anyhow!("start gateway server failed on {}: {}", runtime.bind, e))?;
    let mut processed = 0u32;
    for request in server.incoming_requests() {
        handle_gateway_request(&mut runtime, request)?;
        processed = processed.saturating_add(1);
        if runtime.max_requests > 0 && processed >= runtime.max_requests {
            break;
        }
    }
    gateway_summary!(
        "gateway_out: bind={} processed={} max_requests={}",
        runtime.bind,
        processed,
        runtime.max_requests
    );
    Ok(())
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
        let eth_default_chain_id = u64_env("NOVOVM_GATEWAY_ETH_DEFAULT_CHAIN_ID", 1);
        let ua_store = resolve_gateway_ua_store_backend()?;
        let eth_tx_index_store = resolve_gateway_eth_tx_index_store_backend()?;
        let router = ua_store.load_router()?;
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
    match chain_id {
        56 => novovm_adapter_api::ChainType::BNB,
        137 => novovm_adapter_api::ChainType::Polygon,
        43114 => novovm_adapter_api::ChainType::Avalanche,
        _ => novovm_adapter_api::ChainType::EVM,
    }
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
                if let Ok(envelope) = bincode::deserialize::<GatewayUaStoreEnvelopeV1>(&raw) {
                    if envelope.version != GATEWAY_UA_STORE_ENVELOPE_VERSION {
                        bail!(
                            "unsupported gateway ua store version {} at {}",
                            envelope.version,
                            path.display()
                        );
                    }
                    return Ok(envelope.router);
                }
                let router: UnifiedAccountRouter =
                    bincode::deserialize(&raw).with_context(|| {
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
                if raw.is_none() {
                    raw = db
                        .get(GATEWAY_UA_STORE_ROCKSDB_KEY_ROUTER)
                        .with_context(|| {
                            format!(
                                "read gateway ua legacy router key from default cf failed: {}",
                                path.display()
                            )
                        })?;
                }
                let Some(raw) = raw else {
                    return Ok(UnifiedAccountRouter::new());
                };
                if raw.is_empty() {
                    return Ok(UnifiedAccountRouter::new());
                }
                if let Ok(envelope) = bincode::deserialize::<GatewayUaStoreEnvelopeV1>(&raw) {
                    if envelope.version != GATEWAY_UA_STORE_ENVELOPE_VERSION {
                        bail!(
                            "unsupported gateway ua store version {} at {}",
                            envelope.version,
                            path.display()
                        );
                    }
                    return Ok(envelope.router);
                }
                let router: UnifiedAccountRouter =
                    bincode::deserialize(&raw).with_context(|| {
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
        let encoded =
            bincode::serialize(&envelope).context("serialize gateway ua store envelope failed")?;
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
                if let Ok(record) = bincode::deserialize::<GatewayEthTxIndexRecordV1>(&raw) {
                    if record.version != GATEWAY_ETH_TX_INDEX_RECORD_VERSION {
                        bail!(
                            "unsupported eth tx index record version {} at {}",
                            record.version,
                            path.display()
                        );
                    }
                    return Ok(Some(record.entry));
                }
                let legacy_entry: GatewayEthTxIndexEntry = bincode::deserialize(&raw)
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
                let value = bincode::serialize(&GatewayEthTxIndexRecordV1 {
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
                        bincode::deserialize::<GatewayEthTxIndexRecordV1>(&tx_raw)
                    {
                        if record.version != GATEWAY_ETH_TX_INDEX_RECORD_VERSION {
                            continue;
                        }
                        record.entry
                    } else if let Ok(legacy) =
                        bincode::deserialize::<GatewayEthTxIndexEntry>(&tx_raw)
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
                        bincode::deserialize::<GatewayEthTxIndexRecordV1>(&tx_raw)
                    {
                        if record.version != GATEWAY_ETH_TX_INDEX_RECORD_VERSION {
                            continue;
                        }
                        record.entry
                    } else if let Ok(legacy) =
                        bincode::deserialize::<GatewayEthTxIndexEntry>(&tx_raw)
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
                        bincode::deserialize::<GatewayEthTxIndexRecordV1>(&tx_raw)
                    {
                        if record.version != GATEWAY_ETH_TX_INDEX_RECORD_VERSION {
                            continue;
                        }
                        record.entry
                    } else if let Ok(legacy) =
                        bincode::deserialize::<GatewayEthTxIndexEntry>(&tx_raw)
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
                if let Ok(record) = bincode::deserialize::<GatewayEvmSettlementIndexRecordV1>(&raw)
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
                let legacy_entry: GatewayEvmSettlementIndexEntry = bincode::deserialize(&raw)
                    .with_context(|| {
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
                    bincode::deserialize::<GatewayEvmSettlementTxRefRecordV1>(&raw)
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
                let value_by_id = bincode::serialize(&GatewayEvmSettlementIndexRecordV1 {
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
                let value_by_tx = bincode::serialize(&GatewayEvmSettlementTxRefRecordV1 {
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
                if let Ok(record) = bincode::deserialize::<GatewayEvmPayoutPendingRecordV1>(&raw) {
                    if record.version != GATEWAY_EVM_PAYOUT_PENDING_RECORD_VERSION {
                        bail!(
                            "unsupported evm pending payout record version {} at {}",
                            record.version,
                            path.display()
                        );
                    }
                    return Ok(Some(record.instruction));
                }
                let legacy_instruction: EvmFeePayoutInstructionV1 = bincode::deserialize(&raw)
                    .with_context(|| {
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
                let value = bincode::serialize(&GatewayEvmPayoutPendingRecordV1 {
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
                        bincode::deserialize::<GatewayEvmPayoutPendingRecordV1>(&raw)
                    {
                        if record.version != GATEWAY_EVM_PAYOUT_PENDING_RECORD_VERSION {
                            continue;
                        }
                        record.instruction
                    } else if let Ok(legacy) =
                        bincode::deserialize::<EvmFeePayoutInstructionV1>(&raw)
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
                if let Ok(record) = bincode::deserialize::<GatewayEvmAtomicReadyIndexRecordV1>(&raw)
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
                let legacy_entry: GatewayEvmAtomicReadyIndexEntry = bincode::deserialize(&raw)
                    .with_context(|| {
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
                let value = bincode::serialize(&GatewayEvmAtomicReadyIndexRecordV1 {
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
                    bincode::deserialize::<GatewayEvmAtomicReadyPendingRecordV1>(&raw)
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
                let legacy_item: AtomicBroadcastReadyV1 =
                    bincode::deserialize(&raw).with_context(|| {
                        format!(
                            "decode legacy evm pending atomic-ready record failed: {}",
                            path.display()
                        )
                    })?;
                Ok(Some(legacy_item))
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
                let value = bincode::serialize(&GatewayEvmAtomicReadyPendingRecordV1 {
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
                if let Ok(record) =
                    bincode::deserialize::<GatewayEvmAtomicBroadcastPendingRecordV1>(&raw)
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
                let legacy_ticket: GatewayEvmAtomicBroadcastTicketV1 = bincode::deserialize(&raw)
                    .with_context(|| {
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
                let value = bincode::serialize(&GatewayEvmAtomicBroadcastPendingRecordV1 {
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
                    let ticket = if let Ok(record) =
                        bincode::deserialize::<GatewayEvmAtomicBroadcastPendingRecordV1>(&raw)
                    {
                        if record.version != GATEWAY_EVM_ATOMIC_BROADCAST_PENDING_RECORD_VERSION {
                            continue;
                        }
                        record.ticket
                    } else if let Ok(legacy) =
                        bincode::deserialize::<GatewayEvmAtomicBroadcastTicketV1>(&raw)
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
        "bincode_file" | "file" | "bincode" => Ok(GatewayUaStoreBackend::BincodeFile { path }),
        "rocksdb" => Ok(GatewayUaStoreBackend::RocksDb { path }),
        _ => bail!(
            "invalid NOVOVM_GATEWAY_UA_STORE_BACKEND={}; valid: rocksdb|bincode_file|file|bincode",
            backend
        ),
    }
}

fn resolve_gateway_eth_tx_index_store_backend() -> Result<GatewayEthTxIndexStoreBackend> {
    let backend = string_env(
        "NOVOVM_GATEWAY_ETH_TX_INDEX_BACKEND",
        GATEWAY_ETH_TX_INDEX_BACKEND_MEMORY,
    )
    .trim()
    .to_ascii_lowercase();
    match backend.as_str() {
        "memory" => Ok(GatewayEthTxIndexStoreBackend::Memory),
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

    let mut ctx = GatewayMethodContext {
        eth_tx_index_store: &runtime.eth_tx_index_store,
        eth_default_chain_id: runtime.eth_default_chain_id,
        spool_dir: &runtime.spool_dir,
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
            let body = rpc_error_body_with_data(id, code, &message, data);
            respond_json_http(request, 200, &body)?;
        }
    }
    auto_replay_pending_payouts(runtime);
    Ok(())
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
    let eth_default_gas_price = u64_env("NOVOVM_GATEWAY_ETH_DEFAULT_GAS_PRICE", 1);
    let eth_default_priority_fee = u64_env(
        "NOVOVM_GATEWAY_ETH_DEFAULT_MAX_PRIORITY_FEE_PER_GAS",
        eth_default_gas_price,
    );
    match method {
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
            let chain_id = param_as_u64_any_with_tx(params, &["chain_id", "chainId"])
                .unwrap_or(ctx.eth_default_chain_id);
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
        "eth_maxPriorityFeePerGas" => Ok((
            serde_json::Value::String(format!("0x{:x}", eth_default_priority_fee)),
            false,
        )),
        "eth_feeHistory" => {
            let chain_id = param_as_u64_any_with_tx(params, &["chain_id", "chainId"])
                .unwrap_or(ctx.eth_default_chain_id);
            let base_fee_per_gas_wei = gateway_eth_base_fee_per_gas_wei();
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
            let pending_block =
                gateway_eth_pending_block_from_runtime(chain_id, latest, false).map(|(number, _hash, txs)| {
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
                            eth_default_priority_fee as u128,
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
            let sync_status =
                resolve_gateway_eth_sync_status(chain_id, eth_tx_index, ctx.eth_tx_index_store)?;
            let pending_latest_entries = collect_gateway_eth_chain_entries(
                eth_tx_index,
                ctx.eth_tx_index_store,
                chain_id,
                gateway_eth_query_scan_max(),
            )?;
            let pending_latest = resolve_gateway_eth_latest_block_number(
                chain_id,
                &pending_latest_entries,
                ctx.eth_tx_index_store,
            )?;
            let pending_block_number =
                gateway_eth_pending_block_from_runtime(chain_id, pending_latest, false)
                    .map(|(block_number, _block_hash, _block_txs)| block_number);
            Ok((
                gateway_eth_syncing_json(sync_status, pending_block_number),
                false,
            ))
        }
        "eth_pendingTransactions" => {
            let chain_id = param_as_u64_any_with_tx(params, &["chain_id", "chainId"])
                .unwrap_or(ctx.eth_default_chain_id);
            let (pending_txs, queued_txs) = collect_gateway_eth_txpool_runtime_txs(chain_id);
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
            let entries = collect_gateway_eth_chain_entries(
                eth_tx_index,
                ctx.eth_tx_index_store,
                chain_id,
                gateway_eth_query_scan_max(),
            )?;
            let latest =
                resolve_gateway_eth_latest_block_number(chain_id, &entries, ctx.eth_tx_index_store)?;
            Ok((serde_json::Value::String(format!("0x{:x}", latest)), false))
        }
        "eth_getBalance" => {
            let chain_id = param_as_u64_any_with_tx(params, &["chain_id", "chainId"])
                .unwrap_or(ctx.eth_default_chain_id);
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
            if normalized_tag.eq_ignore_ascii_case("pending") {
                let Some((pending_block_number, _pending_block_hash, pending_entries)) =
                    gateway_eth_pending_block_from_runtime(chain_id, latest, false)
                else {
                    return Ok((serde_json::Value::Null, false));
                };
                return Ok((
                    gateway_eth_block_by_number_json(
                        chain_id,
                        pending_block_number,
                        &pending_entries,
                        full_transactions,
                        true,
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
                if block_number <= latest {
                    return Ok((
                        gateway_eth_block_by_number_json(
                            chain_id,
                            block_number,
                            &[],
                            full_transactions,
                            false,
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
                ),
                false,
            ))
        }
        "eth_getBlockByHash" => {
            let chain_id = param_as_u64_any_with_tx(params, &["chain_id", "chainId"])
                .unwrap_or(ctx.eth_default_chain_id);
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
                        ),
                        false,
                    ));
                }
            }
            if let Some((pending_block_number, pending_hash, pending_entries)) =
                gateway_eth_pending_block_from_runtime(chain_id, latest, false)
            {
                if pending_hash == block_hash {
                    return Ok((
                        gateway_eth_block_by_number_json(
                            chain_id,
                            pending_block_number,
                            &pending_entries,
                            full_transactions,
                            true,
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
                    ),
                    false,
                ));
            }
            Ok((serde_json::Value::Null, false))
        }
        "eth_getTransactionByBlockNumberAndIndex" => {
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
            let latest =
                resolve_gateway_eth_latest_block_number(chain_id, &entries, ctx.eth_tx_index_store)?;
            let block_tag =
                parse_eth_block_query_tag(params).unwrap_or_else(|| "latest".to_string());
            let normalized_tag = block_tag.trim().trim_matches('"');
            if normalized_tag.eq_ignore_ascii_case("pending") {
                let Some((pending_block_number, pending_hash, pending_entries)) =
                    gateway_eth_pending_block_from_runtime(chain_id, latest, false)
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
                gateway_eth_pending_block_from_runtime(chain_id, latest, false)
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
                gateway_eth_pending_block_from_runtime(chain_id, latest, false)
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
            let pending_block = gateway_eth_pending_block_from_runtime(chain_id, latest, false);
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
                if gateway_eth_pending_block_from_runtime(chain_id, latest, false)
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
                    gateway_eth_pending_block_from_runtime(chain_id, latest, false)
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
            let Some(mut filter) = ctx.eth_filters.filters.get(&filter_id).cloned() else {
                bail!("filter not found: 0x{:x}", filter_id);
            };
            let response = match &mut filter {
                GatewayEthFilterKind::Logs(log_filter) => {
                    let entries = collect_gateway_eth_chain_entries(
                        eth_tx_index,
                        ctx.eth_tx_index_store,
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
                                ctx.eth_tx_index_store,
                            )?)
                        }
                    } else {
                        let latest = resolve_gateway_eth_latest_block_number(
                            log_filter.chain_id,
                            &entries,
                            ctx.eth_tx_index_store,
                        )?;
                        let has_runtime_pending = log_filter.query.include_pending_block
                            && gateway_eth_pending_block_from_runtime(
                                log_filter.chain_id,
                                latest,
                                false,
                            )
                            .is_some();
                        let max_visible_block = if has_runtime_pending {
                            latest.saturating_add(1)
                        } else {
                            latest
                        };
                        let from = log_filter.query.from_block.unwrap_or(0).max(log_filter.next_block);
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
                                ctx.eth_tx_index_store,
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
                        ctx.eth_tx_index_store,
                        *chain_id,
                        gateway_eth_query_scan_max(),
                    )?;
                    let latest =
                        resolve_gateway_eth_latest_block_number(*chain_id, &entries, ctx.eth_tx_index_store)?;
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
                            out.push(serde_json::Value::String(format!("0x{}", to_hex(&block_hash))));
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
                                ctx.eth_tx_index_store,
                                *chain_id,
                                block_number,
                                gateway_eth_query_scan_max(),
                            )?;
                            if precise_block_txs.is_empty() {
                                continue;
                            }
                            let block_hash = gateway_eth_block_hash_for_txs(
                                *chain_id,
                                block_number,
                                &precise_block_txs,
                            );
                            out.push(serde_json::Value::String(format!("0x{}", to_hex(&block_hash))));
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
            ctx.eth_filters.filters.insert(filter_id, filter);
            Ok((response, false))
        }
        "txpool_content" => {
            let chain_id = param_as_u64_any_with_tx(params, &["chain_id", "chainId"])
                .unwrap_or(ctx.eth_default_chain_id);
            let (pending_txs, queued_txs) = collect_gateway_eth_txpool_runtime_txs(chain_id);
            if !pending_txs.is_empty() || !queued_txs.is_empty() {
                return Ok((build_gateway_eth_txpool_content_from_ir(pending_txs, queued_txs), false));
            }
            Ok((build_gateway_eth_txpool_content(Vec::new()), false))
        }
        "txpool_contentFrom" => {
            let chain_id = param_as_u64_any_with_tx(params, &["chain_id", "chainId"])
                .unwrap_or(ctx.eth_default_chain_id);
            let address_raw = extract_eth_persona_address_param(params)
                .ok_or_else(|| anyhow::anyhow!("address (or from/external_address) is required"))?;
            let address = decode_hex_bytes(&address_raw, "address")?;
            if address.len() != 20 {
                bail!("address must be 20 bytes");
            }
            let (pending_txs, queued_txs) = collect_gateway_eth_txpool_runtime_txs(chain_id);
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
            let (pending_txs, queued_txs) = collect_gateway_eth_txpool_runtime_txs(chain_id);
            if !pending_txs.is_empty() || !queued_txs.is_empty() {
                return Ok((build_gateway_eth_txpool_inspect_from_ir(pending_txs, queued_txs), false));
            }
            Ok((build_gateway_eth_txpool_inspect(Vec::new()), false))
        }
        "txpool_inspectFrom" => {
            let chain_id = param_as_u64_any_with_tx(params, &["chain_id", "chainId"])
                .unwrap_or(ctx.eth_default_chain_id);
            let address_raw = extract_eth_persona_address_param(params)
                .ok_or_else(|| anyhow::anyhow!("address (or from/external_address) is required"))?;
            let address = decode_hex_bytes(&address_raw, "address")?;
            if address.len() != 20 {
                bail!("address must be 20 bytes");
            }
            let (pending_txs, queued_txs) = collect_gateway_eth_txpool_runtime_txs(chain_id);
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
            let (pending_txs, queued_txs) = collect_gateway_eth_txpool_runtime_txs(chain_id);
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
            let address_raw = extract_eth_persona_address_param(params)
                .ok_or_else(|| anyhow::anyhow!("address (or from/external_address) is required"))?;
            let address = decode_hex_bytes(&address_raw, "address")?;
            if address.len() != 20 {
                bail!("address must be 20 bytes");
            }
            let (pending_txs, queued_txs) = collect_gateway_eth_txpool_runtime_txs(chain_id);
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
            let suggested = gateway_eth_suggest_gas_price_wei(
                chain_id,
                eth_tx_index,
                ctx.eth_tx_index_store,
                eth_default_gas_price,
            )?;
            Ok((
                serde_json::Value::String(format!("0x{:x}", suggested)),
                false,
            ))
        }
        "eth_call" => {
            let chain_id = param_as_u64_any_with_tx(params, &["chain_id", "chainId"])
                .unwrap_or(ctx.eth_default_chain_id);
            if let Some(raw_from) = extract_eth_persona_address_param(params) {
                let _from = decode_hex_bytes(&raw_from, "from")?;
            }
            let raw_to = param_as_string_any_with_tx(params, &["to"])
                .ok_or_else(|| anyhow::anyhow!("to is required for eth_call"))?;
            let to = decode_hex_bytes(&raw_to, "to")?;
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

            // Standard ERC20 balanceOf(address) selector.
            const ERC20_BALANCE_OF_SELECTOR: [u8; 4] = [0x70, 0xa0, 0x82, 0x31];
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

            // Minimal read-only convention: when calldata is exactly one 32-byte slot, reuse
            // the same slot resolver as eth_getStorageAt for deterministic local reads.
            if call_data.len() == 32 {
                let mut slot_bytes = [0u8; 16];
                slot_bytes.copy_from_slice(&call_data[16..32]);
                let slot = u128::from_be_bytes(slot_bytes);
                let storage = gateway_eth_resolve_storage_word_from_entries(&view_entries, &to, slot)
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

            Ok((serde_json::Value::String("0x".to_string()), false))
        }
        "eth_estimateGas" => {
            let chain_id = param_as_u64_any_with_tx(params, &["chain_id", "chainId"])
                .unwrap_or(ctx.eth_default_chain_id);
            let (access_list_address_count, access_list_storage_key_count) =
                parse_eth_access_list_intrinsic_counts(params)?;
            let from = match extract_eth_persona_address_param(params) {
                Some(raw_from) => decode_hex_bytes(&raw_from, "from")?,
                None => vec![0u8; 20],
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
                from,
                to,
                value,
                gas_limit: u64::MAX,
                gas_price: eth_default_gas_price,
                nonce: 0,
                data,
                signature: Vec::new(),
                chain_id,
                tx_type,
                source_chain: None,
                target_chain: None,
            };
            let intrinsic = estimate_intrinsic_gas_m0(&tx_ir);
            let access_list_extra = estimate_access_list_intrinsic_extra_gas_m0(
                access_list_address_count,
                access_list_storage_key_count,
            );
            let estimated = intrinsic.saturating_add(access_list_extra);
            Ok((serde_json::Value::String(format!("0x{:x}", estimated)), false))
        }
        "eth_getCode" => {
            let chain_id = param_as_u64_any_with_tx(params, &["chain_id", "chainId"])
                .unwrap_or(ctx.eth_default_chain_id);
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
            let address_raw = extract_eth_persona_address_param(params)
                .ok_or_else(|| anyhow::anyhow!("address is required for eth_getStorageAt"))?;
            let address = decode_hex_bytes(&address_raw, "address")?;
            let slot_raw = extract_eth_storage_slot_param(params)
                .ok_or_else(|| anyhow::anyhow!("slot/position is required for eth_getStorageAt"))?;
            let Some(slot) = parse_u128_hex_or_dec(&slot_raw) else {
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
            let storage =
                gateway_eth_resolve_storage_word_from_entries(&view_entries, &address, slot)
                .unwrap_or([0u8; 32]);
            Ok((
                serde_json::Value::String(format!("0x{}", to_hex(&storage))),
                false,
            ))
        }
        "eth_getProof" => {
            let chain_id = param_as_u64_any_with_tx(params, &["chain_id", "chainId"])
                .unwrap_or(ctx.eth_default_chain_id);
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
            let balance = gateway_eth_balance_from_entries(&entries, &address);
            let nonce = entries
                .iter()
                .filter(|entry| entry.from == address)
                .map(|entry| entry.nonce.saturating_add(1))
                .max()
                .unwrap_or(0);
            let code = gateway_eth_resolve_code_from_entries(&entries, &address)
                .unwrap_or_default();
            let code_hash = gateway_eth_keccak_hex(&code);

            let mut storage_items = Vec::with_capacity(storage_keys.len());
            for raw_key in storage_keys {
                let Some(slot) = parse_u128_hex_or_dec(&raw_key) else {
                    bail!("invalid storage key for eth_getProof: {}", raw_key);
                };
                let value = gateway_eth_resolve_storage_word_from_entries(&entries, &address, slot)
                    .unwrap_or([0u8; 32]);
                storage_items.push((slot, value));
            }
            let storage_hash = gateway_eth_storage_hash_hex(&address, &storage_items);
            let storage_proof = storage_items
                .iter()
                .map(|(slot, value)| {
                    serde_json::json!({
                        "key": format!("0x{:064x}", slot),
                        "value": format!("0x{}", to_hex(value)),
                        "proof": [],
                    })
                })
                .collect::<Vec<serde_json::Value>>();

            Ok((
                serde_json::json!({
                    "address": format!("0x{}", to_hex(&address)),
                    "accountProof": [],
                    "balance": format!("0x{:x}", balance),
                    "codeHash": code_hash,
                    "nonce": format!("0x{:x}", nonce),
                    "storageHash": storage_hash,
                    "storageProof": storage_proof,
                }),
                false,
            ))
        }
        "ua_createUca" => {
            let uca_id = param_as_string(params, "uca_id")
                .ok_or_else(|| anyhow::anyhow!("uca_id is required for ua_createUca"))?;
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
            let tx_hash_raw = extract_eth_tx_hash_query_param(params)
                .ok_or_else(|| anyhow::anyhow!("tx_hash (or hash) is required"))?;
            let tx_hash_bytes = decode_hex_bytes(&tx_hash_raw, "tx_hash")?;
            let tx_hash = vec_to_32(&tx_hash_bytes, "tx_hash")?;
            let chain_hint = param_as_u64_any_with_tx(params, &["chain_id", "chainId"])
                .or(Some(ctx.eth_default_chain_id));
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
                    gateway_eth_pending_block_from_runtime(
                        pending_entry.chain_id,
                        latest_block_number,
                        false,
                    )
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
            let tx_hash_raw = extract_eth_tx_hash_query_param(params)
                .ok_or_else(|| anyhow::anyhow!("tx_hash (or hash) is required"))?;
            let tx_hash_bytes = decode_hex_bytes(&tx_hash_raw, "tx_hash")?;
            let tx_hash = vec_to_32(&tx_hash_bytes, "tx_hash")?;
            let chain_hint = param_as_u64_any_with_tx(params, &["chain_id", "chainId"])
                .or(Some(ctx.eth_default_chain_id));
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
                    gateway_eth_pending_block_from_runtime(
                        pending_entry.chain_id,
                        latest_block_number,
                        false,
                    )
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
            let mut out = Vec::with_capacity(queries.len());
            for query in queries {
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
                let (item, _) = run_gateway_method(
                    router,
                    eth_tx_index,
                    evm_settlement_index_by_id,
                    evm_settlement_index_by_tx,
                    evm_pending_payout_by_settlement,
                    ctx,
                    "eth_getLogs",
                    &forwarded,
                )?;
                out.push(item);
            }
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
                let (item, _) = run_gateway_method(
                    router,
                    eth_tx_index,
                    evm_settlement_index_by_id,
                    evm_settlement_index_by_tx,
                    evm_pending_payout_by_settlement,
                    ctx,
                    "eth_getFilterChanges",
                    &forwarded,
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
            let mut out = Vec::with_capacity(filters.len());
            for filter in filters {
                let forwarded = match filter {
                    serde_json::Value::Object(map) => serde_json::Value::Object(map),
                    other => serde_json::json!({ "filter_id": other }),
                };
                let (item, _) = run_gateway_method(
                    router,
                    eth_tx_index,
                    evm_settlement_index_by_id,
                    evm_settlement_index_by_tx,
                    evm_pending_payout_by_settlement,
                    ctx,
                    "eth_getFilterLogs",
                    &forwarded,
                )?;
                out.push(item);
            }
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
                match run_gateway_method(
                    router,
                    eth_tx_index,
                    evm_settlement_index_by_id,
                    evm_settlement_index_by_tx,
                    evm_pending_payout_by_settlement,
                    ctx,
                    "evm_publicSendRawTransaction",
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
                match run_gateway_method(
                    router,
                    eth_tx_index,
                    evm_settlement_index_by_id,
                    evm_settlement_index_by_tx,
                    evm_pending_payout_by_settlement,
                    ctx,
                    "evm_publicSendTransaction",
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
            let broadcast = gateway_eth_broadcast_status_json_by_tx(&tx_hash);
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
            let chain_id = param_as_u64_any_with_tx(params, &["chain_id", "chainId"]);
            let mut out = Vec::with_capacity(tx_hashes.len());
            for tx_hash in tx_hashes {
                let forwarded = match chain_id {
                    Some(chain_id) => serde_json::json!({
                        "tx_hash": tx_hash,
                        "chain_id": chain_id,
                    }),
                    None => serde_json::json!({
                        "tx_hash": tx_hash,
                    }),
                };
                let (status, _) = run_gateway_method(
                    router,
                    eth_tx_index,
                    evm_settlement_index_by_id,
                    evm_settlement_index_by_tx,
                    evm_pending_payout_by_settlement,
                    ctx,
                    "evm_getPublicBroadcastStatus",
                    &forwarded,
                )?;
                out.push(status);
            }
            Ok((serde_json::Value::Array(out), false))
        }
        "evm_getTransactionLifecycleBatch"
        | "evm_get_transaction_lifecycle_batch"
        | "evm_getTxSubmitStatusBatch"
        | "evm_get_tx_submit_status_batch" => {
            let tx_hashes = extract_eth_tx_hashes_query_params(params);
            if tx_hashes.is_empty() {
                bail!("tx_hashes (or txHashes/hashes/txs) is required");
            }
            let chain_id = param_as_u64_any_with_tx(params, &["chain_id", "chainId"]);
            let mut out = Vec::with_capacity(tx_hashes.len());
            for tx_hash in tx_hashes {
                let forwarded = match chain_id {
                    Some(chain_id) => serde_json::json!({
                        "tx_hash": tx_hash,
                        "chain_id": chain_id,
                    }),
                    None => serde_json::json!({
                        "tx_hash": tx_hash,
                    }),
                };
                let (status, _) = run_gateway_method(
                    router,
                    eth_tx_index,
                    evm_settlement_index_by_id,
                    evm_settlement_index_by_tx,
                    evm_pending_payout_by_settlement,
                    ctx,
                    "evm_getTransactionLifecycle",
                    &forwarded,
                )?;
                out.push(status);
            }
            Ok((serde_json::Value::Array(out), false))
        }
        "evm_replayPublicBroadcast" | "evm_replay_public_broadcast" => {
            let tx_hash_raw = extract_eth_tx_hash_query_param(params)
                .or_else(|| param_as_string(params, "txHash"))
                .ok_or_else(|| anyhow::anyhow!("tx_hash (or txHash/hash) is required"))?;
            let tx_hash_bytes = decode_hex_bytes(&tx_hash_raw, "tx_hash")?;
            let tx_hash = vec_to_32(&tx_hash_bytes, "tx_hash")?;
            let chain_hint = param_as_u64_any_with_tx(params, &["chain_id", "chainId"]);

            let mut entry = eth_tx_index.get(&tx_hash).cloned();
            if entry.is_none() {
                entry = ctx.eth_tx_index_store.load_eth_tx(&tx_hash)?;
                if let Some(cached) = entry.as_ref() {
                    eth_tx_index.insert(tx_hash, cached.clone());
                }
            }
            if entry.is_none() {
                if let Some(tx) = find_gateway_eth_runtime_tx_by_hash(tx_hash, chain_hint) {
                    entry = Some(gateway_eth_tx_index_entry_from_ir(tx));
                }
            }
            let Some(entry) = entry else {
                return Ok((serde_json::Value::Null, false));
            };
            if chain_hint.is_some_and(|chain_id| entry.chain_id != chain_id) {
                return Ok((serde_json::Value::Null, false));
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
            upsert_gateway_eth_broadcast_status(entry.tx_hash, &broadcast_result);
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
                false,
            ))
        }
        "evm_replayPublicBroadcastBatch" | "evm_replay_public_broadcast_batch" => {
            let tx_hashes = extract_eth_tx_hashes_query_params(params);
            if tx_hashes.is_empty() {
                bail!("tx_hashes (or txHashes/hashes/txs) is required");
            }
            let chain_id = param_as_u64_any_with_tx(params, &["chain_id", "chainId"]);
            let mut replayed = 0usize;
            let mut failed = 0usize;
            let mut results = Vec::with_capacity(tx_hashes.len());
            for tx_hash in tx_hashes {
                let forwarded = match chain_id {
                    Some(chain_id) => serde_json::json!({
                        "tx_hash": tx_hash,
                        "chain_id": chain_id,
                    }),
                    None => serde_json::json!({
                        "tx_hash": tx_hash,
                    }),
                };
                match run_gateway_method(
                    router,
                    eth_tx_index,
                    evm_settlement_index_by_id,
                    evm_settlement_index_by_tx,
                    evm_pending_payout_by_settlement,
                    ctx,
                    "evm_replayPublicBroadcast",
                    &forwarded,
                ) {
                    Ok((item, _)) => {
                        replayed = replayed.saturating_add(1);
                        results.push(item);
                    }
                    Err(e) => {
                        failed = failed.saturating_add(1);
                        results.push(serde_json::json!({
                            "replayed": false,
                            "tx_hash": forwarded
                                .get("tx_hash")
                                .cloned()
                                .unwrap_or(serde_json::Value::Null),
                            "error": e.to_string(),
                        }));
                    }
                }
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
            let chain_id = param_as_u64_any_with_tx(params, &["chain_id", "chainId"]);
            let mut out = Vec::with_capacity(tx_hashes.len());
            for tx_hash in tx_hashes {
                let forwarded = match chain_id {
                    Some(chain_id) => serde_json::json!({
                        "tx_hash": tx_hash,
                        "chain_id": chain_id,
                    }),
                    None => serde_json::json!({
                        "tx_hash": tx_hash,
                    }),
                };
                let (item, _) = run_gateway_method(
                    router,
                    eth_tx_index,
                    evm_settlement_index_by_id,
                    evm_settlement_index_by_tx,
                    evm_pending_payout_by_settlement,
                    ctx,
                    "eth_getTransactionReceipt",
                    &forwarded,
                )?;
                out.push(item);
            }
            Ok((serde_json::Value::Array(out), false))
        }
        "evm_getTransactionByHashBatch" | "evm_get_transaction_by_hash_batch" => {
            let tx_hashes = extract_eth_tx_hashes_query_params(params);
            if tx_hashes.is_empty() {
                bail!("tx_hashes (or txHashes/hashes/txs) is required");
            }
            let chain_id = param_as_u64_any_with_tx(params, &["chain_id", "chainId"]);
            let mut out = Vec::with_capacity(tx_hashes.len());
            for tx_hash in tx_hashes {
                let forwarded = match chain_id {
                    Some(chain_id) => serde_json::json!({
                        "tx_hash": tx_hash,
                        "chain_id": chain_id,
                    }),
                    None => serde_json::json!({
                        "tx_hash": tx_hash,
                    }),
                };
                let (item, _) = run_gateway_method(
                    router,
                    eth_tx_index,
                    evm_settlement_index_by_id,
                    evm_settlement_index_by_tx,
                    evm_pending_payout_by_settlement,
                    ctx,
                    "eth_getTransactionByHash",
                    &forwarded,
                )?;
                out.push(item);
            }
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
            let broadcast = gateway_eth_broadcast_status_json_by_tx(&tx_hash);

            if let Some(tx) = find_gateway_eth_runtime_tx_by_hash(tx_hash, chain_hint) {
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
                let receipt = if let Some((pending_block_number, pending_block_hash, pending_entries)) =
                    gateway_eth_pending_block_from_runtime(
                        pending_entry.chain_id,
                        latest_block_number,
                        false,
                    )
                {
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
                return Ok((
                    serde_json::json!({
                        "accepted": true,
                        "pending": true,
                        "onchain": false,
                        "tx_hash": format!("0x{}", to_hex(&tx_hash)),
                        "chain_id": format!("0x{:x}", pending_entry.chain_id),
                        "receipt": receipt,
                        "broadcast": broadcast,
                        "error_code": serde_json::Value::Null,
                        "error_reason": serde_json::Value::Null,
                    }),
                    false,
                ));
            }

            let mut entry = eth_tx_index.get(&tx_hash).cloned();
            if entry.is_none() {
                entry = ctx.eth_tx_index_store.load_eth_tx(&tx_hash)?;
                if let Some(cached) = entry.as_ref() {
                    eth_tx_index.insert(tx_hash, cached.clone());
                }
            }
            let Some(entry) = entry else {
                return Ok((
                    serde_json::json!({
                        "accepted": false,
                        "pending": false,
                        "onchain": false,
                        "tx_hash": format!("0x{}", to_hex(&tx_hash)),
                        "chain_id": serde_json::Value::Null,
                        "receipt": serde_json::Value::Null,
                        "broadcast": broadcast,
                        "error_code": "TX_NOT_FOUND",
                        "error_reason": "transaction not found",
                    }),
                    false,
                ));
            };
            if chain_hint.is_some_and(|chain_id| entry.chain_id != chain_id) {
                return Ok((
                    serde_json::json!({
                        "accepted": false,
                        "pending": false,
                        "onchain": false,
                        "tx_hash": format!("0x{}", to_hex(&tx_hash)),
                        "chain_id": format!("0x{:x}", entry.chain_id),
                        "receipt": serde_json::Value::Null,
                        "broadcast": broadcast,
                        "error_code": "CHAIN_MISMATCH",
                        "error_reason": "transaction exists but chain_id mismatch",
                    }),
                    false,
                ));
            }
            let receipt = gateway_eth_tx_receipt_query_json(&entry, eth_tx_index, ctx.eth_tx_index_store)?;
            let pending = receipt
                .get("pending")
                .and_then(serde_json::Value::as_bool)
                .unwrap_or(false);
            Ok((
                serde_json::json!({
                    "accepted": true,
                    "pending": pending,
                    "onchain": !pending,
                    "tx_hash": format!("0x{}", to_hex(&tx_hash)),
                    "chain_id": format!("0x{:x}", entry.chain_id),
                    "receipt": receipt,
                    "broadcast": broadcast,
                    "error_code": serde_json::Value::Null,
                    "error_reason": serde_json::Value::Null,
                }),
                false,
            ))
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
            router.update_policy(
                &uca_id,
                role,
                AccountPolicy {
                    nonce_scope,
                    allow_type4_with_delegate_or_session,
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

            let explicit_chain_id = param_as_u64_any_with_tx(params, &["chain_id", "chainId"]);
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

            let from_raw = extract_eth_persona_address_param(params).ok_or_else(|| {
                anyhow::anyhow!("from (or external_address) is required for eth_sendRawTransaction")
            })?;
            let from = decode_hex_bytes(&from_raw, "from")?;
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
                wants_cross_chain_atomic,
                tx_type4: fields.hint.tx_type4,
                session_expires_at,
                now,
            })?;
            let tx_ir = tx_ir_from_raw_fields_m0(&fields, &raw_tx, from.clone(), chain_id);
            let tap_drain =
                apply_gateway_evm_runtime_tap(&tx_ir, wants_cross_chain_atomic)?;
            let tx_hash = vec_to_32(&tx_ir.hash, "tx_hash")?;
            let record = GatewayIngressEthRecordV1 {
                version: GATEWAY_INGRESS_RECORD_VERSION,
                protocol: GATEWAY_INGRESS_PROTOCOL_ETH,
                uca_id: uca_id.clone(),
                chain_id,
                nonce,
                tx_type: fields.hint.tx_type_number,
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
            upsert_gateway_eth_broadcast_status(record.tx_hash, &broadcast_result);
            let wire = encode_gateway_ingress_ops_wire_v1_eth(&record)?;
            write_spool_ops_wire_v1(ctx.spool_dir, &wire)?;
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
                }),
                true,
            ))
        }
        "eth_sendTransaction" => {
            let explicit_uca_id = param_as_string(params, "uca_id");
            let role = parse_account_role(params)?;
            let chain_id = param_as_u64_any_with_tx(params, &["chain_id", "chainId"])
                .unwrap_or(ctx.eth_default_chain_id);
            let from_raw = extract_eth_persona_address_param(params).ok_or_else(|| {
                anyhow::anyhow!("from (or external_address) is required for eth_sendTransaction")
            })?;
            let from = decode_hex_bytes(&from_raw, "from")?;
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
            let nonce = if let Some(explicit_nonce) = param_as_u64_any_with_tx(params, &["nonce"]) {
                explicit_nonce
            } else {
                let entries = collect_gateway_eth_chain_entries(
                    eth_tx_index,
                    ctx.eth_tx_index_store,
                    chain_id,
                    gateway_eth_query_scan_max(),
                )?;
                let latest_nonce = entries
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
            let has_access_list_intrinsic =
                access_list_address_count > 0 || access_list_storage_key_count > 0;
            let gas_limit = param_as_u64_any_with_tx(params, &["gas_limit", "gasLimit", "gas"])
                .unwrap_or(21_000);
            let gas_price = param_as_u64_any_with_tx(
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
            let tx_type_u64 = explicit_tx_type.unwrap_or(if has_eip1559_fee_fields {
                2
            } else if has_access_list_intrinsic {
                1
            } else {
                0
            });
            if tx_type_u64 > u8::MAX as u64 {
                bail!("tx_type out of range: {}", tx_type_u64);
            }
            let tx_type = tx_type_u64 as u8;
            if tx_type == 3 {
                bail!("blob (type 3) write path disabled");
            }
            if has_access_list_intrinsic && tx_type != 1 && tx_type != 2 {
                bail!(
                    "accessList requires tx_type 1 (EIP-2930) or tx_type 2 (EIP-1559), got {}",
                    tx_type
                );
            }
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
                wants_cross_chain_atomic,
                tx_type4,
                session_expires_at,
                now,
            })?;
            let tx_hash = compute_gateway_eth_tx_hash(&GatewayEthTxHashInput {
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
                data: &data,
                signature: &signature,
                access_list_address_count,
                access_list_storage_key_count,
                signature_domain: &signature_domain,
                wants_cross_chain_atomic,
            });
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
            let required_intrinsic = estimate_intrinsic_gas_with_access_list_m0(
                &tx_ir,
                access_list_address_count,
                access_list_storage_key_count,
            );
            if tx_ir.gas_limit < required_intrinsic {
                bail!(
                    "eth_sendTransaction gas too low for intrinsic cost: gas_limit={} required_intrinsic={} access_list_addresses={} access_list_storage_keys={}",
                    tx_ir.gas_limit,
                    required_intrinsic,
                    access_list_address_count,
                    access_list_storage_key_count
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
            upsert_gateway_eth_broadcast_status(record.tx_hash, &broadcast_result);
            let wire = encode_gateway_ingress_ops_wire_v1_eth(&record)?;
            write_spool_ops_wire_v1(ctx.spool_dir, &wire)?;
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
                }),
                true,
            ))
        }
        _ => bail!(
            "unknown method: {}; valid: ua_createUca|ua_rotatePrimaryKey|ua_bindPersona|ua_revokePersona|ua_getBindingOwner|ua_setPolicy|eth_chainId|net_version|web3_clientVersion|web3_sha3|eth_protocolVersion|net_listening|net_peerCount|eth_accounts|eth_coinbase|eth_mining|eth_hashrate|eth_maxPriorityFeePerGas|eth_feeHistory|eth_syncing|eth_pendingTransactions|eth_blockNumber|eth_getBalance|eth_getBlockByNumber|eth_getBlockByHash|eth_getTransactionByBlockNumberAndIndex|eth_getTransactionByBlockHashAndIndex|eth_getBlockTransactionCountByNumber|eth_getBlockTransactionCountByHash|eth_getBlockReceipts|eth_getUncleCountByBlockNumber|eth_getUncleCountByBlockHash|eth_getUncleByBlockNumberAndIndex|eth_getUncleByBlockHashAndIndex|eth_getLogs|eth_subscribe|eth_unsubscribe|eth_newFilter|eth_newBlockFilter|eth_newPendingTransactionFilter|eth_getFilterChanges|eth_getFilterLogs|eth_uninstallFilter|txpool_content|txpool_contentFrom|txpool_inspect|txpool_inspectFrom|txpool_status|txpool_statusFrom|eth_gasPrice|eth_call|eth_estimateGas|eth_getCode|eth_getStorageAt|eth_getProof|eth_sendRawTransaction|eth_sendTransaction|eth_getTransactionCount|eth_getTransactionByHash|eth_getTransactionReceipt|evm_sendRawTransaction|evm_send_raw_transaction|evm_sendTransaction|evm_send_transaction|evm_publicSendRawTransaction|evm_public_send_raw_transaction|evm_publicSendRawTransactionBatch|evm_public_send_raw_transaction_batch|evm_publicSendTransaction|evm_public_send_transaction|evm_publicSendTransactionBatch|evm_public_send_transaction_batch|evm_getLogs|evm_get_logs|evm_getLogsBatch|evm_get_logs_batch|evm_getTransactionReceipt|evm_get_transaction_receipt|evm_getTransactionReceiptBatch|evm_get_transaction_receipt_batch|evm_getTransactionByHashBatch|evm_get_transaction_by_hash_batch|evm_subscribe|evm_unsubscribe|evm_newFilter|evm_new_filter|evm_newBlockFilter|evm_new_block_filter|evm_newPendingTransactionFilter|evm_new_pending_transaction_filter|evm_getFilterChanges|evm_get_filter_changes|evm_getFilterChangesBatch|evm_get_filter_changes_batch|evm_getFilterLogs|evm_get_filter_logs|evm_getFilterLogsBatch|evm_get_filter_logs_batch|evm_uninstallFilter|evm_uninstall_filter|evm_chainId|evm_chain_id|evm_clientVersion|evm_client_version|evm_sha3|evm_protocolVersion|evm_protocol_version|evm_listening|evm_peerCount|evm_peer_count|evm_accounts|evm_coinbase|evm_mining|evm_hashrate|evm_netVersion|evm_net_version|evm_syncing|evm_blockNumber|evm_block_number|evm_getBalance|evm_get_balance|evm_getBlockByNumber|evm_get_block_by_number|evm_getBlockByHash|evm_get_block_by_hash|evm_getBlockReceipts|evm_get_block_receipts|evm_getTransactionByHash|evm_get_transaction_by_hash|evm_getTransactionCount|evm_get_transaction_count|evm_gasPrice|evm_gas_price|evm_call|evm_estimateGas|evm_estimate_gas|evm_getCode|evm_get_code|evm_getStorageAt|evm_get_storage_at|evm_getProof|evm_get_proof|evm_maxPriorityFeePerGas|evm_max_priority_fee_per_gas|evm_feeHistory|evm_fee_history|evm_getTransactionByBlockNumberAndIndex|evm_get_transaction_by_block_number_and_index|evm_getTransactionByBlockHashAndIndex|evm_get_transaction_by_block_hash_and_index|evm_getBlockTransactionCountByNumber|evm_get_block_transaction_count_by_number|evm_getBlockTransactionCountByHash|evm_get_block_transaction_count_by_hash|evm_getUncleCountByBlockNumber|evm_get_uncle_count_by_block_number|evm_getUncleCountByBlockHash|evm_get_uncle_count_by_block_hash|evm_getUncleByBlockNumberAndIndex|evm_get_uncle_by_block_number_and_index|evm_getUncleByBlockHashAndIndex|evm_get_uncle_by_block_hash_and_index|evm_pendingTransactions|evm_pending_transactions|evm_txpoolContent|evm_txpool_content|evm_txpoolContentFrom|evm_txpool_contentFrom|evm_txpool_content_from|evm_txpoolInspect|evm_txpool_inspect|evm_txpoolInspectFrom|evm_txpool_inspectFrom|evm_txpool_inspect_from|evm_txpoolStatus|evm_txpool_status|evm_txpoolStatusFrom|evm_txpool_statusFrom|evm_txpool_status_from|evm_snapshotPendingIngress|evm_snapshot_pending_ingress|evm_snapshotExecutableIngress|evm_snapshot_executable_ingress|evm_drainExecutableIngress|evm_drain_executable_ingress|evm_drainPendingIngress|evm_drain_pending_ingress|evm_snapshotPendingSenderBuckets|evm_snapshot_pending_sender_buckets|evm_getPublicBroadcastStatus|evm_get_public_broadcast_status|evm_getBroadcastStatus|evm_get_broadcast_status|evm_getPublicBroadcastStatusBatch|evm_get_public_broadcast_status_batch|evm_getBroadcastStatusBatch|evm_get_broadcast_status_batch|evm_getTransactionLifecycleBatch|evm_get_transaction_lifecycle_batch|evm_getTxSubmitStatusBatch|evm_get_tx_submit_status_batch|evm_replayPublicBroadcast|evm_replay_public_broadcast|evm_replayPublicBroadcastBatch|evm_replay_public_broadcast_batch|evm_getTransactionLifecycle|evm_get_transaction_lifecycle|evm_getTxSubmitStatus|evm_get_tx_submit_status|evm_getSettlementById|evm_get_settlement_by_id|evm_getSettlementByTxHash|evm_get_settlement_by_tx_hash|evm_replaySettlementPayout|evm_replay_settlement_payout|evm_getAtomicReadyByIntentId|evm_get_atomic_ready_by_intent_id|evm_replayAtomicReady|evm_replay_atomic_ready|evm_queueAtomicBroadcast|evm_queue_atomic_broadcast|evm_replayAtomicBroadcastQueue|evm_replay_atomic_broadcast_queue|evm_markAtomicBroadcastFailed|evm_mark_atomic_broadcast_failed|evm_markAtomicBroadcasted|evm_mark_atomic_broadcasted|evm_executeAtomicBroadcast|evm_execute_atomic_broadcast|evm_executePendingAtomicBroadcasts|evm_execute_pending_atomic_broadcasts|web30_sendRawTransaction|web30_sendTransaction",
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

fn upsert_gateway_eth_broadcast_status(
    tx_hash: [u8; 32],
    broadcast_result: &Option<(String, u64, String)>,
) {
    let status = match broadcast_result {
        Some((output, attempts, executor)) => GatewayEthBroadcastStatus {
            mode: "external".to_string(),
            attempts: Some(*attempts),
            executor: Some(executor.clone()),
            executor_output: Some(output.clone()),
            updated_at_unix_ms: now_unix_millis(),
        },
        None => GatewayEthBroadcastStatus {
            mode: "none".to_string(),
            attempts: None,
            executor: None,
            executor_output: None,
            updated_at_unix_ms: now_unix_millis(),
        },
    };
    if let Ok(mut map) = gateway_eth_broadcast_status_store().lock() {
        map.insert(tx_hash, status);
    }
}

fn gateway_eth_broadcast_status_json_by_tx(tx_hash: &[u8; 32]) -> serde_json::Value {
    let status = if let Ok(map) = gateway_eth_broadcast_status_store().lock() {
        map.get(tx_hash).cloned()
    } else {
        None
    };
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
