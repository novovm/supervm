#![forbid(unsafe_code)]

use anyhow::{bail, Result};
use novovm_exec::{
    build_log_bloom_v1, AoemEventLogV1, SupervmEvmExecutionLogV1, SupervmEvmExecutionReceiptV1,
    AOEM_LOG_BLOOM_BYTES_V1,
};
use novovm_network::{
    default_eth_fullnode_native_worker_runtime_snapshot_path_v1, derive_eth_fullnode_head_view_v1,
    derive_eth_fullnode_sync_view_v1, get_network_runtime_native_sync_status,
    get_network_runtime_sync_status, load_eth_fullnode_native_worker_runtime_snapshot_from_path_v1,
    resolve_eth_fullnode_budget_hooks_v1, resolve_eth_fullnode_canonical_query_method,
    resolve_eth_fullnode_native_runtime_config_resolution_v1,
    resolve_eth_fullnode_runtime_query_method,
    snapshot_eth_fullnode_native_worker_runtime_snapshot_v1, EthFullnodeBlockContextV1,
    EthFullnodeHeadViewV1,
};
use serde_json::{json, Value};
use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use crate::mainline_canonical::{
    derive_mainline_eth_fullnode_block_contexts_v1, derive_mainline_eth_fullnode_chain_view_v1,
    load_mainline_canonical_store, MainlineCanonicalStoreV1,
};

const ETH_NATIVE_RUNTIME_QUERY_SCHEMA_VERSION_V1: u64 = 1;
const ETH_EXEC_FAILURE_CLASSIFICATION_CONTRACT_V1: &str =
    novovm_exec::AOEM_FAILURE_CLASSIFICATION_CONTRACT_V1;
const ETH_PENDING_TX_PROPAGATION_CLASSIFICATION_CONTRACT_V1: &str =
    "novovm-network/native-pending-tx-propagation/v1";
const ETH_NATIVE_PEER_HEALTH_SUMMARY_SCHEMA_V1: &str = "supervm-eth-native-peer-health-summary/v1";
const ETH_NATIVE_SYNC_RUNTIME_SUMMARY_SCHEMA_V1: &str =
    "supervm-eth-native-sync-runtime-summary/v1";
const ETH_NATIVE_SYNC_DEGRADATION_SUMMARY_SCHEMA_V1: &str =
    "supervm-eth-native-sync-degradation-summary/v1";

fn to_hex_prefixed(bytes: &[u8]) -> String {
    let mut out = String::with_capacity(bytes.len() * 2 + 2);
    out.push_str("0x");
    for byte in bytes {
        out.push_str(&format!("{byte:02x}"));
    }
    out
}

fn parse_hex_u64(raw: &str) -> Option<u64> {
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return None;
    }
    let normalized = trimmed
        .strip_prefix("0x")
        .or_else(|| trimmed.strip_prefix("0X"))
        .unwrap_or(trimmed);
    u64::from_str_radix(normalized, 16).ok()
}

fn parse_hex_h256(raw: &str) -> Option<[u8; 32]> {
    let trimmed = raw.trim();
    let normalized = trimmed
        .strip_prefix("0x")
        .or_else(|| trimmed.strip_prefix("0X"))
        .unwrap_or(trimmed);
    if normalized.len() != 64 {
        return None;
    }
    let mut out = [0u8; 32];
    for (idx, chunk) in normalized.as_bytes().chunks(2).enumerate() {
        let hex = std::str::from_utf8(chunk).ok()?;
        out[idx] = u8::from_str_radix(hex, 16).ok()?;
    }
    Some(out)
}

fn value_as_u64(value: &Value) -> Option<u64> {
    match value {
        Value::Number(number) => number.as_u64(),
        Value::String(raw) => parse_hex_u64(raw).or_else(|| raw.trim().parse::<u64>().ok()),
        _ => None,
    }
}

fn value_as_string(value: &Value) -> Option<String> {
    match value {
        Value::String(raw) => {
            let trimmed = raw.trim();
            if trimmed.is_empty() {
                None
            } else {
                Some(trimmed.to_ascii_lowercase())
            }
        }
        _ => None,
    }
}

fn param_as_string(params: &Value, key: &str) -> Option<String> {
    match params {
        Value::Object(map) => map.get(key).and_then(value_as_string),
        Value::Array(items) => {
            if key == "tx_hash" || key == "hash" {
                items.first().and_then(value_as_string)
            } else {
                None
            }
        }
        _ => None,
    }
}

fn canonical_store_path_from_env() -> PathBuf {
    std::env::var("NOVOVM_MAINLINE_EVM_CANONICAL_STORE_PATH")
        .ok()
        .map(|raw| raw.trim().to_string())
        .filter(|raw| !raw.is_empty())
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("artifacts/mainline/evm-canonical-artifacts.json"))
}

fn runtime_snapshot_chain_id_from_env() -> u64 {
    std::env::var("NOVOVM_MAINLINE_QUERY_CHAIN_ID")
        .ok()
        .and_then(|raw| raw.trim().parse::<u64>().ok())
        .unwrap_or(1)
}

pub fn mainline_query_method_from_env() -> Option<String> {
    std::env::var("NOVOVM_MAINLINE_QUERY_METHOD")
        .ok()
        .map(|raw| raw.trim().to_string())
        .filter(|raw| !raw.is_empty())
}

pub fn mainline_query_params_from_env() -> Result<Value> {
    std::env::var("NOVOVM_MAINLINE_QUERY_PARAMS")
        .ok()
        .map(|raw| serde_json::from_str::<Value>(raw.trim()))
        .transpose()
        .map_err(Into::into)
        .map(|value| value.unwrap_or_else(|| Value::Array(Vec::new())))
}

pub fn default_mainline_runtime_snapshot_path() -> PathBuf {
    default_eth_fullnode_native_worker_runtime_snapshot_path_v1()
}

pub fn is_mainline_runtime_query_method(method: &str) -> bool {
    resolve_eth_fullnode_runtime_query_method(method).is_some()
}

fn receipt_status_hex(status_ok: bool) -> &'static str {
    if status_ok {
        "0x1"
    } else {
        "0x0"
    }
}

fn batch_log_matches_topics(
    log: &SupervmEvmExecutionLogV1,
    topics_filter: &[Option<Vec<String>>],
) -> bool {
    for (idx, expected) in topics_filter.iter().enumerate() {
        let Some(expected) = expected else {
            continue;
        };
        let Some(actual) = log.topics.get(idx) else {
            return false;
        };
        let actual_hex = to_hex_prefixed(actual);
        if !expected.iter().any(|candidate| candidate == &actual_hex) {
            return false;
        }
    }
    true
}

fn normalize_topics_filter(params: &Value) -> Vec<Option<Vec<String>>> {
    let Some(raw_topics) = (match params {
        Value::Object(map) => map.get("topics"),
        Value::Array(items) => items.first().and_then(|value| match value {
            Value::Object(map) => map.get("topics"),
            _ => None,
        }),
        _ => None,
    }) else {
        return Vec::new();
    };

    let Value::Array(items) = raw_topics else {
        return Vec::new();
    };
    items
        .iter()
        .map(|item| match item {
            Value::Null => None,
            Value::String(_) => value_as_string(item).map(|topic| vec![topic]),
            Value::Array(values) => {
                let normalized: Vec<String> = values.iter().filter_map(value_as_string).collect();
                if normalized.is_empty() {
                    None
                } else {
                    Some(normalized)
                }
            }
            _ => None,
        })
        .collect()
}

fn normalize_addresses_filter(params: &Value) -> Option<Vec<String>> {
    let raw = match params {
        Value::Object(map) => map.get("address"),
        Value::Array(items) => items.first().and_then(|value| match value {
            Value::Object(map) => map.get("address"),
            _ => None,
        }),
        _ => None,
    }?;
    match raw {
        Value::String(_) => value_as_string(raw).map(|address| vec![address]),
        Value::Array(values) => {
            let normalized: Vec<String> = values.iter().filter_map(value_as_string).collect();
            if normalized.is_empty() {
                None
            } else {
                Some(normalized)
            }
        }
        _ => None,
    }
}

fn normalize_block_bound(params: &Value, key: &str, default_value: u64) -> u64 {
    let value = match params {
        Value::Object(map) => map.get(key),
        Value::Array(items) => items.first().and_then(|value| match value {
            Value::Object(map) => map.get(key),
            _ => None,
        }),
        _ => None,
    };
    let Some(value) = value else {
        return default_value;
    };
    match value {
        Value::String(raw) if raw.eq_ignore_ascii_case("latest") => default_value,
        Value::String(raw) if raw.eq_ignore_ascii_case("earliest") => 0,
        _ => value_as_u64(value).unwrap_or(default_value),
    }
}

fn param_as_bool(params: &Value, key: &str, index: usize) -> Option<bool> {
    match params {
        Value::Object(map) => map.get(key).and_then(Value::as_bool),
        Value::Array(items) => items.get(index).and_then(Value::as_bool),
        _ => None,
    }
}

fn param_as_u64(params: &Value, key: &str, index: usize) -> Option<u64> {
    match params {
        Value::Object(map) => map.get(key).and_then(value_as_u64),
        Value::Array(items) => items.get(index).and_then(value_as_u64),
        _ => None,
    }
}

fn param_as_block_selector(params: &Value, key: &str, index: usize) -> Option<String> {
    match params {
        Value::Object(map) => map.get(key).and_then(value_as_string),
        Value::Array(items) => items.get(index).and_then(value_as_string),
        _ => None,
    }
}

fn resolve_block_number_selector(
    selector: Option<String>,
    latest_block_number: u64,
) -> Option<u64> {
    let selector = selector?;
    if selector.eq_ignore_ascii_case("latest")
        || selector.eq_ignore_ascii_case("safe")
        || selector.eq_ignore_ascii_case("finalized")
    {
        return Some(latest_block_number);
    }
    if selector.eq_ignore_ascii_case("earliest") {
        return Some(0);
    }
    parse_hex_u64(&selector).or_else(|| selector.parse::<u64>().ok())
}

fn zero_hash_hex() -> String {
    to_hex_prefixed(&[0u8; 32])
}

fn now_unix_millis_v1() -> u128 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|value| value.as_millis())
        .unwrap_or(0)
}

fn effective_receipt_logs_v1(
    receipt: &SupervmEvmExecutionReceiptV1,
) -> &[SupervmEvmExecutionLogV1] {
    if receipt.status_ok {
        receipt.logs.as_slice()
    } else {
        &[]
    }
}

fn normalized_receipt_log_bloom_v1(receipt: &SupervmEvmExecutionReceiptV1) -> Vec<u8> {
    if !receipt.status_ok {
        return vec![0u8; AOEM_LOG_BLOOM_BYTES_V1];
    }
    let logs = effective_receipt_logs_v1(receipt);
    if logs.is_empty() {
        return vec![0u8; AOEM_LOG_BLOOM_BYTES_V1];
    }
    let aoem_logs = logs
        .iter()
        .map(|log| AoemEventLogV1 {
            emitter: log.emitter.clone(),
            topics: log.topics.clone(),
            data: log.data.clone(),
            log_index: log.log_index,
        })
        .collect::<Vec<_>>();
    build_log_bloom_v1(aoem_logs.as_slice())
}

fn combine_receipt_log_bloom(receipts: &[SupervmEvmExecutionReceiptV1]) -> Vec<u8> {
    let max_len = receipts
        .iter()
        .map(normalized_receipt_log_bloom_v1)
        .map(|bloom| bloom.len())
        .max()
        .unwrap_or(0);
    if max_len == 0 {
        return vec![0u8; AOEM_LOG_BLOOM_BYTES_V1];
    }
    let mut out = vec![0u8; max_len];
    for receipt in receipts {
        let bloom = normalized_receipt_log_bloom_v1(receipt);
        for (idx, byte) in bloom.iter().enumerate() {
            out[idx] |= *byte;
        }
    }
    out
}

fn block_transactions_json(
    block_context: &EthFullnodeBlockContextV1,
    receipts: &[SupervmEvmExecutionReceiptV1],
    full_transactions: bool,
) -> Vec<Value> {
    receipts
        .iter()
        .map(|receipt| {
            if full_transactions {
                json!({
                    "hash": to_hex_prefixed(&receipt.tx_hash),
                    "transactionIndex": format!("0x{:x}", receipt.tx_index),
                    "blockNumber": format!("0x{:x}", block_context.block_number),
                    "blockHash": to_hex_prefixed(&block_context.block_hash),
                    "type": receipt.receipt_type.map(|value| format!("0x{:x}", value)),
                    "status": receipt_status_hex(receipt.status_ok),
                    "gasUsed": format!("0x{:x}", receipt.gas_used),
                    "contractAddress": if receipt.status_ok {
                        receipt.contract_address.as_ref().map(|value| to_hex_prefixed(value))
                    } else {
                        None
                    },
                })
            } else {
                Value::String(to_hex_prefixed(&receipt.tx_hash))
            }
        })
        .collect()
}

fn head_view_to_eth_json(head: &EthFullnodeHeadViewV1) -> Value {
    json!({
        "blockNumber": format!("0x{:x}", head.block_number),
        "blockHash": to_hex_prefixed(&head.block_hash),
        "parentBlockHash": to_hex_prefixed(&head.parent_block_hash),
        "stateRoot": to_hex_prefixed(&head.state_root),
        "stateVersion": format!("0x{:x}", head.state_version),
        "chainId": format!("0x{:x}", head.chain_id),
        "blockViewSource": head.source.as_str(),
    })
}

#[derive(Debug, Clone)]
struct NativeBlockLifecycleResolutionV1 {
    tracked: bool,
    source: Option<&'static str>,
    lifecycle_stage: Option<novovm_network::NetworkRuntimeNativeBlockLifecycleStageV1>,
    ownership: Option<&'static str>,
    authoritative_canonical_match: Option<bool>,
    authoritative_canonical_block_hash: Option<[u8; 32]>,
}

impl NativeBlockLifecycleResolutionV1 {
    fn untracked() -> Self {
        Self {
            tracked: false,
            source: None,
            lifecycle_stage: None,
            ownership: None,
            authoritative_canonical_match: None,
            authoritative_canonical_block_hash: None,
        }
    }
}

fn resolve_native_block_lifecycle_for_block_context_v1(
    snapshot: Option<&novovm_network::EthFullnodeNativeWorkerRuntimeSnapshotV1>,
    source: &'static str,
    block_context: &EthFullnodeBlockContextV1,
) -> NativeBlockLifecycleResolutionV1 {
    let Some(snapshot) = snapshot else {
        return NativeBlockLifecycleResolutionV1::untracked();
    };
    let canonical_at_height = snapshot
        .native_canonical_blocks
        .iter()
        .find(|block| block.number == block_context.block_number && block.canonical);
    if let Some(block) = snapshot
        .native_canonical_blocks
        .iter()
        .find(|block| block.hash == block_context.block_hash)
    {
        return NativeBlockLifecycleResolutionV1 {
            tracked: true,
            source: Some(source),
            lifecycle_stage: Some(block.lifecycle_stage),
            ownership: Some(native_block_lifecycle_ownership_name(block.lifecycle_stage)),
            authoritative_canonical_match: Some(
                canonical_at_height
                    .map(|candidate| candidate.hash == block.hash)
                    .unwrap_or(block.canonical),
            ),
            authoritative_canonical_block_hash: canonical_at_height.map(|candidate| candidate.hash),
        };
    }
    if let Some(canonical) = canonical_at_height {
        return NativeBlockLifecycleResolutionV1 {
            tracked: true,
            source: Some(source),
            lifecycle_stage: Some(
                novovm_network::NetworkRuntimeNativeBlockLifecycleStageV1::NonCanonical,
            ),
            ownership: Some("non_canonical"),
            authoritative_canonical_match: Some(false),
            authoritative_canonical_block_hash: Some(canonical.hash),
        };
    }
    NativeBlockLifecycleResolutionV1::untracked()
}

fn apply_native_block_lifecycle_metadata_v1(
    value: &mut Value,
    resolution: &NativeBlockLifecycleResolutionV1,
    ownership_field: &str,
) {
    let Some(object) = value.as_object_mut() else {
        return;
    };
    object.insert(
        "nativeLifecycleTracked".to_string(),
        Value::Bool(resolution.tracked),
    );
    object.insert(
        "nativeLifecycleSource".to_string(),
        resolution
            .source
            .map(|value| Value::String(value.to_string()))
            .unwrap_or(Value::Null),
    );
    object.insert(
        "nativeLifecycleStage".to_string(),
        resolution
            .lifecycle_stage
            .map(|value| Value::String(value.as_str().to_string()))
            .unwrap_or(Value::Null),
    );
    object.insert(
        ownership_field.to_string(),
        resolution
            .ownership
            .map(|value| Value::String(value.to_string()))
            .unwrap_or(Value::Null),
    );
    object.insert(
        "authoritativeCanonicalMatch".to_string(),
        resolution
            .authoritative_canonical_match
            .map(Value::Bool)
            .unwrap_or(Value::Null),
    );
    object.insert(
        "authoritativeCanonicalBlockHash".to_string(),
        resolution
            .authoritative_canonical_block_hash
            .map(|value| Value::String(to_hex_prefixed(&value)))
            .unwrap_or(Value::Null),
    );
}

fn sync_view_to_eth_json(sync_view: &novovm_network::EthFullnodeSyncViewV1) -> Value {
    json!({
        "startingBlock": format!("0x{:x}", sync_view.starting_block_number),
        "currentBlock": format!("0x{:x}", sync_view.current_block_number),
        "highestBlock": format!("0x{:x}", sync_view.highest_block_number),
        "currentBlockHash": to_hex_prefixed(&sync_view.current_block_hash),
        "parentBlockHash": to_hex_prefixed(&sync_view.parent_block_hash),
        "currentStateRoot": to_hex_prefixed(&sync_view.current_state_root),
        "currentStateVersion": format!("0x{:x}", sync_view.current_state_version),
        "peerCount": format!("0x{:x}", sync_view.peer_count),
        "chainId": format!("0x{:x}", sync_view.chain_id),
        "blockViewSource": sync_view.source.as_str(),
        "nativeSyncPhase": sync_view.native_sync_phase,
        "syncing": sync_view.syncing,
    })
}

fn native_canonical_chain_to_json(
    value: &novovm_network::NetworkRuntimeNativeCanonicalChainStateV1,
) -> Value {
    json!({
        "chainId": format!("0x{:x}", value.chain_id),
        "lifecycleStage": value.lifecycle_stage.as_str(),
        "retainedBlockCount": value.retained_block_count,
        "canonicalBlockCount": value.canonical_block_count,
        "canonicalUpdateCount": value.canonical_update_count,
        "reorgCount": value.reorg_count,
        "lastReorgDepth": value.last_reorg_depth.map(|depth| format!("0x{:x}", depth)),
        "lastReorgUnixMs": value.last_reorg_unix_ms,
        "lastHeadChangeUnixMs": value.last_head_change_unix_ms,
        "blockLifecycleSummary": {
            "seenCount": value.block_lifecycle_summary.seen_count,
            "headerOnlyCount": value.block_lifecycle_summary.header_only_count,
            "bodyReadyCount": value.block_lifecycle_summary.body_ready_count,
            "canonicalCount": value.block_lifecycle_summary.canonical_count,
            "nonCanonicalCount": value.block_lifecycle_summary.non_canonical_count,
            "reorgedOutCount": value.block_lifecycle_summary.reorged_out_count,
        },
        "head": value.head.as_ref().map(|head| json!({
            "blockNumber": format!("0x{:x}", head.number),
            "blockHash": to_hex_prefixed(&head.hash),
            "parentBlockHash": to_hex_prefixed(&head.parent_hash),
            "stateRoot": to_hex_prefixed(&head.state_root),
            "headerObserved": head.header_observed,
            "bodyAvailable": head.body_available,
            "lifecycleStage": head.lifecycle_stage.as_str(),
            "canonical": head.canonical,
            "safe": head.safe,
            "finalized": head.finalized,
            "sourcePeerId": head.source_peer_id.map(|peer_id| format!("0x{:x}", peer_id)),
            "observedUnixMs": head.observed_unix_ms,
        })).unwrap_or(Value::Null),
    })
}

fn native_block_lifecycle_ownership_name(
    stage: novovm_network::NetworkRuntimeNativeBlockLifecycleStageV1,
) -> &'static str {
    match stage {
        novovm_network::NetworkRuntimeNativeBlockLifecycleStageV1::Canonical => "canonical",
        novovm_network::NetworkRuntimeNativeBlockLifecycleStageV1::ReorgedOut => "reorged_out",
        _ => "non_canonical",
    }
}

fn native_canonical_block_to_json(
    block: &novovm_network::NetworkRuntimeNativeCanonicalBlockStateV1,
) -> Value {
    let ownership = native_block_lifecycle_ownership_name(block.lifecycle_stage);
    json!({
        "blockNumber": format!("0x{:x}", block.number),
        "blockHash": to_hex_prefixed(&block.hash),
        "parentBlockHash": to_hex_prefixed(&block.parent_hash),
        "stateRoot": to_hex_prefixed(&block.state_root),
        "headerObserved": block.header_observed,
        "bodyAvailable": block.body_available,
        "lifecycleStage": block.lifecycle_stage.as_str(),
        "canonical": block.canonical,
        "safe": block.safe,
        "finalized": block.finalized,
        "sourcePeerId": block.source_peer_id.map(|peer_id| format!("0x{:x}", peer_id)),
        "observedUnixMs": block.observed_unix_ms,
        "receiptOwnership": ownership,
        "logOwnership": ownership,
    })
}

fn native_pending_tx_to_json(tx: &novovm_network::NetworkRuntimeNativePendingTxStateV1) -> Value {
    let authoritative_lifecycle = match tx.lifecycle_stage {
        novovm_network::NetworkRuntimeNativePendingTxLifecycleStageV1::Pending => match tx.origin {
            novovm_network::NetworkRuntimeNativePendingTxOriginV1::Local => "local_pending",
            novovm_network::NetworkRuntimeNativePendingTxOriginV1::Remote => "remote_pending",
            novovm_network::NetworkRuntimeNativePendingTxOriginV1::Unknown => "pending",
        },
        novovm_network::NetworkRuntimeNativePendingTxLifecycleStageV1::Seen => match tx.origin {
            novovm_network::NetworkRuntimeNativePendingTxOriginV1::Local => "local_seen",
            novovm_network::NetworkRuntimeNativePendingTxOriginV1::Remote => "remote_seen",
            novovm_network::NetworkRuntimeNativePendingTxOriginV1::Unknown => "seen",
        },
        _ => tx.lifecycle_stage.as_str(),
    };
    json!({
        "txHash": to_hex_prefixed(&tx.tx_hash),
        "lifecycleStage": tx.lifecycle_stage.as_str(),
        "authoritativeLifecycle": authoritative_lifecycle,
        "origin": tx.origin.as_str(),
        "sourcePeerId": tx.source_peer_id.map(|peer_id| format!("0x{:x}", peer_id)),
        "firstSeenUnixMs": tx.first_seen_unix_ms,
        "lastUpdatedUnixMs": tx.last_updated_unix_ms,
        "lastBlockNumber": tx.last_block_number.map(|value| format!("0x{:x}", value)),
        "lastBlockHash": tx.last_block_hash.map(|value| to_hex_prefixed(&value)),
        "canonicalInclusion": tx.canonical_inclusion,
        "ingressCount": tx.ingress_count,
        "propagationCount": tx.propagation_count,
        "propagationAttemptCount": tx.propagation_attempt_count,
        "propagationSuccessCount": tx.propagation_success_count,
        "propagationFailureCount": tx.propagation_failure_count,
        "propagatedPeerCount": tx.propagated_peer_count,
        "lastPropagationAttemptUnixMs": tx.last_propagation_attempt_unix_ms,
        "lastPropagationUnixMs": tx.last_propagation_unix_ms,
        "lastPropagationFailureUnixMs": tx.last_propagation_failure_unix_ms,
        "failureClassificationContract": ETH_PENDING_TX_PROPAGATION_CLASSIFICATION_CONTRACT_V1,
        "failureClass": tx.last_propagation_failure_class,
        "failureClassSource": tx
            .propagation_stop_reason
            .map(|_| "propagation_stop_reason"),
        "failureRecoverability": tx.propagation_recoverability.map(|value| value.as_str()),
        "lastPropagationFailureClass": tx.last_propagation_failure_class,
        "lastPropagationFailurePhase": tx.last_propagation_failure_phase,
        "lastPropagationPeerId": tx.last_propagation_peer_id.map(|peer_id| format!("0x{:x}", peer_id)),
        "lastPropagatedPeerId": tx.last_propagated_peer_id.map(|peer_id| format!("0x{:x}", peer_id)),
        "propagationDisposition": tx.propagation_disposition.map(|value| value.as_str()),
        "propagationStopReason": tx.propagation_stop_reason.map(|value| value.as_str()),
        "propagationRecoverability": tx.propagation_recoverability.map(|value| value.as_str()),
        "retryEligible": tx.retry_eligible,
        "retryAfterUnixMs": tx.retry_after_unix_ms,
        "retryBackoffLevel": tx.retry_backoff_level,
        "retrySuppressedReason": tx.retry_suppressed_reason.map(|value| value.as_str()),
        "pendingFinalDisposition": tx.pending_final_disposition.as_str(),
        "inclusionCount": tx.inclusion_count,
        "reorgBackCount": tx.reorg_back_count,
        "dropCount": tx.drop_count,
        "rejectCount": tx.reject_count,
    })
}

fn native_pending_tx_tombstone_to_json(
    tombstone: &novovm_network::NetworkRuntimeNativePendingTxTombstoneV1,
) -> Value {
    json!({
        "txHash": to_hex_prefixed(&tombstone.tx_hash),
        "lifecycleStage": tombstone.lifecycle_stage.as_str(),
        "origin": tombstone.origin.as_str(),
        "finalDisposition": tombstone.final_disposition.as_str(),
        "failureClassificationContract": ETH_PENDING_TX_PROPAGATION_CLASSIFICATION_CONTRACT_V1,
        "failureClass": tombstone.propagation_stop_reason.map(|value| value.as_str()),
        "failureClassSource": tombstone.propagation_stop_reason.map(|_| "propagation_stop_reason"),
        "failureRecoverability": tombstone.propagation_recoverability.map(|value| value.as_str()),
        "propagationDisposition": tombstone.propagation_disposition.map(|value| value.as_str()),
        "propagationStopReason": tombstone.propagation_stop_reason.map(|value| value.as_str()),
        "propagationRecoverability": tombstone.propagation_recoverability.map(|value| value.as_str()),
        "lastUpdatedUnixMs": tombstone.last_updated_unix_ms,
    })
}

fn tombstone_stop_reason_counts_json_v1(
    tombstones: &[novovm_network::NetworkRuntimeNativePendingTxTombstoneV1],
) -> Value {
    let mut counts = BTreeMap::<String, u64>::new();
    for tombstone in tombstones {
        if let Some(reason) = tombstone.propagation_stop_reason {
            *counts.entry(reason.as_str().to_string()).or_default() += 1;
        }
    }
    let mut out = counts
        .into_iter()
        .map(|(reason, count)| json!({ "reason": reason, "count": count }))
        .collect::<Vec<_>>();
    out.sort_by(|a, b| {
        let a_count = a.get("count").and_then(Value::as_u64).unwrap_or(0);
        let b_count = b.get("count").and_then(Value::as_u64).unwrap_or(0);
        b_count.cmp(&a_count).then_with(|| {
            a.get("reason")
                .and_then(Value::as_str)
                .unwrap_or_default()
                .cmp(b.get("reason").and_then(Value::as_str).unwrap_or_default())
        })
    });
    Value::Array(out)
}

fn top_tombstone_stop_reason_v1(
    tombstones: &[novovm_network::NetworkRuntimeNativePendingTxTombstoneV1],
) -> Option<String> {
    let counts = tombstone_stop_reason_counts_json_v1(tombstones);
    counts
        .as_array()
        .and_then(|items| items.first())
        .and_then(|item| item.get("reason"))
        .and_then(Value::as_str)
        .map(str::to_string)
}

fn cleanup_pressure_status_v1(
    removed_in_window: usize,
    high_pressure_threshold: u64,
    critical_pressure_threshold: u64,
) -> &'static str {
    let removed = removed_in_window as u64;
    if removed >= critical_pressure_threshold {
        return "critical";
    }
    if removed >= high_pressure_threshold {
        return "degraded";
    }
    "healthy"
}

fn block_context_to_eth_json(
    block_context: &EthFullnodeBlockContextV1,
    receipts: &[SupervmEvmExecutionReceiptV1],
    full_transactions: bool,
    native_lifecycle: &NativeBlockLifecycleResolutionV1,
) -> Value {
    let gas_used = receipts.iter().map(|receipt| receipt.gas_used).sum::<u64>();
    let cumulative_gas_used = receipts
        .last()
        .map(|receipt| receipt.cumulative_gas_used)
        .unwrap_or(gas_used);
    let transactions = block_transactions_json(block_context, receipts, full_transactions);
    let logs_bloom = combine_receipt_log_bloom(receipts);
    let mut out = json!({
        "number": format!("0x{:x}", block_context.block_number),
        "hash": to_hex_prefixed(&block_context.block_hash),
        "parentHash": to_hex_prefixed(&block_context.parent_block_hash),
        "nonce": Value::Null,
        "sha3Uncles": zero_hash_hex(),
        "logsBloom": to_hex_prefixed(&logs_bloom),
        "transactionsRoot": Value::Null,
        "stateRoot": to_hex_prefixed(&block_context.state_root),
        "receiptsRoot": Value::Null,
        "miner": Value::Null,
        "difficulty": "0x0",
        "totalDifficulty": "0x0",
        "extraData": "0x",
        "size": format!("0x{:x}", receipts.len()),
        "gasLimit": Value::Null,
        "gasUsed": format!("0x{:x}", gas_used),
        "timestamp": Value::Null,
        "transactions": transactions,
        "uncles": Vec::<Value>::new(),
        "withdrawals": Vec::<Value>::new(),
        "baseFeePerGas": receipts
            .last()
            .and_then(|receipt| receipt.effective_gas_price)
            .map(|value| format!("0x{:x}", value)),
        "blockViewSource": block_context.source.as_str(),
        "canonicalBatchSeq": block_context
            .canonical_batch_seq
            .map(|value| format!("0x{:x}", value)),
        "stateVersion": format!("0x{:x}", block_context.state_version),
        "transactionCount": format!("0x{:x}", receipts.len()),
        "cumulativeGasUsed": format!("0x{:x}", cumulative_gas_used),
        "chainId": format!("0x{:x}", block_context.chain_id),
    });
    apply_native_block_lifecycle_metadata_v1(&mut out, native_lifecycle, "blockOwnership");
    out
}

fn block_context_matches_filter(
    params: &Value,
    block_context: &EthFullnodeBlockContextV1,
    latest_block_number: u64,
) -> bool {
    let block_hash_filter = match params {
        Value::Object(map) => map.get("blockHash").and_then(value_as_string),
        Value::Array(items) => items.first().and_then(|value| match value {
            Value::Object(map) => map.get("blockHash").and_then(value_as_string),
            _ => None,
        }),
        _ => None,
    };
    let block_hash_hex = to_hex_prefixed(&block_context.block_hash);
    if let Some(block_hash) = block_hash_filter {
        return block_hash == block_hash_hex;
    }
    let from_block = normalize_block_bound(params, "fromBlock", 0);
    let to_block = normalize_block_bound(params, "toBlock", latest_block_number);
    block_context.block_number >= from_block && block_context.block_number <= to_block
}

fn log_to_eth_json(
    block_context: &EthFullnodeBlockContextV1,
    receipt: &SupervmEvmExecutionReceiptV1,
    log: &SupervmEvmExecutionLogV1,
    native_lifecycle: &NativeBlockLifecycleResolutionV1,
) -> Value {
    let mut out = json!({
        "address": to_hex_prefixed(&log.emitter),
        "topics": log.topics.iter().map(|topic| to_hex_prefixed(topic)).collect::<Vec<_>>(),
        "data": to_hex_prefixed(&log.data),
        "blockNumber": format!("0x{:x}", block_context.block_number),
        "blockHash": to_hex_prefixed(&block_context.block_hash),
        "transactionHash": to_hex_prefixed(&receipt.tx_hash),
        "transactionIndex": format!("0x{:x}", receipt.tx_index),
        "logIndex": format!("0x{:x}", log.log_index),
        "removed": native_lifecycle
            .ownership
            .map(|value| value != "canonical")
            .unwrap_or(false),
        "stateVersion": format!("0x{:x}", log.state_version),
        "chainId": format!("0x{:x}", block_context.chain_id),
        "blockViewSource": block_context.source.as_str(),
    });
    apply_native_block_lifecycle_metadata_v1(&mut out, native_lifecycle, "logOwnership");
    out
}

fn receipt_to_eth_json(
    block_context: &EthFullnodeBlockContextV1,
    receipt: &SupervmEvmExecutionReceiptV1,
    native_lifecycle: &NativeBlockLifecycleResolutionV1,
) -> Value {
    let logs = effective_receipt_logs_v1(receipt)
        .iter()
        .map(|log| log_to_eth_json(block_context, receipt, log, native_lifecycle))
        .collect::<Vec<_>>();
    let logs_bloom = normalized_receipt_log_bloom_v1(receipt);
    let contract_address = if receipt.status_ok {
        receipt
            .contract_address
            .as_ref()
            .map(|value| to_hex_prefixed(value))
    } else {
        None
    };
    let mut out = json!({
        "transactionHash": to_hex_prefixed(&receipt.tx_hash),
        "transactionIndex": format!("0x{:x}", receipt.tx_index),
        "blockNumber": format!("0x{:x}", block_context.block_number),
        "blockHash": to_hex_prefixed(&block_context.block_hash),
        "from": Value::Null,
        "to": Value::Null,
        "cumulativeGasUsed": format!("0x{:x}", receipt.cumulative_gas_used),
        "gasUsed": format!("0x{:x}", receipt.gas_used),
        "contractAddress": contract_address,
        "logs": logs,
        "logsBloom": to_hex_prefixed(&logs_bloom),
        "status": receipt_status_hex(receipt.status_ok),
        "effectiveGasPrice": receipt.effective_gas_price.map(|value| format!("0x{:x}", value)),
        "type": receipt.receipt_type.map(|value| format!("0x{:x}", value)),
        "revertData": if receipt.status_ok {
            None
        } else {
            receipt.revert_data.as_ref().map(|value| to_hex_prefixed(value))
        },
        "failureClassificationContract": ETH_EXEC_FAILURE_CLASSIFICATION_CONTRACT_V1,
        "stateRoot": to_hex_prefixed(&receipt.state_root),
        "stateVersion": format!("0x{:x}", receipt.state_version),
        "chainId": format!("0x{:x}", block_context.chain_id),
        "blockViewSource": block_context.source.as_str(),
        "canonicalBatchSeq": block_context
            .canonical_batch_seq
            .map(|value| format!("0x{:x}", value)),
        "parentBlockHash": to_hex_prefixed(&block_context.parent_block_hash),
    });
    apply_native_block_lifecycle_metadata_v1(&mut out, native_lifecycle, "receiptOwnership");
    out
}

fn runtime_snapshot_chain_id_from_params(params: &Value) -> u64 {
    match params {
        Value::Object(map) => map
            .get("chainId")
            .or_else(|| map.get("chain_id"))
            .and_then(value_as_u64)
            .unwrap_or_else(runtime_snapshot_chain_id_from_env),
        Value::Array(items) => items
            .first()
            .and_then(value_as_u64)
            .unwrap_or_else(runtime_snapshot_chain_id_from_env),
        _ => runtime_snapshot_chain_id_from_env(),
    }
}

#[inline]
fn runtime_query_limit_v1(
    chain_id: u64,
    requested: Option<u64>,
    default_limit: u64,
    hard_cap: u64,
) -> usize {
    let budget_hooks = resolve_eth_fullnode_budget_hooks_v1(chain_id);
    let budget_cap = budget_hooks.runtime_query_result_max.max(1);
    requested
        .unwrap_or(default_limit)
        .max(1)
        .min(hard_cap)
        .min(budget_cap) as usize
}

fn load_mainline_runtime_snapshot_v1(
    chain_id: u64,
) -> Result<(
    Option<novovm_network::EthFullnodeNativeWorkerRuntimeSnapshotV1>,
    &'static str,
)> {
    let in_memory = snapshot_eth_fullnode_native_worker_runtime_snapshot_v1(chain_id)
        .filter(|snapshot| snapshot.chain_id == chain_id);
    if in_memory.is_some() {
        return Ok((in_memory, "runtime_snapshot_memory"));
    }
    let path = default_mainline_runtime_snapshot_path();
    if path.exists() {
        let snapshot =
            load_eth_fullnode_native_worker_runtime_snapshot_from_path_v1(path.as_path())?;
        if snapshot.chain_id == chain_id {
            return Ok((Some(snapshot), "runtime_snapshot_file"));
        }
    }
    Ok((None, "runtime_snapshot_memory"))
}

fn lifecycle_stage_counts_json(summary: &novovm_network::EthPeerLifecycleSummaryV1) -> Value {
    json!({
        "discovered": summary.discovered_count,
        "connecting": summary.connecting_count,
        "connected": summary.connected_count,
        "helloOk": summary.hello_ok_count,
        "statusOk": summary.status_ok_count,
        "ready": summary.ready_count,
        "syncing": summary.syncing_count,
        "cooldown": summary.cooldown_count,
        "temporarilyFailed": summary.temporarily_failed_count,
        "permanentlyRejected": summary.permanently_rejected_count,
    })
}

fn failure_class_counts_json(summary: &novovm_network::EthPeerLifecycleSummaryV1) -> Value {
    json!({
        "connectFailure": summary.connect_failure_count,
        "handshakeFailure": summary.handshake_failure_count,
        "decodeFailure": summary.decode_failure_count,
        "timeout": summary.timeout_count,
        "validationReject": summary.validation_reject_count,
        "disconnect": summary.disconnect_count,
    })
}

fn last_failure_reasons_json(
    snapshot: &novovm_network::EthFullnodeNativeWorkerRuntimeSnapshotV1,
) -> Value {
    let mut counts = BTreeMap::<String, u64>::new();
    for session in &snapshot.peer_sessions {
        if let Some(reason) = session.last_failure_reason_name.as_ref() {
            *counts.entry(reason.clone()).or_default() += 1;
        }
    }
    Value::Array(
        counts
            .into_iter()
            .map(|(reason, count)| json!({ "reason": reason, "count": count }))
            .collect(),
    )
}

fn ratio_bps_v1(numerator: u64, denominator: u64) -> Option<u64> {
    if denominator == 0 {
        return None;
    }
    Some(((numerator as u128) * 10_000 / (denominator as u128)) as u64)
}

fn is_tx_broadcast_failure_v1(
    failure: &novovm_network::EthFullnodeNativePeerFailureSnapshotV1,
) -> bool {
    let reason = failure.reason_name.as_deref();
    let reason_match = reason
        .is_some_and(|value| value.contains("transaction") || value.starts_with("transactions_"));
    let error_match = failure.error.contains("transaction");
    reason_match || error_match
}

fn tx_broadcast_failure_reason_counts_v1(
    snapshot: &novovm_network::EthFullnodeNativeWorkerRuntimeSnapshotV1,
) -> BTreeMap<String, u64> {
    let mut counts = BTreeMap::<String, u64>::new();
    for failure in &snapshot.peer_failures {
        if !is_tx_broadcast_failure_v1(failure) {
            continue;
        }
        let key = failure
            .reason_name
            .as_deref()
            .map(str::to_string)
            .unwrap_or_else(|| "transactions_unknown".to_string());
        *counts.entry(key).or_default() += 1;
    }
    counts
}

fn tx_broadcast_failure_class_counts_v1(
    snapshot: &novovm_network::EthFullnodeNativeWorkerRuntimeSnapshotV1,
) -> BTreeMap<String, u64> {
    let mut counts = BTreeMap::<String, u64>::new();
    for failure in &snapshot.peer_failures {
        if !is_tx_broadcast_failure_v1(failure) {
            continue;
        }
        *counts.entry(failure.class.clone()).or_default() += 1;
    }
    counts
}

fn tx_broadcast_failure_phase_counts_v1(
    snapshot: &novovm_network::EthFullnodeNativeWorkerRuntimeSnapshotV1,
) -> BTreeMap<String, u64> {
    let mut counts = BTreeMap::<String, u64>::new();
    for failure in &snapshot.peer_failures {
        if !is_tx_broadcast_failure_v1(failure) {
            continue;
        }
        *counts.entry(failure.phase.clone()).or_default() += 1;
    }
    counts
}

fn tx_broadcast_failure_peer_ids_v1(
    snapshot: &novovm_network::EthFullnodeNativeWorkerRuntimeSnapshotV1,
) -> BTreeSet<u64> {
    let mut peers = BTreeSet::<u64>::new();
    for failure in &snapshot.peer_failures {
        if !is_tx_broadcast_failure_v1(failure) {
            continue;
        }
        peers.insert(failure.peer_id);
    }
    peers
}

#[derive(Default)]
struct TxBroadcastPeerFailureStatsV1 {
    failure_count: u64,
    reason_counts: BTreeMap<String, u64>,
    class_counts: BTreeMap<String, u64>,
    phase_counts: BTreeMap<String, u64>,
}

fn tx_broadcast_failure_peer_stats_v1(
    snapshot: &novovm_network::EthFullnodeNativeWorkerRuntimeSnapshotV1,
) -> BTreeMap<u64, TxBroadcastPeerFailureStatsV1> {
    let mut out = BTreeMap::<u64, TxBroadcastPeerFailureStatsV1>::new();
    for failure in &snapshot.peer_failures {
        if !is_tx_broadcast_failure_v1(failure) {
            continue;
        }
        let stats = out.entry(failure.peer_id).or_default();
        stats.failure_count = stats.failure_count.saturating_add(1);
        let reason = failure
            .reason_name
            .as_deref()
            .map(str::to_string)
            .unwrap_or_else(|| "transactions_unknown".to_string());
        *stats.reason_counts.entry(reason).or_default() += 1;
        *stats.class_counts.entry(failure.class.clone()).or_default() += 1;
        *stats.phase_counts.entry(failure.phase.clone()).or_default() += 1;
    }
    out
}

fn tx_broadcast_failure_peer_selection_correlation_json(
    snapshot: &novovm_network::EthFullnodeNativeWorkerRuntimeSnapshotV1,
) -> Value {
    let peer_stats = tx_broadcast_failure_peer_stats_v1(snapshot);
    let mut out = Vec::<Value>::new();
    for (peer_id, stats) in peer_stats {
        let sync_score = snapshot.peer_selection_scores.iter().find(|score| {
            score.peer_id == peer_id
                && matches!(score.role, novovm_network::EthPeerSelectionRoleV1::Sync)
        });
        let bootstrap_score = snapshot.peer_selection_scores.iter().find(|score| {
            score.peer_id == peer_id
                && matches!(
                    score.role,
                    novovm_network::EthPeerSelectionRoleV1::Bootstrap
                )
        });
        let session = snapshot
            .peer_sessions
            .iter()
            .find(|session| session.peer_id == peer_id);
        out.push(json!({
            "peerId": format!("0x{peer_id:x}"),
            "failureCount": stats.failure_count,
            "failureReasons": stats.reason_counts.into_iter().map(|(reason, count)| json!({
                "reason": reason,
                "count": count,
            })).collect::<Vec<_>>(),
            "failureClassCounts": stats.class_counts.into_iter().map(|(class, count)| json!({
                "class": class,
                "count": count,
            })).collect::<Vec<_>>(),
            "failurePhaseCounts": stats.phase_counts.into_iter().map(|(phase, count)| json!({
                "phase": phase,
                "count": count,
            })).collect::<Vec<_>>(),
            "selection": {
                "syncSelected": sync_score.map(|score| score.selected).unwrap_or(false),
                "bootstrapSelected": bootstrap_score.map(|score| score.selected).unwrap_or(false),
                "syncScore": sync_score.map(|score| score.score),
                "bootstrapScore": bootstrap_score.map(|score| score.score),
                "syncLongTermScore": sync_score.map(|score| score.long_term_score),
                "syncEligible": sync_score.map(|score| score.eligible).unwrap_or(false),
                "syncStage": sync_score.map(|score| score.stage.as_str()),
            },
            "session": {
                "known": session.is_some(),
                "sessionReady": session.map(|item| item.session_ready).unwrap_or(false),
                "lifecycleStage": session.map(|item| item.lifecycle_stage.as_str()),
                "consecutiveFailures": session.map(|item| item.consecutive_failures).unwrap_or(0),
                "lastFailureReason": session.and_then(|item| item.last_failure_reason_name.as_ref().map(String::as_str)),
            }
        }));
    }
    Value::Array(out)
}

fn sync_degradation_selection_correlation_json(
    snapshot: &novovm_network::EthFullnodeNativeWorkerRuntimeSnapshotV1,
) -> Value {
    let failing_peers = tx_broadcast_failure_peer_ids_v1(snapshot);
    let selected_sync_peers = snapshot
        .selection_quality_summary
        .selected_sync_peer_ids
        .iter()
        .copied()
        .collect::<BTreeSet<_>>();
    let failing_selected_sync_count = failing_peers
        .iter()
        .filter(|peer_id| selected_sync_peers.contains(peer_id))
        .count() as u64;
    let selected_sync_total = snapshot.selection_quality_summary.selected_sync_peers;
    let failing_selected_sync_bps = ratio_bps_v1(failing_selected_sync_count, selected_sync_total);
    let top_selected_sync_failing = snapshot
        .selection_quality_summary
        .top_selected_sync_peer_id
        .is_some_and(|peer_id| failing_peers.contains(&peer_id));
    json!({
        "selectedSyncPeers": selected_sync_total,
        "failingBroadcastPeerCount": failing_peers.len(),
        "failingSelectedSyncPeerCount": failing_selected_sync_count,
        "failingSelectedSyncPeerRateBps": failing_selected_sync_bps,
        "topSelectedSyncPeerFailing": top_selected_sync_failing,
        "topSelectedSyncPeerId": snapshot.selection_quality_summary.top_selected_sync_peer_id.map(|peer_id| format!("0x{peer_id:x}")),
        "topSelectedSyncScore": snapshot.selection_quality_summary.top_selected_sync_score,
        "averageSelectedSyncScore": snapshot.selection_quality_summary.average_selected_sync_score,
        "likelySelectionQualityIssue": failing_selected_sync_count > 0 || top_selected_sync_failing,
    })
}

fn tx_broadcast_failure_reasons_json(
    snapshot: &novovm_network::EthFullnodeNativeWorkerRuntimeSnapshotV1,
) -> Value {
    let counts = tx_broadcast_failure_reason_counts_v1(snapshot);
    Value::Array(
        counts
            .into_iter()
            .map(|(reason, count)| json!({ "reason": reason, "count": count }))
            .collect(),
    )
}

fn tx_broadcast_failure_class_counts_json(
    snapshot: &novovm_network::EthFullnodeNativeWorkerRuntimeSnapshotV1,
) -> Value {
    let counts = tx_broadcast_failure_class_counts_v1(snapshot);
    Value::Array(
        counts
            .into_iter()
            .map(|(class, count)| json!({ "class": class, "count": count }))
            .collect(),
    )
}

fn tx_broadcast_failure_phase_counts_json(
    snapshot: &novovm_network::EthFullnodeNativeWorkerRuntimeSnapshotV1,
) -> Value {
    let counts = tx_broadcast_failure_phase_counts_v1(snapshot);
    Value::Array(
        counts
            .into_iter()
            .map(|(phase, count)| json!({ "phase": phase, "count": count }))
            .collect(),
    )
}

fn tx_broadcast_failure_peer_ids_json(
    snapshot: &novovm_network::EthFullnodeNativeWorkerRuntimeSnapshotV1,
) -> Value {
    let peers = tx_broadcast_failure_peer_ids_v1(snapshot);
    Value::Array(
        peers
            .into_iter()
            .map(|peer_id| Value::String(format!("0x{:x}", peer_id)))
            .collect(),
    )
}

fn tx_broadcast_runtime_json(
    snapshot: &novovm_network::EthFullnodeNativeWorkerRuntimeSnapshotV1,
) -> Value {
    let runtime = &snapshot.native_pending_tx_broadcast_runtime;
    json!({
        "dispatchTotal": runtime.dispatch_total,
        "dispatchSuccessTotal": runtime.dispatch_success_total,
        "dispatchFailedTotal": runtime.dispatch_failed_total,
        "candidateTxTotal": runtime.candidate_tx_total,
        "broadcastTxTotal": runtime.broadcast_tx_total,
        "dispatchSuccessRateBps": ratio_bps_v1(runtime.dispatch_success_total, runtime.dispatch_total),
        "dispatchFailureRateBps": ratio_bps_v1(runtime.dispatch_failed_total, runtime.dispatch_total),
        "txDeliveryRateBps": ratio_bps_v1(runtime.broadcast_tx_total, runtime.candidate_tx_total),
        "lastPeerId": runtime.last_peer_id.map(|peer_id| format!("0x{:x}", peer_id)),
        "lastCandidateCount": runtime.last_candidate_count,
        "lastBroadcastTxCount": runtime.last_broadcast_tx_count,
        "lastUpdatedUnixMs": runtime.last_updated_unix_ms,
        "failureReasons": tx_broadcast_failure_reasons_json(snapshot),
        "failureClassCounts": tx_broadcast_failure_class_counts_json(snapshot),
        "failurePhaseCounts": tx_broadcast_failure_phase_counts_json(snapshot),
        "failurePeerIds": tx_broadcast_failure_peer_ids_json(snapshot),
    })
}

fn tx_broadcast_runtime_summary_json(
    snapshot: &novovm_network::EthFullnodeNativeWorkerRuntimeSnapshotV1,
) -> Value {
    let runtime = &snapshot.native_pending_tx_broadcast_runtime;
    json!({
        "dispatchTotal": runtime.dispatch_total,
        "dispatchSuccessTotal": runtime.dispatch_success_total,
        "dispatchFailedTotal": runtime.dispatch_failed_total,
        "candidateTxTotal": runtime.candidate_tx_total,
        "broadcastTxTotal": runtime.broadcast_tx_total,
        "dispatchSuccessRateBps": ratio_bps_v1(runtime.dispatch_success_total, runtime.dispatch_total),
        "dispatchFailureRateBps": ratio_bps_v1(runtime.dispatch_failed_total, runtime.dispatch_total),
        "txDeliveryRateBps": ratio_bps_v1(runtime.broadcast_tx_total, runtime.candidate_tx_total),
        "lastPeerId": runtime.last_peer_id.map(|peer_id| format!("0x{:x}", peer_id)),
        "lastUpdatedUnixMs": runtime.last_updated_unix_ms,
    })
}

fn available_peer_count_v1(summary: &novovm_network::EthPeerLifecycleSummaryV1) -> u64 {
    summary.ready_count.saturating_add(summary.syncing_count)
}

fn capacity_rejected_peer_count_v1(
    snapshot: &novovm_network::EthFullnodeNativeWorkerRuntimeSnapshotV1,
) -> u64 {
    snapshot
        .peer_sessions
        .iter()
        .filter(|session| {
            session.disconnect_too_many_peers_count > 0
                || session.last_disconnect_reason_code == Some(0x04)
        })
        .count() as u64
}

const PENDING_CLEANUP_PRESSURE_HIGH_THRESHOLD_V1: usize = 64;
const PENDING_CLEANUP_PRESSURE_CRITICAL_THRESHOLD_V1: usize = 256;
const PENDING_NON_RECOVERABLE_REJECTION_HIGH_THRESHOLD_V1: usize = 32;
const EXECUTION_BUDGET_PRESSURE_HIGH_THRESHOLD_V1: u64 = 1;

fn execution_budget_pressure_reasons_v1(
    snapshot: &novovm_network::EthFullnodeNativeWorkerRuntimeSnapshotV1,
) -> Vec<&'static str> {
    let runtime = &snapshot.native_execution_budget_runtime;
    if runtime.execution_budget_hit_count >= EXECUTION_BUDGET_PRESSURE_HIGH_THRESHOLD_V1
        || runtime.execution_time_slice_exceeded_count
            >= EXECUTION_BUDGET_PRESSURE_HIGH_THRESHOLD_V1
        || runtime.execution_deferred_count > 0
    {
        return vec!["execution_budget_pressure_high"];
    }
    Vec::new()
}

fn execution_budget_light_json_v1(
    snapshot: &novovm_network::EthFullnodeNativeWorkerRuntimeSnapshotV1,
) -> Value {
    let runtime = &snapshot.native_execution_budget_runtime;
    let reasons = execution_budget_pressure_reasons_v1(snapshot);
    json!({
        "executionBudgetStatus": if reasons.is_empty() { "healthy" } else { "degraded" },
        "executionBudgetReasons": reasons,
        "hardExecutionBudgetPerTick": runtime.hard_budget_per_tick,
        "hardExecutionTimeSliceMs": runtime.hard_time_slice_ms,
        "targetExecutionBudgetPerTick": runtime.target_budget_per_tick,
        "targetExecutionTimeSliceMs": runtime.target_time_slice_ms,
        "effectiveExecutionBudgetPerTick": runtime.effective_budget_per_tick,
        "effectiveExecutionTimeSliceMs": runtime.effective_time_slice_ms,
        "executionBudgetHitCount": runtime.execution_budget_hit_count,
        "executionDeferredCount": runtime.execution_deferred_count,
        "executionTimeSliceExceededCount": runtime.execution_time_slice_exceeded_count,
        "lastExecutionTargetReason": runtime.last_execution_target_reason,
        "lastExecutionThrottleReason": runtime.last_execution_throttle_reason,
        "lastUpdatedUnixMs": runtime.last_updated_unix_ms,
    })
}

fn cleanup_pressure_reasons_v1(
    snapshot: &novovm_network::EthFullnodeNativeWorkerRuntimeSnapshotV1,
) -> Vec<&'static str> {
    let summary = &snapshot.native_pending_tx_summary;
    let mut reasons = Vec::<&'static str>::new();
    let cleanup_pressure_total = summary.evicted_count.saturating_add(summary.expired_count);
    if cleanup_pressure_total >= PENDING_CLEANUP_PRESSURE_CRITICAL_THRESHOLD_V1 {
        reasons.push("pending_cleanup_pressure_critical");
    } else if cleanup_pressure_total >= PENDING_CLEANUP_PRESSURE_HIGH_THRESHOLD_V1 {
        reasons.push("pending_cleanup_pressure_high");
    }
    if summary.non_recoverable_count >= PENDING_NON_RECOVERABLE_REJECTION_HIGH_THRESHOLD_V1 {
        reasons.push("pending_non_recoverable_rejections_high");
    }
    reasons
}

fn cleanup_pressure_status_from_reasons_v1(reasons: &[&'static str]) -> &'static str {
    if reasons.contains(&"pending_cleanup_pressure_critical") {
        return "critical";
    }
    if reasons.is_empty() {
        return "healthy";
    }
    "degraded"
}

fn cleanup_pressure_light_json_v1(
    snapshot: &novovm_network::EthFullnodeNativeWorkerRuntimeSnapshotV1,
) -> Value {
    let summary = &snapshot.native_pending_tx_summary;
    let reasons = cleanup_pressure_reasons_v1(snapshot);
    json!({
        "cleanupPressureStatus": cleanup_pressure_status_from_reasons_v1(reasons.as_slice()),
        "cleanupPressureReasons": reasons,
        "nonRecoverableRejectionCount": summary.non_recoverable_count,
        "finalDispositionCounts": {
            "evicted": summary.evicted_count,
            "expired": summary.expired_count,
            "rejected": summary.rejected_count,
        },
    })
}

fn sync_degradation_reasons_v1(
    snapshot: &novovm_network::EthFullnodeNativeWorkerRuntimeSnapshotV1,
) -> Vec<&'static str> {
    let mut reasons = Vec::new();
    let summary = &snapshot.lifecycle_summary;
    let available = available_peer_count_v1(summary);
    let broadcast_runtime = &snapshot.native_pending_tx_broadcast_runtime;
    let tx_broadcast_phase_counts = tx_broadcast_failure_phase_counts_v1(snapshot);
    let broadcast_has_pending_gap =
        broadcast_runtime.candidate_tx_total > broadcast_runtime.broadcast_tx_total;
    let broadcast_has_repeated_failures = broadcast_runtime.dispatch_failed_total > 0
        && (broadcast_runtime.dispatch_failed_total >= 2
            || broadcast_runtime.dispatch_failed_total >= broadcast_runtime.dispatch_success_total);
    let broadcast_has_stall_signal = broadcast_has_pending_gap
        && (!tx_broadcast_phase_counts.is_empty()
            || (broadcast_runtime.dispatch_total > 0
                && broadcast_runtime.last_candidate_count > 0
                && broadcast_runtime.last_broadcast_tx_count == 0));
    if snapshot.candidate_peer_ids.is_empty() {
        reasons.push("no_candidate_peers");
    }
    if summary.peer_count > 0 && summary.permanently_rejected_count >= summary.peer_count {
        reasons.push("all_candidate_peers_permanently_rejected");
    }
    if available == 0 && summary.cooldown_count > 0 {
        reasons.push("all_candidate_peers_in_cooldown");
    }
    if available == 0 {
        reasons.push("no_active_sync_peers");
    }
    if broadcast_has_pending_gap && available == 0 {
        reasons.push("broadcast_no_available_peer");
    }
    if snapshot.sync_view.as_ref().is_some_and(|view| {
        matches!(
            view.source,
            novovm_network::EthFullnodeBlockViewSource::CanonicalHostBatch
        )
    }) {
        reasons.push("source_downgraded_to_canonical_host_batch");
    }
    if snapshot.native_head_body_available == Some(false) {
        reasons.push("native_head_body_unavailable");
    }
    if snapshot.failed_sync_peers > 0 {
        reasons.push("recent_sync_peer_failures");
    }
    if broadcast_has_repeated_failures {
        reasons.push("broadcast_repeated_failure");
    }
    if broadcast_has_stall_signal {
        reasons.push("broadcast_phase_stall");
    }
    for reason in execution_budget_pressure_reasons_v1(snapshot) {
        reasons.push(reason);
    }
    for reason in cleanup_pressure_reasons_v1(snapshot) {
        reasons.push(reason);
    }
    if summary.validation_reject_count > 0 {
        reasons.push("peer_chain_validation_rejects_present");
    }
    if summary.timeout_count > 0 {
        reasons.push("peer_timeouts_present");
    }
    if summary.decode_failure_count > 0 {
        reasons.push("peer_decode_failures_present");
    }
    if summary.handshake_failure_count > 0 {
        reasons.push("peer_handshake_failures_present");
    }
    if summary.connect_failure_count > 0 {
        reasons.push("peer_connect_failures_present");
    }
    if capacity_rejected_peer_count_v1(snapshot) > 0 {
        reasons.push("peer_capacity_rejections_present");
    }
    reasons.sort_unstable();
    reasons.dedup();
    reasons
}

fn sync_degradation_primary_reason_v1(reasons: &[&'static str]) -> Option<&'static str> {
    [
        "no_candidate_peers",
        "all_candidate_peers_permanently_rejected",
        "all_candidate_peers_in_cooldown",
        "no_active_sync_peers",
        "broadcast_no_available_peer",
        "pending_cleanup_pressure_critical",
        "pending_cleanup_pressure_high",
        "pending_non_recoverable_rejections_high",
        "execution_budget_pressure_high",
        "source_downgraded_to_canonical_host_batch",
        "native_head_body_unavailable",
        "recent_sync_peer_failures",
        "broadcast_repeated_failure",
        "broadcast_phase_stall",
        "peer_chain_validation_rejects_present",
        "peer_timeouts_present",
        "peer_decode_failures_present",
        "peer_handshake_failures_present",
        "peer_connect_failures_present",
        "peer_capacity_rejections_present",
    ]
    .into_iter()
    .find(|candidate| reasons.contains(candidate))
}

fn sync_degradation_reason_to_root_cause_v1(reason: &str) -> &'static str {
    match reason {
        "peer_capacity_rejections_present"
        | "all_candidate_peers_in_cooldown"
        | "broadcast_no_available_peer" => "network_capacity_issue",
        "source_downgraded_to_canonical_host_batch"
        | "native_head_body_unavailable"
        | "recent_sync_peer_failures" => "chain_gap_issue",
        "execution_budget_pressure_high" => "execution_budget_issue",
        "pending_cleanup_pressure_critical"
        | "pending_cleanup_pressure_high"
        | "pending_non_recoverable_rejections_high" => "mempool_pressure_issue",
        "broadcast_repeated_failure" | "broadcast_phase_stall" => "broadcast_path_issue",
        "no_candidate_peers"
        | "all_candidate_peers_permanently_rejected"
        | "no_active_sync_peers"
        | "peer_chain_validation_rejects_present"
        | "peer_timeouts_present"
        | "peer_decode_failures_present"
        | "peer_handshake_failures_present"
        | "peer_connect_failures_present" => "peer_quality_issue",
        _ => "peer_quality_issue",
    }
}

fn sync_degradation_root_cause_v1(
    snapshot: &novovm_network::EthFullnodeNativeWorkerRuntimeSnapshotV1,
    reasons: &[&'static str],
    primary_reason: Option<&'static str>,
) -> &'static str {
    if let Some(reason) = primary_reason {
        return sync_degradation_reason_to_root_cause_v1(reason);
    }

    let chain_gap = snapshot.sync_view.as_ref().is_some_and(|view| {
        view.highest_block_number > view.current_block_number
            || (view.syncing && view.highest_block_number > view.starting_block_number)
    });
    let has_broadcast_path = reasons.contains(&"broadcast_repeated_failure")
        || reasons.contains(&"broadcast_phase_stall");
    let has_execution_budget_issue = reasons.contains(&"execution_budget_pressure_high");
    let has_mempool_pressure = reasons.contains(&"pending_cleanup_pressure_critical")
        || reasons.contains(&"pending_cleanup_pressure_high")
        || reasons.contains(&"pending_non_recoverable_rejections_high");
    let has_network_capacity = reasons.contains(&"peer_capacity_rejections_present")
        || reasons.contains(&"all_candidate_peers_in_cooldown")
        || reasons.contains(&"broadcast_no_available_peer");
    let has_chain_gap = reasons.contains(&"source_downgraded_to_canonical_host_batch")
        || reasons.contains(&"native_head_body_unavailable")
        || reasons.contains(&"recent_sync_peer_failures")
        || chain_gap;
    if has_network_capacity {
        return "network_capacity_issue";
    }
    if has_mempool_pressure {
        return "mempool_pressure_issue";
    }
    if has_execution_budget_issue {
        return "execution_budget_issue";
    }
    if has_chain_gap {
        return "chain_gap_issue";
    }
    if has_broadcast_path {
        return "broadcast_path_issue";
    }
    "peer_quality_issue"
}

fn sync_degradation_root_cause_signals_json(
    snapshot: &novovm_network::EthFullnodeNativeWorkerRuntimeSnapshotV1,
    reasons: &[&'static str],
) -> Value {
    let chain_gap = snapshot.sync_view.as_ref().is_some_and(|view| {
        view.highest_block_number > view.current_block_number
            || (view.syncing && view.highest_block_number > view.starting_block_number)
    });
    let peer_quality_issue = reasons.contains(&"no_candidate_peers")
        || reasons.contains(&"all_candidate_peers_permanently_rejected")
        || reasons.contains(&"no_active_sync_peers")
        || reasons.contains(&"peer_chain_validation_rejects_present")
        || reasons.contains(&"peer_timeouts_present")
        || reasons.contains(&"peer_decode_failures_present")
        || reasons.contains(&"peer_handshake_failures_present")
        || reasons.contains(&"peer_connect_failures_present");
    let network_capacity_issue = reasons.contains(&"peer_capacity_rejections_present")
        || reasons.contains(&"all_candidate_peers_in_cooldown")
        || reasons.contains(&"broadcast_no_available_peer");
    let broadcast_path_issue = reasons.contains(&"broadcast_repeated_failure")
        || reasons.contains(&"broadcast_phase_stall");
    let execution_budget_issue = reasons.contains(&"execution_budget_pressure_high");
    let mempool_pressure_issue = reasons.contains(&"pending_cleanup_pressure_critical")
        || reasons.contains(&"pending_cleanup_pressure_high")
        || reasons.contains(&"pending_non_recoverable_rejections_high");
    let chain_gap_issue = reasons.contains(&"source_downgraded_to_canonical_host_batch")
        || reasons.contains(&"native_head_body_unavailable")
        || reasons.contains(&"recent_sync_peer_failures")
        || chain_gap;
    json!({
        "peerQualityIssue": peer_quality_issue,
        "networkCapacityIssue": network_capacity_issue,
        "broadcastPathIssue": broadcast_path_issue,
        "executionBudgetIssue": execution_budget_issue,
        "mempoolPressureIssue": mempool_pressure_issue,
        "chainGapIssue": chain_gap_issue,
        "chainGapDetected": chain_gap,
    })
}

fn runtime_root_cause_bundle_v1(
    snapshot: Option<&novovm_network::EthFullnodeNativeWorkerRuntimeSnapshotV1>,
) -> (
    &'static str,
    Vec<&'static str>,
    Option<&'static str>,
    Option<&'static str>,
    Value,
    Value,
) {
    if let Some(snapshot) = snapshot {
        let reasons = sync_degradation_reasons_v1(snapshot);
        let primary_reason = sync_degradation_primary_reason_v1(reasons.as_slice());
        let root_cause =
            sync_degradation_root_cause_v1(snapshot, reasons.as_slice(), primary_reason);
        let status = if reasons.is_empty() {
            "healthy"
        } else {
            "degraded"
        };
        return (
            status,
            reasons.clone(),
            primary_reason,
            Some(root_cause),
            sync_degradation_root_cause_signals_json(snapshot, reasons.as_slice()),
            sync_degradation_selection_correlation_json(snapshot),
        );
    }
    (
        "unavailable",
        vec!["runtime_snapshot_unavailable"],
        Some("runtime_snapshot_unavailable"),
        Some("runtime_snapshot_unavailable"),
        Value::Null,
        Value::Null,
    )
}

fn peer_health_status_v1(
    snapshot: &novovm_network::EthFullnodeNativeWorkerRuntimeSnapshotV1,
) -> &'static str {
    let summary = &snapshot.lifecycle_summary;
    if snapshot.candidate_peer_ids.is_empty() {
        return "unavailable";
    }
    if available_peer_count_v1(summary) == 0 {
        return "degraded";
    }
    if summary.cooldown_count > 0
        || summary.temporarily_failed_count > 0
        || summary.permanently_rejected_count > 0
        || summary.connect_failure_count > 0
        || summary.handshake_failure_count > 0
        || summary.decode_failure_count > 0
        || summary.timeout_count > 0
        || summary.validation_reject_count > 0
        || summary.disconnect_count > 0
    {
        return "degraded";
    }
    "healthy"
}

fn peer_selection_quality_status_v1(
    snapshot: &novovm_network::EthFullnodeNativeWorkerRuntimeSnapshotV1,
) -> &'static str {
    let summary = &snapshot.selection_quality_summary;
    if summary.candidate_peer_count == 0 {
        return "unavailable";
    }
    if summary.selected_sync_peers == 0 && summary.selected_bootstrap_peers == 0 {
        return "degraded";
    }
    if summary.selected_sync_peers == 0 {
        return "degraded";
    }
    if summary
        .top_selected_sync_score
        .is_some_and(|score| score < 0)
    {
        return "degraded";
    }
    "healthy"
}

fn run_mainline_runtime_query(method: &str, params: &Value) -> Result<Value> {
    match resolve_eth_fullnode_runtime_query_method(method) {
        Some(novovm_network::EthFullnodeRuntimeQueryMethod::NativeRuntimeConfig) => {
            let chain_id = runtime_snapshot_chain_id_from_params(params);
            let resolution = resolve_eth_fullnode_native_runtime_config_resolution_v1(chain_id);
            Ok(json!({
                "method": novovm_network::EthFullnodeRuntimeQueryMethod::NativeRuntimeConfig.as_str(),
                "chainId": format!("0x{:x}", chain_id),
                "found": true,
                "runtimeConfig": resolution.config,
                "runtimeConfigSource": resolution.source,
            }))
        }
        Some(novovm_network::EthFullnodeRuntimeQueryMethod::NativeWorkerRuntime) => {
            let chain_id = runtime_snapshot_chain_id_from_params(params);
            let (snapshot, source) = load_mainline_runtime_snapshot_v1(chain_id)?;
            let found = snapshot.is_some();
            Ok(json!({
                "method": novovm_network::EthFullnodeRuntimeQueryMethod::NativeWorkerRuntime.as_str(),
                "chainId": format!("0x{:x}", snapshot.as_ref().map(|value| value.chain_id).unwrap_or(chain_id)),
                "found": found,
                "source": if found { Value::String(source.to_string()) } else { Value::Null },
                "runtime": snapshot,
            }))
        }
        Some(novovm_network::EthFullnodeRuntimeQueryMethod::NativePeerRuntimeState) => {
            let chain_id = runtime_snapshot_chain_id_from_params(params);
            let (snapshot, source) = load_mainline_runtime_snapshot_v1(chain_id)?;
            let found = snapshot.is_some();
            let lifecycle_summary = snapshot
                .as_ref()
                .map(|value| value.lifecycle_summary.clone());
            let peer_sessions = snapshot.as_ref().map(|value| value.peer_sessions.clone());
            let peer_failures = snapshot.as_ref().map(|value| value.peer_failures.clone());
            let worker = snapshot.as_ref().map(|value| {
                json!({
                    "candidatePeerCount": value.candidate_peer_ids.len(),
                    "scheduledBootstrapPeers": value.scheduled_bootstrap_peers,
                    "scheduledSyncPeers": value.scheduled_sync_peers,
                    "attemptedBootstrapPeers": value.attempted_bootstrap_peers,
                    "attemptedSyncPeers": value.attempted_sync_peers,
                    "failedBootstrapPeers": value.failed_bootstrap_peers,
                    "failedSyncPeers": value.failed_sync_peers,
                    "skippedMissingEndpointPeers": value.skipped_missing_endpoint_peers,
                    "connectedPeers": value.connected_peers,
                    "readyPeers": value.ready_peers,
                    "statusUpdates": value.status_updates,
                    "headerUpdates": value.header_updates,
                    "bodyUpdates": value.body_updates,
                    "syncRequests": value.sync_requests,
                    "inboundFrames": value.inbound_frames,
                    "updatedAtUnixMs": value.updated_at_unix_ms,
                })
            });
            Ok(json!({
                "method": novovm_network::EthFullnodeRuntimeQueryMethod::NativePeerRuntimeState.as_str(),
                "chainId": format!("0x{:x}", chain_id),
                "found": found,
                "source": if found { Value::String(source.to_string()) } else { Value::Null },
                "lifecycleSummary": lifecycle_summary,
                "worker": worker,
                "txBroadcast": snapshot
                    .as_ref()
                    .map(tx_broadcast_runtime_json)
                    .unwrap_or(Value::Null),
                "executionBudget": snapshot
                    .as_ref()
                    .map(execution_budget_light_json_v1)
                    .unwrap_or(Value::Null),
                "peerSessions": peer_sessions.unwrap_or_default(),
                "recentFailures": peer_failures.unwrap_or_default(),
            }))
        }
        Some(novovm_network::EthFullnodeRuntimeQueryMethod::NativeCanonicalChainSummary) => {
            let chain_id = runtime_snapshot_chain_id_from_params(params);
            let (snapshot, source) = load_mainline_runtime_snapshot_v1(chain_id)?;
            let found = snapshot
                .as_ref()
                .and_then(|value| value.native_canonical_chain.as_ref())
                .is_some();
            let canonical_chain = snapshot
                .as_ref()
                .and_then(|value| value.native_canonical_chain.as_ref())
                .map(native_canonical_chain_to_json)
                .unwrap_or(Value::Null);
            Ok(json!({
                "method": novovm_network::EthFullnodeRuntimeQueryMethod::NativeCanonicalChainSummary.as_str(),
                "chainId": format!("0x{:x}", chain_id),
                "found": found,
                "source": if found { Value::String(source.to_string()) } else { Value::Null },
                "canonicalChain": canonical_chain,
            }))
        }
        Some(novovm_network::EthFullnodeRuntimeQueryMethod::NativeBlockLifecycleByNumber) => {
            let chain_id = runtime_snapshot_chain_id_from_params(params);
            let (snapshot, source) = load_mainline_runtime_snapshot_v1(chain_id)?;
            let block_number = match params {
                Value::Object(map) => map
                    .get("blockNumber")
                    .or_else(|| map.get("block_number"))
                    .and_then(value_as_u64),
                Value::Array(items) => items.first().and_then(value_as_u64),
                _ => None,
            }
            .ok_or_else(|| {
                anyhow::anyhow!(
                    "blockNumber is required for supervm_getEthNativeBlockLifecycleByNumber"
                )
            })?;
            let candidates = snapshot
                .as_ref()
                .map(|value| {
                    value
                        .native_canonical_blocks
                        .iter()
                        .filter(|block| block.number == block_number)
                        .cloned()
                        .collect::<Vec<_>>()
                })
                .unwrap_or_default();
            let selected = candidates
                .iter()
                .find(|block| block.canonical)
                .cloned()
                .or_else(|| candidates.first().cloned());
            Ok(json!({
                "method": novovm_network::EthFullnodeRuntimeQueryMethod::NativeBlockLifecycleByNumber.as_str(),
                "chainId": format!("0x{:x}", chain_id),
                "blockNumber": format!("0x{:x}", block_number),
                "found": selected.is_some(),
                "source": if selected.is_some() { Value::String(source.to_string()) } else { Value::Null },
                "selectionPolicy": "canonical_preferred",
                "candidateCount": candidates.len(),
                "candidateLifecycleStages": candidates.iter().map(|block| block.lifecycle_stage.as_str()).collect::<Vec<_>>(),
                "block": selected.as_ref().map(native_canonical_block_to_json).unwrap_or(Value::Null),
            }))
        }
        Some(novovm_network::EthFullnodeRuntimeQueryMethod::NativeBlockLifecycleByHash) => {
            let chain_id = runtime_snapshot_chain_id_from_params(params);
            let (snapshot, source) = load_mainline_runtime_snapshot_v1(chain_id)?;
            let block_hash = match params {
                Value::Object(map) => map
                    .get("blockHash")
                    .or_else(|| map.get("block_hash"))
                    .and_then(value_as_string),
                Value::Array(items) => items.first().and_then(value_as_string),
                _ => None,
            }
            .ok_or_else(|| {
                anyhow::anyhow!(
                    "blockHash is required for supervm_getEthNativeBlockLifecycleByHash"
                )
            })?;
            let block_hash_bytes = parse_hex_h256(&block_hash).ok_or_else(|| {
                anyhow::anyhow!("invalid blockHash for supervm_getEthNativeBlockLifecycleByHash")
            })?;
            let selected = snapshot.as_ref().and_then(|value| {
                value
                    .native_canonical_blocks
                    .iter()
                    .find(|block| block.hash == block_hash_bytes)
                    .cloned()
            });
            Ok(json!({
                "method": novovm_network::EthFullnodeRuntimeQueryMethod::NativeBlockLifecycleByHash.as_str(),
                "chainId": format!("0x{:x}", chain_id),
                "blockHash": block_hash,
                "found": selected.is_some(),
                "source": if selected.is_some() { Value::String(source.to_string()) } else { Value::Null },
                "block": selected.as_ref().map(native_canonical_block_to_json).unwrap_or(Value::Null),
            }))
        }
        Some(novovm_network::EthFullnodeRuntimeQueryMethod::NativePendingTxSummary) => {
            let chain_id = runtime_snapshot_chain_id_from_params(params);
            let (snapshot, source) = load_mainline_runtime_snapshot_v1(chain_id)?;
            let found = snapshot.is_some();
            let summary = snapshot
                .as_ref()
                .map(|value| {
                    let local_pending_count = value
                        .native_pending_txs
                        .iter()
                        .filter(|tx| {
                            matches!(
                                tx.lifecycle_stage,
                                novovm_network::NetworkRuntimeNativePendingTxLifecycleStageV1::Pending
                            ) && matches!(
                                tx.origin,
                                novovm_network::NetworkRuntimeNativePendingTxOriginV1::Local
                            )
                        })
                        .count();
                    let remote_pending_count = value
                        .native_pending_txs
                        .iter()
                        .filter(|tx| {
                            matches!(
                                tx.lifecycle_stage,
                                novovm_network::NetworkRuntimeNativePendingTxLifecycleStageV1::Pending
                            ) && matches!(
                                tx.origin,
                                novovm_network::NetworkRuntimeNativePendingTxOriginV1::Remote
                            )
                        })
                        .count();
                    let unknown_pending_count = value
                        .native_pending_txs
                        .iter()
                        .filter(|tx| {
                            matches!(
                                tx.lifecycle_stage,
                                novovm_network::NetworkRuntimeNativePendingTxLifecycleStageV1::Pending
                            ) && matches!(
                                tx.origin,
                                novovm_network::NetworkRuntimeNativePendingTxOriginV1::Unknown
                            )
                        })
                        .count();
                    json!({
                        "txCount": value.native_pending_tx_summary.tx_count,
                        "localOriginCount": value.native_pending_tx_summary.local_origin_count,
                        "remoteOriginCount": value.native_pending_tx_summary.remote_origin_count,
                        "unknownOriginCount": value.native_pending_tx_summary.unknown_origin_count,
                        "localPendingCount": local_pending_count,
                        "remotePendingCount": remote_pending_count,
                        "unknownPendingCount": unknown_pending_count,
                        "seenCount": value.native_pending_tx_summary.seen_count,
                        "pendingCount": value.native_pending_tx_summary.pending_count,
                        "propagatedCount": value.native_pending_tx_summary.propagated_count,
                        "includedCanonicalCount": value.native_pending_tx_summary.included_canonical_count,
                        "includedNonCanonicalCount": value.native_pending_tx_summary.included_non_canonical_count,
                        "reorgedBackToPendingCount": value.native_pending_tx_summary.reorged_back_to_pending_count,
                        "droppedCount": value.native_pending_tx_summary.dropped_count,
                        "rejectedCount": value.native_pending_tx_summary.rejected_count,
                        "retryEligibleCount": value.native_pending_tx_summary.retry_eligible_count,
                        "budgetSuppressedCount": value.native_pending_tx_summary.budget_suppressed_count,
                        "ioWriteFailureCount": value.native_pending_tx_summary.io_write_failure_count,
                        "nonRecoverableCount": value.native_pending_tx_summary.non_recoverable_count,
                        "propagationAttemptTotal": value.native_pending_tx_summary.propagation_attempt_total,
                        "propagationSuccessTotal": value.native_pending_tx_summary.propagation_success_total,
                        "propagationFailureTotal": value.native_pending_tx_summary.propagation_failure_total,
                        "propagatedPeerTotal": value.native_pending_tx_summary.propagated_peer_total,
                        "coverageRateBps": ratio_bps_v1(
                            value.native_pending_tx_summary.propagation_success_total,
                            value.native_pending_tx_summary.propagation_attempt_total,
                        ),
                        "broadcastDispatchTotal": value.native_pending_tx_summary.broadcast_dispatch_total,
                        "broadcastDispatchSuccessTotal": value.native_pending_tx_summary.broadcast_dispatch_success_total,
                        "broadcastDispatchFailedTotal": value.native_pending_tx_summary.broadcast_dispatch_failed_total,
                        "broadcastCandidateTxTotal": value.native_pending_tx_summary.broadcast_candidate_tx_total,
                        "broadcastTxTotal": value.native_pending_tx_summary.broadcast_tx_total,
                        "lastBroadcastPeerId": value
                            .native_pending_tx_summary
                            .last_broadcast_peer_id
                            .map(|peer_id| format!("0x{:x}", peer_id)),
                        "lastBroadcastCandidateCount": value.native_pending_tx_summary.last_broadcast_candidate_count,
                        "lastBroadcastTxCount": value.native_pending_tx_summary.last_broadcast_tx_count,
                        "lastBroadcastUnixMs": value.native_pending_tx_summary.last_broadcast_unix_ms,
                    })
                })
                .unwrap_or(Value::Null);
            let recent = snapshot
                .as_ref()
                .map(|value| {
                    let recent_limit = runtime_query_limit_v1(chain_id, None, 64, 4_096);
                    value
                        .native_pending_txs
                        .iter()
                        .take(recent_limit)
                        .map(native_pending_tx_to_json)
                        .collect::<Vec<_>>()
                })
                .unwrap_or_default();
            Ok(json!({
                "method": novovm_network::EthFullnodeRuntimeQueryMethod::NativePendingTxSummary.as_str(),
                "chainId": format!("0x{:x}", chain_id),
                "found": found,
                "source": if found { Value::String(source.to_string()) } else { Value::Null },
                "failureClassificationContract": ETH_PENDING_TX_PROPAGATION_CLASSIFICATION_CONTRACT_V1,
                "pendingTxSummary": summary,
                "recentPendingTxs": recent,
            }))
        }
        Some(novovm_network::EthFullnodeRuntimeQueryMethod::NativePendingTxPropagationSummary) => {
            let chain_id = runtime_snapshot_chain_id_from_params(params);
            let (snapshot, source) = load_mainline_runtime_snapshot_v1(chain_id)?;
            let found = snapshot.is_some();
            let propagation = snapshot
                .as_ref()
                .map(|value| {
                    let summary = &value.native_pending_tx_summary;
                    json!({
                        "pendingCount": summary.pending_count,
                        "propagatedCount": summary.propagated_count,
                        "droppedCount": summary.dropped_count,
                        "rejectedCount": summary.rejected_count,
                        "retryEligibleCount": summary.retry_eligible_count,
                        "budgetSuppressedCount": summary.budget_suppressed_count,
                        "ioWriteFailureCount": summary.io_write_failure_count,
                        "nonRecoverableCount": summary.non_recoverable_count,
                        "propagationAttemptTotal": summary.propagation_attempt_total,
                        "propagationSuccessTotal": summary.propagation_success_total,
                        "propagationFailureTotal": summary.propagation_failure_total,
                        "propagatedPeerTotal": summary.propagated_peer_total,
                        "coverageRateBps": ratio_bps_v1(
                            summary.propagation_success_total,
                            summary.propagation_attempt_total,
                        ),
                    })
                })
                .unwrap_or(Value::Null);
            Ok(json!({
                "method": novovm_network::EthFullnodeRuntimeQueryMethod::NativePendingTxPropagationSummary.as_str(),
                "chainId": format!("0x{:x}", chain_id),
                "found": found,
                "source": if found { Value::String(source.to_string()) } else { Value::Null },
                "failureClassificationContract": ETH_PENDING_TX_PROPAGATION_CLASSIFICATION_CONTRACT_V1,
                "propagationSummary": propagation,
                "txBroadcast": snapshot
                    .as_ref()
                    .map(tx_broadcast_runtime_summary_json)
                    .unwrap_or(Value::Null),
            }))
        }
        Some(novovm_network::EthFullnodeRuntimeQueryMethod::NativePendingTxByHash) => {
            let chain_id = runtime_snapshot_chain_id_from_params(params);
            let (snapshot, source) = load_mainline_runtime_snapshot_v1(chain_id)?;
            let tx_hash = match params {
                Value::Object(map) => map
                    .get("txHash")
                    .or_else(|| map.get("tx_hash"))
                    .or_else(|| map.get("hash"))
                    .and_then(value_as_string),
                Value::Array(items) => items.first().and_then(value_as_string),
                _ => None,
            }
            .ok_or_else(|| {
                anyhow::anyhow!("txHash is required for supervm_getEthNativePendingTxByHash")
            })?;
            let tx_hash_bytes = parse_hex_h256(&tx_hash).ok_or_else(|| {
                anyhow::anyhow!("invalid txHash for supervm_getEthNativePendingTxByHash")
            })?;
            let selected = snapshot.as_ref().and_then(|value| {
                value
                    .native_pending_txs
                    .iter()
                    .find(|tx| tx.tx_hash == tx_hash_bytes)
                    .cloned()
            });
            let tombstone = if selected.is_none() {
                novovm_network::get_network_runtime_native_pending_tx_tombstone_v1(
                    chain_id,
                    tx_hash_bytes,
                )
            } else {
                None
            };
            let found = selected.is_some() || tombstone.is_some();
            Ok(json!({
                "method": novovm_network::EthFullnodeRuntimeQueryMethod::NativePendingTxByHash.as_str(),
                "chainId": format!("0x{:x}", chain_id),
                "txHash": tx_hash,
                "found": found,
                "pendingTxFound": selected.is_some(),
                "tombstoneFound": tombstone.is_some(),
                "source": if found { Value::String(source.to_string()) } else { Value::Null },
                "failureClassificationContract": ETH_PENDING_TX_PROPAGATION_CLASSIFICATION_CONTRACT_V1,
                "pendingTx": selected.as_ref().map(native_pending_tx_to_json).unwrap_or(Value::Null),
                "tombstone": tombstone
                    .as_ref()
                    .map(native_pending_tx_tombstone_to_json)
                    .unwrap_or(Value::Null),
            }))
        }
        Some(novovm_network::EthFullnodeRuntimeQueryMethod::NativePendingTxTombstones) => {
            let chain_id = runtime_snapshot_chain_id_from_params(params);
            let limit =
                runtime_query_limit_v1(chain_id, param_as_u64(params, "limit", 1), 128, 4_096);
            let window_ms = param_as_u64(params, "windowMs", 2)
                .or_else(|| param_as_u64(params, "window_ms", 2))
                .unwrap_or(3_600_000) as u128;
            let now = now_unix_millis_v1();
            let budget_hooks = resolve_eth_fullnode_budget_hooks_v1(chain_id);
            let all_tombstones =
                novovm_network::snapshot_network_runtime_native_pending_tx_tombstones_v1(
                    chain_id,
                    budget_hooks.pending_tx_tombstone_retention_max.max(1) as usize,
                );
            let mut filtered = all_tombstones
                .into_iter()
                .filter(|entry| {
                    window_ms == 0 || entry.last_updated_unix_ms >= now.saturating_sub(window_ms)
                })
                .collect::<Vec<_>>();
            let tombstone_count = filtered.len();
            let evicted_count = filtered
                .iter()
                .filter(|entry| {
                    matches!(
                        entry.final_disposition,
                        novovm_network::NetworkRuntimeNativePendingTxFinalDispositionV1::Evicted
                    )
                })
                .count();
            let expired_count = filtered
                .iter()
                .filter(|entry| {
                    matches!(
                        entry.final_disposition,
                        novovm_network::NetworkRuntimeNativePendingTxFinalDispositionV1::Expired
                    )
                })
                .count();
            let dropped_count = filtered
                .iter()
                .filter(|entry| {
                    matches!(
                        entry.lifecycle_stage,
                        novovm_network::NetworkRuntimeNativePendingTxLifecycleStageV1::Dropped
                    )
                })
                .count();
            let rejected_count = filtered
                .iter()
                .filter(|entry| {
                    matches!(
                        entry.lifecycle_stage,
                        novovm_network::NetworkRuntimeNativePendingTxLifecycleStageV1::Rejected
                    )
                })
                .count();
            let stop_reason_counts = tombstone_stop_reason_counts_json_v1(&filtered);
            let top_stop_reason = top_tombstone_stop_reason_v1(&filtered);
            if filtered.len() > limit {
                filtered.truncate(limit);
            }
            let found = tombstone_count > 0;
            Ok(json!({
                "method": novovm_network::EthFullnodeRuntimeQueryMethod::NativePendingTxTombstones.as_str(),
                "chainId": format!("0x{:x}", chain_id),
                "found": found,
                "source": if found { Value::String("runtime_snapshot_memory".to_string()) } else { Value::Null },
                "failureClassificationContract": ETH_PENDING_TX_PROPAGATION_CLASSIFICATION_CONTRACT_V1,
                "windowMs": window_ms,
                "windowStartUnixMs": if window_ms == 0 { Value::Null } else { json!(now.saturating_sub(window_ms)) },
                "limit": limit,
                "tombstoneCount": tombstone_count,
                "finalDispositionCounts": {
                    "evicted": evicted_count,
                    "expired": expired_count,
                },
                "lifecycleStageCounts": {
                    "dropped": dropped_count,
                    "rejected": rejected_count,
                },
                "stopReasonCounts": stop_reason_counts,
                "topStopReason": top_stop_reason,
                "tombstones": filtered.iter().map(native_pending_tx_tombstone_to_json).collect::<Vec<_>>(),
            }))
        }
        Some(novovm_network::EthFullnodeRuntimeQueryMethod::NativePendingTxTombstoneByHash) => {
            let chain_id = runtime_snapshot_chain_id_from_params(params);
            let tx_hash = match params {
                Value::Object(map) => map
                    .get("txHash")
                    .or_else(|| map.get("tx_hash"))
                    .or_else(|| map.get("hash"))
                    .and_then(value_as_string),
                Value::Array(items) => items.first().and_then(value_as_string),
                _ => None,
            }
            .ok_or_else(|| {
                anyhow::anyhow!(
                    "txHash is required for supervm_getEthNativePendingTxTombstoneByHash"
                )
            })?;
            let tx_hash_bytes = parse_hex_h256(&tx_hash).ok_or_else(|| {
                anyhow::anyhow!("invalid txHash for supervm_getEthNativePendingTxTombstoneByHash")
            })?;
            let tombstone = novovm_network::get_network_runtime_native_pending_tx_tombstone_v1(
                chain_id,
                tx_hash_bytes,
            );
            Ok(json!({
                "method": novovm_network::EthFullnodeRuntimeQueryMethod::NativePendingTxTombstoneByHash.as_str(),
                "chainId": format!("0x{:x}", chain_id),
                "txHash": tx_hash,
                "found": tombstone.is_some(),
                "source": if tombstone.is_some() {
                    Value::String("runtime_snapshot_memory".to_string())
                } else {
                    Value::Null
                },
                "failureClassificationContract": ETH_PENDING_TX_PROPAGATION_CLASSIFICATION_CONTRACT_V1,
                "tombstone": tombstone
                    .as_ref()
                    .map(native_pending_tx_tombstone_to_json)
                    .unwrap_or(Value::Null),
            }))
        }
        Some(novovm_network::EthFullnodeRuntimeQueryMethod::NativePendingTxCleanupSummary) => {
            let chain_id = runtime_snapshot_chain_id_from_params(params);
            let window_ms = param_as_u64(params, "windowMs", 1)
                .or_else(|| param_as_u64(params, "window_ms", 1))
                .unwrap_or(3_600_000) as u128;
            let high_pressure_threshold = param_as_u64(params, "highPressureThreshold", 2)
                .or_else(|| param_as_u64(params, "high_pressure_threshold", 2))
                .unwrap_or(64);
            let critical_pressure_threshold = param_as_u64(params, "criticalPressureThreshold", 3)
                .or_else(|| param_as_u64(params, "critical_pressure_threshold", 3))
                .unwrap_or(256)
                .max(high_pressure_threshold);
            let now = now_unix_millis_v1();
            let pending_summary =
                novovm_network::snapshot_network_runtime_native_pending_tx_summary_v1(chain_id);
            let budget_hooks = resolve_eth_fullnode_budget_hooks_v1(chain_id);
            let tombstones =
                novovm_network::snapshot_network_runtime_native_pending_tx_tombstones_v1(
                    chain_id,
                    budget_hooks.pending_tx_tombstone_retention_max.max(1) as usize,
                )
                .into_iter()
                .filter(|entry| {
                    window_ms == 0 || entry.last_updated_unix_ms >= now.saturating_sub(window_ms)
                })
                .collect::<Vec<_>>();
            let removed_in_window = tombstones.len();
            let evicted_in_window = tombstones
                .iter()
                .filter(|entry| {
                    matches!(
                        entry.final_disposition,
                        novovm_network::NetworkRuntimeNativePendingTxFinalDispositionV1::Evicted
                    )
                })
                .count();
            let expired_in_window = tombstones
                .iter()
                .filter(|entry| {
                    matches!(
                        entry.final_disposition,
                        novovm_network::NetworkRuntimeNativePendingTxFinalDispositionV1::Expired
                    )
                })
                .count();
            let dropped_in_window = tombstones
                .iter()
                .filter(|entry| {
                    matches!(
                        entry.lifecycle_stage,
                        novovm_network::NetworkRuntimeNativePendingTxLifecycleStageV1::Dropped
                    )
                })
                .count();
            let rejected_in_window = tombstones
                .iter()
                .filter(|entry| {
                    matches!(
                        entry.lifecycle_stage,
                        novovm_network::NetworkRuntimeNativePendingTxLifecycleStageV1::Rejected
                    )
                })
                .count();
            let recoverable_in_window = tombstones
                .iter()
                .filter(|entry| {
                    matches!(
                        entry.propagation_recoverability,
                        Some(
                            novovm_network::NetworkRuntimeNativePendingTxPropagationRecoverabilityV1::Recoverable
                        )
                    )
                })
                .count();
            let non_recoverable_in_window = tombstones
                .iter()
                .filter(|entry| {
                    matches!(
                        entry.propagation_recoverability,
                        Some(
                            novovm_network::NetworkRuntimeNativePendingTxPropagationRecoverabilityV1::NonRecoverable
                        )
                    )
                })
                .count();
            let stop_reason_counts = tombstone_stop_reason_counts_json_v1(&tombstones);
            let top_stop_reason = top_tombstone_stop_reason_v1(&tombstones);
            let pressure_status = cleanup_pressure_status_v1(
                removed_in_window,
                high_pressure_threshold,
                critical_pressure_threshold,
            );
            Ok(json!({
                "method": novovm_network::EthFullnodeRuntimeQueryMethod::NativePendingTxCleanupSummary.as_str(),
                "chainId": format!("0x{:x}", chain_id),
                "failureClassificationContract": ETH_PENDING_TX_PROPAGATION_CLASSIFICATION_CONTRACT_V1,
                "windowMs": window_ms,
                "windowStartUnixMs": if window_ms == 0 { Value::Null } else { json!(now.saturating_sub(window_ms)) },
                "removedInWindowCount": removed_in_window,
                "cleanupPressure": {
                    "status": pressure_status,
                    "abnormalHigh": removed_in_window as u64 >= high_pressure_threshold,
                    "highPressureThreshold": high_pressure_threshold,
                    "criticalPressureThreshold": critical_pressure_threshold,
                },
                "windowFinalDispositionCounts": {
                    "evicted": evicted_in_window,
                    "expired": expired_in_window,
                },
                "windowLifecycleStageCounts": {
                    "dropped": dropped_in_window,
                    "rejected": rejected_in_window,
                },
                "windowRecoverabilityCounts": {
                    "recoverable": recoverable_in_window,
                    "nonRecoverable": non_recoverable_in_window,
                    "unknown": removed_in_window.saturating_sub(recoverable_in_window + non_recoverable_in_window),
                },
                "windowStopReasonCounts": stop_reason_counts,
                "topStopReason": top_stop_reason,
                "pendingRuntime": {
                    "pendingCount": pending_summary.pending_count,
                    "propagatedCount": pending_summary.propagated_count,
                    "droppedCount": pending_summary.dropped_count,
                    "rejectedCount": pending_summary.rejected_count,
                    "retryEligibleCount": pending_summary.retry_eligible_count,
                    "evictedCount": pending_summary.evicted_count,
                    "expiredCount": pending_summary.expired_count,
                },
            }))
        }
        Some(novovm_network::EthFullnodeRuntimeQueryMethod::NativePendingTxBroadcastCandidates) => {
            let chain_id = runtime_snapshot_chain_id_from_params(params);
            let budget_hooks = resolve_eth_fullnode_budget_hooks_v1(chain_id);
            let limit = runtime_query_limit_v1(
                chain_id,
                params.get("limit").and_then(Value::as_u64),
                32,
                2_048,
            );
            let max_propagation_count = params
                .get("maxPropagationCount")
                .and_then(Value::as_u64)
                .unwrap_or(budget_hooks.tx_broadcast_max_propagations.max(1))
                .clamp(1, budget_hooks.tx_broadcast_max_propagations.max(1));
            let broadcast_runtime =
                novovm_network::snapshot_network_runtime_native_pending_tx_broadcast_runtime_summary_v1(
                    chain_id,
                );
            let candidates =
                novovm_network::snapshot_network_runtime_native_pending_tx_broadcast_candidates_v1(
                    chain_id,
                    limit,
                    max_propagation_count,
                );
            let out = candidates
                .iter()
                .map(|candidate| {
                    json!({
                        "txHash": to_hex_prefixed(&candidate.tx_hash),
                        "lifecycleStage": candidate.lifecycle_stage.as_str(),
                        "propagationCount": candidate.propagation_count,
                        "ingressCount": candidate.ingress_count,
                        "lastUpdatedUnixMs": candidate.last_updated_unix_ms,
                        "payloadSize": candidate.tx_payload_len,
                    })
                })
                .collect::<Vec<_>>();
            Ok(json!({
                "method": novovm_network::EthFullnodeRuntimeQueryMethod::NativePendingTxBroadcastCandidates.as_str(),
                "chainId": format!("0x{:x}", chain_id),
                "found": !out.is_empty(),
                "limit": limit,
                "maxPropagationCount": max_propagation_count,
                "candidateCount": out.len(),
                "broadcastRuntime": {
                    "dispatchTotal": broadcast_runtime.dispatch_total,
                    "dispatchSuccessTotal": broadcast_runtime.dispatch_success_total,
                    "dispatchFailedTotal": broadcast_runtime.dispatch_failed_total,
                    "candidateTxTotal": broadcast_runtime.candidate_tx_total,
                    "broadcastTxTotal": broadcast_runtime.broadcast_tx_total,
                    "lastPeerId": broadcast_runtime
                        .last_peer_id
                        .map(|peer_id| format!("0x{:x}", peer_id)),
                    "lastCandidateCount": broadcast_runtime.last_candidate_count,
                    "lastBroadcastTxCount": broadcast_runtime.last_broadcast_tx_count,
                    "lastUpdatedUnixMs": broadcast_runtime.last_updated_unix_ms,
                },
                "candidates": out,
            }))
        }
        Some(novovm_network::EthFullnodeRuntimeQueryMethod::NativeSyncRuntimeSummary) => {
            let chain_id = runtime_snapshot_chain_id_from_params(params);
            let runtime_resolution =
                resolve_eth_fullnode_native_runtime_config_resolution_v1(chain_id);
            let runtime_config = runtime_resolution.config.clone();
            let (snapshot, source) = load_mainline_runtime_snapshot_v1(chain_id)?;
            let found = snapshot.is_some();
            let (
                degradation_status,
                _degradation_reasons,
                primary_reason,
                root_cause,
                root_cause_signals,
                selection_correlation,
            ) = runtime_root_cause_bundle_v1(snapshot.as_ref());
            let head = snapshot.as_ref().and_then(|value| {
                value.head_view.as_ref().map(|head| {
                    json!({
                        "blockNumber": format!("0x{:x}", head.block_number),
                        "blockHash": to_hex_prefixed(&head.block_hash),
                        "parentBlockHash": to_hex_prefixed(&head.parent_block_hash),
                        "stateRoot": to_hex_prefixed(&head.state_root),
                        "stateVersion": format!("0x{:x}", head.state_version),
                        "chainId": format!("0x{:x}", head.chain_id),
                        "blockViewSource": head.source.as_str(),
                        "bodyAvailable": value.native_head_body_available,
                        "canonical": value.native_head_canonical,
                        "safe": value.native_head_safe,
                        "finalized": value.native_head_finalized,
                    })
                })
            });
            let sync =
                snapshot.as_ref().and_then(|value| {
                    value.sync_view.as_ref().map(|sync_view| json!({
                    "startingBlock": format!("0x{:x}", sync_view.starting_block_number),
                    "currentBlock": format!("0x{:x}", sync_view.current_block_number),
                    "highestBlock": format!("0x{:x}", sync_view.highest_block_number),
                    "currentBlockHash": to_hex_prefixed(&sync_view.current_block_hash),
                    "parentBlockHash": to_hex_prefixed(&sync_view.parent_block_hash),
                    "currentStateRoot": to_hex_prefixed(&sync_view.current_state_root),
                    "currentStateVersion": format!("0x{:x}", sync_view.current_state_version),
                    "peerCount": format!("0x{:x}", sync_view.peer_count),
                    "chainId": format!("0x{:x}", sync_view.chain_id),
                    "blockViewSource": sync_view.source.as_str(),
                    "nativeSyncPhase": sync_view.native_sync_phase,
                    "syncing": sync_view.syncing,
                }))
                });
            let worker = snapshot.as_ref().map(|value| {
                json!({
                    "updatedAtUnixMs": value.updated_at_unix_ms,
                    "syncRequests": value.sync_requests,
                    "headerUpdates": value.header_updates,
                    "bodyUpdates": value.body_updates,
                    "inboundFrames": value.inbound_frames,
                })
            });
            Ok(json!({
                "method": novovm_network::EthFullnodeRuntimeQueryMethod::NativeSyncRuntimeSummary.as_str(),
                "schema": ETH_NATIVE_SYNC_RUNTIME_SUMMARY_SCHEMA_V1,
                "schemaVersion": ETH_NATIVE_RUNTIME_QUERY_SCHEMA_VERSION_V1,
                "chainId": format!("0x{:x}", chain_id),
                "found": found,
                "source": if found { Value::String(source.to_string()) } else { Value::Null },
                "head": head.unwrap_or(Value::Null),
                "sync": sync.unwrap_or(Value::Null),
                "worker": worker.unwrap_or(Value::Null),
                "degradationStatus": degradation_status,
                "primaryReason": primary_reason,
                "rootCause": root_cause,
                "rootCauseSignals": root_cause_signals,
                "selectionCorrelation": selection_correlation,
                "failureClassificationContracts": {
                    "execution": ETH_EXEC_FAILURE_CLASSIFICATION_CONTRACT_V1,
                    "pendingTxPropagation": ETH_PENDING_TX_PROPAGATION_CLASSIFICATION_CONTRACT_V1,
                },
                "cleanupPressure": snapshot
                    .as_ref()
                    .map(cleanup_pressure_light_json_v1)
                    .unwrap_or(Value::Null),
                "executionBudget": snapshot
                    .as_ref()
                    .map(execution_budget_light_json_v1)
                    .unwrap_or(Value::Null),
                "txBroadcast": snapshot
                    .as_ref()
                    .map(tx_broadcast_runtime_summary_json)
                    .unwrap_or(Value::Null),
                "canonicalChain": snapshot
                    .as_ref()
                    .and_then(|value| value.native_canonical_chain.as_ref().map(native_canonical_chain_to_json))
                    .unwrap_or(Value::Null),
                "runtimeConfig": snapshot
                    .as_ref()
                    .map(|value| json!(value.runtime_config))
                    .unwrap_or_else(|| json!(runtime_config)),
                "runtimeConfigSource": json!(runtime_resolution.source),
                "lifecycleSummary": snapshot.as_ref().map(|value| value.lifecycle_summary.clone()),
            }))
        }
        Some(novovm_network::EthFullnodeRuntimeQueryMethod::NativePeerHealthSummary) => {
            let chain_id = runtime_snapshot_chain_id_from_params(params);
            let runtime_resolution =
                resolve_eth_fullnode_native_runtime_config_resolution_v1(chain_id);
            let runtime_config = runtime_resolution.config.clone();
            let (snapshot, source) = load_mainline_runtime_snapshot_v1(chain_id)?;
            let found = snapshot.is_some();
            let (
                status,
                candidate_peer_count,
                available_peer_count,
                capacity_rejected_count,
                primary_reason,
                root_cause,
                root_cause_signals,
                selection_correlation,
            ) = if let Some(snapshot) = snapshot.as_ref() {
                let (
                    _degradation_status,
                    _degradation_reasons,
                    primary_reason,
                    root_cause,
                    root_cause_signals,
                    selection_correlation,
                ) = runtime_root_cause_bundle_v1(Some(snapshot));
                let status = peer_health_status_v1(snapshot);
                (
                    status,
                    snapshot.candidate_peer_ids.len() as u64,
                    available_peer_count_v1(&snapshot.lifecycle_summary),
                    capacity_rejected_peer_count_v1(snapshot),
                    primary_reason,
                    root_cause,
                    root_cause_signals,
                    selection_correlation,
                )
            } else {
                (
                    "unavailable",
                    0,
                    0,
                    0,
                    Some("runtime_snapshot_unavailable"),
                    Some("runtime_snapshot_unavailable"),
                    Value::Null,
                    Value::Null,
                )
            };
            Ok(json!({
                "method": novovm_network::EthFullnodeRuntimeQueryMethod::NativePeerHealthSummary.as_str(),
                "schema": ETH_NATIVE_PEER_HEALTH_SUMMARY_SCHEMA_V1,
                "schemaVersion": ETH_NATIVE_RUNTIME_QUERY_SCHEMA_VERSION_V1,
                "chainId": format!("0x{:x}", chain_id),
                "found": found,
                "source": if found { Value::String(source.to_string()) } else { Value::Null },
                "status": status,
                "primaryReason": primary_reason,
                "rootCause": root_cause,
                "rootCauseSignals": root_cause_signals,
                "selectionCorrelation": selection_correlation,
                "failureClassificationContracts": {
                    "execution": ETH_EXEC_FAILURE_CLASSIFICATION_CONTRACT_V1,
                    "pendingTxPropagation": ETH_PENDING_TX_PROPAGATION_CLASSIFICATION_CONTRACT_V1,
                },
                "cleanupPressure": snapshot
                    .as_ref()
                    .map(cleanup_pressure_light_json_v1)
                    .unwrap_or(Value::Null),
                "executionBudget": snapshot
                    .as_ref()
                    .map(execution_budget_light_json_v1)
                    .unwrap_or(Value::Null),
                "candidatePeerCount": candidate_peer_count,
                "availablePeerCount": available_peer_count,
                "capacityRejectedPeerCount": capacity_rejected_count,
                "lifecycleSummary": snapshot.as_ref().map(|value| value.lifecycle_summary.clone()),
                "stageCounts": snapshot
                    .as_ref()
                    .map(|value| lifecycle_stage_counts_json(&value.lifecycle_summary))
                    .unwrap_or(Value::Null),
                "failureClassCounts": snapshot
                    .as_ref()
                    .map(|value| failure_class_counts_json(&value.lifecycle_summary))
                    .unwrap_or(Value::Null),
                "selectionQualitySummary": snapshot
                    .as_ref()
                    .map(|value| json!(value.selection_quality_summary))
                    .unwrap_or(Value::Null),
                "selectionLongTermSummary": snapshot
                    .as_ref()
                    .map(|value| json!(value.selection_long_term_summary))
                    .unwrap_or(Value::Null),
                "selectionWindowPolicy": snapshot
                    .as_ref()
                    .map(|value| json!(value.selection_window_policy))
                    .unwrap_or_else(|| json!(runtime_config.selection_window_policy)),
                "runtimeConfig": snapshot
                    .as_ref()
                    .map(|value| json!(value.runtime_config))
                    .unwrap_or_else(|| json!(runtime_config)),
                "runtimeConfigSource": json!(runtime_resolution.source),
                "lastFailureReasons": snapshot
                    .as_ref()
                    .map(last_failure_reasons_json)
                    .unwrap_or_else(|| Value::Array(Vec::new())),
                "txBroadcast": snapshot
                    .as_ref()
                    .map(tx_broadcast_runtime_summary_json)
                    .unwrap_or(Value::Null),
                "worker": snapshot.as_ref().map(|value| json!({
                    "scheduledBootstrapPeers": value.scheduled_bootstrap_peers,
                    "scheduledSyncPeers": value.scheduled_sync_peers,
                    "attemptedBootstrapPeers": value.attempted_bootstrap_peers,
                    "attemptedSyncPeers": value.attempted_sync_peers,
                    "failedBootstrapPeers": value.failed_bootstrap_peers,
                    "failedSyncPeers": value.failed_sync_peers,
                    "updatedAtUnixMs": value.updated_at_unix_ms,
                })).unwrap_or(Value::Null),
            }))
        }
        Some(novovm_network::EthFullnodeRuntimeQueryMethod::NativeSyncDegradationSummary) => {
            let chain_id = runtime_snapshot_chain_id_from_params(params);
            let runtime_resolution =
                resolve_eth_fullnode_native_runtime_config_resolution_v1(chain_id);
            let runtime_config = runtime_resolution.config.clone();
            let (snapshot, source) = load_mainline_runtime_snapshot_v1(chain_id)?;
            let found = snapshot.is_some();
            let (
                status,
                reasons,
                primary_reason,
                root_cause,
                root_cause_signals,
                selection_correlation,
            ) = runtime_root_cause_bundle_v1(snapshot.as_ref());
            Ok(json!({
                "method": novovm_network::EthFullnodeRuntimeQueryMethod::NativeSyncDegradationSummary.as_str(),
                "schema": ETH_NATIVE_SYNC_DEGRADATION_SUMMARY_SCHEMA_V1,
                "schemaVersion": ETH_NATIVE_RUNTIME_QUERY_SCHEMA_VERSION_V1,
                "chainId": format!("0x{:x}", chain_id),
                "found": found,
                "source": if found { Value::String(source.to_string()) } else { Value::Null },
                "status": status,
                "primaryReason": primary_reason,
                "reasons": reasons.clone(),
                "rootCause": root_cause,
                "rootCauseSignals": root_cause_signals.clone(),
                "selectionCorrelation": selection_correlation.clone(),
                "failureClassificationContracts": {
                    "execution": ETH_EXEC_FAILURE_CLASSIFICATION_CONTRACT_V1,
                    "pendingTxPropagation": ETH_PENDING_TX_PROPAGATION_CLASSIFICATION_CONTRACT_V1,
                },
                "head": snapshot.as_ref().and_then(|value| value.head_view.as_ref().map(head_view_to_eth_json)).unwrap_or(Value::Null),
                "sync": snapshot.as_ref().and_then(|value| value.sync_view.as_ref().map(sync_view_to_eth_json)).unwrap_or(Value::Null),
                "lifecycleSummary": snapshot.as_ref().map(|value| value.lifecycle_summary.clone()).unwrap_or_default(),
                "crossLayer": snapshot.as_ref().map(|value| json!({
                    "rootCauseSignals": root_cause_signals,
                    "broadcastFailurePeerCorrelations": tx_broadcast_failure_peer_selection_correlation_json(value),
                    "selectionCorrelation": selection_correlation,
                    "txBroadcast": tx_broadcast_runtime_json(value),
                    "executionBudget": execution_budget_light_json_v1(value),
                })).unwrap_or(Value::Null),
                "context": snapshot.as_ref().map(|value| json!({
                    "candidatePeerCount": value.candidate_peer_ids.len(),
                    "availablePeerCount": available_peer_count_v1(&value.lifecycle_summary),
                    "cooldownCount": value.lifecycle_summary.cooldown_count,
                    "permanentlyRejectedCount": value.lifecycle_summary.permanently_rejected_count,
                    "failedSyncPeers": value.failed_sync_peers,
                    "bodyAvailable": value.native_head_body_available,
                    "updatedAtUnixMs": value.updated_at_unix_ms,
                    "selectionQualitySummary": value.selection_quality_summary,
                    "runtimeConfig": value.runtime_config,
                })).unwrap_or(Value::Null),
                "executionBudget": snapshot
                    .as_ref()
                    .map(execution_budget_light_json_v1)
                    .unwrap_or(Value::Null),
                "runtimeConfig": snapshot
                    .as_ref()
                    .map(|value| json!(value.runtime_config))
                    .unwrap_or_else(|| json!(runtime_config)),
                "runtimeConfigSource": json!(runtime_resolution.source),
            }))
        }
        Some(novovm_network::EthFullnodeRuntimeQueryMethod::NativePeerSelectionSummary) => {
            let chain_id = runtime_snapshot_chain_id_from_params(params);
            let runtime_resolution =
                resolve_eth_fullnode_native_runtime_config_resolution_v1(chain_id);
            let runtime_config = runtime_resolution.config.clone();
            let (snapshot, source) = load_mainline_runtime_snapshot_v1(chain_id)?;
            let found = snapshot.is_some();
            let selection_scores = snapshot
                .as_ref()
                .map(|value| value.peer_selection_scores.clone())
                .unwrap_or_default();
            let selected_bootstrap = selection_scores
                .iter()
                .filter(|score| {
                    score.selected
                        && matches!(
                            score.role,
                            novovm_network::EthPeerSelectionRoleV1::Bootstrap
                        )
                })
                .cloned()
                .collect::<Vec<_>>();
            let selected_sync = selection_scores
                .iter()
                .filter(|score| {
                    score.selected
                        && matches!(score.role, novovm_network::EthPeerSelectionRoleV1::Sync)
                })
                .cloned()
                .collect::<Vec<_>>();
            let status = snapshot
                .as_ref()
                .map(peer_selection_quality_status_v1)
                .unwrap_or("unavailable");
            Ok(json!({
                "method": novovm_network::EthFullnodeRuntimeQueryMethod::NativePeerSelectionSummary.as_str(),
                "chainId": format!("0x{:x}", chain_id),
                "found": found,
                "source": if found { Value::String(source.to_string()) } else { Value::Null },
                "status": status,
                "selectionQualitySummary": snapshot
                    .as_ref()
                    .map(|value| value.selection_quality_summary.clone())
                    .unwrap_or_default(),
                "selectionLongTermSummary": snapshot
                    .as_ref()
                    .map(|value| value.selection_long_term_summary.clone())
                    .unwrap_or_default(),
                "selectionWindowPolicy": snapshot
                    .as_ref()
                    .map(|value| value.selection_window_policy.clone())
                    .unwrap_or_else(|| runtime_config.selection_window_policy.clone()),
                "runtimeConfig": snapshot
                    .as_ref()
                    .map(|value| value.runtime_config.clone())
                    .unwrap_or(runtime_config),
                "runtimeConfigSource": runtime_resolution.source,
                "selectedBootstrapPeers": selected_bootstrap,
                "selectedSyncPeers": selected_sync,
                "selectionScores": selection_scores,
            }))
        }
        None => bail!("unsupported mainline runtime query method: {method}"),
    }
}

pub fn run_mainline_query_from_path(path: &Path, method: &str, params: &Value) -> Result<Value> {
    if is_mainline_runtime_query_method(method) {
        return run_mainline_runtime_query(method, params);
    }
    let store = load_mainline_canonical_store(path)?;
    run_mainline_query(&store, method, params)
}

pub fn run_mainline_query_from_env(method: &str, params: &Value) -> Result<Value> {
    if is_mainline_runtime_query_method(method) {
        return run_mainline_runtime_query(method, params);
    }
    let path = canonical_store_path_from_env();
    run_mainline_query_from_path(&path, method, params)
}

pub fn run_mainline_query(
    store: &MainlineCanonicalStoreV1,
    method: &str,
    params: &Value,
) -> Result<Value> {
    if is_mainline_runtime_query_method(method) {
        return run_mainline_runtime_query(method, params);
    }
    let block_contexts = derive_mainline_eth_fullnode_block_contexts_v1(store);
    let chain_view = derive_mainline_eth_fullnode_chain_view_v1(store);
    let head_view = chain_view.as_ref().map(derive_eth_fullnode_head_view_v1);
    let (runtime_snapshot, runtime_snapshot_source) =
        load_mainline_runtime_snapshot_v1(store.chain_id)?;
    match resolve_eth_fullnode_canonical_query_method(method) {
        Some(novovm_network::EthFullnodeCanonicalQueryMethod::BlockNumber) => Ok(json!({
            "method": "eth_blockNumber",
            "result": format!("0x{:x}", head_view.as_ref().map(|head| head.block_number).unwrap_or(0)),
            "head": head_view.as_ref().map(head_view_to_eth_json),
        })),
        Some(novovm_network::EthFullnodeCanonicalQueryMethod::GetBlockByNumber) => {
            let latest_block_number = chain_view
                .as_ref()
                .map(|view| view.current_block_number)
                .unwrap_or(0);
            let selector = param_as_block_selector(params, "blockNumber", 0)
                .or_else(|| param_as_block_selector(params, "block_number", 0))
                .unwrap_or_else(|| "latest".to_string());
            let requested_block_number =
                resolve_block_number_selector(Some(selector.clone()), latest_block_number)
                    .ok_or_else(|| {
                        anyhow::anyhow!("invalid block selector for eth_getBlockByNumber")
                    })?;
            let full_transactions = param_as_bool(params, "fullTransactions", 1)
                .or_else(|| param_as_bool(params, "hydrateTransactions", 1))
                .unwrap_or(false);
            if let Some((batch, block_context)) = store
                .batches
                .iter()
                .zip(block_contexts.iter())
                .find(|(_, block_context)| block_context.block_number == requested_block_number)
            {
                let native_lifecycle = resolve_native_block_lifecycle_for_block_context_v1(
                    runtime_snapshot.as_ref(),
                    runtime_snapshot_source,
                    block_context,
                );
                let block = block_context_to_eth_json(
                    block_context,
                    &batch.receipts,
                    full_transactions,
                    &native_lifecycle,
                );
                return Ok(json!({
                    "method": "eth_getBlockByNumber",
                    "selector": selector,
                    "found": true,
                    "result": block.clone(),
                    "block": block,
                    "head": head_view.as_ref().map(head_view_to_eth_json),
                }));
            }
            Ok(json!({
                "method": "eth_getBlockByNumber",
                "selector": selector,
                "found": false,
                "result": Value::Null,
                "block": Value::Null,
                "head": head_view.as_ref().map(head_view_to_eth_json),
            }))
        }
        Some(novovm_network::EthFullnodeCanonicalQueryMethod::GetBlockByHash) => {
            let block_hash = param_as_block_selector(params, "blockHash", 0)
                .or_else(|| param_as_block_selector(params, "block_hash", 0))
                .ok_or_else(|| anyhow::anyhow!("blockHash is required for eth_getBlockByHash"))?;
            let full_transactions = param_as_bool(params, "fullTransactions", 1)
                .or_else(|| param_as_bool(params, "hydrateTransactions", 1))
                .unwrap_or(false);
            if let Some((batch, block_context)) = store
                .batches
                .iter()
                .zip(block_contexts.iter())
                .find(|(_, block_context)| to_hex_prefixed(&block_context.block_hash) == block_hash)
            {
                let native_lifecycle = resolve_native_block_lifecycle_for_block_context_v1(
                    runtime_snapshot.as_ref(),
                    runtime_snapshot_source,
                    block_context,
                );
                let block = block_context_to_eth_json(
                    block_context,
                    &batch.receipts,
                    full_transactions,
                    &native_lifecycle,
                );
                return Ok(json!({
                    "method": "eth_getBlockByHash",
                    "blockHash": block_hash,
                    "found": true,
                    "result": block.clone(),
                    "block": block,
                    "head": head_view.as_ref().map(head_view_to_eth_json),
                }));
            }
            Ok(json!({
                "method": "eth_getBlockByHash",
                "blockHash": block_hash,
                "found": false,
                "result": Value::Null,
                "block": Value::Null,
                "head": head_view.as_ref().map(head_view_to_eth_json),
            }))
        }
        Some(novovm_network::EthFullnodeCanonicalQueryMethod::Syncing) => {
            let sync_view = derive_eth_fullnode_sync_view_v1(
                chain_view.as_ref(),
                get_network_runtime_sync_status(store.chain_id),
                get_network_runtime_native_sync_status(store.chain_id),
            );
            let result = sync_view
                .as_ref()
                .map(sync_view_to_eth_json)
                .filter(|value| value["syncing"].as_bool().unwrap_or(false))
                .unwrap_or(Value::Bool(false));
            Ok(json!({
                "method": "eth_syncing",
                "result": result,
                "syncView": sync_view.as_ref().map(sync_view_to_eth_json),
                "head": head_view.as_ref().map(head_view_to_eth_json),
            }))
        }
        Some(novovm_network::EthFullnodeCanonicalQueryMethod::GetTransactionReceipt) => {
            let tx_hash = param_as_string(params, "tx_hash")
                .or_else(|| param_as_string(params, "hash"))
                .unwrap_or_default();
            if tx_hash.is_empty() {
                bail!("tx_hash is required for eth_getTransactionReceipt");
            }
            for (batch, block_context) in store.batches.iter().zip(block_contexts.iter()) {
                for receipt in &batch.receipts {
                    let receipt_tx_hash = to_hex_prefixed(&receipt.tx_hash);
                    if receipt_tx_hash == tx_hash {
                        let native_lifecycle = resolve_native_block_lifecycle_for_block_context_v1(
                            runtime_snapshot.as_ref(),
                            runtime_snapshot_source,
                            block_context,
                        );
                        return Ok(json!({
                            "method": "eth_getTransactionReceipt",
                            "tx_hash": tx_hash,
                            "found": true,
                            "receipt": receipt_to_eth_json(block_context, receipt, &native_lifecycle),
                        }));
                    }
                }
            }
            Ok(json!({
                "method": "eth_getTransactionReceipt",
                "tx_hash": tx_hash,
                "found": false,
                "receipt": Value::Null,
            }))
        }
        Some(novovm_network::EthFullnodeCanonicalQueryMethod::GetLogs) => {
            let latest_block_number = block_contexts
                .last()
                .map(|context| context.block_number)
                .unwrap_or(0);
            let address_filter = normalize_addresses_filter(params);
            let topics_filter = normalize_topics_filter(params);
            let mut out = Vec::new();
            for (batch, block_context) in store.batches.iter().zip(block_contexts.iter()) {
                if !block_context_matches_filter(params, block_context, latest_block_number) {
                    continue;
                }
                let native_lifecycle = resolve_native_block_lifecycle_for_block_context_v1(
                    runtime_snapshot.as_ref(),
                    runtime_snapshot_source,
                    block_context,
                );
                for receipt in &batch.receipts {
                    if !receipt.status_ok {
                        continue;
                    }
                    for log in &receipt.logs {
                        let emitter = to_hex_prefixed(&log.emitter);
                        if let Some(addresses) = address_filter.as_ref() {
                            if !addresses.iter().any(|candidate| candidate == &emitter) {
                                continue;
                            }
                        }
                        if !batch_log_matches_topics(log, &topics_filter) {
                            continue;
                        }
                        out.push(log_to_eth_json(
                            block_context,
                            receipt,
                            log,
                            &native_lifecycle,
                        ));
                    }
                }
            }
            Ok(json!({
                "method": "eth_getLogs",
                "count": out.len(),
                "logs": out,
            }))
        }
        None => bail!("unsupported mainline query method: {method}"),
    }
}

pub fn default_mainline_query_store_path() -> PathBuf {
    canonical_store_path_from_env()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::mainline_canonical::MainlineCanonicalBatchRecordV1;
    use anyhow::Context;
    use novovm_adapter_api::{ChainAdapter, ChainConfig, ChainType, StateIR, TxIR, TxType};
    use novovm_adapter_novovm::{
        address_from_seed_v1, signature_payload_with_seed_v1, NovoVmAdapter,
    };
    use novovm_network::{
        clear_eth_fullnode_native_worker_runtime_snapshot_for_chain_v1,
        set_eth_fullnode_native_worker_runtime_snapshot_v1, set_network_runtime_sync_status,
        EthFullnodeNativePeerFailureSnapshotV1, EthFullnodeNativeWorkerRuntimeSnapshotV1,
        EthPeerLifecycleSummaryV1, NetworkRuntimeNativeSyncPhaseV1,
        NetworkRuntimeNativeSyncStatusV1, NetworkRuntimeSyncStatus,
    };
    use serde::Deserialize;
    use sha2::{Digest, Sha256};
    use std::{fs, path::PathBuf};

    fn sample_store() -> MainlineCanonicalStoreV1 {
        MainlineCanonicalStoreV1 {
            schema: "supervm-mainline-canonical/v1".to_string(),
            generated_unix_ms: 1,
            chain_type: "evm".to_string(),
            chain_id: 1,
            batches: vec![MainlineCanonicalBatchRecordV1 {
                seq: 5,
                source_detail: "sample".to_string(),
                tx_count: 1,
                tap_requested: 1,
                tap_accepted: 1,
                tap_dropped: 0,
                apply_verified: true,
                apply_applied: true,
                apply_state_root: [0x11; 32],
                exported_receipt_count: 1,
                mirrored_receipt_count: 1,
                state_version: 9,
                ingress_bypassed: true,
                atomic_guard_enabled: false,
                receipts: vec![SupervmEvmExecutionReceiptV1 {
                    chain_type: ChainType::EVM,
                    chain_id: 1,
                    tx_hash: vec![0x22; 32],
                    tx_index: 0,
                    tx_type: TxType::Transfer,
                    receipt_type: Some(2),
                    status_ok: true,
                    gas_used: 21_000,
                    cumulative_gas_used: 21_000,
                    effective_gas_price: Some(7),
                    log_bloom: vec![0x33; 256],
                    revert_data: None,
                    state_root: [0x44; 32],
                    state_version: 9,
                    contract_address: None,
                    logs: vec![SupervmEvmExecutionLogV1 {
                        emitter: vec![0x55; 20],
                        topics: vec![[0x66; 32]],
                        data: vec![0x77, 0x88],
                        tx_index: 0,
                        log_index: 0,
                        state_version: 9,
                    }],
                }],
                state_mirror_updates: Vec::new(),
            }],
        }
    }

    fn rewrite_store_chain_id_v1(store: &mut MainlineCanonicalStoreV1, chain_id: u64) {
        store.chain_id = chain_id;
        for batch in &mut store.batches {
            for receipt in &mut batch.receipts {
                receipt.chain_id = chain_id;
            }
        }
    }

    fn sample_store_with_chain_id(chain_id: u64) -> MainlineCanonicalStoreV1 {
        let mut store = sample_store();
        rewrite_store_chain_id_v1(&mut store, chain_id);
        store
    }

    fn sample_store_with_failed_receipts() -> MainlineCanonicalStoreV1 {
        MainlineCanonicalStoreV1 {
            schema: "supervm-mainline-canonical/v1".to_string(),
            generated_unix_ms: 1,
            chain_type: "evm".to_string(),
            chain_id: 1,
            batches: vec![MainlineCanonicalBatchRecordV1 {
                seq: 5,
                source_detail: "failed_receipts".to_string(),
                tx_count: 2,
                tap_requested: 2,
                tap_accepted: 2,
                tap_dropped: 0,
                apply_verified: true,
                apply_applied: true,
                apply_state_root: [0x21; 32],
                exported_receipt_count: 2,
                mirrored_receipt_count: 2,
                state_version: 10,
                ingress_bypassed: true,
                atomic_guard_enabled: false,
                receipts: vec![
                    SupervmEvmExecutionReceiptV1 {
                        chain_type: ChainType::EVM,
                        chain_id: 1,
                        tx_hash: vec![0xaa; 32],
                        tx_index: 0,
                        tx_type: TxType::ContractCall,
                        receipt_type: Some(2),
                        status_ok: false,
                        gas_used: 50_000,
                        cumulative_gas_used: 50_000,
                        effective_gas_price: Some(9),
                        log_bloom: vec![0x99; 256],
                        revert_data: Some(vec![0xde, 0xad, 0xbe, 0xef]),
                        state_root: [0x31; 32],
                        state_version: 10,
                        contract_address: Some(vec![0xcc; 20]),
                        logs: vec![SupervmEvmExecutionLogV1 {
                            emitter: vec![0xdd; 20],
                            topics: vec![[0xee; 32]],
                            data: vec![0xab, 0xcd],
                            tx_index: 0,
                            log_index: 0,
                            state_version: 10,
                        }],
                    },
                    SupervmEvmExecutionReceiptV1 {
                        chain_type: ChainType::EVM,
                        chain_id: 1,
                        tx_hash: vec![0xbb; 32],
                        tx_index: 1,
                        tx_type: TxType::ContractDeploy,
                        receipt_type: Some(3),
                        status_ok: false,
                        gas_used: 120_000,
                        cumulative_gas_used: 170_000,
                        effective_gas_price: Some(11),
                        log_bloom: vec![0x98; 256],
                        revert_data: Some(vec![0xca, 0xfe]),
                        state_root: [0x32; 32],
                        state_version: 10,
                        contract_address: Some(vec![0xce; 20]),
                        logs: vec![SupervmEvmExecutionLogV1 {
                            emitter: vec![0xdf; 20],
                            topics: vec![[0xef; 32]],
                            data: vec![0x01],
                            tx_index: 1,
                            log_index: 1,
                            state_version: 10,
                        }],
                    },
                ],
                state_mirror_updates: Vec::new(),
            }],
        }
    }

    fn sample_store_with_success_receipts() -> MainlineCanonicalStoreV1 {
        MainlineCanonicalStoreV1 {
            schema: "supervm-mainline-canonical/v1".to_string(),
            generated_unix_ms: 1,
            chain_type: "evm".to_string(),
            chain_id: 1,
            batches: vec![MainlineCanonicalBatchRecordV1 {
                seq: 7,
                source_detail: "success_receipts".to_string(),
                tx_count: 2,
                tap_requested: 2,
                tap_accepted: 2,
                tap_dropped: 0,
                apply_verified: true,
                apply_applied: true,
                apply_state_root: [0x71; 32],
                exported_receipt_count: 2,
                mirrored_receipt_count: 2,
                state_version: 12,
                ingress_bypassed: true,
                atomic_guard_enabled: false,
                receipts: vec![
                    SupervmEvmExecutionReceiptV1 {
                        chain_type: ChainType::EVM,
                        chain_id: 1,
                        tx_hash: vec![0xcc; 32],
                        tx_index: 0,
                        tx_type: TxType::ContractDeploy,
                        receipt_type: Some(2),
                        status_ok: true,
                        gas_used: 53_000,
                        cumulative_gas_used: 53_000,
                        effective_gas_price: Some(13),
                        log_bloom: vec![0u8; 256],
                        revert_data: None,
                        state_root: [0x72; 32],
                        state_version: 12,
                        contract_address: Some(vec![0xa1; 20]),
                        logs: vec![SupervmEvmExecutionLogV1 {
                            emitter: vec![0xa1; 20],
                            topics: vec![[0xc1; 32]],
                            data: vec![0x01, 0x02],
                            tx_index: 0,
                            log_index: 0,
                            state_version: 12,
                        }],
                    },
                    SupervmEvmExecutionReceiptV1 {
                        chain_type: ChainType::EVM,
                        chain_id: 1,
                        tx_hash: vec![0xdd; 32],
                        tx_index: 1,
                        tx_type: TxType::ContractCall,
                        receipt_type: Some(2),
                        status_ok: true,
                        gas_used: 42_000,
                        cumulative_gas_used: 95_000,
                        effective_gas_price: Some(17),
                        log_bloom: vec![0u8; 256],
                        revert_data: None,
                        state_root: [0x73; 32],
                        state_version: 12,
                        contract_address: None,
                        logs: vec![
                            SupervmEvmExecutionLogV1 {
                                emitter: vec![0xb1; 20],
                                topics: vec![[0xd1; 32]],
                                data: vec![0x03],
                                tx_index: 1,
                                log_index: 1,
                                state_version: 12,
                            },
                            SupervmEvmExecutionLogV1 {
                                emitter: vec![0xb2; 20],
                                topics: vec![[0xd2; 32], [0xd3; 32]],
                                data: vec![0x04, 0x05],
                                tx_index: 1,
                                log_index: 2,
                                state_version: 12,
                            },
                        ],
                    },
                ],
                state_mirror_updates: Vec::new(),
            }],
        }
    }

    fn compute_test_tx_hash_v1(tx: &TxIR) -> Vec<u8> {
        let mut hasher = Sha256::new();
        hasher.update(&tx.from);
        if let Some(to) = &tx.to {
            hasher.update(to);
        }
        hasher.update(tx.value.to_le_bytes());
        hasher.update(tx.nonce.to_le_bytes());
        hasher.update(&tx.data);
        hasher.finalize().to_vec()
    }

    fn build_signed_tx_for_e2e_v1(
        chain_id: u64,
        tx_type: TxType,
        seed: [u8; 32],
        nonce: u64,
        to: Option<Vec<u8>>,
        data: Vec<u8>,
        gas_limit: u64,
        gas_price: u64,
        value: u128,
    ) -> TxIR {
        let from = address_from_seed_v1(seed);
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
        tx.hash = compute_test_tx_hash_v1(&tx);
        tx.signature = signature_payload_with_seed_v1(&tx, seed);
        tx
    }

    fn build_supervm_logs_from_aoem_logs_e2e_v1(
        tx_index: u32,
        state_version: u64,
        logs: &[AoemEventLogV1],
    ) -> Vec<SupervmEvmExecutionLogV1> {
        logs.iter()
            .enumerate()
            .map(|(idx, log)| SupervmEvmExecutionLogV1 {
                emitter: log.emitter.clone(),
                topics: log.topics.clone(),
                data: log.data.clone(),
                tx_index,
                log_index: idx as u32,
                state_version,
            })
            .collect()
    }

    fn build_supervm_receipt_from_artifact_e2e_v1(
        chain_id: u64,
        state_version: u64,
        tx: &TxIR,
        artifact: &novovm_exec::AoemTxExecutionArtifactV1,
    ) -> SupervmEvmExecutionReceiptV1 {
        SupervmEvmExecutionReceiptV1 {
            chain_type: ChainType::EVM,
            chain_id,
            tx_hash: tx.hash.clone(),
            tx_index: artifact.tx_index,
            tx_type: tx.tx_type,
            receipt_type: artifact.receipt_type,
            status_ok: artifact.status_ok,
            gas_used: artifact.gas_used,
            cumulative_gas_used: artifact.cumulative_gas_used,
            effective_gas_price: artifact.effective_gas_price.or(Some(tx.gas_price)),
            log_bloom: artifact.log_bloom.clone(),
            revert_data: artifact.revert_data.clone(),
            state_root: artifact.state_root,
            state_version,
            contract_address: artifact.contract_address.clone(),
            logs: build_supervm_logs_from_aoem_logs_e2e_v1(
                artifact.tx_index,
                state_version,
                artifact.event_logs.as_slice(),
            ),
        }
    }

    fn build_adapter_to_canonical_store_v1(chain_id: u64) -> MainlineCanonicalStoreV1 {
        let mut adapter = NovoVmAdapter::new(ChainConfig {
            chain_type: ChainType::EVM,
            chain_id,
            name: "e2e-geth-parity".to_string(),
            enabled: true,
            custom_config: None,
        });
        adapter.initialize().expect("initialize adapter");
        let mut runtime_state = StateIR::new();
        let state_version = 0x2a_u64;

        let deploy_tx = build_signed_tx_for_e2e_v1(
            chain_id,
            TxType::ContractDeploy,
            [0x11; 32],
            0,
            None,
            vec![0x60, 0x00, 0x60, 0x01],
            150_000,
            11,
            0,
        );
        let revert_call_tx = build_signed_tx_for_e2e_v1(
            chain_id,
            TxType::ContractCall,
            [0x22; 32],
            0,
            Some(vec![0xb2; 20]),
            vec![0xaa, 0xbb, 0xcc],
            120_000,
            13,
            0,
        );
        let invalid_call_tx = build_signed_tx_for_e2e_v1(
            chain_id,
            TxType::ContractCall,
            [0x33; 32],
            0,
            Some(vec![0xc3; 20]),
            vec![0xdd, 0xee],
            120_000,
            17,
            0,
        );

        let deploy_artifact = novovm_exec::AoemTxExecutionArtifactV1 {
            tx_index: 0,
            tx_hash: deploy_tx.hash.clone(),
            status_ok: true,
            gas_used: 60_000,
            cumulative_gas_used: 60_000,
            state_root: [0x41; 32],
            contract_address: Some(vec![0xa1; 20]),
            receipt_type: Some(1),
            effective_gas_price: Some(deploy_tx.gas_price),
            runtime_code: Some(vec![0x60, 0x00, 0x60, 0x01]),
            runtime_code_hash: Some(vec![0x91; 32]),
            event_logs: vec![AoemEventLogV1 {
                emitter: vec![0xa1; 20],
                topics: vec![[0x44; 32]],
                data: vec![0x01, 0x02, 0x03],
                log_index: 0,
            }],
            log_bloom: vec![0u8; AOEM_LOG_BLOOM_BYTES_V1],
            revert_data: None,
            anchor: Some(novovm_exec::AoemTxExecutionAnchorV1 {
                op_index: Some(0),
                processed_ops: 1,
                success_ops: 1,
                failed_index: None,
                total_writes: 3,
                elapsed_us: 9,
                return_code: 0,
                return_code_name: "ok".to_string(),
            }),
        };
        let revert_artifact = novovm_exec::AoemTxExecutionArtifactV1 {
            tx_index: 1,
            tx_hash: revert_call_tx.hash.clone(),
            status_ok: false,
            gas_used: 30_000,
            cumulative_gas_used: 90_000,
            state_root: [0x42; 32],
            contract_address: None,
            receipt_type: Some(2),
            effective_gas_price: Some(revert_call_tx.gas_price),
            runtime_code: None,
            runtime_code_hash: None,
            event_logs: vec![AoemEventLogV1 {
                emitter: vec![0xb2; 20],
                topics: vec![[0x55; 32]],
                data: vec![0x99],
                log_index: 0,
            }],
            log_bloom: vec![0x77; AOEM_LOG_BLOOM_BYTES_V1],
            revert_data: Some(vec![0x08, 0xc3, 0x79, 0xa0]),
            anchor: Some(novovm_exec::AoemTxExecutionAnchorV1 {
                op_index: Some(1),
                processed_ops: 2,
                success_ops: 1,
                failed_index: Some(1),
                total_writes: 5,
                elapsed_us: 13,
                return_code: 0,
                return_code_name: "revert".to_string(),
            }),
        };
        let invalid_artifact = novovm_exec::AoemTxExecutionArtifactV1 {
            tx_index: 2,
            tx_hash: invalid_call_tx.hash.clone(),
            status_ok: false,
            gas_used: 30_000,
            cumulative_gas_used: 120_000,
            state_root: [0x43; 32],
            contract_address: Some(vec![0xc4; 20]),
            receipt_type: Some(3),
            effective_gas_price: Some(invalid_call_tx.gas_price),
            runtime_code: None,
            runtime_code_hash: None,
            event_logs: vec![AoemEventLogV1 {
                emitter: vec![0xc3; 20],
                topics: vec![[0x66; 32]],
                data: vec![0xaa],
                log_index: 0,
            }],
            log_bloom: vec![0x88; AOEM_LOG_BLOOM_BYTES_V1],
            revert_data: Some(vec![0xde, 0xad, 0xbe, 0xef]),
            anchor: Some(novovm_exec::AoemTxExecutionAnchorV1 {
                op_index: Some(2),
                processed_ops: 3,
                success_ops: 1,
                failed_index: Some(2),
                total_writes: 7,
                elapsed_us: 17,
                return_code: 14,
                return_code_name: "invalid opcode".to_string(),
            }),
        };

        let txs = vec![deploy_tx, revert_call_tx, invalid_call_tx];
        let artifacts = vec![deploy_artifact, revert_artifact, invalid_artifact];
        let mut receipts = Vec::with_capacity(txs.len());
        for (tx, artifact) in txs.iter().zip(artifacts.iter()) {
            let resolved = adapter
                .execute_transaction_with_artifact(tx, &mut runtime_state, Some(artifact))
                .expect("execute tx with artifact");
            receipts.push(build_supervm_receipt_from_artifact_e2e_v1(
                chain_id,
                state_version,
                tx,
                &resolved,
            ));
        }
        let final_state_root = receipts
            .last()
            .map(|receipt| receipt.state_root)
            .unwrap_or([0u8; 32]);

        MainlineCanonicalStoreV1 {
            schema: "supervm-mainline-canonical/v1".to_string(),
            generated_unix_ms: 1,
            chain_type: "evm".to_string(),
            chain_id,
            batches: vec![MainlineCanonicalBatchRecordV1 {
                seq: 0x88,
                source_detail: "adapter-canonical-e2e".to_string(),
                tx_count: receipts.len(),
                tap_requested: receipts.len() as u64,
                tap_accepted: receipts.len(),
                tap_dropped: 0,
                apply_verified: true,
                apply_applied: true,
                apply_state_root: final_state_root,
                exported_receipt_count: receipts.len(),
                mirrored_receipt_count: receipts.len(),
                state_version,
                ingress_bypassed: true,
                atomic_guard_enabled: false,
                receipts,
                state_mirror_updates: Vec::new(),
            }],
        }
    }

    struct AdapterSingleTxStoreCaseV1 {
        tx_type: TxType,
        receipt_type: u8,
        seed: [u8; 32],
        nonce: u64,
        to: Option<Vec<u8>>,
        data: Vec<u8>,
        gas_limit: u64,
        gas_price: u64,
        value: u128,
        status_ok: bool,
        gas_used: u64,
        state_root: [u8; 32],
        contract_address: Option<Vec<u8>>,
        runtime_code: Option<Vec<u8>>,
        runtime_code_hash: Option<Vec<u8>>,
        event_logs: Vec<AoemEventLogV1>,
        revert_data: Option<Vec<u8>>,
        include_anchor: bool,
        anchor_return_code: u32,
        anchor_return_code_name: &'static str,
        batch_seq: u64,
        source_detail: &'static str,
    }

    fn build_adapter_single_tx_store_v1(
        chain_id: u64,
        state_version: u64,
        case: AdapterSingleTxStoreCaseV1,
    ) -> MainlineCanonicalStoreV1 {
        let mut adapter = NovoVmAdapter::new(ChainConfig {
            chain_type: ChainType::EVM,
            chain_id,
            name: "e2e-geth-parity-single".to_string(),
            enabled: true,
            custom_config: None,
        });
        adapter.initialize().expect("initialize adapter");
        let mut runtime_state = StateIR::new();

        let tx = build_signed_tx_for_e2e_v1(
            chain_id,
            case.tx_type,
            case.seed,
            case.nonce,
            case.to,
            case.data,
            case.gas_limit,
            case.gas_price,
            case.value,
        );
        let artifact = novovm_exec::AoemTxExecutionArtifactV1 {
            tx_index: 0,
            tx_hash: tx.hash.clone(),
            status_ok: case.status_ok,
            gas_used: case.gas_used,
            cumulative_gas_used: case.gas_used,
            state_root: case.state_root,
            contract_address: case.contract_address,
            receipt_type: Some(case.receipt_type),
            effective_gas_price: Some(tx.gas_price),
            runtime_code: case.runtime_code,
            runtime_code_hash: case.runtime_code_hash,
            event_logs: case.event_logs,
            log_bloom: vec![0u8; AOEM_LOG_BLOOM_BYTES_V1],
            revert_data: case.revert_data,
            anchor: if case.include_anchor {
                Some(novovm_exec::AoemTxExecutionAnchorV1 {
                    op_index: Some(0),
                    processed_ops: 1,
                    success_ops: if case.status_ok { 1 } else { 0 },
                    failed_index: if case.status_ok { None } else { Some(0) },
                    total_writes: 1,
                    elapsed_us: 5,
                    return_code: case.anchor_return_code,
                    return_code_name: case.anchor_return_code_name.to_string(),
                })
            } else {
                None
            },
        };

        let resolved = adapter
            .execute_transaction_with_artifact(&tx, &mut runtime_state, Some(&artifact))
            .expect("execute tx with artifact");
        let receipt =
            build_supervm_receipt_from_artifact_e2e_v1(chain_id, state_version, &tx, &resolved);

        MainlineCanonicalStoreV1 {
            schema: "supervm-mainline-canonical/v1".to_string(),
            generated_unix_ms: 1,
            chain_type: "evm".to_string(),
            chain_id,
            batches: vec![MainlineCanonicalBatchRecordV1 {
                seq: case.batch_seq,
                source_detail: case.source_detail.to_string(),
                tx_count: 1,
                tap_requested: 1,
                tap_accepted: 1,
                tap_dropped: 0,
                apply_verified: true,
                apply_applied: true,
                apply_state_root: receipt.state_root,
                exported_receipt_count: 1,
                mirrored_receipt_count: 1,
                state_version,
                ingress_bypassed: true,
                atomic_guard_enabled: false,
                receipts: vec![receipt],
                state_mirror_updates: Vec::new(),
            }],
        }
    }

    fn build_adapter_geth_create_contract_access_list_store_v1(
        chain_id: u64,
    ) -> MainlineCanonicalStoreV1 {
        build_adapter_single_tx_store_v1(
            chain_id,
            0x31,
            AdapterSingleTxStoreCaseV1 {
                tx_type: TxType::ContractDeploy,
                receipt_type: 0x1,
                seed: [0x51; 32],
                nonce: 0,
                to: None,
                data: vec![0x60, 0x80, 0x60, 0x40, 0x52],
                gas_limit: 120_000,
                gas_price: 0x1ecb7942,
                value: 0,
                status_ok: true,
                gas_used: 0xe01c,
                state_root: [0x61; 32],
                contract_address: Some(
                    decode_hex_bytes_v1("0xfdaa97661a584d977b4d3abb5370766ff5b86a18")
                        .expect("decode contract address"),
                ),
                runtime_code: None,
                runtime_code_hash: None,
                event_logs: Vec::new(),
                revert_data: None,
                include_anchor: false,
                anchor_return_code: 0,
                anchor_return_code_name: "ok",
                batch_seq: 0x5,
                source_detail: "adapter-geth-create-contract-access-list-e2e",
            },
        )
    }

    fn build_adapter_geth_dynamic_fee_failure_store_v1(chain_id: u64) -> MainlineCanonicalStoreV1 {
        build_adapter_single_tx_store_v1(
            chain_id,
            0x32,
            AdapterSingleTxStoreCaseV1 {
                tx_type: TxType::ContractCall,
                receipt_type: 0x2,
                seed: [0x52; 32],
                nonce: 0,
                to: Some(
                    decode_hex_bytes_v1("0x0000000000000000000000000000000000031ec7")
                        .expect("decode call target"),
                ),
                data: vec![0x12, 0x34, 0x56],
                gas_limit: 100_000,
                gas_price: 0x2325c42f,
                value: 0,
                status_ok: false,
                gas_used: 0x5564,
                state_root: [0x62; 32],
                contract_address: None,
                runtime_code: None,
                runtime_code_hash: None,
                event_logs: Vec::new(),
                revert_data: None,
                include_anchor: true,
                anchor_return_code: 0,
                anchor_return_code_name: "revert",
                batch_seq: 0x4,
                source_detail: "adapter-geth-dynamic-fee-failure-e2e",
            },
        )
    }

    fn build_adapter_geth_blob_tx_success_store_v1(chain_id: u64) -> MainlineCanonicalStoreV1 {
        build_adapter_single_tx_store_v1(
            chain_id,
            0x33,
            AdapterSingleTxStoreCaseV1 {
                tx_type: TxType::ContractCall,
                receipt_type: 0x3,
                seed: [0x53; 32],
                nonce: 0,
                to: Some(
                    decode_hex_bytes_v1("0x0d3ab14bbad3d99f4203bd7a11acb94882050e7e")
                        .expect("decode blob target"),
                ),
                data: vec![0xaa, 0xbb],
                gas_limit: 90_000,
                gas_price: 0x1b0a08c4,
                value: 0,
                status_ok: true,
                gas_used: 0x5208,
                state_root: [0x63; 32],
                contract_address: None,
                runtime_code: None,
                runtime_code_hash: None,
                event_logs: Vec::new(),
                revert_data: None,
                include_anchor: false,
                anchor_return_code: 0,
                anchor_return_code_name: "ok",
                batch_seq: 0x6,
                source_detail: "adapter-geth-blob-tx-success-e2e",
            },
        )
    }

    fn build_adapter_geth_legacy_with_logs_store_v1(chain_id: u64) -> MainlineCanonicalStoreV1 {
        build_adapter_single_tx_store_v1(
            chain_id,
            0x34,
            AdapterSingleTxStoreCaseV1 {
                tx_type: TxType::ContractCall,
                receipt_type: 0x0,
                seed: [0x54; 32],
                nonce: 0,
                to: Some(
                    decode_hex_bytes_v1("0x0000000000000000000000000000000000031ec7")
                        .expect("decode legacy log target"),
                ),
                data: vec![0x09, 0x09],
                gas_limit: 100_000,
                gas_price: 0x281c2585,
                value: 0,
                status_ok: true,
                gas_used: 0x5e28,
                state_root: [0x64; 32],
                contract_address: None,
                runtime_code: None,
                runtime_code_hash: None,
                event_logs: vec![AoemEventLogV1 {
                    emitter: decode_hex_bytes_v1("0x0000000000000000000000000000000000031ec7")
                        .expect("decode legacy emitter"),
                    topics: vec![
                        decode_fixed_32_hex_v1(
                            "0xddf252ad1be2c89b69c2b068fc378daa952ba7f163c4a11628f55a4df523b3ef",
                        )
                        .expect("decode topic0"),
                        decode_fixed_32_hex_v1(
                            "0x000000000000000000000000703c4b2bd70c169f5717101caee543299fc946c7",
                        )
                        .expect("decode topic1"),
                        decode_fixed_32_hex_v1(
                            "0x0000000000000000000000000000000000000000000000000000000000000003",
                        )
                        .expect("decode topic2"),
                    ],
                    data: decode_hex_bytes_v1(
                        "0x000000000000000000000000000000000000000000000000000000000000000d",
                    )
                    .expect("decode legacy log data"),
                    log_index: 0,
                }],
                revert_data: None,
                include_anchor: false,
                anchor_return_code: 0,
                anchor_return_code_name: "ok",
                batch_seq: 0x3,
                source_detail: "adapter-geth-legacy-with-logs-e2e",
            },
        )
    }

    fn build_adapter_geth_deploy_success_with_logs_store_v1(
        chain_id: u64,
    ) -> MainlineCanonicalStoreV1 {
        let contract = decode_hex_bytes_v1("0xae9bea628c4ce503dcfd7e305cab4e29e7476592")
            .expect("decode deploy contract");
        build_adapter_single_tx_store_v1(
            chain_id,
            0x35,
            AdapterSingleTxStoreCaseV1 {
                tx_type: TxType::ContractDeploy,
                receipt_type: 0x0,
                seed: [0x55; 32],
                nonce: 0,
                to: None,
                data: vec![0x60, 0x00, 0x60, 0x01],
                gas_limit: 180_000,
                gas_price: 0x2db16291,
                value: 0,
                status_ok: true,
                gas_used: 0xcf50,
                state_root: [0x65; 32],
                contract_address: Some(contract.clone()),
                runtime_code: Some(vec![0x60, 0x00, 0x60, 0x01]),
                runtime_code_hash: Some(vec![0x75; 32]),
                event_logs: vec![AoemEventLogV1 {
                    emitter: contract,
                    topics: vec![decode_fixed_32_hex_v1(
                        "0xddf252ad1be2c89b69c2b068fc378daa952ba7f163c4a11628f55a4df523b3ef",
                    )
                    .expect("decode deploy success topic")],
                    data: decode_hex_bytes_v1("0x01").expect("decode deploy success log data"),
                    log_index: 0,
                }],
                revert_data: None,
                include_anchor: false,
                anchor_return_code: 0,
                anchor_return_code_name: "ok",
                batch_seq: 0x2,
                source_detail: "adapter-geth-deploy-success-with-logs-e2e",
            },
        )
    }

    fn build_adapter_geth_deploy_fail_revert_store_v1(chain_id: u64) -> MainlineCanonicalStoreV1 {
        build_adapter_single_tx_store_v1(
            chain_id,
            0x36,
            AdapterSingleTxStoreCaseV1 {
                tx_type: TxType::ContractDeploy,
                receipt_type: 0x2,
                seed: [0x56; 32],
                nonce: 0,
                to: None,
                data: vec![0x60, 0x01, 0x60, 0x00, 0xfd],
                gas_limit: 100_000,
                gas_price: 0x2325c42f,
                value: 0,
                status_ok: false,
                gas_used: 0x7530,
                state_root: [0x66; 32],
                contract_address: None,
                runtime_code: None,
                runtime_code_hash: None,
                event_logs: Vec::new(),
                revert_data: Some(vec![0x08, 0xc3, 0x79, 0xa0]),
                include_anchor: true,
                anchor_return_code: 0,
                anchor_return_code_name: "revert",
                batch_seq: 0x8,
                source_detail: "adapter-geth-deploy-fail-revert-e2e",
            },
        )
    }

    fn build_adapter_geth_blob_tx_failure_store_v1(chain_id: u64) -> MainlineCanonicalStoreV1 {
        build_adapter_single_tx_store_v1(
            chain_id,
            0x37,
            AdapterSingleTxStoreCaseV1 {
                tx_type: TxType::ContractCall,
                receipt_type: 0x3,
                seed: [0x57; 32],
                nonce: 0,
                to: Some(
                    decode_hex_bytes_v1("0x0d3ab14bbad3d99f4203bd7a11acb94882050e7e")
                        .expect("decode blob failure target"),
                ),
                data: vec![0xaa, 0xbb, 0xcc],
                gas_limit: 80_000,
                gas_price: 0x1b0a08c4,
                value: 0,
                status_ok: false,
                gas_used: 0x13880,
                state_root: [0x67; 32],
                contract_address: None,
                runtime_code: None,
                runtime_code_hash: None,
                event_logs: Vec::new(),
                revert_data: None,
                include_anchor: true,
                anchor_return_code: 13,
                anchor_return_code_name: "out of gas",
                batch_seq: 0x9,
                source_detail: "adapter-geth-blob-tx-failure-e2e",
            },
        )
    }

    fn build_adapter_geth_type2_priority_over_max_fee_store_v1(
        chain_id: u64,
    ) -> MainlineCanonicalStoreV1 {
        build_adapter_single_tx_store_v1(
            chain_id,
            0x38,
            AdapterSingleTxStoreCaseV1 {
                tx_type: TxType::ContractCall,
                receipt_type: 0x2,
                seed: [0x58; 32],
                nonce: 0,
                to: Some(
                    decode_hex_bytes_v1("0x0000000000000000000000000000000000031ec7")
                        .expect("decode type2 fee target"),
                ),
                data: vec![0x01, 0x02],
                gas_limit: 90_000,
                gas_price: 0x1,
                value: 0,
                status_ok: false,
                gas_used: 0x5208,
                state_root: [0x68; 32],
                contract_address: None,
                runtime_code: None,
                runtime_code_hash: None,
                event_logs: Vec::new(),
                revert_data: None,
                include_anchor: true,
                anchor_return_code: 14,
                anchor_return_code_name:
                    "invalid max priority fee per gas higher than max fee per gas",
                batch_seq: 0xa,
                source_detail: "adapter-geth-type2-priority-over-max-fee-e2e",
            },
        )
    }

    fn build_adapter_geth_type2_intrinsic_gas_low_store_v1(
        chain_id: u64,
    ) -> MainlineCanonicalStoreV1 {
        build_adapter_single_tx_store_v1(
            chain_id,
            0x39,
            AdapterSingleTxStoreCaseV1 {
                tx_type: TxType::ContractCall,
                receipt_type: 0x2,
                seed: [0x59; 32],
                nonce: 0,
                to: Some(
                    decode_hex_bytes_v1("0x0000000000000000000000000000000000031ec7")
                        .expect("decode intrinsic gas low target"),
                ),
                data: vec![0x03, 0x04, 0x05],
                gas_limit: 20_999,
                gas_price: 0x2,
                value: 0,
                status_ok: false,
                gas_used: 0x5208,
                state_root: [0x69; 32],
                contract_address: None,
                runtime_code: None,
                runtime_code_hash: None,
                event_logs: Vec::new(),
                revert_data: None,
                include_anchor: true,
                anchor_return_code: 14,
                anchor_return_code_name: "invalid intrinsic gas too low",
                batch_seq: 0xb,
                source_detail: "adapter-geth-type2-intrinsic-gas-low-e2e",
            },
        )
    }

    fn build_adapter_geth_reorg_dual_tx_store_v1(chain_id: u64) -> MainlineCanonicalStoreV1 {
        let mut store = sample_store_with_success_receipts();
        rewrite_store_chain_id_v1(&mut store, chain_id);
        store
    }

    fn sample_native_runtime_snapshot_for_block_context(
        chain_id: u64,
        block_context: &EthFullnodeBlockContextV1,
        canonical_hash_override: Option<[u8; 32]>,
    ) -> EthFullnodeNativeWorkerRuntimeSnapshotV1 {
        let canonical_hash = canonical_hash_override.unwrap_or(block_context.block_hash);
        let head_block = novovm_network::NetworkRuntimeNativeCanonicalBlockStateV1 {
            chain_id,
            number: block_context.block_number,
            hash: canonical_hash,
            parent_hash: block_context.parent_block_hash,
            state_root: block_context.state_root,
            header_observed: true,
            body_available: true,
            lifecycle_stage: novovm_network::NetworkRuntimeNativeBlockLifecycleStageV1::Canonical,
            canonical: true,
            safe: false,
            finalized: false,
            source_peer_id: Some(0x42),
            observed_unix_ms: 7,
        };
        EthFullnodeNativeWorkerRuntimeSnapshotV1 {
            schema: novovm_network::ETH_FULLNODE_NATIVE_WORKER_RUNTIME_SCHEMA_V1.to_string(),
            chain_id,
            updated_at_unix_ms: 7,
            candidate_peer_ids: vec![0x42],
            scheduled_bootstrap_peers: 0,
            scheduled_sync_peers: 1,
            attempted_bootstrap_peers: 0,
            attempted_sync_peers: 1,
            failed_bootstrap_peers: 0,
            failed_sync_peers: 0,
            skipped_missing_endpoint_peers: 0,
            connected_peers: 1,
            ready_peers: 1,
            status_updates: 1,
            header_updates: 1,
            body_updates: 1,
            sync_requests: 1,
            inbound_frames: 3,
            head_view: None,
            sync_view: None,
            native_canonical_chain: Some(
                novovm_network::NetworkRuntimeNativeCanonicalChainStateV1 {
                    chain_id,
                    lifecycle_stage:
                        novovm_network::NetworkRuntimeNativeCanonicalLifecycleStageV1::Advanced,
                    head: Some(head_block.clone()),
                    retained_block_count: 1,
                    canonical_block_count: 1,
                    canonical_update_count: 1,
                    reorg_count: 0,
                    last_reorg_depth: None,
                    last_reorg_unix_ms: None,
                    last_head_change_unix_ms: Some(7),
                    block_lifecycle_summary:
                        novovm_network::NetworkRuntimeNativeBlockLifecycleSummaryV1 {
                            seen_count: 0,
                            header_only_count: 0,
                            body_ready_count: 0,
                            canonical_count: 1,
                            non_canonical_count: 0,
                            reorged_out_count: 0,
                        },
                },
            ),
            native_canonical_blocks: vec![head_block],
            native_pending_tx_summary: Default::default(),
            native_pending_tx_broadcast_runtime: Default::default(),
            native_execution_budget_runtime: Default::default(),
            native_pending_txs: Vec::new(),
            native_head_body_available: Some(true),
            native_head_canonical: Some(true),
            native_head_safe: Some(false),
            native_head_finalized: Some(false),
            lifecycle_summary: EthPeerLifecycleSummaryV1::default(),
            selection_quality_summary: Default::default(),
            selection_long_term_summary: Default::default(),
            selection_window_policy: Default::default(),
            runtime_config: Default::default(),
            peer_selection_scores: Vec::new(),
            peer_sessions: Vec::new(),
            peer_failures: Vec::new(),
        }
    }

    fn assert_same_root_cause_fields_v1(health: &Value, sync: &Value, degradation: &Value) {
        for view in [health, sync, degradation] {
            assert!(view.get("primaryReason").is_some());
            assert!(view.get("rootCause").is_some());
            assert!(view.get("rootCauseSignals").is_some());
            assert!(view.get("selectionCorrelation").is_some());
            assert!(view.get("failureClassificationContracts").is_some());
            assert_eq!(
                view.get("schemaVersion").and_then(Value::as_u64),
                Some(ETH_NATIVE_RUNTIME_QUERY_SCHEMA_VERSION_V1)
            );
        }
        assert_eq!(health["primaryReason"], sync["primaryReason"]);
        assert_eq!(health["primaryReason"], degradation["primaryReason"]);
        assert_eq!(health["rootCause"], sync["rootCause"]);
        assert_eq!(health["rootCause"], degradation["rootCause"]);
        assert_eq!(health["rootCauseSignals"], sync["rootCauseSignals"]);
        assert_eq!(health["rootCauseSignals"], degradation["rootCauseSignals"]);
        assert_eq!(health["selectionCorrelation"], sync["selectionCorrelation"]);
        assert_eq!(
            health["selectionCorrelation"],
            degradation["selectionCorrelation"]
        );
        assert_eq!(
            health["failureClassificationContracts"],
            sync["failureClassificationContracts"]
        );
        assert_eq!(
            health["failureClassificationContracts"],
            degradation["failureClassificationContracts"]
        );
        assert!(health.get("crossLayer").is_none());
        assert!(sync.get("crossLayer").is_none());
        assert!(degradation.get("crossLayer").is_some());
    }

    fn record_mismatch_v1(
        mismatches: &mut Vec<Value>,
        scope: &str,
        field: &str,
        expected: Value,
        actual: Value,
    ) {
        if expected == actual {
            return;
        }
        mismatches.push(json!({
            "scope": scope,
            "field": field,
            "expected": expected,
            "actual": actual,
        }));
    }

    #[derive(Debug, Clone, Deserialize)]
    struct GethParityExpectedBlockV1 {
        number: Option<String>,
        tx_count: Option<usize>,
        logs_bloom: Option<String>,
        tx_types: Option<Vec<String>>,
        tx_statuses: Option<Vec<String>>,
        tx_contract_addresses: Option<Vec<Option<String>>>,
    }

    #[derive(Debug, Clone, Deserialize)]
    struct GethParityExpectedReceiptV1 {
        tx_index: usize,
        status: Option<String>,
        tx_type: Option<String>,
        gas_used: Option<String>,
        cumulative_gas_used: Option<String>,
        contract_address: Option<Option<String>>,
        revert_data: Option<Option<String>>,
        log_count: Option<usize>,
    }

    #[derive(Debug, Clone, Deserialize)]
    struct GethParityExpectedLogsViewV1 {
        count: Option<u64>,
        first_removed: Option<bool>,
        first_address: Option<String>,
        first_log_ownership: Option<String>,
    }

    #[derive(Debug, Clone, Deserialize)]
    struct GethParityExpectedTypedFailureV1 {
        tx_index: usize,
        failure_classification_contract: Option<String>,
        status: Option<String>,
        contract_address_null: Option<bool>,
    }

    #[derive(Debug, Clone, Deserialize)]
    struct GethParityExpectedV1 {
        block: GethParityExpectedBlockV1,
        receipts: Vec<GethParityExpectedReceiptV1>,
        logs_canonical: GethParityExpectedLogsViewV1,
        logs_noncanonical: GethParityExpectedLogsViewV1,
        typed_failures: Vec<GethParityExpectedTypedFailureV1>,
    }

    #[derive(Debug, Clone, Deserialize)]
    struct GethParitySampleFileV1 {
        schema: String,
        name: String,
        #[serde(default)]
        scenario: Option<String>,
        #[serde(default)]
        chain_id: Option<u64>,
        #[serde(default)]
        store_path: Option<String>,
        #[serde(default)]
        store_format: Option<String>,
        expected: GethParityExpectedV1,
        #[serde(default)]
        reorg_hash_hex: Option<String>,
    }

    #[derive(Debug, Clone, Deserialize)]
    struct GethExportLogV1 {
        address: String,
        #[serde(default)]
        topics: Vec<String>,
        #[serde(default)]
        data: String,
        #[serde(rename = "logIndex", default)]
        log_index: Option<String>,
    }

    #[derive(Debug, Clone, Deserialize)]
    struct GethExportReceiptV1 {
        #[serde(rename = "blockHash", default)]
        block_hash: Option<String>,
        #[serde(rename = "blockNumber", default)]
        block_number: Option<String>,
        #[serde(rename = "contractAddress", default)]
        contract_address: Option<String>,
        #[serde(rename = "cumulativeGasUsed", default)]
        cumulative_gas_used: Option<String>,
        #[serde(rename = "effectiveGasPrice", default)]
        effective_gas_price: Option<String>,
        #[serde(rename = "gasUsed", default)]
        gas_used: Option<String>,
        #[serde(rename = "logs", default)]
        logs: Vec<GethExportLogV1>,
        #[serde(rename = "logsBloom", default)]
        logs_bloom: Option<String>,
        #[serde(rename = "status", default)]
        status: Option<String>,
        #[serde(rename = "transactionHash", default)]
        transaction_hash: Option<String>,
        #[serde(rename = "transactionIndex", default)]
        transaction_index: Option<String>,
        #[serde(rename = "type", default)]
        tx_type: Option<String>,
        #[serde(default)]
        to: Option<String>,
        #[serde(rename = "revertData", default)]
        revert_data: Option<String>,
    }

    fn default_geth_parity_sample_dir_v1() -> PathBuf {
        PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("tests")
            .join("fixtures")
            .join("geth-parity")
    }

    fn decode_hex_bytes_v1(raw: &str) -> Result<Vec<u8>> {
        let value = raw.trim();
        let payload = value.strip_prefix("0x").unwrap_or(value);
        if payload.is_empty() {
            return Ok(Vec::new());
        }
        if payload.len() % 2 != 0 {
            bail!("hex payload must have even length, got {}", payload.len());
        }
        let mut out = Vec::with_capacity(payload.len() / 2);
        for (idx, chunk) in payload.as_bytes().chunks_exact(2).enumerate() {
            let hex = std::str::from_utf8(chunk).context("invalid utf8 in hex bytes")?;
            out.push(
                u8::from_str_radix(hex, 16)
                    .with_context(|| format!("invalid hex byte at index {idx}: {hex}"))?,
            );
        }
        Ok(out)
    }

    fn decode_fixed_32_hex_v1(raw: &str) -> Result<[u8; 32]> {
        let value = raw.trim();
        let payload = value.strip_prefix("0x").unwrap_or(value);
        if payload.len() != 64 {
            bail!(
                "hex payload must be 32 bytes, got len={}",
                payload.len() / 2
            );
        }
        let mut out = [0u8; 32];
        for (idx, chunk) in payload.as_bytes().chunks_exact(2).enumerate() {
            let hex = std::str::from_utf8(chunk).context("invalid utf8 in hex bytes")?;
            out[idx] = u8::from_str_radix(hex, 16)
                .with_context(|| format!("invalid hex byte at index {idx}: {hex}"))?;
        }
        Ok(out)
    }

    fn parse_hex_u64_required_v1(raw: &str, field: &str) -> Result<u64> {
        parse_hex_u64(raw)
            .or_else(|| raw.trim().parse::<u64>().ok())
            .with_context(|| format!("parse {field} as u64 failed: {raw}"))
    }

    fn parse_hex_u64_optional_v1(raw: Option<&str>) -> Option<u64> {
        raw.and_then(parse_hex_u64)
            .or_else(|| raw.and_then(|value| value.trim().parse::<u64>().ok()))
    }

    fn resolve_store_path_v1(sample_path: &std::path::Path, raw: &str) -> Result<PathBuf> {
        let mut normalized = raw.trim().to_string();
        if normalized.contains("${NOVOVM_GETH_REPO_ROOT}") {
            let root = std::env::var("NOVOVM_GETH_REPO_ROOT")
                .context("store_path uses ${NOVOVM_GETH_REPO_ROOT} but env var is not set")?;
            normalized = normalized.replace("${NOVOVM_GETH_REPO_ROOT}", root.as_str());
        }
        let candidate = PathBuf::from(&normalized);
        if candidate.is_absolute() {
            return Ok(candidate);
        }
        Ok(sample_path
            .parent()
            .unwrap_or_else(|| std::path::Path::new("."))
            .join(candidate))
    }

    fn state_root_from_tx_hash_v1(tx_hash: &[u8], block_hash: Option<&str>) -> [u8; 32] {
        let mut hasher = Sha256::new();
        hasher.update(b"supervm-geth-export-state-root-v1");
        hasher.update(tx_hash);
        if let Some(raw) = block_hash {
            hasher.update(raw.as_bytes());
        }
        hasher.finalize().into()
    }

    fn map_geth_export_receipt_tx_type_v1(receipt: &GethExportReceiptV1) -> TxType {
        if receipt.contract_address.is_some() {
            return TxType::ContractDeploy;
        }
        if receipt.to.is_some() {
            return TxType::ContractCall;
        }
        TxType::Transfer
    }

    fn parse_geth_export_receipts_from_value_v1(value: &Value) -> Result<Vec<GethExportReceiptV1>> {
        if value.is_array() {
            let receipts: Vec<GethExportReceiptV1> =
                serde_json::from_value(value.clone()).context("parse geth receipt array failed")?;
            return Ok(receipts);
        }
        if value.get("transactionHash").is_some() && value.get("blockNumber").is_some() {
            let receipt: GethExportReceiptV1 = serde_json::from_value(value.clone())
                .context("parse geth receipt object failed")?;
            return Ok(vec![receipt]);
        }
        if let Some(result) = value.get("result") {
            return parse_geth_export_receipts_from_value_v1(result);
        }
        if let Some(receipts) = value.get("receipts") {
            return parse_geth_export_receipts_from_value_v1(receipts);
        }
        bail!("unsupported geth export payload shape");
    }

    fn build_store_from_geth_export_receipts_v1(
        file_path: &std::path::Path,
        chain_id: u64,
        receipts: Vec<GethExportReceiptV1>,
    ) -> Result<MainlineCanonicalStoreV1> {
        if receipts.is_empty() {
            bail!("geth export contains no receipts: {}", file_path.display());
        }

        let mut normalized = receipts;
        normalized.sort_by_key(|receipt| {
            parse_hex_u64_optional_v1(receipt.transaction_index.as_deref()).unwrap_or(u64::MAX)
        });

        let first_block_number = normalized
            .first()
            .and_then(|receipt| receipt.block_number.as_deref())
            .with_context(|| format!("missing blockNumber in {}", file_path.display()))?;
        let batch_seq = parse_hex_u64_required_v1(first_block_number, "blockNumber")?;

        for receipt in normalized.iter().skip(1) {
            if let Some(block_number) = receipt.block_number.as_deref() {
                let value = parse_hex_u64_required_v1(block_number, "blockNumber")?;
                if value != batch_seq {
                    bail!(
                        "mixed blockNumber in geth export {}: expected {}, got {}",
                        file_path.display(),
                        batch_seq,
                        value
                    );
                }
            }
        }

        let mut out_receipts = Vec::with_capacity(normalized.len());
        for (idx, receipt) in normalized.iter().enumerate() {
            let tx_hash =
                decode_hex_bytes_v1(receipt.transaction_hash.as_deref().with_context(|| {
                    format!("missing transactionHash in {}", file_path.display())
                })?)
                .with_context(|| {
                    format!("decode transactionHash failed in {}", file_path.display())
                })?;
            let tx_index = parse_hex_u64_required_v1(
                receipt.transaction_index.as_deref().with_context(|| {
                    format!("missing transactionIndex in {}", file_path.display())
                })?,
                "transactionIndex",
            )? as u32;
            let status_ok = parse_hex_u64_required_v1(
                receipt
                    .status
                    .as_deref()
                    .with_context(|| format!("missing status in {}", file_path.display()))?,
                "status",
            )? != 0;
            let gas_used = parse_hex_u64_required_v1(
                receipt
                    .gas_used
                    .as_deref()
                    .with_context(|| format!("missing gasUsed in {}", file_path.display()))?,
                "gasUsed",
            )?;
            let cumulative_gas_used = parse_hex_u64_required_v1(
                receipt.cumulative_gas_used.as_deref().with_context(|| {
                    format!("missing cumulativeGasUsed in {}", file_path.display())
                })?,
                "cumulativeGasUsed",
            )?;
            let logs = receipt
                .logs
                .iter()
                .enumerate()
                .map(|(log_idx, log)| {
                    let topics = log
                        .topics
                        .iter()
                        .map(|topic| decode_fixed_32_hex_v1(topic))
                        .collect::<Result<Vec<_>>>()?;
                    let log_index = parse_hex_u64_optional_v1(log.log_index.as_deref())
                        .map(|value| value as u32)
                        .unwrap_or(log_idx as u32);
                    Ok(SupervmEvmExecutionLogV1 {
                        emitter: decode_hex_bytes_v1(log.address.as_str())
                            .context("decode log address failed")?,
                        topics,
                        data: decode_hex_bytes_v1(log.data.as_str())
                            .context("decode log data failed")?,
                        tx_index,
                        log_index,
                        state_version: 1,
                    })
                })
                .collect::<Result<Vec<_>>>()
                .with_context(|| format!("decode logs failed in {}", file_path.display()))?;

            out_receipts.push(SupervmEvmExecutionReceiptV1 {
                chain_type: ChainType::EVM,
                chain_id,
                tx_hash: tx_hash.clone(),
                tx_index,
                tx_type: map_geth_export_receipt_tx_type_v1(receipt),
                receipt_type: parse_hex_u64_optional_v1(receipt.tx_type.as_deref())
                    .map(|value| value as u8),
                status_ok,
                gas_used,
                cumulative_gas_used,
                effective_gas_price: parse_hex_u64_optional_v1(
                    receipt.effective_gas_price.as_deref(),
                ),
                log_bloom: receipt
                    .logs_bloom
                    .as_deref()
                    .map(decode_hex_bytes_v1)
                    .transpose()
                    .with_context(|| format!("decode logsBloom failed in {}", file_path.display()))?
                    .unwrap_or_default(),
                revert_data: receipt
                    .revert_data
                    .as_deref()
                    .map(decode_hex_bytes_v1)
                    .transpose()
                    .with_context(|| {
                        format!("decode revertData failed in {}", file_path.display())
                    })?,
                state_root: state_root_from_tx_hash_v1(
                    tx_hash.as_slice(),
                    receipt.block_hash.as_deref(),
                ),
                state_version: 1,
                contract_address: receipt
                    .contract_address
                    .as_deref()
                    .map(decode_hex_bytes_v1)
                    .transpose()
                    .with_context(|| {
                        format!("decode contractAddress failed in {}", file_path.display())
                    })?,
                logs,
            });

            if idx > 0 {
                let prev = out_receipts[idx - 1].cumulative_gas_used;
                if cumulative_gas_used < prev {
                    bail!(
                        "non-monotonic cumulativeGasUsed in {} at tx_index={}",
                        file_path.display(),
                        tx_index
                    );
                }
            }
        }

        let apply_state_root = out_receipts
            .last()
            .map(|receipt| receipt.state_root)
            .unwrap_or([0u8; 32]);

        Ok(MainlineCanonicalStoreV1 {
            schema: "supervm-mainline-canonical/v1".to_string(),
            generated_unix_ms: 1,
            chain_type: "evm".to_string(),
            chain_id,
            batches: vec![MainlineCanonicalBatchRecordV1 {
                seq: batch_seq,
                source_detail: format!("geth-export:{}", file_path.display()),
                tx_count: out_receipts.len(),
                tap_requested: out_receipts.len() as u64,
                tap_accepted: out_receipts.len(),
                tap_dropped: 0,
                apply_verified: true,
                apply_applied: true,
                apply_state_root,
                exported_receipt_count: out_receipts.len(),
                mirrored_receipt_count: out_receipts.len(),
                state_version: 1,
                ingress_bypassed: true,
                atomic_guard_enabled: false,
                receipts: out_receipts,
                state_mirror_updates: Vec::new(),
            }],
        })
    }

    fn load_geth_parity_sample_store_v1(
        sample_path: &std::path::Path,
        sample: &GethParitySampleFileV1,
    ) -> Result<MainlineCanonicalStoreV1> {
        if let Some(path) = sample.store_path.as_ref() {
            let file_path = resolve_store_path_v1(sample_path, path)?;
            let payload = fs::read_to_string(&file_path)
                .with_context(|| format!("read store file failed: {}", file_path.display()))?;
            if matches!(
                sample.store_format.as_deref(),
                Some("geth-export-receipt/v1") | Some("geth-export-receipts/v1")
            ) {
                let value: Value = serde_json::from_str(payload.as_str()).with_context(|| {
                    format!("parse geth export failed: {}", file_path.display())
                })?;
                let receipts =
                    parse_geth_export_receipts_from_value_v1(&value).with_context(|| {
                        format!(
                            "extract geth export receipts failed: {}",
                            file_path.display()
                        )
                    })?;
                return build_store_from_geth_export_receipts_v1(
                    &file_path,
                    sample.chain_id.unwrap_or(1),
                    receipts,
                );
            }
            if let Ok(store) = serde_json::from_str::<MainlineCanonicalStoreV1>(payload.as_str()) {
                return Ok(store);
            }
            let value: Value = serde_json::from_str(payload.as_str())
                .with_context(|| format!("parse store file failed: {}", file_path.display()))?;
            let receipts = parse_geth_export_receipts_from_value_v1(&value).with_context(|| {
                format!(
                    "store file is neither canonical store nor geth export receipts: {}",
                    file_path.display()
                )
            })?;
            return build_store_from_geth_export_receipts_v1(
                &file_path,
                sample.chain_id.unwrap_or(1),
                receipts,
            );
        }
        match sample.scenario.as_deref() {
            Some("adapter_e2e_default_v1") => {
                let chain_id = sample.chain_id.unwrap_or(99_160_715_u64);
                Ok(build_adapter_to_canonical_store_v1(chain_id))
            }
            Some("adapter_e2e_geth_create_contract_access_list_v1") => {
                let chain_id = sample.chain_id.unwrap_or(1_u64);
                Ok(build_adapter_geth_create_contract_access_list_store_v1(
                    chain_id,
                ))
            }
            Some("adapter_e2e_geth_dynamic_fee_failure_v1") => {
                let chain_id = sample.chain_id.unwrap_or(1_u64);
                Ok(build_adapter_geth_dynamic_fee_failure_store_v1(chain_id))
            }
            Some("adapter_e2e_geth_blob_tx_success_v1") => {
                let chain_id = sample.chain_id.unwrap_or(1_u64);
                Ok(build_adapter_geth_blob_tx_success_store_v1(chain_id))
            }
            Some("adapter_e2e_geth_legacy_with_logs_v1") => {
                let chain_id = sample.chain_id.unwrap_or(1_u64);
                Ok(build_adapter_geth_legacy_with_logs_store_v1(chain_id))
            }
            Some("adapter_e2e_geth_deploy_success_with_logs_v1") => {
                let chain_id = sample.chain_id.unwrap_or(1_u64);
                Ok(build_adapter_geth_deploy_success_with_logs_store_v1(
                    chain_id,
                ))
            }
            Some("adapter_e2e_geth_deploy_fail_revert_v1") => {
                let chain_id = sample.chain_id.unwrap_or(1_u64);
                Ok(build_adapter_geth_deploy_fail_revert_store_v1(chain_id))
            }
            Some("adapter_e2e_geth_blob_tx_failure_v1") => {
                let chain_id = sample.chain_id.unwrap_or(1_u64);
                Ok(build_adapter_geth_blob_tx_failure_store_v1(chain_id))
            }
            Some("adapter_e2e_geth_type2_priority_over_max_fee_v1") => {
                let chain_id = sample.chain_id.unwrap_or(1_u64);
                Ok(build_adapter_geth_type2_priority_over_max_fee_store_v1(
                    chain_id,
                ))
            }
            Some("adapter_e2e_geth_type2_intrinsic_gas_low_v1") => {
                let chain_id = sample.chain_id.unwrap_or(1_u64);
                Ok(build_adapter_geth_type2_intrinsic_gas_low_store_v1(
                    chain_id,
                ))
            }
            Some("adapter_e2e_geth_reorg_dual_tx_v1") => {
                let chain_id = sample.chain_id.unwrap_or(1_u64);
                Ok(build_adapter_geth_reorg_dual_tx_store_v1(chain_id))
            }
            Some(other) => bail!("unsupported geth parity sample scenario: {other}"),
            None => bail!("sample must provide either store_path or scenario"),
        }
    }

    fn run_geth_parity_report_from_store_v1(
        sample_name: &str,
        store: &MainlineCanonicalStoreV1,
        expected: &GethParityExpectedV1,
        reorg_hash_override: Option<[u8; 32]>,
    ) -> Value {
        let chain_id = store.chain_id;
        clear_eth_fullnode_native_worker_runtime_snapshot_for_chain_v1(chain_id);
        let block_context = derive_mainline_eth_fullnode_block_contexts_v1(store)
            .into_iter()
            .next()
            .expect("block context");
        let computed_logs_bloom = to_hex_prefixed(&combine_receipt_log_bloom(
            store.batches[0].receipts.as_slice(),
        ));

        let mut block_mismatches = Vec::new();
        let mut receipt_mismatches = Vec::new();
        let mut log_mismatches = Vec::new();
        let mut typed_failure_mismatches = Vec::new();

        let block_out = run_mainline_query(store, "eth_getBlockByNumber", &json!(["latest", true]))
            .expect("block by number");
        let block = &block_out["block"];
        if let Some(number) = expected.block.number.as_ref() {
            record_mismatch_v1(
                &mut block_mismatches,
                "block",
                "number",
                json!(number),
                block["number"].clone(),
            );
        }
        if let Some(tx_count) = expected.block.tx_count {
            record_mismatch_v1(
                &mut block_mismatches,
                "block",
                "tx_count",
                json!(tx_count),
                json!(block["transactions"].as_array().map(Vec::len).unwrap_or(0)),
            );
        }
        if let Some(logs_bloom) = expected.block.logs_bloom.as_ref() {
            record_mismatch_v1(
                &mut block_mismatches,
                "block",
                "logsBloom",
                json!(logs_bloom),
                block["logsBloom"].clone(),
            );
        } else {
            record_mismatch_v1(
                &mut block_mismatches,
                "block",
                "logsBloomComputed",
                json!(computed_logs_bloom),
                block["logsBloom"].clone(),
            );
        }
        if let Some(types) = expected.block.tx_types.as_ref() {
            for (idx, value) in types.iter().enumerate() {
                record_mismatch_v1(
                    &mut block_mismatches,
                    format!("block.tx[{idx}]").as_str(),
                    "type",
                    json!(value),
                    block["transactions"][idx]["type"].clone(),
                );
            }
        }
        if let Some(statuses) = expected.block.tx_statuses.as_ref() {
            for (idx, value) in statuses.iter().enumerate() {
                record_mismatch_v1(
                    &mut block_mismatches,
                    format!("block.tx[{idx}]").as_str(),
                    "status",
                    json!(value),
                    block["transactions"][idx]["status"].clone(),
                );
            }
        }
        if let Some(addresses) = expected.block.tx_contract_addresses.as_ref() {
            for (idx, value) in addresses.iter().enumerate() {
                match value {
                    Some(address) => record_mismatch_v1(
                        &mut block_mismatches,
                        format!("block.tx[{idx}]").as_str(),
                        "contractAddress",
                        json!(address),
                        block["transactions"][idx]["contractAddress"].clone(),
                    ),
                    None => record_mismatch_v1(
                        &mut block_mismatches,
                        format!("block.tx[{idx}]").as_str(),
                        "contractAddress",
                        Value::Null,
                        block["transactions"][idx]["contractAddress"].clone(),
                    ),
                }
            }
        }

        for expected_receipt in &expected.receipts {
            let scope = format!("receipt.tx[{}]", expected_receipt.tx_index);
            let tx_hash =
                to_hex_prefixed(&store.batches[0].receipts[expected_receipt.tx_index].tx_hash);
            let out = run_mainline_query(store, "eth_getTransactionReceipt", &json!([tx_hash]))
                .expect("receipt query");
            let receipt = &out["receipt"];
            if let Some(status) = expected_receipt.status.as_ref() {
                record_mismatch_v1(
                    &mut receipt_mismatches,
                    scope.as_str(),
                    "status",
                    json!(status),
                    receipt["status"].clone(),
                );
            }
            if let Some(tx_type) = expected_receipt.tx_type.as_ref() {
                record_mismatch_v1(
                    &mut receipt_mismatches,
                    scope.as_str(),
                    "type",
                    json!(tx_type),
                    receipt["type"].clone(),
                );
            }
            if let Some(gas_used) = expected_receipt.gas_used.as_ref() {
                record_mismatch_v1(
                    &mut receipt_mismatches,
                    scope.as_str(),
                    "gasUsed",
                    json!(gas_used),
                    receipt["gasUsed"].clone(),
                );
            }
            if let Some(cumulative) = expected_receipt.cumulative_gas_used.as_ref() {
                record_mismatch_v1(
                    &mut receipt_mismatches,
                    scope.as_str(),
                    "cumulativeGasUsed",
                    json!(cumulative),
                    receipt["cumulativeGasUsed"].clone(),
                );
            }
            if let Some(address) = expected_receipt.contract_address.as_ref() {
                match address {
                    Some(value) => record_mismatch_v1(
                        &mut receipt_mismatches,
                        scope.as_str(),
                        "contractAddress",
                        json!(value),
                        receipt["contractAddress"].clone(),
                    ),
                    None => record_mismatch_v1(
                        &mut receipt_mismatches,
                        scope.as_str(),
                        "contractAddress",
                        Value::Null,
                        receipt["contractAddress"].clone(),
                    ),
                }
            }
            if let Some(revert_data) = expected_receipt.revert_data.as_ref() {
                match revert_data {
                    Some(value) => record_mismatch_v1(
                        &mut receipt_mismatches,
                        scope.as_str(),
                        "revertData",
                        json!(value),
                        receipt["revertData"].clone(),
                    ),
                    None => record_mismatch_v1(
                        &mut receipt_mismatches,
                        scope.as_str(),
                        "revertData",
                        Value::Null,
                        receipt["revertData"].clone(),
                    ),
                }
            }
            if let Some(log_count) = expected_receipt.log_count {
                record_mismatch_v1(
                    &mut receipt_mismatches,
                    scope.as_str(),
                    "logCount",
                    json!(log_count),
                    json!(receipt["logs"].as_array().map(Vec::len).unwrap_or(0)),
                );
            }
        }

        let logs_out = run_mainline_query(store, "eth_getLogs", &json!({})).expect("logs query");
        if let Some(count) = expected.logs_canonical.count {
            record_mismatch_v1(
                &mut log_mismatches,
                "logs.canonical",
                "count",
                json!(count),
                logs_out["count"].clone(),
            );
        }
        if let Some(removed) = expected.logs_canonical.first_removed {
            record_mismatch_v1(
                &mut log_mismatches,
                "logs.canonical",
                "first_removed",
                json!(removed),
                logs_out["logs"][0]["removed"].clone(),
            );
        }
        if let Some(address) = expected.logs_canonical.first_address.as_ref() {
            record_mismatch_v1(
                &mut log_mismatches,
                "logs.canonical",
                "first_address",
                json!(address),
                logs_out["logs"][0]["address"].clone(),
            );
        }
        if let Some(ownership) = expected.logs_canonical.first_log_ownership.as_ref() {
            record_mismatch_v1(
                &mut log_mismatches,
                "logs.canonical",
                "first_log_ownership",
                json!(ownership),
                logs_out["logs"][0]["logOwnership"].clone(),
            );
        }

        let reorg_hash = reorg_hash_override.unwrap_or([0xfe; 32]);
        set_eth_fullnode_native_worker_runtime_snapshot_v1(
            chain_id,
            sample_native_runtime_snapshot_for_block_context(
                chain_id,
                &block_context,
                Some(reorg_hash),
            ),
        );
        let logs_noncanonical =
            run_mainline_query(store, "eth_getLogs", &json!({})).expect("logs non-canonical");
        if let Some(count) = expected.logs_noncanonical.count {
            record_mismatch_v1(
                &mut log_mismatches,
                "logs.non_canonical",
                "count",
                json!(count),
                logs_noncanonical["count"].clone(),
            );
        }
        if let Some(removed) = expected.logs_noncanonical.first_removed {
            record_mismatch_v1(
                &mut log_mismatches,
                "logs.non_canonical",
                "first_removed",
                json!(removed),
                logs_noncanonical["logs"][0]["removed"].clone(),
            );
        }
        if let Some(address) = expected.logs_noncanonical.first_address.as_ref() {
            record_mismatch_v1(
                &mut log_mismatches,
                "logs.non_canonical",
                "first_address",
                json!(address),
                logs_noncanonical["logs"][0]["address"].clone(),
            );
        }
        if let Some(ownership) = expected.logs_noncanonical.first_log_ownership.as_ref() {
            record_mismatch_v1(
                &mut log_mismatches,
                "logs.non_canonical",
                "first_log_ownership",
                json!(ownership),
                logs_noncanonical["logs"][0]["logOwnership"].clone(),
            );
        }
        clear_eth_fullnode_native_worker_runtime_snapshot_for_chain_v1(chain_id);

        for expected_failure in &expected.typed_failures {
            let scope = format!("typed_failure.tx[{}]", expected_failure.tx_index);
            let tx_hash =
                to_hex_prefixed(&store.batches[0].receipts[expected_failure.tx_index].tx_hash);
            let out = run_mainline_query(store, "eth_getTransactionReceipt", &json!([tx_hash]))
                .expect("typed failure receipt");
            let receipt = &out["receipt"];
            if let Some(contract) = expected_failure.failure_classification_contract.as_ref() {
                record_mismatch_v1(
                    &mut typed_failure_mismatches,
                    scope.as_str(),
                    "failureClassificationContract",
                    json!(contract),
                    receipt["failureClassificationContract"].clone(),
                );
            }
            if let Some(status) = expected_failure.status.as_ref() {
                record_mismatch_v1(
                    &mut typed_failure_mismatches,
                    scope.as_str(),
                    "status",
                    json!(status),
                    receipt["status"].clone(),
                );
            }
            if let Some(contract_address_null) = expected_failure.contract_address_null {
                let expected_value = if contract_address_null {
                    Value::Null
                } else {
                    receipt["contractAddress"].clone()
                };
                record_mismatch_v1(
                    &mut typed_failure_mismatches,
                    scope.as_str(),
                    "contractAddress",
                    expected_value,
                    receipt["contractAddress"].clone(),
                );
            }
        }

        json!({
            "schema": "supervm-e2e-geth-parity-report/v1",
            "sample": sample_name,
            "chainId": chain_id,
            "sections": {
                "block": {
                    "mismatchCount": block_mismatches.len(),
                    "mismatches": block_mismatches
                },
                "receipt": {
                    "mismatchCount": receipt_mismatches.len(),
                    "mismatches": receipt_mismatches
                },
                "logs": {
                    "mismatchCount": log_mismatches.len(),
                    "mismatches": log_mismatches
                },
                "typedTxFailure": {
                    "mismatchCount": typed_failure_mismatches.len(),
                    "mismatches": typed_failure_mismatches
                }
            },
            "result": {
                "parity": block_mismatches.is_empty()
                    && receipt_mismatches.is_empty()
                    && log_mismatches.is_empty()
                    && typed_failure_mismatches.is_empty(),
                "totalMismatchCount": block_mismatches.len()
                    + receipt_mismatches.len()
                    + log_mismatches.len()
                    + typed_failure_mismatches.len()
            }
        })
    }

    #[test]
    fn eth_get_transaction_receipt_returns_canonical_receipt() {
        let store = sample_store();
        let block_context = derive_mainline_eth_fullnode_block_contexts_v1(&store)
            .into_iter()
            .next()
            .expect("block context");
        let expected_block_hash = to_hex_prefixed(&block_context.block_hash);
        let expected_parent_block_hash = to_hex_prefixed(&block_context.parent_block_hash);
        let tx_hash = format!("0x{}", "22".repeat(32));
        let out = run_mainline_query(
            &store,
            "eth_getTransactionReceipt",
            &json!([tx_hash.clone()]),
        )
        .expect("query receipt");
        assert_eq!(out["found"].as_bool(), Some(true));
        assert_eq!(
            out["receipt"]["transactionHash"].as_str(),
            Some(tx_hash.as_str())
        );
        assert_eq!(out["receipt"]["status"].as_str(), Some("0x1"));
        assert_eq!(out["receipt"]["blockNumber"].as_str(), Some("0x5"));
        assert_eq!(
            out["receipt"]["blockHash"].as_str(),
            Some(expected_block_hash.as_str())
        );
        assert_eq!(
            out["receipt"]["parentBlockHash"].as_str(),
            Some(expected_parent_block_hash.as_str())
        );
        assert_eq!(
            out["receipt"]["blockViewSource"].as_str(),
            Some("canonical_host_batch")
        );
        assert_eq!(
            out["receipt"]["failureClassificationContract"].as_str(),
            Some(ETH_EXEC_FAILURE_CLASSIFICATION_CONTRACT_V1)
        );
        assert_eq!(out["receipt"]["logs"].as_array().map(Vec::len), Some(1));
    }

    #[test]
    fn eth_get_logs_filters_by_address_and_topic() {
        let store = sample_store();
        let out = run_mainline_query(
            &store,
            "eth_getLogs",
            &json!({
                "address": format!("0x{}", "55".repeat(20)),
                "topics": [format!("0x{}", "66".repeat(32))],
                "fromBlock": "0x5",
                "toBlock": "latest",
            }),
        )
        .expect("query logs");
        assert_eq!(out["count"].as_u64(), Some(1));
        assert_eq!(
            out["logs"][0]["address"].as_str(),
            Some(format!("0x{}", "55".repeat(20)).as_str())
        );
        assert_eq!(out["logs"][0]["blockNumber"].as_str(), Some("0x5"));
        assert_eq!(
            out["logs"][0]["blockViewSource"].as_str(),
            Some("canonical_host_batch")
        );
    }

    #[test]
    fn eth_get_transaction_receipt_returns_failed_call_semantics() {
        let store = sample_store_with_failed_receipts();
        let tx_hash = format!("0x{}", "aa".repeat(32));
        let out = run_mainline_query(
            &store,
            "eth_getTransactionReceipt",
            &json!([tx_hash.clone()]),
        )
        .expect("query failed call receipt");
        assert_eq!(out["found"].as_bool(), Some(true));
        assert_eq!(
            out["receipt"]["transactionHash"].as_str(),
            Some(tx_hash.as_str())
        );
        assert_eq!(out["receipt"]["status"].as_str(), Some("0x0"));
        assert_eq!(out["receipt"]["type"].as_str(), Some("0x2"));
        assert_eq!(out["receipt"]["gasUsed"].as_str(), Some("0xc350"));
        assert_eq!(out["receipt"]["cumulativeGasUsed"].as_str(), Some("0xc350"));
        assert_eq!(out["receipt"]["contractAddress"], Value::Null);
        assert_eq!(out["receipt"]["logs"].as_array().map(Vec::len), Some(0));
        assert_eq!(
            out["receipt"]["logsBloom"].as_str(),
            Some(to_hex_prefixed(&vec![0u8; 256]).as_str())
        );
        assert_eq!(out["receipt"]["revertData"].as_str(), Some("0xdeadbeef"));
        assert_eq!(
            out["receipt"]["failureClassificationContract"].as_str(),
            Some(ETH_EXEC_FAILURE_CLASSIFICATION_CONTRACT_V1)
        );
    }

    #[test]
    fn eth_get_transaction_receipt_returns_failed_deploy_semantics() {
        let store = sample_store_with_failed_receipts();
        let tx_hash = format!("0x{}", "bb".repeat(32));
        let out = run_mainline_query(
            &store,
            "eth_getTransactionReceipt",
            &json!([tx_hash.clone()]),
        )
        .expect("query failed deploy receipt");
        assert_eq!(out["found"].as_bool(), Some(true));
        assert_eq!(
            out["receipt"]["transactionHash"].as_str(),
            Some(tx_hash.as_str())
        );
        assert_eq!(out["receipt"]["status"].as_str(), Some("0x0"));
        assert_eq!(out["receipt"]["type"].as_str(), Some("0x3"));
        assert_eq!(out["receipt"]["gasUsed"].as_str(), Some("0x1d4c0"));
        assert_eq!(
            out["receipt"]["cumulativeGasUsed"].as_str(),
            Some("0x29810")
        );
        assert_eq!(out["receipt"]["contractAddress"], Value::Null);
        assert_eq!(out["receipt"]["logs"].as_array().map(Vec::len), Some(0));
        assert_eq!(
            out["receipt"]["logsBloom"].as_str(),
            Some(to_hex_prefixed(&vec![0u8; 256]).as_str())
        );
        assert_eq!(out["receipt"]["revertData"].as_str(), Some("0xcafe"));
        assert_eq!(
            out["receipt"]["failureClassificationContract"].as_str(),
            Some(ETH_EXEC_FAILURE_CLASSIFICATION_CONTRACT_V1)
        );
    }

    #[test]
    fn eth_get_logs_returns_empty_for_failed_receipts_without_logs() {
        let store = sample_store_with_failed_receipts();
        let out = run_mainline_query(&store, "eth_getLogs", &json!({}))
            .expect("query failed receipts logs");
        assert_eq!(out["count"].as_u64(), Some(0));
        assert_eq!(out["logs"].as_array().map(Vec::len), Some(0));
    }

    #[test]
    fn eth_success_receipt_log_and_block_views_are_consistent_v1() {
        let store = sample_store_with_success_receipts();
        let expected_logs_bloom = to_hex_prefixed(&combine_receipt_log_bloom(
            store.batches[0].receipts.as_slice(),
        ));

        let block_out =
            run_mainline_query(&store, "eth_getBlockByNumber", &json!(["latest", true]))
                .expect("block by number");
        assert_eq!(block_out["found"].as_bool(), Some(true));
        assert_eq!(block_out["block"]["number"].as_str(), Some("0x7"));
        assert_eq!(block_out["block"]["gasUsed"].as_str(), Some("0x17318"));
        assert_eq!(
            block_out["block"]["cumulativeGasUsed"].as_str(),
            Some("0x17318")
        );
        assert_eq!(
            block_out["block"]["logsBloom"].as_str(),
            Some(expected_logs_bloom.as_str())
        );
        assert_eq!(
            block_out["block"]["transactions"][0]["contractAddress"].as_str(),
            Some(format!("0x{}", "a1".repeat(20)).as_str())
        );
        assert_eq!(
            block_out["block"]["transactions"][1]["contractAddress"],
            Value::Null
        );

        let deploy_receipt = run_mainline_query(
            &store,
            "eth_getTransactionReceipt",
            &json!([format!("0x{}", "cc".repeat(32))]),
        )
        .expect("deploy receipt");
        assert_eq!(deploy_receipt["found"].as_bool(), Some(true));
        assert_eq!(deploy_receipt["receipt"]["status"].as_str(), Some("0x1"));
        assert_eq!(
            deploy_receipt["receipt"]["contractAddress"].as_str(),
            Some(format!("0x{}", "a1".repeat(20)).as_str())
        );
        assert_eq!(
            deploy_receipt["receipt"]["gasUsed"].as_str(),
            Some("0xcf08")
        );
        assert_eq!(
            deploy_receipt["receipt"]["cumulativeGasUsed"].as_str(),
            Some("0xcf08")
        );
        assert_eq!(
            deploy_receipt["receipt"]["logs"].as_array().map(Vec::len),
            Some(1)
        );
        assert_eq!(deploy_receipt["receipt"]["revertData"], Value::Null);

        let call_receipt = run_mainline_query(
            &store,
            "eth_getTransactionReceipt",
            &json!([format!("0x{}", "dd".repeat(32))]),
        )
        .expect("call receipt");
        assert_eq!(call_receipt["found"].as_bool(), Some(true));
        assert_eq!(call_receipt["receipt"]["status"].as_str(), Some("0x1"));
        assert_eq!(call_receipt["receipt"]["contractAddress"], Value::Null);
        assert_eq!(call_receipt["receipt"]["gasUsed"].as_str(), Some("0xa410"));
        assert_eq!(
            call_receipt["receipt"]["cumulativeGasUsed"].as_str(),
            Some("0x17318")
        );
        assert_eq!(
            call_receipt["receipt"]["logs"].as_array().map(Vec::len),
            Some(2)
        );
        assert_eq!(call_receipt["receipt"]["revertData"], Value::Null);

        let logs_out = run_mainline_query(&store, "eth_getLogs", &json!({})).expect("logs query");
        assert_eq!(logs_out["count"].as_u64(), Some(3));
        assert_eq!(logs_out["logs"][0]["removed"].as_bool(), Some(false));
        assert_eq!(logs_out["logs"][1]["removed"].as_bool(), Some(false));
        assert_eq!(logs_out["logs"][2]["removed"].as_bool(), Some(false));
    }

    #[test]
    fn eth_queries_include_authoritative_native_lifecycle_metadata_when_tracked() {
        let chain_id = 99_160_611_u64;
        clear_eth_fullnode_native_worker_runtime_snapshot_for_chain_v1(chain_id);
        let store = sample_store_with_chain_id(chain_id);
        let block_context = derive_mainline_eth_fullnode_block_contexts_v1(&store)
            .into_iter()
            .next()
            .expect("block context");
        set_eth_fullnode_native_worker_runtime_snapshot_v1(
            chain_id,
            sample_native_runtime_snapshot_for_block_context(chain_id, &block_context, None),
        );

        let block_out =
            run_mainline_query(&store, "eth_getBlockByNumber", &json!(["latest", false]))
                .expect("block by number");
        assert_eq!(
            block_out["block"]["nativeLifecycleTracked"].as_bool(),
            Some(true)
        );
        assert_eq!(
            block_out["block"]["nativeLifecycleStage"].as_str(),
            Some("canonical")
        );
        assert_eq!(
            block_out["block"]["blockOwnership"].as_str(),
            Some("canonical")
        );
        assert_eq!(
            block_out["block"]["authoritativeCanonicalMatch"].as_bool(),
            Some(true)
        );

        let receipt_out = run_mainline_query(
            &store,
            "eth_getTransactionReceipt",
            &json!([format!("0x{}", "22".repeat(32))]),
        )
        .expect("receipt query");
        assert_eq!(
            receipt_out["receipt"]["receiptOwnership"].as_str(),
            Some("canonical")
        );
        assert_eq!(
            receipt_out["receipt"]["nativeLifecycleStage"].as_str(),
            Some("canonical")
        );

        let logs_out = run_mainline_query(&store, "eth_getLogs", &json!({})).expect("logs query");
        assert_eq!(
            logs_out["logs"][0]["logOwnership"].as_str(),
            Some("canonical")
        );
        assert_eq!(logs_out["logs"][0]["removed"].as_bool(), Some(false));

        clear_eth_fullnode_native_worker_runtime_snapshot_for_chain_v1(chain_id);
    }

    #[test]
    fn eth_queries_mark_noncanonical_blocks_when_authoritative_hash_differs() {
        let chain_id = 99_160_612_u64;
        clear_eth_fullnode_native_worker_runtime_snapshot_for_chain_v1(chain_id);
        let store = sample_store_with_chain_id(chain_id);
        let block_context = derive_mainline_eth_fullnode_block_contexts_v1(&store)
            .into_iter()
            .next()
            .expect("block context");
        let authoritative_hash = [0xee; 32];
        set_eth_fullnode_native_worker_runtime_snapshot_v1(
            chain_id,
            sample_native_runtime_snapshot_for_block_context(
                chain_id,
                &block_context,
                Some(authoritative_hash),
            ),
        );

        let block_out =
            run_mainline_query(&store, "eth_getBlockByNumber", &json!(["latest", false]))
                .expect("block by number");
        assert_eq!(
            block_out["block"]["nativeLifecycleTracked"].as_bool(),
            Some(true)
        );
        assert_eq!(
            block_out["block"]["nativeLifecycleStage"].as_str(),
            Some("non_canonical")
        );
        assert_eq!(
            block_out["block"]["blockOwnership"].as_str(),
            Some("non_canonical")
        );
        assert_eq!(
            block_out["block"]["authoritativeCanonicalMatch"].as_bool(),
            Some(false)
        );
        assert_eq!(
            block_out["block"]["authoritativeCanonicalBlockHash"].as_str(),
            Some(to_hex_prefixed(&authoritative_hash).as_str())
        );

        let receipt_out = run_mainline_query(
            &store,
            "eth_getTransactionReceipt",
            &json!([format!("0x{}", "22".repeat(32))]),
        )
        .expect("receipt query");
        assert_eq!(
            receipt_out["receipt"]["receiptOwnership"].as_str(),
            Some("non_canonical")
        );

        let logs_out = run_mainline_query(&store, "eth_getLogs", &json!({})).expect("logs query");
        assert_eq!(
            logs_out["logs"][0]["logOwnership"].as_str(),
            Some("non_canonical")
        );
        assert_eq!(logs_out["logs"][0]["removed"].as_bool(), Some(true));

        clear_eth_fullnode_native_worker_runtime_snapshot_for_chain_v1(chain_id);
    }

    #[test]
    fn eth_queries_switch_lifecycle_immediately_when_reorg_view_changes_v1() {
        let chain_id = 99_160_613_u64;
        clear_eth_fullnode_native_worker_runtime_snapshot_for_chain_v1(chain_id);
        let store = sample_store_with_chain_id(chain_id);
        let block_context = derive_mainline_eth_fullnode_block_contexts_v1(&store)
            .into_iter()
            .next()
            .expect("block context");
        let tx_hash = format!("0x{}", "22".repeat(32));

        set_eth_fullnode_native_worker_runtime_snapshot_v1(
            chain_id,
            sample_native_runtime_snapshot_for_block_context(
                chain_id,
                &block_context,
                Some([0xee; 32]),
            ),
        );
        let noncanonical_receipt = run_mainline_query(
            &store,
            "eth_getTransactionReceipt",
            &json!([tx_hash.clone()]),
        )
        .expect("non-canonical receipt");
        assert_eq!(
            noncanonical_receipt["receipt"]["receiptOwnership"].as_str(),
            Some("non_canonical")
        );
        let noncanonical_logs =
            run_mainline_query(&store, "eth_getLogs", &json!({})).expect("non-canonical logs");
        assert_eq!(
            noncanonical_logs["logs"][0]["logOwnership"].as_str(),
            Some("non_canonical")
        );
        assert_eq!(
            noncanonical_logs["logs"][0]["removed"].as_bool(),
            Some(true)
        );

        set_eth_fullnode_native_worker_runtime_snapshot_v1(
            chain_id,
            sample_native_runtime_snapshot_for_block_context(chain_id, &block_context, None),
        );
        let canonical_receipt =
            run_mainline_query(&store, "eth_getTransactionReceipt", &json!([tx_hash]))
                .expect("canonical receipt");
        assert_eq!(
            canonical_receipt["receipt"]["receiptOwnership"].as_str(),
            Some("canonical")
        );
        let canonical_logs =
            run_mainline_query(&store, "eth_getLogs", &json!({})).expect("canonical logs");
        assert_eq!(
            canonical_logs["logs"][0]["logOwnership"].as_str(),
            Some("canonical")
        );
        assert_eq!(canonical_logs["logs"][0]["removed"].as_bool(), Some(false));

        clear_eth_fullnode_native_worker_runtime_snapshot_for_chain_v1(chain_id);
    }

    #[test]
    fn eth_block_number_returns_head_view() {
        let store = sample_store();
        let out = run_mainline_query(&store, "eth_blockNumber", &json!([])).expect("block number");
        assert_eq!(out["result"].as_str(), Some("0x5"));
        assert_eq!(out["head"]["blockNumber"].as_str(), Some("0x5"));
        assert_eq!(
            out["head"]["blockViewSource"].as_str(),
            Some("canonical_host_batch")
        );
    }

    #[test]
    fn eth_get_block_by_number_returns_formal_block_view() {
        let store = sample_store();
        let block_context = derive_mainline_eth_fullnode_block_contexts_v1(&store)
            .into_iter()
            .next()
            .expect("block context");
        let out = run_mainline_query(&store, "eth_getBlockByNumber", &json!(["latest", true]))
            .expect("block by number");
        assert_eq!(out["found"].as_bool(), Some(true));
        assert_eq!(out["block"]["number"].as_str(), Some("0x5"));
        assert_eq!(
            out["block"]["hash"].as_str(),
            Some(to_hex_prefixed(&block_context.block_hash).as_str())
        );
        assert_eq!(
            out["block"]["blockViewSource"].as_str(),
            Some("canonical_host_batch")
        );
        assert_eq!(
            out["block"]["transactions"][0]["transactionIndex"].as_str(),
            Some("0x0")
        );
    }

    #[test]
    fn eth_get_block_by_number_sanitizes_failed_receipt_bloom_and_contract_address() {
        let store = sample_store_with_failed_receipts();
        let out = run_mainline_query(&store, "eth_getBlockByNumber", &json!(["latest", true]))
            .expect("block by number");
        assert_eq!(out["found"].as_bool(), Some(true));
        assert_eq!(
            out["block"]["logsBloom"].as_str(),
            Some(to_hex_prefixed(&vec![0u8; 256]).as_str())
        );
        assert_eq!(
            out["block"]["transactions"].as_array().map(Vec::len),
            Some(2)
        );
        assert_eq!(
            out["block"]["transactions"][0]["status"].as_str(),
            Some("0x0")
        );
        assert_eq!(
            out["block"]["transactions"][1]["status"].as_str(),
            Some("0x0")
        );
        assert_eq!(
            out["block"]["transactions"][0]["contractAddress"],
            Value::Null
        );
        assert_eq!(
            out["block"]["transactions"][1]["contractAddress"],
            Value::Null
        );
    }

    #[test]
    fn eth_get_block_by_hash_returns_formal_block_view() {
        let store = sample_store();
        let block_context = derive_mainline_eth_fullnode_block_contexts_v1(&store)
            .into_iter()
            .next()
            .expect("block context");
        let block_hash = to_hex_prefixed(&block_context.block_hash);
        let out = run_mainline_query(
            &store,
            "eth_getBlockByHash",
            &json!([block_hash.clone(), false]),
        )
        .expect("block by hash");
        assert_eq!(out["found"].as_bool(), Some(true));
        assert_eq!(out["blockHash"].as_str(), Some(block_hash.as_str()));
        assert_eq!(out["block"]["hash"].as_str(), Some(block_hash.as_str()));
        assert_eq!(
            out["block"]["transactions"][0].as_str(),
            Some(format!("0x{}", "22".repeat(32)).as_str())
        );
    }

    #[test]
    fn eth_end_to_end_adapter_canonical_query_parity_with_geth_baseline_v1() {
        let chain_id = 99_160_714_u64;
        clear_eth_fullnode_native_worker_runtime_snapshot_for_chain_v1(chain_id);
        let store = build_adapter_to_canonical_store_v1(chain_id);
        let block_context = derive_mainline_eth_fullnode_block_contexts_v1(&store)
            .into_iter()
            .next()
            .expect("block context");
        let expected_block_hash = to_hex_prefixed(&block_context.block_hash);
        let expected_logs_bloom = to_hex_prefixed(&combine_receipt_log_bloom(
            store.batches[0].receipts.as_slice(),
        ));

        let block_out =
            run_mainline_query(&store, "eth_getBlockByNumber", &json!(["latest", true]))
                .expect("block by number");
        assert_eq!(block_out["found"].as_bool(), Some(true));
        assert_eq!(block_out["block"]["number"].as_str(), Some("0x88"));
        assert_eq!(
            block_out["block"]["hash"].as_str(),
            Some(expected_block_hash.as_str())
        );
        assert_eq!(
            block_out["block"]["logsBloom"].as_str(),
            Some(expected_logs_bloom.as_str())
        );
        assert_eq!(
            block_out["block"]["transactions"].as_array().map(Vec::len),
            Some(3)
        );
        assert_eq!(
            block_out["block"]["transactions"][0]["type"].as_str(),
            Some("0x1")
        );
        assert_eq!(
            block_out["block"]["transactions"][1]["type"].as_str(),
            Some("0x2")
        );
        assert_eq!(
            block_out["block"]["transactions"][2]["type"].as_str(),
            Some("0x3")
        );
        assert_eq!(
            block_out["block"]["transactions"][0]["status"].as_str(),
            Some("0x1")
        );
        assert_eq!(
            block_out["block"]["transactions"][1]["status"].as_str(),
            Some("0x0")
        );
        assert_eq!(
            block_out["block"]["transactions"][2]["status"].as_str(),
            Some("0x0")
        );
        assert_eq!(
            block_out["block"]["transactions"][0]["contractAddress"].as_str(),
            Some(format!("0x{}", "a1".repeat(20)).as_str())
        );
        assert_eq!(
            block_out["block"]["transactions"][1]["contractAddress"],
            Value::Null
        );
        assert_eq!(
            block_out["block"]["transactions"][2]["contractAddress"],
            Value::Null
        );

        let tx0_hash = to_hex_prefixed(&store.batches[0].receipts[0].tx_hash);
        let tx1_hash = to_hex_prefixed(&store.batches[0].receipts[1].tx_hash);
        let tx2_hash = to_hex_prefixed(&store.batches[0].receipts[2].tx_hash);

        let receipt0 = run_mainline_query(&store, "eth_getTransactionReceipt", &json!([tx0_hash]))
            .expect("tx0 receipt");
        assert_eq!(receipt0["found"].as_bool(), Some(true));
        assert_eq!(receipt0["receipt"]["status"].as_str(), Some("0x1"));
        assert_eq!(receipt0["receipt"]["type"].as_str(), Some("0x1"));
        assert_eq!(
            receipt0["receipt"]["contractAddress"].as_str(),
            Some(format!("0x{}", "a1".repeat(20)).as_str())
        );
        assert_eq!(receipt0["receipt"]["gasUsed"].as_str(), Some("0xea60"));
        assert_eq!(
            receipt0["receipt"]["cumulativeGasUsed"].as_str(),
            Some("0xea60")
        );
        assert_eq!(
            receipt0["receipt"]["logs"].as_array().map(Vec::len),
            Some(1)
        );
        assert_eq!(receipt0["receipt"]["revertData"], Value::Null);

        let receipt1 = run_mainline_query(&store, "eth_getTransactionReceipt", &json!([tx1_hash]))
            .expect("tx1 receipt");
        assert_eq!(receipt1["found"].as_bool(), Some(true));
        assert_eq!(receipt1["receipt"]["status"].as_str(), Some("0x0"));
        assert_eq!(receipt1["receipt"]["type"].as_str(), Some("0x2"));
        assert_eq!(receipt1["receipt"]["contractAddress"], Value::Null);
        assert_eq!(receipt1["receipt"]["gasUsed"].as_str(), Some("0x7530"));
        assert_eq!(
            receipt1["receipt"]["cumulativeGasUsed"].as_str(),
            Some("0x15f90")
        );
        assert_eq!(
            receipt1["receipt"]["logs"].as_array().map(Vec::len),
            Some(0)
        );
        assert_eq!(
            receipt1["receipt"]["revertData"].as_str(),
            Some("0x08c379a0")
        );

        let receipt2 = run_mainline_query(&store, "eth_getTransactionReceipt", &json!([tx2_hash]))
            .expect("tx2 receipt");
        assert_eq!(receipt2["found"].as_bool(), Some(true));
        assert_eq!(receipt2["receipt"]["status"].as_str(), Some("0x0"));
        assert_eq!(receipt2["receipt"]["type"].as_str(), Some("0x3"));
        assert_eq!(receipt2["receipt"]["contractAddress"], Value::Null);
        assert_eq!(receipt2["receipt"]["gasUsed"].as_str(), Some("0x7530"));
        assert_eq!(
            receipt2["receipt"]["cumulativeGasUsed"].as_str(),
            Some("0x1d4c0")
        );
        assert_eq!(
            receipt2["receipt"]["logs"].as_array().map(Vec::len),
            Some(0)
        );
        assert_eq!(receipt2["receipt"]["revertData"], Value::Null);
        assert_eq!(
            receipt2["receipt"]["failureClassificationContract"].as_str(),
            Some(ETH_EXEC_FAILURE_CLASSIFICATION_CONTRACT_V1)
        );

        let logs_out = run_mainline_query(&store, "eth_getLogs", &json!({})).expect("logs query");
        assert_eq!(logs_out["count"].as_u64(), Some(1));
        assert_eq!(
            logs_out["logs"][0]["address"].as_str(),
            Some(format!("0x{}", "a1".repeat(20)).as_str())
        );
        assert_eq!(logs_out["logs"][0]["removed"].as_bool(), Some(false));

        set_eth_fullnode_native_worker_runtime_snapshot_v1(
            chain_id,
            sample_native_runtime_snapshot_for_block_context(
                chain_id,
                &block_context,
                Some([0xfe; 32]),
            ),
        );
        let logs_noncanonical =
            run_mainline_query(&store, "eth_getLogs", &json!({})).expect("logs non-canonical");
        assert_eq!(logs_noncanonical["count"].as_u64(), Some(1));
        assert_eq!(
            logs_noncanonical["logs"][0]["logOwnership"].as_str(),
            Some("non_canonical")
        );
        assert_eq!(
            logs_noncanonical["logs"][0]["removed"].as_bool(),
            Some(true)
        );

        let receipt_noncanonical = run_mainline_query(
            &store,
            "eth_getTransactionReceipt",
            &json!([to_hex_prefixed(&store.batches[0].receipts[0].tx_hash)]),
        )
        .expect("receipt non-canonical");
        assert_eq!(
            receipt_noncanonical["receipt"]["receiptOwnership"].as_str(),
            Some("non_canonical")
        );

        set_eth_fullnode_native_worker_runtime_snapshot_v1(
            chain_id,
            sample_native_runtime_snapshot_for_block_context(chain_id, &block_context, None),
        );
        let logs_canonical =
            run_mainline_query(&store, "eth_getLogs", &json!({})).expect("logs canonical");
        assert_eq!(logs_canonical["logs"][0]["removed"].as_bool(), Some(false));

        clear_eth_fullnode_native_worker_runtime_snapshot_for_chain_v1(chain_id);
    }

    #[test]
    fn eth_end_to_end_geth_sample_batch_parity_report_v1() {
        let chain_id = 99_160_715_u64;
        clear_eth_fullnode_native_worker_runtime_snapshot_for_chain_v1(chain_id);
        let store = build_adapter_to_canonical_store_v1(chain_id);
        let block_context = derive_mainline_eth_fullnode_block_contexts_v1(&store)
            .into_iter()
            .next()
            .expect("block context");
        let expected_block_hash = to_hex_prefixed(&block_context.block_hash);
        let expected_logs_bloom = to_hex_prefixed(&combine_receipt_log_bloom(
            store.batches[0].receipts.as_slice(),
        ));

        let tx0_hash = to_hex_prefixed(&store.batches[0].receipts[0].tx_hash);
        let tx1_hash = to_hex_prefixed(&store.batches[0].receipts[1].tx_hash);
        let tx2_hash = to_hex_prefixed(&store.batches[0].receipts[2].tx_hash);

        let mut block_mismatches = Vec::new();
        let mut receipt_mismatches = Vec::new();
        let mut log_mismatches = Vec::new();
        let mut typed_failure_mismatches = Vec::new();

        let block_out =
            run_mainline_query(&store, "eth_getBlockByNumber", &json!(["latest", true]))
                .expect("block by number");
        let block = &block_out["block"];
        record_mismatch_v1(
            &mut block_mismatches,
            "block",
            "number",
            json!("0x88"),
            block["number"].clone(),
        );
        record_mismatch_v1(
            &mut block_mismatches,
            "block",
            "hash",
            json!(expected_block_hash),
            block["hash"].clone(),
        );
        record_mismatch_v1(
            &mut block_mismatches,
            "block",
            "logsBloom",
            json!(expected_logs_bloom),
            block["logsBloom"].clone(),
        );
        record_mismatch_v1(
            &mut block_mismatches,
            "block",
            "tx_count",
            json!(3),
            json!(block["transactions"].as_array().map(Vec::len).unwrap_or(0)),
        );
        record_mismatch_v1(
            &mut block_mismatches,
            "block.tx[0]",
            "type",
            json!("0x1"),
            block["transactions"][0]["type"].clone(),
        );
        record_mismatch_v1(
            &mut block_mismatches,
            "block.tx[1]",
            "type",
            json!("0x2"),
            block["transactions"][1]["type"].clone(),
        );
        record_mismatch_v1(
            &mut block_mismatches,
            "block.tx[2]",
            "type",
            json!("0x3"),
            block["transactions"][2]["type"].clone(),
        );
        record_mismatch_v1(
            &mut block_mismatches,
            "block.tx[0]",
            "contractAddress",
            json!(format!("0x{}", "a1".repeat(20))),
            block["transactions"][0]["contractAddress"].clone(),
        );
        record_mismatch_v1(
            &mut block_mismatches,
            "block.tx[1]",
            "contractAddress",
            Value::Null,
            block["transactions"][1]["contractAddress"].clone(),
        );
        record_mismatch_v1(
            &mut block_mismatches,
            "block.tx[2]",
            "contractAddress",
            Value::Null,
            block["transactions"][2]["contractAddress"].clone(),
        );

        let receipt_expectations = vec![
            (
                "type1_success",
                tx0_hash.clone(),
                json!({
                    "status": "0x1",
                    "type": "0x1",
                    "gasUsed": "0xea60",
                    "cumulativeGasUsed": "0xea60",
                    "contractAddress": format!("0x{}", "a1".repeat(20)),
                    "revertData": Value::Null,
                    "logCount": 1
                }),
            ),
            (
                "type2_revert_failure",
                tx1_hash.clone(),
                json!({
                    "status": "0x0",
                    "type": "0x2",
                    "gasUsed": "0x7530",
                    "cumulativeGasUsed": "0x15f90",
                    "contractAddress": Value::Null,
                    "revertData": "0x08c379a0",
                    "logCount": 0
                }),
            ),
            (
                "type3_invalid_failure",
                tx2_hash.clone(),
                json!({
                    "status": "0x0",
                    "type": "0x3",
                    "gasUsed": "0x7530",
                    "cumulativeGasUsed": "0x1d4c0",
                    "contractAddress": Value::Null,
                    "revertData": Value::Null,
                    "logCount": 0
                }),
            ),
        ];
        for (scope, tx_hash, expected) in receipt_expectations {
            let out = run_mainline_query(&store, "eth_getTransactionReceipt", &json!([tx_hash]))
                .expect("receipt query");
            let receipt = &out["receipt"];
            record_mismatch_v1(
                &mut receipt_mismatches,
                scope,
                "status",
                expected["status"].clone(),
                receipt["status"].clone(),
            );
            record_mismatch_v1(
                &mut receipt_mismatches,
                scope,
                "type",
                expected["type"].clone(),
                receipt["type"].clone(),
            );
            record_mismatch_v1(
                &mut receipt_mismatches,
                scope,
                "gasUsed",
                expected["gasUsed"].clone(),
                receipt["gasUsed"].clone(),
            );
            record_mismatch_v1(
                &mut receipt_mismatches,
                scope,
                "cumulativeGasUsed",
                expected["cumulativeGasUsed"].clone(),
                receipt["cumulativeGasUsed"].clone(),
            );
            record_mismatch_v1(
                &mut receipt_mismatches,
                scope,
                "contractAddress",
                expected["contractAddress"].clone(),
                receipt["contractAddress"].clone(),
            );
            record_mismatch_v1(
                &mut receipt_mismatches,
                scope,
                "revertData",
                expected["revertData"].clone(),
                receipt["revertData"].clone(),
            );
            record_mismatch_v1(
                &mut receipt_mismatches,
                scope,
                "logCount",
                expected["logCount"].clone(),
                json!(receipt["logs"].as_array().map(Vec::len).unwrap_or(0)),
            );
            if scope.contains("failure") {
                record_mismatch_v1(
                    &mut typed_failure_mismatches,
                    scope,
                    "failureClassificationContract",
                    json!(ETH_EXEC_FAILURE_CLASSIFICATION_CONTRACT_V1),
                    receipt["failureClassificationContract"].clone(),
                );
                record_mismatch_v1(
                    &mut typed_failure_mismatches,
                    scope,
                    "status_is_failed",
                    json!("0x0"),
                    receipt["status"].clone(),
                );
                record_mismatch_v1(
                    &mut typed_failure_mismatches,
                    scope,
                    "contractAddress_null_on_failure",
                    Value::Null,
                    receipt["contractAddress"].clone(),
                );
            }
        }

        let logs_out = run_mainline_query(&store, "eth_getLogs", &json!({})).expect("logs query");
        record_mismatch_v1(
            &mut log_mismatches,
            "logs.canonical",
            "count",
            json!(1),
            logs_out["count"].clone(),
        );
        record_mismatch_v1(
            &mut log_mismatches,
            "logs.canonical",
            "removed",
            json!(false),
            logs_out["logs"][0]["removed"].clone(),
        );
        record_mismatch_v1(
            &mut log_mismatches,
            "logs.canonical",
            "address",
            json!(format!("0x{}", "a1".repeat(20))),
            logs_out["logs"][0]["address"].clone(),
        );

        set_eth_fullnode_native_worker_runtime_snapshot_v1(
            chain_id,
            sample_native_runtime_snapshot_for_block_context(
                chain_id,
                &block_context,
                Some([0xfe; 32]),
            ),
        );
        let logs_noncanonical =
            run_mainline_query(&store, "eth_getLogs", &json!({})).expect("logs non-canonical");
        record_mismatch_v1(
            &mut log_mismatches,
            "logs.non_canonical",
            "count",
            json!(1),
            logs_noncanonical["count"].clone(),
        );
        record_mismatch_v1(
            &mut log_mismatches,
            "logs.non_canonical",
            "removed",
            json!(true),
            logs_noncanonical["logs"][0]["removed"].clone(),
        );
        record_mismatch_v1(
            &mut log_mismatches,
            "logs.non_canonical",
            "logOwnership",
            json!("non_canonical"),
            logs_noncanonical["logs"][0]["logOwnership"].clone(),
        );
        clear_eth_fullnode_native_worker_runtime_snapshot_for_chain_v1(chain_id);

        let report = json!({
            "schema": "supervm-e2e-geth-parity-report/v1",
            "chainId": chain_id,
            "sections": {
                "block": {
                    "checked": 10,
                    "mismatchCount": block_mismatches.len(),
                    "mismatches": block_mismatches
                },
                "receipt": {
                    "checked": 21,
                    "mismatchCount": receipt_mismatches.len(),
                    "mismatches": receipt_mismatches
                },
                "logs": {
                    "checked": 6,
                    "mismatchCount": log_mismatches.len(),
                    "mismatches": log_mismatches
                },
                "typedTxFailure": {
                    "checked": 6,
                    "mismatchCount": typed_failure_mismatches.len(),
                    "mismatches": typed_failure_mismatches
                }
            },
            "result": {
                "parity": block_mismatches.is_empty()
                    && receipt_mismatches.is_empty()
                    && log_mismatches.is_empty()
                    && typed_failure_mismatches.is_empty(),
                "totalMismatchCount": block_mismatches.len()
                    + receipt_mismatches.len()
                    + log_mismatches.len()
                    + typed_failure_mismatches.len()
            }
        });
        println!(
            "{}",
            serde_json::to_string_pretty(&report).expect("render parity report")
        );

        assert_eq!(
            report["result"]["totalMismatchCount"].as_u64(),
            Some(0),
            "geth parity report mismatch: {}",
            serde_json::to_string_pretty(&report).expect("render parity mismatch")
        );
    }

    #[test]
    fn eth_end_to_end_geth_sample_batch_parity_report_from_files_v1() {
        let sample_dir = std::env::var("NOVOVM_GETH_PARITY_SAMPLE_DIR")
            .ok()
            .map(PathBuf::from)
            .unwrap_or_else(default_geth_parity_sample_dir_v1);
        assert!(
            sample_dir.exists(),
            "geth parity sample dir missing: {}",
            sample_dir.display()
        );

        let mut sample_paths = fs::read_dir(&sample_dir)
            .unwrap_or_else(|error| {
                panic!(
                    "read geth parity sample dir failed {}: {error}",
                    sample_dir.display()
                )
            })
            .filter_map(|entry| entry.ok().map(|v| v.path()))
            .filter(|path| {
                path.file_name()
                    .and_then(|v| v.to_str())
                    .map(|name| name.to_ascii_lowercase().ends_with(".sample.json"))
                    .unwrap_or(false)
            })
            .collect::<Vec<_>>();
        sample_paths.sort();
        assert!(
            !sample_paths.is_empty(),
            "geth parity sample dir has no *.sample.json files: {}",
            sample_dir.display()
        );

        let mut reports = Vec::new();
        let mut total_mismatch_count: u64 = 0;
        let mut failed_samples = Vec::new();

        for sample_path in &sample_paths {
            let payload = fs::read_to_string(sample_path).unwrap_or_else(|error| {
                panic!("read sample file failed {}: {error}", sample_path.display())
            });
            let sample: GethParitySampleFileV1 = serde_json::from_str(payload.as_str())
                .unwrap_or_else(|error| {
                    panic!(
                        "parse sample file failed {}: {error}",
                        sample_path.display()
                    )
                });
            assert_eq!(
                sample.schema,
                "supervm-e2e-geth-parity-sample/v1",
                "unexpected parity sample schema: {} ({})",
                sample.schema,
                sample_path.display()
            );
            let store =
                load_geth_parity_sample_store_v1(sample_path, &sample).unwrap_or_else(|error| {
                    panic!(
                        "load sample store failed {}: {error}",
                        sample_path.display()
                    )
                });
            let reorg_hash_override = sample.reorg_hash_hex.as_deref().map(|raw| {
                decode_fixed_32_hex_v1(raw).unwrap_or_else(|error| {
                    panic!(
                        "decode reorg_hash_hex failed {} value={} err={error}",
                        sample_path.display(),
                        raw
                    )
                })
            });
            let report = run_geth_parity_report_from_store_v1(
                sample.name.as_str(),
                &store,
                &sample.expected,
                reorg_hash_override,
            );
            total_mismatch_count += report["result"]["totalMismatchCount"].as_u64().unwrap_or(0);
            if report["result"]["parity"].as_bool() != Some(true) {
                failed_samples.push(sample.name.clone());
            }
            reports.push(report);
        }

        let aggregate = json!({
            "schema": "supervm-e2e-geth-parity-batch-report/v1",
            "sampleDir": sample_dir.display().to_string(),
            "sampleCount": reports.len(),
            "totalMismatchCount": total_mismatch_count,
            "failedSamples": failed_samples,
            "reports": reports,
        });
        println!(
            "{}",
            serde_json::to_string_pretty(&aggregate).expect("render aggregate parity report")
        );

        assert_eq!(
            total_mismatch_count,
            0,
            "geth parity batch mismatch: {}",
            serde_json::to_string_pretty(&aggregate).expect("render aggregate mismatch")
        );
    }

    #[test]
    fn eth_syncing_uses_formal_chain_view_and_runtime_gap() {
        let mut store = sample_store();
        store.chain_id = 99_160_401;
        set_network_runtime_sync_status(
            store.chain_id,
            NetworkRuntimeSyncStatus {
                peer_count: 2,
                starting_block: 4,
                current_block: 5,
                highest_block: 7,
            },
        );
        novovm_network::set_network_runtime_native_sync_status(
            store.chain_id,
            NetworkRuntimeNativeSyncStatusV1 {
                phase: NetworkRuntimeNativeSyncPhaseV1::Headers,
                peer_count: 3,
                starting_block: 4,
                current_block: 5,
                highest_block: 8,
                updated_at_unix_millis: 1,
            },
        );
        let out = run_mainline_query(&store, "eth_syncing", &json!([])).expect("syncing");
        assert_eq!(out["result"]["currentBlock"].as_str(), Some("0x5"));
        assert_eq!(out["result"]["highestBlock"].as_str(), Some("0x8"));
        assert_eq!(
            out["result"]["blockViewSource"].as_str(),
            Some("native_chain_sync")
        );
        assert_eq!(out["syncView"]["nativeSyncPhase"].as_str(), Some("headers"));
        assert_eq!(out["head"]["blockNumber"].as_str(), Some("0x5"));
    }

    #[test]
    fn runtime_query_returns_formal_native_worker_snapshot() {
        let chain_id = 99_160_499_u64;
        clear_eth_fullnode_native_worker_runtime_snapshot_for_chain_v1(chain_id);
        set_eth_fullnode_native_worker_runtime_snapshot_v1(
            chain_id,
            EthFullnodeNativeWorkerRuntimeSnapshotV1 {
                schema: novovm_network::ETH_FULLNODE_NATIVE_WORKER_RUNTIME_SCHEMA_V1.to_string(),
                chain_id,
                updated_at_unix_ms: 77,
                candidate_peer_ids: vec![10, 11],
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
                body_updates: 1,
                sync_requests: 1,
                inbound_frames: 3,
                head_view: Some(EthFullnodeHeadViewV1 {
                    source: novovm_network::EthFullnodeBlockViewSource::NativeChainSync,
                    chain_id,
                    block_number: 0x80,
                    block_hash: [0xaa; 32],
                    parent_block_hash: [0xa9; 32],
                    state_root: [0xbb; 32],
                    state_version: 9,
                    source_priority_policy:
                        novovm_network::derive_eth_fullnode_source_priority_policy_v1(
                            None, None, None, None,
                        ),
                }),
                sync_view: Some(novovm_network::EthFullnodeSyncViewV1 {
                    source: novovm_network::EthFullnodeBlockViewSource::NativeChainSync,
                    chain_id,
                    peer_count: 2,
                    starting_block_number: 0x70,
                    current_block_number: 0x80,
                    highest_block_number: 0x90,
                    current_block_hash: [0xaa; 32],
                    parent_block_hash: [0xa9; 32],
                    current_state_root: [0xbb; 32],
                    current_state_version: 9,
                    native_sync_phase: Some("bodies".to_string()),
                    syncing: true,
                    source_priority_policy:
                        novovm_network::derive_eth_fullnode_source_priority_policy_v1(
                            None, None, None, None,
                        ),
                }),
                native_canonical_chain: Some(
                    novovm_network::NetworkRuntimeNativeCanonicalChainStateV1 {
                        chain_id,
                        lifecycle_stage:
                            novovm_network::NetworkRuntimeNativeCanonicalLifecycleStageV1::Advanced,
                        head: Some(novovm_network::NetworkRuntimeNativeCanonicalBlockStateV1 {
                            chain_id,
                            number: 0x80,
                            hash: [0xaa; 32],
                            parent_hash: [0xa9; 32],
                            state_root: [0xbb; 32],
                            header_observed: true,
                            body_available: true,
                            lifecycle_stage:
                                novovm_network::NetworkRuntimeNativeBlockLifecycleStageV1::Canonical,
                            canonical: true,
                            safe: true,
                            finalized: false,
                            source_peer_id: Some(10),
                            observed_unix_ms: 77,
                        }),
                        retained_block_count: 3,
                        canonical_block_count: 3,
                        canonical_update_count: 2,
                        reorg_count: 0,
                        last_reorg_depth: None,
                        last_reorg_unix_ms: None,
                        last_head_change_unix_ms: Some(77),
                        block_lifecycle_summary:
                            novovm_network::NetworkRuntimeNativeBlockLifecycleSummaryV1 {
                                seen_count: 0,
                                header_only_count: 0,
                                body_ready_count: 0,
                                canonical_count: 3,
                                non_canonical_count: 0,
                                reorged_out_count: 0,
                            },
                    },
                ),
                native_canonical_blocks: vec![
                    novovm_network::NetworkRuntimeNativeCanonicalBlockStateV1 {
                        chain_id,
                        number: 0x80,
                        hash: [0xaa; 32],
                        parent_hash: [0xa9; 32],
                        state_root: [0xbb; 32],
                        header_observed: true,
                        body_available: true,
                        lifecycle_stage:
                            novovm_network::NetworkRuntimeNativeBlockLifecycleStageV1::Canonical,
                        canonical: true,
                        safe: true,
                        finalized: false,
                        source_peer_id: Some(10),
                        observed_unix_ms: 77,
                    },
                    novovm_network::NetworkRuntimeNativeCanonicalBlockStateV1 {
                        chain_id,
                        number: 0x7f,
                        hash: [0xab; 32],
                        parent_hash: [0xaa; 32],
                        state_root: [0xbc; 32],
                        header_observed: true,
                        body_available: false,
                        lifecycle_stage:
                            novovm_network::NetworkRuntimeNativeBlockLifecycleStageV1::HeaderOnly,
                        canonical: false,
                        safe: false,
                        finalized: false,
                        source_peer_id: Some(11),
                        observed_unix_ms: 76,
                    },
                ],
                native_pending_tx_summary:
                    novovm_network::NetworkRuntimeNativePendingTxSummaryV1 {
                        chain_id,
                        tx_count: 2,
                        local_origin_count: 1,
                        remote_origin_count: 1,
                        unknown_origin_count: 0,
                        seen_count: 0,
                        pending_count: 1,
                        propagated_count: 0,
                        included_canonical_count: 1,
                        included_non_canonical_count: 0,
                        reorged_back_to_pending_count: 0,
                        dropped_count: 0,
                        rejected_count: 0,
                        retry_eligible_count: 1,
                        budget_suppressed_count: 0,
                        io_write_failure_count: 0,
                        non_recoverable_count: 0,
                        propagation_attempt_total: 1,
                        propagation_success_total: 1,
                        propagation_failure_total: 0,
                        propagated_peer_total: 1,
                        evicted_count: 0,
                        expired_count: 0,
                        broadcast_dispatch_total: 0,
                        broadcast_dispatch_success_total: 0,
                        broadcast_dispatch_failed_total: 0,
                        broadcast_candidate_tx_total: 0,
                        broadcast_tx_total: 0,
                        last_broadcast_peer_id: None,
                        last_broadcast_candidate_count: 0,
                        last_broadcast_tx_count: 0,
                        last_broadcast_unix_ms: None,
                    },
                native_pending_tx_broadcast_runtime:
                    novovm_network::NetworkRuntimeNativePendingTxBroadcastRuntimeSummaryV1 {
                        chain_id,
                        dispatch_total: 4,
                        dispatch_success_total: 3,
                        dispatch_failed_total: 1,
                        candidate_tx_total: 7,
                        broadcast_tx_total: 6,
                        last_peer_id: Some(10),
                        last_candidate_count: 2,
                        last_broadcast_tx_count: 2,
                        last_updated_unix_ms: Some(77),
                    },
                native_execution_budget_runtime:
                    novovm_network::NetworkRuntimeNativeExecutionBudgetRuntimeSummaryV1 {
                        chain_id,
                        execution_budget_hit_count: 0,
                        execution_deferred_count: 0,
                        execution_time_slice_exceeded_count: 0,
                        hard_budget_per_tick: None,
                        hard_time_slice_ms: None,
                        target_budget_per_tick: None,
                        target_time_slice_ms: None,
                        effective_budget_per_tick: None,
                        effective_time_slice_ms: None,
                        last_execution_target_reason: None,
                        last_execution_throttle_reason: None,
                        last_updated_unix_ms: None,
                    },
                native_pending_txs: vec![
                    novovm_network::NetworkRuntimeNativePendingTxStateV1 {
                        chain_id,
                        tx_hash: [0x91; 32],
                        lifecycle_stage:
                            novovm_network::NetworkRuntimeNativePendingTxLifecycleStageV1::Pending,
                        origin: novovm_network::NetworkRuntimeNativePendingTxOriginV1::Local,
                        source_peer_id: Some(10),
                        first_seen_unix_ms: 70,
                        last_updated_unix_ms: 77,
                        last_block_number: None,
                        last_block_hash: None,
                        canonical_inclusion: None,
                        ingress_count: 1,
                        propagation_count: 0,
                        inclusion_count: 0,
                        reorg_back_count: 0,
                        drop_count: 0,
                        reject_count: 0,
                        propagation_attempt_count: 0,
                        propagation_success_count: 0,
                        propagation_failure_count: 0,
                        propagated_peer_count: 0,
                        last_propagation_unix_ms: None,
                        last_propagation_attempt_unix_ms: None,
                        last_propagation_failure_unix_ms: None,
                        last_propagation_failure_class: None,
                        last_propagation_failure_phase: None,
                        last_propagation_peer_id: None,
                        last_propagated_peer_id: None,
                        propagation_disposition: None,
                        propagation_stop_reason: None,
                        propagation_recoverability: None,
                        retry_eligible: true,
                        retry_after_unix_ms: None,
                        retry_backoff_level: 0,
                        retry_suppressed_reason: None,
                        pending_final_disposition: novovm_network::NetworkRuntimeNativePendingTxFinalDispositionV1::Retained,
                    },
                    novovm_network::NetworkRuntimeNativePendingTxStateV1 {
                        chain_id,
                        tx_hash: [0x92; 32],
                        lifecycle_stage: novovm_network::NetworkRuntimeNativePendingTxLifecycleStageV1::IncludedCanonical,
                        origin: novovm_network::NetworkRuntimeNativePendingTxOriginV1::Remote,
                        source_peer_id: Some(11),
                        first_seen_unix_ms: 70,
                        last_updated_unix_ms: 77,
                        last_block_number: Some(0x80),
                        last_block_hash: Some([0xaa; 32]),
                        canonical_inclusion: Some(true),
                        ingress_count: 1,
                        propagation_count: 1,
                        inclusion_count: 1,
                        reorg_back_count: 0,
                        drop_count: 0,
                        reject_count: 0,
                        propagation_attempt_count: 1,
                        propagation_success_count: 1,
                        propagation_failure_count: 0,
                        propagated_peer_count: 1,
                        last_propagation_unix_ms: Some(77),
                        last_propagation_attempt_unix_ms: Some(77),
                        last_propagation_failure_unix_ms: None,
                        last_propagation_failure_class: None,
                        last_propagation_failure_phase: None,
                        last_propagation_peer_id: Some(11),
                        last_propagated_peer_id: Some(11),
                        propagation_disposition: Some(novovm_network::NetworkRuntimeNativePendingTxPropagationDispositionV1::Propagated),
                        propagation_stop_reason: None,
                        propagation_recoverability: None,
                        retry_eligible: true,
                        retry_after_unix_ms: None,
                        retry_backoff_level: 0,
                        retry_suppressed_reason: None,
                        pending_final_disposition: novovm_network::NetworkRuntimeNativePendingTxFinalDispositionV1::Retained,
                    },
                ],
                native_head_body_available: Some(true),
                native_head_canonical: Some(true),
                native_head_safe: Some(true),
                native_head_finalized: Some(false),
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
                    cooldown_count: 1,
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
                selection_quality_summary: novovm_network::EthPeerSelectionQualitySummaryV1 {
                    chain_id,
                    candidate_peer_count: 2,
                    evaluated_bootstrap_peers: 2,
                    evaluated_sync_peers: 2,
                    retry_eligible_bootstrap_peers: 1,
                    ready_sync_peers: 1,
                    selected_bootstrap_peers: 1,
                    selected_sync_peers: 1,
                    skipped_cooldown_peers: 1,
                    skipped_permanently_rejected_peers: 0,
                    skipped_unready_sync_peers: 1,
                    top_selected_bootstrap_peer_id: Some(10),
                    top_selected_sync_peer_id: Some(10),
                    top_selected_bootstrap_score: Some(900),
                    top_selected_sync_score: Some(1800),
                    average_selected_bootstrap_score: Some(900),
                    average_selected_sync_score: Some(1800),
                    selected_bootstrap_peer_ids: vec![10],
                    selected_sync_peer_ids: vec![10],
                },
                selection_long_term_summary: novovm_network::EthPeerSelectionLongTermSummaryV1 {
                    chain_id,
                    tracked_sync_peers: 1,
                    peers_with_history: 1,
                    peers_with_positive_contribution: 1,
                    peers_currently_in_failure_streak: 0,
                    peers_currently_in_progressless_streak: 0,
                    observed_rounds_total: 12,
                    selected_rounds_total: 6,
                    selected_sync_rounds_total: 3,
                    sync_contribution_rounds_total: 3,
                    selected_without_progress_rounds_total: 1,
                    connect_failure_rounds_total: 0,
                    handshake_failure_rounds_total: 0,
                    decode_failure_rounds_total: 0,
                    timeout_failure_rounds_total: 0,
                    validation_reject_rounds_total: 0,
                    disconnect_rounds_total: 0,
                    capacity_reject_rounds_total: 0,
                    average_selection_hit_rate_bps: 3_333,
                    average_header_success_rate_bps: 5_000,
                    average_body_success_rate_bps: 5_000,
                    average_long_term_score: Some(1_400),
                    top_trusted_sync_peer_id: Some(10),
                    top_trusted_sync_long_term_score: Some(1_400),
                },
                selection_window_policy:
                    novovm_network::default_eth_peer_selection_window_policy_v1(),
                runtime_config: novovm_network::EthFullnodeNativeRuntimeConfigV1 {
                    chain_id,
                    budget_hooks: novovm_network::default_eth_fullnode_budget_hooks_v1(),
                    selection_window_policy:
                        novovm_network::default_eth_peer_selection_window_policy_v1(),
                },
                peer_selection_scores: vec![
                    novovm_network::EthPeerSelectionScoreV1 {
                        chain_id,
                        peer_id: 10,
                        role: novovm_network::EthPeerSelectionRoleV1::Bootstrap,
                        stage: novovm_network::EthPeerLifecycleStageV1::Ready,
                        eligible: false,
                        selected: true,
                        score: 900,
                        reasons: vec!["prior_successful_sessions=1".to_string()],
                        last_head_height: 0x80,
                        successful_sessions: 1,
                        header_response_count: 1,
                        body_response_count: 1,
                        sync_contribution_count: 2,
                        consecutive_failures: 0,
                        last_success_unix_ms: 70,
                        last_failure_unix_ms: 0,
                        cooldown_until_unix_ms: 0,
                        permanently_rejected: false,
                        long_term_score: 700,
                        recent_window: novovm_network::EthPeerRecentWindowStatsV1 {
                            window_rounds: 4,
                            selected_rounds: 3,
                            selected_bootstrap_rounds: 1,
                            selected_sync_rounds: 2,
                            header_success_rounds: 1,
                            body_success_rounds: 1,
                            sync_contribution_rounds: 1,
                            selected_without_progress_rounds: 1,
                            connect_failure_rounds: 0,
                            handshake_failure_rounds: 0,
                            decode_failure_rounds: 0,
                            timeout_failure_rounds: 0,
                            validation_reject_rounds: 0,
                            disconnect_rounds: 0,
                            capacity_reject_rounds: 0,
                            last_selected_unix_ms: 70,
                            last_progress_unix_ms: 70,
                            last_failure_unix_ms: 0,
                            selection_hit_rate_bps: 3_333,
                            header_success_rate_bps: 5_000,
                            body_success_rate_bps: 5_000,
                        },
                        long_term: novovm_network::EthPeerLongTermStatsV1 {
                            total_observed_rounds: 12,
                            total_selected_rounds: 6,
                            total_selected_bootstrap_rounds: 2,
                            total_selected_sync_rounds: 4,
                            total_header_success_rounds: 2,
                            total_body_success_rounds: 2,
                            total_sync_contribution_rounds: 2,
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
                            current_consecutive_selected_without_progress_rounds: 0,
                            max_consecutive_connect_failures: 0,
                            max_consecutive_handshake_failures: 0,
                            max_consecutive_decode_failures: 0,
                            max_consecutive_timeout_failures: 0,
                            max_consecutive_validation_rejects: 0,
                            max_consecutive_disconnects: 0,
                            max_consecutive_selected_without_progress_rounds: 1,
                            last_selected_unix_ms: 70,
                            last_progress_unix_ms: 70,
                            last_failure_unix_ms: 0,
                            selection_hit_rate_bps: 3_333,
                            header_success_rate_bps: 5_000,
                            body_success_rate_bps: 5_000,
                            ..novovm_network::EthPeerLongTermStatsV1::default()
                        },
                        window_layers: novovm_network::EthPeerSelectionWindowLayersV1::default(),
                    },
                    novovm_network::EthPeerSelectionScoreV1 {
                        chain_id,
                        peer_id: 10,
                        role: novovm_network::EthPeerSelectionRoleV1::Sync,
                        stage: novovm_network::EthPeerLifecycleStageV1::Ready,
                        eligible: true,
                        selected: true,
                        score: 1800,
                        reasons: vec!["eligible_ready_session".to_string()],
                        last_head_height: 0x80,
                        successful_sessions: 1,
                        header_response_count: 1,
                        body_response_count: 1,
                        sync_contribution_count: 2,
                        consecutive_failures: 0,
                        last_success_unix_ms: 70,
                        last_failure_unix_ms: 0,
                        cooldown_until_unix_ms: 0,
                        permanently_rejected: false,
                        long_term_score: 1_400,
                        recent_window: novovm_network::EthPeerRecentWindowStatsV1 {
                            window_rounds: 4,
                            selected_rounds: 3,
                            selected_bootstrap_rounds: 1,
                            selected_sync_rounds: 2,
                            header_success_rounds: 1,
                            body_success_rounds: 1,
                            sync_contribution_rounds: 1,
                            selected_without_progress_rounds: 1,
                            connect_failure_rounds: 0,
                            handshake_failure_rounds: 0,
                            decode_failure_rounds: 0,
                            timeout_failure_rounds: 0,
                            validation_reject_rounds: 0,
                            disconnect_rounds: 0,
                            capacity_reject_rounds: 0,
                            last_selected_unix_ms: 70,
                            last_progress_unix_ms: 70,
                            last_failure_unix_ms: 0,
                            selection_hit_rate_bps: 3_333,
                            header_success_rate_bps: 5_000,
                            body_success_rate_bps: 5_000,
                        },
                        long_term: novovm_network::EthPeerLongTermStatsV1 {
                            total_observed_rounds: 12,
                            total_selected_rounds: 6,
                            total_selected_bootstrap_rounds: 2,
                            total_selected_sync_rounds: 4,
                            total_header_success_rounds: 2,
                            total_body_success_rounds: 2,
                            total_sync_contribution_rounds: 2,
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
                            current_consecutive_selected_without_progress_rounds: 0,
                            max_consecutive_connect_failures: 0,
                            max_consecutive_handshake_failures: 0,
                            max_consecutive_decode_failures: 0,
                            max_consecutive_timeout_failures: 0,
                            max_consecutive_validation_rejects: 0,
                            max_consecutive_disconnects: 0,
                            max_consecutive_selected_without_progress_rounds: 1,
                            last_selected_unix_ms: 70,
                            last_progress_unix_ms: 70,
                            last_failure_unix_ms: 0,
                            selection_hit_rate_bps: 3_333,
                            header_success_rate_bps: 5_000,
                            body_success_rate_bps: 5_000,
                            ..novovm_network::EthPeerLongTermStatsV1::default()
                        },
                        window_layers: novovm_network::EthPeerSelectionWindowLayersV1::default(),
                    },
                ],
                peer_sessions: Vec::new(),
                peer_failures: vec![EthFullnodeNativePeerFailureSnapshotV1 {
                    peer_id: 11,
                    endpoint: Some("127.0.0.1:30303".to_string()),
                    phase: "bootstrap".to_string(),
                    class: "connect_failure".to_string(),
                    lifecycle_class: Some("connect_failure".to_string()),
                    reason_code: None,
                    reason_name: Some("connection_refused".to_string()),
                    error: "connection_refused".to_string(),
                },
                EthFullnodeNativePeerFailureSnapshotV1 {
                    peer_id: 10,
                    endpoint: Some("127.0.0.1:30304".to_string()),
                    phase: "sync".to_string(),
                    class: "io".to_string(),
                    lifecycle_class: Some("io".to_string()),
                    reason_code: None,
                    reason_name: Some("transactions_write_failed".to_string()),
                    error: "transactions_write_failed".to_string(),
                }],
            },
        );
        let out = run_mainline_query(
            &sample_store(),
            "supervm_getEthNativeWorkerRuntime",
            &json!({ "chainId": chain_id }),
        )
        .expect("runtime query");
        let expected_chain_id = format!("0x{chain_id:x}");
        assert_eq!(out["found"].as_bool(), Some(true));
        assert_eq!(out["source"].as_str(), Some("runtime_snapshot_memory"));
        assert_eq!(out["chainId"].as_str(), Some(expected_chain_id.as_str()));
        assert_eq!(out["runtime"]["ready_peers"].as_u64(), Some(1));
        assert_eq!(
            out["runtime"]["lifecycle_summary"]["cooldown_count"].as_u64(),
            Some(1)
        );

        let runtime_config = run_mainline_query(
            &sample_store(),
            "supervm_getEthNativeRuntimeConfig",
            &json!({ "chainId": chain_id }),
        )
        .expect("runtime config query");
        assert_eq!(runtime_config["found"].as_bool(), Some(true));
        assert_eq!(
            runtime_config["runtimeConfig"]["budget_hooks"]["active_native_peer_soft_limit"]
                .as_u64(),
            Some(8)
        );
        assert_eq!(
            runtime_config["runtimeConfigSource"]["config_file_found"].as_bool(),
            Some(false)
        );

        let canonical_chain = run_mainline_query(
            &sample_store(),
            "supervm_getEthNativeCanonicalChainSummary",
            &json!({ "chainId": chain_id }),
        )
        .expect("canonical chain query");
        assert_eq!(canonical_chain["found"].as_bool(), Some(true));
        assert_eq!(
            canonical_chain["canonicalChain"]["lifecycleStage"].as_str(),
            Some("advanced")
        );
        assert_eq!(
            canonical_chain["canonicalChain"]["blockLifecycleSummary"]["canonicalCount"].as_u64(),
            Some(3)
        );
        assert_eq!(
            canonical_chain["canonicalChain"]["head"]["bodyAvailable"].as_bool(),
            Some(true)
        );
        assert_eq!(
            canonical_chain["canonicalChain"]["head"]["lifecycleStage"].as_str(),
            Some("canonical")
        );
        assert_eq!(
            canonical_chain["canonicalChain"]["blockLifecycleSummary"]["headerOnlyCount"].as_u64(),
            Some(0)
        );

        let lifecycle_by_number = run_mainline_query(
            &sample_store(),
            "supervm_getEthNativeBlockLifecycleByNumber",
            &json!({ "chainId": chain_id, "blockNumber": "0x80" }),
        )
        .expect("block lifecycle by number query");
        assert_eq!(lifecycle_by_number["found"].as_bool(), Some(true));
        assert_eq!(
            lifecycle_by_number["block"]["lifecycleStage"].as_str(),
            Some("canonical")
        );
        assert_eq!(
            lifecycle_by_number["block"]["receiptOwnership"].as_str(),
            Some("canonical")
        );

        let lifecycle_by_hash = run_mainline_query(
            &sample_store(),
            "supervm_getEthNativeBlockLifecycleByHash",
            &json!({ "chainId": chain_id, "blockHash": to_hex_prefixed(&[0xab; 32]) }),
        )
        .expect("block lifecycle by hash query");
        assert_eq!(lifecycle_by_hash["found"].as_bool(), Some(true));
        assert_eq!(
            lifecycle_by_hash["block"]["lifecycleStage"].as_str(),
            Some("header_only")
        );
        assert_eq!(
            lifecycle_by_hash["block"]["receiptOwnership"].as_str(),
            Some("non_canonical")
        );

        let pending_summary = run_mainline_query(
            &sample_store(),
            "supervm_getEthNativePendingTxSummary",
            &json!({ "chainId": chain_id }),
        )
        .expect("pending tx summary query");
        assert_eq!(pending_summary["found"].as_bool(), Some(true));
        assert_eq!(
            pending_summary["pendingTxSummary"]["txCount"].as_u64(),
            Some(2)
        );
        assert_eq!(
            pending_summary["pendingTxSummary"]["includedCanonicalCount"].as_u64(),
            Some(1)
        );
        assert_eq!(
            pending_summary["pendingTxSummary"]["broadcastDispatchTotal"].as_u64(),
            Some(0)
        );
        assert_eq!(
            pending_summary["pendingTxSummary"]["broadcastTxTotal"].as_u64(),
            Some(0)
        );
        assert_eq!(
            pending_summary["pendingTxSummary"]["localPendingCount"].as_u64(),
            Some(1)
        );
        assert_eq!(
            pending_summary["pendingTxSummary"]["remotePendingCount"].as_u64(),
            Some(0)
        );

        let pending_by_hash = run_mainline_query(
            &sample_store(),
            "supervm_getEthNativePendingTxByHash",
            &json!({ "chainId": chain_id, "txHash": to_hex_prefixed(&[0x92; 32]) }),
        )
        .expect("pending tx by hash query");
        assert_eq!(pending_by_hash["found"].as_bool(), Some(true));
        assert_eq!(
            pending_by_hash["pendingTx"]["lifecycleStage"].as_str(),
            Some("included_canonical")
        );
        assert_eq!(
            pending_by_hash["pendingTx"]["authoritativeLifecycle"].as_str(),
            Some("included_canonical")
        );

        let pending_local_by_hash = run_mainline_query(
            &sample_store(),
            "supervm_getEthNativePendingTxByHash",
            &json!({ "chainId": chain_id, "txHash": to_hex_prefixed(&[0x91; 32]) }),
        )
        .expect("local pending tx by hash query");
        assert_eq!(pending_local_by_hash["found"].as_bool(), Some(true));
        assert_eq!(
            pending_local_by_hash["pendingTx"]["lifecycleStage"].as_str(),
            Some("pending")
        );
        assert_eq!(
            pending_local_by_hash["pendingTx"]["authoritativeLifecycle"].as_str(),
            Some("local_pending")
        );
        assert_eq!(
            pending_by_hash["pendingTx"]["lastPropagationPeerId"].as_str(),
            Some("0xb")
        );
        assert_eq!(
            pending_by_hash["pendingTx"]["lastPropagationFailureClass"].is_null(),
            true
        );
        assert_eq!(
            pending_by_hash["pendingTx"]["propagationDisposition"].as_str(),
            Some("propagated")
        );
        assert_eq!(
            pending_by_hash["pendingTx"]["pendingFinalDisposition"].as_str(),
            Some("retained")
        );
        assert_eq!(pending_by_hash["pendingTxFound"].as_bool(), Some(true));
        assert_eq!(pending_by_hash["tombstoneFound"].as_bool(), Some(false));
        assert_eq!(
            pending_by_hash["failureClassificationContract"].as_str(),
            Some(ETH_PENDING_TX_PROPAGATION_CLASSIFICATION_CONTRACT_V1)
        );
        assert_eq!(
            pending_by_hash["pendingTx"]["propagationStopReason"].is_null(),
            true
        );
        assert_eq!(
            pending_by_hash["pendingTx"]["propagationRecoverability"].is_null(),
            true
        );
        assert_eq!(
            pending_by_hash["pendingTx"]["failureClassificationContract"].as_str(),
            Some(ETH_PENDING_TX_PROPAGATION_CLASSIFICATION_CONTRACT_V1)
        );
        assert!(pending_by_hash["pendingTx"]["failureClass"].is_null());
        assert!(pending_by_hash["pendingTx"]["failureClassSource"].is_null());
        assert!(pending_by_hash["pendingTx"]["failureRecoverability"].is_null());
        assert_eq!(
            pending_by_hash["pendingTx"]["propagationAttemptCount"].as_u64(),
            Some(1)
        );
        assert_eq!(
            pending_by_hash["pendingTx"]["propagationSuccessCount"].as_u64(),
            Some(1)
        );
        assert_eq!(
            pending_by_hash["pendingTx"]["propagationFailureCount"].as_u64(),
            Some(0)
        );
        assert_eq!(
            pending_by_hash["pendingTx"]["retryEligible"].as_bool(),
            Some(true)
        );

        let pending_propagation_summary = run_mainline_query(
            &sample_store(),
            "supervm_getEthNativePendingTxPropagationSummary",
            &json!({ "chainId": chain_id }),
        )
        .expect("pending tx propagation summary query");
        assert_eq!(
            pending_propagation_summary["method"].as_str(),
            Some("supervm_getEthNativePendingTxPropagationSummary")
        );
        assert_eq!(
            pending_propagation_summary["failureClassificationContract"].as_str(),
            Some(ETH_PENDING_TX_PROPAGATION_CLASSIFICATION_CONTRACT_V1)
        );
        assert_eq!(
            pending_propagation_summary["propagationSummary"]["propagationAttemptTotal"].as_u64(),
            Some(1)
        );
        assert_eq!(
            pending_propagation_summary["propagationSummary"]["propagationSuccessTotal"].as_u64(),
            Some(1)
        );
        assert_eq!(
            pending_propagation_summary["propagationSummary"]["propagationFailureTotal"].as_u64(),
            Some(0)
        );
        assert_eq!(
            pending_propagation_summary["propagationSummary"]["coverageRateBps"].as_u64(),
            Some(10_000)
        );

        let pending_broadcast_candidates = run_mainline_query(
            &sample_store(),
            "supervm_getEthNativePendingTxBroadcastCandidates",
            &json!({ "chainId": chain_id, "limit": 8, "maxPropagationCount": 3 }),
        )
        .expect("pending tx broadcast candidates query");
        assert_eq!(
            pending_broadcast_candidates["method"].as_str(),
            Some("supervm_getEthNativePendingTxBroadcastCandidates")
        );
        assert_eq!(pending_broadcast_candidates["limit"].as_u64(), Some(8));
        assert_eq!(
            pending_broadcast_candidates["maxPropagationCount"].as_u64(),
            Some(3)
        );
        assert_eq!(
            pending_broadcast_candidates["candidateCount"].as_u64(),
            pending_broadcast_candidates["candidates"]
                .as_array()
                .map(|items| items.len() as u64)
        );
        assert_eq!(
            pending_broadcast_candidates["broadcastRuntime"]["dispatchTotal"].as_u64(),
            Some(0)
        );

        let peer_state = run_mainline_query(
            &sample_store(),
            "supervm_getEthNativePeerRuntimeState",
            &json!({ "chainId": chain_id }),
        )
        .expect("peer runtime query");
        assert_eq!(peer_state["found"].as_bool(), Some(true));
        assert_eq!(
            peer_state["lifecycleSummary"]["ready_count"].as_u64(),
            Some(1)
        );
        assert_eq!(peer_state["peerSessions"].as_array().map(Vec::len), Some(0));
        assert_eq!(
            peer_state["recentFailures"].as_array().map(Vec::len),
            Some(2)
        );
        assert_eq!(peer_state["worker"]["bodyUpdates"].as_u64(), Some(1));
        assert_eq!(peer_state["txBroadcast"]["dispatchTotal"].as_u64(), Some(4));
        assert_eq!(
            peer_state["txBroadcast"]["dispatchSuccessRateBps"].as_u64(),
            Some(7500)
        );
        assert_eq!(
            peer_state["txBroadcast"]["txDeliveryRateBps"].as_u64(),
            Some(8571)
        );
        assert_eq!(
            peer_state["txBroadcast"]["lastPeerId"].as_str(),
            Some("0xa")
        );
        assert_eq!(
            peer_state["txBroadcast"]["failureReasons"]
                .as_array()
                .and_then(|items| items.first())
                .and_then(|value| value.get("reason"))
                .and_then(Value::as_str),
            Some("transactions_write_failed")
        );
        assert_eq!(
            peer_state["txBroadcast"]["failureClassCounts"]
                .as_array()
                .and_then(|items| items.first())
                .and_then(|value| value.get("class"))
                .and_then(Value::as_str),
            Some("io")
        );
        assert_eq!(
            peer_state["txBroadcast"]["failurePhaseCounts"]
                .as_array()
                .and_then(|items| items.first())
                .and_then(|value| value.get("phase"))
                .and_then(Value::as_str),
            Some("sync")
        );
        assert_eq!(
            peer_state["txBroadcast"]["failurePeerIds"]
                .as_array()
                .and_then(|items| items.first())
                .and_then(Value::as_str),
            Some("0xa")
        );

        let peer_health = run_mainline_query(
            &sample_store(),
            "supervm_getEthNativePeerHealthSummary",
            &json!({ "chainId": chain_id }),
        )
        .expect("peer health summary");
        assert_eq!(peer_health["found"].as_bool(), Some(true));
        assert_eq!(
            peer_health["schema"].as_str(),
            Some(ETH_NATIVE_PEER_HEALTH_SUMMARY_SCHEMA_V1)
        );
        assert_eq!(peer_health["status"].as_str(), Some("degraded"));
        assert_eq!(
            peer_health["primaryReason"].as_str(),
            Some("broadcast_phase_stall")
        );
        assert_eq!(
            peer_health["rootCause"].as_str(),
            Some("broadcast_path_issue")
        );
        assert_eq!(
            peer_health["rootCauseSignals"]["broadcastPathIssue"].as_bool(),
            Some(true)
        );
        assert_eq!(
            peer_health["failureClassificationContracts"]["execution"].as_str(),
            Some(ETH_EXEC_FAILURE_CLASSIFICATION_CONTRACT_V1)
        );
        assert_eq!(
            peer_health["failureClassificationContracts"]["pendingTxPropagation"].as_str(),
            Some(ETH_PENDING_TX_PROPAGATION_CLASSIFICATION_CONTRACT_V1)
        );
        assert_eq!(peer_health["candidatePeerCount"].as_u64(), Some(2));
        assert_eq!(peer_health["availablePeerCount"].as_u64(), Some(2));
        assert_eq!(
            peer_health["selectionCorrelation"]["failingSelectedSyncPeerCount"].as_u64(),
            Some(1)
        );
        assert_eq!(
            peer_health["selectionCorrelation"]["likelySelectionQualityIssue"].as_bool(),
            Some(true)
        );
        assert_eq!(
            peer_health["cleanupPressure"]["cleanupPressureStatus"].as_str(),
            Some("healthy")
        );
        assert_eq!(
            peer_health["cleanupPressure"]["cleanupPressureReasons"]
                .as_array()
                .map(Vec::len),
            Some(0)
        );
        assert_eq!(
            peer_health["cleanupPressure"]["nonRecoverableRejectionCount"].as_u64(),
            Some(0)
        );
        assert_eq!(
            peer_health["cleanupPressure"]["finalDispositionCounts"]["evicted"].as_u64(),
            Some(0)
        );
        assert_eq!(
            peer_health["failureClassCounts"]["connectFailure"].as_u64(),
            Some(1)
        );
        assert_eq!(
            peer_health["selectionQualitySummary"]["selected_sync_peers"].as_u64(),
            Some(1)
        );
        assert_eq!(
            peer_health["selectionLongTermSummary"]["top_trusted_sync_peer_id"].as_u64(),
            Some(10)
        );
        assert_eq!(
            peer_health["selectionWindowPolicy"]["medium_term_rounds"].as_u64(),
            Some(64)
        );
        assert_eq!(
            peer_health["runtimeConfig"]["budget_hooks"]["active_native_peer_soft_limit"].as_u64(),
            Some(8)
        );
        assert_eq!(
            peer_health["runtimeConfigSource"]["config_file_found"].as_bool(),
            Some(false)
        );
        assert_eq!(
            peer_health["lastFailureReasons"].as_array().map(Vec::len),
            Some(0)
        );
        assert_eq!(
            peer_health["txBroadcast"]["dispatchTotal"].as_u64(),
            Some(4)
        );
        assert_eq!(
            peer_health["txBroadcast"]["dispatchFailureRateBps"].as_u64(),
            Some(2500)
        );

        let sync_summary = run_mainline_query(
            &sample_store(),
            "supervm_getEthNativeSyncRuntimeSummary",
            &json!({ "chainId": chain_id }),
        )
        .expect("sync runtime summary");
        assert_eq!(sync_summary["found"].as_bool(), Some(true));
        assert_eq!(
            sync_summary["schema"].as_str(),
            Some(ETH_NATIVE_SYNC_RUNTIME_SUMMARY_SCHEMA_V1)
        );
        assert_eq!(
            sync_summary["head"]["blockViewSource"].as_str(),
            Some("native_chain_sync")
        );
        assert_eq!(sync_summary["head"]["bodyAvailable"].as_bool(), Some(true));
        assert_eq!(
            sync_summary["sync"]["nativeSyncPhase"].as_str(),
            Some("bodies")
        );
        assert_eq!(sync_summary["sync"]["syncing"].as_bool(), Some(true));
        assert_eq!(
            sync_summary["canonicalChain"]["canonicalUpdateCount"].as_u64(),
            Some(2)
        );
        assert_eq!(
            sync_summary["canonicalChain"]["blockLifecycleSummary"]["canonicalCount"].as_u64(),
            Some(3)
        );
        assert_eq!(
            sync_summary["txBroadcast"]["dispatchTotal"].as_u64(),
            Some(4)
        );
        assert_eq!(
            sync_summary["txBroadcast"]["txDeliveryRateBps"].as_u64(),
            Some(8571)
        );
        assert_eq!(sync_summary["degradationStatus"].as_str(), Some("degraded"));
        assert_eq!(
            sync_summary["primaryReason"].as_str(),
            Some("broadcast_phase_stall")
        );
        assert_eq!(
            sync_summary["rootCause"].as_str(),
            Some("broadcast_path_issue")
        );
        assert_eq!(
            sync_summary["rootCauseSignals"]["broadcastPathIssue"].as_bool(),
            Some(true)
        );
        assert_eq!(
            sync_summary["failureClassificationContracts"]["execution"].as_str(),
            Some(ETH_EXEC_FAILURE_CLASSIFICATION_CONTRACT_V1)
        );
        assert_eq!(
            sync_summary["failureClassificationContracts"]["pendingTxPropagation"].as_str(),
            Some(ETH_PENDING_TX_PROPAGATION_CLASSIFICATION_CONTRACT_V1)
        );
        assert_eq!(
            sync_summary["selectionCorrelation"]["failingSelectedSyncPeerCount"].as_u64(),
            Some(1)
        );
        assert_eq!(
            sync_summary["selectionCorrelation"]["likelySelectionQualityIssue"].as_bool(),
            Some(true)
        );
        assert_eq!(
            sync_summary["cleanupPressure"]["cleanupPressureStatus"].as_str(),
            Some("healthy")
        );
        assert_eq!(
            sync_summary["cleanupPressure"]["cleanupPressureReasons"]
                .as_array()
                .map(Vec::len),
            Some(0)
        );
        assert_eq!(
            sync_summary["cleanupPressure"]["nonRecoverableRejectionCount"].as_u64(),
            Some(0)
        );
        assert_eq!(
            sync_summary["cleanupPressure"]["finalDispositionCounts"]["expired"].as_u64(),
            Some(0)
        );

        let sync_degradation = run_mainline_query(
            &sample_store(),
            "supervm_getEthNativeSyncDegradationSummary",
            &json!({ "chainId": chain_id }),
        )
        .expect("sync degradation summary");
        assert_eq!(sync_degradation["found"].as_bool(), Some(true));
        assert_eq!(
            sync_degradation["schema"].as_str(),
            Some(ETH_NATIVE_SYNC_DEGRADATION_SUMMARY_SCHEMA_V1)
        );
        assert_eq!(sync_degradation["status"].as_str(), Some("degraded"));
        assert_eq!(
            sync_degradation["primaryReason"].as_str(),
            Some("broadcast_phase_stall")
        );
        assert_eq!(
            sync_degradation["rootCause"].as_str(),
            Some("broadcast_path_issue")
        );
        assert_eq!(
            sync_degradation["failureClassificationContracts"]["execution"].as_str(),
            Some(ETH_EXEC_FAILURE_CLASSIFICATION_CONTRACT_V1)
        );
        assert_eq!(
            sync_degradation["failureClassificationContracts"]["pendingTxPropagation"].as_str(),
            Some(ETH_PENDING_TX_PROPAGATION_CLASSIFICATION_CONTRACT_V1)
        );
        assert_eq!(
            sync_degradation["reasons"].as_array().map(|items| {
                items
                    .iter()
                    .any(|value| value.as_str() == Some("broadcast_phase_stall"))
            }),
            Some(true)
        );
        assert_eq!(
            sync_degradation["reasons"].as_array().map(|items| {
                items
                    .iter()
                    .any(|value| value.as_str() == Some("peer_connect_failures_present"))
            }),
            Some(true)
        );
        assert_eq!(
            sync_degradation["reasons"].as_array().map(Vec::len),
            Some(2)
        );
        assert_eq!(
            sync_degradation["crossLayer"]["selectionCorrelation"]["selectedSyncPeers"].as_u64(),
            Some(1)
        );
        assert_eq!(
            sync_degradation["crossLayer"]["selectionCorrelation"]["failingBroadcastPeerCount"]
                .as_u64(),
            Some(1)
        );
        assert_eq!(
            sync_degradation["crossLayer"]["selectionCorrelation"]["failingSelectedSyncPeerCount"]
                .as_u64(),
            Some(1)
        );
        assert_eq!(
            sync_degradation["crossLayer"]["rootCauseSignals"]["broadcastPathIssue"].as_bool(),
            Some(true)
        );
        assert_eq!(
            sync_degradation["crossLayer"]["broadcastFailurePeerCorrelations"]
                .as_array()
                .map(Vec::len),
            Some(1)
        );
        assert_eq!(
            sync_degradation["crossLayer"]["broadcastFailurePeerCorrelations"]
                .as_array()
                .and_then(|items| items.first())
                .and_then(|value| value.get("peerId"))
                .and_then(Value::as_str),
            Some("0xa")
        );
        assert_eq!(
            sync_degradation["crossLayer"]["broadcastFailurePeerCorrelations"]
                .as_array()
                .and_then(|items| items.first())
                .and_then(|value| value.get("selection"))
                .and_then(|value| value.get("syncSelected"))
                .and_then(Value::as_bool),
            Some(true)
        );
        assert_same_root_cause_fields_v1(&peer_health, &sync_summary, &sync_degradation);

        let selection_summary = run_mainline_query(
            &sample_store(),
            "supervm_getEthNativePeerSelectionSummary",
            &json!({ "chainId": chain_id }),
        )
        .expect("selection summary");
        assert_eq!(selection_summary["found"].as_bool(), Some(true));
        assert_eq!(selection_summary["status"].as_str(), Some("healthy"));
        assert_eq!(
            selection_summary["selectionQualitySummary"]["selected_sync_peers"].as_u64(),
            Some(1)
        );
        assert_eq!(
            selection_summary["selectionLongTermSummary"]["top_trusted_sync_peer_id"].as_u64(),
            Some(10)
        );
        assert_eq!(
            selection_summary["selectionWindowPolicy"]["long_term_rounds"].as_u64(),
            Some(256)
        );
        assert_eq!(
            selection_summary["runtimeConfig"]["selection_window_policy"]
                ["sync_medium_term_weight_bps"]
                .as_u64(),
            Some(8_500)
        );
        assert_eq!(
            selection_summary["runtimeConfigSource"]["config_file_found"].as_bool(),
            Some(false)
        );
        assert_eq!(
            selection_summary["selectedSyncPeers"]
                .as_array()
                .map(Vec::len),
            Some(1)
        );

        clear_eth_fullnode_native_worker_runtime_snapshot_for_chain_v1(chain_id);
    }

    #[test]
    fn runtime_query_pending_tx_by_hash_falls_back_to_tombstone_after_cleanup() {
        let chain_id = 99_160_701_u64;
        let tx_hash = [0xe1; 32];
        clear_eth_fullnode_native_worker_runtime_snapshot_for_chain_v1(chain_id);
        novovm_network::observe_network_runtime_native_pending_tx_local_ingress_with_payload_v1(
            chain_id,
            tx_hash,
            Some(&[0x01]),
        );
        for _ in 0..13 {
            novovm_network::observe_network_runtime_native_pending_tx_propagation_failure_v1(
                chain_id,
                tx_hash,
                Some(0x21),
                novovm_network::NetworkRuntimeNativePendingTxPropagationStopReasonV1::TemporaryTimeout,
                "sync",
            );
        }
        let _ = novovm_network::snapshot_network_runtime_native_pending_tx_summary_v1(chain_id);
        let out = run_mainline_query(
            &sample_store(),
            "supervm_getEthNativePendingTxByHash",
            &json!({
                "chainId": chain_id,
                "txHash": to_hex_prefixed(&tx_hash),
            }),
        )
        .expect("pending tx tombstone query");
        assert_eq!(out["found"].as_bool(), Some(true));
        assert_eq!(out["pendingTxFound"].as_bool(), Some(false));
        assert_eq!(out["tombstoneFound"].as_bool(), Some(true));
        assert!(out["pendingTx"].is_null());
        assert_eq!(
            out["failureClassificationContract"].as_str(),
            Some(ETH_PENDING_TX_PROPAGATION_CLASSIFICATION_CONTRACT_V1)
        );
        assert_eq!(
            out["tombstone"]["finalDisposition"].as_str(),
            Some("evicted")
        );
        assert_eq!(
            out["tombstone"]["failureClassificationContract"].as_str(),
            Some(ETH_PENDING_TX_PROPAGATION_CLASSIFICATION_CONTRACT_V1)
        );
        assert_eq!(
            out["tombstone"]["failureClass"].as_str(),
            Some("temporary_timeout")
        );
        assert_eq!(
            out["tombstone"]["failureClassSource"].as_str(),
            Some("propagation_stop_reason")
        );
        assert_eq!(
            out["tombstone"]["failureRecoverability"].as_str(),
            Some("recoverable")
        );
        assert_eq!(
            out["tombstone"]["txHash"].as_str(),
            Some(to_hex_prefixed(&tx_hash).as_str())
        );

        let tombstone_by_hash = run_mainline_query(
            &sample_store(),
            "supervm_getEthNativePendingTxTombstoneByHash",
            &json!({
                "chainId": chain_id,
                "txHash": to_hex_prefixed(&tx_hash),
            }),
        )
        .expect("pending tx tombstone by hash query");
        assert_eq!(tombstone_by_hash["found"].as_bool(), Some(true));
        assert_eq!(
            tombstone_by_hash["failureClassificationContract"].as_str(),
            Some(ETH_PENDING_TX_PROPAGATION_CLASSIFICATION_CONTRACT_V1)
        );
        assert_eq!(
            tombstone_by_hash["tombstone"]["finalDisposition"].as_str(),
            Some("evicted")
        );

        let tombstones = run_mainline_query(
            &sample_store(),
            "supervm_getEthNativePendingTxTombstones",
            &json!({
                "chainId": chain_id,
                "limit": 16,
                "windowMs": 3600000u64,
            }),
        )
        .expect("pending tx tombstones query");
        assert_eq!(tombstones["found"].as_bool(), Some(true));
        assert_eq!(
            tombstones["failureClassificationContract"].as_str(),
            Some(ETH_PENDING_TX_PROPAGATION_CLASSIFICATION_CONTRACT_V1)
        );
        assert_eq!(tombstones["tombstoneCount"].as_u64(), Some(1));
        assert_eq!(
            tombstones["topStopReason"].as_str(),
            Some("temporary_timeout")
        );
        assert_eq!(
            tombstones["finalDispositionCounts"]["evicted"].as_u64(),
            Some(1)
        );

        let cleanup_summary = run_mainline_query(
            &sample_store(),
            "supervm_getEthNativePendingTxCleanupSummary",
            &json!({
                "chainId": chain_id,
                "windowMs": 3600000u64,
                "highPressureThreshold": 1,
                "criticalPressureThreshold": 5,
            }),
        )
        .expect("pending tx cleanup summary query");
        assert_eq!(
            cleanup_summary["cleanupPressure"]["status"].as_str(),
            Some("degraded")
        );
        assert_eq!(
            cleanup_summary["failureClassificationContract"].as_str(),
            Some(ETH_PENDING_TX_PROPAGATION_CLASSIFICATION_CONTRACT_V1)
        );
        assert_eq!(
            cleanup_summary["windowFinalDispositionCounts"]["evicted"].as_u64(),
            Some(1)
        );
        assert_eq!(
            cleanup_summary["windowStopReasonCounts"]
                .as_array()
                .and_then(|items| items.first())
                .and_then(|value| value.get("reason"))
                .and_then(Value::as_str),
            Some("temporary_timeout")
        );
        clear_eth_fullnode_native_worker_runtime_snapshot_for_chain_v1(chain_id);
    }

    #[test]
    fn sync_degradation_reports_broadcast_no_available_peer() {
        let chain_id = 1;
        let block_context = novovm_network::EthFullnodeBlockContextV1 {
            source: novovm_network::EthFullnodeBlockViewSource::NativeChainSync,
            chain_id,
            block_number: 0x80,
            canonical_batch_seq: None,
            block_hash: [0xaa; 32],
            parent_block_hash: [0xab; 32],
            state_root: [0xbb; 32],
            state_version: 1,
            tx_count: 0,
        };
        let mut snapshot =
            sample_native_runtime_snapshot_for_block_context(chain_id, &block_context, None);
        snapshot.candidate_peer_ids = vec![0x11];
        snapshot.lifecycle_summary = EthPeerLifecycleSummaryV1 {
            chain_id,
            peer_count: 1,
            cooldown_count: 1,
            ..EthPeerLifecycleSummaryV1::default()
        };
        snapshot.native_pending_tx_broadcast_runtime =
            novovm_network::NetworkRuntimeNativePendingTxBroadcastRuntimeSummaryV1 {
                chain_id,
                dispatch_total: 0,
                dispatch_success_total: 0,
                dispatch_failed_total: 0,
                candidate_tx_total: 3,
                broadcast_tx_total: 0,
                last_peer_id: None,
                last_candidate_count: 0,
                last_broadcast_tx_count: 0,
                last_updated_unix_ms: Some(77),
            };
        let reasons = sync_degradation_reasons_v1(&snapshot);
        assert!(reasons.contains(&"broadcast_no_available_peer"));
    }

    #[test]
    fn sync_degradation_reports_broadcast_repeated_failure() {
        let chain_id = 1;
        let block_context = novovm_network::EthFullnodeBlockContextV1 {
            source: novovm_network::EthFullnodeBlockViewSource::NativeChainSync,
            chain_id,
            block_number: 0x81,
            canonical_batch_seq: None,
            block_hash: [0xba; 32],
            parent_block_hash: [0xbb; 32],
            state_root: [0xbc; 32],
            state_version: 1,
            tx_count: 0,
        };
        let mut snapshot =
            sample_native_runtime_snapshot_for_block_context(chain_id, &block_context, None);
        snapshot.candidate_peer_ids = vec![0x12];
        snapshot.lifecycle_summary = EthPeerLifecycleSummaryV1 {
            chain_id,
            peer_count: 1,
            ready_count: 1,
            ..EthPeerLifecycleSummaryV1::default()
        };
        snapshot.native_pending_tx_broadcast_runtime =
            novovm_network::NetworkRuntimeNativePendingTxBroadcastRuntimeSummaryV1 {
                chain_id,
                dispatch_total: 4,
                dispatch_success_total: 1,
                dispatch_failed_total: 3,
                candidate_tx_total: 4,
                broadcast_tx_total: 4,
                last_peer_id: Some(0x12),
                last_candidate_count: 1,
                last_broadcast_tx_count: 1,
                last_updated_unix_ms: Some(88),
            };
        let reasons = sync_degradation_reasons_v1(&snapshot);
        assert!(reasons.contains(&"broadcast_repeated_failure"));
        assert_eq!(
            sync_degradation_primary_reason_v1(reasons.as_slice()),
            Some("broadcast_repeated_failure")
        );
    }

    #[test]
    fn sync_degradation_reports_pending_cleanup_pressure() {
        let chain_id = 1;
        let block_context = novovm_network::EthFullnodeBlockContextV1 {
            source: novovm_network::EthFullnodeBlockViewSource::NativeChainSync,
            chain_id,
            block_number: 0x90,
            canonical_batch_seq: None,
            block_hash: [0xca; 32],
            parent_block_hash: [0xcb; 32],
            state_root: [0xcc; 32],
            state_version: 1,
            tx_count: 0,
        };
        let mut snapshot =
            sample_native_runtime_snapshot_for_block_context(chain_id, &block_context, None);
        snapshot.lifecycle_summary = EthPeerLifecycleSummaryV1 {
            chain_id,
            peer_count: 1,
            ready_count: 1,
            ..EthPeerLifecycleSummaryV1::default()
        };
        snapshot.native_pending_tx_summary.evicted_count = 300;
        let reasons = sync_degradation_reasons_v1(&snapshot);
        assert!(reasons.contains(&"pending_cleanup_pressure_critical"));
        assert_eq!(
            sync_degradation_primary_reason_v1(reasons.as_slice()),
            Some("pending_cleanup_pressure_critical")
        );
        let (_status, _reasons, _primary, root_cause, root_signals, _selection) =
            runtime_root_cause_bundle_v1(Some(&snapshot));
        assert_eq!(root_cause, Some("mempool_pressure_issue"));
        assert_eq!(root_signals["mempoolPressureIssue"].as_bool(), Some(true));
    }

    #[test]
    fn sync_degradation_reports_execution_budget_pressure() {
        let chain_id = 1;
        let block_context = novovm_network::EthFullnodeBlockContextV1 {
            source: novovm_network::EthFullnodeBlockViewSource::NativeChainSync,
            chain_id,
            block_number: 0x91,
            canonical_batch_seq: None,
            block_hash: [0xda; 32],
            parent_block_hash: [0xdb; 32],
            state_root: [0xdc; 32],
            state_version: 1,
            tx_count: 0,
        };
        let mut snapshot =
            sample_native_runtime_snapshot_for_block_context(chain_id, &block_context, None);
        snapshot.lifecycle_summary = EthPeerLifecycleSummaryV1 {
            chain_id,
            peer_count: 1,
            ready_count: 1,
            ..EthPeerLifecycleSummaryV1::default()
        };
        snapshot
            .native_execution_budget_runtime
            .execution_budget_hit_count = 2;
        snapshot
            .native_execution_budget_runtime
            .execution_deferred_count = 5;
        snapshot
            .native_execution_budget_runtime
            .hard_budget_per_tick = Some(64);
        snapshot.native_execution_budget_runtime.hard_time_slice_ms = Some(10);
        snapshot
            .native_execution_budget_runtime
            .target_budget_per_tick = Some(48);
        snapshot
            .native_execution_budget_runtime
            .target_time_slice_ms = Some(8);
        snapshot
            .native_execution_budget_runtime
            .effective_budget_per_tick = Some(32);
        snapshot
            .native_execution_budget_runtime
            .effective_time_slice_ms = Some(6);
        snapshot
            .native_execution_budget_runtime
            .last_execution_target_reason =
            Some("sync_pressure_high+recent_execution_throttle".to_string());
        snapshot
            .native_execution_budget_runtime
            .last_execution_throttle_reason =
            Some("host_exec_budget_per_tick_exhausted".to_string());
        let execution_budget_json = execution_budget_light_json_v1(&snapshot);
        assert_eq!(
            execution_budget_json["hardExecutionBudgetPerTick"].as_u64(),
            Some(64)
        );
        assert_eq!(
            execution_budget_json["targetExecutionBudgetPerTick"].as_u64(),
            Some(48)
        );
        assert_eq!(
            execution_budget_json["effectiveExecutionBudgetPerTick"].as_u64(),
            Some(32)
        );
        assert_eq!(
            execution_budget_json["lastExecutionTargetReason"].as_str(),
            Some("sync_pressure_high+recent_execution_throttle")
        );
        let reasons = sync_degradation_reasons_v1(&snapshot);
        assert!(reasons.contains(&"execution_budget_pressure_high"));
        assert_eq!(
            sync_degradation_primary_reason_v1(reasons.as_slice()),
            Some("execution_budget_pressure_high")
        );
        let (_status, _reasons, _primary, root_cause, root_signals, _selection) =
            runtime_root_cause_bundle_v1(Some(&snapshot));
        assert_eq!(root_cause, Some("execution_budget_issue"));
        assert_eq!(root_signals["executionBudgetIssue"].as_bool(), Some(true));
    }
}
