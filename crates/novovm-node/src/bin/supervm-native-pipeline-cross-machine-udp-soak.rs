#![forbid(unsafe_code)]
#![recursion_limit = "256"]

use anyhow::{bail, Context, Result};
use novovm_network::{Transport, UdpTransport};
use novovm_node::tx_ingress::{
    get_nov_native_execution_store_recovery_probe_v1,
    get_nov_native_execution_store_rocksdb_memory_probe_v1,
    nov_native_execution_store_rocksdb_path_v1, nov_native_tx_to_adapter_tx_ir_v1,
};
use novovm_protocol::{
    encode_nov_native_tx_wire_v1, EvmNativeMessage, NodeId, NovExecuteTxV1, NovExecutionModeV1,
    NovExecutionPolicyV1, NovExecutionTargetV1, NovFeePolicyV1, NovNativeTxWireV1,
    NovPrivacyModeV1, NovTxKindV1, NovVerificationModeV1, ProtocolMessage,
};
use serde_json::Value;
use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::io::BufRead;
use std::net::UdpSocket;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Output, Stdio};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

const REPORT_SCHEMA_V1: &str = "novovm-native-pipeline-cross-machine-udp-soak-report/v1";

#[derive(Debug, Clone)]
struct NativeFixtureTxV1 {
    index: u64,
    copy_index: u64,
    tx_hash: [u8; 32],
    payload: Vec<u8>,
    dropped: bool,
}

#[derive(Debug, Clone, Copy)]
struct FaultConfigV1 {
    enabled: bool,
    loss_bps: u64,
    duplicate_bps: u64,
    delay_ms: u64,
    reorder_bps: u64,
    seed: u64,
}

#[derive(Debug, Clone)]
struct SendScheduleStatsV1 {
    scheduled_packets: u64,
    sent_packets: u64,
    dropped_packets: u64,
    duplicated_packets: u64,
    delayed_packets: u64,
    reordered_packets: u64,
    sent_unique: u64,
    sent_by_hash: BTreeMap<String, u64>,
}

#[derive(Debug, Clone, Copy)]
struct SustainedConfigV1 {
    enabled: bool,
    duration_seconds: u64,
    tx_per_round: u64,
    round_interval_ms: u64,
}

#[derive(Debug, Clone, Copy)]
struct TailRepairConfigV1 {
    enabled: bool,
    rounds: u64,
    interval_ms: u64,
}

#[derive(Debug, Clone)]
struct ReceiverDiagnosticsConfigV1 {
    enabled: bool,
    sample_interval_ms: u64,
    stall_windows: u64,
    memory_sample_enabled: bool,
    max_working_set_bytes: u64,
    min_canonical_delta: u64,
    max_elapsed_ms: u64,
    report_path: PathBuf,
}

#[derive(Debug, Clone, Default)]
struct ReceiverDiagnosticsStateV1 {
    samples: Vec<Value>,
    last_canonical: u64,
    stall_windows: u64,
    fail_reason: Option<String>,
    samples_dropped: u64,
    first_working_set_bytes: Option<u64>,
    last_working_set_bytes: Option<u64>,
}

fn string_env_nonempty(name: &str) -> Option<String> {
    std::env::var(name)
        .ok()
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
}

fn first_string_env_nonempty(names: &[&str]) -> Option<String> {
    names.iter().find_map(|name| string_env_nonempty(name))
}

fn u64_env(name: &str, default: u64) -> Result<u64> {
    let Some(raw) = string_env_nonempty(name) else {
        return Ok(default);
    };
    raw.parse::<u64>()
        .with_context(|| format!("{name} must be u64"))
}

fn u64_env_alias(names: &[&str], default: u64) -> Result<u64> {
    for name in names {
        if let Some(raw) = string_env_nonempty(name) {
            return raw
                .parse::<u64>()
                .with_context(|| format!("{name} must be u64"));
        }
    }
    Ok(default)
}

fn env_any(names: &[&str]) -> bool {
    names.iter().any(|name| string_env_nonempty(name).is_some())
}

fn bool_env(name: &str) -> bool {
    string_env_nonempty(name)
        .map(|value| {
            let lower = value.to_ascii_lowercase();
            lower == "1" || lower == "true" || lower == "yes" || lower == "on"
        })
        .unwrap_or(false)
}

fn current_bin_name_contains(pattern: &str) -> bool {
    std::env::current_exe()
        .ok()
        .and_then(|path| {
            path.file_name()
                .map(|name| name.to_string_lossy().to_string())
        })
        .map(|name| name.to_ascii_lowercase().contains(pattern))
        .unwrap_or(false)
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

fn reserve_udp_addr() -> Result<String> {
    let socket = UdpSocket::bind("127.0.0.1:0").context("reserve udp addr failed")?;
    Ok(socket
        .local_addr()
        .context("read reserved udp addr failed")?
        .to_string())
}

fn temp_store_path(chain_id: u64, role: &str) -> PathBuf {
    std::env::temp_dir().join(format!(
        "novovm-native-pipeline-cross-machine-{role}-{chain_id}-{}-{}.json",
        std::process::id(),
        unix_ms_now()
    ))
}

fn default_report_path(role: &str) -> PathBuf {
    PathBuf::from(format!(
        "artifacts/native-pipeline/native-pipeline-cross-machine-{role}-report.json"
    ))
}

fn report_path(role: &str) -> PathBuf {
    first_string_env_nonempty(&[
        "NOVOVM_NATIVE_PIPELINE_REPORT_PATH",
        "NOVOVM_NATIVE_PIPELINE_CROSS_MACHINE_REPORT_PATH",
    ])
    .map(PathBuf::from)
    .unwrap_or_else(|| default_report_path(role))
}

fn diagnostics_report_path() -> PathBuf {
    first_string_env_nonempty(&[
        "NOVOVM_NATIVE_PIPELINE_DIAGNOSTICS_REPORT_PATH",
        "NOVOVM_NATIVE_PIPELINE_PROGRESS_REPORT_PATH",
    ])
    .map(PathBuf::from)
    .unwrap_or_else(|| {
        PathBuf::from("artifacts/native-pipeline/receiver-sustained-diagnostics-report.json")
    })
}

fn receiver_stdout_log_path() -> PathBuf {
    first_string_env_nonempty(&["NOVOVM_NATIVE_PIPELINE_RECEIVER_STDOUT_LOG_PATH"])
        .map(PathBuf::from)
        .unwrap_or_else(|| {
            PathBuf::from(
                "artifacts/native-pipeline/receiver-cross-machine-sustained-5min-stdout.log",
            )
        })
}

fn receiver_stderr_log_path() -> PathBuf {
    first_string_env_nonempty(&["NOVOVM_NATIVE_PIPELINE_RECEIVER_STDERR_LOG_PATH"])
        .map(PathBuf::from)
        .unwrap_or_else(|| {
            PathBuf::from(
                "artifacts/native-pipeline/receiver-cross-machine-sustained-5min-stderr.log",
            )
        })
}

fn receiver_exit_report_path() -> PathBuf {
    first_string_env_nonempty(&["NOVOVM_NATIVE_PIPELINE_RECEIVER_EXIT_REPORT_PATH"])
        .map(PathBuf::from)
        .unwrap_or_else(|| {
            PathBuf::from(
                "artifacts/native-pipeline/receiver-cross-machine-sustained-5min-exit.json",
            )
        })
}

fn store_path(chain_id: u64, role: &str) -> PathBuf {
    first_string_env_nonempty(&[
        "NOVOVM_NATIVE_PIPELINE_STORE_PATH",
        "NOVOVM_NATIVE_EXECUTION_TICK_STORE_PATH",
    ])
    .map(PathBuf::from)
    .unwrap_or_else(|| temp_store_path(chain_id, role))
}

fn semantic_ledger_mirror_path(store_path: &Path) -> PathBuf {
    if let Some(path) = string_env_nonempty("NOVOVM_NATIVE_AOEM_SEMANTIC_LEDGER_MIRROR") {
        return PathBuf::from(path);
    }
    let mut raw = store_path.as_os_str().to_os_string();
    raw.push(".aoem-semantic-ledger.jsonl");
    PathBuf::from(raw)
}

fn pipeline_progress_report_path(store_path: &Path) -> PathBuf {
    let mut raw = store_path.as_os_str().to_os_string();
    raw.push(".pipeline-progress.json");
    PathBuf::from(raw)
}

fn receiver_diagnostics_config() -> Result<ReceiverDiagnosticsConfigV1> {
    let enabled = bool_env("NOVOVM_NATIVE_PIPELINE_PROGRESS_WATCHDOG_ENABLED")
        || bool_env("NOVOVM_NATIVE_PIPELINE_DIAGNOSTICS_ENABLED");
    let sample_interval_ms =
        u64_env("NOVOVM_NATIVE_PIPELINE_PROGRESS_SAMPLE_INTERVAL_MS", 5_000)?.max(250);
    let stall_windows = u64_env("NOVOVM_NATIVE_PIPELINE_PROGRESS_STALL_WINDOWS", 3)?.max(1);
    let memory_sample_enabled = bool_env("NOVOVM_NATIVE_PIPELINE_MEMORY_SAMPLE_ENABLED") || enabled;
    let default_max_working_set = if memory_sample_enabled {
        8 * 1024 * 1024 * 1024u64
    } else {
        0
    };
    let sustained_duration_ms =
        u64_env("NOVOVM_NATIVE_PIPELINE_SUSTAINED_DURATION_SECONDS", 0)?.saturating_mul(1_000);
    let tail_repair_rounds = u64_env("NOVOVM_NATIVE_PIPELINE_TAIL_REPAIR_ROUNDS", 3)?;
    let tail_repair_interval_ms = u64_env("NOVOVM_NATIVE_PIPELINE_TAIL_REPAIR_INTERVAL_MS", 1_000)?;
    let default_max_elapsed_ms = if sustained_duration_ms > 0 {
        sustained_duration_ms
            .saturating_add(tail_repair_rounds.saturating_mul(tail_repair_interval_ms))
            .saturating_add(60_000)
    } else {
        0
    };
    Ok(ReceiverDiagnosticsConfigV1 {
        enabled,
        sample_interval_ms,
        stall_windows,
        memory_sample_enabled,
        max_working_set_bytes: u64_env(
            "NOVOVM_NATIVE_PIPELINE_MEMORY_MAX_WORKING_SET_BYTES",
            default_max_working_set,
        )?,
        min_canonical_delta: u64_env("NOVOVM_NATIVE_PIPELINE_PROGRESS_MIN_CANONICAL_DELTA", 0)?,
        max_elapsed_ms: u64_env(
            "NOVOVM_NATIVE_PIPELINE_RECEIVER_MAX_ELAPSED_MS",
            default_max_elapsed_ms,
        )?,
        report_path: diagnostics_report_path(),
    })
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

fn build_native_payloads_from_index(
    chain_id: u64,
    start_index: u64,
    count: u64,
) -> Result<Vec<NativeFixtureTxV1>> {
    let mut out = Vec::with_capacity(count as usize);
    for local_index in 0..count {
        let index = start_index.saturating_add(local_index);
        let nonce = index.saturating_add(1);
        let account_id = format!("acct-native-cross-machine-{nonce}");
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
                .context("encode cross-machine fixture args failed")?,
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
            .map_err(|err| anyhow::anyhow!("encode cross-machine native tx failed: {err}"))?;
        out.push(NativeFixtureTxV1 {
            index,
            copy_index: 0,
            tx_hash,
            payload,
            dropped: false,
        });
    }
    Ok(out)
}

fn merge_send_stats(target: &mut SendScheduleStatsV1, next: SendScheduleStatsV1) {
    target.scheduled_packets = target
        .scheduled_packets
        .saturating_add(next.scheduled_packets);
    target.sent_packets = target.sent_packets.saturating_add(next.sent_packets);
    target.dropped_packets = target.dropped_packets.saturating_add(next.dropped_packets);
    target.duplicated_packets = target
        .duplicated_packets
        .saturating_add(next.duplicated_packets);
    target.delayed_packets = target.delayed_packets.saturating_add(next.delayed_packets);
    target.reordered_packets = target
        .reordered_packets
        .saturating_add(next.reordered_packets);
    for (hash, count) in next.sent_by_hash {
        *target.sent_by_hash.entry(hash).or_default() += count;
    }
    target.sent_unique = target
        .sent_by_hash
        .keys()
        .count()
        .try_into()
        .unwrap_or(u64::MAX);
}

fn empty_send_stats() -> SendScheduleStatsV1 {
    SendScheduleStatsV1 {
        scheduled_packets: 0,
        sent_packets: 0,
        dropped_packets: 0,
        duplicated_packets: 0,
        delayed_packets: 0,
        reordered_packets: 0,
        sent_unique: 0,
        sent_by_hash: BTreeMap::new(),
    }
}

fn build_tail_repair_payloads(
    chain_id: u64,
    tx_count: u64,
    repair_round: u64,
) -> Result<Vec<NativeFixtureTxV1>> {
    let mut txs = build_native_payloads_from_index(chain_id, 0, tx_count)?;
    let copy_index = repair_round.saturating_add(1);
    for tx in &mut txs {
        tx.copy_index = copy_index;
        tx.dropped = false;
    }
    Ok(txs)
}

fn loss_roll_bps(seed: u64, index: u64, copy_index: u64) -> u64 {
    let mut x = seed
        ^ index.wrapping_mul(0x9e37_79b9_7f4a_7c15)
        ^ copy_index.wrapping_mul(0xbf58_476d_1ce4_e5b9);
    x ^= x >> 30;
    x = x.wrapping_mul(0xbf58_476d_1ce4_e5b9);
    x ^= x >> 27;
    x = x.wrapping_mul(0x94d0_49bb_1331_11eb);
    x ^= x >> 31;
    x % 10_000
}

fn apply_fault_schedule(
    base: &[NativeFixtureTxV1],
    fault: FaultConfigV1,
) -> Vec<NativeFixtureTxV1> {
    if !fault.enabled {
        return base.to_vec();
    }
    let duplicate_all = fault.duplicate_bps >= 10_000;
    let mut scheduled = Vec::with_capacity(base.len().saturating_mul(2));
    for tx in base {
        let mut first = tx.clone();
        first.copy_index = 0;
        first.dropped =
            loss_roll_bps(fault.seed, first.index, first.copy_index) < fault.loss_bps.min(10_000);
        scheduled.push(first);

        let duplicate_this = duplicate_all
            || loss_roll_bps(fault.seed ^ 0xa11c_e55d, tx.index, 1)
                < fault.duplicate_bps.min(10_000);
        if duplicate_this {
            let mut dup = tx.clone();
            dup.copy_index = 1;
            dup.dropped =
                loss_roll_bps(fault.seed, dup.index, dup.copy_index) < fault.loss_bps.min(10_000);
            scheduled.push(dup);
        }
    }
    if fault.reorder_bps > 0 {
        let chunk = if fault.reorder_bps >= 10_000 { 4 } else { 8 };
        for part in scheduled.chunks_mut(chunk) {
            part.reverse();
        }
    }
    scheduled
}

fn spawn_receiver_node(
    node_bin: &Path,
    chain_id: u64,
    receiver_node: u64,
    listen_addr: &str,
    store_path: &Path,
    expected_tx_count: u64,
    max_ticks: u64,
    tick_interval_ms: u64,
    batch_budget: u64,
    recv_budget: u64,
) -> Result<Child> {
    let mut cmd = Command::new(node_bin);
    cmd.env_clear();
    for (key, value) in std::env::vars() {
        if key == "NOVOVM_NODE_MODE"
            || key == "NOVOVM_NATIVE_SEND_RAW_TRANSACTION_PIPELINE_ONLY"
            || key.starts_with("NOVOVM_NATIVE_EXECUTION_")
            || key.starts_with("NOVOVM_NATIVE_PIPELINE_")
        {
            continue;
        }
        cmd.env(key, value);
    }
    let envs = [
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
            "NOVOVM_NATIVE_EXECUTION_TICK_INTERVAL_MS",
            tick_interval_ms.to_string(),
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
            "NOVOVM_NATIVE_EXECUTION_PIPELINE_REQUIRE_ROCKSDB_STORE",
            "false".to_string(),
        ),
        (
            "NOVOVM_NATIVE_EXECUTION_PIPELINE_UDP_ENABLED",
            "true".to_string(),
        ),
        (
            "NOVOVM_NATIVE_EXECUTION_PIPELINE_UDP_LISTEN_ADDR",
            listen_addr.to_string(),
        ),
        (
            "NOVOVM_NATIVE_EXECUTION_PIPELINE_LOCAL_NODE",
            receiver_node.to_string(),
        ),
        (
            "NOVOVM_NATIVE_EXECUTION_PIPELINE_UDP_RECV_BUDGET",
            recv_budget.to_string(),
        ),
        (
            "NOVOVM_NATIVE_EXECUTION_PIPELINE_UDP_BROADCAST_ENABLED",
            "false".to_string(),
        ),
        (
            "NOVOVM_NATIVE_EXECUTION_PIPELINE_BROADCAST_ENABLED",
            "false".to_string(),
        ),
        (
            "NOVOVM_NATIVE_EXECUTION_PIPELINE_REQUIRE_PROGRESS",
            "false".to_string(),
        ),
        (
            "NOVOVM_NATIVE_EXECUTION_PIPELINE_QUIET_TICKS",
            "true".to_string(),
        ),
        (
            "NOVOVM_NATIVE_EXECUTION_PIPELINE_MIN_AOEM_EXECUTED",
            expected_tx_count.to_string(),
        ),
        (
            "NOVOVM_NATIVE_EXECUTION_PIPELINE_MIN_INGRESS_TOTAL",
            "0".to_string(),
        ),
        (
            "NOVOVM_NATIVE_EXECUTION_PIPELINE_MIN_INCLUDED_CANONICAL_TOTAL",
            "0".to_string(),
        ),
        (
            "NOVOVM_NATIVE_EXECUTION_PIPELINE_MAX_QUEUE_PENDING_LAST",
            "0".to_string(),
        ),
        (
            "NOVOVM_NATIVE_EXECUTION_TICK_SCAN_LIMIT",
            expected_tx_count
                .clamp(recv_budget.max(1), 65_536)
                .to_string(),
        ),
        (
            "NOVOVM_NATIVE_EXECUTION_PIPELINE_PROGRESS_REPORT_PATH",
            pipeline_progress_report_path(store_path)
                .display()
                .to_string(),
        ),
        (
            "NOVOVM_NATIVE_EXECUTION_PIPELINE_PROGRESS_REPORT_INTERVAL_MS",
            string_env_nonempty("NOVOVM_NATIVE_PIPELINE_PROGRESS_SAMPLE_INTERVAL_MS")
                .unwrap_or_else(|| "5000".to_string()),
        ),
        (
            "NOVOVM_NATIVE_EXECUTION_PIPELINE_EXIT_WHEN_SUMMARY_VALID",
            "true".to_string(),
        ),
    ];
    for (key, value) in envs {
        cmd.env(key, value);
    }
    if let Some(peers) = string_env_nonempty("NOVOVM_NATIVE_PIPELINE_PEERS")
        .or_else(|| string_env_nonempty("NOVOVM_NATIVE_EXECUTION_PIPELINE_UDP_PEERS"))
    {
        cmd.env("NOVOVM_NATIVE_EXECUTION_PIPELINE_UDP_PEERS", peers);
    }
    cmd.stdout(Stdio::piped()).stderr(Stdio::piped());
    cmd.spawn().with_context(|| {
        format!(
            "spawn cross-machine receiver failed: bin={} listen_addr={listen_addr}",
            node_bin.display()
        )
    })
}

fn parse_summary(output: Output, label: &str) -> Result<Value> {
    parse_summary_ref(&output, label)
}

fn parse_summary_ref(output: &Output, label: &str) -> Result<Value> {
    if !output.status.success() {
        bail!(
            "{label} failed: status={} stderr={}",
            output.status,
            String::from_utf8_lossy(&output.stderr)
        );
    }
    serde_json::from_slice::<Value>(output.stdout.as_slice()).with_context(|| {
        format!(
            "{label} did not return JSON summary: stdout={} stderr={}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        )
    })
}

fn run_receiver_node(
    node_bin: &Path,
    chain_id: u64,
    receiver_node: u64,
    listen_addr: &str,
    store_path: &Path,
    expected_tx_count: u64,
    max_ticks: u64,
    tick_interval_ms: u64,
    batch_budget: u64,
    recv_budget: u64,
) -> Result<Value> {
    let diagnostics = receiver_diagnostics_config()?;
    let mut child = spawn_receiver_node(
        node_bin,
        chain_id,
        receiver_node,
        listen_addr,
        store_path,
        expected_tx_count,
        max_ticks,
        tick_interval_ms,
        batch_budget,
        recv_budget,
    )?;
    if !diagnostics.enabled {
        return parse_summary(
            child
                .wait_with_output()
                .context("wait cross-machine receiver failed")?,
            "cross-machine receiver",
        );
    }

    let child_pid = child.id();
    let started_at = Instant::now();
    let mut last_sample_at = Instant::now()
        .checked_sub(Duration::from_millis(diagnostics.sample_interval_ms))
        .unwrap_or_else(Instant::now);
    let mut state = ReceiverDiagnosticsStateV1::default();
    let ledger_path = semantic_ledger_mirror_path(store_path);
    let progress_path = pipeline_progress_report_path(store_path);
    loop {
        if child
            .try_wait()
            .context("poll cross-machine receiver failed")?
            .is_some()
        {
            let output = child
                .wait_with_output()
                .context("wait cross-machine receiver failed")?;
            let (stdout_path, stderr_path, output_artifact_error) =
                persist_child_output_artifacts(&output);
            let summary_result = parse_summary_ref(&output, "cross-machine receiver");
            let summary = match summary_result {
                Ok(summary) => summary,
                Err(err) => {
                    let ledger_stats = semantic_ledger_stats(ledger_path.as_path());
                    let rocksdb_probe =
                        get_nov_native_execution_store_rocksdb_memory_probe_v1(store_path);
                    let memory_sample = if diagnostics.memory_sample_enabled {
                        process_memory_sample(child_pid)
                    } else {
                        serde_json::json!({})
                    };
                    let progress_summary = read_pipeline_progress_summary(progress_path.as_path());
                    let mut sample = if let Some(progress) = progress_summary.as_ref() {
                        diagnostics_summary_sample(
                            started_at,
                            progress,
                            ledger_stats,
                            rocksdb_probe,
                            memory_sample,
                            state.last_canonical,
                        )
                    } else {
                        serde_json::json!({
                            "elapsed_ms": started_at.elapsed().as_millis() as u64,
                            "stable_progress_total": state.last_canonical,
                            "aoem_executed_total": 0u64,
                            "queue_pending_last": 0u64,
                            "semantic_ledger_mirror": ledger_stats,
                            "rocksdb_memory_probe": rocksdb_probe,
                            "process_memory": memory_sample,
                        })
                    };
                    sample["child_exit_parse_error"] = serde_json::json!(err.to_string());
                    if let Some(error) = output_artifact_error.as_ref() {
                        sample["output_artifact_error"] = serde_json::json!(error);
                    }
                    state.samples.push(sample);
                    if state.samples.len() > 256 {
                        let drop_count = state.samples.len().saturating_sub(256);
                        state.samples.drain(0..drop_count);
                        state.samples_dropped = state
                            .samples_dropped
                            .saturating_add(drop_count.try_into().unwrap_or(u64::MAX));
                    }
                    let reason = classify_child_exit_failure(&output, Some(&err));
                    state.fail_reason = Some(reason.clone());
                    write_diagnostics_report(
                        &diagnostics,
                        &state,
                        false,
                        child_pid,
                        expected_tx_count,
                    )?;
                    write_synthetic_receiver_failure_report(
                        expected_tx_count,
                        reason.as_str(),
                        &state,
                    )?;
                    write_receiver_exit_report(
                        child_pid,
                        Some(&output),
                        stdout_path.as_path(),
                        stderr_path.as_path(),
                        diagnostics.report_path.as_path(),
                        expected_tx_count,
                        None,
                        &state,
                        reason.as_str(),
                        false,
                        true,
                        false,
                    )?;
                    return Err(err);
                }
            };
            let ledger_stats = semantic_ledger_stats(ledger_path.as_path());
            let rocksdb_probe = get_nov_native_execution_store_rocksdb_memory_probe_v1(store_path);
            let memory_sample = if diagnostics.memory_sample_enabled {
                process_memory_sample(child_pid)
            } else {
                serde_json::json!({})
            };
            let sample = diagnostics_summary_sample(
                started_at,
                &summary,
                ledger_stats,
                rocksdb_probe,
                memory_sample,
                state.last_canonical,
            );
            state.samples.push(sample);
            write_diagnostics_report(&diagnostics, &state, true, child_pid, expected_tx_count)?;
            write_receiver_exit_report(
                child_pid,
                Some(&output),
                stdout_path.as_path(),
                stderr_path.as_path(),
                diagnostics.report_path.as_path(),
                expected_tx_count,
                Some(&summary),
                &state,
                "normal_pass",
                true,
                true,
                false,
            )?;
            return Ok(summary);
        }

        if last_sample_at.elapsed() >= Duration::from_millis(diagnostics.sample_interval_ms) {
            let ledger_stats = semantic_ledger_stats(ledger_path.as_path());
            let rocksdb_probe = live_receiver_child_rocksdb_memory_probe_v1(store_path);
            let memory_sample = if diagnostics.memory_sample_enabled {
                process_memory_sample(child_pid)
            } else {
                serde_json::json!({})
            };
            let progress_summary = read_pipeline_progress_summary(progress_path.as_path());
            let canonical = progress_summary
                .as_ref()
                .map(|summary| summary_u64(summary, "included_canonical_total"))
                .unwrap_or_else(|| {
                    ledger_stats
                        .get("line_count")
                        .and_then(Value::as_u64)
                        .unwrap_or_default()
                });
            let ledger_progress = ledger_stats
                .get("line_count")
                .and_then(Value::as_u64)
                .unwrap_or_default();
            let aoem_progress = progress_summary
                .as_ref()
                .map(|summary| summary_u64(summary, "aoem_executed_total"))
                .unwrap_or_default();
            let stable_progress = canonical.max(ledger_progress).max(aoem_progress);
            let delta = stable_progress.saturating_sub(state.last_canonical);
            let mut sample = if let Some(summary) = progress_summary.as_ref() {
                diagnostics_summary_sample(
                    started_at,
                    summary,
                    ledger_stats,
                    rocksdb_probe,
                    memory_sample,
                    state.last_canonical,
                )
            } else {
                serde_json::json!({
                "elapsed_ms": started_at.elapsed().as_millis() as u64,
                "received_unique_total": null,
                "canonical_unique_included_total": canonical,
                "stable_progress_total": stable_progress,
                "canonical_delta_since_last_sample": delta,
                "pending_count": null,
                "eligible_count": null,
                "skipped_ineligible_count": null,
                "skipped_already_receipted_count": null,
                "skipped_missing_payload_total": null,
                "skipped_non_native_payload_total": null,
                "skipped_chain_mismatch_total": null,
                "receipt_lookup_count": null,
                "receipt_lookup_hit_count": null,
                "receipt_lookup_miss_count": null,
                "receipt_lookup_elapsed_ms": null,
                "aoem_executed_total": stable_progress,
                "aoem_executed_delta": delta,
                "aoem_batch_elapsed_ms": null,
                "proof_items_total": null,
                "proof_delta": null,
                "proof_elapsed_ms": null,
                "commit_items_total": null,
                "commit_delta": null,
                "rocksdb_read_elapsed_ms": null,
                "rocksdb_write_elapsed_ms": null,
                "semantic_head_height": stable_progress,
                "semantic_head_monotonic": true,
                "semantic_ledger_mirror": ledger_stats,
                "rocksdb_memory_probe": rocksdb_probe,
                "process_memory": memory_sample,
                "queue_pending_last": null,
                "queue_dropped_total": null,
                "queue_rejected_total": null,
                })
            };
            sample["pipeline_progress_report_path"] =
                serde_json::json!(progress_path.display().to_string());
            let pending_count = sample
                .get("pending_count")
                .and_then(Value::as_u64)
                .unwrap_or_default();
            let waiting_for_sender = pending_count == 0 && stable_progress < expected_tx_count;
            sample["waiting_for_sender"] = serde_json::json!(waiting_for_sender);
            if delta == 0 && pending_count > 0 && stable_progress < expected_tx_count {
                state.stall_windows = state.stall_windows.saturating_add(1);
            } else {
                state.stall_windows = 0;
            }
            let working_set = memory_working_set_bytes(&sample["process_memory"]);
            if working_set > 0 {
                sample["process_working_set_bytes"] = serde_json::json!(working_set);
                if state.first_working_set_bytes.is_none() {
                    state.first_working_set_bytes = Some(working_set);
                }
                state.last_working_set_bytes = Some(working_set);
                if let Some(first) = state.first_working_set_bytes {
                    let elapsed_minutes = started_at.elapsed().as_secs().max(1) as f64 / 60.0;
                    let delta = working_set.saturating_sub(first) as f64;
                    sample["working_set_delta_per_minute"] =
                        serde_json::json!((delta / elapsed_minutes) as u64);
                }
            }
            let mut fail_reason = None;
            if state.stall_windows >= diagnostics.stall_windows {
                fail_reason = Some("canonical_progress_stall".to_string());
            }
            if diagnostics.min_canonical_delta > 0
                && delta < diagnostics.min_canonical_delta
                && pending_count > 0
                && stable_progress < expected_tx_count
            {
                fail_reason = Some(format!(
                    "canonical_progress_below_min_delta: delta={} min={}",
                    delta, diagnostics.min_canonical_delta
                ));
            }
            if diagnostics.max_working_set_bytes > 0
                && working_set > diagnostics.max_working_set_bytes
            {
                fail_reason = Some(format!(
                    "process_working_set_exceeded: working_set={} max={}",
                    working_set, diagnostics.max_working_set_bytes
                ));
            }
            if diagnostics.max_elapsed_ms > 0
                && started_at.elapsed() >= Duration::from_millis(diagnostics.max_elapsed_ms)
                && stable_progress < expected_tx_count
                && pending_count == 0
            {
                fail_reason = Some(format!(
                    "receiver_expected_tx_timeout: progress={} expected={} elapsed_ms={} max_elapsed_ms={}",
                    stable_progress,
                    expected_tx_count,
                    started_at.elapsed().as_millis(),
                    diagnostics.max_elapsed_ms
                ));
            }
            state.last_canonical = stable_progress;
            state.samples.push(sample);
            if state.samples.len() > 256 {
                let drop_count = state.samples.len().saturating_sub(256);
                state.samples.drain(0..drop_count);
                state.samples_dropped = state
                    .samples_dropped
                    .saturating_add(drop_count.try_into().unwrap_or(u64::MAX));
            }
            if let Some(reason) = fail_reason {
                state.fail_reason = Some(reason.clone());
                let _ = child.kill();
                let output = child
                    .wait_with_output()
                    .context("wait killed cross-machine receiver failed")?;
                let (stdout_path, stderr_path, output_artifact_error) =
                    persist_child_output_artifacts(&output);
                if let Some(error) = output_artifact_error {
                    if let Some(last) = state.samples.last_mut() {
                        last["output_artifact_error"] = serde_json::json!(error);
                    }
                }
                write_diagnostics_report(
                    &diagnostics,
                    &state,
                    false,
                    child_pid,
                    expected_tx_count,
                )?;
                write_synthetic_receiver_failure_report(
                    expected_tx_count,
                    reason.as_str(),
                    &state,
                )?;
                write_receiver_exit_report(
                    child_pid,
                    Some(&output),
                    stdout_path.as_path(),
                    stderr_path.as_path(),
                    diagnostics.report_path.as_path(),
                    expected_tx_count,
                    None,
                    &state,
                    reason.as_str(),
                    false,
                    true,
                    true,
                )?;
                bail!("cross-machine receiver diagnostics failed: {reason}");
            }
            write_diagnostics_report(&diagnostics, &state, false, child_pid, expected_tx_count)?;
            last_sample_at = Instant::now();
        }
        std::thread::sleep(Duration::from_millis(100));
    }
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

fn probe_u64(probe: &Value, field: &str) -> u64 {
    probe.get(field).and_then(Value::as_u64).unwrap_or_default()
}

fn semantic_sequence(probe: &Value) -> u64 {
    probe
        .get("semantic_head")
        .and_then(|value| value.get("sequence"))
        .and_then(Value::as_u64)
        .unwrap_or_default()
}

fn write_report(path: &Path, report: &Value) -> Result<()> {
    if let Some(parent) = path.parent() {
        if !parent.as_os_str().is_empty() {
            fs::create_dir_all(parent).with_context(|| {
                format!("create cross-machine report dir: {}", parent.display())
            })?;
        }
    }
    let encoded =
        serde_json::to_string_pretty(report).context("encode cross-machine report failed")?;
    fs::write(path, encoded)
        .with_context(|| format!("write cross-machine report failed: {}", path.display()))
}

fn write_artifact_bytes(path: &Path, bytes: &[u8]) -> Result<()> {
    if let Some(parent) = path.parent() {
        if !parent.as_os_str().is_empty() {
            fs::create_dir_all(parent)
                .with_context(|| format!("create artifact dir failed: {}", parent.display()))?;
        }
    }
    fs::write(path, bytes).with_context(|| format!("write artifact failed: {}", path.display()))
}

fn persist_child_output_artifacts(output: &Output) -> (PathBuf, PathBuf, Option<String>) {
    let stdout_path = receiver_stdout_log_path();
    let stderr_path = receiver_stderr_log_path();
    let mut error = None;
    if let Err(err) = write_artifact_bytes(stdout_path.as_path(), output.stdout.as_slice()) {
        error = Some(format!("stdout_log_write_failed: {err}"));
    }
    if let Err(err) = write_artifact_bytes(stderr_path.as_path(), output.stderr.as_slice()) {
        let item = format!("stderr_log_write_failed: {err}");
        error = Some(error.map_or(item.clone(), |prev| format!("{prev}; {item}")));
    }
    (stdout_path, stderr_path, error)
}

fn child_exit_status_json(output: &Output) -> Value {
    serde_json::json!({
        "success": output.status.success(),
        "code": output.status.code(),
        "status": output.status.to_string(),
    })
}

fn classify_child_exit_failure(output: &Output, parse_error: Option<&anyhow::Error>) -> String {
    let stderr = String::from_utf8_lossy(output.stderr.as_slice()).to_ascii_lowercase();
    if stderr.contains("panicked") || stderr.contains("panic") {
        return "child_panic".to_string();
    }
    if stderr.contains("failed to create lock file")
        && stderr.contains("rocksdb")
        && stderr.contains("lock")
    {
        return "rocksdb_lock_conflict".to_string();
    }
    if stderr.contains("open nov native execution rocksdb failed") && stderr.contains("lock") {
        return "rocksdb_lock_conflict".to_string();
    }
    if !output.status.success() {
        return "child_nonzero_exit".to_string();
    }
    if parse_error.is_some() {
        return "child_early_exit_no_report".to_string();
    }
    "child_early_exit_no_report".to_string()
}

fn output_stderr_tail(output: Option<&Output>, max_chars: usize) -> Option<String> {
    output.map(|out| {
        let stderr = String::from_utf8_lossy(out.stderr.as_slice());
        let chars: Vec<char> = stderr.chars().collect();
        let start = chars.len().saturating_sub(max_chars);
        chars[start..].iter().collect::<String>()
    })
}

fn live_receiver_child_rocksdb_memory_probe_v1(store_path: &Path) -> Value {
    let rocksdb_path = nov_native_execution_store_rocksdb_path_v1(store_path);
    serde_json::json!({
        "method": "nov_getNativeExecutionStoreRocksDbMemoryProbe",
        "rocksdb_path": rocksdb_path.display().to_string(),
        "rocksdb_exists": rocksdb_path.exists(),
        "rocksdb_opened": false,
        "rocksdb_probe_skipped": true,
        "rocksdb_probe_skipped_reason": "live_receiver_child_holds_lock",
        "rocksdb_total_estimated_memory_bytes": 0u64,
        "rocksdb_block_cache_estimated_bytes": 0u64,
        "rocksdb_memtable_estimated_bytes": 0u64,
        "rocksdb_index_filter_estimated_bytes": 0u64,
        "rocksdb_memory_probe_supported": false,
    })
}

fn write_receiver_exit_report(
    child_pid: u32,
    output: Option<&Output>,
    stdout_path: &Path,
    stderr_path: &Path,
    diagnostics_path: &Path,
    expected_tx_count: u64,
    summary: Option<&Value>,
    state: &ReceiverDiagnosticsStateV1,
    fail_reason: &str,
    final_report_written: bool,
    diagnostics_report_written: bool,
    child_was_killed: bool,
) -> Result<()> {
    let last_sample = state.samples.last();
    let stable_progress_total = last_sample
        .and_then(|sample| sample.get("stable_progress_total"))
        .and_then(Value::as_u64)
        .or_else(|| summary.map(|value| summary_u64(value, "aoem_executed_total")))
        .unwrap_or_default();
    let aoem_executed_total = last_sample
        .and_then(|sample| sample.get("aoem_executed_total"))
        .and_then(Value::as_u64)
        .or_else(|| summary.map(|value| summary_u64(value, "aoem_executed_total")))
        .unwrap_or_default();
    let queue_pending_last = last_sample
        .and_then(|sample| sample.get("queue_pending_last"))
        .and_then(Value::as_u64)
        .or_else(|| summary.map(|value| summary_u64(value, "queue_pending_last")))
        .unwrap_or_default();
    let child_panicked_detected = output
        .map(|out| {
            let stderr = String::from_utf8_lossy(out.stderr.as_slice()).to_ascii_lowercase();
            stderr.contains("panic") || stderr.contains("panicked")
        })
        .unwrap_or(false);
    let report = serde_json::json!({
        "schema": "novovm-native-pipeline-receiver-exit-forensics/v1",
        "child_pid": child_pid,
        "child_exit": output.map(child_exit_status_json).unwrap_or(serde_json::Value::Null),
        "child_exit_code": output.and_then(|out| out.status.code()),
        "child_exit_status": output.map(|out| out.status.to_string()),
        "child_was_killed": child_was_killed,
        "child_panicked_detected": child_panicked_detected,
        "child_stderr_tail": output_stderr_tail(output, 4096),
        "stdout_path": stdout_path.display().to_string(),
        "stderr_path": stderr_path.display().to_string(),
        "diagnostics_path": diagnostics_path.display().to_string(),
        "final_report_written": final_report_written,
        "diagnostics_report_written": diagnostics_report_written,
        "stable_progress_total": stable_progress_total,
        "expected_tx_total": expected_tx_count,
        "aoem_executed_total": aoem_executed_total,
        "queue_pending_last": queue_pending_last,
        "last_sample_elapsed_ms": last_sample
            .and_then(|sample| sample.get("elapsed_ms"))
            .and_then(Value::as_u64),
        "fail_reason": fail_reason,
    });
    write_report(receiver_exit_report_path().as_path(), &report)
}

fn write_synthetic_receiver_failure_report(
    expected_tx_count: u64,
    fail_reason: &str,
    state: &ReceiverDiagnosticsStateV1,
) -> Result<()> {
    let last_sample = state.samples.last();
    let stable_progress_total = last_sample
        .and_then(|sample| sample.get("stable_progress_total"))
        .and_then(Value::as_u64)
        .unwrap_or_default();
    let aoem_executed_total = last_sample
        .and_then(|sample| sample.get("aoem_executed_total"))
        .and_then(Value::as_u64)
        .unwrap_or_default();
    let queue_pending_last = last_sample
        .and_then(|sample| sample.get("queue_pending_last"))
        .and_then(Value::as_u64)
        .unwrap_or_default();
    let report = serde_json::json!({
        "schema": REPORT_SCHEMA_V1,
        "role": "receiver",
        "accepted": false,
        "synthetic_failure_report": true,
        "fail_reason": fail_reason,
        "tx_count": expected_tx_count,
        "validation": {
            "received_unique": stable_progress_total,
            "canonical_unique_included": stable_progress_total,
            "duplicate_canonical_included": 0u64,
            "duplicate_receipt": 0u64,
            "queue_pending_last": queue_pending_last,
            "semantic_head_monotonic": true,
            "receipt_index_consistent": false,
            "aoem_concurrency_owner": "AOEM_runtime",
        },
        "receiver_summary": {
            "accepted": false,
            "aoem_executed_total": aoem_executed_total,
            "queue_pending_last": queue_pending_last,
            "progress_score": stable_progress_total,
        },
        "violations": [
            format!("receiver exited before expected_tx_total: progress={stable_progress_total} expected={expected_tx_count}"),
        ],
    });
    write_report(report_path("receiver").as_path(), &report)
}

fn semantic_ledger_stats(path: &Path) -> Value {
    let Ok(metadata) = fs::metadata(path) else {
        return serde_json::json!({
            "path": path,
            "exists": false,
            "line_count": 0u64,
            "bytes": 0u64,
        });
    };
    let line_count = fs::File::open(path)
        .ok()
        .map(|file| {
            std::io::BufReader::new(file)
                .lines()
                .filter(|line| line.as_ref().is_ok_and(|item| !item.trim().is_empty()))
                .count() as u64
        })
        .unwrap_or_default();
    serde_json::json!({
        "path": path,
        "exists": true,
        "line_count": line_count,
        "bytes": metadata.len(),
    })
}

fn read_pipeline_progress_summary(path: &Path) -> Option<Value> {
    let raw = fs::read_to_string(path).ok()?;
    let value = serde_json::from_str::<Value>(raw.as_str()).ok()?;
    value.get("summary").cloned()
}

#[cfg(windows)]
fn process_memory_sample(pid: u32) -> Value {
    let script = format!(
        "$p=Get-Process -Id {pid} -ErrorAction Stop; \
         [pscustomobject]@{{\
            WorkingSet64=$p.WorkingSet64;\
            PrivateMemorySize64=$p.PrivateMemorySize64;\
            VirtualMemorySize64=$p.VirtualMemorySize64;\
            PagedMemorySize64=$p.PagedMemorySize64;\
            PagedSystemMemorySize64=$p.PagedSystemMemorySize64;\
            NonpagedSystemMemorySize64=$p.NonpagedSystemMemorySize64;\
            HandleCount=$p.HandleCount;\
            ThreadCount=$p.Threads.Count;\
            CPU=$p.CPU\
         }} | ConvertTo-Json -Compress"
    );
    match Command::new("powershell")
        .args(["-NoProfile", "-Command", script.as_str()])
        .output()
    {
        Ok(output) if output.status.success() => serde_json::from_slice::<Value>(&output.stdout)
            .unwrap_or_else(|_| {
                serde_json::json!({
                    "sample_ok": false,
                    "error": "parse_windows_process_memory_sample_failed",
                })
            }),
        Ok(output) => serde_json::json!({
            "sample_ok": false,
            "error": "windows_process_memory_sample_failed",
            "stderr": String::from_utf8_lossy(&output.stderr),
        }),
        Err(err) => serde_json::json!({
            "sample_ok": false,
            "error": format!("spawn_windows_process_memory_sample_failed: {err}"),
        }),
    }
}

#[cfg(not(windows))]
fn process_memory_sample(pid: u32) -> Value {
    let status_path = PathBuf::from(format!("/proc/{pid}/status"));
    let raw = fs::read_to_string(status_path.as_path()).unwrap_or_default();
    let mut vm_rss_kb = 0u64;
    let mut vm_data_kb = 0u64;
    let mut vm_size_kb = 0u64;
    for line in raw.lines() {
        if let Some(rest) = line.strip_prefix("VmRSS:") {
            vm_rss_kb = rest
                .split_whitespace()
                .next()
                .and_then(|value| value.parse::<u64>().ok())
                .unwrap_or_default();
        }
        if let Some(rest) = line.strip_prefix("VmData:") {
            vm_data_kb = rest
                .split_whitespace()
                .next()
                .and_then(|value| value.parse::<u64>().ok())
                .unwrap_or_default();
        }
        if let Some(rest) = line.strip_prefix("VmSize:") {
            vm_size_kb = rest
                .split_whitespace()
                .next()
                .and_then(|value| value.parse::<u64>().ok())
                .unwrap_or_default();
        }
    }
    serde_json::json!({
        "WorkingSet64": vm_rss_kb.saturating_mul(1024),
        "PrivateMemorySize64": vm_data_kb.saturating_mul(1024),
        "VirtualMemorySize64": vm_size_kb.saturating_mul(1024),
    })
}

fn memory_working_set_bytes(sample: &Value) -> u64 {
    sample
        .get("WorkingSet64")
        .or_else(|| sample.get("working_set_bytes"))
        .and_then(Value::as_u64)
        .unwrap_or_default()
}

fn memory_private_bytes(sample: &Value) -> u64 {
    sample
        .get("PrivateMemorySize64")
        .or_else(|| sample.get("private_bytes"))
        .and_then(Value::as_u64)
        .unwrap_or_default()
}

fn memory_virtual_bytes(sample: &Value) -> u64 {
    sample
        .get("VirtualMemorySize64")
        .or_else(|| sample.get("virtual_bytes"))
        .and_then(Value::as_u64)
        .unwrap_or_default()
}

fn memory_paged_bytes(sample: &Value) -> u64 {
    sample
        .get("PagedMemorySize64")
        .or_else(|| sample.get("paged_bytes"))
        .and_then(Value::as_u64)
        .unwrap_or_default()
}

fn memory_paged_system_bytes(sample: &Value) -> u64 {
    sample
        .get("PagedSystemMemorySize64")
        .or_else(|| sample.get("paged_system_bytes"))
        .and_then(Value::as_u64)
        .unwrap_or_default()
}

fn memory_nonpaged_system_bytes(sample: &Value) -> u64 {
    sample
        .get("NonpagedSystemMemorySize64")
        .or_else(|| sample.get("nonpaged_system_bytes"))
        .and_then(Value::as_u64)
        .unwrap_or_default()
}

fn memory_handle_count(sample: &Value) -> u64 {
    sample
        .get("HandleCount")
        .or_else(|| sample.get("handle_count"))
        .and_then(Value::as_u64)
        .unwrap_or_default()
}

fn memory_thread_count(sample: &Value) -> u64 {
    sample
        .get("ThreadCount")
        .or_else(|| sample.get("thread_count"))
        .and_then(Value::as_u64)
        .unwrap_or_default()
}

fn bytes_per_1000_tx(bytes: u64, tx_count: u64) -> u64 {
    if tx_count == 0 {
        return 0;
    }
    bytes.saturating_mul(1000) / tx_count
}

fn probe_bool_env(name: &str) -> bool {
    string_env_nonempty(name)
        .map(|value| {
            matches!(
                value.to_ascii_lowercase().as_str(),
                "1" | "true" | "yes" | "on"
            )
        })
        .unwrap_or(false)
}

fn sample_u64(sample: &Value, key: &str) -> u64 {
    sample.get(key).and_then(Value::as_u64).unwrap_or_default()
}

fn sample_bool(sample: &Value, key: &str) -> Option<bool> {
    sample.get(key).and_then(Value::as_bool)
}

fn sample_string(sample: &Value, key: &str) -> Option<String> {
    sample.get(key).and_then(Value::as_str).map(str::to_owned)
}

fn is_live_child_memory_sample(sample: &Value) -> bool {
    sample_u64(sample, "process_working_set_bytes") > 0
        || sample_u64(sample, "process_private_bytes") > 0
}

fn last_live_child_sample(samples: &[Value]) -> Option<&Value> {
    samples
        .iter()
        .rev()
        .find(|sample| is_live_child_memory_sample(sample))
}

fn peak_live_child_sample(samples: &[Value]) -> Option<&Value> {
    samples
        .iter()
        .filter(|sample| is_live_child_memory_sample(sample))
        .max_by_key(|sample| sample_u64(sample, "process_working_set_bytes"))
}

fn post_exit_sample_count(samples: &[Value]) -> u64 {
    samples
        .iter()
        .filter(|sample| !is_live_child_memory_sample(sample))
        .count()
        .try_into()
        .unwrap_or(u64::MAX)
}

fn diagnostics_summary_sample(
    started_at: Instant,
    summary: &Value,
    ledger_stats: Value,
    rocksdb_probe: Value,
    memory_sample: Value,
    previous_canonical: u64,
) -> Value {
    let canonical = summary_u64(summary, "included_canonical_total");
    let aoem = summary_u64(summary, "aoem_executed_total");
    let ledger_lines = ledger_stats
        .get("line_count")
        .and_then(Value::as_u64)
        .unwrap_or_default();
    let stable_progress_total = canonical.max(aoem).max(ledger_lines);
    let proof = summary_u64(summary, "max_proof_items_per_tick");
    let commit = summary_u64(summary, "max_commit_items_per_tick");
    let max_queue_admitted = summary_u64(summary, "max_queue_admitted_per_tick");
    let max_network_received = summary_u64(summary, "max_network_received_per_tick");
    let max_broadcast_tx = summary_u64(summary, "max_broadcast_tx_per_tick");
    let ticks = summary_u64(summary, "ticks");
    let working_set_bytes = memory_working_set_bytes(&memory_sample);
    let private_bytes = memory_private_bytes(&memory_sample);
    let virtual_bytes = memory_virtual_bytes(&memory_sample);
    let paged_bytes = memory_paged_bytes(&memory_sample);
    let paged_system_bytes = memory_paged_system_bytes(&memory_sample);
    let nonpaged_system_bytes = memory_nonpaged_system_bytes(&memory_sample);
    let process_handle_count = memory_handle_count(&memory_sample);
    let process_thread_count = memory_thread_count(&memory_sample);
    let runtime_current_view_bytes =
        summary_u64(summary, "queue_tx_count_last").saturating_mul(256);
    let diagnostics_report_estimated_bytes = summary_u64(summary, "ticks")
        .saturating_mul(0)
        .saturating_add(0);
    let semantic_ledger_mirror_bytes = ledger_stats
        .get("bytes")
        .and_then(Value::as_u64)
        .unwrap_or_default();
    let rocksdb_total_estimated_memory_bytes = rocksdb_probe
        .get("rocksdb_total_estimated_memory_bytes")
        .and_then(Value::as_u64)
        .unwrap_or_default();
    let rocksdb_block_cache_estimated_bytes = rocksdb_probe
        .get("rocksdb_block_cache_estimated_bytes")
        .and_then(Value::as_u64)
        .unwrap_or_default();
    let rocksdb_memtable_estimated_bytes = rocksdb_probe
        .get("rocksdb_memtable_estimated_bytes")
        .and_then(Value::as_u64)
        .unwrap_or_default();
    let rocksdb_index_filter_estimated_bytes = rocksdb_probe
        .get("rocksdb_index_filter_estimated_bytes")
        .and_then(Value::as_u64)
        .unwrap_or_default();
    let native_store_materialized_bytes =
        summary_u64(summary, "native_store_materialized_estimated_bytes_max");
    let native_store_clone_bytes =
        summary_u64(summary, "native_store_previous_clone_estimated_bytes_max");
    let rust_estimated_retained_bytes = runtime_current_view_bytes
        .saturating_add(semantic_ledger_mirror_bytes)
        .saturating_add(native_store_materialized_bytes)
        .saturating_add(native_store_clone_bytes);
    let attributed_bytes =
        rust_estimated_retained_bytes.saturating_add(rocksdb_total_estimated_memory_bytes);
    let unattributed_working_set_bytes = working_set_bytes.saturating_sub(attributed_bytes);
    let unattributed_private_bytes = private_bytes.saturating_sub(attributed_bytes);
    let native_heap_unattributed_bytes = unattributed_private_bytes;
    let working_set_minus_private_bytes = working_set_bytes.saturating_sub(private_bytes);
    let private_minus_working_set_bytes = private_bytes.saturating_sub(working_set_bytes);
    let working_set_bytes_per_1000_tx = bytes_per_1000_tx(working_set_bytes, stable_progress_total);
    let private_bytes_per_1000_tx = bytes_per_1000_tx(private_bytes, stable_progress_total);
    let native_heap_unattributed_bytes_per_1000_tx =
        bytes_per_1000_tx(native_heap_unattributed_bytes, stable_progress_total);
    let attributed_bytes_per_1000_tx = bytes_per_1000_tx(attributed_bytes, stable_progress_total);
    let aoem_batch_input_bytes = max_queue_admitted.saturating_mul(1024);
    let aoem_batch_output_bytes =
        summary_u64(summary, "max_aoem_batch_executed_per_tick").saturating_mul(2048);
    let aoem_runtime_estimated_bytes =
        aoem_batch_input_bytes.saturating_add(aoem_batch_output_bytes);
    let proof_projection_bytes = proof.saturating_mul(1024);
    let receipt_projection_bytes = native_store_materialized_bytes
        .saturating_add(native_store_clone_bytes)
        .saturating_add(commit.saturating_mul(1024));
    let canonical_projection_bytes = summary_u64(summary, "included_canonical_last")
        .saturating_add(summary_u64(summary, "included_canonical_total"))
        .saturating_mul(256);
    let udp_receive_buffer_bytes = max_network_received.saturating_mul(4096);
    let decode_buffer_bytes = max_network_received.saturating_mul(2048);
    let json_serialization_buffer_bytes = ticks.min(256).saturating_mul(2048);
    let tick_vec_capacity_bytes = max_queue_admitted
        .saturating_add(proof)
        .saturating_add(commit)
        .saturating_add(max_broadcast_tx)
        .saturating_mul(256);
    let batch_vec_capacity_bytes = max_queue_admitted
        .max(summary_u64(summary, "max_aoem_batch_executed_per_tick"))
        .saturating_mul(1024);
    let stage_estimated_bytes_total = aoem_runtime_estimated_bytes
        .saturating_add(proof_projection_bytes)
        .saturating_add(receipt_projection_bytes)
        .saturating_add(canonical_projection_bytes)
        .saturating_add(udp_receive_buffer_bytes)
        .saturating_add(decode_buffer_bytes)
        .saturating_add(json_serialization_buffer_bytes)
        .saturating_add(tick_vec_capacity_bytes)
        .saturating_add(batch_vec_capacity_bytes);
    let unknown_native_heap_source = native_heap_unattributed_bytes
        > stage_estimated_bytes_total.saturating_add(64 * 1024 * 1024);
    let native_heap_unattributed_bytes_per_tick = if ticks == 0 {
        0
    } else {
        native_heap_unattributed_bytes / ticks
    };
    let large_allocation_suspected_stage = if native_store_materialized_bytes
        .saturating_add(native_store_clone_bytes)
        > 64 * 1024 * 1024
    {
        "native_store_materialization"
    } else if unknown_native_heap_source {
        "unknown_native_heap_source"
    } else if aoem_runtime_estimated_bytes > stage_estimated_bytes_total / 2 {
        "aoem_batch_buffers"
    } else {
        "none"
    };
    let allocator_fragmentation_suspected =
        unattributed_private_bytes > attributed_bytes.max(64 * 1024 * 1024);
    let working_set_not_returned_suspected =
        working_set_bytes > private_bytes.saturating_add(256 * 1024 * 1024) && private_bytes > 0;
    let mut out = serde_json::json!({
        "elapsed_ms": started_at.elapsed().as_millis() as u64,
        "received_unique_total": summary_u64(summary, "ingress_total_last"),
        "canonical_unique_included_total": canonical,
        "stable_progress_total": stable_progress_total,
        "canonical_delta_since_last_sample": stable_progress_total.saturating_sub(previous_canonical),
        "pending_count": summary_u64(summary, "queue_pending_last"),
        "eligible_count": null,
        "skipped_ineligible_count": summary_u64(summary, "skipped_ineligible_stage_total"),
        "skipped_already_receipted_count": summary_u64(summary, "skipped_already_receipted_total"),
        "skipped_missing_payload_total": summary_u64(summary, "skipped_missing_payload_total"),
        "skipped_non_native_payload_total": summary_u64(summary, "skipped_non_native_payload_total"),
        "skipped_chain_mismatch_total": summary_u64(summary, "skipped_chain_mismatch_total"),
        "receipt_lookup_count": null,
        "receipt_lookup_hit_count": summary_u64(summary, "skipped_already_receipted_total"),
        "receipt_lookup_miss_count": null,
        "receipt_lookup_elapsed_ms": null,
        "aoem_executed_total": aoem,
        "aoem_executed_delta": stable_progress_total.saturating_sub(previous_canonical),
        "aoem_batch_elapsed_ms": null,
        "proof_items_total": proof,
        "proof_delta": null,
        "proof_elapsed_ms": null,
        "commit_items_total": commit,
        "commit_delta": null,
        "rocksdb_read_elapsed_ms": null,
        "rocksdb_write_elapsed_ms": null,
        "semantic_head_height": canonical,
        "semantic_head_monotonic": true,
        "semantic_ledger_mirror": ledger_stats,
        "rocksdb_memory_probe": rocksdb_probe,
        "process_memory": memory_sample,
        "process_working_set_bytes": working_set_bytes,
        "process_private_bytes": private_bytes,
        "virtual_bytes": virtual_bytes,
        "process_virtual_bytes": virtual_bytes,
        "process_paged_bytes": paged_bytes,
        "process_paged_system_bytes": paged_system_bytes,
        "process_nonpaged_system_bytes": nonpaged_system_bytes,
        "process_handle_count": process_handle_count,
        "process_thread_count": process_thread_count,
        "rust_estimated_retained_bytes": rust_estimated_retained_bytes,
        "pending_runtime_estimated_bytes": runtime_current_view_bytes,
        "runtime_current_view_bytes_estimate": runtime_current_view_bytes,
        "diagnostics_report_estimated_bytes": diagnostics_report_estimated_bytes,
        "semantic_ledger_mirror_bytes": semantic_ledger_mirror_bytes,
        "jsonl_writer_buffer_bytes": 0u64,
        "native_store_materialized_estimated_bytes": native_store_materialized_bytes,
        "native_store_previous_clone_estimated_bytes": native_store_clone_bytes,
        "rocksdb_total_estimated_memory_bytes": rocksdb_total_estimated_memory_bytes,
        "rocksdb_block_cache_estimated_bytes": rocksdb_block_cache_estimated_bytes,
        "rocksdb_memtable_estimated_bytes": rocksdb_memtable_estimated_bytes,
        "rocksdb_index_filter_estimated_bytes": rocksdb_index_filter_estimated_bytes,
        "native_heap_unattributed_bytes": native_heap_unattributed_bytes,
        "unattributed_private_bytes": unattributed_private_bytes,
        "unattributed_working_set_bytes": unattributed_working_set_bytes,
        "working_set_minus_private_bytes": working_set_minus_private_bytes,
        "private_minus_working_set_bytes": private_minus_working_set_bytes,
        "allocator_fragmentation_suspected": allocator_fragmentation_suspected,
        "working_set_not_returned_suspected": working_set_not_returned_suspected,
        "working_set_bytes_per_1000_tx": working_set_bytes_per_1000_tx,
        "private_bytes_per_1000_tx": private_bytes_per_1000_tx,
        "native_heap_unattributed_bytes_per_1000_tx": native_heap_unattributed_bytes_per_1000_tx,
        "attributed_bytes_per_1000_tx": attributed_bytes_per_1000_tx,
        "queue_pending_last": summary_u64(summary, "queue_pending_last"),
        "queue_dropped_total": summary_u64(summary, "queue_dropped_last"),
        "queue_rejected_total": summary_u64(summary, "queue_rejected_last"),
        "ticks": summary_u64(summary, "ticks"),
        "ticks_per_sec_x1000": summary_u64(summary, "ticks_per_sec_x1000"),
    });
    out["aoem_runtime_estimated_bytes"] = serde_json::json!(aoem_runtime_estimated_bytes);
    out["aoem_batch_input_bytes"] = serde_json::json!(aoem_batch_input_bytes);
    out["aoem_batch_output_bytes"] = serde_json::json!(aoem_batch_output_bytes);
    out["aoem_projection_estimated_bytes"] = serde_json::json!(aoem_runtime_estimated_bytes);
    out["proof_projection_bytes"] = serde_json::json!(proof_projection_bytes);
    out["receipt_projection_bytes"] = serde_json::json!(receipt_projection_bytes);
    out["canonical_projection_bytes"] = serde_json::json!(canonical_projection_bytes);
    out["udp_receive_buffer_bytes"] = serde_json::json!(udp_receive_buffer_bytes);
    out["decode_buffer_bytes"] = serde_json::json!(decode_buffer_bytes);
    out["json_serialization_buffer_bytes"] = serde_json::json!(json_serialization_buffer_bytes);
    out["tick_vec_capacity_bytes"] = serde_json::json!(tick_vec_capacity_bytes);
    out["batch_vec_capacity_bytes"] = serde_json::json!(batch_vec_capacity_bytes);
    out["stage_estimated_bytes_total"] = serde_json::json!(stage_estimated_bytes_total);
    out["native_heap_unattributed_bytes_per_tick"] =
        serde_json::json!(native_heap_unattributed_bytes_per_tick);
    out["unknown_native_heap_source"] = serde_json::json!(unknown_native_heap_source);
    out["large_allocation_suspected_stage"] = serde_json::json!(large_allocation_suspected_stage);
    out["native_heap_source_isolation_confidence"] =
        serde_json::json!(if unknown_native_heap_source {
            "low_unknown_dominates"
        } else {
            "estimated_stage_attribution"
        });
    out["memory_probe_stage_switches"] = serde_json::json!({
        "disable_proof_projection_for_memory_probe": probe_bool_env("NOVOVM_NATIVE_PIPELINE_DISABLE_PROOF_PROJECTION_FOR_MEMORY_PROBE"),
        "disable_canonical_projection_for_memory_probe": probe_bool_env("NOVOVM_NATIVE_PIPELINE_DISABLE_CANONICAL_PROJECTION_FOR_MEMORY_PROBE"),
        "disable_report_serialization_for_memory_probe": probe_bool_env("NOVOVM_NATIVE_PIPELINE_DISABLE_REPORT_SERIALIZATION_FOR_MEMORY_PROBE"),
        "disable_recovery_probe_for_memory_probe": probe_bool_env("NOVOVM_NATIVE_PIPELINE_DISABLE_RECOVERY_PROBE_FOR_MEMORY_PROBE"),
        "applies_to_production_default": false,
        "lifecycle_structure_changed": false,
    });
    out["active_pending_count"] =
        serde_json::json!(summary_u64(summary, "queue_active_pending_last"));
    out["historical_pending_count"] =
        serde_json::json!(summary_u64(summary, "queue_historical_pending_last"));
    out["current_view_received_retained"] =
        serde_json::json!(summary_u64(summary, "ingress_total_last"));
    out["current_view_included_retained"] =
        serde_json::json!(summary_u64(summary, "included_canonical_last"));
    out["current_view_dropped_retained"] =
        serde_json::json!(summary_u64(summary, "queue_dropped_last"));
    out["queue_dropped_last_active"] = serde_json::json!(0u64);
    out["queue_dropped_total_cumulative"] =
        serde_json::json!(summary_u64(summary, "queue_dropped_last"));
    out["historical_compacted_total"] =
        serde_json::json!(summary_u64(summary, "historical_compacted_total"));
    out["historical_payload_bytes_freed"] =
        serde_json::json!(summary_u64(summary, "historical_payload_bytes_freed"));
    out["tombstone_retained_count"] =
        serde_json::json!(summary_u64(summary, "tombstone_retained_count"));
    out["tombstone_evicted_count"] =
        serde_json::json!(summary_u64(summary, "tombstone_evicted_count"));
    out["historical_pending_after_compaction"] =
        serde_json::json!(summary_u64(summary, "historical_pending_after_compaction"));
    out["included_retained_after_compaction"] =
        serde_json::json!(summary_u64(summary, "included_retained_after_compaction"));
    out["dropped_retained_after_compaction"] =
        serde_json::json!(summary_u64(summary, "dropped_retained_after_compaction"));
    out["runtime_current_view_bytes_estimate"] =
        serde_json::json!(summary_u64(summary, "queue_tx_count_last").saturating_mul(256));
    out
}

fn write_diagnostics_report(
    config: &ReceiverDiagnosticsConfigV1,
    state: &ReceiverDiagnosticsStateV1,
    accepted: bool,
    child_pid: u32,
    tx_count: u64,
) -> Result<()> {
    let last_sample_any = state.samples.last();
    let last_live_sample = last_live_child_sample(state.samples.as_slice());
    let peak_live_sample = peak_live_child_sample(state.samples.as_slice());
    let memory_summary_sample = peak_live_sample.or(last_live_sample);
    let post_exit_samples = post_exit_sample_count(state.samples.as_slice());
    let post_exit_working_set_zeroed = last_sample_any
        .map(|sample| {
            !is_live_child_memory_sample(sample)
                && sample.get("process_working_set_bytes").is_some()
        })
        .unwrap_or(false);
    let memory_summary_source = if peak_live_sample.is_some() {
        "live_peak"
    } else if last_live_sample.is_some() {
        "live_last"
    } else if post_exit_samples > 0 {
        "post_exit_invalid"
    } else {
        "none"
    };
    let report = serde_json::json!({
        "schema": "novovm-native-pipeline-cross-machine-sustained-diagnostics/v1",
        "accepted": accepted,
        "child_pid": child_pid,
        "expected_tx_count": tx_count,
        "sample_interval_ms": config.sample_interval_ms,
        "stall_windows": config.stall_windows,
        "memory_sample_enabled": config.memory_sample_enabled,
        "max_working_set_bytes": config.max_working_set_bytes,
        "min_canonical_delta": config.min_canonical_delta,
        "max_elapsed_ms": config.max_elapsed_ms,
        "fail_reason": state.fail_reason,
        "diagnostics_samples_retained": state.samples.len(),
        "diagnostics_samples_dropped": state.samples_dropped,
        "sample_count": state.samples.len(),
        "first_working_set_bytes": state.first_working_set_bytes,
        "last_working_set_bytes": state.last_working_set_bytes,
        "working_set_delta_total_bytes": state
            .last_working_set_bytes
            .zip(state.first_working_set_bytes)
            .map(|(last, first)| last.saturating_sub(first)),
        "last_sample_any": last_sample_any.cloned(),
        "last_live_child_sample": last_live_sample.cloned(),
        "peak_live_child_sample": peak_live_sample.cloned(),
        "post_exit_sample_present": post_exit_samples > 0,
        "post_exit_sample_count": post_exit_samples,
        "live_sample_count": state.samples.len().saturating_sub(post_exit_samples as usize),
        "post_exit_working_set_zeroed": post_exit_working_set_zeroed,
        "memory_summary_source": memory_summary_source,
        "last_sample_any_process_working_set_bytes": last_sample_any
            .map(|sample| sample_u64(sample, "process_working_set_bytes")),
        "last_sample_any_process_private_bytes": last_sample_any
            .map(|sample| sample_u64(sample, "process_private_bytes")),
        "last_process_working_set_bytes": last_live_sample
            .map(|sample| sample_u64(sample, "process_working_set_bytes")),
        "last_process_private_bytes": last_live_sample
            .map(|sample| sample_u64(sample, "process_private_bytes")),
        "last_process_virtual_bytes": last_live_sample
            .map(|sample| sample_u64(sample, "process_virtual_bytes")),
        "last_rust_estimated_retained_bytes": last_live_sample
            .map(|sample| sample_u64(sample, "rust_estimated_retained_bytes")),
        "last_native_heap_unattributed_bytes": last_live_sample
            .map(|sample| sample_u64(sample, "native_heap_unattributed_bytes")),
        "last_unattributed_working_set_bytes": last_live_sample
            .map(|sample| sample_u64(sample, "unattributed_working_set_bytes")),
        "last_rocksdb_total_estimated_memory_bytes": last_live_sample
            .map(|sample| sample_u64(sample, "rocksdb_total_estimated_memory_bytes")),
        "last_working_set_bytes_per_1000_tx": last_live_sample
            .map(|sample| sample_u64(sample, "working_set_bytes_per_1000_tx")),
        "last_private_bytes_per_1000_tx": last_live_sample
            .map(|sample| sample_u64(sample, "private_bytes_per_1000_tx")),
        "last_native_heap_unattributed_bytes_per_1000_tx": last_live_sample
            .map(|sample| sample_u64(sample, "native_heap_unattributed_bytes_per_1000_tx")),
        "peak_live_working_set_bytes": peak_live_sample
            .map(|sample| sample_u64(sample, "process_working_set_bytes")),
        "peak_live_private_bytes": peak_live_sample
            .map(|sample| sample_u64(sample, "process_private_bytes")),
        "peak_live_native_heap_unattributed_bytes": peak_live_sample
            .map(|sample| sample_u64(sample, "native_heap_unattributed_bytes")),
        "peak_live_process_virtual_bytes": peak_live_sample
            .map(|sample| sample_u64(sample, "process_virtual_bytes")),
        "peak_live_allocator_fragmentation_suspected": peak_live_sample
            .and_then(|sample| sample_bool(sample, "allocator_fragmentation_suspected")),
        "peak_live_working_set_not_returned_suspected": peak_live_sample
            .and_then(|sample| sample_bool(sample, "working_set_not_returned_suspected")),
        "allocator_fragmentation_suspected": memory_summary_sample
            .and_then(|sample| sample_bool(sample, "allocator_fragmentation_suspected")),
        "working_set_not_returned_suspected": memory_summary_sample
            .and_then(|sample| sample_bool(sample, "working_set_not_returned_suspected")),
        "summary_aoem_runtime_estimated_bytes": memory_summary_sample
            .map(|sample| sample_u64(sample, "aoem_runtime_estimated_bytes")),
        "summary_aoem_batch_input_bytes": memory_summary_sample
            .map(|sample| sample_u64(sample, "aoem_batch_input_bytes")),
        "summary_aoem_batch_output_bytes": memory_summary_sample
            .map(|sample| sample_u64(sample, "aoem_batch_output_bytes")),
        "summary_proof_projection_bytes": memory_summary_sample
            .map(|sample| sample_u64(sample, "proof_projection_bytes")),
        "summary_receipt_projection_bytes": memory_summary_sample
            .map(|sample| sample_u64(sample, "receipt_projection_bytes")),
        "summary_canonical_projection_bytes": memory_summary_sample
            .map(|sample| sample_u64(sample, "canonical_projection_bytes")),
        "summary_udp_receive_buffer_bytes": memory_summary_sample
            .map(|sample| sample_u64(sample, "udp_receive_buffer_bytes")),
        "summary_decode_buffer_bytes": memory_summary_sample
            .map(|sample| sample_u64(sample, "decode_buffer_bytes")),
        "summary_json_serialization_buffer_bytes": memory_summary_sample
            .map(|sample| sample_u64(sample, "json_serialization_buffer_bytes")),
        "summary_tick_vec_capacity_bytes": memory_summary_sample
            .map(|sample| sample_u64(sample, "tick_vec_capacity_bytes")),
        "summary_batch_vec_capacity_bytes": memory_summary_sample
            .map(|sample| sample_u64(sample, "batch_vec_capacity_bytes")),
        "summary_stage_estimated_bytes_total": memory_summary_sample
            .map(|sample| sample_u64(sample, "stage_estimated_bytes_total")),
        "summary_native_heap_unattributed_bytes_per_tick": memory_summary_sample
            .map(|sample| sample_u64(sample, "native_heap_unattributed_bytes_per_tick")),
        "summary_unknown_native_heap_source": memory_summary_sample
            .and_then(|sample| sample_bool(sample, "unknown_native_heap_source")),
        "summary_large_allocation_suspected_stage": memory_summary_sample
            .and_then(|sample| sample_string(sample, "large_allocation_suspected_stage")),
        "summary_native_heap_source_isolation_confidence": memory_summary_sample
            .and_then(|sample| sample_string(sample, "native_heap_source_isolation_confidence")),
        "samples": state.samples,
    });
    write_report(config.report_path.as_path(), &report)
}

fn send_scheduled_batch(
    chain_id: u64,
    sender_node: u64,
    receiver_node: u64,
    sender_addr: &str,
    receiver_addr: &str,
    txs: &[NativeFixtureTxV1],
    delay_ms: u64,
) -> Result<SendScheduleStatsV1> {
    let sender = UdpTransport::bind_for_chain(NodeId(sender_node), sender_addr, chain_id)
        .with_context(|| format!("bind cross-machine sender UDP failed: {sender_addr}"))?;
    sender
        .register_peer(NodeId(receiver_node), receiver_addr)
        .with_context(|| format!("register cross-machine receiver peer failed: {receiver_addr}"))?;
    let mut sent_by_hash = BTreeMap::<String, u64>::new();
    let mut sent_unique = BTreeSet::<String>::new();
    let mut sent_packets = 0u64;
    let mut dropped_packets = 0u64;
    let duplicated_packets = txs
        .iter()
        .filter(|tx| tx.copy_index > 0)
        .count()
        .try_into()
        .unwrap_or(u64::MAX);
    let reordered_packets = txs
        .windows(2)
        .filter(|pair| pair[0].index > pair[1].index)
        .count()
        .try_into()
        .unwrap_or(u64::MAX);
    for tx in txs {
        if tx.dropped {
            dropped_packets = dropped_packets.saturating_add(1);
            continue;
        }
        let msg = ProtocolMessage::EvmNative(EvmNativeMessage::Transactions {
            from: NodeId(sender_node),
            chain_id,
            tx_hash: tx.tx_hash,
            tx_count: 1,
            payload: tx.payload.clone(),
        });
        sender.send(NodeId(receiver_node), msg).with_context(|| {
            format!(
                "send cross-machine tx index={} copy={} failed",
                tx.index, tx.copy_index
            )
        })?;
        let hash = hex_lower(&tx.tx_hash);
        sent_unique.insert(hash.clone());
        *sent_by_hash.entry(hash).or_default() += 1;
        sent_packets = sent_packets.saturating_add(1);
        if delay_ms > 0 {
            std::thread::sleep(std::time::Duration::from_millis(delay_ms));
        }
    }
    Ok(SendScheduleStatsV1 {
        scheduled_packets: txs.len().try_into().unwrap_or(u64::MAX),
        sent_packets,
        dropped_packets,
        duplicated_packets,
        delayed_packets: if delay_ms > 0 { sent_packets } else { 0 },
        reordered_packets,
        sent_unique: sent_unique.len().try_into().unwrap_or(u64::MAX),
        sent_by_hash,
    })
}

fn validate_boundaries(summary: &Value, violations: &mut Vec<String>) {
    if summary_str(summary, "execution_kernel") != "AOEM" {
        violations.push(format!(
            "execution_kernel={} expected AOEM",
            summary_str(summary, "execution_kernel")
        ));
    }
    if summary_str(summary, "aoem_concurrency_owner") != "AOEM_runtime" {
        violations.push(format!(
            "aoem_concurrency_owner={} expected AOEM_runtime",
            summary_str(summary, "aoem_concurrency_owner")
        ));
    }
    if summary_str(summary, "host_concurrency_policy")
        != "host_drives_lifecycle_only_no_rust_execution_scheduler"
    {
        violations.push(format!(
            "host_concurrency_policy={} expected host lifecycle only",
            summary_str(summary, "host_concurrency_policy")
        ));
    }
}

fn validate_receiver_report(summary: &Value, probe: &Value, tx_count: u64) -> (Value, Vec<String>) {
    let receipt_count = probe_u64(probe, "receipt_count");
    let semantic_sequence = semantic_sequence(probe);
    let received_unique = summary_u64(summary, "ingress_total_last")
        .max(summary_u64(summary, "aoem_executed_total"))
        .max(receipt_count);
    let canonical_unique_included = summary_u64(summary, "included_canonical_total")
        .max(receipt_count)
        .max(semantic_sequence);
    let duplicate_canonical_included = canonical_unique_included.saturating_sub(tx_count);
    let duplicate_receipt = receipt_count.saturating_sub(tx_count);
    let semantic_head_monotonic = probe
        .get("semantic_head_current_recovered")
        .and_then(Value::as_bool)
        == Some(true)
        && probe
            .get("semantic_head_by_height_recovered")
            .and_then(Value::as_bool)
            == Some(true)
        && semantic_sequence >= canonical_unique_included;
    let receipt_index_consistent = probe
        .get("receipt_index_recovered")
        .and_then(Value::as_bool)
        == Some(true)
        && receipt_count == tx_count;
    let mut violations = Vec::<String>::new();
    validate_boundaries(summary, &mut violations);
    if received_unique != tx_count {
        violations.push(format!(
            "received_unique={received_unique} expected tx_count={tx_count}"
        ));
    }
    if canonical_unique_included != tx_count {
        violations.push(format!(
            "canonical_unique_included={canonical_unique_included} expected tx_count={tx_count}"
        ));
    }
    if duplicate_canonical_included != 0 {
        violations.push(format!(
            "duplicate_canonical_included={duplicate_canonical_included} expected 0"
        ));
    }
    if duplicate_receipt != 0 {
        violations.push(format!("duplicate_receipt={duplicate_receipt} expected 0"));
    }
    if summary_u64(summary, "queue_pending_last") != 0 {
        violations.push(format!(
            "queue_pending_last={} expected 0",
            summary_u64(summary, "queue_pending_last")
        ));
    }
    if !semantic_head_monotonic {
        violations.push("semantic_head_monotonic=false".to_string());
    }
    if !receipt_index_consistent {
        violations.push("receipt_index_consistent=false".to_string());
    }
    (
        serde_json::json!({
            "received_unique": received_unique,
            "canonical_unique_included": canonical_unique_included,
            "duplicate_canonical_included": duplicate_canonical_included,
            "duplicate_receipt": duplicate_receipt,
            "queue_pending_last": summary_u64(summary, "queue_pending_last"),
            "semantic_head_monotonic": semantic_head_monotonic,
            "receipt_index_consistent": receipt_index_consistent,
            "aoem_concurrency_owner": summary_str(summary, "aoem_concurrency_owner"),
        }),
        violations,
    )
}

fn run_sender(
    chain_id: u64,
    tx_count: u64,
    sender_node: u64,
    receiver_node: u64,
    sender_addr: &str,
    receiver_addr: &str,
    fault: FaultConfigV1,
    sustained: SustainedConfigV1,
    tail_repair: TailRepairConfigV1,
) -> Result<Value> {
    let mut stats = empty_send_stats();
    let mut repair_stats = empty_send_stats();
    let tx_per_round = if sustained.enabled {
        sustained.tx_per_round.max(1)
    } else {
        tx_count
    };
    let rounds = div_ceil_u64(tx_count, tx_per_round).max(1);
    let mut sent_unique_target = 0u64;
    for round in 0..rounds {
        let remaining = tx_count.saturating_sub(sent_unique_target);
        if remaining == 0 {
            break;
        }
        let round_tx_count = remaining.min(tx_per_round);
        let txs = build_native_payloads_from_index(chain_id, sent_unique_target, round_tx_count)?;
        let scheduled = apply_fault_schedule(txs.as_slice(), fault);
        let round_stats = send_scheduled_batch(
            chain_id,
            sender_node,
            receiver_node,
            sender_addr,
            receiver_addr,
            scheduled.as_slice(),
            fault.delay_ms,
        )?;
        sent_unique_target = sent_unique_target.saturating_add(round_tx_count);
        merge_send_stats(&mut stats, round_stats);
        if sustained.enabled && round + 1 < rounds && sustained.round_interval_ms > 0 {
            std::thread::sleep(std::time::Duration::from_millis(
                sustained.round_interval_ms,
            ));
        }
    }
    let mut repair_rounds_used = 0u64;
    if tail_repair.enabled && tail_repair.rounds > 0 {
        for repair_round in 0..tail_repair.rounds {
            if tail_repair.interval_ms > 0 {
                std::thread::sleep(std::time::Duration::from_millis(tail_repair.interval_ms));
            }
            let txs = build_tail_repair_payloads(chain_id, tx_count, repair_round)?;
            let round_stats = send_scheduled_batch(
                chain_id,
                sender_node,
                receiver_node,
                sender_addr,
                receiver_addr,
                txs.as_slice(),
                0,
            )?;
            merge_send_stats(&mut repair_stats, round_stats);
            repair_rounds_used = repair_rounds_used.saturating_add(1);
        }
        merge_send_stats(&mut stats, repair_stats.clone());
    }
    let accepted = stats.sent_unique == tx_count;
    let report = serde_json::json!({
        "schema": REPORT_SCHEMA_V1,
        "role": "sender",
        "accepted": accepted,
        "chain_id": chain_id,
        "tx_count": tx_count,
        "sender_node": sender_node,
        "receiver_node": receiver_node,
        "sender_addr": sender_addr,
        "receiver_addr": receiver_addr,
        "clean_network": {
            "packet_loss": fault.loss_bps,
            "duplicate": fault.duplicate_bps,
            "delay_ms": fault.delay_ms,
            "reorder": fault.reorder_bps,
            "sent_count": stats.sent_packets,
            "sent_unique": stats.sent_unique,
        },
        "fault_injection": {
            "enabled": fault.enabled,
            "packet_loss_bps": fault.loss_bps,
            "duplicate_bps": fault.duplicate_bps,
            "delay_ms": fault.delay_ms,
            "reorder_bps": fault.reorder_bps,
            "seed": fault.seed,
            "scheduled_packets": stats.scheduled_packets,
            "sent_packets": stats.sent_packets,
            "dropped_packets": stats.dropped_packets,
            "duplicated_packets": stats.duplicated_packets,
            "delayed_packets": stats.delayed_packets,
            "reordered_packets": stats.reordered_packets,
            "sent_unique": stats.sent_unique,
        },
        "sustained": {
            "enabled": sustained.enabled,
            "duration_seconds": sustained.duration_seconds,
            "rounds": rounds,
            "tx_per_round": tx_per_round,
            "round_interval_ms": sustained.round_interval_ms,
            "tx_submitted_total": stats.sent_unique,
        },
        "tail_repair": {
            "enabled": tail_repair.enabled,
            "rounds_configured": tail_repair.rounds,
            "interval_ms": tail_repair.interval_ms,
            "repair_rounds_used": repair_rounds_used,
            "initial_sent_total": stats.sent_packets.saturating_sub(repair_stats.sent_packets),
            "repair_sent_total": repair_stats.sent_packets,
            "repair_scheduled_total": repair_stats.scheduled_packets,
            "tail_repair_success": accepted,
        },
        "sent_by_hash": stats.sent_by_hash,
        "violations": if accepted { Vec::<String>::new() } else { vec!["sender did not send expected unique tx count".to_string()] },
    });
    Ok(compact_sender_report_for_report(report))
}

fn run_receiver(
    chain_id: u64,
    tx_count: u64,
    receiver_node: u64,
    listen_addr: &str,
    node_bin: &Path,
    store_path: &Path,
    max_ticks: u64,
    tick_interval_ms: u64,
    batch_budget: u64,
    recv_budget: u64,
    sustained: SustainedConfigV1,
) -> Result<Value> {
    let receiver_summary = run_receiver_node(
        node_bin,
        chain_id,
        receiver_node,
        listen_addr,
        store_path,
        tx_count,
        max_ticks,
        tick_interval_ms,
        batch_budget,
        recv_budget,
    )?;
    std::env::set_var("NOVOVM_NATIVE_EXECUTION_STORE_BACKEND", "rocksdb");
    let recovery_probe = get_nov_native_execution_store_recovery_probe_v1(store_path)?;
    let (validation, violations) =
        validate_receiver_report(&receiver_summary, &recovery_probe, tx_count);
    let accepted = violations.is_empty();
    Ok(serde_json::json!({
        "schema": REPORT_SCHEMA_V1,
        "role": "receiver",
        "accepted": accepted,
        "chain_id": chain_id,
        "tx_count": tx_count,
        "receiver_node": receiver_node,
        "listen_addr": listen_addr,
        "store_path": store_path,
        "clean_network": {
            "packet_loss": 0,
            "duplicate": 0,
            "delay": 0,
            "reorder": 0
        },
        "sustained": {
            "enabled": sustained.enabled,
            "duration_seconds": sustained.duration_seconds,
            "tx_per_round": sustained.tx_per_round,
            "round_interval_ms": sustained.round_interval_ms,
            "expected_tx_total": tx_count,
        },
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
        "validation": validation,
        "receiver_summary": compact_receiver_summary_for_report(receiver_summary),
        "recovery_probe": compact_probe_for_report(recovery_probe),
        "violations": violations
    }))
}

fn run_local_smoke(
    chain_id: u64,
    tx_count: u64,
    sender_node: u64,
    receiver_node: u64,
    node_bin: &Path,
    store_path: &Path,
    max_ticks: u64,
    tick_interval_ms: u64,
    batch_budget: u64,
    recv_budget: u64,
    startup_wait_ms: u64,
    fault: FaultConfigV1,
    sustained: SustainedConfigV1,
    tail_repair: TailRepairConfigV1,
) -> Result<Value> {
    let sender_addr = reserve_udp_addr()?;
    let receiver_addr = reserve_udp_addr()?;
    let child = spawn_receiver_node(
        node_bin,
        chain_id,
        receiver_node,
        receiver_addr.as_str(),
        store_path,
        tx_count,
        max_ticks,
        tick_interval_ms,
        batch_budget,
        recv_budget,
    )?;
    std::thread::sleep(std::time::Duration::from_millis(startup_wait_ms));
    let sender_report = run_sender(
        chain_id,
        tx_count,
        sender_node,
        receiver_node,
        sender_addr.as_str(),
        receiver_addr.as_str(),
        FaultConfigV1 {
            delay_ms: if fault.enabled { fault.delay_ms } else { 1 },
            ..fault
        },
        sustained,
        tail_repair,
    )?;
    let receiver_summary = parse_summary(
        child
            .wait_with_output()
            .context("wait local cross-machine smoke receiver failed")?,
        "local cross-machine smoke receiver",
    )?;
    std::env::set_var("NOVOVM_NATIVE_EXECUTION_STORE_BACKEND", "rocksdb");
    let recovery_probe = get_nov_native_execution_store_recovery_probe_v1(store_path)?;
    let (validation, violations) =
        validate_receiver_report(&receiver_summary, &recovery_probe, tx_count);
    let accepted = violations.is_empty()
        && sender_report
            .get("accepted")
            .and_then(Value::as_bool)
            .unwrap_or(false);
    Ok(serde_json::json!({
        "schema": REPORT_SCHEMA_V1,
        "role": "local-smoke",
        "accepted": accepted,
        "chain_id": chain_id,
        "tx_count": tx_count,
        "sender_addr": sender_addr,
        "receiver_addr": receiver_addr,
        "sender_report": compact_sender_report_for_report(sender_report),
        "validation": validation,
        "receiver_summary": compact_receiver_summary_for_report(receiver_summary),
        "recovery_probe": compact_probe_for_report(recovery_probe),
        "violations": violations
    }))
}

fn main() -> Result<()> {
    let role = first_string_env_nonempty(&[
        "NOVOVM_NATIVE_PIPELINE_ROLE",
        "NOVOVM_NATIVE_PIPELINE_CROSS_MACHINE_ROLE",
    ])
    .unwrap_or_else(|| "local-smoke".to_string())
    .to_ascii_lowercase();
    let sustained_binary = current_bin_name_contains("sustained");
    let sustained_env = env_any(&[
        "NOVOVM_NATIVE_PIPELINE_SUSTAINED_ENABLED",
        "NOVOVM_NATIVE_PIPELINE_SUSTAINED_DURATION_SECONDS",
        "NOVOVM_NATIVE_PIPELINE_SUSTAINED_TX_PER_ROUND",
        "NOVOVM_NATIVE_PIPELINE_SUSTAINED_ROUND_INTERVAL_MS",
    ]);
    let sustained_enabled = sustained_binary || sustained_env;
    let chain_id = u64_env_alias(
        &[
            "NOVOVM_NATIVE_PIPELINE_CHAIN_ID",
            "NOVOVM_NATIVE_PIPELINE_CROSS_MACHINE_CHAIN_ID",
        ],
        9_998_904,
    )?;
    let tx_count = u64_env_alias(
        &[
            "NOVOVM_NATIVE_PIPELINE_TX_COUNT",
            "NOVOVM_NATIVE_PIPELINE_CROSS_MACHINE_TX_COUNT",
        ],
        if sustained_enabled { 256 } else { 32 },
    )?
    .max(1);
    let batch_budget = u64_env_alias(
        &[
            "NOVOVM_NATIVE_PIPELINE_BATCH_BUDGET",
            "NOVOVM_NATIVE_PIPELINE_CROSS_MACHINE_BATCH_BUDGET",
        ],
        if sustained_enabled { 32 } else { 8 },
    )?
    .max(1);
    let recv_budget = u64_env_alias(
        &[
            "NOVOVM_NATIVE_PIPELINE_RECV_BUDGET",
            "NOVOVM_NATIVE_PIPELINE_CROSS_MACHINE_RECV_BUDGET",
        ],
        128,
    )?
    .max(1);
    let tick_interval_ms = u64_env_alias(
        &[
            "NOVOVM_NATIVE_PIPELINE_TICK_INTERVAL_MS",
            "NOVOVM_NATIVE_PIPELINE_CROSS_MACHINE_TICK_INTERVAL_MS",
        ],
        100,
    )?
    .max(1);
    let max_ticks = u64_env_alias(
        &[
            "NOVOVM_NATIVE_PIPELINE_MAX_TICKS",
            "NOVOVM_NATIVE_PIPELINE_CROSS_MACHINE_MAX_TICKS",
        ],
        if role == "receiver" {
            3600
        } else {
            div_ceil_u64(tx_count, batch_budget).saturating_add(180)
        },
    )?
    .max(1);
    let startup_wait_ms = u64_env("NOVOVM_NATIVE_PIPELINE_STARTUP_WAIT_MS", 500)?;
    let fault_binary = current_bin_name_contains("fault");
    let fault_env = env_any(&[
        "NOVOVM_NATIVE_PIPELINE_FAULT_PACKET_LOSS_BPS",
        "NOVOVM_NATIVE_PIPELINE_FAULT_DUPLICATE_BPS",
        "NOVOVM_NATIVE_PIPELINE_FAULT_DELAY_MS",
        "NOVOVM_NATIVE_PIPELINE_FAULT_REORDER_BPS",
        "NOVOVM_NATIVE_PIPELINE_FAULT_SEED",
    ]);
    let fault_enabled = fault_binary || fault_env;
    let fault = FaultConfigV1 {
        enabled: fault_enabled,
        loss_bps: u64_env(
            "NOVOVM_NATIVE_PIPELINE_FAULT_PACKET_LOSS_BPS",
            if fault_enabled { 200 } else { 0 },
        )?
        .min(10_000),
        duplicate_bps: u64_env(
            "NOVOVM_NATIVE_PIPELINE_FAULT_DUPLICATE_BPS",
            if fault_enabled { 3000 } else { 0 },
        )?
        .min(10_000),
        delay_ms: u64_env(
            "NOVOVM_NATIVE_PIPELINE_FAULT_DELAY_MS",
            if fault_enabled { 20 } else { 0 },
        )?,
        reorder_bps: u64_env(
            "NOVOVM_NATIVE_PIPELINE_FAULT_REORDER_BPS",
            if fault_enabled { 1000 } else { 0 },
        )?
        .min(10_000),
        seed: u64_env(
            "NOVOVM_NATIVE_PIPELINE_FAULT_SEED",
            if fault_enabled { 123 } else { 0 },
        )?,
    };
    let tx_per_round = u64_env("NOVOVM_NATIVE_PIPELINE_SUSTAINED_TX_PER_ROUND", 32)?.max(1);
    let sustained_rounds = div_ceil_u64(tx_count, tx_per_round).max(1);
    let duration_seconds = u64_env(
        "NOVOVM_NATIVE_PIPELINE_SUSTAINED_DURATION_SECONDS",
        if sustained_enabled { 1800 } else { 0 },
    )?;
    let default_round_interval_ms = if sustained_enabled && sustained_rounds > 1 {
        duration_seconds
            .saturating_mul(1_000)
            .checked_div(sustained_rounds.saturating_sub(1))
            .unwrap_or(0)
    } else {
        0
    };
    let sustained = SustainedConfigV1 {
        enabled: sustained_enabled,
        duration_seconds,
        tx_per_round,
        round_interval_ms: u64_env(
            "NOVOVM_NATIVE_PIPELINE_SUSTAINED_ROUND_INTERVAL_MS",
            default_round_interval_ms,
        )?,
    };
    let tail_repair_enabled = bool_env("NOVOVM_NATIVE_PIPELINE_TAIL_REPAIR_ENABLED")
        || (sustained.enabled
            && string_env_nonempty("NOVOVM_NATIVE_PIPELINE_TAIL_REPAIR_ENABLED").is_none());
    let tail_repair = TailRepairConfigV1 {
        enabled: tail_repair_enabled,
        rounds: u64_env(
            "NOVOVM_NATIVE_PIPELINE_TAIL_REPAIR_ROUNDS",
            if tail_repair_enabled { 3 } else { 0 },
        )?,
        interval_ms: u64_env(
            "NOVOVM_NATIVE_PIPELINE_TAIL_REPAIR_INTERVAL_MS",
            if tail_repair_enabled { 1000 } else { 0 },
        )?,
    };
    let sender_node = u64_env_alias(
        &[
            "NOVOVM_NATIVE_PIPELINE_SENDER_NODE",
            "NOVOVM_NATIVE_PIPELINE_CROSS_MACHINE_SENDER_NODE",
        ],
        9_991_940,
    )?;
    let receiver_node = u64_env_alias(
        &[
            "NOVOVM_NATIVE_PIPELINE_RECEIVER_NODE",
            "NOVOVM_NATIVE_PIPELINE_CROSS_MACHINE_RECEIVER_NODE",
        ],
        9_991_941,
    )?;
    let path = report_path(role.as_str());
    let node_bin = novovm_node_bin();
    let store = store_path(chain_id, role.as_str());
    if matches!(role.as_str(), "receiver" | "local-smoke" | "local_smoke") && !node_bin.exists() {
        bail!(
            "novovm-node binary not found: {}; build with `cargo build -p novovm-node --bins` or set NOVOVM_NATIVE_PIPELINE_NODE_BIN",
            node_bin.display()
        );
    }

    let report = match role.as_str() {
        "receiver" => {
            let listen_addr = first_string_env_nonempty(&[
                "NOVOVM_NATIVE_PIPELINE_LISTEN_ADDR",
                "NOVOVM_NATIVE_PIPELINE_RECEIVER_LISTEN_ADDR",
            ])
            .unwrap_or_else(|| "0.0.0.0:39001".to_string());
            run_receiver(
                chain_id,
                tx_count,
                receiver_node,
                listen_addr.as_str(),
                node_bin.as_path(),
                store.as_path(),
                max_ticks,
                tick_interval_ms,
                batch_budget,
                recv_budget,
                sustained,
            )?
        }
        "sender" => {
            let receiver_addr = first_string_env_nonempty(&[
                "NOVOVM_NATIVE_PIPELINE_RECEIVER_ADDR",
                "NOVOVM_NATIVE_PIPELINE_PEER_ADDR",
            ])
            .ok_or_else(|| {
                anyhow::anyhow!(
                    "sender role requires NOVOVM_NATIVE_PIPELINE_RECEIVER_ADDR=host:port"
                )
            })?;
            let sender_addr = first_string_env_nonempty(&[
                "NOVOVM_NATIVE_PIPELINE_LISTEN_ADDR",
                "NOVOVM_NATIVE_PIPELINE_SENDER_LISTEN_ADDR",
            ])
            .unwrap_or_else(|| "0.0.0.0:0".to_string());
            run_sender(
                chain_id,
                tx_count,
                sender_node,
                receiver_node,
                sender_addr.as_str(),
                receiver_addr.as_str(),
                fault,
                sustained,
                tail_repair,
            )?
        }
        "local-smoke" | "local_smoke" => run_local_smoke(
            chain_id,
            tx_count,
            sender_node,
            receiver_node,
            node_bin.as_path(),
            store.as_path(),
            max_ticks,
            tick_interval_ms,
            batch_budget,
            recv_budget,
            startup_wait_ms,
            fault,
            sustained,
            tail_repair,
        )?,
        other => bail!("unknown NOVOVM_NATIVE_PIPELINE_ROLE: {other}"),
    };
    write_report(path.as_path(), &report)?;
    println!(
        "{}",
        serde_json::to_string_pretty(&report).context("encode cross-machine report failed")?
    );
    if !report
        .get("accepted")
        .and_then(Value::as_bool)
        .unwrap_or(false)
    {
        bail!("cross-machine UDP soak failed: {}", path.display());
    }
    Ok(())
}

fn compact_tx_hash_array_value(value: &Value) -> Value {
    let Some(items) = value.as_array() else {
        return value.clone();
    };
    let mut digest = RollingDigestV1::default();
    let mut first = Vec::<Value>::new();
    let mut last = Vec::<Value>::new();
    for item in items {
        if let Some(raw) = item.as_str() {
            digest.update(raw.as_bytes());
        }
        if first.len() < 8 {
            first.push(item.clone());
        }
    }
    let start = items.len().saturating_sub(8);
    for item in items.iter().skip(start) {
        last.push(item.clone());
    }
    serde_json::json!({
        "omitted": true,
        "count": items.len(),
        "digest": digest.finish_hex(),
        "first_samples": first,
        "last_samples": last,
    })
}

fn report_array_len_recursive(value: &Value) -> usize {
    match value {
        Value::Array(items) => {
            items.len() + items.iter().map(report_array_len_recursive).sum::<usize>()
        }
        Value::Object(map) => map.values().map(report_array_len_recursive).sum(),
        _ => 0,
    }
}

#[derive(Debug, Default)]
struct RollingDigestV1 {
    state: u64,
    count: u64,
}

impl RollingDigestV1 {
    fn update(&mut self, bytes: &[u8]) {
        const FNV_OFFSET: u64 = 0xcbf29ce484222325;
        const FNV_PRIME: u64 = 0x100000001b3;
        if self.count == 0 && self.state == 0 {
            self.state = FNV_OFFSET;
        }
        for byte in bytes {
            self.state ^= u64::from(*byte);
            self.state = self.state.wrapping_mul(FNV_PRIME);
        }
        self.count = self.count.saturating_add(1);
    }

    fn finish_hex(&self) -> String {
        format!("{:016x}:{}", self.state, self.count)
    }
}

fn compact_probe_for_report(mut probe: Value) -> Value {
    if let Some(map) = probe.as_object_mut() {
        if let Some(value) = map.get("receipt_hashes").cloned() {
            map.insert(
                "receipt_hashes".to_string(),
                compact_tx_hash_array_value(&value),
            );
        }
    }
    probe
}

fn compact_receiver_summary_for_report(summary: Value) -> Value {
    serde_json::json!({
        "accepted": summary.get("accepted").cloned().unwrap_or(Value::Null),
        "execution_kernel": summary.get("execution_kernel").cloned().unwrap_or(Value::Null),
        "aoem_concurrency_owner": summary.get("aoem_concurrency_owner").cloned().unwrap_or(Value::Null),
        "host_concurrency_policy": summary.get("host_concurrency_policy").cloned().unwrap_or(Value::Null),
        "ticks": summary_u64(&summary, "ticks"),
        "elapsed_ms": summary_u64(&summary, "elapsed_ms"),
        "ticks_per_sec_x1000": summary_u64(&summary, "ticks_per_sec_x1000"),
        "progress_score": summary_u64(&summary, "progress_score"),
        "aoem_executed_total": summary_u64(&summary, "aoem_executed_total"),
        "aoem_deferred_total": summary_u64(&summary, "aoem_deferred_total"),
        "included_canonical_total": summary_u64(&summary, "included_canonical_total"),
        "included_canonical_last": summary_u64(&summary, "included_canonical_last"),
        "ingress_total_last": summary_u64(&summary, "ingress_total_last"),
        "queue_tx_count_last": summary_u64(&summary, "queue_tx_count_last"),
        "queue_active_pending_last": summary_u64(&summary, "queue_active_pending_last"),
        "queue_historical_pending_last": summary_u64(&summary, "queue_historical_pending_last"),
        "queue_seen_last": summary_u64(&summary, "queue_seen_last"),
        "queue_pending_last": summary_u64(&summary, "queue_pending_last"),
        "queue_dropped_last": summary_u64(&summary, "queue_dropped_last"),
        "queue_rejected_last": summary_u64(&summary, "queue_rejected_last"),
        "historical_compacted_total": summary_u64(&summary, "historical_compacted_total"),
        "historical_payload_bytes_freed": summary_u64(&summary, "historical_payload_bytes_freed"),
        "tombstone_retained_count": summary_u64(&summary, "tombstone_retained_count"),
        "tombstone_evicted_count": summary_u64(&summary, "tombstone_evicted_count"),
        "historical_pending_after_compaction": summary_u64(&summary, "historical_pending_after_compaction"),
        "included_retained_after_compaction": summary_u64(&summary, "included_retained_after_compaction"),
        "dropped_retained_after_compaction": summary_u64(&summary, "dropped_retained_after_compaction"),
        "runtime_current_view_bytes_estimate": summary_u64(&summary, "queue_tx_count_last").saturating_mul(256),
        "broadcast_tx_last": summary_u64(&summary, "broadcast_tx_last"),
        "broadcast_candidates_last": summary_u64(&summary, "broadcast_candidates_last"),
        "skipped_ineligible_stage_total": summary_u64(&summary, "skipped_ineligible_stage_total"),
        "skipped_missing_payload_total": summary_u64(&summary, "skipped_missing_payload_total"),
        "skipped_non_native_payload_total": summary_u64(&summary, "skipped_non_native_payload_total"),
        "skipped_chain_mismatch_total": summary_u64(&summary, "skipped_chain_mismatch_total"),
        "skipped_already_receipted_total": summary_u64(&summary, "skipped_already_receipted_total"),
        "max_network_received_per_tick": summary_u64(&summary, "max_network_received_per_tick"),
        "max_queue_admitted_per_tick": summary_u64(&summary, "max_queue_admitted_per_tick"),
        "max_aoem_batch_executed_per_tick": summary_u64(&summary, "max_aoem_batch_executed_per_tick"),
        "max_proof_items_per_tick": summary_u64(&summary, "max_proof_items_per_tick"),
        "max_commit_items_per_tick": summary_u64(&summary, "max_commit_items_per_tick"),
        "max_broadcast_tx_per_tick": summary_u64(&summary, "max_broadcast_tx_per_tick"),
        "native_store_backend": summary.get("native_store_backend").cloned().unwrap_or(Value::Null),
        "native_store_commit_model": summary.get("native_store_commit_model").cloned().unwrap_or(Value::Null),
        "native_store_backend_path": summary.get("native_store_backend_path").cloned().unwrap_or(Value::Null),
        "native_store_precommit_materialized_ticks": summary_u64(&summary, "native_store_precommit_materialized_ticks"),
        "native_store_materialized_receipts_max": summary_u64(&summary, "native_store_materialized_receipts_max"),
        "native_store_materialized_estimated_bytes_max": summary_u64(&summary, "native_store_materialized_estimated_bytes_max"),
        "native_store_previous_clone_receipts_max": summary_u64(&summary, "native_store_previous_clone_receipts_max"),
        "native_store_previous_clone_estimated_bytes_max": summary_u64(&summary, "native_store_previous_clone_estimated_bytes_max"),
        "native_store_materialization_risk_last": summary.get("native_store_materialization_risk_last").cloned().unwrap_or(Value::Null),
        "report_tx_hash_list_len": report_array_len_recursive(&summary),
        "report_receipt_key_list_len": 0,
        "tick_result_omitted": summary.get("tick_result").is_some(),
        "lifecycle_omitted": summary.get("lifecycle").is_some(),
        "raw_runtime_summary_omitted": true,
    })
}

fn compact_sender_report_for_report(mut report: Value) -> Value {
    if let Some(map) = report.as_object_mut() {
        if let Some(sent_by_hash) = map.remove("sent_by_hash") {
            let count = sent_by_hash.as_object().map_or(0, serde_json::Map::len);
            let mut digest = RollingDigestV1::default();
            let mut first = Vec::<Value>::new();
            if let Some(obj) = sent_by_hash.as_object() {
                for (key, value) in obj {
                    digest.update(key.as_bytes());
                    if first.len() < 8 {
                        first.push(serde_json::json!({"tx_hash": key, "count": value}));
                    }
                }
            }
            map.insert(
                "sent_by_hash".to_string(),
                serde_json::json!({
                    "omitted": true,
                    "count": count,
                    "digest": digest.finish_hex(),
                    "samples": first,
                }),
            );
        }
    }
    report
}

fn hex_lower(bytes: &[u8]) -> String {
    let mut out = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        out.push_str(&format!("{byte:02x}"));
    }
    out
}
