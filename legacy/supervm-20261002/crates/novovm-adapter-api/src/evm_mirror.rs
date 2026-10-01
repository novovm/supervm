#![forbid(unsafe_code)]

use anyhow::Result;
use serde::{Deserialize, Serialize};

use crate::{ChainAdapter, ChainType, TxIR};

/// Service role for an EVM mirror node running inside SUPERVM.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum EvmNodeServiceRole {
    Validator,
    ServiceProvider,
}

/// Fee settlement policy for EVM mirror income routing.
///
/// Semantics:
/// - Fees collected from EVM-facing services are accounted to a reserve account.
/// - Reserve value can be converted to the target payout token for the configured node account.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct EvmFeeSettlementPolicyV1 {
    pub reserve_currency_code: String,
    pub reserve_account: Vec<u8>,
    pub payout_token_code: String,
    pub payout_account: Vec<u8>,
}

impl Default for EvmFeeSettlementPolicyV1 {
    fn default() -> Self {
        Self {
            reserve_currency_code: "ETH".to_string(),
            reserve_account: Vec::new(),
            payout_token_code: "NOVO".to_string(),
            payout_account: Vec::new(),
        }
    }
}

/// In-memory ingress frame observed from EVM-facing transaction ingress.
///
/// This is intended for local in-process strategy execution without forcing a
/// secondary JSON-RPC round trip.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EvmMempoolIngressFrameV1 {
    pub chain_id: u64,
    pub tx_hash: Vec<u8>,
    pub raw_tx: Vec<u8>,
    pub parsed_tx: Option<TxIR>,
    pub observed_at_unix_ms: u64,
    #[serde(default)]
    pub overlay_route_id: String,
    #[serde(default)]
    pub overlay_route_epoch: u64,
    #[serde(default)]
    pub overlay_route_mask_bits: u8,
    #[serde(default)]
    pub overlay_route_mode: String,
    #[serde(default)]
    pub overlay_route_region: String,
    #[serde(default)]
    pub overlay_route_relay_bucket: u16,
    #[serde(default)]
    pub overlay_route_relay_set_size: u8,
    #[serde(default)]
    pub overlay_route_relay_round: u64,
    #[serde(default)]
    pub overlay_route_relay_index: u8,
    #[serde(default)]
    pub overlay_route_relay_id: String,
    #[serde(default)]
    pub overlay_route_strategy: String,
    #[serde(default)]
    pub overlay_route_hop_count: u8,
}

/// Cross-chain atomic intent submitted from the plugin side.
///
/// Intended semantics:
/// - Execute legs in-process where possible.
/// - Only broadcast when atomic checks pass.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AtomicCrossChainIntentV1 {
    pub intent_id: String,
    pub source_chain: ChainType,
    pub destination_chain: ChainType,
    pub ttl_unix_ms: u64,
    pub legs: Vec<TxIR>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum AtomicIntentStatus {
    Accepted,
    Executed,
    Rejected,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AtomicIntentReceiptV1 {
    pub intent_id: String,
    pub status: AtomicIntentStatus,
    pub reason: Option<String>,
}

/// Recorded fee income event for settlement.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct EvmFeeIncomeRecordV1 {
    pub chain_id: u64,
    pub tx_hash: Vec<u8>,
    pub fee_amount_wei: u128,
    pub collector_address: Vec<u8>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct EvmFeeSettlementResultV1 {
    pub reserve_delta: u128,
    pub payout_delta: u128,
    pub settlement_id: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct EvmFeeSettlementRecordV1 {
    pub income: EvmFeeIncomeRecordV1,
    pub result: EvmFeeSettlementResultV1,
    pub settled_at_unix_ms: u64,
}

/// Payout instruction generated from settlement records.
///
/// Host side can execute this as the minimal "income -> convert -> payout" loop.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct EvmFeePayoutInstructionV1 {
    pub settlement_id: String,
    pub chain_id: u64,
    pub income_tx_hash: Vec<u8>,
    pub reserve_currency_code: String,
    pub payout_token_code: String,
    pub reserve_delta_wei: u128,
    pub payout_delta_units: u128,
    pub reserve_account: Vec<u8>,
    pub payout_account: Vec<u8>,
    pub generated_at_unix_ms: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct EvmFeeSettlementSnapshotV1 {
    pub policy: EvmFeeSettlementPolicyV1,
    pub settlement_seq: u64,
    pub reserve_total_wei: u128,
    pub payout_total_units: u128,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AtomicBroadcastReadyV1 {
    pub intent: AtomicCrossChainIntentV1,
    pub ready_at_unix_ms: u64,
}

/// Optional extension interface for EVM mirror node mode.
///
/// This trait does not alter the base `ChainAdapter` contract and can be
/// implemented incrementally by EVM plugin backends.
pub trait EvmMirrorNodeAdapterExt: ChainAdapter {
    fn node_role(&self) -> EvmNodeServiceRole;

    fn set_fee_settlement_policy(&mut self, policy: EvmFeeSettlementPolicyV1) -> Result<()>;

    fn get_fee_settlement_policy(&self) -> Result<EvmFeeSettlementPolicyV1>;

    /// Drain up to `max_items` ingress frames from the in-memory queue.
    fn drain_mempool_ingress_frames(
        &mut self,
        max_items: usize,
    ) -> Result<Vec<EvmMempoolIngressFrameV1>>;

    /// Submit an atomic cross-chain intent for local pre-broadcast checks.
    fn submit_atomic_cross_chain_intent(
        &mut self,
        intent: &AtomicCrossChainIntentV1,
    ) -> Result<AtomicIntentReceiptV1>;

    /// Settle fee income according to the configured policy.
    fn settle_fee_income(
        &mut self,
        income: &EvmFeeIncomeRecordV1,
    ) -> Result<EvmFeeSettlementResultV1>;

    fn drain_fee_payout_instructions(
        &mut self,
        max_items: usize,
    ) -> Result<Vec<EvmFeePayoutInstructionV1>>;
}
