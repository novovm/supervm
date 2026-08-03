#![forbid(unsafe_code)]

use anyhow::{bail, Context, Result};
use novovm_node::native_block_ledger::{
    NovNativeBlockLedgerV1, NOV_NATIVE_BLOCK_LEDGER_MAX_HYDRATE_BLOCKS_V1,
};
use novovm_node::tx_ingress::{
    native_business_protocol_config_commitment_v1, NOV_NATIVE_AOEM_NATIVE_TX_BATCH_COMPARE_ENV,
    NOV_NATIVE_AOEM_NATIVE_TX_BATCH_PRODUCTION_CANDIDATE_ENV,
    NOV_NATIVE_AOEM_NATIVE_TX_BATCH_SHADOW_ENV, NOV_NATIVE_AOEM_SEMANTIC_LEDGER_MIRROR_ENV,
    NOV_NATIVE_BLOCK_LEDGER_ROCKSDB_PATH_ENV, NOV_NATIVE_EXECUTION_STORE_ROCKSDB_PATH_ENV,
    NOV_NATIVE_LEGACY_HOST_TRANSITIONAL_FALLBACK_ENV,
    NOV_NATIVE_PROTOCOL_CONFIG_EXPECTED_COMMITMENT_ENV,
    NOV_NATIVE_SEND_RAW_TRANSACTION_PIPELINE_ONLY_ENV,
};
use serde_json::Value;
use std::collections::HashSet;
use std::fs;
use std::net::UdpSocket;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Output, Stdio};
use std::time::{SystemTime, UNIX_EPOCH};

fn string_env_nonempty(name: &str) -> Option<String> {
    std::env::var(name)
        .ok()
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
}

fn u64_env(name: &str, default: u64) -> Result<u64> {
    let Some(raw) = string_env_nonempty(name) else {
        return Ok(default);
    };
    raw.parse::<u64>()
        .with_context(|| format!("{name} must be u64"))
}

fn div_ceil_u64_v1(value: u64, divisor: u64) -> u64 {
    if divisor == 0 {
        return 0;
    }
    value.saturating_add(divisor).saturating_sub(1) / divisor
}

fn reserve_udp_addr_v1() -> Result<String> {
    let socket = UdpSocket::bind("127.0.0.1:0").context("reserve udp addr failed")?;
    Ok(socket
        .local_addr()
        .context("read reserved udp addr failed")?
        .to_string())
}

fn novovm_node_bin_v1() -> PathBuf {
    if let Some(path) = string_env_nonempty("NOVOVM_NATIVE_PIPELINE_NODE_BIN") {
        return PathBuf::from(path);
    }
    let Ok(current) = std::env::current_exe() else {
        return PathBuf::from("novovm-node");
    };
    let Some(dir) = current.parent() else {
        return PathBuf::from("novovm-node");
    };
    let exe = if cfg!(windows) {
        "novovm-node.exe"
    } else {
        "novovm-node"
    };
    dir.join(exe)
}

fn temp_store_path_v1(name: &str, chain_id: u64) -> PathBuf {
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_millis())
        .unwrap_or_default();
    std::env::temp_dir().join(format!(
        "novovm-native-pipeline-dual-node-{name}-{chain_id}-{}-{now}.json",
        std::process::id()
    ))
}

fn append_path_suffix_v1(path: &Path, suffix: &str) -> PathBuf {
    let mut raw = path.as_os_str().to_os_string();
    raw.push(suffix);
    PathBuf::from(raw)
}

fn default_report_path_v1() -> PathBuf {
    PathBuf::from("artifacts/native-pipeline/native-pipeline-dual-node-gate-report.json")
}

fn report_path_v1() -> PathBuf {
    string_env_nonempty("NOVOVM_NATIVE_PIPELINE_DUAL_GATE_REPORT_PATH")
        .map(PathBuf::from)
        .unwrap_or_else(default_report_path_v1)
}

fn inherit_child_env_v1(key: &str) -> bool {
    ![
        "NOVOVM_NATIVE_EXECUTION_PIPELINE_UDP_PEERS",
        "NOVOVM_NATIVE_EXECUTION_PIPELINE_INGRESS_FIXTURE_TX_COUNT",
        "NOVOVM_NATIVE_EXECUTION_PIPELINE_INGRESS_RAW_TX",
        "NOVOVM_NATIVE_EXECUTION_PIPELINE_INGRESS_RAW_TX_FILE",
        "NOVOVM_NOVORUDP_RUN_ID",
        "NOVOVM_NETWORK_RUN_ID",
        "NOVOVM_NOVORUDP_OUTBOUND_RUN_ID",
        "NOVOVM_NETWORK_OUTBOUND_RUN_ID",
    ]
    .iter()
    .any(|blocked| key.eq_ignore_ascii_case(blocked))
}

fn write_report_v1(path: &Path, report: &Value) -> Result<()> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).with_context(|| {
            format!(
                "create dual-node gate report dir failed: {}",
                parent.display()
            )
        })?;
    }
    let encoded =
        serde_json::to_string_pretty(report).context("encode dual-node gate report failed")?;
    fs::write(path, encoded)
        .with_context(|| format!("write dual-node gate report failed: {}", path.display()))
}

fn run_node_v1(bin: &PathBuf, envs: &[(&str, String)]) -> Result<Output> {
    let mut cmd = Command::new(bin);
    cmd.env_clear();
    for (key, value) in std::env::vars() {
        if !inherit_child_env_v1(key.as_str()) {
            continue;
        }
        cmd.env(key, value);
    }
    for (key, value) in envs {
        cmd.env(key, value);
    }
    cmd.output()
        .with_context(|| format!("run novovm-node child failed: {}", bin.display()))
}

fn parse_summary_v1(output: &Output, label: &str) -> Result<Value> {
    if !output.status.success() {
        let diagnostic = serde_json::from_slice::<Value>(&output.stdout)
            .map(|summary| {
                serde_json::json!({
                    "aoem_executed_total": summary_u64(&summary, "aoem_executed_total"),
                    "aoem_deferred_total": summary_u64(&summary, "aoem_deferred_total"),
                    "queue_pending_last": summary_u64(&summary, "queue_pending_last"),
                    "tx_ingress_selected_path": summary.get("tx_ingress_selected_path"),
                    "aoem_native_tx_batch_production_candidate_result_ok": summary.get("aoem_native_tx_batch_production_candidate_result_ok"),
                    "aoem_native_tx_batch_production_mismatch_reasons": summary.get("aoem_native_tx_batch_production_mismatch_reasons"),
                    "aoem_owned_signoff_blocker_reasons": summary.get("aoem_owned_signoff_blocker_reasons"),
                })
            })
            .map(|summary| summary.to_string())
            .unwrap_or_else(|_| String::from_utf8_lossy(&output.stdout).into_owned());
        bail!(
            "{label} node failed: status={} diagnostic={} stderr={}",
            output.status,
            diagnostic,
            String::from_utf8_lossy(&output.stderr)
        );
    }
    serde_json::from_slice::<Value>(&output.stdout).with_context(|| {
        format!(
            "{label} node did not return JSON summary: stdout={} stderr={}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        )
    })
}

fn terminate_receiver_children_v1(children: &mut [(u64, String, Child)]) {
    for (_, _, child) in children {
        match child.try_wait() {
            Ok(Some(_)) => {}
            Ok(None) | Err(_) => {
                let _ = child.kill();
                let _ = child.wait();
            }
        }
    }
}

fn summary_u64(summary: &Value, field: &str) -> u64 {
    summary
        .get(field)
        .and_then(Value::as_u64)
        .unwrap_or_default()
}

fn tps_x1000(count: u64, elapsed_ms: u64) -> u64 {
    if elapsed_ms == 0 {
        return 0;
    }
    count.saturating_mul(1_000_000) / elapsed_ms
}

fn join_udp_peers_v1(peers: &[(u64, String)]) -> String {
    peers
        .iter()
        .map(|(node, addr)| format!("{node}={addr}"))
        .collect::<Vec<_>>()
        .join(",")
}

fn require_min(summary: &Value, field: &str, min: u64, label: &str) -> Result<()> {
    let actual = summary_u64(summary, field);
    if actual < min {
        bail!("{label} summary gate failed: {field}={actual} below min {min}");
    }
    Ok(())
}

fn require_eq_str(summary: &Value, field: &str, expected: &str, label: &str) -> Result<()> {
    let actual = summary.get(field).and_then(Value::as_str).unwrap_or("-");
    if actual != expected {
        bail!("{label} summary gate failed: {field}={actual}, expected {expected}");
    }
    Ok(())
}

fn validate_unsealed_receiver_completion_v1(
    summary: &Value,
    tx_count: u64,
    label: &str,
) -> Result<()> {
    require_min(
        summary,
        "ledger_receipt_proof_close_success_count",
        tx_count,
        label,
    )?;
    require_min(summary, "ledger_completed_count", tx_count, label)?;
    require_min(
        summary,
        "queue_included_non_canonical_last",
        tx_count,
        label,
    )?;
    for field in [
        "included_canonical_total",
        "ledger_canonical_proof_close_success_count",
        "ledger_receipt_proof_missing_sequence_mapping_count",
        "ledger_durable_missing_count",
    ] {
        let actual = summary_u64(summary, field);
        if actual != 0 {
            bail!(
                "{label} summary gate failed: {field}={actual} expected 0 for an authenticated unsealed candidate"
            );
        }
    }
    for field in [
        "aoem_native_tx_batch_production_candidate_enabled",
        "aoem_native_tx_batch_production_candidate_result_ok",
        "aoem_owned_child_runtime_gate_propagated_to_tx_ingress",
        "aoem_owned_single_path_enforced",
        "aoem_owned_regression_signable",
    ] {
        if summary.get(field).and_then(Value::as_bool) != Some(true) {
            bail!("{label} summary gate failed: {field} is not true");
        }
    }
    for field in [
        "legacy_host_transitional_fallback_used",
        "aoem_native_tx_batch_production_fallback_used",
        "aoem_native_tx_batch_production_double_write_legacy_canonical",
    ] {
        if summary.get(field).and_then(Value::as_bool) != Some(false) {
            bail!("{label} summary gate failed: {field} is not false");
        }
    }
    for field in ["tx_ingress_selected_path", "tx_ingress_production_target"] {
        require_eq_str(
            summary,
            field,
            "aoem_runtime_owned_state_persistence",
            label,
        )?;
    }
    require_eq_str(
        summary,
        "aoem_native_tx_batch_production_owner",
        "aoem_runtime_owned_state_persistence",
        label,
    )?;
    let receipt_count = summary_u64(summary, "aoem_native_tx_batch_production_receipt_count");
    if receipt_count != tx_count {
        bail!(
            "{label} summary gate failed: aoem_native_tx_batch_production_receipt_count={receipt_count} expected {tx_count}"
        );
    }
    for field in [
        "aoem_native_tx_batch_production_mismatch_reasons",
        "aoem_owned_signoff_blocker_reasons",
    ] {
        let is_empty = summary
            .get(field)
            .and_then(Value::as_array)
            .is_some_and(Vec::is_empty);
        if !is_empty {
            bail!("{label} summary gate failed: {field} is missing or non-empty");
        }
    }
    Ok(())
}

fn validate_durable_unsealed_ledger_v1(
    native_store: &Path,
    chain_id: u64,
    tx_count: u64,
    protocol_config_commitment: &str,
    label: &str,
) -> Result<Value> {
    let ledger_path = append_path_suffix_v1(native_store, ".block-ledger.rocksdb");
    let ledger = NovNativeBlockLedgerV1::open_existing_read_only(ledger_path.as_path())?
        .with_context(|| {
            format!(
                "{label} durable NOV native block ledger is missing: {}",
                ledger_path.display()
            )
        })?;
    let ownership = ledger
        .load_aoem_ownership()?
        .with_context(|| format!("{label} durable AOEM ownership binding is missing"))?;
    if ownership.chain_id != chain_id
        || ownership.protocol_config_commitment != protocol_config_commitment
    {
        bail!(
            "{label} durable AOEM ownership binding mismatch: chain={} protocol={} expected_chain={} expected_protocol={}",
            ownership.chain_id,
            ownership.protocol_config_commitment,
            chain_id,
            protocol_config_commitment
        );
    }

    let status = ledger.status(chain_id)?;
    if status.prepared.is_some() {
        bail!("{label} durable block ledger retained a prepared candidate after completion");
    }
    if !status.canonical_local || status.safe || status.finalized || status.proof_sealed {
        bail!(
            "{label} durable block ledger status is not local-unsealed: canonical_local={} safe={} finalized={} proof_sealed={}",
            status.canonical_local,
            status.safe,
            status.finalized,
            status.proof_sealed
        );
    }
    let head = status
        .head
        .as_ref()
        .with_context(|| format!("{label} durable block ledger head is missing"))?;
    if !head.canonical_local || head.safe || head.finalized || head.proof_sealed {
        bail!(
            "{label} durable block ledger head is not local-unsealed: canonical_local={} safe={} finalized={} proof_sealed={}",
            head.canonical_local,
            head.safe,
            head.finalized,
            head.proof_sealed
        );
    }
    if head.cumulative_tx_count != tx_count {
        bail!(
            "{label} durable block ledger cumulative_tx_count={} expected {tx_count}",
            head.cumulative_tx_count
        );
    }
    let block_count = usize::try_from(head.block_count)
        .with_context(|| format!("{label} durable block count exceeds usize"))?;
    if block_count == 0 || block_count > NOV_NATIVE_BLOCK_LEDGER_MAX_HYDRATE_BLOCKS_V1 {
        bail!(
            "{label} durable block count {block_count} is outside the bounded audit range 1..={} ",
            NOV_NATIVE_BLOCK_LEDGER_MAX_HYDRATE_BLOCKS_V1
        );
    }
    let blocks = ledger.load_blocks_from_height(chain_id, 1, block_count)?;
    if blocks.len() != block_count {
        bail!(
            "{label} durable block range length={} expected {block_count}",
            blocks.len()
        );
    }

    let mut seen_tx_hashes = HashSet::<[u8; 32]>::with_capacity(tx_count as usize);
    let mut verified_tx_count = 0u64;
    let mut verified_body_bytes = 0u64;
    for block in &blocks {
        let header = &block.header;
        let body = &block.body;
        let evidence = &block.execution_evidence;
        if header.candidate_kind != "local_unsealed_execution_candidate"
            || !header.aoem_readback_verified
            || !header.canonical_local
            || header.safe
            || header.finalized
            || header.proof_sealed
            || evidence.proof_sealed
        {
            bail!(
                "{label} block height={} is not an AOEM-readback-verified local unsealed candidate",
                header.height
            );
        }
        let tx_len = body.tx_hashes.len();
        if body.raw_txs.len() != tx_len
            || evidence.per_block_receipt_commitments.len() != tx_len
            || usize::try_from(header.tx_count).ok() != Some(tx_len)
            || usize::try_from(header.receipt_count).ok() != Some(tx_len)
        {
            bail!(
                "{label} block height={} transaction/body/receipt cardinality mismatch",
                header.height
            );
        }
        for (index, tx_hash) in body.tx_hashes.iter().enumerate() {
            if !seen_tx_hashes.insert(*tx_hash) {
                bail!(
                    "{label} duplicate transaction hash in durable ledger at height={} index={index}",
                    header.height
                );
            }
            let tx_location = ledger
                .load_tx_location(chain_id, *tx_hash)?
                .with_context(|| {
                    format!(
                        "{label} transaction index missing at height={} index={index}",
                        header.height
                    )
                })?;
            let receipt_location = ledger
                .load_receipt_location(chain_id, *tx_hash)?
                .with_context(|| {
                    format!(
                        "{label} receipt index missing at height={} index={index}",
                        header.height
                    )
                })?;
            let expected_index = u32::try_from(index).context("durable tx index exceeds u32")?;
            if tx_location.height != header.height
                || tx_location.block_hash != header.block_hash
                || tx_location.tx_index != expected_index
                || !tx_location.canonical_local
                || receipt_location.height != header.height
                || receipt_location.block_hash != header.block_hash
                || receipt_location.tx_index != expected_index
                || !receipt_location.canonical_local
                || receipt_location.proof_sealed
            {
                bail!(
                    "{label} durable transaction/receipt reverse index mismatch at height={} index={index}",
                    header.height
                );
            }
        }
        verified_tx_count = verified_tx_count.saturating_add(tx_len as u64);
        verified_body_bytes = verified_body_bytes.saturating_add(header.body_bytes);
    }
    if verified_tx_count != tx_count || verified_body_bytes != head.cumulative_body_bytes {
        bail!(
            "{label} durable ledger aggregate mismatch: tx_count={verified_tx_count}/{tx_count} body_bytes={verified_body_bytes}/{}",
            head.cumulative_body_bytes
        );
    }
    let last = blocks
        .last()
        .with_context(|| format!("{label} durable block range is empty"))?;
    if last.header.height != head.height
        || last.header.block_hash != head.block_hash
        || last.header.post_state_root != head.post_state_root
        || last.header.cumulative_receipt_root != head.cumulative_receipt_root
        || last.header.state_version != head.state_version
    {
        bail!("{label} durable block range does not terminate at the verified head");
    }

    Ok(serde_json::json!({
        "ledger_path": ledger_path,
        "ownership_chain_id": ownership.chain_id,
        "ownership_namespace_digest": ownership.namespace_digest,
        "protocol_config_commitment": ownership.protocol_config_commitment,
        "candidate_kind": "local_unsealed_execution_candidate",
        "block_count": head.block_count,
        "head_height": head.height,
        "cumulative_tx_count": head.cumulative_tx_count,
        "cumulative_body_bytes": head.cumulative_body_bytes,
        "verified_tx_index_count": verified_tx_count,
        "verified_receipt_index_count": verified_tx_count,
        "aoem_readback_verified": true,
        "canonical_local": true,
        "chain_canonical": false,
        "safe": false,
        "finalized": false,
        "proof_sealed": false,
        "prepared_present": false,
    }))
}

fn sender_round_aggregate_v1(summaries: &[Value]) -> Value {
    let elapsed_ms = summaries
        .iter()
        .map(|summary| summary_u64(summary, "elapsed_ms"))
        .sum::<u64>()
        .max(1);
    let ticks = summaries
        .iter()
        .map(|summary| summary_u64(summary, "ticks"))
        .sum::<u64>();
    let aoem_executed_total = summaries
        .iter()
        .map(|summary| summary_u64(summary, "aoem_executed_total"))
        .sum::<u64>();
    let aoem_deferred_total = summaries
        .iter()
        .map(|summary| summary_u64(summary, "aoem_deferred_total"))
        .sum::<u64>();
    let max_aoem_batch_executed_per_tick = summaries
        .iter()
        .map(|summary| summary_u64(summary, "max_aoem_batch_executed_per_tick"))
        .max()
        .unwrap_or_default();
    let max_proof_items_per_tick = summaries
        .iter()
        .map(|summary| summary_u64(summary, "max_proof_items_per_tick"))
        .max()
        .unwrap_or_default();
    let max_commit_items_per_tick = summaries
        .iter()
        .map(|summary| summary_u64(summary, "max_commit_items_per_tick"))
        .max()
        .unwrap_or_default();
    let max_broadcast_tx_per_tick = summaries
        .iter()
        .map(|summary| summary_u64(summary, "max_broadcast_tx_per_tick"))
        .max()
        .unwrap_or_default();
    let nonempty_aoem_batch_ticks = summaries
        .iter()
        .map(|summary| summary_u64(summary, "nonempty_aoem_batch_ticks"))
        .sum::<u64>();
    let nonempty_proof_ticks = summaries
        .iter()
        .map(|summary| summary_u64(summary, "nonempty_proof_ticks"))
        .sum::<u64>();
    let nonempty_commit_ticks = summaries
        .iter()
        .map(|summary| summary_u64(summary, "nonempty_commit_ticks"))
        .sum::<u64>();
    let network_enabled_ticks = summaries
        .iter()
        .map(|summary| summary_u64(summary, "network_enabled_ticks"))
        .sum::<u64>();
    let network_ok_ticks = summaries
        .iter()
        .map(|summary| summary_u64(summary, "network_ok_ticks"))
        .sum::<u64>();
    let network_error_ticks = summaries
        .iter()
        .map(|summary| summary_u64(summary, "network_error_ticks"))
        .sum::<u64>();
    let ingress_submitted_total = summaries
        .iter()
        .map(|summary| summary_u64(summary, "ingress_submitted_total"))
        .sum::<u64>();
    let max_product_ingress_submitted_per_tick = summaries
        .iter()
        .map(|summary| summary_u64(summary, "max_product_ingress_submitted_per_tick"))
        .max()
        .unwrap_or_default();
    let max_network_received_per_tick = summaries
        .iter()
        .map(|summary| summary_u64(summary, "max_network_received_per_tick"))
        .max()
        .unwrap_or_default();
    let max_queue_admitted_per_tick = summaries
        .iter()
        .map(|summary| summary_u64(summary, "max_queue_admitted_per_tick"))
        .max()
        .unwrap_or_default();
    let ingress_error_ticks = summaries
        .iter()
        .map(|summary| summary_u64(summary, "ingress_error_ticks"))
        .sum::<u64>();
    let proof_ticks = summaries
        .iter()
        .map(|summary| summary_u64(summary, "proof_ticks"))
        .sum::<u64>();
    let commit_ticks = summaries
        .iter()
        .map(|summary| summary_u64(summary, "commit_ticks"))
        .sum::<u64>();
    let broadcast_dispatch_total_last = summaries
        .iter()
        .map(|summary| summary_u64(summary, "broadcast_dispatch_total_last"))
        .sum::<u64>();
    let broadcast_tx_total_last = summaries
        .iter()
        .map(|summary| summary_u64(summary, "broadcast_tx_total_last"))
        .sum::<u64>();
    let queue_dropped_last = summaries
        .iter()
        .map(|summary| summary_u64(summary, "queue_dropped_last"))
        .sum::<u64>();
    let queue_rejected_last = summaries
        .iter()
        .map(|summary| summary_u64(summary, "queue_rejected_last"))
        .sum::<u64>();

    serde_json::json!({
        "method": "nov_runNativeExecutionPipelineSenderAggregate",
        "accepted": true,
        "execution_kernel": "AOEM",
        "aoem_concurrency_owner": "AOEM_runtime",
        "host_concurrency_policy": "host_drives_lifecycle_only_no_rust_execution_scheduler",
        "rounds": summaries.len() as u64,
        "ticks": ticks,
        "elapsed_ms": elapsed_ms,
        "ticks_per_sec_x1000": ticks.saturating_mul(1_000_000) / elapsed_ms,
        "aoem_executed_total": aoem_executed_total,
        "aoem_deferred_total": aoem_deferred_total,
        "max_aoem_batch_executed_per_tick": max_aoem_batch_executed_per_tick,
        "max_proof_items_per_tick": max_proof_items_per_tick,
        "max_commit_items_per_tick": max_commit_items_per_tick,
        "max_broadcast_tx_per_tick": max_broadcast_tx_per_tick,
        "nonempty_aoem_batch_ticks": nonempty_aoem_batch_ticks,
        "nonempty_proof_ticks": nonempty_proof_ticks,
        "nonempty_commit_ticks": nonempty_commit_ticks,
        "network_enabled_ticks": network_enabled_ticks,
        "network_ok_ticks": network_ok_ticks,
        "network_error_ticks": network_error_ticks,
        "ingress_submitted_total": ingress_submitted_total,
        "max_product_ingress_submitted_per_tick": max_product_ingress_submitted_per_tick,
        "max_network_received_per_tick": max_network_received_per_tick,
        "max_queue_admitted_per_tick": max_queue_admitted_per_tick,
        "ingress_error_ticks": ingress_error_ticks,
        "proof_ticks": proof_ticks,
        "commit_ticks": commit_ticks,
        "broadcast_dispatch_total_last": broadcast_dispatch_total_last,
        "broadcast_tx_total_last": broadcast_tx_total_last,
        "queue_dropped_last": queue_dropped_last,
        "queue_rejected_last": queue_rejected_last,
        "progress_score": ingress_submitted_total.saturating_add(broadcast_tx_total_last),
    })
}

fn main() -> Result<()> {
    let chain_id = u64_env("NOVOVM_NATIVE_PIPELINE_DUAL_GATE_CHAIN_ID", 9_998_895)?;
    let tx_count = u64_env("NOVOVM_NATIVE_PIPELINE_DUAL_GATE_TX_COUNT", 8)?;
    let tick_budget = u64_env("NOVOVM_NATIVE_PIPELINE_DUAL_GATE_TICK_BUDGET", 4)?.max(1);
    let tick_interval_ms = u64_env("NOVOVM_NATIVE_PIPELINE_DUAL_GATE_TICK_INTERVAL_MS", 25)?;
    let ingress_max_per_tick = u64_env(
        "NOVOVM_NATIVE_PIPELINE_DUAL_GATE_INGRESS_MAX_PER_TICK",
        tick_budget,
    )?;
    let udp_broadcast_max_per_tick = u64_env(
        "NOVOVM_NATIVE_PIPELINE_DUAL_GATE_UDP_BROADCAST_MAX_PER_TICK",
        tick_budget,
    )?;
    let udp_recv_budget = u64_env("NOVOVM_NATIVE_PIPELINE_DUAL_GATE_UDP_RECV_BUDGET", 16)?;
    let startup_wait_ms = u64_env("NOVOVM_NATIVE_PIPELINE_DUAL_GATE_STARTUP_WAIT_MS", 300)?;
    let sender_rounds = u64_env("NOVOVM_NATIVE_PIPELINE_DUAL_GATE_SENDER_ROUNDS", 1)?.max(1);
    let receiver_count = u64_env("NOVOVM_NATIVE_PIPELINE_DUAL_GATE_RECEIVER_COUNT", 1)?.clamp(1, 8);
    let sender_round_interval_ms = u64_env(
        "NOVOVM_NATIVE_PIPELINE_DUAL_GATE_SENDER_ROUND_INTERVAL_MS",
        tick_interval_ms,
    )?;
    let sender_round_process_budget_ms = u64_env(
        "NOVOVM_NATIVE_PIPELINE_DUAL_GATE_SENDER_ROUND_PROCESS_BUDGET_MS",
        if sender_rounds > 1 { 180_000 } else { 0 },
    )?;
    if string_env_nonempty("NOVOVM_NATIVE_PIPELINE_DUAL_GATE_MIN_RECEIVER_CANONICAL_TPS_X1000")
        .is_some()
    {
        bail!(
            "NOVOVM_NATIVE_PIPELINE_DUAL_GATE_MIN_RECEIVER_CANONICAL_TPS_X1000 is obsolete: this gate verifies local unsealed candidates, not chain-canonical blocks; use NOVOVM_NATIVE_PIPELINE_DUAL_GATE_MIN_RECEIVER_UNSEALED_CANDIDATE_TPS_X1000"
        );
    }
    let min_receiver_unsealed_candidate_tps_x1000 = u64_env(
        "NOVOVM_NATIVE_PIPELINE_DUAL_GATE_MIN_RECEIVER_UNSEALED_CANDIDATE_TPS_X1000",
        0,
    )?;
    let min_receiver_max_aoem_batch_per_tick = u64_env(
        "NOVOVM_NATIVE_PIPELINE_DUAL_GATE_MIN_RECEIVER_MAX_AOEM_BATCH_PER_TICK",
        tx_count.min(tick_budget).max(1),
    )?;
    let min_sender_max_product_ingress_per_tick = u64_env(
        "NOVOVM_NATIVE_PIPELINE_DUAL_GATE_MIN_SENDER_MAX_PRODUCT_INGRESS_PER_TICK",
        tx_count.min(ingress_max_per_tick).max(1),
    )?;
    let min_receiver_max_network_received_per_tick = u64_env(
        "NOVOVM_NATIVE_PIPELINE_DUAL_GATE_MIN_RECEIVER_MAX_NETWORK_RECEIVED_PER_TICK",
        tx_count
            .min(udp_broadcast_max_per_tick)
            .min(udp_recv_budget)
            .max(1),
    )?;
    let min_receiver_max_queue_admitted_per_tick = u64_env(
        "NOVOVM_NATIVE_PIPELINE_DUAL_GATE_MIN_RECEIVER_MAX_QUEUE_ADMITTED_PER_TICK",
        tx_count.min(tick_budget).max(1),
    )?;
    let min_receiver_max_proof_items_per_tick = u64_env(
        "NOVOVM_NATIVE_PIPELINE_DUAL_GATE_MIN_RECEIVER_MAX_PROOF_ITEMS_PER_TICK",
        min_receiver_max_aoem_batch_per_tick,
    )?;
    let min_receiver_max_commit_items_per_tick = u64_env(
        "NOVOVM_NATIVE_PIPELINE_DUAL_GATE_MIN_RECEIVER_MAX_COMMIT_ITEMS_PER_TICK",
        min_receiver_max_aoem_batch_per_tick,
    )?;
    let min_sender_max_broadcast_tx_per_tick = u64_env(
        "NOVOVM_NATIVE_PIPELINE_DUAL_GATE_MIN_SENDER_MAX_BROADCAST_TX_PER_TICK",
        tx_count
            .min(tick_budget)
            .saturating_mul(receiver_count)
            .max(1),
    )?;
    let min_sender_broadcast_tps_x1000 = u64_env(
        "NOVOVM_NATIVE_PIPELINE_DUAL_GATE_MIN_SENDER_BROADCAST_TPS_X1000",
        0,
    )?;
    let store_backend = string_env_nonempty("NOVOVM_NATIVE_PIPELINE_DUAL_GATE_STORE_BACKEND")
        .unwrap_or_else(|| "dual".to_string());
    let max_sender_round_tx_count = div_ceil_u64_v1(tx_count, sender_rounds);
    let ingress_ticks = div_ceil_u64_v1(max_sender_round_tx_count, ingress_max_per_tick.max(1));
    let total_ingress_ticks = div_ceil_u64_v1(tx_count, ingress_max_per_tick.max(1));
    let execution_ticks = div_ceil_u64_v1(tx_count, tick_budget);
    let min_nonempty_batch_ticks = execution_ticks;
    let startup_ticks = div_ceil_u64_v1(startup_wait_ms, tick_interval_ms.max(1));
    let sender_round_interval_ticks =
        div_ceil_u64_v1(sender_round_interval_ms, tick_interval_ms.max(1));
    let sender_round_process_budget_ticks = div_ceil_u64_v1(
        sender_round_process_budget_ms.saturating_mul(sender_rounds),
        tick_interval_ms.max(1),
    );
    let sender_ticks = u64_env(
        "NOVOVM_NATIVE_PIPELINE_DUAL_GATE_SENDER_TICKS",
        ingress_ticks.max(3),
    )?;
    let receiver_ticks = u64_env(
        "NOVOVM_NATIVE_PIPELINE_DUAL_GATE_RECEIVER_TICKS",
        startup_ticks
            .saturating_add(sender_ticks.saturating_mul(sender_rounds))
            .saturating_add(
                sender_round_interval_ticks.saturating_mul(sender_rounds.saturating_sub(1)),
            )
            .saturating_add(sender_round_process_budget_ticks)
            .saturating_add(execution_ticks.max(6))
            .saturating_add(12),
    )?;
    let sender_node = u64_env("NOVOVM_NATIVE_PIPELINE_DUAL_GATE_SENDER_NODE", 9_991_895)?;
    let receiver_node = u64_env("NOVOVM_NATIVE_PIPELINE_DUAL_GATE_RECEIVER_NODE", 9_991_896)?;
    let transport_run_id = format!("novovm-dual-node-gate-{chain_id}-{}", std::process::id());
    let transport_auth_key = format!(
        "novovm-dual-node-gate-key-{chain_id}-{}",
        std::process::id()
    );
    let udp_broadcast_max_propagations = u64_env(
        "NOVOVM_NATIVE_PIPELINE_DUAL_GATE_UDP_BROADCAST_MAX_PROPAGATIONS",
        receiver_count
            .saturating_mul(sender_rounds.max(1))
            .saturating_mul(2)
            .max(3),
    )?;
    let sender_addr = string_env_nonempty("NOVOVM_NATIVE_PIPELINE_DUAL_GATE_SENDER_ADDR")
        .map(Ok)
        .unwrap_or_else(reserve_udp_addr_v1)?;
    let receiver_addr_override =
        string_env_nonempty("NOVOVM_NATIVE_PIPELINE_DUAL_GATE_RECEIVER_ADDR");
    let mut receivers = Vec::<(u64, String, PathBuf)>::with_capacity(receiver_count as usize);
    for idx in 0..receiver_count {
        let node = receiver_node.saturating_add(idx);
        let addr = if idx == 0 {
            receiver_addr_override
                .clone()
                .map(Ok)
                .unwrap_or_else(reserve_udp_addr_v1)?
        } else {
            reserve_udp_addr_v1()?
        };
        receivers.push((
            node,
            addr,
            temp_store_path_v1(&format!("receiver-{idx}"), chain_id),
        ));
    }
    let sender_store = temp_store_path_v1("sender", chain_id);
    let node_bin = novovm_node_bin_v1();
    if !node_bin.exists() {
        bail!(
            "novovm-node binary not found: {}; build with `cargo build -p novovm-node --bins` or set NOVOVM_NATIVE_PIPELINE_NODE_BIN",
            node_bin.display()
        );
    }
    let protocol_config_commitment = native_business_protocol_config_commitment_v1()
        .context("compute dual-node gate NOV business protocol configuration commitment")?;

    let common = |ticks: u64, store: &PathBuf, node: u64, listen: &str, peer: &str| {
        let host_rocksdb = append_path_suffix_v1(store.as_path(), ".rocksdb");
        let block_ledger = append_path_suffix_v1(store.as_path(), ".block-ledger.rocksdb");
        let semantic_mirror = append_path_suffix_v1(store.as_path(), ".aoem-semantic-ledger.jsonl");
        let unified_account = append_path_suffix_v1(store.as_path(), ".unified-account.rocksdb");
        vec![
            ("NOVOVM_NODE_MODE", "native_execution_pipeline".to_string()),
            (
                "NOVOVM_NATIVE_EXECUTION_TICK_CHAIN_ID",
                chain_id.to_string(),
            ),
            ("NOVOVM_NATIVE_EXECUTION_TICK_MAX_TICKS", ticks.to_string()),
            (
                "NOVOVM_NATIVE_EXECUTION_TICK_INTERVAL_MS",
                tick_interval_ms.to_string(),
            ),
            (
                "NOVOVM_NATIVE_EXECUTION_TICK_HARD_BUDGET",
                tick_budget.to_string(),
            ),
            (
                "NOVOVM_NATIVE_EXECUTION_TICK_TARGET_BUDGET",
                tick_budget.to_string(),
            ),
            (
                "NOVOVM_NATIVE_EXECUTION_TICK_EFFECTIVE_BUDGET",
                tick_budget.to_string(),
            ),
            ("NOVOVM_NATIVE_EXECUTION_STORE", store.display().to_string()),
            (
                NOV_NATIVE_EXECUTION_STORE_ROCKSDB_PATH_ENV,
                host_rocksdb.display().to_string(),
            ),
            (
                NOV_NATIVE_BLOCK_LEDGER_ROCKSDB_PATH_ENV,
                block_ledger.display().to_string(),
            ),
            (
                NOV_NATIVE_AOEM_SEMANTIC_LEDGER_MIRROR_ENV,
                semantic_mirror.display().to_string(),
            ),
            (
                "NOVOVM_UNIFIED_ACCOUNT_DB",
                unified_account.display().to_string(),
            ),
            (
                "NOVOVM_NATIVE_EXECUTION_STORE_BACKEND",
                store_backend.clone(),
            ),
            ("NOVOVM_AOEM_VARIANT", "core".to_string()),
            ("NOVOVM_AOEM_PERSIST_BACKEND", "rocksdb".to_string()),
            (
                "AOEM_PERSISTENCE_PATH",
                store
                    .with_extension("aoem-persistence")
                    .display()
                    .to_string(),
            ),
            (
                "NOVOVM_AOEM_OWNED_STATE_DB_PATH",
                store
                    .with_extension("aoem-owned.rocksdb")
                    .display()
                    .to_string(),
            ),
            (
                "NOVOVM_AOEM_STATE_NAMESPACE",
                format!("dual-node-gate-chain-{chain_id}-node-{node}"),
            ),
            (
                "NOVOVM_NATIVE_AOEM_SEMANTIC_INGRESS_ENABLED",
                "true".to_string(),
            ),
            (
                "NOVOVM_NATIVE_AOEM_SEMANTIC_INGRESS_REQUIRED",
                "true".to_string(),
            ),
            (
                NOV_NATIVE_AOEM_NATIVE_TX_BATCH_PRODUCTION_CANDIDATE_ENV,
                "true".to_string(),
            ),
            (
                NOV_NATIVE_AOEM_NATIVE_TX_BATCH_COMPARE_ENV,
                "false".to_string(),
            ),
            (
                NOV_NATIVE_AOEM_NATIVE_TX_BATCH_SHADOW_ENV,
                "false".to_string(),
            ),
            (
                NOV_NATIVE_LEGACY_HOST_TRANSITIONAL_FALLBACK_ENV,
                "false".to_string(),
            ),
            (
                NOV_NATIVE_SEND_RAW_TRANSACTION_PIPELINE_ONLY_ENV,
                "true".to_string(),
            ),
            (
                NOV_NATIVE_PROTOCOL_CONFIG_EXPECTED_COMMITMENT_ENV,
                protocol_config_commitment.clone(),
            ),
            (
                "NOVOVM_NATIVE_EXECUTION_PIPELINE_REQUIRE_ROCKSDB_STORE",
                "true".to_string(),
            ),
            (
                "NOVOVM_NATIVE_EXECUTION_PIPELINE_UDP_ENABLED",
                "true".to_string(),
            ),
            // The UDP underlay is valid only when it carries NovoRUDP frames.
            ("NOVOVM_NATIVE_PIPELINE_TRANSPORT", "novorudp".to_string()),
            (
                "NOVOVM_NOVORUDP_CONTROL_FRAME_AUTH_KEY",
                transport_auth_key.clone(),
            ),
            (
                "NOVOVM_NOVORUDP_CONTROL_FRAME_AUTH_REQUIRED",
                "true".to_string(),
            ),
            (
                "NOVOVM_NATIVE_EXECUTION_PIPELINE_UDP_LISTEN_ADDR",
                listen.to_string(),
            ),
            (
                "NOVOVM_NATIVE_EXECUTION_PIPELINE_LOCAL_NODE",
                node.to_string(),
            ),
            (
                "NOVOVM_NATIVE_EXECUTION_PIPELINE_UDP_PEERS",
                peer.to_string(),
            ),
            (
                "NOVOVM_NATIVE_EXECUTION_PIPELINE_UDP_BROADCAST_MAX_PER_TICK",
                udp_broadcast_max_per_tick.to_string(),
            ),
            (
                "NOVOVM_NATIVE_EXECUTION_PIPELINE_UDP_BROADCAST_MAX_PROPAGATIONS",
                udp_broadcast_max_propagations.to_string(),
            ),
            (
                "NOVOVM_NATIVE_EXECUTION_PIPELINE_UDP_RECV_BUDGET",
                udp_recv_budget.to_string(),
            ),
            (
                "NOVOVM_NATIVE_EXECUTION_PIPELINE_BROADCAST_ENABLED",
                "false".to_string(),
            ),
            (
                "NOVOVM_NATIVE_EXECUTION_PIPELINE_REQUIRE_PROGRESS",
                "true".to_string(),
            ),
            (
                "NOVOVM_NATIVE_EXECUTION_PIPELINE_QUIET_TICKS",
                "true".to_string(),
            ),
            (
                "NOVOVM_NATIVE_EXECUTION_PIPELINE_EXIT_WHEN_SUMMARY_VALID",
                "true".to_string(),
            ),
            (
                "NOVOVM_NATIVE_EXECUTION_PIPELINE_MIN_MAX_AOEM_BATCH_EXECUTED_PER_TICK",
                "0".to_string(),
            ),
            (
                "NOVOVM_NATIVE_EXECUTION_PIPELINE_MIN_MAX_PROOF_ITEMS_PER_TICK",
                "0".to_string(),
            ),
            (
                "NOVOVM_NATIVE_EXECUTION_PIPELINE_MIN_MAX_COMMIT_ITEMS_PER_TICK",
                "0".to_string(),
            ),
        ]
    };

    let receiver_peer = format!("{sender_node}={sender_addr}");
    let sender_peer = join_udp_peers_v1(
        receivers
            .iter()
            .map(|(node, addr, _)| (*node, addr.clone()))
            .collect::<Vec<_>>()
            .as_slice(),
    );
    let mut sender_env = common(
        sender_ticks,
        &sender_store,
        sender_node,
        &sender_addr,
        &sender_peer,
    );
    sender_env.extend([
        (
            "NOVOVM_NATIVE_EXECUTION_PIPELINE_UDP_BROADCAST_ENABLED",
            "true".to_string(),
        ),
        (
            "NOVOVM_NATIVE_EXECUTION_PIPELINE_INGRESS_MAX_PER_TICK",
            ingress_max_per_tick.to_string(),
        ),
        (
            "NOVOVM_NATIVE_EXECUTION_PIPELINE_MIN_BROADCAST_DISPATCH",
            ingress_ticks.to_string(),
        ),
        (
            "NOVOVM_NATIVE_EXECUTION_PIPELINE_MIN_MAX_PRODUCT_INGRESS_SUBMITTED_PER_TICK",
            min_sender_max_product_ingress_per_tick.to_string(),
        ),
    ]);

    let mut receiver_children = Vec::<(u64, String, Child)>::with_capacity(receiver_count as usize);
    for (node, addr, store) in &receivers {
        let mut receiver_env = common(receiver_ticks, store, *node, addr, &receiver_peer);
        receiver_env.extend([
            (
                "NOVOVM_NATIVE_EXECUTION_PIPELINE_REQUIRE_PROGRESS",
                "false".to_string(),
            ),
            (
                "NOVOVM_NATIVE_EXECUTION_PIPELINE_UDP_BROADCAST_ENABLED",
                "false".to_string(),
            ),
            (
                "NOVOVM_NATIVE_EXECUTION_PIPELINE_MIN_AOEM_EXECUTED",
                tx_count.to_string(),
            ),
            (
                "NOVOVM_NATIVE_EXECUTION_PIPELINE_MIN_NONEMPTY_AOEM_BATCH_TICKS",
                min_nonempty_batch_ticks.to_string(),
            ),
            (
                "NOVOVM_NATIVE_EXECUTION_PIPELINE_MIN_NONEMPTY_PROOF_TICKS",
                min_nonempty_batch_ticks.to_string(),
            ),
            (
                "NOVOVM_NATIVE_EXECUTION_PIPELINE_MIN_NONEMPTY_COMMIT_TICKS",
                min_nonempty_batch_ticks.to_string(),
            ),
            (
                "NOVOVM_NATIVE_EXECUTION_PIPELINE_MIN_MAX_NETWORK_RECEIVED_PER_TICK",
                min_receiver_max_network_received_per_tick.to_string(),
            ),
            (
                "NOVOVM_NATIVE_EXECUTION_PIPELINE_MIN_MAX_QUEUE_ADMITTED_PER_TICK",
                min_receiver_max_queue_admitted_per_tick.to_string(),
            ),
            (
                "NOVOVM_NATIVE_EXECUTION_PIPELINE_MIN_MAX_AOEM_BATCH_EXECUTED_PER_TICK",
                min_receiver_max_aoem_batch_per_tick.to_string(),
            ),
            (
                "NOVOVM_NATIVE_EXECUTION_PIPELINE_MIN_MAX_PROOF_ITEMS_PER_TICK",
                min_receiver_max_proof_items_per_tick.to_string(),
            ),
            (
                "NOVOVM_NATIVE_EXECUTION_PIPELINE_MIN_MAX_COMMIT_ITEMS_PER_TICK",
                min_receiver_max_commit_items_per_tick.to_string(),
            ),
        ]);
        let mut cmd = Command::new(&node_bin);
        cmd.env_clear();
        for (key, value) in std::env::vars() {
            if !inherit_child_env_v1(key.as_str()) {
                continue;
            }
            cmd.env(key, value);
        }
        for (key, value) in &receiver_env {
            cmd.env(key, value);
        }
        cmd.stdout(Stdio::piped()).stderr(Stdio::piped());
        let child = cmd.spawn().with_context(|| {
            format!(
                "spawn receiver node failed: node={} addr={} bin={}",
                node,
                addr,
                node_bin.display()
            )
        })?;
        receiver_children.push((*node, addr.clone(), child));
    }
    std::thread::sleep(std::time::Duration::from_millis(startup_wait_ms));
    let mut sender_summaries = Vec::with_capacity(sender_rounds as usize);
    let mut sent_total = 0u64;
    for round in 0..sender_rounds {
        let remaining = tx_count.saturating_sub(sent_total);
        if remaining == 0 {
            break;
        }
        let round_tx_count = remaining.min(max_sender_round_tx_count.max(1));
        let mut round_env = sender_env.clone();
        round_env.extend([
            (
                "NOVOVM_NOVORUDP_OUTBOUND_RUN_ID",
                format!("{transport_run_id}-round-{}", round + 1),
            ),
            (
                "NOVOVM_NATIVE_EXECUTION_PIPELINE_INGRESS_FIXTURE_TX_COUNT",
                round_tx_count.to_string(),
            ),
            (
                "NOVOVM_NATIVE_EXECUTION_PIPELINE_INGRESS_FIXTURE_NONCE_START",
                sent_total.saturating_add(1).to_string(),
            ),
            (
                "NOVOVM_NATIVE_EXECUTION_PIPELINE_MIN_INGRESS_SUBMITTED",
                round_tx_count.to_string(),
            ),
            (
                "NOVOVM_NATIVE_EXECUTION_PIPELINE_MIN_BROADCAST_TX",
                round_tx_count.to_string(),
            ),
        ]);
        let sender_round = run_node_v1(&node_bin, round_env.as_slice())
            .with_context(|| format!("run sender round {} failed", round + 1))
            .and_then(|sender_out| {
                parse_summary_v1(&sender_out, format!("sender_round_{}", round + 1).as_str())
            });
        let sender_round = match sender_round {
            Ok(summary) => summary,
            Err(error) => {
                terminate_receiver_children_v1(receiver_children.as_mut_slice());
                return Err(error);
            }
        };
        sender_summaries.push(sender_round);
        sent_total = sent_total.saturating_add(round_tx_count);
        if round + 1 < sender_rounds && sent_total < tx_count {
            std::thread::sleep(std::time::Duration::from_millis(sender_round_interval_ms));
        }
    }
    let sender_summary = sender_round_aggregate_v1(sender_summaries.as_slice());
    let mut receiver_summaries = Vec::<Value>::with_capacity(receiver_count as usize);
    for (idx, (node, addr, child)) in receiver_children.into_iter().enumerate() {
        let receiver_out = child
            .wait_with_output()
            .with_context(|| format!("wait receiver node failed: node={node} addr={addr}"))?;
        receiver_summaries.push(
            parse_summary_v1(&receiver_out, format!("receiver_{}", idx + 1).as_str())
                .with_context(|| {
                    format!(
                        "receiver failed after all sender rounds; sender_summary={}",
                        serde_json::to_string(&sender_summary).unwrap_or_else(|_| "-".to_string())
                    )
                })?,
        );
    }
    let receiver_summary = receiver_summaries
        .first()
        .cloned()
        .ok_or_else(|| anyhow::anyhow!("dual-node gate did not collect receiver summaries"))?;
    let durable_receiver_ledgers = receivers
        .iter()
        .enumerate()
        .map(|(idx, (_, _, store))| {
            validate_durable_unsealed_ledger_v1(
                store.as_path(),
                chain_id,
                tx_count,
                protocol_config_commitment.as_str(),
                format!("receiver_{}", idx + 1).as_str(),
            )
        })
        .collect::<Result<Vec<_>>>()?;
    let sender_broadcast_tps_x1000 = tps_x1000(
        summary_u64(&sender_summary, "broadcast_tx_total_last"),
        summary_u64(&sender_summary, "elapsed_ms"),
    );
    let receiver_unsealed_candidate_tps_x1000 = tps_x1000(
        durable_receiver_ledgers
            .first()
            .and_then(|summary| summary.get("cumulative_tx_count"))
            .and_then(Value::as_u64)
            .unwrap_or_default(),
        summary_u64(&receiver_summary, "elapsed_ms"),
    );

    let validation = (|| -> Result<()> {
        require_eq_str(&sender_summary, "execution_kernel", "AOEM", "sender")?;
        require_eq_str(
            &sender_summary,
            "aoem_concurrency_owner",
            "AOEM_runtime",
            "sender",
        )?;
        require_eq_str(
            &sender_summary,
            "host_concurrency_policy",
            "host_drives_lifecycle_only_no_rust_execution_scheduler",
            "sender",
        )?;
        require_min(&sender_summary, "network_ok_ticks", 1, "sender")?;
        if summary_u64(&sender_summary, "queue_dropped_last") != 0 {
            bail!(
                "sender summary gate failed: queue_dropped_last={} expected 0",
                summary_u64(&sender_summary, "queue_dropped_last")
            );
        }
        if summary_u64(&sender_summary, "queue_rejected_last") != 0 {
            bail!(
                "sender summary gate failed: queue_rejected_last={} expected 0",
                summary_u64(&sender_summary, "queue_rejected_last")
            );
        }
        for (idx, summary) in receiver_summaries.iter().enumerate() {
            let label = format!("receiver_{}", idx + 1);
            require_eq_str(summary, "execution_kernel", "AOEM", label.as_str())?;
            require_eq_str(
                summary,
                "aoem_concurrency_owner",
                "AOEM_runtime",
                label.as_str(),
            )?;
            require_eq_str(
                summary,
                "host_concurrency_policy",
                "host_drives_lifecycle_only_no_rust_execution_scheduler",
                label.as_str(),
            )?;
            require_min(summary, "network_ok_ticks", 1, label.as_str())?;
            require_min(summary, "aoem_executed_total", tx_count, label.as_str())?;
            require_min(
                summary,
                "max_aoem_batch_executed_per_tick",
                min_receiver_max_aoem_batch_per_tick,
                label.as_str(),
            )?;
            require_min(
                summary,
                "max_network_received_per_tick",
                min_receiver_max_network_received_per_tick,
                label.as_str(),
            )?;
            require_min(
                summary,
                "max_queue_admitted_per_tick",
                min_receiver_max_queue_admitted_per_tick,
                label.as_str(),
            )?;
            require_min(
                summary,
                "max_proof_items_per_tick",
                min_receiver_max_proof_items_per_tick,
                label.as_str(),
            )?;
            require_min(
                summary,
                "max_commit_items_per_tick",
                min_receiver_max_commit_items_per_tick,
                label.as_str(),
            )?;
            require_min(
                summary,
                "nonempty_aoem_batch_ticks",
                min_nonempty_batch_ticks,
                label.as_str(),
            )?;
            require_min(
                summary,
                "nonempty_proof_ticks",
                min_nonempty_batch_ticks,
                label.as_str(),
            )?;
            require_min(
                summary,
                "nonempty_commit_ticks",
                min_nonempty_batch_ticks,
                label.as_str(),
            )?;
            require_min(summary, "proof_ticks", execution_ticks, label.as_str())?;
            require_min(summary, "commit_ticks", execution_ticks, label.as_str())?;
            validate_unsealed_receiver_completion_v1(summary, tx_count, label.as_str())?;
            if summary_u64(summary, "queue_pending_last") != 0 {
                bail!(
                    "{label} summary gate failed: queue_pending_last={} expected 0",
                    summary_u64(summary, "queue_pending_last")
                );
            }
            if summary_u64(summary, "queue_dropped_last") != 0 {
                bail!(
                    "{label} summary gate failed: queue_dropped_last={} expected 0",
                    summary_u64(summary, "queue_dropped_last")
                );
            }
            if summary_u64(summary, "queue_rejected_last") != 0 {
                bail!(
                    "{label} summary gate failed: queue_rejected_last={} expected 0",
                    summary_u64(summary, "queue_rejected_last")
                );
            }
        }
        require_min(
            &sender_summary,
            "ingress_submitted_total",
            tx_count,
            "sender",
        )?;
        require_min(
            &sender_summary,
            "max_product_ingress_submitted_per_tick",
            min_sender_max_product_ingress_per_tick,
            "sender",
        )?;
        require_min(
            &sender_summary,
            "broadcast_tx_total_last",
            tx_count.saturating_mul(receiver_count),
            "sender",
        )?;
        require_min(
            &sender_summary,
            "broadcast_dispatch_total_last",
            total_ingress_ticks.saturating_mul(receiver_count),
            "sender",
        )?;
        require_min(
            &sender_summary,
            "max_broadcast_tx_per_tick",
            min_sender_max_broadcast_tx_per_tick,
            "sender",
        )?;
        if sender_broadcast_tps_x1000 < min_sender_broadcast_tps_x1000 {
            bail!(
                "sender summary gate failed: broadcast_tps_x1000={} below min {}",
                sender_broadcast_tps_x1000,
                min_sender_broadcast_tps_x1000
            );
        }
        if receiver_unsealed_candidate_tps_x1000 < min_receiver_unsealed_candidate_tps_x1000 {
            bail!(
                "receiver summary gate failed: unsealed_candidate_tps_x1000={} below min {}",
                receiver_unsealed_candidate_tps_x1000,
                min_receiver_unsealed_candidate_tps_x1000
            );
        }
        Ok(())
    })();
    if let Err(err) = validation {
        bail!(
            "{err}; sender_summary={}; receiver_summary={}",
            serde_json::to_string(&sender_summary).unwrap_or_else(|_| "-".to_string()),
            serde_json::to_string(&receiver_summary).unwrap_or_else(|_| "-".to_string())
        );
    }

    let report = serde_json::json!({
        "method": "supervm_native_pipeline_dual_node_gate",
        "accepted": true,
        "chain_id": chain_id,
        "tx_count": tx_count,
        "tick_budget": tick_budget,
        "tick_interval_ms": tick_interval_ms,
        "startup_wait_ms": startup_wait_ms,
        "receiver_count": receiver_count,
        "sender_rounds": sender_rounds,
        "sender_round_interval_ms": sender_round_interval_ms,
        "sender_round_process_budget_ms": sender_round_process_budget_ms,
        "sender_ticks": sender_ticks,
        "receiver_ticks": receiver_ticks,
        "receiver_completion_class": "authenticated_receipt_closed_aoem_owned_durable_unsealed_candidate_not_chain_canonical",
        "protocol_config_commitment": protocol_config_commitment,
        "transport_run_id_base": transport_run_id,
        "store_backend": store_backend,
        "ingress_max_per_tick": ingress_max_per_tick,
        "total_ingress_ticks": total_ingress_ticks,
        "udp_broadcast_max_per_tick": udp_broadcast_max_per_tick,
        "udp_broadcast_max_propagations": udp_broadcast_max_propagations,
        "udp_recv_budget": udp_recv_budget,
        "sender_addr": sender_addr,
        "receiver_addr": receivers
            .first()
            .map(|(_, addr, _)| addr.clone())
            .unwrap_or_default(),
        "receiver_addrs": receivers
            .iter()
            .map(|(node, addr, _)| serde_json::json!({"node": node, "addr": addr}))
            .collect::<Vec<_>>(),
        "metrics": {
            "sender_broadcast_tps_x1000": sender_broadcast_tps_x1000,
            "receiver_unsealed_candidate_tps_x1000": receiver_unsealed_candidate_tps_x1000,
            "min_sender_broadcast_tps_x1000": min_sender_broadcast_tps_x1000,
            "min_receiver_unsealed_candidate_tps_x1000": min_receiver_unsealed_candidate_tps_x1000,
            "min_sender_max_product_ingress_per_tick": min_sender_max_product_ingress_per_tick,
            "min_receiver_max_network_received_per_tick": min_receiver_max_network_received_per_tick,
            "min_receiver_max_queue_admitted_per_tick": min_receiver_max_queue_admitted_per_tick,
            "min_receiver_max_aoem_batch_per_tick": min_receiver_max_aoem_batch_per_tick,
            "min_receiver_max_proof_items_per_tick": min_receiver_max_proof_items_per_tick,
            "min_receiver_max_commit_items_per_tick": min_receiver_max_commit_items_per_tick,
            "min_sender_max_broadcast_tx_per_tick": min_sender_max_broadcast_tx_per_tick,
            "min_nonempty_batch_ticks": min_nonempty_batch_ticks,
        },
        "sender_summary": sender_summary,
        "sender_round_summaries": sender_summaries,
        "receiver_summary": receiver_summary,
        "receiver_summaries": receiver_summaries,
        "durable_receiver_ledgers": durable_receiver_ledgers,
    });
    let report_path = report_path_v1();
    write_report_v1(report_path.as_path(), &report)?;
    println!(
        "{}",
        serde_json::to_string_pretty(&report).context("encode dual node gate report failed")?
    );
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn unsealed_summary_v1() -> Value {
        serde_json::json!({
            "ledger_receipt_proof_close_success_count": 8,
            "ledger_completed_count": 8,
            "queue_included_non_canonical_last": 8,
            "included_canonical_total": 0,
            "ledger_canonical_proof_close_success_count": 0,
            "ledger_receipt_proof_missing_sequence_mapping_count": 0,
            "ledger_durable_missing_count": 0,
            "aoem_native_tx_batch_production_candidate_enabled": true,
            "aoem_native_tx_batch_production_candidate_result_ok": true,
            "aoem_owned_child_runtime_gate_propagated_to_tx_ingress": true,
            "aoem_owned_single_path_enforced": true,
            "aoem_owned_regression_signable": true,
            "legacy_host_transitional_fallback_used": false,
            "aoem_native_tx_batch_production_fallback_used": false,
            "aoem_native_tx_batch_production_double_write_legacy_canonical": false,
            "tx_ingress_selected_path": "aoem_runtime_owned_state_persistence",
            "tx_ingress_production_target": "aoem_runtime_owned_state_persistence",
            "aoem_native_tx_batch_production_owner": "aoem_runtime_owned_state_persistence",
            "aoem_native_tx_batch_production_receipt_count": 8,
            "aoem_native_tx_batch_production_mismatch_reasons": [],
            "aoem_owned_signoff_blocker_reasons": [],
        })
    }

    #[test]
    fn unsealed_receiver_completion_accepts_receipt_closed_noncanonical_execution() {
        validate_unsealed_receiver_completion_v1(&unsealed_summary_v1(), 8, "receiver")
            .expect("receipt-closed unsealed execution should satisfy the gate");
    }

    #[test]
    fn unsealed_receiver_completion_rejects_false_canonical_promotion() {
        let mut summary = unsealed_summary_v1();
        summary["included_canonical_total"] = serde_json::json!(1);
        let error = validate_unsealed_receiver_completion_v1(&summary, 8, "receiver")
            .expect_err("an unsealed execution must not satisfy a canonical gate");
        assert!(error.to_string().contains("included_canonical_total=1"));
    }

    #[test]
    fn unsealed_receiver_completion_requires_authenticated_sequence_closure() {
        let mut summary = unsealed_summary_v1();
        summary["ledger_receipt_proof_missing_sequence_mapping_count"] = serde_json::json!(1);
        let error = validate_unsealed_receiver_completion_v1(&summary, 8, "receiver")
            .expect_err("a missing authenticated sequence mapping must fail the gate");
        assert!(error
            .to_string()
            .contains("ledger_receipt_proof_missing_sequence_mapping_count=1"));
    }

    #[test]
    fn child_environment_does_not_inherit_transport_run_identity() {
        for key in [
            "NOVOVM_NOVORUDP_RUN_ID",
            "NOVOVM_NETWORK_RUN_ID",
            "NOVOVM_NOVORUDP_OUTBOUND_RUN_ID",
            "NOVOVM_NETWORK_OUTBOUND_RUN_ID",
        ] {
            assert!(!inherit_child_env_v1(key));
            assert!(!inherit_child_env_v1(key.to_ascii_lowercase().as_str()));
        }
        assert!(inherit_child_env_v1(
            "NOVOVM_NOVORUDP_CONTROL_FRAME_AUTH_KEY"
        ));
    }
}
