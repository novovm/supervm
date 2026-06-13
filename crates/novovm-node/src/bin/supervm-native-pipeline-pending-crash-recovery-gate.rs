#![forbid(unsafe_code)]

use anyhow::{bail, Context, Result};
use novovm_network::{
    observe_network_runtime_native_pending_tx_local_native_payload_v1,
    snapshot_network_runtime_native_pending_tx_summary_v1,
};
use novovm_node::tx_ingress::{
    get_nov_native_execution_store_recovery_probe_v1, nov_native_tx_to_adapter_tx_ir_v1,
};
use novovm_protocol::{
    encode_nov_native_tx_wire_v1, NovExecuteTxV1, NovExecutionModeV1, NovExecutionPolicyV1,
    NovExecutionTargetV1, NovFeePolicyV1, NovNativeTxWireV1, NovPrivacyModeV1, NovTxKindV1,
    NovVerificationModeV1,
};
use serde_json::Value;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};
use std::time::{SystemTime, UNIX_EPOCH};

const REPORT_SCHEMA_V1: &str = "novovm-native-pipeline-pending-crash-recovery-report/v1";
const PENDING_POLICY_V1: &str = "volatile";

#[derive(Debug, Clone)]
struct NativeFixtureTxV1 {
    tx_hash: [u8; 32],
    payload: Vec<u8>,
}

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

fn temp_store_path(chain_id: u64) -> PathBuf {
    std::env::temp_dir().join(format!(
        "novovm-native-pipeline-pending-crash-{chain_id}-{}-{}.json",
        std::process::id(),
        unix_ms_now()
    ))
}

fn default_report_path() -> PathBuf {
    PathBuf::from("artifacts/native-pipeline/native-pipeline-pending-crash-recovery-report.json")
}

fn report_path() -> PathBuf {
    string_env_nonempty("NOVOVM_NATIVE_PIPELINE_PENDING_CRASH_REPORT_PATH")
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

fn build_native_payloads(
    chain_id: u64,
    count: u64,
    account_prefix: &str,
) -> Result<Vec<NativeFixtureTxV1>> {
    let mut out = Vec::with_capacity(count as usize);
    for index in 0..count {
        let nonce = index.saturating_add(1);
        let account_id = format!("{account_prefix}-{nonce}");
        let tx = NovNativeTxWireV1 {
            chain_id,
            kind: NovTxKindV1::Execute(NovExecuteTxV1 {
                caller: vec![(nonce & 0xff) as u8; 20],
                account_id: Some(account_id.clone()),
                fee_owner_account_id: Some(account_id.clone()),
                nonce_owner_account_id: Some(account_id),
                target: NovExecutionTargetV1::NativeModule("treasury".to_string()),
                method: "deposit_reserve".to_string(),
                args: serde_json::to_vec(&serde_json::json!({
                    "asset": "USDT",
                    "amount": nonce,
                }))
                .context("encode pending crash fixture args failed")?,
                execution_mode: NovExecutionModeV1::Batch,
                execution_policy: NovExecutionPolicyV1::Standard,
                privacy_mode: NovPrivacyModeV1::Public,
                verification_mode: NovVerificationModeV1::Standard,
                fee_policy: NovFeePolicyV1 {
                    pay_asset: "USDT".to_string(),
                    max_pay_amount: 10_000,
                    slippage_bps: 100,
                },
                gas_like_limit: Some(90_000),
                nonce,
            }),
            signature: [(nonce & 0xff) as u8; 32],
        };
        let ir = nov_native_tx_to_adapter_tx_ir_v1(&tx)?;
        let mut tx_hash = [0u8; 32];
        let copy_len = ir.hash.len().min(32);
        tx_hash[..copy_len].copy_from_slice(&ir.hash[..copy_len]);
        let payload = encode_nov_native_tx_wire_v1(&tx)
            .map_err(|err| anyhow::anyhow!("encode pending crash native tx failed: {err}"))?;
        out.push(NativeFixtureTxV1 { tx_hash, payload });
    }
    Ok(out)
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
            "run novovm-node pending crash child failed: {}",
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

fn write_report(path: &Path, report: &Value) -> Result<()> {
    if let Some(parent) = path.parent() {
        if !parent.as_os_str().is_empty() {
            fs::create_dir_all(parent).with_context(|| {
                format!("create pending crash report dir: {}", parent.display())
            })?;
        }
    }
    let encoded =
        serde_json::to_string_pretty(report).context("encode pending crash report failed")?;
    fs::write(path, encoded)
        .with_context(|| format!("write pending crash report failed: {}", path.display()))
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

fn main() -> Result<()> {
    let chain_id = u64_env("NOVOVM_NATIVE_PIPELINE_PENDING_CRASH_CHAIN_ID", 9_998_901)?;
    let pending_tx_count =
        u64_env("NOVOVM_NATIVE_PIPELINE_PENDING_CRASH_PENDING_TX_COUNT", 16)?.max(1);
    let partial_tx_count =
        u64_env("NOVOVM_NATIVE_PIPELINE_PENDING_CRASH_PARTIAL_TX_COUNT", 32)?.max(2);
    let batch_budget = u64_env("NOVOVM_NATIVE_PIPELINE_PENDING_CRASH_BATCH_BUDGET", 8)?.max(1);
    let tick_interval_ms =
        u64_env("NOVOVM_NATIVE_PIPELINE_PENDING_CRASH_TICK_INTERVAL_MS", 5)?.max(1);
    let partial_included = partial_tx_count.min(batch_budget);
    let partial_store = temp_store_path(chain_id);
    let node_bin = novovm_node_bin();
    if !node_bin.exists() {
        bail!(
            "novovm-node binary not found: {}; build with `cargo build -p novovm-node --bins` or set NOVOVM_NATIVE_PIPELINE_NODE_BIN",
            node_bin.display()
        );
    }

    let pending_payloads = build_native_payloads(
        chain_id,
        pending_tx_count,
        "acct-native-pending-crash-volatile",
    )?;
    for tx in &pending_payloads {
        observe_network_runtime_native_pending_tx_local_native_payload_v1(
            chain_id,
            tx.tx_hash,
            Some(tx.payload.as_slice()),
        );
    }
    let pending_before_crash = snapshot_network_runtime_native_pending_tx_summary_v1(chain_id);
    let crash_before_aoem_tick = serde_json::json!({
        "scenario": "crash_before_aoem_tick",
        "pending_policy": PENDING_POLICY_V1,
        "pending_submitted_count": pending_tx_count,
        "pending_before_crash_count": pending_before_crash.pending_count as u64,
        "persistent_pending_queue_supported": false,
        "volatile_pending_not_recovered": true,
        "pending_lost_count": pending_tx_count,
        "reason": "network_runtime_native_pending queue is process-local volatile state; only AOEM/dirty-committed state is recovered from RocksDB",
    });

    let mut first_env = base_env(
        chain_id,
        partial_store.as_path(),
        1,
        tick_interval_ms,
        batch_budget,
    );
    first_env.extend([
        (
            "NOVOVM_NATIVE_EXECUTION_PIPELINE_INGRESS_FIXTURE_TX_COUNT",
            partial_tx_count.to_string(),
        ),
        (
            "NOVOVM_NATIVE_EXECUTION_PIPELINE_INGRESS_MAX_PER_TICK",
            partial_tx_count.to_string(),
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
            "NOVOVM_NATIVE_EXECUTION_PIPELINE_REQUIRE_PRODUCT_INGRESS",
            "true".to_string(),
        ),
        (
            "NOVOVM_NATIVE_EXECUTION_PIPELINE_REQUIRE_ROCKSDB_STORE",
            "true".to_string(),
        ),
        (
            "NOVOVM_NATIVE_EXECUTION_PIPELINE_MIN_INGRESS_SUBMITTED",
            partial_tx_count.to_string(),
        ),
        (
            "NOVOVM_NATIVE_EXECUTION_PIPELINE_MIN_AOEM_EXECUTED",
            partial_included.to_string(),
        ),
        (
            "NOVOVM_NATIVE_EXECUTION_PIPELINE_MIN_INCLUDED_CANONICAL_TOTAL",
            partial_included.to_string(),
        ),
    ]);
    let first_summary = parse_summary(
        &run_node(node_bin.as_path(), first_env.as_slice())?,
        "partial_first",
    )?;

    std::env::set_var("NOVOVM_NATIVE_EXECUTION_STORE_BACKEND", "rocksdb");
    let first_probe = get_nov_native_execution_store_recovery_probe_v1(partial_store.as_path())?;

    let restart_env = base_env(
        chain_id,
        partial_store.as_path(),
        3,
        tick_interval_ms,
        batch_budget,
    );
    let restart_summary = parse_summary(
        &run_node(node_bin.as_path(), restart_env.as_slice())?,
        "partial_restart",
    )?;
    let restart_probe = get_nov_native_execution_store_recovery_probe_v1(partial_store.as_path())?;

    let canonical_before_restart = summary_u64(&first_summary, "included_canonical_total");
    let duplicate_canonical_after_restart =
        summary_u64(&restart_summary, "included_canonical_total");
    let duplicate_receipt_after_restart = restart_probe
        .get("receipt_count")
        .and_then(Value::as_u64)
        .unwrap_or_default()
        .saturating_sub(
            first_probe
                .get("receipt_count")
                .and_then(Value::as_u64)
                .unwrap_or_default(),
        );
    let pending_lost_after_partial = partial_tx_count.saturating_sub(canonical_before_restart);
    let semantic_head_monotonic_after_restart = restart_probe
        .get("semantic_head_current_recovered")
        .and_then(Value::as_bool)
        == Some(true)
        && restart_probe
            .get("semantic_head_by_height_recovered")
            .and_then(Value::as_bool)
            == Some(true)
        && restart_probe
            .get("semantic_head")
            .and_then(|value| value.get("sequence"))
            .and_then(Value::as_u64)
            .unwrap_or_default()
            >= canonical_before_restart;
    let receipt_index_consistent_after_restart = restart_probe
        .get("receipt_index_recovered")
        .and_then(Value::as_bool)
        == Some(true)
        && restart_probe
            .get("receipt_count")
            .and_then(Value::as_u64)
            .unwrap_or_default()
            == canonical_before_restart;

    let mut violations = Vec::<String>::new();
    if summary_str(&first_summary, "execution_kernel") != "AOEM" {
        violations.push("first_summary execution_kernel is not AOEM".to_string());
    }
    if summary_str(&first_summary, "aoem_concurrency_owner") != "AOEM_runtime" {
        violations.push("first_summary aoem_concurrency_owner is not AOEM_runtime".to_string());
    }
    if summary_str(&first_summary, "host_concurrency_policy")
        != "host_drives_lifecycle_only_no_rust_execution_scheduler"
    {
        violations.push("first_summary host concurrency policy drifted".to_string());
    }
    if summary_str(&restart_summary, "execution_kernel") != "AOEM" {
        violations.push("restart_summary execution_kernel is not AOEM".to_string());
    }
    if summary_u64(&restart_summary, "aoem_executed_total") != 0 {
        violations.push(format!(
            "restart aoem_executed_total={} expected 0 for volatile pending",
            summary_u64(&restart_summary, "aoem_executed_total")
        ));
    }
    if duplicate_canonical_after_restart != 0 {
        violations.push(format!(
            "duplicate_canonical_after_restart={duplicate_canonical_after_restart} expected 0"
        ));
    }
    if duplicate_receipt_after_restart != 0 {
        violations.push(format!(
            "duplicate_receipt_after_restart={duplicate_receipt_after_restart} expected 0"
        ));
    }
    if !semantic_head_monotonic_after_restart {
        violations.push("semantic_head_monotonic_after_restart=false".to_string());
    }
    if !receipt_index_consistent_after_restart {
        violations.push("receipt_index_consistent_after_restart=false".to_string());
    }
    if pending_lost_after_partial != partial_tx_count.saturating_sub(partial_included) {
        violations.push(format!(
            "pending_lost_after_partial={pending_lost_after_partial} expected {}",
            partial_tx_count.saturating_sub(partial_included)
        ));
    }

    let accepted = violations.is_empty();
    let report = serde_json::json!({
        "schema": REPORT_SCHEMA_V1,
        "method": "supervm_native_pipeline_pending_crash_recovery_gate",
        "accepted": accepted,
        "chain_id": chain_id,
        "pending_policy": PENDING_POLICY_V1,
        "boundaries": {
            "lifecycle_structure": "frozen",
            "execution_kernel": "AOEM",
            "aoem_concurrency_owner": "AOEM_runtime",
            "host_concurrency_policy": "host_drives_lifecycle_only_no_rust_execution_scheduler",
            "product_entry": "pending_only",
            "receipt_state_source": "AOEM_tick_lifecycle",
            "commit": "dirty_sharded_atomic_commit",
            "canonical_body_head_recovery": "not_claimed_by_this_gate"
        },
        "crash_before_aoem_tick": crash_before_aoem_tick,
        "crash_after_partial_commit": {
            "scenario": "crash_after_partial_commit",
            "submitted": partial_tx_count,
            "canonical_before_restart": canonical_before_restart,
            "pending_policy": PENDING_POLICY_V1,
            "pending_lost_count": pending_lost_after_partial,
            "volatile_pending_not_recovered": true,
            "canonical_after_restart": canonical_before_restart,
            "duplicate_canonical_after_restart": duplicate_canonical_after_restart,
            "duplicate_receipt_after_restart": duplicate_receipt_after_restart,
            "semantic_head_monotonic_after_restart": semantic_head_monotonic_after_restart,
            "receipt_index_consistent_after_restart": receipt_index_consistent_after_restart
        },
        "first_summary": first_summary,
        "first_recovery_probe": first_probe,
        "restart_summary": restart_summary,
        "restart_recovery_probe": restart_probe,
        "violations": violations
    });
    let path = report_path();
    write_report(path.as_path(), &report)?;
    println!(
        "{}",
        serde_json::to_string_pretty(&report).context("encode pending crash report failed")?
    );
    if !accepted {
        bail!(
            "native pipeline pending crash recovery gate failed: {}",
            path.display()
        );
    }
    Ok(())
}
