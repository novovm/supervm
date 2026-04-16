#![forbid(unsafe_code)]

use crate::eth_chain_config::{
    build_eth_fork_id_from_chain_config_v1, eth_chain_config_genesis_hash_v1,
    resolve_eth_chain_config_v1, EthChainConfigPeerValidationReasonV1,
};
use crate::eth_rlpx::{eth_rlpx_disconnect_reason_name_v1, EthRlpxStatusV1};
use crate::eth_runtime_config::{
    resolve_eth_fullnode_budget_hooks_v1, resolve_eth_fullnode_native_runtime_config_v1,
    EthFullnodeNativeRuntimeConfigV1,
};
use crate::runtime_status::{
    get_network_runtime_native_body_snapshot_v1, get_network_runtime_native_head_snapshot_v1,
    get_network_runtime_native_header_snapshot_v1, get_network_runtime_native_sync_status,
    get_network_runtime_sync_status, network_runtime_native_sync_is_active,
    plan_network_runtime_sync_pull_window, snapshot_network_runtime_native_canonical_chain_v1,
    NetworkRuntimeNativeBodySnapshotV1, NetworkRuntimeNativeCanonicalBlockStateV1,
    NetworkRuntimeNativeCanonicalChainStateV1, NetworkRuntimeNativeExecutionBudgetRuntimeSummaryV1,
    NetworkRuntimeNativeHeadSnapshotV1, NetworkRuntimeNativeHeaderSnapshotV1,
    NetworkRuntimeNativePendingTxBroadcastRuntimeSummaryV1, NetworkRuntimeNativePendingTxStateV1,
    NetworkRuntimeNativePendingTxSummaryV1, NetworkRuntimeNativeSyncPhaseV1,
    NetworkRuntimeNativeSyncStatusV1, NetworkRuntimeSyncStatus,
};
use novovm_protocol::{EvmNativeMessage, NodeId, ProtocolMessage};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fs;
use std::io::{Error as IoError, ErrorKind as IoErrorKind};
use std::path::{Path, PathBuf};
use std::sync::{Mutex, OnceLock};
use std::time::{SystemTime, UNIX_EPOCH};

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub enum EthWireVersion {
    V66,
    V67,
    V68,
    V69,
    V70,
}

impl EthWireVersion {
    #[must_use]
    pub fn as_u8(self) -> u8 {
        match self {
            Self::V66 => 66,
            Self::V67 => 67,
            Self::V68 => 68,
            Self::V69 => 69,
            Self::V70 => 70,
        }
    }

    #[must_use]
    pub fn as_str(self) -> &'static str {
        match self {
            Self::V66 => "eth/66",
            Self::V67 => "eth/67",
            Self::V68 => "eth/68",
            Self::V69 => "eth/69",
            Self::V70 => "eth/70",
        }
    }

    #[must_use]
    pub fn parse(raw: u8) -> Option<Self> {
        match raw {
            66 => Some(Self::V66),
            67 => Some(Self::V67),
            68 => Some(Self::V68),
            69 => Some(Self::V69),
            70 => Some(Self::V70),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum SnapWireVersion {
    V1,
}

impl SnapWireVersion {
    #[must_use]
    pub fn as_u8(self) -> u8 {
        match self {
            Self::V1 => 1,
        }
    }

    #[must_use]
    pub fn as_str(self) -> &'static str {
        match self {
            Self::V1 => "snap/1",
        }
    }

    #[must_use]
    pub fn parse(raw: u8) -> Option<Self> {
        match raw {
            1 => Some(Self::V1),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum EthFullnodeCanonicalQueryMethod {
    BlockNumber,
    GetBlockByNumber,
    GetBlockByHash,
    Syncing,
    GetTransactionReceipt,
    GetLogs,
}

impl EthFullnodeCanonicalQueryMethod {
    #[must_use]
    pub fn as_str(self) -> &'static str {
        match self {
            Self::BlockNumber => "eth_blockNumber",
            Self::GetBlockByNumber => "eth_getBlockByNumber",
            Self::GetBlockByHash => "eth_getBlockByHash",
            Self::Syncing => "eth_syncing",
            Self::GetTransactionReceipt => "eth_getTransactionReceipt",
            Self::GetLogs => "eth_getLogs",
        }
    }
}

pub const ETH_FULLNODE_CANONICAL_QUERY_METHODS: &[EthFullnodeCanonicalQueryMethod] = &[
    EthFullnodeCanonicalQueryMethod::BlockNumber,
    EthFullnodeCanonicalQueryMethod::GetBlockByNumber,
    EthFullnodeCanonicalQueryMethod::GetBlockByHash,
    EthFullnodeCanonicalQueryMethod::Syncing,
    EthFullnodeCanonicalQueryMethod::GetTransactionReceipt,
    EthFullnodeCanonicalQueryMethod::GetLogs,
];

#[must_use]
pub fn resolve_eth_fullnode_canonical_query_method(
    method: &str,
) -> Option<EthFullnodeCanonicalQueryMethod> {
    match method.trim() {
        "eth_blockNumber" => Some(EthFullnodeCanonicalQueryMethod::BlockNumber),
        "eth_getBlockByNumber" => Some(EthFullnodeCanonicalQueryMethod::GetBlockByNumber),
        "eth_getBlockByHash" => Some(EthFullnodeCanonicalQueryMethod::GetBlockByHash),
        "eth_syncing" => Some(EthFullnodeCanonicalQueryMethod::Syncing),
        "eth_getTransactionReceipt" => Some(EthFullnodeCanonicalQueryMethod::GetTransactionReceipt),
        "eth_getLogs" => Some(EthFullnodeCanonicalQueryMethod::GetLogs),
        _ => None,
    }
}

#[must_use]
pub fn is_eth_fullnode_canonical_query_method(method: &str) -> bool {
    resolve_eth_fullnode_canonical_query_method(method).is_some()
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum EthFullnodeRuntimeQueryMethod {
    NativeRuntimeConfig,
    NativeWorkerRuntime,
    NativeCanonicalChainSummary,
    NativeBlockLifecycleByNumber,
    NativeBlockLifecycleByHash,
    NativePendingTxSummary,
    NativePendingTxPropagationSummary,
    NativePendingTxByHash,
    NativePendingTxTombstones,
    NativePendingTxTombstoneByHash,
    NativePendingTxCleanupSummary,
    NativePendingTxBroadcastCandidates,
    NativePeerRuntimeState,
    NativeSyncRuntimeSummary,
    NativePeerHealthSummary,
    NativeSyncDegradationSummary,
    NativePeerSelectionSummary,
}

impl EthFullnodeRuntimeQueryMethod {
    #[must_use]
    pub fn as_str(self) -> &'static str {
        match self {
            Self::NativeRuntimeConfig => "supervm_getEthNativeRuntimeConfig",
            Self::NativeWorkerRuntime => "supervm_getEthNativeWorkerRuntime",
            Self::NativeCanonicalChainSummary => "supervm_getEthNativeCanonicalChainSummary",
            Self::NativeBlockLifecycleByNumber => "supervm_getEthNativeBlockLifecycleByNumber",
            Self::NativeBlockLifecycleByHash => "supervm_getEthNativeBlockLifecycleByHash",
            Self::NativePendingTxSummary => "supervm_getEthNativePendingTxSummary",
            Self::NativePendingTxPropagationSummary => {
                "supervm_getEthNativePendingTxPropagationSummary"
            }
            Self::NativePendingTxByHash => "supervm_getEthNativePendingTxByHash",
            Self::NativePendingTxTombstones => "supervm_getEthNativePendingTxTombstones",
            Self::NativePendingTxTombstoneByHash => "supervm_getEthNativePendingTxTombstoneByHash",
            Self::NativePendingTxCleanupSummary => "supervm_getEthNativePendingTxCleanupSummary",
            Self::NativePendingTxBroadcastCandidates => {
                "supervm_getEthNativePendingTxBroadcastCandidates"
            }
            Self::NativePeerRuntimeState => "supervm_getEthNativePeerRuntimeState",
            Self::NativeSyncRuntimeSummary => "supervm_getEthNativeSyncRuntimeSummary",
            Self::NativePeerHealthSummary => "supervm_getEthNativePeerHealthSummary",
            Self::NativeSyncDegradationSummary => "supervm_getEthNativeSyncDegradationSummary",
            Self::NativePeerSelectionSummary => "supervm_getEthNativePeerSelectionSummary",
        }
    }
}

pub const ETH_FULLNODE_RUNTIME_QUERY_METHODS: &[EthFullnodeRuntimeQueryMethod] = &[
    EthFullnodeRuntimeQueryMethod::NativeRuntimeConfig,
    EthFullnodeRuntimeQueryMethod::NativeWorkerRuntime,
    EthFullnodeRuntimeQueryMethod::NativeCanonicalChainSummary,
    EthFullnodeRuntimeQueryMethod::NativeBlockLifecycleByNumber,
    EthFullnodeRuntimeQueryMethod::NativeBlockLifecycleByHash,
    EthFullnodeRuntimeQueryMethod::NativePendingTxSummary,
    EthFullnodeRuntimeQueryMethod::NativePendingTxPropagationSummary,
    EthFullnodeRuntimeQueryMethod::NativePendingTxByHash,
    EthFullnodeRuntimeQueryMethod::NativePendingTxTombstones,
    EthFullnodeRuntimeQueryMethod::NativePendingTxTombstoneByHash,
    EthFullnodeRuntimeQueryMethod::NativePendingTxCleanupSummary,
    EthFullnodeRuntimeQueryMethod::NativePendingTxBroadcastCandidates,
    EthFullnodeRuntimeQueryMethod::NativePeerRuntimeState,
    EthFullnodeRuntimeQueryMethod::NativeSyncRuntimeSummary,
    EthFullnodeRuntimeQueryMethod::NativePeerHealthSummary,
    EthFullnodeRuntimeQueryMethod::NativeSyncDegradationSummary,
    EthFullnodeRuntimeQueryMethod::NativePeerSelectionSummary,
];

#[must_use]
pub fn resolve_eth_fullnode_runtime_query_method(
    method: &str,
) -> Option<EthFullnodeRuntimeQueryMethod> {
    match method.trim() {
        "supervm_getEthNativeRuntimeConfig" => {
            Some(EthFullnodeRuntimeQueryMethod::NativeRuntimeConfig)
        }
        "supervm_getEthNativeWorkerRuntime" => {
            Some(EthFullnodeRuntimeQueryMethod::NativeWorkerRuntime)
        }
        "supervm_getEthNativeCanonicalChainSummary" => {
            Some(EthFullnodeRuntimeQueryMethod::NativeCanonicalChainSummary)
        }
        "supervm_getEthNativeBlockLifecycleByNumber" => {
            Some(EthFullnodeRuntimeQueryMethod::NativeBlockLifecycleByNumber)
        }
        "supervm_getEthNativeBlockLifecycleByHash" => {
            Some(EthFullnodeRuntimeQueryMethod::NativeBlockLifecycleByHash)
        }
        "supervm_getEthNativePendingTxSummary" => {
            Some(EthFullnodeRuntimeQueryMethod::NativePendingTxSummary)
        }
        "supervm_getEthNativePendingTxPropagationSummary" => {
            Some(EthFullnodeRuntimeQueryMethod::NativePendingTxPropagationSummary)
        }
        "supervm_getEthNativePendingTxByHash" => {
            Some(EthFullnodeRuntimeQueryMethod::NativePendingTxByHash)
        }
        "supervm_getEthNativePendingTxTombstones" => {
            Some(EthFullnodeRuntimeQueryMethod::NativePendingTxTombstones)
        }
        "supervm_getEthNativePendingTxTombstoneByHash" => {
            Some(EthFullnodeRuntimeQueryMethod::NativePendingTxTombstoneByHash)
        }
        "supervm_getEthNativePendingTxCleanupSummary" => {
            Some(EthFullnodeRuntimeQueryMethod::NativePendingTxCleanupSummary)
        }
        "supervm_getEthNativePendingTxBroadcastCandidates" => {
            Some(EthFullnodeRuntimeQueryMethod::NativePendingTxBroadcastCandidates)
        }
        "supervm_getEthNativePeerRuntimeState" => {
            Some(EthFullnodeRuntimeQueryMethod::NativePeerRuntimeState)
        }
        "supervm_getEthNativeSyncRuntimeSummary" => {
            Some(EthFullnodeRuntimeQueryMethod::NativeSyncRuntimeSummary)
        }
        "supervm_getEthNativePeerHealthSummary" => {
            Some(EthFullnodeRuntimeQueryMethod::NativePeerHealthSummary)
        }
        "supervm_getEthNativeSyncDegradationSummary" => {
            Some(EthFullnodeRuntimeQueryMethod::NativeSyncDegradationSummary)
        }
        "supervm_getEthNativePeerSelectionSummary" => {
            Some(EthFullnodeRuntimeQueryMethod::NativePeerSelectionSummary)
        }
        _ => None,
    }
}

#[must_use]
pub fn is_eth_fullnode_runtime_query_method(method: &str) -> bool {
    resolve_eth_fullnode_runtime_query_method(method).is_some()
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum EthFullnodeBlockViewSource {
    CanonicalHostBatch,
    NativeChainSync,
}

impl EthFullnodeBlockViewSource {
    #[must_use]
    pub fn as_str(self) -> &'static str {
        match self {
            Self::CanonicalHostBatch => "canonical_host_batch",
            Self::NativeChainSync => "native_chain_sync",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum EthFullnodeSourceDowngradeReasonV1 {
    CanonicalProjectionOnly,
    NativeSyncActiveBlockObjectUnavailable,
    NoAvailableBlockSource,
}

impl EthFullnodeSourceDowngradeReasonV1 {
    #[must_use]
    pub fn as_str(self) -> &'static str {
        match self {
            Self::CanonicalProjectionOnly => "canonical_projection_only",
            Self::NativeSyncActiveBlockObjectUnavailable => {
                "native_sync_active_block_object_unavailable"
            }
            Self::NoAvailableBlockSource => "no_available_block_source",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct EthFullnodeBudgetHooksV1 {
    // Network isolation budget.
    pub native_recv_budget_per_tick: u64,
    pub sync_target_fanout: u64,
    pub rlpx_request_timeout_ms: u64,
    pub sync_request_interval_ms: u64,
    pub tx_broadcast_interval_ms: u64,
    pub tx_broadcast_max_per_tick: u64,
    pub tx_broadcast_max_propagations: u64,

    // Query isolation budget.
    pub runtime_query_result_max: u64,

    // Storage isolation budget.
    pub runtime_block_snapshot_limit: u64,
    pub runtime_pending_tx_snapshot_limit: u64,
    pub pending_tx_canonical_retain_depth: u64,
    pub pending_tx_reorg_return_window_ms: u64,
    pub pending_tx_ttl_ms: u64,
    pub pending_tx_no_success_attempt_limit: u64,
    pub pending_tx_tombstone_retention_max: u64,

    // Execution isolation budget.
    pub host_exec_budget_per_tick: u64,
    pub host_exec_time_slice_ms: u64,
    pub host_exec_target_per_tick: u64,
    pub host_exec_target_time_slice_ms: u64,

    pub sync_pull_headers_batch: u64,
    pub sync_pull_bodies_batch: u64,
    pub sync_pull_state_batch: u64,
    pub sync_pull_finalize_batch: u64,
    pub sync_decode_concurrency: u64,
    pub sync_apply_concurrency: u64,
    pub native_block_store_flush_batch: u64,
    pub block_query_scan_max: u64,
    pub active_native_peer_soft_limit: u64,
    pub active_native_peer_hard_limit: u64,
}

pub const ETH_FULLNODE_DEFAULT_NATIVE_RECV_BUDGET_PER_TICK: u64 = 128;
pub const ETH_FULLNODE_DEFAULT_SYNC_TARGET_FANOUT: u64 = 1;
pub const ETH_FULLNODE_DEFAULT_RLPX_REQUEST_TIMEOUT_MS: u64 = 5_000;
pub const ETH_FULLNODE_DEFAULT_SYNC_REQUEST_INTERVAL_MS: u64 = 1_000;
pub const ETH_FULLNODE_DEFAULT_TX_BROADCAST_INTERVAL_MS: u64 = 1_000;
pub const ETH_FULLNODE_DEFAULT_TX_BROADCAST_MAX_PER_TICK: u64 = 8;
pub const ETH_FULLNODE_DEFAULT_TX_BROADCAST_MAX_PROPAGATIONS: u64 = 3;
pub const ETH_FULLNODE_DEFAULT_RUNTIME_QUERY_RESULT_MAX: u64 = 2_048;
pub const ETH_FULLNODE_DEFAULT_RUNTIME_BLOCK_SNAPSHOT_LIMIT: u64 = 1_024;
pub const ETH_FULLNODE_DEFAULT_RUNTIME_PENDING_TX_SNAPSHOT_LIMIT: u64 = 2_048;
pub const ETH_FULLNODE_DEFAULT_PENDING_TX_CANONICAL_RETAIN_DEPTH: u64 = 128;
pub const ETH_FULLNODE_DEFAULT_PENDING_TX_REORG_RETURN_WINDOW_MS: u64 = 15 * 60 * 1_000;
pub const ETH_FULLNODE_DEFAULT_PENDING_TX_TTL_MS: u64 = 30 * 60 * 1_000;
pub const ETH_FULLNODE_DEFAULT_PENDING_TX_NO_SUCCESS_ATTEMPT_LIMIT: u64 = 12;
pub const ETH_FULLNODE_DEFAULT_PENDING_TX_TOMBSTONE_RETENTION_MAX: u64 = 4_096;
pub const ETH_FULLNODE_DEFAULT_HOST_EXEC_BUDGET_PER_TICK: u64 = 64;
pub const ETH_FULLNODE_DEFAULT_HOST_EXEC_TIME_SLICE_MS: u64 = 10;
pub const ETH_FULLNODE_DEFAULT_HOST_EXEC_TARGET_PER_TICK: u64 =
    ETH_FULLNODE_DEFAULT_HOST_EXEC_BUDGET_PER_TICK;
pub const ETH_FULLNODE_DEFAULT_HOST_EXEC_TARGET_TIME_SLICE_MS: u64 =
    ETH_FULLNODE_DEFAULT_HOST_EXEC_TIME_SLICE_MS;

pub const ETH_FULLNODE_DEFAULT_SYNC_PULL_HEADERS_BATCH: u64 = 2_048;
pub const ETH_FULLNODE_DEFAULT_SYNC_PULL_BODIES_BATCH: u64 = 256;
pub const ETH_FULLNODE_DEFAULT_SYNC_PULL_STATE_BATCH: u64 = 64;
pub const ETH_FULLNODE_DEFAULT_SYNC_PULL_FINALIZE_BATCH: u64 = 16;
pub const ETH_FULLNODE_DEFAULT_SYNC_DECODE_CONCURRENCY: u64 = 1;
pub const ETH_FULLNODE_DEFAULT_SYNC_APPLY_CONCURRENCY: u64 = 1;
pub const ETH_FULLNODE_DEFAULT_NATIVE_BLOCK_STORE_FLUSH_BATCH: u64 = 64;
pub const ETH_FULLNODE_DEFAULT_BLOCK_QUERY_SCAN_MAX: u64 = 2_048;
pub const ETH_FULLNODE_DEFAULT_ACTIVE_NATIVE_PEER_SOFT_LIMIT: u64 = 8;
pub const ETH_FULLNODE_DEFAULT_ACTIVE_NATIVE_PEER_HARD_LIMIT: u64 = 16;

impl Default for EthFullnodeBudgetHooksV1 {
    fn default() -> Self {
        Self {
            native_recv_budget_per_tick: ETH_FULLNODE_DEFAULT_NATIVE_RECV_BUDGET_PER_TICK,
            sync_target_fanout: ETH_FULLNODE_DEFAULT_SYNC_TARGET_FANOUT,
            rlpx_request_timeout_ms: ETH_FULLNODE_DEFAULT_RLPX_REQUEST_TIMEOUT_MS,
            sync_request_interval_ms: ETH_FULLNODE_DEFAULT_SYNC_REQUEST_INTERVAL_MS,
            tx_broadcast_interval_ms: ETH_FULLNODE_DEFAULT_TX_BROADCAST_INTERVAL_MS,
            tx_broadcast_max_per_tick: ETH_FULLNODE_DEFAULT_TX_BROADCAST_MAX_PER_TICK,
            tx_broadcast_max_propagations: ETH_FULLNODE_DEFAULT_TX_BROADCAST_MAX_PROPAGATIONS,
            runtime_query_result_max: ETH_FULLNODE_DEFAULT_RUNTIME_QUERY_RESULT_MAX,
            runtime_block_snapshot_limit: ETH_FULLNODE_DEFAULT_RUNTIME_BLOCK_SNAPSHOT_LIMIT,
            runtime_pending_tx_snapshot_limit:
                ETH_FULLNODE_DEFAULT_RUNTIME_PENDING_TX_SNAPSHOT_LIMIT,
            pending_tx_canonical_retain_depth:
                ETH_FULLNODE_DEFAULT_PENDING_TX_CANONICAL_RETAIN_DEPTH,
            pending_tx_reorg_return_window_ms:
                ETH_FULLNODE_DEFAULT_PENDING_TX_REORG_RETURN_WINDOW_MS,
            pending_tx_ttl_ms: ETH_FULLNODE_DEFAULT_PENDING_TX_TTL_MS,
            pending_tx_no_success_attempt_limit:
                ETH_FULLNODE_DEFAULT_PENDING_TX_NO_SUCCESS_ATTEMPT_LIMIT,
            pending_tx_tombstone_retention_max:
                ETH_FULLNODE_DEFAULT_PENDING_TX_TOMBSTONE_RETENTION_MAX,
            host_exec_budget_per_tick: ETH_FULLNODE_DEFAULT_HOST_EXEC_BUDGET_PER_TICK,
            host_exec_time_slice_ms: ETH_FULLNODE_DEFAULT_HOST_EXEC_TIME_SLICE_MS,
            host_exec_target_per_tick: ETH_FULLNODE_DEFAULT_HOST_EXEC_TARGET_PER_TICK,
            host_exec_target_time_slice_ms: ETH_FULLNODE_DEFAULT_HOST_EXEC_TARGET_TIME_SLICE_MS,
            sync_pull_headers_batch: ETH_FULLNODE_DEFAULT_SYNC_PULL_HEADERS_BATCH,
            sync_pull_bodies_batch: ETH_FULLNODE_DEFAULT_SYNC_PULL_BODIES_BATCH,
            sync_pull_state_batch: ETH_FULLNODE_DEFAULT_SYNC_PULL_STATE_BATCH,
            sync_pull_finalize_batch: ETH_FULLNODE_DEFAULT_SYNC_PULL_FINALIZE_BATCH,
            sync_decode_concurrency: ETH_FULLNODE_DEFAULT_SYNC_DECODE_CONCURRENCY,
            sync_apply_concurrency: ETH_FULLNODE_DEFAULT_SYNC_APPLY_CONCURRENCY,
            native_block_store_flush_batch: ETH_FULLNODE_DEFAULT_NATIVE_BLOCK_STORE_FLUSH_BATCH,
            block_query_scan_max: ETH_FULLNODE_DEFAULT_BLOCK_QUERY_SCAN_MAX,
            active_native_peer_soft_limit: ETH_FULLNODE_DEFAULT_ACTIVE_NATIVE_PEER_SOFT_LIMIT,
            active_native_peer_hard_limit: ETH_FULLNODE_DEFAULT_ACTIVE_NATIVE_PEER_HARD_LIMIT,
        }
    }
}

#[must_use]
pub fn default_eth_fullnode_budget_hooks_v1() -> EthFullnodeBudgetHooksV1 {
    EthFullnodeBudgetHooksV1::default()
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct EthNativeBlockHeaderV1 {
    pub chain_id: u64,
    pub number: u64,
    pub hash: [u8; 32],
    pub parent_hash: [u8; 32],
    pub state_root: [u8; 32],
    pub transactions_root: [u8; 32],
    pub receipts_root: [u8; 32],
    pub ommers_hash: [u8; 32],
    pub logs_bloom: Vec<u8>,
    pub gas_limit: Option<u64>,
    pub gas_used: Option<u64>,
    pub timestamp: Option<u64>,
    pub base_fee_per_gas: Option<u128>,
    pub withdrawals_root: Option<[u8; 32]>,
    pub blob_gas_used: Option<u64>,
    pub excess_blob_gas: Option<u64>,
    pub source_peer_id: Option<u64>,
    pub observed_unix_ms: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct EthNativeBlockBodyV1 {
    pub tx_hashes: Vec<[u8; 32]>,
    pub tx_count: usize,
    pub ommer_hashes: Vec<[u8; 32]>,
    pub withdrawal_count: Option<usize>,
    pub body_available: bool,
    pub txs_materialized: bool,
    pub observed_unix_ms: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct EthNativeBlockObjectV1 {
    pub header: EthNativeBlockHeaderV1,
    pub body: Option<EthNativeBlockBodyV1>,
    pub canonical: bool,
    pub safe: bool,
    pub finalized: bool,
    pub reorg_depth_hint: Option<u64>,
    pub source_peer_count: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct EthFullnodeSourcePriorityPolicyV1 {
    pub head_source: EthFullnodeBlockViewSource,
    pub block_object_source: EthFullnodeBlockViewSource,
    pub sync_source: EthFullnodeBlockViewSource,
    pub downgrade_reason: Option<EthFullnodeSourceDowngradeReasonV1>,
    pub budget_hooks: EthFullnodeBudgetHooksV1,
}

fn eth_fullnode_source_priority_chain_id_v1(
    chain_view: Option<&EthFullnodeChainViewV1>,
    native_head_block: Option<&EthNativeBlockObjectV1>,
    native_canonical_chain: Option<&NetworkRuntimeNativeCanonicalChainStateV1>,
    runtime_native_sync: Option<NetworkRuntimeNativeSyncStatusV1>,
) -> Option<u64> {
    let _ = runtime_native_sync;
    native_canonical_chain
        .and_then(|chain| chain.head.as_ref().map(|head| head.chain_id))
        .or_else(|| native_head_block.map(|block| block.header.chain_id))
        .or_else(|| chain_view.map(|view| view.chain_id))
}

#[must_use]
pub fn derive_eth_fullnode_source_priority_policy_v1(
    chain_view: Option<&EthFullnodeChainViewV1>,
    native_head_block: Option<&EthNativeBlockObjectV1>,
    native_canonical_chain: Option<&NetworkRuntimeNativeCanonicalChainStateV1>,
    runtime_native_sync: Option<NetworkRuntimeNativeSyncStatusV1>,
) -> EthFullnodeSourcePriorityPolicyV1 {
    let budget_hooks = eth_fullnode_source_priority_chain_id_v1(
        chain_view,
        native_head_block,
        native_canonical_chain,
        runtime_native_sync,
    )
    .map(resolve_eth_fullnode_budget_hooks_v1)
    .unwrap_or_else(default_eth_fullnode_budget_hooks_v1);
    if native_canonical_chain
        .and_then(|chain| chain.head.as_ref())
        .is_some()
        || native_head_block.is_some()
    {
        return EthFullnodeSourcePriorityPolicyV1 {
            head_source: EthFullnodeBlockViewSource::NativeChainSync,
            block_object_source: EthFullnodeBlockViewSource::NativeChainSync,
            sync_source: EthFullnodeBlockViewSource::NativeChainSync,
            downgrade_reason: None,
            budget_hooks,
        };
    }

    if runtime_native_sync
        .as_ref()
        .is_some_and(network_runtime_native_sync_is_active)
    {
        let fallback_source = chain_view
            .map(|view| view.source)
            .unwrap_or(EthFullnodeBlockViewSource::CanonicalHostBatch);
        return EthFullnodeSourcePriorityPolicyV1 {
            head_source: fallback_source,
            block_object_source: fallback_source,
            sync_source: EthFullnodeBlockViewSource::NativeChainSync,
            downgrade_reason: Some(
                EthFullnodeSourceDowngradeReasonV1::NativeSyncActiveBlockObjectUnavailable,
            ),
            budget_hooks,
        };
    }

    if let Some(chain_view) = chain_view {
        let downgrade_reason = if matches!(
            chain_view.source,
            EthFullnodeBlockViewSource::NativeChainSync
        ) {
            None
        } else {
            Some(EthFullnodeSourceDowngradeReasonV1::CanonicalProjectionOnly)
        };
        return EthFullnodeSourcePriorityPolicyV1 {
            head_source: chain_view.source,
            block_object_source: chain_view.source,
            sync_source: chain_view.source,
            downgrade_reason,
            budget_hooks,
        };
    }

    EthFullnodeSourcePriorityPolicyV1 {
        head_source: EthFullnodeBlockViewSource::CanonicalHostBatch,
        block_object_source: EthFullnodeBlockViewSource::CanonicalHostBatch,
        sync_source: EthFullnodeBlockViewSource::CanonicalHostBatch,
        downgrade_reason: Some(EthFullnodeSourceDowngradeReasonV1::NoAvailableBlockSource),
        budget_hooks,
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct EthFullnodeBlockContextV1 {
    pub source: EthFullnodeBlockViewSource,
    pub chain_id: u64,
    pub block_number: u64,
    pub canonical_batch_seq: Option<u64>,
    pub block_hash: [u8; 32],
    pub parent_block_hash: [u8; 32],
    pub state_root: [u8; 32],
    pub state_version: u64,
    pub tx_count: usize,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct EthFullnodeChainViewV1 {
    pub source: EthFullnodeBlockViewSource,
    pub chain_id: u64,
    pub starting_block_number: u64,
    pub current_block_number: u64,
    pub highest_block_number: u64,
    pub current_block_hash: [u8; 32],
    pub parent_block_hash: [u8; 32],
    pub current_state_root: [u8; 32],
    pub current_state_version: u64,
    pub total_blocks: usize,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct EthFullnodeHeadViewV1 {
    pub source: EthFullnodeBlockViewSource,
    pub chain_id: u64,
    pub block_number: u64,
    pub block_hash: [u8; 32],
    pub parent_block_hash: [u8; 32],
    pub state_root: [u8; 32],
    pub state_version: u64,
    pub source_priority_policy: EthFullnodeSourcePriorityPolicyV1,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct EthFullnodeSyncViewV1 {
    pub source: EthFullnodeBlockViewSource,
    pub chain_id: u64,
    pub peer_count: u64,
    pub starting_block_number: u64,
    pub current_block_number: u64,
    pub highest_block_number: u64,
    pub current_block_hash: [u8; 32],
    pub parent_block_hash: [u8; 32],
    pub current_state_root: [u8; 32],
    pub current_state_version: u64,
    pub native_sync_phase: Option<String>,
    pub syncing: bool,
    pub source_priority_policy: EthFullnodeSourcePriorityPolicyV1,
}

#[must_use]
pub fn derive_eth_fullnode_chain_view_v1(
    blocks: &[EthFullnodeBlockContextV1],
) -> Option<EthFullnodeChainViewV1> {
    let first = blocks.first()?;
    let latest = blocks.last()?;
    Some(EthFullnodeChainViewV1 {
        source: latest.source,
        chain_id: latest.chain_id,
        starting_block_number: first.block_number,
        current_block_number: latest.block_number,
        highest_block_number: latest.block_number,
        current_block_hash: latest.block_hash,
        parent_block_hash: latest.parent_block_hash,
        current_state_root: latest.state_root,
        current_state_version: latest.state_version,
        total_blocks: blocks.len(),
    })
}

#[must_use]
pub fn derive_eth_fullnode_chain_view_from_native_block_v1(
    native_head_block: &EthNativeBlockObjectV1,
    starting_block_number: u64,
    total_blocks: usize,
) -> EthFullnodeChainViewV1 {
    EthFullnodeChainViewV1 {
        source: EthFullnodeBlockViewSource::NativeChainSync,
        chain_id: native_head_block.header.chain_id,
        starting_block_number: starting_block_number.min(native_head_block.header.number),
        current_block_number: native_head_block.header.number,
        highest_block_number: native_head_block.header.number,
        current_block_hash: native_head_block.header.hash,
        parent_block_hash: native_head_block.header.parent_hash,
        current_state_root: native_head_block.header.state_root,
        current_state_version: 0,
        total_blocks,
    }
}

#[must_use]
pub fn derive_eth_native_block_object_from_runtime_snapshots_v1(
    head_snapshot: &NetworkRuntimeNativeHeadSnapshotV1,
    header_snapshot: &NetworkRuntimeNativeHeaderSnapshotV1,
    body_snapshot: Option<&NetworkRuntimeNativeBodySnapshotV1>,
) -> Option<EthNativeBlockObjectV1> {
    if head_snapshot.chain_id != header_snapshot.chain_id
        || head_snapshot.block_number != header_snapshot.number
        || head_snapshot.block_hash != header_snapshot.hash
        || head_snapshot.parent_block_hash != header_snapshot.parent_hash
        || head_snapshot.state_root != header_snapshot.state_root
    {
        return None;
    }
    let body = match body_snapshot {
        Some(body)
            if body.chain_id == header_snapshot.chain_id
                && body.number == header_snapshot.number
                && body.block_hash == header_snapshot.hash =>
        {
            Some(EthNativeBlockBodyV1 {
                tx_hashes: body.tx_hashes.clone(),
                tx_count: body.tx_hashes.len(),
                ommer_hashes: body.ommer_hashes.clone(),
                withdrawal_count: body.withdrawal_count,
                body_available: body.body_available,
                txs_materialized: body.txs_materialized,
                observed_unix_ms: body.observed_unix_ms as u64,
            })
        }
        Some(_) => return None,
        None => None,
    };

    Some(EthNativeBlockObjectV1 {
        header: EthNativeBlockHeaderV1 {
            chain_id: header_snapshot.chain_id,
            number: header_snapshot.number,
            hash: header_snapshot.hash,
            parent_hash: header_snapshot.parent_hash,
            state_root: header_snapshot.state_root,
            transactions_root: header_snapshot.transactions_root,
            receipts_root: header_snapshot.receipts_root,
            ommers_hash: header_snapshot.ommers_hash,
            logs_bloom: header_snapshot.logs_bloom.clone(),
            gas_limit: header_snapshot.gas_limit,
            gas_used: header_snapshot.gas_used,
            timestamp: header_snapshot.timestamp,
            base_fee_per_gas: header_snapshot.base_fee_per_gas,
            withdrawals_root: header_snapshot.withdrawals_root,
            blob_gas_used: header_snapshot.blob_gas_used,
            excess_blob_gas: header_snapshot.excess_blob_gas,
            source_peer_id: header_snapshot
                .source_peer_id
                .or(head_snapshot.source_peer_id),
            observed_unix_ms: header_snapshot.observed_unix_ms as u64,
        },
        body,
        canonical: head_snapshot.canonical,
        safe: head_snapshot.safe,
        finalized: head_snapshot.finalized,
        reorg_depth_hint: head_snapshot.reorg_depth_hint,
        source_peer_count: head_snapshot.peer_count,
    })
}

#[must_use]
pub fn snapshot_eth_fullnode_native_head_block_object_v1(
    chain_id: u64,
) -> Option<EthNativeBlockObjectV1> {
    let head_snapshot = get_network_runtime_native_head_snapshot_v1(chain_id)?;
    let header_snapshot = get_network_runtime_native_header_snapshot_v1(chain_id)?;
    let body_snapshot = get_network_runtime_native_body_snapshot_v1(chain_id);
    derive_eth_native_block_object_from_runtime_snapshots_v1(
        &head_snapshot,
        &header_snapshot,
        body_snapshot.as_ref(),
    )
}

#[must_use]
pub fn derive_eth_fullnode_chain_view_with_native_preference_v1(
    fallback_chain_view: Option<&EthFullnodeChainViewV1>,
    native_head_block: Option<&EthNativeBlockObjectV1>,
    native_canonical_chain: Option<&NetworkRuntimeNativeCanonicalChainStateV1>,
) -> Option<EthFullnodeChainViewV1> {
    if let Some(native_canonical_chain) = native_canonical_chain {
        if let Some(head) = native_canonical_chain.head.as_ref() {
            let canonical_block_count = native_canonical_chain.canonical_block_count.max(1);
            let starting_block_number = fallback_chain_view
                .map(|view| view.starting_block_number)
                .unwrap_or_else(|| {
                    head.number
                        .saturating_add(1)
                        .saturating_sub(canonical_block_count as u64)
                });
            let total_blocks = fallback_chain_view
                .map(|view| view.total_blocks)
                .unwrap_or(native_canonical_chain.canonical_block_count.max(1));
            return Some(EthFullnodeChainViewV1 {
                source: EthFullnodeBlockViewSource::NativeChainSync,
                chain_id: head.chain_id,
                starting_block_number: starting_block_number.min(head.number),
                current_block_number: head.number,
                highest_block_number: head.number,
                current_block_hash: head.hash,
                parent_block_hash: head.parent_hash,
                current_state_root: head.state_root,
                current_state_version: 0,
                total_blocks,
            });
        }
    }
    if let Some(native_head_block) = native_head_block {
        let starting_block_number = fallback_chain_view
            .map(|view| view.starting_block_number)
            .unwrap_or(native_head_block.header.number);
        let total_blocks = fallback_chain_view
            .map(|view| view.total_blocks)
            .unwrap_or(1);
        return Some(derive_eth_fullnode_chain_view_from_native_block_v1(
            native_head_block,
            starting_block_number,
            total_blocks,
        ));
    }
    fallback_chain_view.cloned()
}

#[must_use]
pub fn derive_eth_fullnode_head_view_with_native_preference_v1(
    fallback_chain_view: Option<&EthFullnodeChainViewV1>,
    native_head_block: Option<&EthNativeBlockObjectV1>,
    native_canonical_chain: Option<&NetworkRuntimeNativeCanonicalChainStateV1>,
    runtime_native_sync: Option<NetworkRuntimeNativeSyncStatusV1>,
) -> Option<EthFullnodeHeadViewV1> {
    let prioritized = derive_eth_fullnode_chain_view_with_native_preference_v1(
        fallback_chain_view,
        native_head_block,
        native_canonical_chain,
    )?;
    let source_priority_policy = derive_eth_fullnode_source_priority_policy_v1(
        Some(&prioritized),
        native_head_block,
        native_canonical_chain,
        runtime_native_sync,
    );
    Some(EthFullnodeHeadViewV1 {
        source: prioritized.source,
        chain_id: prioritized.chain_id,
        block_number: prioritized.current_block_number,
        block_hash: prioritized.current_block_hash,
        parent_block_hash: prioritized.parent_block_hash,
        state_root: prioritized.current_state_root,
        state_version: prioritized.current_state_version,
        source_priority_policy,
    })
}

#[must_use]
pub fn derive_eth_fullnode_head_view_v1(
    chain_view: &EthFullnodeChainViewV1,
) -> EthFullnodeHeadViewV1 {
    derive_eth_fullnode_head_view_with_native_preference_v1(Some(chain_view), None, None, None)
        .expect("chain view must produce head view")
}

#[must_use]
pub fn derive_eth_fullnode_sync_view_with_native_preference_v1(
    fallback_chain_view: Option<&EthFullnodeChainViewV1>,
    native_head_block: Option<&EthNativeBlockObjectV1>,
    native_canonical_chain: Option<&NetworkRuntimeNativeCanonicalChainStateV1>,
    runtime_sync: Option<NetworkRuntimeSyncStatus>,
    runtime_native_sync: Option<NetworkRuntimeNativeSyncStatusV1>,
) -> Option<EthFullnodeSyncViewV1> {
    let prioritized_chain_view = derive_eth_fullnode_chain_view_with_native_preference_v1(
        fallback_chain_view,
        native_head_block,
        native_canonical_chain,
    );
    let source_priority_policy = derive_eth_fullnode_source_priority_policy_v1(
        prioritized_chain_view.as_ref(),
        native_head_block,
        native_canonical_chain,
        runtime_native_sync,
    );
    let source = source_priority_policy.sync_source;
    let chain_id = prioritized_chain_view
        .as_ref()
        .map(|view| view.chain_id)
        .or_else(|| native_head_block.map(|block| block.header.chain_id))
        .unwrap_or(0);
    let mut peer_count = native_head_block
        .map(|block| block.source_peer_count)
        .unwrap_or(0);
    let mut starting_block_number = prioritized_chain_view
        .as_ref()
        .map(|view| view.starting_block_number)
        .unwrap_or_else(|| {
            native_head_block
                .map(|block| block.header.number)
                .unwrap_or(0)
        });
    let mut current_block_number = prioritized_chain_view
        .as_ref()
        .map(|view| view.current_block_number)
        .unwrap_or_else(|| {
            native_head_block
                .map(|block| block.header.number)
                .unwrap_or(0)
        });
    let mut highest_block_number = prioritized_chain_view
        .as_ref()
        .map(|view| view.highest_block_number)
        .unwrap_or(current_block_number);
    let chain_view_current_block_number = prioritized_chain_view
        .as_ref()
        .map(|view| view.current_block_number)
        .unwrap_or(current_block_number);
    let mut current_block_hash = prioritized_chain_view
        .as_ref()
        .map(|view| view.current_block_hash)
        .unwrap_or_else(|| {
            native_head_block
                .map(|block| block.header.hash)
                .unwrap_or([0u8; 32])
        });
    let mut parent_block_hash = prioritized_chain_view
        .as_ref()
        .map(|view| view.parent_block_hash)
        .unwrap_or_else(|| {
            native_head_block
                .map(|block| block.header.parent_hash)
                .unwrap_or([0u8; 32])
        });
    let mut current_state_root = prioritized_chain_view
        .as_ref()
        .map(|view| view.current_state_root)
        .unwrap_or_else(|| {
            native_head_block
                .map(|block| block.header.state_root)
                .unwrap_or([0u8; 32])
        });
    let mut current_state_version = prioritized_chain_view
        .as_ref()
        .map(|view| view.current_state_version)
        .unwrap_or(0);
    if let Some(runtime) = runtime_sync {
        peer_count = peer_count.max(runtime.peer_count);
        starting_block_number = if starting_block_number == 0 {
            runtime.starting_block
        } else {
            starting_block_number.min(runtime.starting_block)
        };
        current_block_number = current_block_number.max(runtime.current_block);
        highest_block_number = highest_block_number.max(runtime.highest_block);
    }
    let native_sync_phase = runtime_native_sync
        .filter(network_runtime_native_sync_is_active)
        .map(|native| {
            peer_count = peer_count.max(native.peer_count);
            starting_block_number = if starting_block_number == 0 {
                native.starting_block
            } else {
                starting_block_number.min(native.starting_block)
            };
            current_block_number = current_block_number.max(native.current_block);
            highest_block_number = highest_block_number.max(native.highest_block);
            native.phase.as_str().to_string()
        });
    if highest_block_number < current_block_number {
        highest_block_number = current_block_number;
    }
    if starting_block_number > current_block_number {
        starting_block_number = current_block_number;
    }
    if prioritized_chain_view.is_none() || current_block_number != chain_view_current_block_number {
        current_block_hash = [0u8; 32];
        parent_block_hash = [0u8; 32];
        current_state_root = [0u8; 32];
        current_state_version = 0;
    }
    if prioritized_chain_view.is_none()
        && peer_count == 0
        && highest_block_number == 0
        && current_block_number == 0
    {
        return None;
    }
    Some(EthFullnodeSyncViewV1 {
        source,
        chain_id,
        peer_count,
        starting_block_number,
        current_block_number,
        highest_block_number,
        current_block_hash,
        parent_block_hash,
        current_state_root,
        current_state_version,
        native_sync_phase,
        syncing: highest_block_number > current_block_number,
        source_priority_policy,
    })
}

#[must_use]
pub fn derive_eth_fullnode_sync_view_v1(
    chain_view: Option<&EthFullnodeChainViewV1>,
    runtime_sync: Option<NetworkRuntimeSyncStatus>,
    runtime_native_sync: Option<NetworkRuntimeNativeSyncStatusV1>,
) -> Option<EthFullnodeSyncViewV1> {
    derive_eth_fullnode_sync_view_with_native_preference_v1(
        chain_view,
        None,
        None,
        runtime_sync,
        runtime_native_sync,
    )
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum EthSyncTransportProfile {
    /// Target final shape: same-level Ethereum native protocol stack
    /// (devp2p/discovery/RLPx/eth,snap wire).
    NativeDevp2pRlpx,
    /// Current production shape in this repository: native runtime bridge
    /// in novovm-network + gateway full-node-only policy.
    NovovmNativeBridge,
}

impl EthSyncTransportProfile {
    #[must_use]
    pub fn as_str(self) -> &'static str {
        match self {
            Self::NativeDevp2pRlpx => "native_devp2p_rlpx",
            Self::NovovmNativeBridge => "novovm_native_bridge",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct EthNativeCapabilities {
    pub eth_versions: Vec<EthWireVersion>,
    pub snap_versions: Vec<SnapWireVersion>,
    pub tx_broadcast_enabled: bool,
    pub block_propagation_enabled: bool,
    pub state_sync_enabled: bool,
}

impl EthNativeCapabilities {
    #[must_use]
    pub fn highest_eth_version(&self) -> Option<EthWireVersion> {
        self.eth_versions.iter().copied().max()
    }

    #[must_use]
    pub fn highest_snap_version(&self) -> Option<SnapWireVersion> {
        self.snap_versions.iter().copied().max_by_key(|v| v.as_u8())
    }
}

#[must_use]
pub fn default_eth_native_capabilities() -> EthNativeCapabilities {
    EthNativeCapabilities {
        eth_versions: vec![
            EthWireVersion::V70,
            EthWireVersion::V69,
            EthWireVersion::V68,
            EthWireVersion::V67,
            EthWireVersion::V66,
        ],
        snap_versions: vec![SnapWireVersion::V1],
        tx_broadcast_enabled: true,
        block_propagation_enabled: true,
        state_sync_enabled: true,
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct EthNegotiatedCapabilities {
    pub eth_version: EthWireVersion,
    pub snap_version: Option<SnapWireVersion>,
}

#[must_use]
pub fn negotiate_eth_native_capabilities(
    local: &EthNativeCapabilities,
    remote_eth_versions: &[u8],
    remote_snap_versions: &[u8],
) -> Option<EthNegotiatedCapabilities> {
    let remote_eth = remote_eth_versions
        .iter()
        .copied()
        .filter_map(EthWireVersion::parse)
        .collect::<Vec<_>>();
    let remote_snap = remote_snap_versions
        .iter()
        .copied()
        .filter_map(SnapWireVersion::parse)
        .collect::<Vec<_>>();

    let mut shared_eth = local
        .eth_versions
        .iter()
        .copied()
        .filter(|v| remote_eth.contains(v))
        .collect::<Vec<_>>();
    if shared_eth.is_empty() {
        return None;
    }
    shared_eth.sort_unstable();
    let eth_version = shared_eth.into_iter().last()?;

    let snap_version = if local.state_sync_enabled {
        let mut shared_snap = local
            .snap_versions
            .iter()
            .copied()
            .filter(|v| remote_snap.contains(v))
            .collect::<Vec<_>>();
        shared_snap.sort_by_key(|v| v.as_u8());
        shared_snap.into_iter().last()
    } else {
        None
    };

    Some(EthNegotiatedCapabilities {
        eth_version,
        snap_version,
    })
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct EthNativeParityProgress {
    pub profile: EthSyncTransportProfile,
    pub full_node_only: bool,
    pub upstream_fallback_disabled: bool,
    pub native_peer_discovery: bool,
    pub native_eth_handshake: bool,
    pub native_snap_sync_state_machine: bool,
    pub state_proof_semantics_closed: bool,
    pub rpc_core_semantics_closed: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum EthPeerLifecycleStageV1 {
    Discovered,
    Connecting,
    Connected,
    HelloOk,
    StatusOk,
    Ready,
    Syncing,
    Cooldown,
    TemporarilyFailed,
    PermanentlyRejected,
}

impl EthPeerLifecycleStageV1 {
    #[must_use]
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Discovered => "discovered",
            Self::Connecting => "connecting",
            Self::Connected => "connected",
            Self::HelloOk => "hello_ok",
            Self::StatusOk => "status_ok",
            Self::Ready => "ready",
            Self::Syncing => "syncing",
            Self::Cooldown => "cooldown",
            Self::TemporarilyFailed => "temporarily_failed",
            Self::PermanentlyRejected => "permanently_rejected",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum EthPeerLifecycleProgressStageV1 {
    Discovered,
    Connecting,
    Connected,
    HelloOk,
    StatusOk,
    Ready,
    Syncing,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum EthPeerFailureClassV1 {
    ConnectFailure,
    HandshakeFailure,
    DecodeFailure,
    Timeout,
    ValidationReject,
    Disconnect,
}

impl EthPeerFailureClassV1 {
    #[must_use]
    pub fn as_str(self) -> &'static str {
        match self {
            Self::ConnectFailure => "connect_failure",
            Self::HandshakeFailure => "handshake_failure",
            Self::DecodeFailure => "decode_failure",
            Self::Timeout => "timeout",
            Self::ValidationReject => "validation_reject",
            Self::Disconnect => "disconnect",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct EthPeerLifecycleSummaryV1 {
    pub chain_id: u64,
    pub peer_count: u64,
    pub discovered_count: u64,
    pub connecting_count: u64,
    pub connected_count: u64,
    pub hello_ok_count: u64,
    pub status_ok_count: u64,
    pub ready_count: u64,
    pub syncing_count: u64,
    pub cooldown_count: u64,
    pub temporarily_failed_count: u64,
    pub permanently_rejected_count: u64,
    pub retry_eligible_count: u64,
    pub connect_failure_count: u64,
    pub handshake_failure_count: u64,
    pub decode_failure_count: u64,
    pub timeout_count: u64,
    pub validation_reject_count: u64,
    pub disconnect_count: u64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum EthPeerSelectionRoleV1 {
    Bootstrap,
    Sync,
}

impl EthPeerSelectionRoleV1 {
    #[must_use]
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Bootstrap => "bootstrap",
            Self::Sync => "sync",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct EthPeerSelectionScoreV1 {
    pub chain_id: u64,
    pub peer_id: u64,
    pub role: EthPeerSelectionRoleV1,
    pub stage: EthPeerLifecycleStageV1,
    pub eligible: bool,
    pub selected: bool,
    pub score: i64,
    pub reasons: Vec<String>,
    pub last_head_height: u64,
    pub successful_sessions: u64,
    pub header_response_count: u64,
    pub body_response_count: u64,
    pub sync_contribution_count: u64,
    pub consecutive_failures: u64,
    pub last_success_unix_ms: u64,
    pub last_failure_unix_ms: u64,
    pub cooldown_until_unix_ms: u64,
    pub permanently_rejected: bool,
    #[serde(default)]
    pub long_term_score: i64,
    pub recent_window: EthPeerRecentWindowStatsV1,
    #[serde(default)]
    pub long_term: EthPeerLongTermStatsV1,
    #[serde(default)]
    pub window_layers: EthPeerSelectionWindowLayersV1,
}

#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct EthPeerSelectionQualitySummaryV1 {
    pub chain_id: u64,
    pub candidate_peer_count: u64,
    pub evaluated_bootstrap_peers: u64,
    pub evaluated_sync_peers: u64,
    pub retry_eligible_bootstrap_peers: u64,
    pub ready_sync_peers: u64,
    pub selected_bootstrap_peers: u64,
    pub selected_sync_peers: u64,
    pub skipped_cooldown_peers: u64,
    pub skipped_permanently_rejected_peers: u64,
    pub skipped_unready_sync_peers: u64,
    pub top_selected_bootstrap_peer_id: Option<u64>,
    pub top_selected_sync_peer_id: Option<u64>,
    pub top_selected_bootstrap_score: Option<i64>,
    pub top_selected_sync_score: Option<i64>,
    pub average_selected_bootstrap_score: Option<i64>,
    pub average_selected_sync_score: Option<i64>,
    pub selected_bootstrap_peer_ids: Vec<u64>,
    pub selected_sync_peer_ids: Vec<u64>,
}

#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct EthPeerRecentWindowStatsV1 {
    pub window_rounds: u64,
    pub selected_rounds: u64,
    pub selected_bootstrap_rounds: u64,
    pub selected_sync_rounds: u64,
    pub header_success_rounds: u64,
    pub body_success_rounds: u64,
    pub sync_contribution_rounds: u64,
    pub selected_without_progress_rounds: u64,
    pub connect_failure_rounds: u64,
    pub handshake_failure_rounds: u64,
    pub decode_failure_rounds: u64,
    pub timeout_failure_rounds: u64,
    pub validation_reject_rounds: u64,
    pub disconnect_rounds: u64,
    pub capacity_reject_rounds: u64,
    pub last_selected_unix_ms: u64,
    pub last_progress_unix_ms: u64,
    pub last_failure_unix_ms: u64,
    pub selection_hit_rate_bps: u64,
    pub header_success_rate_bps: u64,
    pub body_success_rate_bps: u64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum EthPeerSelectionWindowRoleV1 {
    ShortTermVeto,
    MediumTermStability,
    LongTermRetention,
}

impl EthPeerSelectionWindowRoleV1 {
    #[must_use]
    pub fn as_str(self) -> &'static str {
        match self {
            Self::ShortTermVeto => "short_term_veto",
            Self::MediumTermStability => "medium_term_stability",
            Self::LongTermRetention => "long_term_retention",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct EthPeerSelectionWindowLayerV1 {
    pub role: EthPeerSelectionWindowRoleV1,
    pub max_rounds: u64,
    pub stats: EthPeerRecentWindowStatsV1,
    pub signal_score: i64,
    pub active: bool,
}

impl Default for EthPeerSelectionWindowLayerV1 {
    fn default() -> Self {
        Self {
            role: EthPeerSelectionWindowRoleV1::ShortTermVeto,
            max_rounds: 0,
            stats: EthPeerRecentWindowStatsV1::default(),
            signal_score: 0,
            active: false,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct EthPeerSelectionWindowLayersV1 {
    pub short_term: EthPeerSelectionWindowLayerV1,
    pub medium_term: EthPeerSelectionWindowLayerV1,
    pub long_term: EthPeerSelectionWindowLayerV1,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct EthPeerSelectionWindowPolicyV1 {
    pub short_term_role: EthPeerSelectionWindowRoleV1,
    pub short_term_rounds: u64,
    pub medium_term_role: EthPeerSelectionWindowRoleV1,
    pub medium_term_rounds: u64,
    pub long_term_role: EthPeerSelectionWindowRoleV1,
    pub long_term_rounds: u64,
    pub sync_short_term_weight_bps: u64,
    pub sync_medium_term_weight_bps: u64,
    pub sync_long_term_weight_bps: u64,
    pub bootstrap_short_term_weight_bps: u64,
    pub bootstrap_medium_term_weight_bps: u64,
    pub bootstrap_long_term_weight_bps: u64,
    pub medium_term_selection_hit_rate_floor_bps: u64,
    pub long_term_selection_hit_rate_floor_bps: u64,
    pub long_term_body_success_rate_floor_bps: u64,
}

impl Default for EthPeerSelectionWindowPolicyV1 {
    fn default() -> Self {
        Self {
            short_term_role: EthPeerSelectionWindowRoleV1::ShortTermVeto,
            short_term_rounds: ETH_PEER_SELECTION_SHORT_WINDOW_ROUNDS_V1 as u64,
            medium_term_role: EthPeerSelectionWindowRoleV1::MediumTermStability,
            medium_term_rounds: ETH_PEER_SELECTION_MEDIUM_WINDOW_ROUNDS_V1 as u64,
            long_term_role: EthPeerSelectionWindowRoleV1::LongTermRetention,
            long_term_rounds: ETH_PEER_SELECTION_LONG_WINDOW_ROUNDS_V1 as u64,
            sync_short_term_weight_bps: 10_000,
            sync_medium_term_weight_bps: 8_500,
            sync_long_term_weight_bps: 9_500,
            bootstrap_short_term_weight_bps: 10_000,
            bootstrap_medium_term_weight_bps: 6_000,
            bootstrap_long_term_weight_bps: 3_500,
            medium_term_selection_hit_rate_floor_bps: 4_500,
            long_term_selection_hit_rate_floor_bps: 4_000,
            long_term_body_success_rate_floor_bps: 2_000,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct EthPeerLongTermStatsV1 {
    pub total_observed_rounds: u64,
    pub total_selected_rounds: u64,
    pub total_selected_bootstrap_rounds: u64,
    pub total_selected_sync_rounds: u64,
    pub total_header_success_rounds: u64,
    pub total_body_success_rounds: u64,
    pub total_sync_contribution_rounds: u64,
    pub total_selected_without_progress_rounds: u64,
    pub total_connect_failure_rounds: u64,
    pub total_handshake_failure_rounds: u64,
    pub total_decode_failure_rounds: u64,
    pub total_timeout_failure_rounds: u64,
    pub total_validation_reject_rounds: u64,
    pub total_disconnect_rounds: u64,
    pub total_capacity_reject_rounds: u64,
    pub current_consecutive_connect_failures: u64,
    pub current_consecutive_handshake_failures: u64,
    pub current_consecutive_decode_failures: u64,
    pub current_consecutive_timeout_failures: u64,
    pub current_consecutive_validation_rejects: u64,
    pub current_consecutive_disconnects: u64,
    pub current_consecutive_selected_without_progress_rounds: u64,
    pub max_consecutive_connect_failures: u64,
    pub max_consecutive_handshake_failures: u64,
    pub max_consecutive_decode_failures: u64,
    pub max_consecutive_timeout_failures: u64,
    pub max_consecutive_validation_rejects: u64,
    pub max_consecutive_disconnects: u64,
    pub max_consecutive_selected_without_progress_rounds: u64,
    pub last_selected_unix_ms: u64,
    pub last_progress_unix_ms: u64,
    pub last_failure_unix_ms: u64,
    pub selection_hit_rate_bps: u64,
    pub header_success_rate_bps: u64,
    pub body_success_rate_bps: u64,
    #[serde(default)]
    pub medium_window: EthPeerRecentWindowStatsV1,
    #[serde(default)]
    pub retention_window: EthPeerRecentWindowStatsV1,
    #[serde(default)]
    pub short_term_veto_active: bool,
    #[serde(default)]
    pub medium_term_stable: bool,
    #[serde(default)]
    pub long_term_trusted: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct EthPeerSelectionLongTermSummaryV1 {
    pub chain_id: u64,
    pub tracked_sync_peers: u64,
    pub peers_with_history: u64,
    pub peers_with_positive_contribution: u64,
    pub peers_currently_in_failure_streak: u64,
    pub peers_currently_in_progressless_streak: u64,
    pub observed_rounds_total: u64,
    pub selected_rounds_total: u64,
    pub selected_sync_rounds_total: u64,
    pub sync_contribution_rounds_total: u64,
    pub selected_without_progress_rounds_total: u64,
    pub connect_failure_rounds_total: u64,
    pub handshake_failure_rounds_total: u64,
    pub decode_failure_rounds_total: u64,
    pub timeout_failure_rounds_total: u64,
    pub validation_reject_rounds_total: u64,
    pub disconnect_rounds_total: u64,
    pub capacity_reject_rounds_total: u64,
    pub average_selection_hit_rate_bps: u64,
    pub average_header_success_rate_bps: u64,
    pub average_body_success_rate_bps: u64,
    pub average_long_term_score: Option<i64>,
    pub top_trusted_sync_peer_id: Option<u64>,
    pub top_trusted_sync_long_term_score: Option<i64>,
}

pub const ETH_FULLNODE_NATIVE_WORKER_RUNTIME_SCHEMA_V1: &str =
    "supervm-eth-fullnode-native-runtime/v1";
pub const ETH_FULLNODE_NATIVE_RUNTIME_BLOCK_SNAPSHOT_LIMIT_V1: usize =
    ETH_FULLNODE_DEFAULT_RUNTIME_BLOCK_SNAPSHOT_LIMIT as usize;
pub const ETH_FULLNODE_NATIVE_RUNTIME_PENDING_TX_SNAPSHOT_LIMIT_V1: usize =
    ETH_FULLNODE_DEFAULT_RUNTIME_PENDING_TX_SNAPSHOT_LIMIT as usize;
pub const ETH_FULLNODE_NATIVE_WORKER_RUNTIME_SNAPSHOT_ENV_V1: &str =
    "NOVOVM_ETH_NATIVE_WORKER_RUNTIME_SNAPSHOT_PATH";
pub const ETH_FULLNODE_NATIVE_WORKER_RUNTIME_SNAPSHOT_DEFAULT_PATH_V1: &str =
    "artifacts/mainline/eth-native-worker-runtime.json";

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct EthFullnodeNativePeerFailureSnapshotV1 {
    pub peer_id: u64,
    pub endpoint: Option<String>,
    pub phase: String,
    pub class: String,
    #[serde(default)]
    pub lifecycle_class: Option<String>,
    #[serde(default)]
    pub reason_code: Option<u64>,
    #[serde(default)]
    pub reason_name: Option<String>,
    pub error: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct EthFullnodeNativeWorkerRuntimeSnapshotV1 {
    pub schema: String,
    pub chain_id: u64,
    pub updated_at_unix_ms: u64,
    pub candidate_peer_ids: Vec<u64>,
    pub scheduled_bootstrap_peers: u64,
    pub scheduled_sync_peers: u64,
    pub attempted_bootstrap_peers: u64,
    pub attempted_sync_peers: u64,
    pub failed_bootstrap_peers: u64,
    pub failed_sync_peers: u64,
    pub skipped_missing_endpoint_peers: u64,
    pub connected_peers: u64,
    pub ready_peers: u64,
    pub status_updates: u64,
    pub header_updates: u64,
    pub body_updates: u64,
    pub sync_requests: u64,
    pub inbound_frames: u64,
    pub head_view: Option<EthFullnodeHeadViewV1>,
    pub sync_view: Option<EthFullnodeSyncViewV1>,
    #[serde(default)]
    pub native_canonical_chain: Option<NetworkRuntimeNativeCanonicalChainStateV1>,
    #[serde(default)]
    pub native_canonical_blocks: Vec<NetworkRuntimeNativeCanonicalBlockStateV1>,
    #[serde(default)]
    pub native_pending_tx_summary: NetworkRuntimeNativePendingTxSummaryV1,
    #[serde(default)]
    pub native_pending_tx_broadcast_runtime: NetworkRuntimeNativePendingTxBroadcastRuntimeSummaryV1,
    #[serde(default)]
    pub native_execution_budget_runtime: NetworkRuntimeNativeExecutionBudgetRuntimeSummaryV1,
    #[serde(default)]
    pub native_pending_txs: Vec<NetworkRuntimeNativePendingTxStateV1>,
    pub native_head_body_available: Option<bool>,
    pub native_head_canonical: Option<bool>,
    pub native_head_safe: Option<bool>,
    pub native_head_finalized: Option<bool>,
    pub lifecycle_summary: EthPeerLifecycleSummaryV1,
    #[serde(default)]
    pub selection_quality_summary: EthPeerSelectionQualitySummaryV1,
    #[serde(default)]
    pub selection_long_term_summary: EthPeerSelectionLongTermSummaryV1,
    #[serde(default)]
    pub selection_window_policy: EthPeerSelectionWindowPolicyV1,
    #[serde(default)]
    pub runtime_config: EthFullnodeNativeRuntimeConfigV1,
    #[serde(default)]
    pub peer_selection_scores: Vec<EthPeerSelectionScoreV1>,
    pub peer_sessions: Vec<EthPeerSessionSnapshot>,
    pub peer_failures: Vec<EthFullnodeNativePeerFailureSnapshotV1>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct EthPeerSessionSnapshot {
    pub chain_id: u64,
    pub peer_id: u64,
    pub negotiated: EthNegotiatedCapabilities,
    pub lifecycle_stage: EthPeerLifecycleStageV1,
    pub retry_eligible: bool,
    pub session_ready: bool,
    pub last_head_height: u64,
    pub successful_sessions: u64,
    #[serde(default)]
    pub header_response_count: u64,
    #[serde(default)]
    pub body_response_count: u64,
    #[serde(default)]
    pub sync_contribution_count: u64,
    pub connect_failure_count: u64,
    pub handshake_failure_count: u64,
    pub decode_failure_count: u64,
    pub timeout_count: u64,
    pub validation_reject_count: u64,
    pub last_validation_reject_reason: Option<EthChainConfigPeerValidationReasonV1>,
    pub disconnect_count: u64,
    pub disconnect_too_many_peers_count: u64,
    pub last_disconnect_reason_code: Option<u64>,
    pub last_failure_class: Option<EthPeerFailureClassV1>,
    pub last_failure_reason_code: Option<u64>,
    pub last_failure_reason_name: Option<String>,
    pub consecutive_failures: u64,
    pub first_seen_unix_ms: u64,
    pub first_failure_unix_ms: u64,
    pub cooldown_until_unix_ms: u64,
    pub permanently_rejected: bool,
    pub last_success_unix_ms: u64,
    #[serde(default)]
    pub last_header_success_unix_ms: u64,
    #[serde(default)]
    pub last_body_success_unix_ms: u64,
    pub last_failure_unix_ms: u64,
    pub last_state_change_unix_ms: u64,
    #[serde(default)]
    pub recent_window: EthPeerRecentWindowStatsV1,
    #[serde(default)]
    pub long_term: EthPeerLongTermStatsV1,
    #[serde(default)]
    pub window_layers: EthPeerSelectionWindowLayersV1,
}

#[derive(Debug, Clone)]
struct EthPeerSessionState {
    negotiated: EthNegotiatedCapabilities,
    progress_stage: EthPeerLifecycleProgressStageV1,
    session_ready: bool,
    last_head_height: u64,
    successful_sessions: u64,
    header_response_count: u64,
    body_response_count: u64,
    sync_contribution_count: u64,
    connect_failure_count: u64,
    handshake_failure_count: u64,
    decode_failure_count: u64,
    timeout_count: u64,
    validation_reject_count: u64,
    last_validation_reject_reason: Option<EthChainConfigPeerValidationReasonV1>,
    disconnect_count: u64,
    disconnect_too_many_peers_count: u64,
    last_disconnect_reason_code: Option<u64>,
    last_failure_class: Option<EthPeerFailureClassV1>,
    last_failure_reason_code: Option<u64>,
    last_failure_reason_name: Option<String>,
    consecutive_failures: u64,
    first_seen_unix_ms: u64,
    first_failure_unix_ms: u64,
    cooldown_until_unix_ms: u64,
    permanently_rejected: bool,
    last_success_unix_ms: u64,
    last_header_success_unix_ms: u64,
    last_body_success_unix_ms: u64,
    last_failure_unix_ms: u64,
    last_state_change_unix_ms: u64,
    long_term: EthPeerLongTermStatsV1,
    recent_rounds: std::collections::VecDeque<EthPeerRoundOutcomeV1>,
    medium_rounds: std::collections::VecDeque<EthPeerRoundOutcomeV1>,
    long_rounds: std::collections::VecDeque<EthPeerRoundOutcomeV1>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct EthPeerRoundOutcomeV1 {
    selected_bootstrap: bool,
    selected_sync: bool,
    header_success: bool,
    body_success: bool,
    connect_failure: bool,
    handshake_failure: bool,
    decode_failure: bool,
    timeout_failure: bool,
    validation_reject: bool,
    disconnect: bool,
    capacity_reject: bool,
    observed_unix_ms: u64,
}

type EthPeerSessionMap = HashMap<u64, HashMap<u64, EthPeerSessionState>>;
static ETH_PEER_SESSIONS: OnceLock<Mutex<EthPeerSessionMap>> = OnceLock::new();
static ETH_FULLNODE_NATIVE_WORKER_RUNTIME_SNAPSHOTS: OnceLock<
    Mutex<HashMap<u64, EthFullnodeNativeWorkerRuntimeSnapshotV1>>,
> = OnceLock::new();
static ETH_NATIVE_SYNC_EVIDENCE: OnceLock<Mutex<HashMap<u64, EthNativeSyncEvidence>>> =
    OnceLock::new();

fn eth_peer_sessions() -> &'static Mutex<EthPeerSessionMap> {
    ETH_PEER_SESSIONS.get_or_init(|| Mutex::new(HashMap::new()))
}

fn eth_fullnode_native_worker_runtime_snapshot_map(
) -> &'static Mutex<HashMap<u64, EthFullnodeNativeWorkerRuntimeSnapshotV1>> {
    ETH_FULLNODE_NATIVE_WORKER_RUNTIME_SNAPSHOTS.get_or_init(|| Mutex::new(HashMap::new()))
}

#[must_use]
pub fn default_eth_fullnode_native_worker_runtime_snapshot_path_v1() -> PathBuf {
    std::env::var(ETH_FULLNODE_NATIVE_WORKER_RUNTIME_SNAPSHOT_ENV_V1)
        .ok()
        .map(|raw| raw.trim().to_string())
        .filter(|raw| !raw.is_empty())
        .map(PathBuf::from)
        .unwrap_or_else(|| {
            let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
            let workspace_root = manifest_dir
                .parent()
                .and_then(Path::parent)
                .map(Path::to_path_buf)
                .unwrap_or(manifest_dir);
            workspace_root.join(ETH_FULLNODE_NATIVE_WORKER_RUNTIME_SNAPSHOT_DEFAULT_PATH_V1)
        })
}

pub fn set_eth_fullnode_native_worker_runtime_snapshot_v1(
    chain_id: u64,
    snapshot: EthFullnodeNativeWorkerRuntimeSnapshotV1,
) {
    eth_fullnode_native_worker_runtime_snapshot_map()
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
        .insert(chain_id, snapshot);
}

#[must_use]
pub fn snapshot_eth_fullnode_native_worker_runtime_snapshot_v1(
    chain_id: u64,
) -> Option<EthFullnodeNativeWorkerRuntimeSnapshotV1> {
    eth_fullnode_native_worker_runtime_snapshot_map()
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
        .get(&chain_id)
        .cloned()
}

pub fn clear_eth_fullnode_native_worker_runtime_snapshot_for_chain_v1(chain_id: u64) {
    eth_fullnode_native_worker_runtime_snapshot_map()
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
        .remove(&chain_id);
}

pub fn write_eth_fullnode_native_worker_runtime_snapshot_to_path_v1(
    path: &Path,
    snapshot: &EthFullnodeNativeWorkerRuntimeSnapshotV1,
) -> std::io::Result<()> {
    if let Some(parent) = path.parent() {
        if !parent.as_os_str().is_empty() {
            fs::create_dir_all(parent)?;
        }
    }
    let encoded = serde_json::to_vec_pretty(snapshot)
        .map_err(|err| IoError::new(IoErrorKind::InvalidData, err.to_string()))?;
    fs::write(path, encoded)
}

pub fn write_eth_fullnode_native_worker_runtime_snapshot_default_path_v1(
    snapshot: &EthFullnodeNativeWorkerRuntimeSnapshotV1,
) -> std::io::Result<PathBuf> {
    let path = default_eth_fullnode_native_worker_runtime_snapshot_path_v1();
    write_eth_fullnode_native_worker_runtime_snapshot_to_path_v1(path.as_path(), snapshot)?;
    Ok(path)
}

pub fn load_eth_fullnode_native_worker_runtime_snapshot_from_path_v1(
    path: &Path,
) -> std::io::Result<EthFullnodeNativeWorkerRuntimeSnapshotV1> {
    let bytes = fs::read(path)?;
    serde_json::from_slice(bytes.as_slice())
        .map_err(|err| IoError::new(IoErrorKind::InvalidData, err.to_string()))
}

fn eth_peer_now_unix_ms_v1() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_millis() as u64)
        .unwrap_or_default()
}

const ETH_PEER_FAILURE_BACKOFF_BASE_MS_V1: u64 = 2_500;
const ETH_PEER_FAILURE_BACKOFF_MAX_MS_V1: u64 = 300_000;
const ETH_PEER_PROTOCOL_BACKOFF_FLOOR_MS_V1: u64 = 10_000;
const ETH_PEER_TIMEOUT_BACKOFF_FLOOR_MS_V1: u64 = 15_000;
const ETH_PEER_SELECTION_SHORT_WINDOW_ROUNDS_V1: usize = 16;
const ETH_PEER_SELECTION_MEDIUM_WINDOW_ROUNDS_V1: usize = 64;
const ETH_PEER_SELECTION_LONG_WINDOW_ROUNDS_V1: usize = 256;

fn default_eth_peer_negotiated_capabilities_v1() -> EthNegotiatedCapabilities {
    EthNegotiatedCapabilities {
        eth_version: EthWireVersion::V66,
        snap_version: None,
    }
}

fn eth_peer_session_state_default_v1(now: u64) -> EthPeerSessionState {
    EthPeerSessionState {
        negotiated: default_eth_peer_negotiated_capabilities_v1(),
        progress_stage: EthPeerLifecycleProgressStageV1::Discovered,
        session_ready: false,
        last_head_height: 0,
        successful_sessions: 0,
        header_response_count: 0,
        body_response_count: 0,
        sync_contribution_count: 0,
        connect_failure_count: 0,
        handshake_failure_count: 0,
        decode_failure_count: 0,
        timeout_count: 0,
        validation_reject_count: 0,
        last_validation_reject_reason: None,
        disconnect_count: 0,
        disconnect_too_many_peers_count: 0,
        last_disconnect_reason_code: None,
        last_failure_class: None,
        last_failure_reason_code: None,
        last_failure_reason_name: None,
        consecutive_failures: 0,
        first_seen_unix_ms: now,
        first_failure_unix_ms: 0,
        cooldown_until_unix_ms: 0,
        permanently_rejected: false,
        last_success_unix_ms: 0,
        last_header_success_unix_ms: 0,
        last_body_success_unix_ms: 0,
        last_failure_unix_ms: 0,
        last_state_change_unix_ms: now,
        long_term: EthPeerLongTermStatsV1::default(),
        recent_rounds: std::collections::VecDeque::new(),
        medium_rounds: std::collections::VecDeque::new(),
        long_rounds: std::collections::VecDeque::new(),
    }
}

fn eth_peer_failure_backoff_ms_v1(consecutive_failures: u64) -> u64 {
    if consecutive_failures == 0 {
        return 0;
    }
    let shift = consecutive_failures.saturating_sub(1).min(8) as u32;
    ETH_PEER_FAILURE_BACKOFF_BASE_MS_V1
        .saturating_mul(1u64 << shift)
        .clamp(
            ETH_PEER_FAILURE_BACKOFF_BASE_MS_V1,
            ETH_PEER_FAILURE_BACKOFF_MAX_MS_V1,
        )
}

fn eth_peer_disconnect_cooldown_ms_v1(reason_code: Option<u64>) -> u64 {
    match reason_code {
        Some(0x04) => 60_000,
        Some(0x03) => 5_000,
        Some(0x10) => 60_000,
        Some(0x0b) => 10_000,
        Some(_) => 5_000,
        None => 2_500,
    }
}

fn eth_peer_validation_cooldown_ms_v1(reason: EthChainConfigPeerValidationReasonV1) -> u64 {
    match reason {
        EthChainConfigPeerValidationReasonV1::WrongNetwork
        | EthChainConfigPeerValidationReasonV1::WrongGenesis => 0,
        EthChainConfigPeerValidationReasonV1::RemoteStaleForkId => 120_000,
        EthChainConfigPeerValidationReasonV1::UnsupportedForkProgression => 300_000,
    }
}

fn eth_peer_validation_is_permanent_v1(reason: EthChainConfigPeerValidationReasonV1) -> bool {
    matches!(
        reason,
        EthChainConfigPeerValidationReasonV1::WrongNetwork
            | EthChainConfigPeerValidationReasonV1::WrongGenesis
    )
}

fn eth_peer_failure_count_for_class_v1(
    state: &EthPeerSessionState,
    class: EthPeerFailureClassV1,
) -> u64 {
    match class {
        EthPeerFailureClassV1::ConnectFailure => state.connect_failure_count,
        EthPeerFailureClassV1::HandshakeFailure => state.handshake_failure_count,
        EthPeerFailureClassV1::DecodeFailure => state.decode_failure_count,
        EthPeerFailureClassV1::Timeout => state.timeout_count,
        EthPeerFailureClassV1::ValidationReject => state.validation_reject_count,
        EthPeerFailureClassV1::Disconnect => state.disconnect_count,
    }
}

fn eth_peer_failure_cooldown_ms_v1(
    state: &EthPeerSessionState,
    class: EthPeerFailureClassV1,
    reason_code: Option<u64>,
    validation_reason: Option<EthChainConfigPeerValidationReasonV1>,
) -> u64 {
    match class {
        EthPeerFailureClassV1::ConnectFailure => eth_peer_failure_backoff_ms_v1(
            eth_peer_failure_count_for_class_v1(state, class).saturating_add(1),
        )
        .max(ETH_PEER_FAILURE_BACKOFF_BASE_MS_V1),
        EthPeerFailureClassV1::HandshakeFailure | EthPeerFailureClassV1::DecodeFailure => {
            eth_peer_failure_backoff_ms_v1(
                eth_peer_failure_count_for_class_v1(state, class).saturating_add(1),
            )
            .max(ETH_PEER_PROTOCOL_BACKOFF_FLOOR_MS_V1)
        }
        EthPeerFailureClassV1::Timeout => eth_peer_failure_backoff_ms_v1(
            eth_peer_failure_count_for_class_v1(state, class).saturating_add(1),
        )
        .max(ETH_PEER_TIMEOUT_BACKOFF_FLOOR_MS_V1),
        EthPeerFailureClassV1::ValidationReject => validation_reason
            .map(eth_peer_validation_cooldown_ms_v1)
            .unwrap_or(ETH_PEER_PROTOCOL_BACKOFF_FLOOR_MS_V1),
        EthPeerFailureClassV1::Disconnect => eth_peer_failure_backoff_ms_v1(
            eth_peer_failure_count_for_class_v1(state, class).saturating_add(1),
        )
        .max(eth_peer_disconnect_cooldown_ms_v1(reason_code)),
    }
}

fn eth_peer_progress_stage_to_lifecycle_stage_v1(
    stage: EthPeerLifecycleProgressStageV1,
) -> EthPeerLifecycleStageV1 {
    match stage {
        EthPeerLifecycleProgressStageV1::Discovered => EthPeerLifecycleStageV1::Discovered,
        EthPeerLifecycleProgressStageV1::Connecting => EthPeerLifecycleStageV1::Connecting,
        EthPeerLifecycleProgressStageV1::Connected => EthPeerLifecycleStageV1::Connected,
        EthPeerLifecycleProgressStageV1::HelloOk => EthPeerLifecycleStageV1::HelloOk,
        EthPeerLifecycleProgressStageV1::StatusOk => EthPeerLifecycleStageV1::StatusOk,
        EthPeerLifecycleProgressStageV1::Ready => EthPeerLifecycleStageV1::Ready,
        EthPeerLifecycleProgressStageV1::Syncing => EthPeerLifecycleStageV1::Syncing,
    }
}

fn eth_peer_effective_stage_v1(state: &EthPeerSessionState, now: u64) -> EthPeerLifecycleStageV1 {
    if state.permanently_rejected {
        return EthPeerLifecycleStageV1::PermanentlyRejected;
    }
    if state.cooldown_until_unix_ms > now {
        return EthPeerLifecycleStageV1::Cooldown;
    }
    if !state.session_ready
        && state.last_failure_class.is_some()
        && state.last_failure_unix_ms >= state.last_success_unix_ms
    {
        return EthPeerLifecycleStageV1::TemporarilyFailed;
    }
    eth_peer_progress_stage_to_lifecycle_stage_v1(state.progress_stage)
}

fn eth_peer_window_stats_from_rounds_v1(
    rounds: &std::collections::VecDeque<EthPeerRoundOutcomeV1>,
) -> EthPeerRecentWindowStatsV1 {
    let mut stats = EthPeerRecentWindowStatsV1::default();
    stats.window_rounds = rounds.len() as u64;
    for round in rounds {
        if round.selected_bootstrap || round.selected_sync {
            stats.selected_rounds = stats.selected_rounds.saturating_add(1);
            stats.last_selected_unix_ms = stats.last_selected_unix_ms.max(round.observed_unix_ms);
        }
        if round.selected_bootstrap {
            stats.selected_bootstrap_rounds = stats.selected_bootstrap_rounds.saturating_add(1);
        }
        if round.selected_sync {
            stats.selected_sync_rounds = stats.selected_sync_rounds.saturating_add(1);
        }
        if round.header_success {
            stats.header_success_rounds = stats.header_success_rounds.saturating_add(1);
            stats.last_progress_unix_ms = stats.last_progress_unix_ms.max(round.observed_unix_ms);
        }
        if round.body_success {
            stats.body_success_rounds = stats.body_success_rounds.saturating_add(1);
            stats.last_progress_unix_ms = stats.last_progress_unix_ms.max(round.observed_unix_ms);
        }
        if round.header_success || round.body_success {
            stats.sync_contribution_rounds = stats.sync_contribution_rounds.saturating_add(1);
        } else if round.selected_sync {
            stats.selected_without_progress_rounds =
                stats.selected_without_progress_rounds.saturating_add(1);
        }
        if round.connect_failure {
            stats.connect_failure_rounds = stats.connect_failure_rounds.saturating_add(1);
            stats.last_failure_unix_ms = stats.last_failure_unix_ms.max(round.observed_unix_ms);
        }
        if round.handshake_failure {
            stats.handshake_failure_rounds = stats.handshake_failure_rounds.saturating_add(1);
            stats.last_failure_unix_ms = stats.last_failure_unix_ms.max(round.observed_unix_ms);
        }
        if round.decode_failure {
            stats.decode_failure_rounds = stats.decode_failure_rounds.saturating_add(1);
            stats.last_failure_unix_ms = stats.last_failure_unix_ms.max(round.observed_unix_ms);
        }
        if round.timeout_failure {
            stats.timeout_failure_rounds = stats.timeout_failure_rounds.saturating_add(1);
            stats.last_failure_unix_ms = stats.last_failure_unix_ms.max(round.observed_unix_ms);
        }
        if round.validation_reject {
            stats.validation_reject_rounds = stats.validation_reject_rounds.saturating_add(1);
            stats.last_failure_unix_ms = stats.last_failure_unix_ms.max(round.observed_unix_ms);
        }
        if round.disconnect {
            stats.disconnect_rounds = stats.disconnect_rounds.saturating_add(1);
            stats.last_failure_unix_ms = stats.last_failure_unix_ms.max(round.observed_unix_ms);
        }
        if round.capacity_reject {
            stats.capacity_reject_rounds = stats.capacity_reject_rounds.saturating_add(1);
        }
    }
    if stats.selected_rounds > 0 {
        stats.selection_hit_rate_bps = stats
            .sync_contribution_rounds
            .saturating_mul(10_000)
            .saturating_div(stats.selected_rounds);
    }
    if stats.selected_sync_rounds > 0 {
        stats.header_success_rate_bps = stats
            .header_success_rounds
            .saturating_mul(10_000)
            .saturating_div(stats.selected_sync_rounds);
        stats.body_success_rate_bps = stats
            .body_success_rounds
            .saturating_mul(10_000)
            .saturating_div(stats.selected_sync_rounds);
    }
    stats
}

fn eth_peer_recent_window_stats_v1(state: &EthPeerSessionState) -> EthPeerRecentWindowStatsV1 {
    eth_peer_window_stats_from_rounds_v1(&state.recent_rounds)
}

fn eth_peer_push_round_outcome_to_window_v1(
    rounds: &mut std::collections::VecDeque<EthPeerRoundOutcomeV1>,
    max_rounds: usize,
    outcome: &EthPeerRoundOutcomeV1,
) {
    if rounds.len() >= max_rounds {
        rounds.pop_front();
    }
    rounds.push_back(outcome.clone());
}

fn eth_peer_push_round_outcome_v1(state: &mut EthPeerSessionState, outcome: EthPeerRoundOutcomeV1) {
    eth_peer_push_round_outcome_to_window_v1(
        &mut state.recent_rounds,
        ETH_PEER_SELECTION_SHORT_WINDOW_ROUNDS_V1,
        &outcome,
    );
    eth_peer_push_round_outcome_to_window_v1(
        &mut state.medium_rounds,
        ETH_PEER_SELECTION_MEDIUM_WINDOW_ROUNDS_V1,
        &outcome,
    );
    eth_peer_push_round_outcome_to_window_v1(
        &mut state.long_rounds,
        ETH_PEER_SELECTION_LONG_WINDOW_ROUNDS_V1,
        &outcome,
    );
}

fn eth_peer_update_round_streak_v1(active: bool, current: &mut u64, max: &mut u64) {
    if active {
        *current = current.saturating_add(1);
        *max = (*max).max(*current);
    } else {
        *current = 0;
    }
}

fn eth_peer_update_long_term_round_stats_v1(
    state: &mut EthPeerSessionState,
    outcome: &EthPeerRoundOutcomeV1,
) {
    let stats = &mut state.long_term;
    stats.total_observed_rounds = stats.total_observed_rounds.saturating_add(1);

    let selected = outcome.selected_bootstrap || outcome.selected_sync;
    if selected {
        stats.total_selected_rounds = stats.total_selected_rounds.saturating_add(1);
        stats.last_selected_unix_ms = stats.last_selected_unix_ms.max(outcome.observed_unix_ms);
    }
    if outcome.selected_bootstrap {
        stats.total_selected_bootstrap_rounds =
            stats.total_selected_bootstrap_rounds.saturating_add(1);
    }
    if outcome.selected_sync {
        stats.total_selected_sync_rounds = stats.total_selected_sync_rounds.saturating_add(1);
    }

    if outcome.header_success {
        stats.total_header_success_rounds = stats.total_header_success_rounds.saturating_add(1);
        stats.last_progress_unix_ms = stats.last_progress_unix_ms.max(outcome.observed_unix_ms);
    }
    if outcome.body_success {
        stats.total_body_success_rounds = stats.total_body_success_rounds.saturating_add(1);
        stats.last_progress_unix_ms = stats.last_progress_unix_ms.max(outcome.observed_unix_ms);
    }
    if outcome.header_success || outcome.body_success {
        stats.total_sync_contribution_rounds =
            stats.total_sync_contribution_rounds.saturating_add(1);
    }

    let selected_without_progress =
        outcome.selected_sync && !outcome.header_success && !outcome.body_success;
    if selected_without_progress {
        stats.total_selected_without_progress_rounds = stats
            .total_selected_without_progress_rounds
            .saturating_add(1);
    }
    eth_peer_update_round_streak_v1(
        selected_without_progress,
        &mut stats.current_consecutive_selected_without_progress_rounds,
        &mut stats.max_consecutive_selected_without_progress_rounds,
    );

    if outcome.connect_failure {
        stats.total_connect_failure_rounds = stats.total_connect_failure_rounds.saturating_add(1);
    }
    if outcome.handshake_failure {
        stats.total_handshake_failure_rounds =
            stats.total_handshake_failure_rounds.saturating_add(1);
    }
    if outcome.decode_failure {
        stats.total_decode_failure_rounds = stats.total_decode_failure_rounds.saturating_add(1);
    }
    if outcome.timeout_failure {
        stats.total_timeout_failure_rounds = stats.total_timeout_failure_rounds.saturating_add(1);
    }
    if outcome.validation_reject {
        stats.total_validation_reject_rounds =
            stats.total_validation_reject_rounds.saturating_add(1);
    }
    if outcome.disconnect {
        stats.total_disconnect_rounds = stats.total_disconnect_rounds.saturating_add(1);
    }
    if outcome.capacity_reject {
        stats.total_capacity_reject_rounds = stats.total_capacity_reject_rounds.saturating_add(1);
    }

    let any_failure = outcome.connect_failure
        || outcome.handshake_failure
        || outcome.decode_failure
        || outcome.timeout_failure
        || outcome.validation_reject
        || outcome.disconnect;
    if any_failure {
        stats.last_failure_unix_ms = stats.last_failure_unix_ms.max(outcome.observed_unix_ms);
    }

    eth_peer_update_round_streak_v1(
        outcome.connect_failure,
        &mut stats.current_consecutive_connect_failures,
        &mut stats.max_consecutive_connect_failures,
    );
    eth_peer_update_round_streak_v1(
        outcome.handshake_failure,
        &mut stats.current_consecutive_handshake_failures,
        &mut stats.max_consecutive_handshake_failures,
    );
    eth_peer_update_round_streak_v1(
        outcome.decode_failure,
        &mut stats.current_consecutive_decode_failures,
        &mut stats.max_consecutive_decode_failures,
    );
    eth_peer_update_round_streak_v1(
        outcome.timeout_failure,
        &mut stats.current_consecutive_timeout_failures,
        &mut stats.max_consecutive_timeout_failures,
    );
    eth_peer_update_round_streak_v1(
        outcome.validation_reject,
        &mut stats.current_consecutive_validation_rejects,
        &mut stats.max_consecutive_validation_rejects,
    );
    eth_peer_update_round_streak_v1(
        outcome.disconnect,
        &mut stats.current_consecutive_disconnects,
        &mut stats.max_consecutive_disconnects,
    );

    if stats.total_selected_rounds > 0 {
        stats.selection_hit_rate_bps = stats
            .total_sync_contribution_rounds
            .saturating_mul(10_000)
            .saturating_div(stats.total_selected_rounds);
    }
    if stats.total_selected_sync_rounds > 0 {
        stats.header_success_rate_bps = stats
            .total_header_success_rounds
            .saturating_mul(10_000)
            .saturating_div(stats.total_selected_sync_rounds);
        stats.body_success_rate_bps = stats
            .total_body_success_rounds
            .saturating_mul(10_000)
            .saturating_div(stats.total_selected_sync_rounds);
    }
}

fn eth_peer_long_term_signal_v1(stats: &EthPeerLongTermStatsV1) -> i64 {
    let mut score = 0i64;
    score += (stats.total_sync_contribution_rounds.min(4_096) * 45) as i64;
    score += (stats.total_header_success_rounds.min(4_096) * 35) as i64;
    score += (stats.total_body_success_rounds.min(4_096) * 70) as i64;
    score += (stats.selection_hit_rate_bps / 22) as i64;
    score += (stats.header_success_rate_bps / 28) as i64;
    score += (stats.body_success_rate_bps / 20) as i64;

    if stats.total_observed_rounds >= 16 {
        score += 300;
    }
    if stats.total_observed_rounds >= 64 {
        score += 700;
    }
    if stats.total_observed_rounds >= 128 {
        score += 1_200;
    }

    score -= (stats.total_selected_without_progress_rounds.min(4_096) * 35) as i64;
    score -= (stats.total_capacity_reject_rounds.min(2_048) * 40) as i64;
    score -= (stats.current_consecutive_connect_failures * 400) as i64;
    score -= (stats.current_consecutive_handshake_failures * 550) as i64;
    score -= (stats.current_consecutive_decode_failures * 650) as i64;
    score -= (stats.current_consecutive_timeout_failures * 450) as i64;
    score -= (stats.current_consecutive_validation_rejects * 6_000) as i64;
    score -= (stats.current_consecutive_disconnects * 250) as i64;
    score -= (stats.current_consecutive_selected_without_progress_rounds * 300) as i64;
    score -= (stats.max_consecutive_timeout_failures.min(64) * 25) as i64;
    score -= (stats.max_consecutive_decode_failures.min(64) * 40) as i64;
    score
}

#[must_use]
pub fn default_eth_peer_selection_window_policy_v1() -> EthPeerSelectionWindowPolicyV1 {
    EthPeerSelectionWindowPolicyV1::default()
}

fn eth_peer_weighted_window_signal_v1(signal: i64, weight_bps: u64) -> i64 {
    signal.saturating_mul(weight_bps.min(100_000) as i64) / 10_000
}

fn eth_peer_calibration_delta_from_floor_v1(
    observed_bps: u64,
    floor_bps: u64,
    positive_divisor: i64,
    negative_divisor: i64,
    max_abs: i64,
) -> i64 {
    let diff = observed_bps as i64 - floor_bps as i64;
    let divisor = if diff >= 0 {
        positive_divisor.max(1)
    } else {
        negative_divisor.max(1)
    };
    (diff / divisor).clamp(-max_abs, max_abs)
}

fn eth_peer_medium_term_calibration_delta_v1(
    stats: &EthPeerRecentWindowStatsV1,
    policy: &EthPeerSelectionWindowPolicyV1,
) -> i64 {
    if stats.window_rounds < policy.short_term_rounds {
        return 0;
    }
    eth_peer_calibration_delta_from_floor_v1(
        stats.selection_hit_rate_bps,
        policy.medium_term_selection_hit_rate_floor_bps,
        30,
        18,
        1_500,
    )
}

fn eth_peer_long_term_calibration_delta_v1(
    stats: &EthPeerRecentWindowStatsV1,
    policy: &EthPeerSelectionWindowPolicyV1,
) -> i64 {
    if stats.window_rounds < policy.medium_term_rounds {
        return 0;
    }
    let selection_delta = eth_peer_calibration_delta_from_floor_v1(
        stats.selection_hit_rate_bps,
        policy.long_term_selection_hit_rate_floor_bps,
        35,
        20,
        1_800,
    );
    let body_delta = eth_peer_calibration_delta_from_floor_v1(
        stats.body_success_rate_bps,
        policy.long_term_body_success_rate_floor_bps,
        35,
        20,
        1_800,
    );
    selection_delta + body_delta
}

fn eth_peer_short_term_veto_active_v1(stats: &EthPeerRecentWindowStatsV1) -> bool {
    stats.validation_reject_rounds > 0
        || stats.decode_failure_rounds > 0
        || stats.timeout_failure_rounds >= 2
        || stats.selected_without_progress_rounds >= 3
}

fn eth_peer_medium_term_stable_v1(stats: &EthPeerRecentWindowStatsV1) -> bool {
    stats.window_rounds >= 16
        && stats.selection_hit_rate_bps >= 4_500
        && stats.header_success_rate_bps >= 4_000
        && stats.timeout_failure_rounds <= 2
        && stats.decode_failure_rounds == 0
        && stats.validation_reject_rounds == 0
}

fn eth_peer_long_term_trusted_v1(stats: &EthPeerRecentWindowStatsV1) -> bool {
    stats.window_rounds >= 64
        && stats.selection_hit_rate_bps >= 4_000
        && stats.body_success_rate_bps >= 2_000
        && stats.validation_reject_rounds == 0
        && stats.decode_failure_rounds == 0
}

fn eth_peer_short_window_signal_v1(stats: &EthPeerRecentWindowStatsV1) -> i64 {
    let mut score = 0i64;
    score += (stats.header_success_rounds.min(16) * 120) as i64;
    score += (stats.body_success_rounds.min(16) * 180) as i64;
    score += (stats.selection_hit_rate_bps / 28) as i64;
    score += (stats.body_success_rate_bps / 24) as i64;
    score -= (stats.selected_without_progress_rounds.min(16) * 700) as i64;
    score -= (stats.timeout_failure_rounds.min(16) * 900) as i64;
    score -= (stats.decode_failure_rounds.min(16) * 1_200) as i64;
    score -= (stats.validation_reject_rounds.min(16) * 10_000) as i64;
    score -= (stats.capacity_reject_rounds.min(16) * 150) as i64;
    if eth_peer_short_term_veto_active_v1(stats) {
        score -= 25_000;
    }
    score
}

fn eth_peer_medium_window_signal_v1(stats: &EthPeerRecentWindowStatsV1) -> i64 {
    let mut score = 0i64;
    score += (stats.selection_hit_rate_bps / 35) as i64;
    score += (stats.header_success_rate_bps / 40) as i64;
    score += (stats.body_success_rate_bps / 45) as i64;
    score += (stats.sync_contribution_rounds.min(64) * 40) as i64;
    score -= (stats.selected_without_progress_rounds.min(64) * 250) as i64;
    score -= (stats.timeout_failure_rounds.min(64) * 320) as i64;
    score -= (stats.decode_failure_rounds.min(64) * 450) as i64;
    score -= (stats.capacity_reject_rounds.min(64) * 80) as i64;
    if eth_peer_medium_term_stable_v1(stats) {
        score += 1_200;
    }
    score
}

fn eth_peer_long_window_signal_v1(stats: &EthPeerRecentWindowStatsV1) -> i64 {
    let mut score = 0i64;
    score += (stats.selection_hit_rate_bps / 42) as i64;
    score += (stats.header_success_rate_bps / 45) as i64;
    score += (stats.body_success_rate_bps / 32) as i64;
    score += (stats.sync_contribution_rounds.min(256) * 22) as i64;
    score -= (stats.selected_without_progress_rounds.min(256) * 90) as i64;
    score -= (stats.timeout_failure_rounds.min(256) * 110) as i64;
    score -= (stats.decode_failure_rounds.min(256) * 150) as i64;
    score -= (stats.capacity_reject_rounds.min(256) * 35) as i64;
    if eth_peer_long_term_trusted_v1(stats) {
        score += 1_800;
    }
    score
}

fn eth_peer_window_layers_from_state_v1(
    state: &EthPeerSessionState,
) -> EthPeerSelectionWindowLayersV1 {
    let short_stats = eth_peer_window_stats_from_rounds_v1(&state.recent_rounds);
    let medium_stats = eth_peer_window_stats_from_rounds_v1(&state.medium_rounds);
    let long_stats = eth_peer_window_stats_from_rounds_v1(&state.long_rounds);
    EthPeerSelectionWindowLayersV1 {
        short_term: EthPeerSelectionWindowLayerV1 {
            role: EthPeerSelectionWindowRoleV1::ShortTermVeto,
            max_rounds: ETH_PEER_SELECTION_SHORT_WINDOW_ROUNDS_V1 as u64,
            signal_score: eth_peer_short_window_signal_v1(&short_stats),
            active: eth_peer_short_term_veto_active_v1(&short_stats),
            stats: short_stats,
        },
        medium_term: EthPeerSelectionWindowLayerV1 {
            role: EthPeerSelectionWindowRoleV1::MediumTermStability,
            max_rounds: ETH_PEER_SELECTION_MEDIUM_WINDOW_ROUNDS_V1 as u64,
            signal_score: eth_peer_medium_window_signal_v1(&medium_stats),
            active: eth_peer_medium_term_stable_v1(&medium_stats),
            stats: medium_stats,
        },
        long_term: EthPeerSelectionWindowLayerV1 {
            role: EthPeerSelectionWindowRoleV1::LongTermRetention,
            max_rounds: ETH_PEER_SELECTION_LONG_WINDOW_ROUNDS_V1 as u64,
            signal_score: eth_peer_long_window_signal_v1(&long_stats),
            active: eth_peer_long_term_trusted_v1(&long_stats),
            stats: long_stats,
        },
    }
}

fn build_eth_fullnode_peer_selection_long_term_summary_v1(
    chain_id: u64,
    scores: &[EthPeerSelectionScoreV1],
) -> EthPeerSelectionLongTermSummaryV1 {
    let sync_scores = scores
        .iter()
        .filter(|score| matches!(score.role, EthPeerSelectionRoleV1::Sync))
        .collect::<Vec<_>>();
    let mut summary = EthPeerSelectionLongTermSummaryV1 {
        chain_id,
        tracked_sync_peers: sync_scores.len() as u64,
        ..EthPeerSelectionLongTermSummaryV1::default()
    };
    let mut rate_peer_count = 0u64;
    let mut long_term_score_sum = 0i64;

    for score in sync_scores {
        let long_term = &score.long_term;
        summary.observed_rounds_total = summary
            .observed_rounds_total
            .saturating_add(long_term.total_observed_rounds);
        summary.selected_rounds_total = summary
            .selected_rounds_total
            .saturating_add(long_term.total_selected_rounds);
        summary.selected_sync_rounds_total = summary
            .selected_sync_rounds_total
            .saturating_add(long_term.total_selected_sync_rounds);
        summary.sync_contribution_rounds_total = summary
            .sync_contribution_rounds_total
            .saturating_add(long_term.total_sync_contribution_rounds);
        summary.selected_without_progress_rounds_total = summary
            .selected_without_progress_rounds_total
            .saturating_add(long_term.total_selected_without_progress_rounds);
        summary.connect_failure_rounds_total = summary
            .connect_failure_rounds_total
            .saturating_add(long_term.total_connect_failure_rounds);
        summary.handshake_failure_rounds_total = summary
            .handshake_failure_rounds_total
            .saturating_add(long_term.total_handshake_failure_rounds);
        summary.decode_failure_rounds_total = summary
            .decode_failure_rounds_total
            .saturating_add(long_term.total_decode_failure_rounds);
        summary.timeout_failure_rounds_total = summary
            .timeout_failure_rounds_total
            .saturating_add(long_term.total_timeout_failure_rounds);
        summary.validation_reject_rounds_total = summary
            .validation_reject_rounds_total
            .saturating_add(long_term.total_validation_reject_rounds);
        summary.disconnect_rounds_total = summary
            .disconnect_rounds_total
            .saturating_add(long_term.total_disconnect_rounds);
        summary.capacity_reject_rounds_total = summary
            .capacity_reject_rounds_total
            .saturating_add(long_term.total_capacity_reject_rounds);

        if long_term.total_observed_rounds > 0 {
            summary.peers_with_history = summary.peers_with_history.saturating_add(1);
            rate_peer_count = rate_peer_count.saturating_add(1);
            summary.average_selection_hit_rate_bps = summary
                .average_selection_hit_rate_bps
                .saturating_add(long_term.selection_hit_rate_bps);
            summary.average_header_success_rate_bps = summary
                .average_header_success_rate_bps
                .saturating_add(long_term.header_success_rate_bps);
            summary.average_body_success_rate_bps = summary
                .average_body_success_rate_bps
                .saturating_add(long_term.body_success_rate_bps);
            long_term_score_sum += score.long_term_score;
        }
        if long_term.total_sync_contribution_rounds > 0 {
            summary.peers_with_positive_contribution =
                summary.peers_with_positive_contribution.saturating_add(1);
        }
        if long_term.current_consecutive_connect_failures > 0
            || long_term.current_consecutive_handshake_failures > 0
            || long_term.current_consecutive_decode_failures > 0
            || long_term.current_consecutive_timeout_failures > 0
            || long_term.current_consecutive_validation_rejects > 0
            || long_term.current_consecutive_disconnects > 0
        {
            summary.peers_currently_in_failure_streak =
                summary.peers_currently_in_failure_streak.saturating_add(1);
        }
        if long_term.current_consecutive_selected_without_progress_rounds > 0 {
            summary.peers_currently_in_progressless_streak = summary
                .peers_currently_in_progressless_streak
                .saturating_add(1);
        }
        if summary
            .top_trusted_sync_long_term_score
            .is_none_or(|current| score.long_term_score > current)
        {
            summary.top_trusted_sync_peer_id = Some(score.peer_id);
            summary.top_trusted_sync_long_term_score = Some(score.long_term_score);
        }
    }

    if rate_peer_count > 0 {
        summary.average_selection_hit_rate_bps = summary
            .average_selection_hit_rate_bps
            .saturating_div(rate_peer_count);
        summary.average_header_success_rate_bps = summary
            .average_header_success_rate_bps
            .saturating_div(rate_peer_count);
        summary.average_body_success_rate_bps = summary
            .average_body_success_rate_bps
            .saturating_div(rate_peer_count);
        summary.average_long_term_score = Some(long_term_score_sum / rate_peer_count as i64);
    }

    summary
}

fn eth_peer_retry_eligible_v1(state: &EthPeerSessionState, now: u64) -> bool {
    if state.permanently_rejected || state.cooldown_until_unix_ms > now {
        return false;
    }
    matches!(
        eth_peer_effective_stage_v1(state, now),
        EthPeerLifecycleStageV1::Discovered | EthPeerLifecycleStageV1::TemporarilyFailed
    )
}

fn eth_peer_mark_progress_stage_v1(
    state: &mut EthPeerSessionState,
    stage: EthPeerLifecycleProgressStageV1,
    now: u64,
) {
    state.progress_stage = stage;
    state.last_state_change_unix_ms = now;
}

fn eth_peer_record_failure_v1(
    state: &mut EthPeerSessionState,
    class: EthPeerFailureClassV1,
    reason_code: Option<u64>,
    reason_name: impl Into<String>,
    cooldown_ms: u64,
    permanent: bool,
    now: u64,
) {
    state.session_ready = false;
    state.consecutive_failures = state.consecutive_failures.saturating_add(1);
    state.last_failure_class = Some(class);
    state.last_failure_reason_code = reason_code;
    state.last_failure_reason_name = Some(reason_name.into());
    if state.first_failure_unix_ms == 0 {
        state.first_failure_unix_ms = now;
    }
    state.last_failure_unix_ms = now;
    state.last_state_change_unix_ms = now;
    match class {
        EthPeerFailureClassV1::ConnectFailure => {
            state.connect_failure_count = state.connect_failure_count.saturating_add(1);
        }
        EthPeerFailureClassV1::HandshakeFailure => {
            state.handshake_failure_count = state.handshake_failure_count.saturating_add(1);
        }
        EthPeerFailureClassV1::DecodeFailure => {
            state.decode_failure_count = state.decode_failure_count.saturating_add(1);
        }
        EthPeerFailureClassV1::Timeout => {
            state.timeout_count = state.timeout_count.saturating_add(1);
        }
        EthPeerFailureClassV1::ValidationReject => {
            state.validation_reject_count = state.validation_reject_count.saturating_add(1);
        }
        EthPeerFailureClassV1::Disconnect => {
            state.disconnect_count = state.disconnect_count.saturating_add(1);
            if reason_code == Some(0x04) {
                state.disconnect_too_many_peers_count =
                    state.disconnect_too_many_peers_count.saturating_add(1);
            }
            state.last_disconnect_reason_code = reason_code;
        }
    }
    if permanent {
        state.permanently_rejected = true;
        state.cooldown_until_unix_ms = u64::MAX;
    } else {
        state.cooldown_until_unix_ms = now.saturating_add(cooldown_ms);
    }
}

fn eth_peer_session_snapshot_from_state_v1(
    chain_id: u64,
    peer_id: u64,
    state: &EthPeerSessionState,
    now: u64,
) -> EthPeerSessionSnapshot {
    let window_layers = eth_peer_window_layers_from_state_v1(state);
    let mut long_term = state.long_term.clone();
    long_term.medium_window = window_layers.medium_term.stats.clone();
    long_term.retention_window = window_layers.long_term.stats.clone();
    long_term.short_term_veto_active = window_layers.short_term.active;
    long_term.medium_term_stable = window_layers.medium_term.active;
    long_term.long_term_trusted = window_layers.long_term.active;
    EthPeerSessionSnapshot {
        chain_id,
        peer_id,
        negotiated: state.negotiated.clone(),
        lifecycle_stage: eth_peer_effective_stage_v1(state, now),
        retry_eligible: eth_peer_retry_eligible_v1(state, now),
        session_ready: state.session_ready,
        last_head_height: state.last_head_height,
        successful_sessions: state.successful_sessions,
        header_response_count: state.header_response_count,
        body_response_count: state.body_response_count,
        sync_contribution_count: state.sync_contribution_count,
        connect_failure_count: state.connect_failure_count,
        handshake_failure_count: state.handshake_failure_count,
        decode_failure_count: state.decode_failure_count,
        timeout_count: state.timeout_count,
        validation_reject_count: state.validation_reject_count,
        last_validation_reject_reason: state.last_validation_reject_reason,
        disconnect_count: state.disconnect_count,
        disconnect_too_many_peers_count: state.disconnect_too_many_peers_count,
        last_disconnect_reason_code: state.last_disconnect_reason_code,
        last_failure_class: state.last_failure_class,
        last_failure_reason_code: state.last_failure_reason_code,
        last_failure_reason_name: state.last_failure_reason_name.clone(),
        consecutive_failures: state.consecutive_failures,
        first_seen_unix_ms: state.first_seen_unix_ms,
        first_failure_unix_ms: state.first_failure_unix_ms,
        cooldown_until_unix_ms: state.cooldown_until_unix_ms,
        permanently_rejected: state.permanently_rejected,
        last_success_unix_ms: state.last_success_unix_ms,
        last_header_success_unix_ms: state.last_header_success_unix_ms,
        last_body_success_unix_ms: state.last_body_success_unix_ms,
        last_failure_unix_ms: state.last_failure_unix_ms,
        last_state_change_unix_ms: state.last_state_change_unix_ms,
        recent_window: eth_peer_recent_window_stats_v1(state),
        long_term,
        window_layers,
    }
}

fn eth_peer_synthetic_discovered_snapshot_v1(
    chain_id: u64,
    peer_id: u64,
) -> EthPeerSessionSnapshot {
    EthPeerSessionSnapshot {
        chain_id,
        peer_id,
        negotiated: default_eth_peer_negotiated_capabilities_v1(),
        lifecycle_stage: EthPeerLifecycleStageV1::Discovered,
        retry_eligible: true,
        session_ready: false,
        last_head_height: 0,
        successful_sessions: 0,
        header_response_count: 0,
        body_response_count: 0,
        sync_contribution_count: 0,
        connect_failure_count: 0,
        handshake_failure_count: 0,
        decode_failure_count: 0,
        timeout_count: 0,
        validation_reject_count: 0,
        last_validation_reject_reason: None,
        disconnect_count: 0,
        disconnect_too_many_peers_count: 0,
        last_disconnect_reason_code: None,
        last_failure_class: None,
        last_failure_reason_code: None,
        last_failure_reason_name: None,
        consecutive_failures: 0,
        first_seen_unix_ms: 0,
        first_failure_unix_ms: 0,
        cooldown_until_unix_ms: 0,
        permanently_rejected: false,
        last_success_unix_ms: 0,
        last_header_success_unix_ms: 0,
        last_body_success_unix_ms: 0,
        last_failure_unix_ms: 0,
        last_state_change_unix_ms: 0,
        recent_window: EthPeerRecentWindowStatsV1::default(),
        long_term: EthPeerLongTermStatsV1::default(),
        window_layers: EthPeerSelectionWindowLayersV1::default(),
    }
}

fn eth_peer_recent_success_bonus_v1(last_success_unix_ms: u64, now: u64) -> i64 {
    if last_success_unix_ms == 0 {
        return 0;
    }
    let age_ms = now.saturating_sub(last_success_unix_ms);
    if age_ms <= 5_000 {
        500
    } else if age_ms <= 30_000 {
        300
    } else if age_ms <= 120_000 {
        150
    } else {
        0
    }
}

fn eth_peer_recent_body_bonus_v1(last_body_success_unix_ms: u64, now: u64) -> i64 {
    if last_body_success_unix_ms == 0 {
        return 0;
    }
    let age_ms = now.saturating_sub(last_body_success_unix_ms);
    if age_ms <= 5_000 {
        650
    } else if age_ms <= 30_000 {
        350
    } else if age_ms <= 120_000 {
        175
    } else {
        0
    }
}

fn eth_peer_sync_score_v1(snapshot: &EthPeerSessionSnapshot, now: u64) -> EthPeerSelectionScoreV1 {
    let recent_window = snapshot.recent_window.clone();
    let long_term = snapshot.long_term.clone();
    let window_layers = snapshot.window_layers.clone();
    let policy =
        resolve_eth_fullnode_native_runtime_config_v1(snapshot.chain_id).selection_window_policy;
    let eligible = matches!(
        snapshot.lifecycle_stage,
        EthPeerLifecycleStageV1::Ready | EthPeerLifecycleStageV1::Syncing
    ) && snapshot.session_ready
        && !snapshot.permanently_rejected
        && snapshot.cooldown_until_unix_ms <= now;
    let mut reasons = Vec::new();
    let mut score = 0i64;

    if eligible {
        reasons.push("eligible_ready_session".to_string());
    } else if snapshot.permanently_rejected {
        reasons.push("skipped_permanently_rejected".to_string());
        score -= 1_000_000;
    } else if snapshot.cooldown_until_unix_ms > now {
        reasons.push("skipped_cooldown".to_string());
        score -= 250_000;
    } else if !snapshot.session_ready {
        reasons.push("skipped_session_not_ready".to_string());
        score -= 125_000;
    } else {
        reasons.push(format!(
            "skipped_stage={}",
            snapshot.lifecycle_stage.as_str()
        ));
        score -= 100_000;
    }

    if snapshot.last_head_height > 0 {
        reasons.push(format!("head_height={}", snapshot.last_head_height));
    }
    if snapshot.successful_sessions > 0 {
        reasons.push(format!(
            "successful_sessions={}",
            snapshot.successful_sessions
        ));
    }
    if snapshot.header_response_count > 0 {
        reasons.push(format!(
            "header_response_count={}",
            snapshot.header_response_count
        ));
    }
    if snapshot.body_response_count > 0 {
        reasons.push(format!(
            "body_response_count={}",
            snapshot.body_response_count
        ));
    }

    score += (snapshot.last_head_height.min(200_000) / 8) as i64;
    score += (snapshot.successful_sessions.min(1_000) * 700) as i64;
    score += (snapshot.header_response_count.min(1_000) * 180) as i64;
    score += (snapshot.body_response_count.min(1_000) * 320) as i64;
    score += (snapshot.sync_contribution_count.min(2_000) * 90) as i64;
    score += eth_peer_recent_success_bonus_v1(snapshot.last_success_unix_ms, now);
    score += eth_peer_recent_body_bonus_v1(snapshot.last_body_success_unix_ms, now);
    score += (snapshot.recent_window.selection_hit_rate_bps / 25) as i64;
    score += (snapshot.recent_window.body_success_rate_bps / 30) as i64;
    score += (snapshot.recent_window.header_success_rate_bps / 40) as i64;
    score += (snapshot.recent_window.sync_contribution_rounds.min(64) * 120) as i64;
    let short_term_weighted_signal = eth_peer_weighted_window_signal_v1(
        window_layers.short_term.signal_score,
        policy.sync_short_term_weight_bps,
    );
    let medium_term_weighted_signal = eth_peer_weighted_window_signal_v1(
        window_layers.medium_term.signal_score,
        policy.sync_medium_term_weight_bps,
    );
    let long_term_weighted_signal = eth_peer_weighted_window_signal_v1(
        window_layers.long_term.signal_score,
        policy.sync_long_term_weight_bps,
    );
    let medium_term_calibration_delta =
        eth_peer_medium_term_calibration_delta_v1(&window_layers.medium_term.stats, &policy);
    let long_term_calibration_delta =
        eth_peer_long_term_calibration_delta_v1(&window_layers.long_term.stats, &policy);
    score += short_term_weighted_signal;
    score += medium_term_weighted_signal;
    score += long_term_weighted_signal;
    score += medium_term_calibration_delta;
    score += long_term_calibration_delta;
    let long_term_score = eth_peer_long_term_signal_v1(&long_term);
    score += long_term_score;

    if snapshot.session_ready {
        score += 1_500;
    }
    if matches!(snapshot.lifecycle_stage, EthPeerLifecycleStageV1::Syncing) {
        score += 500;
    }

    let penalty = snapshot.connect_failure_count.saturating_mul(900)
        + snapshot.handshake_failure_count.saturating_mul(1_100)
        + snapshot.decode_failure_count.saturating_mul(1_200)
        + snapshot.timeout_count.saturating_mul(950)
        + snapshot.validation_reject_count.saturating_mul(10_000)
        + snapshot.disconnect_too_many_peers_count.saturating_mul(250)
        + snapshot.disconnect_count.saturating_mul(300)
        + snapshot.consecutive_failures.saturating_mul(700)
        + snapshot
            .recent_window
            .selected_without_progress_rounds
            .saturating_mul(400)
        + snapshot
            .recent_window
            .timeout_failure_rounds
            .saturating_mul(550)
        + snapshot
            .recent_window
            .decode_failure_rounds
            .saturating_mul(650)
        + snapshot
            .recent_window
            .capacity_reject_rounds
            .saturating_mul(180);
    if snapshot.connect_failure_count > 0 {
        reasons.push(format!(
            "penalty_connect_failure_count={}",
            snapshot.connect_failure_count
        ));
    }
    if snapshot.handshake_failure_count > 0 {
        reasons.push(format!(
            "penalty_handshake_failure_count={}",
            snapshot.handshake_failure_count
        ));
    }
    if snapshot.decode_failure_count > 0 {
        reasons.push(format!(
            "penalty_decode_failure_count={}",
            snapshot.decode_failure_count
        ));
    }
    if snapshot.timeout_count > 0 {
        reasons.push(format!("penalty_timeout_count={}", snapshot.timeout_count));
    }
    if snapshot.disconnect_too_many_peers_count > 0 {
        reasons.push(format!(
            "penalty_capacity_rejections={}",
            snapshot.disconnect_too_many_peers_count
        ));
    }
    if snapshot.validation_reject_count > 0 {
        reasons.push(format!(
            "penalty_validation_reject_count={}",
            snapshot.validation_reject_count
        ));
    }
    if snapshot.recent_window.window_rounds > 0 {
        reasons.push(format!(
            "recent_selection_hit_rate_bps={}",
            snapshot.recent_window.selection_hit_rate_bps
        ));
        reasons.push(format!(
            "recent_body_success_rate_bps={}",
            snapshot.recent_window.body_success_rate_bps
        ));
    }
    if window_layers.short_term.stats.window_rounds > 0 {
        reasons.push(format!(
            "short_term_signal_score={}",
            window_layers.short_term.signal_score
        ));
        reasons.push(format!(
            "short_term_weighted_signal={short_term_weighted_signal}"
        ));
        if window_layers.short_term.active {
            reasons.push("short_term_veto_active".to_string());
        }
    }
    if window_layers.medium_term.stats.window_rounds > 0 {
        reasons.push(format!(
            "medium_term_signal_score={}",
            window_layers.medium_term.signal_score
        ));
        reasons.push(format!(
            "medium_term_weighted_signal={medium_term_weighted_signal}"
        ));
        reasons.push(format!(
            "medium_term_calibration_delta={medium_term_calibration_delta}"
        ));
        if window_layers.medium_term.active {
            reasons.push("medium_term_stable".to_string());
        }
    }
    if window_layers.long_term.stats.window_rounds > 0 {
        reasons.push(format!(
            "long_term_window_signal_score={}",
            window_layers.long_term.signal_score
        ));
        reasons.push(format!(
            "long_term_weighted_signal={long_term_weighted_signal}"
        ));
        reasons.push(format!(
            "long_term_calibration_delta={long_term_calibration_delta}"
        ));
        if window_layers.long_term.active {
            reasons.push("long_term_retention_trusted".to_string());
        }
    }
    if long_term.total_observed_rounds > 0 {
        reasons.push(format!(
            "long_term_selection_hit_rate_bps={}",
            long_term.selection_hit_rate_bps
        ));
        reasons.push(format!(
            "long_term_body_success_rate_bps={}",
            long_term.body_success_rate_bps
        ));
        reasons.push(format!(
            "long_term_observed_rounds={}",
            long_term.total_observed_rounds
        ));
    }
    score -= penalty as i64;

    EthPeerSelectionScoreV1 {
        chain_id: snapshot.chain_id,
        peer_id: snapshot.peer_id,
        role: EthPeerSelectionRoleV1::Sync,
        stage: snapshot.lifecycle_stage,
        eligible,
        selected: false,
        score,
        reasons,
        last_head_height: snapshot.last_head_height,
        successful_sessions: snapshot.successful_sessions,
        header_response_count: snapshot.header_response_count,
        body_response_count: snapshot.body_response_count,
        sync_contribution_count: snapshot.sync_contribution_count,
        consecutive_failures: snapshot.consecutive_failures,
        last_success_unix_ms: snapshot.last_success_unix_ms,
        last_failure_unix_ms: snapshot.last_failure_unix_ms,
        cooldown_until_unix_ms: snapshot.cooldown_until_unix_ms,
        permanently_rejected: snapshot.permanently_rejected,
        long_term_score,
        recent_window,
        long_term,
        window_layers,
    }
}

fn eth_peer_bootstrap_score_v1(
    chain_id: u64,
    snapshot: Option<&EthPeerSessionSnapshot>,
    peer_id: u64,
    now: u64,
) -> EthPeerSelectionScoreV1 {
    let snapshot = snapshot
        .cloned()
        .unwrap_or_else(|| eth_peer_synthetic_discovered_snapshot_v1(chain_id, peer_id));
    let recent_window = snapshot.recent_window.clone();
    let long_term = snapshot.long_term.clone();
    let window_layers = snapshot.window_layers.clone();
    let policy = resolve_eth_fullnode_native_runtime_config_v1(chain_id).selection_window_policy;
    let eligible = snapshot.retry_eligible
        && !snapshot.session_ready
        && !snapshot.permanently_rejected
        && snapshot.cooldown_until_unix_ms <= now;
    let mut reasons = Vec::new();
    let mut score = 0i64;

    if eligible {
        reasons.push("eligible_bootstrap_candidate".to_string());
    } else if snapshot.permanently_rejected {
        reasons.push("skipped_permanently_rejected".to_string());
        score -= 1_000_000;
    } else if snapshot.cooldown_until_unix_ms > now {
        reasons.push("skipped_cooldown".to_string());
        score -= 250_000;
    } else if snapshot.session_ready {
        reasons.push("skipped_session_already_ready".to_string());
        score -= 150_000;
    } else {
        reasons.push("skipped_retry_ineligible".to_string());
        score -= 100_000;
    }

    if snapshot.successful_sessions > 0 {
        reasons.push(format!(
            "prior_successful_sessions={}",
            snapshot.successful_sessions
        ));
    } else if snapshot.connect_failure_count == 0
        && snapshot.handshake_failure_count == 0
        && snapshot.decode_failure_count == 0
        && snapshot.timeout_count == 0
        && snapshot.validation_reject_count == 0
        && snapshot.disconnect_count == 0
    {
        reasons.push("pristine_candidate".to_string());
        score += 500;
    }

    score += (snapshot.successful_sessions.min(1_000) * 900) as i64;
    score += eth_peer_recent_success_bonus_v1(snapshot.last_success_unix_ms, now);
    score += (snapshot.recent_window.selection_hit_rate_bps / 35) as i64;
    score += (snapshot.recent_window.sync_contribution_rounds.min(64) * 80) as i64;
    let short_term_weighted_signal = eth_peer_weighted_window_signal_v1(
        window_layers.short_term.signal_score,
        policy.bootstrap_short_term_weight_bps,
    );
    let medium_term_weighted_signal = eth_peer_weighted_window_signal_v1(
        window_layers.medium_term.signal_score,
        policy.bootstrap_medium_term_weight_bps,
    );
    let long_term_weighted_signal = eth_peer_weighted_window_signal_v1(
        window_layers.long_term.signal_score,
        policy.bootstrap_long_term_weight_bps,
    );
    let medium_term_calibration_delta =
        eth_peer_medium_term_calibration_delta_v1(&window_layers.medium_term.stats, &policy) / 2;
    let long_term_calibration_delta =
        eth_peer_long_term_calibration_delta_v1(&window_layers.long_term.stats, &policy) / 3;
    score += short_term_weighted_signal;
    score += medium_term_weighted_signal;
    score += long_term_weighted_signal;
    score += medium_term_calibration_delta;
    score += long_term_calibration_delta;
    let long_term_score = eth_peer_long_term_signal_v1(&long_term) / 2;
    score += long_term_score;

    let penalty = snapshot.connect_failure_count.saturating_mul(1_100)
        + snapshot.handshake_failure_count.saturating_mul(1_000)
        + snapshot.decode_failure_count.saturating_mul(1_050)
        + snapshot.timeout_count.saturating_mul(900)
        + snapshot.validation_reject_count.saturating_mul(10_000)
        + snapshot.disconnect_too_many_peers_count.saturating_mul(250)
        + snapshot.disconnect_count.saturating_mul(300)
        + snapshot.consecutive_failures.saturating_mul(800)
        + snapshot
            .recent_window
            .connect_failure_rounds
            .saturating_mul(500)
        + snapshot
            .recent_window
            .handshake_failure_rounds
            .saturating_mul(600)
        + snapshot
            .recent_window
            .decode_failure_rounds
            .saturating_mul(700)
        + snapshot
            .recent_window
            .timeout_failure_rounds
            .saturating_mul(550);
    if snapshot.connect_failure_count > 0 {
        reasons.push(format!(
            "penalty_connect_failure_count={}",
            snapshot.connect_failure_count
        ));
    }
    if snapshot.handshake_failure_count > 0 {
        reasons.push(format!(
            "penalty_handshake_failure_count={}",
            snapshot.handshake_failure_count
        ));
    }
    if snapshot.decode_failure_count > 0 {
        reasons.push(format!(
            "penalty_decode_failure_count={}",
            snapshot.decode_failure_count
        ));
    }
    if snapshot.timeout_count > 0 {
        reasons.push(format!("penalty_timeout_count={}", snapshot.timeout_count));
    }
    if snapshot.disconnect_too_many_peers_count > 0 {
        reasons.push(format!(
            "penalty_capacity_rejections={}",
            snapshot.disconnect_too_many_peers_count
        ));
    }
    if snapshot.validation_reject_count > 0 {
        reasons.push(format!(
            "penalty_validation_reject_count={}",
            snapshot.validation_reject_count
        ));
    }
    if snapshot.recent_window.window_rounds > 0 {
        reasons.push(format!(
            "recent_connect_failure_rounds={}",
            snapshot.recent_window.connect_failure_rounds
        ));
        reasons.push(format!(
            "recent_timeout_failure_rounds={}",
            snapshot.recent_window.timeout_failure_rounds
        ));
    }
    if window_layers.short_term.stats.window_rounds > 0 {
        reasons.push(format!(
            "short_term_signal_score={}",
            window_layers.short_term.signal_score
        ));
        reasons.push(format!(
            "short_term_weighted_signal={short_term_weighted_signal}"
        ));
        if window_layers.short_term.active {
            reasons.push("short_term_veto_active".to_string());
        }
    }
    if window_layers.medium_term.stats.window_rounds > 0 {
        reasons.push(format!(
            "medium_term_signal_score={}",
            window_layers.medium_term.signal_score
        ));
        reasons.push(format!(
            "medium_term_weighted_signal={medium_term_weighted_signal}"
        ));
        reasons.push(format!(
            "medium_term_calibration_delta={medium_term_calibration_delta}"
        ));
        if window_layers.medium_term.active {
            reasons.push("medium_term_stable".to_string());
        }
    }
    if window_layers.long_term.stats.window_rounds > 0 {
        reasons.push(format!(
            "long_term_window_signal_score={}",
            window_layers.long_term.signal_score
        ));
        reasons.push(format!(
            "long_term_weighted_signal={long_term_weighted_signal}"
        ));
        reasons.push(format!(
            "long_term_calibration_delta={long_term_calibration_delta}"
        ));
        if window_layers.long_term.active {
            reasons.push("long_term_retention_trusted".to_string());
        }
    }
    if long_term.total_observed_rounds > 0 {
        reasons.push(format!(
            "long_term_selection_hit_rate_bps={}",
            long_term.selection_hit_rate_bps
        ));
        reasons.push(format!(
            "long_term_observed_rounds={}",
            long_term.total_observed_rounds
        ));
    }
    score -= penalty as i64;

    EthPeerSelectionScoreV1 {
        chain_id,
        peer_id,
        role: EthPeerSelectionRoleV1::Bootstrap,
        stage: snapshot.lifecycle_stage,
        eligible,
        selected: false,
        score,
        reasons,
        last_head_height: snapshot.last_head_height,
        successful_sessions: snapshot.successful_sessions,
        header_response_count: snapshot.header_response_count,
        body_response_count: snapshot.body_response_count,
        sync_contribution_count: snapshot.sync_contribution_count,
        consecutive_failures: snapshot.consecutive_failures,
        last_success_unix_ms: snapshot.last_success_unix_ms,
        last_failure_unix_ms: snapshot.last_failure_unix_ms,
        cooldown_until_unix_ms: snapshot.cooldown_until_unix_ms,
        permanently_rejected: snapshot.permanently_rejected,
        long_term_score,
        recent_window,
        long_term,
        window_layers,
    }
}

fn eth_peer_selection_sort_key_v1(score: &EthPeerSelectionScoreV1) -> (u8, i64, u64) {
    (
        if score.eligible { 0 } else { 1 },
        -score.score,
        score.peer_id,
    )
}

fn build_eth_fullnode_peer_selection_quality_summary_v1(
    chain_id: u64,
    scores: &[EthPeerSelectionScoreV1],
) -> EthPeerSelectionQualitySummaryV1 {
    let candidate_peers = scores
        .iter()
        .map(|score| score.peer_id)
        .collect::<std::collections::BTreeSet<_>>();
    let mut summary = EthPeerSelectionQualitySummaryV1 {
        chain_id,
        candidate_peer_count: candidate_peers.len() as u64,
        ..EthPeerSelectionQualitySummaryV1::default()
    };
    let mut selected_bootstrap_sum = 0i64;
    let mut selected_sync_sum = 0i64;
    let mut cooldown_peers = std::collections::BTreeSet::new();
    let mut permanently_rejected_peers = std::collections::BTreeSet::new();

    for score in scores {
        match score.role {
            EthPeerSelectionRoleV1::Bootstrap => {
                summary.evaluated_bootstrap_peers =
                    summary.evaluated_bootstrap_peers.saturating_add(1);
                if score.eligible {
                    summary.retry_eligible_bootstrap_peers =
                        summary.retry_eligible_bootstrap_peers.saturating_add(1);
                }
                if score.selected {
                    summary.selected_bootstrap_peers =
                        summary.selected_bootstrap_peers.saturating_add(1);
                    summary.selected_bootstrap_peer_ids.push(score.peer_id);
                    selected_bootstrap_sum += score.score;
                    if summary
                        .top_selected_bootstrap_score
                        .is_none_or(|current| score.score > current)
                    {
                        summary.top_selected_bootstrap_score = Some(score.score);
                        summary.top_selected_bootstrap_peer_id = Some(score.peer_id);
                    }
                }
            }
            EthPeerSelectionRoleV1::Sync => {
                summary.evaluated_sync_peers = summary.evaluated_sync_peers.saturating_add(1);
                if score.eligible {
                    summary.ready_sync_peers = summary.ready_sync_peers.saturating_add(1);
                }
                if score.selected {
                    summary.selected_sync_peers = summary.selected_sync_peers.saturating_add(1);
                    summary.selected_sync_peer_ids.push(score.peer_id);
                    selected_sync_sum += score.score;
                    if summary
                        .top_selected_sync_score
                        .is_none_or(|current| score.score > current)
                    {
                        summary.top_selected_sync_score = Some(score.score);
                        summary.top_selected_sync_peer_id = Some(score.peer_id);
                    }
                }
            }
        }

        if score.permanently_rejected {
            permanently_rejected_peers.insert(score.peer_id);
        } else if score.cooldown_until_unix_ms > 0 && !score.eligible {
            cooldown_peers.insert(score.peer_id);
        } else if matches!(score.role, EthPeerSelectionRoleV1::Sync) && !score.eligible {
            summary.skipped_unready_sync_peers =
                summary.skipped_unready_sync_peers.saturating_add(1);
        }
    }

    if summary.selected_bootstrap_peers > 0 {
        summary.average_selected_bootstrap_score =
            Some(selected_bootstrap_sum / summary.selected_bootstrap_peers as i64);
    }
    if summary.selected_sync_peers > 0 {
        summary.average_selected_sync_score =
            Some(selected_sync_sum / summary.selected_sync_peers as i64);
    }
    summary.skipped_cooldown_peers = cooldown_peers.len() as u64;
    summary.skipped_permanently_rejected_peers = permanently_rejected_peers.len() as u64;

    summary
}

#[must_use]
pub fn snapshot_eth_fullnode_peer_selection_scores_v1(
    chain_id: u64,
    peers: &[NodeId],
    selected_bootstrap_peers: &[NodeId],
    selected_sync_peers: &[NodeId],
) -> (
    Vec<EthPeerSelectionScoreV1>,
    EthPeerSelectionQualitySummaryV1,
    EthPeerSelectionLongTermSummaryV1,
) {
    let now = eth_peer_now_unix_ms_v1();
    let snapshots = snapshot_network_runtime_eth_peer_sessions_for_peers_v1(chain_id, peers)
        .into_iter()
        .map(|snapshot| (snapshot.peer_id, snapshot))
        .collect::<HashMap<_, _>>();
    let selected_bootstrap = selected_bootstrap_peers
        .iter()
        .map(|peer| peer.0)
        .collect::<std::collections::BTreeSet<_>>();
    let selected_sync = selected_sync_peers
        .iter()
        .map(|peer| peer.0)
        .collect::<std::collections::BTreeSet<_>>();
    let mut scores = Vec::new();
    for peer in peers {
        let bootstrap_snapshot = snapshots.get(&peer.0);
        let mut bootstrap_score =
            eth_peer_bootstrap_score_v1(chain_id, bootstrap_snapshot, peer.0, now);
        bootstrap_score.selected = selected_bootstrap.contains(&peer.0);
        scores.push(bootstrap_score);

        let sync_snapshot = snapshots
            .get(&peer.0)
            .cloned()
            .unwrap_or_else(|| eth_peer_synthetic_discovered_snapshot_v1(chain_id, peer.0));
        let mut sync_score = eth_peer_sync_score_v1(&sync_snapshot, now);
        sync_score.selected = selected_sync.contains(&peer.0);
        scores.push(sync_score);
    }
    scores.sort_by_key(eth_peer_selection_sort_key_v1);
    let summary = build_eth_fullnode_peer_selection_quality_summary_v1(chain_id, &scores);
    let long_term_summary =
        build_eth_fullnode_peer_selection_long_term_summary_v1(chain_id, &scores);
    (scores, summary, long_term_summary)
}

fn eth_native_sync_evidence_map() -> &'static Mutex<HashMap<u64, EthNativeSyncEvidence>> {
    ETH_NATIVE_SYNC_EVIDENCE.get_or_init(|| Mutex::new(HashMap::new()))
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct EthNativeSyncEvidence {
    pub discovery_seen: bool,
    pub rlpx_auth_seen: bool,
    pub rlpx_auth_ack_seen: bool,
    pub hello_seen: bool,
    pub status_seen: bool,
    pub headers_pull_seen: bool,
    pub headers_response_seen: bool,
    pub bodies_pull_seen: bool,
    pub bodies_response_seen: bool,
    pub snap_pull_seen: bool,
    pub snap_response_seen: bool,
}

fn with_eth_native_sync_evidence_mut(chain_id: u64, f: impl FnOnce(&mut EthNativeSyncEvidence)) {
    let mut guard = eth_native_sync_evidence_map()
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    let entry = guard.entry(chain_id).or_default();
    f(entry);
}

#[must_use]
pub fn snapshot_eth_native_sync_evidence(chain_id: u64) -> EthNativeSyncEvidence {
    let guard = eth_native_sync_evidence_map()
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    guard.get(&chain_id).copied().unwrap_or_default()
}

pub fn observe_eth_native_headers_pull(chain_id: u64) {
    with_eth_native_sync_evidence_mut(chain_id, |e| e.headers_pull_seen = true);
}

pub fn observe_eth_native_headers_response(chain_id: u64) {
    with_eth_native_sync_evidence_mut(chain_id, |e| e.headers_response_seen = true);
}

pub fn observe_eth_native_bodies_pull(chain_id: u64) {
    with_eth_native_sync_evidence_mut(chain_id, |e| e.bodies_pull_seen = true);
}

pub fn observe_eth_native_bodies_response(chain_id: u64) {
    with_eth_native_sync_evidence_mut(chain_id, |e| e.bodies_response_seen = true);
}

pub fn observe_eth_native_snap_pull(chain_id: u64) {
    with_eth_native_sync_evidence_mut(chain_id, |e| e.snap_pull_seen = true);
}

pub fn observe_eth_native_snap_response(chain_id: u64) {
    with_eth_native_sync_evidence_mut(chain_id, |e| e.snap_response_seen = true);
}

pub fn observe_eth_native_discovery(chain_id: u64) {
    with_eth_native_sync_evidence_mut(chain_id, |e| e.discovery_seen = true);
}

pub fn observe_eth_native_rlpx_auth(chain_id: u64) {
    with_eth_native_sync_evidence_mut(chain_id, |e| e.rlpx_auth_seen = true);
}

pub fn observe_eth_native_rlpx_auth_ack(chain_id: u64) {
    with_eth_native_sync_evidence_mut(chain_id, |e| e.rlpx_auth_ack_seen = true);
}

pub fn observe_eth_native_hello(chain_id: u64) {
    with_eth_native_sync_evidence_mut(chain_id, |e| e.hello_seen = true);
}

pub fn observe_eth_native_status(chain_id: u64) {
    with_eth_native_sync_evidence_mut(chain_id, |e| e.status_seen = true);
}

fn native_peer_discovery_ready(chain_id: u64) -> bool {
    snapshot_eth_native_sync_evidence(chain_id).discovery_seen
}

fn native_eth_handshake_ready(chain_id: u64) -> bool {
    let e = snapshot_eth_native_sync_evidence(chain_id);
    e.rlpx_auth_seen && e.rlpx_auth_ack_seen && e.hello_seen && e.status_seen
}

fn native_snap_state_machine_ready(chain_id: u64) -> bool {
    let evidence = snapshot_eth_native_sync_evidence(chain_id);
    let sessions = snapshot_network_runtime_eth_peer_sessions(chain_id);
    let has_snap_peer = sessions.iter().any(|s| s.negotiated.snap_version.is_some());
    has_snap_peer
        && evidence.headers_pull_seen
        && evidence.headers_response_seen
        && evidence.bodies_pull_seen
        && evidence.bodies_response_seen
        && evidence.snap_pull_seen
        && evidence.snap_response_seen
}

pub fn upsert_network_runtime_eth_peer_session(
    chain_id: u64,
    peer_id: u64,
    remote_eth_versions: &[u8],
    remote_snap_versions: &[u8],
    announced_head_height: Option<u64>,
) -> Option<EthNegotiatedCapabilities> {
    let local = default_eth_native_capabilities();
    let negotiated =
        negotiate_eth_native_capabilities(&local, remote_eth_versions, remote_snap_versions)?;
    let now = eth_peer_now_unix_ms_v1();
    let mut guard = eth_peer_sessions()
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    let chain = guard.entry(chain_id).or_default();
    let entry = chain.entry(peer_id).or_insert_with(|| {
        let mut state = eth_peer_session_state_default_v1(now);
        state.negotiated = negotiated.clone();
        state
    });
    entry.negotiated = negotiated.clone();
    if entry.first_seen_unix_ms == 0 {
        entry.first_seen_unix_ms = now;
    }
    if let Some(height) = announced_head_height {
        entry.last_head_height = entry.last_head_height.max(height);
        if !entry.session_ready {
            entry.successful_sessions = entry.successful_sessions.saturating_add(1);
        }
        entry.session_ready = true;
        entry.progress_stage = EthPeerLifecycleProgressStageV1::Ready;
        entry.consecutive_failures = 0;
        entry.cooldown_until_unix_ms = 0;
        entry.permanently_rejected = false;
        entry.last_disconnect_reason_code = None;
        entry.last_validation_reject_reason = None;
        entry.last_failure_class = None;
        entry.last_failure_reason_code = None;
        entry.last_failure_reason_name = None;
        entry.last_success_unix_ms = now;
        entry.last_state_change_unix_ms = now;
    }
    Some(negotiated)
}

pub fn observe_network_runtime_eth_peer_discovered_v1(chain_id: u64, peer_id: u64) {
    let now = eth_peer_now_unix_ms_v1();
    let mut guard = eth_peer_sessions()
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    let chain = guard.entry(chain_id).or_default();
    let entry = chain
        .entry(peer_id)
        .or_insert_with(|| eth_peer_session_state_default_v1(now));
    if entry.first_seen_unix_ms == 0 {
        entry.first_seen_unix_ms = now;
    }
}

pub fn observe_network_runtime_eth_peer_connecting_v1(chain_id: u64, peer_id: u64) {
    let now = eth_peer_now_unix_ms_v1();
    let mut guard = eth_peer_sessions()
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    let chain = guard.entry(chain_id).or_default();
    let entry = chain
        .entry(peer_id)
        .or_insert_with(|| eth_peer_session_state_default_v1(now));
    eth_peer_mark_progress_stage_v1(entry, EthPeerLifecycleProgressStageV1::Connecting, now);
}

pub fn observe_network_runtime_eth_peer_connected_v1(chain_id: u64, peer_id: u64) {
    let now = eth_peer_now_unix_ms_v1();
    let mut guard = eth_peer_sessions()
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    let chain = guard.entry(chain_id).or_default();
    let entry = chain
        .entry(peer_id)
        .or_insert_with(|| eth_peer_session_state_default_v1(now));
    eth_peer_mark_progress_stage_v1(entry, EthPeerLifecycleProgressStageV1::Connected, now);
}

pub fn observe_network_runtime_eth_peer_hello_ok_v1(chain_id: u64, peer_id: u64) {
    let now = eth_peer_now_unix_ms_v1();
    let mut guard = eth_peer_sessions()
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    let chain = guard.entry(chain_id).or_default();
    let entry = chain
        .entry(peer_id)
        .or_insert_with(|| eth_peer_session_state_default_v1(now));
    eth_peer_mark_progress_stage_v1(entry, EthPeerLifecycleProgressStageV1::HelloOk, now);
}

pub fn observe_network_runtime_eth_peer_status_ok_v1(
    chain_id: u64,
    peer_id: u64,
    announced_head_height: Option<u64>,
) {
    let now = eth_peer_now_unix_ms_v1();
    let mut guard = eth_peer_sessions()
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    let chain = guard.entry(chain_id).or_default();
    let entry = chain
        .entry(peer_id)
        .or_insert_with(|| eth_peer_session_state_default_v1(now));
    if let Some(height) = announced_head_height {
        entry.last_head_height = entry.last_head_height.max(height);
    }
    eth_peer_mark_progress_stage_v1(entry, EthPeerLifecycleProgressStageV1::StatusOk, now);
}

pub fn observe_network_runtime_eth_peer_syncing_v1(chain_id: u64, peer_id: u64) {
    let now = eth_peer_now_unix_ms_v1();
    let mut guard = eth_peer_sessions()
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    let chain = guard.entry(chain_id).or_default();
    let entry = chain
        .entry(peer_id)
        .or_insert_with(|| eth_peer_session_state_default_v1(now));
    eth_peer_mark_progress_stage_v1(entry, EthPeerLifecycleProgressStageV1::Syncing, now);
}

pub fn observe_network_runtime_eth_peer_connect_failure_v1(
    chain_id: u64,
    peer_id: u64,
    reason_name: impl Into<String>,
    permanent: bool,
) {
    let now = eth_peer_now_unix_ms_v1();
    let mut guard = eth_peer_sessions()
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    let chain = guard.entry(chain_id).or_default();
    let entry = chain
        .entry(peer_id)
        .or_insert_with(|| eth_peer_session_state_default_v1(now));
    let cooldown_ms = if permanent {
        0
    } else {
        eth_peer_failure_cooldown_ms_v1(entry, EthPeerFailureClassV1::ConnectFailure, None, None)
    };
    eth_peer_record_failure_v1(
        entry,
        EthPeerFailureClassV1::ConnectFailure,
        None,
        reason_name,
        cooldown_ms,
        permanent,
        now,
    );
}

pub fn observe_network_runtime_eth_peer_handshake_failure_v1(
    chain_id: u64,
    peer_id: u64,
    reason_name: impl Into<String>,
) {
    let now = eth_peer_now_unix_ms_v1();
    let mut guard = eth_peer_sessions()
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    let chain = guard.entry(chain_id).or_default();
    let entry = chain
        .entry(peer_id)
        .or_insert_with(|| eth_peer_session_state_default_v1(now));
    let cooldown_ms =
        eth_peer_failure_cooldown_ms_v1(entry, EthPeerFailureClassV1::HandshakeFailure, None, None);
    eth_peer_record_failure_v1(
        entry,
        EthPeerFailureClassV1::HandshakeFailure,
        None,
        reason_name,
        cooldown_ms,
        false,
        now,
    );
}

pub fn observe_network_runtime_eth_peer_decode_failure_v1(
    chain_id: u64,
    peer_id: u64,
    reason_name: impl Into<String>,
) {
    let now = eth_peer_now_unix_ms_v1();
    let mut guard = eth_peer_sessions()
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    let chain = guard.entry(chain_id).or_default();
    let entry = chain
        .entry(peer_id)
        .or_insert_with(|| eth_peer_session_state_default_v1(now));
    let cooldown_ms =
        eth_peer_failure_cooldown_ms_v1(entry, EthPeerFailureClassV1::DecodeFailure, None, None);
    eth_peer_record_failure_v1(
        entry,
        EthPeerFailureClassV1::DecodeFailure,
        None,
        reason_name,
        cooldown_ms,
        false,
        now,
    );
}

pub fn observe_network_runtime_eth_peer_timeout_v1(
    chain_id: u64,
    peer_id: u64,
    reason_name: impl Into<String>,
) {
    let now = eth_peer_now_unix_ms_v1();
    let mut guard = eth_peer_sessions()
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    let chain = guard.entry(chain_id).or_default();
    let entry = chain
        .entry(peer_id)
        .or_insert_with(|| eth_peer_session_state_default_v1(now));
    let cooldown_ms =
        eth_peer_failure_cooldown_ms_v1(entry, EthPeerFailureClassV1::Timeout, None, None);
    eth_peer_record_failure_v1(
        entry,
        EthPeerFailureClassV1::Timeout,
        None,
        reason_name,
        cooldown_ms,
        false,
        now,
    );
}

pub fn observe_network_runtime_eth_peer_head(chain_id: u64, peer_id: u64, head_height: u64) {
    let mut guard = eth_peer_sessions()
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    let chain = guard.entry(chain_id).or_default();
    let now = eth_peer_now_unix_ms_v1();
    let entry = chain
        .entry(peer_id)
        .or_insert_with(|| eth_peer_session_state_default_v1(now));
    entry.last_head_height = entry.last_head_height.max(head_height);
}

pub fn observe_network_runtime_eth_peer_header_success_v1(
    chain_id: u64,
    peer_id: u64,
    head_height: u64,
) {
    let now = eth_peer_now_unix_ms_v1();
    let mut guard = eth_peer_sessions()
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    let chain = guard.entry(chain_id).or_default();
    let entry = chain
        .entry(peer_id)
        .or_insert_with(|| eth_peer_session_state_default_v1(now));
    entry.last_head_height = entry.last_head_height.max(head_height);
    entry.header_response_count = entry.header_response_count.saturating_add(1);
    entry.sync_contribution_count = entry.sync_contribution_count.saturating_add(1);
    entry.last_header_success_unix_ms = now;
    entry.last_success_unix_ms = now;
    entry.last_state_change_unix_ms = now;
}

pub fn observe_network_runtime_eth_peer_body_success_v1(
    chain_id: u64,
    peer_id: u64,
    head_height: u64,
) {
    let now = eth_peer_now_unix_ms_v1();
    let mut guard = eth_peer_sessions()
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    let chain = guard.entry(chain_id).or_default();
    let entry = chain
        .entry(peer_id)
        .or_insert_with(|| eth_peer_session_state_default_v1(now));
    entry.last_head_height = entry.last_head_height.max(head_height);
    entry.body_response_count = entry.body_response_count.saturating_add(1);
    entry.sync_contribution_count = entry.sync_contribution_count.saturating_add(1);
    entry.last_body_success_unix_ms = now;
    entry.last_success_unix_ms = now;
    entry.last_state_change_unix_ms = now;
}

pub fn observe_network_runtime_eth_peer_selection_round_v1(
    chain_id: u64,
    peers: &[NodeId],
    selected_bootstrap_peers: &[NodeId],
    selected_sync_peers: &[NodeId],
    header_success_peers: &[u64],
    body_success_peers: &[u64],
    connect_failure_peers: &[u64],
    handshake_failure_peers: &[u64],
    decode_failure_peers: &[u64],
    timeout_failure_peers: &[u64],
    validation_reject_peers: &[u64],
    disconnect_peers: &[u64],
    capacity_reject_peers: &[u64],
) {
    let now = eth_peer_now_unix_ms_v1();
    let selected_bootstrap = selected_bootstrap_peers
        .iter()
        .map(|peer| peer.0)
        .collect::<std::collections::BTreeSet<_>>();
    let selected_sync = selected_sync_peers
        .iter()
        .map(|peer| peer.0)
        .collect::<std::collections::BTreeSet<_>>();
    let header_success = header_success_peers
        .iter()
        .copied()
        .collect::<std::collections::BTreeSet<_>>();
    let body_success = body_success_peers
        .iter()
        .copied()
        .collect::<std::collections::BTreeSet<_>>();
    let connect_failure = connect_failure_peers
        .iter()
        .copied()
        .collect::<std::collections::BTreeSet<_>>();
    let handshake_failure = handshake_failure_peers
        .iter()
        .copied()
        .collect::<std::collections::BTreeSet<_>>();
    let decode_failure = decode_failure_peers
        .iter()
        .copied()
        .collect::<std::collections::BTreeSet<_>>();
    let timeout_failure = timeout_failure_peers
        .iter()
        .copied()
        .collect::<std::collections::BTreeSet<_>>();
    let validation_reject = validation_reject_peers
        .iter()
        .copied()
        .collect::<std::collections::BTreeSet<_>>();
    let disconnect = disconnect_peers
        .iter()
        .copied()
        .collect::<std::collections::BTreeSet<_>>();
    let capacity_reject = capacity_reject_peers
        .iter()
        .copied()
        .collect::<std::collections::BTreeSet<_>>();

    let mut guard = eth_peer_sessions()
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    let chain = guard.entry(chain_id).or_default();
    for peer in peers {
        let entry = chain
            .entry(peer.0)
            .or_insert_with(|| eth_peer_session_state_default_v1(now));
        let outcome = EthPeerRoundOutcomeV1 {
            selected_bootstrap: selected_bootstrap.contains(&peer.0),
            selected_sync: selected_sync.contains(&peer.0),
            header_success: header_success.contains(&peer.0),
            body_success: body_success.contains(&peer.0),
            connect_failure: connect_failure.contains(&peer.0),
            handshake_failure: handshake_failure.contains(&peer.0),
            decode_failure: decode_failure.contains(&peer.0),
            timeout_failure: timeout_failure.contains(&peer.0),
            validation_reject: validation_reject.contains(&peer.0),
            disconnect: disconnect.contains(&peer.0),
            capacity_reject: capacity_reject.contains(&peer.0),
            observed_unix_ms: now,
        };
        eth_peer_push_round_outcome_v1(entry, outcome.clone());
        eth_peer_update_long_term_round_stats_v1(entry, &outcome);
    }
}

pub fn mark_network_runtime_eth_peer_session_ready_v1(
    chain_id: u64,
    peer_id: u64,
    announced_head_height: Option<u64>,
) {
    let now = eth_peer_now_unix_ms_v1();
    let mut guard = eth_peer_sessions()
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    let chain = guard.entry(chain_id).or_default();
    let entry = chain
        .entry(peer_id)
        .or_insert_with(|| eth_peer_session_state_default_v1(now));
    if let Some(height) = announced_head_height {
        entry.last_head_height = entry.last_head_height.max(height);
    }
    if !entry.session_ready {
        entry.successful_sessions = entry.successful_sessions.saturating_add(1);
    }
    entry.session_ready = true;
    eth_peer_mark_progress_stage_v1(entry, EthPeerLifecycleProgressStageV1::Ready, now);
    entry.consecutive_failures = 0;
    entry.cooldown_until_unix_ms = 0;
    entry.permanently_rejected = false;
    entry.last_disconnect_reason_code = None;
    entry.last_validation_reject_reason = None;
    entry.last_failure_class = None;
    entry.last_failure_reason_code = None;
    entry.last_failure_reason_name = None;
    entry.last_success_unix_ms = now;
}

pub fn observe_network_runtime_eth_peer_validation_reject_v1(
    chain_id: u64,
    peer_id: u64,
    reason: EthChainConfigPeerValidationReasonV1,
) {
    let now = eth_peer_now_unix_ms_v1();
    let mut guard = eth_peer_sessions()
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    let chain = guard.entry(chain_id).or_default();
    let entry = chain
        .entry(peer_id)
        .or_insert_with(|| eth_peer_session_state_default_v1(now));
    entry.last_validation_reject_reason = Some(reason);
    let permanent = eth_peer_validation_is_permanent_v1(reason);
    let cooldown_ms = eth_peer_failure_cooldown_ms_v1(
        entry,
        EthPeerFailureClassV1::ValidationReject,
        None,
        Some(reason),
    );
    eth_peer_record_failure_v1(
        entry,
        EthPeerFailureClassV1::ValidationReject,
        None,
        reason.as_str(),
        cooldown_ms,
        permanent,
        now,
    );
}

pub fn observe_network_runtime_eth_peer_disconnect_v1(
    chain_id: u64,
    peer_id: u64,
    reason_code: Option<u64>,
) {
    let now = eth_peer_now_unix_ms_v1();
    let mut guard = eth_peer_sessions()
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    let chain = guard.entry(chain_id).or_default();
    let entry = chain
        .entry(peer_id)
        .or_insert_with(|| eth_peer_session_state_default_v1(now));
    let cooldown_ms = eth_peer_failure_cooldown_ms_v1(
        entry,
        EthPeerFailureClassV1::Disconnect,
        reason_code,
        None,
    );
    eth_peer_record_failure_v1(
        entry,
        EthPeerFailureClassV1::Disconnect,
        reason_code,
        eth_rlpx_disconnect_reason_name_v1(reason_code.unwrap_or(u64::MAX)),
        cooldown_ms,
        false,
        now,
    );
}

#[must_use]
pub fn snapshot_network_runtime_eth_peer_sessions(chain_id: u64) -> Vec<EthPeerSessionSnapshot> {
    let now = eth_peer_now_unix_ms_v1();
    let guard = eth_peer_sessions()
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    guard
        .get(&chain_id)
        .map(|peers| {
            peers
                .iter()
                .map(|(peer_id, state)| {
                    eth_peer_session_snapshot_from_state_v1(chain_id, *peer_id, state, now)
                })
                .collect::<Vec<_>>()
        })
        .unwrap_or_default()
}

#[must_use]
pub fn snapshot_network_runtime_eth_peer_sessions_for_peers_v1(
    chain_id: u64,
    peers: &[NodeId],
) -> Vec<EthPeerSessionSnapshot> {
    let now = eth_peer_now_unix_ms_v1();
    let guard = eth_peer_sessions()
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    let chain = guard.get(&chain_id);
    peers
        .iter()
        .copied()
        .map(|peer| {
            chain
                .and_then(|states| states.get(&peer.0))
                .map(|state| eth_peer_session_snapshot_from_state_v1(chain_id, peer.0, state, now))
                .unwrap_or_else(|| eth_peer_synthetic_discovered_snapshot_v1(chain_id, peer.0))
        })
        .collect()
}

#[must_use]
pub fn snapshot_network_runtime_eth_peer_lifecycle_summary_v1(
    chain_id: u64,
    peers: &[NodeId],
) -> EthPeerLifecycleSummaryV1 {
    let mut summary = EthPeerLifecycleSummaryV1 {
        chain_id,
        ..EthPeerLifecycleSummaryV1::default()
    };
    for snapshot in snapshot_network_runtime_eth_peer_sessions_for_peers_v1(chain_id, peers) {
        summary.peer_count = summary.peer_count.saturating_add(1);
        match snapshot.lifecycle_stage {
            EthPeerLifecycleStageV1::Discovered => {
                summary.discovered_count = summary.discovered_count.saturating_add(1)
            }
            EthPeerLifecycleStageV1::Connecting => {
                summary.connecting_count = summary.connecting_count.saturating_add(1)
            }
            EthPeerLifecycleStageV1::Connected => {
                summary.connected_count = summary.connected_count.saturating_add(1)
            }
            EthPeerLifecycleStageV1::HelloOk => {
                summary.hello_ok_count = summary.hello_ok_count.saturating_add(1)
            }
            EthPeerLifecycleStageV1::StatusOk => {
                summary.status_ok_count = summary.status_ok_count.saturating_add(1)
            }
            EthPeerLifecycleStageV1::Ready => {
                summary.ready_count = summary.ready_count.saturating_add(1)
            }
            EthPeerLifecycleStageV1::Syncing => {
                summary.syncing_count = summary.syncing_count.saturating_add(1)
            }
            EthPeerLifecycleStageV1::Cooldown => {
                summary.cooldown_count = summary.cooldown_count.saturating_add(1)
            }
            EthPeerLifecycleStageV1::TemporarilyFailed => {
                summary.temporarily_failed_count =
                    summary.temporarily_failed_count.saturating_add(1)
            }
            EthPeerLifecycleStageV1::PermanentlyRejected => {
                summary.permanently_rejected_count =
                    summary.permanently_rejected_count.saturating_add(1)
            }
        }
        if snapshot.retry_eligible {
            summary.retry_eligible_count = summary.retry_eligible_count.saturating_add(1);
        }
        summary.connect_failure_count = summary
            .connect_failure_count
            .saturating_add(snapshot.connect_failure_count);
        summary.handshake_failure_count = summary
            .handshake_failure_count
            .saturating_add(snapshot.handshake_failure_count);
        summary.decode_failure_count = summary
            .decode_failure_count
            .saturating_add(snapshot.decode_failure_count);
        summary.timeout_count = summary.timeout_count.saturating_add(snapshot.timeout_count);
        summary.validation_reject_count = summary
            .validation_reject_count
            .saturating_add(snapshot.validation_reject_count);
        summary.disconnect_count = summary
            .disconnect_count
            .saturating_add(snapshot.disconnect_count);
    }
    summary
}

#[must_use]
pub fn has_network_runtime_eth_peer_session(chain_id: u64, peer_id: u64) -> bool {
    let guard = eth_peer_sessions()
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    guard
        .get(&chain_id)
        .and_then(|peers| peers.get(&peer_id))
        .map(|state| {
            state.session_ready
                && matches!(
                    eth_peer_effective_stage_v1(state, eth_peer_now_unix_ms_v1()),
                    EthPeerLifecycleStageV1::Ready | EthPeerLifecycleStageV1::Syncing
                )
        })
        .unwrap_or(false)
}

#[must_use]
pub fn select_eth_fullnode_native_sync_targets_v1(
    chain_id: u64,
    peers: &[NodeId],
    max_targets: usize,
) -> Vec<NodeId> {
    if peers.is_empty() || max_targets == 0 {
        return Vec::new();
    }

    let mut peer_order = Vec::new();
    for peer in peers {
        if !peer_order.contains(peer) {
            peer_order.push(*peer);
        }
    }

    let now = eth_peer_now_unix_ms_v1();
    let mut ranked = snapshot_network_runtime_eth_peer_sessions_for_peers_v1(chain_id, peers)
        .into_iter()
        .map(|session| eth_peer_sync_score_v1(&session, now))
        .filter(|score| peer_order.iter().any(|peer| peer.0 == score.peer_id))
        .collect::<Vec<_>>();
    ranked.sort_by_key(eth_peer_selection_sort_key_v1);

    let mut selected = Vec::new();
    for score in ranked {
        if selected.len() >= max_targets {
            break;
        }
        if !score.eligible {
            continue;
        }
        let peer = NodeId(score.peer_id);
        if !selected.contains(&peer) {
            selected.push(peer);
        }
    }

    selected
}

#[must_use]
pub fn select_eth_fullnode_native_bootstrap_candidates_v1(
    chain_id: u64,
    peers: &[NodeId],
    max_targets: usize,
) -> Vec<NodeId> {
    if peers.is_empty() || max_targets == 0 {
        return Vec::new();
    }
    let snapshots = snapshot_network_runtime_eth_peer_sessions_for_peers_v1(chain_id, peers)
        .into_iter()
        .map(|snapshot| (snapshot.peer_id, snapshot))
        .collect::<HashMap<_, _>>();
    let now = eth_peer_now_unix_ms_v1();
    let mut ranked = peers
        .iter()
        .map(|peer| eth_peer_bootstrap_score_v1(chain_id, snapshots.get(&peer.0), peer.0, now))
        .collect::<Vec<_>>();
    ranked.sort_by_key(eth_peer_selection_sort_key_v1);
    ranked
        .into_iter()
        .filter(|score| score.eligible)
        .take(max_targets)
        .map(|score| NodeId(score.peer_id))
        .collect()
}

fn eth_native_head_height_hint_v1(chain_id: u64) -> u64 {
    get_network_runtime_native_head_snapshot_v1(chain_id)
        .map(|snapshot| snapshot.block_number)
        .or_else(|| get_network_runtime_sync_status(chain_id).map(|status| status.current_block))
        .unwrap_or(0)
}

fn eth_native_head_hash_hint_v1(chain_id: u64) -> [u8; 32] {
    get_network_runtime_native_head_snapshot_v1(chain_id)
        .map(|snapshot| snapshot.block_hash)
        .or_else(|| get_network_runtime_native_header_snapshot_v1(chain_id).map(|s| s.hash))
        .unwrap_or_else(|| {
            if eth_native_head_height_hint_v1(chain_id) == 0 {
                eth_chain_config_genesis_hash_v1(chain_id)
            } else {
                [0u8; 32]
            }
        })
}

fn eth_native_head_time_hint_v1(chain_id: u64) -> u64 {
    get_network_runtime_native_header_snapshot_v1(chain_id)
        .and_then(|snapshot| snapshot.timestamp)
        .unwrap_or(0)
}

fn eth_native_total_difficulty_hint_v1(chain_id: u64) -> u128 {
    eth_native_head_height_hint_v1(chain_id) as u128
}

#[must_use]
pub fn build_eth_fullnode_native_rlpx_status_v1(
    chain_id: u64,
    protocol_version: u32,
) -> EthRlpxStatusV1 {
    let native_head_block = snapshot_eth_fullnode_native_head_block_object_v1(chain_id);
    let native_canonical_chain = snapshot_network_runtime_native_canonical_chain_v1(chain_id);
    let runtime_sync = get_network_runtime_sync_status(chain_id);
    let runtime_native_sync = get_network_runtime_native_sync_status(chain_id);
    let chain_view = derive_eth_fullnode_chain_view_with_native_preference_v1(
        None,
        native_head_block.as_ref(),
        native_canonical_chain.as_ref(),
    );
    let head_view = derive_eth_fullnode_head_view_with_native_preference_v1(
        chain_view.as_ref(),
        native_head_block.as_ref(),
        native_canonical_chain.as_ref(),
        runtime_native_sync,
    );
    let chain_config = resolve_eth_chain_config_v1(chain_id);
    let genesis_hash = chain_config.genesis_hash;
    let earliest_block = runtime_sync
        .map(|status| status.starting_block.min(status.current_block))
        .or_else(|| chain_view.as_ref().map(|view| view.starting_block_number))
        .unwrap_or(0);
    let latest_block = head_view
        .as_ref()
        .map(|view| view.block_number)
        .or_else(|| chain_view.as_ref().map(|view| view.current_block_number))
        .or_else(|| runtime_sync.map(|status| status.current_block))
        .unwrap_or(0);
    let latest_block_hash = head_view
        .as_ref()
        .map(|view| view.block_hash)
        .or_else(|| chain_view.as_ref().map(|view| view.current_block_hash))
        .unwrap_or_else(|| {
            if latest_block == 0 {
                genesis_hash
            } else {
                eth_native_head_hash_hint_v1(chain_id)
            }
        });
    let fork_id = build_eth_fork_id_from_chain_config_v1(
        &chain_config,
        latest_block,
        eth_native_head_time_hint_v1(chain_id),
    );
    EthRlpxStatusV1 {
        protocol_version,
        network_id: chain_id,
        genesis_hash,
        fork_id,
        earliest_block,
        latest_block,
        latest_block_hash,
    }
}

#[must_use]
pub fn build_eth_fullnode_native_hello_message_v1(
    local_node: NodeId,
    chain_id: u64,
) -> ProtocolMessage {
    let caps = default_eth_native_capabilities();
    ProtocolMessage::EvmNative(EvmNativeMessage::Hello {
        from: local_node,
        chain_id,
        eth_versions: caps.eth_versions.iter().map(|v| v.as_u8()).collect(),
        snap_versions: caps.snap_versions.iter().map(|v| v.as_u8()).collect(),
        network_id: chain_id,
        total_difficulty: eth_native_total_difficulty_hint_v1(chain_id),
        head_hash: eth_native_head_hash_hint_v1(chain_id),
        genesis_hash: eth_chain_config_genesis_hash_v1(chain_id),
    })
}

#[must_use]
pub fn build_eth_fullnode_native_status_message_v1(
    local_node: NodeId,
    chain_id: u64,
) -> ProtocolMessage {
    ProtocolMessage::EvmNative(EvmNativeMessage::Status {
        from: local_node,
        chain_id,
        total_difficulty: eth_native_total_difficulty_hint_v1(chain_id),
        head_height: eth_native_head_height_hint_v1(chain_id),
        head_hash: eth_native_head_hash_hint_v1(chain_id),
        genesis_hash: eth_chain_config_genesis_hash_v1(chain_id),
    })
}

#[must_use]
pub fn build_eth_fullnode_native_bootstrap_messages_v1(
    local_node: NodeId,
    peer: NodeId,
    chain_id: u64,
) -> Vec<ProtocolMessage> {
    let mut auth_tag = [0u8; 32];
    auth_tag[0..8].copy_from_slice(&chain_id.to_le_bytes());
    auth_tag[8..16].copy_from_slice(&local_node.0.to_le_bytes());
    auth_tag[16..24].copy_from_slice(&peer.0.to_le_bytes());

    vec![
        ProtocolMessage::EvmNative(EvmNativeMessage::DiscoveryPing {
            from: local_node,
            chain_id,
            tcp_port: 0,
            udp_port: 0,
        }),
        ProtocolMessage::EvmNative(EvmNativeMessage::RlpxAuth {
            from: local_node,
            chain_id,
            network_id: chain_id,
            auth_tag,
        }),
        build_eth_fullnode_native_hello_message_v1(local_node, chain_id),
        build_eth_fullnode_native_status_message_v1(local_node, chain_id),
    ]
}

#[must_use]
pub fn build_eth_fullnode_native_sync_request_v1(
    local_node: NodeId,
    chain_id: u64,
) -> Option<ProtocolMessage> {
    let window = plan_network_runtime_sync_pull_window(chain_id)?;
    let span = window
        .to_block
        .saturating_sub(window.from_block)
        .saturating_add(1)
        .max(1);
    Some(match window.phase {
        NetworkRuntimeNativeSyncPhaseV1::State => {
            ProtocolMessage::EvmNative(EvmNativeMessage::SnapGetAccountRange {
                from: local_node,
                block_hash: eth_native_head_hash_hint_v1(chain_id),
                origin: {
                    let mut origin = [0u8; 32];
                    origin[0..8].copy_from_slice(&window.from_block.to_le_bytes());
                    origin
                },
                limit: span,
            })
        }
        _ => ProtocolMessage::EvmNative(EvmNativeMessage::GetBlockHeaders {
            from: local_node,
            start_height: window.from_block,
            max: span,
            skip: 0,
            reverse: false,
        }),
    })
}

#[must_use]
pub fn build_eth_fullnode_native_bodies_request_v1(
    local_node: NodeId,
    hashes: &[[u8; 32]],
) -> Option<ProtocolMessage> {
    if hashes.is_empty() {
        return None;
    }
    Some(ProtocolMessage::EvmNative(
        EvmNativeMessage::GetBlockBodies {
            from: local_node,
            hashes: hashes.to_vec(),
        },
    ))
}

impl EthNativeParityProgress {
    #[must_use]
    pub fn completed_count(&self) -> u64 {
        let flags = [
            self.full_node_only,
            self.upstream_fallback_disabled,
            self.native_peer_discovery,
            self.native_eth_handshake,
            self.native_snap_sync_state_machine,
            self.state_proof_semantics_closed,
            self.rpc_core_semantics_closed,
        ];
        flags.iter().filter(|v| **v).count() as u64
    }

    #[must_use]
    pub fn total_count(&self) -> u64 {
        7
    }

    #[must_use]
    pub fn progress_pct(&self) -> f64 {
        if self.total_count() == 0 {
            return 0.0;
        }
        (self.completed_count() as f64) * 100.0 / (self.total_count() as f64)
    }
}

#[must_use]
pub fn current_eth_native_parity_progress() -> EthNativeParityProgress {
    EthNativeParityProgress {
        profile: EthSyncTransportProfile::NovovmNativeBridge,
        full_node_only: true,
        upstream_fallback_disabled: true,
        native_peer_discovery: true,
        native_eth_handshake: true,
        native_snap_sync_state_machine: false,
        state_proof_semantics_closed: true,
        rpc_core_semantics_closed: true,
    }
}

#[must_use]
pub fn current_eth_native_parity_progress_for_chain(chain_id: u64) -> EthNativeParityProgress {
    let mut progress = current_eth_native_parity_progress();
    progress.profile = if native_eth_handshake_ready(chain_id) {
        EthSyncTransportProfile::NativeDevp2pRlpx
    } else {
        EthSyncTransportProfile::NovovmNativeBridge
    };
    progress.native_peer_discovery = native_peer_discovery_ready(chain_id);
    progress.native_eth_handshake = native_eth_handshake_ready(chain_id);
    progress.native_snap_sync_state_machine = native_snap_state_machine_ready(chain_id);
    progress
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn negotiate_eth_native_caps_prefers_highest_shared() {
        let local = default_eth_native_capabilities();
        let negotiated =
            negotiate_eth_native_capabilities(&local, &[66, 67], &[1]).expect("must negotiate");
        assert_eq!(negotiated.eth_version, EthWireVersion::V67);
        assert_eq!(negotiated.snap_version, Some(SnapWireVersion::V1));
    }

    #[test]
    fn negotiate_eth_native_caps_none_if_no_eth_intersection() {
        let local = default_eth_native_capabilities();
        let negotiated = negotiate_eth_native_capabilities(&local, &[64, 65], &[1]);
        assert!(negotiated.is_none());
    }

    #[test]
    fn parity_progress_matches_expected_bootstrap_state() {
        let progress = current_eth_native_parity_progress();
        assert_eq!(
            progress.profile,
            EthSyncTransportProfile::NovovmNativeBridge
        );
        assert!(progress.full_node_only);
        assert!(progress.upstream_fallback_disabled);
        assert!(progress.native_peer_discovery);
        assert!(progress.native_eth_handshake);
        assert!(!progress.native_snap_sync_state_machine);
        assert!(progress.progress_pct() > 50.0);
    }

    #[test]
    fn session_snapshot_keeps_max_head_and_caps() {
        let chain_id = 1;
        let peer_id = 42;
        let negotiated =
            upsert_network_runtime_eth_peer_session(chain_id, peer_id, &[66, 68], &[1], Some(128))
                .expect("negotiated");
        assert_eq!(negotiated.eth_version, EthWireVersion::V68);
        observe_network_runtime_eth_peer_head(chain_id, peer_id, 256);
        let snapshots = snapshot_network_runtime_eth_peer_sessions(chain_id);
        assert_eq!(snapshots.len(), 1);
        assert!(snapshots[0].session_ready);
        assert_eq!(snapshots[0].last_head_height, 256);
        assert_eq!(snapshots[0].negotiated.eth_version, EthWireVersion::V68);
        assert_eq!(snapshots[0].lifecycle_stage, EthPeerLifecycleStageV1::Ready);
        assert!(!snapshots[0].retry_eligible);
    }

    #[test]
    fn lifecycle_summary_includes_discovered_failure_and_ready_states() {
        let chain_id = 99_160_315_u64;
        let peer_a = NodeId(301);
        let peer_b = NodeId(302);
        let peer_c = NodeId(303);
        let peer_d = NodeId(304);

        observe_network_runtime_eth_peer_connect_failure_v1(
            chain_id,
            peer_a.0,
            "connect_failed",
            false,
        );
        observe_network_runtime_eth_peer_decode_failure_v1(
            chain_id,
            peer_b.0,
            "status_payload_decode_failed",
        );
        let _ =
            upsert_network_runtime_eth_peer_session(chain_id, peer_c.0, &[68, 70], &[1], Some(64))
                .expect("ready session");
        observe_network_runtime_eth_peer_syncing_v1(chain_id, peer_c.0);

        let views = snapshot_network_runtime_eth_peer_sessions_for_peers_v1(
            chain_id,
            &[peer_a, peer_b, peer_c, peer_d],
        );
        let stages = views
            .iter()
            .map(|snapshot| (snapshot.peer_id, snapshot.lifecycle_stage))
            .collect::<HashMap<_, _>>();
        assert_eq!(
            stages.get(&peer_a.0),
            Some(&EthPeerLifecycleStageV1::Cooldown)
        );
        assert_eq!(
            stages.get(&peer_b.0),
            Some(&EthPeerLifecycleStageV1::Cooldown)
        );
        assert_eq!(
            stages.get(&peer_c.0),
            Some(&EthPeerLifecycleStageV1::Syncing)
        );
        assert_eq!(
            stages.get(&peer_d.0),
            Some(&EthPeerLifecycleStageV1::Discovered)
        );

        let summary = snapshot_network_runtime_eth_peer_lifecycle_summary_v1(
            chain_id,
            &[peer_a, peer_b, peer_c, peer_d],
        );
        assert_eq!(summary.peer_count, 4);
        assert_eq!(summary.cooldown_count, 2);
        assert_eq!(summary.syncing_count, 1);
        assert_eq!(summary.discovered_count, 1);
        assert_eq!(summary.connect_failure_count, 1);
        assert_eq!(summary.decode_failure_count, 1);
        assert_eq!(summary.retry_eligible_count, 1);
    }

    #[test]
    fn temporary_failures_become_retry_eligible_after_cooldown_expires() {
        let chain_id = 99_160_320_u64;
        let peer = NodeId(601);
        observe_network_runtime_eth_peer_disconnect_v1(chain_id, peer.0, None);

        {
            let mut guard = eth_peer_sessions()
                .lock()
                .unwrap_or_else(|poisoned| poisoned.into_inner());
            let chain = guard.get_mut(&chain_id).expect("chain state");
            let state = chain.get_mut(&peer.0).expect("peer state");
            state.cooldown_until_unix_ms = 0;
        }

        let snapshot =
            snapshot_network_runtime_eth_peer_sessions_for_peers_v1(chain_id, &[peer])[0].clone();
        assert_eq!(
            snapshot.lifecycle_stage,
            EthPeerLifecycleStageV1::TemporarilyFailed
        );
        assert!(snapshot.retry_eligible);
    }

    #[test]
    fn select_native_sync_targets_prefers_highest_observed_peer_head() {
        let chain_id = 99_160_318_u64;
        let peer_a = NodeId(401);
        let peer_b = NodeId(402);
        let peer_c = NodeId(403);
        let _ =
            upsert_network_runtime_eth_peer_session(chain_id, peer_a.0, &[66, 68], &[1], Some(120))
                .expect("session a");
        let _ =
            upsert_network_runtime_eth_peer_session(chain_id, peer_b.0, &[66, 68], &[1], Some(220))
                .expect("session b");
        let _ =
            upsert_network_runtime_eth_peer_session(chain_id, peer_c.0, &[66, 68], &[1], Some(180))
                .expect("session c");

        let selected =
            select_eth_fullnode_native_sync_targets_v1(chain_id, &[peer_a, peer_b, peer_c], 2);
        assert_eq!(selected, vec![peer_b, peer_c]);
    }

    #[test]
    fn sync_selection_prefers_stable_body_contributor_over_flaky_higher_head_peer() {
        let chain_id = 99_160_318_1_u64;
        let stable_peer = NodeId(411);
        let flaky_peer = NodeId(412);
        let _ = upsert_network_runtime_eth_peer_session(
            chain_id,
            stable_peer.0,
            &[66, 68, 70],
            &[1],
            Some(10_000),
        )
        .expect("stable session");
        let _ = upsert_network_runtime_eth_peer_session(
            chain_id,
            flaky_peer.0,
            &[66, 68, 70],
            &[1],
            Some(10_064),
        )
        .expect("flaky session");

        observe_network_runtime_eth_peer_header_success_v1(chain_id, stable_peer.0, 10_000);
        observe_network_runtime_eth_peer_body_success_v1(chain_id, stable_peer.0, 10_000);
        observe_network_runtime_eth_peer_body_success_v1(chain_id, stable_peer.0, 10_000);
        observe_network_runtime_eth_peer_timeout_v1(chain_id, flaky_peer.0, "headers_timeout");
        observe_network_runtime_eth_peer_timeout_v1(chain_id, flaky_peer.0, "bodies_timeout");
        observe_network_runtime_eth_peer_disconnect_v1(chain_id, flaky_peer.0, Some(0x04));

        {
            let mut guard = eth_peer_sessions()
                .lock()
                .unwrap_or_else(|poisoned| poisoned.into_inner());
            let chain = guard.get_mut(&chain_id).expect("chain state");
            let stable = chain.get_mut(&stable_peer.0).expect("stable");
            stable.cooldown_until_unix_ms = 0;
            stable.session_ready = true;
            stable.progress_stage = EthPeerLifecycleProgressStageV1::Ready;
            let flaky = chain.get_mut(&flaky_peer.0).expect("flaky");
            flaky.cooldown_until_unix_ms = 0;
            flaky.session_ready = true;
            flaky.progress_stage = EthPeerLifecycleProgressStageV1::Ready;
        }

        let selected =
            select_eth_fullnode_native_sync_targets_v1(chain_id, &[stable_peer, flaky_peer], 1);
        assert_eq!(selected, vec![stable_peer]);

        let (scores, summary, long_term_summary) = snapshot_eth_fullnode_peer_selection_scores_v1(
            chain_id,
            &[stable_peer, flaky_peer],
            &[],
            &[stable_peer],
        );
        let stable_score = scores
            .iter()
            .find(|score| {
                score.peer_id == stable_peer.0 && matches!(score.role, EthPeerSelectionRoleV1::Sync)
            })
            .expect("stable sync score");
        let flaky_score = scores
            .iter()
            .find(|score| {
                score.peer_id == flaky_peer.0 && matches!(score.role, EthPeerSelectionRoleV1::Sync)
            })
            .expect("flaky sync score");
        assert!(stable_score.score > flaky_score.score);
        assert_eq!(summary.selected_sync_peer_ids, vec![stable_peer.0]);
        assert_eq!(
            long_term_summary.top_trusted_sync_peer_id,
            Some(stable_peer.0)
        );
    }

    #[test]
    fn sync_selection_prefers_long_term_trusted_peer_over_ephemeral_higher_head_peer() {
        let chain_id = 99_160_318_2_u64;
        let trusted_peer = NodeId(421);
        let ephemeral_peer = NodeId(422);
        let now = eth_peer_now_unix_ms_v1();

        let _ = upsert_network_runtime_eth_peer_session(
            chain_id,
            trusted_peer.0,
            &[66, 68, 70],
            &[1],
            Some(12_000),
        )
        .expect("trusted session");
        let _ = upsert_network_runtime_eth_peer_session(
            chain_id,
            ephemeral_peer.0,
            &[66, 68, 70],
            &[1],
            Some(13_000),
        )
        .expect("ephemeral session");

        {
            let mut guard = eth_peer_sessions()
                .lock()
                .unwrap_or_else(|poisoned| poisoned.into_inner());
            let chain = guard.get_mut(&chain_id).expect("chain state");

            let trusted = chain.get_mut(&trusted_peer.0).expect("trusted");
            trusted.cooldown_until_unix_ms = 0;
            trusted.session_ready = true;
            trusted.progress_stage = EthPeerLifecycleProgressStageV1::Ready;
            trusted.long_term = EthPeerLongTermStatsV1 {
                total_observed_rounds: 128,
                total_selected_rounds: 72,
                total_selected_bootstrap_rounds: 8,
                total_selected_sync_rounds: 64,
                total_header_success_rounds: 52,
                total_body_success_rounds: 40,
                total_sync_contribution_rounds: 52,
                total_selected_without_progress_rounds: 12,
                total_connect_failure_rounds: 1,
                total_handshake_failure_rounds: 0,
                total_decode_failure_rounds: 0,
                total_timeout_failure_rounds: 1,
                total_validation_reject_rounds: 0,
                total_disconnect_rounds: 0,
                total_capacity_reject_rounds: 0,
                current_consecutive_connect_failures: 0,
                current_consecutive_handshake_failures: 0,
                current_consecutive_decode_failures: 0,
                current_consecutive_timeout_failures: 0,
                current_consecutive_validation_rejects: 0,
                current_consecutive_disconnects: 0,
                current_consecutive_selected_without_progress_rounds: 0,
                max_consecutive_connect_failures: 1,
                max_consecutive_handshake_failures: 0,
                max_consecutive_decode_failures: 0,
                max_consecutive_timeout_failures: 1,
                max_consecutive_validation_rejects: 0,
                max_consecutive_disconnects: 0,
                max_consecutive_selected_without_progress_rounds: 2,
                last_selected_unix_ms: now,
                last_progress_unix_ms: now,
                last_failure_unix_ms: now.saturating_sub(30_000),
                selection_hit_rate_bps: 7_222,
                header_success_rate_bps: 8_125,
                body_success_rate_bps: 6_250,
                ..EthPeerLongTermStatsV1::default()
            };

            let ephemeral = chain.get_mut(&ephemeral_peer.0).expect("ephemeral");
            ephemeral.cooldown_until_unix_ms = 0;
            ephemeral.session_ready = true;
            ephemeral.progress_stage = EthPeerLifecycleProgressStageV1::Ready;
            ephemeral.long_term = EthPeerLongTermStatsV1 {
                total_observed_rounds: 4,
                total_selected_rounds: 2,
                total_selected_bootstrap_rounds: 0,
                total_selected_sync_rounds: 2,
                total_header_success_rounds: 1,
                total_body_success_rounds: 0,
                total_sync_contribution_rounds: 1,
                total_selected_without_progress_rounds: 1,
                total_connect_failure_rounds: 0,
                total_handshake_failure_rounds: 0,
                total_decode_failure_rounds: 0,
                total_timeout_failure_rounds: 0,
                total_validation_reject_rounds: 0,
                total_disconnect_rounds: 0,
                total_capacity_reject_rounds: 0,
                current_consecutive_connect_failures: 0,
                current_consecutive_handshake_failures: 0,
                current_consecutive_decode_failures: 0,
                current_consecutive_timeout_failures: 0,
                current_consecutive_validation_rejects: 0,
                current_consecutive_disconnects: 0,
                current_consecutive_selected_without_progress_rounds: 1,
                max_consecutive_connect_failures: 0,
                max_consecutive_handshake_failures: 0,
                max_consecutive_decode_failures: 0,
                max_consecutive_timeout_failures: 0,
                max_consecutive_validation_rejects: 0,
                max_consecutive_disconnects: 0,
                max_consecutive_selected_without_progress_rounds: 1,
                last_selected_unix_ms: now,
                last_progress_unix_ms: now.saturating_sub(1_000),
                last_failure_unix_ms: 0,
                selection_hit_rate_bps: 5_000,
                header_success_rate_bps: 5_000,
                body_success_rate_bps: 0,
                ..EthPeerLongTermStatsV1::default()
            };
        }

        let selected = select_eth_fullnode_native_sync_targets_v1(
            chain_id,
            &[trusted_peer, ephemeral_peer],
            1,
        );
        assert_eq!(selected, vec![trusted_peer]);

        let (scores, _summary, long_term_summary) = snapshot_eth_fullnode_peer_selection_scores_v1(
            chain_id,
            &[trusted_peer, ephemeral_peer],
            &[],
            &[trusted_peer],
        );
        let trusted_score = scores
            .iter()
            .find(|score| {
                score.peer_id == trusted_peer.0
                    && matches!(score.role, EthPeerSelectionRoleV1::Sync)
            })
            .expect("trusted sync score");
        let ephemeral_score = scores
            .iter()
            .find(|score| {
                score.peer_id == ephemeral_peer.0
                    && matches!(score.role, EthPeerSelectionRoleV1::Sync)
            })
            .expect("ephemeral sync score");
        assert!(trusted_score.long_term_score > ephemeral_score.long_term_score);
        assert!(trusted_score.score > ephemeral_score.score);
        assert_eq!(
            long_term_summary.top_trusted_sync_peer_id,
            Some(trusted_peer.0)
        );
    }

    #[test]
    fn bootstrap_candidates_skip_cooldown_and_permanent_rejects() {
        let chain_id = 99_160_319_u64;
        let peer_a = NodeId(501);
        let peer_b = NodeId(502);
        let peer_c = NodeId(503);
        let _ = upsert_network_runtime_eth_peer_session(chain_id, peer_a.0, &[66, 68], &[1], None)
            .expect("hello-only peer");
        observe_network_runtime_eth_peer_disconnect_v1(chain_id, peer_a.0, Some(0x04));
        observe_network_runtime_eth_peer_validation_reject_v1(
            chain_id,
            peer_b.0,
            EthChainConfigPeerValidationReasonV1::WrongGenesis,
        );

        let selected = select_eth_fullnode_native_bootstrap_candidates_v1(
            chain_id,
            &[peer_a, peer_b, peer_c],
            3,
        );
        assert_eq!(selected, vec![peer_c]);
    }

    #[test]
    fn parity_progress_for_chain_marks_native_snap_state_machine_ready_after_full_cycle() {
        let chain_id = 99_160_316_u64;
        let peer_id = 88_u64;
        let _ = upsert_network_runtime_eth_peer_session(chain_id, peer_id, &[66, 68], &[1], None)
            .expect("negotiated");

        observe_eth_native_headers_pull(chain_id);
        observe_eth_native_headers_response(chain_id);
        observe_eth_native_bodies_pull(chain_id);
        observe_eth_native_bodies_response(chain_id);
        observe_eth_native_snap_pull(chain_id);
        observe_eth_native_snap_response(chain_id);

        let progress = current_eth_native_parity_progress_for_chain(chain_id);
        assert!(progress.native_snap_sync_state_machine);
    }

    #[test]
    fn parity_progress_for_chain_marks_native_rlpx_profile_after_handshake_cycle() {
        let chain_id = 99_160_317_u64;
        observe_eth_native_discovery(chain_id);
        observe_eth_native_rlpx_auth(chain_id);
        observe_eth_native_rlpx_auth_ack(chain_id);
        observe_eth_native_hello(chain_id);
        observe_eth_native_status(chain_id);
        let progress = current_eth_native_parity_progress_for_chain(chain_id);
        assert_eq!(progress.profile, EthSyncTransportProfile::NativeDevp2pRlpx);
        assert!(progress.native_peer_discovery);
        assert!(progress.native_eth_handshake);
    }

    #[test]
    fn canonical_query_method_set_is_pinned() {
        assert_eq!(
            resolve_eth_fullnode_canonical_query_method("eth_blockNumber"),
            Some(EthFullnodeCanonicalQueryMethod::BlockNumber)
        );
        assert_eq!(
            resolve_eth_fullnode_canonical_query_method("eth_getBlockByNumber"),
            Some(EthFullnodeCanonicalQueryMethod::GetBlockByNumber)
        );
        assert_eq!(
            resolve_eth_fullnode_canonical_query_method("eth_getBlockByHash"),
            Some(EthFullnodeCanonicalQueryMethod::GetBlockByHash)
        );
        assert_eq!(
            resolve_eth_fullnode_canonical_query_method("eth_syncing"),
            Some(EthFullnodeCanonicalQueryMethod::Syncing)
        );
        assert_eq!(
            resolve_eth_fullnode_canonical_query_method("eth_getTransactionReceipt"),
            Some(EthFullnodeCanonicalQueryMethod::GetTransactionReceipt)
        );
        assert_eq!(
            resolve_eth_fullnode_canonical_query_method("eth_getLogs"),
            Some(EthFullnodeCanonicalQueryMethod::GetLogs)
        );
        assert!(is_eth_fullnode_canonical_query_method(
            "eth_getTransactionReceipt"
        ));
        assert!(is_eth_fullnode_canonical_query_method("eth_getLogs"));
        assert!(is_eth_fullnode_canonical_query_method("eth_blockNumber"));
        assert!(is_eth_fullnode_canonical_query_method(
            "eth_getBlockByNumber"
        ));
        assert!(is_eth_fullnode_canonical_query_method("eth_getBlockByHash"));
        assert!(is_eth_fullnode_canonical_query_method("eth_syncing"));
        assert!(!is_eth_fullnode_canonical_query_method("eth_getBalance"));
        assert_eq!(ETH_FULLNODE_CANONICAL_QUERY_METHODS.len(), 6);
    }

    #[test]
    fn runtime_query_method_set_includes_selection_summary() {
        assert_eq!(
            resolve_eth_fullnode_runtime_query_method("supervm_getEthNativeWorkerRuntime"),
            Some(EthFullnodeRuntimeQueryMethod::NativeWorkerRuntime)
        );
        assert_eq!(
            resolve_eth_fullnode_runtime_query_method("supervm_getEthNativePeerRuntimeState"),
            Some(EthFullnodeRuntimeQueryMethod::NativePeerRuntimeState)
        );
        assert_eq!(
            resolve_eth_fullnode_runtime_query_method("supervm_getEthNativeSyncRuntimeSummary"),
            Some(EthFullnodeRuntimeQueryMethod::NativeSyncRuntimeSummary)
        );
        assert_eq!(
            resolve_eth_fullnode_runtime_query_method("supervm_getEthNativePeerHealthSummary"),
            Some(EthFullnodeRuntimeQueryMethod::NativePeerHealthSummary)
        );
        assert_eq!(
            resolve_eth_fullnode_runtime_query_method("supervm_getEthNativeSyncDegradationSummary"),
            Some(EthFullnodeRuntimeQueryMethod::NativeSyncDegradationSummary)
        );
        assert_eq!(
            resolve_eth_fullnode_runtime_query_method("supervm_getEthNativeRuntimeConfig"),
            Some(EthFullnodeRuntimeQueryMethod::NativeRuntimeConfig)
        );
        assert_eq!(
            resolve_eth_fullnode_runtime_query_method("supervm_getEthNativeCanonicalChainSummary"),
            Some(EthFullnodeRuntimeQueryMethod::NativeCanonicalChainSummary)
        );
        assert_eq!(
            resolve_eth_fullnode_runtime_query_method("supervm_getEthNativeBlockLifecycleByNumber"),
            Some(EthFullnodeRuntimeQueryMethod::NativeBlockLifecycleByNumber)
        );
        assert_eq!(
            resolve_eth_fullnode_runtime_query_method("supervm_getEthNativeBlockLifecycleByHash"),
            Some(EthFullnodeRuntimeQueryMethod::NativeBlockLifecycleByHash)
        );
        assert_eq!(
            resolve_eth_fullnode_runtime_query_method("supervm_getEthNativePendingTxSummary"),
            Some(EthFullnodeRuntimeQueryMethod::NativePendingTxSummary)
        );
        assert_eq!(
            resolve_eth_fullnode_runtime_query_method(
                "supervm_getEthNativePendingTxPropagationSummary"
            ),
            Some(EthFullnodeRuntimeQueryMethod::NativePendingTxPropagationSummary)
        );
        assert_eq!(
            resolve_eth_fullnode_runtime_query_method("supervm_getEthNativePendingTxByHash"),
            Some(EthFullnodeRuntimeQueryMethod::NativePendingTxByHash)
        );
        assert_eq!(
            resolve_eth_fullnode_runtime_query_method("supervm_getEthNativePendingTxTombstones"),
            Some(EthFullnodeRuntimeQueryMethod::NativePendingTxTombstones)
        );
        assert_eq!(
            resolve_eth_fullnode_runtime_query_method(
                "supervm_getEthNativePendingTxTombstoneByHash"
            ),
            Some(EthFullnodeRuntimeQueryMethod::NativePendingTxTombstoneByHash)
        );
        assert_eq!(
            resolve_eth_fullnode_runtime_query_method(
                "supervm_getEthNativePendingTxCleanupSummary"
            ),
            Some(EthFullnodeRuntimeQueryMethod::NativePendingTxCleanupSummary)
        );
        assert_eq!(
            resolve_eth_fullnode_runtime_query_method(
                "supervm_getEthNativePendingTxBroadcastCandidates"
            ),
            Some(EthFullnodeRuntimeQueryMethod::NativePendingTxBroadcastCandidates)
        );
        assert_eq!(
            resolve_eth_fullnode_runtime_query_method("supervm_getEthNativePeerSelectionSummary"),
            Some(EthFullnodeRuntimeQueryMethod::NativePeerSelectionSummary)
        );
        assert_eq!(ETH_FULLNODE_RUNTIME_QUERY_METHODS.len(), 17);
    }

    #[test]
    fn canonical_host_batch_block_view_derives_chain_view() {
        let blocks = vec![
            EthFullnodeBlockContextV1 {
                source: EthFullnodeBlockViewSource::CanonicalHostBatch,
                chain_id: 1,
                block_number: 5,
                canonical_batch_seq: Some(5),
                block_hash: [0x11; 32],
                parent_block_hash: [0x00; 32],
                state_root: [0x21; 32],
                state_version: 9,
                tx_count: 1,
            },
            EthFullnodeBlockContextV1 {
                source: EthFullnodeBlockViewSource::CanonicalHostBatch,
                chain_id: 1,
                block_number: 6,
                canonical_batch_seq: Some(6),
                block_hash: [0x12; 32],
                parent_block_hash: [0x11; 32],
                state_root: [0x22; 32],
                state_version: 10,
                tx_count: 2,
            },
        ];

        let view = derive_eth_fullnode_chain_view_v1(&blocks).expect("chain view");
        assert_eq!(view.source, EthFullnodeBlockViewSource::CanonicalHostBatch);
        assert_eq!(view.starting_block_number, 5);
        assert_eq!(view.current_block_number, 6);
        assert_eq!(view.highest_block_number, 6);
        assert_eq!(view.current_block_hash, [0x12; 32]);
        assert_eq!(view.parent_block_hash, [0x11; 32]);
        assert_eq!(view.current_state_root, [0x22; 32]);
        assert_eq!(view.current_state_version, 10);
        assert_eq!(view.total_blocks, 2);
    }

    #[test]
    fn block_view_source_strings_are_stable() {
        assert_eq!(
            EthFullnodeBlockViewSource::CanonicalHostBatch.as_str(),
            "canonical_host_batch"
        );
        assert_eq!(
            EthFullnodeBlockViewSource::NativeChainSync.as_str(),
            "native_chain_sync"
        );
        assert_eq!(
            EthFullnodeSourceDowngradeReasonV1::CanonicalProjectionOnly.as_str(),
            "canonical_projection_only"
        );
        assert_eq!(
            EthFullnodeSourceDowngradeReasonV1::NativeSyncActiveBlockObjectUnavailable.as_str(),
            "native_sync_active_block_object_unavailable"
        );
        assert_eq!(
            EthFullnodeSourceDowngradeReasonV1::NoAvailableBlockSource.as_str(),
            "no_available_block_source"
        );
    }

    #[test]
    fn head_view_projects_from_chain_view() {
        let chain_view = EthFullnodeChainViewV1 {
            source: EthFullnodeBlockViewSource::CanonicalHostBatch,
            chain_id: 1,
            starting_block_number: 5,
            current_block_number: 6,
            highest_block_number: 6,
            current_block_hash: [0x42; 32],
            parent_block_hash: [0x41; 32],
            current_state_root: [0x51; 32],
            current_state_version: 9,
            total_blocks: 2,
        };
        let head = derive_eth_fullnode_head_view_v1(&chain_view);
        assert_eq!(head.source, EthFullnodeBlockViewSource::CanonicalHostBatch);
        assert_eq!(head.block_number, 6);
        assert_eq!(head.block_hash, [0x42; 32]);
        assert_eq!(head.parent_block_hash, [0x41; 32]);
        assert_eq!(head.state_root, [0x51; 32]);
        assert_eq!(head.state_version, 9);
        assert_eq!(
            head.source_priority_policy.head_source,
            EthFullnodeBlockViewSource::CanonicalHostBatch
        );
        assert_eq!(
            head.source_priority_policy.downgrade_reason,
            Some(EthFullnodeSourceDowngradeReasonV1::CanonicalProjectionOnly)
        );
    }

    #[test]
    fn sync_view_prefers_native_gap_over_chain_view_when_active() {
        let chain_view = EthFullnodeChainViewV1 {
            source: EthFullnodeBlockViewSource::CanonicalHostBatch,
            chain_id: 99,
            starting_block_number: 5,
            current_block_number: 10,
            highest_block_number: 10,
            current_block_hash: [0x61; 32],
            parent_block_hash: [0x60; 32],
            current_state_root: [0x71; 32],
            current_state_version: 12,
            total_blocks: 6,
        };
        let sync_view = derive_eth_fullnode_sync_view_v1(
            Some(&chain_view),
            Some(NetworkRuntimeSyncStatus {
                peer_count: 2,
                starting_block: 8,
                current_block: 11,
                highest_block: 14,
            }),
            Some(NetworkRuntimeNativeSyncStatusV1 {
                phase: crate::runtime_status::NetworkRuntimeNativeSyncPhaseV1::Headers,
                peer_count: 3,
                starting_block: 9,
                current_block: 12,
                highest_block: 16,
                updated_at_unix_millis: 1,
            }),
        )
        .expect("sync view");
        assert_eq!(
            sync_view.source,
            EthFullnodeBlockViewSource::NativeChainSync
        );
        assert_eq!(sync_view.peer_count, 3);
        assert_eq!(sync_view.starting_block_number, 5);
        assert_eq!(sync_view.current_block_number, 12);
        assert_eq!(sync_view.highest_block_number, 16);
        assert_eq!(sync_view.native_sync_phase.as_deref(), Some("headers"));
        assert!(sync_view.syncing);
        assert_eq!(
            sync_view.source_priority_policy.head_source,
            EthFullnodeBlockViewSource::CanonicalHostBatch
        );
        assert_eq!(
            sync_view.source_priority_policy.block_object_source,
            EthFullnodeBlockViewSource::CanonicalHostBatch
        );
        assert_eq!(
            sync_view.source_priority_policy.sync_source,
            EthFullnodeBlockViewSource::NativeChainSync
        );
        assert_eq!(
            sync_view.source_priority_policy.downgrade_reason,
            Some(EthFullnodeSourceDowngradeReasonV1::NativeSyncActiveBlockObjectUnavailable)
        );
    }

    #[test]
    fn default_budget_hooks_are_pinned() {
        let budget = default_eth_fullnode_budget_hooks_v1();
        assert_eq!(budget.native_recv_budget_per_tick, 128);
        assert_eq!(budget.sync_target_fanout, 1);
        assert_eq!(budget.rlpx_request_timeout_ms, 5_000);
        assert_eq!(budget.sync_request_interval_ms, 1_000);
        assert_eq!(budget.tx_broadcast_interval_ms, 1_000);
        assert_eq!(budget.tx_broadcast_max_per_tick, 8);
        assert_eq!(budget.tx_broadcast_max_propagations, 3);
        assert_eq!(budget.runtime_query_result_max, 2_048);
        assert_eq!(budget.runtime_block_snapshot_limit, 1_024);
        assert_eq!(budget.runtime_pending_tx_snapshot_limit, 2_048);
        assert_eq!(budget.pending_tx_canonical_retain_depth, 128);
        assert_eq!(budget.pending_tx_reorg_return_window_ms, 900_000);
        assert_eq!(budget.pending_tx_ttl_ms, 1_800_000);
        assert_eq!(budget.pending_tx_no_success_attempt_limit, 12);
        assert_eq!(budget.pending_tx_tombstone_retention_max, 4_096);
        assert_eq!(budget.host_exec_budget_per_tick, 64);
        assert_eq!(budget.host_exec_time_slice_ms, 10);
        assert_eq!(budget.host_exec_target_per_tick, 64);
        assert_eq!(budget.host_exec_target_time_slice_ms, 10);
        assert_eq!(budget.sync_pull_headers_batch, 2_048);
        assert_eq!(budget.sync_pull_bodies_batch, 256);
        assert_eq!(budget.sync_pull_state_batch, 64);
        assert_eq!(budget.sync_pull_finalize_batch, 16);
        assert_eq!(budget.sync_decode_concurrency, 1);
        assert_eq!(budget.sync_apply_concurrency, 1);
        assert_eq!(budget.native_block_store_flush_batch, 64);
        assert_eq!(budget.block_query_scan_max, 2_048);
        assert_eq!(budget.active_native_peer_soft_limit, 8);
        assert_eq!(budget.active_native_peer_hard_limit, 16);
    }

    #[test]
    fn default_selection_window_policy_is_pinned() {
        let policy = default_eth_peer_selection_window_policy_v1();
        assert_eq!(policy.short_term_rounds, 16);
        assert_eq!(policy.medium_term_rounds, 64);
        assert_eq!(policy.long_term_rounds, 256);
        assert_eq!(policy.sync_short_term_weight_bps, 10_000);
        assert_eq!(policy.sync_medium_term_weight_bps, 8_500);
        assert_eq!(policy.sync_long_term_weight_bps, 9_500);
        assert_eq!(policy.bootstrap_short_term_weight_bps, 10_000);
        assert_eq!(policy.bootstrap_medium_term_weight_bps, 6_000);
        assert_eq!(policy.bootstrap_long_term_weight_bps, 3_500);
        assert_eq!(policy.medium_term_selection_hit_rate_floor_bps, 4_500);
        assert_eq!(policy.long_term_selection_hit_rate_floor_bps, 4_000);
        assert_eq!(policy.long_term_body_success_rate_floor_bps, 2_000);
    }

    #[test]
    fn native_block_object_derives_native_chain_view() {
        let native_block = EthNativeBlockObjectV1 {
            header: EthNativeBlockHeaderV1 {
                chain_id: 1,
                number: 44,
                hash: [0x81; 32],
                parent_hash: [0x80; 32],
                state_root: [0x91; 32],
                transactions_root: [0x92; 32],
                receipts_root: [0x93; 32],
                ommers_hash: [0x94; 32],
                logs_bloom: vec![0u8; 256],
                gas_limit: Some(30_000_000),
                gas_used: Some(21_000),
                timestamp: Some(1),
                base_fee_per_gas: Some(7),
                withdrawals_root: None,
                blob_gas_used: None,
                excess_blob_gas: None,
                source_peer_id: Some(7),
                observed_unix_ms: 1,
            },
            body: Some(EthNativeBlockBodyV1 {
                tx_hashes: vec![[0x95; 32]],
                tx_count: 1,
                ommer_hashes: Vec::new(),
                withdrawal_count: Some(0),
                body_available: true,
                txs_materialized: true,
                observed_unix_ms: 1,
            }),
            canonical: true,
            safe: true,
            finalized: false,
            reorg_depth_hint: Some(0),
            source_peer_count: 3,
        };

        let chain_view = derive_eth_fullnode_chain_view_from_native_block_v1(&native_block, 40, 5);
        assert_eq!(
            chain_view.source,
            EthFullnodeBlockViewSource::NativeChainSync
        );
        assert_eq!(chain_view.starting_block_number, 40);
        assert_eq!(chain_view.current_block_number, 44);
        assert_eq!(chain_view.current_block_hash, [0x81; 32]);
        assert_eq!(chain_view.parent_block_hash, [0x80; 32]);
        assert_eq!(chain_view.current_state_root, [0x91; 32]);
        assert_eq!(chain_view.total_blocks, 5);
    }

    #[test]
    fn source_priority_policy_prefers_native_block_object_over_canonical_projection() {
        let chain_view = EthFullnodeChainViewV1 {
            source: EthFullnodeBlockViewSource::CanonicalHostBatch,
            chain_id: 1,
            starting_block_number: 5,
            current_block_number: 6,
            highest_block_number: 6,
            current_block_hash: [0x42; 32],
            parent_block_hash: [0x41; 32],
            current_state_root: [0x51; 32],
            current_state_version: 9,
            total_blocks: 2,
        };
        let native_block = EthNativeBlockObjectV1 {
            header: EthNativeBlockHeaderV1 {
                chain_id: 1,
                number: 7,
                hash: [0xa1; 32],
                parent_hash: [0xa0; 32],
                state_root: [0xb1; 32],
                transactions_root: [0xb2; 32],
                receipts_root: [0xb3; 32],
                ommers_hash: [0xb4; 32],
                logs_bloom: vec![0u8; 256],
                gas_limit: None,
                gas_used: None,
                timestamp: None,
                base_fee_per_gas: None,
                withdrawals_root: None,
                blob_gas_used: None,
                excess_blob_gas: None,
                source_peer_id: None,
                observed_unix_ms: 1,
            },
            body: None,
            canonical: true,
            safe: false,
            finalized: false,
            reorg_depth_hint: None,
            source_peer_count: 2,
        };

        let policy = derive_eth_fullnode_source_priority_policy_v1(
            Some(&chain_view),
            Some(&native_block),
            None,
            None,
        );
        assert_eq!(
            policy.head_source,
            EthFullnodeBlockViewSource::NativeChainSync
        );
        assert_eq!(
            policy.block_object_source,
            EthFullnodeBlockViewSource::NativeChainSync
        );
        assert_eq!(
            policy.sync_source,
            EthFullnodeBlockViewSource::NativeChainSync
        );
        assert_eq!(policy.downgrade_reason, None);
    }

    #[test]
    fn runtime_native_snapshots_build_block_object_and_native_preferred_views() {
        let chain_id = 99_160_411_u64;
        crate::runtime_status::set_network_runtime_native_header_snapshot_v1(
            chain_id,
            crate::runtime_status::NetworkRuntimeNativeHeaderSnapshotV1 {
                chain_id: 0,
                number: 77,
                hash: [0xc1; 32],
                parent_hash: [0xc0; 32],
                state_root: [0xd1; 32],
                transactions_root: [0xd2; 32],
                receipts_root: [0xd3; 32],
                ommers_hash: [0xd4; 32],
                logs_bloom: vec![0u8; 256],
                gas_limit: Some(30_000_000),
                gas_used: Some(42_000),
                timestamp: Some(17),
                base_fee_per_gas: Some(9),
                withdrawals_root: None,
                blob_gas_used: None,
                excess_blob_gas: None,
                source_peer_id: Some(55),
                observed_unix_ms: 10,
            },
        );
        crate::runtime_status::set_network_runtime_native_body_snapshot_v1(
            chain_id,
            crate::runtime_status::NetworkRuntimeNativeBodySnapshotV1 {
                chain_id: 0,
                number: 77,
                block_hash: [0xc1; 32],
                tx_hashes: vec![[0xe1; 32], [0xe2; 32]],
                ommer_hashes: vec![[0xf1; 32]],
                withdrawal_count: Some(0),
                body_available: true,
                txs_materialized: true,
                observed_unix_ms: 11,
            },
        );
        crate::runtime_status::set_network_runtime_native_head_snapshot_v1(
            chain_id,
            crate::runtime_status::NetworkRuntimeNativeHeadSnapshotV1 {
                chain_id: 0,
                phase: crate::runtime_status::NetworkRuntimeNativeSyncPhaseV1::Bodies,
                peer_count: 4,
                block_number: 77,
                block_hash: [0xc1; 32],
                parent_block_hash: [0xc0; 32],
                state_root: [0xd1; 32],
                canonical: true,
                safe: true,
                finalized: false,
                reorg_depth_hint: Some(2),
                body_available: true,
                source_peer_id: Some(55),
                observed_unix_ms: 12,
            },
        );

        let native_block =
            snapshot_eth_fullnode_native_head_block_object_v1(chain_id).expect("native block");
        assert_eq!(native_block.header.chain_id, chain_id);
        assert_eq!(native_block.header.number, 77);
        assert_eq!(native_block.header.hash, [0xc1; 32]);
        assert_eq!(native_block.source_peer_count, 4);
        assert_eq!(
            native_block.body.as_ref().map(|body| body.tx_count),
            Some(2)
        );

        let fallback_chain_view = EthFullnodeChainViewV1 {
            source: EthFullnodeBlockViewSource::CanonicalHostBatch,
            chain_id,
            starting_block_number: 70,
            current_block_number: 76,
            highest_block_number: 76,
            current_block_hash: [0xb1; 32],
            parent_block_hash: [0xb0; 32],
            current_state_root: [0xb2; 32],
            current_state_version: 9,
            total_blocks: 7,
        };
        let runtime_sync = NetworkRuntimeSyncStatus {
            peer_count: 2,
            starting_block: 70,
            current_block: 77,
            highest_block: 90,
        };
        let runtime_native_sync = NetworkRuntimeNativeSyncStatusV1 {
            phase: crate::runtime_status::NetworkRuntimeNativeSyncPhaseV1::Bodies,
            peer_count: 4,
            starting_block: 70,
            current_block: 77,
            highest_block: 95,
            updated_at_unix_millis: 12,
        };

        let chain_view = derive_eth_fullnode_chain_view_with_native_preference_v1(
            Some(&fallback_chain_view),
            Some(&native_block),
            None,
        )
        .expect("chain view");
        assert_eq!(
            chain_view.source,
            EthFullnodeBlockViewSource::NativeChainSync
        );
        assert_eq!(chain_view.current_block_number, 77);
        assert_eq!(chain_view.current_block_hash, [0xc1; 32]);
        assert_eq!(chain_view.parent_block_hash, [0xc0; 32]);
        assert_eq!(chain_view.current_state_root, [0xd1; 32]);

        let head_view = derive_eth_fullnode_head_view_with_native_preference_v1(
            Some(&fallback_chain_view),
            Some(&native_block),
            None,
            Some(runtime_native_sync),
        )
        .expect("head view");
        assert_eq!(
            head_view.source,
            EthFullnodeBlockViewSource::NativeChainSync
        );
        assert_eq!(head_view.block_number, 77);
        assert_eq!(
            head_view.source_priority_policy.head_source,
            EthFullnodeBlockViewSource::NativeChainSync
        );
        assert_eq!(head_view.source_priority_policy.downgrade_reason, None);

        let sync_view = derive_eth_fullnode_sync_view_with_native_preference_v1(
            Some(&fallback_chain_view),
            Some(&native_block),
            None,
            Some(runtime_sync),
            Some(runtime_native_sync),
        )
        .expect("sync view");
        assert_eq!(
            sync_view.source,
            EthFullnodeBlockViewSource::NativeChainSync
        );
        assert_eq!(sync_view.current_block_number, 77);
        assert_eq!(sync_view.highest_block_number, 95);
        assert_eq!(sync_view.current_block_hash, [0xc1; 32]);
        assert_eq!(sync_view.parent_block_hash, [0xc0; 32]);
        assert_eq!(sync_view.current_state_root, [0xd1; 32]);
        assert_eq!(sync_view.native_sync_phase.as_deref(), Some("bodies"));
        assert_eq!(
            sync_view.source_priority_policy.block_object_source,
            EthFullnodeBlockViewSource::NativeChainSync
        );
        assert_eq!(sync_view.source_priority_policy.downgrade_reason, None);
    }

    #[test]
    fn build_native_rlpx_status_prefers_local_canonical_head_and_genesis() {
        let chain_id = 1_u64;
        crate::runtime_status::set_network_runtime_native_header_snapshot_v1(
            chain_id,
            crate::runtime_status::NetworkRuntimeNativeHeaderSnapshotV1 {
                chain_id,
                number: 77,
                hash: [0xa1; 32],
                parent_hash: [0xa0; 32],
                state_root: [0xb1; 32],
                transactions_root: [0xb2; 32],
                receipts_root: [0xb3; 32],
                ommers_hash: [0xb4; 32],
                logs_bloom: vec![0u8; 256],
                gas_limit: Some(30_000_000),
                gas_used: Some(42_000),
                timestamp: Some(17),
                base_fee_per_gas: Some(9),
                withdrawals_root: None,
                blob_gas_used: None,
                excess_blob_gas: None,
                source_peer_id: Some(55),
                observed_unix_ms: 10,
            },
        );
        crate::runtime_status::set_network_runtime_native_head_snapshot_v1(
            chain_id,
            crate::runtime_status::NetworkRuntimeNativeHeadSnapshotV1 {
                chain_id,
                phase: crate::runtime_status::NetworkRuntimeNativeSyncPhaseV1::Headers,
                peer_count: 2,
                block_number: 77,
                block_hash: [0xa1; 32],
                parent_block_hash: [0xa0; 32],
                state_root: [0xb1; 32],
                canonical: true,
                safe: false,
                finalized: false,
                reorg_depth_hint: None,
                body_available: false,
                source_peer_id: Some(55),
                observed_unix_ms: 11,
            },
        );
        crate::runtime_status::set_network_runtime_sync_status(
            chain_id,
            crate::runtime_status::NetworkRuntimeSyncStatus {
                peer_count: 2,
                starting_block: 40,
                current_block: 77,
                highest_block: 120,
            },
        );
        let status = build_eth_fullnode_native_rlpx_status_v1(chain_id, 70);
        assert_eq!(status.network_id, chain_id);
        assert_eq!(status.protocol_version, 70);
        assert_eq!(status.earliest_block, 40);
        assert_eq!(status.latest_block, 77);
        assert_eq!(status.latest_block_hash, [0xa1; 32]);
        assert_eq!(status.genesis_hash, crate::ETH_MAINNET_GENESIS_HASH_V1);
        assert_eq!(
            status.fork_id,
            crate::EthForkIdV1 {
                hash: [0xfc, 0x64, 0xec, 0x04],
                next: 1_150_000,
            }
        );
    }

    #[test]
    fn local_mainnet_fork_id_matches_geth_reference_vectors() {
        let config = crate::resolve_eth_chain_config_v1(1);
        assert_eq!(
            crate::build_eth_fork_id_from_chain_config_v1(&config, 0, 0),
            crate::EthForkIdV1 {
                hash: [0xfc, 0x64, 0xec, 0x04],
                next: 1_150_000,
            }
        );
        assert_eq!(
            crate::build_eth_fork_id_from_chain_config_v1(&config, 15_050_000, 0),
            crate::EthForkIdV1 {
                hash: [0xf0, 0xaf, 0xd0, 0xe3],
                next: 1_681_338_455,
            }
        );
        assert_eq!(
            crate::build_eth_fork_id_from_chain_config_v1(&config, 30_000_000, 1_710_338_135),
            crate::EthForkIdV1 {
                hash: [0x9f, 0x3d, 0x22, 0x54],
                next: 1_746_612_311,
            }
        );
    }

    #[test]
    fn canonical_chain_head_overrides_higher_noncanonical_native_head_in_preferred_views() {
        let chain_id = 1;
        let fallback_chain_view = EthFullnodeChainViewV1 {
            source: EthFullnodeBlockViewSource::CanonicalHostBatch,
            chain_id,
            starting_block_number: 70,
            current_block_number: 76,
            highest_block_number: 76,
            current_block_hash: [0xb1; 32],
            parent_block_hash: [0xb0; 32],
            current_state_root: [0xb2; 32],
            current_state_version: 9,
            total_blocks: 7,
        };
        let native_block = EthNativeBlockObjectV1 {
            header: EthNativeBlockHeaderV1 {
                chain_id,
                number: 78,
                hash: [0xd1; 32],
                parent_hash: [0xd0; 32],
                state_root: [0xd2; 32],
                transactions_root: [0xd3; 32],
                receipts_root: [0xd4; 32],
                ommers_hash: [0xd5; 32],
                logs_bloom: vec![0u8; 256],
                gas_limit: Some(30_000_000),
                gas_used: Some(21_000),
                timestamp: Some(1_000),
                base_fee_per_gas: Some(7),
                withdrawals_root: None,
                blob_gas_used: None,
                excess_blob_gas: None,
                source_peer_id: Some(77),
                observed_unix_ms: 18,
            },
            body: Some(EthNativeBlockBodyV1 {
                tx_hashes: vec![[0xa1; 32]],
                tx_count: 1,
                ommer_hashes: vec![],
                withdrawal_count: Some(0),
                body_available: true,
                txs_materialized: true,
                observed_unix_ms: 18,
            }),
            canonical: false,
            safe: false,
            finalized: false,
            reorg_depth_hint: Some(1),
            source_peer_count: 3,
        };
        let native_canonical_chain = crate::NetworkRuntimeNativeCanonicalChainStateV1 {
            chain_id,
            lifecycle_stage: crate::NetworkRuntimeNativeCanonicalLifecycleStageV1::Advanced,
            head: Some(crate::NetworkRuntimeNativeCanonicalBlockStateV1 {
                chain_id,
                number: 77,
                hash: [0xc1; 32],
                parent_hash: [0xc0; 32],
                state_root: [0xc2; 32],
                header_observed: true,
                body_available: true,
                lifecycle_stage: crate::NetworkRuntimeNativeBlockLifecycleStageV1::Canonical,
                canonical: true,
                safe: true,
                finalized: false,
                source_peer_id: Some(55),
                observed_unix_ms: 17,
            }),
            retained_block_count: 8,
            canonical_block_count: 8,
            canonical_update_count: 3,
            reorg_count: 1,
            last_reorg_depth: Some(2),
            last_reorg_unix_ms: Some(16),
            last_head_change_unix_ms: Some(17),
            block_lifecycle_summary: crate::NetworkRuntimeNativeBlockLifecycleSummaryV1 {
                seen_count: 0,
                header_only_count: 0,
                body_ready_count: 0,
                canonical_count: 8,
                non_canonical_count: 1,
                reorged_out_count: 1,
            },
        };
        let runtime_sync = NetworkRuntimeSyncStatus {
            peer_count: 2,
            starting_block: 70,
            current_block: 77,
            highest_block: 90,
        };
        let runtime_native_sync = NetworkRuntimeNativeSyncStatusV1 {
            phase: crate::runtime_status::NetworkRuntimeNativeSyncPhaseV1::Bodies,
            peer_count: 3,
            starting_block: 70,
            current_block: 77,
            highest_block: 95,
            updated_at_unix_millis: 18,
        };

        let chain_view = derive_eth_fullnode_chain_view_with_native_preference_v1(
            Some(&fallback_chain_view),
            Some(&native_block),
            Some(&native_canonical_chain),
        )
        .expect("chain view");
        assert_eq!(
            chain_view.source,
            EthFullnodeBlockViewSource::NativeChainSync
        );
        assert_eq!(chain_view.current_block_number, 77);
        assert_eq!(chain_view.current_block_hash, [0xc1; 32]);
        assert_eq!(chain_view.parent_block_hash, [0xc0; 32]);
        assert_eq!(chain_view.current_state_root, [0xc2; 32]);

        let head_view = derive_eth_fullnode_head_view_with_native_preference_v1(
            Some(&fallback_chain_view),
            Some(&native_block),
            Some(&native_canonical_chain),
            Some(runtime_native_sync),
        )
        .expect("head view");
        assert_eq!(head_view.block_number, 77);
        assert_eq!(head_view.block_hash, [0xc1; 32]);
        assert_eq!(
            head_view.source_priority_policy.head_source,
            EthFullnodeBlockViewSource::NativeChainSync
        );
        assert_eq!(head_view.source_priority_policy.downgrade_reason, None);

        let sync_view = derive_eth_fullnode_sync_view_with_native_preference_v1(
            Some(&fallback_chain_view),
            Some(&native_block),
            Some(&native_canonical_chain),
            Some(runtime_sync),
            Some(runtime_native_sync),
        )
        .expect("sync view");
        assert_eq!(sync_view.current_block_number, 77);
        assert_eq!(sync_view.current_block_hash, [0xc1; 32]);
        assert_eq!(sync_view.parent_block_hash, [0xc0; 32]);
        assert_eq!(sync_view.current_state_root, [0xc2; 32]);
        assert_eq!(sync_view.highest_block_number, 95);
        assert_eq!(sync_view.native_sync_phase.as_deref(), Some("bodies"));
    }

    #[test]
    fn native_worker_runtime_snapshot_round_trips_in_memory_and_json() {
        let chain_id = 9_914_101_u64;
        clear_eth_fullnode_native_worker_runtime_snapshot_for_chain_v1(chain_id);
        let snapshot = EthFullnodeNativeWorkerRuntimeSnapshotV1 {
            schema: ETH_FULLNODE_NATIVE_WORKER_RUNTIME_SCHEMA_V1.to_string(),
            chain_id,
            updated_at_unix_ms: 55,
            candidate_peer_ids: vec![11, 12],
            scheduled_bootstrap_peers: 2,
            scheduled_sync_peers: 1,
            attempted_bootstrap_peers: 2,
            attempted_sync_peers: 1,
            failed_bootstrap_peers: 1,
            failed_sync_peers: 0,
            skipped_missing_endpoint_peers: 0,
            connected_peers: 1,
            ready_peers: 1,
            status_updates: 1,
            header_updates: 1,
            body_updates: 0,
            sync_requests: 1,
            inbound_frames: 2,
            head_view: None,
            sync_view: None,
            native_canonical_chain: Some(crate::NetworkRuntimeNativeCanonicalChainStateV1 {
                chain_id,
                lifecycle_stage: crate::NetworkRuntimeNativeCanonicalLifecycleStageV1::Advanced,
                head: Some(crate::NetworkRuntimeNativeCanonicalBlockStateV1 {
                    chain_id,
                    number: 7,
                    hash: [0x71; 32],
                    parent_hash: [0x70; 32],
                    state_root: [0x72; 32],
                    header_observed: true,
                    body_available: true,
                    lifecycle_stage: crate::NetworkRuntimeNativeBlockLifecycleStageV1::Canonical,
                    canonical: true,
                    safe: false,
                    finalized: false,
                    source_peer_id: Some(11),
                    observed_unix_ms: 55,
                }),
                retained_block_count: 2,
                canonical_block_count: 2,
                canonical_update_count: 1,
                reorg_count: 0,
                last_reorg_depth: None,
                last_reorg_unix_ms: None,
                last_head_change_unix_ms: Some(55),
                block_lifecycle_summary: crate::NetworkRuntimeNativeBlockLifecycleSummaryV1 {
                    seen_count: 0,
                    header_only_count: 0,
                    body_ready_count: 0,
                    canonical_count: 2,
                    non_canonical_count: 0,
                    reorged_out_count: 0,
                },
            }),
            native_canonical_blocks: vec![crate::NetworkRuntimeNativeCanonicalBlockStateV1 {
                chain_id,
                number: 7,
                hash: [0x71; 32],
                parent_hash: [0x70; 32],
                state_root: [0x72; 32],
                header_observed: true,
                body_available: true,
                lifecycle_stage: crate::NetworkRuntimeNativeBlockLifecycleStageV1::Canonical,
                canonical: true,
                safe: false,
                finalized: false,
                source_peer_id: Some(11),
                observed_unix_ms: 55,
            }],
            native_pending_tx_summary: Default::default(),
            native_pending_tx_broadcast_runtime: Default::default(),
            native_execution_budget_runtime: Default::default(),
            native_pending_txs: Vec::new(),
            native_head_body_available: None,
            native_head_canonical: None,
            native_head_safe: None,
            native_head_finalized: None,
            lifecycle_summary: EthPeerLifecycleSummaryV1 {
                chain_id,
                peer_count: 2,
                discovered_count: 0,
                connecting_count: 0,
                connected_count: 1,
                hello_ok_count: 1,
                status_ok_count: 1,
                ready_count: 1,
                syncing_count: 1,
                cooldown_count: 0,
                temporarily_failed_count: 1,
                permanently_rejected_count: 0,
                retry_eligible_count: 2,
                connect_failure_count: 1,
                handshake_failure_count: 0,
                decode_failure_count: 0,
                timeout_count: 0,
                validation_reject_count: 0,
                disconnect_count: 0,
            },
            selection_quality_summary: EthPeerSelectionQualitySummaryV1 {
                chain_id,
                candidate_peer_count: 2,
                evaluated_bootstrap_peers: 2,
                evaluated_sync_peers: 2,
                retry_eligible_bootstrap_peers: 1,
                ready_sync_peers: 1,
                selected_bootstrap_peers: 1,
                selected_sync_peers: 1,
                skipped_cooldown_peers: 0,
                skipped_permanently_rejected_peers: 0,
                skipped_unready_sync_peers: 1,
                top_selected_bootstrap_peer_id: Some(11),
                top_selected_sync_peer_id: Some(11),
                top_selected_bootstrap_score: Some(1200),
                top_selected_sync_score: Some(2400),
                average_selected_bootstrap_score: Some(1200),
                average_selected_sync_score: Some(2400),
                selected_bootstrap_peer_ids: vec![11],
                selected_sync_peer_ids: vec![11],
            },
            selection_long_term_summary: EthPeerSelectionLongTermSummaryV1 {
                chain_id,
                tracked_sync_peers: 1,
                peers_with_history: 1,
                peers_with_positive_contribution: 1,
                peers_currently_in_failure_streak: 0,
                peers_currently_in_progressless_streak: 0,
                observed_rounds_total: 8,
                selected_rounds_total: 4,
                selected_sync_rounds_total: 2,
                sync_contribution_rounds_total: 2,
                selected_without_progress_rounds_total: 0,
                connect_failure_rounds_total: 0,
                handshake_failure_rounds_total: 0,
                decode_failure_rounds_total: 0,
                timeout_failure_rounds_total: 0,
                validation_reject_rounds_total: 0,
                disconnect_rounds_total: 0,
                capacity_reject_rounds_total: 0,
                average_selection_hit_rate_bps: 5_000,
                average_header_success_rate_bps: 10_000,
                average_body_success_rate_bps: 10_000,
                average_long_term_score: Some(1_600),
                top_trusted_sync_peer_id: Some(11),
                top_trusted_sync_long_term_score: Some(1_600),
            },
            selection_window_policy: default_eth_peer_selection_window_policy_v1(),
            runtime_config: EthFullnodeNativeRuntimeConfigV1 {
                chain_id,
                budget_hooks: default_eth_fullnode_budget_hooks_v1(),
                selection_window_policy: default_eth_peer_selection_window_policy_v1(),
            },
            peer_selection_scores: vec![
                EthPeerSelectionScoreV1 {
                    chain_id,
                    peer_id: 11,
                    role: EthPeerSelectionRoleV1::Bootstrap,
                    stage: EthPeerLifecycleStageV1::Ready,
                    eligible: false,
                    selected: true,
                    score: 1200,
                    reasons: vec!["prior_successful_sessions=1".to_string()],
                    last_head_height: 128,
                    successful_sessions: 1,
                    header_response_count: 1,
                    body_response_count: 1,
                    sync_contribution_count: 2,
                    consecutive_failures: 0,
                    last_success_unix_ms: 2,
                    last_failure_unix_ms: 0,
                    cooldown_until_unix_ms: 0,
                    permanently_rejected: false,
                    long_term_score: 800,
                    recent_window: EthPeerRecentWindowStatsV1 {
                        window_rounds: 2,
                        selected_rounds: 2,
                        selected_bootstrap_rounds: 1,
                        selected_sync_rounds: 1,
                        header_success_rounds: 1,
                        body_success_rounds: 1,
                        sync_contribution_rounds: 1,
                        selected_without_progress_rounds: 0,
                        connect_failure_rounds: 0,
                        handshake_failure_rounds: 0,
                        decode_failure_rounds: 0,
                        timeout_failure_rounds: 0,
                        validation_reject_rounds: 0,
                        disconnect_rounds: 0,
                        capacity_reject_rounds: 0,
                        last_selected_unix_ms: 2,
                        last_progress_unix_ms: 2,
                        last_failure_unix_ms: 0,
                        selection_hit_rate_bps: 5_000,
                        header_success_rate_bps: 10_000,
                        body_success_rate_bps: 10_000,
                    },
                    long_term: EthPeerLongTermStatsV1 {
                        total_observed_rounds: 8,
                        total_selected_rounds: 4,
                        total_selected_bootstrap_rounds: 2,
                        total_selected_sync_rounds: 2,
                        total_header_success_rounds: 2,
                        total_body_success_rounds: 2,
                        total_sync_contribution_rounds: 2,
                        total_selected_without_progress_rounds: 0,
                        total_connect_failure_rounds: 0,
                        total_handshake_failure_rounds: 0,
                        total_decode_failure_rounds: 0,
                        total_timeout_failure_rounds: 0,
                        total_validation_reject_rounds: 0,
                        total_disconnect_rounds: 0,
                        total_capacity_reject_rounds: 0,
                        current_consecutive_connect_failures: 0,
                        current_consecutive_handshake_failures: 0,
                        current_consecutive_decode_failures: 0,
                        current_consecutive_timeout_failures: 0,
                        current_consecutive_validation_rejects: 0,
                        current_consecutive_disconnects: 0,
                        current_consecutive_selected_without_progress_rounds: 0,
                        max_consecutive_connect_failures: 0,
                        max_consecutive_handshake_failures: 0,
                        max_consecutive_decode_failures: 0,
                        max_consecutive_timeout_failures: 0,
                        max_consecutive_validation_rejects: 0,
                        max_consecutive_disconnects: 0,
                        max_consecutive_selected_without_progress_rounds: 0,
                        last_selected_unix_ms: 2,
                        last_progress_unix_ms: 2,
                        last_failure_unix_ms: 0,
                        selection_hit_rate_bps: 5_000,
                        header_success_rate_bps: 10_000,
                        body_success_rate_bps: 10_000,
                        ..EthPeerLongTermStatsV1::default()
                    },
                    window_layers: EthPeerSelectionWindowLayersV1::default(),
                },
                EthPeerSelectionScoreV1 {
                    chain_id,
                    peer_id: 11,
                    role: EthPeerSelectionRoleV1::Sync,
                    stage: EthPeerLifecycleStageV1::Ready,
                    eligible: true,
                    selected: true,
                    score: 2400,
                    reasons: vec!["eligible_ready_session".to_string()],
                    last_head_height: 128,
                    successful_sessions: 1,
                    header_response_count: 1,
                    body_response_count: 1,
                    sync_contribution_count: 2,
                    consecutive_failures: 0,
                    last_success_unix_ms: 2,
                    last_failure_unix_ms: 0,
                    cooldown_until_unix_ms: 0,
                    permanently_rejected: false,
                    long_term_score: 1_600,
                    recent_window: EthPeerRecentWindowStatsV1 {
                        window_rounds: 2,
                        selected_rounds: 2,
                        selected_bootstrap_rounds: 1,
                        selected_sync_rounds: 1,
                        header_success_rounds: 1,
                        body_success_rounds: 1,
                        sync_contribution_rounds: 1,
                        selected_without_progress_rounds: 0,
                        connect_failure_rounds: 0,
                        handshake_failure_rounds: 0,
                        decode_failure_rounds: 0,
                        timeout_failure_rounds: 0,
                        validation_reject_rounds: 0,
                        disconnect_rounds: 0,
                        capacity_reject_rounds: 0,
                        last_selected_unix_ms: 2,
                        last_progress_unix_ms: 2,
                        last_failure_unix_ms: 0,
                        selection_hit_rate_bps: 5_000,
                        header_success_rate_bps: 10_000,
                        body_success_rate_bps: 10_000,
                    },
                    long_term: EthPeerLongTermStatsV1 {
                        total_observed_rounds: 8,
                        total_selected_rounds: 4,
                        total_selected_bootstrap_rounds: 2,
                        total_selected_sync_rounds: 2,
                        total_header_success_rounds: 2,
                        total_body_success_rounds: 2,
                        total_sync_contribution_rounds: 2,
                        total_selected_without_progress_rounds: 0,
                        total_connect_failure_rounds: 0,
                        total_handshake_failure_rounds: 0,
                        total_decode_failure_rounds: 0,
                        total_timeout_failure_rounds: 0,
                        total_validation_reject_rounds: 0,
                        total_disconnect_rounds: 0,
                        total_capacity_reject_rounds: 0,
                        current_consecutive_connect_failures: 0,
                        current_consecutive_handshake_failures: 0,
                        current_consecutive_decode_failures: 0,
                        current_consecutive_timeout_failures: 0,
                        current_consecutive_validation_rejects: 0,
                        current_consecutive_disconnects: 0,
                        current_consecutive_selected_without_progress_rounds: 0,
                        max_consecutive_connect_failures: 0,
                        max_consecutive_handshake_failures: 0,
                        max_consecutive_decode_failures: 0,
                        max_consecutive_timeout_failures: 0,
                        max_consecutive_validation_rejects: 0,
                        max_consecutive_disconnects: 0,
                        max_consecutive_selected_without_progress_rounds: 0,
                        last_selected_unix_ms: 2,
                        last_progress_unix_ms: 2,
                        last_failure_unix_ms: 0,
                        selection_hit_rate_bps: 5_000,
                        header_success_rate_bps: 10_000,
                        body_success_rate_bps: 10_000,
                        ..EthPeerLongTermStatsV1::default()
                    },
                    window_layers: EthPeerSelectionWindowLayersV1::default(),
                },
            ],
            peer_sessions: vec![EthPeerSessionSnapshot {
                chain_id,
                peer_id: 11,
                negotiated: default_eth_peer_negotiated_capabilities_v1(),
                lifecycle_stage: EthPeerLifecycleStageV1::Ready,
                retry_eligible: true,
                session_ready: true,
                last_head_height: 128,
                successful_sessions: 1,
                header_response_count: 1,
                body_response_count: 1,
                sync_contribution_count: 2,
                connect_failure_count: 0,
                handshake_failure_count: 0,
                decode_failure_count: 0,
                timeout_count: 0,
                validation_reject_count: 0,
                last_validation_reject_reason: None,
                disconnect_count: 0,
                disconnect_too_many_peers_count: 0,
                last_disconnect_reason_code: None,
                last_failure_class: None,
                last_failure_reason_code: None,
                last_failure_reason_name: None,
                consecutive_failures: 0,
                first_seen_unix_ms: 1,
                first_failure_unix_ms: 0,
                cooldown_until_unix_ms: 0,
                permanently_rejected: false,
                last_success_unix_ms: 2,
                last_header_success_unix_ms: 2,
                last_body_success_unix_ms: 2,
                last_failure_unix_ms: 0,
                last_state_change_unix_ms: 2,
                recent_window: EthPeerRecentWindowStatsV1 {
                    window_rounds: 2,
                    selected_rounds: 2,
                    selected_bootstrap_rounds: 1,
                    selected_sync_rounds: 1,
                    header_success_rounds: 1,
                    body_success_rounds: 1,
                    sync_contribution_rounds: 1,
                    selected_without_progress_rounds: 0,
                    connect_failure_rounds: 0,
                    handshake_failure_rounds: 0,
                    decode_failure_rounds: 0,
                    timeout_failure_rounds: 0,
                    validation_reject_rounds: 0,
                    disconnect_rounds: 0,
                    capacity_reject_rounds: 0,
                    last_selected_unix_ms: 2,
                    last_progress_unix_ms: 2,
                    last_failure_unix_ms: 0,
                    selection_hit_rate_bps: 5_000,
                    header_success_rate_bps: 10_000,
                    body_success_rate_bps: 10_000,
                },
                long_term: EthPeerLongTermStatsV1 {
                    total_observed_rounds: 8,
                    total_selected_rounds: 4,
                    total_selected_bootstrap_rounds: 2,
                    total_selected_sync_rounds: 2,
                    total_header_success_rounds: 2,
                    total_body_success_rounds: 2,
                    total_sync_contribution_rounds: 2,
                    total_selected_without_progress_rounds: 0,
                    total_connect_failure_rounds: 0,
                    total_handshake_failure_rounds: 0,
                    total_decode_failure_rounds: 0,
                    total_timeout_failure_rounds: 0,
                    total_validation_reject_rounds: 0,
                    total_disconnect_rounds: 0,
                    total_capacity_reject_rounds: 0,
                    current_consecutive_connect_failures: 0,
                    current_consecutive_handshake_failures: 0,
                    current_consecutive_decode_failures: 0,
                    current_consecutive_timeout_failures: 0,
                    current_consecutive_validation_rejects: 0,
                    current_consecutive_disconnects: 0,
                    current_consecutive_selected_without_progress_rounds: 0,
                    max_consecutive_connect_failures: 0,
                    max_consecutive_handshake_failures: 0,
                    max_consecutive_decode_failures: 0,
                    max_consecutive_timeout_failures: 0,
                    max_consecutive_validation_rejects: 0,
                    max_consecutive_disconnects: 0,
                    max_consecutive_selected_without_progress_rounds: 0,
                    last_selected_unix_ms: 2,
                    last_progress_unix_ms: 2,
                    last_failure_unix_ms: 0,
                    selection_hit_rate_bps: 5_000,
                    header_success_rate_bps: 10_000,
                    body_success_rate_bps: 10_000,
                    ..EthPeerLongTermStatsV1::default()
                },
                window_layers: EthPeerSelectionWindowLayersV1::default(),
            }],
            peer_failures: vec![EthFullnodeNativePeerFailureSnapshotV1 {
                peer_id: 12,
                endpoint: Some("127.0.0.1:30303".to_string()),
                phase: "bootstrap".to_string(),
                class: "connect_failure".to_string(),
                lifecycle_class: Some("connect_failure".to_string()),
                reason_code: None,
                reason_name: Some("connection_refused".to_string()),
                error: "connection_refused".to_string(),
            }],
        };
        set_eth_fullnode_native_worker_runtime_snapshot_v1(chain_id, snapshot.clone());
        let in_memory = snapshot_eth_fullnode_native_worker_runtime_snapshot_v1(chain_id)
            .expect("in-memory snapshot");
        assert_eq!(in_memory, snapshot);

        let path = std::env::temp_dir().join(format!(
            "supervm-eth-native-runtime-{chain_id}-{}.json",
            eth_peer_now_unix_ms_v1()
        ));
        write_eth_fullnode_native_worker_runtime_snapshot_to_path_v1(path.as_path(), &snapshot)
            .expect("write runtime snapshot");
        let loaded = load_eth_fullnode_native_worker_runtime_snapshot_from_path_v1(path.as_path())
            .expect("load runtime snapshot");
        assert_eq!(loaded, snapshot);
        let _ = fs::remove_file(path);
        clear_eth_fullnode_native_worker_runtime_snapshot_for_chain_v1(chain_id);
    }
}
