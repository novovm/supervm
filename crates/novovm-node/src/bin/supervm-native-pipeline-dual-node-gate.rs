#![forbid(unsafe_code)]

use anyhow::{bail, Context, Result};
use serde_json::Value;
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

fn default_report_path_v1() -> PathBuf {
    PathBuf::from("artifacts/native-pipeline/native-pipeline-dual-node-gate-report.json")
}

fn report_path_v1() -> PathBuf {
    string_env_nonempty("NOVOVM_NATIVE_PIPELINE_DUAL_GATE_REPORT_PATH")
        .map(PathBuf::from)
        .unwrap_or_else(default_report_path_v1)
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
        if key == "NOVOVM_NATIVE_EXECUTION_PIPELINE_UDP_PEERS"
            || key == "NOVOVM_NATIVE_EXECUTION_PIPELINE_INGRESS_FIXTURE_TX_COUNT"
            || key == "NOVOVM_NATIVE_EXECUTION_PIPELINE_INGRESS_RAW_TX"
            || key == "NOVOVM_NATIVE_EXECUTION_PIPELINE_INGRESS_RAW_TX_FILE"
        {
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
        bail!(
            "{label} node failed: status={} stderr={}",
            output.status,
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
        if sender_rounds > 1 { 1_000 } else { 0 },
    )?;
    let min_receiver_canonical_tps_x1000 = u64_env(
        "NOVOVM_NATIVE_PIPELINE_DUAL_GATE_MIN_RECEIVER_CANONICAL_TPS_X1000",
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
        tx_count.min(udp_recv_budget).max(1),
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

    let common = |ticks: u64, store: &PathBuf, node: u64, listen: &str, peer: &str| {
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
            (
                "NOVOVM_NATIVE_EXECUTION_TICK_STORE_PATH",
                store.display().to_string(),
            ),
            (
                "NOVOVM_NATIVE_EXECUTION_STORE_BACKEND",
                store_backend.clone(),
            ),
            (
                "NOVOVM_NATIVE_EXECUTION_PIPELINE_REQUIRE_ROCKSDB_STORE",
                "true".to_string(),
            ),
            (
                "NOVOVM_NATIVE_EXECUTION_PIPELINE_UDP_ENABLED",
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
                "0".to_string(),
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
        ]);
        let mut cmd = Command::new(&node_bin);
        cmd.env_clear();
        for (key, value) in std::env::vars() {
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
        let sender_out = run_node_v1(&node_bin, round_env.as_slice())
            .with_context(|| format!("run sender round {} failed", round + 1))?;
        sender_summaries.push(parse_summary_v1(
            &sender_out,
            format!("sender_round_{}", round + 1).as_str(),
        )?);
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
        receiver_summaries.push(parse_summary_v1(
            &receiver_out,
            format!("receiver_{}", idx + 1).as_str(),
        )?);
    }
    let receiver_summary = receiver_summaries
        .first()
        .cloned()
        .ok_or_else(|| anyhow::anyhow!("dual-node gate did not collect receiver summaries"))?;
    let sender_broadcast_tps_x1000 = tps_x1000(
        summary_u64(&sender_summary, "broadcast_tx_total_last"),
        summary_u64(&sender_summary, "elapsed_ms"),
    );
    let receiver_canonical_tps_x1000 = tps_x1000(
        summary_u64(&receiver_summary, "included_canonical_total"),
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
            require_min(
                summary,
                "included_canonical_total",
                tx_count,
                label.as_str(),
            )?;
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
        if receiver_canonical_tps_x1000 < min_receiver_canonical_tps_x1000 {
            bail!(
                "receiver summary gate failed: canonical_tps_x1000={} below min {}",
                receiver_canonical_tps_x1000,
                min_receiver_canonical_tps_x1000
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
            "receiver_canonical_tps_x1000": receiver_canonical_tps_x1000,
            "min_sender_broadcast_tps_x1000": min_sender_broadcast_tps_x1000,
            "min_receiver_canonical_tps_x1000": min_receiver_canonical_tps_x1000,
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
    });
    let report_path = report_path_v1();
    write_report_v1(report_path.as_path(), &report)?;
    println!(
        "{}",
        serde_json::to_string_pretty(&report).context("encode dual node gate report failed")?
    );
    Ok(())
}
