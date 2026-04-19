#![forbid(unsafe_code)]

use crate::clearing_router::{NovClearingRouterImplV1, NovClearingRouterV1};
use crate::clearing_types::{
    NovClearingFailureCodeV1, NovClearingRouteQuoteV1, NovExecutionFeeRequestV1,
    NovLastClearingRouteV1, NovReceiptRouteMetaV1, NovRouteSourceV1, NovStaticAmmPoolStateV1,
};
use crate::liquidity_sources::{StaticAmmPoolLiquidityV1, TreasuryDirectLiquidityV1};
use crate::treasury_settlement::settle_clearing_result_into_treasury_v1;
use anyhow::{bail, Context, Result};
use novovm_adapter_api::{TxIR, TxType};
use novovm_exec::{
    EncodedOpsWire, ExecOpV2, OpsWireOp, OpsWireV1Builder, RawIngressCodecRegistry,
    AOEM_OPS_WIRE_V1_MAGIC, AOEM_OPS_WIRE_V1_VERSION,
};
use novovm_network::{
    eth_rlpx_transaction_hash_v1, eth_rlpx_validate_transaction_envelope_payload_v1,
    observe_network_runtime_native_pending_tx_local_ingress_with_payload_v1,
    observe_network_runtime_native_pending_tx_rejected_v1,
};
use novovm_protocol::{
    decode_local_tx_wire_v1 as decode_tx_wire_v1, decode_nov_native_tx_wire_v1,
    encode_nov_native_tx_wire_v1, LocalTxWireV1, NovExecuteTxV1, NovExecutionModeV1,
    NovExecutionTargetV1, NovFeePolicyV1, NovGovernanceTxV1, NovNativeTxWireV1, NovPrivacyModeV1,
    NovTxKindV1, NovVerificationModeV1,
};
use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::OnceLock;

pub const LOCAL_TX_WIRE_V1_BYTES: usize = 4 + 1 + (8 * 5) + 32;
pub const NOV_NATIVE_GOVERNANCE_ALLOWLIST_ENV: &str = "NOVOVM_NATIVE_GOVERNANCE_PROPOSERS";
pub const NOV_NATIVE_GOVERNANCE_ENABLED_ENV: &str = "NOVOVM_NATIVE_GOVERNANCE_ENABLED";
pub const NOV_NATIVE_EXECUTION_STORE_ENV: &str = "NOVOVM_NATIVE_EXECUTION_STORE";
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
pub const NOV_NATIVE_CLEARING_ENABLED_ENV: &str = "NOVOVM_NATIVE_CLEARING_ENABLED";
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
const NOV_NATIVE_EXECUTION_STORE_SCHEMA_V1: &str = "novovm-native-execution-runtime/v1";
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
pub struct NovNativeExecutionLogV1 {
    pub module: String,
    pub method: String,
    pub event: String,
    pub data: serde_json::Value,
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct NovNativeExecutionReceiptV1 {
    pub tx_hash: String,
    pub status: bool,
    pub target: String,
    pub module: String,
    pub method: String,
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

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct NovTreasurySettlementJournalEntryV1 {
    #[serde(default)]
    pub seq: u64,
    pub unix_ms: u128,
    pub kind: String,
    pub tx_hash: String,
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
pub struct NovNativeExecutionModuleStateV1 {
    #[serde(default)]
    pub treasury_reserves: BTreeMap<String, u128>,
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
    pub credit_vaults: BTreeMap<u64, NovCreditVaultStateV1>,
    #[serde(default)]
    pub next_credit_vault_id: u64,
}

impl Default for NovNativeExecutionModuleStateV1 {
    fn default() -> Self {
        Self {
            treasury_reserves: BTreeMap::new(),
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
            last_fee_quote: None,
            last_fee_quote_failure: None,
            last_execution_trace: None,
            execution_traces_by_tx: BTreeMap::new(),
            execution_trace_order: Vec::new(),
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

pub fn tx_ingress_record_to_adapter_tx_ir(record: &TxIngressRecord, chain_id: u64) -> TxIR {
    let mut ir = TxIR {
        hash: Vec::new(),
        from: encode_adapter_address(record.account),
        to: Some(encode_adapter_address(record.key)),
        value: record.value as u128,
        gas_limit: 21_000,
        gas_price: record.fee,
        nonce: record.nonce,
        data: Vec::new(),
        signature: record.signature.to_vec(),
        chain_id,
        tx_type: TxType::Transfer,
        source_chain: None,
        target_chain: None,
    };
    ir.compute_hash();
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

fn to_hex(bytes: &[u8]) -> String {
    let mut out = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        out.push_str(&format!("{byte:02x}"));
    }
    out
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
            to: Some(transfer.to.clone()),
            value: transfer.amount,
            gas_limit: 21_000,
            gas_price: 1,
            nonce: transfer.nonce,
            data: transfer.asset.as_bytes().to_vec(),
            signature: tx.signature.to_vec(),
            chain_id: tx.chain_id,
            tx_type: TxType::Transfer,
            source_chain: None,
            target_chain: None,
        },
        NovTxKindV1::Execute(execute) => {
            let target_addr = pseudo_target_address_v1(&execute.target, &execute.method);
            TxIR {
                hash: Vec::new(),
                from: execute.caller.clone(),
                to: Some(target_addr),
                value: 0,
                gas_limit: execute.gas_like_limit.unwrap_or(300_000),
                gas_price: 1,
                nonce: execute.nonce,
                data: execute.args.clone(),
                signature: tx.signature.to_vec(),
                chain_id: tx.chain_id,
                tx_type: TxType::ContractCall,
                source_chain: None,
                target_chain: None,
            }
        }
        NovTxKindV1::Governance(governance) => TxIR {
            hash: Vec::new(),
            from: governance.proposer.clone(),
            to: None,
            value: 0,
            gas_limit: 80_000,
            gas_price: 1,
            nonce: governance.nonce,
            data: governance.payload.clone(),
            signature: tx.signature.to_vec(),
            chain_id: tx.chain_id,
            tx_type: TxType::Privacy,
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
    observe_network_runtime_native_pending_tx_local_ingress_with_payload_v1(
        native_tx.chain_id,
        tx_hash,
        None,
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

fn caller_account_ref_v1(request: &NovExecutionRequestV1) -> String {
    to_hex_prefixed_v1(request.caller.as_slice()).to_ascii_lowercase()
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

pub fn load_nov_native_execution_store_v1(path: &Path) -> Result<NovNativeExecutionStoreV1> {
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

pub fn save_nov_native_execution_store_v1(
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
        "nov_treasury_policy_v1:{}:{}:{}:{}:{}:{}:{}:{}:{}:{}:{}:{}:{}:{}:{}:{}",
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
        policy.clearing_constrained_strategy
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
        match resolve_fee_rate_ppm_with_source_v1(store, pay_asset.as_str(), now_ms) {
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
        let settlement_input =
            settle_clearing_result_into_treasury_v1(quote.quote_id.clone(), &result);

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
    store: &mut NovNativeExecutionStoreV1,
    now_ms: u128,
) -> Result<NovSettledFeeV1> {
    let quote = quote_fee_policy_from_execution_request_v1(request, store, now_ms)?;
    let tx_hash = to_hex(&request.tx_hash);
    settle_fee_quote_into_treasury_v1(store, &quote, tx_hash.as_str(), now_ms)
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
    }
}

fn build_success_native_receipt_v1(
    request: &NovExecutionRequestV1,
    settled_fee: &NovSettledFeeV1,
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
    let amount = args_json
        .get("amount")
        .or_else(|| args_json.get("nov_amount"))
        .and_then(parse_u128_from_json_value_v1)
        .unwrap_or(0);
    if amount == 0 {
        increment_settlement_failure_v1(store, "invalid_redeem_amount");
        return build_failed_native_receipt_v1(
            request,
            settled_fee,
            "treasury".to_string(),
            method_label.to_string(),
            fee_settlement_reason_v1("invalid_redeem_amount", "amount must be > 0"),
        );
    }

    if asset == "NOV" {
        let available = store.module_state.treasury_reserve_bucket_nov;
        if available < amount {
            increment_settlement_failure_v1(store, "insufficient_reserve");
            return build_failed_native_receipt_v1(
                request,
                settled_fee,
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
        let caller = caller_account_ref_v1(request);
        let caller_balance_after =
            credit_native_account_asset_balance_v1(store, caller.as_str(), "NOV", amount);
        let log = NovNativeExecutionLogV1 {
            module: "treasury".to_string(),
            method: method_label.to_string(),
            event: "treasury.reserve_redeemed".to_string(),
            data: serde_json::json!({
                "asset": "NOV",
                "amount": amount,
                "caller": caller,
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
            "treasury",
            method_label,
            vec![log],
        );
    }

    let available = store
        .module_state
        .treasury_reserves
        .get(asset.as_str())
        .copied()
        .unwrap_or(0);
    if available < amount {
        increment_settlement_failure_v1(store, "insufficient_reserve");
        return build_failed_native_receipt_v1(
            request,
            settled_fee,
            "treasury".to_string(),
            method_label.to_string(),
            fee_settlement_reason_v1(
                "insufficient_reserve",
                format!(
                    "asset={} requested={} available={}",
                    asset, amount, available
                )
                .as_str(),
            ),
        );
    }
    let reserve_after = {
        let entry = store
            .module_state
            .treasury_reserves
            .entry(asset.clone())
            .or_insert(0);
        *entry = entry.saturating_sub(amount);
        *entry
    };
    let redeemed_entry = store
        .module_state
        .treasury_redeemed_by_asset
        .entry(asset.clone())
        .or_insert(0);
    *redeemed_entry = redeemed_entry.saturating_add(amount);
    append_treasury_settlement_journal_v1(
        store,
        NovTreasurySettlementJournalEntryV1 {
            seq: 0,
            unix_ms: now_unix_millis_v1(),
            kind: "reserve_redeem".to_string(),
            tx_hash: to_hex(&request.tx_hash),
            source_asset: asset.clone(),
            source_amount: amount,
            settled_nov: 0,
            reserve_bucket_delta_nov: 0,
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
    let caller = caller_account_ref_v1(request);
    let caller_balance_after =
        credit_native_account_asset_balance_v1(store, caller.as_str(), asset.as_str(), amount);
    let log = NovNativeExecutionLogV1 {
        module: "treasury".to_string(),
        method: method_label.to_string(),
        event: "treasury.reserve_redeemed".to_string(),
        data: serde_json::json!({
            "asset": asset,
            "amount": amount,
            "caller": caller,
            "caller_balance_after": caller_balance_after,
            "reserve_after": reserve_after,
        }),
    };
    build_success_native_receipt_v1(request, settled_fee, "treasury", method_label, vec![log])
}

fn dispatch_amm_swap_exact_in_v1(
    request: &NovExecutionRequestV1,
    settled_fee: &NovSettledFeeV1,
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
            "amm".to_string(),
            "swap_exact_in".to_string(),
            "amm.route_unavailable: quoted output is zero".to_string(),
        );
    }
    if amount_out < min_amount_out {
        return build_failed_native_receipt_v1(
            request,
            settled_fee,
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
            "amm".to_string(),
            "swap_exact_in".to_string(),
            err.to_string(),
        );
    }
    let caller = caller_account_ref_v1(request);
    let caller_asset_in_before =
        native_account_asset_balance_v1(store, caller.as_str(), asset_in.as_str());
    if let Err(err) =
        debit_native_account_asset_balance_v1(store, caller.as_str(), asset_in.as_str(), amount_in)
    {
        return build_failed_native_receipt_v1(
            request,
            settled_fee,
            "amm".to_string(),
            "swap_exact_in".to_string(),
            format!("amm.insufficient_user_balance: {err}"),
        );
    }
    let caller_asset_out_after = credit_native_account_asset_balance_v1(
        store,
        caller.as_str(),
        asset_out.as_str(),
        amount_out,
    );
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
            "caller": caller,
            "pool_id": selected_pool_id,
            "asset_in": asset_in,
            "asset_out": asset_out,
            "amount_in": amount_in,
            "amount_out": amount_out,
            "min_amount_out": min_amount_out,
            "requested_slippage_bps": requested_slippage_bps,
            "nov_leg_amount": nov_leg_amount,
            "caller_asset_in_before": caller_asset_in_before,
            "caller_asset_in_after": native_account_asset_balance_v1(store, caller.as_str(), asset_in.as_str()),
            "caller_asset_out_after": caller_asset_out_after,
            "pool_reserve_out_before": reserve_out_before,
            "pool_reserve_x_after": pool.as_ref().map(|value| value.reserve_x).unwrap_or(0),
            "pool_reserve_y_after": pool.as_ref().map(|value| value.reserve_y).unwrap_or(0),
            "swap_fee_ppm": pool.as_ref().map(|value| value.swap_fee_ppm).unwrap_or(0),
        }),
    };
    build_success_native_receipt_v1(request, settled_fee, "amm", "swap_exact_in", vec![log])
}

fn dispatch_credit_engine_open_vault_v1(
    request: &NovExecutionRequestV1,
    settled_fee: &NovSettledFeeV1,
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
                "credit_engine".to_string(),
                "open_vault".to_string(),
                err.to_string(),
            );
        }
    }
    let caller = caller_account_ref_v1(request);
    if let Err(err) = debit_native_account_asset_balance_v1(
        store,
        caller.as_str(),
        collateral_asset.as_str(),
        collateral_amount,
    ) {
        return build_failed_native_receipt_v1(
            request,
            settled_fee,
            "credit_engine".to_string(),
            "open_vault".to_string(),
            format!("credit_engine.insufficient_user_balance: {err}"),
        );
    }
    let vault_id = store.module_state.next_credit_vault_id.saturating_add(1);
    store.module_state.next_credit_vault_id = vault_id;
    let vault = NovCreditVaultStateV1 {
        vault_id,
        owner: caller.clone(),
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
        credit_native_account_asset_balance_v1(
            store,
            caller.as_str(),
            debt_asset.as_str(),
            mint_amount,
        )
    } else {
        native_account_asset_balance_v1(store, caller.as_str(), debt_asset.as_str())
    };
    let log = NovNativeExecutionLogV1 {
        module: "credit_engine".to_string(),
        method: "open_vault".to_string(),
        event: "credit_engine.vault_opened".to_string(),
        data: serde_json::json!({
            "caller": caller,
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
        "credit_engine",
        "open_vault",
        vec![log],
    )
}

fn dispatch_native_module_execute_v1(
    request: &NovExecutionRequestV1,
    settled_fee: &NovSettledFeeV1,
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
                "treasury",
                "deposit_reserve",
                vec![log],
            )
        }
        ("treasury", "redeem") => {
            dispatch_treasury_redeem_v1(request, settled_fee, store, &args_json, "redeem")
        }
        ("treasury", "redeem_reserve") => {
            dispatch_treasury_redeem_v1(request, settled_fee, store, &args_json, "redeem_reserve")
        }
        ("amm", "swap_exact_in") => {
            dispatch_amm_swap_exact_in_v1(request, settled_fee, store, &args_json)
        }
        ("credit_engine", "open_vault") => {
            dispatch_credit_engine_open_vault_v1(request, settled_fee, store, &args_json)
        }
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
            }
        }
        ("governance", "apply_treasury_policy") => {
            if let Err(err) = governance_execute_authorized_v1(request, &args_json) {
                return build_failed_native_receipt_v1(
                    request,
                    settled_fee,
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
            store.module_state.clearing_require_healthy_risk_buffer =
                clearing_require_healthy_risk_buffer;
            store.module_state.clearing_daily_nov_hard_limit = clearing_daily_nov_hard_limit;
            store.module_state.clearing_constrained_max_slippage_bps =
                clearing_constrained_max_slippage_bps;
            store.module_state.clearing_constrained_daily_usage_bps =
                clearing_constrained_daily_usage_bps;
            store.module_state.clearing_constrained_strategy =
                clearing_constrained_strategy.clone();
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
                    "clearing_require_healthy_risk_buffer": clearing_require_healthy_risk_buffer,
                    "clearing_daily_nov_hard_limit": clearing_daily_nov_hard_limit,
                    "clearing_constrained_max_slippage_bps": clearing_constrained_max_slippage_bps,
                    "clearing_constrained_daily_usage_bps": clearing_constrained_daily_usage_bps,
                    "clearing_constrained_strategy": clearing_constrained_strategy,
                }),
            };
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
            }
        }
        _ => build_failed_native_receipt_v1(
            request,
            settled_fee,
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
    dispatch_and_persist_nov_execution_request_with_store_path_v1(path.as_path(), request)
}

pub fn dispatch_and_persist_nov_execution_request_with_store_path_v1(
    path: &Path,
    request: &NovExecutionRequestV1,
) -> Result<NovNativeExecutionReceiptV1> {
    let mut store = load_nov_native_execution_store_v1(path)?;
    let now_ms = now_unix_millis_v1();
    let settled_fee = match settle_fee_policy_from_execution_request_v1(request, &mut store, now_ms)
    {
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
                "fee".to_string(),
                fee_method.to_string(),
                reason,
            );
            store
                .receipts
                .insert(failed.tx_hash.clone(), failed.clone());
            let trace = build_execution_trace_v1(request, &unresolved_fee, &failed, &store, now_ms);
            persist_execution_trace_v1(&mut store, trace);
            store.last_updated_unix_ms = now_ms;
            save_nov_native_execution_store_v1(path, &store)?;
            return Ok(failed);
        }
    };
    let receipt = dispatch_native_module_execute_v1(request, &settled_fee, &mut store);
    store
        .receipts
        .insert(receipt.tx_hash.clone(), receipt.clone());
    let trace = build_execution_trace_v1(request, &settled_fee, &receipt, &store, now_ms);
    persist_execution_trace_v1(&mut store, trace);
    store.last_updated_unix_ms = now_ms;
    save_nov_native_execution_store_v1(path, &store)?;
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
                        "current_threshold_state": current_threshold_state.clone(),
                        "risk_buffer_status": risk_buffer_status,
                        "bucket_boundaries": bucket_boundaries.clone(),
                        "clearing_policy_gate": clearing_policy_gate.clone(),
                        "current_risk_buffer_nov": store.module_state.treasury_risk_buffer_nov,
                    },
                }),
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
                    let clearing_rate_ppm = store
                        .module_state
                        .clearing_rate_ppm
                        .get(asset.as_str())
                        .copied()
                        .unwrap_or_else(|| default_fee_rate_ppm_for_asset_v1(asset.as_str()));
                    serde_json::json!({
                        "method": "nov_call",
                        "target": "treasury",
                        "module_method": "get_clearing_liquidity",
                        "found": true,
                        "result": {
                            "asset": asset,
                            "available_nov": available_nov,
                            "clearing_rate_ppm": clearing_rate_ppm,
                        },
                    })
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
                        "oracle_source": if store.module_state.fee_oracle_source.trim().is_empty() {
                            "runtime_oracle".to_string()
                        } else {
                            store.module_state.fee_oracle_source.clone()
                        },
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
    let execution_request = nov_native_tx_to_execution_request_v1(&native_tx)?;
    let store_path_override = resolve_native_execution_store_path_from_params_v1(params);
    let execution_receipt = if let Some(request) = execution_request.as_ref() {
        Some(if let Some(path) = store_path_override.as_deref() {
            dispatch_and_persist_nov_execution_request_with_store_path_v1(path, request)?
        } else {
            dispatch_and_persist_nov_execution_request_v1(request)?
        })
    } else {
        None
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
        "native_receipt": execution_receipt,
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

fn parse_nov_privacy_mode_v1(raw: Option<&str>) -> NovPrivacyModeV1 {
    match raw.unwrap_or("public").to_ascii_lowercase().as_str() {
        "private" => NovPrivacyModeV1::Private,
        "confidential" => NovPrivacyModeV1::Confidential,
        _ => NovPrivacyModeV1::Public,
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
    let tx = NovNativeTxWireV1 {
        chain_id,
        kind: NovTxKindV1::Execute(NovExecuteTxV1 {
            caller,
            target,
            method,
            args,
            execution_mode,
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
        let _ = fs::remove_file(path);
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

    #[test]
    fn tx_ingress_record_maps_to_adapter_tx_ir_with_fee_and_signature() {
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
        assert_eq!(ir.signature, vec![0xab; 32]);
        assert_eq!(ir.from.len(), 20);
        assert_eq!(ir.to.as_ref().map(Vec::len), Some(20));
        assert!(!ir.hash.is_empty());
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
            assert_eq!(stored.logs.len(), 1);
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
            assert!(receipt.fee_price_source.contains("quote=runtime_oracle"));
            assert!(receipt
                .fee_price_source
                .contains("rate_source=runtime_oracle"));
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
            .expect("dispatch should return route unavailable receipt");
            assert!(!receipt.status);
            assert_eq!(receipt.module, "fee");
            assert_eq!(receipt.method, "settlement");
            assert!(receipt
                .failure_reason
                .clone()
                .unwrap_or_default()
                .starts_with("fee.clearing.route_unavailable"));
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
            assert_eq!(receipt.method, "settlement");
            assert!(receipt
                .failure_reason
                .clone()
                .unwrap_or_default()
                .starts_with("fee.clearing.slippage_exceeded"));
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
                    "clearing_constrained_max_slippage_bps": 25u64,
                    "clearing_constrained_daily_usage_bps": 7500u64,
                    "clearing_constrained_strategy": "treasury_direct_only"
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
        let err = settle_fee_quote_into_treasury_v1(&mut store, &quote, "deadbeef", 200)
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
