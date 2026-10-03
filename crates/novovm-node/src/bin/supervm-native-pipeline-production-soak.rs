#![forbid(unsafe_code)]

use anyhow::{bail, Context, Result};
use serde_json::Value;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};
use std::time::{SystemTime, UNIX_EPOCH};

const REPORT_SCHEMA_V1: &str = "novovm-native-pipeline-production-soak-report/v1";

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

fn bool_env(name: &str, default: bool) -> bool {
    string_env_nonempty(name)
        .map(|raw| {
            raw == "1"
                || raw.eq_ignore_ascii_case("true")
                || raw.eq_ignore_ascii_case("yes")
                || raw.eq_ignore_ascii_case("on")
        })
        .unwrap_or(default)
}

fn div_ceil_u64(value: u64, divisor: u64) -> u64 {
    if divisor == 0 {
        return 0;
    }
    value.saturating_add(divisor).saturating_sub(1) / divisor
}

fn unix_ms_now() -> u128 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_millis())
        .unwrap_or_default()
}

fn profile_default_duration_seconds(profile: &str) -> u64 {
    match profile.to_ascii_lowercase().as_str() {
        "2h" | "2hr" | "2hour" | "2hours" => 7_200,
        "overnight" | "8h" | "8hr" | "8hour" | "8hours" => 28_800,
        _ => 1_800,
    }
}

fn normalized_profile(profile: &str) -> String {
    let value = profile.trim().to_ascii_lowercase();
    if value.is_empty() {
        return "30min".to_string();
    }
    value
        .chars()
        .map(|ch| {
            if ch.is_ascii_alphanumeric() || ch == '-' {
                ch
            } else {
                '-'
            }
        })
        .collect()
}

fn default_report_path(profile: &str) -> PathBuf {
    PathBuf::from(format!(
        "artifacts/native-pipeline/native-pipeline-production-soak-{}.json",
        normalized_profile(profile)
    ))
}

fn report_path(profile: &str) -> PathBuf {
    string_env_nonempty("NOVOVM_NATIVE_PIPELINE_PRODUCTION_SOAK_REPORT_PATH")
        .map(PathBuf::from)
        .unwrap_or_else(|| default_report_path(profile))
}

fn temp_store_path(profile: &str, chain_id: u64) -> PathBuf {
    std::env::temp_dir().join(format!(
        "novovm-native-pipeline-production-soak-{}-{chain_id}-{}-{}.json",
        normalized_profile(profile),
        std::process::id(),
        unix_ms_now()
    ))
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
            "run novovm-node production soak child failed: {}",
            bin.display()
        )
    })
}

fn parse_summary(output: &Output) -> Result<Value> {
    if !output.status.success() {
        bail!(
            "production soak child failed: status={} stderr={}",
            output.status,
            String::from_utf8_lossy(&output.stderr)
        );
    }
    serde_json::from_slice::<Value>(&output.stdout).with_context(|| {
        format!(
            "production soak child did not return JSON summary: stdout={} stderr={}",
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

fn validate_summary(
    summary: &Value,
    expected_ticks: u64,
    tx_count: u64,
    batch_budget: u64,
    allow_dropped: u64,
    allow_rejected: u64,
) -> Vec<String> {
    let mut violations = Vec::new();
    require_eq_str(summary, "execution_kernel", "AOEM", &mut violations);
    require_eq_str(
        summary,
        "aoem_concurrency_owner",
        "AOEM_runtime",
        &mut violations,
    );
    require_eq_str(
        summary,
        "host_concurrency_policy",
        "host_drives_lifecycle_only_no_rust_execution_scheduler",
        &mut violations,
    );
    require_eq_str(
        summary,
        "tx_ingress_selected_path",
        "aoem_runtime_owned_state_persistence",
        &mut violations,
    );
    require_eq_str(
        summary,
        "tx_ingress_production_target",
        "aoem_runtime_owned_state_persistence",
        &mut violations,
    );
    for field in [
        "tx_ingress_aoem_gate_config_production_candidate",
        "aoem_owned_single_path_enforced",
        "aoem_native_tx_batch_production_candidate_result_ok",
        "aoem_owned_regression_signable",
    ] {
        if summary.get(field).and_then(Value::as_bool) != Some(true) {
            violations.push(format!("{field} is not true"));
        }
    }
    for field in [
        "legacy_host_transitional_fallback_used",
        "aoem_native_tx_batch_production_fallback_used",
        "aoem_native_tx_batch_production_double_write_legacy_canonical",
    ] {
        if summary.get(field).and_then(Value::as_bool) != Some(false) {
            violations.push(format!("{field} is not false"));
        }
    }
    require_min(
        summary,
        "aoem_native_tx_batch_production_receipt_count",
        tx_count,
        &mut violations,
    );
    require_min(summary, "ticks", expected_ticks, &mut violations);
    require_min(
        summary,
        "ingress_submitted_total",
        tx_count,
        &mut violations,
    );
    require_min(
        summary,
        "product_ingress_submitted_total",
        tx_count,
        &mut violations,
    );
    require_min(summary, "aoem_executed_total", tx_count, &mut violations);
    require_min(
        summary,
        "included_canonical_total",
        tx_count,
        &mut violations,
    );
    require_min(
        summary,
        "max_product_ingress_submitted_per_tick",
        batch_budget,
        &mut violations,
    );
    require_min(
        summary,
        "max_queue_admitted_per_tick",
        batch_budget,
        &mut violations,
    );
    require_min(
        summary,
        "max_aoem_batch_executed_per_tick",
        batch_budget,
        &mut violations,
    );
    require_min(
        summary,
        "max_proof_items_per_tick",
        batch_budget,
        &mut violations,
    );
    require_min(
        summary,
        "max_commit_items_per_tick",
        batch_budget,
        &mut violations,
    );
    require_min(
        summary,
        "max_broadcast_tx_per_tick",
        batch_budget,
        &mut violations,
    );
    require_max(summary, "queue_pending_last", 0, &mut violations);
    require_max(
        summary,
        "queue_dropped_last",
        allow_dropped,
        &mut violations,
    );
    require_max(
        summary,
        "queue_rejected_last",
        allow_rejected,
        &mut violations,
    );
    if summary_str(summary, "native_store_commit_model").contains("dirty_sharded_atomic_batch") {
        // ok
    } else {
        violations.push(format!(
            "native_store_commit_model={} missing dirty_sharded_atomic_batch",
            summary_str(summary, "native_store_commit_model")
        ));
    }
    if summary
        .get("native_store_rocksdb_enabled")
        .and_then(Value::as_bool)
        != Some(true)
    {
        violations.push("native_store_rocksdb_enabled is not true".to_string());
    }
    if summary
        .get("native_store_transactional_commit")
        .and_then(Value::as_bool)
        != Some(true)
    {
        violations.push("native_store_transactional_commit is not true".to_string());
    }
    violations
}

fn write_report(path: &Path, report: &Value) -> Result<()> {
    if let Some(parent) = path.parent() {
        if !parent.as_os_str().is_empty() {
            fs::create_dir_all(parent).with_context(|| {
                format!("create production soak report dir: {}", parent.display())
            })?;
        }
    }
    let encoded =
        serde_json::to_string_pretty(report).context("encode production soak report failed")?;
    fs::write(path, encoded)
        .with_context(|| format!("write production soak report failed: {}", path.display()))
}

fn main() -> Result<()> {
    let profile = string_env_nonempty("NOVOVM_NATIVE_PIPELINE_PRODUCTION_SOAK_PROFILE")
        .unwrap_or_else(|| "30min".to_string());
    let duration_seconds = u64_env(
        "NOVOVM_NATIVE_PIPELINE_PRODUCTION_SOAK_DURATION_SECONDS",
        profile_default_duration_seconds(profile.as_str()),
    )?
    .max(1);
    let interval_ms = u64_env("NOVOVM_NATIVE_PIPELINE_PRODUCTION_SOAK_TICK_INTERVAL_MS", 5)?.max(1);
    let batch_budget = u64_env("NOVOVM_NATIVE_PIPELINE_PRODUCTION_SOAK_BATCH_BUDGET", 32)?.max(1);
    let chain_id = u64_env("NOVOVM_NATIVE_PIPELINE_PRODUCTION_SOAK_CHAIN_ID", 9_998_898)?;
    let tx_count = u64_env(
        "NOVOVM_NATIVE_PIPELINE_PRODUCTION_SOAK_TX_COUNT",
        batch_budget.saturating_mul(8),
    )?
    .max(batch_budget);
    let max_ticks = u64_env(
        "NOVOVM_NATIVE_PIPELINE_PRODUCTION_SOAK_MAX_TICKS",
        div_ceil_u64(duration_seconds.saturating_mul(1_000), interval_ms),
    )?
    .max(1);
    let expected_ticks = u64_env(
        "NOVOVM_NATIVE_PIPELINE_PRODUCTION_SOAK_MIN_TICKS",
        max_ticks,
    )?;
    let store_backend = string_env_nonempty("NOVOVM_NATIVE_PIPELINE_PRODUCTION_SOAK_STORE_BACKEND")
        .unwrap_or_else(|| "dual".to_string());
    let store_path = string_env_nonempty("NOVOVM_NATIVE_PIPELINE_PRODUCTION_SOAK_STORE_PATH")
        .map(PathBuf::from)
        .unwrap_or_else(|| temp_store_path(profile.as_str(), chain_id));
    let aoem_persistence_path = store_path.with_extension("aoem-persistence");
    let aoem_owned_state_db_path = store_path.with_extension("aoem-owned.rocksdb");
    let child_summary_path =
        string_env_nonempty("NOVOVM_NATIVE_PIPELINE_PRODUCTION_SOAK_CHILD_SUMMARY_PATH")
            .map(PathBuf::from)
            .unwrap_or_else(|| {
                PathBuf::from(format!(
                    "artifacts/native-pipeline/native-pipeline-production-soak-{}-summary.json",
                    normalized_profile(profile.as_str())
                ))
            });
    let allow_dropped = u64_env(
        "NOVOVM_NATIVE_PIPELINE_PRODUCTION_SOAK_MAX_QUEUE_DROPPED",
        0,
    )?;
    let allow_rejected = u64_env(
        "NOVOVM_NATIVE_PIPELINE_PRODUCTION_SOAK_MAX_QUEUE_REJECTED",
        0,
    )?;
    let node_bin = novovm_node_bin();
    if !node_bin.exists() {
        bail!(
            "novovm-node binary not found: {}; build with `cargo build -p novovm-node --bins` or set NOVOVM_NATIVE_PIPELINE_NODE_BIN",
            node_bin.display()
        );
    }

    let started_at_ms = unix_ms_now();
    let envs = vec![
        ("NOVOVM_NODE_MODE", "native_execution_pipeline".to_string()),
        (
            "NOVOVM_NATIVE_EXECUTION_TICK_CHAIN_ID",
            chain_id.to_string(),
        ),
        (
            "NOVOVM_NATIVE_EXECUTION_TICK_MAX_TICKS",
            max_ticks.to_string(),
        ),
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
            interval_ms.to_string(),
        ),
        (
            "NOVOVM_NATIVE_EXECUTION_STORE",
            store_path.display().to_string(),
        ),
        (
            "NOVOVM_NATIVE_EXECUTION_STORE_BACKEND",
            store_backend.clone(),
        ),
        (
            "NOVOVM_NATIVE_SEND_RAW_TRANSACTION_PIPELINE_ONLY",
            "true".to_string(),
        ),
        (
            "NOVOVM_NATIVE_EXECUTION_PIPELINE_INGRESS_FIXTURE_TX_COUNT",
            tx_count.to_string(),
        ),
        (
            "NOVOVM_NATIVE_EXECUTION_PIPELINE_INGRESS_MAX_PER_TICK",
            batch_budget.to_string(),
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
            expected_ticks.to_string(),
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
            div_ceil_u64(tx_count, batch_budget).to_string(),
        ),
        (
            "NOVOVM_NATIVE_EXECUTION_PIPELINE_MIN_NONEMPTY_PROOF_TICKS",
            div_ceil_u64(tx_count, batch_budget).to_string(),
        ),
        (
            "NOVOVM_NATIVE_EXECUTION_PIPELINE_MIN_NONEMPTY_COMMIT_TICKS",
            div_ceil_u64(tx_count, batch_budget).to_string(),
        ),
        (
            "NOVOVM_NATIVE_EXECUTION_PIPELINE_MIN_PROOF_TICKS",
            expected_ticks.to_string(),
        ),
        (
            "NOVOVM_NATIVE_EXECUTION_PIPELINE_MIN_COMMIT_TICKS",
            expected_ticks.to_string(),
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
            div_ceil_u64(tx_count, batch_budget).to_string(),
        ),
        (
            "NOVOVM_NATIVE_EXECUTION_PIPELINE_MAX_QUEUE_PENDING_LAST",
            "0".to_string(),
        ),
        (
            "NOVOVM_NATIVE_EXECUTION_PIPELINE_QUIET_TICKS",
            "true".to_string(),
        ),
        (
            "NOVOVM_NATIVE_EXECUTION_PIPELINE_SUMMARY_REPORT_PATH",
            child_summary_path.display().to_string(),
        ),
        ("NOVOVM_AOEM_VARIANT", "core".to_string()),
        ("NOVOVM_AOEM_PERSIST_BACKEND", "rocksdb".to_string()),
        (
            "AOEM_PERSISTENCE_PATH",
            aoem_persistence_path.display().to_string(),
        ),
        (
            "NOVOVM_AOEM_OWNED_STATE_DB_PATH",
            aoem_owned_state_db_path.display().to_string(),
        ),
        (
            "NOVOVM_AOEM_STATE_NAMESPACE",
            format!("production-soak-{profile}-chain-{chain_id}"),
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
            "NOVOVM_AOEM_NATIVE_TX_BATCH_PRODUCTION_CANDIDATE",
            "true".to_string(),
        ),
        ("NOVOVM_AOEM_NATIVE_TX_BATCH_COMPARE", "true".to_string()),
        ("NOVOVM_AOEM_NATIVE_TX_BATCH_SHADOW", "false".to_string()),
        (
            "NOVOVM_LEGACY_HOST_TRANSITIONAL_FALLBACK",
            "false".to_string(),
        ),
    ];
    let output = run_node(node_bin.as_path(), envs.as_slice())?;
    let child_summary = parse_summary(&output)?;
    let finished_at_ms = unix_ms_now();
    let violations = validate_summary(
        &child_summary,
        expected_ticks,
        tx_count,
        batch_budget,
        allow_dropped,
        allow_rejected,
    );
    let pass = violations.is_empty();
    let report = serde_json::json!({
        "schema": REPORT_SCHEMA_V1,
        "method": "supervm_native_pipeline_production_soak",
        "accepted": pass,
        "profile": profile,
        "duration_seconds": duration_seconds,
        "configured": {
            "chain_id": chain_id,
            "max_ticks": max_ticks,
            "expected_ticks": expected_ticks,
            "tick_interval_ms": interval_ms,
            "tx_count": tx_count,
            "batch_budget": batch_budget,
            "store_backend": store_backend,
            "store_path": store_path,
            "child_summary_path": child_summary_path,
            "node_bin": node_bin,
        },
        "boundaries": {
            "lifecycle_structure": "frozen",
            "execution_kernel": "AOEM",
            "aoem_concurrency_owner": "AOEM_runtime",
            "host_concurrency_policy": "host_drives_lifecycle_only_no_rust_execution_scheduler",
            "product_entry": "pending_only",
            "receipt_state_source": "AOEM_semantic_graph_v3",
            "state_receipt_owner": "AOEM_runtime",
            "host_store_role": "validated_query_and_lifecycle_projection",
            "commit": "aoem_submit_semantic_graph_v3_atomic_persistence",
            "legacy_host_transitional_fallback": false,
            "legacy_canonical_double_write": false,
            "feature_expansion": "stopped"
        },
        "budgets": {
            "max_queue_dropped": allow_dropped,
            "max_queue_rejected": allow_rejected
        },
        "started_at_unix_ms": started_at_ms,
        "finished_at_unix_ms": finished_at_ms,
        "elapsed_ms": finished_at_ms.saturating_sub(started_at_ms),
        "pass": pass,
        "violations": violations,
        "summary": child_summary,
    });
    let path = report_path(profile.as_str());
    write_report(path.as_path(), &report)?;
    println!(
        "{}",
        serde_json::to_string_pretty(&report).context("encode production soak report failed")?
    );
    if !pass && !bool_env("NOVOVM_NATIVE_PIPELINE_PRODUCTION_SOAK_ALLOW_FAIL", false) {
        bail!("native pipeline production soak failed: {}", path.display());
    }
    Ok(())
}
