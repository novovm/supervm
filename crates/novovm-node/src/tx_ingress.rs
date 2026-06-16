#![forbid(unsafe_code)]

use crate::clearing_router::{NovClearingRouterImplV1, NovClearingRouterV1};
use crate::clearing_types::{
    NovClearingFailureCodeV1, NovClearingRouteQuoteV1, NovExecutionFeeRequestV1,
    NovLastClearingRouteV1, NovReceiptRouteMetaV1, NovRouteSourceV1, NovStaticAmmPoolStateV1,
};
use crate::liquidity_sources::{StaticAmmPoolLiquidityV1, TreasuryDirectLiquidityV1};
use crate::treasury_settlement::settle_clearing_result_into_treasury_v1;
use crate::unified_account_surface::get_unified_account_key_algo_with_store_path_v1;
use anyhow::{bail, Context, Result};
use novovm_adapter_api::{TxExecutionPolicyV1, TxIR, TxType, UcaKeyAlgo};
use novovm_exec::{
    recommend_threads_auto, AoemExecFacade, AoemHostHint, AoemRuntimeConfig, EncodedOpsWire,
    ExecOpV2, OpsWireOp, OpsWireV1Builder, RawIngressCodecRegistry, AOEM_OPS_WIRE_V1_MAGIC,
    AOEM_OPS_WIRE_V1_VERSION,
};
use novovm_governance_observability::{append_governance_event_auto, GovernanceEvent};
use novovm_network::{
    eth_rlpx_transaction_hash_v1, eth_rlpx_validate_transaction_envelope_payload_v1,
    get_network_runtime_native_head_snapshot_v1, get_network_runtime_native_pending_tx_payload_v1,
    observe_network_runtime_native_execution_budget_target_v1,
    observe_network_runtime_native_execution_budget_throttle_v1,
    observe_network_runtime_native_pending_tx_dropped_v1,
    observe_network_runtime_native_pending_tx_local_ingress_with_payload_v1,
    observe_network_runtime_native_pending_tx_local_native_payload_v1,
    observe_network_runtime_native_pending_tx_rejected_v1,
    set_network_runtime_native_body_snapshot_v1, set_network_runtime_native_head_snapshot_v1,
    snapshot_network_runtime_native_active_pending_txs_v1,
    snapshot_network_runtime_native_execution_budget_runtime_summary_v1,
    NetworkRuntimeNativeBodySnapshotV1, NetworkRuntimeNativeExecutionBudgetTargetObservationV1,
    NetworkRuntimeNativeHeadSnapshotV1, NetworkRuntimeNativePendingTxLifecycleStageV1,
    NetworkRuntimeNativeSyncPhaseV1,
};
use novovm_protocol::{
    decode_local_tx_wire_v1 as decode_tx_wire_v1, decode_nov_native_tx_wire_v1,
    encode_nov_native_tx_wire_v1, LocalTxWireV1, NovExecuteTxV1, NovExecutionModeV1,
    NovExecutionPolicyV1, NovExecutionTargetV1, NovFeePolicyV1, NovGovernanceTxV1,
    NovNativeTxWireV1, NovPrivacyModeV1, NovTxKindV1, NovVerificationModeV1,
};
use rocksdb::{
    Direction as RocksDbDirection, IteratorMode as RocksDbIteratorMode, Options as RocksDbOptions,
    WriteBatch as RocksDbWriteBatch, DB as RocksDb,
};
use std::collections::{BTreeMap, BTreeSet};
use std::fs::{self, OpenOptions};
use std::io::{BufRead, BufReader, Write};
use std::path::{Path, PathBuf};
use std::sync::OnceLock;
use std::time::{Duration, Instant};

pub const LOCAL_TX_WIRE_V1_BYTES: usize = 4 + 1 + (8 * 5) + 32;
pub const NOV_NATIVE_GOVERNANCE_ALLOWLIST_ENV: &str = "NOVOVM_NATIVE_GOVERNANCE_PROPOSERS";
pub const NOV_NATIVE_GOVERNANCE_ENABLED_ENV: &str = "NOVOVM_NATIVE_GOVERNANCE_ENABLED";
pub const NOV_NATIVE_EXECUTION_STORE_ENV: &str = "NOVOVM_NATIVE_EXECUTION_STORE";
pub const NOV_NATIVE_EXECUTION_STORE_BACKEND_ENV: &str = "NOVOVM_NATIVE_EXECUTION_STORE_BACKEND";
pub const NOV_NATIVE_EXECUTION_STORE_ROCKSDB_PATH_ENV: &str =
    "NOVOVM_NATIVE_EXECUTION_STORE_ROCKSDB_PATH";
pub const NOV_EXECUTION_FEE_CLASSIFICATION_CONTRACT_V1: &str = "novovm-exec-fee/v1";
pub const NOV_EXECUTION_FEE_QUOTE_CONTRACT_V1: &str = "novovm-exec-fee-quote/v1";
pub const NOV_EXECUTION_FEE_CLEARING_CONTRACT_V1: &str = "novovm-exec-fee-clearing/v1";
pub const NOV_NATIVE_FEE_QUOTE_TTL_MS_ENV: &str = "NOVOVM_NATIVE_FEE_QUOTE_TTL_MS";
pub const NOV_NATIVE_FEE_ORACLE_MAX_AGE_MS_ENV: &str = "NOVOVM_NATIVE_FEE_ORACLE_MAX_AGE_MS";
pub const NOV_NATIVE_FEE_RATE_PPM_ENV: &str = "NOVOVM_NATIVE_FEE_RATE_PPM";
pub const NOV_NATIVE_FEE_DEFAULT_CLEARING_NOV_LIQUIDITY_ENV: &str =
    "NOVOVM_NATIVE_FEE_DEFAULT_CLEARING_NOV_LIQUIDITY";
pub const NOV_NATIVE_FEE_CLEARING_DEFAULT_ASSETS_ENV: &str =
    "NOVOVM_NATIVE_FEE_CLEARING_DEFAULT_ASSETS";
pub const NOV_NATIVE_TREASURY_SETTLEMENT_PAUSED_ENV: &str =
    "NOVOVM_NATIVE_TREASURY_SETTLEMENT_PAUSED";
pub const NOV_NATIVE_TREASURY_REDEEM_PAUSED_ENV: &str = "NOVOVM_NATIVE_TREASURY_REDEEM_PAUSED";
pub const NOV_NATIVE_TREASURY_RESERVE_SHARE_BPS_ENV: &str =
    "NOVOVM_NATIVE_TREASURY_RESERVE_SHARE_BPS";
pub const NOV_NATIVE_TREASURY_FEE_SHARE_BPS_ENV: &str = "NOVOVM_NATIVE_TREASURY_FEE_SHARE_BPS";
pub const NOV_NATIVE_TREASURY_RISK_BUFFER_SHARE_BPS_ENV: &str =
    "NOVOVM_NATIVE_TREASURY_RISK_BUFFER_SHARE_BPS";
pub const NOV_NATIVE_TREASURY_MIN_RESERVE_BUCKET_NOV_ENV: &str =
    "NOVOVM_NATIVE_TREASURY_MIN_RESERVE_BUCKET_NOV";
pub const NOV_NATIVE_TREASURY_MIN_FEE_BUCKET_NOV_ENV: &str =
    "NOVOVM_NATIVE_TREASURY_MIN_FEE_BUCKET_NOV";
pub const NOV_NATIVE_TREASURY_MIN_RISK_BUFFER_NOV_ENV: &str =
    "NOVOVM_NATIVE_TREASURY_MIN_RISK_BUFFER_NOV";
pub const NOV_NATIVE_AOEM_SEMANTIC_INGRESS_ENABLED_ENV: &str =
    "NOVOVM_NATIVE_AOEM_SEMANTIC_INGRESS_ENABLED";
pub const NOV_NATIVE_AOEM_SEMANTIC_INGRESS_REQUIRED_ENV: &str =
    "NOVOVM_NATIVE_AOEM_SEMANTIC_INGRESS_REQUIRED";
pub const NOV_NATIVE_AOEM_SEMANTIC_LEDGER_MIRROR_ENV: &str =
    "NOVOVM_NATIVE_AOEM_SEMANTIC_LEDGER_MIRROR";
pub const NOV_NATIVE_AOEM_CONCURRENT_EXECUTION_ENABLED_ENV: &str =
    "NOVOVM_NATIVE_AOEM_CONCURRENT_EXECUTION_ENABLED";
pub const NOV_NATIVE_AOEM_BATCH_MAX_SIZE_ENV: &str = "NOVOVM_NATIVE_AOEM_BATCH_MAX_SIZE";
pub const NOV_NATIVE_SEND_RAW_TRANSACTION_PIPELINE_ONLY_ENV: &str =
    "NOVOVM_NATIVE_SEND_RAW_TRANSACTION_PIPELINE_ONLY";
pub const NOV_NATIVE_CLEARING_ENABLED_ENV: &str = "NOVOVM_NATIVE_CLEARING_ENABLED";
const ERR_PQ_REQUIRED_BUT_KEY_NOT_PQ: &str = "ERR_PQ_REQUIRED_BUT_KEY_NOT_PQ";
const ERR_PRIVACY_REQUIRED_BUT_PATH_NOT_AVAILABLE: &str =
    "ERR_PRIVACY_REQUIRED_BUT_PATH_NOT_AVAILABLE";
pub const NOV_NATIVE_CLEARING_DAILY_NOV_HARD_LIMIT_ENV: &str =
    "NOVOVM_NATIVE_CLEARING_DAILY_NOV_HARD_LIMIT";
pub const NOV_NATIVE_CLEARING_REQUIRE_HEALTHY_RISK_BUFFER_ENV: &str =
    "NOVOVM_NATIVE_CLEARING_REQUIRE_HEALTHY_RISK_BUFFER";
pub const NOV_NATIVE_CLEARING_CONSTRAINED_MAX_SLIPPAGE_BPS_ENV: &str =
    "NOVOVM_NATIVE_CLEARING_CONSTRAINED_MAX_SLIPPAGE_BPS";
pub const NOV_NATIVE_CLEARING_CONSTRAINED_DAILY_USAGE_BPS_ENV: &str =
    "NOVOVM_NATIVE_CLEARING_CONSTRAINED_DAILY_USAGE_BPS";
pub const NOV_NATIVE_CLEARING_CONSTRAINED_STRATEGY_ENV: &str =
    "NOVOVM_NATIVE_CLEARING_CONSTRAINED_STRATEGY";
pub const NOV_NATIVE_PROTOCOL_CLEARING_EPOCH_MS_ENV: &str =
    "NOVOVM_NATIVE_PROTOCOL_CLEARING_EPOCH_MS";
const NOV_NATIVE_EXECUTION_STORE_SCHEMA_V1: &str = "novovm-native-execution-runtime/v1";
const NOV_NATIVE_EXECUTION_STORE_BACKEND_JSON_V1: &str = "json";
const NOV_NATIVE_EXECUTION_STORE_BACKEND_ROCKSDB_V1: &str = "rocksdb";
const NOV_NATIVE_EXECUTION_STORE_BACKEND_DUAL_V1: &str = "dual";
const NOV_NATIVE_EXECUTION_STORE_ROCKSDB_KEY_SNAPSHOT_V1: &[u8] =
    b"nov_native_execution_store:snapshot:v1";
const NOV_NATIVE_EXECUTION_STORE_ROCKSDB_KEY_MODULE_STATE_CORE_V1: &[u8] = b"module_state/core";
const NOV_NATIVE_EXECUTION_STORE_ROCKSDB_KEY_MODULE_STATE_TREASURY_V1: &[u8] =
    b"module_state/treasury/state";
const NOV_NATIVE_EXECUTION_STORE_ROCKSDB_KEY_MODULE_STATE_CLEARING_V1: &[u8] =
    b"module_state/clearing/state";
const NOV_NATIVE_EXECUTION_STORE_ROCKSDB_KEY_MODULE_STATE_VAULT_V1: &[u8] =
    b"module_state/vault/state";
const NOV_NATIVE_EXECUTION_STORE_ROCKSDB_KEY_MODULE_STATE_POLICY_V1: &[u8] =
    b"module_state/policy/state";
const NOV_NATIVE_EXECUTION_STORE_ROCKSDB_KEY_MODULE_STATE_GOVERNANCE_V1: &[u8] =
    b"module_state/governance/state";
const NOV_NATIVE_EXECUTION_STORE_ROCKSDB_KEY_MODULE_STATE_NATIVE_EXECUTION_V1: &[u8] =
    b"module_state/native_execution/state";
const NOV_NATIVE_EXECUTION_STORE_ROCKSDB_KEY_SEMANTIC_HEAD_V1: &[u8] = b"semantic_head/current";
const NOV_NATIVE_EXECUTION_STORE_ROCKSDB_KEY_SEMANTIC_SEQUENCE_V1: &[u8] =
    b"semantic_head/current_sequence";
const NOV_NATIVE_EXECUTION_STORE_ROCKSDB_ACCOUNT_ASSET_PREFIX_V1: &[u8] = b"account/";
const NOV_NATIVE_EXECUTION_STORE_ROCKSDB_RECEIPT_PREFIX_V1: &[u8] = b"receipt/";
const NOV_NATIVE_EXECUTION_STORE_ROCKSDB_RECEIPT_BY_HEIGHT_PREFIX_V1: &[u8] = b"receipt_by_height/";
const NOV_NATIVE_EXECUTION_STORE_ROCKSDB_SEMANTIC_BY_HEIGHT_PREFIX_V1: &[u8] =
    b"semantic_head/by_height/";
const NOV_NATIVE_EXECUTION_STORE_ROCKSDB_SNAPSHOT_META_PREFIX_V1: &[u8] = b"snapshot_meta/";
const NOV_NATIVE_EXECUTION_STORE_ROCKSDB_SNAPSHOT_META_CURRENT_V1: &[u8] = b"snapshot_meta/current";
const NOV_NATIVE_EXECUTION_STORE_LOCK_TIMEOUT_MS_V1: u64 = 10_000;
const NOV_NATIVE_EXECUTION_STORE_LOCK_STALE_MS_V1: u64 = 60_000;
const NOV_NATIVE_EXECUTION_STORE_LOCK_POLL_MS_V1: u64 = 10;
const NOV_FEE_RATE_PPM_DENOMINATOR_V1: u128 = 1_000_000;
const NOV_FEE_RATE_PPM_NOV_V1: u128 = NOV_FEE_RATE_PPM_DENOMINATOR_V1;
const NOV_FEE_RATE_PPM_USDT_V1: u128 = 2_000_000;
const NOV_FEE_RATE_PPM_DAI_V1: u128 = 2_000_000;
const NOV_FEE_RATE_PPM_NUSD_V1: u128 = NOV_FEE_RATE_PPM_DENOMINATOR_V1;
const NOV_FEE_RATE_PPM_ETH_V1: u128 = 6_000_000_000;
const NOV_FEE_RATE_PPM_BTC_V1: u128 = 50_000_000_000;
const NOV_FEE_QUOTE_DEFAULT_TTL_MS_V1: u128 = 15_000;
const NOV_FEE_ORACLE_DEFAULT_MAX_AGE_MS_V1: u128 = 300_000;
const NOV_FEE_DEFAULT_CLEARING_NOV_LIQUIDITY_V1: u128 = 1_000_000_000;
const NOV_FEE_CLEARING_DEFAULT_ASSETS_V1: &str = "USDT,DAI,NUSD,ETH,BTC";
const NOV_FEE_FAILURE_QUOTE_PREFIX_V1: &str = "fee.quote";
const NOV_FEE_FAILURE_CLEARING_PREFIX_V1: &str = "fee.clearing";
const NOV_FEE_FAILURE_SETTLEMENT_PREFIX_V1: &str = "fee.settlement";
const NOV_TREASURY_SHARE_BPS_DENOMINATOR_V1: u32 = 10_000;
const NOV_TREASURY_RESERVE_SHARE_BPS_DEFAULT_V1: u32 = 7000;
const NOV_TREASURY_FEE_SHARE_BPS_DEFAULT_V1: u32 = 2000;
const NOV_TREASURY_RISK_BUFFER_SHARE_BPS_DEFAULT_V1: u32 = 1000;
const NOV_TREASURY_MIN_RESERVE_BUCKET_NOV_DEFAULT_V1: u128 = 0;
const NOV_TREASURY_MIN_FEE_BUCKET_NOV_DEFAULT_V1: u128 = 0;
const NOV_TREASURY_MIN_RISK_BUFFER_NOV_DEFAULT_V1: u128 = 1_000;
const NOV_TREASURY_POLICY_VERSION_DEFAULT_V1: u32 = 1;
const NOV_TREASURY_SETTLEMENT_JOURNAL_MAX_ENTRIES_V1: usize = 512;
const NOV_EXECUTION_TRACE_MAX_ENTRIES_V1: usize = 512;
const NOV_CLEARING_DAILY_NOV_HARD_LIMIT_DEFAULT_V1: u128 = 0;
const NOV_CLEARING_CONSTRAINED_MAX_SLIPPAGE_BPS_DEFAULT_V1: u32 = 50;
const NOV_CLEARING_CONSTRAINED_DAILY_USAGE_BPS_DEFAULT_V1: u32 = 8_000;
const NOV_CLEARING_CONSTRAINED_STRATEGY_DAILY_VOLUME_ONLY_V1: &str = "daily_volume_only";
const NOV_CLEARING_CONSTRAINED_STRATEGY_TREASURY_DIRECT_ONLY_V1: &str = "treasury_direct_only";
const NOV_CLEARING_CONSTRAINED_STRATEGY_BLOCKED_V1: &str = "blocked";
const NOV_PROTOCOL_CLEARING_EPOCH_MS_DEFAULT_V1: u128 = 300_000;
const NOV_NATIVE_AOEM_BATCH_MAX_SIZE_DEFAULT_V1: usize = 1024;
const NOV_PROTOCOL_CLEARING_MAX_EPOCH_UP_BPS_V1: u32 = 500;
const NOV_PROTOCOL_CLEARING_MAX_EPOCH_DOWN_BPS_V1: u32 = 500;
const NOV_PROTOCOL_CLEARING_MAX_SOURCE_DEVIATION_BPS_V1: u32 = 2_000;
const NOV_PROTOCOL_CLEARING_MIN_AMM_TWAP_NOV_LIQUIDITY_V1: u128 = 1_000_000;
const NOV_PROTOCOL_CLEARING_RESERVE_HAIRCUT_BPS_V1: u32 = 100;
const NOV_PROTOCOL_CLEARING_LIQUIDITY_HAIRCUT_BPS_V1: u32 = 100;
const NOV_PROTOCOL_CLEARING_VOLATILITY_HAIRCUT_BPS_V1: u32 = 0;
const NOV_PROTOCOL_CLEARING_REDEMPTION_SPREAD_BPS_V1: u32 = 100;
const NOV_PROTOCOL_CLEARING_RISK_SURCHARGE_BPS_V1: u32 = 0;
const NOV_CREDIT_ENGINE_MIN_COLLATERAL_RATIO_BPS_V1: u32 = 15_000;
const NOV_MILLIS_PER_DAY_V1: u128 = 86_400_000;

#[derive(Debug, Clone, Copy)]
pub struct TxIngressRecord {
    pub account: u64,
    pub key: u64,
    pub value: u64,
    pub nonce: u64,
    pub fee: u64,
    pub signature: [u8; 32],
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
pub enum NovExecutionRequestTargetV1 {
    NativeModule(String),
    WasmApp(String),
    Plugin(String),
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
pub struct NovExecutionRequestV1 {
    pub tx_hash: [u8; 32],
    pub chain_id: u64,
    pub caller: Vec<u8>,
    pub target: NovExecutionRequestTargetV1,
    pub method: String,
    pub args: Vec<u8>,
    pub fee_pay_asset: String,
    pub fee_max_pay_amount: u128,
    pub fee_slippage_bps: u32,
    pub gas_like_limit: Option<u64>,
    pub nonce: u64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct NovRequestedExecutionBehaviorV1 {
    execution_policy: NovExecutionPolicyV1,
    privacy_mode: NovPrivacyModeV1,
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct NovExecutionSubjectMetaV1 {
    pub account_id: String,
    pub fee_owner_account_id: String,
    pub nonce_owner_account_id: String,
    #[serde(default)]
    pub key_algo: String,
    #[serde(default)]
    pub execution_policy: String,
    #[serde(default)]
    pub policy_enforced: bool,
    #[serde(default)]
    pub policy_rejection_reason: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct NovSettledFeeV1 {
    pub nov_amount: u128,
    pub source_asset: String,
    pub source_amount: u128,
    #[serde(default)]
    pub required_source_amount: u128,
    #[serde(default)]
    pub quote_expires_at_unix_ms: u128,
    #[serde(default)]
    pub clearing_route_ref: String,
    #[serde(default)]
    pub clearing_source: String,
    #[serde(default)]
    pub clearing_rate_ppm: u128,
    #[serde(default)]
    pub route_expected_nov_out: u128,
    #[serde(default)]
    pub route_fee_ppm: u32,
    #[serde(default)]
    pub route_selection_reason: String,
    #[serde(default)]
    pub route_candidate_count: u32,
    pub route: String,
    pub fee_contract: String,
    pub quote_id: String,
    pub quote_contract: String,
    pub clearing_contract: String,
    pub price_source: String,
    #[serde(default)]
    pub policy_contract_id: String,
    #[serde(default)]
    pub policy_version: u32,
    #[serde(default)]
    pub policy_source: String,
    #[serde(default)]
    pub policy_threshold_state: String,
    #[serde(default)]
    pub policy_constrained_strategy: String,
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct NovFeeQuoteV1 {
    pub quote_id: String,
    pub pay_asset: String,
    pub nov_amount: u128,
    pub quoted_pay_amount: u128,
    pub quoted_pay_amount_with_slippage: u128,
    pub max_pay_amount: u128,
    pub slippage_bps: u32,
    pub quoted_at_unix_ms: u128,
    pub expires_at_unix_ms: u128,
    pub rate_ppm: u128,
    #[serde(default)]
    pub oracle_updated_at_unix_ms: u128,
    pub route: String,
    pub quote_contract: String,
    pub price_source: String,
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct NovProtocolClearingPriceV1 {
    pub asset: String,
    pub epoch: u128,
    pub epoch_ms: u128,
    pub p_prev_ppm: u128,
    pub p_ref_ppm: u128,
    pub p_epoch_ppm: u128,
    pub p_pay_ppm: u128,
    pub p_redeem_ppm: u128,
    pub p_amm_twap_ppm: Option<u128>,
    pub p_nav_ppm: Option<u128>,
    pub p_oracle_ref_ppm: Option<u128>,
    pub reserve_haircut_bps: u32,
    pub liquidity_haircut_bps: u32,
    pub volatility_haircut_bps: u32,
    pub redemption_spread_bps: u32,
    pub risk_surcharge_bps: u32,
    pub max_epoch_up_bps: u32,
    pub max_epoch_down_bps: u32,
    pub max_source_deviation_bps: u32,
    pub state: String,
    pub sources_used: Vec<String>,
    pub sources_rejected: Vec<String>,
    pub reason: Option<String>,
    pub updated_unix_ms: u128,
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct NovNativeExecutionLogV1 {
    pub module: String,
    pub method: String,
    pub event: String,
    pub data: serde_json::Value,
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize, Default)]
pub struct NovAoemSemanticIngressMetaV1 {
    pub execution_kernel: String,
    pub semantic_entry: String,
    pub algebraic_semantic_entry: bool,
    #[serde(default)]
    pub ingress_scope: String,
    #[serde(default)]
    pub batch_plan_id: Option<u64>,
    #[serde(default)]
    pub batch_item_index: Option<usize>,
    #[serde(default)]
    pub batch_item_count: Option<usize>,
    #[serde(default)]
    pub concurrent_execution_enabled: bool,
    #[serde(default)]
    pub concurrent_execution_model: String,
    #[serde(default)]
    pub batch_mode: bool,
    #[serde(default)]
    pub batch_size: usize,
    #[serde(default)]
    pub recommended_threads: usize,
    #[serde(default)]
    pub ingress_workers: u32,
    #[serde(default)]
    pub host_hw_threads: usize,
    #[serde(default)]
    pub host_budget_threads: usize,
    #[serde(default)]
    pub parallelism_reason: String,
    pub enabled: bool,
    pub required: bool,
    pub submitted: bool,
    pub op_count: usize,
    pub plan_id: u64,
    pub wire_digest: String,
    pub processed_ops: u32,
    pub success_ops: u32,
    pub total_writes: u64,
    #[serde(default)]
    pub semantic_delta_count: usize,
    #[serde(default)]
    pub semantic_delta_digest: String,
    #[serde(default)]
    pub semantic_state_before_digest: String,
    #[serde(default)]
    pub semantic_state_after_digest: String,
    #[serde(default)]
    pub semantic_ledger_sequence: u64,
    #[serde(default)]
    pub semantic_ledger_prev_seal: String,
    #[serde(default)]
    pub semantic_ledger_commit_seal: String,
    pub return_code_name: String,
    pub fallback_reason: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct NovAoemSemanticLedgerMirrorRecordV1 {
    pub schema: String,
    pub execution_kernel: String,
    pub semantic_entry: String,
    pub algebraic_semantic_entry: bool,
    pub sequence: u64,
    pub tx_hash: String,
    pub plan_id: u64,
    pub wire_digest: String,
    pub delta_digest: String,
    pub state_before_digest: String,
    pub state_after_digest: String,
    pub prev_seal: String,
    pub commit_seal: String,
    pub mirror_backend: String,
    pub source: String,
    pub created_at_ms: u128,
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct NovAoemSemanticMutationCommitV1 {
    pub schema: String,
    pub execution_kernel: String,
    pub semantic_entry: String,
    pub algebraic_semantic_entry: bool,
    pub enabled: bool,
    pub required: bool,
    pub submitted: bool,
    pub op_count: usize,
    pub plan_id: u64,
    pub wire_digest: String,
    pub processed_ops: u32,
    pub success_ops: u32,
    pub total_writes: u64,
    pub semantic_delta_count: usize,
    pub semantic_delta_digest: String,
    pub state_before_digest: String,
    pub state_after_digest: String,
    pub sequence: u64,
    pub prev_seal: String,
    pub commit_seal: String,
    pub source: String,
    pub subject: String,
    pub action: String,
    pub tx_ref: String,
    pub mirror_backend: String,
    pub fallback_reason: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct NovNativeExecutionReceiptV1 {
    pub tx_hash: String,
    pub status: bool,
    pub target: String,
    pub module: String,
    pub method: String,
    #[serde(default)]
    pub account_id: String,
    #[serde(default)]
    pub fee_owner_account_id: String,
    #[serde(default)]
    pub nonce_owner_account_id: String,
    #[serde(default)]
    pub key_algo: String,
    #[serde(default)]
    pub execution_policy: String,
    #[serde(default)]
    pub policy_enforced: bool,
    #[serde(default)]
    pub policy_rejection_reason: Option<String>,
    pub settled_fee_nov: u128,
    pub paid_asset: String,
    pub paid_amount: u128,
    pub logs: Vec<NovNativeExecutionLogV1>,
    pub failure_reason: Option<String>,
    pub fee_contract: String,
    #[serde(default)]
    pub fee_route: String,
    #[serde(default)]
    pub fee_quote_id: String,
    #[serde(default)]
    pub fee_quote_contract: String,
    #[serde(default)]
    pub fee_clearing_contract: String,
    #[serde(default)]
    pub fee_price_source: String,
    #[serde(default)]
    pub fee_quote_required_pay_amount: u128,
    #[serde(default)]
    pub fee_quote_expires_at_unix_ms: u128,
    #[serde(default)]
    pub fee_clearing_route_ref: String,
    #[serde(default)]
    pub fee_clearing_source: String,
    #[serde(default)]
    pub fee_clearing_rate_ppm: u128,
    #[serde(default)]
    pub route_meta: Option<NovReceiptRouteMetaV1>,
    #[serde(default)]
    pub policy_meta: Option<NovReceiptPolicyMetaV1>,
    #[serde(default)]
    pub aoem_semantic_ingress: Option<NovAoemSemanticIngressMetaV1>,
    #[serde(default)]
    pub aoem_semantic_commit: Option<NovAoemSemanticMutationCommitV1>,
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct NovReceiptPolicyMetaV1 {
    pub policy_contract_id: String,
    pub policy_version: u32,
    pub policy_source: String,
    #[serde(default, alias = "threshold_state")]
    pub policy_threshold_state: String,
    #[serde(default, alias = "constrained_strategy")]
    pub policy_constrained_strategy: String,
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize, Default)]
pub struct NovTraceQuotePhaseV1 {
    pub quote_id: Option<String>,
    pub quoted_pay_amount: Option<u128>,
    pub quoted_pay_amount_with_slippage: Option<u128>,
    pub quoted_at_unix_ms: Option<u128>,
    pub quote_expiry_unix_ms: Option<u128>,
    pub oracle_source: Option<String>,
    pub oracle_updated_at_unix_ms: Option<u128>,
    pub quote_failure_code: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize, Default)]
pub struct NovTraceRouteCandidateV1 {
    pub route_id: String,
    pub route_source: String,
    pub expected_nov_out: u128,
    pub liquidity_available: u128,
    pub fee_ppm: u32,
    pub quoted_at_ms: u64,
    pub expires_at_ms: u64,
    pub rejected_by_policy: bool,
    pub rejected_reason: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize, Default)]
pub struct NovTraceSelectedRouteV1 {
    pub route_id: String,
    pub route_source: String,
    pub expected_nov_out: u128,
    pub selection_reason: String,
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize, Default)]
pub struct NovTraceRoutingPhaseV1 {
    pub candidate_route_count: usize,
    pub candidate_routes: Vec<NovTraceRouteCandidateV1>,
    pub selected_route: Option<NovTraceSelectedRouteV1>,
    pub routing_failure_code: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize, Default)]
pub struct NovTraceClearingPhaseV1 {
    pub actual_route_id: Option<String>,
    pub actual_route_source: Option<String>,
    pub actual_pay_amount: Option<u128>,
    pub actual_nov_out: Option<u128>,
    pub actual_fee_ppm: Option<u32>,
    pub slippage_bps_realized: Option<u32>,
    pub clearing_failure_code: Option<String>,
    pub cleared_at_ms: Option<u64>,
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize, Default)]
pub struct NovTraceSettlementPhaseV1 {
    pub settled_fee_nov: Option<u128>,
    pub reserve_bucket_delta_nov: Option<i128>,
    pub fee_bucket_delta_nov: Option<i128>,
    pub risk_buffer_delta_nov: Option<i128>,
    pub settlement_journal_entry_type: Option<String>,
    pub settlement_status: Option<String>,
    pub settlement_failure_code: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize, Default)]
pub struct NovExecutionTraceV1 {
    pub trace_id: String,
    pub tx_id: String,
    #[serde(default)]
    pub account_id: String,
    #[serde(default)]
    pub fee_owner_account_id: String,
    #[serde(default)]
    pub nonce_owner_account_id: String,
    #[serde(default)]
    pub key_algo: String,
    #[serde(default)]
    pub execution_policy: String,
    #[serde(default)]
    pub policy_enforced: bool,
    #[serde(default)]
    pub policy_rejection_reason: Option<String>,
    pub pay_asset: String,
    pub max_pay_amount: u128,
    pub nov_needed: u128,
    pub policy_contract_id: String,
    pub policy_source: String,
    pub policy_threshold_state: String,
    pub policy_constrained_strategy: String,
    pub quote_phase: NovTraceQuotePhaseV1,
    pub routing_phase: NovTraceRoutingPhaseV1,
    pub clearing_phase: NovTraceClearingPhaseV1,
    pub settlement_phase: NovTraceSettlementPhaseV1,
    #[serde(default)]
    pub aoem_semantic_ingress: Option<NovAoemSemanticIngressMetaV1>,
    pub final_status: String,
    pub final_failure_code: Option<String>,
    pub created_at_ms: u128,
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct NovTreasurySettlementPolicyV1 {
    pub policy_version: u32,
    pub policy_source: String,
    pub reserve_share_bps: u32,
    pub fee_share_bps: u32,
    pub risk_buffer_share_bps: u32,
    pub min_reserve_bucket_nov: u128,
    pub min_fee_bucket_nov: u128,
    pub min_risk_buffer_nov: u128,
    pub settlement_paused: bool,
    pub redeem_paused: bool,
    pub mapped_lock_bridge_paused: bool,
    #[serde(default)]
    pub mapped_lock_min_confirmations: u64,
    #[serde(default)]
    pub mapped_lock_contract_address: String,
    pub mapped_asset_burn_paused: bool,
    pub mapped_asset_release_paused: bool,
    pub mapped_asset_auto_heal_enabled: bool,
    #[serde(default)]
    pub mapped_asset_auto_heal_rollback_enabled: bool,
    #[serde(default)]
    pub mapped_asset_reorg_response_policy: String,
    pub clearing_enabled: bool,
    pub clearing_daily_nov_hard_limit: u128,
    pub clearing_daily_nov_used: u128,
    pub clearing_daily_window_day: u64,
    pub clearing_require_healthy_risk_buffer: bool,
    pub clearing_constrained_max_slippage_bps: u32,
    pub clearing_constrained_daily_usage_bps: u32,
    pub clearing_constrained_strategy: String,
    pub source: String,
}

pub fn mapped_asset_reorg_response_policy_v1(auto_heal: bool, rollback: bool) -> &'static str {
    match (auto_heal, rollback) {
        (false, _) => "report_only",
        (true, false) => "freeze_only",
        (true, true) => "freeze_and_rollback",
    }
}

fn parse_mapped_asset_reorg_response_policy_v1(raw: &str) -> Option<&'static str> {
    match raw.trim().to_ascii_lowercase().as_str() {
        "report_only" | "dry_run_only" | "observe_only" => Some("report_only"),
        "freeze_only" | "auto_freeze" => Some("freeze_only"),
        "freeze_and_rollback" | "auto_freeze_and_rollback" => Some("freeze_and_rollback"),
        _ => None,
    }
}

fn mapped_asset_reorg_response_policy_flags_v1(policy: &str) -> Option<(bool, bool)> {
    match parse_mapped_asset_reorg_response_policy_v1(policy)? {
        "report_only" => Some((false, false)),
        "freeze_only" => Some((true, false)),
        "freeze_and_rollback" => Some((true, true)),
        _ => None,
    }
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct NovTreasurySettlementJournalEntryV1 {
    #[serde(default)]
    pub seq: u64,
    pub unix_ms: u128,
    pub kind: String,
    pub tx_hash: String,
    #[serde(default)]
    pub account_id: String,
    #[serde(default)]
    pub fee_owner_account_id: String,
    #[serde(default)]
    pub nonce_owner_account_id: String,
    #[serde(default)]
    pub key_algo: String,
    #[serde(default)]
    pub execution_policy: String,
    #[serde(default)]
    pub policy_enforced: bool,
    #[serde(default)]
    pub policy_rejection_reason: Option<String>,
    pub source_asset: String,
    pub source_amount: u128,
    pub settled_nov: u128,
    pub reserve_bucket_delta_nov: i128,
    pub fee_bucket_delta_nov: i128,
    pub risk_buffer_delta_nov: i128,
    #[serde(default)]
    pub route_ref: String,
    #[serde(default)]
    pub clearing_source: String,
    #[serde(default)]
    pub clearing_rate_ppm: u128,
    #[serde(default)]
    pub policy_version: u32,
    #[serde(default)]
    pub policy_source: String,
    #[serde(default)]
    pub policy_contract_id: String,
    #[serde(default)]
    pub policy_threshold_state: String,
    #[serde(default)]
    pub policy_constrained_strategy: String,
    #[serde(default)]
    pub policy_event_state: String,
    pub status: String,
    pub reason: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct NovCreditVaultStateV1 {
    pub vault_id: u64,
    pub owner: String,
    pub collateral_asset: String,
    pub collateral_amount: u128,
    pub debt_asset: String,
    pub debt_amount: u128,
    pub min_collateral_ratio_bps: u32,
    pub opened_at_unix_ms: u128,
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct NovTreasuryReserveProofV1 {
    pub asset: String,
    pub reserve_amount: u128,
    pub proof_type: String,
    pub proof_digest: String,
    #[serde(default)]
    pub proof_source: String,
    #[serde(default)]
    pub proof_reference: String,
    #[serde(default)]
    pub observed_at_unix_ms: u128,
    #[serde(default)]
    pub expires_at_unix_ms: u128,
    #[serde(default)]
    pub policy_version: u32,
    #[serde(default)]
    pub policy_source: String,
    #[serde(default)]
    pub status: String,
    #[serde(default)]
    pub automated_verification: bool,
    #[serde(default)]
    pub verification_mode: String,
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct NovNativeExecutionModuleStateV1 {
    #[serde(default)]
    pub treasury_reserves: BTreeMap<String, u128>,
    #[serde(default)]
    pub treasury_reserve_proofs: BTreeMap<String, NovTreasuryReserveProofV1>,
    #[serde(default)]
    pub account_asset_balances: BTreeMap<String, BTreeMap<String, u128>>,
    #[serde(default)]
    pub governance_proposals: BTreeMap<u64, serde_json::Value>,
    #[serde(default)]
    pub next_governance_proposal_id: u64,
    #[serde(default)]
    pub treasury_settled_nov_total: u128,
    #[serde(default)]
    pub treasury_settlements: u64,
    #[serde(default)]
    pub treasury_settled_by_asset: BTreeMap<String, u128>,
    #[serde(default)]
    pub treasury_redeemed_nov_total: u128,
    #[serde(default)]
    pub treasury_redeemed_by_asset: BTreeMap<String, u128>,
    #[serde(default)]
    pub treasury_reserve_bucket_nov: u128,
    #[serde(default)]
    pub treasury_fee_bucket_nov: u128,
    #[serde(default)]
    pub treasury_risk_buffer_nov: u128,
    #[serde(default)]
    pub treasury_settlement_failure_counts: BTreeMap<String, u64>,
    #[serde(default)]
    pub treasury_settlement_paused: bool,
    #[serde(default)]
    pub treasury_redeem_paused: bool,
    #[serde(default)]
    pub mapped_lock_bridge_paused: bool,
    #[serde(default)]
    pub mapped_lock_min_confirmations: u64,
    #[serde(default)]
    pub mapped_lock_contract_address: String,
    #[serde(default)]
    pub mapped_asset_burn_paused: bool,
    #[serde(default)]
    pub mapped_asset_release_paused: bool,
    #[serde(default)]
    pub mapped_asset_auto_heal_enabled: bool,
    #[serde(default)]
    pub mapped_asset_auto_heal_rollback_enabled: bool,
    #[serde(default)]
    pub mapped_header_source_required: bool,
    #[serde(default)]
    pub mapped_header_source_allowed_peer_ids: Vec<u64>,
    #[serde(default)]
    pub mapped_header_source_disabled_peer_ids: Vec<u64>,
    #[serde(default)]
    pub mapped_header_source_disabled_peer_reasons: BTreeMap<u64, String>,
    #[serde(default)]
    pub mapped_header_source_peer_rotations: BTreeMap<u64, u64>,
    #[serde(default)]
    pub mapped_header_source_min_quorum: u32,
    #[serde(default)]
    pub mapped_header_source_policy_source: String,
    #[serde(default)]
    pub mapped_header_source_policy_version: u32,
    #[serde(default)]
    pub mapped_header_source_policy_updated_unix_ms: u128,
    #[serde(default)]
    pub mapped_header_attestation_required: bool,
    #[serde(default)]
    pub mapped_header_attestation_allowed_signers: Vec<String>,
    #[serde(default)]
    pub mapped_header_attestation_disabled_signers: Vec<String>,
    #[serde(default)]
    pub mapped_header_attestation_disabled_signer_reasons: BTreeMap<String, String>,
    #[serde(default)]
    pub mapped_header_attestation_signer_rotations: BTreeMap<String, String>,
    #[serde(default)]
    pub mapped_header_attestation_min_quorum: u32,
    #[serde(default)]
    pub mapped_header_attestation_policy_source: String,
    #[serde(default)]
    pub mapped_header_attestation_policy_version: u32,
    #[serde(default)]
    pub mapped_header_attestation_policy_updated_unix_ms: u128,
    #[serde(default)]
    pub treasury_reserve_share_bps: u32,
    #[serde(default)]
    pub treasury_fee_share_bps: u32,
    #[serde(default)]
    pub treasury_risk_buffer_share_bps: u32,
    #[serde(default)]
    pub treasury_min_reserve_bucket_nov: u128,
    #[serde(default)]
    pub treasury_min_fee_bucket_nov: u128,
    #[serde(default)]
    pub treasury_min_risk_buffer_nov: u128,
    #[serde(default)]
    pub treasury_settlement_journal: Vec<NovTreasurySettlementJournalEntryV1>,
    #[serde(default)]
    pub treasury_settlement_journal_next_seq: u64,
    #[serde(default)]
    pub treasury_policy_version: u32,
    #[serde(default)]
    pub treasury_policy_source: String,
    #[serde(default)]
    pub treasury_policy_last_update_unix_ms: u128,
    #[serde(default)]
    pub clearing_nov_liquidity: BTreeMap<String, u128>,
    #[serde(default)]
    pub clearing_rate_ppm: BTreeMap<String, u128>,
    #[serde(default)]
    pub protocol_clearing_prices: BTreeMap<String, NovProtocolClearingPriceV1>,
    #[serde(default)]
    pub protocol_clearing_amm_twap_rate_ppm: BTreeMap<String, u128>,
    #[serde(default)]
    pub protocol_clearing_nav_rate_ppm: BTreeMap<String, u128>,
    #[serde(default = "default_true_v1")]
    pub clearing_enabled: bool,
    #[serde(default)]
    pub clearing_require_healthy_risk_buffer: bool,
    #[serde(default)]
    pub clearing_constrained_max_slippage_bps: u32,
    #[serde(default)]
    pub clearing_constrained_daily_usage_bps: u32,
    #[serde(default)]
    pub clearing_constrained_strategy: String,
    #[serde(default)]
    pub clearing_daily_nov_hard_limit: u128,
    #[serde(default)]
    pub clearing_daily_window_day: u64,
    #[serde(default)]
    pub clearing_daily_nov_used: u128,
    #[serde(default)]
    pub clearing_failure_counts: BTreeMap<String, u64>,
    #[serde(default)]
    pub last_clearing_failure_code: String,
    #[serde(default)]
    pub last_clearing_failure_reason: String,
    #[serde(default)]
    pub last_clearing_failure_unix_ms: u128,
    #[serde(default)]
    pub clearing_static_amm_pools: BTreeMap<String, NovStaticAmmPoolStateV1>,
    #[serde(default)]
    pub last_clearing_route: Option<NovLastClearingRouteV1>,
    #[serde(default)]
    pub last_clearing_candidates: Vec<NovClearingRouteQuoteV1>,
    #[serde(default)]
    pub fee_quote_failure_counts: BTreeMap<String, u64>,
    #[serde(default)]
    pub fee_oracle_rates_ppm: BTreeMap<String, u128>,
    #[serde(default)]
    pub fee_oracle_updated_unix_ms: u128,
    #[serde(default)]
    pub fee_oracle_source: String,
    #[serde(default)]
    pub fee_oracle_allowed_sources: Vec<String>,
    #[serde(default)]
    pub fee_oracle_disabled_sources: Vec<String>,
    #[serde(default)]
    pub fee_oracle_disabled_source_reasons: BTreeMap<String, String>,
    #[serde(default)]
    pub fee_oracle_source_rotations: BTreeMap<String, String>,
    #[serde(default)]
    pub last_fee_quote: Option<NovFeeQuoteV1>,
    #[serde(default)]
    pub last_fee_quote_failure: Option<String>,
    #[serde(default)]
    pub last_execution_trace: Option<NovExecutionTraceV1>,
    #[serde(default)]
    pub execution_traces_by_tx: BTreeMap<String, NovExecutionTraceV1>,
    #[serde(default)]
    pub execution_trace_order: Vec<String>,
    #[serde(default)]
    pub aoem_semantic_ledger_sequence: u64,
    #[serde(default)]
    pub aoem_semantic_ledger_head: String,
    #[serde(default)]
    pub unified_account_semantic_event_count: u64,
    #[serde(default)]
    pub unified_account_semantic_head: String,
    #[serde(default)]
    pub unified_account_semantic_last_digest: String,
    #[serde(default)]
    pub unified_account_semantic_last_subject: String,
    #[serde(default)]
    pub unified_account_semantic_last_action: String,
    #[serde(default)]
    pub credit_vaults: BTreeMap<u64, NovCreditVaultStateV1>,
    #[serde(default)]
    pub next_credit_vault_id: u64,
}

impl Default for NovNativeExecutionModuleStateV1 {
    fn default() -> Self {
        Self {
            treasury_reserves: BTreeMap::new(),
            treasury_reserve_proofs: BTreeMap::new(),
            account_asset_balances: BTreeMap::new(),
            governance_proposals: BTreeMap::new(),
            next_governance_proposal_id: 0,
            treasury_settled_nov_total: 0,
            treasury_settlements: 0,
            treasury_settled_by_asset: BTreeMap::new(),
            treasury_redeemed_nov_total: 0,
            treasury_redeemed_by_asset: BTreeMap::new(),
            treasury_reserve_bucket_nov: 0,
            treasury_fee_bucket_nov: 0,
            treasury_risk_buffer_nov: 0,
            treasury_settlement_failure_counts: BTreeMap::new(),
            treasury_settlement_paused: false,
            treasury_redeem_paused: false,
            mapped_lock_bridge_paused: false,
            mapped_lock_min_confirmations: 0,
            mapped_lock_contract_address: String::new(),
            mapped_asset_burn_paused: false,
            mapped_asset_release_paused: false,
            mapped_asset_auto_heal_enabled: false,
            mapped_asset_auto_heal_rollback_enabled: false,
            mapped_header_source_required: false,
            mapped_header_source_allowed_peer_ids: Vec::new(),
            mapped_header_source_disabled_peer_ids: Vec::new(),
            mapped_header_source_disabled_peer_reasons: BTreeMap::new(),
            mapped_header_source_peer_rotations: BTreeMap::new(),
            mapped_header_source_min_quorum: 1,
            mapped_header_source_policy_source: "config_path".to_string(),
            mapped_header_source_policy_version: 1,
            mapped_header_source_policy_updated_unix_ms: 0,
            mapped_header_attestation_required: false,
            mapped_header_attestation_allowed_signers: Vec::new(),
            mapped_header_attestation_disabled_signers: Vec::new(),
            mapped_header_attestation_disabled_signer_reasons: BTreeMap::new(),
            mapped_header_attestation_signer_rotations: BTreeMap::new(),
            mapped_header_attestation_min_quorum: 1,
            mapped_header_attestation_policy_source: "config_path".to_string(),
            mapped_header_attestation_policy_version: 1,
            mapped_header_attestation_policy_updated_unix_ms: 0,
            treasury_reserve_share_bps: 0,
            treasury_fee_share_bps: 0,
            treasury_risk_buffer_share_bps: 0,
            treasury_min_reserve_bucket_nov: 0,
            treasury_min_fee_bucket_nov: 0,
            treasury_min_risk_buffer_nov: 0,
            treasury_settlement_journal: Vec::new(),
            treasury_settlement_journal_next_seq: 0,
            treasury_policy_version: NOV_TREASURY_POLICY_VERSION_DEFAULT_V1,
            treasury_policy_source: "config_path".to_string(),
            treasury_policy_last_update_unix_ms: 0,
            clearing_nov_liquidity: BTreeMap::new(),
            clearing_rate_ppm: BTreeMap::new(),
            protocol_clearing_prices: BTreeMap::new(),
            protocol_clearing_amm_twap_rate_ppm: BTreeMap::new(),
            protocol_clearing_nav_rate_ppm: BTreeMap::new(),
            clearing_enabled: true,
            clearing_require_healthy_risk_buffer: false,
            clearing_constrained_max_slippage_bps:
                NOV_CLEARING_CONSTRAINED_MAX_SLIPPAGE_BPS_DEFAULT_V1,
            clearing_constrained_daily_usage_bps:
                NOV_CLEARING_CONSTRAINED_DAILY_USAGE_BPS_DEFAULT_V1,
            clearing_constrained_strategy: NOV_CLEARING_CONSTRAINED_STRATEGY_DAILY_VOLUME_ONLY_V1
                .to_string(),
            clearing_daily_nov_hard_limit: 0,
            clearing_daily_window_day: 0,
            clearing_daily_nov_used: 0,
            clearing_failure_counts: BTreeMap::new(),
            last_clearing_failure_code: String::new(),
            last_clearing_failure_reason: String::new(),
            last_clearing_failure_unix_ms: 0,
            clearing_static_amm_pools: BTreeMap::new(),
            last_clearing_route: None,
            last_clearing_candidates: Vec::new(),
            fee_quote_failure_counts: BTreeMap::new(),
            fee_oracle_rates_ppm: BTreeMap::new(),
            fee_oracle_updated_unix_ms: 0,
            fee_oracle_source: String::new(),
            fee_oracle_allowed_sources: Vec::new(),
            fee_oracle_disabled_sources: Vec::new(),
            fee_oracle_disabled_source_reasons: BTreeMap::new(),
            fee_oracle_source_rotations: BTreeMap::new(),
            last_fee_quote: None,
            last_fee_quote_failure: None,
            last_execution_trace: None,
            execution_traces_by_tx: BTreeMap::new(),
            execution_trace_order: Vec::new(),
            aoem_semantic_ledger_sequence: 0,
            aoem_semantic_ledger_head: String::new(),
            unified_account_semantic_event_count: 0,
            unified_account_semantic_head: String::new(),
            unified_account_semantic_last_digest: String::new(),
            unified_account_semantic_last_subject: String::new(),
            unified_account_semantic_last_action: String::new(),
            credit_vaults: BTreeMap::new(),
            next_credit_vault_id: 0,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct NovNativeExecutionStoreV1 {
    pub schema: String,
    #[serde(default)]
    pub receipts: BTreeMap<String, NovNativeExecutionReceiptV1>,
    #[serde(default)]
    pub module_state: NovNativeExecutionModuleStateV1,
    #[serde(default)]
    pub last_updated_unix_ms: u128,
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct NovNativeExecutionStoreSnapshotMetaV1 {
    pub schema: String,
    pub store_schema: String,
    pub backend_schema: String,
    pub last_updated_unix_ms: u128,
    pub receipt_count: usize,
    pub account_count: usize,
    pub account_asset_count: usize,
    pub semantic_ledger_sequence: u64,
    pub semantic_ledger_head: String,
}

impl Default for NovNativeExecutionStoreV1 {
    fn default() -> Self {
        Self {
            schema: NOV_NATIVE_EXECUTION_STORE_SCHEMA_V1.to_string(),
            receipts: BTreeMap::new(),
            module_state: NovNativeExecutionModuleStateV1::default(),
            last_updated_unix_ms: 0,
        }
    }
}

fn estimate_native_execution_store_retained_bytes_v1(store: &NovNativeExecutionStoreV1) -> u64 {
    // This is intentionally a low-cost estimate for soak diagnostics. It avoids serializing the
    // full store during the hot path and is only used to attribute likely memory growth sources.
    let receipt_count = store.receipts.len() as u64;
    let trace_count = store.module_state.execution_traces_by_tx.len() as u64;
    let trace_order_count = store.module_state.execution_trace_order.len() as u64;
    let account_asset_count = store
        .module_state
        .account_asset_balances
        .values()
        .map(BTreeMap::len)
        .sum::<usize>() as u64;
    receipt_count
        .saturating_mul(1024)
        .saturating_add(trace_count.saturating_mul(2048))
        .saturating_add(trace_order_count.saturating_mul(96))
        .saturating_add(account_asset_count.saturating_mul(128))
}

#[derive(Debug)]
pub struct ExecBatchBuffer {
    // Keep key/value payloads alive so ExecOpV2 raw pointers remain valid.
    _keys: Vec<[u8; 8]>,
    _values: Vec<[u8; 8]>,
    pub ops: Vec<ExecOpV2>,
}

impl ExecBatchBuffer {
    pub fn len(&self) -> usize {
        self.ops.len()
    }

    pub fn is_empty(&self) -> bool {
        self.ops.is_empty()
    }
}

pub type OpsWirePayload = EncodedOpsWire;

pub const LOCAL_TX_WIRE_CODEC_WRITE_U64LE_V1: &str = "local_tx_wire_v1_write_u64le_v1";
static LOCAL_TX_RECORD_CODEC_REGISTRY: OnceLock<RawIngressCodecRegistry> = OnceLock::new();

#[inline]
fn from_tx_wire_v1(wire: &LocalTxWireV1) -> TxIngressRecord {
    TxIngressRecord {
        account: wire.account,
        key: wire.key,
        value: wire.value,
        nonce: wire.nonce,
        fee: wire.fee,
        signature: wire.signature,
    }
}

pub fn encode_adapter_address(seed: u64) -> Vec<u8> {
    let mut out = vec![0u8; 20];
    out[12..20].copy_from_slice(&seed.to_be_bytes());
    out
}

fn local_tx_record_adapter_signing_seed_v1(account: u64) -> [u8; 32] {
    use sha2::{Digest, Sha256};

    let mut hasher = Sha256::new();
    hasher.update(b"novovm-mainline-local-tx-adapter-seed-v1");
    hasher.update(account.to_le_bytes());
    hasher.finalize().into()
}

pub fn tx_ingress_record_to_adapter_tx_ir(record: &TxIngressRecord, chain_id: u64) -> TxIR {
    let signing_seed = local_tx_record_adapter_signing_seed_v1(record.account);
    let mut ir = TxIR {
        hash: Vec::new(),
        from: novovm_adapter_novovm::address_from_seed_v1(signing_seed),
        account_id: None,
        fee_owner_account_id: None,
        nonce_owner_account_id: None,
        to: Some(encode_adapter_address(record.key)),
        value: record.value as u128,
        gas_limit: 21_000,
        gas_price: record.fee,
        nonce: record.nonce,
        data: Vec::new(),
        signature: Vec::new(),
        chain_id,
        tx_type: TxType::Transfer,
        execution_policy: TxExecutionPolicyV1::Standard,
        evm_access_list: Vec::new(),
        source_chain: None,
        target_chain: None,
    };
    ir.compute_hash();
    ir.signature = novovm_adapter_novovm::signature_payload_with_seed_v1(&ir, signing_seed);
    ir
}

pub fn tx_ingress_records_to_adapter_tx_irs(
    records: &[TxIngressRecord],
    chain_id: u64,
) -> Vec<TxIR> {
    records
        .iter()
        .map(|record| tx_ingress_record_to_adapter_tx_ir(record, chain_id))
        .collect()
}

fn bool_env_default_v1(name: &str, default: bool) -> bool {
    match std::env::var(name) {
        Ok(raw) => {
            let value = raw.trim();
            value == "1"
                || value.eq_ignore_ascii_case("true")
                || value.eq_ignore_ascii_case("yes")
                || value.eq_ignore_ascii_case("on")
        }
        Err(_) => default,
    }
}

fn native_aoem_semantic_ingress_enabled_v1() -> bool {
    bool_env_default_v1(NOV_NATIVE_AOEM_SEMANTIC_INGRESS_ENABLED_ENV, true)
}

fn native_aoem_semantic_ingress_required_v1() -> bool {
    bool_env_default_v1(NOV_NATIVE_AOEM_SEMANTIC_INGRESS_REQUIRED_ENV, false)
}

fn native_aoem_concurrent_execution_enabled_v1() -> bool {
    bool_env_default_v1(NOV_NATIVE_AOEM_CONCURRENT_EXECUTION_ENABLED_ENV, true)
}

fn parse_bool_token_v1(raw: &str) -> Option<bool> {
    let value = raw.trim();
    if value == "1"
        || value.eq_ignore_ascii_case("true")
        || value.eq_ignore_ascii_case("yes")
        || value.eq_ignore_ascii_case("on")
    {
        return Some(true);
    }
    if value == "0"
        || value.eq_ignore_ascii_case("false")
        || value.eq_ignore_ascii_case("no")
        || value.eq_ignore_ascii_case("off")
    {
        return Some(false);
    }
    None
}

fn bool_param_any_v1(params: &serde_json::Value, keys: &[&str]) -> Option<bool> {
    keys.iter()
        .filter_map(|key| params.get(*key))
        .find_map(|value| match value {
            serde_json::Value::Bool(value) => Some(*value),
            serde_json::Value::Number(number) => number.as_u64().map(|value| value != 0),
            serde_json::Value::String(raw) => parse_bool_token_v1(raw),
            _ => None,
        })
}

fn native_send_raw_transaction_pipeline_only_v1(params: &serde_json::Value) -> bool {
    bool_param_any_v1(
        params,
        &[
            "pipeline_only",
            "pipelineOnly",
            "pending_only",
            "pendingOnly",
        ],
    )
    .unwrap_or_else(|| {
        bool_env_default_v1(NOV_NATIVE_SEND_RAW_TRANSACTION_PIPELINE_ONLY_ENV, false)
    })
}

fn usize_env_default_v1(name: &str, default: usize) -> usize {
    std::env::var(name)
        .ok()
        .and_then(|raw| raw.trim().parse::<usize>().ok())
        .filter(|value| *value > 0)
        .unwrap_or(default)
}

fn native_aoem_batch_max_size_v1() -> usize {
    usize_env_default_v1(
        NOV_NATIVE_AOEM_BATCH_MAX_SIZE_ENV,
        NOV_NATIVE_AOEM_BATCH_MAX_SIZE_DEFAULT_V1,
    )
}

const fn default_true_v1() -> bool {
    true
}

fn normalize_hex_token_v1(raw: &str) -> Option<String> {
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return None;
    }
    let token = trimmed
        .strip_prefix("0x")
        .or_else(|| trimmed.strip_prefix("0X"))
        .unwrap_or(trimmed)
        .to_ascii_lowercase();
    if token.is_empty() || !token.bytes().all(|b| b.is_ascii_hexdigit()) {
        return None;
    }
    Some(token)
}

fn normalize_eth_address_policy_v1(raw: &str) -> Option<String> {
    let token = normalize_hex_token_v1(raw)?;
    if token.len() == 40 {
        Some(format!("0x{token}"))
    } else {
        None
    }
}

fn to_hex(bytes: &[u8]) -> String {
    let mut out = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        out.push_str(&format!("{byte:02x}"));
    }
    out
}

fn sha256_bytes_v1(parts: &[&[u8]]) -> [u8; 32] {
    use sha2::{Digest, Sha256};

    let mut hasher = Sha256::new();
    for part in parts {
        hasher.update(part);
    }
    hasher.finalize().into()
}

fn native_aoem_semantic_entry_v1() -> &'static str {
    "aoem.ops_wire_v1.native_asset_semantic_ingress"
}

fn native_aoem_raw_tx_batch_precommit_entry_v1() -> &'static str {
    "aoem.ops_wire_v1.native_raw_tx_batch_precommit"
}

fn native_execution_request_plan_id_v1(
    request: &NovExecutionRequestV1,
    subject_meta: &NovExecutionSubjectMetaV1,
) -> u64 {
    let target = execution_target_label_v1(&request.target);
    let digest = sha256_bytes_v1(&[
        b"novovm-native-aoem-semantic-plan-id-v1",
        request.tx_hash.as_slice(),
        target.as_bytes(),
        request.method.as_bytes(),
        subject_meta.account_id.as_bytes(),
        &request.nonce.to_le_bytes(),
    ]);
    let mut out = [0u8; 8];
    out.copy_from_slice(&digest[..8]);
    u64::from_le_bytes(out)
}

fn build_native_execution_aoem_ops_wire_v1(
    request: &NovExecutionRequestV1,
    settled_fee: &NovSettledFeeV1,
    subject_meta: &NovExecutionSubjectMetaV1,
) -> Result<(OpsWirePayload, u64)> {
    let target = execution_target_label_v1(&request.target);
    let plan_id = native_execution_request_plan_id_v1(request, subject_meta);
    let key = sha256_bytes_v1(&[
        b"novovm-native-aoem-semantic-key-v1",
        request.tx_hash.as_slice(),
        target.as_bytes(),
        request.method.as_bytes(),
        subject_meta.account_id.as_bytes(),
    ]);
    let value = sha256_bytes_v1(&[
        b"novovm-native-aoem-semantic-value-v1",
        request.args.as_slice(),
        settled_fee.source_asset.as_bytes(),
        &settled_fee.source_amount.to_le_bytes(),
        &settled_fee.nov_amount.to_le_bytes(),
        subject_meta.execution_policy.as_bytes(),
        subject_meta.key_algo.as_bytes(),
    ]);
    let mut builder = OpsWireV1Builder::new();
    builder.push(OpsWireOp {
        opcode: 2,
        flags: 0,
        reserved: 0,
        key: &key,
        value: &value,
        delta: 0,
        expect_version: None,
        plan_id,
    })?;
    Ok((builder.finish(), plan_id))
}

fn native_aoem_parallelism_meta_v1(
    op_count: usize,
    runtime: Option<&AoemRuntimeConfig>,
) -> (bool, String, usize, u32, usize, usize, String) {
    let concurrent_enabled = native_aoem_concurrent_execution_enabled_v1();
    let hint = AoemHostHint {
        txs: op_count.max(1) as u64,
        batch: op_count.min(u32::MAX as usize).max(1) as u32,
        key_space: op_count.max(1) as u64,
        rw: 1.0,
    };
    let decision = recommend_threads_auto(&hint);
    let recommended_threads = if concurrent_enabled {
        decision.recommended_threads.max(1)
    } else {
        1
    };
    let ingress_workers = runtime
        .and_then(|cfg| cfg.ingress_workers)
        .unwrap_or(recommended_threads.min(u32::MAX as usize) as u32)
        .max(1);
    (
        concurrent_enabled,
        "AOEM algebraic semantic batch ingress; deterministic ledger commit after batch precommit"
            .to_string(),
        recommended_threads,
        ingress_workers,
        decision.hw_threads,
        decision.budget_threads,
        decision.reason.to_string(),
    )
}

fn attach_native_aoem_parallelism_meta_v1(
    meta: &mut NovAoemSemanticIngressMetaV1,
    runtime: Option<&AoemRuntimeConfig>,
) {
    let (
        concurrent_enabled,
        model,
        recommended_threads,
        ingress_workers,
        hw_threads,
        budget_threads,
        reason,
    ) = native_aoem_parallelism_meta_v1(meta.op_count, runtime);
    meta.concurrent_execution_enabled = concurrent_enabled;
    meta.concurrent_execution_model = model;
    meta.batch_size = meta.op_count;
    meta.recommended_threads = recommended_threads;
    meta.ingress_workers = ingress_workers;
    meta.host_hw_threads = hw_threads;
    meta.host_budget_threads = budget_threads;
    meta.parallelism_reason = reason;
}

fn base_native_aoem_semantic_ingress_meta_v1(
    enabled: bool,
    required: bool,
    plan_id: u64,
    wire: &OpsWirePayload,
) -> NovAoemSemanticIngressMetaV1 {
    let mut meta = NovAoemSemanticIngressMetaV1 {
        execution_kernel: "AOEM".to_string(),
        semantic_entry: native_aoem_semantic_entry_v1().to_string(),
        algebraic_semantic_entry: true,
        ingress_scope: "single_request".to_string(),
        batch_plan_id: None,
        batch_item_index: None,
        batch_item_count: None,
        concurrent_execution_enabled: false,
        concurrent_execution_model: String::new(),
        batch_mode: wire.op_count > 1,
        batch_size: wire.op_count,
        recommended_threads: 1,
        ingress_workers: 1,
        host_hw_threads: 1,
        host_budget_threads: 1,
        parallelism_reason: String::new(),
        enabled,
        required,
        submitted: false,
        op_count: wire.op_count,
        plan_id,
        wire_digest: to_hex(&sha256_bytes_v1(&[
            b"novovm-native-aoem-semantic-wire-digest-v1",
            wire.bytes.as_slice(),
        ])),
        processed_ops: 0,
        success_ops: 0,
        total_writes: 0,
        semantic_delta_count: 0,
        semantic_delta_digest: String::new(),
        semantic_state_before_digest: String::new(),
        semantic_state_after_digest: String::new(),
        semantic_ledger_sequence: 0,
        semantic_ledger_prev_seal: String::new(),
        semantic_ledger_commit_seal: String::new(),
        return_code_name: String::new(),
        fallback_reason: None,
    };
    attach_native_aoem_parallelism_meta_v1(&mut meta, None);
    meta
}

fn execute_native_request_via_aoem_semantic_ingress_v1(
    request: &NovExecutionRequestV1,
    settled_fee: &NovSettledFeeV1,
    subject_meta: &NovExecutionSubjectMetaV1,
) -> Result<NovAoemSemanticIngressMetaV1> {
    let enabled = native_aoem_semantic_ingress_enabled_v1();
    let required = native_aoem_semantic_ingress_required_v1();
    let (wire, plan_id) =
        build_native_execution_aoem_ops_wire_v1(request, settled_fee, subject_meta)?;
    let mut meta = base_native_aoem_semantic_ingress_meta_v1(enabled, required, plan_id, &wire);
    if !enabled {
        meta.fallback_reason = Some("aoem_semantic_ingress_disabled".to_string());
        return Ok(meta);
    }

    let runtime = match AoemRuntimeConfig::from_env() {
        Ok(runtime) => runtime,
        Err(err) => {
            if required {
                return Err(err).context("aoem semantic ingress runtime config failed");
            }
            meta.fallback_reason = Some(format!("runtime_config_unavailable: {err}"));
            return Ok(meta);
        }
    };
    attach_native_aoem_parallelism_meta_v1(&mut meta, Some(&runtime));
    if !runtime.dll_path.exists() {
        if required {
            bail!(
                "aoem semantic ingress required but AOEM runtime DLL is missing: {}",
                runtime.dll_path.display()
            );
        }
        meta.fallback_reason = Some(format!(
            "runtime_dll_missing: {}",
            runtime.dll_path.display()
        ));
        return Ok(meta);
    }
    let facade = match AoemExecFacade::open_with_runtime(&runtime) {
        Ok(facade) => facade,
        Err(err) => {
            if required {
                return Err(err).context("open AOEM semantic ingress runtime failed");
            }
            meta.fallback_reason = Some(format!("runtime_open_failed: {err}"));
            return Ok(meta);
        }
    };
    if !facade.supports_ops_wire_v1() {
        if required {
            bail!("aoem semantic ingress required but ops_wire_v1 is unsupported");
        }
        meta.fallback_reason = Some("ops_wire_v1_unsupported".to_string());
        return Ok(meta);
    }
    let session = match facade.create_session() {
        Ok(session) => session,
        Err(err) => {
            if required {
                return Err(err).context("create AOEM semantic ingress session failed");
            }
            meta.fallback_reason = Some(format!("session_create_failed: {err}"));
            return Ok(meta);
        }
    };
    match session.submit_ops_wire(wire.bytes.as_slice()) {
        Ok(output) => {
            meta.submitted = true;
            meta.processed_ops = output.metrics.processed_ops;
            meta.success_ops = output.metrics.success_ops;
            meta.total_writes = output.metrics.total_writes;
            meta.return_code_name = output.metrics.return_code_name;
            Ok(meta)
        }
        Err(err) => {
            if required {
                return Err(err).context("submit AOEM semantic ingress ops-wire failed");
            }
            meta.fallback_reason = Some(format!("submit_failed: {err}"));
            Ok(meta)
        }
    }
}

pub fn get_nov_native_aoem_semantic_ingress_status_v1() -> serde_json::Value {
    let enabled = native_aoem_semantic_ingress_enabled_v1();
    let required = native_aoem_semantic_ingress_required_v1();
    let concurrent_enabled = native_aoem_concurrent_execution_enabled_v1();
    let max_batch_size = native_aoem_batch_max_size_v1();
    let (
        _concurrent_meta_enabled,
        concurrency_model,
        recommended_threads,
        mut ingress_workers,
        hw_threads,
        budget_threads,
        parallelism_reason,
    ) = native_aoem_parallelism_meta_v1(max_batch_size, None);
    let mut runtime_dll = serde_json::Value::Null;
    let mut runtime_config_ok = false;
    let mut runtime_dll_exists = false;
    let mut ops_wire_v1_supported = false;
    let mut runtime_error = serde_json::Value::Null;

    match AoemRuntimeConfig::from_env() {
        Ok(runtime) => {
            runtime_config_ok = true;
            runtime_dll = serde_json::Value::String(runtime.dll_path.display().to_string());
            ingress_workers = runtime.ingress_workers.unwrap_or(ingress_workers).max(1);
            runtime_dll_exists = runtime.dll_path.exists();
            if runtime_dll_exists {
                match AoemExecFacade::open_with_runtime(&runtime) {
                    Ok(facade) => {
                        ops_wire_v1_supported = facade.supports_ops_wire_v1();
                    }
                    Err(err) => {
                        runtime_error =
                            serde_json::Value::String(format!("runtime_open_failed: {err}"));
                    }
                }
            }
        }
        Err(err) => {
            runtime_error = serde_json::Value::String(format!("runtime_config_unavailable: {err}"));
        }
    }

    let ready = enabled && runtime_config_ok && runtime_dll_exists && ops_wire_v1_supported;
    serde_json::json!({
        "method": "nov_getAoemSemanticIngressStatus",
        "execution_kernel": "AOEM",
        "semantic_entry": native_aoem_semantic_entry_v1(),
        "algebraic_semantic_entry": true,
        "concurrent_execution_enabled": concurrent_enabled,
        "concurrent_execution_model": concurrency_model,
        "native_batch_entry": "nov_sendRawTransactionBatch",
        "native_execute_batch_alias": "nov_executeBatch",
        "max_batch_size": max_batch_size,
        "recommended_threads": recommended_threads,
        "ingress_workers": ingress_workers,
        "host_hw_threads": hw_threads,
        "host_budget_threads": budget_threads,
        "parallelism_reason": parallelism_reason,
        "enabled": enabled,
        "required": required,
        "fail_closed": enabled && required,
        "ready": ready,
        "runtime_config_ok": runtime_config_ok,
        "runtime_dll": runtime_dll,
        "runtime_dll_exists": runtime_dll_exists,
        "ops_wire_v1_supported": ops_wire_v1_supported,
        "runtime_error": runtime_error,
        "fallback_allowed": enabled && !required,
        "fallback_policy": if enabled && required {
            "fail_closed_on_unavailable"
        } else if enabled {
            "record_fallback_reason_and_continue"
        } else {
            "disabled"
        },
        "product_boundary": "native_asset_execution_must_enter_aoem_algebraic_semantic_ingress_for_production_required_mode",
        "storage_fallback_boundary": "json_store_lock_is_transitional_persistence_guard_not_aoem_concurrency_model",
    })
}

pub fn get_nov_native_execution_store_backend_status_v1(path: Option<&Path>) -> serde_json::Value {
    let store_path = path
        .map(Path::to_path_buf)
        .unwrap_or_else(nov_native_execution_store_path_v1);
    let backend = nov_native_execution_store_backend_v1();
    let rocksdb_path = nov_native_execution_store_rocksdb_path_v1(store_path.as_path());
    serde_json::json!({
        "method": "nov_getNativeExecutionStoreBackendStatus",
        "store_path": store_path.display().to_string(),
        "backend": backend,
        "valid_backend": matches!(
            backend.as_str(),
            NOV_NATIVE_EXECUTION_STORE_BACKEND_JSON_V1
                | NOV_NATIVE_EXECUTION_STORE_BACKEND_ROCKSDB_V1
                | NOV_NATIVE_EXECUTION_STORE_BACKEND_DUAL_V1
        ),
        "json_snapshot_enabled": native_execution_store_backend_writes_json_v1(backend.as_str()),
        "rocksdb_enabled": native_execution_store_backend_writes_rocksdb_v1(backend.as_str()),
        "rocksdb_path": rocksdb_path.display().to_string(),
        "json_snapshot_exists": store_path.exists(),
        "rocksdb_exists": rocksdb_path.exists(),
        "transactional_commit": backend == NOV_NATIVE_EXECUTION_STORE_BACKEND_ROCKSDB_V1
            || backend == NOV_NATIVE_EXECUTION_STORE_BACKEND_DUAL_V1,
        "commit_model": if backend == NOV_NATIVE_EXECUTION_STORE_BACKEND_ROCKSDB_V1 {
            "dirty_sharded_atomic_batch_primary"
        } else if backend == NOV_NATIVE_EXECUTION_STORE_BACKEND_DUAL_V1 {
            "dirty_sharded_atomic_batch_with_json_compat_snapshot"
        } else {
            "legacy_json_snapshot"
        },
        "sharded_keyspaces": [
            "account/{account_id}/asset/{asset_id}",
            "receipt/{tx_hash}",
            "receipt_by_height/{height}/{index}/{tx_hash}",
            "module_state/treasury/{key}",
            "module_state/clearing/{key}",
            "module_state/vault/{key}",
            "module_state/policy/{key}",
            "module_state/governance/{key}",
            "module_state/native_execution/{key}",
            "semantic_head/current",
            "semantic_head/by_height/{height}",
            "snapshot_meta/{height}",
        ],
        "product_boundary": "rocksdb_dirty_sharded_backend_removes_single_json_snapshot_and_module_state_core_as_primary_store_but_ordered_commit_is_still_deterministic",
    })
}

#[derive(Debug, Default)]
struct RollingDigestV1 {
    state: u64,
    count: u64,
}

impl RollingDigestV1 {
    fn update(&mut self, bytes: &[u8]) {
        const FNV_OFFSET: u64 = 0xcbf29ce484222325;
        const FNV_PRIME: u64 = 0x100000001b3;
        if self.count == 0 && self.state == 0 {
            self.state = FNV_OFFSET;
        }
        for byte in bytes {
            self.state ^= u64::from(*byte);
            self.state = self.state.wrapping_mul(FNV_PRIME);
        }
        self.count = self.count.saturating_add(1);
    }

    fn finish_hex(&self) -> String {
        format!("{:016x}:{}", self.state, self.count)
    }
}

pub fn get_nov_native_execution_store_recovery_probe_v1(path: &Path) -> Result<serde_json::Value> {
    let store = load_nov_native_execution_store_v1(path)?;
    let sequence = store.module_state.aoem_semantic_ledger_sequence;
    let head = store.module_state.aoem_semantic_ledger_head.clone();
    let rocksdb_path = nov_native_execution_store_rocksdb_path_v1(path);
    let include_full_receipt_hashes = bool_env_default_v1(
        "NOVOVM_NATIVE_EXECUTION_RECOVERY_PROBE_INCLUDE_FULL_RECEIPT_HASHES",
        false,
    );
    let receipt_hash_sample_limit = usize_env_default_v1(
        "NOVOVM_NATIVE_EXECUTION_RECOVERY_PROBE_RECEIPT_HASH_SAMPLE_LIMIT",
        8,
    );
    let mut semantic_head_current_recovered = false;
    let mut semantic_head_by_height_recovered = false;
    let mut snapshot_meta_current_recovered = false;
    let mut snapshot_meta_by_height_recovered = false;
    let mut receipt_by_height_count = 0usize;
    let mut receipt_by_height_hashes = BTreeSet::<String>::new();
    let mut receipt_by_height_digest = RollingDigestV1::default();
    let mut receipt_by_height_hash_samples = Vec::<String>::new();
    let mut rocksdb_opened = false;

    if rocksdb_path.exists() {
        let db = open_nov_native_execution_store_rocksdb_v1(rocksdb_path.as_path())?;
        rocksdb_opened = true;
        semantic_head_current_recovered = db
            .get(NOV_NATIVE_EXECUTION_STORE_ROCKSDB_KEY_SEMANTIC_HEAD_V1)
            .with_context(|| {
                format!(
                    "read nov native execution recovery semantic_head/current failed: {}",
                    rocksdb_path.display()
                )
            })?
            .as_deref()
            == Some(head.as_bytes());
        semantic_head_by_height_recovered = db
            .get(native_rocksdb_semantic_by_height_key_v1(sequence))
            .with_context(|| {
                format!(
                    "read nov native execution recovery semantic_head/by_height failed: {}",
                    rocksdb_path.display()
                )
            })?
            .as_deref()
            == Some(head.as_bytes());
        snapshot_meta_current_recovered = db
            .get(NOV_NATIVE_EXECUTION_STORE_ROCKSDB_SNAPSHOT_META_CURRENT_V1)
            .with_context(|| {
                format!(
                    "read nov native execution recovery snapshot_meta/current failed: {}",
                    rocksdb_path.display()
                )
            })?
            .is_some();
        snapshot_meta_by_height_recovered = db
            .get(native_rocksdb_snapshot_meta_by_height_key_v1(sequence))
            .with_context(|| {
                format!(
                    "read nov native execution recovery snapshot_meta/by_height failed: {}",
                    rocksdb_path.display()
                )
            })?
            .is_some();
        for item in native_rocksdb_iter_prefix_v1(
            &db,
            NOV_NATIVE_EXECUTION_STORE_ROCKSDB_RECEIPT_BY_HEIGHT_PREFIX_V1,
        ) {
            let (_key, raw) = item.with_context(|| {
                format!(
                    "iterate nov native execution recovery receipt_by_height failed: {}",
                    rocksdb_path.display()
                )
            })?;
            receipt_by_height_count = receipt_by_height_count.saturating_add(1);
            if let Ok(tx_hash) = String::from_utf8(raw.as_ref().to_vec()) {
                receipt_by_height_digest.update(tx_hash.as_bytes());
                if receipt_by_height_hash_samples.len() < receipt_hash_sample_limit {
                    receipt_by_height_hash_samples.push(tx_hash.clone());
                }
                receipt_by_height_hashes.insert(tx_hash);
            }
        }
    }

    let receipt_count = store.receipts.len();
    let materialized_account_count = store.module_state.account_asset_balances.len();
    let materialized_account_asset_count = store
        .module_state
        .account_asset_balances
        .values()
        .map(BTreeMap::len)
        .sum::<usize>();
    let mut receipt_hash_digest = RollingDigestV1::default();
    let mut receipt_hash_samples = Vec::<String>::new();
    let mut receipt_index_missing_count = 0usize;
    let mut receipt_hashes_full = if include_full_receipt_hashes {
        Some(Vec::<String>::with_capacity(receipt_count))
    } else {
        None
    };
    for tx_hash in store.receipts.keys() {
        receipt_hash_digest.update(tx_hash.as_bytes());
        if receipt_hash_samples.len() < receipt_hash_sample_limit {
            receipt_hash_samples.push(tx_hash.clone());
        }
        if !receipt_by_height_hashes.contains(tx_hash) {
            receipt_index_missing_count = receipt_index_missing_count.saturating_add(1);
        }
        if let Some(full) = receipt_hashes_full.as_mut() {
            full.push(tx_hash.clone());
        }
    }
    let receipt_index_recovered = receipt_count > 0
        && receipt_by_height_count >= receipt_count
        && receipt_index_missing_count == 0;
    let materialized_view_rebuilt = sequence > 0
        && !head.is_empty()
        && receipt_count > 0
        && store
            .receipts
            .values()
            .all(|receipt| receipt.aoem_semantic_ingress.is_some());

    Ok(serde_json::json!({
        "method": "nov_getNativeExecutionStoreRecoveryProbe",
        "store_path": path.display().to_string(),
        "rocksdb_path": rocksdb_path.display().to_string(),
        "rocksdb_exists": rocksdb_path.exists(),
        "rocksdb_opened": rocksdb_opened,
        "semantic_head_current_recovered": semantic_head_current_recovered,
        "semantic_head_by_height_recovered": semantic_head_by_height_recovered,
        "snapshot_meta_current_recovered": snapshot_meta_current_recovered,
        "snapshot_meta_by_height_recovered": snapshot_meta_by_height_recovered,
        "receipt_index_recovered": receipt_index_recovered,
        "receipt_by_height_count": receipt_by_height_count,
        "receipt_by_height_hash_digest": receipt_by_height_digest.finish_hex(),
        "receipt_by_height_hash_samples": receipt_by_height_hash_samples,
        "receipt_count": receipt_count,
        "receipt_hash_digest": receipt_hash_digest.finish_hex(),
        "receipt_hash_samples": receipt_hash_samples.clone(),
        "receipt_hashes_omitted": !include_full_receipt_hashes,
        "receipt_hashes_full_count": if include_full_receipt_hashes { receipt_count } else { 0 },
        "receipt_index_missing_count": receipt_index_missing_count,
        "recovery_probe_materialized_key_count": receipt_hash_sample_limit.min(receipt_count),
        "receipt_hashes": if let Some(full) = receipt_hashes_full {
            serde_json::json!({
                "omitted": false,
                "count": receipt_count,
                "digest": receipt_hash_digest.finish_hex(),
                "samples": receipt_hash_samples.clone(),
                "items": full,
            })
        } else {
            serde_json::json!({
                "omitted": true,
                "count": receipt_count,
                "digest": receipt_hash_digest.finish_hex(),
                "samples": receipt_hash_samples.clone(),
            })
        },
        "materialized_view_rebuilt": materialized_view_rebuilt,
        "materialized_account_count": materialized_account_count,
        "materialized_account_asset_count": materialized_account_asset_count,
        "semantic_head": {
            "sequence": sequence,
            "head": head,
        },
        "canonical_body_head_recovery": {
            "supported": false,
            "reason": "native canonical body/head projection is currently network runtime state, not yet persisted in the native execution RocksDB store",
        },
        "recovery_ok": rocksdb_opened
            && semantic_head_current_recovered
            && semantic_head_by_height_recovered
            && snapshot_meta_current_recovered
            && snapshot_meta_by_height_recovered
            && receipt_index_recovered
            && materialized_view_rebuilt,
    }))
}

fn u128_delta_i128_v1(before: u128, after: u128) -> i128 {
    if after >= before {
        saturating_u128_to_i128_v1(after.saturating_sub(before))
    } else {
        -saturating_u128_to_i128_v1(before.saturating_sub(after))
    }
}

fn push_native_semantic_delta_v1(
    deltas: &mut Vec<serde_json::Value>,
    kind: &str,
    owner: Option<&str>,
    asset: &str,
    before: u128,
    after: u128,
) {
    if before == after {
        return;
    }
    deltas.push(serde_json::json!({
        "kind": kind,
        "owner": owner,
        "asset": normalize_asset_symbol_v1(asset),
        "before": before,
        "after": after,
        "delta": u128_delta_i128_v1(before, after),
    }));
}

fn push_native_json_semantic_delta_v1(
    deltas: &mut Vec<serde_json::Value>,
    kind: &str,
    before: serde_json::Value,
    after: serde_json::Value,
) {
    if before == after {
        return;
    }
    let before_bytes = serde_json::to_vec(&before).unwrap_or_default();
    let after_bytes = serde_json::to_vec(&after).unwrap_or_default();
    deltas.push(serde_json::json!({
        "kind": kind,
        "before_digest": to_hex(&sha256_bytes_v1(&[
            b"novovm-native-aoem-semantic-json-delta-before-v1",
            before_bytes.as_slice(),
        ])),
        "after_digest": to_hex(&sha256_bytes_v1(&[
            b"novovm-native-aoem-semantic-json-delta-after-v1",
            after_bytes.as_slice(),
        ])),
    }));
}

fn native_policy_state_projection_v1(state: &NovNativeExecutionModuleStateV1) -> serde_json::Value {
    serde_json::json!({
        "governance_proposals": state.governance_proposals,
        "next_governance_proposal_id": state.next_governance_proposal_id,
        "treasury_reserve_proofs": state.treasury_reserve_proofs,
        "treasury_policy": {
            "reserve_share_bps": state.treasury_reserve_share_bps,
            "fee_share_bps": state.treasury_fee_share_bps,
            "risk_buffer_share_bps": state.treasury_risk_buffer_share_bps,
            "min_reserve_bucket_nov": state.treasury_min_reserve_bucket_nov,
            "min_fee_bucket_nov": state.treasury_min_fee_bucket_nov,
            "min_risk_buffer_nov": state.treasury_min_risk_buffer_nov,
            "version": state.treasury_policy_version,
            "source": state.treasury_policy_source,
            "last_update_unix_ms": state.treasury_policy_last_update_unix_ms,
        },
        "mapped_lock_policy": {
            "bridge_paused": state.mapped_lock_bridge_paused,
            "min_confirmations": state.mapped_lock_min_confirmations,
            "contract_address": state.mapped_lock_contract_address,
            "burn_paused": state.mapped_asset_burn_paused,
            "release_paused": state.mapped_asset_release_paused,
            "auto_heal_enabled": state.mapped_asset_auto_heal_enabled,
            "auto_heal_rollback_enabled": state.mapped_asset_auto_heal_rollback_enabled,
        },
        "mapped_header_source_policy": {
            "required": state.mapped_header_source_required,
            "allowed_peer_ids": state.mapped_header_source_allowed_peer_ids,
            "disabled_peer_ids": state.mapped_header_source_disabled_peer_ids,
            "disabled_peer_reasons": state.mapped_header_source_disabled_peer_reasons,
            "peer_rotations": state.mapped_header_source_peer_rotations,
            "min_quorum": state.mapped_header_source_min_quorum,
            "policy_source": state.mapped_header_source_policy_source,
            "policy_version": state.mapped_header_source_policy_version,
            "updated_unix_ms": state.mapped_header_source_policy_updated_unix_ms,
        },
        "mapped_header_attestation_policy": {
            "required": state.mapped_header_attestation_required,
            "allowed_signers": state.mapped_header_attestation_allowed_signers,
            "disabled_signers": state.mapped_header_attestation_disabled_signers,
            "disabled_signer_reasons": state.mapped_header_attestation_disabled_signer_reasons,
            "signer_rotations": state.mapped_header_attestation_signer_rotations,
            "min_quorum": state.mapped_header_attestation_min_quorum,
            "policy_source": state.mapped_header_attestation_policy_source,
            "policy_version": state.mapped_header_attestation_policy_version,
            "updated_unix_ms": state.mapped_header_attestation_policy_updated_unix_ms,
        },
        "protocol_clearing_policy": {
            "clearing_enabled": state.clearing_enabled,
            "clearing_require_healthy_risk_buffer": state.clearing_require_healthy_risk_buffer,
            "clearing_constrained_max_slippage_bps": state.clearing_constrained_max_slippage_bps,
            "clearing_constrained_daily_usage_bps": state.clearing_constrained_daily_usage_bps,
            "clearing_constrained_strategy": state.clearing_constrained_strategy,
            "clearing_daily_nov_hard_limit": state.clearing_daily_nov_hard_limit,
        },
        "protocol_clearing_anchors": {
            "prices": state.protocol_clearing_prices,
            "amm_twap_rate_ppm": state.protocol_clearing_amm_twap_rate_ppm,
            "nav_rate_ppm": state.protocol_clearing_nav_rate_ppm,
            "static_amm_pools": state.clearing_static_amm_pools,
            "direct_liquidity": state.clearing_nov_liquidity,
            "legacy_rate_ppm": state.clearing_rate_ppm,
        },
        "permissioned_oracle_policy": {
            "rates_ppm": state.fee_oracle_rates_ppm,
            "updated_unix_ms": state.fee_oracle_updated_unix_ms,
            "source": state.fee_oracle_source,
            "allowed_sources": state.fee_oracle_allowed_sources,
            "disabled_sources": state.fee_oracle_disabled_sources,
            "disabled_source_reasons": state.fee_oracle_disabled_source_reasons,
            "source_rotations": state.fee_oracle_source_rotations,
        },
        "unified_account_semantic_projection": {
            "event_count": state.unified_account_semantic_event_count,
            "head": state.unified_account_semantic_head,
            "last_digest": state.unified_account_semantic_last_digest,
            "last_subject": state.unified_account_semantic_last_subject,
            "last_action": state.unified_account_semantic_last_action,
        },
    })
}

fn build_native_execution_semantic_deltas_v1(
    before: &NovNativeExecutionModuleStateV1,
    after: &NovNativeExecutionModuleStateV1,
) -> Vec<serde_json::Value> {
    let mut deltas = Vec::new();
    for account in before
        .account_asset_balances
        .keys()
        .chain(after.account_asset_balances.keys())
    {
        let account_before = before.account_asset_balances.get(account.as_str());
        let account_after = after.account_asset_balances.get(account.as_str());
        let mut assets = account_before
            .into_iter()
            .flat_map(|items| items.keys())
            .chain(account_after.into_iter().flat_map(|items| items.keys()))
            .cloned()
            .collect::<Vec<_>>();
        assets.sort();
        assets.dedup();
        for asset in assets {
            let balance_before = account_before
                .and_then(|items| items.get(asset.as_str()).copied())
                .unwrap_or(0);
            let balance_after = account_after
                .and_then(|items| items.get(asset.as_str()).copied())
                .unwrap_or(0);
            push_native_semantic_delta_v1(
                &mut deltas,
                "account_asset_balance",
                Some(account.as_str()),
                asset.as_str(),
                balance_before,
                balance_after,
            );
        }
    }

    let mut reserve_assets = before
        .treasury_reserves
        .keys()
        .chain(after.treasury_reserves.keys())
        .cloned()
        .collect::<Vec<_>>();
    reserve_assets.sort();
    reserve_assets.dedup();
    for asset in reserve_assets {
        push_native_semantic_delta_v1(
            &mut deltas,
            "treasury_reserve",
            None,
            asset.as_str(),
            before
                .treasury_reserves
                .get(asset.as_str())
                .copied()
                .unwrap_or(0),
            after
                .treasury_reserves
                .get(asset.as_str())
                .copied()
                .unwrap_or(0),
        );
    }

    push_native_semantic_delta_v1(
        &mut deltas,
        "treasury_bucket",
        None,
        "NOV:reserve_bucket",
        before.treasury_reserve_bucket_nov,
        after.treasury_reserve_bucket_nov,
    );
    push_native_semantic_delta_v1(
        &mut deltas,
        "treasury_bucket",
        None,
        "NOV:fee_bucket",
        before.treasury_fee_bucket_nov,
        after.treasury_fee_bucket_nov,
    );
    push_native_semantic_delta_v1(
        &mut deltas,
        "treasury_bucket",
        None,
        "NOV:risk_buffer",
        before.treasury_risk_buffer_nov,
        after.treasury_risk_buffer_nov,
    );

    push_native_json_semantic_delta_v1(
        &mut deltas,
        "native_policy_state",
        native_policy_state_projection_v1(before),
        native_policy_state_projection_v1(after),
    );

    deltas
}

fn native_semantic_delta_digest_v1(deltas: &[serde_json::Value]) -> String {
    if deltas.is_empty() {
        return String::new();
    }
    let bytes = serde_json::to_vec(deltas).unwrap_or_default();
    to_hex(&sha256_bytes_v1(&[
        b"novovm-native-aoem-semantic-delta-digest-v1",
        bytes.as_slice(),
    ]))
}

fn native_semantic_ledger_state_digest_v1(state: &NovNativeExecutionModuleStateV1) -> String {
    let projection = serde_json::json!({
        "account_asset_balances": state.account_asset_balances,
        "treasury_reserves": state.treasury_reserves,
        "treasury_settled_nov_total": state.treasury_settled_nov_total,
        "treasury_settlements": state.treasury_settlements,
        "treasury_settled_by_asset": state.treasury_settled_by_asset,
        "treasury_redeemed_nov_total": state.treasury_redeemed_nov_total,
        "treasury_redeemed_by_asset": state.treasury_redeemed_by_asset,
        "treasury_reserve_bucket_nov": state.treasury_reserve_bucket_nov,
        "treasury_fee_bucket_nov": state.treasury_fee_bucket_nov,
        "treasury_risk_buffer_nov": state.treasury_risk_buffer_nov,
        "credit_vaults": state.credit_vaults,
        "next_credit_vault_id": state.next_credit_vault_id,
        "native_policy_state": native_policy_state_projection_v1(state),
    });
    let bytes = serde_json::to_vec(&projection).unwrap_or_default();
    to_hex(&sha256_bytes_v1(&[
        b"novovm-native-aoem-semantic-ledger-state-digest-v1",
        bytes.as_slice(),
    ]))
}

fn attach_native_semantic_ledger_commit_to_receipt_v1(
    receipt: &mut NovNativeExecutionReceiptV1,
    before: &NovNativeExecutionModuleStateV1,
    after: &NovNativeExecutionModuleStateV1,
    prev_sequence: u64,
    prev_seal: &str,
) -> Option<(u64, String)> {
    let meta = receipt.aoem_semantic_ingress.as_mut()?;
    if meta.semantic_delta_digest.trim().is_empty() {
        return None;
    }
    let next_sequence = prev_sequence.saturating_add(1);
    let state_before_digest = native_semantic_ledger_state_digest_v1(before);
    let state_after_digest = native_semantic_ledger_state_digest_v1(after);
    let next_seal = to_hex(&sha256_bytes_v1(&[
        b"novovm-native-aoem-semantic-ledger-commit-seal-v1",
        receipt.tx_hash.as_bytes(),
        meta.semantic_entry.as_bytes(),
        &meta.plan_id.to_le_bytes(),
        meta.wire_digest.as_bytes(),
        meta.semantic_delta_digest.as_bytes(),
        state_before_digest.as_bytes(),
        state_after_digest.as_bytes(),
        prev_seal.as_bytes(),
        &next_sequence.to_le_bytes(),
    ]));

    meta.semantic_state_before_digest = state_before_digest.clone();
    meta.semantic_state_after_digest = state_after_digest.clone();
    meta.semantic_ledger_sequence = next_sequence;
    meta.semantic_ledger_prev_seal = prev_seal.to_string();
    meta.semantic_ledger_commit_seal = next_seal.clone();

    receipt.logs.push(NovNativeExecutionLogV1 {
        module: "aoem".to_string(),
        method: "semantic_ledger_commit".to_string(),
        event: "aoem.native_asset.semantic_ledger_commit".to_string(),
        data: serde_json::json!({
            "semantic_entry": meta.semantic_entry,
            "ledger_sequence": next_sequence,
            "prev_seal": prev_seal,
            "commit_seal": next_seal,
            "state_before_digest": state_before_digest,
            "state_after_digest": state_after_digest,
            "delta_digest": meta.semantic_delta_digest,
            "plan_id": meta.plan_id,
            "wire_digest": meta.wire_digest,
        }),
    });
    Some((next_sequence, next_seal))
}

fn build_native_aoem_semantic_ledger_mirror_record_v1(
    receipt: &NovNativeExecutionReceiptV1,
    now_ms: u128,
) -> Option<NovAoemSemanticLedgerMirrorRecordV1> {
    let meta = receipt.aoem_semantic_ingress.as_ref()?;
    if meta.semantic_ledger_commit_seal.trim().is_empty() {
        return None;
    }
    Some(NovAoemSemanticLedgerMirrorRecordV1 {
        schema: "novovm-native-aoem-semantic-ledger-mirror/v1".to_string(),
        execution_kernel: meta.execution_kernel.clone(),
        semantic_entry: meta.semantic_entry.clone(),
        algebraic_semantic_entry: meta.algebraic_semantic_entry,
        sequence: meta.semantic_ledger_sequence,
        tx_hash: receipt.tx_hash.clone(),
        plan_id: meta.plan_id,
        wire_digest: meta.wire_digest.clone(),
        delta_digest: meta.semantic_delta_digest.clone(),
        state_before_digest: meta.semantic_state_before_digest.clone(),
        state_after_digest: meta.semantic_state_after_digest.clone(),
        prev_seal: meta.semantic_ledger_prev_seal.clone(),
        commit_seal: meta.semantic_ledger_commit_seal.clone(),
        mirror_backend: "jsonl_append_only".to_string(),
        source: "novovm-node.native_execution.aoem_semantic_ingress".to_string(),
        created_at_ms: now_ms,
    })
}

fn build_native_receipt_aoem_semantic_commit_v1(
    receipt: &NovNativeExecutionReceiptV1,
) -> Option<NovAoemSemanticMutationCommitV1> {
    let meta = receipt.aoem_semantic_ingress.as_ref()?;
    if meta.semantic_ledger_commit_seal.trim().is_empty() {
        return None;
    }
    Some(NovAoemSemanticMutationCommitV1 {
        schema: "novovm-native-aoem-semantic-mutation-commit/v1".to_string(),
        execution_kernel: meta.execution_kernel.clone(),
        semantic_entry: meta.semantic_entry.clone(),
        algebraic_semantic_entry: meta.algebraic_semantic_entry,
        enabled: meta.enabled,
        required: meta.required,
        submitted: meta.submitted,
        op_count: meta.op_count,
        plan_id: meta.plan_id,
        wire_digest: meta.wire_digest.clone(),
        processed_ops: meta.processed_ops,
        success_ops: meta.success_ops,
        total_writes: meta.total_writes,
        semantic_delta_count: meta.semantic_delta_count,
        semantic_delta_digest: meta.semantic_delta_digest.clone(),
        state_before_digest: meta.semantic_state_before_digest.clone(),
        state_after_digest: meta.semantic_state_after_digest.clone(),
        sequence: meta.semantic_ledger_sequence,
        prev_seal: meta.semantic_ledger_prev_seal.clone(),
        commit_seal: meta.semantic_ledger_commit_seal.clone(),
        source: "novovm-node.native_execution.aoem_semantic_ingress".to_string(),
        subject: receipt.module.clone(),
        action: receipt.method.clone(),
        tx_ref: receipt.tx_hash.clone(),
        mirror_backend: "jsonl_append_only".to_string(),
        fallback_reason: meta.fallback_reason.clone(),
    })
}

fn native_aoem_semantic_mutation_plan_id_v1(
    source: &str,
    tx_ref: &str,
    subject: &str,
    action: &str,
) -> u64 {
    let digest = sha256_bytes_v1(&[
        b"novovm-native-aoem-semantic-mutation-plan-id-v1",
        source.as_bytes(),
        tx_ref.as_bytes(),
        subject.as_bytes(),
        action.as_bytes(),
    ]);
    let mut out = [0u8; 8];
    out.copy_from_slice(&digest[..8]);
    u64::from_le_bytes(out)
}

fn build_native_aoem_semantic_mutation_ops_wire_v1(
    source: &str,
    tx_ref: &str,
    subject: &str,
    action: &str,
) -> Result<(OpsWirePayload, u64)> {
    let plan_id = native_aoem_semantic_mutation_plan_id_v1(source, tx_ref, subject, action);
    let key = sha256_bytes_v1(&[
        b"novovm-native-aoem-semantic-mutation-key-v1",
        source.as_bytes(),
        tx_ref.as_bytes(),
        subject.as_bytes(),
        action.as_bytes(),
    ]);
    let value = sha256_bytes_v1(&[
        b"novovm-native-aoem-semantic-mutation-value-v1",
        native_aoem_semantic_entry_v1().as_bytes(),
        source.as_bytes(),
        subject.as_bytes(),
        action.as_bytes(),
    ]);
    let mut builder = OpsWireV1Builder::new();
    builder.push(OpsWireOp {
        opcode: 2,
        flags: 0,
        reserved: 0,
        key: &key,
        value: &value,
        delta: 0,
        expect_version: None,
        plan_id,
    })?;
    Ok((builder.finish(), plan_id))
}

fn execute_native_semantic_mutation_aoem_ingress_v1(
    source: &str,
    tx_ref: &str,
    subject: &str,
    action: &str,
) -> Result<NovAoemSemanticIngressMetaV1> {
    let enabled = native_aoem_semantic_ingress_enabled_v1();
    let required = native_aoem_semantic_ingress_required_v1();
    let (wire, plan_id) =
        build_native_aoem_semantic_mutation_ops_wire_v1(source, tx_ref, subject, action)?;
    let mut meta = base_native_aoem_semantic_ingress_meta_v1(enabled, required, plan_id, &wire);
    if !enabled {
        meta.fallback_reason = Some("disabled_by_env".to_string());
        return Ok(meta);
    }
    let runtime = match AoemRuntimeConfig::from_env() {
        Ok(runtime) => runtime,
        Err(err) => {
            if required {
                return Err(err).context("load AOEM semantic mutation runtime config failed");
            }
            meta.fallback_reason = Some(format!("runtime_config_unavailable: {err}"));
            return Ok(meta);
        }
    };
    attach_native_aoem_parallelism_meta_v1(&mut meta, Some(&runtime));
    let facade = match AoemExecFacade::open_with_runtime(&runtime) {
        Ok(facade) => facade,
        Err(err) => {
            if required {
                return Err(err).context("open AOEM semantic mutation runtime failed");
            }
            meta.fallback_reason = Some(format!("runtime_open_failed: {err}"));
            return Ok(meta);
        }
    };
    if !facade.supports_ops_wire_v1() {
        if required {
            bail!("aoem semantic mutation required but ops_wire_v1 is unsupported");
        }
        meta.fallback_reason = Some("ops_wire_v1_unsupported".to_string());
        return Ok(meta);
    }
    let session = match facade.create_session() {
        Ok(session) => session,
        Err(err) => {
            if required {
                return Err(err).context("create AOEM semantic mutation session failed");
            }
            meta.fallback_reason = Some(format!("session_create_failed: {err}"));
            return Ok(meta);
        }
    };
    match session.submit_ops_wire(wire.bytes.as_slice()) {
        Ok(output) => {
            meta.submitted = true;
            meta.processed_ops = output.metrics.processed_ops;
            meta.success_ops = output.metrics.success_ops;
            meta.total_writes = output.metrics.total_writes;
            meta.return_code_name = output.metrics.return_code_name;
            Ok(meta)
        }
        Err(err) => {
            if required {
                return Err(err).context("submit AOEM semantic mutation ops-wire failed");
            }
            meta.fallback_reason = Some(format!("submit_failed: {err}"));
            Ok(meta)
        }
    }
}

pub fn mutate_nov_native_execution_store_with_aoem_semantic_commit_v1<T, F>(
    path: &Path,
    source: &str,
    tx_ref: &str,
    subject: &str,
    action: &str,
    now_ms: u128,
    mutate: F,
) -> Result<(T, Option<NovAoemSemanticMutationCommitV1>)>
where
    F: FnOnce(&mut NovNativeExecutionStoreV1) -> Result<T>,
{
    let mut meta =
        execute_native_semantic_mutation_aoem_ingress_v1(source, tx_ref, subject, action)?;
    let _write_lock = acquire_nov_native_execution_store_write_lock_v1(path)?;
    let mut store = load_nov_native_execution_store_v1(path)?;
    let previous_store = store.clone();
    let before = store.module_state.clone();
    let output = mutate(&mut store)?;
    let after = store.module_state.clone();
    let deltas = build_native_execution_semantic_deltas_v1(&before, &after);
    if deltas.is_empty() {
        save_nov_native_execution_store_with_previous_v1(path, Some(&previous_store), &store)?;
        return Ok((output, None));
    }

    let delta_digest = native_semantic_delta_digest_v1(deltas.as_slice());
    let next_sequence = before.aoem_semantic_ledger_sequence.saturating_add(1);
    let prev_seal = before.aoem_semantic_ledger_head.clone();
    let state_before_digest = native_semantic_ledger_state_digest_v1(&before);
    let state_after_digest = native_semantic_ledger_state_digest_v1(&after);
    let commit_seal = to_hex(&sha256_bytes_v1(&[
        b"novovm-native-aoem-semantic-mutation-commit-seal-v1",
        source.as_bytes(),
        tx_ref.as_bytes(),
        subject.as_bytes(),
        action.as_bytes(),
        &meta.plan_id.to_le_bytes(),
        meta.wire_digest.as_bytes(),
        delta_digest.as_bytes(),
        state_before_digest.as_bytes(),
        state_after_digest.as_bytes(),
        prev_seal.as_bytes(),
        &next_sequence.to_le_bytes(),
    ]));

    meta.semantic_delta_count = deltas.len();
    meta.semantic_delta_digest = delta_digest.clone();
    meta.semantic_state_before_digest = state_before_digest.clone();
    meta.semantic_state_after_digest = state_after_digest.clone();
    meta.semantic_ledger_sequence = next_sequence;
    meta.semantic_ledger_prev_seal = prev_seal.clone();
    meta.semantic_ledger_commit_seal = commit_seal.clone();

    store.module_state.aoem_semantic_ledger_sequence = next_sequence;
    store.module_state.aoem_semantic_ledger_head = commit_seal.clone();

    let mirror_record = NovAoemSemanticLedgerMirrorRecordV1 {
        schema: "novovm-native-aoem-semantic-ledger-mirror/v1".to_string(),
        execution_kernel: meta.execution_kernel.clone(),
        semantic_entry: meta.semantic_entry.clone(),
        algebraic_semantic_entry: meta.algebraic_semantic_entry,
        sequence: next_sequence,
        tx_hash: tx_ref.to_string(),
        plan_id: meta.plan_id,
        wire_digest: meta.wire_digest.clone(),
        delta_digest: delta_digest.clone(),
        state_before_digest: state_before_digest.clone(),
        state_after_digest: state_after_digest.clone(),
        prev_seal: prev_seal.clone(),
        commit_seal: commit_seal.clone(),
        mirror_backend: "jsonl_append_only".to_string(),
        source: source.to_string(),
        created_at_ms: now_ms,
    };
    let mirror_path = nov_native_aoem_semantic_ledger_mirror_path_v1(path);
    append_nov_native_aoem_semantic_ledger_mirror_record_v1(mirror_path.as_path(), &mirror_record)?;
    save_nov_native_execution_store_with_previous_v1(path, Some(&previous_store), &store)?;

    let commit = NovAoemSemanticMutationCommitV1 {
        schema: "novovm-native-aoem-semantic-mutation-commit/v1".to_string(),
        execution_kernel: meta.execution_kernel,
        semantic_entry: meta.semantic_entry,
        algebraic_semantic_entry: meta.algebraic_semantic_entry,
        enabled: meta.enabled,
        required: meta.required,
        submitted: meta.submitted,
        op_count: meta.op_count,
        plan_id: meta.plan_id,
        wire_digest: meta.wire_digest,
        processed_ops: meta.processed_ops,
        success_ops: meta.success_ops,
        total_writes: meta.total_writes,
        semantic_delta_count: deltas.len(),
        semantic_delta_digest: delta_digest,
        state_before_digest,
        state_after_digest,
        sequence: next_sequence,
        prev_seal,
        commit_seal,
        source: source.to_string(),
        subject: subject.to_string(),
        action: action.to_string(),
        tx_ref: tx_ref.to_string(),
        mirror_backend: "jsonl_append_only".to_string(),
        fallback_reason: meta.fallback_reason,
    };
    Ok((output, Some(commit)))
}

pub fn commit_unified_account_semantic_event_v1(
    path: &Path,
    tx_ref: &str,
    subject: &str,
    action: &str,
    event_digest: &str,
    now_ms: u128,
) -> Result<Option<NovAoemSemanticMutationCommitV1>> {
    let event_digest = event_digest.trim().to_ascii_lowercase();
    if event_digest.is_empty() {
        bail!("unified account AOEM semantic event digest is required");
    }
    let ((), commit) = mutate_nov_native_execution_store_with_aoem_semantic_commit_v1(
        path,
        "unified_account_surface",
        tx_ref,
        subject,
        action,
        now_ms,
        |store| {
            let next_event_count = store
                .module_state
                .unified_account_semantic_event_count
                .saturating_add(1);
            let prev_head = store.module_state.unified_account_semantic_head.clone();
            let next_head = to_hex(&sha256_bytes_v1(&[
                b"novovm-unified-account-aoem-semantic-head-v1",
                prev_head.as_bytes(),
                tx_ref.as_bytes(),
                subject.as_bytes(),
                action.as_bytes(),
                event_digest.as_bytes(),
                &next_event_count.to_le_bytes(),
            ]));
            store.module_state.unified_account_semantic_event_count = next_event_count;
            store.module_state.unified_account_semantic_head = next_head;
            store.module_state.unified_account_semantic_last_digest = event_digest;
            store.module_state.unified_account_semantic_last_subject = subject.to_string();
            store.module_state.unified_account_semantic_last_action = action.to_string();
            store.last_updated_unix_ms = now_ms;
            Ok(())
        },
    )?;
    Ok(commit)
}

fn attach_native_semantic_deltas_to_receipt_v1(
    receipt: &mut NovNativeExecutionReceiptV1,
    deltas: Vec<serde_json::Value>,
) {
    if deltas.is_empty() {
        return;
    }
    let digest = native_semantic_delta_digest_v1(deltas.as_slice());
    if let Some(meta) = receipt.aoem_semantic_ingress.as_mut() {
        meta.semantic_delta_count = deltas.len();
        meta.semantic_delta_digest = digest.clone();
    }
    receipt.logs.push(NovNativeExecutionLogV1 {
        module: "aoem".to_string(),
        method: "semantic_ingress".to_string(),
        event: "aoem.native_asset.semantic_deltas".to_string(),
        data: serde_json::json!({
            "semantic_entry": native_aoem_semantic_entry_v1(),
            "delta_count": deltas.len(),
            "delta_digest": digest,
            "deltas": deltas,
        }),
    });
}

fn governance_allowlist_env_v1() -> Vec<String> {
    let raw = std::env::var(NOV_NATIVE_GOVERNANCE_ALLOWLIST_ENV).unwrap_or_default();
    let mut out = Vec::new();
    for token in raw.split(',') {
        if let Some(item) = normalize_hex_token_v1(token) {
            if !out.contains(&item) {
                out.push(item);
            }
        }
    }
    out
}

fn governance_authority_check_v1(
    governance: &NovGovernanceTxV1,
    params: &serde_json::Value,
) -> Result<()> {
    if !bool_env_default_v1(NOV_NATIVE_GOVERNANCE_ENABLED_ENV, false) {
        bail!(
            "native governance tx is disabled (set {}=true to enable)",
            NOV_NATIVE_GOVERNANCE_ENABLED_ENV
        );
    }
    if params
        .get("governance_authorized")
        .and_then(|value| value.as_bool())
        .unwrap_or(false)
    {
        return Ok(());
    }
    let proposer = to_hex(&governance.proposer);
    let allowlist = governance_allowlist_env_v1();
    if allowlist.is_empty() {
        bail!(
            "governance authority missing: allow {} or provide governance_authorized=true",
            NOV_NATIVE_GOVERNANCE_ALLOWLIST_ENV
        );
    }
    if !allowlist.iter().any(|item| item == &proposer) {
        bail!(
            "governance proposer not authorized: proposer=0x{} allowlist_env={}",
            proposer,
            NOV_NATIVE_GOVERNANCE_ALLOWLIST_ENV
        );
    }
    Ok(())
}

fn governance_execute_authorized_v1(
    request: &NovExecutionRequestV1,
    args: &serde_json::Value,
) -> Result<()> {
    if !bool_env_default_v1(NOV_NATIVE_GOVERNANCE_ENABLED_ENV, false) {
        bail!(
            "native governance tx is disabled (set {}=true to enable)",
            NOV_NATIVE_GOVERNANCE_ENABLED_ENV
        );
    }
    if args
        .get("governance_authorized")
        .and_then(|value| value.as_bool())
        .unwrap_or(false)
    {
        return Ok(());
    }
    let proposer = to_hex(&request.caller);
    let allowlist = governance_allowlist_env_v1();
    if allowlist.is_empty() {
        bail!(
            "governance authority missing: allow {} or provide governance_authorized=true",
            NOV_NATIVE_GOVERNANCE_ALLOWLIST_ENV
        );
    }
    if !allowlist.iter().any(|item| item == &proposer) {
        bail!(
            "governance proposer not authorized: proposer=0x{} allowlist_env={}",
            proposer,
            NOV_NATIVE_GOVERNANCE_ALLOWLIST_ENV
        );
    }
    Ok(())
}

fn pseudo_target_address_v1(target: &NovExecutionTargetV1, method: &str) -> Vec<u8> {
    use sha2::{Digest, Sha256};
    let mut hasher = Sha256::new();
    match target {
        NovExecutionTargetV1::NativeModule(name) => {
            hasher.update(b"native:");
            hasher.update(name.as_bytes());
        }
        NovExecutionTargetV1::WasmApp(app_id) => {
            hasher.update(b"wasm:");
            hasher.update(app_id.as_bytes());
        }
        NovExecutionTargetV1::Plugin(plugin_id) => {
            hasher.update(b"plugin:");
            hasher.update(plugin_id.as_bytes());
        }
    }
    hasher.update(b":");
    hasher.update(method.as_bytes());
    let digest = hasher.finalize();
    digest[..20].to_vec()
}

pub fn nov_native_tx_to_execution_request_v1(
    tx: &NovNativeTxWireV1,
) -> Result<Option<NovExecutionRequestV1>> {
    let NovTxKindV1::Execute(execute) = &tx.kind else {
        return Ok(None);
    };
    let mut ir = nov_native_tx_to_adapter_tx_ir_v1(tx)?;
    ir.compute_hash();
    let mut tx_hash = [0u8; 32];
    let hash = ir.hash.as_slice();
    if hash.len() >= 32 {
        tx_hash.copy_from_slice(&hash[..32]);
    }
    let target = match &execute.target {
        NovExecutionTargetV1::NativeModule(name) => {
            NovExecutionRequestTargetV1::NativeModule(name.clone())
        }
        NovExecutionTargetV1::WasmApp(app) => NovExecutionRequestTargetV1::WasmApp(app.clone()),
        NovExecutionTargetV1::Plugin(plugin) => NovExecutionRequestTargetV1::Plugin(plugin.clone()),
    };
    Ok(Some(NovExecutionRequestV1 {
        tx_hash,
        chain_id: tx.chain_id,
        caller: execute.caller.clone(),
        target,
        method: execute.method.clone(),
        args: execute.args.clone(),
        fee_pay_asset: execute.fee_policy.pay_asset.clone(),
        fee_max_pay_amount: execute.fee_policy.max_pay_amount,
        fee_slippage_bps: execute.fee_policy.slippage_bps,
        gas_like_limit: execute.gas_like_limit,
        nonce: execute.nonce,
    }))
}

pub fn nov_native_tx_to_adapter_tx_ir_v1(tx: &NovNativeTxWireV1) -> Result<TxIR> {
    let mut ir = match &tx.kind {
        NovTxKindV1::Transfer(transfer) => TxIR {
            hash: Vec::new(),
            from: transfer.from.clone(),
            account_id: None,
            fee_owner_account_id: None,
            nonce_owner_account_id: None,
            to: Some(transfer.to.clone()),
            value: transfer.amount,
            gas_limit: 21_000,
            gas_price: 1,
            nonce: transfer.nonce,
            data: transfer.asset.as_bytes().to_vec(),
            signature: tx.signature.to_vec(),
            chain_id: tx.chain_id,
            tx_type: TxType::Transfer,
            execution_policy: TxExecutionPolicyV1::Standard,
            evm_access_list: Vec::new(),
            source_chain: None,
            target_chain: None,
        },
        NovTxKindV1::Execute(execute) => {
            let target_addr = pseudo_target_address_v1(&execute.target, &execute.method);
            TxIR {
                hash: Vec::new(),
                from: execute.caller.clone(),
                account_id: execute.account_id.clone(),
                fee_owner_account_id: execute.fee_owner_account_id.clone(),
                nonce_owner_account_id: execute.nonce_owner_account_id.clone(),
                to: Some(target_addr),
                value: 0,
                gas_limit: execute.gas_like_limit.unwrap_or(300_000),
                gas_price: 1,
                nonce: execute.nonce,
                data: execute.args.clone(),
                signature: tx.signature.to_vec(),
                chain_id: tx.chain_id,
                tx_type: TxType::ContractCall,
                execution_policy: tx_execution_policy_from_nov_v1(execute.execution_policy),
                evm_access_list: Vec::new(),
                source_chain: None,
                target_chain: None,
            }
        }
        NovTxKindV1::Governance(governance) => TxIR {
            hash: Vec::new(),
            from: governance.proposer.clone(),
            account_id: None,
            fee_owner_account_id: None,
            nonce_owner_account_id: None,
            to: None,
            value: 0,
            gas_limit: 80_000,
            gas_price: 1,
            nonce: governance.nonce,
            data: governance.payload.clone(),
            signature: tx.signature.to_vec(),
            chain_id: tx.chain_id,
            tx_type: TxType::Privacy,
            execution_policy: TxExecutionPolicyV1::Standard,
            evm_access_list: Vec::new(),
            source_chain: None,
            target_chain: None,
        },
    };
    ir.compute_hash();
    Ok(ir)
}

fn tx_hash_array_from_ir_v1(ir: &TxIR) -> [u8; 32] {
    let mut hash = [0u8; 32];
    let copy_len = ir.hash.len().min(32);
    hash[..copy_len].copy_from_slice(&ir.hash[..copy_len]);
    hash
}

pub fn ingest_local_nov_raw_tx_payload_v1(
    params: &serde_json::Value,
    payload: &[u8],
) -> Result<(NovNativeTxWireV1, TxIR, [u8; 32])> {
    if payload.is_empty() {
        bail!("nov_sendRawTransaction payload is empty");
    }
    let native_tx = decode_nov_native_tx_wire_v1(payload)
        .map_err(|err| anyhow::anyhow!("nov_sendRawTransaction payload decode failed: {err}"))?;
    if let NovTxKindV1::Governance(governance) = &native_tx.kind {
        governance_authority_check_v1(governance, params)?;
    }
    let ir = nov_native_tx_to_adapter_tx_ir_v1(&native_tx)?;
    let tx_hash = tx_hash_array_from_ir_v1(&ir);
    observe_network_runtime_native_pending_tx_local_native_payload_v1(
        native_tx.chain_id,
        tx_hash,
        Some(payload),
    );
    Ok((native_tx, ir, tx_hash))
}

pub fn ingest_local_eth_raw_tx_payload_v1(chain_id: u64, payload: &[u8]) -> Result<[u8; 32]> {
    if payload.is_empty() {
        bail!("eth_sendRawTransaction payload is empty");
    }
    let tx_hash = eth_rlpx_transaction_hash_v1(payload);
    if !eth_rlpx_validate_transaction_envelope_payload_v1(payload) {
        observe_network_runtime_native_pending_tx_rejected_v1(chain_id, tx_hash, None);
        bail!("eth_sendRawTransaction payload is not a valid ethereum tx envelope");
    }
    observe_network_runtime_native_pending_tx_local_ingress_with_payload_v1(
        chain_id,
        tx_hash,
        Some(payload),
    );
    Ok(tx_hash)
}

fn to_hex_prefixed_v1(bytes: &[u8]) -> String {
    let mut out = String::with_capacity(bytes.len() * 2 + 2);
    out.push_str("0x");
    for byte in bytes {
        out.push_str(&format!("{byte:02x}"));
    }
    out
}

fn now_unix_millis_v1() -> u128 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_else(|_| std::time::Duration::from_millis(0))
        .as_millis()
}

fn normalize_asset_symbol_v1(raw: &str) -> String {
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        "NOV".to_string()
    } else {
        trimmed.to_ascii_uppercase()
    }
}

fn normalize_account_ref_v1(raw: &str) -> Option<String> {
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return None;
    }
    if let Some(token) = normalize_hex_token_v1(trimmed) {
        return Some(format!("0x{}", token));
    }
    Some(trimmed.to_ascii_lowercase())
}

fn normalize_subject_account_ref_v1(raw: &str) -> Result<String> {
    normalize_account_ref_v1(raw).ok_or_else(|| anyhow::anyhow!("invalid account reference"))
}

fn caller_account_ref_v1(request: &NovExecutionRequestV1) -> String {
    to_hex_prefixed_v1(request.caller.as_slice()).to_ascii_lowercase()
}

fn fallback_execution_subject_meta_v1(
    request: &NovExecutionRequestV1,
) -> NovExecutionSubjectMetaV1 {
    let account_id = caller_account_ref_v1(request);
    NovExecutionSubjectMetaV1 {
        account_id: account_id.clone(),
        fee_owner_account_id: account_id.clone(),
        nonce_owner_account_id: account_id,
        key_algo: String::new(),
        execution_policy: NovExecutionPolicyV1::Standard.as_str().to_string(),
        policy_enforced: false,
        policy_rejection_reason: None,
    }
}

fn subject_meta_from_execute_tx_v1(execute: &NovExecuteTxV1) -> NovExecutionSubjectMetaV1 {
    let caller_fallback =
        normalize_account_ref_v1(to_hex_prefixed_v1(execute.caller.as_slice()).as_str())
            .unwrap_or_else(|| to_hex_prefixed_v1(execute.caller.as_slice()).to_ascii_lowercase());
    let account_id = execute
        .account_id
        .as_deref()
        .and_then(normalize_account_ref_v1)
        .unwrap_or_else(|| caller_fallback.clone());
    let fee_owner_account_id = execute
        .fee_owner_account_id
        .as_deref()
        .and_then(normalize_account_ref_v1)
        .unwrap_or_else(|| account_id.clone());
    let nonce_owner_account_id = execute
        .nonce_owner_account_id
        .as_deref()
        .and_then(normalize_account_ref_v1)
        .unwrap_or_else(|| account_id.clone());
    NovExecutionSubjectMetaV1 {
        account_id,
        fee_owner_account_id,
        nonce_owner_account_id,
        key_algo: String::new(),
        execution_policy: execute.execution_policy.as_str().to_string(),
        policy_enforced: false,
        policy_rejection_reason: None,
    }
}

fn subject_meta_with_execution_policy_v1(
    mut subject_meta: NovExecutionSubjectMetaV1,
    key_algo: Option<UcaKeyAlgo>,
    execution_policy: NovExecutionPolicyV1,
    policy_enforced: bool,
    policy_rejection_reason: Option<String>,
) -> NovExecutionSubjectMetaV1 {
    subject_meta.key_algo = key_algo
        .map(|value| value.as_str().to_string())
        .unwrap_or_default();
    subject_meta.execution_policy = execution_policy.as_str().to_string();
    subject_meta.policy_enforced = policy_enforced;
    subject_meta.policy_rejection_reason = policy_rejection_reason;
    subject_meta
}

fn apply_subject_meta_to_receipt_v1(
    mut receipt: NovNativeExecutionReceiptV1,
    subject_meta: &NovExecutionSubjectMetaV1,
) -> NovNativeExecutionReceiptV1 {
    receipt.account_id = subject_meta.account_id.clone();
    receipt.fee_owner_account_id = subject_meta.fee_owner_account_id.clone();
    receipt.nonce_owner_account_id = subject_meta.nonce_owner_account_id.clone();
    receipt.key_algo = subject_meta.key_algo.clone();
    receipt.execution_policy = subject_meta.execution_policy.clone();
    receipt.policy_enforced = subject_meta.policy_enforced;
    receipt.policy_rejection_reason = subject_meta.policy_rejection_reason.clone();
    receipt
}

fn native_account_asset_balance_v1(
    store: &NovNativeExecutionStoreV1,
    account: &str,
    asset: &str,
) -> u128 {
    let account_key = match normalize_account_ref_v1(account) {
        Some(value) => value,
        None => return 0,
    };
    let asset_key = normalize_asset_symbol_v1(asset);
    store
        .module_state
        .account_asset_balances
        .get(account_key.as_str())
        .and_then(|assets| assets.get(asset_key.as_str()).copied())
        .unwrap_or(0)
}

fn credit_native_account_asset_balance_v1(
    store: &mut NovNativeExecutionStoreV1,
    account: &str,
    asset: &str,
    amount: u128,
) -> u128 {
    let account_key = normalize_account_ref_v1(account).unwrap_or_else(|| account.to_string());
    let asset_key = normalize_asset_symbol_v1(asset);
    let balances = store
        .module_state
        .account_asset_balances
        .entry(account_key)
        .or_default();
    let entry = balances.entry(asset_key).or_insert(0);
    *entry = entry.saturating_add(amount);
    *entry
}

fn debit_native_account_asset_balance_v1(
    store: &mut NovNativeExecutionStoreV1,
    account: &str,
    asset: &str,
    amount: u128,
) -> Result<u128> {
    let account_key = normalize_account_ref_v1(account)
        .ok_or_else(|| anyhow::anyhow!("invalid account reference"))?;
    let asset_key = normalize_asset_symbol_v1(asset);
    let balances = store
        .module_state
        .account_asset_balances
        .entry(account_key.clone())
        .or_default();
    let entry = balances.entry(asset_key.clone()).or_insert(0);
    if *entry < amount {
        bail!(
            "insufficient user balance: account={} asset={} requested={} available={}",
            account_key,
            asset_key,
            amount,
            *entry
        );
    }
    *entry = entry.saturating_sub(amount);
    Ok(*entry)
}

fn normalize_policy_source_v1(raw: &str) -> String {
    let normalized = raw.trim().to_ascii_lowercase();
    if normalized.is_empty() || normalized == "default" {
        "config_path".to_string()
    } else {
        normalized
    }
}

fn normalize_constrained_strategy_v1(raw: &str) -> &'static str {
    match raw.trim().to_ascii_lowercase().as_str() {
        "daily_volume_only" | "daily" => NOV_CLEARING_CONSTRAINED_STRATEGY_DAILY_VOLUME_ONLY_V1,
        "treasury_direct_only" | "treasury_direct" | "treasury" => {
            NOV_CLEARING_CONSTRAINED_STRATEGY_TREASURY_DIRECT_ONLY_V1
        }
        "blocked" => NOV_CLEARING_CONSTRAINED_STRATEGY_BLOCKED_V1,
        _ => NOV_CLEARING_CONSTRAINED_STRATEGY_DAILY_VOLUME_ONLY_V1,
    }
}

fn parse_constrained_strategy_strict_v1(raw: &str) -> Option<&'static str> {
    match raw.trim().to_ascii_lowercase().as_str() {
        "daily_volume_only" | "daily" => {
            Some(NOV_CLEARING_CONSTRAINED_STRATEGY_DAILY_VOLUME_ONLY_V1)
        }
        "treasury_direct_only" | "treasury_direct" | "treasury" => {
            Some(NOV_CLEARING_CONSTRAINED_STRATEGY_TREASURY_DIRECT_ONLY_V1)
        }
        "blocked" => Some(NOV_CLEARING_CONSTRAINED_STRATEGY_BLOCKED_V1),
        _ => None,
    }
}

fn normalize_tx_hash_hex_v1(raw: &str) -> String {
    raw.trim()
        .strip_prefix("0x")
        .or_else(|| raw.trim().strip_prefix("0X"))
        .unwrap_or(raw.trim())
        .to_ascii_lowercase()
}

fn parse_u128_from_json_value_v1(value: &serde_json::Value) -> Option<u128> {
    match value {
        serde_json::Value::Number(number) => number.as_u64().map(u128::from),
        serde_json::Value::String(raw) => {
            let trimmed = raw.trim();
            if let Some(hex) = trimmed
                .strip_prefix("0x")
                .or_else(|| trimmed.strip_prefix("0X"))
            {
                u128::from_str_radix(hex, 16).ok()
            } else {
                trimmed.parse::<u128>().ok()
            }
        }
        _ => None,
    }
}

fn parse_string_list_from_json_value_v1(value: &serde_json::Value) -> Option<Vec<String>> {
    let mut out = Vec::new();
    match value {
        serde_json::Value::Array(items) => {
            for item in items {
                let raw = item.as_str()?.trim();
                if !raw.is_empty() {
                    out.push(raw.to_string());
                }
            }
        }
        serde_json::Value::String(raw) => {
            for item in raw.split(',') {
                let trimmed = item.trim();
                if !trimmed.is_empty() {
                    out.push(trimmed.to_string());
                }
            }
        }
        _ => return None,
    }
    out.sort();
    out.dedup();
    Some(out)
}

fn parse_string_map_from_json_value_v1(
    value: &serde_json::Value,
) -> Option<BTreeMap<String, String>> {
    let mut out = BTreeMap::new();
    match value {
        serde_json::Value::Object(map) => {
            for (key, value) in map {
                let raw_value = value.as_str()?.trim();
                let raw_key = key.trim();
                if !raw_key.is_empty() && !raw_value.is_empty() {
                    out.insert(raw_key.to_string(), raw_value.to_string());
                }
            }
        }
        _ => return None,
    }
    Some(out)
}

fn decode_execute_args_json_v1(args: &[u8]) -> Option<serde_json::Value> {
    if args.is_empty() {
        return None;
    }
    serde_json::from_slice(args).ok()
}

fn fallback_execute_args_value_v1(args: &[u8]) -> serde_json::Value {
    serde_json::json!({
        "raw_args_hex": to_hex_prefixed_v1(args),
        "raw_args_len": args.len(),
    })
}

const NOV_NATIVE_MODULE_REGISTRY_V1: [(&str, &[&str]); 6] = [
    (
        "treasury",
        &[
            "deposit_reserve",
            "redeem",
            "redeem_reserve",
            "get_reserve_balance",
            "get_reserve_proof",
            "get_reserve_snapshot",
            "get_settlement_summary",
            "get_settlement_policy",
            "get_settlement_journal",
            "get_clearing_liquidity",
            "get_clearing_routes",
            "get_last_clearing_route",
            "get_last_clearing_candidates",
            "get_clearing_risk_summary",
            "get_last_execution_trace",
            "get_execution_trace_by_tx",
            "get_clearing_metrics_summary",
            "get_policy_metrics_summary",
            "get_fee_quote_summary",
            "get_fee_oracle_rates",
        ],
    ),
    ("credit_engine", &["open_vault"]),
    ("amm", &["swap_exact_in"]),
    (
        "governance",
        &[
            "submit_proposal",
            "apply_treasury_policy",
            "set_reserve_proof",
            "get_proposal",
            "list_proposals",
        ],
    ),
    ("account", &[]),
    ("asset", &[]),
];

pub fn nov_native_module_methods_v1(module: &str) -> Option<Vec<String>> {
    let normalized = module.trim().to_ascii_lowercase();
    NOV_NATIVE_MODULE_REGISTRY_V1
        .iter()
        .find(|(name, _)| *name == normalized)
        .map(|(_, methods)| methods.iter().map(|item| item.to_string()).collect())
}

pub fn nov_native_module_info_v1(module: &str) -> Option<serde_json::Value> {
    let normalized = module.trim().to_ascii_lowercase();
    let methods = nov_native_module_methods_v1(normalized.as_str())?;
    Some(serde_json::json!({
        "name": normalized,
        "version": "v1",
        "entry_kind": "native_module",
        "state": "active",
        "methods": methods,
    }))
}

pub fn nov_native_execution_store_path_v1() -> PathBuf {
    std::env::var(NOV_NATIVE_EXECUTION_STORE_ENV)
        .map(PathBuf::from)
        .unwrap_or_else(|_| PathBuf::from("artifacts").join("novovm-native-execution-store.json"))
}

pub fn nov_native_aoem_semantic_ledger_mirror_path_v1(native_store_path: &Path) -> PathBuf {
    std::env::var(NOV_NATIVE_AOEM_SEMANTIC_LEDGER_MIRROR_ENV)
        .map(PathBuf::from)
        .unwrap_or_else(|_| {
            let mut raw = native_store_path.as_os_str().to_os_string();
            raw.push(".aoem-semantic-ledger.jsonl");
            PathBuf::from(raw)
        })
}

pub fn nov_native_execution_store_rocksdb_path_v1(native_store_path: &Path) -> PathBuf {
    std::env::var(NOV_NATIVE_EXECUTION_STORE_ROCKSDB_PATH_ENV)
        .map(PathBuf::from)
        .unwrap_or_else(|_| {
            let mut raw = native_store_path.as_os_str().to_os_string();
            raw.push(".rocksdb");
            PathBuf::from(raw)
        })
}

fn nov_native_execution_store_backend_v1() -> String {
    std::env::var(NOV_NATIVE_EXECUTION_STORE_BACKEND_ENV)
        .unwrap_or_else(|_| NOV_NATIVE_EXECUTION_STORE_BACKEND_JSON_V1.to_string())
        .trim()
        .to_ascii_lowercase()
}

fn native_execution_store_backend_reads_rocksdb_v1(backend: &str) -> bool {
    matches!(
        backend,
        NOV_NATIVE_EXECUTION_STORE_BACKEND_ROCKSDB_V1 | NOV_NATIVE_EXECUTION_STORE_BACKEND_DUAL_V1
    )
}

fn native_execution_store_backend_writes_json_v1(backend: &str) -> bool {
    matches!(
        backend,
        NOV_NATIVE_EXECUTION_STORE_BACKEND_JSON_V1 | NOV_NATIVE_EXECUTION_STORE_BACKEND_DUAL_V1
    )
}

fn native_execution_store_backend_writes_rocksdb_v1(backend: &str) -> bool {
    native_execution_store_backend_reads_rocksdb_v1(backend)
}

fn open_nov_native_execution_store_rocksdb_v1(path: &Path) -> Result<RocksDb> {
    if let Some(parent) = path.parent() {
        if !parent.as_os_str().is_empty() {
            fs::create_dir_all(parent).with_context(|| {
                format!(
                    "create nov native execution rocksdb parent dir failed: {}",
                    parent.display()
                )
            })?;
        }
    }
    let mut opts = RocksDbOptions::default();
    opts.create_if_missing(true);
    RocksDb::open(&opts, path).with_context(|| {
        format!(
            "open nov native execution rocksdb failed: {}",
            path.display()
        )
    })
}

fn rocksdb_property_u64_v1(db: &RocksDb, property: &str) -> Option<u64> {
    db.property_int_value(property).ok().flatten()
}

pub fn get_nov_native_execution_store_rocksdb_memory_probe_v1(path: &Path) -> serde_json::Value {
    let rocksdb_path = nov_native_execution_store_rocksdb_path_v1(path);
    if !rocksdb_path.exists() {
        return serde_json::json!({
            "method": "nov_getNativeExecutionStoreRocksDbMemoryProbe",
            "rocksdb_path": rocksdb_path.display().to_string(),
            "rocksdb_exists": false,
            "rocksdb_opened": false,
            "rocksdb_total_estimated_memory_bytes": 0u64,
            "rocksdb_memory_probe_supported": true,
        });
    }
    let db = match open_nov_native_execution_store_rocksdb_v1(rocksdb_path.as_path()) {
        Ok(db) => db,
        Err(err) => {
            return serde_json::json!({
                "method": "nov_getNativeExecutionStoreRocksDbMemoryProbe",
                "rocksdb_path": rocksdb_path.display().to_string(),
                "rocksdb_exists": true,
                "rocksdb_opened": false,
                "rocksdb_open_error": err.to_string(),
                "rocksdb_total_estimated_memory_bytes": 0u64,
                "rocksdb_memory_probe_supported": false,
            });
        }
    };
    let block_cache = rocksdb_property_u64_v1(&db, "rocksdb.block-cache-usage").unwrap_or(0);
    let block_cache_pinned =
        rocksdb_property_u64_v1(&db, "rocksdb.block-cache-pinned-usage").unwrap_or(0);
    let memtable_current =
        rocksdb_property_u64_v1(&db, "rocksdb.cur-size-all-mem-tables").unwrap_or(0);
    let memtable_total =
        rocksdb_property_u64_v1(&db, "rocksdb.size-all-mem-tables").unwrap_or(memtable_current);
    let table_readers =
        rocksdb_property_u64_v1(&db, "rocksdb.estimate-table-readers-mem").unwrap_or(0);
    let estimate_num_keys = rocksdb_property_u64_v1(&db, "rocksdb.estimate-num-keys").unwrap_or(0);
    let total = block_cache
        .saturating_add(block_cache_pinned)
        .saturating_add(memtable_total)
        .saturating_add(table_readers);
    serde_json::json!({
        "method": "nov_getNativeExecutionStoreRocksDbMemoryProbe",
        "rocksdb_path": rocksdb_path.display().to_string(),
        "rocksdb_exists": true,
        "rocksdb_opened": true,
        "rocksdb_block_cache_estimated_bytes": block_cache,
        "rocksdb_block_cache_pinned_estimated_bytes": block_cache_pinned,
        "rocksdb_memtable_estimated_bytes": memtable_total,
        "rocksdb_current_memtable_estimated_bytes": memtable_current,
        "rocksdb_index_filter_estimated_bytes": table_readers,
        "rocksdb_estimate_num_keys": estimate_num_keys,
        "rocksdb_total_estimated_memory_bytes": total,
        "rocksdb_memory_probe_supported": true,
    })
}

fn native_rocksdb_account_asset_key_v1(account_id: &str, asset: &str) -> Vec<u8> {
    format!(
        "account/{}/asset/{}",
        account_id,
        normalize_asset_symbol_v1(asset)
    )
    .into_bytes()
}

fn native_rocksdb_receipt_key_v1(tx_hash: &str) -> Vec<u8> {
    format!("receipt/{tx_hash}").into_bytes()
}

fn native_rocksdb_receipt_by_height_key_v1(height: u64, index: usize, tx_hash: &str) -> Vec<u8> {
    let mut key = NOV_NATIVE_EXECUTION_STORE_ROCKSDB_RECEIPT_BY_HEIGHT_PREFIX_V1.to_vec();
    key.extend_from_slice(format!("{height:020}/{index:020}/{tx_hash}").as_bytes());
    key
}

fn native_rocksdb_semantic_by_height_key_v1(height: u64) -> Vec<u8> {
    let mut key = NOV_NATIVE_EXECUTION_STORE_ROCKSDB_SEMANTIC_BY_HEIGHT_PREFIX_V1.to_vec();
    key.extend_from_slice(format!("{height:020}").as_bytes());
    key
}

fn native_rocksdb_snapshot_meta_by_height_key_v1(height: u64) -> Vec<u8> {
    let mut key = NOV_NATIVE_EXECUTION_STORE_ROCKSDB_SNAPSHOT_META_PREFIX_V1.to_vec();
    key.extend_from_slice(format!("{height:020}").as_bytes());
    key
}

fn native_rocksdb_key_has_prefix_v1(key: &[u8], prefix: &[u8]) -> bool {
    key.starts_with(prefix)
}

fn native_rocksdb_decode_account_asset_key_v1(key: &[u8]) -> Option<(String, String)> {
    let raw = std::str::from_utf8(key).ok()?;
    let rest = raw.strip_prefix("account/")?;
    let (account, asset) = rest.split_once("/asset/")?;
    if account.is_empty() || asset.is_empty() {
        return None;
    }
    Some((account.to_string(), normalize_asset_symbol_v1(asset)))
}

fn native_rocksdb_iter_prefix_v1(
    db: &RocksDb,
    prefix: &[u8],
) -> Vec<Result<(Box<[u8]>, Box<[u8]>), rocksdb::Error>> {
    db.iterator(RocksDbIteratorMode::From(prefix, RocksDbDirection::Forward))
        .take_while(|item| {
            item.as_ref()
                .map(|(key, _)| native_rocksdb_key_has_prefix_v1(key.as_ref(), prefix))
                .unwrap_or(true)
        })
        .collect()
}

fn native_rocksdb_snapshot_meta_v1(
    store: &NovNativeExecutionStoreV1,
) -> NovNativeExecutionStoreSnapshotMetaV1 {
    let account_asset_count = store
        .module_state
        .account_asset_balances
        .values()
        .map(BTreeMap::len)
        .sum();
    NovNativeExecutionStoreSnapshotMetaV1 {
        schema: "novovm-native-execution-store-snapshot-meta/v1".to_string(),
        store_schema: store.schema.clone(),
        backend_schema: "rocksdb_sharded_commit/v1".to_string(),
        last_updated_unix_ms: store.last_updated_unix_ms,
        receipt_count: store.receipts.len(),
        account_count: store.module_state.account_asset_balances.len(),
        account_asset_count,
        semantic_ledger_sequence: store.module_state.aoem_semantic_ledger_sequence,
        semantic_ledger_head: store.module_state.aoem_semantic_ledger_head.clone(),
    }
}

fn native_rocksdb_module_state_shard_keys_v1() -> [(&'static [u8], &'static str); 6] {
    [
        (
            NOV_NATIVE_EXECUTION_STORE_ROCKSDB_KEY_MODULE_STATE_TREASURY_V1,
            "treasury",
        ),
        (
            NOV_NATIVE_EXECUTION_STORE_ROCKSDB_KEY_MODULE_STATE_CLEARING_V1,
            "clearing",
        ),
        (
            NOV_NATIVE_EXECUTION_STORE_ROCKSDB_KEY_MODULE_STATE_VAULT_V1,
            "vault",
        ),
        (
            NOV_NATIVE_EXECUTION_STORE_ROCKSDB_KEY_MODULE_STATE_POLICY_V1,
            "policy",
        ),
        (
            NOV_NATIVE_EXECUTION_STORE_ROCKSDB_KEY_MODULE_STATE_GOVERNANCE_V1,
            "governance",
        ),
        (
            NOV_NATIVE_EXECUTION_STORE_ROCKSDB_KEY_MODULE_STATE_NATIVE_EXECUTION_V1,
            "native_execution",
        ),
    ]
}

fn native_module_state_shard_value_v1(
    module_state: &NovNativeExecutionModuleStateV1,
    shard: &str,
) -> Result<Vec<u8>> {
    let value = match shard {
        "treasury" => serde_json::json!({
            "treasury_reserves": module_state.treasury_reserves,
            "treasury_reserve_proofs": module_state.treasury_reserve_proofs,
            "treasury_settled_nov_total": module_state.treasury_settled_nov_total,
            "treasury_settlements": module_state.treasury_settlements,
            "treasury_settled_by_asset": module_state.treasury_settled_by_asset,
            "treasury_redeemed_nov_total": module_state.treasury_redeemed_nov_total,
            "treasury_redeemed_by_asset": module_state.treasury_redeemed_by_asset,
            "treasury_reserve_bucket_nov": module_state.treasury_reserve_bucket_nov,
            "treasury_fee_bucket_nov": module_state.treasury_fee_bucket_nov,
            "treasury_risk_buffer_nov": module_state.treasury_risk_buffer_nov,
            "treasury_settlement_failure_counts": module_state.treasury_settlement_failure_counts,
            "treasury_settlement_journal": module_state.treasury_settlement_journal,
            "treasury_settlement_journal_next_seq": module_state.treasury_settlement_journal_next_seq,
        }),
        "clearing" => serde_json::json!({
            "clearing_nov_liquidity": module_state.clearing_nov_liquidity,
            "clearing_rate_ppm": module_state.clearing_rate_ppm,
            "protocol_clearing_prices": module_state.protocol_clearing_prices,
            "protocol_clearing_amm_twap_rate_ppm": module_state.protocol_clearing_amm_twap_rate_ppm,
            "protocol_clearing_nav_rate_ppm": module_state.protocol_clearing_nav_rate_ppm,
            "clearing_enabled": module_state.clearing_enabled,
            "clearing_require_healthy_risk_buffer": module_state.clearing_require_healthy_risk_buffer,
            "clearing_constrained_max_slippage_bps": module_state.clearing_constrained_max_slippage_bps,
            "clearing_constrained_daily_usage_bps": module_state.clearing_constrained_daily_usage_bps,
            "clearing_constrained_strategy": module_state.clearing_constrained_strategy,
            "clearing_daily_nov_hard_limit": module_state.clearing_daily_nov_hard_limit,
            "clearing_daily_window_day": module_state.clearing_daily_window_day,
            "clearing_daily_nov_used": module_state.clearing_daily_nov_used,
            "clearing_failure_counts": module_state.clearing_failure_counts,
            "last_clearing_failure_code": module_state.last_clearing_failure_code,
            "last_clearing_failure_reason": module_state.last_clearing_failure_reason,
            "last_clearing_failure_unix_ms": module_state.last_clearing_failure_unix_ms,
            "clearing_static_amm_pools": module_state.clearing_static_amm_pools,
            "last_clearing_route": module_state.last_clearing_route,
            "last_clearing_candidates": module_state.last_clearing_candidates,
            "fee_quote_failure_counts": module_state.fee_quote_failure_counts,
            "fee_oracle_rates_ppm": module_state.fee_oracle_rates_ppm,
            "fee_oracle_updated_unix_ms": module_state.fee_oracle_updated_unix_ms,
            "fee_oracle_source": module_state.fee_oracle_source,
            "fee_oracle_allowed_sources": module_state.fee_oracle_allowed_sources,
            "fee_oracle_disabled_sources": module_state.fee_oracle_disabled_sources,
            "fee_oracle_disabled_source_reasons": module_state.fee_oracle_disabled_source_reasons,
            "fee_oracle_source_rotations": module_state.fee_oracle_source_rotations,
            "last_fee_quote": module_state.last_fee_quote,
            "last_fee_quote_failure": module_state.last_fee_quote_failure,
        }),
        "vault" => serde_json::json!({
            "credit_vaults": module_state.credit_vaults,
            "next_credit_vault_id": module_state.next_credit_vault_id,
        }),
        "policy" => serde_json::json!({
            "treasury_settlement_paused": module_state.treasury_settlement_paused,
            "treasury_redeem_paused": module_state.treasury_redeem_paused,
            "mapped_lock_bridge_paused": module_state.mapped_lock_bridge_paused,
            "mapped_lock_min_confirmations": module_state.mapped_lock_min_confirmations,
            "mapped_lock_contract_address": module_state.mapped_lock_contract_address,
            "mapped_asset_burn_paused": module_state.mapped_asset_burn_paused,
            "mapped_asset_release_paused": module_state.mapped_asset_release_paused,
            "mapped_asset_auto_heal_enabled": module_state.mapped_asset_auto_heal_enabled,
            "mapped_asset_auto_heal_rollback_enabled": module_state.mapped_asset_auto_heal_rollback_enabled,
            "mapped_header_source_required": module_state.mapped_header_source_required,
            "mapped_header_source_allowed_peer_ids": module_state.mapped_header_source_allowed_peer_ids,
            "mapped_header_source_disabled_peer_ids": module_state.mapped_header_source_disabled_peer_ids,
            "mapped_header_source_disabled_peer_reasons": module_state.mapped_header_source_disabled_peer_reasons,
            "mapped_header_source_peer_rotations": module_state.mapped_header_source_peer_rotations,
            "mapped_header_source_min_quorum": module_state.mapped_header_source_min_quorum,
            "mapped_header_source_policy_source": module_state.mapped_header_source_policy_source,
            "mapped_header_source_policy_version": module_state.mapped_header_source_policy_version,
            "mapped_header_source_policy_updated_unix_ms": module_state.mapped_header_source_policy_updated_unix_ms,
            "mapped_header_attestation_required": module_state.mapped_header_attestation_required,
            "mapped_header_attestation_allowed_signers": module_state.mapped_header_attestation_allowed_signers,
            "mapped_header_attestation_disabled_signers": module_state.mapped_header_attestation_disabled_signers,
            "mapped_header_attestation_disabled_signer_reasons": module_state.mapped_header_attestation_disabled_signer_reasons,
            "mapped_header_attestation_signer_rotations": module_state.mapped_header_attestation_signer_rotations,
            "mapped_header_attestation_min_quorum": module_state.mapped_header_attestation_min_quorum,
            "mapped_header_attestation_policy_source": module_state.mapped_header_attestation_policy_source,
            "mapped_header_attestation_policy_version": module_state.mapped_header_attestation_policy_version,
            "mapped_header_attestation_policy_updated_unix_ms": module_state.mapped_header_attestation_policy_updated_unix_ms,
            "treasury_reserve_share_bps": module_state.treasury_reserve_share_bps,
            "treasury_fee_share_bps": module_state.treasury_fee_share_bps,
            "treasury_risk_buffer_share_bps": module_state.treasury_risk_buffer_share_bps,
            "treasury_min_reserve_bucket_nov": module_state.treasury_min_reserve_bucket_nov,
            "treasury_min_fee_bucket_nov": module_state.treasury_min_fee_bucket_nov,
            "treasury_min_risk_buffer_nov": module_state.treasury_min_risk_buffer_nov,
            "treasury_policy_version": module_state.treasury_policy_version,
            "treasury_policy_source": module_state.treasury_policy_source,
            "treasury_policy_last_update_unix_ms": module_state.treasury_policy_last_update_unix_ms,
        }),
        "governance" => serde_json::json!({
            "governance_proposals": module_state.governance_proposals,
            "next_governance_proposal_id": module_state.next_governance_proposal_id,
        }),
        "native_execution" => serde_json::json!({
            "last_execution_trace": module_state.last_execution_trace,
            "execution_traces_by_tx": module_state.execution_traces_by_tx,
            "execution_trace_order": module_state.execution_trace_order,
            "aoem_semantic_ledger_sequence": module_state.aoem_semantic_ledger_sequence,
            "aoem_semantic_ledger_head": module_state.aoem_semantic_ledger_head,
            "unified_account_semantic_event_count": module_state.unified_account_semantic_event_count,
            "unified_account_semantic_head": module_state.unified_account_semantic_head,
            "unified_account_semantic_last_digest": module_state.unified_account_semantic_last_digest,
            "unified_account_semantic_last_subject": module_state.unified_account_semantic_last_subject,
            "unified_account_semantic_last_action": module_state.unified_account_semantic_last_action,
        }),
        _ => bail!("unknown nov native execution module_state shard: {shard}"),
    };
    serde_json::to_vec(&value)
        .with_context(|| format!("serialize nov native execution module_state/{shard} failed"))
}

fn native_apply_module_state_shard_v1(
    module_state: &mut NovNativeExecutionModuleStateV1,
    shard: &str,
    raw: &[u8],
) -> Result<()> {
    let value: serde_json::Value = serde_json::from_slice(raw)
        .with_context(|| format!("parse nov native execution module_state/{shard} failed"))?;
    macro_rules! assign_field {
        ($field:ident) => {
            if let Some(raw_field) = value.get(stringify!($field)) {
                module_state.$field =
                    serde_json::from_value(raw_field.clone()).with_context(|| {
                        format!(
                            "parse nov native execution module_state/{}/{} failed",
                            shard,
                            stringify!($field)
                        )
                    })?;
            }
        };
    }
    match shard {
        "treasury" => {
            assign_field!(treasury_reserves);
            assign_field!(treasury_reserve_proofs);
            assign_field!(treasury_settled_nov_total);
            assign_field!(treasury_settlements);
            assign_field!(treasury_settled_by_asset);
            assign_field!(treasury_redeemed_nov_total);
            assign_field!(treasury_redeemed_by_asset);
            assign_field!(treasury_reserve_bucket_nov);
            assign_field!(treasury_fee_bucket_nov);
            assign_field!(treasury_risk_buffer_nov);
            assign_field!(treasury_settlement_failure_counts);
            assign_field!(treasury_settlement_journal);
            assign_field!(treasury_settlement_journal_next_seq);
        }
        "clearing" => {
            assign_field!(clearing_nov_liquidity);
            assign_field!(clearing_rate_ppm);
            assign_field!(protocol_clearing_prices);
            assign_field!(protocol_clearing_amm_twap_rate_ppm);
            assign_field!(protocol_clearing_nav_rate_ppm);
            assign_field!(clearing_enabled);
            assign_field!(clearing_require_healthy_risk_buffer);
            assign_field!(clearing_constrained_max_slippage_bps);
            assign_field!(clearing_constrained_daily_usage_bps);
            assign_field!(clearing_constrained_strategy);
            assign_field!(clearing_daily_nov_hard_limit);
            assign_field!(clearing_daily_window_day);
            assign_field!(clearing_daily_nov_used);
            assign_field!(clearing_failure_counts);
            assign_field!(last_clearing_failure_code);
            assign_field!(last_clearing_failure_reason);
            assign_field!(last_clearing_failure_unix_ms);
            assign_field!(clearing_static_amm_pools);
            assign_field!(last_clearing_route);
            assign_field!(last_clearing_candidates);
            assign_field!(fee_quote_failure_counts);
            assign_field!(fee_oracle_rates_ppm);
            assign_field!(fee_oracle_updated_unix_ms);
            assign_field!(fee_oracle_source);
            assign_field!(fee_oracle_allowed_sources);
            assign_field!(fee_oracle_disabled_sources);
            assign_field!(fee_oracle_disabled_source_reasons);
            assign_field!(fee_oracle_source_rotations);
            assign_field!(last_fee_quote);
            assign_field!(last_fee_quote_failure);
        }
        "vault" => {
            assign_field!(credit_vaults);
            assign_field!(next_credit_vault_id);
        }
        "policy" => {
            assign_field!(treasury_settlement_paused);
            assign_field!(treasury_redeem_paused);
            assign_field!(mapped_lock_bridge_paused);
            assign_field!(mapped_lock_min_confirmations);
            assign_field!(mapped_lock_contract_address);
            assign_field!(mapped_asset_burn_paused);
            assign_field!(mapped_asset_release_paused);
            assign_field!(mapped_asset_auto_heal_enabled);
            assign_field!(mapped_asset_auto_heal_rollback_enabled);
            assign_field!(mapped_header_source_required);
            assign_field!(mapped_header_source_allowed_peer_ids);
            assign_field!(mapped_header_source_disabled_peer_ids);
            assign_field!(mapped_header_source_disabled_peer_reasons);
            assign_field!(mapped_header_source_peer_rotations);
            assign_field!(mapped_header_source_min_quorum);
            assign_field!(mapped_header_source_policy_source);
            assign_field!(mapped_header_source_policy_version);
            assign_field!(mapped_header_source_policy_updated_unix_ms);
            assign_field!(mapped_header_attestation_required);
            assign_field!(mapped_header_attestation_allowed_signers);
            assign_field!(mapped_header_attestation_disabled_signers);
            assign_field!(mapped_header_attestation_disabled_signer_reasons);
            assign_field!(mapped_header_attestation_signer_rotations);
            assign_field!(mapped_header_attestation_min_quorum);
            assign_field!(mapped_header_attestation_policy_source);
            assign_field!(mapped_header_attestation_policy_version);
            assign_field!(mapped_header_attestation_policy_updated_unix_ms);
            assign_field!(treasury_reserve_share_bps);
            assign_field!(treasury_fee_share_bps);
            assign_field!(treasury_risk_buffer_share_bps);
            assign_field!(treasury_min_reserve_bucket_nov);
            assign_field!(treasury_min_fee_bucket_nov);
            assign_field!(treasury_min_risk_buffer_nov);
            assign_field!(treasury_policy_version);
            assign_field!(treasury_policy_source);
            assign_field!(treasury_policy_last_update_unix_ms);
        }
        "governance" => {
            assign_field!(governance_proposals);
            assign_field!(next_governance_proposal_id);
        }
        "native_execution" => {
            assign_field!(last_execution_trace);
            assign_field!(execution_traces_by_tx);
            assign_field!(execution_trace_order);
            assign_field!(aoem_semantic_ledger_sequence);
            assign_field!(aoem_semantic_ledger_head);
            assign_field!(unified_account_semantic_event_count);
            assign_field!(unified_account_semantic_head);
            assign_field!(unified_account_semantic_last_digest);
            assign_field!(unified_account_semantic_last_subject);
            assign_field!(unified_account_semantic_last_action);
        }
        _ => bail!("unknown nov native execution module_state shard: {shard}"),
    }
    Ok(())
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
struct NovNativeExecutionDirtySetV1 {
    account_asset_upserts: Vec<(String, String)>,
    account_asset_deletes: Vec<(String, String)>,
    receipt_upserts: Vec<String>,
    receipt_deletes: Vec<String>,
    module_state_shards: Vec<&'static str>,
    semantic_head: bool,
    snapshot_meta: bool,
}

fn native_execution_store_dirty_set_stats_json_v1(
    dirty: &NovNativeExecutionDirtySetV1,
) -> serde_json::Value {
    let semantic_head_writes = if dirty.semantic_head { 2u64 } else { 0 };
    let snapshot_meta_writes = if dirty.snapshot_meta { 2u64 } else { 0 };
    let module_state_shard_writes = dirty.module_state_shards.len() as u64;
    let account_asset_upserts = dirty.account_asset_upserts.len() as u64;
    let account_asset_deletes = dirty.account_asset_deletes.len() as u64;
    let receipt_upserts = dirty.receipt_upserts.len() as u64;
    let receipt_deletes = dirty.receipt_deletes.len() as u64;
    let receipt_index_writes = receipt_upserts;
    let dirty_write_count = module_state_shard_writes
        .saturating_add(account_asset_upserts)
        .saturating_add(receipt_upserts)
        .saturating_add(receipt_index_writes)
        .saturating_add(semantic_head_writes)
        .saturating_add(snapshot_meta_writes);
    let dirty_delete_count = account_asset_deletes
        .saturating_add(receipt_deletes)
        .saturating_add(1); // legacy snapshot blob delete guard.
    serde_json::json!({
        "dirty_account_asset_upserts": account_asset_upserts,
        "dirty_account_asset_deletes": account_asset_deletes,
        "dirty_receipt_upserts": receipt_upserts,
        "dirty_receipt_deletes": receipt_deletes,
        "dirty_receipt_index_writes": receipt_index_writes,
        "dirty_module_state_shards": module_state_shard_writes,
        "dirty_semantic_head": dirty.semantic_head,
        "dirty_semantic_head_writes": semantic_head_writes,
        "dirty_snapshot_meta": dirty.snapshot_meta,
        "dirty_snapshot_meta_writes": snapshot_meta_writes,
        "dirty_write_count": dirty_write_count,
        "dirty_delete_count": dirty_delete_count,
        "dirty_total_count": dirty_write_count.saturating_add(dirty_delete_count),
        "commit_model": "dirty_sharded_atomic_batch",
    })
}

fn native_execution_store_dirty_set_v1(
    previous: &NovNativeExecutionStoreV1,
    next: &NovNativeExecutionStoreV1,
    force_all_module_shards: bool,
) -> Result<NovNativeExecutionDirtySetV1> {
    let mut dirty = NovNativeExecutionDirtySetV1::default();
    for (key, shard) in native_rocksdb_module_state_shard_keys_v1() {
        let _ = key;
        let previous_shard = native_module_state_shard_value_v1(&previous.module_state, shard)?;
        let next_shard = native_module_state_shard_value_v1(&next.module_state, shard)?;
        if force_all_module_shards || previous_shard != next_shard {
            dirty.module_state_shards.push(shard);
        }
    }
    for (account_id, assets) in &next.module_state.account_asset_balances {
        for (asset, amount) in assets {
            let previous_amount = previous
                .module_state
                .account_asset_balances
                .get(account_id)
                .and_then(|items| items.get(asset))
                .copied();
            if previous_amount != Some(*amount) {
                dirty
                    .account_asset_upserts
                    .push((account_id.clone(), asset.clone()));
            }
        }
    }
    for (account_id, assets) in &previous.module_state.account_asset_balances {
        for asset in assets.keys() {
            if !next
                .module_state
                .account_asset_balances
                .get(account_id)
                .is_some_and(|items| items.contains_key(asset))
            {
                dirty
                    .account_asset_deletes
                    .push((account_id.clone(), asset.clone()));
            }
        }
    }
    for (tx_hash, receipt) in &next.receipts {
        if previous.receipts.get(tx_hash) != Some(receipt) {
            dirty.receipt_upserts.push(tx_hash.clone());
        }
    }
    for tx_hash in previous.receipts.keys() {
        if !next.receipts.contains_key(tx_hash) {
            dirty.receipt_deletes.push(tx_hash.clone());
        }
    }
    dirty.semantic_head = previous.module_state.aoem_semantic_ledger_sequence
        != next.module_state.aoem_semantic_ledger_sequence
        || previous.module_state.aoem_semantic_ledger_head
            != next.module_state.aoem_semantic_ledger_head;
    dirty.snapshot_meta = native_rocksdb_snapshot_meta_v1(previous)
        != native_rocksdb_snapshot_meta_v1(next)
        || !dirty.module_state_shards.is_empty()
        || !dirty.account_asset_upserts.is_empty()
        || !dirty.account_asset_deletes.is_empty()
        || !dirty.receipt_upserts.is_empty()
        || !dirty.receipt_deletes.is_empty()
        || dirty.semantic_head;
    Ok(dirty)
}

fn nov_native_execution_store_lock_path_v1(path: &Path) -> PathBuf {
    let mut raw = path.as_os_str().to_os_string();
    raw.push(".lock");
    PathBuf::from(raw)
}

struct NovNativeExecutionStoreWriteLockV1 {
    path: PathBuf,
    _file: fs::File,
}

impl Drop for NovNativeExecutionStoreWriteLockV1 {
    fn drop(&mut self) {
        let _ = fs::remove_file(self.path.as_path());
    }
}

fn is_stale_native_execution_store_lock_v1(path: &Path, now: std::time::SystemTime) -> bool {
    let Ok(metadata) = fs::metadata(path) else {
        return false;
    };
    let Ok(modified) = metadata.modified() else {
        return false;
    };
    now.duration_since(modified)
        .map(|age| age.as_millis() > u128::from(NOV_NATIVE_EXECUTION_STORE_LOCK_STALE_MS_V1))
        .unwrap_or(false)
}

fn acquire_nov_native_execution_store_write_lock_v1(
    path: &Path,
) -> Result<NovNativeExecutionStoreWriteLockV1> {
    let lock_path = nov_native_execution_store_lock_path_v1(path);
    if let Some(parent) = lock_path.parent() {
        if !parent.as_os_str().is_empty() {
            fs::create_dir_all(parent).with_context(|| {
                format!(
                    "create nov native execution store lock parent dir failed: {}",
                    parent.display()
                )
            })?;
        }
    }
    let started = Instant::now();
    loop {
        match fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(lock_path.as_path())
        {
            Ok(mut file) => {
                use std::io::Write;
                let _ = writeln!(
                    file,
                    "pid={} acquired_unix_ms={}",
                    std::process::id(),
                    now_unix_millis_v1()
                );
                return Ok(NovNativeExecutionStoreWriteLockV1 {
                    path: lock_path,
                    _file: file,
                });
            }
            Err(err) if err.kind() == std::io::ErrorKind::AlreadyExists => {
                if is_stale_native_execution_store_lock_v1(
                    lock_path.as_path(),
                    std::time::SystemTime::now(),
                ) {
                    let _ = fs::remove_file(lock_path.as_path());
                    continue;
                }
                if started.elapsed()
                    >= Duration::from_millis(NOV_NATIVE_EXECUTION_STORE_LOCK_TIMEOUT_MS_V1)
                {
                    bail!(
                        "nov native execution store write lock timeout: store={} lock={}",
                        path.display(),
                        lock_path.display()
                    );
                }
                std::thread::sleep(Duration::from_millis(
                    NOV_NATIVE_EXECUTION_STORE_LOCK_POLL_MS_V1,
                ));
            }
            Err(err) => {
                return Err(err).with_context(|| {
                    format!(
                        "acquire nov native execution store write lock failed: {}",
                        lock_path.display()
                    )
                });
            }
        }
    }
}

pub fn load_nov_native_execution_store_v1(path: &Path) -> Result<NovNativeExecutionStoreV1> {
    let backend = nov_native_execution_store_backend_v1();
    if native_execution_store_backend_reads_rocksdb_v1(backend.as_str()) {
        let rocksdb_path = nov_native_execution_store_rocksdb_path_v1(path);
        if rocksdb_path.exists() {
            return load_nov_native_execution_store_rocksdb_v1(rocksdb_path.as_path());
        }
        if backend == NOV_NATIVE_EXECUTION_STORE_BACKEND_ROCKSDB_V1 {
            return Ok(NovNativeExecutionStoreV1::default());
        }
    }
    load_nov_native_execution_store_json_v1(path)
}

fn load_nov_native_execution_store_json_v1(path: &Path) -> Result<NovNativeExecutionStoreV1> {
    if !path.exists() {
        return Ok(NovNativeExecutionStoreV1::default());
    }
    let bytes = fs::read(path)
        .with_context(|| format!("read nov native execution store failed: {}", path.display()))?;
    if bytes.is_empty() {
        return Ok(NovNativeExecutionStoreV1::default());
    }
    let mut store: NovNativeExecutionStoreV1 = serde_json::from_slice(bytes.as_slice())
        .with_context(|| {
            format!(
                "parse nov native execution store failed: {}",
                path.display()
            )
        })?;
    if store.schema.trim().is_empty() {
        store.schema = NOV_NATIVE_EXECUTION_STORE_SCHEMA_V1.to_string();
    }
    Ok(store)
}

fn load_nov_native_execution_store_rocksdb_v1(path: &Path) -> Result<NovNativeExecutionStoreV1> {
    let db = open_nov_native_execution_store_rocksdb_v1(path)?;
    if db
        .get(NOV_NATIVE_EXECUTION_STORE_ROCKSDB_SNAPSHOT_META_CURRENT_V1)
        .with_context(|| {
            format!(
                "read nov native execution rocksdb snapshot meta failed: {}",
                path.display()
            )
        })?
        .is_some()
    {
        return materialize_nov_native_execution_store_from_rocksdb_v1(&db, path);
    }

    let legacy_snapshot = db
        .get(NOV_NATIVE_EXECUTION_STORE_ROCKSDB_KEY_SNAPSHOT_V1)
        .with_context(|| {
            format!(
                "read legacy nov native execution rocksdb snapshot failed: {}",
                path.display()
            )
        })?;
    if let Some(raw) = legacy_snapshot {
        let mut store: NovNativeExecutionStoreV1 = serde_json::from_slice(raw.as_slice())
            .with_context(|| {
                format!(
                    "parse legacy nov native execution rocksdb snapshot failed: {}",
                    path.display()
                )
            })?;
        if store.schema.trim().is_empty() {
            store.schema = NOV_NATIVE_EXECUTION_STORE_SCHEMA_V1.to_string();
        }
        return Ok(store);
    }
    Ok(NovNativeExecutionStoreV1::default())
}

enum NovNativeExecutionReceiptLookupV1 {
    RocksDb { db: RocksDb, path: PathBuf },
    Materialized { store: NovNativeExecutionStoreV1 },
}

impl NovNativeExecutionReceiptLookupV1 {
    fn open(path: &Path) -> Result<Self> {
        let backend = nov_native_execution_store_backend_v1();
        if native_execution_store_backend_reads_rocksdb_v1(backend.as_str()) {
            let rocksdb_path = nov_native_execution_store_rocksdb_path_v1(path);
            if rocksdb_path.exists() {
                return Ok(Self::RocksDb {
                    db: open_nov_native_execution_store_rocksdb_v1(rocksdb_path.as_path())?,
                    path: rocksdb_path,
                });
            }
            if backend == NOV_NATIVE_EXECUTION_STORE_BACKEND_ROCKSDB_V1 {
                return Ok(Self::Materialized {
                    store: NovNativeExecutionStoreV1::default(),
                });
            }
        }
        Ok(Self::Materialized {
            store: load_nov_native_execution_store_json_v1(path)?,
        })
    }

    fn contains(&self, tx_hash: &str) -> Result<bool> {
        let key = normalize_tx_hash_hex_v1(tx_hash);
        match self {
            Self::RocksDb { db, path } => {
                let receipt_key = native_rocksdb_receipt_key_v1(key.as_str());
                Ok(db
                    .get(receipt_key.as_slice())
                    .with_context(|| {
                        format!(
                            "read nov native execution rocksdb receipt failed: store={} tx_hash={key}",
                            path.display()
                        )
                    })?
                    .is_some())
            }
            Self::Materialized { store } => Ok(store.receipts.contains_key(key.as_str())),
        }
    }
}

fn materialize_nov_native_execution_store_from_rocksdb_v1(
    db: &RocksDb,
    path: &Path,
) -> Result<NovNativeExecutionStoreV1> {
    let mut module_state = NovNativeExecutionModuleStateV1::default();
    let mut loaded_namespaced_module_state = false;
    for (key, shard) in native_rocksdb_module_state_shard_keys_v1() {
        if let Some(raw) = db.get(key).with_context(|| {
            format!(
                "read nov native execution rocksdb module_state/{shard} failed: {}",
                path.display()
            )
        })? {
            native_apply_module_state_shard_v1(&mut module_state, shard, raw.as_slice())?;
            loaded_namespaced_module_state = true;
        }
    }
    if !loaded_namespaced_module_state {
        module_state = match db
            .get(NOV_NATIVE_EXECUTION_STORE_ROCKSDB_KEY_MODULE_STATE_CORE_V1)
            .with_context(|| {
                format!(
                    "read nov native execution rocksdb module_state/core failed: {}",
                    path.display()
                )
            })? {
            Some(raw) => serde_json::from_slice::<NovNativeExecutionModuleStateV1>(raw.as_slice())
                .with_context(|| {
                    format!(
                        "parse nov native execution rocksdb module_state/core failed: {}",
                        path.display()
                    )
                })?,
            None => module_state,
        };
    }

    let mut store = NovNativeExecutionStoreV1 {
        schema: NOV_NATIVE_EXECUTION_STORE_SCHEMA_V1.to_string(),
        receipts: BTreeMap::new(),
        module_state,
        last_updated_unix_ms: 0,
    };

    if let Some(raw) = db
        .get(NOV_NATIVE_EXECUTION_STORE_ROCKSDB_SNAPSHOT_META_CURRENT_V1)
        .with_context(|| {
            format!(
                "read nov native execution rocksdb snapshot_meta/current failed: {}",
                path.display()
            )
        })?
    {
        let meta: NovNativeExecutionStoreSnapshotMetaV1 = serde_json::from_slice(raw.as_slice())
            .with_context(|| {
                format!(
                    "parse nov native execution rocksdb snapshot_meta/current failed: {}",
                    path.display()
                )
            })?;
        store.schema = meta.store_schema;
        store.last_updated_unix_ms = meta.last_updated_unix_ms;
    }

    store.module_state.account_asset_balances.clear();
    for item in native_rocksdb_iter_prefix_v1(
        db,
        NOV_NATIVE_EXECUTION_STORE_ROCKSDB_ACCOUNT_ASSET_PREFIX_V1,
    ) {
        let (key, raw) = item.with_context(|| {
            format!(
                "iterate nov native execution rocksdb account asset keys failed: {}",
                path.display()
            )
        })?;
        let Some((account_id, asset)) = native_rocksdb_decode_account_asset_key_v1(key.as_ref())
        else {
            continue;
        };
        let amount: u128 = serde_json::from_slice(raw.as_ref()).with_context(|| {
            format!(
                "parse nov native execution rocksdb account asset failed: key={}",
                String::from_utf8_lossy(key.as_ref())
            )
        })?;
        store
            .module_state
            .account_asset_balances
            .entry(account_id)
            .or_default()
            .insert(asset, amount);
    }

    store.receipts.clear();
    for item in
        native_rocksdb_iter_prefix_v1(db, NOV_NATIVE_EXECUTION_STORE_ROCKSDB_RECEIPT_PREFIX_V1)
    {
        let (key, raw) = item.with_context(|| {
            format!(
                "iterate nov native execution rocksdb receipt keys failed: {}",
                path.display()
            )
        })?;
        let receipt: NovNativeExecutionReceiptV1 = serde_json::from_slice(raw.as_ref())
            .with_context(|| {
                format!(
                    "parse nov native execution rocksdb receipt failed: key={}",
                    String::from_utf8_lossy(key.as_ref())
                )
            })?;
        store.receipts.insert(receipt.tx_hash.clone(), receipt);
    }

    Ok(store)
}

pub fn save_nov_native_execution_store_v1(
    path: &Path,
    store: &NovNativeExecutionStoreV1,
) -> Result<()> {
    save_nov_native_execution_store_with_previous_v1(path, None, store)
}

fn save_nov_native_execution_store_with_previous_v1(
    path: &Path,
    previous: Option<&NovNativeExecutionStoreV1>,
    store: &NovNativeExecutionStoreV1,
) -> Result<()> {
    let backend = nov_native_execution_store_backend_v1();
    if !matches!(
        backend.as_str(),
        NOV_NATIVE_EXECUTION_STORE_BACKEND_JSON_V1
            | NOV_NATIVE_EXECUTION_STORE_BACKEND_ROCKSDB_V1
            | NOV_NATIVE_EXECUTION_STORE_BACKEND_DUAL_V1
    ) {
        bail!(
            "invalid {}={}; valid: json|rocksdb|dual",
            NOV_NATIVE_EXECUTION_STORE_BACKEND_ENV,
            backend
        );
    }
    if native_execution_store_backend_writes_rocksdb_v1(backend.as_str()) {
        let rocksdb_path = nov_native_execution_store_rocksdb_path_v1(path);
        save_nov_native_execution_store_rocksdb_v1(rocksdb_path.as_path(), previous, store)?;
    }
    if !native_execution_store_backend_writes_json_v1(backend.as_str()) {
        return Ok(());
    }
    save_nov_native_execution_store_json_v1(path, store)
}

fn save_nov_native_execution_store_json_v1(
    path: &Path,
    store: &NovNativeExecutionStoreV1,
) -> Result<()> {
    if let Some(parent) = path.parent() {
        if !parent.as_os_str().is_empty() {
            fs::create_dir_all(parent).with_context(|| {
                format!(
                    "create nov native execution store parent dir failed: {}",
                    parent.display()
                )
            })?;
        }
    }
    let serialized = serde_json::to_string_pretty(store)
        .context("serialize nov native execution store failed")?;
    fs::write(path, serialized).with_context(|| {
        format!(
            "write nov native execution store failed: {}",
            path.display()
        )
    })?;
    Ok(())
}

fn save_nov_native_execution_store_rocksdb_v1(
    path: &Path,
    previous: Option<&NovNativeExecutionStoreV1>,
    store: &NovNativeExecutionStoreV1,
) -> Result<()> {
    let db = open_nov_native_execution_store_rocksdb_v1(path)?;
    let legacy_module_state_core_exists = db
        .get(NOV_NATIVE_EXECUTION_STORE_ROCKSDB_KEY_MODULE_STATE_CORE_V1)
        .with_context(|| {
            format!(
                "read nov native execution rocksdb module_state/core migration marker failed: {}",
                path.display()
            )
        })?
        .is_some();
    let previous_loaded;
    let previous_ref = match previous {
        Some(value) => value,
        None => {
            previous_loaded = materialize_nov_native_execution_store_from_rocksdb_v1(&db, path)
                .unwrap_or_else(|_| NovNativeExecutionStoreV1::default());
            &previous_loaded
        }
    };
    let dirty =
        native_execution_store_dirty_set_v1(previous_ref, store, legacy_module_state_core_exists)?;
    let meta = native_rocksdb_snapshot_meta_v1(store);
    let meta_encoded = serde_json::to_vec(&meta)
        .context("serialize nov native execution rocksdb snapshot meta failed")?;
    let mut batch = RocksDbWriteBatch::default();
    batch.delete(NOV_NATIVE_EXECUTION_STORE_ROCKSDB_KEY_SNAPSHOT_V1);
    if !dirty.module_state_shards.is_empty() {
        for (key, shard) in native_rocksdb_module_state_shard_keys_v1() {
            if !dirty.module_state_shards.contains(&shard) {
                continue;
            }
            let encoded = native_module_state_shard_value_v1(&store.module_state, shard)?;
            batch.put(key, encoded.as_slice());
        }
        batch.delete(NOV_NATIVE_EXECUTION_STORE_ROCKSDB_KEY_MODULE_STATE_CORE_V1);
    }
    if dirty.semantic_head || legacy_module_state_core_exists {
        batch.put(
            NOV_NATIVE_EXECUTION_STORE_ROCKSDB_KEY_SEMANTIC_HEAD_V1,
            store.module_state.aoem_semantic_ledger_head.as_bytes(),
        );
        batch.put(
            NOV_NATIVE_EXECUTION_STORE_ROCKSDB_KEY_SEMANTIC_SEQUENCE_V1,
            store
                .module_state
                .aoem_semantic_ledger_sequence
                .to_be_bytes(),
        );
        let by_height_key = native_rocksdb_semantic_by_height_key_v1(
            store.module_state.aoem_semantic_ledger_sequence,
        );
        batch.put(
            by_height_key.as_slice(),
            store.module_state.aoem_semantic_ledger_head.as_bytes(),
        );
    }
    if dirty.snapshot_meta || legacy_module_state_core_exists {
        batch.put(
            NOV_NATIVE_EXECUTION_STORE_ROCKSDB_SNAPSHOT_META_CURRENT_V1,
            meta_encoded.as_slice(),
        );
        let meta_by_height_key = native_rocksdb_snapshot_meta_by_height_key_v1(
            store.module_state.aoem_semantic_ledger_sequence,
        );
        batch.put(meta_by_height_key.as_slice(), meta_encoded.as_slice());
    }

    for (account_id, asset) in &dirty.account_asset_upserts {
        if let Some(amount) = store
            .module_state
            .account_asset_balances
            .get(account_id)
            .and_then(|items| items.get(asset))
        {
            let key = native_rocksdb_account_asset_key_v1(account_id, asset);
            let encoded = serde_json::to_vec(amount).with_context(|| {
                format!(
                    "serialize nov native account asset failed: account={account_id} asset={asset}"
                )
            })?;
            batch.put(key.as_slice(), encoded.as_slice());
        }
    }
    for (account_id, asset) in &dirty.account_asset_deletes {
        let key = native_rocksdb_account_asset_key_v1(account_id, asset);
        batch.delete(key.as_slice());
    }

    for tx_hash in &dirty.receipt_upserts {
        let Some(receipt) = store.receipts.get(tx_hash) else {
            continue;
        };
        let key = native_rocksdb_receipt_key_v1(tx_hash);
        let encoded = serde_json::to_vec(receipt)
            .with_context(|| format!("serialize nov native execution receipt failed: {tx_hash}"))?;
        batch.put(key.as_slice(), encoded.as_slice());
        let idx = store
            .receipts
            .keys()
            .position(|candidate| candidate == tx_hash)
            .unwrap_or(0);
        let receipt_height_key = native_rocksdb_receipt_by_height_key_v1(
            store.module_state.aoem_semantic_ledger_sequence,
            idx,
            tx_hash,
        );
        batch.put(receipt_height_key.as_slice(), tx_hash.as_bytes());
    }
    for tx_hash in &dirty.receipt_deletes {
        let key = native_rocksdb_receipt_key_v1(tx_hash);
        batch.delete(key.as_slice());
    }
    db.write(batch).with_context(|| {
        format!(
            "write nov native execution rocksdb atomic batch failed: {}",
            path.display()
        )
    })?;
    Ok(())
}

pub fn mutate_nov_native_execution_store_with_write_lock_v1<T, F>(
    path: &Path,
    mutate: F,
) -> Result<T>
where
    F: FnOnce(&mut NovNativeExecutionStoreV1) -> Result<T>,
{
    let _write_lock = acquire_nov_native_execution_store_write_lock_v1(path)?;
    let mut store = load_nov_native_execution_store_v1(path)?;
    let previous_store = store.clone();
    let output = mutate(&mut store)?;
    save_nov_native_execution_store_with_previous_v1(path, Some(&previous_store), &store)?;
    Ok(output)
}

pub fn load_last_nov_native_aoem_semantic_ledger_mirror_record_v1(
    path: &Path,
) -> Result<Option<NovAoemSemanticLedgerMirrorRecordV1>> {
    if !path.exists() {
        return Ok(None);
    }
    let file = fs::File::open(path).with_context(|| {
        format!(
            "open AOEM semantic ledger mirror failed: {}",
            path.display()
        )
    })?;
    let reader = BufReader::new(file);
    let mut last = None;
    for line in reader.lines() {
        let line = line.with_context(|| {
            format!(
                "read AOEM semantic ledger mirror line failed: {}",
                path.display()
            )
        })?;
        let trimmed = line.trim();
        if trimmed.is_empty() {
            continue;
        }
        last = Some(
            serde_json::from_str::<NovAoemSemanticLedgerMirrorRecordV1>(trimmed).with_context(
                || {
                    format!(
                        "decode AOEM semantic ledger mirror record failed: {}",
                        path.display()
                    )
                },
            )?,
        );
    }
    Ok(last)
}

fn append_nov_native_aoem_semantic_ledger_mirror_record_v1(
    path: &Path,
    record: &NovAoemSemanticLedgerMirrorRecordV1,
) -> Result<()> {
    append_nov_native_aoem_semantic_ledger_mirror_records_v1(path, std::slice::from_ref(record))
}

fn append_nov_native_aoem_semantic_ledger_mirror_records_v1(
    path: &Path,
    records: &[NovAoemSemanticLedgerMirrorRecordV1],
) -> Result<()> {
    if records.is_empty() {
        return Ok(());
    }
    if let Some(parent) = path.parent() {
        if !parent.as_os_str().is_empty() {
            fs::create_dir_all(parent).with_context(|| {
                format!(
                    "create AOEM semantic ledger mirror dir failed: {}",
                    parent.display()
                )
            })?;
        }
    }
    let mut previous = load_last_nov_native_aoem_semantic_ledger_mirror_record_v1(path)?;
    for record in records {
        if let Some(last) = previous.as_ref() {
            if record.sequence != last.sequence.saturating_add(1) {
                bail!(
                    "ERR_AOEM_SEMANTIC_LEDGER_MIRROR_SEQUENCE_GAP: expected={} got={}",
                    last.sequence.saturating_add(1),
                    record.sequence
                );
            }
            if record.prev_seal != last.commit_seal {
                bail!(
                    "ERR_AOEM_SEMANTIC_LEDGER_MIRROR_PREV_SEAL_MISMATCH: expected={} got={}",
                    last.commit_seal,
                    record.prev_seal
                );
            }
        }
        previous = Some(record.clone());
    }
    let mut writer = OpenOptions::new()
        .create(true)
        .append(true)
        .open(path)
        .with_context(|| {
            format!(
                "open AOEM semantic ledger mirror for append failed: {}",
                path.display()
            )
        })?;
    for record in records {
        let bytes = serde_json::to_vec(record)
            .context("serialize AOEM semantic ledger mirror record failed")?;
        writer.write_all(bytes.as_slice()).with_context(|| {
            format!(
                "append AOEM semantic ledger mirror record failed: {}",
                path.display()
            )
        })?;
        writer.write_all(b"\n").with_context(|| {
            format!(
                "append AOEM semantic ledger mirror newline failed: {}",
                path.display()
            )
        })?;
    }
    writer.flush().with_context(|| {
        format!(
            "flush AOEM semantic ledger mirror record failed: {}",
            path.display()
        )
    })?;
    Ok(())
}

fn env_u128_or_v1(name: &str, default: u128) -> u128 {
    std::env::var(name)
        .ok()
        .and_then(|raw| raw.trim().parse::<u128>().ok())
        .unwrap_or(default)
}

fn ceil_div_u128_v1(numerator: u128, denominator: u128) -> u128 {
    if denominator == 0 {
        return u128::MAX;
    }
    numerator
        .saturating_add(denominator.saturating_sub(1))
        .saturating_div(denominator)
}

fn default_fee_rate_ppm_for_asset_v1(asset: &str) -> u128 {
    match asset {
        "NOV" => NOV_FEE_RATE_PPM_NOV_V1,
        "USDT" => NOV_FEE_RATE_PPM_USDT_V1,
        "DAI" => NOV_FEE_RATE_PPM_DAI_V1,
        "NUSD" => NOV_FEE_RATE_PPM_NUSD_V1,
        "ETH" => NOV_FEE_RATE_PPM_ETH_V1,
        "BTC" => NOV_FEE_RATE_PPM_BTC_V1,
        _ => 0,
    }
}

fn configured_fee_rate_ppm_v1(asset: &str) -> Option<u128> {
    let raw = std::env::var(NOV_NATIVE_FEE_RATE_PPM_ENV).unwrap_or_default();
    for token in raw.split(',') {
        let trimmed = token.trim();
        if trimmed.is_empty() {
            continue;
        }
        let mut parts = trimmed.splitn(2, '=');
        let Some(symbol_raw) = parts.next() else {
            continue;
        };
        let Some(rate_raw) = parts.next() else {
            continue;
        };
        if normalize_asset_symbol_v1(symbol_raw) != asset {
            continue;
        }
        if let Ok(rate) = rate_raw.trim().parse::<u128>() {
            if rate > 0 {
                return Some(rate);
            }
        }
    }
    None
}

fn execution_fee_oracle_max_age_ms_v1() -> u128 {
    env_u128_or_v1(
        NOV_NATIVE_FEE_ORACLE_MAX_AGE_MS_ENV,
        NOV_FEE_ORACLE_DEFAULT_MAX_AGE_MS_V1,
    )
}

fn normalize_fee_oracle_source_v1(raw: &str) -> String {
    let normalized = raw.trim().to_ascii_lowercase();
    if normalized.is_empty() {
        "runtime_oracle".to_string()
    } else {
        normalized
    }
}

fn fee_oracle_source_v1(store: &NovNativeExecutionStoreV1) -> String {
    normalize_fee_oracle_source_v1(store.module_state.fee_oracle_source.as_str())
}

fn fee_oracle_allowed_sources_v1(store: &NovNativeExecutionStoreV1) -> Vec<String> {
    let mut sources: Vec<String> = store
        .module_state
        .fee_oracle_allowed_sources
        .iter()
        .map(|source| normalize_fee_oracle_source_v1(source.as_str()))
        .filter(|source| !source.trim().is_empty())
        .collect();
    if sources.is_empty() {
        sources.push("runtime_oracle".to_string());
    }
    sources.sort();
    sources.dedup();
    sources
}

fn fee_oracle_disabled_sources_v1(store: &NovNativeExecutionStoreV1) -> Vec<String> {
    let mut sources: Vec<String> = store
        .module_state
        .fee_oracle_disabled_sources
        .iter()
        .map(|source| normalize_fee_oracle_source_v1(source.as_str()))
        .filter(|source| !source.trim().is_empty())
        .collect();
    sources.sort();
    sources.dedup();
    sources
}

fn fee_oracle_disabled_source_reasons_v1(
    store: &NovNativeExecutionStoreV1,
) -> BTreeMap<String, String> {
    store
        .module_state
        .fee_oracle_disabled_source_reasons
        .iter()
        .map(|(source, reason)| {
            (
                normalize_fee_oracle_source_v1(source.as_str()),
                reason.trim().to_string(),
            )
        })
        .filter(|(source, reason)| !source.is_empty() && !reason.is_empty())
        .collect()
}

fn fee_oracle_source_rotations_v1(store: &NovNativeExecutionStoreV1) -> BTreeMap<String, String> {
    store
        .module_state
        .fee_oracle_source_rotations
        .iter()
        .map(|(old_source, new_source)| {
            (
                normalize_fee_oracle_source_v1(old_source.as_str()),
                normalize_fee_oracle_source_v1(new_source.as_str()),
            )
        })
        .filter(|(old_source, new_source)| !old_source.is_empty() && !new_source.is_empty())
        .collect()
}

fn fee_oracle_source_disabled_v1(store: &NovNativeExecutionStoreV1) -> bool {
    let source = fee_oracle_source_v1(store);
    fee_oracle_disabled_sources_v1(store)
        .iter()
        .any(|disabled| disabled == &source)
}

fn fee_oracle_disabled_reason_v1(store: &NovNativeExecutionStoreV1) -> Option<String> {
    let source = fee_oracle_source_v1(store);
    fee_oracle_disabled_source_reasons_v1(store)
        .get(source.as_str())
        .cloned()
        .or_else(|| {
            if fee_oracle_source_disabled_v1(store) {
                Some("disabled_by_governance".to_string())
            } else {
                None
            }
        })
}

fn fee_oracle_rotation_target_v1(store: &NovNativeExecutionStoreV1) -> Option<String> {
    let source = fee_oracle_source_v1(store);
    fee_oracle_source_rotations_v1(store)
        .get(source.as_str())
        .cloned()
}

fn fee_oracle_source_allowed_v1(store: &NovNativeExecutionStoreV1) -> bool {
    let source = fee_oracle_source_v1(store);
    if fee_oracle_source_disabled_v1(store) {
        return false;
    }
    fee_oracle_allowed_sources_v1(store)
        .iter()
        .any(|allowed| allowed == &source)
}

fn normalize_reserve_proof_status_v1(raw: &str) -> String {
    match raw.trim().to_ascii_lowercase().as_str() {
        "active" | "valid" => "active".to_string(),
        "constrained" | "review" | "under_review" => "constrained".to_string(),
        "revoked" | "disabled" => "revoked".to_string(),
        "expired" => "expired".to_string(),
        _ => "active".to_string(),
    }
}

fn reserve_proof_effective_status_v1(proof: &NovTreasuryReserveProofV1, now_ms: u128) -> String {
    let status = normalize_reserve_proof_status_v1(proof.status.as_str());
    if status == "revoked" {
        return status;
    }
    if proof.expires_at_unix_ms > 0 && now_ms > proof.expires_at_unix_ms {
        return "expired".to_string();
    }
    status
}

fn reserve_proof_block_reason_for_asset_v1(
    store: &NovNativeExecutionStoreV1,
    asset: &str,
    now_ms: u128,
) -> Option<String> {
    let normalized = normalize_asset_symbol_v1(asset);
    if normalized == "NOV" {
        return None;
    }
    let proof = store
        .module_state
        .treasury_reserve_proofs
        .get(normalized.as_str())?;
    let effective_status = reserve_proof_effective_status_v1(proof, now_ms);
    if effective_status == "active" {
        return None;
    }
    Some(format!(
        "asset={} reserve_proof_effective_status={} proof_type={} proof_source={} proof_reference={}",
        normalized,
        effective_status,
        proof.proof_type,
        proof.proof_source,
        proof.proof_reference
    ))
}

fn reserve_proof_capacity_block_reason_v1(
    store: &NovNativeExecutionStoreV1,
    asset: &str,
    projected_reserve_after: u128,
    now_ms: u128,
) -> Option<String> {
    let normalized = normalize_asset_symbol_v1(asset);
    if normalized == "NOV" {
        return None;
    }
    let proof = store
        .module_state
        .treasury_reserve_proofs
        .get(normalized.as_str())?;
    let effective_status = reserve_proof_effective_status_v1(proof, now_ms);
    if effective_status != "active" || projected_reserve_after <= proof.reserve_amount {
        return None;
    }
    Some(format!(
        "asset={} projected_reserve_after={} proof_reserve_amount={} proof_type={} proof_source={} proof_reference={}",
        normalized,
        projected_reserve_after,
        proof.reserve_amount,
        proof.proof_type,
        proof.proof_source,
        proof.proof_reference
    ))
}

fn account_asset_liability_v1(store: &NovNativeExecutionStoreV1, asset: &str) -> u128 {
    let normalized = normalize_asset_symbol_v1(asset);
    store
        .module_state
        .account_asset_balances
        .values()
        .filter_map(|assets| assets.get(normalized.as_str()).copied())
        .fold(0u128, u128::saturating_add)
}

fn m2_bridge_risk_block_reason_for_asset_v1(
    store: &NovNativeExecutionStoreV1,
    asset: &str,
    now_ms: u128,
) -> Option<String> {
    let normalized = normalize_asset_symbol_v1(asset);
    if normalized != "NETH" {
        return None;
    }
    if store.module_state.mapped_lock_bridge_paused {
        return Some("asset=NETH m2_bridge_risk=mapped_lock_bridge_paused".to_string());
    }
    if store.module_state.mapped_asset_burn_paused {
        return Some("asset=NETH m2_bridge_risk=mapped_asset_burn_paused".to_string());
    }
    if store.module_state.mapped_asset_release_paused {
        return Some("asset=NETH m2_bridge_risk=mapped_asset_release_paused".to_string());
    }
    if store
        .module_state
        .mapped_lock_contract_address
        .trim()
        .is_empty()
    {
        return Some("asset=NETH m2_bridge_risk=mapped_lock_contract_address_unset".to_string());
    }
    if store.module_state.mapped_lock_min_confirmations == 0 {
        return Some("asset=NETH m2_bridge_risk=mapped_lock_min_confirmations_unset".to_string());
    }
    let Some(proof) = store.module_state.treasury_reserve_proofs.get("NETH") else {
        return Some("asset=NETH m2_bridge_risk=reserve_proof_missing".to_string());
    };
    let effective_status = reserve_proof_effective_status_v1(proof, now_ms);
    if effective_status != "active" {
        return Some(format!(
            "asset=NETH m2_bridge_risk=reserve_proof_effective_status_{} proof_type={} proof_source={} proof_reference={}",
            effective_status, proof.proof_type, proof.proof_source, proof.proof_reference
        ));
    }
    let liability = account_asset_liability_v1(store, "NETH");
    let treasury_reserve = store
        .module_state
        .treasury_reserves
        .get("NETH")
        .copied()
        .unwrap_or(0);
    if liability > treasury_reserve {
        return Some(format!(
            "asset=NETH m2_bridge_risk=m2_liability_exceeds_treasury_reserve liability={} treasury_reserve={}",
            liability, treasury_reserve
        ));
    }
    if liability > proof.reserve_amount {
        return Some(format!(
            "asset=NETH m2_bridge_risk=m2_liability_exceeds_reserve_proof liability={} proof_reserve_amount={}",
            liability, proof.reserve_amount
        ));
    }
    if treasury_reserve > proof.reserve_amount {
        return Some(format!(
            "asset=NETH m2_bridge_risk=treasury_reserve_exceeds_reserve_proof treasury_reserve={} proof_reserve_amount={}",
            treasury_reserve, proof.reserve_amount
        ));
    }
    None
}

fn fee_quote_reason_v1(code: &str, detail: &str) -> String {
    format!("{}.{}: {}", NOV_FEE_FAILURE_QUOTE_PREFIX_V1, code, detail)
}

fn fee_clearing_reason_v1(code: &str, detail: &str) -> String {
    format!(
        "{}.{}: {}",
        NOV_FEE_FAILURE_CLEARING_PREFIX_V1, code, detail
    )
}

fn fee_settlement_reason_v1(code: &str, detail: &str) -> String {
    format!(
        "{}.{}: {}",
        NOV_FEE_FAILURE_SETTLEMENT_PREFIX_V1, code, detail
    )
}

fn is_fee_quote_reason_v1(reason: &str) -> bool {
    reason.starts_with(&format!("{NOV_FEE_FAILURE_QUOTE_PREFIX_V1}."))
}

fn fee_reason_code_v1<'a>(reason: &'a str, prefix: &str) -> Option<&'a str> {
    let needle = format!("{prefix}.");
    let tail = reason.strip_prefix(needle.as_str())?;
    let code = tail.split_once(':').map(|(code, _)| code).unwrap_or(tail);
    let trimmed = code.trim();
    if trimmed.is_empty() {
        None
    } else {
        Some(trimmed)
    }
}

fn increment_quote_failure_v1(store: &mut NovNativeExecutionStoreV1, asset: &str, reason: &str) {
    let key = format!("{}:{}", normalize_asset_symbol_v1(asset), reason);
    let counter = store
        .module_state
        .fee_quote_failure_counts
        .entry(key)
        .or_insert(0);
    *counter = counter.saturating_add(1);
}

fn increment_settlement_failure_v1(store: &mut NovNativeExecutionStoreV1, reason: &str) {
    let counter = store
        .module_state
        .treasury_settlement_failure_counts
        .entry(reason.to_string())
        .or_insert(0);
    *counter = counter.saturating_add(1);
}

fn increment_string_counter_v1(map: &mut BTreeMap<String, u64>, key: impl Into<String>) {
    let counter = map.entry(key.into()).or_insert(0);
    *counter = counter.saturating_add(1);
}

fn extract_failure_code_v1(reason: &str) -> Option<String> {
    let code = reason
        .split_once(':')
        .map(|(head, _)| head)
        .unwrap_or(reason)
        .trim();
    if code.starts_with("fee.") {
        Some(code.to_string())
    } else {
        None
    }
}

fn is_policy_rejected_failure_code_v1(code: &str) -> bool {
    code.starts_with("fee.clearing.constrained_")
}

fn find_latest_journal_entry_by_tx_hash_v1<'a>(
    store: &'a NovNativeExecutionStoreV1,
    tx_hash: &str,
) -> Option<&'a NovTreasurySettlementJournalEntryV1> {
    let key = normalize_tx_hash_hex_v1(tx_hash);
    store
        .module_state
        .treasury_settlement_journal
        .iter()
        .rev()
        .find(|entry| normalize_tx_hash_hex_v1(entry.tx_hash.as_str()) == key)
}

fn build_execution_trace_v1(
    request: &NovExecutionRequestV1,
    settled_fee: &NovSettledFeeV1,
    receipt: &NovNativeExecutionReceiptV1,
    subject_meta: &NovExecutionSubjectMetaV1,
    store: &NovNativeExecutionStoreV1,
    now_ms: u128,
) -> NovExecutionTraceV1 {
    let final_failure_code = receipt
        .failure_reason
        .as_deref()
        .and_then(extract_failure_code_v1);

    let quote_ref =
        store.module_state.last_fee_quote.as_ref().filter(|quote| {
            settled_fee.quote_id.is_empty() || quote.quote_id == settled_fee.quote_id
        });
    let quote_phase = NovTraceQuotePhaseV1 {
        quote_id: quote_ref.map(|quote| quote.quote_id.clone()),
        quoted_pay_amount: quote_ref.map(|quote| quote.quoted_pay_amount),
        quoted_pay_amount_with_slippage: quote_ref
            .map(|quote| quote.quoted_pay_amount_with_slippage),
        quoted_at_unix_ms: quote_ref.map(|quote| quote.quoted_at_unix_ms),
        quote_expiry_unix_ms: quote_ref.map(|quote| quote.expires_at_unix_ms),
        oracle_source: quote_ref.map(|quote| quote.price_source.clone()),
        oracle_updated_at_unix_ms: quote_ref.map(|quote| quote.oracle_updated_at_unix_ms),
        quote_failure_code: final_failure_code
            .as_ref()
            .filter(|code| code.starts_with("fee.quote."))
            .cloned(),
    };

    let policy_rejected_reason = final_failure_code
        .as_ref()
        .filter(|code| is_policy_rejected_failure_code_v1(code))
        .cloned();
    let candidate_routes = store
        .module_state
        .last_clearing_candidates
        .iter()
        .cloned()
        .map(|candidate| NovTraceRouteCandidateV1 {
            route_id: candidate.route_id,
            route_source: candidate.source_id.source.as_str().to_string(),
            expected_nov_out: candidate.expected_nov_out,
            liquidity_available: candidate.liquidity_available,
            fee_ppm: candidate.fee_ppm,
            quoted_at_ms: candidate.quoted_at_ms,
            expires_at_ms: candidate.expires_at_ms,
            rejected_by_policy: policy_rejected_reason.is_some(),
            rejected_reason: policy_rejected_reason.clone(),
        })
        .collect::<Vec<_>>();
    let selected_route = if let Some(meta) = &receipt.route_meta {
        Some(NovTraceSelectedRouteV1 {
            route_id: meta.route_id.clone(),
            route_source: meta.route_source.clone(),
            expected_nov_out: meta.expected_nov_out,
            selection_reason: meta.selection_reason.clone(),
        })
    } else {
        store
            .module_state
            .last_clearing_route
            .as_ref()
            .map(|route| NovTraceSelectedRouteV1 {
                route_id: route.route_id.clone(),
                route_source: route.route_source.clone(),
                expected_nov_out: route.expected_nov_out,
                selection_reason: route.selection_reason.clone(),
            })
    };
    let routing_phase = NovTraceRoutingPhaseV1 {
        candidate_route_count: candidate_routes.len(),
        candidate_routes,
        selected_route,
        routing_failure_code: final_failure_code
            .as_ref()
            .filter(|code| code.starts_with("fee.clearing."))
            .cloned(),
    };

    let expected_out = receipt
        .route_meta
        .as_ref()
        .map(|meta| meta.expected_nov_out)
        .unwrap_or_default();
    let actual_out = receipt.settled_fee_nov;
    let slippage_bps_realized = if expected_out == 0 {
        None
    } else if actual_out >= expected_out {
        Some(0)
    } else {
        Some(
            expected_out
                .saturating_sub(actual_out)
                .saturating_mul(10_000)
                .saturating_div(expected_out) as u32,
        )
    };
    let cleared_at_ms = if receipt.fee_clearing_route_ref.trim().is_empty() {
        None
    } else {
        store
            .module_state
            .last_clearing_route
            .as_ref()
            .filter(|route| route.route_id == receipt.fee_clearing_route_ref)
            .map(|route| route.cleared_at_ms)
    };
    let clearing_phase = NovTraceClearingPhaseV1 {
        actual_route_id: if receipt.fee_clearing_route_ref.trim().is_empty() {
            None
        } else {
            Some(receipt.fee_clearing_route_ref.clone())
        },
        actual_route_source: if receipt.fee_clearing_source.trim().is_empty() {
            None
        } else {
            Some(receipt.fee_clearing_source.clone())
        },
        actual_pay_amount: Some(receipt.paid_amount),
        actual_nov_out: Some(receipt.settled_fee_nov),
        actual_fee_ppm: receipt.route_meta.as_ref().map(|meta| meta.route_fee_ppm),
        slippage_bps_realized,
        clearing_failure_code: final_failure_code
            .as_ref()
            .filter(|code| code.starts_with("fee.clearing."))
            .cloned(),
        cleared_at_ms,
    };

    let journal_entry = find_latest_journal_entry_by_tx_hash_v1(store, receipt.tx_hash.as_str());
    let settlement_phase = NovTraceSettlementPhaseV1 {
        settled_fee_nov: Some(receipt.settled_fee_nov),
        reserve_bucket_delta_nov: journal_entry.map(|entry| entry.reserve_bucket_delta_nov),
        fee_bucket_delta_nov: journal_entry.map(|entry| entry.fee_bucket_delta_nov),
        risk_buffer_delta_nov: journal_entry.map(|entry| entry.risk_buffer_delta_nov),
        settlement_journal_entry_type: journal_entry.map(|entry| entry.kind.clone()),
        settlement_status: journal_entry.map(|entry| entry.status.clone()),
        settlement_failure_code: final_failure_code
            .as_ref()
            .filter(|code| code.starts_with("fee.settlement."))
            .cloned(),
    };

    NovExecutionTraceV1 {
        trace_id: format!("{}:{now_ms}", receipt.tx_hash),
        tx_id: receipt.tx_hash.clone(),
        account_id: subject_meta.account_id.clone(),
        fee_owner_account_id: subject_meta.fee_owner_account_id.clone(),
        nonce_owner_account_id: subject_meta.nonce_owner_account_id.clone(),
        key_algo: subject_meta.key_algo.clone(),
        execution_policy: subject_meta.execution_policy.clone(),
        policy_enforced: subject_meta.policy_enforced,
        policy_rejection_reason: subject_meta.policy_rejection_reason.clone(),
        pay_asset: normalize_asset_symbol_v1(request.fee_pay_asset.as_str()),
        max_pay_amount: request.fee_max_pay_amount,
        nov_needed: settled_fee.nov_amount,
        policy_contract_id: settled_fee.policy_contract_id.clone(),
        policy_source: settled_fee.policy_source.clone(),
        policy_threshold_state: settled_fee.policy_threshold_state.clone(),
        policy_constrained_strategy: settled_fee.policy_constrained_strategy.clone(),
        quote_phase,
        routing_phase,
        clearing_phase,
        settlement_phase,
        aoem_semantic_ingress: receipt.aoem_semantic_ingress.clone(),
        final_status: if receipt.status {
            "success".to_string()
        } else {
            "failed".to_string()
        },
        final_failure_code,
        created_at_ms: now_ms,
    }
}

fn persist_execution_trace_v1(store: &mut NovNativeExecutionStoreV1, trace: NovExecutionTraceV1) {
    let key = normalize_tx_hash_hex_v1(trace.tx_id.as_str());
    store
        .module_state
        .execution_trace_order
        .retain(|item| item != &key);
    store.module_state.execution_trace_order.push(key.clone());
    store
        .module_state
        .execution_traces_by_tx
        .insert(key.clone(), trace.clone());
    store.module_state.last_execution_trace = Some(trace);

    while store.module_state.execution_trace_order.len() > NOV_EXECUTION_TRACE_MAX_ENTRIES_V1 {
        if let Some(evicted) = store.module_state.execution_trace_order.first().cloned() {
            store.module_state.execution_trace_order.remove(0);
            store
                .module_state
                .execution_traces_by_tx
                .remove(evicted.as_str());
        } else {
            break;
        }
    }
}

fn build_clearing_metrics_summary_v1(store: &NovNativeExecutionStoreV1) -> serde_json::Value {
    let mut route_source_hits = BTreeMap::<String, u64>::new();
    let mut route_source_failures = BTreeMap::<String, u64>::new();
    let mut selection_reason_hits = BTreeMap::<String, u64>::new();
    let mut successful_clearings = 0u64;
    let mut failed_clearings = 0u64;

    for trace in store.module_state.execution_traces_by_tx.values() {
        if trace.final_status == "success" {
            successful_clearings = successful_clearings.saturating_add(1);
        } else {
            failed_clearings = failed_clearings.saturating_add(1);
        }
        if let Some(selected) = &trace.routing_phase.selected_route {
            increment_string_counter_v1(&mut route_source_hits, selected.route_source.clone());
            if !selected.selection_reason.trim().is_empty() {
                increment_string_counter_v1(
                    &mut selection_reason_hits,
                    selected.selection_reason.clone(),
                );
            }
        }
        if let Some(code) = trace.final_failure_code.as_deref() {
            if code.starts_with("fee.clearing.") {
                let source = trace
                    .clearing_phase
                    .actual_route_source
                    .clone()
                    .or_else(|| {
                        trace
                            .routing_phase
                            .selected_route
                            .as_ref()
                            .map(|selected| selected.route_source.clone())
                    })
                    .unwrap_or_else(|| "unknown".to_string());
                increment_string_counter_v1(&mut route_source_failures, source);
            }
        }
    }

    serde_json::json!({
        "trace_count": store.module_state.execution_traces_by_tx.len(),
        "total_clearing_attempts": successful_clearings.saturating_add(failed_clearings),
        "successful_clearings": successful_clearings,
        "failed_clearings": failed_clearings,
        "route_source_hits": route_source_hits,
        "route_source_failures": route_source_failures,
        "selection_reason_hits": selection_reason_hits,
        "failure_counts": store.module_state.clearing_failure_counts.clone(),
    })
}

fn build_policy_metrics_summary_v1(
    store: &NovNativeExecutionStoreV1,
    policy_contract_id: &str,
    policy_source: &str,
    threshold_state: &str,
    constrained_strategy: &str,
) -> serde_json::Value {
    let mut threshold_state_hits = BTreeMap::<String, u64>::new();
    let mut constrained_strategy_hits = BTreeMap::<String, u64>::new();
    let mut policy_event_state_hits = BTreeMap::<String, u64>::new();

    for trace in store.module_state.execution_traces_by_tx.values() {
        if !trace.policy_threshold_state.trim().is_empty() {
            increment_string_counter_v1(
                &mut threshold_state_hits,
                trace.policy_threshold_state.clone(),
            );
        }
        if !trace.policy_constrained_strategy.trim().is_empty() {
            increment_string_counter_v1(
                &mut constrained_strategy_hits,
                trace.policy_constrained_strategy.clone(),
            );
        }
    }
    for entry in &store.module_state.treasury_settlement_journal {
        if !entry.policy_event_state.trim().is_empty() {
            increment_string_counter_v1(
                &mut policy_event_state_hits,
                entry.policy_event_state.clone(),
            );
        }
    }

    serde_json::json!({
        "policy_contract_id": policy_contract_id,
        "policy_source": policy_source,
        "threshold_state": threshold_state,
        "constrained_strategy": constrained_strategy,
        "threshold_state_hits": threshold_state_hits,
        "constrained_strategy_hits": constrained_strategy_hits,
        "policy_event_state_hits": policy_event_state_hits,
        "trace_count": store.module_state.execution_traces_by_tx.len(),
        "journal_entries": store.module_state.treasury_settlement_journal.len(),
    })
}

fn current_day_index_v1(now_ms: u128) -> u64 {
    now_ms.saturating_div(NOV_MILLIS_PER_DAY_V1) as u64
}

fn refresh_clearing_daily_window_v1(store: &mut NovNativeExecutionStoreV1, now_ms: u128) {
    let day = current_day_index_v1(now_ms);
    if store.module_state.clearing_daily_window_day != day {
        store.module_state.clearing_daily_window_day = day;
        store.module_state.clearing_daily_nov_used = 0;
    }
}

fn clearing_daily_nov_hard_limit_v1(store: &NovNativeExecutionStoreV1) -> u128 {
    if store.module_state.clearing_daily_nov_hard_limit > 0 {
        store.module_state.clearing_daily_nov_hard_limit
    } else {
        env_u128_or_v1(
            NOV_NATIVE_CLEARING_DAILY_NOV_HARD_LIMIT_ENV,
            NOV_CLEARING_DAILY_NOV_HARD_LIMIT_DEFAULT_V1,
        )
    }
}

fn clearing_enabled_v1(store: &NovNativeExecutionStoreV1) -> bool {
    store.module_state.clearing_enabled
}

fn clearing_constrained_max_slippage_bps_v1(store: &NovNativeExecutionStoreV1) -> u32 {
    if store.module_state.clearing_constrained_max_slippage_bps > 0 {
        store.module_state.clearing_constrained_max_slippage_bps
    } else {
        env_u128_or_v1(
            NOV_NATIVE_CLEARING_CONSTRAINED_MAX_SLIPPAGE_BPS_ENV,
            u128::from(NOV_CLEARING_CONSTRAINED_MAX_SLIPPAGE_BPS_DEFAULT_V1),
        ) as u32
    }
}

fn clearing_constrained_daily_usage_bps_v1(store: &NovNativeExecutionStoreV1) -> u32 {
    let raw = if store.module_state.clearing_constrained_daily_usage_bps > 0 {
        u128::from(store.module_state.clearing_constrained_daily_usage_bps)
    } else {
        env_u128_or_v1(
            NOV_NATIVE_CLEARING_CONSTRAINED_DAILY_USAGE_BPS_ENV,
            u128::from(NOV_CLEARING_CONSTRAINED_DAILY_USAGE_BPS_DEFAULT_V1),
        )
    };
    let clamped = raw.clamp(1, u128::from(NOV_TREASURY_SHARE_BPS_DENOMINATOR_V1));
    clamped as u32
}

fn clearing_constrained_strategy_v1(store: &NovNativeExecutionStoreV1) -> String {
    if !store
        .module_state
        .clearing_constrained_strategy
        .trim()
        .is_empty()
    {
        return normalize_constrained_strategy_v1(
            store.module_state.clearing_constrained_strategy.as_str(),
        )
        .to_string();
    }
    std::env::var(NOV_NATIVE_CLEARING_CONSTRAINED_STRATEGY_ENV)
        .ok()
        .map(|raw| normalize_constrained_strategy_v1(raw.as_str()).to_string())
        .unwrap_or_else(|| NOV_CLEARING_CONSTRAINED_STRATEGY_DAILY_VOLUME_ONLY_V1.to_string())
}

fn clearing_require_healthy_risk_buffer_v1(store: &NovNativeExecutionStoreV1) -> bool {
    if store.module_state.clearing_require_healthy_risk_buffer {
        true
    } else {
        bool_env_default_v1(NOV_NATIVE_CLEARING_REQUIRE_HEALTHY_RISK_BUFFER_ENV, false)
    }
}

fn resolve_treasury_settlement_policy_v1(
    store: &NovNativeExecutionStoreV1,
) -> NovTreasurySettlementPolicyV1 {
    let policy_version = store
        .module_state
        .treasury_policy_version
        .max(NOV_TREASURY_POLICY_VERSION_DEFAULT_V1);
    let state_policy_source = if store.module_state.treasury_policy_source.trim().is_empty() {
        None
    } else {
        Some(normalize_policy_source_v1(
            store.module_state.treasury_policy_source.as_str(),
        ))
    };
    let clearing_enabled = clearing_enabled_v1(store);
    let clearing_require_healthy_risk_buffer = clearing_require_healthy_risk_buffer_v1(store);
    let clearing_constrained_max_slippage_bps = clearing_constrained_max_slippage_bps_v1(store);
    let clearing_constrained_daily_usage_bps = clearing_constrained_daily_usage_bps_v1(store);
    let clearing_constrained_strategy = clearing_constrained_strategy_v1(store);
    let clearing_daily_nov_hard_limit = clearing_daily_nov_hard_limit_v1(store);
    let clearing_daily_nov_used = store.module_state.clearing_daily_nov_used;
    let clearing_daily_window_day = store.module_state.clearing_daily_window_day;
    let state_reserve = store.module_state.treasury_reserve_share_bps;
    let state_fee = store.module_state.treasury_fee_share_bps;
    let state_buffer = store.module_state.treasury_risk_buffer_share_bps;
    let state_total = state_reserve
        .saturating_add(state_fee)
        .saturating_add(state_buffer);
    if state_reserve > 0
        && state_fee > 0
        && state_buffer > 0
        && state_total == NOV_TREASURY_SHARE_BPS_DENOMINATOR_V1
    {
        return NovTreasurySettlementPolicyV1 {
            policy_version,
            policy_source: state_policy_source
                .clone()
                .unwrap_or_else(|| "runtime_path".to_string()),
            reserve_share_bps: state_reserve,
            fee_share_bps: state_fee,
            risk_buffer_share_bps: state_buffer,
            min_reserve_bucket_nov: store.module_state.treasury_min_reserve_bucket_nov,
            min_fee_bucket_nov: store.module_state.treasury_min_fee_bucket_nov,
            min_risk_buffer_nov: store.module_state.treasury_min_risk_buffer_nov.max(1),
            settlement_paused: store.module_state.treasury_settlement_paused,
            redeem_paused: store.module_state.treasury_redeem_paused,
            mapped_lock_bridge_paused: store.module_state.mapped_lock_bridge_paused,
            mapped_lock_min_confirmations: store.module_state.mapped_lock_min_confirmations,
            mapped_lock_contract_address: store.module_state.mapped_lock_contract_address.clone(),
            mapped_asset_burn_paused: store.module_state.mapped_asset_burn_paused,
            mapped_asset_release_paused: store.module_state.mapped_asset_release_paused,
            mapped_asset_auto_heal_enabled: store.module_state.mapped_asset_auto_heal_enabled,
            mapped_asset_auto_heal_rollback_enabled: store
                .module_state
                .mapped_asset_auto_heal_rollback_enabled,
            mapped_asset_reorg_response_policy: mapped_asset_reorg_response_policy_v1(
                store.module_state.mapped_asset_auto_heal_enabled,
                store.module_state.mapped_asset_auto_heal_rollback_enabled,
            )
            .to_string(),
            clearing_enabled,
            clearing_daily_nov_hard_limit,
            clearing_daily_nov_used,
            clearing_daily_window_day,
            clearing_require_healthy_risk_buffer,
            clearing_constrained_max_slippage_bps,
            clearing_constrained_daily_usage_bps,
            clearing_constrained_strategy: clearing_constrained_strategy.clone(),
            source: "runtime_state".to_string(),
        };
    }

    let env_reserve_raw = std::env::var(NOV_NATIVE_TREASURY_RESERVE_SHARE_BPS_ENV).ok();
    let env_fee_raw = std::env::var(NOV_NATIVE_TREASURY_FEE_SHARE_BPS_ENV).ok();
    let env_buffer_raw = std::env::var(NOV_NATIVE_TREASURY_RISK_BUFFER_SHARE_BPS_ENV).ok();
    let env_any = env_reserve_raw.is_some() || env_fee_raw.is_some() || env_buffer_raw.is_some();
    let env_reserve = env_reserve_raw
        .as_deref()
        .and_then(|raw| raw.trim().parse::<u32>().ok());
    let env_fee = env_fee_raw
        .as_deref()
        .and_then(|raw| raw.trim().parse::<u32>().ok());
    let env_buffer = env_buffer_raw
        .as_deref()
        .and_then(|raw| raw.trim().parse::<u32>().ok());
    let env_tuple = env_reserve
        .zip(env_fee)
        .zip(env_buffer)
        .map(|((r, f), b)| (r, f, b));
    let (reserve_share_bps, fee_share_bps, risk_buffer_share_bps, source) =
        if let Some((r, f, b)) = env_tuple {
            let total = r.saturating_add(f).saturating_add(b);
            if r > 0 && f > 0 && b > 0 && total == NOV_TREASURY_SHARE_BPS_DENOMINATOR_V1 {
                (r, f, b, "env")
            } else {
                (
                    NOV_TREASURY_RESERVE_SHARE_BPS_DEFAULT_V1,
                    NOV_TREASURY_FEE_SHARE_BPS_DEFAULT_V1,
                    NOV_TREASURY_RISK_BUFFER_SHARE_BPS_DEFAULT_V1,
                    "default_fallback_invalid_env",
                )
            }
        } else if env_any {
            (
                NOV_TREASURY_RESERVE_SHARE_BPS_DEFAULT_V1,
                NOV_TREASURY_FEE_SHARE_BPS_DEFAULT_V1,
                NOV_TREASURY_RISK_BUFFER_SHARE_BPS_DEFAULT_V1,
                "default_fallback_partial_env",
            )
        } else {
            (
                NOV_TREASURY_RESERVE_SHARE_BPS_DEFAULT_V1,
                NOV_TREASURY_FEE_SHARE_BPS_DEFAULT_V1,
                NOV_TREASURY_RISK_BUFFER_SHARE_BPS_DEFAULT_V1,
                "default",
            )
        };
    let settlement_paused = store.module_state.treasury_settlement_paused
        || bool_env_default_v1(NOV_NATIVE_TREASURY_SETTLEMENT_PAUSED_ENV, false);
    let redeem_paused = store.module_state.treasury_redeem_paused
        || bool_env_default_v1(NOV_NATIVE_TREASURY_REDEEM_PAUSED_ENV, false);
    let min_reserve_bucket_nov = if store.module_state.treasury_min_reserve_bucket_nov > 0 {
        store.module_state.treasury_min_reserve_bucket_nov
    } else {
        env_u128_or_v1(
            NOV_NATIVE_TREASURY_MIN_RESERVE_BUCKET_NOV_ENV,
            NOV_TREASURY_MIN_RESERVE_BUCKET_NOV_DEFAULT_V1,
        )
    };
    let min_fee_bucket_nov = if store.module_state.treasury_min_fee_bucket_nov > 0 {
        store.module_state.treasury_min_fee_bucket_nov
    } else {
        env_u128_or_v1(
            NOV_NATIVE_TREASURY_MIN_FEE_BUCKET_NOV_ENV,
            NOV_TREASURY_MIN_FEE_BUCKET_NOV_DEFAULT_V1,
        )
    };
    let min_risk_buffer_nov = if store.module_state.treasury_min_risk_buffer_nov > 0 {
        store.module_state.treasury_min_risk_buffer_nov
    } else {
        env_u128_or_v1(
            NOV_NATIVE_TREASURY_MIN_RISK_BUFFER_NOV_ENV,
            NOV_TREASURY_MIN_RISK_BUFFER_NOV_DEFAULT_V1,
        )
    }
    .max(1);
    NovTreasurySettlementPolicyV1 {
        policy_version,
        policy_source: state_policy_source.unwrap_or_else(|| {
            if source == "env" {
                "config_path".to_string()
            } else {
                "default_path".to_string()
            }
        }),
        reserve_share_bps,
        fee_share_bps,
        risk_buffer_share_bps,
        min_reserve_bucket_nov,
        min_fee_bucket_nov,
        min_risk_buffer_nov,
        settlement_paused,
        redeem_paused,
        mapped_lock_bridge_paused: store.module_state.mapped_lock_bridge_paused,
        mapped_lock_min_confirmations: store.module_state.mapped_lock_min_confirmations,
        mapped_lock_contract_address: store.module_state.mapped_lock_contract_address.clone(),
        mapped_asset_burn_paused: store.module_state.mapped_asset_burn_paused,
        mapped_asset_release_paused: store.module_state.mapped_asset_release_paused,
        mapped_asset_auto_heal_enabled: store.module_state.mapped_asset_auto_heal_enabled,
        mapped_asset_auto_heal_rollback_enabled: store
            .module_state
            .mapped_asset_auto_heal_rollback_enabled,
        mapped_asset_reorg_response_policy: mapped_asset_reorg_response_policy_v1(
            store.module_state.mapped_asset_auto_heal_enabled,
            store.module_state.mapped_asset_auto_heal_rollback_enabled,
        )
        .to_string(),
        clearing_enabled,
        clearing_daily_nov_hard_limit,
        clearing_daily_nov_used,
        clearing_daily_window_day,
        clearing_require_healthy_risk_buffer,
        clearing_constrained_max_slippage_bps,
        clearing_constrained_daily_usage_bps,
        clearing_constrained_strategy,
        source: source.to_string(),
    }
}

fn apply_treasury_settlement_split_v1(
    store: &mut NovNativeExecutionStoreV1,
    settled_nov: u128,
    policy: &NovTreasurySettlementPolicyV1,
) -> (u128, u128, u128) {
    let reserve_nov = settled_nov
        .saturating_mul(u128::from(policy.reserve_share_bps))
        .saturating_div(u128::from(NOV_TREASURY_SHARE_BPS_DENOMINATOR_V1));
    let fee_nov = settled_nov
        .saturating_mul(u128::from(policy.fee_share_bps))
        .saturating_div(u128::from(NOV_TREASURY_SHARE_BPS_DENOMINATOR_V1));
    let risk_buffer_nov = settled_nov
        .saturating_sub(reserve_nov)
        .saturating_sub(fee_nov);
    store.module_state.treasury_reserve_bucket_nov = store
        .module_state
        .treasury_reserve_bucket_nov
        .saturating_add(reserve_nov);
    store.module_state.treasury_fee_bucket_nov = store
        .module_state
        .treasury_fee_bucket_nov
        .saturating_add(fee_nov);
    store.module_state.treasury_risk_buffer_nov = store
        .module_state
        .treasury_risk_buffer_nov
        .saturating_add(risk_buffer_nov);
    (reserve_nov, fee_nov, risk_buffer_nov)
}

fn saturating_u128_to_i128_v1(value: u128) -> i128 {
    if value > i128::MAX as u128 {
        i128::MAX
    } else {
        value as i128
    }
}

fn append_treasury_settlement_journal_v1(
    store: &mut NovNativeExecutionStoreV1,
    mut entry: NovTreasurySettlementJournalEntryV1,
) {
    let next_seq = store
        .module_state
        .treasury_settlement_journal_next_seq
        .saturating_add(1);
    store.module_state.treasury_settlement_journal_next_seq = next_seq;
    entry.seq = next_seq;
    store.module_state.treasury_settlement_journal.push(entry);
    let len = store.module_state.treasury_settlement_journal.len();
    if len > NOV_TREASURY_SETTLEMENT_JOURNAL_MAX_ENTRIES_V1 {
        let trim = len.saturating_sub(NOV_TREASURY_SETTLEMENT_JOURNAL_MAX_ENTRIES_V1);
        store
            .module_state
            .treasury_settlement_journal
            .drain(0..trim);
    }
}

fn build_treasury_accounting_snapshot_v1(store: &NovNativeExecutionStoreV1) -> serde_json::Value {
    let bucket_total_nov = store
        .module_state
        .treasury_reserve_bucket_nov
        .saturating_add(store.module_state.treasury_fee_bucket_nov)
        .saturating_add(store.module_state.treasury_risk_buffer_nov);
    let net_settled_nov = store
        .module_state
        .treasury_settled_nov_total
        .saturating_sub(store.module_state.treasury_redeemed_nov_total);
    let nov_reserve_total = store
        .module_state
        .treasury_reserves
        .get("NOV")
        .copied()
        .unwrap_or(0);
    serde_json::json!({
        "net_settled_nov": net_settled_nov,
        "bucket_total_nov": bucket_total_nov,
        "bucket_consistent_with_net_settled": bucket_total_nov == net_settled_nov,
        "nov_reserve_total": nov_reserve_total,
        "nov_reserve_minus_bucket_nov": saturating_u128_to_i128_v1(nov_reserve_total) - saturating_u128_to_i128_v1(bucket_total_nov),
    })
}

fn risk_buffer_status_v1(
    store: &NovNativeExecutionStoreV1,
    policy: &NovTreasurySettlementPolicyV1,
) -> &'static str {
    if store.module_state.treasury_risk_buffer_nov < policy.min_risk_buffer_nov {
        "below_min"
    } else {
        "healthy"
    }
}

fn bucket_status_v1(current: u128, min_required: u128) -> &'static str {
    if current < min_required {
        "below_min"
    } else {
        "healthy"
    }
}

fn bucket_boundary_snapshot_v1(
    store: &NovNativeExecutionStoreV1,
    policy: &NovTreasurySettlementPolicyV1,
) -> serde_json::Value {
    serde_json::json!({
        "reserve_bucket": {
            "current_nov": store.module_state.treasury_reserve_bucket_nov,
            "min_required_nov": policy.min_reserve_bucket_nov,
            "status": bucket_status_v1(
                store.module_state.treasury_reserve_bucket_nov,
                policy.min_reserve_bucket_nov
            ),
        },
        "fee_bucket": {
            "current_nov": store.module_state.treasury_fee_bucket_nov,
            "min_required_nov": policy.min_fee_bucket_nov,
            "status": bucket_status_v1(
                store.module_state.treasury_fee_bucket_nov,
                policy.min_fee_bucket_nov
            ),
        },
        "risk_buffer": {
            "current_nov": store.module_state.treasury_risk_buffer_nov,
            "min_required_nov": policy.min_risk_buffer_nov,
            "status": risk_buffer_status_v1(store, policy),
        },
    })
}

fn allocation_parameters_snapshot_v1(policy: &NovTreasurySettlementPolicyV1) -> serde_json::Value {
    let total = policy
        .reserve_share_bps
        .saturating_add(policy.fee_share_bps)
        .saturating_add(policy.risk_buffer_share_bps);
    serde_json::json!({
        "reserve_allocation_bps": policy.reserve_share_bps,
        "fee_allocation_bps": policy.fee_share_bps,
        "risk_buffer_allocation_bps": policy.risk_buffer_share_bps,
        "bps_denominator": NOV_TREASURY_SHARE_BPS_DENOMINATOR_V1,
        "allocation_total_bps": total,
        "allocation_tuple_valid": total == NOV_TREASURY_SHARE_BPS_DENOMINATOR_V1,
    })
}

fn treasury_policy_contract_id_v1(policy: &NovTreasurySettlementPolicyV1) -> String {
    format!(
        "nov_treasury_policy_v1:{}:{}:{}:{}:{}:{}:{}:{}:{}:{}:{}:{}:{}:{}:{}:{}:{}:{}",
        policy.policy_version,
        normalize_policy_source_v1(policy.policy_source.as_str()),
        policy.reserve_share_bps,
        policy.fee_share_bps,
        policy.risk_buffer_share_bps,
        policy.min_reserve_bucket_nov,
        policy.min_fee_bucket_nov,
        policy.min_risk_buffer_nov,
        if policy.settlement_paused { 1 } else { 0 },
        if policy.redeem_paused { 1 } else { 0 },
        if policy.clearing_enabled { 1 } else { 0 },
        policy.clearing_daily_nov_hard_limit,
        if policy.clearing_require_healthy_risk_buffer {
            1
        } else {
            0
        },
        policy.clearing_constrained_max_slippage_bps,
        policy.clearing_constrained_daily_usage_bps,
        policy.clearing_constrained_strategy,
        if policy.mapped_asset_auto_heal_rollback_enabled {
            1
        } else {
            0
        },
        policy.mapped_asset_reorg_response_policy
    )
}

fn treasury_policy_contract_snapshot_v1(
    policy: &NovTreasurySettlementPolicyV1,
    allocation_parameters: &serde_json::Value,
) -> serde_json::Value {
    serde_json::json!({
        "contract": "nov.treasury.policy.contract/v1",
        "policy_contract_id": treasury_policy_contract_id_v1(policy),
        "policy_version": policy.policy_version,
        "policy_source": normalize_policy_source_v1(policy.policy_source.as_str()),
        "parameters": {
            "allocation_parameters": allocation_parameters.clone(),
            "min_reserve_bucket_nov": policy.min_reserve_bucket_nov,
            "min_fee_bucket_nov": policy.min_fee_bucket_nov,
            "min_risk_buffer_nov": policy.min_risk_buffer_nov,
            "settlement_paused": policy.settlement_paused,
            "redeem_paused": policy.redeem_paused,
            "mapped_asset_auto_heal_enabled": policy.mapped_asset_auto_heal_enabled,
            "mapped_asset_auto_heal_rollback_enabled": policy.mapped_asset_auto_heal_rollback_enabled,
            "mapped_asset_reorg_response_policy": policy.mapped_asset_reorg_response_policy,
            "mapped_lock_contract_address": policy.mapped_lock_contract_address,
            "clearing_enabled": policy.clearing_enabled,
            "clearing_daily_nov_hard_limit": policy.clearing_daily_nov_hard_limit,
            "clearing_require_healthy_risk_buffer": policy.clearing_require_healthy_risk_buffer,
            "clearing_constrained_max_slippage_bps": policy.clearing_constrained_max_slippage_bps,
            "clearing_constrained_daily_usage_bps": policy.clearing_constrained_daily_usage_bps,
            "clearing_constrained_strategy": policy.clearing_constrained_strategy,
        },
    })
}

fn treasury_policy_context_snapshot_v1(
    policy: &NovTreasurySettlementPolicyV1,
    policy_contract_id: &str,
    threshold_state: &str,
) -> serde_json::Value {
    serde_json::json!({
        "policy_contract_id": policy_contract_id,
        "policy_version": policy.policy_version,
        "policy_source": normalize_policy_source_v1(policy.policy_source.as_str()),
        "policy_threshold_state": threshold_state,
        "policy_constrained_strategy": policy.clearing_constrained_strategy,
    })
}

fn clearing_policy_gate_snapshot_v1(
    store: &NovNativeExecutionStoreV1,
    policy: &NovTreasurySettlementPolicyV1,
) -> serde_json::Value {
    let risk_buffer_healthy =
        store.module_state.treasury_risk_buffer_nov >= policy.min_risk_buffer_nov;
    let reserve_bucket_healthy =
        store.module_state.treasury_reserve_bucket_nov >= policy.min_reserve_bucket_nov;
    let fee_bucket_healthy =
        store.module_state.treasury_fee_bucket_nov >= policy.min_fee_bucket_nov;
    let daily_limit_reached = policy.clearing_daily_nov_hard_limit > 0
        && store.module_state.clearing_daily_nov_used >= policy.clearing_daily_nov_hard_limit;
    let constrained_daily_nov_cap = if policy.clearing_daily_nov_hard_limit > 0 {
        policy
            .clearing_daily_nov_hard_limit
            .saturating_mul(u128::from(policy.clearing_constrained_daily_usage_bps))
            .saturating_div(u128::from(NOV_TREASURY_SHARE_BPS_DENOMINATOR_V1))
            .max(1)
    } else {
        0
    };
    let daily_limit_constrained = policy.clearing_daily_nov_hard_limit > 0
        && store
            .module_state
            .clearing_daily_nov_used
            .saturating_mul(u128::from(10_000u32))
            >= policy
                .clearing_daily_nov_hard_limit
                .saturating_mul(u128::from(policy.clearing_constrained_daily_usage_bps));
    let mut blockers = Vec::new();
    if !policy.clearing_enabled {
        blockers.push("clearing_disabled");
    }
    if policy.clearing_require_healthy_risk_buffer && !risk_buffer_healthy {
        blockers.push("risk_buffer_below_min");
    }
    if daily_limit_reached {
        blockers.push("daily_volume_exceeded");
    }
    let mut constrained_reasons = Vec::new();
    if policy.clearing_require_healthy_risk_buffer && !risk_buffer_healthy {
        constrained_reasons.push("risk_buffer_below_min");
    }
    if !reserve_bucket_healthy {
        constrained_reasons.push("reserve_bucket_below_min");
    }
    if !fee_bucket_healthy {
        constrained_reasons.push("fee_bucket_below_min");
    }
    if daily_limit_constrained && !daily_limit_reached {
        constrained_reasons.push("daily_limit_near");
    }
    let threshold_state = if !blockers.is_empty() {
        "blocked"
    } else if !constrained_reasons.is_empty() {
        "constrained"
    } else {
        "healthy"
    };
    serde_json::json!({
        "can_clear_non_nov_now": blockers.is_empty(),
        "threshold_state": threshold_state,
        "blockers": blockers,
        "constrained_reasons": constrained_reasons,
        "risk_buffer_gate_enabled": policy.clearing_require_healthy_risk_buffer,
        "risk_buffer_healthy": risk_buffer_healthy,
        "reserve_bucket_healthy": reserve_bucket_healthy,
        "fee_bucket_healthy": fee_bucket_healthy,
        "constrained_max_slippage_bps": policy.clearing_constrained_max_slippage_bps,
        "constrained_daily_usage_bps": policy.clearing_constrained_daily_usage_bps,
        "constrained_daily_nov_cap": constrained_daily_nov_cap,
        "constrained_strategy": policy.clearing_constrained_strategy.clone(),
        "constrained_route_strategy": if policy.clearing_constrained_strategy.as_str() == NOV_CLEARING_CONSTRAINED_STRATEGY_TREASURY_DIRECT_ONLY_V1 {
            "treasury_direct_only"
        } else {
            "none"
        },
        "daily_limit_reached": daily_limit_reached,
        "daily_limit_near": daily_limit_constrained && !daily_limit_reached,
        "daily_nov_used": store.module_state.clearing_daily_nov_used,
        "daily_nov_hard_limit": policy.clearing_daily_nov_hard_limit,
    })
}

fn treasury_policy_paths_snapshot_v1(
    store: &NovNativeExecutionStoreV1,
    policy: &NovTreasurySettlementPolicyV1,
) -> serde_json::Value {
    serde_json::json!({
        "active_path": policy.policy_source,
        "config_path": {
            "supported": true,
            "source_hint": "config_path",
            "env_keys": [
                NOV_NATIVE_TREASURY_RESERVE_SHARE_BPS_ENV,
                NOV_NATIVE_TREASURY_FEE_SHARE_BPS_ENV,
                NOV_NATIVE_TREASURY_RISK_BUFFER_SHARE_BPS_ENV,
                NOV_NATIVE_TREASURY_MIN_RESERVE_BUCKET_NOV_ENV,
                NOV_NATIVE_TREASURY_MIN_FEE_BUCKET_NOV_ENV,
                NOV_NATIVE_TREASURY_MIN_RISK_BUFFER_NOV_ENV,
            ],
        },
        "governance_path": {
            "supported": true,
            "source_hint": "governance_path",
            "last_update_unix_ms": store.module_state.treasury_policy_last_update_unix_ms,
            "last_version": store.module_state.treasury_policy_version,
        },
    })
}

fn default_clearing_assets_v1() -> Vec<String> {
    std::env::var(NOV_NATIVE_FEE_CLEARING_DEFAULT_ASSETS_ENV)
        .ok()
        .filter(|raw| !raw.trim().is_empty())
        .unwrap_or_else(|| NOV_FEE_CLEARING_DEFAULT_ASSETS_V1.to_string())
        .split(',')
        .map(normalize_asset_symbol_v1)
        .filter(|item| !item.trim().is_empty())
        .collect()
}

fn is_default_clearing_asset_enabled_v1(asset: &str) -> bool {
    let normalized = normalize_asset_symbol_v1(asset);
    default_clearing_assets_v1()
        .iter()
        .any(|item| item == &normalized)
}

fn default_clearing_liquidity_v1() -> u128 {
    env_u128_or_v1(
        NOV_NATIVE_FEE_DEFAULT_CLEARING_NOV_LIQUIDITY_ENV,
        NOV_FEE_DEFAULT_CLEARING_NOV_LIQUIDITY_V1,
    )
}

fn protocol_clearing_epoch_ms_v1() -> u128 {
    env_u128_or_v1(
        NOV_NATIVE_PROTOCOL_CLEARING_EPOCH_MS_ENV,
        NOV_PROTOCOL_CLEARING_EPOCH_MS_DEFAULT_V1,
    )
    .max(1)
}

fn clamp_epoch_rate_ppm_v1(rate: u128, prev: u128, max_down_bps: u32, max_up_bps: u32) -> u128 {
    if prev == 0 {
        return rate;
    }
    let min_rate = prev
        .saturating_mul(u128::from(
            10_000u32.saturating_sub(max_down_bps.min(10_000)),
        ))
        .saturating_div(10_000);
    let max_rate = prev
        .saturating_mul(u128::from(10_000u32.saturating_add(max_up_bps)))
        .saturating_div(10_000)
        .max(min_rate);
    rate.clamp(min_rate, max_rate)
}

fn apply_down_bps_v1(value: u128, bps: u32) -> u128 {
    value
        .saturating_mul(u128::from(10_000u32.saturating_sub(bps.min(10_000))))
        .saturating_div(10_000)
}

fn apply_up_bps_v1(value: u128, bps: u32) -> u128 {
    value
        .saturating_mul(u128::from(10_000u32.saturating_add(bps)))
        .saturating_add(9_999)
        .saturating_div(10_000)
}

fn rate_deviation_bps_v1(left: u128, right: u128) -> u32 {
    if left == 0 || right == 0 {
        return 10_000;
    }
    let high = left.max(right);
    let low = left.min(right);
    high.saturating_sub(low)
        .saturating_mul(10_000)
        .saturating_div(low)
        .min(10_000) as u32
}

fn median_rate_ppm_v1(mut rates: Vec<u128>) -> Option<u128> {
    rates.retain(|rate| *rate > 0);
    if rates.is_empty() {
        return None;
    }
    rates.sort_unstable();
    Some(rates[rates.len() / 2])
}

fn has_amm_twap_liquidity_v1(store: &NovNativeExecutionStoreV1, asset: &str) -> bool {
    let normalized = normalize_asset_symbol_v1(asset);
    store
        .module_state
        .clearing_static_amm_pools
        .values()
        .any(|pool| {
            pool.enabled
                && normalize_asset_symbol_v1(pool.asset_x.as_str()) == normalized
                && normalize_asset_symbol_v1(pool.asset_y.as_str()) == "NOV"
                && pool.reserve_x > 0
                && pool.reserve_y >= NOV_PROTOCOL_CLEARING_MIN_AMM_TWAP_NOV_LIQUIDITY_V1
        })
}

fn protocol_oracle_ref_rate_ppm_v1(
    store: &NovNativeExecutionStoreV1,
    asset: &str,
    now_ms: u128,
) -> Option<u128> {
    if !fee_oracle_source_allowed_v1(store) {
        return None;
    }
    let normalized = normalize_asset_symbol_v1(asset);
    let rate = store
        .module_state
        .fee_oracle_rates_ppm
        .get(&normalized)
        .copied()?;
    if rate == 0 {
        return None;
    }
    let updated = store.module_state.fee_oracle_updated_unix_ms;
    if updated == 0 {
        return None;
    }
    if updated > 0 && now_ms > updated.saturating_add(execution_fee_oracle_max_age_ms_v1().max(1)) {
        return None;
    }
    Some(rate)
}

fn build_protocol_clearing_price_v1(
    store: &NovNativeExecutionStoreV1,
    asset: &str,
    now_ms: u128,
) -> Result<NovProtocolClearingPriceV1> {
    let normalized = normalize_asset_symbol_v1(asset);
    if normalized == "NOV" {
        return Ok(NovProtocolClearingPriceV1 {
            asset: normalized,
            epoch: now_ms.saturating_div(protocol_clearing_epoch_ms_v1()),
            epoch_ms: protocol_clearing_epoch_ms_v1(),
            p_prev_ppm: NOV_FEE_RATE_PPM_NOV_V1,
            p_ref_ppm: NOV_FEE_RATE_PPM_NOV_V1,
            p_epoch_ppm: NOV_FEE_RATE_PPM_NOV_V1,
            p_pay_ppm: NOV_FEE_RATE_PPM_NOV_V1,
            p_redeem_ppm: NOV_FEE_RATE_PPM_NOV_V1,
            p_amm_twap_ppm: None,
            p_nav_ppm: Some(NOV_FEE_RATE_PPM_NOV_V1),
            p_oracle_ref_ppm: None,
            reserve_haircut_bps: 0,
            liquidity_haircut_bps: 0,
            volatility_haircut_bps: 0,
            redemption_spread_bps: 0,
            risk_surcharge_bps: 0,
            max_epoch_up_bps: 0,
            max_epoch_down_bps: 0,
            max_source_deviation_bps: NOV_PROTOCOL_CLEARING_MAX_SOURCE_DEVIATION_BPS_V1,
            state: "healthy".to_string(),
            sources_used: vec!["native_nov".to_string()],
            sources_rejected: Vec::new(),
            reason: None,
            updated_unix_ms: now_ms,
        });
    }

    let epoch_ms = protocol_clearing_epoch_ms_v1();
    let epoch = now_ms.saturating_div(epoch_ms);
    let p_prev_ppm = store
        .module_state
        .protocol_clearing_prices
        .get(&normalized)
        .map(|price| price.p_epoch_ppm)
        .or_else(|| {
            store
                .module_state
                .clearing_rate_ppm
                .get(&normalized)
                .copied()
        })
        .or_else(|| configured_fee_rate_ppm_v1(normalized.as_str()))
        .unwrap_or_else(|| default_fee_rate_ppm_for_asset_v1(normalized.as_str()));
    let p_amm_twap_ppm = store
        .module_state
        .protocol_clearing_amm_twap_rate_ppm
        .get(&normalized)
        .copied()
        .filter(|rate| *rate > 0);
    let p_nav_ppm = store
        .module_state
        .protocol_clearing_nav_rate_ppm
        .get(&normalized)
        .copied()
        .filter(|rate| *rate > 0);
    let p_oracle_ref_ppm = protocol_oracle_ref_rate_ppm_v1(store, normalized.as_str(), now_ms);
    let amm_twap_has_liquidity = p_amm_twap_ppm
        .map(|_| has_amm_twap_liquidity_v1(store, normalized.as_str()))
        .unwrap_or(false);

    let mut candidates = Vec::<(&'static str, u128)>::new();
    let mut rejected = Vec::<String>::new();
    if let Some(rate) = p_amm_twap_ppm {
        if amm_twap_has_liquidity {
            candidates.push(("amm_twap", rate));
        } else {
            rejected.push("amm_twap:low_liquidity".to_string());
        }
    }
    if let Some(rate) = p_nav_ppm {
        candidates.push(("treasury_nav", rate));
    }
    if let Some(rate) = p_oracle_ref_ppm {
        candidates.push(("permissioned_oracle_ref", rate));
    } else if store
        .module_state
        .fee_oracle_rates_ppm
        .contains_key(&normalized)
        && !fee_oracle_source_allowed_v1(store)
    {
        if fee_oracle_source_disabled_v1(store) {
            rejected.push(format!(
                "permissioned_oracle_ref:source_disabled source={} reason={} rotation_target={}",
                fee_oracle_source_v1(store),
                fee_oracle_disabled_reason_v1(store).unwrap_or_else(|| "disabled".to_string()),
                fee_oracle_rotation_target_v1(store).unwrap_or_default()
            ));
        } else {
            rejected.push(format!(
                "permissioned_oracle_ref:source_not_allowed source={}",
                fee_oracle_source_v1(store)
            ));
        }
    } else if store
        .module_state
        .fee_oracle_rates_ppm
        .contains_key(&normalized)
    {
        let rate = store
            .module_state
            .fee_oracle_rates_ppm
            .get(&normalized)
            .copied()
            .unwrap_or_default();
        if rate == 0 {
            rejected.push(format!(
                "permissioned_oracle_ref:rate_zero source={}",
                fee_oracle_source_v1(store)
            ));
        } else {
            let updated = store.module_state.fee_oracle_updated_unix_ms;
            let max_age_ms = execution_fee_oracle_max_age_ms_v1().max(1);
            if updated == 0 {
                rejected.push(format!(
                    "permissioned_oracle_ref:missing_timestamp source={}",
                    fee_oracle_source_v1(store)
                ));
            } else if now_ms > updated.saturating_add(max_age_ms) {
                rejected.push(format!(
                    "permissioned_oracle_ref:stale source={} now={} oracle_updated={} max_age_ms={}",
                    fee_oracle_source_v1(store),
                    now_ms,
                    updated,
                    max_age_ms
                ));
            }
        }
    }

    let anchor = p_nav_ppm
        .or(if amm_twap_has_liquidity {
            p_amm_twap_ppm
        } else {
            None
        })
        .or(if p_prev_ppm > 0 {
            Some(p_prev_ppm)
        } else {
            None
        });
    let oracle_has_non_oracle_anchor = anchor.is_some();
    let max_deviation = NOV_PROTOCOL_CLEARING_MAX_SOURCE_DEVIATION_BPS_V1;
    candidates.retain(|(source, rate)| {
        if *source == "permissioned_oracle_ref" && !oracle_has_non_oracle_anchor {
            rejected.push("permissioned_oracle_ref:single_source_no_anchor".to_string());
            return false;
        }
        if *source == "treasury_nav" {
            return true;
        }
        if let Some(anchor_rate) = anchor {
            let deviation = rate_deviation_bps_v1(*rate, anchor_rate);
            if deviation > max_deviation {
                rejected.push(format!(
                    "{}:deviation_bps={} max_deviation_bps={}",
                    source, deviation, max_deviation
                ));
                return false;
            }
        }
        true
    });

    if candidates.is_empty() && p_prev_ppm == 0 {
        bail!(
            "{}",
            fee_clearing_reason_v1(
                "route_unavailable",
                format!("asset={normalized} has no protocol clearing source").as_str(),
            )
        );
    }

    let p_ref_ppm = median_rate_ppm_v1(candidates.iter().map(|(_, rate)| *rate).collect())
        .unwrap_or(p_prev_ppm);
    let p_epoch_ppm = clamp_epoch_rate_ppm_v1(
        p_ref_ppm,
        p_prev_ppm,
        NOV_PROTOCOL_CLEARING_MAX_EPOCH_DOWN_BPS_V1,
        NOV_PROTOCOL_CLEARING_MAX_EPOCH_UP_BPS_V1,
    );
    let reserve_haircut_bps = NOV_PROTOCOL_CLEARING_RESERVE_HAIRCUT_BPS_V1;
    let liquidity_haircut_bps = NOV_PROTOCOL_CLEARING_LIQUIDITY_HAIRCUT_BPS_V1;
    let volatility_haircut_bps = NOV_PROTOCOL_CLEARING_VOLATILITY_HAIRCUT_BPS_V1;
    let redemption_spread_bps = NOV_PROTOCOL_CLEARING_REDEMPTION_SPREAD_BPS_V1;
    let risk_surcharge_bps = NOV_PROTOCOL_CLEARING_RISK_SURCHARGE_BPS_V1;
    let p_pay_ppm = apply_down_bps_v1(
        apply_down_bps_v1(
            apply_down_bps_v1(p_epoch_ppm, reserve_haircut_bps),
            liquidity_haircut_bps,
        ),
        volatility_haircut_bps,
    )
    .max(1);
    let p_redeem_ppm = apply_up_bps_v1(
        apply_up_bps_v1(p_epoch_ppm, redemption_spread_bps),
        risk_surcharge_bps,
    )
    .max(p_epoch_ppm);
    let sources_used = candidates
        .iter()
        .map(|(source, _)| (*source).to_string())
        .collect::<Vec<_>>();
    let state = if sources_used.is_empty() {
        "constrained"
    } else if sources_used.len() == 1 || !rejected.is_empty() {
        "constrained"
    } else {
        "healthy"
    };
    let reason = if rejected.is_empty() && !sources_used.is_empty() {
        None
    } else if sources_used.is_empty() {
        Some("fallback_to_previous_epoch_price".to_string())
    } else {
        Some("one_or_more_sources_rejected".to_string())
    };

    Ok(NovProtocolClearingPriceV1 {
        asset: normalized,
        epoch,
        epoch_ms,
        p_prev_ppm,
        p_ref_ppm,
        p_epoch_ppm,
        p_pay_ppm,
        p_redeem_ppm,
        p_amm_twap_ppm,
        p_nav_ppm,
        p_oracle_ref_ppm,
        reserve_haircut_bps,
        liquidity_haircut_bps,
        volatility_haircut_bps,
        redemption_spread_bps,
        risk_surcharge_bps,
        max_epoch_up_bps: NOV_PROTOCOL_CLEARING_MAX_EPOCH_UP_BPS_V1,
        max_epoch_down_bps: NOV_PROTOCOL_CLEARING_MAX_EPOCH_DOWN_BPS_V1,
        max_source_deviation_bps: max_deviation,
        state: state.to_string(),
        sources_used,
        sources_rejected: rejected,
        reason,
        updated_unix_ms: now_ms,
    })
}

fn resolve_protocol_clearing_pay_rate_ppm_v1(
    store: &mut NovNativeExecutionStoreV1,
    asset: &str,
    now_ms: u128,
) -> Result<(u128, String, u128)> {
    let price = build_protocol_clearing_price_v1(store, asset, now_ms)?;
    let rate = price.p_pay_ppm;
    let source = format!("protocol_clearing_price:{}", price.state);
    let updated = price.updated_unix_ms;
    store
        .module_state
        .protocol_clearing_prices
        .insert(price.asset.clone(), price);
    Ok((rate, source, updated))
}

fn build_treasury_direct_source_v1(
    store: &NovNativeExecutionStoreV1,
    pay_asset: &str,
    clearing_rate_ppm: u128,
) -> Option<TreasuryDirectLiquidityV1> {
    let normalized = normalize_asset_symbol_v1(pay_asset);
    if normalized == "NOV" || clearing_rate_ppm == 0 {
        return None;
    }
    let runtime_available = store
        .module_state
        .clearing_nov_liquidity
        .get(normalized.as_str())
        .copied();
    let available_nov = match runtime_available {
        Some(value) => value,
        None if is_default_clearing_asset_enabled_v1(normalized.as_str()) => {
            default_clearing_liquidity_v1()
        }
        None => return None,
    };
    Some(TreasuryDirectLiquidityV1 {
        asset: normalized,
        available_liquidity_nov: available_nov,
        clearing_rate_ppm,
        quote_ttl_ms: execution_fee_quote_ttl_ms_v1() as u64,
    })
}

fn static_amm_sources_for_asset_v1(
    store: &NovNativeExecutionStoreV1,
    pay_asset: &str,
) -> Vec<StaticAmmPoolLiquidityV1> {
    let normalized = normalize_asset_symbol_v1(pay_asset);
    store
        .module_state
        .clearing_static_amm_pools
        .values()
        .filter(|pool| {
            pool.enabled
                && normalize_asset_symbol_v1(pool.asset_x.as_str()) == normalized
                && normalize_asset_symbol_v1(pool.asset_y.as_str()) == "NOV"
        })
        .map(|pool| StaticAmmPoolLiquidityV1 {
            pool_id: pool.pool_id.clone(),
            asset_x: normalize_asset_symbol_v1(pool.asset_x.as_str()),
            asset_y: normalize_asset_symbol_v1(pool.asset_y.as_str()),
            reserve_x: pool.reserve_x,
            reserve_y: pool.reserve_y,
            swap_fee_ppm: pool.swap_fee_ppm,
            quote_ttl_ms: execution_fee_quote_ttl_ms_v1() as u64,
        })
        .collect()
}

fn clearing_failure_to_reason_v1(
    code: NovClearingFailureCodeV1,
    pay_asset: &str,
    detail: impl Into<String>,
) -> String {
    fee_clearing_reason_v1(
        code.short_reason(),
        format!(
            "asset={} {}",
            normalize_asset_symbol_v1(pay_asset),
            detail.into()
        )
        .as_str(),
    )
}

struct NovSelectedClearingPersistInputV1<'a> {
    request: &'a NovExecutionFeeRequestV1,
    selected_expected_nov_out: u128,
    route_fee_ppm: u32,
    selection_reason: &'a str,
    candidates: &'a [NovClearingRouteQuoteV1],
    result: &'a crate::clearing_types::NovClearingResultV1,
    now_ms: u128,
}

fn apply_selected_clearing_result_v1(
    store: &mut NovNativeExecutionStoreV1,
    input: NovSelectedClearingPersistInputV1<'_>,
) {
    let request = input.request;
    let result = input.result;

    match result.route_source {
        NovRouteSourceV1::TreasuryDirect => {
            let normalized = normalize_asset_symbol_v1(result.pay_asset.as_str());
            let current = store
                .module_state
                .clearing_nov_liquidity
                .get(normalized.as_str())
                .copied()
                .unwrap_or_else(default_clearing_liquidity_v1);
            store.module_state.clearing_nov_liquidity.insert(
                normalized,
                current.saturating_sub(request.nov_needed.min(current)),
            );
        }
        NovRouteSourceV1::AmmPool => {
            if let Some(pool) = store
                .module_state
                .clearing_static_amm_pools
                .values_mut()
                .find(|pool| {
                    result
                        .route_id
                        .contains(format!(":{}:", pool.pool_id).as_str())
                })
            {
                pool.reserve_x = pool.reserve_x.saturating_add(result.pay_amount);
                pool.reserve_y = pool.reserve_y.saturating_sub(request.nov_needed);
            }
        }
        NovRouteSourceV1::StaticConfig => {}
    }

    store.module_state.last_clearing_candidates = input.candidates.to_vec();
    store.module_state.last_clearing_route = Some(NovLastClearingRouteV1 {
        route_id: result.route_id.clone(),
        route_source: result.route_source.as_str().to_string(),
        pay_asset: result.pay_asset.clone(),
        pay_amount: result.pay_amount,
        nov_amount_out: result.nov_amount_out,
        expected_nov_out: input.selected_expected_nov_out,
        route_fee_ppm: input.route_fee_ppm,
        cleared_at_ms: input.now_ms as u64,
        selection_reason: input.selection_reason.to_string(),
        candidate_route_count: input.candidates.len() as u32,
    });
}

fn quote_fail_v1<T>(
    store: &mut NovNativeExecutionStoreV1,
    asset: &str,
    code: &str,
    detail: impl Into<String>,
) -> Result<T> {
    let detail_text = detail.into();
    increment_quote_failure_v1(store, asset, code);
    let reason = fee_quote_reason_v1(code, detail_text.as_str());
    store.module_state.last_fee_quote_failure = Some(reason.clone());
    bail!(reason);
}

fn resolve_fee_rate_ppm_with_source_v1(
    store: &NovNativeExecutionStoreV1,
    asset: &str,
    now_ms: u128,
) -> Result<(u128, String, u128)> {
    let normalized = normalize_asset_symbol_v1(asset);
    if let Some(rate) = store
        .module_state
        .fee_oracle_rates_ppm
        .get(&normalized)
        .copied()
    {
        if rate == 0 {
            bail!(
                "{}",
                fee_quote_reason_v1("oracle_rate_zero", format!("asset={normalized}").as_str())
            );
        }
        let updated = store.module_state.fee_oracle_updated_unix_ms;
        let max_age_ms = execution_fee_oracle_max_age_ms_v1().max(1);
        if updated > 0 && now_ms > updated.saturating_add(max_age_ms) {
            bail!(
                "{}",
                fee_quote_reason_v1(
                    "oracle_stale",
                    format!(
                        "asset={} now={} oracle_updated={} max_age_ms={}",
                        normalized, now_ms, updated, max_age_ms
                    )
                    .as_str(),
                )
            );
        }
        let source = if store.module_state.fee_oracle_source.trim().is_empty() {
            "runtime_oracle".to_string()
        } else {
            store.module_state.fee_oracle_source.clone()
        };
        return Ok((rate, source, updated));
    }

    if let Some(rate) = configured_fee_rate_ppm_v1(normalized.as_str()) {
        return Ok((rate, "config_rate_ppm".to_string(), 0));
    }

    let default_rate = default_fee_rate_ppm_for_asset_v1(normalized.as_str());
    if default_rate == 0 {
        bail!(
            "{}",
            fee_quote_reason_v1(
                "unsupported_pay_asset",
                format!("asset={normalized}").as_str()
            )
        );
    }
    Ok((default_rate, "default_rate_ppm".to_string(), 0))
}

fn resolve_clearing_rate_ppm_with_source_v1(
    store: &NovNativeExecutionStoreV1,
    asset: &str,
    now_ms: u128,
) -> Result<(u128, String, u128)> {
    let normalized = normalize_asset_symbol_v1(asset);
    let protocol_price = build_protocol_clearing_price_v1(store, normalized.as_str(), now_ms);
    match protocol_price {
        Ok(price) => {
            return Ok((
                price.p_pay_ppm,
                format!("protocol_clearing_price:{}", price.state),
                price.updated_unix_ms,
            ));
        }
        Err(err)
            if store
                .module_state
                .fee_oracle_rates_ppm
                .contains_key(&normalized)
                && !store
                    .module_state
                    .clearing_rate_ppm
                    .contains_key(&normalized) =>
        {
            return Err(err);
        }
        Err(_) => {}
    }
    if let Some(rate) = store
        .module_state
        .clearing_rate_ppm
        .get(&normalized)
        .copied()
    {
        if rate == 0 {
            bail!(
                "{}",
                fee_clearing_reason_v1(
                    "route_unavailable",
                    format!("asset={normalized} clearing_rate_ppm is zero").as_str()
                )
            );
        }
        return Ok((rate, "clearing_route_rate_ppm".to_string(), 0));
    }
    resolve_fee_rate_ppm_with_source_v1(store, normalized.as_str(), now_ms)
}

fn resolve_fee_quote_rate_ppm_with_source_v1(
    store: &mut NovNativeExecutionStoreV1,
    asset: &str,
    now_ms: u128,
) -> Result<(u128, String, u128)> {
    let normalized = normalize_asset_symbol_v1(asset);
    if normalized == "NOV" {
        return Ok((NOV_FEE_RATE_PPM_NOV_V1, "direct_nov".to_string(), now_ms));
    }
    match resolve_protocol_clearing_pay_rate_ppm_v1(store, normalized.as_str(), now_ms) {
        Ok(value) => Ok(value),
        Err(err) => {
            if store
                .module_state
                .fee_oracle_rates_ppm
                .contains_key(&normalized)
            {
                return Err(err);
            }
            resolve_fee_rate_ppm_with_source_v1(store, normalized.as_str(), now_ms)
        }
    }
}

fn execution_fee_quote_ttl_ms_v1() -> u128 {
    env_u128_or_v1(
        NOV_NATIVE_FEE_QUOTE_TTL_MS_ENV,
        NOV_FEE_QUOTE_DEFAULT_TTL_MS_V1,
    )
}

fn estimate_execution_fee_nov_v1(request: &NovExecutionRequestV1) -> u128 {
    let method_cost = request.method.len() as u128;
    let args_cost = ((request.args.len() as u128).saturating_add(15)).saturating_div(16);
    let gas_cost = request
        .gas_like_limit
        .map(u128::from)
        .unwrap_or(21_000)
        .saturating_div(5_000);
    let target_cost = match &request.target {
        NovExecutionRequestTargetV1::NativeModule(_) => 8,
        NovExecutionRequestTargetV1::WasmApp(_) => 16,
        NovExecutionRequestTargetV1::Plugin(_) => 24,
    };
    20u128
        .saturating_add(method_cost.min(32))
        .saturating_add(args_cost.min(64))
        .saturating_add(gas_cost.min(64))
        .saturating_add(target_cost)
        .max(1)
}

fn build_fee_quote_id_v1(request: &NovExecutionRequestV1, now_ms: u128) -> String {
    let tx_hex = to_hex(&request.tx_hash);
    let prefix_len = tx_hex.len().min(12);
    format!("q-{}-{:x}", &tx_hex[..prefix_len], now_ms)
}

fn quote_fee_policy_from_execution_request_v1(
    request: &NovExecutionRequestV1,
    store: &mut NovNativeExecutionStoreV1,
    now_ms: u128,
) -> Result<NovFeeQuoteV1> {
    let pay_asset = normalize_asset_symbol_v1(request.fee_pay_asset.as_str());
    let nov_amount = estimate_execution_fee_nov_v1(request);
    let (rate_ppm, price_source, oracle_updated_at_unix_ms) =
        match resolve_fee_quote_rate_ppm_with_source_v1(store, pay_asset.as_str(), now_ms) {
            Ok(value) => value,
            Err(err) => {
                let reason_text = format!("{err}");
                if is_fee_quote_reason_v1(reason_text.as_str()) {
                    let code =
                        fee_reason_code_v1(reason_text.as_str(), NOV_FEE_FAILURE_QUOTE_PREFIX_V1)
                            .unwrap_or("rate_unavailable");
                    increment_quote_failure_v1(store, pay_asset.as_str(), code);
                    store.module_state.last_fee_quote_failure = Some(reason_text.clone());
                    bail!(reason_text);
                }
                return quote_fail_v1(
                    store,
                    pay_asset.as_str(),
                    "rate_unavailable",
                    format!("{err}"),
                );
            }
        };
    let quoted_pay_amount = ceil_div_u128_v1(
        nov_amount.saturating_mul(NOV_FEE_RATE_PPM_DENOMINATOR_V1),
        rate_ppm,
    )
    .max(1);
    let slippage_bps = request.fee_slippage_bps.min(10_000);
    let quoted_with_slippage = ceil_div_u128_v1(
        quoted_pay_amount.saturating_mul(10_000u128.saturating_add(slippage_bps as u128)),
        10_000,
    )
    .max(quoted_pay_amount);
    let max_pay_amount = if request.fee_max_pay_amount == 0 {
        quoted_with_slippage
    } else {
        request.fee_max_pay_amount
    };
    if quoted_with_slippage > max_pay_amount {
        return quote_fail_v1(
            store,
            pay_asset.as_str(),
            "max_pay_exceeded",
            format!(
                "required_with_slippage={} max_pay_amount={} pay_asset={}",
                quoted_with_slippage, max_pay_amount, pay_asset
            ),
        );
    }
    let ttl_ms = execution_fee_quote_ttl_ms_v1().max(1);
    let quote = NovFeeQuoteV1 {
        quote_id: build_fee_quote_id_v1(request, now_ms),
        pay_asset: pay_asset.clone(),
        nov_amount,
        quoted_pay_amount,
        quoted_pay_amount_with_slippage: quoted_with_slippage,
        max_pay_amount,
        slippage_bps,
        quoted_at_unix_ms: now_ms,
        expires_at_unix_ms: now_ms.saturating_add(ttl_ms),
        rate_ppm,
        oracle_updated_at_unix_ms,
        route: if pay_asset == "NOV" {
            "direct_nov".to_string()
        } else {
            format!("{}_to_nov", pay_asset.to_ascii_lowercase())
        },
        quote_contract: NOV_EXECUTION_FEE_QUOTE_CONTRACT_V1.to_string(),
        price_source,
    };
    store.module_state.last_fee_quote = Some(quote.clone());
    store.module_state.last_fee_quote_failure = None;
    Ok(quote)
}

fn increment_clearing_failure_v1(store: &mut NovNativeExecutionStoreV1, asset: &str, reason: &str) {
    let key = format!("{}:{}", normalize_asset_symbol_v1(asset), reason);
    let counter = store
        .module_state
        .clearing_failure_counts
        .entry(key)
        .or_insert(0);
    *counter = counter.saturating_add(1);
}

fn record_clearing_failure_v1(
    store: &mut NovNativeExecutionStoreV1,
    asset: &str,
    code: NovClearingFailureCodeV1,
    reason_text: &str,
    now_ms: u128,
) {
    increment_clearing_failure_v1(store, asset, code.short_reason());
    store.module_state.last_clearing_failure_code = code.as_error_code().to_string();
    store.module_state.last_clearing_failure_reason = reason_text.to_string();
    store.module_state.last_clearing_failure_unix_ms = now_ms;
}

fn clearing_fail_v1<T>(
    store: &mut NovNativeExecutionStoreV1,
    pay_asset: &str,
    code: NovClearingFailureCodeV1,
    detail: impl Into<String>,
    now_ms: u128,
) -> Result<T> {
    let reason = clearing_failure_to_reason_v1(code, pay_asset, detail.into());
    record_clearing_failure_v1(store, pay_asset, code, reason.as_str(), now_ms);
    bail!(reason);
}

fn record_user_flow_failure_reason_v1(
    store: &mut NovNativeExecutionStoreV1,
    pay_asset: &str,
    code: NovClearingFailureCodeV1,
    detail: impl Into<String>,
    now_ms: u128,
) -> String {
    let reason = clearing_failure_to_reason_v1(code, pay_asset, detail.into());
    record_clearing_failure_v1(store, pay_asset, code, reason.as_str(), now_ms);
    reason
}

struct NovClearingFailureJournalContextV1<'a> {
    tx_hash: &'a str,
    subject_meta: &'a NovExecutionSubjectMetaV1,
    settlement_policy: &'a NovTreasurySettlementPolicyV1,
    settlement_policy_contract_id: &'a str,
    settlement_threshold_state: &'a str,
}

fn clearing_fail_with_settlement_journal_v1<T>(
    store: &mut NovNativeExecutionStoreV1,
    quote: &NovFeeQuoteV1,
    context: &NovClearingFailureJournalContextV1<'_>,
    code: NovClearingFailureCodeV1,
    detail: impl Into<String>,
    now_ms: u128,
) -> Result<T> {
    let reason = clearing_failure_to_reason_v1(code, quote.pay_asset.as_str(), detail.into());
    record_clearing_failure_v1(
        store,
        quote.pay_asset.as_str(),
        code,
        reason.as_str(),
        now_ms,
    );
    let settlement_policy_source =
        normalize_policy_source_v1(context.settlement_policy.policy_source.as_str());
    append_treasury_settlement_journal_v1(
        store,
        NovTreasurySettlementJournalEntryV1 {
            seq: 0,
            unix_ms: now_ms,
            kind: "fee_settlement".to_string(),
            tx_hash: context.tx_hash.to_string(),
            account_id: context.subject_meta.account_id.clone(),
            fee_owner_account_id: context.subject_meta.fee_owner_account_id.clone(),
            nonce_owner_account_id: context.subject_meta.nonce_owner_account_id.clone(),
            key_algo: context.subject_meta.key_algo.clone(),
            execution_policy: context.subject_meta.execution_policy.clone(),
            policy_enforced: context.subject_meta.policy_enforced,
            policy_rejection_reason: context.subject_meta.policy_rejection_reason.clone(),
            source_asset: quote.pay_asset.clone(),
            source_amount: quote.quoted_pay_amount,
            settled_nov: 0,
            reserve_bucket_delta_nov: 0,
            fee_bucket_delta_nov: 0,
            risk_buffer_delta_nov: 0,
            route_ref: "clearing.rejected".to_string(),
            clearing_source: "clearing_policy".to_string(),
            clearing_rate_ppm: quote.rate_ppm,
            policy_version: context.settlement_policy.policy_version,
            policy_source: settlement_policy_source,
            policy_contract_id: context.settlement_policy_contract_id.to_string(),
            policy_threshold_state: context.settlement_threshold_state.to_string(),
            policy_constrained_strategy: context
                .settlement_policy
                .clearing_constrained_strategy
                .clone(),
            policy_event_state: "rejected".to_string(),
            status: "rejected".to_string(),
            reason: Some(reason.clone()),
        },
    );
    bail!(reason);
}

fn settle_fee_quote_into_treasury_v1(
    store: &mut NovNativeExecutionStoreV1,
    quote: &NovFeeQuoteV1,
    tx_hash: &str,
    subject_meta: &NovExecutionSubjectMetaV1,
    now_ms: u128,
) -> Result<NovSettledFeeV1> {
    refresh_clearing_daily_window_v1(store, now_ms);
    let settlement_policy = resolve_treasury_settlement_policy_v1(store);
    let settlement_policy_contract_id = treasury_policy_contract_id_v1(&settlement_policy);
    let settlement_gate_snapshot = clearing_policy_gate_snapshot_v1(store, &settlement_policy);
    let settlement_threshold_state = settlement_gate_snapshot
        .get("threshold_state")
        .and_then(|value| value.as_str())
        .unwrap_or("healthy")
        .to_string();
    let settlement_policy_source =
        normalize_policy_source_v1(settlement_policy.policy_source.as_str());
    let clearing_failure_context = NovClearingFailureJournalContextV1 {
        tx_hash,
        subject_meta,
        settlement_policy: &settlement_policy,
        settlement_policy_contract_id: settlement_policy_contract_id.as_str(),
        settlement_threshold_state: settlement_threshold_state.as_str(),
    };
    if settlement_policy.source.starts_with("default_fallback") {
        increment_settlement_failure_v1(store, "policy_fallback");
    }
    if settlement_policy.settlement_paused {
        increment_settlement_failure_v1(store, "settlement_paused");
        bail!(
            "{}",
            fee_settlement_reason_v1("settlement_paused", "treasury settlement is paused")
        );
    }
    if now_ms > quote.expires_at_unix_ms {
        return clearing_fail_v1(
            store,
            quote.pay_asset.as_str(),
            NovClearingFailureCodeV1::QuoteExpired,
            format!(
                "quote_id={} now={} expires_at={}",
                quote.quote_id, now_ms, quote.expires_at_unix_ms
            ),
            now_ms,
        );
    }
    if quote.pay_asset != "NOV" {
        if !settlement_policy.clearing_enabled {
            return clearing_fail_v1(
                store,
                quote.pay_asset.as_str(),
                NovClearingFailureCodeV1::ClearingDisabled,
                "clearing policy is disabled",
                now_ms,
            );
        }
        if settlement_policy.clearing_daily_nov_hard_limit > 0 {
            let projected_daily_nov = store
                .module_state
                .clearing_daily_nov_used
                .saturating_add(quote.nov_amount);
            if projected_daily_nov > settlement_policy.clearing_daily_nov_hard_limit {
                return clearing_fail_v1(
                    store,
                    quote.pay_asset.as_str(),
                    NovClearingFailureCodeV1::DailyVolumeExceeded,
                    format!(
                        "projected_daily_nov={} hard_limit={} day={}",
                        projected_daily_nov,
                        settlement_policy.clearing_daily_nov_hard_limit,
                        store.module_state.clearing_daily_window_day
                    ),
                    now_ms,
                );
            }
        }
        if settlement_policy.clearing_require_healthy_risk_buffer
            && store.module_state.treasury_risk_buffer_nov < settlement_policy.min_risk_buffer_nov
        {
            return clearing_fail_v1(
                store,
                quote.pay_asset.as_str(),
                NovClearingFailureCodeV1::RiskBufferBelowMin,
                format!(
                    "risk_buffer_nov={} min_required={}",
                    store.module_state.treasury_risk_buffer_nov,
                    settlement_policy.min_risk_buffer_nov
                ),
                now_ms,
            );
        }
        if let Some(reason) =
            reserve_proof_block_reason_for_asset_v1(store, quote.pay_asset.as_str(), now_ms)
        {
            return clearing_fail_v1(
                store,
                quote.pay_asset.as_str(),
                NovClearingFailureCodeV1::ReserveProofNotActive,
                reason,
                now_ms,
            );
        }
        if let Some(reason) =
            m2_bridge_risk_block_reason_for_asset_v1(store, quote.pay_asset.as_str(), now_ms)
        {
            return clearing_fail_v1(
                store,
                quote.pay_asset.as_str(),
                NovClearingFailureCodeV1::ReserveProofNotActive,
                reason,
                now_ms,
            );
        }
    }

    let (
        source_amount,
        current_required_pay_amount,
        clearing_route_ref,
        clearing_source,
        current_rate_ppm,
        clearing_price_source,
        route_expected_nov_out,
        route_fee_ppm,
        route_selection_reason,
        route_candidate_count,
    ) = if quote.pay_asset == "NOV" {
        store.module_state.last_clearing_candidates.clear();
        (
            quote.nov_amount,
            quote.nov_amount,
            "route:direct_nov".to_string(),
            "direct_wallet_nov".to_string(),
            NOV_FEE_RATE_PPM_NOV_V1,
            "direct_nov".to_string(),
            quote.nov_amount,
            0,
            "direct_nov".to_string(),
            1,
        )
    } else {
        let fee_request = NovExecutionFeeRequestV1 {
            tx_id: quote.quote_id.clone(),
            pay_asset: normalize_asset_symbol_v1(quote.pay_asset.as_str()),
            max_pay_amount: quote.max_pay_amount,
            nov_needed: quote.nov_amount,
            slippage_bps: quote.slippage_bps,
            quote_required_pay_amount: quote.quoted_pay_amount,
            quote_with_slippage_pay_amount: quote.quoted_pay_amount_with_slippage,
            quote_expires_at_ms: quote.expires_at_unix_ms as u64,
        };

        let mut sources: Vec<Box<dyn crate::liquidity_sources::NovLiquiditySourceV1>> = Vec::new();
        let mut treasury_rate_source: Option<String> = None;
        if let Ok((rate, source, _updated_at)) =
            resolve_clearing_rate_ppm_with_source_v1(store, quote.pay_asset.as_str(), now_ms)
        {
            treasury_rate_source = Some(source);
            if let Some(treasury_direct) =
                build_treasury_direct_source_v1(store, quote.pay_asset.as_str(), rate)
            {
                sources.push(Box::new(treasury_direct));
            }
        }
        for pool in static_amm_sources_for_asset_v1(store, quote.pay_asset.as_str()) {
            sources.push(Box::new(pool));
        }
        if sources.is_empty() {
            store.module_state.last_clearing_candidates.clear();
            return clearing_fail_v1(
                store,
                quote.pay_asset.as_str(),
                NovClearingFailureCodeV1::RouteUnavailable,
                format!("asset={} has no enabled clearing route", quote.pay_asset),
                now_ms,
            );
        }

        let now_ms_u64 = now_ms as u64;
        let mut router = NovClearingRouterImplV1::new(sources);
        let routes = router.quote_routes(&fee_request, now_ms_u64);
        if routes.is_empty() {
            store.module_state.last_clearing_candidates.clear();
            return clearing_fail_v1(
                store,
                quote.pay_asset.as_str(),
                NovClearingFailureCodeV1::RouteUnavailable,
                format!("asset={} has no quotable clearing route", quote.pay_asset),
                now_ms,
            );
        }
        let candidate_routes = routes.clone();
        let gate_snapshot = clearing_policy_gate_snapshot_v1(store, &settlement_policy);
        let threshold_state = gate_snapshot
            .get("threshold_state")
            .and_then(|value| value.as_str())
            .unwrap_or("healthy");
        let constrained_strategy = settlement_policy.clearing_constrained_strategy.as_str();
        let policy_constrained_routes = if threshold_state == "constrained" {
            match constrained_strategy {
                NOV_CLEARING_CONSTRAINED_STRATEGY_BLOCKED_V1 => {
                    store.module_state.last_clearing_candidates = candidate_routes.clone();
                    return clearing_fail_with_settlement_journal_v1(
                        store,
                        quote,
                        &clearing_failure_context,
                        NovClearingFailureCodeV1::ConstrainedBlocked,
                        format!(
                            "threshold_state=constrained strategy=blocked candidate_count={}",
                            candidate_routes.len()
                        ),
                        now_ms,
                    );
                }
                NOV_CLEARING_CONSTRAINED_STRATEGY_DAILY_VOLUME_ONLY_V1 => {
                    if quote.slippage_bps > settlement_policy.clearing_constrained_max_slippage_bps
                    {
                        store.module_state.last_clearing_candidates = candidate_routes.clone();
                        return clearing_fail_with_settlement_journal_v1(
                            store,
                            quote,
                            &clearing_failure_context,
                            NovClearingFailureCodeV1::SlippageExceeded,
                            format!(
                                "threshold_state=constrained strategy=daily_volume_only quote_slippage_bps={} constrained_max_slippage_bps={} candidate_count={}",
                                quote.slippage_bps,
                                settlement_policy.clearing_constrained_max_slippage_bps,
                                candidate_routes.len()
                            ),
                            now_ms,
                        );
                    }
                    if settlement_policy.clearing_daily_nov_hard_limit > 0 {
                        let projected_daily_nov = store
                            .module_state
                            .clearing_daily_nov_used
                            .saturating_add(quote.nov_amount);
                        let constrained_daily_nov_cap = settlement_policy
                            .clearing_daily_nov_hard_limit
                            .saturating_mul(u128::from(
                                settlement_policy.clearing_constrained_daily_usage_bps,
                            ))
                            .saturating_div(u128::from(NOV_TREASURY_SHARE_BPS_DENOMINATOR_V1))
                            .max(1);
                        if projected_daily_nov > constrained_daily_nov_cap {
                            store.module_state.last_clearing_candidates = candidate_routes.clone();
                            return clearing_fail_with_settlement_journal_v1(
                                store,
                                quote,
                                &clearing_failure_context,
                                NovClearingFailureCodeV1::ConstrainedDailyVolumeExceeded,
                                format!(
                                    "threshold_state=constrained strategy=daily_volume_only projected_daily_nov={} constrained_daily_nov_cap={} daily_hard_limit={} constrained_daily_usage_bps={} candidate_count={}",
                                    projected_daily_nov,
                                    constrained_daily_nov_cap,
                                    settlement_policy.clearing_daily_nov_hard_limit,
                                    settlement_policy.clearing_constrained_daily_usage_bps,
                                    candidate_routes.len()
                                ),
                                now_ms,
                            );
                        }
                    }
                    routes
                }
                NOV_CLEARING_CONSTRAINED_STRATEGY_TREASURY_DIRECT_ONLY_V1 => routes
                    .into_iter()
                    .filter(|route| route.source_id.source == NovRouteSourceV1::TreasuryDirect)
                    .collect::<Vec<_>>(),
                _ => routes,
            }
        } else {
            routes
        };
        if threshold_state == "constrained"
            && constrained_strategy == NOV_CLEARING_CONSTRAINED_STRATEGY_TREASURY_DIRECT_ONLY_V1
            && policy_constrained_routes.is_empty()
        {
            store.module_state.last_clearing_candidates = candidate_routes.clone();
            return clearing_fail_with_settlement_journal_v1(
                store,
                quote,
                &clearing_failure_context,
                NovClearingFailureCodeV1::ConstrainedRouteRestricted,
                format!(
                    "threshold_state=constrained strategy=treasury_direct_only candidate_count={}",
                    candidate_routes.len()
                ),
                now_ms,
            );
        }
        let considered_route_count = policy_constrained_routes.len();
        let mut viable_routes = Vec::new();
        let mut expired_count = 0usize;
        let mut slippage_count = 0usize;
        let mut insufficient_liquidity_count = 0usize;
        for route in policy_constrained_routes {
            if now_ms_u64 > route.expires_at_ms || now_ms_u64 > fee_request.quote_expires_at_ms {
                expired_count = expired_count.saturating_add(1);
                continue;
            }
            if route.pay_amount_in > fee_request.quote_with_slippage_pay_amount
                || route.pay_amount_in > fee_request.max_pay_amount
            {
                slippage_count = slippage_count.saturating_add(1);
                continue;
            }
            if route.liquidity_available < fee_request.nov_needed {
                insufficient_liquidity_count = insufficient_liquidity_count.saturating_add(1);
                continue;
            }
            viable_routes.push(route);
        }
        if viable_routes.is_empty() {
            store.module_state.last_clearing_candidates = candidate_routes.clone();
            let total = considered_route_count;
            if expired_count > 0 && expired_count == total {
                return clearing_fail_v1(
                    store,
                    quote.pay_asset.as_str(),
                    NovClearingFailureCodeV1::QuoteExpired,
                    format!(
                        "all_routes_expired route_count={} quote_id={}",
                        total, quote.quote_id
                    ),
                    now_ms,
                );
            }
            if insufficient_liquidity_count > 0 && insufficient_liquidity_count == total {
                return clearing_fail_v1(
                    store,
                    quote.pay_asset.as_str(),
                    NovClearingFailureCodeV1::InsufficientLiquidity,
                    format!(
                        "all_routes_insufficient_liquidity route_count={} nov_needed={}",
                        total, fee_request.nov_needed
                    ),
                    now_ms,
                );
            }
            if slippage_count > 0 {
                return clearing_fail_v1(
                    store,
                    quote.pay_asset.as_str(),
                    NovClearingFailureCodeV1::SlippageExceeded,
                    format!(
                        "all_routes_rejected_by_pay_constraints route_count={} slippage_filtered={}",
                        total, slippage_count
                    ),
                    now_ms,
                );
            }
            return clearing_fail_v1(
                store,
                quote.pay_asset.as_str(),
                NovClearingFailureCodeV1::RouteUnavailable,
                format!("asset={} has no viable clearing route", quote.pay_asset),
                now_ms,
            );
        }

        let selected = match router.select_best_route(viable_routes.as_slice()) {
            Ok(value) => value,
            Err(code) => {
                store.module_state.last_clearing_candidates = candidate_routes.clone();
                return clearing_fail_v1(
                    store,
                    quote.pay_asset.as_str(),
                    code,
                    "selection failed",
                    now_ms,
                );
            }
        };
        let selected_expected_nov_out = selected.route_quote.expected_nov_out;
        let selected_route_fee_ppm = selected.route_quote.fee_ppm;
        let selected_route_ref = selected.route_quote.route_id.clone();
        let selected_reason = selected.selection_reason.clone();
        let candidate_count = candidate_routes.len();
        let result = match router.execute_selected_route(&selected, &fee_request, now_ms_u64) {
            Ok(value) => value,
            Err(code) => {
                store.module_state.last_clearing_candidates = candidate_routes.clone();
                return clearing_fail_v1(
                    store,
                    quote.pay_asset.as_str(),
                    code,
                    format!("route_ref={selected_route_ref}"),
                    now_ms,
                );
            }
        };
        if result.nov_amount_out < quote.nov_amount {
            return clearing_fail_v1(
                store,
                quote.pay_asset.as_str(),
                NovClearingFailureCodeV1::InsufficientLiquidity,
                format!(
                    "asset={} required_nov={} actual_nov_out={} route_ref={}",
                    quote.pay_asset, quote.nov_amount, result.nov_amount_out, result.route_id
                ),
                now_ms,
            );
        }

        let settlement_input =
            settle_clearing_result_into_treasury_v1(quote.quote_id.clone(), &result);
        let current_foreign_reserve = store
            .module_state
            .treasury_reserves
            .get(quote.pay_asset.as_str())
            .copied()
            .unwrap_or(0);
        let projected_foreign_reserve_after =
            current_foreign_reserve.saturating_add(settlement_input.pay_amount);
        if let Some(reason) = reserve_proof_capacity_block_reason_v1(
            store,
            quote.pay_asset.as_str(),
            projected_foreign_reserve_after,
            now_ms,
        ) {
            return clearing_fail_v1(
                store,
                quote.pay_asset.as_str(),
                NovClearingFailureCodeV1::ReserveProofCapacityExceeded,
                reason,
                now_ms,
            );
        }
        if is_native_m2_fee_asset_symbol_v1(quote.pay_asset.as_str())
            && settlement_input.pay_amount > 0
        {
            if let Err(err) = debit_native_account_asset_balance_v1(
                store,
                subject_meta.fee_owner_account_id.as_str(),
                quote.pay_asset.as_str(),
                settlement_input.pay_amount,
            ) {
                return clearing_fail_v1(
                    store,
                    quote.pay_asset.as_str(),
                    NovClearingFailureCodeV1::InsufficientUserBalance,
                    format!("m2_fee_asset_debit_failed: {err}"),
                    now_ms,
                );
            }
        }
        apply_selected_clearing_result_v1(
            store,
            NovSelectedClearingPersistInputV1 {
                request: &fee_request,
                selected_expected_nov_out,
                route_fee_ppm: selected_route_fee_ppm,
                selection_reason: selected_reason.as_str(),
                candidates: candidate_routes.as_slice(),
                result: &result,
                now_ms,
            },
        );

        let effective_rate_ppm = if settlement_input.pay_amount == 0 {
            0
        } else {
            quote
                .nov_amount
                .saturating_mul(NOV_FEE_RATE_PPM_DENOMINATOR_V1)
                .saturating_div(settlement_input.pay_amount)
        };
        (
            settlement_input.pay_amount,
            settlement_input.pay_amount,
            settlement_input.route_id,
            settlement_input.route_source,
            effective_rate_ppm,
            if result.route_source == NovRouteSourceV1::TreasuryDirect {
                format!(
                    "router=multi_route selection={} source={} rate_source={}",
                    selected_reason,
                    result.route_source.as_str(),
                    treasury_rate_source
                        .clone()
                        .unwrap_or_else(|| "runtime_oracle".to_string())
                )
            } else {
                format!(
                    "router=multi_route selection={} source={}",
                    selected_reason,
                    result.route_source.as_str(),
                )
            },
            selected_expected_nov_out,
            selected_route_fee_ppm,
            selected_reason,
            candidate_count as u32,
        )
    };

    let nov_entry = store
        .module_state
        .treasury_reserves
        .entry("NOV".to_string())
        .or_insert(0);
    *nov_entry = nov_entry.saturating_add(quote.nov_amount);
    if quote.pay_asset != "NOV" {
        let foreign_entry = store
            .module_state
            .treasury_reserves
            .entry(quote.pay_asset.clone())
            .or_insert(0);
        *foreign_entry = foreign_entry.saturating_add(source_amount);
        store.module_state.clearing_daily_nov_used = store
            .module_state
            .clearing_daily_nov_used
            .saturating_add(quote.nov_amount);
    }
    store.module_state.treasury_settled_nov_total = store
        .module_state
        .treasury_settled_nov_total
        .saturating_add(quote.nov_amount);
    store.module_state.treasury_settlements =
        store.module_state.treasury_settlements.saturating_add(1);
    let settled_by_asset = store
        .module_state
        .treasury_settled_by_asset
        .entry(quote.pay_asset.clone())
        .or_insert(0);
    *settled_by_asset = settled_by_asset.saturating_add(source_amount);
    let (reserve_delta, fee_delta, risk_buffer_delta) =
        apply_treasury_settlement_split_v1(store, quote.nov_amount, &settlement_policy);
    append_treasury_settlement_journal_v1(
        store,
        NovTreasurySettlementJournalEntryV1 {
            seq: 0,
            unix_ms: now_ms,
            kind: "fee_settlement".to_string(),
            tx_hash: tx_hash.to_string(),
            account_id: subject_meta.account_id.clone(),
            fee_owner_account_id: subject_meta.fee_owner_account_id.clone(),
            nonce_owner_account_id: subject_meta.nonce_owner_account_id.clone(),
            key_algo: subject_meta.key_algo.clone(),
            execution_policy: subject_meta.execution_policy.clone(),
            policy_enforced: subject_meta.policy_enforced,
            policy_rejection_reason: subject_meta.policy_rejection_reason.clone(),
            source_asset: quote.pay_asset.clone(),
            source_amount,
            settled_nov: quote.nov_amount,
            reserve_bucket_delta_nov: saturating_u128_to_i128_v1(reserve_delta),
            fee_bucket_delta_nov: saturating_u128_to_i128_v1(fee_delta),
            risk_buffer_delta_nov: saturating_u128_to_i128_v1(risk_buffer_delta),
            route_ref: clearing_route_ref.clone(),
            clearing_source: clearing_source.clone(),
            clearing_rate_ppm: current_rate_ppm,
            policy_version: settlement_policy.policy_version,
            policy_source: settlement_policy_source.clone(),
            policy_contract_id: settlement_policy_contract_id.clone(),
            policy_threshold_state: settlement_threshold_state.clone(),
            policy_constrained_strategy: settlement_policy.clearing_constrained_strategy.clone(),
            policy_event_state: "settled".to_string(),
            status: "applied".to_string(),
            reason: None,
        },
    );

    Ok(NovSettledFeeV1 {
        nov_amount: quote.nov_amount,
        source_asset: quote.pay_asset.clone(),
        source_amount,
        required_source_amount: current_required_pay_amount,
        quote_expires_at_unix_ms: quote.expires_at_unix_ms,
        clearing_route_ref: clearing_route_ref.clone(),
        clearing_source: clearing_source.clone(),
        clearing_rate_ppm: current_rate_ppm,
        route_expected_nov_out,
        route_fee_ppm,
        route_selection_reason,
        route_candidate_count,
        route: if quote.pay_asset == "NOV" {
            "direct_nov".to_string()
        } else {
            "quote_and_route_clear_to_nov".to_string()
        },
        fee_contract: NOV_EXECUTION_FEE_CLASSIFICATION_CONTRACT_V1.to_string(),
        quote_id: quote.quote_id.clone(),
        quote_contract: quote.quote_contract.clone(),
        clearing_contract: NOV_EXECUTION_FEE_CLEARING_CONTRACT_V1.to_string(),
        price_source: format!(
            "quote={} clearing={}",
            quote.price_source, clearing_price_source
        ),
        policy_contract_id: settlement_policy_contract_id,
        policy_version: settlement_policy.policy_version,
        policy_source: settlement_policy_source,
        policy_threshold_state: settlement_threshold_state,
        policy_constrained_strategy: settlement_policy.clearing_constrained_strategy.clone(),
    })
}

fn settle_fee_policy_from_execution_request_v1(
    request: &NovExecutionRequestV1,
    subject_meta: &NovExecutionSubjectMetaV1,
    store: &mut NovNativeExecutionStoreV1,
    now_ms: u128,
) -> Result<NovSettledFeeV1> {
    let quote = quote_fee_policy_from_execution_request_v1(request, store, now_ms)?;
    let tx_hash = to_hex(&request.tx_hash);
    settle_fee_quote_into_treasury_v1(store, &quote, tx_hash.as_str(), subject_meta, now_ms)
}

fn unresolved_settled_fee_v1(request: &NovExecutionRequestV1) -> NovSettledFeeV1 {
    NovSettledFeeV1 {
        nov_amount: 0,
        source_asset: normalize_asset_symbol_v1(request.fee_pay_asset.as_str()),
        source_amount: 0,
        required_source_amount: 0,
        quote_expires_at_unix_ms: 0,
        clearing_route_ref: String::new(),
        clearing_source: String::new(),
        clearing_rate_ppm: 0,
        route_expected_nov_out: 0,
        route_fee_ppm: 0,
        route_selection_reason: String::new(),
        route_candidate_count: 0,
        route: "settlement_failed".to_string(),
        fee_contract: NOV_EXECUTION_FEE_CLASSIFICATION_CONTRACT_V1.to_string(),
        quote_id: String::new(),
        quote_contract: NOV_EXECUTION_FEE_QUOTE_CONTRACT_V1.to_string(),
        clearing_contract: NOV_EXECUTION_FEE_CLEARING_CONTRACT_V1.to_string(),
        price_source: "unresolved".to_string(),
        policy_contract_id: String::new(),
        policy_version: 0,
        policy_source: String::new(),
        policy_threshold_state: String::new(),
        policy_constrained_strategy: String::new(),
    }
}

fn execution_target_label_v1(target: &NovExecutionRequestTargetV1) -> String {
    match target {
        NovExecutionRequestTargetV1::NativeModule(name) => format!("native:{name}"),
        NovExecutionRequestTargetV1::WasmApp(app) => format!("wasm:{app}"),
        NovExecutionRequestTargetV1::Plugin(plugin) => format!("plugin:{plugin}"),
    }
}

fn route_meta_from_settled_fee_v1(settled_fee: &NovSettledFeeV1) -> Option<NovReceiptRouteMetaV1> {
    if settled_fee.clearing_route_ref.trim().is_empty() {
        return None;
    }
    Some(NovReceiptRouteMetaV1 {
        route_id: settled_fee.clearing_route_ref.clone(),
        route_source: settled_fee.clearing_source.clone(),
        expected_nov_out: settled_fee.route_expected_nov_out,
        route_fee_ppm: settled_fee.route_fee_ppm,
        selection_reason: settled_fee.route_selection_reason.clone(),
        candidate_route_count: settled_fee.route_candidate_count,
    })
}

fn policy_meta_from_settled_fee_v1(
    settled_fee: &NovSettledFeeV1,
) -> Option<NovReceiptPolicyMetaV1> {
    if settled_fee.policy_contract_id.trim().is_empty() {
        return None;
    }
    Some(NovReceiptPolicyMetaV1 {
        policy_contract_id: settled_fee.policy_contract_id.clone(),
        policy_version: settled_fee.policy_version,
        policy_source: settled_fee.policy_source.clone(),
        policy_threshold_state: settled_fee.policy_threshold_state.clone(),
        policy_constrained_strategy: settled_fee.policy_constrained_strategy.clone(),
    })
}

fn build_failed_native_receipt_v1(
    request: &NovExecutionRequestV1,
    settled_fee: &NovSettledFeeV1,
    subject_meta: &NovExecutionSubjectMetaV1,
    module: String,
    method: String,
    reason: String,
) -> NovNativeExecutionReceiptV1 {
    NovNativeExecutionReceiptV1 {
        tx_hash: to_hex(&request.tx_hash),
        status: false,
        target: execution_target_label_v1(&request.target),
        module,
        method,
        account_id: subject_meta.account_id.clone(),
        fee_owner_account_id: subject_meta.fee_owner_account_id.clone(),
        nonce_owner_account_id: subject_meta.nonce_owner_account_id.clone(),
        key_algo: subject_meta.key_algo.clone(),
        execution_policy: subject_meta.execution_policy.clone(),
        policy_enforced: subject_meta.policy_enforced,
        policy_rejection_reason: subject_meta.policy_rejection_reason.clone(),
        settled_fee_nov: settled_fee.nov_amount,
        paid_asset: settled_fee.source_asset.clone(),
        paid_amount: settled_fee.source_amount,
        logs: Vec::new(),
        failure_reason: Some(reason),
        fee_contract: settled_fee.fee_contract.clone(),
        fee_route: settled_fee.route.clone(),
        fee_quote_id: settled_fee.quote_id.clone(),
        fee_quote_contract: settled_fee.quote_contract.clone(),
        fee_clearing_contract: settled_fee.clearing_contract.clone(),
        fee_price_source: settled_fee.price_source.clone(),
        fee_quote_required_pay_amount: settled_fee.required_source_amount,
        fee_quote_expires_at_unix_ms: settled_fee.quote_expires_at_unix_ms,
        fee_clearing_route_ref: settled_fee.clearing_route_ref.clone(),
        fee_clearing_source: settled_fee.clearing_source.clone(),
        fee_clearing_rate_ppm: settled_fee.clearing_rate_ppm,
        route_meta: route_meta_from_settled_fee_v1(settled_fee),
        policy_meta: policy_meta_from_settled_fee_v1(settled_fee),
        aoem_semantic_ingress: None,
        aoem_semantic_commit: None,
    }
}

fn build_success_native_receipt_v1(
    request: &NovExecutionRequestV1,
    settled_fee: &NovSettledFeeV1,
    subject_meta: &NovExecutionSubjectMetaV1,
    module: &str,
    method: &str,
    logs: Vec<NovNativeExecutionLogV1>,
) -> NovNativeExecutionReceiptV1 {
    NovNativeExecutionReceiptV1 {
        tx_hash: to_hex(&request.tx_hash),
        status: true,
        target: execution_target_label_v1(&request.target),
        module: module.to_string(),
        method: method.to_string(),
        account_id: subject_meta.account_id.clone(),
        fee_owner_account_id: subject_meta.fee_owner_account_id.clone(),
        nonce_owner_account_id: subject_meta.nonce_owner_account_id.clone(),
        key_algo: subject_meta.key_algo.clone(),
        execution_policy: subject_meta.execution_policy.clone(),
        policy_enforced: subject_meta.policy_enforced,
        policy_rejection_reason: subject_meta.policy_rejection_reason.clone(),
        settled_fee_nov: settled_fee.nov_amount,
        paid_asset: settled_fee.source_asset.clone(),
        paid_amount: settled_fee.source_amount,
        logs,
        failure_reason: None,
        fee_contract: settled_fee.fee_contract.clone(),
        fee_route: settled_fee.route.clone(),
        fee_quote_id: settled_fee.quote_id.clone(),
        fee_quote_contract: settled_fee.quote_contract.clone(),
        fee_clearing_contract: settled_fee.clearing_contract.clone(),
        fee_price_source: settled_fee.price_source.clone(),
        fee_quote_required_pay_amount: settled_fee.required_source_amount,
        fee_quote_expires_at_unix_ms: settled_fee.quote_expires_at_unix_ms,
        fee_clearing_route_ref: settled_fee.clearing_route_ref.clone(),
        fee_clearing_source: settled_fee.clearing_source.clone(),
        fee_clearing_rate_ppm: settled_fee.clearing_rate_ppm,
        route_meta: route_meta_from_settled_fee_v1(settled_fee),
        policy_meta: policy_meta_from_settled_fee_v1(settled_fee),
        aoem_semantic_ingress: None,
        aoem_semantic_commit: None,
    }
}

fn constrained_daily_nov_cap_v1(policy: &NovTreasurySettlementPolicyV1) -> u128 {
    if policy.clearing_daily_nov_hard_limit == 0 {
        0
    } else {
        policy
            .clearing_daily_nov_hard_limit
            .saturating_mul(u128::from(policy.clearing_constrained_daily_usage_bps))
            .saturating_div(u128::from(NOV_TREASURY_SHARE_BPS_DENOMINATOR_V1))
            .max(1)
    }
}

fn enforce_user_market_risk_gate_v1(
    store: &mut NovNativeExecutionStoreV1,
    pay_asset: &str,
    projected_nov_usage: u128,
    requested_slippage_bps: u32,
    requires_amm_route: bool,
    now_ms: u128,
) -> Result<()> {
    refresh_clearing_daily_window_v1(store, now_ms);
    let policy = resolve_treasury_settlement_policy_v1(store);
    if !policy.clearing_enabled {
        bail!(
            "{}",
            record_user_flow_failure_reason_v1(
                store,
                pay_asset,
                NovClearingFailureCodeV1::ClearingDisabled,
                "user execution path is disabled by clearing policy",
                now_ms,
            )
        );
    }
    if policy.clearing_daily_nov_hard_limit > 0 {
        let projected_daily_nov = store
            .module_state
            .clearing_daily_nov_used
            .saturating_add(projected_nov_usage);
        if projected_daily_nov > policy.clearing_daily_nov_hard_limit {
            bail!(
                "{}",
                record_user_flow_failure_reason_v1(
                    store,
                    pay_asset,
                    NovClearingFailureCodeV1::DailyVolumeExceeded,
                    format!(
                        "projected_daily_nov={} hard_limit={} day={}",
                        projected_daily_nov,
                        policy.clearing_daily_nov_hard_limit,
                        store.module_state.clearing_daily_window_day
                    ),
                    now_ms,
                )
            );
        }
    }
    if policy.clearing_require_healthy_risk_buffer
        && store.module_state.treasury_risk_buffer_nov < policy.min_risk_buffer_nov
    {
        bail!(
            "{}",
            record_user_flow_failure_reason_v1(
                store,
                pay_asset,
                NovClearingFailureCodeV1::RiskBufferBelowMin,
                format!(
                    "risk_buffer_nov={} min_required={}",
                    store.module_state.treasury_risk_buffer_nov, policy.min_risk_buffer_nov
                ),
                now_ms,
            )
        );
    }
    let threshold_state = clearing_policy_gate_snapshot_v1(store, &policy)
        .get("threshold_state")
        .and_then(|value| value.as_str())
        .unwrap_or("healthy")
        .to_string();
    if threshold_state == "constrained" {
        match policy.clearing_constrained_strategy.as_str() {
            NOV_CLEARING_CONSTRAINED_STRATEGY_BLOCKED_V1 => bail!(
                "{}",
                record_user_flow_failure_reason_v1(
                    store,
                    pay_asset,
                    NovClearingFailureCodeV1::ConstrainedBlocked,
                    "threshold_state=constrained strategy=blocked",
                    now_ms,
                )
            ),
            NOV_CLEARING_CONSTRAINED_STRATEGY_DAILY_VOLUME_ONLY_V1 => {
                if requested_slippage_bps > policy.clearing_constrained_max_slippage_bps {
                    bail!(
                        "{}",
                        record_user_flow_failure_reason_v1(
                            store,
                            pay_asset,
                            NovClearingFailureCodeV1::SlippageExceeded,
                            format!(
                                "threshold_state=constrained strategy=daily_volume_only requested_slippage_bps={} constrained_max_slippage_bps={}",
                                requested_slippage_bps,
                                policy.clearing_constrained_max_slippage_bps
                            ),
                            now_ms,
                        )
                    );
                }
                let constrained_cap = constrained_daily_nov_cap_v1(&policy);
                if constrained_cap > 0 {
                    let projected_daily_nov = store
                        .module_state
                        .clearing_daily_nov_used
                        .saturating_add(projected_nov_usage);
                    if projected_daily_nov > constrained_cap {
                        bail!(
                            "{}",
                            record_user_flow_failure_reason_v1(
                                store,
                                pay_asset,
                                NovClearingFailureCodeV1::ConstrainedDailyVolumeExceeded,
                                format!(
                                    "threshold_state=constrained strategy=daily_volume_only projected_daily_nov={} constrained_daily_nov_cap={} daily_hard_limit={} constrained_daily_usage_bps={}",
                                    projected_daily_nov,
                                    constrained_cap,
                                    policy.clearing_daily_nov_hard_limit,
                                    policy.clearing_constrained_daily_usage_bps
                                ),
                                now_ms,
                            )
                        );
                    }
                }
            }
            NOV_CLEARING_CONSTRAINED_STRATEGY_TREASURY_DIRECT_ONLY_V1 if requires_amm_route => {
                bail!(
                    "{}",
                    record_user_flow_failure_reason_v1(
                        store,
                        pay_asset,
                        NovClearingFailureCodeV1::ConstrainedRouteRestricted,
                        "threshold_state=constrained strategy=treasury_direct_only",
                        now_ms,
                    )
                );
            }
            _ => {}
        }
    }
    Ok(())
}

fn amm_output_for_exact_input_v1(
    reserve_in: u128,
    reserve_out: u128,
    amount_in: u128,
    fee_ppm: u32,
) -> Option<u128> {
    if reserve_in == 0 || reserve_out == 0 || amount_in == 0 {
        return None;
    }
    let fee_den = 1_000_000u128;
    let amount_in_after_fee =
        amount_in.saturating_mul(fee_den.saturating_sub(u128::from(fee_ppm))) / fee_den;
    if amount_in_after_fee == 0 {
        return None;
    }
    let numerator = amount_in_after_fee.saturating_mul(reserve_out);
    let denominator = reserve_in.saturating_add(amount_in_after_fee);
    if denominator == 0 {
        return None;
    }
    Some(numerator / denominator)
}

fn dispatch_treasury_redeem_v1(
    request: &NovExecutionRequestV1,
    settled_fee: &NovSettledFeeV1,
    subject_meta: &NovExecutionSubjectMetaV1,
    store: &mut NovNativeExecutionStoreV1,
    args_json: &serde_json::Value,
    method_label: &str,
) -> NovNativeExecutionReceiptV1 {
    let policy = resolve_treasury_settlement_policy_v1(store);
    let policy_contract_id = treasury_policy_contract_id_v1(&policy);
    let policy_threshold_state = clearing_policy_gate_snapshot_v1(store, &policy)
        .get("threshold_state")
        .and_then(|value| value.as_str())
        .unwrap_or("healthy")
        .to_string();
    let policy_source = normalize_policy_source_v1(policy.policy_source.as_str());
    if policy.redeem_paused {
        increment_settlement_failure_v1(store, "redeem_paused");
        return build_failed_native_receipt_v1(
            request,
            settled_fee,
            subject_meta,
            "treasury".to_string(),
            method_label.to_string(),
            fee_settlement_reason_v1("redeem_paused", "treasury redeem path is paused"),
        );
    }
    let asset = args_json
        .get("asset")
        .or_else(|| args_json.get("asset_out"))
        .and_then(|value| value.as_str())
        .map(normalize_asset_symbol_v1)
        .unwrap_or_else(|| "NOV".to_string());
    let requested_asset_amount = args_json
        .get("amount")
        .and_then(parse_u128_from_json_value_v1);
    let requested_nov_amount = args_json
        .get("nov_amount")
        .and_then(parse_u128_from_json_value_v1);
    let amount = requested_asset_amount.or(requested_nov_amount).unwrap_or(0);
    if amount == 0 {
        increment_settlement_failure_v1(store, "invalid_redeem_amount");
        return build_failed_native_receipt_v1(
            request,
            settled_fee,
            subject_meta,
            "treasury".to_string(),
            method_label.to_string(),
            fee_settlement_reason_v1("invalid_redeem_amount", "amount must be > 0"),
        );
    }

    if asset != "NOV" {
        if requested_asset_amount.is_some() {
            increment_settlement_failure_v1(store, "redeem_requires_nov_amount");
            return build_failed_native_receipt_v1(
                request,
                settled_fee,
                subject_meta,
                "treasury".to_string(),
                method_label.to_string(),
                fee_settlement_reason_v1(
                    "redeem_requires_nov_amount",
                    format!(
                        "asset={} non-NOV reserve redeem must use asset_out + nov_amount and P_redeem",
                        asset
                    )
                    .as_str(),
                ),
            );
        }
        if let Some(reason) =
            reserve_proof_block_reason_for_asset_v1(store, asset.as_str(), now_unix_millis_v1())
        {
            increment_settlement_failure_v1(store, "reserve_proof_not_active");
            return build_failed_native_receipt_v1(
                request,
                settled_fee,
                subject_meta,
                "treasury".to_string(),
                method_label.to_string(),
                fee_settlement_reason_v1("reserve_proof_not_active", reason.as_str()),
            );
        }
        if let Some(reason) =
            m2_bridge_risk_block_reason_for_asset_v1(store, asset.as_str(), now_unix_millis_v1())
        {
            increment_settlement_failure_v1(store, "m2_bridge_risk_blocked");
            return build_failed_native_receipt_v1(
                request,
                settled_fee,
                subject_meta,
                "treasury".to_string(),
                method_label.to_string(),
                fee_settlement_reason_v1("m2_bridge_risk_blocked", reason.as_str()),
            );
        }
    }

    if asset == "NOV" {
        let available = store.module_state.treasury_reserve_bucket_nov;
        if available < amount {
            increment_settlement_failure_v1(store, "insufficient_reserve");
            return build_failed_native_receipt_v1(
                request,
                settled_fee,
                subject_meta,
                "treasury".to_string(),
                method_label.to_string(),
                fee_settlement_reason_v1(
                    "insufficient_reserve",
                    format!(
                        "asset=NOV requested={} available_reserve_bucket={}",
                        amount, available
                    )
                    .as_str(),
                ),
            );
        }
        let reserve_bucket_after = available.saturating_sub(amount);
        if reserve_bucket_after < policy.min_reserve_bucket_nov {
            increment_settlement_failure_v1(store, "reserve_bucket_below_min");
            return build_failed_native_receipt_v1(
                request,
                settled_fee,
                subject_meta,
                "treasury".to_string(),
                method_label.to_string(),
                fee_settlement_reason_v1(
                    "reserve_bucket_below_min",
                    format!(
                        "requested={} reserve_bucket_after={} min_reserve_bucket_nov={}",
                        amount, reserve_bucket_after, policy.min_reserve_bucket_nov
                    )
                    .as_str(),
                ),
            );
        }
        let available_total = store
            .module_state
            .treasury_reserves
            .get("NOV")
            .copied()
            .unwrap_or(0);
        if available_total < amount {
            increment_settlement_failure_v1(store, "insufficient_total_reserve");
            return build_failed_native_receipt_v1(
                request,
                settled_fee,
                subject_meta,
                "treasury".to_string(),
                method_label.to_string(),
                fee_settlement_reason_v1(
                    "insufficient_total_reserve",
                    format!(
                        "asset=NOV requested={} available_total={}",
                        amount, available_total
                    )
                    .as_str(),
                ),
            );
        }
        let total_reserve_after = {
            let nov_entry = store
                .module_state
                .treasury_reserves
                .entry("NOV".to_string())
                .or_insert(0);
            *nov_entry = nov_entry.saturating_sub(amount);
            *nov_entry
        };
        store.module_state.treasury_reserve_bucket_nov = store
            .module_state
            .treasury_reserve_bucket_nov
            .saturating_sub(amount);
        let reserve_bucket_after = store.module_state.treasury_reserve_bucket_nov;
        store.module_state.treasury_redeemed_nov_total = store
            .module_state
            .treasury_redeemed_nov_total
            .saturating_add(amount);
        let redeemed_nov = store
            .module_state
            .treasury_redeemed_by_asset
            .entry("NOV".to_string())
            .or_insert(0);
        *redeemed_nov = redeemed_nov.saturating_add(amount);
        append_treasury_settlement_journal_v1(
            store,
            NovTreasurySettlementJournalEntryV1 {
                seq: 0,
                unix_ms: now_unix_millis_v1(),
                kind: "reserve_redeem".to_string(),
                tx_hash: to_hex(&request.tx_hash),
                account_id: subject_meta.account_id.clone(),
                fee_owner_account_id: subject_meta.fee_owner_account_id.clone(),
                nonce_owner_account_id: subject_meta.nonce_owner_account_id.clone(),
                key_algo: subject_meta.key_algo.clone(),
                execution_policy: subject_meta.execution_policy.clone(),
                policy_enforced: subject_meta.policy_enforced,
                policy_rejection_reason: subject_meta.policy_rejection_reason.clone(),
                source_asset: "NOV".to_string(),
                source_amount: amount,
                settled_nov: amount,
                reserve_bucket_delta_nov: -saturating_u128_to_i128_v1(amount),
                fee_bucket_delta_nov: 0,
                risk_buffer_delta_nov: 0,
                route_ref: "treasury.reserve_redeem".to_string(),
                clearing_source: "treasury".to_string(),
                clearing_rate_ppm: 0,
                policy_version: policy.policy_version,
                policy_source: policy_source.clone(),
                policy_contract_id: policy_contract_id.clone(),
                policy_threshold_state: policy_threshold_state.clone(),
                policy_constrained_strategy: policy.clearing_constrained_strategy.clone(),
                policy_event_state: "redeemed".to_string(),
                status: "applied".to_string(),
                reason: None,
            },
        );
        let caller = subject_meta.account_id.as_str();
        let caller_balance_after =
            credit_native_account_asset_balance_v1(store, caller, "NOV", amount);
        let log = NovNativeExecutionLogV1 {
            module: "treasury".to_string(),
            method: method_label.to_string(),
            event: "treasury.reserve_redeemed".to_string(),
            data: serde_json::json!({
                "asset": "NOV",
                "amount": amount,
                "account_id": caller,
                "caller_balance_after": caller_balance_after,
                "reserve_bucket_after": reserve_bucket_after,
                "total_reserve_after": total_reserve_after,
                "risk_buffer_nov": store.module_state.treasury_risk_buffer_nov,
                "risk_buffer_min_nov": policy.min_risk_buffer_nov,
            }),
        };
        return build_success_native_receipt_v1(
            request,
            settled_fee,
            subject_meta,
            "treasury",
            method_label,
            vec![log],
        );
    }

    let protocol_redeem_price = if requested_asset_amount.is_none() {
        requested_nov_amount
            .filter(|nov_amount| *nov_amount > 0)
            .and_then(|_| {
                build_protocol_clearing_price_v1(store, asset.as_str(), now_unix_millis_v1()).ok()
            })
    } else {
        None
    };
    let (asset_out_amount, nov_redeem_amount, redeem_rate_ppm, redeem_price_source) =
        if let Some(price) = protocol_redeem_price {
            let nov_amount = requested_nov_amount.unwrap_or(0);
            let asset_out = nov_amount
                .saturating_mul(NOV_FEE_RATE_PPM_DENOMINATOR_V1)
                .saturating_div(price.p_redeem_ppm)
                .max(1);
            (
                asset_out,
                nov_amount,
                price.p_redeem_ppm,
                format!("protocol_clearing_redeem:{}", price.state),
            )
        } else {
            (amount, 0, 0, "legacy_asset_amount".to_string())
        };

    let caller = subject_meta.account_id.as_str();
    if nov_redeem_amount > 0 {
        if let Err(err) =
            debit_native_account_asset_balance_v1(store, caller, "NOV", nov_redeem_amount)
        {
            increment_settlement_failure_v1(store, "insufficient_user_nov");
            return build_failed_native_receipt_v1(
                request,
                settled_fee,
                subject_meta,
                "treasury".to_string(),
                method_label.to_string(),
                fee_settlement_reason_v1(
                    "insufficient_user_nov",
                    format!("redeem requires NOV debit: {err}").as_str(),
                ),
            );
        }
    }

    let available = store
        .module_state
        .treasury_reserves
        .get(asset.as_str())
        .copied()
        .unwrap_or(0);
    if available < asset_out_amount {
        increment_settlement_failure_v1(store, "insufficient_reserve");
        if nov_redeem_amount > 0 {
            let _ = credit_native_account_asset_balance_v1(store, caller, "NOV", nov_redeem_amount);
        }
        return build_failed_native_receipt_v1(
            request,
            settled_fee,
            subject_meta,
            "treasury".to_string(),
            method_label.to_string(),
            fee_settlement_reason_v1(
                "insufficient_reserve",
                format!(
                    "asset={} requested={} available={}",
                    asset, asset_out_amount, available
                )
                .as_str(),
            ),
        );
    }
    let projected_reserve_after = available.saturating_sub(asset_out_amount);
    if let Some(reason) = reserve_proof_capacity_block_reason_v1(
        store,
        asset.as_str(),
        projected_reserve_after,
        now_unix_millis_v1(),
    ) {
        increment_settlement_failure_v1(store, "reserve_proof_capacity_exceeded");
        if nov_redeem_amount > 0 {
            let _ = credit_native_account_asset_balance_v1(store, caller, "NOV", nov_redeem_amount);
        }
        return build_failed_native_receipt_v1(
            request,
            settled_fee,
            subject_meta,
            "treasury".to_string(),
            method_label.to_string(),
            fee_settlement_reason_v1("reserve_proof_capacity_exceeded", reason.as_str()),
        );
    }
    let reserve_after = {
        let entry = store
            .module_state
            .treasury_reserves
            .entry(asset.clone())
            .or_insert(0);
        *entry = entry.saturating_sub(asset_out_amount);
        *entry
    };
    let redeemed_entry = store
        .module_state
        .treasury_redeemed_by_asset
        .entry(asset.clone())
        .or_insert(0);
    *redeemed_entry = redeemed_entry.saturating_add(asset_out_amount);
    if nov_redeem_amount > 0 {
        store.module_state.treasury_redeemed_nov_total = store
            .module_state
            .treasury_redeemed_nov_total
            .saturating_add(nov_redeem_amount);
    }
    append_treasury_settlement_journal_v1(
        store,
        NovTreasurySettlementJournalEntryV1 {
            seq: 0,
            unix_ms: now_unix_millis_v1(),
            kind: "reserve_redeem".to_string(),
            tx_hash: to_hex(&request.tx_hash),
            account_id: subject_meta.account_id.clone(),
            fee_owner_account_id: subject_meta.fee_owner_account_id.clone(),
            nonce_owner_account_id: subject_meta.nonce_owner_account_id.clone(),
            key_algo: subject_meta.key_algo.clone(),
            execution_policy: subject_meta.execution_policy.clone(),
            policy_enforced: subject_meta.policy_enforced,
            policy_rejection_reason: subject_meta.policy_rejection_reason.clone(),
            source_asset: asset.clone(),
            source_amount: asset_out_amount,
            settled_nov: nov_redeem_amount,
            reserve_bucket_delta_nov: 0,
            fee_bucket_delta_nov: 0,
            risk_buffer_delta_nov: 0,
            route_ref: "treasury.reserve_redeem".to_string(),
            clearing_source: redeem_price_source.clone(),
            clearing_rate_ppm: redeem_rate_ppm,
            policy_version: policy.policy_version,
            policy_source: policy_source.clone(),
            policy_contract_id: policy_contract_id.clone(),
            policy_threshold_state: policy_threshold_state.clone(),
            policy_constrained_strategy: policy.clearing_constrained_strategy.clone(),
            policy_event_state: "redeemed".to_string(),
            status: "applied".to_string(),
            reason: None,
        },
    );
    let caller_balance_after =
        credit_native_account_asset_balance_v1(store, caller, asset.as_str(), asset_out_amount);
    let log = NovNativeExecutionLogV1 {
        module: "treasury".to_string(),
        method: method_label.to_string(),
        event: "treasury.reserve_redeemed".to_string(),
        data: serde_json::json!({
            "asset": asset,
            "amount": asset_out_amount,
            "nov_redeem_amount": nov_redeem_amount,
            "redeem_rate_ppm": redeem_rate_ppm,
            "redeem_price_source": redeem_price_source,
            "account_id": caller,
            "caller_balance_after": caller_balance_after,
            "reserve_after": reserve_after,
        }),
    };
    build_success_native_receipt_v1(
        request,
        settled_fee,
        subject_meta,
        "treasury",
        method_label,
        vec![log],
    )
}

fn dispatch_amm_swap_exact_in_v1(
    request: &NovExecutionRequestV1,
    settled_fee: &NovSettledFeeV1,
    subject_meta: &NovExecutionSubjectMetaV1,
    store: &mut NovNativeExecutionStoreV1,
    args_json: &serde_json::Value,
) -> NovNativeExecutionReceiptV1 {
    let asset_in = args_json
        .get("asset_in")
        .and_then(|value| value.as_str())
        .map(normalize_asset_symbol_v1)
        .unwrap_or_default();
    let asset_out = args_json
        .get("asset_out")
        .and_then(|value| value.as_str())
        .map(normalize_asset_symbol_v1)
        .unwrap_or_default();
    let amount_in = args_json
        .get("amount_in")
        .and_then(parse_u128_from_json_value_v1)
        .unwrap_or(0);
    let min_amount_out = args_json
        .get("min_amount_out")
        .and_then(parse_u128_from_json_value_v1)
        .unwrap_or(0);
    let requested_slippage_bps = args_json
        .get("slippage_bps")
        .and_then(parse_u128_from_json_value_v1)
        .map(|value| value as u32)
        .unwrap_or(100);
    if asset_in.is_empty() || asset_out.is_empty() || asset_in == asset_out || amount_in == 0 {
        return build_failed_native_receipt_v1(
            request,
            settled_fee,
            subject_meta,
            "amm".to_string(),
            "swap_exact_in".to_string(),
            "amm.invalid_args: asset_in/asset_out/amount_in are required".to_string(),
        );
    }

    let best_pool = store
        .module_state
        .clearing_static_amm_pools
        .values()
        .filter(|pool| pool.enabled)
        .filter_map(|pool| {
            let pool_x = normalize_asset_symbol_v1(pool.asset_x.as_str());
            let pool_y = normalize_asset_symbol_v1(pool.asset_y.as_str());
            if pool_x == asset_in && pool_y == asset_out {
                let amount_out = amm_output_for_exact_input_v1(
                    pool.reserve_x,
                    pool.reserve_y,
                    amount_in,
                    pool.swap_fee_ppm,
                )?;
                Some((pool.pool_id.clone(), amount_out, pool.reserve_y, false))
            } else if pool_y == asset_in && pool_x == asset_out {
                let amount_out = amm_output_for_exact_input_v1(
                    pool.reserve_y,
                    pool.reserve_x,
                    amount_in,
                    pool.swap_fee_ppm,
                )?;
                Some((pool.pool_id.clone(), amount_out, pool.reserve_x, true))
            } else {
                None
            }
        })
        .max_by_key(|(pool_id, amount_out, reserve_out, reversed)| {
            (*amount_out, *reserve_out, !*reversed, pool_id.clone())
        });
    let (selected_pool_id, amount_out, reserve_out_before, reversed) = match best_pool {
        Some(value) => value,
        None => {
            return build_failed_native_receipt_v1(
                request,
                settled_fee,
                subject_meta,
                "amm".to_string(),
                "swap_exact_in".to_string(),
                "amm.route_unavailable: no enabled single-hop pool for pair".to_string(),
            )
        }
    };
    if amount_out == 0 {
        return build_failed_native_receipt_v1(
            request,
            settled_fee,
            subject_meta,
            "amm".to_string(),
            "swap_exact_in".to_string(),
            "amm.route_unavailable: quoted output is zero".to_string(),
        );
    }
    if amount_out < min_amount_out {
        return build_failed_native_receipt_v1(
            request,
            settled_fee,
            subject_meta,
            "amm".to_string(),
            "swap_exact_in".to_string(),
            format!(
                "amm.slippage_exceeded: amount_out={} min_amount_out={}",
                amount_out, min_amount_out
            ),
        );
    }
    let nov_leg_amount = if asset_out == "NOV" {
        amount_out
    } else if asset_in == "NOV" {
        amount_in
    } else {
        return build_failed_native_receipt_v1(
            request,
            settled_fee,
            subject_meta,
            "amm".to_string(),
            "swap_exact_in".to_string(),
            "amm.route_unavailable: minimal path currently supports NOV pairs only".to_string(),
        );
    };
    if let Err(err) = enforce_user_market_risk_gate_v1(
        store,
        asset_in.as_str(),
        nov_leg_amount,
        requested_slippage_bps,
        true,
        now_unix_millis_v1(),
    ) {
        return build_failed_native_receipt_v1(
            request,
            settled_fee,
            subject_meta,
            "amm".to_string(),
            "swap_exact_in".to_string(),
            err.to_string(),
        );
    }
    let caller = subject_meta.account_id.as_str();
    let caller_asset_in_before = native_account_asset_balance_v1(store, caller, asset_in.as_str());
    if let Err(err) =
        debit_native_account_asset_balance_v1(store, caller, asset_in.as_str(), amount_in)
    {
        return build_failed_native_receipt_v1(
            request,
            settled_fee,
            subject_meta,
            "amm".to_string(),
            "swap_exact_in".to_string(),
            format!("amm.insufficient_user_balance: {err}"),
        );
    }
    let caller_asset_out_after =
        credit_native_account_asset_balance_v1(store, caller, asset_out.as_str(), amount_out);
    if let Some(pool) = store
        .module_state
        .clearing_static_amm_pools
        .get_mut(selected_pool_id.as_str())
    {
        if reversed {
            pool.reserve_y = pool.reserve_y.saturating_add(amount_in);
            pool.reserve_x = pool.reserve_x.saturating_sub(amount_out);
        } else {
            pool.reserve_x = pool.reserve_x.saturating_add(amount_in);
            pool.reserve_y = pool.reserve_y.saturating_sub(amount_out);
        }
    }
    store.module_state.clearing_daily_nov_used = store
        .module_state
        .clearing_daily_nov_used
        .saturating_add(nov_leg_amount);
    let pool = store
        .module_state
        .clearing_static_amm_pools
        .get(selected_pool_id.as_str())
        .cloned();
    let log = NovNativeExecutionLogV1 {
        module: "amm".to_string(),
        method: "swap_exact_in".to_string(),
        event: "amm.swap_exact_in.applied".to_string(),
        data: serde_json::json!({
            "account_id": caller,
            "pool_id": selected_pool_id,
            "asset_in": asset_in,
            "asset_out": asset_out,
            "amount_in": amount_in,
            "amount_out": amount_out,
            "min_amount_out": min_amount_out,
            "requested_slippage_bps": requested_slippage_bps,
            "nov_leg_amount": nov_leg_amount,
            "caller_asset_in_before": caller_asset_in_before,
            "caller_asset_in_after": native_account_asset_balance_v1(store, caller, asset_in.as_str()),
            "caller_asset_out_after": caller_asset_out_after,
            "pool_reserve_out_before": reserve_out_before,
            "pool_reserve_x_after": pool.as_ref().map(|value| value.reserve_x).unwrap_or(0),
            "pool_reserve_y_after": pool.as_ref().map(|value| value.reserve_y).unwrap_or(0),
            "swap_fee_ppm": pool.as_ref().map(|value| value.swap_fee_ppm).unwrap_or(0),
        }),
    };
    build_success_native_receipt_v1(
        request,
        settled_fee,
        subject_meta,
        "amm",
        "swap_exact_in",
        vec![log],
    )
}

fn dispatch_credit_engine_open_vault_v1(
    request: &NovExecutionRequestV1,
    settled_fee: &NovSettledFeeV1,
    subject_meta: &NovExecutionSubjectMetaV1,
    store: &mut NovNativeExecutionStoreV1,
    args_json: &serde_json::Value,
) -> NovNativeExecutionReceiptV1 {
    let collateral_asset = args_json
        .get("collateral_asset")
        .and_then(|value| value.as_str())
        .map(normalize_asset_symbol_v1)
        .unwrap_or_default();
    let collateral_amount = args_json
        .get("collateral_amount")
        .and_then(parse_u128_from_json_value_v1)
        .unwrap_or(0);
    let debt_asset = args_json
        .get("debt_asset")
        .and_then(|value| value.as_str())
        .map(normalize_asset_symbol_v1)
        .unwrap_or_else(|| "NUSD".to_string());
    let mint_amount = args_json
        .get("mint_amount")
        .and_then(parse_u128_from_json_value_v1)
        .unwrap_or(0);
    if collateral_asset.is_empty() || collateral_amount == 0 {
        return build_failed_native_receipt_v1(
            request,
            settled_fee,
            subject_meta,
            "credit_engine".to_string(),
            "open_vault".to_string(),
            "credit_engine.invalid_args: collateral_asset/collateral_amount are required"
                .to_string(),
        );
    }
    if mint_amount > 0 {
        let required_collateral = mint_amount
            .saturating_mul(u128::from(NOV_CREDIT_ENGINE_MIN_COLLATERAL_RATIO_BPS_V1))
            .saturating_add(9_999)
            / 10_000;
        if collateral_amount < required_collateral {
            return build_failed_native_receipt_v1(
                request,
                settled_fee,
                subject_meta,
                "credit_engine".to_string(),
                "open_vault".to_string(),
                format!(
                    "credit_engine.collateral_ratio_below_min: collateral_amount={} mint_amount={} min_ratio_bps={}",
                    collateral_amount,
                    mint_amount,
                    NOV_CREDIT_ENGINE_MIN_COLLATERAL_RATIO_BPS_V1
                ),
            );
        }
        if let Err(err) = enforce_user_market_risk_gate_v1(
            store,
            debt_asset.as_str(),
            mint_amount,
            0,
            false,
            now_unix_millis_v1(),
        ) {
            return build_failed_native_receipt_v1(
                request,
                settled_fee,
                subject_meta,
                "credit_engine".to_string(),
                "open_vault".to_string(),
                err.to_string(),
            );
        }
    }
    let caller = subject_meta.account_id.as_str();
    if let Err(err) = debit_native_account_asset_balance_v1(
        store,
        caller,
        collateral_asset.as_str(),
        collateral_amount,
    ) {
        return build_failed_native_receipt_v1(
            request,
            settled_fee,
            subject_meta,
            "credit_engine".to_string(),
            "open_vault".to_string(),
            format!("credit_engine.insufficient_user_balance: {err}"),
        );
    }
    let vault_id = store.module_state.next_credit_vault_id.saturating_add(1);
    store.module_state.next_credit_vault_id = vault_id;
    let vault = NovCreditVaultStateV1 {
        vault_id,
        owner: caller.to_string(),
        collateral_asset: collateral_asset.clone(),
        collateral_amount,
        debt_asset: debt_asset.clone(),
        debt_amount: mint_amount,
        min_collateral_ratio_bps: NOV_CREDIT_ENGINE_MIN_COLLATERAL_RATIO_BPS_V1,
        opened_at_unix_ms: now_unix_millis_v1(),
    };
    store
        .module_state
        .credit_vaults
        .insert(vault_id, vault.clone());
    let caller_debt_balance_after = if mint_amount > 0 {
        store.module_state.clearing_daily_nov_used = store
            .module_state
            .clearing_daily_nov_used
            .saturating_add(mint_amount);
        credit_native_account_asset_balance_v1(store, caller, debt_asset.as_str(), mint_amount)
    } else {
        native_account_asset_balance_v1(store, caller, debt_asset.as_str())
    };
    let log = NovNativeExecutionLogV1 {
        module: "credit_engine".to_string(),
        method: "open_vault".to_string(),
        event: "credit_engine.vault_opened".to_string(),
        data: serde_json::json!({
            "account_id": caller,
            "vault_id": vault_id,
            "collateral_asset": collateral_asset,
            "collateral_amount": collateral_amount,
            "debt_asset": debt_asset,
            "mint_amount": mint_amount,
            "min_collateral_ratio_bps": NOV_CREDIT_ENGINE_MIN_COLLATERAL_RATIO_BPS_V1,
            "caller_collateral_after": native_account_asset_balance_v1(store, vault.owner.as_str(), vault.collateral_asset.as_str()),
            "caller_debt_asset_after": caller_debt_balance_after,
        }),
    };
    build_success_native_receipt_v1(
        request,
        settled_fee,
        subject_meta,
        "credit_engine",
        "open_vault",
        vec![log],
    )
}

fn dispatch_native_module_execute_v1(
    request: &NovExecutionRequestV1,
    settled_fee: &NovSettledFeeV1,
    subject_meta: &NovExecutionSubjectMetaV1,
    store: &mut NovNativeExecutionStoreV1,
) -> NovNativeExecutionReceiptV1 {
    let (module_name, method_name) = match &request.target {
        NovExecutionRequestTargetV1::NativeModule(module) => {
            (module.trim().to_ascii_lowercase(), request.method.clone())
        }
        _ => {
            return build_failed_native_receipt_v1(
                request,
                settled_fee,
                subject_meta,
                "unsupported".to_string(),
                request.method.clone(),
                "target is not a native module".to_string(),
            );
        }
    };

    if nov_native_module_methods_v1(module_name.as_str()).is_none() {
        return build_failed_native_receipt_v1(
            request,
            settled_fee,
            subject_meta,
            module_name,
            method_name,
            "unknown native module".to_string(),
        );
    }

    let args_json = decode_execute_args_json_v1(request.args.as_slice())
        .unwrap_or_else(|| fallback_execute_args_value_v1(request.args.as_slice()));
    match (module_name.as_str(), request.method.as_str()) {
        ("treasury", "deposit_reserve") => {
            let asset = args_json
                .get("asset")
                .and_then(|value| value.as_str())
                .map(normalize_asset_symbol_v1)
                .unwrap_or_else(|| normalize_asset_symbol_v1(request.fee_pay_asset.as_str()));
            let amount = args_json
                .get("amount")
                .and_then(parse_u128_from_json_value_v1)
                .unwrap_or_else(|| request.fee_max_pay_amount.max(1));
            let now_ms = now_unix_millis_v1();
            if asset != "NOV" {
                if let Some(reason) =
                    reserve_proof_block_reason_for_asset_v1(store, asset.as_str(), now_ms)
                {
                    increment_settlement_failure_v1(store, "reserve_proof_not_active");
                    return build_failed_native_receipt_v1(
                        request,
                        settled_fee,
                        subject_meta,
                        "treasury".to_string(),
                        "deposit_reserve".to_string(),
                        fee_settlement_reason_v1("reserve_proof_not_active", reason.as_str()),
                    );
                }
                let current_reserve = store
                    .module_state
                    .treasury_reserves
                    .get(asset.as_str())
                    .copied()
                    .unwrap_or(0);
                let projected_reserve_after = current_reserve.saturating_add(amount);
                if let Some(reason) = reserve_proof_capacity_block_reason_v1(
                    store,
                    asset.as_str(),
                    projected_reserve_after,
                    now_ms,
                ) {
                    increment_settlement_failure_v1(store, "reserve_proof_capacity_exceeded");
                    return build_failed_native_receipt_v1(
                        request,
                        settled_fee,
                        subject_meta,
                        "treasury".to_string(),
                        "deposit_reserve".to_string(),
                        fee_settlement_reason_v1(
                            "reserve_proof_capacity_exceeded",
                            reason.as_str(),
                        ),
                    );
                }
            }
            let reserve_entry = store
                .module_state
                .treasury_reserves
                .entry(asset.clone())
                .or_insert(0);
            *reserve_entry = reserve_entry.saturating_add(amount);
            let log = NovNativeExecutionLogV1 {
                module: "treasury".to_string(),
                method: "deposit_reserve".to_string(),
                event: "treasury.reserve_deposited".to_string(),
                data: serde_json::json!({
                    "asset": asset,
                    "amount": amount,
                    "reserve_after": *reserve_entry,
                    "fee_route": settled_fee.route,
                }),
            };
            build_success_native_receipt_v1(
                request,
                settled_fee,
                subject_meta,
                "treasury",
                "deposit_reserve",
                vec![log],
            )
        }
        ("treasury", "redeem") => dispatch_treasury_redeem_v1(
            request,
            settled_fee,
            subject_meta,
            store,
            &args_json,
            "redeem",
        ),
        ("treasury", "redeem_reserve") => dispatch_treasury_redeem_v1(
            request,
            settled_fee,
            subject_meta,
            store,
            &args_json,
            "redeem_reserve",
        ),
        ("amm", "swap_exact_in") => {
            dispatch_amm_swap_exact_in_v1(request, settled_fee, subject_meta, store, &args_json)
        }
        ("credit_engine", "open_vault") => dispatch_credit_engine_open_vault_v1(
            request,
            settled_fee,
            subject_meta,
            store,
            &args_json,
        ),
        ("governance", "submit_proposal") => {
            let proposal_payload = args_json.clone();
            let proposal_id = store
                .module_state
                .next_governance_proposal_id
                .saturating_add(1);
            store.module_state.next_governance_proposal_id = proposal_id;
            store
                .module_state
                .governance_proposals
                .insert(proposal_id, proposal_payload.clone());
            let log = NovNativeExecutionLogV1 {
                module: "governance".to_string(),
                method: "submit_proposal".to_string(),
                event: "governance.proposal_submitted".to_string(),
                data: serde_json::json!({
                    "proposal_id": proposal_id,
                    "payload": proposal_payload,
                }),
            };
            apply_subject_meta_to_receipt_v1(
                NovNativeExecutionReceiptV1 {
                    tx_hash: to_hex(&request.tx_hash),
                    status: true,
                    target: execution_target_label_v1(&request.target),
                    module: "governance".to_string(),
                    method: "submit_proposal".to_string(),
                    settled_fee_nov: settled_fee.nov_amount,
                    paid_asset: settled_fee.source_asset.clone(),
                    paid_amount: settled_fee.source_amount,
                    logs: vec![log],
                    failure_reason: None,
                    fee_contract: settled_fee.fee_contract.clone(),
                    fee_route: settled_fee.route.clone(),
                    fee_quote_id: settled_fee.quote_id.clone(),
                    fee_quote_contract: settled_fee.quote_contract.clone(),
                    fee_clearing_contract: settled_fee.clearing_contract.clone(),
                    fee_price_source: settled_fee.price_source.clone(),
                    fee_quote_required_pay_amount: settled_fee.required_source_amount,
                    fee_quote_expires_at_unix_ms: settled_fee.quote_expires_at_unix_ms,
                    fee_clearing_route_ref: settled_fee.clearing_route_ref.clone(),
                    fee_clearing_source: settled_fee.clearing_source.clone(),
                    fee_clearing_rate_ppm: settled_fee.clearing_rate_ppm,
                    route_meta: route_meta_from_settled_fee_v1(settled_fee),
                    policy_meta: policy_meta_from_settled_fee_v1(settled_fee),
                    account_id: String::new(),
                    fee_owner_account_id: String::new(),
                    nonce_owner_account_id: String::new(),
                    key_algo: String::new(),
                    execution_policy: String::new(),
                    policy_enforced: false,
                    policy_rejection_reason: None,
                    aoem_semantic_ingress: None,
                    aoem_semantic_commit: None,
                },
                subject_meta,
            )
        }
        ("governance", "set_reserve_proof") => {
            if let Err(err) = governance_execute_authorized_v1(request, &args_json) {
                return build_failed_native_receipt_v1(
                    request,
                    settled_fee,
                    subject_meta,
                    "governance".to_string(),
                    "set_reserve_proof".to_string(),
                    format!("governance.policy.authority_denied: {err}"),
                );
            }
            let asset = args_json
                .get("asset")
                .and_then(|value| value.as_str())
                .map(normalize_asset_symbol_v1)
                .unwrap_or_default();
            if asset.is_empty() {
                return build_failed_native_receipt_v1(
                    request,
                    settled_fee,
                    subject_meta,
                    "governance".to_string(),
                    "set_reserve_proof".to_string(),
                    "governance.reserve_proof.asset_required".to_string(),
                );
            }
            let proof_digest = args_json
                .get("proof_digest")
                .or_else(|| args_json.get("digest"))
                .and_then(|value| value.as_str())
                .map(|value| value.trim().to_ascii_lowercase())
                .unwrap_or_default();
            if proof_digest.is_empty() {
                return build_failed_native_receipt_v1(
                    request,
                    settled_fee,
                    subject_meta,
                    "governance".to_string(),
                    "set_reserve_proof".to_string(),
                    "governance.reserve_proof.digest_required".to_string(),
                );
            }
            let reserve_amount = args_json
                .get("reserve_amount")
                .or_else(|| args_json.get("amount"))
                .and_then(parse_u128_from_json_value_v1)
                .unwrap_or_else(|| {
                    store
                        .module_state
                        .treasury_reserves
                        .get(asset.as_str())
                        .copied()
                        .unwrap_or(0)
                });
            let proof_type = args_json
                .get("proof_type")
                .and_then(|value| value.as_str())
                .map(|value| value.trim().to_ascii_lowercase())
                .filter(|value| !value.is_empty())
                .unwrap_or_else(|| "manual_attestation_v1".to_string());
            let proof_source = args_json
                .get("proof_source")
                .or_else(|| args_json.get("source"))
                .and_then(|value| value.as_str())
                .map(normalize_policy_source_v1)
                .unwrap_or_else(|| "governance_path".to_string());
            let proof_reference = args_json
                .get("proof_reference")
                .or_else(|| args_json.get("reference"))
                .and_then(|value| value.as_str())
                .map(|value| value.trim().to_string())
                .unwrap_or_default();
            let now_ms = now_unix_millis_v1();
            let observed_at_unix_ms = args_json
                .get("observed_at_unix_ms")
                .and_then(parse_u128_from_json_value_v1)
                .unwrap_or(now_ms);
            let expires_at_unix_ms = args_json
                .get("expires_at_unix_ms")
                .and_then(parse_u128_from_json_value_v1)
                .unwrap_or(0);
            let status = normalize_reserve_proof_status_v1(
                args_json
                    .get("status")
                    .and_then(|value| value.as_str())
                    .unwrap_or("active"),
            );
            let policy_version = args_json
                .get("policy_version")
                .and_then(parse_u128_from_json_value_v1)
                .map(|value| value as u32)
                .unwrap_or_else(|| {
                    store
                        .module_state
                        .treasury_policy_version
                        .max(NOV_TREASURY_POLICY_VERSION_DEFAULT_V1)
                });
            let proof = NovTreasuryReserveProofV1 {
                asset: asset.clone(),
                reserve_amount,
                proof_type,
                proof_digest,
                proof_source,
                proof_reference,
                observed_at_unix_ms,
                expires_at_unix_ms,
                policy_version,
                policy_source: "governance_path".to_string(),
                status: status.clone(),
                automated_verification: false,
                verification_mode: "manual_governance_attestation".to_string(),
            };
            let effective_status = reserve_proof_effective_status_v1(&proof, now_ms);
            store
                .module_state
                .treasury_reserve_proofs
                .insert(asset.clone(), proof.clone());

            let log = NovNativeExecutionLogV1 {
                module: "governance".to_string(),
                method: "set_reserve_proof".to_string(),
                event: "governance.treasury_reserve_proof_set".to_string(),
                data: serde_json::json!({
                    "asset": asset,
                    "reserve_amount": reserve_amount,
                    "proof_type": proof.proof_type,
                    "proof_digest": proof.proof_digest,
                    "proof_source": proof.proof_source,
                    "proof_reference": proof.proof_reference,
                    "observed_at_unix_ms": proof.observed_at_unix_ms,
                    "expires_at_unix_ms": proof.expires_at_unix_ms,
                    "policy_version": proof.policy_version,
                    "policy_source": proof.policy_source,
                    "status": status,
                    "effective_status": effective_status,
                    "automated_verification": false,
                    "verification_mode": "manual_governance_attestation",
                    "claims": {
                        "real_external_reserve_auto_verified": false,
                        "nov_mint_authorized": false,
                        "external_redemption_authorized": false
                    }
                }),
            };
            build_success_native_receipt_v1(
                request,
                settled_fee,
                subject_meta,
                "governance",
                "set_reserve_proof",
                vec![log],
            )
        }
        ("governance", "apply_treasury_policy") => {
            if let Err(err) = governance_execute_authorized_v1(request, &args_json) {
                return build_failed_native_receipt_v1(
                    request,
                    settled_fee,
                    subject_meta,
                    "governance".to_string(),
                    "apply_treasury_policy".to_string(),
                    format!("governance.policy.authority_denied: {err}"),
                );
            }
            let active_policy = resolve_treasury_settlement_policy_v1(store);
            let reserve_share_bps = args_json
                .get("reserve_allocation_bps")
                .or_else(|| args_json.get("reserve_share_bps"))
                .and_then(parse_u128_from_json_value_v1)
                .map(|value| value as u32)
                .unwrap_or(active_policy.reserve_share_bps);
            let fee_share_bps = args_json
                .get("fee_allocation_bps")
                .or_else(|| args_json.get("fee_share_bps"))
                .and_then(parse_u128_from_json_value_v1)
                .map(|value| value as u32)
                .unwrap_or(active_policy.fee_share_bps);
            let risk_buffer_share_bps = args_json
                .get("risk_buffer_allocation_bps")
                .or_else(|| args_json.get("risk_buffer_share_bps"))
                .and_then(parse_u128_from_json_value_v1)
                .map(|value| value as u32)
                .unwrap_or(active_policy.risk_buffer_share_bps);
            let share_total = reserve_share_bps
                .saturating_add(fee_share_bps)
                .saturating_add(risk_buffer_share_bps);
            if reserve_share_bps == 0
                || fee_share_bps == 0
                || risk_buffer_share_bps == 0
                || share_total != NOV_TREASURY_SHARE_BPS_DENOMINATOR_V1
            {
                return build_failed_native_receipt_v1(
                    request,
                    settled_fee,
                    subject_meta,
                    "governance".to_string(),
                    "apply_treasury_policy".to_string(),
                    format!(
                        "governance.policy.invalid_share_tuple: reserve={} fee={} risk={} total={} expected={}",
                        reserve_share_bps,
                        fee_share_bps,
                        risk_buffer_share_bps,
                        share_total,
                        NOV_TREASURY_SHARE_BPS_DENOMINATOR_V1
                    ),
                );
            }

            let min_reserve_bucket_nov = args_json
                .get("min_reserve_bucket_nov")
                .and_then(parse_u128_from_json_value_v1)
                .unwrap_or(active_policy.min_reserve_bucket_nov);
            let min_fee_bucket_nov = args_json
                .get("min_fee_bucket_nov")
                .and_then(parse_u128_from_json_value_v1)
                .unwrap_or(active_policy.min_fee_bucket_nov);
            let min_risk_buffer_nov = args_json
                .get("min_risk_buffer_nov")
                .and_then(parse_u128_from_json_value_v1)
                .unwrap_or(active_policy.min_risk_buffer_nov)
                .max(1);
            let clearing_enabled = args_json
                .get("clearing_enabled")
                .and_then(|value| value.as_bool())
                .unwrap_or(active_policy.clearing_enabled);
            let mapped_lock_bridge_paused = args_json
                .get("mapped_lock_bridge_paused")
                .or_else(|| args_json.get("bridge_mint_paused"))
                .and_then(|value| value.as_bool())
                .unwrap_or(active_policy.mapped_lock_bridge_paused);
            let mapped_lock_min_confirmations = args_json
                .get("mapped_lock_min_confirmations")
                .or_else(|| args_json.get("eth_lock_min_confirmations"))
                .and_then(parse_u128_from_json_value_v1)
                .map(|value| value.min(u128::from(u64::MAX)) as u64)
                .unwrap_or(active_policy.mapped_lock_min_confirmations);
            let mapped_lock_contract_address = match args_json
                .get("mapped_lock_contract_address")
                .or_else(|| args_json.get("eth_lock_contract_address"))
                .or_else(|| args_json.get("lock_contract_address"))
                .and_then(|value| value.as_str())
            {
                Some(raw) => match normalize_eth_address_policy_v1(raw) {
                    Some(address) => address,
                    None => {
                        return build_failed_native_receipt_v1(
                            request,
                            settled_fee,
                            subject_meta,
                            "governance".to_string(),
                            "apply_treasury_policy".to_string(),
                            "governance.policy.invalid_mapped_lock_contract_address".to_string(),
                        );
                    }
                },
                None => active_policy.mapped_lock_contract_address.clone(),
            };
            let mapped_asset_burn_paused = args_json
                .get("mapped_asset_burn_paused")
                .or_else(|| args_json.get("bridge_burn_paused"))
                .and_then(|value| value.as_bool())
                .unwrap_or(active_policy.mapped_asset_burn_paused);
            let mapped_asset_release_paused = args_json
                .get("mapped_asset_release_paused")
                .or_else(|| args_json.get("bridge_release_paused"))
                .and_then(|value| value.as_bool())
                .unwrap_or(active_policy.mapped_asset_release_paused);
            let mut mapped_asset_auto_heal_enabled = args_json
                .get("mapped_asset_auto_heal_enabled")
                .or_else(|| args_json.get("auto_heal_mapped_assets_enabled"))
                .and_then(|value| value.as_bool())
                .unwrap_or(active_policy.mapped_asset_auto_heal_enabled);
            let mut mapped_asset_auto_heal_rollback_enabled = args_json
                .get("mapped_asset_auto_heal_rollback_enabled")
                .or_else(|| args_json.get("auto_heal_mapped_asset_rollback_enabled"))
                .or_else(|| args_json.get("auto_heal_mapped_assets_rollback_enabled"))
                .and_then(|value| value.as_bool())
                .unwrap_or(active_policy.mapped_asset_auto_heal_rollback_enabled);
            if let Some(raw_reorg_policy) = args_json
                .get("mapped_asset_reorg_response_policy")
                .or_else(|| args_json.get("reorg_response_policy"))
                .and_then(|value| value.as_str())
            {
                let Some((auto_heal, rollback)) =
                    mapped_asset_reorg_response_policy_flags_v1(raw_reorg_policy)
                else {
                    return build_failed_native_receipt_v1(
                        request,
                        settled_fee,
                        subject_meta,
                        "governance".to_string(),
                        "apply_treasury_policy".to_string(),
                        format!(
                            "governance.policy.invalid_mapped_asset_reorg_response_policy: {}",
                            raw_reorg_policy
                        ),
                    );
                };
                mapped_asset_auto_heal_enabled = auto_heal;
                mapped_asset_auto_heal_rollback_enabled = rollback;
            }
            let mapped_asset_reorg_response_policy = mapped_asset_reorg_response_policy_v1(
                mapped_asset_auto_heal_enabled,
                mapped_asset_auto_heal_rollback_enabled,
            );
            let clearing_require_healthy_risk_buffer = args_json
                .get("clearing_require_healthy_risk_buffer")
                .and_then(|value| value.as_bool())
                .unwrap_or(active_policy.clearing_require_healthy_risk_buffer);
            let clearing_daily_nov_hard_limit = args_json
                .get("clearing_daily_nov_hard_limit")
                .and_then(parse_u128_from_json_value_v1)
                .unwrap_or(active_policy.clearing_daily_nov_hard_limit);
            let clearing_constrained_max_slippage_bps = args_json
                .get("clearing_constrained_max_slippage_bps")
                .and_then(parse_u128_from_json_value_v1)
                .map(|value| value as u32)
                .unwrap_or(active_policy.clearing_constrained_max_slippage_bps)
                .max(1);
            let clearing_constrained_daily_usage_bps = args_json
                .get("clearing_constrained_daily_usage_bps")
                .and_then(parse_u128_from_json_value_v1)
                .map(|value| value as u32)
                .unwrap_or(active_policy.clearing_constrained_daily_usage_bps)
                .clamp(1, NOV_TREASURY_SHARE_BPS_DENOMINATOR_V1);
            let clearing_constrained_strategy = match args_json
                .get("clearing_constrained_strategy")
                .and_then(|value| value.as_str())
            {
                Some(raw) => match parse_constrained_strategy_strict_v1(raw) {
                    Some(value) => value.to_string(),
                    None => {
                        return build_failed_native_receipt_v1(
                            request,
                            settled_fee,
                            subject_meta,
                            "governance".to_string(),
                            "apply_treasury_policy".to_string(),
                            format!("governance.policy.invalid_constrained_strategy: {}", raw),
                        );
                    }
                },
                None => normalize_constrained_strategy_v1(
                    active_policy.clearing_constrained_strategy.as_str(),
                )
                .to_string(),
            };
            let fee_oracle_allowed_sources = match args_json
                .get("fee_oracle_allowed_sources")
                .or_else(|| args_json.get("oracle_allowed_sources"))
            {
                Some(value) => {
                    let Some(raw_sources) = parse_string_list_from_json_value_v1(value) else {
                        return build_failed_native_receipt_v1(
                            request,
                            settled_fee,
                            subject_meta,
                            "governance".to_string(),
                            "apply_treasury_policy".to_string(),
                            "governance.policy.invalid_fee_oracle_allowed_sources".to_string(),
                        );
                    };
                    let mut sources: Vec<String> = raw_sources
                        .iter()
                        .map(|source| normalize_fee_oracle_source_v1(source.as_str()))
                        .filter(|source| !source.trim().is_empty())
                        .collect();
                    sources.sort();
                    sources.dedup();
                    if sources.is_empty() {
                        return build_failed_native_receipt_v1(
                            request,
                            settled_fee,
                            subject_meta,
                            "governance".to_string(),
                            "apply_treasury_policy".to_string(),
                            "governance.policy.empty_fee_oracle_allowed_sources".to_string(),
                        );
                    }
                    sources
                }
                None => fee_oracle_allowed_sources_v1(store),
            };
            let fee_oracle_disabled_sources = match args_json
                .get("fee_oracle_disabled_sources")
                .or_else(|| args_json.get("oracle_disabled_sources"))
            {
                Some(value) => {
                    let Some(raw_sources) = parse_string_list_from_json_value_v1(value) else {
                        return build_failed_native_receipt_v1(
                            request,
                            settled_fee,
                            subject_meta,
                            "governance".to_string(),
                            "apply_treasury_policy".to_string(),
                            "governance.policy.invalid_fee_oracle_disabled_sources".to_string(),
                        );
                    };
                    let mut sources: Vec<String> = raw_sources
                        .iter()
                        .map(|source| normalize_fee_oracle_source_v1(source.as_str()))
                        .filter(|source| !source.trim().is_empty())
                        .collect();
                    sources.sort();
                    sources.dedup();
                    sources
                }
                None => fee_oracle_disabled_sources_v1(store),
            };
            let fee_oracle_disabled_source_reasons = match args_json
                .get("fee_oracle_disabled_source_reasons")
                .or_else(|| args_json.get("oracle_disabled_source_reasons"))
                .or_else(|| args_json.get("oracle_slashing_reasons"))
            {
                Some(value) => {
                    let Some(raw_reasons) = parse_string_map_from_json_value_v1(value) else {
                        return build_failed_native_receipt_v1(
                            request,
                            settled_fee,
                            subject_meta,
                            "governance".to_string(),
                            "apply_treasury_policy".to_string(),
                            "governance.policy.invalid_fee_oracle_disabled_source_reasons"
                                .to_string(),
                        );
                    };
                    raw_reasons
                        .iter()
                        .map(|(source, reason)| {
                            (
                                normalize_fee_oracle_source_v1(source.as_str()),
                                reason.trim().to_string(),
                            )
                        })
                        .filter(|(source, reason)| !source.is_empty() && !reason.is_empty())
                        .collect::<BTreeMap<_, _>>()
                }
                None => fee_oracle_disabled_source_reasons_v1(store),
            };
            let fee_oracle_source_rotations = match args_json
                .get("fee_oracle_source_rotations")
                .or_else(|| args_json.get("oracle_source_rotations"))
            {
                Some(value) => {
                    let Some(raw_rotations) = parse_string_map_from_json_value_v1(value) else {
                        return build_failed_native_receipt_v1(
                            request,
                            settled_fee,
                            subject_meta,
                            "governance".to_string(),
                            "apply_treasury_policy".to_string(),
                            "governance.policy.invalid_fee_oracle_source_rotations".to_string(),
                        );
                    };
                    raw_rotations
                        .iter()
                        .map(|(old_source, new_source)| {
                            (
                                normalize_fee_oracle_source_v1(old_source.as_str()),
                                normalize_fee_oracle_source_v1(new_source.as_str()),
                            )
                        })
                        .filter(|(old_source, new_source)| {
                            !old_source.is_empty() && !new_source.is_empty()
                        })
                        .collect::<BTreeMap<_, _>>()
                }
                None => fee_oracle_source_rotations_v1(store),
            };
            let provided_policy_version = args_json
                .get("policy_version")
                .and_then(parse_u128_from_json_value_v1)
                .map(|value| value as u32);
            let next_policy_version = active_policy.policy_version.saturating_add(1).max(1);
            let policy_version = provided_policy_version.unwrap_or(next_policy_version);
            if policy_version < active_policy.policy_version {
                return build_failed_native_receipt_v1(
                    request,
                    settled_fee,
                    subject_meta,
                    "governance".to_string(),
                    "apply_treasury_policy".to_string(),
                    format!(
                        "governance.policy.version_regression: current={} proposed={}",
                        active_policy.policy_version, policy_version
                    ),
                );
            }

            store.module_state.treasury_reserve_share_bps = reserve_share_bps;
            store.module_state.treasury_fee_share_bps = fee_share_bps;
            store.module_state.treasury_risk_buffer_share_bps = risk_buffer_share_bps;
            store.module_state.treasury_min_reserve_bucket_nov = min_reserve_bucket_nov;
            store.module_state.treasury_min_fee_bucket_nov = min_fee_bucket_nov;
            store.module_state.treasury_min_risk_buffer_nov = min_risk_buffer_nov;
            store.module_state.clearing_enabled = clearing_enabled;
            store.module_state.mapped_lock_bridge_paused = mapped_lock_bridge_paused;
            store.module_state.mapped_lock_min_confirmations = mapped_lock_min_confirmations;
            store.module_state.mapped_lock_contract_address = mapped_lock_contract_address.clone();
            store.module_state.mapped_asset_burn_paused = mapped_asset_burn_paused;
            store.module_state.mapped_asset_release_paused = mapped_asset_release_paused;
            store.module_state.mapped_asset_auto_heal_enabled = mapped_asset_auto_heal_enabled;
            store.module_state.mapped_asset_auto_heal_rollback_enabled =
                mapped_asset_auto_heal_rollback_enabled;
            store.module_state.clearing_require_healthy_risk_buffer =
                clearing_require_healthy_risk_buffer;
            store.module_state.clearing_daily_nov_hard_limit = clearing_daily_nov_hard_limit;
            store.module_state.clearing_constrained_max_slippage_bps =
                clearing_constrained_max_slippage_bps;
            store.module_state.clearing_constrained_daily_usage_bps =
                clearing_constrained_daily_usage_bps;
            store.module_state.clearing_constrained_strategy =
                clearing_constrained_strategy.clone();
            store.module_state.fee_oracle_allowed_sources = fee_oracle_allowed_sources.clone();
            store.module_state.fee_oracle_disabled_sources = fee_oracle_disabled_sources.clone();
            store.module_state.fee_oracle_disabled_source_reasons =
                fee_oracle_disabled_source_reasons.clone();
            store.module_state.fee_oracle_source_rotations = fee_oracle_source_rotations.clone();
            store.module_state.treasury_policy_version = policy_version;
            store.module_state.treasury_policy_source = "governance_path".to_string();
            store.module_state.treasury_policy_last_update_unix_ms = now_unix_millis_v1();

            let log = NovNativeExecutionLogV1 {
                module: "governance".to_string(),
                method: "apply_treasury_policy".to_string(),
                event: "governance.treasury_policy_applied".to_string(),
                data: serde_json::json!({
                    "policy_version": policy_version,
                    "policy_source": "governance_path",
                    "reserve_share_bps": reserve_share_bps,
                    "fee_share_bps": fee_share_bps,
                    "risk_buffer_share_bps": risk_buffer_share_bps,
                    "min_reserve_bucket_nov": min_reserve_bucket_nov,
                    "min_fee_bucket_nov": min_fee_bucket_nov,
                    "min_risk_buffer_nov": min_risk_buffer_nov,
                    "clearing_enabled": clearing_enabled,
                    "mapped_lock_bridge_paused": mapped_lock_bridge_paused,
                    "mapped_lock_min_confirmations": mapped_lock_min_confirmations,
                    "mapped_lock_contract_address": mapped_lock_contract_address,
                    "mapped_asset_burn_paused": mapped_asset_burn_paused,
                    "mapped_asset_release_paused": mapped_asset_release_paused,
                    "mapped_asset_auto_heal_enabled": mapped_asset_auto_heal_enabled,
                    "mapped_asset_auto_heal_rollback_enabled": mapped_asset_auto_heal_rollback_enabled,
                    "mapped_asset_reorg_response_policy": mapped_asset_reorg_response_policy,
                    "clearing_require_healthy_risk_buffer": clearing_require_healthy_risk_buffer,
                    "clearing_daily_nov_hard_limit": clearing_daily_nov_hard_limit,
                    "clearing_constrained_max_slippage_bps": clearing_constrained_max_slippage_bps,
                    "clearing_constrained_daily_usage_bps": clearing_constrained_daily_usage_bps,
                    "clearing_constrained_strategy": clearing_constrained_strategy,
                    "fee_oracle_allowed_sources": fee_oracle_allowed_sources,
                    "fee_oracle_disabled_sources": fee_oracle_disabled_sources,
                    "fee_oracle_disabled_source_reasons": fee_oracle_disabled_source_reasons,
                    "fee_oracle_source_rotations": fee_oracle_source_rotations,
                    "oracle_open_feed_allowed": false,
                }),
            };
            apply_subject_meta_to_receipt_v1(
                NovNativeExecutionReceiptV1 {
                    tx_hash: to_hex(&request.tx_hash),
                    status: true,
                    target: execution_target_label_v1(&request.target),
                    module: "governance".to_string(),
                    method: "apply_treasury_policy".to_string(),
                    settled_fee_nov: settled_fee.nov_amount,
                    paid_asset: settled_fee.source_asset.clone(),
                    paid_amount: settled_fee.source_amount,
                    logs: vec![log],
                    failure_reason: None,
                    fee_contract: settled_fee.fee_contract.clone(),
                    fee_route: settled_fee.route.clone(),
                    fee_quote_id: settled_fee.quote_id.clone(),
                    fee_quote_contract: settled_fee.quote_contract.clone(),
                    fee_clearing_contract: settled_fee.clearing_contract.clone(),
                    fee_price_source: settled_fee.price_source.clone(),
                    fee_quote_required_pay_amount: settled_fee.required_source_amount,
                    fee_quote_expires_at_unix_ms: settled_fee.quote_expires_at_unix_ms,
                    fee_clearing_route_ref: settled_fee.clearing_route_ref.clone(),
                    fee_clearing_source: settled_fee.clearing_source.clone(),
                    fee_clearing_rate_ppm: settled_fee.clearing_rate_ppm,
                    route_meta: route_meta_from_settled_fee_v1(settled_fee),
                    policy_meta: policy_meta_from_settled_fee_v1(settled_fee),
                    account_id: String::new(),
                    fee_owner_account_id: String::new(),
                    nonce_owner_account_id: String::new(),
                    key_algo: String::new(),
                    execution_policy: String::new(),
                    policy_enforced: false,
                    policy_rejection_reason: None,
                    aoem_semantic_ingress: None,
                    aoem_semantic_commit: None,
                },
                subject_meta,
            )
        }
        _ => build_failed_native_receipt_v1(
            request,
            settled_fee,
            subject_meta,
            module_name,
            method_name,
            "unsupported native module method".to_string(),
        ),
    }
}

pub fn dispatch_and_persist_nov_execution_request_v1(
    request: &NovExecutionRequestV1,
) -> Result<NovNativeExecutionReceiptV1> {
    let path = nov_native_execution_store_path_v1();
    dispatch_and_persist_nov_execution_request_with_subjects_and_store_path_v1(
        path.as_path(),
        request,
        None,
        None,
        None,
    )
}

pub fn dispatch_and_persist_nov_execution_request_with_store_path_v1(
    path: &Path,
    request: &NovExecutionRequestV1,
) -> Result<NovNativeExecutionReceiptV1> {
    dispatch_and_persist_nov_execution_request_with_subjects_and_store_path_v1(
        path, request, None, None, None,
    )
}

fn dispatch_nov_execution_request_into_loaded_store_v1(
    mirror_base_path: &Path,
    store: &mut NovNativeExecutionStoreV1,
    request: &NovExecutionRequestV1,
    subject_meta: Option<&NovExecutionSubjectMetaV1>,
    requested_behavior: Option<&NovRequestedExecutionBehaviorV1>,
    unified_account_store_path: Option<&Path>,
    aoem_semantic_ingress_override: Option<NovAoemSemanticIngressMetaV1>,
    mirror_records: Option<&mut Vec<NovAoemSemanticLedgerMirrorRecordV1>>,
    now_ms: u128,
) -> Result<NovNativeExecutionReceiptV1> {
    let effective_subject_meta = subject_meta
        .cloned()
        .unwrap_or_else(|| fallback_execution_subject_meta_v1(request));
    let effective_subject_meta = match enforce_requested_execution_behavior_v1(
        &effective_subject_meta,
        requested_behavior,
        unified_account_store_path,
    ) {
        Ok(meta) => meta,
        Err(rejection) => {
            let rejected_subject_meta = subject_meta_with_execution_policy_v1(
                effective_subject_meta.clone(),
                rejection.key_algo,
                rejection.execution_policy,
                false,
                Some(rejection.reason.to_string()),
            );
            let unresolved_fee = unresolved_settled_fee_v1(request);
            let failed = build_failed_native_receipt_v1(
                request,
                &unresolved_fee,
                &rejected_subject_meta,
                "execution_policy".to_string(),
                "enforce".to_string(),
                rejection.reason.to_string(),
            );
            store
                .receipts
                .insert(failed.tx_hash.clone(), failed.clone());
            let trace = build_execution_trace_v1(
                request,
                &unresolved_fee,
                &failed,
                &rejected_subject_meta,
                store,
                now_ms,
            );
            persist_execution_trace_v1(store, trace);
            store.last_updated_unix_ms = now_ms;
            return Ok(failed);
        }
    };
    let settled_fee = match settle_fee_policy_from_execution_request_v1(
        request,
        &effective_subject_meta,
        store,
        now_ms,
    ) {
        Ok(value) => value,
        Err(err) => {
            let reason = format!("{err}");
            let fee_method = if is_fee_quote_reason_v1(reason.as_str()) {
                "quote"
            } else {
                "settlement"
            };
            let unresolved_fee = unresolved_settled_fee_v1(request);
            let failed = build_failed_native_receipt_v1(
                request,
                &unresolved_fee,
                &effective_subject_meta,
                "fee".to_string(),
                fee_method.to_string(),
                reason,
            );
            store
                .receipts
                .insert(failed.tx_hash.clone(), failed.clone());
            let trace = build_execution_trace_v1(
                request,
                &unresolved_fee,
                &failed,
                &effective_subject_meta,
                store,
                now_ms,
            );
            persist_execution_trace_v1(store, trace);
            store.last_updated_unix_ms = now_ms;
            return Ok(failed);
        }
    };
    let aoem_semantic_ingress = if let Some(meta) = aoem_semantic_ingress_override {
        Some(meta)
    } else {
        match execute_native_request_via_aoem_semantic_ingress_v1(
            request,
            &settled_fee,
            &effective_subject_meta,
        ) {
            Ok(meta) => Some(meta),
            Err(err) => {
                let unresolved_fee = unresolved_settled_fee_v1(request);
                let failed = build_failed_native_receipt_v1(
                    request,
                    &unresolved_fee,
                    &effective_subject_meta,
                    "aoem".to_string(),
                    "semantic_ingress".to_string(),
                    format!("aoem.semantic_ingress.required_failed: {err}"),
                );
                store
                    .receipts
                    .insert(failed.tx_hash.clone(), failed.clone());
                let trace = build_execution_trace_v1(
                    request,
                    &unresolved_fee,
                    &failed,
                    &effective_subject_meta,
                    store,
                    now_ms,
                );
                persist_execution_trace_v1(store, trace);
                store.last_updated_unix_ms = now_ms;
                return Ok(failed);
            }
        }
    };
    let module_state_before_execution = store.module_state.clone();
    let mut receipt =
        dispatch_native_module_execute_v1(request, &settled_fee, &effective_subject_meta, store);
    receipt.aoem_semantic_ingress = aoem_semantic_ingress;
    let semantic_deltas = build_native_execution_semantic_deltas_v1(
        &module_state_before_execution,
        &store.module_state,
    );
    attach_native_semantic_deltas_to_receipt_v1(&mut receipt, semantic_deltas);
    if let Some((sequence, commit_seal)) = attach_native_semantic_ledger_commit_to_receipt_v1(
        &mut receipt,
        &module_state_before_execution,
        &store.module_state,
        module_state_before_execution.aoem_semantic_ledger_sequence,
        module_state_before_execution
            .aoem_semantic_ledger_head
            .as_str(),
    ) {
        store.module_state.aoem_semantic_ledger_sequence = sequence;
        store.module_state.aoem_semantic_ledger_head = commit_seal;
    }
    receipt.aoem_semantic_commit = build_native_receipt_aoem_semantic_commit_v1(&receipt);
    store
        .receipts
        .insert(receipt.tx_hash.clone(), receipt.clone());
    let trace = build_execution_trace_v1(
        request,
        &settled_fee,
        &receipt,
        &effective_subject_meta,
        store,
        now_ms,
    );
    persist_execution_trace_v1(store, trace);
    store.last_updated_unix_ms = now_ms;
    if let Some(mirror_record) =
        build_native_aoem_semantic_ledger_mirror_record_v1(&receipt, now_ms)
    {
        if let Some(records) = mirror_records {
            records.push(mirror_record);
        } else {
            let mirror_path = nov_native_aoem_semantic_ledger_mirror_path_v1(mirror_base_path);
            append_nov_native_aoem_semantic_ledger_mirror_record_v1(
                mirror_path.as_path(),
                &mirror_record,
            )?;
        }
    }
    Ok(receipt)
}

fn dispatch_and_persist_nov_execution_request_with_subjects_and_store_path_v1(
    path: &Path,
    request: &NovExecutionRequestV1,
    subject_meta: Option<&NovExecutionSubjectMetaV1>,
    requested_behavior: Option<&NovRequestedExecutionBehaviorV1>,
    unified_account_store_path: Option<&Path>,
) -> Result<NovNativeExecutionReceiptV1> {
    let _write_lock = acquire_nov_native_execution_store_write_lock_v1(path)?;
    let mut store = load_nov_native_execution_store_v1(path)?;
    let previous_store = store.clone();
    let now_ms = now_unix_millis_v1();
    let receipt = dispatch_nov_execution_request_into_loaded_store_v1(
        path,
        &mut store,
        request,
        subject_meta,
        requested_behavior,
        unified_account_store_path,
        None,
        None,
        now_ms,
    )?;
    save_nov_native_execution_store_with_previous_v1(path, Some(&previous_store), &store)?;
    Ok(receipt)
}

pub fn get_nov_native_execution_receipt_by_hash_with_store_path_v1(
    path: &Path,
    tx_hash: &str,
) -> Result<Option<NovNativeExecutionReceiptV1>> {
    let store = load_nov_native_execution_store_v1(path)?;
    let key = normalize_tx_hash_hex_v1(tx_hash);
    Ok(store.receipts.get(key.as_str()).cloned())
}

pub fn get_nov_native_execution_receipt_by_hash_v1(
    tx_hash: &str,
) -> Result<Option<NovNativeExecutionReceiptV1>> {
    let path = nov_native_execution_store_path_v1();
    get_nov_native_execution_receipt_by_hash_with_store_path_v1(path.as_path(), tx_hash)
}

pub fn get_nov_native_account_asset_balance_with_store_path_v1(
    path: &Path,
    account: &str,
    asset: &str,
) -> Result<u128> {
    let store = load_nov_native_execution_store_v1(path)?;
    Ok(native_account_asset_balance_v1(&store, account, asset))
}

pub fn get_nov_native_account_asset_balance_v1(account: &str, asset: &str) -> Result<u128> {
    let path = nov_native_execution_store_path_v1();
    get_nov_native_account_asset_balance_with_store_path_v1(path.as_path(), account, asset)
}

pub fn get_nov_native_treasury_settlement_summary_with_store_path_v1(
    path: &Path,
) -> Result<serde_json::Value> {
    let out = run_nov_native_call_from_params_with_store_path_v1(
        &serde_json::json!({
            "target": {"kind": "native_module", "id": "treasury"},
            "method": "get_settlement_summary",
            "args": {},
        }),
        Some(path),
    )?;
    Ok(out
        .get("result")
        .cloned()
        .unwrap_or_else(|| serde_json::json!({})))
}

pub fn get_nov_native_treasury_settlement_summary_v1() -> Result<serde_json::Value> {
    let path = nov_native_execution_store_path_v1();
    get_nov_native_treasury_settlement_summary_with_store_path_v1(path.as_path())
}

pub fn get_nov_native_treasury_clearing_summary_with_store_path_v1(
    path: &Path,
) -> Result<serde_json::Value> {
    let routes = run_nov_native_call_from_params_with_store_path_v1(
        &serde_json::json!({
            "target": {"kind": "native_module", "id": "treasury"},
            "method": "get_clearing_routes",
            "args": {},
        }),
        Some(path),
    )?;
    let last_route = run_nov_native_call_from_params_with_store_path_v1(
        &serde_json::json!({
            "target": {"kind": "native_module", "id": "treasury"},
            "method": "get_last_clearing_route",
            "args": {},
        }),
        Some(path),
    )?;
    let last_candidates = run_nov_native_call_from_params_with_store_path_v1(
        &serde_json::json!({
            "target": {"kind": "native_module", "id": "treasury"},
            "method": "get_last_clearing_candidates",
            "args": {},
        }),
        Some(path),
    )?;
    let risk = run_nov_native_call_from_params_with_store_path_v1(
        &serde_json::json!({
            "target": {"kind": "native_module", "id": "treasury"},
            "method": "get_clearing_risk_summary",
            "args": {},
        }),
        Some(path),
    )?;
    Ok(serde_json::json!({
        "routes": routes.get("result").cloned().unwrap_or_else(|| serde_json::json!({})),
        "last_route": last_route.get("result").cloned().unwrap_or(serde_json::Value::Null),
        "last_candidates": last_candidates.get("result").cloned().unwrap_or_else(|| serde_json::json!({})),
        "risk": risk.get("result").cloned().unwrap_or_else(|| serde_json::json!({})),
    }))
}

pub fn get_nov_native_treasury_clearing_summary_v1() -> Result<serde_json::Value> {
    let path = nov_native_execution_store_path_v1();
    get_nov_native_treasury_clearing_summary_with_store_path_v1(path.as_path())
}

fn resolve_native_execution_store_path_from_params_v1(
    params: &serde_json::Value,
) -> Option<PathBuf> {
    params
        .get("native_execution_store_path")
        .and_then(|value| value.as_str())
        .and_then(|raw| {
            let trimmed = raw.trim();
            if trimmed.is_empty() {
                None
            } else {
                Some(PathBuf::from(trimmed))
            }
        })
}

fn resolve_unified_account_store_path_from_params_v1(
    params: &serde_json::Value,
    native_execution_store_path: &Path,
) -> Option<PathBuf> {
    params
        .get("unified_account_store_path")
        .or_else(|| params.get("ua_store_path"))
        .and_then(|value| value.as_str())
        .and_then(|raw| {
            let trimmed = raw.trim();
            if trimmed.is_empty() {
                None
            } else {
                Some(PathBuf::from(trimmed))
            }
        })
        .or_else(|| match std::env::var("NOVOVM_UNIFIED_ACCOUNT_DB") {
            Ok(raw) => {
                let trimmed = raw.trim();
                if trimmed.is_empty() {
                    None
                } else {
                    Some(PathBuf::from(trimmed))
                }
            }
            Err(_) => None,
        })
        .or_else(|| {
            native_execution_store_path
                .parent()
                .map(|parent| parent.join("novovm-unified-account-router.rocksdb"))
        })
}

fn requested_execution_behavior_v1(
    execution_policy: NovExecutionPolicyV1,
    privacy_mode: NovPrivacyModeV1,
) -> NovRequestedExecutionBehaviorV1 {
    NovRequestedExecutionBehaviorV1 {
        execution_policy,
        privacy_mode,
    }
}

fn default_execution_behavior_v1() -> NovRequestedExecutionBehaviorV1 {
    requested_execution_behavior_v1(NovExecutionPolicyV1::Standard, NovPrivacyModeV1::Public)
}

fn is_native_m2_fee_asset_symbol_v1(asset_id: &str) -> bool {
    let normalized = asset_id.trim().to_ascii_uppercase();
    normalized != "NOV" && normalized.starts_with('N')
}

fn effective_execution_policy_for_fee_asset_v1(
    requested_policy: NovExecutionPolicyV1,
    pay_asset: &str,
) -> NovExecutionPolicyV1 {
    if is_native_m2_fee_asset_symbol_v1(pay_asset) {
        NovExecutionPolicyV1::PrivacyRequired
    } else {
        requested_policy
    }
}

fn is_qualified_runtime_policy_demand_v1(subject_meta: &NovExecutionSubjectMetaV1) -> bool {
    let account_id = subject_meta.account_id.trim();
    let fee_owner = subject_meta.fee_owner_account_id.trim();
    let nonce_owner = subject_meta.nonce_owner_account_id.trim();
    !account_id.is_empty() && !fee_owner.is_empty() && !nonce_owner.is_empty()
}

fn emit_runtime_policy_observability_event_v1(
    subject_meta: &NovExecutionSubjectMetaV1,
    policy: NovExecutionPolicyV1,
    accepted: bool,
    reason: Option<&str>,
) {
    if matches!(policy, NovExecutionPolicyV1::Standard) {
        return;
    }
    let qualified_demand = is_qualified_runtime_policy_demand_v1(subject_meta);
    let policy_label = policy.as_str().to_string();
    let reason_owned = reason.map(|value| value.to_string());
    let _ = append_governance_event_auto(
        "tx_ingress",
        GovernanceEvent::RuntimePolicyEvaluated {
            policy: policy_label.clone(),
            required: true,
            accepted,
            reason: reason_owned.clone(),
            qualified_demand: Some(qualified_demand),
            account_id: Some(subject_meta.account_id.clone()),
            demand_source: Some("nov_execute".to_string()),
        },
    );
    if !accepted {
        let _ = append_governance_event_auto(
            "tx_ingress",
            GovernanceEvent::RuntimeConstraintHit {
                policy: policy_label,
                reason: reason_owned.unwrap_or_else(|| "policy_rejected".to_string()),
                qualified_demand: Some(qualified_demand),
                account_id: Some(subject_meta.account_id.clone()),
            },
        );
    }
}

#[derive(Clone, Copy, Debug)]
struct NovExecutionPolicyRejectionV1 {
    key_algo: Option<UcaKeyAlgo>,
    execution_policy: NovExecutionPolicyV1,
    reason: &'static str,
}

fn enforce_requested_execution_behavior_v1(
    subject_meta: &NovExecutionSubjectMetaV1,
    requested_behavior: Option<&NovRequestedExecutionBehaviorV1>,
    unified_account_store_path: Option<&Path>,
) -> std::result::Result<NovExecutionSubjectMetaV1, NovExecutionPolicyRejectionV1> {
    let requested = requested_behavior
        .copied()
        .unwrap_or_else(default_execution_behavior_v1);
    let resolved_key_algo = unified_account_store_path.and_then(|path| {
        get_unified_account_key_algo_with_store_path_v1(path, subject_meta.account_id.as_str())
            .ok()
            .flatten()
    });
    let with_success = || {
        subject_meta_with_execution_policy_v1(
            subject_meta.clone(),
            resolved_key_algo,
            requested.execution_policy,
            true,
            None,
        )
    };

    match requested.execution_policy {
        NovExecutionPolicyV1::Standard => Ok(with_success()),
        NovExecutionPolicyV1::PqRequired => {
            if resolved_key_algo == Some(UcaKeyAlgo::Mldsa87) {
                emit_runtime_policy_observability_event_v1(
                    subject_meta,
                    requested.execution_policy,
                    true,
                    None,
                );
                Ok(with_success())
            } else {
                emit_runtime_policy_observability_event_v1(
                    subject_meta,
                    requested.execution_policy,
                    false,
                    Some(ERR_PQ_REQUIRED_BUT_KEY_NOT_PQ),
                );
                Err(NovExecutionPolicyRejectionV1 {
                    key_algo: resolved_key_algo,
                    execution_policy: requested.execution_policy,
                    reason: ERR_PQ_REQUIRED_BUT_KEY_NOT_PQ,
                })
            }
        }
        NovExecutionPolicyV1::PrivacyRequired => {
            if matches!(requested.privacy_mode, NovPrivacyModeV1::Public) {
                emit_runtime_policy_observability_event_v1(
                    subject_meta,
                    requested.execution_policy,
                    false,
                    Some(ERR_PRIVACY_REQUIRED_BUT_PATH_NOT_AVAILABLE),
                );
                Err(NovExecutionPolicyRejectionV1 {
                    key_algo: resolved_key_algo,
                    execution_policy: requested.execution_policy,
                    reason: ERR_PRIVACY_REQUIRED_BUT_PATH_NOT_AVAILABLE,
                })
            } else {
                emit_runtime_policy_observability_event_v1(
                    subject_meta,
                    requested.execution_policy,
                    true,
                    None,
                );
                Ok(with_success())
            }
        }
    }
}

pub fn has_nov_native_call_shape_v1(params: &serde_json::Value) -> bool {
    let call_obj = match params {
        serde_json::Value::Array(arr) => arr.first().cloned().unwrap_or(serde_json::Value::Null),
        serde_json::Value::Object(map) => map
            .get("call")
            .or_else(|| map.get("tx"))
            .or_else(|| map.get("transaction"))
            .cloned()
            .unwrap_or_else(|| params.clone()),
        _ => serde_json::Value::Null,
    };
    call_obj.get("target").is_some() && call_obj.get("method").is_some()
}

pub fn run_nov_native_call_from_params_with_store_path_v1(
    params: &serde_json::Value,
    store_path: Option<&Path>,
) -> Result<serde_json::Value> {
    let call_obj = match params {
        serde_json::Value::Array(arr) => arr.first().cloned().unwrap_or(serde_json::Value::Null),
        serde_json::Value::Object(map) => map
            .get("call")
            .or_else(|| map.get("tx"))
            .or_else(|| map.get("transaction"))
            .cloned()
            .unwrap_or_else(|| params.clone()),
        _ => serde_json::Value::Null,
    };
    let target = parse_nov_execution_target_v1(
        call_obj
            .get("target")
            .ok_or_else(|| anyhow::anyhow!("nov_call requires target"))?,
    );
    let method = call_obj
        .get("method")
        .and_then(|value| value.as_str())
        .ok_or_else(|| anyhow::anyhow!("nov_call requires method"))?
        .trim()
        .to_ascii_lowercase();
    let args = call_obj
        .get("args")
        .cloned()
        .unwrap_or(serde_json::json!({}));
    let path = store_path
        .map(Path::to_path_buf)
        .unwrap_or_else(nov_native_execution_store_path_v1);
    let store = load_nov_native_execution_store_v1(path.as_path())?;
    let settlement_policy = resolve_treasury_settlement_policy_v1(&store);
    let policy_source = normalize_policy_source_v1(settlement_policy.policy_source.as_str());
    let allocation_parameters = allocation_parameters_snapshot_v1(&settlement_policy);
    let risk_buffer_status = risk_buffer_status_v1(&store, &settlement_policy);
    let bucket_boundaries = bucket_boundary_snapshot_v1(&store, &settlement_policy);
    let clearing_policy_gate = clearing_policy_gate_snapshot_v1(&store, &settlement_policy);
    let policy_paths = treasury_policy_paths_snapshot_v1(&store, &settlement_policy);
    let policy_contract =
        treasury_policy_contract_snapshot_v1(&settlement_policy, &allocation_parameters);
    let policy_contract_id = policy_contract
        .get("policy_contract_id")
        .and_then(|value| value.as_str())
        .unwrap_or_default()
        .to_string();
    let current_threshold_state = clearing_policy_gate
        .get("threshold_state")
        .and_then(|value| value.as_str())
        .unwrap_or("healthy")
        .to_string();
    let policy_context = treasury_policy_context_snapshot_v1(
        &settlement_policy,
        policy_contract_id.as_str(),
        current_threshold_state.as_str(),
    );
    let accounting_snapshot = build_treasury_accounting_snapshot_v1(&store);
    let oracle_policy = serde_json::json!({
        "oracle_source": fee_oracle_source_v1(&store),
        "oracle_allowed_sources": fee_oracle_allowed_sources_v1(&store),
        "oracle_disabled_sources": fee_oracle_disabled_sources_v1(&store),
        "oracle_disabled_source_reasons": fee_oracle_disabled_source_reasons_v1(&store),
        "oracle_source_rotations": fee_oracle_source_rotations_v1(&store),
        "oracle_source_allowed": fee_oracle_source_allowed_v1(&store),
        "oracle_source_disabled": fee_oracle_source_disabled_v1(&store),
        "oracle_disabled_reason": fee_oracle_disabled_reason_v1(&store),
        "oracle_rotation_target": fee_oracle_rotation_target_v1(&store),
        "oracle_open_feed_allowed": false,
    });
    let reserve_proof_now_ms = now_unix_millis_v1();
    let reserve_proofs = store
        .module_state
        .treasury_reserve_proofs
        .iter()
        .map(|(asset, proof)| {
            (
                asset.clone(),
                serde_json::json!({
                    "proof": proof,
                    "effective_status": reserve_proof_effective_status_v1(
                        proof,
                        reserve_proof_now_ms
                    ),
                    "claims": {
                        "real_external_reserve_auto_verified": proof.automated_verification,
                        "nov_mint_authorized": false,
                        "external_redemption_authorized": false
                    }
                }),
            )
        })
        .collect::<BTreeMap<String, serde_json::Value>>();
    let journal_total = store.module_state.treasury_settlement_journal.len();
    let journal_next_seq = store.module_state.treasury_settlement_journal_next_seq;
    let journal_last_seq = store
        .module_state
        .treasury_settlement_journal
        .last()
        .map(|entry| entry.seq)
        .unwrap_or(0);
    let out = match target {
        NovExecutionTargetV1::NativeModule(module) => {
            let module_name = module.trim().to_ascii_lowercase();
            match (module_name.as_str(), method.as_str()) {
                ("treasury", "get_reserve_balance") => {
                    let asset = args
                        .get("asset")
                        .and_then(|value| value.as_str())
                        .map(normalize_asset_symbol_v1)
                        .unwrap_or_else(|| "NOV".to_string());
                    let balance = store
                        .module_state
                        .treasury_reserves
                        .get(asset.as_str())
                        .copied()
                        .unwrap_or(0);
                    serde_json::json!({
                        "method": "nov_call",
                        "target": "treasury",
                        "module_method": "get_reserve_balance",
                        "found": true,
                        "result": {
                            "asset": asset,
                            "reserve_balance": balance,
                            "reserve_proof": reserve_proofs.get(asset.as_str()).cloned(),
                        },
                    })
                }
                ("treasury", "get_reserve_proof") => {
                    let asset = args
                        .get("asset")
                        .and_then(|value| value.as_str())
                        .map(normalize_asset_symbol_v1)
                        .unwrap_or_else(|| "NOV".to_string());
                    let proof = reserve_proofs.get(asset.as_str()).cloned();
                    serde_json::json!({
                        "method": "nov_call",
                        "target": "treasury",
                        "module_method": "get_reserve_proof",
                        "found": proof.is_some(),
                        "result": {
                            "asset": asset,
                            "reserve_balance": store.module_state.treasury_reserves
                                .get(asset.as_str())
                                .copied()
                                .unwrap_or(0),
                            "reserve_proof": proof,
                            "proof_required_for_nov_mint": true,
                            "automated_external_verification_complete": false,
                        },
                    })
                }
                ("treasury", "get_reserve_snapshot") => serde_json::json!({
                    "method": "nov_call",
                    "target": "treasury",
                    "module_method": "get_reserve_snapshot",
                    "found": true,
                    "result": {
                        "reserves": store.module_state.treasury_reserves.clone(),
                        "reserve_proofs": reserve_proofs.clone(),
                        "settled_nov_total": store.module_state.treasury_settled_nov_total,
                        "redeemed_nov_total": store.module_state.treasury_redeemed_nov_total,
                        "settlement_count": store.module_state.treasury_settlements,
                        "settled_by_asset": store.module_state.treasury_settled_by_asset.clone(),
                        "redeemed_by_asset": store.module_state.treasury_redeemed_by_asset.clone(),
                        "settlement_buckets_nov": {
                            "reserve": store.module_state.treasury_reserve_bucket_nov,
                            "fee": store.module_state.treasury_fee_bucket_nov,
                            "risk_buffer": store.module_state.treasury_risk_buffer_nov,
                        },
                        "aoem_semantic_ledger": {
                            "execution_kernel": "AOEM",
                            "semantic_entry": native_aoem_semantic_entry_v1(),
                            "algebraic_semantic_entry": true,
                            "sequence": store.module_state.aoem_semantic_ledger_sequence,
                            "head": store.module_state.aoem_semantic_ledger_head,
                        },
                        "accounting": accounting_snapshot.clone(),
                        "journal": {
                            "total_entries": journal_total,
                            "last_seq": journal_last_seq,
                            "next_seq": journal_next_seq,
                        },
                        "settlement_policy": settlement_policy.clone(),
                        "allocation_parameters": allocation_parameters.clone(),
                        "policy_contract": policy_contract.clone(),
                        "policy_contract_id": policy_contract_id.clone(),
                        "policy_version": settlement_policy.policy_version,
                        "policy_source": policy_source.clone(),
                        "policy_context": policy_context.clone(),
                        "policy_paths": policy_paths.clone(),
                        "oracle_policy": oracle_policy.clone(),
                        "current_threshold_state": current_threshold_state.clone(),
                        "risk_buffer_status": risk_buffer_status,
                        "bucket_boundaries": bucket_boundaries.clone(),
                        "clearing_policy_gate": clearing_policy_gate.clone(),
                        "last_fee_quote": store.module_state.last_fee_quote.clone(),
                        "last_fee_quote_failure": store.module_state.last_fee_quote_failure.clone(),
                    },
                }),
                ("treasury", "get_settlement_summary") => serde_json::json!({
                    "method": "nov_call",
                    "target": "treasury",
                    "module_method": "get_settlement_summary",
                    "found": true,
                    "result": {
                        "settled_nov_total": store.module_state.treasury_settled_nov_total,
                        "redeemed_nov_total": store.module_state.treasury_redeemed_nov_total,
                        "settlement_count": store.module_state.treasury_settlements,
                        "settled_by_asset": store.module_state.treasury_settled_by_asset.clone(),
                        "redeemed_by_asset": store.module_state.treasury_redeemed_by_asset.clone(),
                        "settlement_buckets_nov": {
                            "reserve": store.module_state.treasury_reserve_bucket_nov,
                            "fee": store.module_state.treasury_fee_bucket_nov,
                            "risk_buffer": store.module_state.treasury_risk_buffer_nov,
                        },
                        "aoem_semantic_ledger": {
                            "execution_kernel": "AOEM",
                            "semantic_entry": native_aoem_semantic_entry_v1(),
                            "algebraic_semantic_entry": true,
                            "sequence": store.module_state.aoem_semantic_ledger_sequence,
                            "head": store.module_state.aoem_semantic_ledger_head,
                        },
                        "accounting": accounting_snapshot.clone(),
                        "journal": {
                            "total_entries": journal_total,
                            "last_seq": journal_last_seq,
                            "next_seq": journal_next_seq,
                        },
                        "settlement_policy": settlement_policy.clone(),
                        "allocation_parameters": allocation_parameters.clone(),
                        "policy_contract": policy_contract.clone(),
                        "policy_contract_id": policy_contract_id.clone(),
                        "policy_version": settlement_policy.policy_version,
                        "policy_source": policy_source.clone(),
                        "policy_context": policy_context.clone(),
                        "policy_paths": policy_paths.clone(),
                        "oracle_policy": oracle_policy.clone(),
                        "current_threshold_state": current_threshold_state.clone(),
                        "settlement_failures": store
                            .module_state
                            .treasury_settlement_failure_counts
                            .clone(),
                        "risk_buffer_status": risk_buffer_status,
                        "bucket_boundaries": bucket_boundaries.clone(),
                        "clearing_policy_gate": clearing_policy_gate.clone(),
                        "clearing_failures": store.module_state.clearing_failure_counts.clone(),
                        "quote_failures": store.module_state.fee_quote_failure_counts.clone(),
                        "last_fee_quote": store.module_state.last_fee_quote.clone(),
                        "last_fee_quote_failure": store.module_state.last_fee_quote_failure.clone(),
                    },
                }),
                ("treasury", "get_settlement_journal") => {
                    let requested_limit = args
                        .get("limit")
                        .and_then(parse_u128_from_json_value_v1)
                        .map(|value| value as usize)
                        .unwrap_or(50);
                    let limit = requested_limit.clamp(1, 500);
                    let total = store.module_state.treasury_settlement_journal.len();
                    let start = total.saturating_sub(limit);
                    let entries = store
                        .module_state
                        .treasury_settlement_journal
                        .iter()
                        .skip(start)
                        .cloned()
                        .collect::<Vec<_>>();
                    serde_json::json!({
                        "method": "nov_call",
                        "target": "treasury",
                        "module_method": "get_settlement_journal",
                        "found": true,
                        "result": {
                            "requested_limit": requested_limit,
                            "effective_limit": limit,
                            "total_entries": total,
                            "next_seq": journal_next_seq,
                            "policy_contract_id": policy_contract_id.clone(),
                            "policy_context": policy_context.clone(),
                            "entries": entries,
                        },
                    })
                }
                ("treasury", "get_settlement_policy") => serde_json::json!({
                    "method": "nov_call",
                    "target": "treasury",
                    "module_method": "get_settlement_policy",
                    "found": true,
                    "result": {
                        "policy": settlement_policy.clone(),
                        "allocation_parameters": allocation_parameters.clone(),
                        "policy_contract": policy_contract.clone(),
                        "policy_contract_id": policy_contract_id.clone(),
                        "policy_version": settlement_policy.policy_version,
                        "policy_source": policy_source.clone(),
                        "policy_context": policy_context.clone(),
                        "policy_paths": policy_paths.clone(),
                        "oracle_policy": oracle_policy.clone(),
                        "current_threshold_state": current_threshold_state.clone(),
                        "risk_buffer_status": risk_buffer_status,
                        "bucket_boundaries": bucket_boundaries.clone(),
                        "clearing_policy_gate": clearing_policy_gate.clone(),
                        "current_risk_buffer_nov": store.module_state.treasury_risk_buffer_nov,
                    },
                }),
                ("treasury", "get_protocol_clearing_price") => {
                    let asset = args
                        .get("asset")
                        .and_then(|value| value.as_str())
                        .map(normalize_asset_symbol_v1)
                        .unwrap_or_else(|| "USDT".to_string());
                    let now_ms = now_unix_millis_v1();
                    match build_protocol_clearing_price_v1(&store, asset.as_str(), now_ms) {
                        Ok(price) => serde_json::json!({
                            "method": "nov_call",
                            "target": "treasury",
                            "module_method": "get_protocol_clearing_price",
                            "found": true,
                            "result": {
                                "asset": asset,
                                "price": price,
                                "semantics": {
                                    "p_clear": "p_epoch_ppm is NOV per 1 unit of asset in ppm",
                                    "p_pay": "p_pay_ppm is conservative fee/payment clearing price",
                                    "p_redeem": "p_redeem_ppm is conservative treasury-out redemption price",
                                    "epoch_fixed": true,
                                    "amm_spot_allowed": false,
                                    "oracle_open_feed_allowed": false,
                                    "oracle_source": fee_oracle_source_v1(&store),
                                    "oracle_allowed_sources": fee_oracle_allowed_sources_v1(&store),
                                    "oracle_disabled_sources": fee_oracle_disabled_sources_v1(&store),
                                    "oracle_disabled_source_reasons": fee_oracle_disabled_source_reasons_v1(&store),
                                    "oracle_source_rotations": fee_oracle_source_rotations_v1(&store),
                                    "oracle_source_allowed": fee_oracle_source_allowed_v1(&store),
                                    "oracle_source_disabled": fee_oracle_source_disabled_v1(&store),
                                    "oracle_disabled_reason": fee_oracle_disabled_reason_v1(&store),
                                    "oracle_rotation_target": fee_oracle_rotation_target_v1(&store),
                                }
                            },
                        }),
                        Err(err) => serde_json::json!({
                            "method": "nov_call",
                            "target": "treasury",
                            "module_method": "get_protocol_clearing_price",
                            "found": false,
                            "result": {
                                "asset": asset,
                                "state": "blocked",
                                "reason": err.to_string(),
                            },
                        }),
                    }
                }
                ("treasury", "get_clearing_liquidity") => {
                    let asset = args
                        .get("asset")
                        .and_then(|value| value.as_str())
                        .map(normalize_asset_symbol_v1)
                        .unwrap_or_else(|| "USDT".to_string());
                    let default_liquidity = env_u128_or_v1(
                        NOV_NATIVE_FEE_DEFAULT_CLEARING_NOV_LIQUIDITY_ENV,
                        NOV_FEE_DEFAULT_CLEARING_NOV_LIQUIDITY_V1,
                    );
                    let available_nov = store
                        .module_state
                        .clearing_nov_liquidity
                        .get(asset.as_str())
                        .copied()
                        .unwrap_or(default_liquidity);
                    match resolve_clearing_rate_ppm_with_source_v1(
                        &store,
                        asset.as_str(),
                        now_unix_millis_v1(),
                    ) {
                        Ok((clearing_rate_ppm, price_source, updated_unix_ms)) => {
                            serde_json::json!({
                                "method": "nov_call",
                                "target": "treasury",
                                "module_method": "get_clearing_liquidity",
                                "found": true,
                                "result": {
                                    "asset": asset,
                                    "available_nov": available_nov,
                                    "clearing_rate_ppm": clearing_rate_ppm,
                                    "price_source": price_source,
                                    "price_updated_unix_ms": updated_unix_ms,
                                    "state": "available",
                                },
                            })
                        }
                        Err(err) => serde_json::json!({
                            "method": "nov_call",
                            "target": "treasury",
                            "module_method": "get_clearing_liquidity",
                            "found": false,
                            "result": {
                                "asset": asset,
                                "available_nov": available_nov,
                                "clearing_rate_ppm": 0,
                                "price_source": "unavailable",
                                "price_updated_unix_ms": 0,
                                "state": "blocked",
                                "reason": err.to_string(),
                            },
                        }),
                    }
                }
                ("treasury", "get_clearing_routes") => {
                    let asset = args
                        .get("asset")
                        .and_then(|value| value.as_str())
                        .map(normalize_asset_symbol_v1)
                        .unwrap_or_else(|| "USDT".to_string());
                    let mut routes = Vec::new();
                    if let Ok((rate_ppm, source, _updated)) =
                        resolve_clearing_rate_ppm_with_source_v1(
                            &store,
                            asset.as_str(),
                            now_unix_millis_v1(),
                        )
                    {
                        if let Some(treasury_source) =
                            build_treasury_direct_source_v1(&store, asset.as_str(), rate_ppm)
                        {
                            routes.push(serde_json::json!({
                                "route_id": format!("route:treasury_direct:{}:nov", asset.to_ascii_lowercase()),
                                "route_source": "treasury_direct",
                                "asset_in": asset,
                                "asset_out": "NOV",
                                "available_nov": treasury_source.available_liquidity_nov,
                                "clearing_rate_ppm": rate_ppm,
                                "price_source": source,
                            }));
                        }
                    }
                    for pool in static_amm_sources_for_asset_v1(&store, asset.as_str()) {
                        routes.push(serde_json::json!({
                            "route_id": format!("route:amm_pool:{}:{}->{}", pool.pool_id, pool.asset_x, pool.asset_y),
                            "route_source": "amm_pool",
                            "asset_in": pool.asset_x,
                            "asset_out": pool.asset_y,
                            "pool_id": pool.pool_id,
                            "reserve_in": pool.reserve_x,
                            "reserve_out": pool.reserve_y,
                            "swap_fee_ppm": pool.swap_fee_ppm,
                        }));
                    }
                    serde_json::json!({
                        "method": "nov_call",
                        "target": "treasury",
                        "module_method": "get_clearing_routes",
                        "found": true,
                        "result": {
                            "asset": asset,
                            "route_count": routes.len(),
                            "routes": routes,
                        },
                    })
                }
                ("treasury", "get_last_clearing_route") => {
                    let result = store
                        .module_state
                        .last_clearing_route
                        .clone()
                        .and_then(|route| serde_json::to_value(route).ok())
                        .map(|mut value| {
                            if let Some(obj) = value.as_object_mut() {
                                obj.insert("policy_context".to_string(), policy_context.clone());
                            }
                            value
                        });
                    serde_json::json!({
                        "method": "nov_call",
                        "target": "treasury",
                        "module_method": "get_last_clearing_route",
                        "found": result.is_some(),
                        "result": result,
                    })
                }
                ("treasury", "get_last_clearing_candidates") => serde_json::json!({
                    "method": "nov_call",
                    "target": "treasury",
                    "module_method": "get_last_clearing_candidates",
                    "found": !store.module_state.last_clearing_candidates.is_empty(),
                    "result": {
                        "route_count": store.module_state.last_clearing_candidates.len(),
                        "routes": store.module_state.last_clearing_candidates.clone(),
                        "policy_context": policy_context.clone(),
                    },
                }),
                ("treasury", "get_clearing_risk_summary") => {
                    let mut top_failures = store
                        .module_state
                        .clearing_failure_counts
                        .iter()
                        .map(|(reason, count)| {
                            serde_json::json!({
                                "reason": reason,
                                "count": count,
                            })
                        })
                        .collect::<Vec<_>>();
                    top_failures.sort_by(|a, b| {
                        b["count"]
                            .as_u64()
                            .unwrap_or_default()
                            .cmp(&a["count"].as_u64().unwrap_or_default())
                    });
                    top_failures.truncate(5);
                    let total_failures = store
                        .module_state
                        .clearing_failure_counts
                        .values()
                        .copied()
                        .fold(0u64, |acc, value| acc.saturating_add(value));
                    serde_json::json!({
                        "method": "nov_call",
                        "target": "treasury",
                        "module_method": "get_clearing_risk_summary",
                        "found": true,
                        "result": {
                            "policy": settlement_policy.clone(),
                            "allocation_parameters": allocation_parameters.clone(),
                            "policy_contract": policy_contract.clone(),
                            "policy_contract_id": policy_contract_id.clone(),
                            "policy_version": settlement_policy.policy_version,
                            "policy_source": policy_source.clone(),
                            "policy_context": policy_context.clone(),
                            "policy_paths": policy_paths.clone(),
                            "current_threshold_state": current_threshold_state.clone(),
                            "bucket_boundaries": bucket_boundaries.clone(),
                            "effective_gate": clearing_policy_gate.clone(),
                            "last_trigger": {
                                "failure_code": if store.module_state.last_clearing_failure_code.trim().is_empty() {
                                    serde_json::Value::Null
                                } else {
                                    serde_json::json!(store.module_state.last_clearing_failure_code.clone())
                                },
                                "failure_reason": if store.module_state.last_clearing_failure_reason.trim().is_empty() {
                                    serde_json::Value::Null
                                } else {
                                    serde_json::json!(store.module_state.last_clearing_failure_reason.clone())
                                },
                                "failure_unix_ms": if store.module_state.last_clearing_failure_unix_ms == 0 {
                                    serde_json::Value::Null
                                } else {
                                    serde_json::json!(store.module_state.last_clearing_failure_unix_ms)
                                },
                            },
                            "failure_summary": {
                                "total_failures": total_failures,
                                "by_reason": store.module_state.clearing_failure_counts.clone(),
                                "top_reasons": top_failures,
                            },
                            "last_candidate_routes": {
                                "route_count": store.module_state.last_clearing_candidates.len(),
                                "routes": store.module_state.last_clearing_candidates.clone(),
                                "policy_context": policy_context.clone(),
                            },
                            "last_selected_route": store.module_state.last_clearing_route.clone(),
                            "last_selected_route_policy_context": policy_context.clone(),
                        },
                    })
                }
                ("treasury", "get_last_execution_trace") => {
                    let result = store
                        .module_state
                        .last_execution_trace
                        .clone()
                        .and_then(|trace| serde_json::to_value(trace).ok())
                        .map(|mut value| {
                            if let Some(obj) = value.as_object_mut() {
                                obj.insert("policy_context".to_string(), policy_context.clone());
                            }
                            value
                        });
                    serde_json::json!({
                        "method": "nov_call",
                        "target": "treasury",
                        "module_method": "get_last_execution_trace",
                        "found": result.is_some(),
                        "result": result,
                    })
                }
                ("treasury", "get_execution_trace_by_tx") => {
                    let tx_hash = args
                        .get("tx_hash")
                        .or_else(|| args.get("hash"))
                        .and_then(|value| value.as_str())
                        .ok_or_else(|| {
                            anyhow::anyhow!(
                                "nov_call treasury.get_execution_trace_by_tx requires tx_hash/hash"
                            )
                        })?;
                    let key = normalize_tx_hash_hex_v1(tx_hash);
                    let result = store
                        .module_state
                        .execution_traces_by_tx
                        .get(key.as_str())
                        .cloned()
                        .and_then(|trace| serde_json::to_value(trace).ok())
                        .map(|mut value| {
                            if let Some(obj) = value.as_object_mut() {
                                obj.insert("policy_context".to_string(), policy_context.clone());
                            }
                            value
                        });
                    serde_json::json!({
                        "method": "nov_call",
                        "target": "treasury",
                        "module_method": "get_execution_trace_by_tx",
                        "found": result.is_some(),
                        "tx_hash": key,
                        "result": result,
                    })
                }
                ("treasury", "get_clearing_metrics_summary") => serde_json::json!({
                    "method": "nov_call",
                    "target": "treasury",
                    "module_method": "get_clearing_metrics_summary",
                    "found": true,
                    "result": {
                        "metrics": build_clearing_metrics_summary_v1(&store),
                        "policy_context": policy_context.clone(),
                    },
                }),
                ("treasury", "get_policy_metrics_summary") => serde_json::json!({
                    "method": "nov_call",
                    "target": "treasury",
                    "module_method": "get_policy_metrics_summary",
                    "found": true,
                    "result": {
                        "metrics": build_policy_metrics_summary_v1(
                            &store,
                            policy_contract_id.as_str(),
                            policy_source.as_str(),
                            current_threshold_state.as_str(),
                            settlement_policy.clearing_constrained_strategy.as_str(),
                        ),
                        "policy_context": policy_context.clone(),
                    },
                }),
                ("treasury", "get_fee_quote_summary") => serde_json::json!({
                    "method": "nov_call",
                    "target": "treasury",
                    "module_method": "get_fee_quote_summary",
                    "found": true,
                    "result": {
                        "last_fee_quote": store.module_state.last_fee_quote.clone(),
                        "last_fee_quote_failure": store.module_state.last_fee_quote_failure.clone(),
                        "quote_failures": store.module_state.fee_quote_failure_counts.clone(),
                    },
                }),
                ("treasury", "get_fee_oracle_rates") => serde_json::json!({
                    "method": "nov_call",
                    "target": "treasury",
                    "module_method": "get_fee_oracle_rates",
                    "found": true,
                    "result": {
                        "rates_ppm": store.module_state.fee_oracle_rates_ppm.clone(),
                        "oracle_updated_unix_ms": store.module_state.fee_oracle_updated_unix_ms,
                        "oracle_source": fee_oracle_source_v1(&store),
                        "oracle_allowed_sources": fee_oracle_allowed_sources_v1(&store),
                        "oracle_disabled_sources": fee_oracle_disabled_sources_v1(&store),
                        "oracle_disabled_source_reasons": fee_oracle_disabled_source_reasons_v1(&store),
                        "oracle_source_rotations": fee_oracle_source_rotations_v1(&store),
                        "oracle_source_allowed": fee_oracle_source_allowed_v1(&store),
                        "oracle_source_disabled": fee_oracle_source_disabled_v1(&store),
                        "oracle_disabled_reason": fee_oracle_disabled_reason_v1(&store),
                        "oracle_rotation_target": fee_oracle_rotation_target_v1(&store),
                        "oracle_open_feed_allowed": false,
                        "oracle_max_age_ms": execution_fee_oracle_max_age_ms_v1(),
                    },
                }),
                ("governance", "get_proposal") => {
                    let proposal_id = args
                        .get("proposal_id")
                        .and_then(parse_u128_from_json_value_v1)
                        .map(|value| value as u64)
                        .ok_or_else(|| {
                            anyhow::anyhow!("nov_call governance.get_proposal requires proposal_id")
                        })?;
                    let proposal = store
                        .module_state
                        .governance_proposals
                        .get(&proposal_id)
                        .cloned();
                    serde_json::json!({
                        "method": "nov_call",
                        "target": "governance",
                        "module_method": "get_proposal",
                        "found": proposal.is_some(),
                        "result": proposal,
                    })
                }
                ("governance", "list_proposals") => serde_json::json!({
                    "method": "nov_call",
                    "target": "governance",
                    "module_method": "list_proposals",
                    "count": store.module_state.governance_proposals.len(),
                    "result": store.module_state.governance_proposals.clone(),
                }),
                _ => bail!(
                    "unsupported nov_call native module method: {}.{}",
                    module_name,
                    method
                ),
            }
        }
        NovExecutionTargetV1::WasmApp(app) => bail!("unsupported nov_call wasm target: {}", app),
        NovExecutionTargetV1::Plugin(plugin) => {
            bail!("unsupported nov_call plugin target: {}", plugin)
        }
    };
    Ok(out)
}

pub fn run_nov_native_call_from_params_v1(params: &serde_json::Value) -> Result<serde_json::Value> {
    run_nov_native_call_from_params_with_store_path_v1(params, None)
}

fn decode_hex_nibble_v1(byte: u8) -> Option<u8> {
    match byte {
        b'0'..=b'9' => Some(byte - b'0'),
        b'a'..=b'f' => Some(byte - b'a' + 10),
        b'A'..=b'F' => Some(byte - b'A' + 10),
        _ => None,
    }
}

pub fn decode_eth_send_raw_hex_payload_v1(raw: &str, field: &str) -> Result<Vec<u8>> {
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        bail!("{field} is empty");
    }
    let hex = trimmed
        .strip_prefix("0x")
        .or_else(|| trimmed.strip_prefix("0X"))
        .unwrap_or(trimmed);
    if hex.is_empty() {
        bail!("{field} is empty after 0x prefix");
    }
    if !hex.len().is_multiple_of(2) {
        bail!("{field} must be even-length hex, got len={}", hex.len());
    }
    let mut out = Vec::with_capacity(hex.len() / 2);
    let bytes = hex.as_bytes();
    for (idx, pair) in bytes.chunks_exact(2).enumerate() {
        let hi = decode_hex_nibble_v1(pair[0]).ok_or_else(|| {
            anyhow::anyhow!(
                "{field} contains invalid hex at byte={} char={}",
                idx * 2,
                pair[0] as char
            )
        })?;
        let lo = decode_hex_nibble_v1(pair[1]).ok_or_else(|| {
            anyhow::anyhow!(
                "{field} contains invalid hex at byte={} char={}",
                idx * 2 + 1,
                pair[1] as char
            )
        })?;
        out.push((hi << 4) | lo);
    }
    Ok(out)
}

pub fn run_eth_send_raw_transaction_from_params_v1(
    params: &serde_json::Value,
) -> Result<serde_json::Value> {
    let raw_tx = params
        .get("raw_tx")
        .and_then(|value| value.as_str())
        .or_else(|| {
            params
                .as_array()
                .and_then(|items| items.first())
                .and_then(|value| value.as_str())
        })
        .ok_or_else(|| anyhow::anyhow!("raw_tx is required for eth_sendRawTransaction"))?;
    let payload = decode_eth_send_raw_hex_payload_v1(raw_tx, "raw_tx")?;
    let chain_id = params
        .get("chain_id")
        .and_then(|value| value.as_u64())
        .or_else(|| {
            params
                .as_array()
                .and_then(|items| items.get(1))
                .and_then(|value| value.as_u64())
        })
        .unwrap_or(1);
    let tx_hash = ingest_local_eth_raw_tx_payload_v1(chain_id, payload.as_slice())?;
    Ok(serde_json::json!({
        "method": "eth_sendRawTransaction",
        "accepted": true,
        "pending_tx_local_ingress": true,
        "pending_tx_hash": to_hex_prefixed_v1(&tx_hash),
        "chain_id": chain_id,
    }))
}

pub fn run_nov_send_raw_transaction_from_params_v1(
    params: &serde_json::Value,
) -> Result<serde_json::Value> {
    let raw_tx = params
        .get("raw_tx")
        .and_then(|value| value.as_str())
        .or_else(|| {
            params
                .as_array()
                .and_then(|items| items.first())
                .and_then(|value| value.as_str())
        })
        .ok_or_else(|| anyhow::anyhow!("raw_tx is required for nov_sendRawTransaction"))?;
    let payload = decode_eth_send_raw_hex_payload_v1(raw_tx, "raw_tx")?;
    let (native_tx, ir, tx_hash) = ingest_local_nov_raw_tx_payload_v1(params, payload.as_slice())?;
    let requested_execution_policy_override = parse_nov_execution_policy_v1(
        params
            .get("execution_policy")
            .and_then(|value| value.as_str()),
    );
    let requested_privacy_mode_override =
        parse_nov_privacy_mode_v1(params.get("privacy_mode").and_then(|value| value.as_str()));
    let execution_subject = match &native_tx.kind {
        NovTxKindV1::Execute(execute) => Some(subject_meta_from_execute_tx_v1(execute)),
        _ => None,
    };
    let requested_execution_behavior = match &native_tx.kind {
        NovTxKindV1::Execute(execute) => {
            let requested_policy = if params.get("execution_policy").is_some() {
                requested_execution_policy_override
            } else {
                execute.execution_policy
            };
            Some(requested_execution_behavior_v1(
                effective_execution_policy_for_fee_asset_v1(
                    requested_policy,
                    execute.fee_policy.pay_asset.as_str(),
                ),
                if params.get("privacy_mode").is_some() {
                    requested_privacy_mode_override
                } else {
                    execute.privacy_mode
                },
            ))
        }
        _ => None,
    };
    let execution_request = nov_native_tx_to_execution_request_v1(&native_tx)?;
    let store_path_override = resolve_native_execution_store_path_from_params_v1(params);
    let effective_native_store_path = store_path_override
        .clone()
        .unwrap_or_else(nov_native_execution_store_path_v1);
    let unified_account_store_path = resolve_unified_account_store_path_from_params_v1(
        params,
        effective_native_store_path.as_path(),
    );
    let pipeline_only = native_send_raw_transaction_pipeline_only_v1(params);
    let execution_receipt = if !pipeline_only {
        if let Some(request) = execution_request.as_ref() {
            Some(if let Some(path) = store_path_override.as_deref() {
                dispatch_and_persist_nov_execution_request_with_subjects_and_store_path_v1(
                    path,
                    request,
                    execution_subject.as_ref(),
                    requested_execution_behavior.as_ref(),
                    unified_account_store_path.as_deref(),
                )?
            } else {
                dispatch_and_persist_nov_execution_request_with_subjects_and_store_path_v1(
                    effective_native_store_path.as_path(),
                    request,
                    execution_subject.as_ref(),
                    requested_execution_behavior.as_ref(),
                    unified_account_store_path.as_deref(),
                )?
            })
        } else {
            None
        }
    } else {
        None
    };
    let execution_lifecycle = if pipeline_only {
        "pending_runtime_to_aoem_tick"
    } else {
        "compat_immediate_dispatch_after_pending_ingress"
    };
    Ok(serde_json::json!({
        "method": "nov_sendRawTransaction",
        "accepted": true,
        "pending_tx_local_ingress": true,
        "pending_tx_hash": to_hex_prefixed_v1(&tx_hash),
        "chain_id": native_tx.chain_id,
        "nov_tx_kind": match native_tx.kind {
            NovTxKindV1::Transfer(_) => "transfer",
            NovTxKindV1::Execute(_) => "execute",
            NovTxKindV1::Governance(_) => "governance",
        },
        "tx_ir_type": format!("{:?}", ir.tx_type),
        "execution_request": execution_request,
        "execution_subject": execution_subject,
        "pipeline_only": pipeline_only,
        "immediate_execution": execution_receipt.is_some(),
        "execution_lifecycle": execution_lifecycle,
        "aoem_lifecycle": execution_lifecycle,
        "native_receipt": execution_receipt,
    }))
}

fn native_aoem_raw_tx_batch_plan_id_v1(raw_payloads: &[Vec<u8>]) -> u64 {
    let mut parts: Vec<&[u8]> = Vec::with_capacity(raw_payloads.len() + 1);
    parts.push(b"novovm-native-aoem-raw-tx-batch-plan-id-v1");
    for payload in raw_payloads {
        parts.push(payload.as_slice());
    }
    let digest = sha256_bytes_v1(parts.as_slice());
    let mut out = [0u8; 8];
    out.copy_from_slice(&digest[..8]);
    u64::from_le_bytes(out)
}

fn build_native_aoem_raw_tx_batch_ops_wire_v1(
    raw_payloads: &[Vec<u8>],
) -> Result<(OpsWirePayload, u64)> {
    if raw_payloads.is_empty() {
        bail!("nov raw transaction batch must not be empty");
    }
    let max_batch = native_aoem_batch_max_size_v1();
    if raw_payloads.len() > max_batch {
        bail!(
            "nov raw transaction batch too large: got={}, max={} ({})",
            raw_payloads.len(),
            max_batch,
            NOV_NATIVE_AOEM_BATCH_MAX_SIZE_ENV
        );
    }
    let batch_plan_id = native_aoem_raw_tx_batch_plan_id_v1(raw_payloads);
    let mut builder = OpsWireV1Builder::new();
    for (idx, payload) in raw_payloads.iter().enumerate() {
        let key = sha256_bytes_v1(&[
            b"novovm-native-aoem-raw-tx-batch-key-v1",
            &batch_plan_id.to_le_bytes(),
            &(idx as u64).to_le_bytes(),
            payload.as_slice(),
        ]);
        let value = sha256_bytes_v1(&[
            b"novovm-native-aoem-raw-tx-batch-value-v1",
            native_aoem_semantic_entry_v1().as_bytes(),
            &(idx as u64).to_le_bytes(),
            payload.as_slice(),
        ]);
        let plan_id_digest = sha256_bytes_v1(&[
            b"novovm-native-aoem-raw-tx-batch-item-plan-id-v1",
            &batch_plan_id.to_le_bytes(),
            &(idx as u64).to_le_bytes(),
        ]);
        let mut plan_id_bytes = [0u8; 8];
        plan_id_bytes.copy_from_slice(&plan_id_digest[..8]);
        builder.push(OpsWireOp {
            opcode: 2,
            flags: 0,
            reserved: 0,
            key: &key,
            value: &value,
            delta: 0,
            expect_version: None,
            plan_id: u64::from_le_bytes(plan_id_bytes),
        })?;
    }
    Ok((builder.finish(), batch_plan_id))
}

fn execute_native_raw_tx_batch_via_aoem_semantic_ingress_v1(
    raw_payloads: &[Vec<u8>],
) -> Result<NovAoemSemanticIngressMetaV1> {
    let enabled = native_aoem_semantic_ingress_enabled_v1();
    let required = native_aoem_semantic_ingress_required_v1();
    let (wire, plan_id) = build_native_aoem_raw_tx_batch_ops_wire_v1(raw_payloads)?;
    let mut meta = base_native_aoem_semantic_ingress_meta_v1(enabled, required, plan_id, &wire);
    meta.semantic_entry = native_aoem_raw_tx_batch_precommit_entry_v1().to_string();
    meta.ingress_scope = "raw_tx_batch_precommit".to_string();
    meta.batch_plan_id = Some(plan_id);
    meta.batch_item_count = Some(raw_payloads.len());
    meta.batch_mode = true;
    meta.batch_size = raw_payloads.len();
    if !enabled {
        meta.fallback_reason = Some("aoem_semantic_ingress_disabled".to_string());
        return Ok(meta);
    }

    let runtime = match AoemRuntimeConfig::from_env() {
        Ok(runtime) => runtime,
        Err(err) => {
            if required {
                return Err(err).context("aoem raw transaction batch runtime config failed");
            }
            meta.fallback_reason = Some(format!("runtime_config_unavailable: {err}"));
            return Ok(meta);
        }
    };
    attach_native_aoem_parallelism_meta_v1(&mut meta, Some(&runtime));
    meta.batch_mode = true;
    meta.batch_size = raw_payloads.len();
    if !runtime.dll_path.exists() {
        if required {
            bail!(
                "aoem raw transaction batch required but AOEM runtime DLL is missing: {}",
                runtime.dll_path.display()
            );
        }
        meta.fallback_reason = Some(format!(
            "runtime_dll_missing: {}",
            runtime.dll_path.display()
        ));
        return Ok(meta);
    }
    let facade = match AoemExecFacade::open_with_runtime(&runtime) {
        Ok(facade) => facade,
        Err(err) => {
            if required {
                return Err(err).context("open AOEM raw transaction batch runtime failed");
            }
            meta.fallback_reason = Some(format!("runtime_open_failed: {err}"));
            return Ok(meta);
        }
    };
    if !facade.supports_ops_wire_v1() {
        if required {
            bail!("aoem raw transaction batch required but ops_wire_v1 is unsupported");
        }
        meta.fallback_reason = Some("ops_wire_v1_unsupported".to_string());
        return Ok(meta);
    }
    let session = match facade.create_session() {
        Ok(session) => session,
        Err(err) => {
            if required {
                return Err(err).context("create AOEM raw transaction batch session failed");
            }
            meta.fallback_reason = Some(format!("session_create_failed: {err}"));
            return Ok(meta);
        }
    };
    match session.submit_ops_wire(wire.bytes.as_slice()) {
        Ok(output) => {
            meta.submitted = true;
            meta.processed_ops = output.metrics.processed_ops;
            meta.success_ops = output.metrics.success_ops;
            meta.total_writes = output.metrics.total_writes;
            meta.return_code_name = output.metrics.return_code_name;
            Ok(meta)
        }
        Err(err) => {
            if required {
                return Err(err).context("submit AOEM raw transaction batch ops-wire failed");
            }
            meta.fallback_reason = Some(format!("submit_failed: {err}"));
            Ok(meta)
        }
    }
}

fn aggregate_native_aoem_raw_tx_batch_chunks_v1(
    raw_payloads: &[Vec<u8>],
    chunks: &[NovAoemSemanticIngressMetaV1],
) -> NovAoemSemanticIngressMetaV1 {
    let enabled = native_aoem_semantic_ingress_enabled_v1();
    let required = native_aoem_semantic_ingress_required_v1();
    let plan_id = native_aoem_raw_tx_batch_plan_id_v1(raw_payloads);
    let mut digest_parts: Vec<&[u8]> = Vec::with_capacity(chunks.len() + 1);
    digest_parts.push(b"novovm-native-aoem-raw-tx-batch-chunked-wire-digest-v1");
    for chunk in chunks {
        digest_parts.push(chunk.wire_digest.as_bytes());
    }
    let mut meta = NovAoemSemanticIngressMetaV1 {
        execution_kernel: "AOEM".to_string(),
        semantic_entry: native_aoem_raw_tx_batch_precommit_entry_v1().to_string(),
        algebraic_semantic_entry: true,
        ingress_scope: "raw_tx_batch_precommit_chunked".to_string(),
        batch_plan_id: Some(plan_id),
        batch_item_index: None,
        batch_item_count: Some(raw_payloads.len()),
        concurrent_execution_enabled: false,
        concurrent_execution_model: String::new(),
        batch_mode: true,
        batch_size: raw_payloads.len(),
        recommended_threads: 1,
        ingress_workers: 1,
        host_hw_threads: 1,
        host_budget_threads: 1,
        parallelism_reason: String::new(),
        enabled,
        required,
        submitted: enabled && !chunks.is_empty() && chunks.iter().all(|chunk| chunk.submitted),
        op_count: raw_payloads.len(),
        plan_id,
        wire_digest: to_hex(&sha256_bytes_v1(digest_parts.as_slice())),
        processed_ops: chunks.iter().map(|chunk| chunk.processed_ops).sum::<u32>(),
        success_ops: chunks.iter().map(|chunk| chunk.success_ops).sum::<u32>(),
        total_writes: chunks.iter().map(|chunk| chunk.total_writes).sum::<u64>(),
        semantic_delta_count: 0,
        semantic_delta_digest: String::new(),
        semantic_state_before_digest: String::new(),
        semantic_state_after_digest: String::new(),
        semantic_ledger_sequence: 0,
        semantic_ledger_prev_seal: String::new(),
        semantic_ledger_commit_seal: String::new(),
        return_code_name: chunks
            .first()
            .map(|chunk| chunk.return_code_name.clone())
            .unwrap_or_default(),
        fallback_reason: None,
    };
    attach_native_aoem_parallelism_meta_v1(&mut meta, None);
    if chunks.is_empty() {
        meta.fallback_reason = Some("aoem_batch_chunks_empty".to_string());
    } else if chunks
        .iter()
        .all(|chunk| chunk.fallback_reason == chunks[0].fallback_reason)
    {
        meta.fallback_reason = chunks[0].fallback_reason.clone();
    } else {
        meta.fallback_reason = Some("aoem_batch_chunk_fallback_mixed".to_string());
    }
    meta
}

fn execute_native_raw_tx_batch_chunks_via_aoem_semantic_ingress_v1(
    raw_payloads: &[Vec<u8>],
) -> Result<(
    NovAoemSemanticIngressMetaV1,
    Vec<NovAoemSemanticIngressMetaV1>,
)> {
    if raw_payloads.is_empty() {
        bail!("nov raw transaction batch must not be empty");
    }
    let max_batch = native_aoem_batch_max_size_v1();
    let chunk_count = (raw_payloads.len() + max_batch - 1) / max_batch;
    let mut chunks = Vec::with_capacity(chunk_count);
    for chunk in raw_payloads.chunks(max_batch) {
        chunks.push(execute_native_raw_tx_batch_via_aoem_semantic_ingress_v1(
            chunk,
        )?);
    }
    let aggregate = if chunks.len() == 1 {
        chunks[0].clone()
    } else {
        aggregate_native_aoem_raw_tx_batch_chunks_v1(raw_payloads, chunks.as_slice())
    };
    Ok((aggregate, chunks))
}

fn native_aoem_batch_item_ingress_meta_v1(
    batch_meta: &NovAoemSemanticIngressMetaV1,
    item_index: usize,
    item_count: usize,
) -> NovAoemSemanticIngressMetaV1 {
    let mut meta = batch_meta.clone();
    meta.ingress_scope = "raw_tx_batch_precommit_item".to_string();
    meta.batch_plan_id = Some(batch_meta.batch_plan_id.unwrap_or(batch_meta.plan_id));
    meta.batch_item_index = Some(item_index);
    meta.batch_item_count = Some(item_count);
    meta
}

pub fn run_nov_send_raw_transaction_batch_from_params_v1(
    params: &serde_json::Value,
) -> Result<serde_json::Value> {
    let raw_values = params
        .get("raw_txs")
        .or_else(|| params.get("raw_transactions"))
        .or_else(|| params.get("transactions"))
        .and_then(|value| value.as_array())
        .ok_or_else(|| {
            anyhow::anyhow!("raw_txs array is required for nov_sendRawTransactionBatch")
        })?;
    if raw_values.is_empty() {
        bail!("raw_txs array must not be empty");
    }

    struct PreparedNovRawBatchItemV1 {
        native_tx: NovNativeTxWireV1,
        ir: TxIR,
        tx_hash: [u8; 32],
        execution_subject: Option<NovExecutionSubjectMetaV1>,
        requested_execution_behavior: Option<NovRequestedExecutionBehaviorV1>,
        execution_request: Option<NovExecutionRequestV1>,
    }

    let mut raw_payloads = Vec::with_capacity(raw_values.len());
    let mut prepared = Vec::with_capacity(raw_values.len());
    let requested_execution_policy_override = parse_nov_execution_policy_v1(
        params
            .get("execution_policy")
            .and_then(|value| value.as_str()),
    );
    let requested_privacy_mode_override =
        parse_nov_privacy_mode_v1(params.get("privacy_mode").and_then(|value| value.as_str()));
    for (idx, value) in raw_values.iter().enumerate() {
        let raw = value
            .as_str()
            .ok_or_else(|| anyhow::anyhow!("raw_txs[{idx}] must be a hex string"))?;
        let payload = decode_eth_send_raw_hex_payload_v1(raw, "raw_txs")?;
        let (native_tx, ir, tx_hash) =
            ingest_local_nov_raw_tx_payload_v1(params, payload.as_slice())?;
        let execution_subject = match &native_tx.kind {
            NovTxKindV1::Execute(execute) => Some(subject_meta_from_execute_tx_v1(execute)),
            _ => None,
        };
        let requested_execution_behavior = match &native_tx.kind {
            NovTxKindV1::Execute(execute) => {
                let requested_policy = if params.get("execution_policy").is_some() {
                    requested_execution_policy_override
                } else {
                    execute.execution_policy
                };
                Some(requested_execution_behavior_v1(
                    effective_execution_policy_for_fee_asset_v1(
                        requested_policy,
                        execute.fee_policy.pay_asset.as_str(),
                    ),
                    if params.get("privacy_mode").is_some() {
                        requested_privacy_mode_override
                    } else {
                        execute.privacy_mode
                    },
                ))
            }
            _ => None,
        };
        let execution_request = nov_native_tx_to_execution_request_v1(&native_tx)?;
        raw_payloads.push(payload);
        prepared.push(PreparedNovRawBatchItemV1 {
            native_tx,
            ir,
            tx_hash,
            execution_subject,
            requested_execution_behavior,
            execution_request,
        });
    }

    let (aoem_batch_ingress, aoem_batch_chunks) =
        execute_native_raw_tx_batch_chunks_via_aoem_semantic_ingress_v1(raw_payloads.as_slice())?;
    let aoem_chunk_size = native_aoem_batch_max_size_v1();
    let store_path_override = resolve_native_execution_store_path_from_params_v1(params);
    let effective_native_store_path = store_path_override
        .clone()
        .unwrap_or_else(nov_native_execution_store_path_v1);
    let unified_account_store_path = resolve_unified_account_store_path_from_params_v1(
        params,
        effective_native_store_path.as_path(),
    );
    let now_ms = now_unix_millis_v1();
    let _write_lock =
        acquire_nov_native_execution_store_write_lock_v1(effective_native_store_path.as_path())?;
    let mut store = load_nov_native_execution_store_v1(effective_native_store_path.as_path())?;
    let precommit_store_materialized_receipts = store.receipts.len();
    let precommit_store_materialized_estimated_bytes =
        estimate_native_execution_store_retained_bytes_v1(&store);
    let previous_store = store.clone();
    let previous_store_clone_receipts = previous_store.receipts.len();
    let previous_store_clone_estimated_bytes =
        estimate_native_execution_store_retained_bytes_v1(&previous_store);
    let mut results = Vec::with_capacity(prepared.len());
    let mut mirror_records = Vec::new();
    let item_count = prepared.len();
    for (item_index, item) in prepared.into_iter().enumerate() {
        let item_batch_ingress = aoem_batch_chunks
            .get(item_index / aoem_chunk_size)
            .unwrap_or(&aoem_batch_ingress);
        let execution_receipt = if let Some(request) = item.execution_request.as_ref() {
            Some(dispatch_nov_execution_request_into_loaded_store_v1(
                effective_native_store_path.as_path(),
                &mut store,
                request,
                item.execution_subject.as_ref(),
                item.requested_execution_behavior.as_ref(),
                unified_account_store_path.as_deref(),
                Some(native_aoem_batch_item_ingress_meta_v1(
                    item_batch_ingress,
                    item_index,
                    item_count,
                )),
                Some(&mut mirror_records),
                now_ms,
            )?)
        } else {
            None
        };
        results.push(serde_json::json!({
            "method": "nov_sendRawTransaction",
            "accepted": true,
            "pending_tx_local_ingress": true,
            "pending_tx_hash": to_hex_prefixed_v1(&item.tx_hash),
            "chain_id": item.native_tx.chain_id,
            "nov_tx_kind": match item.native_tx.kind {
                NovTxKindV1::Transfer(_) => "transfer",
                NovTxKindV1::Execute(_) => "execute",
                NovTxKindV1::Governance(_) => "governance",
            },
            "tx_ir_type": format!("{:?}", item.ir.tx_type),
            "execution_request": item.execution_request,
            "execution_subject": item.execution_subject,
            "native_receipt": execution_receipt,
        }));
    }
    let mirror_path =
        nov_native_aoem_semantic_ledger_mirror_path_v1(effective_native_store_path.as_path());
    append_nov_native_aoem_semantic_ledger_mirror_records_v1(
        mirror_path.as_path(),
        mirror_records.as_slice(),
    )?;
    let native_store_dirty_set =
        native_execution_store_dirty_set_v1(&previous_store, &store, false)?;
    let native_store_dirty_stats =
        native_execution_store_dirty_set_stats_json_v1(&native_store_dirty_set);
    save_nov_native_execution_store_with_previous_v1(
        effective_native_store_path.as_path(),
        Some(&previous_store),
        &store,
    )?;
    let aoem_batch_chunk_count = aoem_batch_chunks.len();
    let native_store_backend_status = get_nov_native_execution_store_backend_status_v1(Some(
        effective_native_store_path.as_path(),
    ));

    Ok(serde_json::json!({
        "method": "nov_sendRawTransactionBatch",
        "accepted": true,
        "execution_kernel": "AOEM",
        "concurrent_execution": aoem_batch_ingress.concurrent_execution_enabled,
        "batch_size": results.len(),
        "aoem_concurrency_owner": "AOEM_runtime",
        "aoem_batch_ingress": aoem_batch_ingress,
        "aoem_batch_chunking": {
            "enabled": aoem_batch_chunk_count > 1,
            "chunk_count": aoem_batch_chunk_count,
            "max_chunk_size": aoem_chunk_size,
            "model": "bounded_ops_wire_chunks_submitted_to_aoem_runtime_no_host_thread_scheduler",
        },
        "aoem_batch_chunks": aoem_batch_chunks,
        "deterministic_commit": "post_aoem_batch_precommit_deterministic_sharded_dirty_atomic_commit",
        "native_store_commit": {
            "model": "post_aoem_deterministic_dirty_store_commit",
            "load_count": 1,
            "save_count": 1,
            "ordered_results": true,
            "aoem_precommit_chunk_count": aoem_batch_chunk_count,
            "precommit_store_materialized": true,
            "precommit_store_materialized_receipts": precommit_store_materialized_receipts,
            "precommit_store_materialized_estimated_bytes": precommit_store_materialized_estimated_bytes,
            "previous_store_clone_receipts": previous_store_clone_receipts,
            "previous_store_clone_estimated_bytes": previous_store_clone_estimated_bytes,
            "materialization_risk": if precommit_store_materialized_receipts > 0 {
                "rocksdb_full_receipt_materialization_before_dirty_commit"
            } else {
                "empty_store_or_first_batch"
            },
            "dirty_set": native_store_dirty_stats,
        },
        "native_store_backend_status": native_store_backend_status,
        "results": results,
    }))
}

fn native_pending_execution_eligible_stage_v1(
    stage: NetworkRuntimeNativePendingTxLifecycleStageV1,
) -> bool {
    matches!(
        stage,
        NetworkRuntimeNativePendingTxLifecycleStageV1::Seen
            | NetworkRuntimeNativePendingTxLifecycleStageV1::Pending
            | NetworkRuntimeNativePendingTxLifecycleStageV1::Propagated
            | NetworkRuntimeNativePendingTxLifecycleStageV1::ReorgedBackToPending
    )
}

fn project_executed_native_pending_batch_to_canonical_v1(
    chain_id: u64,
    tx_hashes: &[[u8; 32]],
    raw_txs: &[Vec<u8>],
) -> serde_json::Value {
    if tx_hashes.is_empty() {
        return serde_json::json!({
            "enabled": false,
            "reason": "empty_executed_native_pending_batch",
        });
    }
    let now_ms = now_unix_millis_v1();
    let previous_head = get_network_runtime_native_head_snapshot_v1(chain_id);
    let block_number = previous_head
        .as_ref()
        .map(|head| head.block_number.saturating_add(1))
        .unwrap_or(1);
    let parent_block_hash = previous_head
        .as_ref()
        .map(|head| head.block_hash)
        .unwrap_or([0u8; 32]);
    let mut block_hash_parts = Vec::<&[u8]>::with_capacity(tx_hashes.len().saturating_add(4));
    let chain_id_bytes = chain_id.to_be_bytes();
    let block_number_bytes = block_number.to_be_bytes();
    block_hash_parts.push(b"novovm-native-execution-canonical-projection-v1");
    block_hash_parts.push(&chain_id_bytes);
    block_hash_parts.push(&block_number_bytes);
    block_hash_parts.push(parent_block_hash.as_slice());
    for tx_hash in tx_hashes {
        block_hash_parts.push(tx_hash.as_slice());
    }
    let block_hash = sha256_bytes_v1(block_hash_parts.as_slice());
    let state_root = sha256_bytes_v1(&[
        b"novovm-native-execution-state-root-projection-v1",
        block_hash.as_slice(),
    ]);
    set_network_runtime_native_head_snapshot_v1(
        chain_id,
        NetworkRuntimeNativeHeadSnapshotV1 {
            chain_id,
            phase: NetworkRuntimeNativeSyncPhaseV1::Finalize,
            peer_count: 0,
            block_number,
            block_hash,
            parent_block_hash,
            state_root,
            canonical: true,
            safe: true,
            finalized: true,
            reorg_depth_hint: Some(0),
            body_available: true,
            source_peer_id: None,
            observed_unix_ms: now_ms,
        },
    );
    set_network_runtime_native_body_snapshot_v1(
        chain_id,
        NetworkRuntimeNativeBodySnapshotV1 {
            chain_id,
            number: block_number,
            block_hash,
            tx_hashes: tx_hashes.to_vec(),
            raw_tx_rlps: raw_txs.to_vec(),
            ommer_hashes: Vec::new(),
            withdrawal_rlp_items: None,
            withdrawal_count: None,
            body_available: true,
            txs_materialized: true,
            observed_unix_ms: now_ms,
        },
    );
    serde_json::json!({
        "enabled": true,
        "projection": "native_execution_batch_to_canonical_body_head",
        "block_number": block_number,
        "block_hash": to_hex_prefixed_v1(&block_hash),
        "parent_block_hash": to_hex_prefixed_v1(&parent_block_hash),
        "tx_count": tx_hashes.len(),
        "included_canonical": true,
    })
}

pub fn run_nov_execute_pending_native_tx_batch_from_params_v1(
    params: &serde_json::Value,
) -> Result<serde_json::Value> {
    let chain_id = params
        .get("chain_id")
        .and_then(|value| value.as_u64())
        .unwrap_or(1);
    let requested_limit = params
        .get("limit")
        .or_else(|| params.get("max_txs"))
        .and_then(|value| value.as_u64())
        .unwrap_or(native_aoem_batch_max_size_v1() as u64);
    let limit = requested_limit.clamp(1, native_aoem_batch_max_size_v1().max(1) as u64) as usize;
    let scan_limit = params
        .get("scan_limit")
        .and_then(|value| value.as_u64())
        .unwrap_or((limit as u64).saturating_mul(4).max(limit as u64))
        .clamp(limit as u64, 16_384) as usize;
    let native_store_path = resolve_native_execution_store_path_from_params_v1(params)
        .unwrap_or_else(nov_native_execution_store_path_v1);
    let receipt_lookup = NovNativeExecutionReceiptLookupV1::open(native_store_path.as_path())?;
    let pending = snapshot_network_runtime_native_active_pending_txs_v1(chain_id, scan_limit);
    let mut raw_txs = Vec::with_capacity(limit);
    let mut selected_raw_payloads = Vec::with_capacity(limit);
    let mut selected_hash_bytes = Vec::with_capacity(limit);
    let mut selected_hashes = Vec::with_capacity(limit);
    let mut skipped_missing_payload = 0usize;
    let mut skipped_non_native_payload = 0usize;
    let mut skipped_chain_mismatch = 0usize;
    let mut skipped_ineligible_stage = 0usize;
    let mut skipped_already_receipted = 0usize;

    for pending_tx in pending.iter() {
        if raw_txs.len() >= limit {
            break;
        }
        if !native_pending_execution_eligible_stage_v1(pending_tx.lifecycle_stage) {
            skipped_ineligible_stage = skipped_ineligible_stage.saturating_add(1);
            continue;
        }
        let Some(payload) =
            get_network_runtime_native_pending_tx_payload_v1(chain_id, pending_tx.tx_hash)
        else {
            skipped_missing_payload = skipped_missing_payload.saturating_add(1);
            continue;
        };
        let native_tx = match decode_nov_native_tx_wire_v1(payload.as_slice()) {
            Ok(value) => value,
            Err(_) => {
                skipped_non_native_payload = skipped_non_native_payload.saturating_add(1);
                continue;
            }
        };
        if native_tx.chain_id != chain_id {
            skipped_chain_mismatch = skipped_chain_mismatch.saturating_add(1);
            continue;
        }
        let pending_tx_hash = to_hex_prefixed_v1(&pending_tx.tx_hash);
        let pending_tx_hash_noprefix = pending_tx_hash
            .strip_prefix("0x")
            .unwrap_or(pending_tx_hash.as_str());
        if receipt_lookup.contains(pending_tx_hash.as_str())?
            || receipt_lookup.contains(pending_tx_hash_noprefix)?
        {
            skipped_already_receipted = skipped_already_receipted.saturating_add(1);
            observe_network_runtime_native_pending_tx_dropped_v1(chain_id, pending_tx.tx_hash);
            continue;
        }
        raw_txs.push(serde_json::Value::String(to_hex_prefixed_v1(
            payload.as_slice(),
        )));
        selected_raw_payloads.push(payload);
        selected_hash_bytes.push(pending_tx.tx_hash);
        selected_hashes.push(to_hex_prefixed_v1(&pending_tx.tx_hash));
    }

    if raw_txs.is_empty() {
        return Ok(serde_json::json!({
            "method": "nov_executePendingNativeTxBatch",
            "accepted": true,
            "execution_kernel": "AOEM",
            "source": "network_runtime_native_pending",
            "chain_id": chain_id,
            "requested_limit": requested_limit,
            "effective_limit": limit,
            "scan_limit": scan_limit,
            "pending_scanned": pending.len(),
            "selected_count": 0,
            "executed": false,
            "reason": "no_eligible_native_pending_payload",
            "skipped": {
                "missing_payload": skipped_missing_payload,
                "non_native_payload": skipped_non_native_payload,
                "chain_mismatch": skipped_chain_mismatch,
                "ineligible_stage": skipped_ineligible_stage,
                "already_receipted": skipped_already_receipted,
            }
        }));
    }
    drop(receipt_lookup);

    let mut batch_params = params.clone();
    let obj = batch_params.as_object_mut().ok_or_else(|| {
        anyhow::anyhow!("nov_executePendingNativeTxBatch params must be an object")
    })?;
    obj.insert("raw_txs".to_string(), serde_json::Value::Array(raw_txs));
    let batch_result = run_nov_send_raw_transaction_batch_from_params_v1(&batch_params)?;
    let canonical_projection = project_executed_native_pending_batch_to_canonical_v1(
        chain_id,
        selected_hash_bytes.as_slice(),
        selected_raw_payloads.as_slice(),
    );
    Ok(serde_json::json!({
        "method": "nov_executePendingNativeTxBatch",
        "accepted": true,
        "execution_kernel": "AOEM",
        "source": "network_runtime_native_pending",
        "chain_id": chain_id,
        "requested_limit": requested_limit,
        "effective_limit": limit,
        "scan_limit": scan_limit,
        "pending_scanned": pending.len(),
        "selected_count": selected_hashes.len(),
        "selected_tx_hashes": selected_hashes,
        "executed": true,
        "canonical_projection": canonical_projection,
        "skipped": {
            "missing_payload": skipped_missing_payload,
            "non_native_payload": skipped_non_native_payload,
            "chain_mismatch": skipped_chain_mismatch,
            "ineligible_stage": skipped_ineligible_stage,
            "already_receipted": skipped_already_receipted,
        },
        "batch_result": batch_result,
    }))
}

pub fn run_nov_native_execution_tick_from_params_v1(
    params: &serde_json::Value,
) -> Result<serde_json::Value> {
    let chain_id = params
        .get("chain_id")
        .and_then(|value| value.as_u64())
        .unwrap_or(1);
    let max_aoem_batch = native_aoem_batch_max_size_v1().max(1) as u64;
    let hard_budget = params
        .get("hard_budget_per_tick")
        .or_else(|| params.get("hard_max_txs"))
        .and_then(|value| value.as_u64())
        .unwrap_or(max_aoem_batch)
        .clamp(1, max_aoem_batch);
    let target_budget = params
        .get("target_budget_per_tick")
        .or_else(|| params.get("limit"))
        .or_else(|| params.get("max_txs"))
        .and_then(|value| value.as_u64())
        .unwrap_or(hard_budget)
        .clamp(1, hard_budget);
    let effective_budget = params
        .get("effective_budget_per_tick")
        .and_then(|value| value.as_u64())
        .unwrap_or(target_budget)
        .clamp(1, hard_budget);
    let hard_time_slice_ms = params
        .get("hard_time_slice_ms")
        .and_then(|value| value.as_u64())
        .unwrap_or(250)
        .max(1);
    let target_time_slice_ms = params
        .get("target_time_slice_ms")
        .and_then(|value| value.as_u64())
        .unwrap_or(hard_time_slice_ms)
        .clamp(1, hard_time_slice_ms);
    let effective_time_slice_ms = params
        .get("effective_time_slice_ms")
        .and_then(|value| value.as_u64())
        .unwrap_or(target_time_slice_ms)
        .clamp(1, hard_time_slice_ms);
    let scan_limit = params
        .get("scan_limit")
        .and_then(|value| value.as_u64())
        .unwrap_or(effective_budget.saturating_mul(4).max(effective_budget))
        .clamp(effective_budget, 16_384);
    let pending_before =
        snapshot_network_runtime_native_active_pending_txs_v1(chain_id, scan_limit as usize);
    let eligible_before = pending_before
        .iter()
        .filter(|pending| native_pending_execution_eligible_stage_v1(pending.lifecycle_stage))
        .count() as u64;
    let deferred_count = eligible_before.saturating_sub(effective_budget);
    let started_at = Instant::now();

    observe_network_runtime_native_execution_budget_target_v1(
        chain_id,
        &NetworkRuntimeNativeExecutionBudgetTargetObservationV1 {
            hard_budget_per_tick: hard_budget,
            hard_time_slice_ms,
            target_budget_per_tick: target_budget,
            target_time_slice_ms,
            effective_budget_per_tick: effective_budget,
            effective_time_slice_ms,
            reason: Some("mainline_native_execution_tick".to_string()),
        },
    );
    if deferred_count > 0 {
        observe_network_runtime_native_execution_budget_throttle_v1(
            chain_id,
            "native_execution_tick_budget_deferred",
            deferred_count,
            true,
            false,
        );
    }

    let mut batch_params = params.clone();
    let obj = batch_params
        .as_object_mut()
        .ok_or_else(|| anyhow::anyhow!("nov_runNativeExecutionTick params must be an object"))?;
    obj.insert(
        "limit".to_string(),
        serde_json::Value::Number(serde_json::Number::from(effective_budget)),
    );
    obj.insert(
        "scan_limit".to_string(),
        serde_json::Value::Number(serde_json::Number::from(scan_limit)),
    );
    let batch_result = run_nov_execute_pending_native_tx_batch_from_params_v1(&batch_params)?;
    let elapsed_ms = started_at.elapsed().as_millis() as u64;
    let time_slice_exceeded = elapsed_ms > effective_time_slice_ms;
    if time_slice_exceeded {
        observe_network_runtime_native_execution_budget_throttle_v1(
            chain_id,
            "native_execution_tick_time_slice_exceeded",
            0,
            false,
            true,
        );
    }
    let executed_count = batch_result
        .get("selected_count")
        .and_then(|value| value.as_u64())
        .unwrap_or_default();
    let budget_runtime =
        snapshot_network_runtime_native_execution_budget_runtime_summary_v1(chain_id);

    Ok(serde_json::json!({
        "method": "nov_runNativeExecutionTick",
        "accepted": true,
        "scheduler_mode": "mainline_native_execution_tick",
        "background_daemon": false,
        "execution_kernel": "AOEM",
        "aoem_concurrency_owner": "AOEM_runtime",
        "lifecycle": {
            "ingress": "network_runtime_native_pending",
            "execution": "aoem_batch_precommit",
            "commit": "deterministic_sharded_dirty_atomic_commit",
            "egress": "native_receipt_and_state_projection",
        },
        "chain_id": chain_id,
        "pending_before": pending_before.len(),
        "eligible_before": eligible_before,
        "executed_count": executed_count,
        "deferred_count": deferred_count,
        "elapsed_ms": elapsed_ms,
        "time_slice_exceeded": time_slice_exceeded,
        "budget": {
            "hard_budget_per_tick": hard_budget,
            "target_budget_per_tick": target_budget,
            "effective_budget_per_tick": effective_budget,
            "hard_time_slice_ms": hard_time_slice_ms,
            "target_time_slice_ms": target_time_slice_ms,
            "effective_time_slice_ms": effective_time_slice_ms,
            "scan_limit": scan_limit,
        },
        "budget_runtime": {
            "hard_budget_per_tick": budget_runtime.hard_budget_per_tick,
            "target_budget_per_tick": budget_runtime.target_budget_per_tick,
            "effective_budget_per_tick": budget_runtime.effective_budget_per_tick,
            "execution_budget_hit_count": budget_runtime.execution_budget_hit_count,
            "execution_deferred_count": budget_runtime.execution_deferred_count,
            "execution_time_slice_exceeded_count": budget_runtime.execution_time_slice_exceeded_count,
            "last_execution_target_reason": budget_runtime.last_execution_target_reason,
            "last_execution_throttle_reason": budget_runtime.last_execution_throttle_reason,
        },
        "batch_result": batch_result,
    }))
}

pub fn run_nov_send_transaction_from_params_v1(
    params: &serde_json::Value,
) -> Result<serde_json::Value> {
    let tx_value = params
        .get("tx")
        .cloned()
        .or_else(|| params.as_array().and_then(|items| items.first()).cloned())
        .ok_or_else(|| anyhow::anyhow!("tx is required for nov_sendTransaction"))?;
    let tx: NovNativeTxWireV1 = serde_json::from_value(tx_value)
        .map_err(|err| anyhow::anyhow!("nov_sendTransaction tx decode failed: {err}"))?;
    let encoded = encode_nov_native_tx_wire_v1(&tx)
        .map_err(|err| anyhow::anyhow!("nov_sendTransaction tx encode failed: {err}"))?;
    let mut merged = params.clone();
    if let Some(obj) = merged.as_object_mut() {
        obj.insert(
            "raw_tx".to_string(),
            serde_json::Value::String(to_hex_prefixed_v1(encoded.as_slice())),
        );
        obj.remove("tx");
    } else {
        merged = serde_json::json!({
            "raw_tx": to_hex_prefixed_v1(encoded.as_slice()),
            "chain_id": tx.chain_id,
        });
    }
    run_nov_send_raw_transaction_from_params_v1(&merged)
}

fn param_as_u64_from_value(params: &serde_json::Value, key: &str) -> Option<u64> {
    params.get(key).and_then(|value| match value {
        serde_json::Value::Number(num) => num.as_u64(),
        serde_json::Value::String(raw) => {
            let trimmed = raw.trim();
            if let Some(hex) = trimmed
                .strip_prefix("0x")
                .or_else(|| trimmed.strip_prefix("0X"))
            {
                u64::from_str_radix(hex, 16).ok()
            } else {
                trimmed.parse::<u64>().ok()
            }
        }
        _ => None,
    })
}

fn parse_nov_mode_v1(raw: Option<&str>) -> NovExecutionModeV1 {
    match raw.unwrap_or("standard").to_ascii_lowercase().as_str() {
        "high_priority" | "high-priority" => NovExecutionModeV1::HighPriority,
        "batch" => NovExecutionModeV1::Batch,
        _ => NovExecutionModeV1::Standard,
    }
}

fn parse_nov_execution_policy_v1(raw: Option<&str>) -> NovExecutionPolicyV1 {
    match raw.unwrap_or("standard").to_ascii_lowercase().as_str() {
        "pqrequired" | "pq_required" | "pq-required" => NovExecutionPolicyV1::PqRequired,
        "privacyrequired" | "privacy_required" | "privacy-required" => {
            NovExecutionPolicyV1::PrivacyRequired
        }
        _ => NovExecutionPolicyV1::Standard,
    }
}

fn parse_nov_privacy_mode_v1(raw: Option<&str>) -> NovPrivacyModeV1 {
    match raw.unwrap_or("public").to_ascii_lowercase().as_str() {
        "private" => NovPrivacyModeV1::Private,
        "confidential" => NovPrivacyModeV1::Confidential,
        _ => NovPrivacyModeV1::Public,
    }
}

fn tx_execution_policy_from_nov_v1(policy: NovExecutionPolicyV1) -> TxExecutionPolicyV1 {
    match policy {
        NovExecutionPolicyV1::Standard => TxExecutionPolicyV1::Standard,
        NovExecutionPolicyV1::PqRequired => TxExecutionPolicyV1::PqRequired,
        NovExecutionPolicyV1::PrivacyRequired => TxExecutionPolicyV1::PrivacyRequired,
    }
}

fn parse_nov_verification_mode_v1(raw: Option<&str>) -> NovVerificationModeV1 {
    match raw.unwrap_or("standard").to_ascii_lowercase().as_str() {
        "auditable" => NovVerificationModeV1::Auditable,
        "mandatoryzk" | "mandatory_zk" | "mandatory-zk" => NovVerificationModeV1::MandatoryZk,
        _ => NovVerificationModeV1::Standard,
    }
}

fn parse_nov_execution_target_v1(target: &serde_json::Value) -> NovExecutionTargetV1 {
    if let Some(raw) = target.as_str() {
        return NovExecutionTargetV1::NativeModule(raw.to_string());
    }
    let kind = target
        .get("kind")
        .and_then(|value| value.as_str())
        .unwrap_or("native_module")
        .to_ascii_lowercase();
    let id = target
        .get("id")
        .and_then(|value| value.as_str())
        .or_else(|| target.get("value").and_then(|value| value.as_str()))
        .unwrap_or("default")
        .to_string();
    match kind.as_str() {
        "plugin" => NovExecutionTargetV1::Plugin(id),
        "wasm_app" | "wasm" => NovExecutionTargetV1::WasmApp(id),
        _ => NovExecutionTargetV1::NativeModule(id),
    }
}

fn parse_nov_signature_v1(params: &serde_json::Value) -> Result<[u8; 32]> {
    let Some(raw_sig) = params.get("signature").and_then(|value| value.as_str()) else {
        return Ok([0u8; 32]);
    };
    let sig = decode_eth_send_raw_hex_payload_v1(raw_sig, "signature")?;
    if sig.len() != 32 {
        bail!("signature must be 32 bytes, got={}", sig.len());
    }
    let mut out = [0u8; 32];
    out.copy_from_slice(&sig);
    Ok(out)
}

pub fn run_nov_execute_from_params_v1(params: &serde_json::Value) -> Result<serde_json::Value> {
    let caller_raw = params
        .get("caller")
        .and_then(|value| value.as_str())
        .or_else(|| params.get("from").and_then(|value| value.as_str()))
        .ok_or_else(|| anyhow::anyhow!("caller/from is required for nov_execute"))?;
    let caller = decode_eth_send_raw_hex_payload_v1(caller_raw, "caller")?;
    let caller_account_id = normalize_subject_account_ref_v1(caller_raw)?;
    let account_id = params
        .get("account_id")
        .or_else(|| params.get("uca_id"))
        .and_then(|value| value.as_str())
        .map(normalize_subject_account_ref_v1)
        .transpose()?
        .unwrap_or_else(|| caller_account_id.clone());
    let fee_owner_account_id = params
        .get("fee_owner_account_id")
        .and_then(|value| value.as_str())
        .map(normalize_subject_account_ref_v1)
        .transpose()?
        .unwrap_or_else(|| account_id.clone());
    let nonce_owner_account_id = params
        .get("nonce_owner_account_id")
        .and_then(|value| value.as_str())
        .map(normalize_subject_account_ref_v1)
        .transpose()?
        .unwrap_or_else(|| account_id.clone());

    let method = params
        .get("method_name")
        .and_then(|value| value.as_str())
        .or_else(|| params.get("method").and_then(|value| value.as_str()))
        .ok_or_else(|| anyhow::anyhow!("method is required for nov_execute"))?
        .to_string();
    let args = if let Some(raw_args_hex) = params.get("args_hex").and_then(|value| value.as_str()) {
        decode_eth_send_raw_hex_payload_v1(raw_args_hex, "args_hex")?
    } else if let Some(args_val) = params.get("args") {
        serde_json::to_vec(args_val)
            .map_err(|err| anyhow::anyhow!("args serialization failed: {err}"))?
    } else {
        Vec::new()
    };
    let target = parse_nov_execution_target_v1(
        params
            .get("target")
            .unwrap_or(&serde_json::Value::String("default".to_string())),
    );
    let chain_id = param_as_u64_from_value(params, "chain_id").unwrap_or(1);
    let nonce = param_as_u64_from_value(params, "nonce").unwrap_or(0);
    let gas_like_limit = param_as_u64_from_value(params, "gas_like_limit")
        .or_else(|| param_as_u64_from_value(params, "gas_limit"));
    let execution_mode = parse_nov_mode_v1(
        params
            .get("execution_mode")
            .and_then(|value| value.as_str()),
    );
    let execution_policy = parse_nov_execution_policy_v1(
        params
            .get("execution_policy")
            .and_then(|value| value.as_str()),
    );
    let privacy_mode =
        parse_nov_privacy_mode_v1(params.get("privacy_mode").and_then(|value| value.as_str()));
    let verification_mode = parse_nov_verification_mode_v1(
        params
            .get("verification_mode")
            .and_then(|value| value.as_str()),
    );
    let fee_policy = if let Some(policy_obj) = params.get("fee_policy") {
        serde_json::from_value::<NovFeePolicyV1>(policy_obj.clone())
            .map_err(|err| anyhow::anyhow!("fee_policy decode failed: {err}"))?
    } else {
        NovFeePolicyV1 {
            pay_asset: params
                .get("pay_asset")
                .and_then(|value| value.as_str())
                .unwrap_or("NOV")
                .to_string(),
            max_pay_amount: params
                .get("max_pay_amount")
                .and_then(|value| value.as_u64())
                .map(u128::from)
                .unwrap_or(0),
            slippage_bps: params
                .get("slippage_bps")
                .and_then(|value| value.as_u64())
                .map(|value| value as u32)
                .unwrap_or(100),
        }
    };
    let effective_execution_policy = effective_execution_policy_for_fee_asset_v1(
        execution_policy,
        fee_policy.pay_asset.as_str(),
    );
    let tx = NovNativeTxWireV1 {
        chain_id,
        kind: NovTxKindV1::Execute(NovExecuteTxV1 {
            caller,
            account_id: Some(account_id.clone()),
            fee_owner_account_id: Some(fee_owner_account_id.clone()),
            nonce_owner_account_id: Some(nonce_owner_account_id.clone()),
            target,
            method,
            args,
            execution_mode,
            execution_policy: effective_execution_policy,
            privacy_mode,
            verification_mode,
            fee_policy,
            gas_like_limit,
            nonce,
        }),
        signature: parse_nov_signature_v1(params)?,
    };
    let raw = encode_nov_native_tx_wire_v1(&tx)
        .map_err(|err| anyhow::anyhow!("nov_execute encode failed: {err}"))?;
    let mut merged = serde_json::json!({
        "raw_tx": to_hex_prefixed_v1(raw.as_slice()),
        "chain_id": chain_id,
        "caller": caller_raw,
        "account_id": account_id,
        "fee_owner_account_id": fee_owner_account_id,
        "nonce_owner_account_id": nonce_owner_account_id,
    });
    if let Some(path) = params
        .get("native_execution_store_path")
        .and_then(|value| value.as_str())
    {
        if let Some(obj) = merged.as_object_mut() {
            obj.insert(
                "native_execution_store_path".to_string(),
                serde_json::Value::String(path.to_string()),
            );
        }
    }
    if let Some(path) = params
        .get("unified_account_store_path")
        .or_else(|| params.get("ua_store_path"))
        .and_then(|value| value.as_str())
    {
        if let Some(obj) = merged.as_object_mut() {
            obj.insert(
                "unified_account_store_path".to_string(),
                serde_json::Value::String(path.to_string()),
            );
        }
    }
    for key in [
        "pipeline_only",
        "pipelineOnly",
        "pending_only",
        "pendingOnly",
    ] {
        if let Some(value) = params.get(key) {
            if let Some(obj) = merged.as_object_mut() {
                obj.insert(key.to_string(), value.clone());
            }
        }
    }
    run_nov_send_raw_transaction_from_params_v1(&merged)
}

fn load_tx_wire_bytes(path: &Path) -> Result<Vec<u8>> {
    let bytes = fs::read(path)
        .with_context(|| format!("failed to read tx wire ingress file {}", path.display()))?;
    if bytes.is_empty() {
        bail!("tx wire ingress file is empty: {}", path.display());
    }
    if !bytes.len().is_multiple_of(LOCAL_TX_WIRE_V1_BYTES) {
        bail!(
            "tx wire ingress size mismatch: bytes={} not multiple of record_len={} (path={})",
            bytes.len(),
            LOCAL_TX_WIRE_V1_BYTES,
            path.display()
        );
    }
    Ok(bytes)
}

pub fn load_payload_bytes(path: &Path) -> Result<Vec<u8>> {
    let bytes = fs::read(path)
        .with_context(|| format!("failed to read ingress file {}", path.display()))?;
    if bytes.is_empty() {
        bail!("ingress file is empty: {}", path.display());
    }
    Ok(bytes)
}

fn parse_ops_wire_v1_op_count(bytes: &[u8]) -> Result<usize> {
    const HEADER_LEN: usize = 5 + 2 + 2 + 4;
    if bytes.len() < HEADER_LEN {
        bail!(
            "ops-wire payload too short: len={} header_len={HEADER_LEN}",
            bytes.len()
        );
    }
    if &bytes[..AOEM_OPS_WIRE_V1_MAGIC.len()] != AOEM_OPS_WIRE_V1_MAGIC {
        bail!("ops-wire magic mismatch");
    }
    let mut cursor = AOEM_OPS_WIRE_V1_MAGIC.len();
    let version = u16::from_le_bytes([bytes[cursor], bytes[cursor + 1]]);
    cursor += 2;
    if version != AOEM_OPS_WIRE_V1_VERSION {
        bail!("ops-wire version mismatch: got={version}, expected={AOEM_OPS_WIRE_V1_VERSION}");
    }
    cursor += 2; // flags
    let count = u32::from_le_bytes([
        bytes[cursor],
        bytes[cursor + 1],
        bytes[cursor + 2],
        bytes[cursor + 3],
    ]) as usize;
    Ok(count)
}

fn encode_local_tx_wire_v1_write_u64le_v1(
    payload: &[u8],
    builder: &mut OpsWireV1Builder,
) -> Result<()> {
    if payload.is_empty() {
        bail!("tx wire payload is empty");
    }
    if !payload.len().is_multiple_of(LOCAL_TX_WIRE_V1_BYTES) {
        bail!(
            "tx wire payload size mismatch: bytes={} not multiple of record_len={}",
            payload.len(),
            LOCAL_TX_WIRE_V1_BYTES
        );
    }

    for (idx, chunk) in payload.chunks_exact(LOCAL_TX_WIRE_V1_BYTES).enumerate() {
        let wire = decode_tx_wire_v1(chunk)
            .with_context(|| format!("decode tx wire failed at record={idx}"))?;
        let key = wire.key.to_le_bytes();
        let value = wire.value.to_le_bytes();
        let plan_id = (wire.account << 32) | wire.nonce.saturating_add(1);
        builder.push(OpsWireOp {
            opcode: 2, // write
            flags: 0,
            reserved: 0,
            key: &key,
            value: &value,
            delta: 0,
            expect_version: None,
            plan_id,
        })?;
    }
    Ok(())
}

fn local_tx_record_codec_registry() -> &'static RawIngressCodecRegistry {
    LOCAL_TX_RECORD_CODEC_REGISTRY.get_or_init(|| {
        let mut registry = RawIngressCodecRegistry::new();
        registry
            .register(
                LOCAL_TX_WIRE_CODEC_WRITE_U64LE_V1,
                encode_local_tx_wire_v1_write_u64le_v1,
            )
            .expect("register local tx record codec");
        registry
    })
}

pub fn available_ingress_codecs() -> Vec<&'static str> {
    local_tx_record_codec_registry().codec_names()
}

pub fn encode_ops_wire_v1_from_payload(codec: &str, payload: &[u8]) -> Result<OpsWirePayload> {
    local_tx_record_codec_registry().encode(codec, payload)
}

pub fn load_ops_wire_v1_payload_file(path: &Path, codec: &str) -> Result<OpsWirePayload> {
    let payload = load_payload_bytes(path)?;
    encode_ops_wire_v1_from_payload(codec, &payload)
}

pub fn load_ops_wire_v1_file(path: &Path) -> Result<OpsWirePayload> {
    let bytes = load_payload_bytes(path)?;
    let op_count = parse_ops_wire_v1_op_count(&bytes)?;
    Ok(OpsWirePayload { bytes, op_count })
}

pub fn load_tx_records_from_wire_file(path: &Path) -> Result<Vec<TxIngressRecord>> {
    let bytes = load_tx_wire_bytes(path)?;

    let mut txs = Vec::with_capacity(bytes.len() / LOCAL_TX_WIRE_V1_BYTES);
    for (idx, chunk) in bytes.chunks_exact(LOCAL_TX_WIRE_V1_BYTES).enumerate() {
        let wire = decode_tx_wire_v1(chunk)
            .with_context(|| format!("decode tx wire failed at record={idx}"))?;
        txs.push(from_tx_wire_v1(&wire));
    }
    if txs.is_empty() {
        bail!(
            "tx wire ingress decoded zero transactions: {}",
            path.display()
        );
    }
    Ok(txs)
}

pub fn build_exec_batch_from_records<F>(
    records: &[TxIngressRecord],
    mut plan_id_for: F,
) -> ExecBatchBuffer
where
    F: FnMut(usize, &TxIngressRecord) -> u64,
{
    let mut keys: Vec<[u8; 8]> = records.iter().map(|rec| rec.key.to_le_bytes()).collect();
    let mut values: Vec<[u8; 8]> = records.iter().map(|rec| rec.value.to_le_bytes()).collect();
    let mut ops = Vec::with_capacity(records.len());

    for (i, ((key, value), rec)) in keys
        .iter_mut()
        .zip(values.iter_mut())
        .zip(records.iter())
        .enumerate()
    {
        ops.push(ExecOpV2 {
            opcode: 2,
            flags: 0,
            reserved: 0,
            key_ptr: key.as_mut_ptr(),
            key_len: key.len() as u32,
            value_ptr: value.as_mut_ptr(),
            value_len: value.len() as u32,
            delta: 0,
            expect_version: u64::MAX,
            plan_id: plan_id_for(i, rec),
        });
    }

    ExecBatchBuffer {
        _keys: keys,
        _values: values,
        ops,
    }
}

pub fn load_exec_batch_from_wire_file<F>(path: &Path, mut plan_id_for: F) -> Result<ExecBatchBuffer>
where
    F: FnMut(usize, &TxIngressRecord) -> u64,
{
    let records = load_tx_records_from_wire_file(path)?;
    Ok(build_exec_batch_from_records(&records, |idx, rec| {
        plan_id_for(idx, rec)
    }))
}

pub fn build_ops_wire_v1_from_records<F>(
    records: &[TxIngressRecord],
    mut plan_id_for: F,
) -> OpsWirePayload
where
    F: FnMut(usize, &TxIngressRecord) -> u64,
{
    let mut builder = OpsWireV1Builder::new();
    for (idx, rec) in records.iter().enumerate() {
        let key = rec.key.to_le_bytes();
        let value = rec.value.to_le_bytes();
        let plan_id = plan_id_for(idx, rec);
        builder
            .push(OpsWireOp {
                opcode: 2, // write
                flags: 0,
                reserved: 0,
                key: &key,
                value: &value,
                delta: 0,
                expect_version: None,
                plan_id,
            })
            .expect("encode local tx records into ops-wire");
    }
    builder.finish()
}

pub fn load_ops_wire_v1_from_tx_wire_file(path: &Path) -> Result<OpsWirePayload> {
    let bytes = load_tx_wire_bytes(path)?;
    let tx_count = bytes.len() / LOCAL_TX_WIRE_V1_BYTES;
    if tx_count == 0 {
        bail!(
            "tx wire ingress decoded zero transactions: {}",
            path.display()
        );
    }
    encode_ops_wire_v1_from_payload(LOCAL_TX_WIRE_CODEC_WRITE_U64LE_V1, &bytes)
}

#[cfg(test)]
mod tests {
    use super::*;
    use novovm_protocol::{
        NovExecutionModeV1, NovFeePolicyV1, NovNativeTxWireV1, NovPrivacyModeV1, NovTxKindV1,
        NovVerificationModeV1,
    };

    fn with_test_native_execution_store_path_v1<F, T>(test_fn: F) -> T
    where
        F: FnOnce(std::path::PathBuf) -> T,
    {
        let mut path = std::env::temp_dir();
        let nonce = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .expect("clock should be >= epoch")
            .as_nanos();
        path.push(format!("novovm-native-exec-store-{}.json", nonce));
        let out = test_fn(path.clone());
        let lock_path = nov_native_execution_store_lock_path_v1(path.as_path());
        let mirror_path = nov_native_aoem_semantic_ledger_mirror_path_v1(path.as_path());
        let rocksdb_path = nov_native_execution_store_rocksdb_path_v1(path.as_path());
        let _ = fs::remove_file(path);
        let _ = fs::remove_file(lock_path);
        let _ = fs::remove_file(mirror_path);
        let _ = fs::remove_dir_all(rocksdb_path);
        out
    }

    fn with_env_override_v1<F, T>(key: &str, value: &str, test_fn: F) -> T
    where
        F: FnOnce() -> T,
    {
        struct EnvGuard {
            key: String,
            previous: Option<String>,
        }

        impl Drop for EnvGuard {
            fn drop(&mut self) {
                if let Some(previous) = self.previous.take() {
                    std::env::set_var(self.key.as_str(), previous);
                } else {
                    std::env::remove_var(self.key.as_str());
                }
            }
        }

        let previous = std::env::var(key).ok();
        std::env::set_var(key, value);
        let _guard = EnvGuard {
            key: key.to_string(),
            previous,
        };
        test_fn()
    }

    fn test_native_execution_receipt_v1(
        tx_hash: &str,
        account_id: &str,
    ) -> NovNativeExecutionReceiptV1 {
        NovNativeExecutionReceiptV1 {
            tx_hash: tx_hash.to_string(),
            status: true,
            target: "native_module:treasury".to_string(),
            module: "treasury".to_string(),
            method: "deposit_reserve".to_string(),
            account_id: account_id.to_string(),
            fee_owner_account_id: account_id.to_string(),
            nonce_owner_account_id: account_id.to_string(),
            key_algo: "ed25519".to_string(),
            execution_policy: "standard".to_string(),
            policy_enforced: true,
            policy_rejection_reason: None,
            settled_fee_nov: 1,
            paid_asset: "NOV".to_string(),
            paid_amount: 1,
            logs: Vec::new(),
            failure_reason: None,
            fee_contract: NOV_EXECUTION_FEE_CLASSIFICATION_CONTRACT_V1.to_string(),
            fee_route: "native".to_string(),
            fee_quote_id: "q-test".to_string(),
            fee_quote_contract: NOV_EXECUTION_FEE_QUOTE_CONTRACT_V1.to_string(),
            fee_clearing_contract: NOV_EXECUTION_FEE_CLEARING_CONTRACT_V1.to_string(),
            fee_price_source: "protocol".to_string(),
            fee_quote_required_pay_amount: 1,
            fee_quote_expires_at_unix_ms: 9999,
            fee_clearing_route_ref: "route:test".to_string(),
            fee_clearing_source: "treasury".to_string(),
            fee_clearing_rate_ppm: 1_000_000,
            route_meta: None,
            policy_meta: None,
            aoem_semantic_ingress: None,
            aoem_semantic_commit: None,
        }
    }

    #[test]
    fn native_policy_state_projection_includes_protocol_clearing_and_oracle_policy() {
        let before = NovNativeExecutionModuleStateV1::default();
        let mut after = before.clone();
        after.clearing_enabled = false;
        after.clearing_constrained_strategy = "treasury_direct_only".to_string();
        after
            .protocol_clearing_nav_rate_ppm
            .insert("NETH".to_string(), 1_500_000);
        after
            .protocol_clearing_amm_twap_rate_ppm
            .insert("NETH".to_string(), 1_490_000);
        after.fee_oracle_source = "governance_oracle".to_string();
        after
            .fee_oracle_rates_ppm
            .insert("NETH".to_string(), 1_510_000);
        after.fee_oracle_updated_unix_ms = 1_000;
        after
            .fee_oracle_allowed_sources
            .push("governance_oracle".to_string());
        after.clearing_static_amm_pools.insert(
            "NETH/NOV".to_string(),
            NovStaticAmmPoolStateV1 {
                pool_id: "NETH/NOV".to_string(),
                asset_x: "NETH".to_string(),
                asset_y: "NOV".to_string(),
                reserve_x: 10_000,
                reserve_y: 15_000,
                swap_fee_ppm: 3_000,
                enabled: true,
            },
        );

        let projection = native_policy_state_projection_v1(&after);
        assert_eq!(
            projection["protocol_clearing_policy"]["clearing_enabled"].as_bool(),
            Some(false)
        );
        assert_eq!(
            projection["protocol_clearing_policy"]["clearing_constrained_strategy"].as_str(),
            Some("treasury_direct_only")
        );
        assert_eq!(
            projection["protocol_clearing_anchors"]["nav_rate_ppm"]["NETH"].as_u64(),
            Some(1_500_000)
        );
        assert_eq!(
            projection["protocol_clearing_anchors"]["amm_twap_rate_ppm"]["NETH"].as_u64(),
            Some(1_490_000)
        );
        assert_eq!(
            projection["permissioned_oracle_policy"]["source"].as_str(),
            Some("governance_oracle")
        );
        assert_eq!(
            projection["permissioned_oracle_policy"]["rates_ppm"]["NETH"].as_u64(),
            Some(1_510_000)
        );

        let deltas = build_native_execution_semantic_deltas_v1(&before, &after);
        assert!(
            deltas
                .iter()
                .any(|delta| delta["kind"].as_str() == Some("native_policy_state")),
            "clearing/oracle policy changes must be covered by AOEM native_policy_state delta"
        );
    }

    #[test]
    fn native_execution_store_write_lock_is_scoped_to_dispatch() {
        with_test_native_execution_store_path_v1(|path| {
            let lock_path = nov_native_execution_store_lock_path_v1(path.as_path());
            {
                let _guard = acquire_nov_native_execution_store_write_lock_v1(path.as_path())
                    .expect("write lock should be acquired");
                assert!(
                    lock_path.exists(),
                    "lock file should exist while guard lives"
                );
            }
            assert!(
                !lock_path.exists(),
                "lock file should be removed when guard drops"
            );

            let request = NovExecutionRequestV1 {
                tx_hash: [0x92; 32],
                chain_id: 7092,
                caller: vec![0x92; 20],
                target: NovExecutionRequestTargetV1::NativeModule("treasury".to_string()),
                method: "deposit_reserve".to_string(),
                args: serde_json::to_vec(&serde_json::json!({
                    "asset": "NOV",
                    "amount": 1u64
                }))
                .expect("encode args"),
                fee_pay_asset: "NOV".to_string(),
                fee_max_pay_amount: 10_000,
                fee_slippage_bps: 50,
                gas_like_limit: Some(90_000),
                nonce: 92,
            };
            let receipt = dispatch_and_persist_nov_execution_request_with_store_path_v1(
                path.as_path(),
                &request,
            )
            .expect("dispatch should succeed under write lock guard");
            assert!(receipt.status);
            assert!(
                !lock_path.exists(),
                "dispatch should release native execution store write lock"
            );
            let stored = load_nov_native_execution_store_v1(path.as_path())
                .expect("store should remain readable after dispatch");
            assert!(stored.receipts.contains_key(receipt.tx_hash.as_str()));
        });
    }

    #[test]
    fn native_execution_store_rocksdb_backend_roundtrips_receipts_and_semantic_head() {
        with_env_override_v1(
            NOV_NATIVE_EXECUTION_STORE_BACKEND_ENV,
            NOV_NATIVE_EXECUTION_STORE_BACKEND_ROCKSDB_V1,
            || {
                with_env_override_v1(
                    NOV_NATIVE_AOEM_SEMANTIC_INGRESS_ENABLED_ENV,
                    "false",
                    || {
                        with_test_native_execution_store_path_v1(|path| {
                            let request = NovExecutionRequestV1 {
                                tx_hash: [0xc7; 32],
                                chain_id: 7092,
                                caller: vec![0xc7; 20],
                                target: NovExecutionRequestTargetV1::NativeModule(
                                    "treasury".to_string(),
                                ),
                                method: "deposit_reserve".to_string(),
                                args: serde_json::to_vec(&serde_json::json!({
                                    "asset": "USDT",
                                    "amount": 9u64
                                }))
                                .expect("encode args"),
                                fee_pay_asset: "USDT".to_string(),
                                fee_max_pay_amount: 10_000,
                                fee_slippage_bps: 50,
                                gas_like_limit: Some(90_000),
                                nonce: 901,
                            };
                            let receipt =
                                dispatch_and_persist_nov_execution_request_with_store_path_v1(
                                    path.as_path(),
                                    &request,
                                )
                                .expect("dispatch should persist through rocksdb backend");
                            assert_eq!(receipt.status, true);
                            assert!(
                                !path.exists(),
                                "rocksdb backend must not write the legacy json snapshot"
                            );
                            let rocksdb_path =
                                nov_native_execution_store_rocksdb_path_v1(path.as_path());
                            assert!(
                                rocksdb_path.exists(),
                                "rocksdb backend directory should exist"
                            );

                            let loaded = load_nov_native_execution_store_v1(path.as_path())
                                .expect("rocksdb store should load");
                            assert_eq!(
                                loaded
                                    .receipts
                                    .get(receipt.tx_hash.as_str())
                                    .map(|item| item.method.as_str()),
                                Some("deposit_reserve")
                            );
                            assert_eq!(loaded.module_state.aoem_semantic_ledger_sequence, 1);
                            assert_eq!(
                                loaded.module_state.aoem_semantic_ledger_head,
                                receipt
                                    .aoem_semantic_ingress
                                    .as_ref()
                                    .expect("aoem ingress metadata")
                                    .semantic_ledger_commit_seal
                            );
                        })
                    },
                )
            },
        );
    }

    #[test]
    fn native_execution_store_rocksdb_sharded_commit_writes_materialized_keyspaces() {
        with_env_override_v1(
            NOV_NATIVE_EXECUTION_STORE_BACKEND_ENV,
            NOV_NATIVE_EXECUTION_STORE_BACKEND_ROCKSDB_V1,
            || {
                with_test_native_execution_store_path_v1(|path| {
                    let mut store = NovNativeExecutionStoreV1::default();
                    store.last_updated_unix_ms = 1234;
                    store
                        .module_state
                        .account_asset_balances
                        .entry("acct-shard".to_string())
                        .or_default()
                        .insert("NETH".to_string(), 42);
                    store.module_state.aoem_semantic_ledger_sequence = 7;
                    store.module_state.aoem_semantic_ledger_head = "head-shard-7".to_string();
                    let receipt = NovNativeExecutionReceiptV1 {
                        tx_hash: "c8".repeat(32),
                        status: true,
                        target: "native_module:treasury".to_string(),
                        module: "treasury".to_string(),
                        method: "deposit_reserve".to_string(),
                        account_id: "acct-shard".to_string(),
                        fee_owner_account_id: "acct-shard".to_string(),
                        nonce_owner_account_id: "acct-shard".to_string(),
                        key_algo: "ed25519".to_string(),
                        execution_policy: "standard".to_string(),
                        policy_enforced: true,
                        policy_rejection_reason: None,
                        settled_fee_nov: 1,
                        paid_asset: "NOV".to_string(),
                        paid_amount: 1,
                        logs: Vec::new(),
                        failure_reason: None,
                        fee_contract: NOV_EXECUTION_FEE_CLASSIFICATION_CONTRACT_V1.to_string(),
                        fee_route: "native".to_string(),
                        fee_quote_id: "q-shard".to_string(),
                        fee_quote_contract: NOV_EXECUTION_FEE_QUOTE_CONTRACT_V1.to_string(),
                        fee_clearing_contract: NOV_EXECUTION_FEE_CLEARING_CONTRACT_V1.to_string(),
                        fee_price_source: "protocol".to_string(),
                        fee_quote_required_pay_amount: 1,
                        fee_quote_expires_at_unix_ms: 9999,
                        fee_clearing_route_ref: "route:shard".to_string(),
                        fee_clearing_source: "treasury".to_string(),
                        fee_clearing_rate_ppm: 1_000_000,
                        route_meta: None,
                        policy_meta: None,
                        aoem_semantic_ingress: None,
                        aoem_semantic_commit: None,
                    };
                    store
                        .receipts
                        .insert(receipt.tx_hash.clone(), receipt.clone());
                    save_nov_native_execution_store_v1(path.as_path(), &store)
                        .expect("save rocksdb sharded store");

                    assert!(!path.exists(), "rocksdb mode must not write json snapshot");
                    let rocksdb_path = nov_native_execution_store_rocksdb_path_v1(path.as_path());
                    let db = open_nov_native_execution_store_rocksdb_v1(rocksdb_path.as_path())
                        .expect("open rocksdb");
                    assert!(
                        db.get(NOV_NATIVE_EXECUTION_STORE_ROCKSDB_KEY_SNAPSHOT_V1)
                            .expect("read legacy snapshot key")
                            .is_none(),
                        "new sharded commit must not keep the legacy whole-store snapshot blob"
                    );
                    assert!(
                        db.get(NOV_NATIVE_EXECUTION_STORE_ROCKSDB_KEY_MODULE_STATE_CORE_V1)
                            .expect("read legacy module_state/core key")
                            .is_none(),
                        "dirty sharded commit must not keep module_state/core as production state"
                    );
                    assert!(db
                        .get(
                            NOV_NATIVE_EXECUTION_STORE_ROCKSDB_KEY_MODULE_STATE_NATIVE_EXECUTION_V1
                        )
                        .expect("read native execution module shard")
                        .is_some());
                    assert!(db
                        .get(native_rocksdb_account_asset_key_v1("acct-shard", "NETH"))
                        .expect("read account asset shard")
                        .is_some());
                    assert!(db
                        .get(native_rocksdb_receipt_key_v1(receipt.tx_hash.as_str()))
                        .expect("read receipt shard")
                        .is_some());
                    assert!(db
                        .get(native_rocksdb_receipt_by_height_key_v1(
                            7,
                            0,
                            receipt.tx_hash.as_str()
                        ))
                        .expect("read receipt by height shard")
                        .is_some());
                    assert_eq!(
                        db.get(NOV_NATIVE_EXECUTION_STORE_ROCKSDB_KEY_SEMANTIC_HEAD_V1)
                            .expect("read semantic head")
                            .as_deref(),
                        Some("head-shard-7".as_bytes())
                    );
                    assert!(db
                        .get(NOV_NATIVE_EXECUTION_STORE_ROCKSDB_SNAPSHOT_META_CURRENT_V1)
                        .expect("read snapshot meta")
                        .is_some());
                    drop(db);

                    let loaded = load_nov_native_execution_store_v1(path.as_path())
                        .expect("materialized rocksdb load");
                    assert_eq!(loaded, store);
                })
            },
        );
    }

    #[test]
    fn native_execution_store_dirty_set_tracks_only_changed_assets_and_module_shards() {
        let mut previous = NovNativeExecutionStoreV1::default();
        previous
            .module_state
            .account_asset_balances
            .entry("acct-a".to_string())
            .or_default()
            .insert("NETH".to_string(), 10);
        previous
            .module_state
            .account_asset_balances
            .entry("acct-b".to_string())
            .or_default()
            .insert("NUSDT".to_string(), 20);

        let mut next = previous.clone();
        next.module_state
            .account_asset_balances
            .get_mut("acct-a")
            .expect("acct-a exists")
            .insert("NETH".to_string(), 11);
        next.module_state.treasury_reserve_bucket_nov = 100;
        next.module_state.treasury_settlement_paused = true;

        let dirty = native_execution_store_dirty_set_v1(&previous, &next, false)
            .expect("dirty set should build");
        assert_eq!(
            dirty.account_asset_upserts,
            vec![("acct-a".to_string(), "NETH".to_string())]
        );
        assert!(
            !dirty
                .account_asset_upserts
                .contains(&("acct-b".to_string(), "NUSDT".to_string())),
            "unchanged account asset must not be marked dirty"
        );
        assert!(dirty.module_state_shards.contains(&"treasury"));
        assert!(dirty.module_state_shards.contains(&"policy"));
        assert!(!dirty.module_state_shards.contains(&"clearing"));
        assert!(!dirty.module_state_shards.contains(&"native_execution"));
    }

    #[test]
    fn native_execution_store_rocksdb_dirty_module_state_namespaces_materialize() {
        with_env_override_v1(
            NOV_NATIVE_EXECUTION_STORE_BACKEND_ENV,
            NOV_NATIVE_EXECUTION_STORE_BACKEND_ROCKSDB_V1,
            || {
                with_test_native_execution_store_path_v1(|path| {
                    let mut store = NovNativeExecutionStoreV1::default();
                    store.last_updated_unix_ms = 777;
                    store.module_state.treasury_reserve_bucket_nov = 123;
                    store.module_state.treasury_settlement_paused = true;
                    store.module_state.treasury_policy_source = "test-policy".to_string();
                    save_nov_native_execution_store_v1(path.as_path(), &store)
                        .expect("save dirty module namespace store");

                    let rocksdb_path = nov_native_execution_store_rocksdb_path_v1(path.as_path());
                    let db = open_nov_native_execution_store_rocksdb_v1(rocksdb_path.as_path())
                        .expect("open rocksdb");
                    assert!(db
                        .get(NOV_NATIVE_EXECUTION_STORE_ROCKSDB_KEY_MODULE_STATE_TREASURY_V1)
                        .expect("read treasury module shard")
                        .is_some());
                    assert!(db
                        .get(NOV_NATIVE_EXECUTION_STORE_ROCKSDB_KEY_MODULE_STATE_POLICY_V1)
                        .expect("read policy module shard")
                        .is_some());
                    assert!(db
                        .get(NOV_NATIVE_EXECUTION_STORE_ROCKSDB_KEY_MODULE_STATE_CORE_V1)
                        .expect("read legacy module core")
                        .is_none());
                    drop(db);

                    let loaded = load_nov_native_execution_store_v1(path.as_path())
                        .expect("load materialized dirty module namespace store");
                    assert_eq!(loaded, store);
                })
            },
        );
    }

    #[test]
    fn native_execution_store_rocksdb_loaded_previous_commit_deletes_removed_asset() {
        with_env_override_v1(
            NOV_NATIVE_EXECUTION_STORE_BACKEND_ENV,
            NOV_NATIVE_EXECUTION_STORE_BACKEND_ROCKSDB_V1,
            || {
                with_test_native_execution_store_path_v1(|path| {
                    let mut first = NovNativeExecutionStoreV1::default();
                    first
                        .module_state
                        .account_asset_balances
                        .entry("acct-prev".to_string())
                        .or_default()
                        .insert("NETH".to_string(), 100);
                    first
                        .module_state
                        .account_asset_balances
                        .entry("acct-remove".to_string())
                        .or_default()
                        .insert("NUSDT".to_string(), 200);
                    first.module_state.aoem_semantic_ledger_sequence = 1;
                    first.module_state.aoem_semantic_ledger_head = "head-prev-1".to_string();
                    save_nov_native_execution_store_v1(path.as_path(), &first)
                        .expect("seed first rocksdb store");

                    let previous = load_nov_native_execution_store_v1(path.as_path())
                        .expect("load previous rocksdb materialized store");
                    let mut next = previous.clone();
                    next.module_state
                        .account_asset_balances
                        .remove("acct-remove");
                    next.module_state.aoem_semantic_ledger_sequence = 2;
                    next.module_state.aoem_semantic_ledger_head = "head-prev-2".to_string();
                    save_nov_native_execution_store_with_previous_v1(
                        path.as_path(),
                        Some(&previous),
                        &next,
                    )
                    .expect("save with loaded previous should commit dirty delete");

                    let rocksdb_path = nov_native_execution_store_rocksdb_path_v1(path.as_path());
                    let db = open_nov_native_execution_store_rocksdb_v1(rocksdb_path.as_path())
                        .expect("open rocksdb");
                    assert!(db
                        .get(native_rocksdb_account_asset_key_v1("acct-remove", "NUSDT"))
                        .expect("read removed account asset")
                        .is_none());
                    drop(db);

                    let loaded = load_nov_native_execution_store_v1(path.as_path())
                        .expect("load next materialized store");
                    assert_eq!(loaded, next);
                })
            },
        );
    }

    #[test]
    fn native_execution_store_rocksdb_receipt_by_height_is_append_only() {
        with_env_override_v1(
            NOV_NATIVE_EXECUTION_STORE_BACKEND_ENV,
            NOV_NATIVE_EXECUTION_STORE_BACKEND_ROCKSDB_V1,
            || {
                with_test_native_execution_store_path_v1(|path| {
                    let mut first = NovNativeExecutionStoreV1::default();
                    first.module_state.aoem_semantic_ledger_sequence = 1;
                    first.module_state.aoem_semantic_ledger_head = "head-1".to_string();
                    let receipt_one =
                        test_native_execution_receipt_v1("d1".repeat(32).as_str(), "acct-r");
                    first
                        .receipts
                        .insert(receipt_one.tx_hash.clone(), receipt_one.clone());
                    save_nov_native_execution_store_v1(path.as_path(), &first)
                        .expect("save first receipt");

                    let mut second = first.clone();
                    second.module_state.aoem_semantic_ledger_sequence = 2;
                    second.module_state.aoem_semantic_ledger_head = "head-2".to_string();
                    let receipt_two =
                        test_native_execution_receipt_v1("d2".repeat(32).as_str(), "acct-r");
                    second
                        .receipts
                        .insert(receipt_two.tx_hash.clone(), receipt_two.clone());
                    save_nov_native_execution_store_v1(path.as_path(), &second)
                        .expect("save second receipt");

                    let rocksdb_path = nov_native_execution_store_rocksdb_path_v1(path.as_path());
                    let db = open_nov_native_execution_store_rocksdb_v1(rocksdb_path.as_path())
                        .expect("open rocksdb");
                    assert!(db
                        .get(native_rocksdb_receipt_by_height_key_v1(
                            1,
                            0,
                            receipt_one.tx_hash.as_str()
                        ))
                        .expect("read first height receipt index")
                        .is_some());
                    assert!(db
                        .get(native_rocksdb_receipt_by_height_key_v1(
                            2,
                            1,
                            receipt_two.tx_hash.as_str()
                        ))
                        .expect("read second height receipt index")
                        .is_some());
                })
            },
        );
    }

    #[test]
    fn native_execution_store_dual_backend_matches_json_snapshot_and_rocksdb_materialized_view() {
        with_env_override_v1(
            NOV_NATIVE_EXECUTION_STORE_BACKEND_ENV,
            NOV_NATIVE_EXECUTION_STORE_BACKEND_DUAL_V1,
            || {
                with_test_native_execution_store_path_v1(|path| {
                    let mut store = NovNativeExecutionStoreV1::default();
                    store.last_updated_unix_ms = 5678;
                    store
                        .module_state
                        .account_asset_balances
                        .entry("acct-dual".to_string())
                        .or_default()
                        .insert("NUSDT".to_string(), 77);
                    store.module_state.aoem_semantic_ledger_sequence = 8;
                    store.module_state.aoem_semantic_ledger_head = "head-dual-8".to_string();
                    save_nov_native_execution_store_v1(path.as_path(), &store)
                        .expect("save dual store");

                    let json_store = load_nov_native_execution_store_json_v1(path.as_path())
                        .expect("json snapshot should load");
                    let rocksdb_path = nov_native_execution_store_rocksdb_path_v1(path.as_path());
                    let rocksdb_store =
                        load_nov_native_execution_store_rocksdb_v1(rocksdb_path.as_path())
                            .expect("rocksdb materialized store should load");
                    assert_eq!(json_store, store);
                    assert_eq!(rocksdb_store, store);
                })
            },
        );
    }

    #[test]
    fn native_execution_receipt_exposes_aoem_semantic_ingress_metadata() {
        with_env_override_v1(NOV_NATIVE_AOEM_SEMANTIC_INGRESS_ENABLED_ENV, "true", || {
            with_env_override_v1(
                NOV_NATIVE_AOEM_SEMANTIC_INGRESS_REQUIRED_ENV,
                "false",
                || {
                    with_test_native_execution_store_path_v1(|path| {
                        let request = NovExecutionRequestV1 {
                            tx_hash: [0xa0; 32],
                            chain_id: 7092,
                            caller: vec![0xa0; 20],
                            target: NovExecutionRequestTargetV1::NativeModule(
                                "treasury".to_string(),
                            ),
                            method: "deposit_reserve".to_string(),
                            args: serde_json::to_vec(&serde_json::json!({
                                "asset": "NOV",
                                "amount": 3u64
                            }))
                            .expect("encode args"),
                            fee_pay_asset: "NOV".to_string(),
                            fee_max_pay_amount: 10_000,
                            fee_slippage_bps: 50,
                            gas_like_limit: Some(90_000),
                            nonce: 160,
                        };
                        let receipt =
                            dispatch_and_persist_nov_execution_request_with_store_path_v1(
                                path.as_path(),
                                &request,
                            )
                            .expect("dispatch should succeed with AOEM semantic metadata");
                        assert!(receipt.status);
                        let meta = receipt
                            .aoem_semantic_ingress
                            .as_ref()
                            .expect("receipt must expose AOEM semantic ingress metadata");
                        assert_eq!(meta.execution_kernel, "AOEM");
                        assert_eq!(meta.semantic_entry, native_aoem_semantic_entry_v1());
                        assert!(meta.algebraic_semantic_entry);
                        assert!(meta.enabled);
                        assert!(!meta.required);
                        assert_eq!(meta.op_count, 1);
                        assert_ne!(meta.plan_id, 0);
                        assert_eq!(meta.wire_digest.len(), 64);
                        assert!(
                            meta.semantic_delta_count >= 1,
                            "native asset execution should expose semantic deltas"
                        );
                        assert_eq!(meta.semantic_delta_digest.len(), 64);
                        assert_eq!(meta.semantic_state_before_digest.len(), 64);
                        assert_eq!(meta.semantic_state_after_digest.len(), 64);
                        assert_ne!(
                            meta.semantic_state_before_digest,
                            meta.semantic_state_after_digest
                        );
                        assert_eq!(meta.semantic_ledger_sequence, 1);
                        assert!(meta.semantic_ledger_prev_seal.is_empty());
                        assert_eq!(meta.semantic_ledger_commit_seal.len(), 64);
                        let delta_log = receipt
                            .logs
                            .iter()
                            .find(|log| log.event == "aoem.native_asset.semantic_deltas")
                            .expect("receipt should include AOEM native asset semantic delta log");
                        assert_eq!(
                            delta_log.data["semantic_entry"].as_str(),
                            Some(native_aoem_semantic_entry_v1())
                        );
                        assert_eq!(
                            delta_log.data["delta_digest"].as_str(),
                            Some(meta.semantic_delta_digest.as_str())
                        );
                        let commit_log = receipt
                            .logs
                            .iter()
                            .find(|log| log.event == "aoem.native_asset.semantic_ledger_commit")
                            .expect("receipt should include AOEM semantic ledger commit log");
                        assert_eq!(
                            commit_log.data["commit_seal"].as_str(),
                            Some(meta.semantic_ledger_commit_seal.as_str())
                        );
                        assert_eq!(commit_log.data["ledger_sequence"].as_u64(), Some(1));
                        assert_eq!(
                            commit_log.data["state_after_digest"].as_str(),
                            Some(meta.semantic_state_after_digest.as_str())
                        );

                        let stored = load_nov_native_execution_store_v1(path.as_path())
                            .expect("store should be readable");
                        assert_eq!(stored.module_state.aoem_semantic_ledger_sequence, 1);
                        assert_eq!(
                            stored.module_state.aoem_semantic_ledger_head,
                            meta.semantic_ledger_commit_seal
                        );
                        let trace = stored
                            .module_state
                            .last_execution_trace
                            .as_ref()
                            .expect("trace should be persisted");
                        assert_eq!(trace.tx_id, receipt.tx_hash);
                        assert_eq!(
                            trace
                                .aoem_semantic_ingress
                                .as_ref()
                                .map(|value| value.plan_id),
                            Some(meta.plan_id)
                        );
                        assert_eq!(
                            trace
                                .aoem_semantic_ingress
                                .as_ref()
                                .map(|value| value.semantic_delta_digest.clone()),
                            Some(meta.semantic_delta_digest.clone())
                        );
                        assert_eq!(
                            trace
                                .aoem_semantic_ingress
                                .as_ref()
                                .map(|value| value.semantic_ledger_commit_seal.clone()),
                            Some(meta.semantic_ledger_commit_seal.clone())
                        );
                        let summary = run_nov_native_call_from_params_with_store_path_v1(
                            &serde_json::json!({
                                "target": {"kind": "native_module", "id": "treasury"},
                                "method": "get_settlement_summary",
                                "args": {},
                            }),
                            Some(path.as_path()),
                        )
                        .expect("settlement summary should expose AOEM ledger head");
                        assert_eq!(
                            summary["result"]["aoem_semantic_ledger"]["sequence"].as_u64(),
                            Some(1)
                        );
                        assert_eq!(
                            summary["result"]["aoem_semantic_ledger"]["head"].as_str(),
                            Some(meta.semantic_ledger_commit_seal.as_str())
                        );
                    });
                },
            );
        });
    }

    #[test]
    fn native_aoem_semantic_ingress_status_exposes_required_gate() {
        with_env_override_v1(NOV_NATIVE_AOEM_SEMANTIC_INGRESS_ENABLED_ENV, "true", || {
            with_env_override_v1(
                NOV_NATIVE_AOEM_SEMANTIC_INGRESS_REQUIRED_ENV,
                "true",
                || {
                    let status = get_nov_native_aoem_semantic_ingress_status_v1();
                    assert_eq!(
                        status["method"].as_str(),
                        Some("nov_getAoemSemanticIngressStatus")
                    );
                    assert_eq!(status["execution_kernel"].as_str(), Some("AOEM"));
                    assert_eq!(
                        status["semantic_entry"].as_str(),
                        Some(native_aoem_semantic_entry_v1())
                    );
                    assert_eq!(status["algebraic_semantic_entry"].as_bool(), Some(true));
                    assert_eq!(status["concurrent_execution_enabled"].as_bool(), Some(true));
                    assert_eq!(
                        status["native_batch_entry"].as_str(),
                        Some("nov_sendRawTransactionBatch")
                    );
                    assert!(status["recommended_threads"].as_u64().unwrap_or_default() >= 1);
                    assert!(
                        status["max_batch_size"].as_u64().unwrap_or_default()
                            >= NOV_NATIVE_AOEM_BATCH_MAX_SIZE_DEFAULT_V1 as u64
                    );
                    assert_eq!(status["enabled"].as_bool(), Some(true));
                    assert_eq!(status["required"].as_bool(), Some(true));
                    assert_eq!(status["fail_closed"].as_bool(), Some(true));
                    assert_eq!(status["fallback_allowed"].as_bool(), Some(false));
                    assert_eq!(
                        status["fallback_policy"].as_str(),
                        Some("fail_closed_on_unavailable")
                    );
                    assert_eq!(
                        status["storage_fallback_boundary"].as_str(),
                        Some(
                            "json_store_lock_is_transitional_persistence_guard_not_aoem_concurrency_model"
                        )
                    );
                },
            );
        });
    }

    #[test]
    fn run_nov_send_raw_transaction_batch_uses_aoem_batch_ingress_then_commits_ordered_results() {
        with_env_override_v1(
            NOV_NATIVE_AOEM_SEMANTIC_INGRESS_ENABLED_ENV,
            "false",
            || {
                with_test_native_execution_store_path_v1(|path| {
                    let build_raw = |nonce: u64, account: &str, amount: u64| {
                        let native_tx = NovNativeTxWireV1 {
                            chain_id: 77,
                            kind: NovTxKindV1::Execute(novovm_protocol::NovExecuteTxV1 {
                                caller: vec![nonce as u8; 20],
                                account_id: Some(account.to_string()),
                                fee_owner_account_id: Some(account.to_string()),
                                nonce_owner_account_id: Some(account.to_string()),
                                target: novovm_protocol::NovExecutionTargetV1::NativeModule(
                                    "treasury".to_string(),
                                ),
                                method: "deposit_reserve".to_string(),
                                args: serde_json::to_vec(&serde_json::json!({
                                    "asset": "USDT",
                                    "amount": amount
                                }))
                                .expect("encode args"),
                                execution_mode: NovExecutionModeV1::Batch,
                                execution_policy: NovExecutionPolicyV1::Standard,
                                privacy_mode: NovPrivacyModeV1::Public,
                                verification_mode: NovVerificationModeV1::Standard,
                                fee_policy: NovFeePolicyV1 {
                                    pay_asset: "USDT".to_string(),
                                    max_pay_amount: 50,
                                    slippage_bps: 100,
                                },
                                gas_like_limit: Some(90_000),
                                nonce,
                            }),
                            signature: [0xabu8; 32],
                        };
                        let raw = encode_nov_native_tx_wire_v1(&native_tx).expect("encode nov tx");
                        to_hex_prefixed_v1(raw.as_slice())
                    };
                    let out =
                        run_nov_send_raw_transaction_batch_from_params_v1(&serde_json::json!({
                            "raw_txs": [
                                build_raw(1, "acct-batch-1", 25),
                                build_raw(2, "acct-batch-2", 35)
                            ],
                            "native_execution_store_path": path,
                        }))
                        .expect("batch should pass AOEM precommit and ordered commit");

                    assert_eq!(out["method"].as_str(), Some("nov_sendRawTransactionBatch"));
                    assert_eq!(out["accepted"].as_bool(), Some(true));
                    assert_eq!(out["batch_size"].as_u64(), Some(2));
                    assert_eq!(
                        out["deterministic_commit"].as_str(),
                        Some("post_aoem_batch_precommit_deterministic_sharded_dirty_atomic_commit")
                    );
                    assert_eq!(
                        out["native_store_commit"]["model"].as_str(),
                        Some("post_aoem_deterministic_dirty_store_commit")
                    );
                    assert_eq!(out["native_store_commit"]["load_count"].as_u64(), Some(1));
                    assert_eq!(out["native_store_commit"]["save_count"].as_u64(), Some(1));
                    assert_eq!(
                        out["native_store_commit"]["ordered_results"].as_bool(),
                        Some(true)
                    );
                    assert_eq!(
                        out["aoem_batch_ingress"]["execution_kernel"].as_str(),
                        Some("AOEM")
                    );
                    assert_eq!(
                        out["aoem_batch_ingress"]["semantic_entry"].as_str(),
                        Some(native_aoem_raw_tx_batch_precommit_entry_v1())
                    );
                    assert_eq!(
                        out["aoem_batch_ingress"]["ingress_scope"].as_str(),
                        Some("raw_tx_batch_precommit")
                    );
                    assert_eq!(
                        out["aoem_batch_ingress"]["batch_item_count"].as_u64(),
                        Some(2)
                    );
                    assert_eq!(
                        out["aoem_batch_ingress"]["batch_mode"].as_bool(),
                        Some(true)
                    );
                    assert_eq!(out["aoem_batch_ingress"]["op_count"].as_u64(), Some(2));
                    assert_eq!(out["aoem_batch_ingress"]["batch_size"].as_u64(), Some(2));
                    assert_eq!(
                        out["aoem_batch_ingress"]["concurrent_execution_enabled"].as_bool(),
                        Some(true)
                    );
                    assert!(
                        out["aoem_batch_ingress"]["recommended_threads"]
                            .as_u64()
                            .unwrap_or_default()
                            >= 1
                    );
                    assert_eq!(
                        out["aoem_batch_ingress"]["fallback_reason"].as_str(),
                        Some("aoem_semantic_ingress_disabled")
                    );
                    let results = out["results"].as_array().expect("batch results");
                    assert_eq!(results.len(), 2);
                    assert!(results.iter().all(|item| {
                        item["accepted"].as_bool() == Some(true)
                            && item["native_receipt"]["status"].as_bool() == Some(true)
                            && item["native_receipt"]["module"].as_str() == Some("treasury")
                            && item["native_receipt"]["method"].as_str() == Some("deposit_reserve")
                            && item["native_receipt"]["aoem_semantic_ingress"]["semantic_entry"]
                                .as_str()
                                == Some(native_aoem_raw_tx_batch_precommit_entry_v1())
                            && item["native_receipt"]["aoem_semantic_ingress"]["ingress_scope"]
                                .as_str()
                                == Some("raw_tx_batch_precommit_item")
                    }));
                    assert_eq!(
                        results[0]["native_receipt"]["aoem_semantic_ingress"]["batch_item_index"]
                            .as_u64(),
                        Some(0)
                    );
                    assert_eq!(
                        results[1]["native_receipt"]["aoem_semantic_ingress"]["batch_item_index"]
                            .as_u64(),
                        Some(1)
                    );
                    let store = load_nov_native_execution_store_v1(path.as_path())
                        .expect("batch store should load");
                    assert_eq!(store.receipts.len(), 2);
                    assert!(
                        store
                            .module_state
                            .treasury_reserves
                            .get("USDT")
                            .copied()
                            .unwrap_or_default()
                            >= 60,
                        "batch treasury reserve must include both ordered deposits"
                    );
                    let mirror_path =
                        nov_native_aoem_semantic_ledger_mirror_path_v1(path.as_path());
                    let mirror_body = fs::read_to_string(mirror_path.as_path())
                        .expect("batch mirror should be written");
                    assert_eq!(
                        mirror_body
                            .lines()
                            .filter(|line| !line.trim().is_empty())
                            .count(),
                        2,
                        "batch mirror append should persist one record per receipt"
                    );
                    let last_mirror = load_last_nov_native_aoem_semantic_ledger_mirror_record_v1(
                        mirror_path.as_path(),
                    )
                    .expect("batch mirror should load")
                    .expect("batch mirror should have last record");
                    assert_eq!(last_mirror.sequence, 2);
                })
            },
        )
    }

    #[test]
    fn run_nov_send_raw_transaction_batch_chunks_aoem_precommit_without_extra_store_commits() {
        with_env_override_v1(
            NOV_NATIVE_AOEM_SEMANTIC_INGRESS_ENABLED_ENV,
            "false",
            || {
                with_env_override_v1(NOV_NATIVE_AOEM_BATCH_MAX_SIZE_ENV, "2", || {
                    with_test_native_execution_store_path_v1(|path| {
                        let build_raw = |nonce: u64, account: &str, amount: u64| {
                            let native_tx = NovNativeTxWireV1 {
                                chain_id: 77,
                                kind: NovTxKindV1::Execute(novovm_protocol::NovExecuteTxV1 {
                                    caller: vec![nonce as u8; 20],
                                    account_id: Some(account.to_string()),
                                    fee_owner_account_id: Some(account.to_string()),
                                    nonce_owner_account_id: Some(account.to_string()),
                                    target: novovm_protocol::NovExecutionTargetV1::NativeModule(
                                        "treasury".to_string(),
                                    ),
                                    method: "deposit_reserve".to_string(),
                                    args: serde_json::to_vec(&serde_json::json!({
                                        "asset": "USDT",
                                        "amount": amount
                                    }))
                                    .expect("encode args"),
                                    execution_mode: NovExecutionModeV1::Batch,
                                    execution_policy: NovExecutionPolicyV1::Standard,
                                    privacy_mode: NovPrivacyModeV1::Public,
                                    verification_mode: NovVerificationModeV1::Standard,
                                    fee_policy: NovFeePolicyV1 {
                                        pay_asset: "USDT".to_string(),
                                        max_pay_amount: 50,
                                        slippage_bps: 100,
                                    },
                                    gas_like_limit: Some(90_000),
                                    nonce,
                                }),
                                signature: [0xcdu8; 32],
                            };
                            let raw =
                                encode_nov_native_tx_wire_v1(&native_tx).expect("encode nov tx");
                            to_hex_prefixed_v1(raw.as_slice())
                        };
                        let out =
                            run_nov_send_raw_transaction_batch_from_params_v1(&serde_json::json!({
                                "raw_txs": [
                                    build_raw(11, "acct-chunk-1", 25),
                                    build_raw(12, "acct-chunk-2", 35),
                                    build_raw(13, "acct-chunk-3", 45)
                                ],
                                "native_execution_store_path": path,
                            }))
                            .expect("chunked batch should precommit through AOEM and commit once");

                        assert_eq!(out["method"].as_str(), Some("nov_sendRawTransactionBatch"));
                        assert_eq!(out["accepted"].as_bool(), Some(true));
                        assert_eq!(out["batch_size"].as_u64(), Some(3));
                        assert_eq!(out["aoem_concurrency_owner"].as_str(), Some("AOEM_runtime"));
                        assert_eq!(
                            out["aoem_batch_ingress"]["ingress_scope"].as_str(),
                            Some("raw_tx_batch_precommit_chunked")
                        );
                        assert_eq!(out["aoem_batch_ingress"]["op_count"].as_u64(), Some(3));
                        assert_eq!(
                            out["aoem_batch_ingress"]["fallback_reason"].as_str(),
                            Some("aoem_semantic_ingress_disabled")
                        );
                        assert_eq!(
                            out["aoem_batch_chunking"]["model"].as_str(),
                            Some(
                                "bounded_ops_wire_chunks_submitted_to_aoem_runtime_no_host_thread_scheduler"
                            )
                        );
                        assert_eq!(out["aoem_batch_chunking"]["enabled"].as_bool(), Some(true));
                        assert_eq!(out["aoem_batch_chunking"]["chunk_count"].as_u64(), Some(2));
                        assert_eq!(
                            out["aoem_batch_chunking"]["max_chunk_size"].as_u64(),
                            Some(2)
                        );
                        let chunks = out["aoem_batch_chunks"]
                            .as_array()
                            .expect("chunk metadata array");
                        assert_eq!(chunks.len(), 2);
                        assert_eq!(chunks[0]["op_count"].as_u64(), Some(2));
                        assert_eq!(chunks[1]["op_count"].as_u64(), Some(1));
                        assert_eq!(
                            out["native_store_commit"]["model"].as_str(),
                            Some("post_aoem_deterministic_dirty_store_commit")
                        );
                        assert_eq!(out["native_store_commit"]["load_count"].as_u64(), Some(1));
                        assert_eq!(out["native_store_commit"]["save_count"].as_u64(), Some(1));
                        assert_eq!(
                            out["native_store_commit"]["aoem_precommit_chunk_count"].as_u64(),
                            Some(2)
                        );
                        let results = out["results"].as_array().expect("batch results");
                        assert_eq!(results.len(), 3);
                        assert_eq!(
                            results[2]["native_receipt"]["aoem_semantic_ingress"]
                                ["batch_item_index"]
                                .as_u64(),
                            Some(2)
                        );
                        assert_eq!(
                            results[2]["native_receipt"]["aoem_semantic_ingress"]
                                ["batch_item_count"]
                                .as_u64(),
                            Some(3)
                        );
                        let store = load_nov_native_execution_store_v1(path.as_path())
                            .expect("batch store should load");
                        assert_eq!(store.receipts.len(), 3);
                        assert!(
                            store
                                .module_state
                                .treasury_reserves
                                .get("USDT")
                                .copied()
                                .unwrap_or_default()
                                >= 105,
                            "chunked batch treasury reserve must include every ordered deposit"
                        );
                    })
                })
            },
        )
    }

    #[test]
    fn run_nov_execute_pending_native_tx_batch_consumes_network_pending_into_aoem_batch() {
        with_env_override_v1(
            NOV_NATIVE_AOEM_SEMANTIC_INGRESS_ENABLED_ENV,
            "false",
            || {
                with_test_native_execution_store_path_v1(|path| {
                    let chain_id = 88_017;
                    let build_and_ingest_pending = |nonce: u64, account: &str, amount: u64| {
                        let native_tx = NovNativeTxWireV1 {
                            chain_id,
                            kind: NovTxKindV1::Execute(novovm_protocol::NovExecuteTxV1 {
                                caller: vec![nonce as u8; 20],
                                account_id: Some(account.to_string()),
                                fee_owner_account_id: Some(account.to_string()),
                                nonce_owner_account_id: Some(account.to_string()),
                                target: novovm_protocol::NovExecutionTargetV1::NativeModule(
                                    "treasury".to_string(),
                                ),
                                method: "deposit_reserve".to_string(),
                                args: serde_json::to_vec(&serde_json::json!({
                                    "asset": "USDT",
                                    "amount": amount
                                }))
                                .expect("encode args"),
                                execution_mode: NovExecutionModeV1::Batch,
                                execution_policy: NovExecutionPolicyV1::Standard,
                                privacy_mode: NovPrivacyModeV1::Public,
                                verification_mode: NovVerificationModeV1::Standard,
                                fee_policy: NovFeePolicyV1 {
                                    pay_asset: "USDT".to_string(),
                                    max_pay_amount: 50,
                                    slippage_bps: 100,
                                },
                                gas_like_limit: Some(90_000),
                                nonce,
                            }),
                            signature: [0xefu8; 32],
                        };
                        let raw = encode_nov_native_tx_wire_v1(&native_tx).expect("encode nov tx");
                        let (_, _, tx_hash) = ingest_local_nov_raw_tx_payload_v1(
                            &serde_json::json!({}),
                            raw.as_slice(),
                        )
                        .expect("native pending ingress should store payload");
                        tx_hash
                    };
                    let first_hash = build_and_ingest_pending(21, "acct-pending-1", 25);
                    let second_hash = build_and_ingest_pending(22, "acct-pending-2", 35);

                    assert!(
                        get_network_runtime_native_pending_tx_payload_v1(chain_id, first_hash)
                            .is_some(),
                        "native pending payload must be retained for batch execution"
                    );
                    assert!(
                        get_network_runtime_native_pending_tx_payload_v1(chain_id, second_hash)
                            .is_some(),
                        "native pending payload must be retained for batch execution"
                    );

                    let out = run_nov_execute_pending_native_tx_batch_from_params_v1(
                        &serde_json::json!({
                            "chain_id": chain_id,
                            "limit": 8,
                            "native_execution_store_path": path,
                        }),
                    )
                    .expect("pending native batch should execute through AOEM batch");

                    assert_eq!(
                        out["method"].as_str(),
                        Some("nov_executePendingNativeTxBatch")
                    );
                    assert_eq!(out["accepted"].as_bool(), Some(true));
                    assert_eq!(
                        out["source"].as_str(),
                        Some("network_runtime_native_pending")
                    );
                    assert_eq!(out["executed"].as_bool(), Some(true));
                    assert_eq!(out["selected_count"].as_u64(), Some(2));
                    assert_eq!(
                        out["batch_result"]["method"].as_str(),
                        Some("nov_sendRawTransactionBatch")
                    );
                    assert_eq!(
                        out["batch_result"]["aoem_concurrency_owner"].as_str(),
                        Some("AOEM_runtime")
                    );
                    assert_eq!(
                        out["batch_result"]["native_store_commit"]["load_count"].as_u64(),
                        Some(1)
                    );
                    assert_eq!(
                        out["batch_result"]["native_store_commit"]["save_count"].as_u64(),
                        Some(1)
                    );
                    assert_eq!(
                        out["batch_result"]["native_store_commit"]["model"].as_str(),
                        Some("post_aoem_deterministic_dirty_store_commit")
                    );
                    assert_eq!(
                        out["canonical_projection"]["included_canonical"].as_bool(),
                        Some(true)
                    );
                    let first_pending = novovm_network::get_network_runtime_native_pending_tx_v1(
                        chain_id, first_hash,
                    )
                    .expect("first pending tx should remain visible after canonical inclusion");
                    assert_eq!(
                        first_pending.lifecycle_stage,
                        novovm_network::NetworkRuntimeNativePendingTxLifecycleStageV1::IncludedCanonical
                    );
                    let store = load_nov_native_execution_store_v1(path.as_path())
                        .expect("pending batch store should load");
                    assert_eq!(store.receipts.len(), 2);
                    assert!(
                        store
                            .module_state
                            .treasury_reserves
                            .get("USDT")
                            .copied()
                            .unwrap_or_default()
                            >= 60,
                        "pending native batch must commit both deposits"
                    );
                })
            },
        )
    }

    #[test]
    fn run_nov_native_execution_tick_budget_drains_pending_through_aoem_batch() {
        with_env_override_v1(
            NOV_NATIVE_AOEM_SEMANTIC_INGRESS_ENABLED_ENV,
            "false",
            || {
                with_test_native_execution_store_path_v1(|path| {
                    let chain_id = 88_018;
                    let build_and_ingest_pending = |nonce: u64, account: &str, amount: u64| {
                        let native_tx = NovNativeTxWireV1 {
                            chain_id,
                            kind: NovTxKindV1::Execute(novovm_protocol::NovExecuteTxV1 {
                                caller: vec![nonce as u8; 20],
                                account_id: Some(account.to_string()),
                                fee_owner_account_id: Some(account.to_string()),
                                nonce_owner_account_id: Some(account.to_string()),
                                target: novovm_protocol::NovExecutionTargetV1::NativeModule(
                                    "treasury".to_string(),
                                ),
                                method: "deposit_reserve".to_string(),
                                args: serde_json::to_vec(&serde_json::json!({
                                    "asset": "USDT",
                                    "amount": amount
                                }))
                                .expect("encode args"),
                                execution_mode: NovExecutionModeV1::Batch,
                                execution_policy: NovExecutionPolicyV1::Standard,
                                privacy_mode: NovPrivacyModeV1::Public,
                                verification_mode: NovVerificationModeV1::Standard,
                                fee_policy: NovFeePolicyV1 {
                                    pay_asset: "USDT".to_string(),
                                    max_pay_amount: 50,
                                    slippage_bps: 100,
                                },
                                gas_like_limit: Some(90_000),
                                nonce,
                            }),
                            signature: [0xeeu8; 32],
                        };
                        let raw = encode_nov_native_tx_wire_v1(&native_tx).expect("encode nov tx");
                        ingest_local_nov_raw_tx_payload_v1(&serde_json::json!({}), raw.as_slice())
                            .expect("native pending ingress should store payload");
                    };
                    build_and_ingest_pending(31, "acct-tick-1", 25);
                    build_and_ingest_pending(32, "acct-tick-2", 35);
                    build_and_ingest_pending(33, "acct-tick-3", 45);

                    let out = run_nov_native_execution_tick_from_params_v1(&serde_json::json!({
                        "chain_id": chain_id,
                        "hard_budget_per_tick": 3,
                        "target_budget_per_tick": 2,
                        "effective_budget_per_tick": 2,
                        "native_execution_store_path": path,
                    }))
                    .expect("native execution tick should drain pending through AOEM batch");

                    assert_eq!(out["method"].as_str(), Some("nov_runNativeExecutionTick"));
                    assert_eq!(
                        out["scheduler_mode"].as_str(),
                        Some("mainline_native_execution_tick")
                    );
                    assert_eq!(out["background_daemon"].as_bool(), Some(false));
                    assert_eq!(out["execution_kernel"].as_str(), Some("AOEM"));
                    assert_eq!(out["aoem_concurrency_owner"].as_str(), Some("AOEM_runtime"));
                    assert_eq!(out["eligible_before"].as_u64(), Some(3));
                    assert_eq!(out["executed_count"].as_u64(), Some(2));
                    assert_eq!(out["deferred_count"].as_u64(), Some(1));
                    assert_eq!(out["budget"]["effective_budget_per_tick"].as_u64(), Some(2));
                    assert_eq!(
                        out["budget_runtime"]["execution_budget_hit_count"].as_u64(),
                        Some(1)
                    );
                    assert_eq!(
                        out["budget_runtime"]["execution_deferred_count"].as_u64(),
                        Some(1)
                    );
                    assert_eq!(
                        out["batch_result"]["batch_result"]["method"].as_str(),
                        Some("nov_sendRawTransactionBatch")
                    );
                    assert_eq!(
                        out["batch_result"]["batch_result"]["aoem_concurrency_owner"].as_str(),
                        Some("AOEM_runtime")
                    );
                    assert_eq!(
                        out["batch_result"]["batch_result"]["native_store_commit"]["model"]
                            .as_str(),
                        Some("post_aoem_deterministic_dirty_store_commit")
                    );
                    assert_eq!(
                        out["batch_result"]["canonical_projection"]["included_canonical"].as_bool(),
                        Some(true)
                    );
                    let pending_summary =
                        novovm_network::snapshot_network_runtime_native_pending_tx_summary_v1(
                            chain_id,
                        );
                    assert_eq!(pending_summary.included_canonical_count, 2);
                    let store = load_nov_native_execution_store_v1(path.as_path())
                        .expect("tick store should load");
                    assert_eq!(store.receipts.len(), 2);
                    assert!(
                        store
                            .module_state
                            .treasury_reserves
                            .get("USDT")
                            .copied()
                            .unwrap_or_default()
                            >= 60,
                        "tick must commit only the AOEM-executed budgeted batch"
                    );
                })
            },
        )
    }

    #[test]
    fn native_aoem_semantic_ledger_chains_commit_seals() {
        with_env_override_v1(NOV_NATIVE_AOEM_SEMANTIC_INGRESS_ENABLED_ENV, "true", || {
            with_env_override_v1(
                NOV_NATIVE_AOEM_SEMANTIC_INGRESS_REQUIRED_ENV,
                "false",
                || {
                    with_test_native_execution_store_path_v1(|path| {
                        let first = NovExecutionRequestV1 {
                            tx_hash: [0xb0; 32],
                            chain_id: 7092,
                            caller: vec![0xb0; 20],
                            target: NovExecutionRequestTargetV1::NativeModule(
                                "treasury".to_string(),
                            ),
                            method: "deposit_reserve".to_string(),
                            args: serde_json::to_vec(&serde_json::json!({
                                "asset": "NOV",
                                "amount": 2u64
                            }))
                            .expect("encode args"),
                            fee_pay_asset: "NOV".to_string(),
                            fee_max_pay_amount: 10_000,
                            fee_slippage_bps: 50,
                            gas_like_limit: Some(90_000),
                            nonce: 176,
                        };
                        let first_receipt =
                            dispatch_and_persist_nov_execution_request_with_store_path_v1(
                                path.as_path(),
                                &first,
                            )
                            .expect("first dispatch should succeed");
                        let first_meta = first_receipt
                            .aoem_semantic_ingress
                            .as_ref()
                            .expect("first receipt should expose AOEM semantic metadata");
                        assert_eq!(first_meta.semantic_ledger_sequence, 1);
                        assert!(first_meta.semantic_ledger_prev_seal.is_empty());
                        let first_seal = first_meta.semantic_ledger_commit_seal.clone();
                        assert_eq!(first_seal.len(), 64);

                        let second = NovExecutionRequestV1 {
                            tx_hash: [0xb1; 32],
                            chain_id: 7092,
                            caller: vec![0xb1; 20],
                            target: NovExecutionRequestTargetV1::NativeModule(
                                "treasury".to_string(),
                            ),
                            method: "deposit_reserve".to_string(),
                            args: serde_json::to_vec(&serde_json::json!({
                                "asset": "USDT",
                                "amount": 4u64
                            }))
                            .expect("encode args"),
                            fee_pay_asset: "USDT".to_string(),
                            fee_max_pay_amount: 10_000,
                            fee_slippage_bps: 50,
                            gas_like_limit: Some(90_000),
                            nonce: 177,
                        };
                        let second_receipt =
                            dispatch_and_persist_nov_execution_request_with_store_path_v1(
                                path.as_path(),
                                &second,
                            )
                            .expect("second dispatch should succeed");
                        let second_meta = second_receipt
                            .aoem_semantic_ingress
                            .as_ref()
                            .expect("second receipt should expose AOEM semantic metadata");
                        assert_eq!(second_meta.semantic_ledger_sequence, 2);
                        assert_eq!(second_meta.semantic_ledger_prev_seal, first_seal);
                        assert_eq!(second_meta.semantic_ledger_commit_seal.len(), 64);
                        assert_ne!(
                            second_meta.semantic_ledger_commit_seal,
                            second_meta.semantic_ledger_prev_seal
                        );

                        let store = load_nov_native_execution_store_v1(path.as_path())
                            .expect("store should be readable");
                        assert_eq!(store.module_state.aoem_semantic_ledger_sequence, 2);
                        assert_eq!(
                            store.module_state.aoem_semantic_ledger_head,
                            second_meta.semantic_ledger_commit_seal
                        );

                        let mirror_path =
                            nov_native_aoem_semantic_ledger_mirror_path_v1(path.as_path());
                        let last_mirror =
                            load_last_nov_native_aoem_semantic_ledger_mirror_record_v1(
                                mirror_path.as_path(),
                            )
                            .expect("mirror should be readable")
                            .expect("mirror should contain the latest AOEM semantic commit");
                        assert_eq!(
                            last_mirror.schema,
                            "novovm-native-aoem-semantic-ledger-mirror/v1"
                        );
                        assert_eq!(last_mirror.execution_kernel, "AOEM");
                        assert_eq!(last_mirror.algebraic_semantic_entry, true);
                        assert_eq!(last_mirror.sequence, 2);
                        assert_eq!(last_mirror.tx_hash, second_receipt.tx_hash);
                        assert_eq!(last_mirror.prev_seal, first_seal);
                        assert_eq!(
                            last_mirror.commit_seal,
                            second_meta.semantic_ledger_commit_seal
                        );
                        assert_eq!(last_mirror.mirror_backend, "jsonl_append_only");
                    });
                },
            );
        });
    }

    #[test]
    fn tx_ingress_record_maps_to_adapter_verifiable_tx_ir() {
        let record = TxIngressRecord {
            account: 7,
            key: 9,
            value: 11,
            nonce: 13,
            fee: 17,
            signature: [0xabu8; 32],
        };
        let ir = tx_ingress_record_to_adapter_tx_ir(&record, 1);
        assert_eq!(ir.chain_id, 1);
        assert_eq!(ir.tx_type, TxType::Transfer);
        assert_eq!(ir.value, 11);
        assert_eq!(ir.gas_limit, 21_000);
        assert_eq!(ir.gas_price, 17);
        assert_eq!(ir.nonce, 13);
        assert_eq!(ir.signature.len(), 96);
        assert_eq!(ir.from.len(), 20);
        assert_eq!(ir.to.as_ref().map(Vec::len), Some(20));
        assert!(!ir.hash.is_empty());

        let mut adapter =
            novovm_adapter_novovm::create_native_adapter(novovm_adapter_api::ChainConfig {
                chain_type: novovm_adapter_api::ChainType::EVM,
                chain_id: 1,
                name: "test-evm".to_string(),
                enabled: true,
                custom_config: None,
            })
            .expect("create adapter");
        adapter.initialize().expect("initialize adapter");
        assert!(adapter.verify_transaction(&ir).expect("verify tx ir"));
        adapter.shutdown().expect("shutdown adapter");
    }

    #[test]
    fn decode_eth_send_raw_hex_payload_v1_accepts_prefixed_payload() {
        let payload = decode_eth_send_raw_hex_payload_v1("0x0102a0", "raw_tx")
            .expect("decode should succeed");
        assert_eq!(payload, vec![0x01, 0x02, 0xa0]);
    }

    #[test]
    fn run_eth_send_raw_transaction_from_params_v1_tracks_pending() {
        let chain_id = 98_877_663;
        let raw_tx_hex =
            "0x02e20180021e827530946e6e6e6e6e6e6e6e6e6e6e6e6e6e6e6e6e6e6e6e0480c0010101";
        let payload =
            decode_eth_send_raw_hex_payload_v1(raw_tx_hex, "raw_tx").expect("decode raw tx");
        let expected_hash = eth_rlpx_transaction_hash_v1(payload.as_slice());

        let out = run_eth_send_raw_transaction_from_params_v1(&serde_json::json!({
            "raw_tx": raw_tx_hex,
            "chain_id": chain_id,
        }))
        .expect("route should succeed");
        assert_eq!(out["accepted"].as_bool(), Some(true));
        assert_eq!(
            out["pending_tx_hash"].as_str(),
            Some(to_hex_prefixed_v1(&expected_hash).as_str())
        );
        assert_eq!(out["chain_id"].as_u64(), Some(chain_id));

        let pending =
            novovm_network::get_network_runtime_native_pending_tx_v1(chain_id, expected_hash)
                .expect("pending tx should exist");
        assert_eq!(
            pending.origin,
            novovm_network::NetworkRuntimeNativePendingTxOriginV1::Local
        );
    }

    #[test]
    fn ingest_local_eth_raw_tx_payload_marks_rejected_when_invalid() {
        let chain_id = 98_877_663;
        let payload = vec![0x01, 0x02, 0x03];
        let expected_hash = novovm_network::eth_rlpx_transaction_hash_v1(payload.as_slice());
        let err = ingest_local_eth_raw_tx_payload_v1(chain_id, payload.as_slice())
            .expect_err("invalid envelope should fail");
        assert!(format!("{err}").contains("not a valid ethereum tx envelope"));
        let state =
            novovm_network::get_network_runtime_native_pending_tx_v1(chain_id, expected_hash)
                .expect("invalid local tx should still be tracked as rejected");
        assert_eq!(
            state.lifecycle_stage,
            novovm_network::NetworkRuntimeNativePendingTxLifecycleStageV1::Rejected
        );
        assert_eq!(
            state.origin,
            novovm_network::NetworkRuntimeNativePendingTxOriginV1::Local
        );
        assert_eq!(state.reject_count, 1);
    }

    #[test]
    fn run_nov_send_raw_transaction_from_params_v1_tracks_pending_and_builds_execution_request() {
        with_test_native_execution_store_path_v1(|path| {
            let native_tx = NovNativeTxWireV1 {
                chain_id: 77,
                kind: NovTxKindV1::Execute(novovm_protocol::NovExecuteTxV1 {
                    caller: vec![0x11; 20],
                    account_id: Some("acct-pending".to_string()),
                    fee_owner_account_id: Some("acct-fee".to_string()),
                    nonce_owner_account_id: Some("acct-nonce".to_string()),
                    target: novovm_protocol::NovExecutionTargetV1::NativeModule(
                        "treasury".to_string(),
                    ),
                    method: "deposit_reserve".to_string(),
                    args: serde_json::to_vec(&serde_json::json!({
                        "asset": "USDT",
                        "amount": 25u64
                    }))
                    .expect("encode args"),
                    execution_mode: NovExecutionModeV1::Standard,
                    execution_policy: NovExecutionPolicyV1::Standard,
                    privacy_mode: NovPrivacyModeV1::Public,
                    verification_mode: NovVerificationModeV1::Standard,
                    fee_policy: NovFeePolicyV1 {
                        pay_asset: "USDT".to_string(),
                        max_pay_amount: 50,
                        slippage_bps: 100,
                    },
                    gas_like_limit: Some(90_000),
                    nonce: 6,
                }),
                signature: [0xabu8; 32],
            };
            let raw = encode_nov_native_tx_wire_v1(&native_tx).expect("encode nov tx");
            let out = run_nov_send_raw_transaction_from_params_v1(&serde_json::json!({
                "raw_tx": to_hex_prefixed_v1(raw.as_slice()),
                "native_execution_store_path": path,
            }))
            .expect("nov_sendRawTransaction should succeed");
            assert_eq!(out["accepted"].as_bool(), Some(true));
            assert_eq!(out["nov_tx_kind"].as_str(), Some("execute"));
            assert_eq!(out["chain_id"].as_u64(), Some(77));
            assert_eq!(
                out["execution_subject"]["account_id"].as_str(),
                Some("acct-pending")
            );
            assert_eq!(
                out["native_receipt"]["account_id"].as_str(),
                Some("acct-pending")
            );
            assert!(out["pending_tx_hash"]
                .as_str()
                .unwrap_or_default()
                .starts_with("0x"));
            assert!(out["execution_request"].is_object());
            assert!(
                out["native_receipt"]["settled_fee_nov"]
                    .as_u64()
                    .unwrap_or_default()
                    > 0
            );
            assert_eq!(out["native_receipt"]["paid_asset"].as_str(), Some("USDT"));
            assert!(
                out["native_receipt"]["paid_amount"]
                    .as_u64()
                    .unwrap_or_default()
                    > 0
            );
            assert!(
                out["native_receipt"]["paid_amount"]
                    .as_u64()
                    .unwrap_or_default()
                    <= 50
            );
            assert_eq!(out["native_receipt"]["status"].as_bool(), Some(true));
            assert_eq!(
                out["native_receipt"]["fee_contract"].as_str(),
                Some(NOV_EXECUTION_FEE_CLASSIFICATION_CONTRACT_V1)
            );
            assert_eq!(
                out["native_receipt"]["fee_quote_contract"].as_str(),
                Some(NOV_EXECUTION_FEE_QUOTE_CONTRACT_V1)
            );
            assert_eq!(
                out["native_receipt"]["fee_clearing_contract"].as_str(),
                Some(NOV_EXECUTION_FEE_CLEARING_CONTRACT_V1)
            );
            assert!(out["native_receipt"]["fee_price_source"]
                .as_str()
                .unwrap_or_default()
                .contains("clearing="));
            assert!(out["native_receipt"]["fee_quote_id"]
                .as_str()
                .unwrap_or_default()
                .starts_with("q-"));
            assert!(
                out["native_receipt"]["fee_quote_required_pay_amount"]
                    .as_u64()
                    .unwrap_or_default()
                    > 0
            );
            assert!(out["native_receipt"]["fee_clearing_route_ref"]
                .as_str()
                .unwrap_or_default()
                .starts_with("route:"));
            assert!(!out["native_receipt"]["fee_clearing_source"]
                .as_str()
                .unwrap_or_default()
                .is_empty());
            assert!(
                out["native_receipt"]["fee_clearing_rate_ppm"]
                    .as_u64()
                    .unwrap_or_default()
                    > 0
            );
            assert!(out["native_receipt"]["route_meta"]["route_id"]
                .as_str()
                .unwrap_or_default()
                .starts_with("route:"));
            assert!(out["native_receipt"]["route_meta"]["route_source"]
                .as_str()
                .is_some_and(|v| !v.is_empty()));
            assert!(
                out["native_receipt"]["route_meta"]["expected_nov_out"]
                    .as_u64()
                    .unwrap_or_default()
                    > 0
            );

            let tx_hash_hex = out["pending_tx_hash"]
                .as_str()
                .expect("pending tx hash")
                .trim_start_matches("0x");
            let stored = get_nov_native_execution_receipt_by_hash_with_store_path_v1(
                path.as_path(),
                tx_hash_hex,
            )
            .expect("load native receipt")
            .expect("native receipt exists");
            assert_eq!(stored.module, "treasury");
            assert_eq!(stored.method, "deposit_reserve");
            assert!(stored.logs.iter().any(|log| {
                log.module == "treasury"
                    && log.method == "deposit_reserve"
                    && log.event == "treasury.reserve_deposited"
            }));
            assert!(stored.logs.iter().any(|log| {
                log.module == "aoem"
                    && log.method == "semantic_ingress"
                    && log.event == "aoem.native_asset.semantic_deltas"
            }));
            let aoem_meta = stored
                .aoem_semantic_ingress
                .as_ref()
                .expect("receipt should expose AOEM semantic ingress metadata");
            assert_eq!(aoem_meta.execution_kernel, "AOEM");
            assert!(aoem_meta.algebraic_semantic_entry);
            assert_eq!(aoem_meta.semantic_entry, native_aoem_semantic_entry_v1());
            assert!(aoem_meta.semantic_delta_count > 0);
            assert_eq!(aoem_meta.semantic_delta_digest.len(), 64);
            assert!(stored.settled_fee_nov > 0);
            assert!(stored.paid_amount > 0);
            assert_eq!(
                stored.fee_contract,
                NOV_EXECUTION_FEE_CLASSIFICATION_CONTRACT_V1
            );
            assert_eq!(
                stored.fee_quote_contract,
                NOV_EXECUTION_FEE_QUOTE_CONTRACT_V1
            );
            assert_eq!(
                stored.fee_clearing_contract,
                NOV_EXECUTION_FEE_CLEARING_CONTRACT_V1
            );
        });
    }

    #[test]
    fn run_nov_send_raw_transaction_pipeline_only_waits_for_aoem_tick() {
        with_test_native_execution_store_path_v1(|path| {
            with_env_override_v1(
                NOV_NATIVE_SEND_RAW_TRANSACTION_PIPELINE_ONLY_ENV,
                "true",
                || {
                    let native_tx = NovNativeTxWireV1 {
                        chain_id: 990_778,
                        kind: NovTxKindV1::Execute(novovm_protocol::NovExecuteTxV1 {
                            caller: vec![0x21; 20],
                            account_id: Some("acct-pipeline-only".to_string()),
                            fee_owner_account_id: Some("acct-pipeline-fee".to_string()),
                            nonce_owner_account_id: Some("acct-pipeline-nonce".to_string()),
                            target: novovm_protocol::NovExecutionTargetV1::NativeModule(
                                "treasury".to_string(),
                            ),
                            method: "deposit_reserve".to_string(),
                            args: serde_json::to_vec(&serde_json::json!({
                                "asset": "USDT",
                                "amount": 19u64
                            }))
                            .expect("encode args"),
                            execution_mode: NovExecutionModeV1::Standard,
                            execution_policy: NovExecutionPolicyV1::Standard,
                            privacy_mode: NovPrivacyModeV1::Public,
                            verification_mode: NovVerificationModeV1::Standard,
                            fee_policy: NovFeePolicyV1 {
                                pay_asset: "USDT".to_string(),
                                max_pay_amount: 50,
                                slippage_bps: 100,
                            },
                            gas_like_limit: Some(90_000),
                            nonce: 7,
                        }),
                        signature: [0xcdu8; 32],
                    };
                    let raw = encode_nov_native_tx_wire_v1(&native_tx).expect("encode nov tx");
                    let out = run_nov_send_raw_transaction_from_params_v1(&serde_json::json!({
                        "raw_tx": to_hex_prefixed_v1(raw.as_slice()),
                        "native_execution_store_path": path,
                    }))
                    .expect("nov_sendRawTransaction should accept pending-only tx");
                    assert_eq!(out["accepted"].as_bool(), Some(true));
                    assert_eq!(out["pending_tx_local_ingress"].as_bool(), Some(true));
                    assert_eq!(out["pipeline_only"].as_bool(), Some(true));
                    assert_eq!(out["immediate_execution"].as_bool(), Some(false));
                    assert_eq!(
                        out["execution_lifecycle"].as_str(),
                        Some("pending_runtime_to_aoem_tick")
                    );
                    assert!(out["execution_request"].is_object());
                    assert!(out["native_receipt"].is_null());

                    let tx_hash_hex = out["pending_tx_hash"]
                        .as_str()
                        .expect("pending tx hash")
                        .trim_start_matches("0x")
                        .to_string();
                    let pre_tick = get_nov_native_execution_receipt_by_hash_with_store_path_v1(
                        path.as_path(),
                        tx_hash_hex.as_str(),
                    )
                    .expect("pre-tick receipt lookup should not fail");
                    assert!(pre_tick.is_none());

                    let tick = run_nov_native_execution_tick_from_params_v1(&serde_json::json!({
                        "chain_id": 990_778,
                        "limit": 1,
                        "scan_limit": 8,
                        "native_execution_store_path": path,
                    }))
                    .expect("AOEM tick should execute pending tx");
                    assert_eq!(tick["execution_kernel"].as_str(), Some("AOEM"));
                    assert_eq!(tick["executed_count"].as_u64(), Some(1));
                    assert_eq!(
                        tick["lifecycle"]["execution"].as_str(),
                        Some("aoem_batch_precommit")
                    );

                    let stored = get_nov_native_execution_receipt_by_hash_with_store_path_v1(
                        path.as_path(),
                        tx_hash_hex.as_str(),
                    )
                    .expect("post-tick receipt lookup should not fail")
                    .expect("AOEM tick should persist receipt");
                    assert_eq!(stored.account_id.as_str(), "acct-pipeline-only");
                    assert!(stored.status);
                    assert_eq!(
                        stored
                            .aoem_semantic_ingress
                            .as_ref()
                            .map(|meta| meta.execution_kernel.as_str()),
                        Some("AOEM")
                    );
                },
            );
        });
    }

    #[test]
    fn run_nov_native_call_reads_runtime_module_state() {
        with_test_native_execution_store_path_v1(|path| {
            let request = NovExecutionRequestV1 {
                tx_hash: [0x22; 32],
                chain_id: 991,
                caller: vec![0x33; 20],
                target: NovExecutionRequestTargetV1::NativeModule("treasury".to_string()),
                method: "deposit_reserve".to_string(),
                args: serde_json::to_vec(&serde_json::json!({
                    "asset": "ETH",
                    "amount": 77u64
                }))
                .expect("encode args"),
                fee_pay_asset: "ETH".to_string(),
                fee_max_pay_amount: 12,
                fee_slippage_bps: 30,
                gas_like_limit: Some(80_000),
                nonce: 1,
            };
            let _ = dispatch_and_persist_nov_execution_request_with_store_path_v1(
                path.as_path(),
                &request,
            )
            .expect("dispatch native request");
            let out = run_nov_native_call_from_params_with_store_path_v1(
                &serde_json::json!({
                    "target": {"kind": "native_module", "id": "treasury"},
                    "method": "get_reserve_balance",
                    "args": {"asset": "ETH"},
                }),
                Some(path.as_path()),
            )
            .expect("nov native call should succeed");
            assert_eq!(out["found"].as_bool(), Some(true));
            assert_eq!(out["result"]["asset"].as_str(), Some("ETH"));
            // Includes both explicit deposit_reserve(amount=77) and fee settlement reserve credit.
            assert!(
                out["result"]["reserve_balance"]
                    .as_u64()
                    .unwrap_or_default()
                    >= 77
            );
            let summary = run_nov_native_call_from_params_with_store_path_v1(
                &serde_json::json!({
                    "target": {"kind": "native_module", "id": "treasury"},
                    "method": "get_settlement_summary",
                    "args": {},
                }),
                Some(path.as_path()),
            )
            .expect("nov settlement summary should succeed");
            assert_eq!(summary["found"].as_bool(), Some(true));
            assert!(
                summary["result"]["settled_nov_total"]
                    .as_u64()
                    .unwrap_or_default()
                    > 0
            );
            assert!(
                summary["result"]["settlement_count"]
                    .as_u64()
                    .unwrap_or_default()
                    >= 1
            );
        });
    }

    #[test]
    fn treasury_settlement_summary_exposes_policy_and_buckets() {
        with_test_native_execution_store_path_v1(|path| {
            let request = NovExecutionRequestV1 {
                tx_hash: [0x31; 32],
                chain_id: 1201,
                caller: vec![0x21; 20],
                target: NovExecutionRequestTargetV1::NativeModule("treasury".to_string()),
                method: "deposit_reserve".to_string(),
                args: serde_json::to_vec(&serde_json::json!({
                    "asset": "NOV",
                    "amount": 9u64
                }))
                .expect("encode args"),
                fee_pay_asset: "NOV".to_string(),
                fee_max_pay_amount: 500,
                fee_slippage_bps: 0,
                gas_like_limit: Some(80_000),
                nonce: 12,
            };
            let receipt = dispatch_and_persist_nov_execution_request_with_store_path_v1(
                path.as_path(),
                &request,
            )
            .expect("dispatch should succeed");
            assert!(
                receipt.status,
                "failure_reason={:?}",
                receipt.failure_reason
            );

            let summary = run_nov_native_call_from_params_with_store_path_v1(
                &serde_json::json!({
                    "target": {"kind": "native_module", "id": "treasury"},
                    "method": "get_settlement_summary",
                    "args": {},
                }),
                Some(path.as_path()),
            )
            .expect("nov settlement summary should succeed");
            let result = &summary["result"];
            assert!(
                result["settled_nov_total"].as_u64().unwrap_or_default() > 0,
                "settled_nov_total should be positive"
            );
            let reserve = result["settlement_buckets_nov"]["reserve"]
                .as_u64()
                .unwrap_or_default();
            let fee = result["settlement_buckets_nov"]["fee"]
                .as_u64()
                .unwrap_or_default();
            let risk = result["settlement_buckets_nov"]["risk_buffer"]
                .as_u64()
                .unwrap_or_default();
            let total = result["settled_nov_total"].as_u64().unwrap_or_default();
            assert_eq!(reserve.saturating_add(fee).saturating_add(risk), total);
            assert_eq!(
                result["accounting"]["bucket_total_nov"].as_u64(),
                Some(total)
            );
            assert_eq!(
                result["accounting"]["bucket_consistent_with_net_settled"].as_bool(),
                Some(true)
            );
            assert!(
                result["journal"]["total_entries"]
                    .as_u64()
                    .unwrap_or_default()
                    >= 1,
                "settlement journal should include fee settlement entries"
            );
            assert!(!result["settlement_policy"]["source"]
                .as_str()
                .unwrap_or_default()
                .is_empty());
        });
    }

    #[test]
    fn treasury_settlement_summary_helper_returns_result_body() {
        with_test_native_execution_store_path_v1(|path| {
            let request = NovExecutionRequestV1 {
                tx_hash: [0x3a; 32],
                chain_id: 1210,
                caller: vec![0x2a; 20],
                target: NovExecutionRequestTargetV1::NativeModule("treasury".to_string()),
                method: "deposit_reserve".to_string(),
                args: serde_json::to_vec(&serde_json::json!({
                    "asset": "NOV",
                    "amount": 17u64
                }))
                .expect("encode args"),
                fee_pay_asset: "NOV".to_string(),
                fee_max_pay_amount: 800,
                fee_slippage_bps: 0,
                gas_like_limit: Some(90_000),
                nonce: 21,
            };
            let receipt = dispatch_and_persist_nov_execution_request_with_store_path_v1(
                path.as_path(),
                &request,
            )
            .expect("dispatch should succeed");
            assert!(
                receipt.status,
                "failure_reason={:?}",
                receipt.failure_reason
            );

            let summary =
                get_nov_native_treasury_settlement_summary_with_store_path_v1(path.as_path())
                    .expect("treasury settlement summary helper should succeed");
            assert!(
                summary["settled_nov_total"].as_u64().unwrap_or_default() > 0,
                "helper must return inner result body"
            );
            assert!(summary["settlement_policy"]["source"]
                .as_str()
                .is_some_and(|v| !v.is_empty()));
        });
    }

    #[test]
    fn treasury_settlement_journal_returns_recent_entries() {
        with_test_native_execution_store_path_v1(|path| {
            let request = NovExecutionRequestV1 {
                tx_hash: [0x44; 32],
                chain_id: 1220,
                caller: vec![0x2f; 20],
                target: NovExecutionRequestTargetV1::NativeModule("treasury".to_string()),
                method: "deposit_reserve".to_string(),
                args: serde_json::to_vec(&serde_json::json!({
                    "asset": "NOV",
                    "amount": 13u64
                }))
                .expect("encode args"),
                fee_pay_asset: "NOV".to_string(),
                fee_max_pay_amount: 800,
                fee_slippage_bps: 0,
                gas_like_limit: Some(90_000),
                nonce: 31,
            };
            let receipt = dispatch_and_persist_nov_execution_request_with_store_path_v1(
                path.as_path(),
                &request,
            )
            .expect("dispatch should succeed");
            assert!(receipt.status);

            let journal = run_nov_native_call_from_params_with_store_path_v1(
                &serde_json::json!({
                    "target": {"kind": "native_module", "id": "treasury"},
                    "method": "get_settlement_journal",
                    "args": {"limit": 10},
                }),
                Some(path.as_path()),
            )
            .expect("nov settlement journal should succeed");
            let entries = journal["result"]["entries"]
                .as_array()
                .cloned()
                .unwrap_or_default();
            assert!(
                !entries.is_empty(),
                "journal must expose at least one entry"
            );
            let first = &entries[0];
            assert_eq!(first["kind"].as_str(), Some("fee_settlement"));
            assert_eq!(first["status"].as_str(), Some("applied"));
            assert_eq!(first["policy_event_state"].as_str(), Some("settled"));
            assert!(first["seq"].as_u64().unwrap_or_default() >= 1);
        });
    }

    #[test]
    fn treasury_get_clearing_routes_exposes_treasury_and_amm_sources() {
        with_test_native_execution_store_path_v1(|path| {
            let mut pre = NovNativeExecutionStoreV1::default();
            pre.module_state
                .clearing_nov_liquidity
                .insert("USDT".to_string(), 1_000_000);
            pre.module_state
                .clearing_rate_ppm
                .insert("USDT".to_string(), 100_000);
            pre.module_state
                .fee_oracle_rates_ppm
                .insert("USDT".to_string(), 100_000);
            pre.module_state.fee_oracle_updated_unix_ms = now_unix_millis_v1();
            pre.module_state.fee_oracle_source = "runtime_oracle".to_string();
            pre.module_state.clearing_static_amm_pools.insert(
                "usdt_nov_pool".to_string(),
                NovStaticAmmPoolStateV1 {
                    pool_id: "usdt_nov_pool".to_string(),
                    asset_x: "USDT".to_string(),
                    asset_y: "NOV".to_string(),
                    reserve_x: 1_000_000,
                    reserve_y: 2_500_000,
                    swap_fee_ppm: 3_000,
                    enabled: true,
                },
            );
            save_nov_native_execution_store_v1(path.as_path(), &pre)
                .expect("seed clearing routes store");

            let out = run_nov_native_call_from_params_with_store_path_v1(
                &serde_json::json!({
                    "target": {"kind": "native_module", "id": "treasury"},
                    "method": "get_clearing_routes",
                    "args": {"asset": "USDT"},
                }),
                Some(path.as_path()),
            )
            .expect("get_clearing_routes should succeed");
            let routes = out["result"]["routes"]
                .as_array()
                .cloned()
                .unwrap_or_default();
            assert!(
                routes.len() >= 2,
                "expected at least treasury_direct + amm_pool routes"
            );
            let has_treasury = routes.iter().any(|route| {
                route["route_source"]
                    .as_str()
                    .is_some_and(|value| value == "treasury_direct")
            });
            let has_amm = routes.iter().any(|route| {
                route["route_source"]
                    .as_str()
                    .is_some_and(|value| value == "amm_pool")
            });
            assert!(has_treasury, "treasury_direct route should be present");
            assert!(has_amm, "amm_pool route should be present");
        });
    }

    #[test]
    fn treasury_get_clearing_routes_rejects_oracle_only_treasury_direct() {
        with_test_native_execution_store_path_v1(|path| {
            let mut pre = NovNativeExecutionStoreV1::default();
            pre.module_state
                .clearing_nov_liquidity
                .insert("NNEW".to_string(), 1_000_000);
            pre.module_state
                .fee_oracle_rates_ppm
                .insert("NNEW".to_string(), 1_000_000);
            pre.module_state.fee_oracle_updated_unix_ms = now_unix_millis_v1();
            pre.module_state.fee_oracle_source = "runtime_oracle".to_string();
            save_nov_native_execution_store_v1(path.as_path(), &pre)
                .expect("seed oracle-only clearing route store");

            let out = run_nov_native_call_from_params_with_store_path_v1(
                &serde_json::json!({
                    "target": {"kind": "native_module", "id": "treasury"},
                    "method": "get_clearing_routes",
                    "args": {"asset": "NNEW"},
                }),
                Some(path.as_path()),
            )
            .expect("get_clearing_routes should succeed");
            let routes = out["result"]["routes"]
                .as_array()
                .cloned()
                .unwrap_or_default();
            assert!(
                routes
                    .iter()
                    .all(|route| { route["route_source"].as_str() != Some("treasury_direct") }),
                "oracle-only asset must not expose treasury_direct route: {routes:?}"
            );
        });
    }

    #[test]
    fn treasury_get_clearing_liquidity_blocks_oracle_only_default_price() {
        with_test_native_execution_store_path_v1(|path| {
            let mut pre = NovNativeExecutionStoreV1::default();
            pre.module_state
                .clearing_nov_liquidity
                .insert("NNEW".to_string(), 1_000_000);
            pre.module_state
                .fee_oracle_rates_ppm
                .insert("NNEW".to_string(), 1_000_000);
            pre.module_state.fee_oracle_updated_unix_ms = now_unix_millis_v1();
            pre.module_state.fee_oracle_source = "runtime_oracle".to_string();
            save_nov_native_execution_store_v1(path.as_path(), &pre)
                .expect("seed oracle-only clearing liquidity store");

            let out = run_nov_native_call_from_params_with_store_path_v1(
                &serde_json::json!({
                    "target": {"kind": "native_module", "id": "treasury"},
                    "method": "get_clearing_liquidity",
                    "args": {"asset": "NNEW"},
                }),
                Some(path.as_path()),
            )
            .expect("get_clearing_liquidity should return blocked response");
            assert_eq!(out["found"].as_bool(), Some(false));
            assert_eq!(out["result"]["asset"].as_str(), Some("NNEW"));
            assert_eq!(out["result"]["state"].as_str(), Some("blocked"));
            assert_eq!(out["result"]["clearing_rate_ppm"].as_u64(), Some(0));
            assert_eq!(out["result"]["price_source"].as_str(), Some("unavailable"));
            let reason = out["result"]["reason"].as_str().unwrap_or_default();
            assert!(reason.contains("fee.clearing.route_unavailable"));
            assert!(reason.contains("asset=NNEW has no protocol clearing source"));
        });
    }

    #[test]
    fn protocol_clearing_price_clamps_epoch_and_uses_conservative_pay_redeem() {
        let mut store = NovNativeExecutionStoreV1::default();
        store
            .module_state
            .clearing_rate_ppm
            .insert("USDT".to_string(), 1_000_000);
        store
            .module_state
            .protocol_clearing_nav_rate_ppm
            .insert("USDT".to_string(), 1_300_000);
        store
            .module_state
            .protocol_clearing_amm_twap_rate_ppm
            .insert("USDT".to_string(), 1_280_000);
        store
            .module_state
            .fee_oracle_rates_ppm
            .insert("USDT".to_string(), 1_290_000);
        store.module_state.fee_oracle_updated_unix_ms = 600_000;
        store.module_state.clearing_static_amm_pools.insert(
            "usdt_nov_twap_pool".to_string(),
            NovStaticAmmPoolStateV1 {
                pool_id: "usdt_nov_twap_pool".to_string(),
                asset_x: "USDT".to_string(),
                asset_y: "NOV".to_string(),
                reserve_x: 1_000_000,
                reserve_y: 2_000_000,
                swap_fee_ppm: 3_000,
                enabled: true,
            },
        );

        let price = build_protocol_clearing_price_v1(&store, "USDT", 600_000)
            .expect("protocol clearing price should resolve");
        assert_eq!(price.asset, "USDT");
        assert_eq!(price.p_epoch_ppm, 1_050_000);
        assert!(price.p_pay_ppm < price.p_epoch_ppm);
        assert!(price.p_redeem_ppm > price.p_epoch_ppm);
        assert_eq!(price.state, "healthy");
        assert!(price.sources_used.iter().any(|source| source == "amm_twap"));
        assert!(price
            .sources_used
            .iter()
            .any(|source| source == "treasury_nav"));
        assert!(price
            .sources_used
            .iter()
            .any(|source| source == "permissioned_oracle_ref"));
    }

    #[test]
    fn protocol_clearing_price_rejects_deviated_amm_twap() {
        let mut store = NovNativeExecutionStoreV1::default();
        store
            .module_state
            .clearing_rate_ppm
            .insert("USDT".to_string(), 1_000_000);
        store
            .module_state
            .protocol_clearing_nav_rate_ppm
            .insert("USDT".to_string(), 1_000_000);
        store
            .module_state
            .protocol_clearing_amm_twap_rate_ppm
            .insert("USDT".to_string(), 5_000_000);
        store
            .module_state
            .fee_oracle_rates_ppm
            .insert("USDT".to_string(), 1_000_000);
        store.module_state.fee_oracle_updated_unix_ms = 600_000;
        store.module_state.clearing_static_amm_pools.insert(
            "usdt_nov_bad_twap_pool".to_string(),
            NovStaticAmmPoolStateV1 {
                pool_id: "usdt_nov_bad_twap_pool".to_string(),
                asset_x: "USDT".to_string(),
                asset_y: "NOV".to_string(),
                reserve_x: 1_000_000,
                reserve_y: 2_000_000,
                swap_fee_ppm: 3_000,
                enabled: true,
            },
        );

        let price = build_protocol_clearing_price_v1(&store, "USDT", 600_000)
            .expect("protocol clearing price should resolve without AMM source");
        assert_eq!(price.state, "constrained");
        assert!(!price.sources_used.iter().any(|source| source == "amm_twap"));
        assert!(price
            .sources_rejected
            .iter()
            .any(|reason| reason.starts_with("amm_twap:deviation_bps=")));
        assert_eq!(price.p_epoch_ppm, 1_000_000);
    }

    #[test]
    fn protocol_clearing_price_rejects_low_liquidity_amm_twap() {
        let mut store = NovNativeExecutionStoreV1::default();
        store
            .module_state
            .clearing_rate_ppm
            .insert("USDT".to_string(), 1_000_000);
        store
            .module_state
            .protocol_clearing_nav_rate_ppm
            .insert("USDT".to_string(), 1_000_000);
        store
            .module_state
            .protocol_clearing_amm_twap_rate_ppm
            .insert("USDT".to_string(), 1_000_000);
        store
            .module_state
            .fee_oracle_rates_ppm
            .insert("USDT".to_string(), 1_000_000);
        store.module_state.fee_oracle_updated_unix_ms = 600_000;
        store.module_state.clearing_static_amm_pools.insert(
            "usdt_nov_dust_twap_pool".to_string(),
            NovStaticAmmPoolStateV1 {
                pool_id: "usdt_nov_dust_twap_pool".to_string(),
                asset_x: "USDT".to_string(),
                asset_y: "NOV".to_string(),
                reserve_x: 1_000_000,
                reserve_y: 1,
                swap_fee_ppm: 3_000,
                enabled: true,
            },
        );

        let price = build_protocol_clearing_price_v1(&store, "USDT", 600_000)
            .expect("protocol clearing price should resolve without low-liquidity AMM source");
        assert_eq!(price.state, "constrained");
        assert!(!price.sources_used.iter().any(|source| source == "amm_twap"));
        assert!(price
            .sources_rejected
            .iter()
            .any(|reason| reason == "amm_twap:low_liquidity"));
        assert_eq!(price.p_epoch_ppm, 1_000_000);
    }

    #[test]
    fn protocol_clearing_price_does_not_use_low_liquidity_amm_as_anchor() {
        let mut store = NovNativeExecutionStoreV1::default();
        store
            .module_state
            .clearing_rate_ppm
            .insert("NBAZ".to_string(), 1_000_000);
        store
            .module_state
            .protocol_clearing_amm_twap_rate_ppm
            .insert("NBAZ".to_string(), 5_000_000);
        store
            .module_state
            .fee_oracle_rates_ppm
            .insert("NBAZ".to_string(), 1_000_000);
        store.module_state.fee_oracle_updated_unix_ms = 600_000;
        store.module_state.fee_oracle_source = "runtime_oracle".to_string();
        store.module_state.clearing_static_amm_pools.insert(
            "nbaz_nov_dust_twap_pool".to_string(),
            NovStaticAmmPoolStateV1 {
                pool_id: "nbaz_nov_dust_twap_pool".to_string(),
                asset_x: "NBAZ".to_string(),
                asset_y: "NOV".to_string(),
                reserve_x: 1_000_000,
                reserve_y: 1,
                swap_fee_ppm: 3_000,
                enabled: true,
            },
        );

        let price = build_protocol_clearing_price_v1(&store, "NBAZ", 600_000)
            .expect("protocol clearing price should not anchor on low-liquidity AMM");
        assert_eq!(price.state, "constrained");
        assert_eq!(price.p_prev_ppm, 1_000_000);
        assert_eq!(price.p_ref_ppm, 1_000_000);
        assert_eq!(price.p_epoch_ppm, 1_000_000);
        assert!(!price.sources_used.iter().any(|source| source == "amm_twap"));
        assert!(price
            .sources_used
            .iter()
            .any(|source| source == "permissioned_oracle_ref"));
        assert!(price
            .sources_rejected
            .iter()
            .any(|reason| reason == "amm_twap:low_liquidity"));
        assert!(!price
            .sources_rejected
            .iter()
            .any(|reason| reason.starts_with("permissioned_oracle_ref:deviation_bps=")));
    }

    #[test]
    fn protocol_clearing_price_rejects_oracle_only_without_anchor() {
        let mut store = NovNativeExecutionStoreV1::default();
        store
            .module_state
            .fee_oracle_rates_ppm
            .insert("NNEW".to_string(), 1_000_000);
        store.module_state.fee_oracle_updated_unix_ms = 600_000;
        store.module_state.fee_oracle_source = "runtime_oracle".to_string();

        let err = build_protocol_clearing_price_v1(&store, "NNEW", 600_000)
            .expect_err("oracle-only new asset price must not resolve");
        let err = err.to_string();
        assert!(err.contains("route_unavailable"));
        assert!(err.contains("asset=NNEW has no protocol clearing source"));
    }

    #[test]
    fn protocol_clearing_price_ignores_unpermitted_oracle_source() {
        let mut store = NovNativeExecutionStoreV1::default();
        store
            .module_state
            .clearing_rate_ppm
            .insert("USDT".to_string(), 1_000_000);
        store
            .module_state
            .protocol_clearing_nav_rate_ppm
            .insert("USDT".to_string(), 1_000_000);
        store
            .module_state
            .fee_oracle_rates_ppm
            .insert("USDT".to_string(), 5_000_000);
        store.module_state.fee_oracle_updated_unix_ms = 600_000;
        store.module_state.fee_oracle_source = "untrusted_feed".to_string();
        store.module_state.fee_oracle_allowed_sources = vec!["runtime_oracle".to_string()];

        let price = build_protocol_clearing_price_v1(&store, "USDT", 600_000)
            .expect("protocol clearing price should resolve without unpermitted oracle");
        assert_eq!(price.state, "constrained");
        assert_eq!(price.p_oracle_ref_ppm, None);
        assert!(!price
            .sources_used
            .iter()
            .any(|source| source == "permissioned_oracle_ref"));
        assert!(price
            .sources_rejected
            .iter()
            .any(|reason| reason.starts_with("permissioned_oracle_ref:source_not_allowed")));
        assert_eq!(price.p_epoch_ppm, 1_000_000);
    }

    #[test]
    fn protocol_clearing_price_rejects_stale_oracle_without_prev_pollution() {
        let mut store = NovNativeExecutionStoreV1::default();
        store
            .module_state
            .protocol_clearing_nav_rate_ppm
            .insert("NFOO".to_string(), 1_000_000);
        store
            .module_state
            .fee_oracle_rates_ppm
            .insert("NFOO".to_string(), 5_000_000);
        store.module_state.fee_oracle_updated_unix_ms = 1;
        store.module_state.fee_oracle_source = "runtime_oracle".to_string();

        let price = build_protocol_clearing_price_v1(&store, "NFOO", 600_000)
            .expect("protocol clearing price should resolve without stale oracle");
        assert_eq!(price.state, "constrained");
        assert_eq!(price.p_prev_ppm, 0);
        assert_eq!(price.p_ref_ppm, 1_000_000);
        assert_eq!(price.p_epoch_ppm, 1_000_000);
        assert_eq!(price.p_oracle_ref_ppm, None);
        assert!(!price
            .sources_used
            .iter()
            .any(|source| source == "permissioned_oracle_ref"));
        assert!(price.sources_rejected.iter().any(|reason| {
            reason.starts_with("permissioned_oracle_ref:stale")
                && reason.contains("source=runtime_oracle")
        }));
    }

    #[test]
    fn protocol_clearing_price_rejects_oracle_without_timestamp() {
        let mut store = NovNativeExecutionStoreV1::default();
        store
            .module_state
            .protocol_clearing_nav_rate_ppm
            .insert("NBAR".to_string(), 1_000_000);
        store
            .module_state
            .fee_oracle_rates_ppm
            .insert("NBAR".to_string(), 5_000_000);
        store.module_state.fee_oracle_source = "runtime_oracle".to_string();

        let price = build_protocol_clearing_price_v1(&store, "NBAR", 600_000)
            .expect("protocol clearing price should resolve without untimestamped oracle");
        assert_eq!(price.state, "constrained");
        assert_eq!(price.p_prev_ppm, 0);
        assert_eq!(price.p_ref_ppm, 1_000_000);
        assert_eq!(price.p_epoch_ppm, 1_000_000);
        assert_eq!(price.p_oracle_ref_ppm, None);
        assert!(!price
            .sources_used
            .iter()
            .any(|source| source == "permissioned_oracle_ref"));
        assert!(price.sources_rejected.iter().any(|reason| {
            reason == "permissioned_oracle_ref:missing_timestamp source=runtime_oracle"
        }));
    }

    #[test]
    fn protocol_clearing_price_ignores_disabled_oracle_source_with_rotation() {
        let mut store = NovNativeExecutionStoreV1::default();
        store
            .module_state
            .clearing_rate_ppm
            .insert("USDT".to_string(), 1_000_000);
        store
            .module_state
            .protocol_clearing_nav_rate_ppm
            .insert("USDT".to_string(), 1_000_000);
        store
            .module_state
            .fee_oracle_rates_ppm
            .insert("USDT".to_string(), 5_000_000);
        store.module_state.fee_oracle_updated_unix_ms = 600_000;
        store.module_state.fee_oracle_source = "governance_oracle".to_string();
        store.module_state.fee_oracle_allowed_sources = vec![
            "runtime_oracle".to_string(),
            "governance_oracle".to_string(),
        ];
        store.module_state.fee_oracle_disabled_sources = vec!["governance_oracle".to_string()];
        store
            .module_state
            .fee_oracle_disabled_source_reasons
            .insert(
                "governance_oracle".to_string(),
                "deviation_slash".to_string(),
            );
        store.module_state.fee_oracle_source_rotations.insert(
            "governance_oracle".to_string(),
            "governance_oracle_v2".to_string(),
        );

        let price = build_protocol_clearing_price_v1(&store, "USDT", 600_000)
            .expect("protocol clearing price should resolve without disabled oracle");
        assert_eq!(price.p_oracle_ref_ppm, None);
        assert!(!price
            .sources_used
            .iter()
            .any(|source| source == "permissioned_oracle_ref"));
        assert!(price.sources_rejected.iter().any(|reason| {
            reason.contains("permissioned_oracle_ref:source_disabled")
                && reason.contains("reason=deviation_slash")
                && reason.contains("rotation_target=governance_oracle_v2")
        }));
        assert_eq!(price.p_epoch_ppm, 1_000_000);
    }

    #[test]
    fn fee_quote_uses_protocol_pay_price_and_persists_price_snapshot() {
        let mut store = NovNativeExecutionStoreV1::default();
        store
            .module_state
            .clearing_rate_ppm
            .insert("USDT".to_string(), 1_000_000);
        store
            .module_state
            .protocol_clearing_nav_rate_ppm
            .insert("USDT".to_string(), 1_000_000);
        store
            .module_state
            .protocol_clearing_amm_twap_rate_ppm
            .insert("USDT".to_string(), 1_000_000);
        store
            .module_state
            .fee_oracle_rates_ppm
            .insert("USDT".to_string(), 1_000_000);
        store.module_state.fee_oracle_updated_unix_ms = 600_000;
        store.module_state.clearing_static_amm_pools.insert(
            "usdt_nov_quote_twap_pool".to_string(),
            NovStaticAmmPoolStateV1 {
                pool_id: "usdt_nov_quote_twap_pool".to_string(),
                asset_x: "USDT".to_string(),
                asset_y: "NOV".to_string(),
                reserve_x: 1_000_000,
                reserve_y: 2_000_000,
                swap_fee_ppm: 3_000,
                enabled: true,
            },
        );
        let request = NovExecutionRequestV1 {
            tx_hash: [0x9cu8; 32],
            chain_id: 9001,
            caller: vec![0x9c; 20],
            target: NovExecutionRequestTargetV1::NativeModule("treasury".to_string()),
            method: "deposit_reserve".to_string(),
            args: Vec::new(),
            fee_pay_asset: "USDT".to_string(),
            fee_max_pay_amount: 10_000,
            fee_slippage_bps: 0,
            gas_like_limit: Some(90_000),
            nonce: 1,
        };

        let quote = quote_fee_policy_from_execution_request_v1(&request, &mut store, 600_000)
            .expect("quote should use protocol clearing price");
        assert_eq!(quote.price_source, "protocol_clearing_price:healthy");
        assert!(quote.rate_ppm < 1_000_000);
        let persisted = store
            .module_state
            .protocol_clearing_prices
            .get("USDT")
            .expect("price snapshot should be persisted");
        assert_eq!(persisted.p_pay_ppm, quote.rate_ppm);
        assert!(persisted.p_redeem_ppm > persisted.p_epoch_ppm);
    }

    #[test]
    fn fee_quote_rejects_oracle_only_without_protocol_anchor() {
        let mut store = NovNativeExecutionStoreV1::default();
        store
            .module_state
            .fee_oracle_rates_ppm
            .insert("NNEW".to_string(), 1_000_000);
        store.module_state.fee_oracle_updated_unix_ms = 600_000;
        store.module_state.fee_oracle_source = "runtime_oracle".to_string();
        let request = NovExecutionRequestV1 {
            tx_hash: [0x9du8; 32],
            chain_id: 9002,
            caller: vec![0x9d; 20],
            target: NovExecutionRequestTargetV1::NativeModule("treasury".to_string()),
            method: "deposit_reserve".to_string(),
            args: Vec::new(),
            fee_pay_asset: "NNEW".to_string(),
            fee_max_pay_amount: 10_000,
            fee_slippage_bps: 0,
            gas_like_limit: Some(90_000),
            nonce: 1,
        };

        let err = quote_fee_policy_from_execution_request_v1(&request, &mut store, 600_000)
            .expect_err("oracle-only new asset fee quote must fail closed");
        let err = err.to_string();
        assert!(err.starts_with("fee.quote.rate_unavailable"));
        assert!(err.contains("fee.clearing.route_unavailable"));
        assert!(err.contains("asset=NNEW has no protocol clearing source"));
        assert!(!store
            .module_state
            .protocol_clearing_prices
            .contains_key("NNEW"));
    }

    #[test]
    fn fee_clearing_prefers_best_route_by_expected_nov_out() {
        with_test_native_execution_store_path_v1(|path| {
            let mut pre = NovNativeExecutionStoreV1::default();
            pre.module_state
                .clearing_nov_liquidity
                .insert("USDT".to_string(), 1_000_000);
            pre.module_state
                .clearing_rate_ppm
                .insert("USDT".to_string(), 1_000_000);
            pre.module_state.clearing_static_amm_pools.insert(
                "usdt_nov_pool".to_string(),
                NovStaticAmmPoolStateV1 {
                    pool_id: "usdt_nov_pool".to_string(),
                    asset_x: "USDT".to_string(),
                    asset_y: "NOV".to_string(),
                    reserve_x: 1_000_000,
                    reserve_y: 3_000_000,
                    swap_fee_ppm: 3_000,
                    enabled: true,
                },
            );
            save_nov_native_execution_store_v1(path.as_path(), &pre)
                .expect("seed native execution store");

            let request = NovExecutionRequestV1 {
                tx_hash: [0x8au8; 32],
                chain_id: 7101,
                caller: vec![0x55; 20],
                target: NovExecutionRequestTargetV1::NativeModule("treasury".to_string()),
                method: "deposit_reserve".to_string(),
                args: serde_json::to_vec(&serde_json::json!({
                    "asset": "USDT",
                    "amount": 5u64
                }))
                .expect("encode args"),
                fee_pay_asset: "USDT".to_string(),
                fee_max_pay_amount: 5_000,
                fee_slippage_bps: 10_000,
                gas_like_limit: Some(90_000),
                nonce: 13,
            };
            let receipt = dispatch_and_persist_nov_execution_request_with_store_path_v1(
                path.as_path(),
                &request,
            )
            .expect("dispatch should succeed");
            assert!(
                receipt.status,
                "failure_reason={:?}",
                receipt.failure_reason
            );
            assert_eq!(receipt.fee_clearing_source, "amm_pool");
            assert_eq!(
                receipt
                    .route_meta
                    .as_ref()
                    .map(|meta| meta.route_source.as_str()),
                Some("amm_pool")
            );
            let route_meta = receipt
                .route_meta
                .as_ref()
                .expect("route_meta should exist");
            assert!(route_meta.route_id.starts_with("route:amm_pool:"));
            assert_eq!(
                route_meta.selection_reason,
                "expected_out_then_liquidity_then_freshness"
            );
            assert_eq!(route_meta.candidate_route_count, 2);
        });
    }

    #[test]
    fn fee_clearing_exposes_candidate_routes_and_selected_reason_with_three_routes() {
        with_test_native_execution_store_path_v1(|path| {
            let mut pre = NovNativeExecutionStoreV1::default();
            pre.module_state
                .clearing_nov_liquidity
                .insert("USDT".to_string(), 1_000_000);
            pre.module_state
                .clearing_rate_ppm
                .insert("USDT".to_string(), 1_000_000);
            pre.module_state.clearing_static_amm_pools.insert(
                "usdt_nov_pool_a".to_string(),
                NovStaticAmmPoolStateV1 {
                    pool_id: "usdt_nov_pool_a".to_string(),
                    asset_x: "USDT".to_string(),
                    asset_y: "NOV".to_string(),
                    reserve_x: 1_000_000,
                    reserve_y: 2_000_000,
                    swap_fee_ppm: 3_000,
                    enabled: true,
                },
            );
            pre.module_state.clearing_static_amm_pools.insert(
                "usdt_nov_pool_b".to_string(),
                NovStaticAmmPoolStateV1 {
                    pool_id: "usdt_nov_pool_b".to_string(),
                    asset_x: "USDT".to_string(),
                    asset_y: "NOV".to_string(),
                    reserve_x: 1_000_000,
                    reserve_y: 3_000_000,
                    swap_fee_ppm: 3_000,
                    enabled: true,
                },
            );
            save_nov_native_execution_store_v1(path.as_path(), &pre)
                .expect("seed native execution store");

            let request = NovExecutionRequestV1 {
                tx_hash: [0x8bu8; 32],
                chain_id: 7102,
                caller: vec![0x66; 20],
                target: NovExecutionRequestTargetV1::NativeModule("treasury".to_string()),
                method: "deposit_reserve".to_string(),
                args: serde_json::to_vec(&serde_json::json!({
                    "asset": "USDT",
                    "amount": 7u64
                }))
                .expect("encode args"),
                fee_pay_asset: "USDT".to_string(),
                fee_max_pay_amount: 6_000,
                fee_slippage_bps: 10_000,
                gas_like_limit: Some(90_000),
                nonce: 14,
            };
            let receipt = dispatch_and_persist_nov_execution_request_with_store_path_v1(
                path.as_path(),
                &request,
            )
            .expect("dispatch should succeed");
            assert!(receipt.status);
            let route_meta = receipt
                .route_meta
                .as_ref()
                .expect("route_meta should exist");
            assert_eq!(
                route_meta.selection_reason,
                "expected_out_then_liquidity_then_freshness"
            );
            assert_eq!(route_meta.candidate_route_count, 3);

            let candidates = run_nov_native_call_from_params_with_store_path_v1(
                &serde_json::json!({
                    "target": {"kind": "native_module", "id": "treasury"},
                    "method": "get_last_clearing_candidates",
                    "args": {},
                }),
                Some(path.as_path()),
            )
            .expect("get_last_clearing_candidates should succeed");
            assert_eq!(candidates["result"]["route_count"].as_u64(), Some(3));
            let routes = candidates["result"]["routes"]
                .as_array()
                .cloned()
                .unwrap_or_default();
            assert_eq!(routes.len(), 3);

            let selected = run_nov_native_call_from_params_with_store_path_v1(
                &serde_json::json!({
                    "target": {"kind": "native_module", "id": "treasury"},
                    "method": "get_last_clearing_route",
                    "args": {},
                }),
                Some(path.as_path()),
            )
            .expect("get_last_clearing_route should succeed");
            assert_eq!(
                selected["result"]["selection_reason"].as_str(),
                Some("expected_out_then_liquidity_then_freshness")
            );
            assert_eq!(
                selected["result"]["candidate_route_count"].as_u64(),
                Some(3)
            );
        });
    }

    #[test]
    fn fee_settlement_paused_returns_standardized_failure() {
        with_test_native_execution_store_path_v1(|path| {
            let mut pre = NovNativeExecutionStoreV1::default();
            pre.module_state.treasury_settlement_paused = true;
            save_nov_native_execution_store_v1(path.as_path(), &pre)
                .expect("seed paused settlement state");

            let request = NovExecutionRequestV1 {
                tx_hash: [0x32; 32],
                chain_id: 1202,
                caller: vec![0x22; 20],
                target: NovExecutionRequestTargetV1::NativeModule("treasury".to_string()),
                method: "deposit_reserve".to_string(),
                args: serde_json::to_vec(&serde_json::json!({
                    "asset": "NOV",
                    "amount": 1u64
                }))
                .expect("encode args"),
                fee_pay_asset: "NOV".to_string(),
                fee_max_pay_amount: 500,
                fee_slippage_bps: 0,
                gas_like_limit: Some(80_000),
                nonce: 13,
            };
            let receipt = dispatch_and_persist_nov_execution_request_with_store_path_v1(
                path.as_path(),
                &request,
            )
            .expect("dispatch should return settlement paused receipt");
            assert!(!receipt.status);
            assert_eq!(receipt.module, "fee");
            assert_eq!(receipt.method, "settlement");
            assert!(receipt
                .failure_reason
                .clone()
                .unwrap_or_default()
                .starts_with("fee.settlement.settlement_paused"));

            let state = load_nov_native_execution_store_v1(path.as_path())
                .expect("load native execution store");
            assert_eq!(
                state
                    .module_state
                    .treasury_settlement_failure_counts
                    .get("settlement_paused")
                    .copied()
                    .unwrap_or_default(),
                1
            );
        });
    }

    #[test]
    fn treasury_redeem_reserve_fails_when_insufficient() {
        with_test_native_execution_store_path_v1(|path| {
            let mut pre = NovNativeExecutionStoreV1::default();
            pre.module_state
                .treasury_reserves
                .insert("NOV".to_string(), 10);
            pre.module_state.treasury_reserve_bucket_nov = 10;
            save_nov_native_execution_store_v1(path.as_path(), &pre).expect("seed reserve state");

            let request = NovExecutionRequestV1 {
                tx_hash: [0x33; 32],
                chain_id: 1203,
                caller: vec![0x23; 20],
                target: NovExecutionRequestTargetV1::NativeModule("treasury".to_string()),
                method: "redeem_reserve".to_string(),
                args: serde_json::to_vec(&serde_json::json!({
                    "asset": "NOV",
                    "amount": 1_000_000u64
                }))
                .expect("encode args"),
                fee_pay_asset: "NOV".to_string(),
                fee_max_pay_amount: 500,
                fee_slippage_bps: 0,
                gas_like_limit: Some(80_000),
                nonce: 14,
            };
            let receipt = dispatch_and_persist_nov_execution_request_with_store_path_v1(
                path.as_path(),
                &request,
            )
            .expect("dispatch should return insufficient reserve receipt");
            assert!(!receipt.status);
            assert_eq!(receipt.module, "treasury");
            assert_eq!(receipt.method, "redeem_reserve");
            assert!(receipt
                .failure_reason
                .clone()
                .unwrap_or_default()
                .starts_with("fee.settlement.insufficient_reserve"));
        });
    }

    #[test]
    fn treasury_redeem_reserve_updates_accounting_and_journal() {
        with_test_native_execution_store_path_v1(|path| {
            let mut pre = NovNativeExecutionStoreV1::default();
            pre.module_state
                .treasury_reserves
                .insert("NOV".to_string(), 250);
            pre.module_state.treasury_reserve_bucket_nov = 200;
            pre.module_state.treasury_fee_bucket_nov = 30;
            pre.module_state.treasury_risk_buffer_nov = 20;
            pre.module_state.treasury_settled_nov_total = 250;
            pre.module_state.treasury_settlements = 1;
            save_nov_native_execution_store_v1(path.as_path(), &pre).expect("seed reserve state");

            let request = NovExecutionRequestV1 {
                tx_hash: [0x34; 32],
                chain_id: 1204,
                caller: vec![0x24; 20],
                target: NovExecutionRequestTargetV1::NativeModule("treasury".to_string()),
                method: "redeem_reserve".to_string(),
                args: serde_json::to_vec(&serde_json::json!({
                    "asset": "NOV",
                    "amount": 50u64
                }))
                .expect("encode args"),
                fee_pay_asset: "NOV".to_string(),
                fee_max_pay_amount: 500,
                fee_slippage_bps: 0,
                gas_like_limit: Some(80_000),
                nonce: 15,
            };
            let receipt = dispatch_and_persist_nov_execution_request_with_store_path_v1(
                path.as_path(),
                &request,
            )
            .expect("dispatch should return successful redeem receipt");
            assert!(receipt.status);
            assert_eq!(receipt.module, "treasury");
            assert_eq!(receipt.method, "redeem_reserve");

            let summary = run_nov_native_call_from_params_with_store_path_v1(
                &serde_json::json!({
                    "target": {"kind": "native_module", "id": "treasury"},
                    "method": "get_settlement_summary",
                    "args": {},
                }),
                Some(path.as_path()),
            )
            .expect("nov settlement summary should succeed");
            assert_eq!(summary["result"]["redeemed_nov_total"].as_u64(), Some(50));
            assert_eq!(
                summary["result"]["redeemed_by_asset"]["NOV"].as_u64(),
                Some(50)
            );
            assert_eq!(
                summary["result"]["accounting"]["bucket_consistent_with_net_settled"].as_bool(),
                Some(true)
            );
            assert!(
                summary["result"]["accounting"]["net_settled_nov"]
                    .as_u64()
                    .unwrap_or_default()
                    >= 200
            );

            let journal = run_nov_native_call_from_params_with_store_path_v1(
                &serde_json::json!({
                    "target": {"kind": "native_module", "id": "treasury"},
                    "method": "get_settlement_journal",
                    "args": {"limit": 5},
                }),
                Some(path.as_path()),
            )
            .expect("nov settlement journal should succeed");
            let entries = journal["result"]["entries"]
                .as_array()
                .cloned()
                .unwrap_or_default();
            assert!(
                entries.len() >= 2,
                "journal should include both fee_settlement and reserve_redeem entries"
            );
            let last = entries.last().cloned().unwrap_or(serde_json::Value::Null);
            assert_eq!(last["kind"].as_str(), Some("reserve_redeem"));
            assert_eq!(last["source_asset"].as_str(), Some("NOV"));
        });
    }

    #[test]
    fn fee_clearing_fails_when_liquidity_is_insufficient() {
        with_test_native_execution_store_path_v1(|path| {
            let mut pre = NovNativeExecutionStoreV1::default();
            pre.module_state
                .clearing_nov_liquidity
                .insert("USDT".to_string(), 1);
            save_nov_native_execution_store_v1(path.as_path(), &pre)
                .expect("seed native execution store");

            let request = NovExecutionRequestV1 {
                tx_hash: [0x51; 32],
                chain_id: 991,
                caller: vec![0x33; 20],
                target: NovExecutionRequestTargetV1::NativeModule("treasury".to_string()),
                method: "deposit_reserve".to_string(),
                args: serde_json::to_vec(&serde_json::json!({
                    "asset": "USDT",
                    "amount": 10u64
                }))
                .expect("encode args"),
                fee_pay_asset: "USDT".to_string(),
                fee_max_pay_amount: 100,
                fee_slippage_bps: 100,
                gas_like_limit: Some(90_000),
                nonce: 7,
            };
            let receipt = dispatch_and_persist_nov_execution_request_with_store_path_v1(
                path.as_path(),
                &request,
            )
            .expect("dispatch should return failure receipt");
            assert!(!receipt.status);
            assert_eq!(receipt.module, "fee");
            assert_eq!(receipt.method, "settlement");
            assert!(receipt
                .failure_reason
                .clone()
                .unwrap_or_default()
                .starts_with("fee.clearing.insufficient_liquidity"));
            assert_eq!(receipt.settled_fee_nov, 0);
            assert_eq!(receipt.paid_amount, 0);

            let state = load_nov_native_execution_store_v1(path.as_path())
                .expect("load native execution store");
            assert_eq!(
                state
                    .module_state
                    .clearing_nov_liquidity
                    .get("USDT")
                    .copied(),
                Some(1)
            );
            assert_eq!(
                state
                    .module_state
                    .treasury_reserves
                    .get("NOV")
                    .copied()
                    .unwrap_or(0),
                0
            );
        });
    }

    #[test]
    fn fee_quote_failure_max_pay_exceeded_is_standardized_as_quote_phase() {
        with_test_native_execution_store_path_v1(|path| {
            let request = NovExecutionRequestV1 {
                tx_hash: [0x71; 32],
                chain_id: 7001,
                caller: vec![0x44; 20],
                target: NovExecutionRequestTargetV1::NativeModule("treasury".to_string()),
                method: "deposit_reserve".to_string(),
                args: serde_json::to_vec(&serde_json::json!({
                    "asset": "USDT",
                    "amount": 1u64
                }))
                .expect("encode args"),
                fee_pay_asset: "USDT".to_string(),
                fee_max_pay_amount: 1,
                fee_slippage_bps: 0,
                gas_like_limit: Some(90_000),
                nonce: 8,
            };
            let receipt = dispatch_and_persist_nov_execution_request_with_store_path_v1(
                path.as_path(),
                &request,
            )
            .expect("dispatch should return standardized quote failure");
            assert!(!receipt.status);
            assert_eq!(receipt.module, "fee");
            assert_eq!(receipt.method, "quote");
            assert!(receipt
                .failure_reason
                .clone()
                .unwrap_or_default()
                .starts_with("fee.quote.max_pay_exceeded"));

            let state = load_nov_native_execution_store_v1(path.as_path())
                .expect("load native execution store");
            assert_eq!(
                state
                    .module_state
                    .fee_quote_failure_counts
                    .iter()
                    .find(|(k, _)| k.starts_with("USDT:max_pay_exceeded"))
                    .map(|(_, v)| *v)
                    .unwrap_or_default(),
                1
            );
        });
    }

    #[test]
    fn fee_quote_prefers_runtime_oracle_rate_when_fresh() {
        with_test_native_execution_store_path_v1(|path| {
            let mut pre = NovNativeExecutionStoreV1::default();
            pre.module_state
                .fee_oracle_rates_ppm
                .insert("USDT".to_string(), 3_000_000);
            pre.module_state.fee_oracle_updated_unix_ms = now_unix_millis_v1();
            pre.module_state.fee_oracle_source = "runtime_oracle".to_string();
            save_nov_native_execution_store_v1(path.as_path(), &pre)
                .expect("seed native execution store");

            let request = NovExecutionRequestV1 {
                tx_hash: [0x72; 32],
                chain_id: 7002,
                caller: vec![0x55; 20],
                target: NovExecutionRequestTargetV1::NativeModule("treasury".to_string()),
                method: "deposit_reserve".to_string(),
                args: serde_json::to_vec(&serde_json::json!({
                    "asset": "USDT",
                    "amount": 2u64
                }))
                .expect("encode args"),
                fee_pay_asset: "USDT".to_string(),
                fee_max_pay_amount: 1_000,
                fee_slippage_bps: 50,
                gas_like_limit: Some(90_000),
                nonce: 9,
            };
            let receipt = dispatch_and_persist_nov_execution_request_with_store_path_v1(
                path.as_path(),
                &request,
            )
            .expect("dispatch should succeed");
            assert!(receipt.status);
            assert!(receipt
                .fee_price_source
                .contains("quote=protocol_clearing_price:constrained"));
            assert!(receipt
                .fee_price_source
                .contains("rate_source=protocol_clearing_price:constrained"));
            assert!(receipt.fee_quote_id.starts_with("q-"));
        });
    }

    #[test]
    fn fee_clearing_fails_with_route_unavailable() {
        with_test_native_execution_store_path_v1(|path| {
            let mut pre = NovNativeExecutionStoreV1::default();
            pre.module_state
                .fee_oracle_rates_ppm
                .insert("DOGE".to_string(), 1_000_000);
            pre.module_state.fee_oracle_updated_unix_ms = now_unix_millis_v1();
            pre.module_state.fee_oracle_source = "runtime_oracle".to_string();
            save_nov_native_execution_store_v1(path.as_path(), &pre)
                .expect("seed native execution store");

            let request = NovExecutionRequestV1 {
                tx_hash: [0x73; 32],
                chain_id: 7003,
                caller: vec![0x66; 20],
                target: NovExecutionRequestTargetV1::NativeModule("treasury".to_string()),
                method: "deposit_reserve".to_string(),
                args: serde_json::to_vec(&serde_json::json!({
                    "asset": "DOGE",
                    "amount": 2u64
                }))
                .expect("encode args"),
                fee_pay_asset: "DOGE".to_string(),
                fee_max_pay_amount: 1_000,
                fee_slippage_bps: 100,
                gas_like_limit: Some(90_000),
                nonce: 10,
            };
            let receipt = dispatch_and_persist_nov_execution_request_with_store_path_v1(
                path.as_path(),
                &request,
            )
            .expect("dispatch should return oracle-only quote rejection receipt");
            assert!(!receipt.status);
            assert_eq!(receipt.module, "fee");
            assert_eq!(receipt.method, "quote");
            let failure = receipt.failure_reason.clone().unwrap_or_default();
            assert!(failure.starts_with("fee.quote.rate_unavailable"));
            assert!(failure.contains("fee.clearing.route_unavailable"));
        });
    }

    #[test]
    fn fee_clearing_fails_with_slippage_exceeded() {
        with_test_native_execution_store_path_v1(|path| {
            let mut pre = NovNativeExecutionStoreV1::default();
            pre.module_state
                .fee_oracle_rates_ppm
                .insert("USDT".to_string(), 3_000_000);
            pre.module_state
                .clearing_rate_ppm
                .insert("USDT".to_string(), 100_000);
            pre.module_state.fee_oracle_updated_unix_ms = now_unix_millis_v1();
            pre.module_state.fee_oracle_source = "runtime_oracle".to_string();
            save_nov_native_execution_store_v1(path.as_path(), &pre)
                .expect("seed native execution store");

            let request = NovExecutionRequestV1 {
                tx_hash: [0x74; 32],
                chain_id: 7004,
                caller: vec![0x77; 20],
                target: NovExecutionRequestTargetV1::NativeModule("treasury".to_string()),
                method: "deposit_reserve".to_string(),
                args: serde_json::to_vec(&serde_json::json!({
                    "asset": "USDT",
                    "amount": 2u64
                }))
                .expect("encode args"),
                fee_pay_asset: "USDT".to_string(),
                fee_max_pay_amount: 100,
                fee_slippage_bps: 50,
                gas_like_limit: Some(90_000),
                nonce: 11,
            };

            let receipt = dispatch_and_persist_nov_execution_request_with_store_path_v1(
                path.as_path(),
                &request,
            )
            .expect("dispatch should return slippage exceeded receipt");
            assert!(!receipt.status);
            assert_eq!(receipt.module, "fee");
            assert_eq!(receipt.method, "quote");
            assert!(receipt
                .failure_reason
                .clone()
                .unwrap_or_default()
                .starts_with("fee.quote.max_pay_exceeded"));
        });
    }

    #[test]
    fn fee_clearing_fails_when_global_clearing_is_disabled() {
        with_test_native_execution_store_path_v1(|path| {
            let mut pre = NovNativeExecutionStoreV1::default();
            pre.module_state.clearing_enabled = false;
            save_nov_native_execution_store_v1(path.as_path(), &pre)
                .expect("seed clearing disabled store");

            let request = NovExecutionRequestV1 {
                tx_hash: [0x75; 32],
                chain_id: 7005,
                caller: vec![0x88; 20],
                target: NovExecutionRequestTargetV1::NativeModule("treasury".to_string()),
                method: "deposit_reserve".to_string(),
                args: serde_json::to_vec(&serde_json::json!({
                    "asset": "USDT",
                    "amount": 2u64
                }))
                .expect("encode args"),
                fee_pay_asset: "USDT".to_string(),
                fee_max_pay_amount: 1_000,
                fee_slippage_bps: 50,
                gas_like_limit: Some(90_000),
                nonce: 12,
            };
            let receipt = dispatch_and_persist_nov_execution_request_with_store_path_v1(
                path.as_path(),
                &request,
            )
            .expect("dispatch should return clearing disabled receipt");
            assert!(!receipt.status);
            assert_eq!(receipt.module, "fee");
            assert_eq!(receipt.method, "settlement");
            assert!(receipt
                .failure_reason
                .clone()
                .unwrap_or_default()
                .starts_with("fee.clearing.clearing_disabled"));

            let state = load_nov_native_execution_store_v1(path.as_path())
                .expect("load native execution store");
            assert_eq!(
                state.module_state.last_clearing_failure_code.as_str(),
                "fee.clearing.clearing_disabled"
            );
        });
    }

    #[test]
    fn fee_clearing_fails_when_daily_hard_limit_is_exceeded() {
        with_test_native_execution_store_path_v1(|path| {
            let mut pre = NovNativeExecutionStoreV1::default();
            pre.module_state.clearing_enabled = true;
            pre.module_state.clearing_daily_nov_hard_limit = 10;
            pre.module_state.clearing_daily_nov_used = 9;
            save_nov_native_execution_store_v1(path.as_path(), &pre)
                .expect("seed daily limit store");

            let request = NovExecutionRequestV1 {
                tx_hash: [0x76; 32],
                chain_id: 7006,
                caller: vec![0x99; 20],
                target: NovExecutionRequestTargetV1::NativeModule("treasury".to_string()),
                method: "deposit_reserve".to_string(),
                args: serde_json::to_vec(&serde_json::json!({
                    "asset": "USDT",
                    "amount": 2u64
                }))
                .expect("encode args"),
                fee_pay_asset: "USDT".to_string(),
                fee_max_pay_amount: 1_000,
                fee_slippage_bps: 50,
                gas_like_limit: Some(90_000),
                nonce: 13,
            };
            let receipt = dispatch_and_persist_nov_execution_request_with_store_path_v1(
                path.as_path(),
                &request,
            )
            .expect("dispatch should return daily limit exceeded receipt");
            assert!(!receipt.status);
            assert_eq!(receipt.module, "fee");
            assert_eq!(receipt.method, "settlement");
            assert!(receipt
                .failure_reason
                .clone()
                .unwrap_or_default()
                .starts_with("fee.clearing.daily_volume_exceeded"));
        });
    }

    #[test]
    fn fee_clearing_fails_when_risk_buffer_gate_is_enabled_and_below_min() {
        with_test_native_execution_store_path_v1(|path| {
            let mut pre = NovNativeExecutionStoreV1::default();
            pre.module_state.clearing_enabled = true;
            pre.module_state.clearing_require_healthy_risk_buffer = true;
            pre.module_state.treasury_min_risk_buffer_nov = 1_000;
            pre.module_state.treasury_risk_buffer_nov = 100;
            save_nov_native_execution_store_v1(path.as_path(), &pre)
                .expect("seed risk-buffer-gated store");

            let request = NovExecutionRequestV1 {
                tx_hash: [0x79; 32],
                chain_id: 7009,
                caller: vec![0xab; 20],
                target: NovExecutionRequestTargetV1::NativeModule("treasury".to_string()),
                method: "deposit_reserve".to_string(),
                args: serde_json::to_vec(&serde_json::json!({
                    "asset": "USDT",
                    "amount": 2u64
                }))
                .expect("encode args"),
                fee_pay_asset: "USDT".to_string(),
                fee_max_pay_amount: 1_000,
                fee_slippage_bps: 50,
                gas_like_limit: Some(90_000),
                nonce: 15,
            };
            let receipt = dispatch_and_persist_nov_execution_request_with_store_path_v1(
                path.as_path(),
                &request,
            )
            .expect("dispatch should return risk buffer gate receipt");
            assert!(!receipt.status);
            assert_eq!(receipt.module, "fee");
            assert_eq!(receipt.method, "settlement");
            assert!(receipt
                .failure_reason
                .clone()
                .unwrap_or_default()
                .starts_with("fee.clearing.risk_buffer_below_min"));

            let state = load_nov_native_execution_store_v1(path.as_path())
                .expect("load native execution store");
            assert_eq!(
                state.module_state.last_clearing_failure_code.as_str(),
                "fee.clearing.risk_buffer_below_min"
            );
        });
    }

    #[test]
    fn treasury_settlement_policy_query_exposes_bucket_boundaries() {
        with_test_native_execution_store_path_v1(|path| {
            let mut pre = NovNativeExecutionStoreV1::default();
            pre.module_state.treasury_min_reserve_bucket_nov = 50;
            pre.module_state.treasury_min_fee_bucket_nov = 30;
            pre.module_state.treasury_min_risk_buffer_nov = 200;
            pre.module_state.treasury_reserve_bucket_nov = 20;
            pre.module_state.treasury_fee_bucket_nov = 40;
            pre.module_state.treasury_risk_buffer_nov = 100;
            pre.module_state.clearing_require_healthy_risk_buffer = true;
            save_nov_native_execution_store_v1(path.as_path(), &pre)
                .expect("seed boundary policy store");

            let out = run_nov_native_call_from_params_with_store_path_v1(
                &serde_json::json!({
                    "target": {"kind": "native_module", "id": "treasury"},
                    "method": "get_settlement_policy",
                    "args": {}
                }),
                Some(path.as_path()),
            )
            .expect("get_settlement_policy should succeed");

            assert_eq!(
                out["result"]["policy"]["min_reserve_bucket_nov"].as_u64(),
                Some(50)
            );
            assert_eq!(
                out["result"]["policy"]["min_fee_bucket_nov"].as_u64(),
                Some(30)
            );
            assert_eq!(
                out["result"]["policy"]["clearing_require_healthy_risk_buffer"].as_bool(),
                Some(true)
            );
            assert_eq!(
                out["result"]["bucket_boundaries"]["reserve_bucket"]["status"].as_str(),
                Some("below_min")
            );
            assert_eq!(
                out["result"]["bucket_boundaries"]["fee_bucket"]["status"].as_str(),
                Some("healthy")
            );
            assert_eq!(
                out["result"]["bucket_boundaries"]["risk_buffer"]["status"].as_str(),
                Some("below_min")
            );
            assert_eq!(
                out["result"]["clearing_policy_gate"]["can_clear_non_nov_now"].as_bool(),
                Some(false)
            );
        });
    }

    #[test]
    fn treasury_redeem_reserve_rejects_when_bucket_would_drop_below_min() {
        with_test_native_execution_store_path_v1(|path| {
            let mut pre = NovNativeExecutionStoreV1::default();
            pre.module_state
                .treasury_reserves
                .insert("NOV".to_string(), 1_000);
            pre.module_state.treasury_reserve_bucket_nov = 200;
            pre.module_state.treasury_min_reserve_bucket_nov = 150;
            save_nov_native_execution_store_v1(path.as_path(), &pre).expect("seed reserve state");

            let request = NovExecutionRequestV1 {
                tx_hash: [0x7a; 32],
                chain_id: 7010,
                caller: vec![0xcd; 20],
                target: NovExecutionRequestTargetV1::NativeModule("treasury".to_string()),
                method: "redeem_reserve".to_string(),
                args: serde_json::to_vec(&serde_json::json!({
                    "asset": "NOV",
                    "amount": 100u64
                }))
                .expect("encode args"),
                fee_pay_asset: "NOV".to_string(),
                fee_max_pay_amount: 10_000,
                fee_slippage_bps: 50,
                gas_like_limit: Some(90_000),
                nonce: 16,
            };
            let receipt = dispatch_and_persist_nov_execution_request_with_store_path_v1(
                path.as_path(),
                &request,
            )
            .expect("dispatch should return reserve bucket min guard receipt");
            assert!(!receipt.status);
            assert_eq!(receipt.module, "treasury");
            assert_eq!(receipt.method, "redeem_reserve");
            assert!(receipt
                .failure_reason
                .clone()
                .unwrap_or_default()
                .starts_with("fee.settlement.reserve_bucket_below_min"));
        });
    }

    #[test]
    fn governance_submit_proposal_exposes_aoem_semantic_commit() {
        with_test_native_execution_store_path_v1(|path| {
            let request = NovExecutionRequestV1 {
                tx_hash: [0x9a; 32],
                chain_id: 7011,
                caller: vec![0x9a; 20],
                target: NovExecutionRequestTargetV1::NativeModule("governance".to_string()),
                method: "submit_proposal".to_string(),
                args: serde_json::to_vec(&serde_json::json!({
                    "proposal_type": "treasury_policy_update",
                    "title": "AOEM policy commit visibility",
                    "payload": {"policy_version": 12u64}
                }))
                .expect("encode proposal args"),
                fee_pay_asset: "NOV".to_string(),
                fee_max_pay_amount: 10_000,
                fee_slippage_bps: 50,
                gas_like_limit: Some(90_000),
                nonce: 18,
            };
            let receipt = dispatch_and_persist_nov_execution_request_with_store_path_v1(
                path.as_path(),
                &request,
            )
            .expect("dispatch should submit governance proposal");
            assert!(receipt.status);
            assert_eq!(receipt.module, "governance");
            assert_eq!(receipt.method, "submit_proposal");
            assert_eq!(
                receipt.logs[0].event.as_str(),
                "governance.proposal_submitted"
            );
            let aoem_meta = receipt
                .aoem_semantic_ingress
                .as_ref()
                .expect("submit proposal receipt should expose AOEM semantic ingress");
            let aoem_commit = receipt
                .aoem_semantic_commit
                .as_ref()
                .expect("submit proposal receipt should expose AOEM semantic commit");
            assert_eq!(aoem_commit.execution_kernel, "AOEM");
            assert_eq!(aoem_commit.subject, "governance");
            assert_eq!(aoem_commit.action, "submit_proposal");
            assert_eq!(aoem_commit.tx_ref, receipt.tx_hash);
            assert_eq!(aoem_commit.sequence, 1);
            assert_eq!(aoem_commit.sequence, aoem_meta.semantic_ledger_sequence);
            assert_eq!(
                aoem_commit.commit_seal,
                aoem_meta.semantic_ledger_commit_seal
            );
            assert_eq!(aoem_commit.semantic_delta_count, 1);

            let stored = load_nov_native_execution_store_v1(path.as_path())
                .expect("store should be readable after proposal");
            assert_eq!(stored.module_state.governance_proposals.len(), 1);
            assert_eq!(stored.module_state.aoem_semantic_ledger_sequence, 1);
            assert_eq!(
                stored.module_state.aoem_semantic_ledger_head,
                aoem_commit.commit_seal
            );
        });
    }

    #[test]
    fn governance_apply_treasury_policy_updates_version_and_source() {
        with_test_native_execution_store_path_v1(|path| {
            let request = NovExecutionRequestV1 {
                tx_hash: [0x7b; 32],
                chain_id: 7011,
                caller: vec![0xde; 20],
                target: NovExecutionRequestTargetV1::NativeModule("governance".to_string()),
                method: "apply_treasury_policy".to_string(),
                args: serde_json::to_vec(&serde_json::json!({
                    "governance_authorized": true,
                    "policy_version": 9u64,
                    "reserve_allocation_bps": 6500u64,
                    "fee_allocation_bps": 2500u64,
                    "risk_buffer_allocation_bps": 1000u64,
                    "min_reserve_bucket_nov": 120u64,
                    "min_fee_bucket_nov": 80u64,
                    "min_risk_buffer_nov": 400u64,
                    "mapped_lock_bridge_paused": true,
                    "mapped_lock_min_confirmations": 18u64,
                    "mapped_asset_burn_paused": true,
                    "mapped_asset_release_paused": true,
                    "mapped_asset_reorg_response_policy": "freeze_and_rollback",
                    "clearing_constrained_max_slippage_bps": 25u64,
                    "clearing_constrained_daily_usage_bps": 7500u64,
                    "clearing_constrained_strategy": "treasury_direct_only",
                    "fee_oracle_allowed_sources": ["runtime_oracle", "governance_oracle", "governance_oracle_v2"],
                    "fee_oracle_disabled_sources": ["governance_oracle"],
                    "fee_oracle_disabled_source_reasons": {"governance_oracle": "deviation_slash"},
                    "fee_oracle_source_rotations": {"governance_oracle": "governance_oracle_v2"}
                }))
                .expect("encode args"),
                fee_pay_asset: "NOV".to_string(),
                fee_max_pay_amount: 10_000,
                fee_slippage_bps: 50,
                gas_like_limit: Some(90_000),
                nonce: 17,
            };
            let receipt = with_env_override_v1(NOV_NATIVE_GOVERNANCE_ENABLED_ENV, "true", || {
                dispatch_and_persist_nov_execution_request_with_store_path_v1(
                    path.as_path(),
                    &request,
                )
                .expect("dispatch should apply governance policy")
            });
            assert!(receipt.status);
            assert_eq!(receipt.module, "governance");
            assert_eq!(receipt.method, "apply_treasury_policy");
            let aoem_meta = receipt
                .aoem_semantic_ingress
                .as_ref()
                .expect("governance policy receipt should expose AOEM semantic ingress");
            let aoem_commit = receipt
                .aoem_semantic_commit
                .as_ref()
                .expect("governance policy receipt should expose AOEM semantic commit");
            assert_eq!(aoem_commit.execution_kernel, "AOEM");
            assert_eq!(aoem_commit.subject, "governance");
            assert_eq!(aoem_commit.action, "apply_treasury_policy");
            assert_eq!(aoem_commit.tx_ref, receipt.tx_hash);
            assert_eq!(aoem_commit.sequence, aoem_meta.semantic_ledger_sequence);
            assert_eq!(
                aoem_commit.commit_seal,
                aoem_meta.semantic_ledger_commit_seal
            );
            assert_eq!(aoem_commit.semantic_delta_count, 1);
            let receipt_policy_meta = receipt
                .policy_meta
                .as_ref()
                .expect("receipt policy_meta should be present");
            assert_eq!(receipt_policy_meta.policy_source, "config_path");
            assert_eq!(receipt_policy_meta.policy_version, 1);
            assert_eq!(receipt_policy_meta.policy_threshold_state, "healthy");
            assert_eq!(
                receipt_policy_meta.policy_constrained_strategy,
                "daily_volume_only"
            );

            let out = run_nov_native_call_from_params_with_store_path_v1(
                &serde_json::json!({
                    "target": {"kind": "native_module", "id": "treasury"},
                    "method": "get_settlement_policy",
                    "args": {}
                }),
                Some(path.as_path()),
            )
            .expect("get_settlement_policy should succeed");
            assert_eq!(out["result"]["policy_version"].as_u64(), Some(9));
            assert_eq!(
                out["result"]["policy_source"].as_str(),
                Some("governance_path")
            );
            assert_eq!(
                out["result"]["policy"]["reserve_share_bps"].as_u64(),
                Some(6500)
            );
            assert_eq!(
                out["result"]["policy"]["clearing_constrained_max_slippage_bps"].as_u64(),
                Some(25)
            );
            assert_eq!(
                out["result"]["policy"]["clearing_constrained_daily_usage_bps"].as_u64(),
                Some(7500)
            );
            assert_eq!(
                out["result"]["policy"]["clearing_constrained_strategy"].as_str(),
                Some("treasury_direct_only")
            );
            assert_eq!(
                out["result"]["policy"]["mapped_lock_bridge_paused"].as_bool(),
                Some(true)
            );
            assert_eq!(
                out["result"]["policy"]["mapped_lock_min_confirmations"].as_u64(),
                Some(18)
            );
            assert_eq!(
                out["result"]["policy"]["mapped_asset_burn_paused"].as_bool(),
                Some(true)
            );
            assert_eq!(
                out["result"]["policy"]["mapped_asset_release_paused"].as_bool(),
                Some(true)
            );
            assert_eq!(
                out["result"]["policy"]["mapped_asset_auto_heal_enabled"].as_bool(),
                Some(true)
            );
            assert_eq!(
                out["result"]["policy"]["mapped_asset_auto_heal_rollback_enabled"].as_bool(),
                Some(true)
            );
            assert_eq!(
                out["result"]["policy"]["mapped_asset_reorg_response_policy"].as_str(),
                Some("freeze_and_rollback")
            );
            let oracle_sources = out["result"]["oracle_policy"]["oracle_allowed_sources"]
                .as_array()
                .expect("oracle allowed source list should be present");
            assert!(oracle_sources
                .iter()
                .any(|source| source.as_str() == Some("runtime_oracle")));
            assert!(oracle_sources
                .iter()
                .any(|source| source.as_str() == Some("governance_oracle")));
            assert_eq!(
                out["result"]["oracle_policy"]["oracle_open_feed_allowed"].as_bool(),
                Some(false)
            );
            assert!(out["result"]["oracle_policy"]["oracle_disabled_sources"]
                .as_array()
                .expect("oracle disabled source list should be present")
                .iter()
                .any(|source| source.as_str() == Some("governance_oracle")));
            assert_eq!(
                out["result"]["oracle_policy"]["oracle_disabled_source_reasons"]
                    ["governance_oracle"]
                    .as_str(),
                Some("deviation_slash")
            );
            assert_eq!(
                out["result"]["oracle_policy"]["oracle_source_rotations"]["governance_oracle"]
                    .as_str(),
                Some("governance_oracle_v2")
            );
            let oracle = run_nov_native_call_from_params_with_store_path_v1(
                &serde_json::json!({
                    "target": {"kind": "native_module", "id": "treasury"},
                    "method": "get_fee_oracle_rates",
                    "args": {}
                }),
                Some(path.as_path()),
            )
            .expect("get_fee_oracle_rates should succeed");
            assert_eq!(
                oracle["result"]["oracle_source"].as_str(),
                Some("runtime_oracle")
            );
            assert_eq!(
                oracle["result"]["oracle_source_allowed"].as_bool(),
                Some(true)
            );
            assert_eq!(
                oracle["result"]["oracle_open_feed_allowed"].as_bool(),
                Some(false)
            );
            assert!(oracle["result"]["oracle_allowed_sources"]
                .as_array()
                .expect("oracle query should expose allowed sources")
                .iter()
                .any(|source| source.as_str() == Some("governance_oracle")));
            assert_eq!(
                oracle["result"]["oracle_disabled_source_reasons"]["governance_oracle"].as_str(),
                Some("deviation_slash")
            );
            assert_eq!(
                oracle["result"]["oracle_source_rotations"]["governance_oracle"].as_str(),
                Some("governance_oracle_v2")
            );
            let policy_contract_id = out["result"]["policy_contract_id"]
                .as_str()
                .unwrap_or_default()
                .to_string();
            assert!(
                !policy_contract_id.trim().is_empty(),
                "policy_contract_id must be present in policy query"
            );

            let followup = NovExecutionRequestV1 {
                tx_hash: [0x7d; 32],
                chain_id: 7011,
                caller: vec![0xdd; 20],
                target: NovExecutionRequestTargetV1::NativeModule("treasury".to_string()),
                method: "deposit_reserve".to_string(),
                args: serde_json::to_vec(&serde_json::json!({
                    "asset": "NOV",
                    "amount": 1u64
                }))
                .expect("encode followup args"),
                fee_pay_asset: "NOV".to_string(),
                fee_max_pay_amount: 10_000,
                fee_slippage_bps: 50,
                gas_like_limit: Some(90_000),
                nonce: 19,
            };
            let followup_receipt = dispatch_and_persist_nov_execution_request_with_store_path_v1(
                path.as_path(),
                &followup,
            )
            .expect("followup settlement should succeed");
            assert!(followup_receipt.status);
            let followup_policy_meta = followup_receipt
                .policy_meta
                .as_ref()
                .expect("followup receipt policy_meta should be present");
            assert_eq!(followup_policy_meta.policy_source, "governance_path");
            assert_eq!(followup_policy_meta.policy_version, 9);
            assert_eq!(
                followup_policy_meta.policy_constrained_strategy,
                "treasury_direct_only"
            );
            assert_eq!(followup_policy_meta.policy_contract_id, policy_contract_id);

            let summary = run_nov_native_call_from_params_with_store_path_v1(
                &serde_json::json!({
                    "target": {"kind": "native_module", "id": "treasury"},
                    "method": "get_settlement_summary",
                    "args": {}
                }),
                Some(path.as_path()),
            )
            .expect("get_settlement_summary should succeed");
            assert_eq!(summary["result"]["policy_version"].as_u64(), Some(9));
            assert_eq!(
                summary["result"]["policy_source"].as_str(),
                Some("governance_path")
            );
            assert_eq!(
                summary["result"]["policy_contract_id"].as_str(),
                Some(policy_contract_id.as_str())
            );
            assert_eq!(
                summary["result"]["policy_context"]["policy_contract_id"].as_str(),
                Some(policy_contract_id.as_str())
            );
            assert_eq!(
                summary["result"]["policy_context"]["policy_source"].as_str(),
                Some("governance_path")
            );
            assert_eq!(
                summary["result"]["policy_context"]["policy_threshold_state"].as_str(),
                summary["result"]["current_threshold_state"].as_str()
            );
            assert_eq!(
                Some(followup_policy_meta.policy_threshold_state.as_str()),
                summary["result"]["current_threshold_state"].as_str()
            );

            let journal = run_nov_native_call_from_params_with_store_path_v1(
                &serde_json::json!({
                    "target": {"kind": "native_module", "id": "treasury"},
                    "method": "get_settlement_journal",
                    "args": {"limit": 1}
                }),
                Some(path.as_path()),
            )
            .expect("get_settlement_journal should succeed");
            assert_eq!(
                journal["result"]["entries"][0]["policy_version"].as_u64(),
                Some(9)
            );
            assert_eq!(
                journal["result"]["entries"][0]["policy_source"].as_str(),
                Some("governance_path")
            );
            assert_eq!(
                journal["result"]["policy_contract_id"].as_str(),
                Some(policy_contract_id.as_str())
            );
            assert_eq!(
                journal["result"]["entries"][0]["policy_contract_id"].as_str(),
                Some(policy_contract_id.as_str())
            );
            assert_eq!(
                journal["result"]["policy_context"]["policy_contract_id"].as_str(),
                Some(policy_contract_id.as_str())
            );
            assert_eq!(
                journal["result"]["entries"][0]["policy_constrained_strategy"].as_str(),
                Some("treasury_direct_only")
            );
            assert_eq!(
                journal["result"]["entries"][0]["policy_event_state"].as_str(),
                Some("settled")
            );

            let risk = run_nov_native_call_from_params_with_store_path_v1(
                &serde_json::json!({
                    "target": {"kind": "native_module", "id": "treasury"},
                    "method": "get_clearing_risk_summary",
                    "args": {}
                }),
                Some(path.as_path()),
            )
            .expect("get_clearing_risk_summary should succeed");
            assert_eq!(risk["result"]["policy_version"].as_u64(), Some(9));
            assert_eq!(
                risk["result"]["policy_source"].as_str(),
                Some("governance_path")
            );
            assert_eq!(
                risk["result"]["policy"]["clearing_constrained_strategy"].as_str(),
                Some("treasury_direct_only")
            );
            assert_eq!(
                risk["result"]["policy_contract_id"].as_str(),
                Some(policy_contract_id.as_str())
            );
            assert_eq!(
                risk["result"]["last_selected_route_policy_context"]["policy_contract_id"].as_str(),
                Some(policy_contract_id.as_str())
            );
        });
    }

    #[test]
    fn governance_set_reserve_proof_exposes_manual_status_without_mint_claims() {
        with_test_native_execution_store_path_v1(|path| {
            let request = NovExecutionRequestV1 {
                tx_hash: [0x91; 32],
                chain_id: 7011,
                caller: vec![0x91; 20],
                target: NovExecutionRequestTargetV1::NativeModule("governance".to_string()),
                method: "set_reserve_proof".to_string(),
                args: serde_json::to_vec(&serde_json::json!({
                    "governance_authorized": true,
                    "asset": "NETH",
                    "reserve_amount": 1_000u64,
                    "proof_type": "custody_statement_v1",
                    "proof_digest": "0xabc123",
                    "proof_source": "treasury_committee",
                    "proof_reference": "reserve-report-001",
                    "observed_at_unix_ms": 700_000u64,
                    "expires_at_unix_ms": 0u64,
                    "policy_version": 11u64,
                    "status": "active"
                }))
                .expect("encode reserve proof args"),
                fee_pay_asset: "NOV".to_string(),
                fee_max_pay_amount: 10_000,
                fee_slippage_bps: 50,
                gas_like_limit: Some(90_000),
                nonce: 21,
            };
            let receipt = with_env_override_v1(NOV_NATIVE_GOVERNANCE_ENABLED_ENV, "true", || {
                dispatch_and_persist_nov_execution_request_with_store_path_v1(
                    path.as_path(),
                    &request,
                )
                .expect("dispatch should set reserve proof")
            });
            assert!(receipt.status);
            assert_eq!(receipt.module, "governance");
            assert_eq!(receipt.method, "set_reserve_proof");
            let aoem_meta = receipt
                .aoem_semantic_ingress
                .as_ref()
                .expect("reserve proof receipt should expose AOEM semantic ingress");
            let aoem_commit = receipt
                .aoem_semantic_commit
                .as_ref()
                .expect("reserve proof receipt should expose AOEM semantic commit");
            assert_eq!(aoem_commit.execution_kernel, "AOEM");
            assert_eq!(aoem_commit.subject, "governance");
            assert_eq!(aoem_commit.action, "set_reserve_proof");
            assert_eq!(aoem_commit.tx_ref, receipt.tx_hash);
            assert_eq!(aoem_commit.sequence, aoem_meta.semantic_ledger_sequence);
            assert_eq!(
                aoem_commit.commit_seal,
                aoem_meta.semantic_ledger_commit_seal
            );
            assert_eq!(aoem_commit.semantic_delta_count, 1);
            assert_eq!(
                receipt.logs[0].data["claims"]["nov_mint_authorized"].as_bool(),
                Some(false)
            );
            assert_eq!(
                receipt.logs[0].data["automated_verification"].as_bool(),
                Some(false)
            );

            let proof = run_nov_native_call_from_params_with_store_path_v1(
                &serde_json::json!({
                    "target": {"kind": "native_module", "id": "treasury"},
                    "method": "get_reserve_proof",
                    "args": {"asset": "NETH"}
                }),
                Some(path.as_path()),
            )
            .expect("get_reserve_proof should succeed");
            assert_eq!(proof["found"].as_bool(), Some(true));
            assert_eq!(
                proof["result"]["reserve_proof"]["effective_status"].as_str(),
                Some("active")
            );
            assert_eq!(
                proof["result"]["reserve_proof"]["proof"]["reserve_amount"].as_u64(),
                Some(1_000)
            );
            assert_eq!(
                proof["result"]["reserve_proof"]["proof"]["automated_verification"].as_bool(),
                Some(false)
            );
            assert_eq!(
                proof["result"]["reserve_proof"]["claims"]["external_redemption_authorized"]
                    .as_bool(),
                Some(false)
            );
            assert_eq!(
                proof["result"]["automated_external_verification_complete"].as_bool(),
                Some(false)
            );

            let snapshot = run_nov_native_call_from_params_with_store_path_v1(
                &serde_json::json!({
                    "target": {"kind": "native_module", "id": "treasury"},
                    "method": "get_reserve_snapshot",
                    "args": {}
                }),
                Some(path.as_path()),
            )
            .expect("get_reserve_snapshot should succeed");
            assert_eq!(
                snapshot["result"]["reserve_proofs"]["NETH"]["proof"]["proof_reference"].as_str(),
                Some("reserve-report-001")
            );
        });
    }

    #[test]
    fn reserve_proof_revoked_blocks_non_nov_fee_clearing() {
        with_test_native_execution_store_path_v1(|path| {
            let mut pre = NovNativeExecutionStoreV1::default();
            pre.module_state
                .fee_oracle_rates_ppm
                .insert("USDT".to_string(), 2_000_000);
            pre.module_state.fee_oracle_updated_unix_ms = now_unix_millis_v1();
            pre.module_state.fee_oracle_source = "runtime_oracle".to_string();
            pre.module_state.clearing_enabled = true;
            pre.module_state.clearing_require_healthy_risk_buffer = false;
            pre.module_state.treasury_reserve_proofs.insert(
                "USDT".to_string(),
                NovTreasuryReserveProofV1 {
                    asset: "USDT".to_string(),
                    reserve_amount: 1_000,
                    proof_type: "custody_statement_v1".to_string(),
                    proof_digest: "0xdeadbeef".to_string(),
                    proof_source: "treasury_committee".to_string(),
                    proof_reference: "revoked-report-001".to_string(),
                    observed_at_unix_ms: 1,
                    expires_at_unix_ms: 0,
                    policy_version: 1,
                    policy_source: "governance_path".to_string(),
                    status: "revoked".to_string(),
                    automated_verification: false,
                    verification_mode: "manual_governance_attestation".to_string(),
                },
            );
            save_nov_native_execution_store_v1(path.as_path(), &pre)
                .expect("seed revoked reserve proof");

            let request = NovExecutionRequestV1 {
                tx_hash: [0x91; 32],
                chain_id: 7091,
                caller: vec![0x91; 20],
                target: NovExecutionRequestTargetV1::NativeModule("treasury".to_string()),
                method: "deposit_reserve".to_string(),
                args: serde_json::to_vec(&serde_json::json!({
                    "asset": "USDT",
                    "amount": 10u64
                }))
                .expect("encode args"),
                fee_pay_asset: "USDT".to_string(),
                fee_max_pay_amount: 1_000,
                fee_slippage_bps: 50,
                gas_like_limit: Some(90_000),
                nonce: 91,
            };
            let receipt = dispatch_and_persist_nov_execution_request_with_store_path_v1(
                path.as_path(),
                &request,
            )
            .expect("dispatch should return reserve proof failure receipt");
            assert!(!receipt.status);
            assert_eq!(receipt.module, "fee");
            assert_eq!(receipt.method, "settlement");
            let failure = receipt.failure_reason.clone().unwrap_or_default();
            assert!(failure.starts_with("fee.clearing.reserve_proof_not_active"));
            assert!(failure.contains("reserve_proof_effective_status=revoked"));
            assert!(failure.contains("proof_reference=revoked-report-001"));

            let state = load_nov_native_execution_store_v1(path.as_path())
                .expect("load native execution store");
            assert_eq!(
                state
                    .module_state
                    .treasury_reserves
                    .get("USDT")
                    .copied()
                    .unwrap_or_default(),
                0
            );
            assert_eq!(
                state
                    .module_state
                    .clearing_failure_counts
                    .get("USDT:reserve_proof_not_active")
                    .copied(),
                Some(1)
            );
        });
    }

    #[test]
    fn neth_m2_bridge_risk_blocks_fee_clearing() {
        with_test_native_execution_store_path_v1(|path| {
            let caller = vec![0x95; 20];
            let caller_hex = format!("0x{}", to_hex(caller.as_slice()));
            let mut pre = NovNativeExecutionStoreV1::default();
            pre.module_state
                .fee_oracle_rates_ppm
                .insert("NETH".to_string(), 2_000_000);
            pre.module_state.fee_oracle_updated_unix_ms = now_unix_millis_v1();
            pre.module_state.fee_oracle_source = "runtime_oracle".to_string();
            pre.module_state
                .protocol_clearing_nav_rate_ppm
                .insert("NETH".to_string(), 2_000_000);
            pre.module_state.clearing_enabled = true;
            pre.module_state.clearing_require_healthy_risk_buffer = false;
            pre.module_state
                .treasury_reserves
                .insert("NETH".to_string(), 1_000);
            pre.module_state.account_asset_balances.insert(
                caller_hex,
                BTreeMap::from([("NETH".to_string(), 1_200u128)]),
            );
            pre.module_state.treasury_reserve_proofs.insert(
                "NETH".to_string(),
                NovTreasuryReserveProofV1 {
                    asset: "NETH".to_string(),
                    reserve_amount: 900,
                    proof_type: "custody_statement_v1".to_string(),
                    proof_digest: "0xnethfeeclosingrisk01".to_string(),
                    proof_source: "treasury_committee".to_string(),
                    proof_reference: "neth-fee-risk-report-001".to_string(),
                    observed_at_unix_ms: 1,
                    expires_at_unix_ms: 0,
                    policy_version: 1,
                    policy_source: "governance_path".to_string(),
                    status: "active".to_string(),
                    automated_verification: false,
                    verification_mode: "manual_governance_attestation".to_string(),
                },
            );
            pre.module_state.mapped_lock_contract_address = format!("0x{}", "95".repeat(20));
            pre.module_state.mapped_lock_min_confirmations = 21;
            save_nov_native_execution_store_v1(path.as_path(), &pre)
                .expect("seed uncovered NETH M2 fee clearing risk");

            let request = NovExecutionRequestV1 {
                tx_hash: [0x95; 32],
                chain_id: 7095,
                caller,
                target: NovExecutionRequestTargetV1::NativeModule("treasury".to_string()),
                method: "deposit_reserve".to_string(),
                args: serde_json::to_vec(&serde_json::json!({
                    "asset": "NETH",
                    "amount": 10u64
                }))
                .expect("encode args"),
                fee_pay_asset: "NETH".to_string(),
                fee_max_pay_amount: 1_000,
                fee_slippage_bps: 50,
                gas_like_limit: Some(90_000),
                nonce: 95,
            };
            let receipt = dispatch_and_persist_nov_execution_request_with_store_path_v1(
                path.as_path(),
                &request,
            )
            .expect("dispatch should return NETH M2 risk failure receipt");
            assert!(!receipt.status);
            assert_eq!(receipt.module, "fee");
            assert_eq!(receipt.method, "settlement");
            let failure = receipt.failure_reason.clone().unwrap_or_default();
            assert!(failure.starts_with("fee.clearing.reserve_proof_not_active"));
            assert!(failure.contains("m2_bridge_risk=m2_liability_exceeds_treasury_reserve"));
            assert!(failure.contains("liability=1200"));
            assert!(failure.contains("treasury_reserve=1000"));
        });
    }

    #[test]
    fn neth_missing_reserve_proof_blocks_fee_clearing() {
        with_test_native_execution_store_path_v1(|path| {
            let caller = vec![0x96; 20];
            let caller_hex = format!("0x{}", to_hex(caller.as_slice()));
            let mut pre = NovNativeExecutionStoreV1::default();
            pre.module_state
                .fee_oracle_rates_ppm
                .insert("NETH".to_string(), 2_000_000);
            pre.module_state.fee_oracle_updated_unix_ms = now_unix_millis_v1();
            pre.module_state.fee_oracle_source = "runtime_oracle".to_string();
            pre.module_state
                .protocol_clearing_nav_rate_ppm
                .insert("NETH".to_string(), 2_000_000);
            pre.module_state.clearing_enabled = true;
            pre.module_state.clearing_require_healthy_risk_buffer = false;
            pre.module_state
                .treasury_reserves
                .insert("NETH".to_string(), 1_000);
            pre.module_state
                .account_asset_balances
                .insert(caller_hex, BTreeMap::from([("NETH".to_string(), 100u128)]));
            pre.module_state.mapped_lock_contract_address = format!("0x{}", "96".repeat(20));
            pre.module_state.mapped_lock_min_confirmations = 21;
            save_nov_native_execution_store_v1(path.as_path(), &pre)
                .expect("seed missing NETH reserve proof fee clearing risk");

            let request = NovExecutionRequestV1 {
                tx_hash: [0x96; 32],
                chain_id: 7096,
                caller,
                target: NovExecutionRequestTargetV1::NativeModule("treasury".to_string()),
                method: "deposit_reserve".to_string(),
                args: serde_json::to_vec(&serde_json::json!({
                    "asset": "NETH",
                    "amount": 10u64
                }))
                .expect("encode args"),
                fee_pay_asset: "NETH".to_string(),
                fee_max_pay_amount: 1_000,
                fee_slippage_bps: 50,
                gas_like_limit: Some(90_000),
                nonce: 96,
            };
            let receipt = dispatch_and_persist_nov_execution_request_with_store_path_v1(
                path.as_path(),
                &request,
            )
            .expect("dispatch should return missing proof NETH M2 risk failure receipt");
            assert!(!receipt.status);
            assert_eq!(receipt.module, "fee");
            assert_eq!(receipt.method, "settlement");
            let failure = receipt.failure_reason.clone().unwrap_or_default();
            assert!(failure.starts_with("fee.clearing.reserve_proof_not_active"));
            assert!(failure.contains("m2_bridge_risk=reserve_proof_missing"));
        });
    }

    #[test]
    fn neth_unset_lock_policy_blocks_fee_clearing() {
        with_test_native_execution_store_path_v1(|path| {
            let caller = vec![0x97; 20];
            let caller_hex = format!("0x{}", to_hex(caller.as_slice()));
            let mut pre = NovNativeExecutionStoreV1::default();
            pre.module_state
                .fee_oracle_rates_ppm
                .insert("NETH".to_string(), 2_000_000);
            pre.module_state.fee_oracle_updated_unix_ms = now_unix_millis_v1();
            pre.module_state.fee_oracle_source = "runtime_oracle".to_string();
            pre.module_state
                .protocol_clearing_nav_rate_ppm
                .insert("NETH".to_string(), 2_000_000);
            pre.module_state.clearing_enabled = true;
            pre.module_state.clearing_require_healthy_risk_buffer = false;
            pre.module_state
                .treasury_reserves
                .insert("NETH".to_string(), 1_000);
            pre.module_state
                .account_asset_balances
                .insert(caller_hex, BTreeMap::from([("NETH".to_string(), 100u128)]));
            pre.module_state.treasury_reserve_proofs.insert(
                "NETH".to_string(),
                NovTreasuryReserveProofV1 {
                    asset: "NETH".to_string(),
                    reserve_amount: 1_000,
                    proof_type: "custody_statement_v1".to_string(),
                    proof_digest: "0xnethunsetpolicyfee01".to_string(),
                    proof_source: "treasury_committee".to_string(),
                    proof_reference: "neth-unset-policy-fee-001".to_string(),
                    observed_at_unix_ms: 1,
                    expires_at_unix_ms: 0,
                    policy_version: 1,
                    policy_source: "governance_path".to_string(),
                    status: "active".to_string(),
                    automated_verification: false,
                    verification_mode: "manual_governance_attestation".to_string(),
                },
            );
            pre.module_state.mapped_lock_min_confirmations = 21;
            save_nov_native_execution_store_v1(path.as_path(), &pre)
                .expect("seed unset NETH lock policy fee clearing risk");

            let request = NovExecutionRequestV1 {
                tx_hash: [0x97; 32],
                chain_id: 7097,
                caller,
                target: NovExecutionRequestTargetV1::NativeModule("treasury".to_string()),
                method: "deposit_reserve".to_string(),
                args: serde_json::to_vec(&serde_json::json!({
                    "asset": "NETH",
                    "amount": 10u64
                }))
                .expect("encode args"),
                fee_pay_asset: "NETH".to_string(),
                fee_max_pay_amount: 1_000,
                fee_slippage_bps: 50,
                gas_like_limit: Some(90_000),
                nonce: 97,
            };
            let receipt = dispatch_and_persist_nov_execution_request_with_store_path_v1(
                path.as_path(),
                &request,
            )
            .expect("dispatch should return unset lock policy NETH M2 risk failure receipt");
            assert!(!receipt.status);
            assert_eq!(receipt.module, "fee");
            assert_eq!(receipt.method, "settlement");
            let failure = receipt.failure_reason.clone().unwrap_or_default();
            assert!(failure.starts_with("fee.clearing.reserve_proof_not_active"));
            assert!(failure.contains("m2_bridge_risk=mapped_lock_contract_address_unset"));
        });
    }

    #[test]
    fn neth_unset_min_confirmations_blocks_fee_clearing() {
        with_test_native_execution_store_path_v1(|path| {
            let caller = vec![0x98; 20];
            let caller_hex = format!("0x{}", to_hex(caller.as_slice()));
            let mut pre = NovNativeExecutionStoreV1::default();
            pre.module_state
                .fee_oracle_rates_ppm
                .insert("NETH".to_string(), 2_000_000);
            pre.module_state.fee_oracle_updated_unix_ms = now_unix_millis_v1();
            pre.module_state.fee_oracle_source = "runtime_oracle".to_string();
            pre.module_state
                .protocol_clearing_nav_rate_ppm
                .insert("NETH".to_string(), 2_000_000);
            pre.module_state.clearing_enabled = true;
            pre.module_state.clearing_require_healthy_risk_buffer = false;
            pre.module_state
                .treasury_reserves
                .insert("NETH".to_string(), 1_000);
            pre.module_state
                .account_asset_balances
                .insert(caller_hex, BTreeMap::from([("NETH".to_string(), 100u128)]));
            pre.module_state.treasury_reserve_proofs.insert(
                "NETH".to_string(),
                NovTreasuryReserveProofV1 {
                    asset: "NETH".to_string(),
                    reserve_amount: 1_000,
                    proof_type: "custody_statement_v1".to_string(),
                    proof_digest: "0xnethunsetconfirmationsfee01".to_string(),
                    proof_source: "treasury_committee".to_string(),
                    proof_reference: "neth-unset-confirmations-fee-001".to_string(),
                    observed_at_unix_ms: 1,
                    expires_at_unix_ms: 0,
                    policy_version: 1,
                    policy_source: "governance_path".to_string(),
                    status: "active".to_string(),
                    automated_verification: false,
                    verification_mode: "manual_governance_attestation".to_string(),
                },
            );
            pre.module_state.mapped_lock_contract_address = format!("0x{}", "98".repeat(20));
            save_nov_native_execution_store_v1(path.as_path(), &pre)
                .expect("seed unset NETH min confirmations fee clearing risk");

            let request = NovExecutionRequestV1 {
                tx_hash: [0x98; 32],
                chain_id: 7098,
                caller,
                target: NovExecutionRequestTargetV1::NativeModule("treasury".to_string()),
                method: "deposit_reserve".to_string(),
                args: serde_json::to_vec(&serde_json::json!({
                    "asset": "NETH",
                    "amount": 10u64
                }))
                .expect("encode args"),
                fee_pay_asset: "NETH".to_string(),
                fee_max_pay_amount: 1_000,
                fee_slippage_bps: 50,
                gas_like_limit: Some(90_000),
                nonce: 98,
            };
            let receipt = dispatch_and_persist_nov_execution_request_with_store_path_v1(
                path.as_path(),
                &request,
            )
            .expect("dispatch should return unset min confirmations NETH M2 risk failure receipt");
            assert!(!receipt.status);
            assert_eq!(receipt.module, "fee");
            assert_eq!(receipt.method, "settlement");
            let failure = receipt.failure_reason.clone().unwrap_or_default();
            assert!(failure.starts_with("fee.clearing.reserve_proof_not_active"));
            assert!(failure.contains("m2_bridge_risk=mapped_lock_min_confirmations_unset"));
        });
    }

    #[test]
    fn neth_bridge_pause_blocks_fee_clearing() {
        with_test_native_execution_store_path_v1(|path| {
            let caller = vec![0x99; 20];
            let caller_hex = format!("0x{}", to_hex(caller.as_slice()));
            let mut pre = NovNativeExecutionStoreV1::default();
            pre.module_state
                .fee_oracle_rates_ppm
                .insert("NETH".to_string(), 2_000_000);
            pre.module_state.fee_oracle_updated_unix_ms = now_unix_millis_v1();
            pre.module_state.fee_oracle_source = "runtime_oracle".to_string();
            pre.module_state
                .protocol_clearing_nav_rate_ppm
                .insert("NETH".to_string(), 2_000_000);
            pre.module_state.clearing_enabled = true;
            pre.module_state.clearing_require_healthy_risk_buffer = false;
            pre.module_state
                .treasury_reserves
                .insert("NETH".to_string(), 1_000);
            pre.module_state
                .account_asset_balances
                .insert(caller_hex, BTreeMap::from([("NETH".to_string(), 100u128)]));
            pre.module_state.treasury_reserve_proofs.insert(
                "NETH".to_string(),
                NovTreasuryReserveProofV1 {
                    asset: "NETH".to_string(),
                    reserve_amount: 1_000,
                    proof_type: "custody_statement_v1".to_string(),
                    proof_digest: "0xnethbridgepausefee01".to_string(),
                    proof_source: "treasury_committee".to_string(),
                    proof_reference: "neth-bridge-pause-fee-001".to_string(),
                    observed_at_unix_ms: 1,
                    expires_at_unix_ms: 0,
                    policy_version: 1,
                    policy_source: "governance_path".to_string(),
                    status: "active".to_string(),
                    automated_verification: false,
                    verification_mode: "manual_governance_attestation".to_string(),
                },
            );
            pre.module_state.mapped_lock_contract_address = format!("0x{}", "99".repeat(20));
            pre.module_state.mapped_lock_min_confirmations = 21;
            pre.module_state.mapped_lock_bridge_paused = true;
            save_nov_native_execution_store_v1(path.as_path(), &pre)
                .expect("seed paused NETH bridge fee clearing risk");

            let request = NovExecutionRequestV1 {
                tx_hash: [0x99; 32],
                chain_id: 7099,
                caller,
                target: NovExecutionRequestTargetV1::NativeModule("treasury".to_string()),
                method: "deposit_reserve".to_string(),
                args: serde_json::to_vec(&serde_json::json!({
                    "asset": "NETH",
                    "amount": 10u64
                }))
                .expect("encode args"),
                fee_pay_asset: "NETH".to_string(),
                fee_max_pay_amount: 1_000,
                fee_slippage_bps: 50,
                gas_like_limit: Some(90_000),
                nonce: 99,
            };
            let receipt = dispatch_and_persist_nov_execution_request_with_store_path_v1(
                path.as_path(),
                &request,
            )
            .expect("dispatch should return paused bridge NETH M2 risk failure receipt");
            assert!(!receipt.status);
            assert_eq!(receipt.module, "fee");
            assert_eq!(receipt.method, "settlement");
            let failure = receipt.failure_reason.clone().unwrap_or_default();
            assert!(failure.starts_with("fee.clearing.reserve_proof_not_active"));
            assert!(failure.contains("m2_bridge_risk=mapped_lock_bridge_paused"));
        });
    }

    #[test]
    fn neth_burn_pause_blocks_fee_clearing() {
        with_test_native_execution_store_path_v1(|path| {
            let caller = vec![0x9b; 20];
            let caller_hex = format!("0x{}", to_hex(caller.as_slice()));
            let mut pre = NovNativeExecutionStoreV1::default();
            pre.module_state
                .fee_oracle_rates_ppm
                .insert("NETH".to_string(), 2_000_000);
            pre.module_state.fee_oracle_updated_unix_ms = now_unix_millis_v1();
            pre.module_state.fee_oracle_source = "runtime_oracle".to_string();
            pre.module_state
                .protocol_clearing_nav_rate_ppm
                .insert("NETH".to_string(), 2_000_000);
            pre.module_state.clearing_enabled = true;
            pre.module_state.clearing_require_healthy_risk_buffer = false;
            pre.module_state
                .treasury_reserves
                .insert("NETH".to_string(), 1_000);
            pre.module_state
                .account_asset_balances
                .insert(caller_hex, BTreeMap::from([("NETH".to_string(), 100u128)]));
            pre.module_state.treasury_reserve_proofs.insert(
                "NETH".to_string(),
                NovTreasuryReserveProofV1 {
                    asset: "NETH".to_string(),
                    reserve_amount: 1_000,
                    proof_type: "custody_statement_v1".to_string(),
                    proof_digest: "0xnethburnpausefee01".to_string(),
                    proof_source: "treasury_committee".to_string(),
                    proof_reference: "neth-burn-pause-fee-001".to_string(),
                    observed_at_unix_ms: 1,
                    expires_at_unix_ms: 0,
                    policy_version: 1,
                    policy_source: "governance_path".to_string(),
                    status: "active".to_string(),
                    automated_verification: false,
                    verification_mode: "manual_governance_attestation".to_string(),
                },
            );
            pre.module_state.mapped_lock_contract_address = format!("0x{}", "9b".repeat(20));
            pre.module_state.mapped_lock_min_confirmations = 21;
            pre.module_state.mapped_asset_burn_paused = true;
            save_nov_native_execution_store_v1(path.as_path(), &pre)
                .expect("seed paused NETH burn fee clearing risk");

            let request = NovExecutionRequestV1 {
                tx_hash: [0x9b; 32],
                chain_id: 7100,
                caller,
                target: NovExecutionRequestTargetV1::NativeModule("treasury".to_string()),
                method: "deposit_reserve".to_string(),
                args: serde_json::to_vec(&serde_json::json!({
                    "asset": "NETH",
                    "amount": 10u64
                }))
                .expect("encode args"),
                fee_pay_asset: "NETH".to_string(),
                fee_max_pay_amount: 1_000,
                fee_slippage_bps: 50,
                gas_like_limit: Some(90_000),
                nonce: 100,
            };
            let receipt = dispatch_and_persist_nov_execution_request_with_store_path_v1(
                path.as_path(),
                &request,
            )
            .expect("dispatch should return paused burn NETH M2 risk failure receipt");
            assert!(!receipt.status);
            assert_eq!(receipt.module, "fee");
            assert_eq!(receipt.method, "settlement");
            let failure = receipt.failure_reason.clone().unwrap_or_default();
            assert!(failure.starts_with("fee.clearing.reserve_proof_not_active"));
            assert!(failure.contains("m2_bridge_risk=mapped_asset_burn_paused"));
        });
    }

    #[test]
    fn reserve_proof_amount_cap_blocks_non_nov_fee_clearing_expansion() {
        with_test_native_execution_store_path_v1(|path| {
            let mut pre = NovNativeExecutionStoreV1::default();
            pre.module_state
                .fee_oracle_rates_ppm
                .insert("USDT".to_string(), 2_000_000);
            pre.module_state.fee_oracle_updated_unix_ms = now_unix_millis_v1();
            pre.module_state.fee_oracle_source = "runtime_oracle".to_string();
            pre.module_state.clearing_enabled = true;
            pre.module_state.clearing_require_healthy_risk_buffer = false;
            pre.module_state.treasury_reserve_proofs.insert(
                "USDT".to_string(),
                NovTreasuryReserveProofV1 {
                    asset: "USDT".to_string(),
                    reserve_amount: 1,
                    proof_type: "custody_statement_v1".to_string(),
                    proof_digest: "0xcap01".to_string(),
                    proof_source: "treasury_committee".to_string(),
                    proof_reference: "cap-report-001".to_string(),
                    observed_at_unix_ms: 1,
                    expires_at_unix_ms: 0,
                    policy_version: 1,
                    policy_source: "governance_path".to_string(),
                    status: "active".to_string(),
                    automated_verification: false,
                    verification_mode: "manual_governance_attestation".to_string(),
                },
            );
            save_nov_native_execution_store_v1(path.as_path(), &pre)
                .expect("seed low reserve proof cap");

            let request = NovExecutionRequestV1 {
                tx_hash: [0x93; 32],
                chain_id: 7093,
                caller: vec![0x93; 20],
                target: NovExecutionRequestTargetV1::NativeModule("treasury".to_string()),
                method: "deposit_reserve".to_string(),
                args: serde_json::to_vec(&serde_json::json!({
                    "asset": "USDT",
                    "amount": 10u64
                }))
                .expect("encode args"),
                fee_pay_asset: "USDT".to_string(),
                fee_max_pay_amount: 1_000,
                fee_slippage_bps: 50,
                gas_like_limit: Some(90_000),
                nonce: 93,
            };
            let receipt = dispatch_and_persist_nov_execution_request_with_store_path_v1(
                path.as_path(),
                &request,
            )
            .expect("dispatch should return reserve proof capacity failure receipt");
            assert!(!receipt.status);
            assert_eq!(receipt.module, "fee");
            assert_eq!(receipt.method, "settlement");
            let failure = receipt.failure_reason.clone().unwrap_or_default();
            assert!(failure.starts_with("fee.clearing.reserve_proof_capacity_exceeded"));
            assert!(failure.contains("proof_reserve_amount=1"));
            assert!(failure.contains("proof_reference=cap-report-001"));

            let state = load_nov_native_execution_store_v1(path.as_path())
                .expect("load native execution store");
            assert_eq!(
                state
                    .module_state
                    .treasury_reserves
                    .get("USDT")
                    .copied()
                    .unwrap_or_default(),
                0
            );
            assert_eq!(
                state
                    .module_state
                    .clearing_failure_counts
                    .get("USDT:reserve_proof_capacity_exceeded")
                    .copied(),
                Some(1)
            );
        });
    }

    #[test]
    fn constrained_threshold_state_tightens_non_nov_clearing_slippage() {
        with_test_native_execution_store_path_v1(|path| {
            let mut pre = NovNativeExecutionStoreV1::default();
            pre.module_state
                .fee_oracle_rates_ppm
                .insert("USDT".to_string(), 2_000_000);
            pre.module_state.fee_oracle_updated_unix_ms = now_unix_millis_v1();
            pre.module_state.fee_oracle_source = "runtime_oracle".to_string();
            pre.module_state.clearing_enabled = true;
            pre.module_state.clearing_require_healthy_risk_buffer = false;
            pre.module_state.clearing_constrained_strategy = "daily_volume_only".to_string();
            pre.module_state.clearing_constrained_max_slippage_bps = 10;
            pre.module_state.treasury_min_reserve_bucket_nov = 100;
            pre.module_state.treasury_reserve_bucket_nov = 0;
            save_nov_native_execution_store_v1(path.as_path(), &pre)
                .expect("seed constrained threshold state");

            let request = NovExecutionRequestV1 {
                tx_hash: [0x7c; 32],
                chain_id: 7012,
                caller: vec![0xef; 20],
                target: NovExecutionRequestTargetV1::NativeModule("treasury".to_string()),
                method: "deposit_reserve".to_string(),
                args: serde_json::to_vec(&serde_json::json!({
                    "asset": "USDT",
                    "amount": 2u64
                }))
                .expect("encode args"),
                fee_pay_asset: "USDT".to_string(),
                fee_max_pay_amount: 1_000,
                fee_slippage_bps: 50,
                gas_like_limit: Some(90_000),
                nonce: 18,
            };
            let receipt = dispatch_and_persist_nov_execution_request_with_store_path_v1(
                path.as_path(),
                &request,
            )
            .expect("dispatch should return constrained slippage receipt");
            assert!(!receipt.status);
            assert_eq!(receipt.module, "fee");
            assert_eq!(receipt.method, "settlement");
            assert!(receipt
                .failure_reason
                .clone()
                .unwrap_or_default()
                .starts_with("fee.clearing.slippage_exceeded"));

            let risk = run_nov_native_call_from_params_with_store_path_v1(
                &serde_json::json!({
                    "target": {"kind": "native_module", "id": "treasury"},
                    "method": "get_clearing_risk_summary",
                    "args": {}
                }),
                Some(path.as_path()),
            )
            .expect("get_clearing_risk_summary should succeed");
            assert_eq!(
                risk["result"]["current_threshold_state"].as_str(),
                Some("constrained")
            );
        });
    }

    #[test]
    fn healthy_threshold_state_allows_non_nov_clearing() {
        with_test_native_execution_store_path_v1(|path| {
            let mut pre = NovNativeExecutionStoreV1::default();
            pre.module_state
                .fee_oracle_rates_ppm
                .insert("USDT".to_string(), 2_000_000);
            pre.module_state.fee_oracle_updated_unix_ms = now_unix_millis_v1();
            pre.module_state.fee_oracle_source = "runtime_oracle".to_string();
            pre.module_state.clearing_enabled = true;
            pre.module_state.clearing_require_healthy_risk_buffer = false;
            pre.module_state.treasury_min_reserve_bucket_nov = 0;
            pre.module_state.treasury_min_fee_bucket_nov = 0;
            pre.module_state.treasury_min_risk_buffer_nov = 1;
            pre.module_state.treasury_risk_buffer_nov = 10;
            save_nov_native_execution_store_v1(path.as_path(), &pre)
                .expect("seed healthy threshold state");

            let request = NovExecutionRequestV1 {
                tx_hash: [0x7e; 32],
                chain_id: 7013,
                caller: vec![0xee; 20],
                target: NovExecutionRequestTargetV1::NativeModule("treasury".to_string()),
                method: "deposit_reserve".to_string(),
                args: serde_json::to_vec(&serde_json::json!({
                    "asset": "USDT",
                    "amount": 2u64
                }))
                .expect("encode args"),
                fee_pay_asset: "USDT".to_string(),
                fee_max_pay_amount: 1_000,
                fee_slippage_bps: 50,
                gas_like_limit: Some(90_000),
                nonce: 20,
            };
            let receipt = dispatch_and_persist_nov_execution_request_with_store_path_v1(
                path.as_path(),
                &request,
            )
            .expect("dispatch should succeed in healthy threshold state");
            assert!(receipt.status);
            assert_eq!(receipt.module, "treasury");
            assert_eq!(receipt.method, "deposit_reserve");

            let policy = run_nov_native_call_from_params_with_store_path_v1(
                &serde_json::json!({
                    "target": {"kind": "native_module", "id": "treasury"},
                    "method": "get_settlement_policy",
                    "args": {}
                }),
                Some(path.as_path()),
            )
            .expect("get_settlement_policy should succeed");
            assert_eq!(
                policy["result"]["policy_source"].as_str(),
                Some("config_path")
            );

            let risk = run_nov_native_call_from_params_with_store_path_v1(
                &serde_json::json!({
                    "target": {"kind": "native_module", "id": "treasury"},
                    "method": "get_clearing_risk_summary",
                    "args": {}
                }),
                Some(path.as_path()),
            )
            .expect("get_clearing_risk_summary should succeed");
            assert_eq!(
                risk["result"]["current_threshold_state"].as_str(),
                Some("healthy")
            );
            assert_eq!(
                risk["result"]["policy_source"].as_str(),
                Some("config_path")
            );
        });
    }

    #[test]
    fn config_path_policy_is_consistent_across_policy_summary_risk_and_journal() {
        with_test_native_execution_store_path_v1(|path| {
            let request = NovExecutionRequestV1 {
                tx_hash: [0x7f; 32],
                chain_id: 7014,
                caller: vec![0xaf; 20],
                target: NovExecutionRequestTargetV1::NativeModule("treasury".to_string()),
                method: "deposit_reserve".to_string(),
                args: serde_json::to_vec(&serde_json::json!({
                    "asset": "NOV",
                    "amount": 3u64
                }))
                .expect("encode args"),
                fee_pay_asset: "NOV".to_string(),
                fee_max_pay_amount: 10_000,
                fee_slippage_bps: 50,
                gas_like_limit: Some(90_000),
                nonce: 21,
            };
            let receipt = dispatch_and_persist_nov_execution_request_with_store_path_v1(
                path.as_path(),
                &request,
            )
            .expect("dispatch should succeed for config-path policy");
            assert!(receipt.status);
            let receipt_policy_meta = receipt
                .policy_meta
                .as_ref()
                .expect("config-path receipt policy_meta should be present");
            assert_eq!(receipt_policy_meta.policy_source, "config_path");
            assert_eq!(receipt_policy_meta.policy_version, 1);

            let policy = run_nov_native_call_from_params_with_store_path_v1(
                &serde_json::json!({
                    "target": {"kind": "native_module", "id": "treasury"},
                    "method": "get_settlement_policy",
                    "args": {}
                }),
                Some(path.as_path()),
            )
            .expect("get_settlement_policy should succeed");
            assert_eq!(policy["result"]["policy_version"].as_u64(), Some(1));
            assert_eq!(
                policy["result"]["policy_source"].as_str(),
                Some("config_path")
            );
            assert_eq!(
                policy["result"]["allocation_parameters"]["reserve_allocation_bps"].as_u64(),
                Some(7000)
            );
            assert_eq!(
                policy["result"]["policy"]["clearing_constrained_daily_usage_bps"].as_u64(),
                Some(8000)
            );
            assert_eq!(
                policy["result"]["policy"]["clearing_constrained_strategy"].as_str(),
                Some("daily_volume_only")
            );
            let policy_contract_id = policy["result"]["policy_contract_id"]
                .as_str()
                .unwrap_or_default()
                .to_string();
            assert!(
                !policy_contract_id.trim().is_empty(),
                "policy_contract_id must be present in policy query"
            );

            let summary = run_nov_native_call_from_params_with_store_path_v1(
                &serde_json::json!({
                    "target": {"kind": "native_module", "id": "treasury"},
                    "method": "get_settlement_summary",
                    "args": {}
                }),
                Some(path.as_path()),
            )
            .expect("get_settlement_summary should succeed");
            assert_eq!(summary["result"]["policy_version"].as_u64(), Some(1));
            assert_eq!(
                summary["result"]["policy_source"].as_str(),
                Some("config_path")
            );
            assert_eq!(
                summary["result"]["allocation_parameters"]["allocation_total_bps"].as_u64(),
                Some(10_000)
            );
            assert_eq!(
                summary["result"]["policy_contract_id"].as_str(),
                Some(policy_contract_id.as_str())
            );
            assert_eq!(
                summary["result"]["policy_context"]["policy_contract_id"].as_str(),
                Some(policy_contract_id.as_str())
            );
            assert_eq!(
                summary["result"]["policy_context"]["policy_source"].as_str(),
                Some("config_path")
            );
            assert_eq!(
                summary["result"]["policy_context"]["policy_threshold_state"].as_str(),
                summary["result"]["current_threshold_state"].as_str()
            );
            assert_eq!(
                Some(receipt_policy_meta.policy_threshold_state.as_str()),
                summary["result"]["current_threshold_state"].as_str()
            );
            assert_eq!(
                receipt_policy_meta.policy_constrained_strategy,
                "daily_volume_only"
            );

            let risk = run_nov_native_call_from_params_with_store_path_v1(
                &serde_json::json!({
                    "target": {"kind": "native_module", "id": "treasury"},
                    "method": "get_clearing_risk_summary",
                    "args": {}
                }),
                Some(path.as_path()),
            )
            .expect("get_clearing_risk_summary should succeed");
            assert_eq!(risk["result"]["policy_version"].as_u64(), Some(1));
            assert_eq!(
                risk["result"]["policy_source"].as_str(),
                Some("config_path")
            );
            assert_eq!(
                risk["result"]["allocation_parameters"]["risk_buffer_allocation_bps"].as_u64(),
                Some(1000)
            );
            assert_eq!(
                risk["result"]["policy"]["clearing_constrained_daily_usage_bps"].as_u64(),
                Some(8000)
            );
            assert_eq!(
                risk["result"]["policy"]["clearing_constrained_strategy"].as_str(),
                Some("daily_volume_only")
            );
            assert_eq!(
                risk["result"]["policy_contract_id"].as_str(),
                Some(policy_contract_id.as_str())
            );

            let journal = run_nov_native_call_from_params_with_store_path_v1(
                &serde_json::json!({
                    "target": {"kind": "native_module", "id": "treasury"},
                    "method": "get_settlement_journal",
                    "args": {"limit": 1}
                }),
                Some(path.as_path()),
            )
            .expect("get_settlement_journal should succeed");
            assert_eq!(
                journal["result"]["entries"][0]["policy_version"].as_u64(),
                Some(1)
            );
            assert_eq!(
                journal["result"]["entries"][0]["policy_source"].as_str(),
                Some("config_path")
            );
            assert_eq!(
                journal["result"]["policy_contract_id"].as_str(),
                Some(policy_contract_id.as_str())
            );
            assert_eq!(
                journal["result"]["entries"][0]["policy_contract_id"].as_str(),
                Some(policy_contract_id.as_str())
            );
            assert_eq!(
                journal["result"]["policy_context"]["policy_contract_id"].as_str(),
                Some(policy_contract_id.as_str())
            );
            assert_eq!(
                journal["result"]["entries"][0]["policy_constrained_strategy"].as_str(),
                Some("daily_volume_only")
            );
            assert_eq!(
                journal["result"]["entries"][0]["policy_event_state"].as_str(),
                Some("settled")
            );
            assert_eq!(
                journal["result"]["entries"][0]["policy_threshold_state"].as_str(),
                summary["result"]["current_threshold_state"].as_str()
            );
        });
    }

    #[test]
    fn constrained_threshold_state_restricts_clearing_to_treasury_direct() {
        with_test_native_execution_store_path_v1(|path| {
            let mut pre = NovNativeExecutionStoreV1::default();
            pre.module_state
                .fee_oracle_rates_ppm
                .insert("ABC".to_string(), 1_500_000);
            pre.module_state.fee_oracle_updated_unix_ms = now_unix_millis_v1();
            pre.module_state.fee_oracle_source = "runtime_oracle".to_string();
            pre.module_state
                .protocol_clearing_nav_rate_ppm
                .insert("ABC".to_string(), 1_500_000);
            pre.module_state.clearing_enabled = true;
            pre.module_state.clearing_require_healthy_risk_buffer = false;
            pre.module_state.clearing_constrained_strategy = "treasury_direct_only".to_string();
            pre.module_state.treasury_min_reserve_bucket_nov = 100;
            pre.module_state.treasury_reserve_bucket_nov = 0;
            pre.module_state.treasury_min_risk_buffer_nov = 1;
            pre.module_state.treasury_risk_buffer_nov = 10;
            pre.module_state.clearing_static_amm_pools.insert(
                "abc_pool".to_string(),
                NovStaticAmmPoolStateV1 {
                    pool_id: "abc_pool".to_string(),
                    asset_x: "ABC".to_string(),
                    asset_y: "NOV".to_string(),
                    reserve_x: 1_000_000,
                    reserve_y: 1_000_000,
                    swap_fee_ppm: 3_000,
                    enabled: true,
                },
            );
            save_nov_native_execution_store_v1(path.as_path(), &pre)
                .expect("seed constrained non-treasury route state");

            let request = NovExecutionRequestV1 {
                tx_hash: [0x80; 32],
                chain_id: 7015,
                caller: vec![0xb0; 20],
                target: NovExecutionRequestTargetV1::NativeModule("treasury".to_string()),
                method: "deposit_reserve".to_string(),
                args: serde_json::to_vec(&serde_json::json!({
                    "asset": "ABC",
                    "amount": 2u64
                }))
                .expect("encode args"),
                fee_pay_asset: "ABC".to_string(),
                fee_max_pay_amount: 0,
                fee_slippage_bps: 50,
                gas_like_limit: Some(90_000),
                nonce: 22,
            };
            let receipt = dispatch_and_persist_nov_execution_request_with_store_path_v1(
                path.as_path(),
                &request,
            )
            .expect("dispatch should return constrained route restriction receipt");
            assert!(!receipt.status);
            assert_eq!(receipt.module, "fee");
            assert_eq!(receipt.method, "settlement");
            let failure_reason = receipt.failure_reason.clone().unwrap_or_default();
            assert!(
                failure_reason.starts_with("fee.clearing.constrained_route_restricted"),
                "unexpected failure reason: {failure_reason}"
            );

            let risk = run_nov_native_call_from_params_with_store_path_v1(
                &serde_json::json!({
                    "target": {"kind": "native_module", "id": "treasury"},
                    "method": "get_clearing_risk_summary",
                    "args": {}
                }),
                Some(path.as_path()),
            )
            .expect("get_clearing_risk_summary should succeed");
            assert_eq!(
                risk["result"]["current_threshold_state"].as_str(),
                Some("constrained")
            );
            assert_eq!(
                risk["result"]["last_trigger"]["failure_code"].as_str(),
                Some("fee.clearing.constrained_route_restricted")
            );
            assert_eq!(
                risk["result"]["last_candidate_routes"]["route_count"].as_u64(),
                Some(1)
            );

            let candidates = run_nov_native_call_from_params_with_store_path_v1(
                &serde_json::json!({
                    "target": {"kind": "native_module", "id": "treasury"},
                    "method": "get_last_clearing_candidates",
                    "args": {}
                }),
                Some(path.as_path()),
            )
            .expect("get_last_clearing_candidates should succeed");
            assert_eq!(candidates["result"]["route_count"].as_u64(), Some(1));

            let journal = run_nov_native_call_from_params_with_store_path_v1(
                &serde_json::json!({
                    "target": {"kind": "native_module", "id": "treasury"},
                    "method": "get_settlement_journal",
                    "args": {"limit": 1}
                }),
                Some(path.as_path()),
            )
            .expect("get_settlement_journal should succeed");
            assert_eq!(
                journal["result"]["entries"][0]["status"].as_str(),
                Some("rejected")
            );
            assert_eq!(
                journal["result"]["entries"][0]["policy_constrained_strategy"].as_str(),
                Some("treasury_direct_only")
            );
            assert_eq!(
                journal["result"]["entries"][0]["policy_event_state"].as_str(),
                Some("rejected")
            );
        });
    }

    #[test]
    fn constrained_treasury_direct_only_selects_treasury_route_when_available() {
        with_test_native_execution_store_path_v1(|path| {
            let mut pre = NovNativeExecutionStoreV1::default();
            pre.module_state
                .fee_oracle_rates_ppm
                .insert("USDT".to_string(), 1_500_000);
            pre.module_state.fee_oracle_updated_unix_ms = now_unix_millis_v1();
            pre.module_state.fee_oracle_source = "runtime_oracle".to_string();
            pre.module_state.clearing_enabled = true;
            pre.module_state.clearing_require_healthy_risk_buffer = false;
            pre.module_state.clearing_constrained_strategy = "treasury_direct_only".to_string();
            pre.module_state.treasury_min_reserve_bucket_nov = 100;
            pre.module_state.treasury_reserve_bucket_nov = 0;
            pre.module_state.treasury_min_risk_buffer_nov = 1;
            pre.module_state.treasury_risk_buffer_nov = 10;
            pre.module_state.clearing_static_amm_pools.insert(
                "usdt_pool_treasury_pref".to_string(),
                NovStaticAmmPoolStateV1 {
                    pool_id: "usdt_pool_treasury_pref".to_string(),
                    asset_x: "USDT".to_string(),
                    asset_y: "NOV".to_string(),
                    reserve_x: 1_000_000,
                    reserve_y: 1_000_000,
                    swap_fee_ppm: 3_000,
                    enabled: true,
                },
            );
            save_nov_native_execution_store_v1(path.as_path(), &pre)
                .expect("seed constrained treasury-direct preferred state");

            let request = NovExecutionRequestV1 {
                tx_hash: [0x83; 32],
                chain_id: 7018,
                caller: vec![0xb3; 20],
                target: NovExecutionRequestTargetV1::NativeModule("treasury".to_string()),
                method: "deposit_reserve".to_string(),
                args: serde_json::to_vec(&serde_json::json!({
                    "asset": "USDT",
                    "amount": 2u64
                }))
                .expect("encode args"),
                fee_pay_asset: "USDT".to_string(),
                fee_max_pay_amount: 10_000,
                fee_slippage_bps: 50,
                gas_like_limit: Some(90_000),
                nonce: 25,
            };
            let receipt = dispatch_and_persist_nov_execution_request_with_store_path_v1(
                path.as_path(),
                &request,
            )
            .expect("dispatch should succeed with treasury-direct constrained strategy");
            assert!(receipt.status);
            let route_meta = receipt.route_meta.expect("route meta should exist");
            assert_eq!(route_meta.route_source, "treasury_direct");
            assert!(route_meta.candidate_route_count >= 2);

            let candidates = run_nov_native_call_from_params_with_store_path_v1(
                &serde_json::json!({
                    "target": {"kind": "native_module", "id": "treasury"},
                    "method": "get_last_clearing_candidates",
                    "args": {}
                }),
                Some(path.as_path()),
            )
            .expect("get_last_clearing_candidates should succeed");
            assert!(
                candidates["result"]["route_count"]
                    .as_u64()
                    .unwrap_or_default()
                    >= 2
            );
            assert_eq!(
                candidates["result"]["policy_context"]["policy_constrained_strategy"].as_str(),
                Some("treasury_direct_only")
            );
            assert_eq!(
                candidates["result"]["policy_context"]["policy_threshold_state"].as_str(),
                Some("constrained")
            );

            let selected = run_nov_native_call_from_params_with_store_path_v1(
                &serde_json::json!({
                    "target": {"kind": "native_module", "id": "treasury"},
                    "method": "get_last_clearing_route",
                    "args": {}
                }),
                Some(path.as_path()),
            )
            .expect("get_last_clearing_route should succeed");
            assert_eq!(selected["found"].as_bool(), Some(true));
            assert_eq!(
                selected["result"]["route_source"].as_str(),
                Some("treasury_direct")
            );
            assert_eq!(
                selected["result"]["selection_reason"].as_str(),
                Some("expected_out_then_liquidity_then_freshness")
            );
            assert_eq!(
                selected["result"]["policy_context"]["policy_constrained_strategy"].as_str(),
                Some("treasury_direct_only")
            );
            assert_eq!(
                selected["result"]["policy_context"]["policy_threshold_state"].as_str(),
                Some("constrained")
            );

            let journal = run_nov_native_call_from_params_with_store_path_v1(
                &serde_json::json!({
                    "target": {"kind": "native_module", "id": "treasury"},
                    "method": "get_settlement_journal",
                    "args": {"limit": 1}
                }),
                Some(path.as_path()),
            )
            .expect("get_settlement_journal should succeed");
            assert_eq!(
                journal["result"]["entries"][0]["status"].as_str(),
                Some("applied")
            );
            assert_eq!(
                journal["result"]["entries"][0]["policy_constrained_strategy"].as_str(),
                Some("treasury_direct_only")
            );
            assert_eq!(
                journal["result"]["entries"][0]["policy_event_state"].as_str(),
                Some("settled")
            );
        });
    }

    #[test]
    fn constrained_threshold_state_enforces_constrained_daily_volume_cap() {
        with_test_native_execution_store_path_v1(|path| {
            let mut pre = NovNativeExecutionStoreV1::default();
            pre.module_state
                .fee_oracle_rates_ppm
                .insert("USDT".to_string(), 2_000_000);
            pre.module_state.fee_oracle_updated_unix_ms = now_unix_millis_v1();
            pre.module_state.fee_oracle_source = "runtime_oracle".to_string();
            pre.module_state.clearing_enabled = true;
            pre.module_state.clearing_require_healthy_risk_buffer = false;
            pre.module_state.clearing_constrained_strategy = "daily_volume_only".to_string();
            pre.module_state.clearing_daily_nov_hard_limit = 1_000;
            pre.module_state.clearing_daily_nov_used = 80;
            pre.module_state.clearing_daily_window_day = current_day_index_v1(now_unix_millis_v1());
            pre.module_state.clearing_constrained_daily_usage_bps = 930;
            pre.module_state.treasury_min_reserve_bucket_nov = 100;
            pre.module_state.treasury_reserve_bucket_nov = 0;
            pre.module_state.treasury_min_risk_buffer_nov = 1;
            pre.module_state.treasury_risk_buffer_nov = 10;
            save_nov_native_execution_store_v1(path.as_path(), &pre)
                .expect("seed constrained daily cap state");

            let request = NovExecutionRequestV1 {
                tx_hash: [0x81; 32],
                chain_id: 7016,
                caller: vec![0xb1; 20],
                target: NovExecutionRequestTargetV1::NativeModule("treasury".to_string()),
                method: "deposit_reserve".to_string(),
                args: serde_json::to_vec(&serde_json::json!({
                    "asset": "USDT",
                    "amount": 2u64
                }))
                .expect("encode args"),
                fee_pay_asset: "USDT".to_string(),
                fee_max_pay_amount: 10_000,
                fee_slippage_bps: 50,
                gas_like_limit: Some(90_000),
                nonce: 23,
            };
            let receipt = dispatch_and_persist_nov_execution_request_with_store_path_v1(
                path.as_path(),
                &request,
            )
            .expect("dispatch should return constrained daily cap receipt");
            assert!(!receipt.status);
            assert_eq!(receipt.module, "fee");
            assert_eq!(receipt.method, "settlement");
            assert!(receipt
                .failure_reason
                .clone()
                .unwrap_or_default()
                .starts_with("fee.clearing.constrained_daily_volume_exceeded"));

            let risk = run_nov_native_call_from_params_with_store_path_v1(
                &serde_json::json!({
                    "target": {"kind": "native_module", "id": "treasury"},
                    "method": "get_clearing_risk_summary",
                    "args": {}
                }),
                Some(path.as_path()),
            )
            .expect("get_clearing_risk_summary should succeed");
            assert_eq!(
                risk["result"]["current_threshold_state"].as_str(),
                Some("constrained")
            );
            assert_eq!(
                risk["result"]["last_trigger"]["failure_code"].as_str(),
                Some("fee.clearing.constrained_daily_volume_exceeded")
            );
            assert_eq!(
                risk["result"]["last_candidate_routes"]["route_count"].as_u64(),
                Some(1)
            );

            let journal = run_nov_native_call_from_params_with_store_path_v1(
                &serde_json::json!({
                    "target": {"kind": "native_module", "id": "treasury"},
                    "method": "get_settlement_journal",
                    "args": {"limit": 1}
                }),
                Some(path.as_path()),
            )
            .expect("get_settlement_journal should succeed");
            assert_eq!(
                journal["result"]["entries"][0]["status"].as_str(),
                Some("rejected")
            );
            assert_eq!(
                journal["result"]["entries"][0]["policy_constrained_strategy"].as_str(),
                Some("daily_volume_only")
            );
            assert_eq!(
                journal["result"]["entries"][0]["policy_event_state"].as_str(),
                Some("rejected")
            );
        });
    }

    #[test]
    fn constrained_strategy_blocked_rejects_non_nov_clearing() {
        with_test_native_execution_store_path_v1(|path| {
            let mut pre = NovNativeExecutionStoreV1::default();
            pre.module_state
                .fee_oracle_rates_ppm
                .insert("USDT".to_string(), 2_000_000);
            pre.module_state.fee_oracle_updated_unix_ms = now_unix_millis_v1();
            pre.module_state.fee_oracle_source = "runtime_oracle".to_string();
            pre.module_state.clearing_enabled = true;
            pre.module_state.clearing_require_healthy_risk_buffer = false;
            pre.module_state.clearing_constrained_strategy = "blocked".to_string();
            pre.module_state.treasury_min_reserve_bucket_nov = 100;
            pre.module_state.treasury_reserve_bucket_nov = 0;
            pre.module_state.treasury_min_risk_buffer_nov = 1;
            pre.module_state.treasury_risk_buffer_nov = 10;
            pre.module_state.clearing_static_amm_pools.insert(
                "usdt_pool_blocked".to_string(),
                NovStaticAmmPoolStateV1 {
                    pool_id: "usdt_pool_blocked".to_string(),
                    asset_x: "USDT".to_string(),
                    asset_y: "NOV".to_string(),
                    reserve_x: 1_000_000,
                    reserve_y: 1_000_000,
                    swap_fee_ppm: 3_000,
                    enabled: true,
                },
            );
            save_nov_native_execution_store_v1(path.as_path(), &pre)
                .expect("seed constrained blocked strategy state");

            let request = NovExecutionRequestV1 {
                tx_hash: [0x82; 32],
                chain_id: 7017,
                caller: vec![0xb2; 20],
                target: NovExecutionRequestTargetV1::NativeModule("treasury".to_string()),
                method: "deposit_reserve".to_string(),
                args: serde_json::to_vec(&serde_json::json!({
                    "asset": "USDT",
                    "amount": 2u64
                }))
                .expect("encode args"),
                fee_pay_asset: "USDT".to_string(),
                fee_max_pay_amount: 10_000,
                fee_slippage_bps: 50,
                gas_like_limit: Some(90_000),
                nonce: 24,
            };
            let receipt = dispatch_and_persist_nov_execution_request_with_store_path_v1(
                path.as_path(),
                &request,
            )
            .expect("dispatch should return constrained blocked receipt");
            assert!(!receipt.status);
            assert_eq!(receipt.module, "fee");
            assert_eq!(receipt.method, "settlement");
            assert!(receipt
                .failure_reason
                .clone()
                .unwrap_or_default()
                .starts_with("fee.clearing.constrained_blocked"));

            let risk = run_nov_native_call_from_params_with_store_path_v1(
                &serde_json::json!({
                    "target": {"kind": "native_module", "id": "treasury"},
                    "method": "get_clearing_risk_summary",
                    "args": {}
                }),
                Some(path.as_path()),
            )
            .expect("get_clearing_risk_summary should succeed");
            assert_eq!(
                risk["result"]["current_threshold_state"].as_str(),
                Some("constrained")
            );
            assert_eq!(
                risk["result"]["policy"]["clearing_constrained_strategy"].as_str(),
                Some("blocked")
            );
            assert_eq!(
                risk["result"]["last_trigger"]["failure_code"].as_str(),
                Some("fee.clearing.constrained_blocked")
            );
            assert!(
                risk["result"]["last_candidate_routes"]["route_count"]
                    .as_u64()
                    .unwrap_or_default()
                    >= 2
            );

            let last_route = run_nov_native_call_from_params_with_store_path_v1(
                &serde_json::json!({
                    "target": {"kind": "native_module", "id": "treasury"},
                    "method": "get_last_clearing_route",
                    "args": {}
                }),
                Some(path.as_path()),
            )
            .expect("get_last_clearing_route should succeed");
            assert_eq!(last_route["found"].as_bool(), Some(false));

            let journal = run_nov_native_call_from_params_with_store_path_v1(
                &serde_json::json!({
                    "target": {"kind": "native_module", "id": "treasury"},
                    "method": "get_settlement_journal",
                    "args": {"limit": 1}
                }),
                Some(path.as_path()),
            )
            .expect("get_settlement_journal should succeed");
            assert_eq!(
                journal["result"]["entries"][0]["status"].as_str(),
                Some("rejected")
            );
            assert_eq!(
                journal["result"]["entries"][0]["policy_constrained_strategy"].as_str(),
                Some("blocked")
            );
            assert_eq!(
                journal["result"]["entries"][0]["policy_event_state"].as_str(),
                Some("rejected")
            );
        });
    }

    #[test]
    fn fee_quote_expired_is_recorded_as_clearing_failure() {
        let mut store = NovNativeExecutionStoreV1::default();
        let quote = NovFeeQuoteV1 {
            quote_id: "q-expired-test".to_string(),
            pay_asset: "USDT".to_string(),
            nov_amount: 100,
            quoted_pay_amount: 50,
            quoted_pay_amount_with_slippage: 60,
            max_pay_amount: 60,
            slippage_bps: 100,
            quoted_at_unix_ms: 100,
            expires_at_unix_ms: 150,
            rate_ppm: 2_000_000,
            oracle_updated_at_unix_ms: 100,
            route: "usdt_to_nov".to_string(),
            quote_contract: NOV_EXECUTION_FEE_QUOTE_CONTRACT_V1.to_string(),
            price_source: "test".to_string(),
        };
        let subject_meta = NovExecutionSubjectMetaV1 {
            account_id: "acct-expired".to_string(),
            fee_owner_account_id: "acct-expired".to_string(),
            nonce_owner_account_id: "acct-expired".to_string(),
            key_algo: String::new(),
            execution_policy: NovExecutionPolicyV1::Standard.as_str().to_string(),
            policy_enforced: false,
            policy_rejection_reason: None,
        };
        let err =
            settle_fee_quote_into_treasury_v1(&mut store, &quote, "deadbeef", &subject_meta, 200)
                .expect_err("expired quote should fail");
        let reason = format!("{err}");
        assert!(reason.starts_with("fee.clearing.quote_expired"));
        assert_eq!(
            store.module_state.last_clearing_failure_code.as_str(),
            "fee.clearing.quote_expired"
        );
        assert!(
            store
                .module_state
                .clearing_failure_counts
                .get("USDT:quote_expired")
                .copied()
                .unwrap_or_default()
                >= 1
        );
    }

    #[test]
    fn treasury_get_clearing_risk_summary_exposes_policy_and_last_trigger() {
        with_test_native_execution_store_path_v1(|path| {
            let mut pre = NovNativeExecutionStoreV1::default();
            pre.module_state.clearing_enabled = false;
            save_nov_native_execution_store_v1(path.as_path(), &pre)
                .expect("seed clearing disabled store");

            let request = NovExecutionRequestV1 {
                tx_hash: [0x77; 32],
                chain_id: 7007,
                caller: vec![0xaa; 20],
                target: NovExecutionRequestTargetV1::NativeModule("treasury".to_string()),
                method: "deposit_reserve".to_string(),
                args: serde_json::to_vec(&serde_json::json!({
                    "asset": "USDT",
                    "amount": 2u64
                }))
                .expect("encode args"),
                fee_pay_asset: "USDT".to_string(),
                fee_max_pay_amount: 1_000,
                fee_slippage_bps: 50,
                gas_like_limit: Some(90_000),
                nonce: 14,
            };
            let _ = dispatch_and_persist_nov_execution_request_with_store_path_v1(
                path.as_path(),
                &request,
            )
            .expect("dispatch should return clearing disabled receipt");

            let summary = run_nov_native_call_from_params_with_store_path_v1(
                &serde_json::json!({
                    "target": {"kind": "native_module", "id": "treasury"},
                    "method": "get_clearing_risk_summary",
                    "args": {},
                }),
                Some(path.as_path()),
            )
            .expect("get_clearing_risk_summary should succeed");
            assert_eq!(
                summary["result"]["policy"]["clearing_enabled"].as_bool(),
                Some(false)
            );
            assert_eq!(
                summary["result"]["last_trigger"]["failure_code"].as_str(),
                Some("fee.clearing.clearing_disabled")
            );
            assert_eq!(
                summary["result"]["current_threshold_state"].as_str(),
                Some("blocked")
            );
            assert!(
                summary["result"]["failure_summary"]["total_failures"]
                    .as_u64()
                    .unwrap_or_default()
                    >= 1
            );
        });
    }

    #[test]
    fn treasury_execution_trace_and_metrics_queries_work() {
        with_test_native_execution_store_path_v1(|path| {
            let request = NovExecutionRequestV1 {
                tx_hash: [0x93; 32],
                chain_id: 7020,
                caller: vec![0x33; 20],
                target: NovExecutionRequestTargetV1::NativeModule("treasury".to_string()),
                method: "deposit_reserve".to_string(),
                args: serde_json::to_vec(&serde_json::json!({
                    "asset": "USDT",
                    "amount": 3u64
                }))
                .expect("encode args"),
                fee_pay_asset: "USDT".to_string(),
                fee_max_pay_amount: 10_000,
                fee_slippage_bps: 80,
                gas_like_limit: Some(95_000),
                nonce: 31,
            };
            let receipt = dispatch_and_persist_nov_execution_request_with_store_path_v1(
                path.as_path(),
                &request,
            )
            .expect("dispatch should succeed");
            assert!(receipt.status);

            let last_trace = run_nov_native_call_from_params_with_store_path_v1(
                &serde_json::json!({
                    "target": {"kind": "native_module", "id": "treasury"},
                    "method": "get_last_execution_trace",
                    "args": {}
                }),
                Some(path.as_path()),
            )
            .expect("get_last_execution_trace should succeed");
            assert_eq!(last_trace["found"].as_bool(), Some(true));
            assert_eq!(
                last_trace["result"]["tx_id"].as_str(),
                Some(receipt.tx_hash.as_str())
            );

            let trace_by_tx = run_nov_native_call_from_params_with_store_path_v1(
                &serde_json::json!({
                    "target": {"kind": "native_module", "id": "treasury"},
                    "method": "get_execution_trace_by_tx",
                    "args": {"tx_hash": receipt.tx_hash}
                }),
                Some(path.as_path()),
            )
            .expect("get_execution_trace_by_tx should succeed");
            assert_eq!(trace_by_tx["found"].as_bool(), Some(true));
            assert_eq!(
                trace_by_tx["result"]["final_status"].as_str(),
                Some("success")
            );

            let clearing_metrics = run_nov_native_call_from_params_with_store_path_v1(
                &serde_json::json!({
                    "target": {"kind": "native_module", "id": "treasury"},
                    "method": "get_clearing_metrics_summary",
                    "args": {}
                }),
                Some(path.as_path()),
            )
            .expect("get_clearing_metrics_summary should succeed");
            assert!(
                clearing_metrics["result"]["metrics"]["total_clearing_attempts"]
                    .as_u64()
                    .unwrap_or_default()
                    >= 1
            );

            let policy_metrics = run_nov_native_call_from_params_with_store_path_v1(
                &serde_json::json!({
                    "target": {"kind": "native_module", "id": "treasury"},
                    "method": "get_policy_metrics_summary",
                    "args": {}
                }),
                Some(path.as_path()),
            )
            .expect("get_policy_metrics_summary should succeed");
            assert!(!policy_metrics["result"]["metrics"]["policy_contract_id"]
                .as_str()
                .unwrap_or_default()
                .is_empty());
            assert!(
                policy_metrics["result"]["metrics"]["trace_count"]
                    .as_u64()
                    .unwrap_or_default()
                    >= 1
            );
        });
    }

    #[test]
    fn treasury_redeem_alias_credits_user_balance_and_journal() {
        with_test_native_execution_store_path_v1(|path| {
            let mut pre = NovNativeExecutionStoreV1::default();
            pre.module_state
                .treasury_reserves
                .insert("NOV".to_string(), 250);
            pre.module_state.treasury_reserve_bucket_nov = 200;
            pre.module_state.treasury_fee_bucket_nov = 30;
            pre.module_state.treasury_risk_buffer_nov = 20;
            pre.module_state.treasury_settled_nov_total = 250;
            pre.module_state.treasury_settlements = 1;
            save_nov_native_execution_store_v1(path.as_path(), &pre).expect("seed reserve state");

            let request = NovExecutionRequestV1 {
                tx_hash: [0xa1; 32],
                chain_id: 8021,
                caller: vec![0x41; 20],
                target: NovExecutionRequestTargetV1::NativeModule("treasury".to_string()),
                method: "redeem".to_string(),
                args: serde_json::to_vec(&serde_json::json!({
                    "asset_out": "NOV",
                    "nov_amount": 50u64
                }))
                .expect("encode args"),
                fee_pay_asset: "NOV".to_string(),
                fee_max_pay_amount: 500,
                fee_slippage_bps: 0,
                gas_like_limit: Some(80_000),
                nonce: 41,
            };
            let receipt = dispatch_and_persist_nov_execution_request_with_store_path_v1(
                path.as_path(),
                &request,
            )
            .expect("dispatch should return successful redeem receipt");
            assert!(receipt.status);
            assert_eq!(receipt.module, "treasury");
            assert_eq!(receipt.method, "redeem");

            let caller = to_hex_prefixed_v1(request.caller.as_slice());
            let balance = get_nov_native_account_asset_balance_with_store_path_v1(
                path.as_path(),
                caller.as_str(),
                "NOV",
            )
            .expect("native account balance should load");
            assert_eq!(balance, 50);

            let journal = run_nov_native_call_from_params_with_store_path_v1(
                &serde_json::json!({
                    "target": {"kind": "native_module", "id": "treasury"},
                    "method": "get_settlement_journal",
                    "args": {"limit": 1}
                }),
                Some(path.as_path()),
            )
            .expect("get_settlement_journal should succeed");
            assert_eq!(
                journal["result"]["entries"][0]["kind"].as_str(),
                Some("reserve_redeem")
            );
            assert_eq!(
                journal["result"]["entries"][0]["status"].as_str(),
                Some("applied")
            );
        });
    }

    #[test]
    fn treasury_redeem_m2_asset_uses_protocol_redeem_price_and_debits_nov() {
        with_test_native_execution_store_path_v1(|path| {
            let caller = format!("0x{}", "49".repeat(20));
            let mut pre = NovNativeExecutionStoreV1::default();
            credit_native_account_asset_balance_v1(&mut pre, caller.as_str(), "NOV", 500);
            pre.module_state
                .treasury_reserves
                .insert("USDT".to_string(), 1_000);
            pre.module_state
                .clearing_rate_ppm
                .insert("USDT".to_string(), 1_000_000);
            pre.module_state
                .protocol_clearing_nav_rate_ppm
                .insert("USDT".to_string(), 1_000_000);
            save_nov_native_execution_store_v1(path.as_path(), &pre)
                .expect("seed USDT reserve and NOV balance");

            let request = NovExecutionRequestV1 {
                tx_hash: [0xa9; 32],
                chain_id: 8029,
                caller: vec![0x49; 20],
                target: NovExecutionRequestTargetV1::NativeModule("treasury".to_string()),
                method: "redeem".to_string(),
                args: serde_json::to_vec(&serde_json::json!({
                    "asset_out": "USDT",
                    "nov_amount": 100u64
                }))
                .expect("encode args"),
                fee_pay_asset: "NOV".to_string(),
                fee_max_pay_amount: 500,
                fee_slippage_bps: 0,
                gas_like_limit: Some(80_000),
                nonce: 49,
            };
            let receipt = dispatch_and_persist_nov_execution_request_with_store_path_v1(
                path.as_path(),
                &request,
            )
            .expect("dispatch should return successful USDT redeem receipt");
            assert!(
                receipt.status,
                "failure_reason={:?}",
                receipt.failure_reason
            );
            assert_eq!(receipt.module, "treasury");
            assert_eq!(receipt.method, "redeem");

            let nov_after = get_nov_native_account_asset_balance_with_store_path_v1(
                path.as_path(),
                caller.as_str(),
                "NOV",
            )
            .expect("load NOV balance");
            let usdt_after = get_nov_native_account_asset_balance_with_store_path_v1(
                path.as_path(),
                caller.as_str(),
                "USDT",
            )
            .expect("load USDT balance");
            assert_eq!(nov_after, 400);
            assert_eq!(usdt_after, 99);

            let journal = run_nov_native_call_from_params_with_store_path_v1(
                &serde_json::json!({
                    "target": {"kind": "native_module", "id": "treasury"},
                    "method": "get_settlement_journal",
                    "args": {"limit": 1}
                }),
                Some(path.as_path()),
            )
            .expect("get_settlement_journal should succeed");
            let entry = &journal["result"]["entries"][0];
            assert_eq!(entry["kind"].as_str(), Some("reserve_redeem"));
            assert_eq!(entry["source_asset"].as_str(), Some("USDT"));
            assert_eq!(entry["source_amount"].as_u64(), Some(99));
            assert_eq!(entry["settled_nov"].as_u64(), Some(100));
            assert_eq!(
                entry["clearing_source"].as_str(),
                Some("protocol_clearing_redeem:constrained")
            );
            assert!(
                entry["clearing_rate_ppm"].as_u64().unwrap_or_default() > 1_000_000,
                "redeem must use reverse conservative price"
            );
        });
    }

    #[test]
    fn treasury_redeem_m2_asset_rejects_legacy_asset_amount_without_nov_debit() {
        with_test_native_execution_store_path_v1(|path| {
            let caller = format!("0x{}", "4a".repeat(20));
            let mut pre = NovNativeExecutionStoreV1::default();
            credit_native_account_asset_balance_v1(&mut pre, caller.as_str(), "NOV", 500);
            pre.module_state
                .treasury_reserves
                .insert("USDT".to_string(), 1_000);
            pre.module_state
                .protocol_clearing_nav_rate_ppm
                .insert("USDT".to_string(), 1_000_000);
            save_nov_native_execution_store_v1(path.as_path(), &pre)
                .expect("seed USDT reserve and NOV balance");

            let request = NovExecutionRequestV1 {
                tx_hash: [0xaa; 32],
                chain_id: 8030,
                caller: vec![0x4a; 20],
                target: NovExecutionRequestTargetV1::NativeModule("treasury".to_string()),
                method: "redeem".to_string(),
                args: serde_json::to_vec(&serde_json::json!({
                    "asset_out": "USDT",
                    "amount": 10u64
                }))
                .expect("encode args"),
                fee_pay_asset: "NOV".to_string(),
                fee_max_pay_amount: 500,
                fee_slippage_bps: 0,
                gas_like_limit: Some(80_000),
                nonce: 50,
            };
            let receipt = dispatch_and_persist_nov_execution_request_with_store_path_v1(
                path.as_path(),
                &request,
            )
            .expect("dispatch should reject legacy direct asset amount redeem");
            assert!(!receipt.status);
            assert_eq!(receipt.module, "treasury");
            assert_eq!(receipt.method, "redeem");
            let failure = receipt.failure_reason.clone().unwrap_or_default();
            assert!(failure.starts_with("fee.settlement.redeem_requires_nov_amount"));
            assert!(failure.contains("asset=USDT"));
            assert!(failure.contains("P_redeem"));

            let nov_after = get_nov_native_account_asset_balance_with_store_path_v1(
                path.as_path(),
                caller.as_str(),
                "NOV",
            )
            .expect("load NOV balance");
            let usdt_after = get_nov_native_account_asset_balance_with_store_path_v1(
                path.as_path(),
                caller.as_str(),
                "USDT",
            )
            .expect("load USDT balance");
            let state = load_nov_native_execution_store_v1(path.as_path())
                .expect("load native execution store");
            assert_eq!(nov_after, 500);
            assert_eq!(usdt_after, 0);
            assert_eq!(
                state.module_state.treasury_reserves.get("USDT").copied(),
                Some(1_000)
            );
        });
    }

    #[test]
    fn reserve_proof_revoked_blocks_non_nov_treasury_redeem_without_nov_debit() {
        with_test_native_execution_store_path_v1(|path| {
            let caller = format!("0x{}", "92".repeat(20));
            let mut pre = NovNativeExecutionStoreV1::default();
            credit_native_account_asset_balance_v1(&mut pre, caller.as_str(), "NOV", 500);
            pre.module_state
                .treasury_reserves
                .insert("USDT".to_string(), 1_000);
            pre.module_state.treasury_reserve_proofs.insert(
                "USDT".to_string(),
                NovTreasuryReserveProofV1 {
                    asset: "USDT".to_string(),
                    reserve_amount: 1_000,
                    proof_type: "custody_statement_v1".to_string(),
                    proof_digest: "0xdeadbeef02".to_string(),
                    proof_source: "treasury_committee".to_string(),
                    proof_reference: "revoked-report-002".to_string(),
                    observed_at_unix_ms: 1,
                    expires_at_unix_ms: 0,
                    policy_version: 1,
                    policy_source: "governance_path".to_string(),
                    status: "revoked".to_string(),
                    automated_verification: false,
                    verification_mode: "manual_governance_attestation".to_string(),
                },
            );
            save_nov_native_execution_store_v1(path.as_path(), &pre)
                .expect("seed revoked reserve proof");

            let request = NovExecutionRequestV1 {
                tx_hash: [0x92; 32],
                chain_id: 8092,
                caller: vec![0x92; 20],
                target: NovExecutionRequestTargetV1::NativeModule("treasury".to_string()),
                method: "redeem".to_string(),
                args: serde_json::to_vec(&serde_json::json!({
                    "asset_out": "USDT",
                    "nov_amount": 100u64
                }))
                .expect("encode args"),
                fee_pay_asset: "NOV".to_string(),
                fee_max_pay_amount: 500,
                fee_slippage_bps: 0,
                gas_like_limit: Some(80_000),
                nonce: 92,
            };
            let receipt = dispatch_and_persist_nov_execution_request_with_store_path_v1(
                path.as_path(),
                &request,
            )
            .expect("dispatch should return reserve proof failure receipt");
            assert!(!receipt.status);
            assert_eq!(receipt.module, "treasury");
            assert_eq!(receipt.method, "redeem");
            let failure = receipt.failure_reason.clone().unwrap_or_default();
            assert!(failure.starts_with("fee.settlement.reserve_proof_not_active"));
            assert!(failure.contains("reserve_proof_effective_status=revoked"));
            assert!(failure.contains("proof_reference=revoked-report-002"));

            let nov_after = get_nov_native_account_asset_balance_with_store_path_v1(
                path.as_path(),
                caller.as_str(),
                "NOV",
            )
            .expect("load NOV balance");
            let usdt_after = get_nov_native_account_asset_balance_with_store_path_v1(
                path.as_path(),
                caller.as_str(),
                "USDT",
            )
            .expect("load USDT balance");
            assert_eq!(nov_after, 500);
            assert_eq!(usdt_after, 0);
            let state = load_nov_native_execution_store_v1(path.as_path())
                .expect("load native execution store");
            assert_eq!(
                state.module_state.treasury_reserves.get("USDT").copied(),
                Some(1_000)
            );
        });
    }

    #[test]
    fn neth_missing_reserve_proof_blocks_treasury_redeem_without_nov_debit() {
        with_test_native_execution_store_path_v1(|path| {
            let caller = format!("0x{}", "96".repeat(20));
            let mut pre = NovNativeExecutionStoreV1::default();
            credit_native_account_asset_balance_v1(&mut pre, caller.as_str(), "NOV", 500);
            pre.module_state
                .treasury_reserves
                .insert("NETH".to_string(), 1_000);
            pre.module_state.mapped_lock_contract_address = format!("0x{}", "96".repeat(20));
            pre.module_state.mapped_lock_min_confirmations = 21;
            save_nov_native_execution_store_v1(path.as_path(), &pre)
                .expect("seed missing NETH reserve proof");

            let request = NovExecutionRequestV1 {
                tx_hash: [0x96; 32],
                chain_id: 8096,
                caller: vec![0x96; 20],
                target: NovExecutionRequestTargetV1::NativeModule("treasury".to_string()),
                method: "redeem".to_string(),
                args: serde_json::to_vec(&serde_json::json!({
                    "asset_out": "NETH",
                    "nov_amount": 100u64
                }))
                .expect("encode args"),
                fee_pay_asset: "NOV".to_string(),
                fee_max_pay_amount: 500,
                fee_slippage_bps: 0,
                gas_like_limit: Some(80_000),
                nonce: 96,
            };
            let receipt = dispatch_and_persist_nov_execution_request_with_store_path_v1(
                path.as_path(),
                &request,
            )
            .expect("dispatch should return missing proof NETH M2 risk failure receipt");
            assert!(!receipt.status);
            assert_eq!(receipt.module, "treasury");
            assert_eq!(receipt.method, "redeem");
            let failure = receipt.failure_reason.clone().unwrap_or_default();
            assert!(failure.starts_with("fee.settlement.m2_bridge_risk_blocked"));
            assert!(failure.contains("m2_bridge_risk=reserve_proof_missing"));

            let nov_after = get_nov_native_account_asset_balance_with_store_path_v1(
                path.as_path(),
                caller.as_str(),
                "NOV",
            )
            .expect("load NOV balance");
            let neth_after = get_nov_native_account_asset_balance_with_store_path_v1(
                path.as_path(),
                caller.as_str(),
                "NETH",
            )
            .expect("load NETH balance");
            assert_eq!(nov_after, 500);
            assert_eq!(neth_after, 0);
            let state = load_nov_native_execution_store_v1(path.as_path())
                .expect("load native execution store");
            assert_eq!(
                state.module_state.treasury_reserves.get("NETH").copied(),
                Some(1_000)
            );
        });
    }

    #[test]
    fn neth_unset_lock_policy_blocks_treasury_redeem_without_neth_credit() {
        with_test_native_execution_store_path_v1(|path| {
            let caller = format!("0x{}", "97".repeat(20));
            let mut pre = NovNativeExecutionStoreV1::default();
            credit_native_account_asset_balance_v1(&mut pre, caller.as_str(), "NOV", 500);
            pre.module_state
                .treasury_reserves
                .insert("NETH".to_string(), 1_000);
            pre.module_state.treasury_reserve_proofs.insert(
                "NETH".to_string(),
                NovTreasuryReserveProofV1 {
                    asset: "NETH".to_string(),
                    reserve_amount: 1_000,
                    proof_type: "custody_statement_v1".to_string(),
                    proof_digest: "0xnethunsetpolicyredeem01".to_string(),
                    proof_source: "treasury_committee".to_string(),
                    proof_reference: "neth-unset-policy-redeem-001".to_string(),
                    observed_at_unix_ms: 1,
                    expires_at_unix_ms: 0,
                    policy_version: 1,
                    policy_source: "governance_path".to_string(),
                    status: "active".to_string(),
                    automated_verification: false,
                    verification_mode: "manual_governance_attestation".to_string(),
                },
            );
            pre.module_state.mapped_lock_min_confirmations = 21;
            save_nov_native_execution_store_v1(path.as_path(), &pre)
                .expect("seed unset NETH lock policy redeem risk");

            let request = NovExecutionRequestV1 {
                tx_hash: [0x97; 32],
                chain_id: 8097,
                caller: vec![0x97; 20],
                target: NovExecutionRequestTargetV1::NativeModule("treasury".to_string()),
                method: "redeem".to_string(),
                args: serde_json::to_vec(&serde_json::json!({
                    "asset_out": "NETH",
                    "nov_amount": 100u64
                }))
                .expect("encode args"),
                fee_pay_asset: "NOV".to_string(),
                fee_max_pay_amount: 500,
                fee_slippage_bps: 0,
                gas_like_limit: Some(80_000),
                nonce: 97,
            };
            let receipt = dispatch_and_persist_nov_execution_request_with_store_path_v1(
                path.as_path(),
                &request,
            )
            .expect("dispatch should return unset lock policy NETH M2 risk failure receipt");
            assert!(!receipt.status);
            assert_eq!(receipt.module, "treasury");
            assert_eq!(receipt.method, "redeem");
            let failure = receipt.failure_reason.clone().unwrap_or_default();
            assert!(failure.starts_with("fee.settlement.m2_bridge_risk_blocked"));
            assert!(failure.contains("m2_bridge_risk=mapped_lock_contract_address_unset"));

            let nov_after = get_nov_native_account_asset_balance_with_store_path_v1(
                path.as_path(),
                caller.as_str(),
                "NOV",
            )
            .expect("load NOV balance");
            let neth_after = get_nov_native_account_asset_balance_with_store_path_v1(
                path.as_path(),
                caller.as_str(),
                "NETH",
            )
            .expect("load NETH balance");
            assert!(
                nov_after <= 500,
                "treasury redeem rejection must not mint or increase NOV"
            );
            assert_eq!(neth_after, 0);
            let state = load_nov_native_execution_store_v1(path.as_path())
                .expect("load native execution store");
            assert_eq!(
                state.module_state.treasury_reserves.get("NETH").copied(),
                Some(1_000)
            );
        });
    }

    #[test]
    fn neth_unset_min_confirmations_blocks_treasury_redeem_without_neth_credit() {
        with_test_native_execution_store_path_v1(|path| {
            let caller = format!("0x{}", "98".repeat(20));
            let mut pre = NovNativeExecutionStoreV1::default();
            credit_native_account_asset_balance_v1(&mut pre, caller.as_str(), "NOV", 500);
            pre.module_state
                .treasury_reserves
                .insert("NETH".to_string(), 1_000);
            pre.module_state.treasury_reserve_proofs.insert(
                "NETH".to_string(),
                NovTreasuryReserveProofV1 {
                    asset: "NETH".to_string(),
                    reserve_amount: 1_000,
                    proof_type: "custody_statement_v1".to_string(),
                    proof_digest: "0xnethunsetconfirmationsredeem01".to_string(),
                    proof_source: "treasury_committee".to_string(),
                    proof_reference: "neth-unset-confirmations-redeem-001".to_string(),
                    observed_at_unix_ms: 1,
                    expires_at_unix_ms: 0,
                    policy_version: 1,
                    policy_source: "governance_path".to_string(),
                    status: "active".to_string(),
                    automated_verification: false,
                    verification_mode: "manual_governance_attestation".to_string(),
                },
            );
            pre.module_state.mapped_lock_contract_address = format!("0x{}", "98".repeat(20));
            save_nov_native_execution_store_v1(path.as_path(), &pre)
                .expect("seed unset NETH min confirmations redeem risk");

            let request = NovExecutionRequestV1 {
                tx_hash: [0x98; 32],
                chain_id: 8098,
                caller: vec![0x98; 20],
                target: NovExecutionRequestTargetV1::NativeModule("treasury".to_string()),
                method: "redeem".to_string(),
                args: serde_json::to_vec(&serde_json::json!({
                    "asset_out": "NETH",
                    "nov_amount": 100u64
                }))
                .expect("encode args"),
                fee_pay_asset: "NOV".to_string(),
                fee_max_pay_amount: 500,
                fee_slippage_bps: 0,
                gas_like_limit: Some(80_000),
                nonce: 98,
            };
            let receipt = dispatch_and_persist_nov_execution_request_with_store_path_v1(
                path.as_path(),
                &request,
            )
            .expect("dispatch should return unset min confirmations NETH M2 risk failure receipt");
            assert!(!receipt.status);
            assert_eq!(receipt.module, "treasury");
            assert_eq!(receipt.method, "redeem");
            let failure = receipt.failure_reason.clone().unwrap_or_default();
            assert!(failure.starts_with("fee.settlement.m2_bridge_risk_blocked"));
            assert!(failure.contains("m2_bridge_risk=mapped_lock_min_confirmations_unset"));

            let nov_after = get_nov_native_account_asset_balance_with_store_path_v1(
                path.as_path(),
                caller.as_str(),
                "NOV",
            )
            .expect("load NOV balance");
            let neth_after = get_nov_native_account_asset_balance_with_store_path_v1(
                path.as_path(),
                caller.as_str(),
                "NETH",
            )
            .expect("load NETH balance");
            assert!(
                nov_after <= 500,
                "treasury redeem rejection must not mint or increase NOV"
            );
            assert_eq!(neth_after, 0);
            let state = load_nov_native_execution_store_v1(path.as_path())
                .expect("load native execution store");
            assert_eq!(
                state.module_state.treasury_reserves.get("NETH").copied(),
                Some(1_000)
            );
        });
    }

    #[test]
    fn neth_release_pause_blocks_treasury_redeem_without_neth_credit() {
        with_test_native_execution_store_path_v1(|path| {
            let caller = format!("0x{}", "9a".repeat(20));
            let mut pre = NovNativeExecutionStoreV1::default();
            credit_native_account_asset_balance_v1(&mut pre, caller.as_str(), "NOV", 500);
            pre.module_state
                .treasury_reserves
                .insert("NETH".to_string(), 1_000);
            pre.module_state.treasury_reserve_proofs.insert(
                "NETH".to_string(),
                NovTreasuryReserveProofV1 {
                    asset: "NETH".to_string(),
                    reserve_amount: 1_000,
                    proof_type: "custody_statement_v1".to_string(),
                    proof_digest: "0xnethreleasepauseredeem01".to_string(),
                    proof_source: "treasury_committee".to_string(),
                    proof_reference: "neth-release-pause-redeem-001".to_string(),
                    observed_at_unix_ms: 1,
                    expires_at_unix_ms: 0,
                    policy_version: 1,
                    policy_source: "governance_path".to_string(),
                    status: "active".to_string(),
                    automated_verification: false,
                    verification_mode: "manual_governance_attestation".to_string(),
                },
            );
            pre.module_state.mapped_lock_contract_address = format!("0x{}", "9a".repeat(20));
            pre.module_state.mapped_lock_min_confirmations = 21;
            pre.module_state.mapped_asset_release_paused = true;
            save_nov_native_execution_store_v1(path.as_path(), &pre)
                .expect("seed paused NETH release redeem risk");

            let request = NovExecutionRequestV1 {
                tx_hash: [0x9a; 32],
                chain_id: 8099,
                caller: vec![0x9a; 20],
                target: NovExecutionRequestTargetV1::NativeModule("treasury".to_string()),
                method: "redeem".to_string(),
                args: serde_json::to_vec(&serde_json::json!({
                    "asset_out": "NETH",
                    "nov_amount": 100u64
                }))
                .expect("encode args"),
                fee_pay_asset: "NOV".to_string(),
                fee_max_pay_amount: 500,
                fee_slippage_bps: 0,
                gas_like_limit: Some(80_000),
                nonce: 99,
            };
            let receipt = dispatch_and_persist_nov_execution_request_with_store_path_v1(
                path.as_path(),
                &request,
            )
            .expect("dispatch should return paused release NETH M2 risk failure receipt");
            assert!(!receipt.status);
            assert_eq!(receipt.module, "treasury");
            assert_eq!(receipt.method, "redeem");
            let failure = receipt.failure_reason.clone().unwrap_or_default();
            assert!(failure.starts_with("fee.settlement.m2_bridge_risk_blocked"));
            assert!(failure.contains("m2_bridge_risk=mapped_asset_release_paused"));

            let nov_after = get_nov_native_account_asset_balance_with_store_path_v1(
                path.as_path(),
                caller.as_str(),
                "NOV",
            )
            .expect("load NOV balance");
            let neth_after = get_nov_native_account_asset_balance_with_store_path_v1(
                path.as_path(),
                caller.as_str(),
                "NETH",
            )
            .expect("load NETH balance");
            assert!(
                nov_after <= 500,
                "treasury redeem rejection must not mint or increase NOV"
            );
            assert_eq!(neth_after, 0);
            let state = load_nov_native_execution_store_v1(path.as_path())
                .expect("load native execution store");
            assert_eq!(
                state.module_state.treasury_reserves.get("NETH").copied(),
                Some(1_000)
            );
        });
    }

    #[test]
    fn neth_burn_pause_blocks_treasury_redeem_without_neth_credit() {
        with_test_native_execution_store_path_v1(|path| {
            let caller = format!("0x{}", "9b".repeat(20));
            let mut pre = NovNativeExecutionStoreV1::default();
            credit_native_account_asset_balance_v1(&mut pre, caller.as_str(), "NOV", 500);
            pre.module_state
                .treasury_reserves
                .insert("NETH".to_string(), 1_000);
            pre.module_state.treasury_reserve_proofs.insert(
                "NETH".to_string(),
                NovTreasuryReserveProofV1 {
                    asset: "NETH".to_string(),
                    reserve_amount: 1_000,
                    proof_type: "custody_statement_v1".to_string(),
                    proof_digest: "0xnethburnpauseredeem01".to_string(),
                    proof_source: "treasury_committee".to_string(),
                    proof_reference: "neth-burn-pause-redeem-001".to_string(),
                    observed_at_unix_ms: 1,
                    expires_at_unix_ms: 0,
                    policy_version: 1,
                    policy_source: "governance_path".to_string(),
                    status: "active".to_string(),
                    automated_verification: false,
                    verification_mode: "manual_governance_attestation".to_string(),
                },
            );
            pre.module_state.mapped_lock_contract_address = format!("0x{}", "9b".repeat(20));
            pre.module_state.mapped_lock_min_confirmations = 21;
            pre.module_state.mapped_asset_burn_paused = true;
            save_nov_native_execution_store_v1(path.as_path(), &pre)
                .expect("seed paused NETH burn redeem risk");

            let request = NovExecutionRequestV1 {
                tx_hash: [0x9b; 32],
                chain_id: 8100,
                caller: vec![0x9b; 20],
                target: NovExecutionRequestTargetV1::NativeModule("treasury".to_string()),
                method: "redeem".to_string(),
                args: serde_json::to_vec(&serde_json::json!({
                    "asset_out": "NETH",
                    "nov_amount": 100u64
                }))
                .expect("encode args"),
                fee_pay_asset: "NOV".to_string(),
                fee_max_pay_amount: 500,
                fee_slippage_bps: 0,
                gas_like_limit: Some(80_000),
                nonce: 100,
            };
            let receipt = dispatch_and_persist_nov_execution_request_with_store_path_v1(
                path.as_path(),
                &request,
            )
            .expect("dispatch should return paused burn NETH M2 risk failure receipt");
            assert!(!receipt.status);
            assert_eq!(receipt.module, "treasury");
            assert_eq!(receipt.method, "redeem");
            let failure = receipt.failure_reason.clone().unwrap_or_default();
            assert!(failure.starts_with("fee.settlement.m2_bridge_risk_blocked"));
            assert!(failure.contains("m2_bridge_risk=mapped_asset_burn_paused"));

            let nov_after = get_nov_native_account_asset_balance_with_store_path_v1(
                path.as_path(),
                caller.as_str(),
                "NOV",
            )
            .expect("load NOV balance");
            let neth_after = get_nov_native_account_asset_balance_with_store_path_v1(
                path.as_path(),
                caller.as_str(),
                "NETH",
            )
            .expect("load NETH balance");
            assert!(
                nov_after <= 500,
                "treasury redeem rejection must not mint or increase NOV"
            );
            assert_eq!(neth_after, 0);
            let state = load_nov_native_execution_store_v1(path.as_path())
                .expect("load native execution store");
            assert_eq!(
                state.module_state.treasury_reserves.get("NETH").copied(),
                Some(1_000)
            );
        });
    }

    #[test]
    fn reserve_proof_amount_cap_blocks_redeem_that_keeps_reserve_over_cap() {
        with_test_native_execution_store_path_v1(|path| {
            let caller = format!("0x{}", "94".repeat(20));
            let mut pre = NovNativeExecutionStoreV1::default();
            credit_native_account_asset_balance_v1(&mut pre, caller.as_str(), "NOV", 500);
            pre.module_state
                .treasury_reserves
                .insert("USDT".to_string(), 1_000);
            pre.module_state
                .clearing_rate_ppm
                .insert("USDT".to_string(), 1_000_000);
            pre.module_state
                .protocol_clearing_nav_rate_ppm
                .insert("USDT".to_string(), 1_000_000);
            pre.module_state.treasury_reserve_proofs.insert(
                "USDT".to_string(),
                NovTreasuryReserveProofV1 {
                    asset: "USDT".to_string(),
                    reserve_amount: 500,
                    proof_type: "custody_statement_v1".to_string(),
                    proof_digest: "0xcap02".to_string(),
                    proof_source: "treasury_committee".to_string(),
                    proof_reference: "cap-report-002".to_string(),
                    observed_at_unix_ms: 1,
                    expires_at_unix_ms: 0,
                    policy_version: 1,
                    policy_source: "governance_path".to_string(),
                    status: "active".to_string(),
                    automated_verification: false,
                    verification_mode: "manual_governance_attestation".to_string(),
                },
            );
            save_nov_native_execution_store_v1(path.as_path(), &pre)
                .expect("seed low reserve proof cap");

            let request = NovExecutionRequestV1 {
                tx_hash: [0x94; 32],
                chain_id: 8094,
                caller: vec![0x94; 20],
                target: NovExecutionRequestTargetV1::NativeModule("treasury".to_string()),
                method: "redeem".to_string(),
                args: serde_json::to_vec(&serde_json::json!({
                    "asset_out": "USDT",
                    "nov_amount": 100u64
                }))
                .expect("encode args"),
                fee_pay_asset: "NOV".to_string(),
                fee_max_pay_amount: 500,
                fee_slippage_bps: 0,
                gas_like_limit: Some(80_000),
                nonce: 94,
            };
            let receipt = dispatch_and_persist_nov_execution_request_with_store_path_v1(
                path.as_path(),
                &request,
            )
            .expect("dispatch should return reserve proof capacity failure receipt");
            assert!(!receipt.status);
            assert_eq!(receipt.module, "treasury");
            assert_eq!(receipt.method, "redeem");
            let failure = receipt.failure_reason.clone().unwrap_or_default();
            assert!(failure.starts_with("fee.settlement.reserve_proof_capacity_exceeded"));
            assert!(failure.contains("proof_reserve_amount=500"));
            assert!(failure.contains("proof_reference=cap-report-002"));

            let nov_after = get_nov_native_account_asset_balance_with_store_path_v1(
                path.as_path(),
                caller.as_str(),
                "NOV",
            )
            .expect("load NOV balance");
            let usdt_after = get_nov_native_account_asset_balance_with_store_path_v1(
                path.as_path(),
                caller.as_str(),
                "USDT",
            )
            .expect("load USDT balance");
            assert_eq!(nov_after, 500);
            assert_eq!(usdt_after, 0);
            let state = load_nov_native_execution_store_v1(path.as_path())
                .expect("load native execution store");
            assert_eq!(
                state.module_state.treasury_reserves.get("USDT").copied(),
                Some(1_000)
            );
        });
    }

    #[test]
    fn amm_swap_exact_in_updates_balances_and_trace() {
        with_test_native_execution_store_path_v1(|path| {
            let caller = format!("0x{}", "51".repeat(20));
            let mut pre = NovNativeExecutionStoreV1::default();
            credit_native_account_asset_balance_v1(&mut pre, caller.as_str(), "USDT", 1_000);
            pre.module_state.clearing_static_amm_pools.insert(
                "usdt_nov_user_pool".to_string(),
                NovStaticAmmPoolStateV1 {
                    pool_id: "usdt_nov_user_pool".to_string(),
                    asset_x: "USDT".to_string(),
                    asset_y: "NOV".to_string(),
                    reserve_x: 1_000_000,
                    reserve_y: 2_000_000,
                    swap_fee_ppm: 3_000,
                    enabled: true,
                },
            );
            save_nov_native_execution_store_v1(path.as_path(), &pre).expect("seed amm state");

            let request = NovExecutionRequestV1 {
                tx_hash: [0xa2; 32],
                chain_id: 8022,
                caller: vec![0x51; 20],
                target: NovExecutionRequestTargetV1::NativeModule("amm".to_string()),
                method: "swap_exact_in".to_string(),
                args: serde_json::to_vec(&serde_json::json!({
                    "asset_in": "USDT",
                    "asset_out": "NOV",
                    "amount_in": 100u64,
                    "min_amount_out": 1u64,
                    "slippage_bps": 25u64
                }))
                .expect("encode args"),
                fee_pay_asset: "NOV".to_string(),
                fee_max_pay_amount: 500,
                fee_slippage_bps: 0,
                gas_like_limit: Some(90_000),
                nonce: 42,
            };
            let receipt = dispatch_and_persist_nov_execution_request_with_store_path_v1(
                path.as_path(),
                &request,
            )
            .expect("dispatch should succeed");
            assert!(
                receipt.status,
                "failure_reason={:?}",
                receipt.failure_reason
            );
            assert_eq!(receipt.module, "amm");
            assert_eq!(receipt.method, "swap_exact_in");

            let usdt_after = get_nov_native_account_asset_balance_with_store_path_v1(
                path.as_path(),
                caller.as_str(),
                "USDT",
            )
            .expect("load USDT balance");
            let nov_after = get_nov_native_account_asset_balance_with_store_path_v1(
                path.as_path(),
                caller.as_str(),
                "NOV",
            )
            .expect("load NOV balance");
            assert_eq!(usdt_after, 900);
            assert!(nov_after > 0);

            let trace = run_nov_native_call_from_params_with_store_path_v1(
                &serde_json::json!({
                    "target": {"kind": "native_module", "id": "treasury"},
                    "method": "get_execution_trace_by_tx",
                    "args": {"tx_hash": receipt.tx_hash}
                }),
                Some(path.as_path()),
            )
            .expect("trace lookup should succeed");
            assert_eq!(trace["found"].as_bool(), Some(true));
            assert_eq!(trace["result"]["final_status"].as_str(), Some("success"));
        });
    }

    #[test]
    fn amm_swap_exact_in_rejects_when_constrained_strategy_blocks_user_path() {
        with_test_native_execution_store_path_v1(|path| {
            let caller = format!("0x{}", "52".repeat(20));
            let mut pre = NovNativeExecutionStoreV1::default();
            credit_native_account_asset_balance_v1(&mut pre, caller.as_str(), "USDT", 1_000);
            pre.module_state.clearing_enabled = true;
            pre.module_state.clearing_require_healthy_risk_buffer = false;
            pre.module_state.clearing_constrained_strategy = "blocked".to_string();
            pre.module_state.treasury_min_reserve_bucket_nov = 100;
            pre.module_state.treasury_reserve_bucket_nov = 0;
            pre.module_state.treasury_min_risk_buffer_nov = 1;
            pre.module_state.treasury_risk_buffer_nov = 10;
            pre.module_state.clearing_static_amm_pools.insert(
                "usdt_nov_blocked_pool".to_string(),
                NovStaticAmmPoolStateV1 {
                    pool_id: "usdt_nov_blocked_pool".to_string(),
                    asset_x: "USDT".to_string(),
                    asset_y: "NOV".to_string(),
                    reserve_x: 1_000_000,
                    reserve_y: 1_000_000,
                    swap_fee_ppm: 3_000,
                    enabled: true,
                },
            );
            save_nov_native_execution_store_v1(path.as_path(), &pre).expect("seed blocked state");

            let request = NovExecutionRequestV1 {
                tx_hash: [0xa3; 32],
                chain_id: 8023,
                caller: vec![0x52; 20],
                target: NovExecutionRequestTargetV1::NativeModule("amm".to_string()),
                method: "swap_exact_in".to_string(),
                args: serde_json::to_vec(&serde_json::json!({
                    "asset_in": "USDT",
                    "asset_out": "NOV",
                    "amount_in": 100u64,
                    "min_amount_out": 1u64,
                    "slippage_bps": 25u64
                }))
                .expect("encode args"),
                fee_pay_asset: "NOV".to_string(),
                fee_max_pay_amount: 500,
                fee_slippage_bps: 0,
                gas_like_limit: Some(90_000),
                nonce: 43,
            };
            let receipt = dispatch_and_persist_nov_execution_request_with_store_path_v1(
                path.as_path(),
                &request,
            )
            .expect("dispatch should return blocked receipt");
            assert!(!receipt.status);
            assert_eq!(receipt.module, "amm");
            assert_eq!(receipt.method, "swap_exact_in");
            assert!(receipt
                .failure_reason
                .clone()
                .unwrap_or_default()
                .starts_with("fee.clearing.constrained_blocked"));
        });
    }

    #[test]
    fn credit_engine_open_vault_persists_vault_and_mints_debt_asset() {
        with_test_native_execution_store_path_v1(|path| {
            let caller = format!("0x{}", "53".repeat(20));
            let mut pre = NovNativeExecutionStoreV1::default();
            credit_native_account_asset_balance_v1(&mut pre, caller.as_str(), "ETH", 500);
            save_nov_native_execution_store_v1(path.as_path(), &pre)
                .expect("seed vault collateral state");

            let request = NovExecutionRequestV1 {
                tx_hash: [0xa4; 32],
                chain_id: 8024,
                caller: vec![0x53; 20],
                target: NovExecutionRequestTargetV1::NativeModule("credit_engine".to_string()),
                method: "open_vault".to_string(),
                args: serde_json::to_vec(&serde_json::json!({
                    "collateral_asset": "ETH",
                    "collateral_amount": 300u64,
                    "debt_asset": "NUSD",
                    "mint_amount": 100u64
                }))
                .expect("encode args"),
                fee_pay_asset: "NOV".to_string(),
                fee_max_pay_amount: 500,
                fee_slippage_bps: 0,
                gas_like_limit: Some(90_000),
                nonce: 44,
            };
            let receipt = dispatch_and_persist_nov_execution_request_with_store_path_v1(
                path.as_path(),
                &request,
            )
            .expect("dispatch should succeed");
            assert!(
                receipt.status,
                "failure_reason={:?}",
                receipt.failure_reason
            );
            assert_eq!(receipt.module, "credit_engine");
            assert_eq!(receipt.method, "open_vault");

            let store = load_nov_native_execution_store_v1(path.as_path())
                .expect("reload native execution store");
            assert_eq!(store.module_state.credit_vaults.len(), 1);
            let vault = store
                .module_state
                .credit_vaults
                .values()
                .next()
                .expect("vault should exist");
            assert_eq!(vault.owner, caller);
            assert_eq!(vault.collateral_asset, "ETH");
            assert_eq!(vault.collateral_amount, 300);
            assert_eq!(vault.debt_asset, "NUSD");
            assert_eq!(vault.debt_amount, 100);

            let eth_after = get_nov_native_account_asset_balance_with_store_path_v1(
                path.as_path(),
                caller.as_str(),
                "ETH",
            )
            .expect("load ETH balance");
            let nusd_after = get_nov_native_account_asset_balance_with_store_path_v1(
                path.as_path(),
                caller.as_str(),
                "NUSD",
            )
            .expect("load NUSD balance");
            assert_eq!(eth_after, 200);
            assert_eq!(nusd_after, 100);
        });
    }

    #[test]
    fn run_nov_send_transaction_from_params_v1_accepts_structured_tx_payload() {
        let tx_json = serde_json::json!({
            "chain_id": 99,
            "kind": {
                "Transfer": {
                    "from": [1,1,1,1,1,1,1,1,1,1,1,1,1,1,1,1,1,1,1,1],
                    "to": [2,2,2,2,2,2,2,2,2,2,2,2,2,2,2,2,2,2,2,2],
                    "asset": "NOV",
                    "amount": 123,
                    "nonce": 1,
                    "fee_policy": {
                        "pay_asset": "NOV",
                        "max_pay_amount": 1,
                        "slippage_bps": 50
                    }
                }
            },
            "signature": [0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0]
        });
        let out = run_nov_send_transaction_from_params_v1(&serde_json::json!({
            "tx": tx_json
        }))
        .expect("nov_sendTransaction should succeed");
        assert_eq!(out["accepted"].as_bool(), Some(true));
        assert_eq!(out["nov_tx_kind"].as_str(), Some("transfer"));
        assert_eq!(out["chain_id"].as_u64(), Some(99));
    }
}
