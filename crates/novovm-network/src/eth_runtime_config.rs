#![forbid(unsafe_code)]

use crate::eth_selection_config::{
    apply_eth_peer_selection_window_policy_lookup_v1, normalize_eth_peer_selection_window_policy_v1,
};
use crate::{
    default_eth_fullnode_budget_hooks_v1, default_eth_peer_selection_window_policy_v1,
    EthFullnodeBudgetHooksV1, EthPeerSelectionWindowPolicyV1,
};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::fs;
use std::io::{Error as IoError, ErrorKind as IoErrorKind};
use std::path::{Path, PathBuf};

pub const ETH_FULLNODE_NATIVE_RUNTIME_CONFIG_PATH_ENV_V1: &str =
    "NOVOVM_NETWORK_ETH_RUNTIME_CONFIG_PATH";
pub const ETH_FULLNODE_NATIVE_RUNTIME_CONFIG_DEFAULT_PATH_V1: &str =
    "config/novovm-eth-native-runtime-config.json";

#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct EthFullnodeNativeRuntimeConfigV1 {
    pub chain_id: u64,
    pub budget_hooks: EthFullnodeBudgetHooksV1,
    pub selection_window_policy: EthPeerSelectionWindowPolicyV1,
}

#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct EthFullnodeBudgetHooksPatchV1 {
    pub native_recv_budget_per_tick: Option<u64>,
    pub sync_target_fanout: Option<u64>,
    pub rlpx_request_timeout_ms: Option<u64>,
    pub sync_request_interval_ms: Option<u64>,
    pub tx_broadcast_interval_ms: Option<u64>,
    pub tx_broadcast_max_per_tick: Option<u64>,
    pub tx_broadcast_max_propagations: Option<u64>,
    pub runtime_query_result_max: Option<u64>,
    pub runtime_block_snapshot_limit: Option<u64>,
    pub runtime_pending_tx_snapshot_limit: Option<u64>,
    pub pending_tx_canonical_retain_depth: Option<u64>,
    pub pending_tx_reorg_return_window_ms: Option<u64>,
    pub pending_tx_ttl_ms: Option<u64>,
    pub pending_tx_no_success_attempt_limit: Option<u64>,
    pub pending_tx_tombstone_retention_max: Option<u64>,
    pub host_exec_budget_per_tick: Option<u64>,
    pub host_exec_time_slice_ms: Option<u64>,
    pub host_exec_target_per_tick: Option<u64>,
    pub host_exec_target_time_slice_ms: Option<u64>,
    pub sync_pull_headers_batch: Option<u64>,
    pub sync_pull_bodies_batch: Option<u64>,
    pub sync_pull_state_batch: Option<u64>,
    pub sync_pull_finalize_batch: Option<u64>,
    pub sync_decode_concurrency: Option<u64>,
    pub sync_apply_concurrency: Option<u64>,
    pub native_block_store_flush_batch: Option<u64>,
    pub block_query_scan_max: Option<u64>,
    pub active_native_peer_soft_limit: Option<u64>,
    pub active_native_peer_hard_limit: Option<u64>,
}

#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct EthPeerSelectionWindowPolicyPatchV1 {
    pub short_term_rounds: Option<u64>,
    pub medium_term_rounds: Option<u64>,
    pub long_term_rounds: Option<u64>,
    pub sync_short_term_weight_bps: Option<u64>,
    pub sync_medium_term_weight_bps: Option<u64>,
    pub sync_long_term_weight_bps: Option<u64>,
    pub bootstrap_short_term_weight_bps: Option<u64>,
    pub bootstrap_medium_term_weight_bps: Option<u64>,
    pub bootstrap_long_term_weight_bps: Option<u64>,
    pub medium_term_selection_hit_rate_floor_bps: Option<u64>,
    pub long_term_selection_hit_rate_floor_bps: Option<u64>,
    pub long_term_body_success_rate_floor_bps: Option<u64>,
}

#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct EthFullnodeNativeRuntimeConfigPatchV1 {
    #[serde(default)]
    pub budget_hooks: Option<EthFullnodeBudgetHooksPatchV1>,
    #[serde(default)]
    pub selection_window_policy: Option<EthPeerSelectionWindowPolicyPatchV1>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct EthFullnodeNativeRuntimeConfigFileV1 {
    #[serde(default = "default_eth_fullnode_native_runtime_config_file_version_v1")]
    pub version: u32,
    #[serde(default)]
    pub default: Option<EthFullnodeNativeRuntimeConfigPatchV1>,
    #[serde(default)]
    pub chains: BTreeMap<String, EthFullnodeNativeRuntimeConfigPatchV1>,
}

impl Default for EthFullnodeNativeRuntimeConfigFileV1 {
    fn default() -> Self {
        Self {
            version: default_eth_fullnode_native_runtime_config_file_version_v1(),
            default: None,
            chains: BTreeMap::new(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct EthFullnodeNativeRuntimeConfigSourceV1 {
    pub config_path: String,
    pub config_file_found: bool,
    #[serde(default)]
    pub config_parse_error: Option<String>,
    pub file_default_applied: bool,
    pub file_chain_override_applied: bool,
    #[serde(default)]
    pub env_budget_override_keys: Vec<String>,
    #[serde(default)]
    pub env_selection_override_keys: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct EthFullnodeNativeRuntimeConfigResolutionV1 {
    pub config: EthFullnodeNativeRuntimeConfigV1,
    pub source: EthFullnodeNativeRuntimeConfigSourceV1,
}

fn default_eth_fullnode_native_runtime_config_file_version_v1() -> u32 {
    1
}

fn eth_runtime_config_env_keys_v1(chain_id: u64, suffix: &str) -> [String; 2] {
    [
        format!("NOVOVM_NETWORK_ETH_RUNTIME_CHAIN_{chain_id}_{suffix}"),
        format!("NOVOVM_NETWORK_ETH_RUNTIME_{suffix}"),
    ]
}

fn eth_runtime_config_lookup_u64_v1(
    chain_id: u64,
    suffix: &str,
    lookup: &impl Fn(&str) -> Option<String>,
) -> Option<(u64, String)> {
    for key in eth_runtime_config_env_keys_v1(chain_id, suffix) {
        if let Some(raw) = lookup(key.as_str()) {
            if let Ok(parsed) = raw.trim().parse::<u64>() {
                return Some((parsed, key));
            }
        }
    }
    None
}

fn normalize_eth_fullnode_budget_hooks_v1(budget: &mut EthFullnodeBudgetHooksV1) {
    budget.native_recv_budget_per_tick = budget.native_recv_budget_per_tick.max(1);
    budget.sync_target_fanout = budget.sync_target_fanout.max(1);
    budget.rlpx_request_timeout_ms = budget.rlpx_request_timeout_ms.max(1);
    budget.sync_request_interval_ms = budget.sync_request_interval_ms.max(1);
    budget.tx_broadcast_interval_ms = budget.tx_broadcast_interval_ms.max(1);
    budget.tx_broadcast_max_per_tick = budget.tx_broadcast_max_per_tick.max(1);
    budget.tx_broadcast_max_propagations = budget.tx_broadcast_max_propagations.max(1);
    budget.runtime_query_result_max = budget.runtime_query_result_max.max(1);
    budget.runtime_block_snapshot_limit = budget.runtime_block_snapshot_limit.max(1);
    budget.runtime_pending_tx_snapshot_limit = budget.runtime_pending_tx_snapshot_limit.max(1);
    budget.pending_tx_canonical_retain_depth = budget.pending_tx_canonical_retain_depth.max(1);
    budget.pending_tx_reorg_return_window_ms = budget.pending_tx_reorg_return_window_ms.max(1);
    budget.pending_tx_ttl_ms = budget.pending_tx_ttl_ms.max(1);
    budget.pending_tx_no_success_attempt_limit = budget.pending_tx_no_success_attempt_limit.max(1);
    budget.pending_tx_tombstone_retention_max = budget.pending_tx_tombstone_retention_max.max(1);
    budget.host_exec_budget_per_tick = budget.host_exec_budget_per_tick.max(1);
    budget.host_exec_time_slice_ms = budget.host_exec_time_slice_ms.max(1);
    budget.host_exec_target_per_tick = budget
        .host_exec_target_per_tick
        .max(1)
        .min(budget.host_exec_budget_per_tick);
    budget.host_exec_target_time_slice_ms = budget
        .host_exec_target_time_slice_ms
        .max(1)
        .min(budget.host_exec_time_slice_ms);
    budget.sync_pull_headers_batch = budget.sync_pull_headers_batch.max(1);
    budget.sync_pull_bodies_batch = budget.sync_pull_bodies_batch.max(1);
    budget.sync_pull_state_batch = budget.sync_pull_state_batch.max(1);
    budget.sync_pull_finalize_batch = budget.sync_pull_finalize_batch.max(1);
    budget.sync_decode_concurrency = budget.sync_decode_concurrency.max(1);
    budget.sync_apply_concurrency = budget.sync_apply_concurrency.max(1);
    budget.native_block_store_flush_batch = budget.native_block_store_flush_batch.max(1);
    budget.block_query_scan_max = budget.block_query_scan_max.max(1);
    budget.active_native_peer_soft_limit = budget.active_native_peer_soft_limit.max(1);
    budget.active_native_peer_hard_limit = budget
        .active_native_peer_hard_limit
        .max(budget.active_native_peer_soft_limit);
}

fn apply_eth_fullnode_budget_patch_v1(
    budget: &mut EthFullnodeBudgetHooksV1,
    patch: &EthFullnodeBudgetHooksPatchV1,
) {
    if let Some(value) = patch.native_recv_budget_per_tick {
        budget.native_recv_budget_per_tick = value;
    }
    if let Some(value) = patch.sync_target_fanout {
        budget.sync_target_fanout = value;
    }
    if let Some(value) = patch.rlpx_request_timeout_ms {
        budget.rlpx_request_timeout_ms = value;
    }
    if let Some(value) = patch.sync_request_interval_ms {
        budget.sync_request_interval_ms = value;
    }
    if let Some(value) = patch.tx_broadcast_interval_ms {
        budget.tx_broadcast_interval_ms = value;
    }
    if let Some(value) = patch.tx_broadcast_max_per_tick {
        budget.tx_broadcast_max_per_tick = value;
    }
    if let Some(value) = patch.tx_broadcast_max_propagations {
        budget.tx_broadcast_max_propagations = value;
    }
    if let Some(value) = patch.runtime_query_result_max {
        budget.runtime_query_result_max = value;
    }
    if let Some(value) = patch.runtime_block_snapshot_limit {
        budget.runtime_block_snapshot_limit = value;
    }
    if let Some(value) = patch.runtime_pending_tx_snapshot_limit {
        budget.runtime_pending_tx_snapshot_limit = value;
    }
    if let Some(value) = patch.pending_tx_canonical_retain_depth {
        budget.pending_tx_canonical_retain_depth = value;
    }
    if let Some(value) = patch.pending_tx_reorg_return_window_ms {
        budget.pending_tx_reorg_return_window_ms = value;
    }
    if let Some(value) = patch.pending_tx_ttl_ms {
        budget.pending_tx_ttl_ms = value;
    }
    if let Some(value) = patch.pending_tx_no_success_attempt_limit {
        budget.pending_tx_no_success_attempt_limit = value;
    }
    if let Some(value) = patch.pending_tx_tombstone_retention_max {
        budget.pending_tx_tombstone_retention_max = value;
    }
    if let Some(value) = patch.host_exec_budget_per_tick {
        budget.host_exec_budget_per_tick = value;
    }
    if let Some(value) = patch.host_exec_time_slice_ms {
        budget.host_exec_time_slice_ms = value;
    }
    if let Some(value) = patch.host_exec_target_per_tick {
        budget.host_exec_target_per_tick = value;
    }
    if let Some(value) = patch.host_exec_target_time_slice_ms {
        budget.host_exec_target_time_slice_ms = value;
    }
    if let Some(value) = patch.sync_pull_headers_batch {
        budget.sync_pull_headers_batch = value;
    }
    if let Some(value) = patch.sync_pull_bodies_batch {
        budget.sync_pull_bodies_batch = value;
    }
    if let Some(value) = patch.sync_pull_state_batch {
        budget.sync_pull_state_batch = value;
    }
    if let Some(value) = patch.sync_pull_finalize_batch {
        budget.sync_pull_finalize_batch = value;
    }
    if let Some(value) = patch.sync_decode_concurrency {
        budget.sync_decode_concurrency = value;
    }
    if let Some(value) = patch.sync_apply_concurrency {
        budget.sync_apply_concurrency = value;
    }
    if let Some(value) = patch.native_block_store_flush_batch {
        budget.native_block_store_flush_batch = value;
    }
    if let Some(value) = patch.block_query_scan_max {
        budget.block_query_scan_max = value;
    }
    if let Some(value) = patch.active_native_peer_soft_limit {
        budget.active_native_peer_soft_limit = value;
    }
    if let Some(value) = patch.active_native_peer_hard_limit {
        budget.active_native_peer_hard_limit = value;
    }
    normalize_eth_fullnode_budget_hooks_v1(budget);
}

fn apply_eth_peer_selection_window_policy_patch_v1(
    policy: &mut EthPeerSelectionWindowPolicyV1,
    patch: &EthPeerSelectionWindowPolicyPatchV1,
) {
    if let Some(value) = patch.short_term_rounds {
        policy.short_term_rounds = value.max(1);
    }
    if let Some(value) = patch.medium_term_rounds {
        policy.medium_term_rounds = value.max(1);
    }
    if let Some(value) = patch.long_term_rounds {
        policy.long_term_rounds = value.max(1);
    }
    if let Some(value) = patch.sync_short_term_weight_bps {
        policy.sync_short_term_weight_bps = value;
    }
    if let Some(value) = patch.sync_medium_term_weight_bps {
        policy.sync_medium_term_weight_bps = value;
    }
    if let Some(value) = patch.sync_long_term_weight_bps {
        policy.sync_long_term_weight_bps = value;
    }
    if let Some(value) = patch.bootstrap_short_term_weight_bps {
        policy.bootstrap_short_term_weight_bps = value;
    }
    if let Some(value) = patch.bootstrap_medium_term_weight_bps {
        policy.bootstrap_medium_term_weight_bps = value;
    }
    if let Some(value) = patch.bootstrap_long_term_weight_bps {
        policy.bootstrap_long_term_weight_bps = value;
    }
    if let Some(value) = patch.medium_term_selection_hit_rate_floor_bps {
        policy.medium_term_selection_hit_rate_floor_bps = value;
    }
    if let Some(value) = patch.long_term_selection_hit_rate_floor_bps {
        policy.long_term_selection_hit_rate_floor_bps = value;
    }
    if let Some(value) = patch.long_term_body_success_rate_floor_bps {
        policy.long_term_body_success_rate_floor_bps = value;
    }
    normalize_eth_peer_selection_window_policy_v1(policy);
}

fn apply_eth_fullnode_native_runtime_config_patch_v1(
    config: &mut EthFullnodeNativeRuntimeConfigV1,
    patch: &EthFullnodeNativeRuntimeConfigPatchV1,
) {
    if let Some(budget_patch) = patch.budget_hooks.as_ref() {
        apply_eth_fullnode_budget_patch_v1(&mut config.budget_hooks, budget_patch);
    }
    if let Some(selection_patch) = patch.selection_window_policy.as_ref() {
        apply_eth_peer_selection_window_policy_patch_v1(
            &mut config.selection_window_policy,
            selection_patch,
        );
    }
}

fn apply_eth_fullnode_budget_lookup_v1(
    chain_id: u64,
    budget: &mut EthFullnodeBudgetHooksV1,
    lookup: &impl Fn(&str) -> Option<String>,
) -> Vec<String> {
    let mut applied_keys = Vec::new();
    if let Some((value, key)) =
        eth_runtime_config_lookup_u64_v1(chain_id, "NATIVE_RECV_BUDGET_PER_TICK", lookup)
    {
        budget.native_recv_budget_per_tick = value;
        applied_keys.push(key);
    }
    if let Some((value, key)) =
        eth_runtime_config_lookup_u64_v1(chain_id, "SYNC_TARGET_FANOUT", lookup)
    {
        budget.sync_target_fanout = value;
        applied_keys.push(key);
    }
    if let Some((value, key)) =
        eth_runtime_config_lookup_u64_v1(chain_id, "RLPX_REQUEST_TIMEOUT_MS", lookup)
    {
        budget.rlpx_request_timeout_ms = value;
        applied_keys.push(key);
    }
    if let Some((value, key)) =
        eth_runtime_config_lookup_u64_v1(chain_id, "SYNC_REQUEST_INTERVAL_MS", lookup)
    {
        budget.sync_request_interval_ms = value;
        applied_keys.push(key);
    }
    if let Some((value, key)) =
        eth_runtime_config_lookup_u64_v1(chain_id, "TX_BROADCAST_INTERVAL_MS", lookup)
    {
        budget.tx_broadcast_interval_ms = value;
        applied_keys.push(key);
    }
    if let Some((value, key)) =
        eth_runtime_config_lookup_u64_v1(chain_id, "TX_BROADCAST_MAX_PER_TICK", lookup)
    {
        budget.tx_broadcast_max_per_tick = value;
        applied_keys.push(key);
    }
    if let Some((value, key)) =
        eth_runtime_config_lookup_u64_v1(chain_id, "TX_BROADCAST_MAX_PROPAGATIONS", lookup)
    {
        budget.tx_broadcast_max_propagations = value;
        applied_keys.push(key);
    }
    if let Some((value, key)) =
        eth_runtime_config_lookup_u64_v1(chain_id, "RUNTIME_QUERY_RESULT_MAX", lookup)
    {
        budget.runtime_query_result_max = value;
        applied_keys.push(key);
    }
    if let Some((value, key)) =
        eth_runtime_config_lookup_u64_v1(chain_id, "RUNTIME_BLOCK_SNAPSHOT_LIMIT", lookup)
    {
        budget.runtime_block_snapshot_limit = value;
        applied_keys.push(key);
    }
    if let Some((value, key)) =
        eth_runtime_config_lookup_u64_v1(chain_id, "RUNTIME_PENDING_TX_SNAPSHOT_LIMIT", lookup)
    {
        budget.runtime_pending_tx_snapshot_limit = value;
        applied_keys.push(key);
    }
    if let Some((value, key)) =
        eth_runtime_config_lookup_u64_v1(chain_id, "PENDING_TX_CANONICAL_RETAIN_DEPTH", lookup)
    {
        budget.pending_tx_canonical_retain_depth = value;
        applied_keys.push(key);
    }
    if let Some((value, key)) =
        eth_runtime_config_lookup_u64_v1(chain_id, "PENDING_TX_REORG_RETURN_WINDOW_MS", lookup)
    {
        budget.pending_tx_reorg_return_window_ms = value;
        applied_keys.push(key);
    }
    if let Some((value, key)) =
        eth_runtime_config_lookup_u64_v1(chain_id, "PENDING_TX_TTL_MS", lookup)
    {
        budget.pending_tx_ttl_ms = value;
        applied_keys.push(key);
    }
    if let Some((value, key)) =
        eth_runtime_config_lookup_u64_v1(chain_id, "PENDING_TX_NO_SUCCESS_ATTEMPT_LIMIT", lookup)
    {
        budget.pending_tx_no_success_attempt_limit = value;
        applied_keys.push(key);
    }
    if let Some((value, key)) =
        eth_runtime_config_lookup_u64_v1(chain_id, "PENDING_TX_TOMBSTONE_RETENTION_MAX", lookup)
    {
        budget.pending_tx_tombstone_retention_max = value;
        applied_keys.push(key);
    }
    if let Some((value, key)) =
        eth_runtime_config_lookup_u64_v1(chain_id, "HOST_EXEC_BUDGET_PER_TICK", lookup)
    {
        budget.host_exec_budget_per_tick = value;
        applied_keys.push(key);
    }
    if let Some((value, key)) =
        eth_runtime_config_lookup_u64_v1(chain_id, "HOST_EXEC_TIME_SLICE_MS", lookup)
    {
        budget.host_exec_time_slice_ms = value;
        applied_keys.push(key);
    }
    if let Some((value, key)) =
        eth_runtime_config_lookup_u64_v1(chain_id, "HOST_EXEC_TARGET_PER_TICK", lookup)
    {
        budget.host_exec_target_per_tick = value;
        applied_keys.push(key);
    }
    if let Some((value, key)) =
        eth_runtime_config_lookup_u64_v1(chain_id, "HOST_EXEC_TARGET_TIME_SLICE_MS", lookup)
    {
        budget.host_exec_target_time_slice_ms = value;
        applied_keys.push(key);
    }
    if let Some((value, key)) =
        eth_runtime_config_lookup_u64_v1(chain_id, "SYNC_PULL_HEADERS_BATCH", lookup)
    {
        budget.sync_pull_headers_batch = value;
        applied_keys.push(key);
    }
    if let Some((value, key)) =
        eth_runtime_config_lookup_u64_v1(chain_id, "SYNC_PULL_BODIES_BATCH", lookup)
    {
        budget.sync_pull_bodies_batch = value;
        applied_keys.push(key);
    }
    if let Some((value, key)) =
        eth_runtime_config_lookup_u64_v1(chain_id, "SYNC_PULL_STATE_BATCH", lookup)
    {
        budget.sync_pull_state_batch = value;
        applied_keys.push(key);
    }
    if let Some((value, key)) =
        eth_runtime_config_lookup_u64_v1(chain_id, "SYNC_PULL_FINALIZE_BATCH", lookup)
    {
        budget.sync_pull_finalize_batch = value;
        applied_keys.push(key);
    }
    if let Some((value, key)) =
        eth_runtime_config_lookup_u64_v1(chain_id, "SYNC_DECODE_CONCURRENCY", lookup)
    {
        budget.sync_decode_concurrency = value;
        applied_keys.push(key);
    }
    if let Some((value, key)) =
        eth_runtime_config_lookup_u64_v1(chain_id, "SYNC_APPLY_CONCURRENCY", lookup)
    {
        budget.sync_apply_concurrency = value;
        applied_keys.push(key);
    }
    if let Some((value, key)) =
        eth_runtime_config_lookup_u64_v1(chain_id, "NATIVE_BLOCK_STORE_FLUSH_BATCH", lookup)
    {
        budget.native_block_store_flush_batch = value;
        applied_keys.push(key);
    }
    if let Some((value, key)) =
        eth_runtime_config_lookup_u64_v1(chain_id, "BLOCK_QUERY_SCAN_MAX", lookup)
    {
        budget.block_query_scan_max = value;
        applied_keys.push(key);
    }
    if let Some((value, key)) =
        eth_runtime_config_lookup_u64_v1(chain_id, "ACTIVE_NATIVE_PEER_SOFT_LIMIT", lookup)
    {
        budget.active_native_peer_soft_limit = value;
        applied_keys.push(key);
    }
    if let Some((value, key)) =
        eth_runtime_config_lookup_u64_v1(chain_id, "ACTIVE_NATIVE_PEER_HARD_LIMIT", lookup)
    {
        budget.active_native_peer_hard_limit = value;
        applied_keys.push(key);
    }
    normalize_eth_fullnode_budget_hooks_v1(budget);
    applied_keys
}

#[must_use]
pub fn default_eth_fullnode_native_runtime_config_path_v1() -> PathBuf {
    std::env::var(ETH_FULLNODE_NATIVE_RUNTIME_CONFIG_PATH_ENV_V1)
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
            workspace_root.join(ETH_FULLNODE_NATIVE_RUNTIME_CONFIG_DEFAULT_PATH_V1)
        })
}

pub fn load_eth_fullnode_native_runtime_config_file_from_path_v1(
    path: &Path,
) -> std::io::Result<EthFullnodeNativeRuntimeConfigFileV1> {
    let bytes = fs::read(path)?;
    serde_json::from_slice(bytes.as_slice())
        .map_err(|err| IoError::new(IoErrorKind::InvalidData, err.to_string()))
}

fn resolve_eth_fullnode_native_runtime_config_resolution_with_lookup_v1(
    chain_id: u64,
    config_path: &Path,
    lookup: &impl Fn(&str) -> Option<String>,
) -> EthFullnodeNativeRuntimeConfigResolutionV1 {
    let mut resolution = EthFullnodeNativeRuntimeConfigResolutionV1 {
        config: EthFullnodeNativeRuntimeConfigV1 {
            chain_id,
            budget_hooks: default_eth_fullnode_budget_hooks_v1(),
            selection_window_policy: default_eth_peer_selection_window_policy_v1(),
        },
        source: EthFullnodeNativeRuntimeConfigSourceV1 {
            config_path: config_path.to_string_lossy().to_string(),
            ..EthFullnodeNativeRuntimeConfigSourceV1::default()
        },
    };

    match load_eth_fullnode_native_runtime_config_file_from_path_v1(config_path) {
        Ok(file) => {
            resolution.source.config_file_found = true;
            if let Some(default_patch) = file.default.as_ref() {
                apply_eth_fullnode_native_runtime_config_patch_v1(
                    &mut resolution.config,
                    default_patch,
                );
                resolution.source.file_default_applied = true;
            }
            if let Some(chain_patch) = file.chains.get(chain_id.to_string().as_str()) {
                apply_eth_fullnode_native_runtime_config_patch_v1(
                    &mut resolution.config,
                    chain_patch,
                );
                resolution.source.file_chain_override_applied = true;
            }
        }
        Err(err) if err.kind() == IoErrorKind::NotFound => {}
        Err(err) => {
            resolution.source.config_parse_error = Some(err.to_string());
        }
    }

    resolution.source.env_budget_override_keys =
        apply_eth_fullnode_budget_lookup_v1(chain_id, &mut resolution.config.budget_hooks, lookup);
    resolution.source.env_selection_override_keys =
        apply_eth_peer_selection_window_policy_lookup_v1(
            chain_id,
            &mut resolution.config.selection_window_policy,
            lookup,
        );
    resolution
}

#[must_use]
pub fn resolve_eth_fullnode_native_runtime_config_resolution_v1(
    chain_id: u64,
) -> EthFullnodeNativeRuntimeConfigResolutionV1 {
    let path = default_eth_fullnode_native_runtime_config_path_v1();
    resolve_eth_fullnode_native_runtime_config_resolution_with_lookup_v1(
        chain_id,
        path.as_path(),
        &|name| std::env::var(name).ok(),
    )
}

#[must_use]
pub fn resolve_eth_fullnode_budget_hooks_v1(chain_id: u64) -> EthFullnodeBudgetHooksV1 {
    resolve_eth_fullnode_native_runtime_config_v1(chain_id).budget_hooks
}

#[must_use]
pub fn resolve_eth_fullnode_native_runtime_config_v1(
    chain_id: u64,
) -> EthFullnodeNativeRuntimeConfigV1 {
    resolve_eth_fullnode_native_runtime_config_resolution_v1(chain_id).config
}

#[cfg(test)]
pub(crate) fn tests_only_resolve_eth_fullnode_native_runtime_config_resolution_with_lookup_v1(
    chain_id: u64,
    config_path: &Path,
    lookup: &impl Fn(&str) -> Option<String>,
) -> EthFullnodeNativeRuntimeConfigResolutionV1 {
    resolve_eth_fullnode_native_runtime_config_resolution_with_lookup_v1(
        chain_id,
        config_path,
        lookup,
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;
    use std::time::{SystemTime, UNIX_EPOCH};

    fn temp_config_path(name: &str) -> PathBuf {
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|value| value.as_nanos())
            .unwrap_or_default();
        std::env::temp_dir().join(format!(
            "novovm-eth-runtime-config-{name}-{}-{nanos}.json",
            std::process::id()
        ))
    }

    #[test]
    fn resolve_eth_fullnode_budget_hooks_defaults_are_pinned() {
        let path = temp_config_path("defaults");
        let resolution =
            tests_only_resolve_eth_fullnode_native_runtime_config_resolution_with_lookup_v1(
                1,
                path.as_path(),
                &|_| None,
            );
        assert_eq!(
            resolution.config.budget_hooks.native_recv_budget_per_tick,
            128
        );
        assert_eq!(resolution.config.budget_hooks.sync_target_fanout, 1);
        assert_eq!(
            resolution.config.budget_hooks.rlpx_request_timeout_ms,
            5_000
        );
        assert_eq!(
            resolution.config.budget_hooks.sync_request_interval_ms,
            1_000
        );
        assert_eq!(
            resolution.config.budget_hooks.tx_broadcast_interval_ms,
            1_000
        );
        assert_eq!(resolution.config.budget_hooks.tx_broadcast_max_per_tick, 8);
        assert_eq!(
            resolution.config.budget_hooks.tx_broadcast_max_propagations,
            3
        );
        assert_eq!(
            resolution.config.budget_hooks.runtime_query_result_max,
            2_048
        );
        assert_eq!(
            resolution.config.budget_hooks.runtime_block_snapshot_limit,
            1_024
        );
        assert_eq!(
            resolution
                .config
                .budget_hooks
                .runtime_pending_tx_snapshot_limit,
            2_048
        );
        assert_eq!(
            resolution
                .config
                .budget_hooks
                .pending_tx_canonical_retain_depth,
            128
        );
        assert_eq!(
            resolution
                .config
                .budget_hooks
                .pending_tx_reorg_return_window_ms,
            900_000
        );
        assert_eq!(resolution.config.budget_hooks.pending_tx_ttl_ms, 1_800_000);
        assert_eq!(
            resolution
                .config
                .budget_hooks
                .pending_tx_no_success_attempt_limit,
            12
        );
        assert_eq!(
            resolution
                .config
                .budget_hooks
                .pending_tx_tombstone_retention_max,
            4_096
        );
        assert_eq!(resolution.config.budget_hooks.host_exec_budget_per_tick, 64);
        assert_eq!(resolution.config.budget_hooks.host_exec_time_slice_ms, 10);
        assert_eq!(resolution.config.budget_hooks.host_exec_target_per_tick, 64);
        assert_eq!(
            resolution
                .config
                .budget_hooks
                .host_exec_target_time_slice_ms,
            10
        );
        assert_eq!(
            resolution.config.budget_hooks.sync_pull_headers_batch,
            2_048
        );
        assert_eq!(resolution.config.budget_hooks.sync_pull_bodies_batch, 256);
        assert_eq!(resolution.config.budget_hooks.sync_pull_state_batch, 64);
        assert_eq!(resolution.config.budget_hooks.sync_pull_finalize_batch, 16);
        assert_eq!(resolution.config.budget_hooks.sync_decode_concurrency, 1);
        assert_eq!(resolution.config.budget_hooks.sync_apply_concurrency, 1);
        assert_eq!(
            resolution
                .config
                .budget_hooks
                .native_block_store_flush_batch,
            64
        );
        assert_eq!(resolution.config.budget_hooks.block_query_scan_max, 2_048);
        assert_eq!(
            resolution.config.budget_hooks.active_native_peer_soft_limit,
            8
        );
        assert_eq!(
            resolution.config.budget_hooks.active_native_peer_hard_limit,
            16
        );
        assert!(!resolution.source.config_file_found);
    }

    #[test]
    fn resolve_eth_fullnode_budget_hooks_honors_chain_specific_overrides() {
        let path = temp_config_path("env");
        let mut env = HashMap::new();
        env.insert(
            "NOVOVM_NETWORK_ETH_RUNTIME_SYNC_PULL_HEADERS_BATCH".to_string(),
            "777".to_string(),
        );
        env.insert(
            "NOVOVM_NETWORK_ETH_RUNTIME_CHAIN_1_SYNC_PULL_HEADERS_BATCH".to_string(),
            "123".to_string(),
        );
        env.insert(
            "NOVOVM_NETWORK_ETH_RUNTIME_ACTIVE_NATIVE_PEER_SOFT_LIMIT".to_string(),
            "5".to_string(),
        );
        env.insert(
            "NOVOVM_NETWORK_ETH_RUNTIME_ACTIVE_NATIVE_PEER_HARD_LIMIT".to_string(),
            "4".to_string(),
        );
        env.insert(
            "NOVOVM_NETWORK_ETH_RUNTIME_CHAIN_1_TX_BROADCAST_MAX_PER_TICK".to_string(),
            "11".to_string(),
        );
        env.insert(
            "NOVOVM_NETWORK_ETH_RUNTIME_CHAIN_1_HOST_EXEC_BUDGET_PER_TICK".to_string(),
            "40".to_string(),
        );
        env.insert(
            "NOVOVM_NETWORK_ETH_RUNTIME_CHAIN_1_HOST_EXEC_TARGET_PER_TICK".to_string(),
            "128".to_string(),
        );
        env.insert(
            "NOVOVM_NETWORK_ETH_RUNTIME_CHAIN_1_HOST_EXEC_TIME_SLICE_MS".to_string(),
            "20".to_string(),
        );
        env.insert(
            "NOVOVM_NETWORK_ETH_RUNTIME_CHAIN_1_HOST_EXEC_TARGET_TIME_SLICE_MS".to_string(),
            "64".to_string(),
        );
        let resolution =
            tests_only_resolve_eth_fullnode_native_runtime_config_resolution_with_lookup_v1(
                1,
                path.as_path(),
                &|name| env.get(name).cloned(),
            );
        assert_eq!(resolution.config.budget_hooks.sync_pull_headers_batch, 123);
        assert_eq!(
            resolution.config.budget_hooks.active_native_peer_soft_limit,
            5
        );
        assert_eq!(
            resolution.config.budget_hooks.active_native_peer_hard_limit,
            5
        );
        assert_eq!(resolution.config.budget_hooks.tx_broadcast_max_per_tick, 11);
        assert_eq!(resolution.config.budget_hooks.host_exec_budget_per_tick, 40);
        assert_eq!(resolution.config.budget_hooks.host_exec_target_per_tick, 40);
        assert_eq!(resolution.config.budget_hooks.host_exec_time_slice_ms, 20);
        assert_eq!(
            resolution
                .config
                .budget_hooks
                .host_exec_target_time_slice_ms,
            20
        );
        assert_eq!(resolution.source.env_budget_override_keys.len(), 8);
    }

    #[test]
    fn resolve_eth_fullnode_native_runtime_config_honors_file_defaults_and_chain_patch() {
        let path = temp_config_path("file");
        let file = EthFullnodeNativeRuntimeConfigFileV1 {
            version: 1,
            default: Some(EthFullnodeNativeRuntimeConfigPatchV1 {
                budget_hooks: Some(EthFullnodeBudgetHooksPatchV1 {
                    active_native_peer_soft_limit: Some(3),
                    ..EthFullnodeBudgetHooksPatchV1::default()
                }),
                selection_window_policy: Some(EthPeerSelectionWindowPolicyPatchV1 {
                    medium_term_rounds: Some(48),
                    ..EthPeerSelectionWindowPolicyPatchV1::default()
                }),
            }),
            chains: BTreeMap::from([(
                "1".to_string(),
                EthFullnodeNativeRuntimeConfigPatchV1 {
                    budget_hooks: Some(EthFullnodeBudgetHooksPatchV1 {
                        active_native_peer_hard_limit: Some(9),
                        ..EthFullnodeBudgetHooksPatchV1::default()
                    }),
                    selection_window_policy: Some(EthPeerSelectionWindowPolicyPatchV1 {
                        long_term_rounds: Some(384),
                        ..EthPeerSelectionWindowPolicyPatchV1::default()
                    }),
                },
            )]),
        };
        fs::write(
            path.as_path(),
            serde_json::to_vec_pretty(&file).expect("encode runtime config file"),
        )
        .expect("write runtime config file");
        let resolution =
            tests_only_resolve_eth_fullnode_native_runtime_config_resolution_with_lookup_v1(
                1,
                path.as_path(),
                &|_| None,
            );
        assert!(resolution.source.config_file_found);
        assert!(resolution.source.file_default_applied);
        assert!(resolution.source.file_chain_override_applied);
        assert_eq!(
            resolution.config.budget_hooks.active_native_peer_soft_limit,
            3
        );
        assert_eq!(
            resolution.config.budget_hooks.active_native_peer_hard_limit,
            9
        );
        assert_eq!(
            resolution.config.selection_window_policy.medium_term_rounds,
            48
        );
        assert_eq!(
            resolution.config.selection_window_policy.long_term_rounds,
            384
        );
        let _ = fs::remove_file(path);
    }

    #[test]
    fn resolve_eth_fullnode_native_runtime_config_env_overrides_file() {
        let path = temp_config_path("overlay");
        let file = EthFullnodeNativeRuntimeConfigFileV1 {
            version: 1,
            default: Some(EthFullnodeNativeRuntimeConfigPatchV1 {
                budget_hooks: Some(EthFullnodeBudgetHooksPatchV1 {
                    active_native_peer_soft_limit: Some(4),
                    ..EthFullnodeBudgetHooksPatchV1::default()
                }),
                selection_window_policy: Some(EthPeerSelectionWindowPolicyPatchV1 {
                    long_term_rounds: Some(320),
                    ..EthPeerSelectionWindowPolicyPatchV1::default()
                }),
            }),
            chains: BTreeMap::new(),
        };
        fs::write(
            path.as_path(),
            serde_json::to_vec_pretty(&file).expect("encode runtime config file"),
        )
        .expect("write runtime config file");
        let mut env = HashMap::new();
        env.insert(
            "NOVOVM_NETWORK_ETH_RUNTIME_CHAIN_1_ACTIVE_NATIVE_PEER_SOFT_LIMIT".to_string(),
            "6".to_string(),
        );
        env.insert(
            "NOVOVM_NETWORK_ETH_SELECTION_CHAIN_1_LONG_TERM_ROUNDS".to_string(),
            "512".to_string(),
        );
        let resolution =
            tests_only_resolve_eth_fullnode_native_runtime_config_resolution_with_lookup_v1(
                1,
                path.as_path(),
                &|name| env.get(name).cloned(),
            );
        assert_eq!(
            resolution.config.budget_hooks.active_native_peer_soft_limit,
            6
        );
        assert_eq!(
            resolution.config.selection_window_policy.long_term_rounds,
            512
        );
        assert_eq!(resolution.source.env_budget_override_keys.len(), 1);
        assert_eq!(resolution.source.env_selection_override_keys.len(), 1);
        let _ = fs::remove_file(path);
    }
}
