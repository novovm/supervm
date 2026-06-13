#![forbid(unsafe_code)]

use anyhow::{bail, Context, Result};
use novovm_node::tx_ingress::{
    get_nov_native_execution_store_recovery_probe_v1, nov_native_execution_store_rocksdb_path_v1,
};
use serde_json::Value;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};
use std::time::{SystemTime, UNIX_EPOCH};

const REPORT_SCHEMA_V1: &str = "novovm-native-pipeline-rocksdb-recovery-report/v1";

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

fn unix_ms_now() -> u128 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_millis())
        .unwrap_or_default()
}

fn div_ceil_u64(value: u64, divisor: u64) -> u64 {
    if divisor == 0 {
        return 0;
    }
    value.saturating_add(divisor).saturating_sub(1) / divisor
}

fn temp_store_path(chain_id: u64) -> PathBuf {
    std::env::temp_dir().join(format!(
        "novovm-native-pipeline-rocksdb-recovery-{chain_id}-{}-{}.json",
        std::process::id(),
        unix_ms_now()
    ))
}

fn default_report_path() -> PathBuf {
    PathBuf::from("artifacts/native-pipeline/native-pipeline-rocksdb-recovery-report.json")
}

fn report_path() -> PathBuf {
    string_env_nonempty("NOVOVM_NATIVE_PIPELINE_ROCKSDB_RECOVERY_REPORT_PATH")
        .map(PathBuf::from)
        .unwrap_or_else(default_report_path)
}

fn novovm_node_bin() -> PathBuf {
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

fn run_node(bin: &Path, envs: &[(&str, String)]) -> Result<Output> {
    let mut cmd = Command::new(bin);
    for (key, _) in std::env::vars() {
        if key == "NOVOVM_NODE_MODE"
            || key == "NOVOVM_NATIVE_SEND_RAW_TRANSACTION_PIPELINE_ONLY"
            || key.starts_with("NOVOVM_NATIVE_EXECUTION_")
            || key.starts_with("NOVOVM_NATIVE_PIPELINE_")
        {
            cmd.env_remove(key);
        }
    }
    for (key, value) in envs {
        cmd.env(key, value);
    }
    cmd.output().with_context(|| {
        format!(
            "run novovm-node rocksdb recovery child failed: {}",
            bin.display()
        )
    })
}

fn parse_summary(output: &Output, label: &str) -> Result<Value> {
    if !output.status.success() {
        bail!(
            "{label} child failed: status={} stderr={}",
            output.status,
            String::from_utf8_lossy(&output.stderr)
        );
    }
    serde_json::from_slice::<Value>(&output.stdout).with_context(|| {
        format!(
            "{label} child did not return JSON summary: stdout={} stderr={}",
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

fn summary_str<'a>(summary: &'a Value, field: &str) -> &'a str {
    summary.get(field).and_then(Value::as_str).unwrap_or("-")
}

fn require_eq_str(summary: &Value, field: &str, expected: &str, violations: &mut Vec<String>) {
    let actual = summary_str(summary, field);
    if actual != expected {
        violations.push(format!("{field}={actual}, expected {expected}"));
    }
}

fn require_min(summary: &Value, field: &str, min: u64, violations: &mut Vec<String>) {
    let actual = summary_u64(summary, field);
    if actual < min {
        violations.push(format!("{field}={actual} below min {min}"));
    }
}

fn require_max(summary: &Value, field: &str, max: u64, violations: &mut Vec<String>) {
    let actual = summary_u64(summary, field);
    if actual > max {
        violations.push(format!("{field}={actual} exceeds max {max}"));
    }
}

fn base_env(
    chain_id: u64,
    store_path: &Path,
    ticks: u64,
    tick_interval_ms: u64,
    batch_budget: u64,
) -> Vec<(&'static str, String)> {
    vec![
        ("NOVOVM_NODE_MODE", "native_execution_pipeline".to_string()),
        (
            "NOVOVM_NATIVE_EXECUTION_TICK_CHAIN_ID",
            chain_id.to_string(),
        ),
        ("NOVOVM_NATIVE_EXECUTION_TICK_MAX_TICKS", ticks.to_string()),
        (
            "NOVOVM_NATIVE_EXECUTION_TICK_HARD_BUDGET",
            batch_budget.to_string(),
        ),
        (
            "NOVOVM_NATIVE_EXECUTION_TICK_TARGET_BUDGET",
            batch_budget.to_string(),
        ),
        (
            "NOVOVM_NATIVE_EXECUTION_TICK_EFFECTIVE_BUDGET",
            batch_budget.to_string(),
        ),
        (
            "NOVOVM_NATIVE_EXECUTION_TICK_INTERVAL_MS",
            tick_interval_ms.to_string(),
        ),
        (
            "NOVOVM_NATIVE_EXECUTION_TICK_STORE_PATH",
            store_path.display().to_string(),
        ),
        (
            "NOVOVM_NATIVE_EXECUTION_STORE_BACKEND",
            "rocksdb".to_string(),
        ),
        (
            "NOVOVM_NATIVE_SEND_RAW_TRANSACTION_PIPELINE_ONLY",
            "true".to_string(),
        ),
        (
            "NOVOVM_NATIVE_EXECUTION_PIPELINE_QUIET_TICKS",
            "true".to_string(),
        ),
    ]
}

fn write_report(path: &Path, report: &Value) -> Result<()> {
    if let Some(parent) = path.parent() {
        if !parent.as_os_str().is_empty() {
            fs::create_dir_all(parent)
                .with_context(|| format!("create recovery report dir: {}", parent.display()))?;
        }
    }
    let encoded = serde_json::to_string_pretty(report).context("encode recovery report failed")?;
    fs::write(path, encoded)
        .with_context(|| format!("write recovery report failed: {}", path.display()))
}

fn main() -> Result<()> {
    let chain_id = u64_env(
        "NOVOVM_NATIVE_PIPELINE_ROCKSDB_RECOVERY_CHAIN_ID",
        9_998_899,
    )?;
    let tx_count = u64_env("NOVOVM_NATIVE_PIPELINE_ROCKSDB_RECOVERY_TX_COUNT", 64)?.max(1);
    let batch_budget = u64_env("NOVOVM_NATIVE_PIPELINE_ROCKSDB_RECOVERY_BATCH_BUDGET", 16)?.max(1);
    let tick_interval_ms = u64_env(
        "NOVOVM_NATIVE_PIPELINE_ROCKSDB_RECOVERY_TICK_INTERVAL_MS",
        5,
    )?
    .max(1);
    let execution_ticks = div_ceil_u64(tx_count, batch_budget);
    let write_ticks = u64_env(
        "NOVOVM_NATIVE_PIPELINE_ROCKSDB_RECOVERY_WRITE_TICKS",
        execution_ticks.saturating_add(4),
    )?
    .max(execution_ticks);
    let restart_ticks = u64_env("NOVOVM_NATIVE_PIPELINE_ROCKSDB_RECOVERY_RESTART_TICKS", 3)?.max(1);
    let store_path = string_env_nonempty("NOVOVM_NATIVE_PIPELINE_ROCKSDB_RECOVERY_STORE_PATH")
        .map(PathBuf::from)
        .unwrap_or_else(|| temp_store_path(chain_id));
    let node_bin = novovm_node_bin();
    if !node_bin.exists() {
        bail!(
            "novovm-node binary not found: {}; build with `cargo build -p novovm-node --bins` or set NOVOVM_NATIVE_PIPELINE_NODE_BIN",
            node_bin.display()
        );
    }

    let mut write_env = base_env(
        chain_id,
        store_path.as_path(),
        write_ticks,
        tick_interval_ms,
        batch_budget,
    );
    write_env.extend([
        (
            "NOVOVM_NATIVE_EXECUTION_PIPELINE_INGRESS_FIXTURE_TX_COUNT",
            tx_count.to_string(),
        ),
        (
            "NOVOVM_NATIVE_EXECUTION_PIPELINE_INGRESS_MAX_PER_TICK",
            batch_budget.to_string(),
        ),
        (
            "NOVOVM_NATIVE_EXECUTION_PIPELINE_INGRESS_FIXTURE_FEE_ASSET",
            "USDT".to_string(),
        ),
        (
            "NOVOVM_NATIVE_EXECUTION_PIPELINE_INGRESS_FIXTURE_MAX_PAY_AMOUNT",
            "10000".to_string(),
        ),
        (
            "NOVOVM_NATIVE_EXECUTION_PIPELINE_REQUIRE_PROGRESS",
            "true".to_string(),
        ),
        (
            "NOVOVM_NATIVE_EXECUTION_PIPELINE_REQUIRE_FULL_LIFECYCLE",
            "true".to_string(),
        ),
        (
            "NOVOVM_NATIVE_EXECUTION_PIPELINE_REQUIRE_PRODUCT_INGRESS",
            "true".to_string(),
        ),
        (
            "NOVOVM_NATIVE_EXECUTION_PIPELINE_REQUIRE_ROCKSDB_STORE",
            "true".to_string(),
        ),
        (
            "NOVOVM_NATIVE_EXECUTION_PIPELINE_MIN_TICKS",
            write_ticks.to_string(),
        ),
        (
            "NOVOVM_NATIVE_EXECUTION_PIPELINE_MIN_INGRESS_SUBMITTED",
            tx_count.to_string(),
        ),
        (
            "NOVOVM_NATIVE_EXECUTION_PIPELINE_MIN_MAX_PRODUCT_INGRESS_SUBMITTED_PER_TICK",
            batch_budget.to_string(),
        ),
        (
            "NOVOVM_NATIVE_EXECUTION_PIPELINE_MIN_MAX_QUEUE_ADMITTED_PER_TICK",
            batch_budget.to_string(),
        ),
        (
            "NOVOVM_NATIVE_EXECUTION_PIPELINE_MIN_AOEM_EXECUTED",
            tx_count.to_string(),
        ),
        (
            "NOVOVM_NATIVE_EXECUTION_PIPELINE_MIN_MAX_AOEM_BATCH_EXECUTED_PER_TICK",
            batch_budget.to_string(),
        ),
        (
            "NOVOVM_NATIVE_EXECUTION_PIPELINE_MIN_MAX_PROOF_ITEMS_PER_TICK",
            batch_budget.to_string(),
        ),
        (
            "NOVOVM_NATIVE_EXECUTION_PIPELINE_MIN_MAX_COMMIT_ITEMS_PER_TICK",
            batch_budget.to_string(),
        ),
        (
            "NOVOVM_NATIVE_EXECUTION_PIPELINE_MIN_MAX_BROADCAST_TX_PER_TICK",
            batch_budget.to_string(),
        ),
        (
            "NOVOVM_NATIVE_EXECUTION_PIPELINE_MIN_NONEMPTY_AOEM_BATCH_TICKS",
            execution_ticks.to_string(),
        ),
        (
            "NOVOVM_NATIVE_EXECUTION_PIPELINE_MIN_NONEMPTY_PROOF_TICKS",
            execution_ticks.to_string(),
        ),
        (
            "NOVOVM_NATIVE_EXECUTION_PIPELINE_MIN_NONEMPTY_COMMIT_TICKS",
            execution_ticks.to_string(),
        ),
        (
            "NOVOVM_NATIVE_EXECUTION_PIPELINE_MIN_PROOF_TICKS",
            write_ticks.to_string(),
        ),
        (
            "NOVOVM_NATIVE_EXECUTION_PIPELINE_MIN_COMMIT_TICKS",
            write_ticks.to_string(),
        ),
        (
            "NOVOVM_NATIVE_EXECUTION_PIPELINE_MIN_INCLUDED_CANONICAL_TOTAL",
            tx_count.to_string(),
        ),
        (
            "NOVOVM_NATIVE_EXECUTION_PIPELINE_MIN_BROADCAST_TX",
            tx_count.to_string(),
        ),
        (
            "NOVOVM_NATIVE_EXECUTION_PIPELINE_MIN_BROADCAST_DISPATCH",
            execution_ticks.to_string(),
        ),
        (
            "NOVOVM_NATIVE_EXECUTION_PIPELINE_MAX_QUEUE_PENDING_LAST",
            "0".to_string(),
        ),
    ]);
    let write_summary = parse_summary(
        &run_node(node_bin.as_path(), write_env.as_slice())?,
        "write",
    )?;

    std::env::set_var("NOVOVM_NATIVE_EXECUTION_STORE_BACKEND", "rocksdb");
    let recovery_probe = get_nov_native_execution_store_recovery_probe_v1(store_path.as_path())?;

    let restart_env = base_env(
        chain_id,
        store_path.as_path(),
        restart_ticks,
        tick_interval_ms,
        batch_budget,
    );
    let restart_summary = parse_summary(
        &run_node(node_bin.as_path(), restart_env.as_slice())?,
        "restart",
    )?;

    let mut violations = Vec::<String>::new();
    require_eq_str(&write_summary, "execution_kernel", "AOEM", &mut violations);
    require_eq_str(
        &write_summary,
        "aoem_concurrency_owner",
        "AOEM_runtime",
        &mut violations,
    );
    require_eq_str(
        &write_summary,
        "host_concurrency_policy",
        "host_drives_lifecycle_only_no_rust_execution_scheduler",
        &mut violations,
    );
    require_min(
        &write_summary,
        "product_ingress_submitted_total",
        tx_count,
        &mut violations,
    );
    require_min(
        &write_summary,
        "aoem_executed_total",
        tx_count,
        &mut violations,
    );
    require_min(
        &write_summary,
        "included_canonical_total",
        tx_count,
        &mut violations,
    );
    require_max(&write_summary, "queue_pending_last", 0, &mut violations);
    require_max(&write_summary, "queue_dropped_last", 0, &mut violations);
    require_max(&write_summary, "queue_rejected_last", 0, &mut violations);
    for (field, expected) in [
        ("recovery_ok", true),
        ("semantic_head_current_recovered", true),
        ("semantic_head_by_height_recovered", true),
        ("snapshot_meta_current_recovered", true),
        ("snapshot_meta_by_height_recovered", true),
        ("receipt_index_recovered", true),
        ("materialized_view_rebuilt", true),
    ] {
        if recovery_probe.get(field).and_then(Value::as_bool) != Some(expected) {
            violations.push(format!("{field} is not {expected}"));
        }
    }
    if recovery_probe
        .get("receipt_count")
        .and_then(Value::as_u64)
        .unwrap_or_default()
        < tx_count
    {
        violations.push(format!(
            "receipt_count={} below tx_count={tx_count}",
            recovery_probe
                .get("receipt_count")
                .and_then(Value::as_u64)
                .unwrap_or_default()
        ));
    }
    require_eq_str(
        &restart_summary,
        "execution_kernel",
        "AOEM",
        &mut violations,
    );
    require_eq_str(
        &restart_summary,
        "aoem_concurrency_owner",
        "AOEM_runtime",
        &mut violations,
    );
    require_eq_str(
        &restart_summary,
        "host_concurrency_policy",
        "host_drives_lifecycle_only_no_rust_execution_scheduler",
        &mut violations,
    );
    require_max(&restart_summary, "aoem_executed_total", 0, &mut violations);
    require_max(
        &restart_summary,
        "included_canonical_total",
        0,
        &mut violations,
    );
    require_max(&restart_summary, "queue_pending_last", 0, &mut violations);

    let duplicate_canonical_after_restart =
        summary_u64(&restart_summary, "included_canonical_total");
    let pass = violations.is_empty();
    let rocksdb_path = nov_native_execution_store_rocksdb_path_v1(store_path.as_path());
    let report = serde_json::json!({
        "schema": REPORT_SCHEMA_V1,
        "method": "supervm_native_pipeline_rocksdb_recovery_gate",
        "accepted": pass,
        "chain_id": chain_id,
        "tx_count": tx_count,
        "batch_budget": batch_budget,
        "store_path": store_path,
        "rocksdb_path": rocksdb_path,
        "boundaries": {
            "lifecycle_structure": "frozen",
            "execution_kernel": "AOEM",
            "aoem_concurrency_owner": "AOEM_runtime",
            "host_concurrency_policy": "host_drives_lifecycle_only_no_rust_execution_scheduler",
            "product_entry": "pending_only",
            "receipt_state_source": "AOEM_tick_lifecycle",
            "commit": "dirty_sharded_atomic_commit",
        },
        "recovery": {
            "recovery_ok": recovery_probe["recovery_ok"].clone(),
            "semantic_head_recovered": recovery_probe["semantic_head_current_recovered"].clone(),
            "semantic_head_by_height_recovered": recovery_probe["semantic_head_by_height_recovered"].clone(),
            "snapshot_meta_recovered": recovery_probe["snapshot_meta_current_recovered"].clone(),
            "snapshot_meta_by_height_recovered": recovery_probe["snapshot_meta_by_height_recovered"].clone(),
            "receipt_index_recovered": recovery_probe["receipt_index_recovered"].clone(),
            "materialized_view_rebuilt": recovery_probe["materialized_view_rebuilt"].clone(),
            "duplicate_canonical_after_restart": duplicate_canonical_after_restart,
            "canonical_body_head_recovery": recovery_probe["canonical_body_head_recovery"].clone(),
        },
        "violations": violations,
        "write_summary": write_summary,
        "recovery_probe": recovery_probe,
        "restart_summary": restart_summary,
    });
    let path = report_path();
    write_report(path.as_path(), &report)?;
    println!(
        "{}",
        serde_json::to_string_pretty(&report).context("encode recovery report failed")?
    );
    if !pass {
        bail!(
            "native pipeline rocksdb recovery gate failed: {}",
            path.display()
        );
    }
    Ok(())
}
