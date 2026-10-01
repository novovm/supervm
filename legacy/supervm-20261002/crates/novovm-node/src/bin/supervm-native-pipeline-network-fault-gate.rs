#![forbid(unsafe_code)]

use anyhow::{bail, Context, Result};
use novovm_network::{
    build_evm_native_transaction_frame_auth_v1, EvmNativeTransactionFrameAuthInputV1, Transport,
    UdpTransport,
};
use novovm_node::tx_ingress::{
    get_nov_native_execution_store_recovery_probe_v1, nov_native_tx_to_adapter_tx_ir_v1,
    sign_nov_native_tx_with_seed_v1,
};
use novovm_protocol::{
    encode_nov_native_tx_wire_v1, EvmNativeMessage, NodeId, NovExecuteTxV1, NovExecutionModeV1,
    NovExecutionPolicyV1, NovExecutionTargetV1, NovFeePolicyV1, NovNativeTxWireV1,
    NovPrivacyModeV1, NovTxKindV1, NovVerificationModeV1, ProtocolMessage,
};
use serde_json::Value;
use sha2::{Digest, Sha256};
use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::net::UdpSocket;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Output, Stdio};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

const REPORT_SCHEMA_V1: &str = "novovm-native-pipeline-network-fault-injection-report/v1";

#[derive(Debug, Clone)]
struct NativePacketV1 {
    index: u64,
    copy_index: u64,
    tx_hash: [u8; 32],
    payload: Vec<u8>,
    dropped: bool,
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

fn div_ceil_u64(value: u64, divisor: u64) -> u64 {
    if divisor == 0 {
        return 0;
    }
    value.saturating_add(divisor).saturating_sub(1) / divisor
}

fn native_fixture_signing_seed_v1(chain_id: u64, fixture_identity: u64) -> [u8; 32] {
    let mut hasher = Sha256::new();
    hasher.update(b"novovm-native-fixture-signing-seed/v1");
    hasher.update(chain_id.to_le_bytes());
    hasher.update(fixture_identity.to_le_bytes());
    hasher.finalize().into()
}

fn reserve_udp_addr() -> Result<String> {
    let socket = UdpSocket::bind("127.0.0.1:0").context("reserve udp addr failed")?;
    Ok(socket
        .local_addr()
        .context("read reserved udp addr failed")?
        .to_string())
}

fn temp_store_path(chain_id: u64) -> PathBuf {
    std::env::temp_dir().join(format!(
        "novovm-native-pipeline-network-fault-{chain_id}-{}-{}.json",
        std::process::id(),
        unix_ms_now()
    ))
}

fn default_report_path() -> PathBuf {
    PathBuf::from("artifacts/native-pipeline/native-pipeline-network-fault-injection-report.json")
}

fn report_path() -> PathBuf {
    string_env_nonempty("NOVOVM_NATIVE_PIPELINE_FAULT_REPORT_PATH")
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

fn build_native_payloads(chain_id: u64, count: u64) -> Result<Vec<NativePacketV1>> {
    let mut out = Vec::with_capacity(count as usize);
    for index in 0..count {
        let fixture_identity = index.saturating_add(1);
        let mut tx = NovNativeTxWireV1 {
            chain_id,
            kind: NovTxKindV1::Execute(NovExecuteTxV1 {
                caller: Vec::new(),
                account_id: None,
                fee_owner_account_id: None,
                nonce_owner_account_id: None,
                target: NovExecutionTargetV1::NativeModule("treasury".to_string()),
                method: "deposit_reserve".to_string(),
                args: serde_json::to_vec(&serde_json::json!({
                    "asset": "USDT",
                    "amount": fixture_identity,
                }))
                .context("encode network fault fixture args failed")?,
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
                nonce: 0,
            }),
            signature: Vec::new(),
        };
        sign_nov_native_tx_with_seed_v1(
            &mut tx,
            native_fixture_signing_seed_v1(chain_id, fixture_identity),
        )?;
        let ir = nov_native_tx_to_adapter_tx_ir_v1(&tx)?;
        let mut tx_hash = [0u8; 32];
        let copy_len = ir.hash.len().min(32);
        tx_hash[..copy_len].copy_from_slice(&ir.hash[..copy_len]);
        let payload = encode_nov_native_tx_wire_v1(&tx)
            .map_err(|err| anyhow::anyhow!("encode native network fault tx failed: {err}"))?;
        out.push(NativePacketV1 {
            index,
            copy_index: 0,
            tx_hash,
            payload,
            dropped: false,
        });
    }
    Ok(out)
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
    base: &[NativePacketV1],
    loss_bps: u64,
    duplicate_bps: u64,
    reorder_bps: u64,
    seed: u64,
) -> Vec<NativePacketV1> {
    let duplicate_all = duplicate_bps >= 10_000;
    let mut scheduled = Vec::with_capacity(base.len().saturating_mul(2));
    for packet in base {
        let mut first = packet.clone();
        first.copy_index = 0;
        first.dropped = loss_roll_bps(seed, first.index, first.copy_index) < loss_bps.min(10_000);
        scheduled.push(first);
        let duplicate_this =
            duplicate_all || loss_roll_bps(seed ^ 0xa11c_e55d, packet.index, 1) < duplicate_bps;
        if duplicate_this {
            let mut dup = packet.clone();
            dup.copy_index = 1;
            dup.dropped = loss_roll_bps(seed, dup.index, dup.copy_index) < loss_bps.min(10_000);
            scheduled.push(dup);
        }
    }
    if reorder_bps > 0 {
        let chunk = if reorder_bps >= 10_000 { 4 } else { 8 };
        for part in scheduled.chunks_mut(chunk) {
            part.reverse();
        }
    }
    scheduled
}

struct ReceiverSpawnInput<'a> {
    node_bin: &'a Path,
    chain_id: u64,
    receiver_node: u64,
    sender_node: u64,
    receiver_addr: &'a str,
    sender_addr: &'a str,
    store_path: &'a Path,
    progress_report_path: &'a Path,
    control_frame_auth_key: &'a str,
    run_id: &'a str,
    receiver_ticks: u64,
    tick_interval_ms: u64,
    batch_budget: u64,
    recv_budget: u64,
    expected_unique: u64,
}

fn spawn_receiver(input: ReceiverSpawnInput<'_>) -> Result<Child> {
    let ReceiverSpawnInput {
        node_bin,
        chain_id,
        receiver_node,
        sender_node,
        receiver_addr,
        sender_addr,
        store_path,
        progress_report_path,
        control_frame_auth_key,
        run_id,
        receiver_ticks,
        tick_interval_ms,
        batch_budget,
        recv_budget,
        expected_unique,
    } = input;
    let aoem_persistence_path = store_path.with_extension("aoem-persistence");
    let aoem_owned_state_db_path = store_path.with_extension("aoem-owned.rocksdb");
    let mut cmd = Command::new(node_bin);
    cmd.env_clear();
    for (key, value) in std::env::vars() {
        if key == "NOVOVM_NODE_MODE"
            || key == "NOVOVM_NATIVE_SEND_RAW_TRANSACTION_PIPELINE_ONLY"
            || key.starts_with("NOVOVM_NATIVE_EXECUTION_")
            || key.starts_with("NOVOVM_NATIVE_PIPELINE_")
            || key.starts_with("NOVOVM_NOVORUDP_")
            || key.starts_with("NOVOVM_NETWORK_")
        {
            continue;
        }
        cmd.env(key, value);
    }
    let execution_ticks = div_ceil_u64(expected_unique, batch_budget.max(1)).max(1);
    let envs = [
        ("NOVOVM_NODE_MODE", "native_execution_pipeline".to_string()),
        (
            "NOVOVM_NATIVE_EXECUTION_TICK_CHAIN_ID",
            chain_id.to_string(),
        ),
        (
            "NOVOVM_NATIVE_EXECUTION_TICK_MAX_TICKS",
            receiver_ticks.to_string(),
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
            "NOVOVM_NATIVE_EXECUTION_STORE",
            store_path.display().to_string(),
        ),
        (
            "NOVOVM_NATIVE_EXECUTION_STORE_BACKEND",
            "rocksdb".to_string(),
        ),
        (
            "NOVOVM_NATIVE_EXECUTION_PIPELINE_PROGRESS_REPORT_PATH",
            progress_report_path.display().to_string(),
        ),
        (
            "NOVOVM_NATIVE_EXECUTION_PIPELINE_PROGRESS_REPORT_INTERVAL_MS",
            "1".to_string(),
        ),
        (
            "NOVOVM_NATIVE_SEND_RAW_TRANSACTION_PIPELINE_ONLY",
            "true".to_string(),
        ),
        (
            "NOVOVM_NATIVE_EXECUTION_PIPELINE_REQUIRE_ROCKSDB_STORE",
            "true".to_string(),
        ),
        (
            "NOVOVM_NATIVE_EXECUTION_PIPELINE_UDP_ENABLED",
            "true".to_string(),
        ),
        ("NOVOVM_NATIVE_PIPELINE_TRANSPORT", "novorudp".to_string()),
        (
            "NOVOVM_NATIVE_EXECUTION_PIPELINE_UDP_LISTEN_ADDR",
            receiver_addr.to_string(),
        ),
        (
            "NOVOVM_NATIVE_EXECUTION_PIPELINE_LOCAL_NODE",
            receiver_node.to_string(),
        ),
        (
            "NOVOVM_NATIVE_EXECUTION_PIPELINE_UDP_PEERS",
            format!("{sender_node}={sender_addr}"),
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
            "NOVOVM_NOVORUDP_CONTROL_FRAME_AUTH_KEY",
            control_frame_auth_key.to_string(),
        ),
        (
            "NOVOVM_NOVORUDP_CONTROL_FRAME_AUTH_REQUIRED",
            "true".to_string(),
        ),
        ("NOVOVM_NOVORUDP_RUN_ID", run_id.to_string()),
        (
            "NOVOVM_NETWORK_CONTROL_FRAME_AUTH_KEY",
            control_frame_auth_key.to_string(),
        ),
        (
            "NOVOVM_NETWORK_CONTROL_FRAME_AUTH_REQUIRED",
            "true".to_string(),
        ),
        ("NOVOVM_NETWORK_RUN_ID", run_id.to_string()),
        (
            "NOVOVM_NOVORUDP_SOURCE_PINNING_ENABLED",
            "false".to_string(),
        ),
        ("NOVOVM_NETWORK_SOURCE_PINNING_ENABLED", "false".to_string()),
        (
            "NOVOVM_NOVORUDP_SOURCE_PINNING_REQUIRED",
            "false".to_string(),
        ),
        (
            "NOVOVM_NETWORK_SOURCE_PINNING_REQUIRED",
            "false".to_string(),
        ),
        (
            "NOVOVM_NOVORUDP_ENDPOINT_RECORD_REQUIRED",
            "false".to_string(),
        ),
        (
            "NOVOVM_NETWORK_ENDPOINT_RECORD_REQUIRED",
            "false".to_string(),
        ),
        ("NOVOVM_NOVORUDP_SOURCE_REBIND_ALLOWED", "false".to_string()),
        ("NOVOVM_NETWORK_SOURCE_REBIND_ALLOWED", "false".to_string()),
        (
            "NOVOVM_NOVORUDP_ADAPTIVE_ENDPOINT_ENABLED",
            "false".to_string(),
        ),
        (
            "NOVOVM_NETWORK_ADAPTIVE_ENDPOINT_ENABLED",
            "false".to_string(),
        ),
        (
            "NOVOVM_NOVORUDP_RECEIVER_RATE_LIMIT_ENABLED",
            "false".to_string(),
        ),
        (
            "NOVOVM_NETWORK_RECEIVER_RATE_LIMIT_ENABLED",
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
            "0".to_string(),
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
            format!("network-fault-chain-{chain_id}"),
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
    for (key, value) in envs {
        cmd.env(key, value);
    }
    cmd.stdout(Stdio::piped()).stderr(Stdio::piped());
    cmd.spawn().with_context(|| {
        format!(
            "spawn network fault receiver failed: bin={} addr={receiver_addr}",
            node_bin.display()
        )
    })
}

fn wait_receiver_ready(
    child: &mut Child,
    progress_report_path: &Path,
    timeout_ms: u64,
) -> Result<()> {
    let started_at = Instant::now();
    loop {
        if let Ok(encoded) = fs::read(progress_report_path) {
            if serde_json::from_slice::<Value>(&encoded)
                .ok()
                .and_then(|report| {
                    report
                        .get("schema")
                        .and_then(Value::as_str)
                        .map(str::to_owned)
                })
                .as_deref()
                == Some("novovm-native-execution-pipeline-progress-report/v1")
            {
                return Ok(());
            }
        }
        if let Some(status) = child
            .try_wait()
            .context("poll network fault receiver process failed")?
        {
            bail!("network fault receiver exited before readiness: status={status}");
        }
        if started_at.elapsed() >= Duration::from_millis(timeout_ms.max(1)) {
            bail!(
                "network fault receiver readiness timed out after {}ms: progress_report={}",
                timeout_ms,
                progress_report_path.display()
            );
        }
        std::thread::sleep(Duration::from_millis(10));
    }
}

fn parse_summary(output: Output) -> Result<Value> {
    if !output.status.success() {
        bail!(
            "network fault receiver failed: status={} stdout={} stderr={}",
            output.status,
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        );
    }
    serde_json::from_slice::<Value>(&output.stdout).with_context(|| {
        format!(
            "network fault receiver did not return JSON summary: stdout={} stderr={}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        )
    })
}

fn require_u64_field(summary: &Value, field: &str, violations: &mut Vec<String>) -> u64 {
    match summary.get(field) {
        Some(value) => match value.as_u64() {
            Some(value) => value,
            None => {
                violations.push(format!("{field} must be present as u64"));
                0
            }
        },
        None => {
            violations.push(format!("{field} is missing"));
            0
        }
    }
}

fn summary_str<'a>(summary: &'a Value, field: &str) -> &'a str {
    summary.get(field).and_then(Value::as_str).unwrap_or("-")
}

fn require_aoem_owned_production_v1(summary: &Value, violations: &mut Vec<String>) {
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
    for field in ["tx_ingress_selected_path", "tx_ingress_production_target"] {
        if summary_str(summary, field) != "aoem_runtime_owned_state_persistence" {
            violations.push(format!(
                "{field}={} expected aoem_runtime_owned_state_persistence",
                summary_str(summary, field)
            ));
        }
    }
}

fn write_report(path: &Path, report: &Value) -> Result<()> {
    if let Some(parent) = path.parent() {
        if !parent.as_os_str().is_empty() {
            fs::create_dir_all(parent).with_context(|| {
                format!("create network fault report dir: {}", parent.display())
            })?;
        }
    }
    let encoded =
        serde_json::to_string_pretty(report).context("encode network fault report failed")?;
    fs::write(path, encoded)
        .with_context(|| format!("write network fault report failed: {}", path.display()))
}

fn main() -> Result<()> {
    let chain_id = u64_env("NOVOVM_NATIVE_PIPELINE_FAULT_CHAIN_ID", 9_998_900)?;
    let tx_count = u64_env("NOVOVM_NATIVE_PIPELINE_FAULT_TX_COUNT", 32)?.max(1);
    let batch_budget = u64_env("NOVOVM_NATIVE_PIPELINE_FAULT_BATCH_BUDGET", 8)?.max(1);
    let recv_budget = u64_env("NOVOVM_NATIVE_PIPELINE_FAULT_RECV_BUDGET", 64)?.max(1);
    let tick_interval_ms = u64_env("NOVOVM_NATIVE_PIPELINE_FAULT_TICK_INTERVAL_MS", 10)?.max(1);
    let startup_wait_ms = u64_env("NOVOVM_NATIVE_PIPELINE_FAULT_STARTUP_WAIT_MS", 10_000)?;
    let delay_ms = u64_env("NOVOVM_NATIVE_PIPELINE_FAULT_DELAY_MS", 1)?;
    let loss_bps = u64_env("NOVOVM_NATIVE_PIPELINE_FAULT_PACKET_LOSS_BPS", 500)?.min(10_000);
    let duplicate_bps = u64_env("NOVOVM_NATIVE_PIPELINE_FAULT_DUPLICATE_BPS", 10_000)?;
    let reorder_bps = u64_env("NOVOVM_NATIVE_PIPELINE_FAULT_REORDER_BPS", 10_000)?;
    let seed = u64_env("NOVOVM_NATIVE_PIPELINE_FAULT_SEED", 0x5eed_2026)?;
    let max_unique_loss = u64_env("NOVOVM_NATIVE_PIPELINE_FAULT_MAX_UNIQUE_LOSS", 4)?;
    let sender_node = u64_env("NOVOVM_NATIVE_PIPELINE_FAULT_SENDER_NODE", 9_991_900)?;
    let receiver_node = u64_env("NOVOVM_NATIVE_PIPELINE_FAULT_RECEIVER_NODE", 9_991_901)?;
    let sender_addr = reserve_udp_addr()?;
    let receiver_addr = reserve_udp_addr()?;
    let store_path = temp_store_path(chain_id);
    let gate_instance = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos();
    let control_frame_auth_key =
        format!("novovm-network-fault-gate-key-{chain_id}-{gate_instance}");
    let run_id = format!("novovm-network-fault-gate-run-{chain_id}-{gate_instance}");
    let progress_report_path = store_path.with_extension("progress.json");
    let node_bin = novovm_node_bin();
    if !node_bin.exists() {
        bail!(
            "novovm-node binary not found: {}; build with `cargo build -p novovm-node --bins` or set NOVOVM_NATIVE_PIPELINE_NODE_BIN",
            node_bin.display()
        );
    }

    let base_packets = build_native_payloads(chain_id, tx_count)?;
    let schedule = apply_fault_schedule(
        base_packets.as_slice(),
        loss_bps,
        duplicate_bps,
        reorder_bps,
        seed,
    );
    let mut delivered_unique = BTreeSet::<String>::new();
    let mut sent_packets = 0u64;
    let mut dropped_packets = 0u64;
    let mut sent_by_hash = BTreeMap::<String, u64>::new();
    for packet in &schedule {
        if packet.dropped {
            dropped_packets = dropped_packets.saturating_add(1);
        } else {
            let hash = hex_lower(&packet.tx_hash);
            delivered_unique.insert(hash.clone());
            *sent_by_hash.entry(hash).or_default() += 1;
            sent_packets = sent_packets.saturating_add(1);
        }
    }
    let delivered_unique_count = delivered_unique.len() as u64;
    let duplicate_sent_packets = sent_packets.saturating_sub(delivered_unique_count);
    let unique_loss = tx_count.saturating_sub(delivered_unique_count);
    let send_window_ticks = div_ceil_u64(sent_packets.saturating_mul(delay_ms), tick_interval_ms);
    let receiver_ticks = u64_env(
        "NOVOVM_NATIVE_PIPELINE_FAULT_RECEIVER_TICKS",
        div_ceil_u64(delivered_unique_count, batch_budget)
            .saturating_add(send_window_ticks)
            .saturating_add(delivered_unique_count.saturating_mul(2).max(64)),
    )?;

    let mut receiver = spawn_receiver(ReceiverSpawnInput {
        node_bin: node_bin.as_path(),
        chain_id,
        receiver_node,
        sender_node,
        receiver_addr: receiver_addr.as_str(),
        sender_addr: sender_addr.as_str(),
        store_path: store_path.as_path(),
        progress_report_path: progress_report_path.as_path(),
        control_frame_auth_key: control_frame_auth_key.as_str(),
        run_id: run_id.as_str(),
        receiver_ticks,
        tick_interval_ms,
        batch_budget,
        recv_budget,
        expected_unique: delivered_unique_count,
    })?;
    wait_receiver_ready(
        &mut receiver,
        progress_report_path.as_path(),
        startup_wait_ms,
    )?;

    let sender = UdpTransport::bind_for_chain(NodeId(sender_node), sender_addr.as_str(), chain_id)
        .with_context(|| format!("bind fault sender UDP failed: {sender_addr}"))?;
    sender
        .register_peer(NodeId(receiver_node), receiver_addr.as_str())
        .with_context(|| format!("register receiver peer failed: {receiver_addr}"))?;

    for packet in &schedule {
        if packet.dropped {
            continue;
        }
        // Fault-injected duplicate copies are not explicit repair-window
        // frames. Their copy index remains authenticated without changing the
        // transaction count or invoking repair admission semantics.
        let tx_count = 1;
        let msg = ProtocolMessage::EvmNative(EvmNativeMessage::Transactions {
            from: NodeId(sender_node),
            chain_id,
            tx_hash: packet.tx_hash,
            tx_count,
            payload: packet.payload.clone(),
            transport_auth: Some(build_evm_native_transaction_frame_auth_v1(
                EvmNativeTransactionFrameAuthInputV1 {
                    from: NodeId(sender_node),
                    chain_id,
                    tx_hash: &packet.tx_hash,
                    tx_count,
                    payload: packet.payload.as_slice(),
                    frame_kind: "primary",
                    run_id: run_id.as_str(),
                    sequence: packet.index,
                    copy_index: packet.copy_index,
                    key: control_frame_auth_key.as_str(),
                },
            )),
        });
        sender.send(NodeId(receiver_node), msg).with_context(|| {
            format!(
                "send packet index={} copy={} failed",
                packet.index, packet.copy_index
            )
        })?;
        if delay_ms > 0 {
            std::thread::sleep(std::time::Duration::from_millis(delay_ms));
        }
    }

    let receiver_summary = parse_summary(
        receiver
            .wait_with_output()
            .context("wait network fault receiver failed")?,
    )?;
    std::env::set_var("NOVOVM_NATIVE_EXECUTION_STORE_BACKEND", "rocksdb");
    std::env::set_var(
        "AOEM_PERSISTENCE_PATH",
        store_path.with_extension("aoem-persistence"),
    );
    std::env::set_var(
        "NOVOVM_AOEM_OWNED_STATE_DB_PATH",
        store_path.with_extension("aoem-owned.rocksdb"),
    );
    std::env::set_var(
        "NOVOVM_AOEM_STATE_NAMESPACE",
        format!("network-fault-chain-{chain_id}"),
    );
    let recovery_probe = get_nov_native_execution_store_recovery_probe_v1(store_path.as_path())?;
    let mut violations = Vec::<String>::new();
    let included = require_u64_field(
        &receiver_summary,
        "included_canonical_total",
        &mut violations,
    );
    let aoem_executed_total =
        require_u64_field(&receiver_summary, "aoem_executed_total", &mut violations);
    let queue_pending_last =
        require_u64_field(&receiver_summary, "queue_pending_last", &mut violations);
    let queue_dropped_last =
        require_u64_field(&receiver_summary, "queue_dropped_last", &mut violations);
    let queue_rejected_last =
        require_u64_field(&receiver_summary, "queue_rejected_last", &mut violations);
    let network_received_total =
        require_u64_field(&receiver_summary, "network_received_total", &mut violations);
    let receiver_socket_recv_count = require_u64_field(
        &receiver_summary,
        "receiver_udp_packet_recv_count",
        &mut violations,
    );
    let receiver_decode_attempt_count = require_u64_field(
        &receiver_summary,
        "receiver_udp_packet_decode_attempt_count",
        &mut violations,
    );
    let receiver_decode_ok_count = require_u64_field(
        &receiver_summary,
        "receiver_udp_packet_decode_ok_count",
        &mut violations,
    );
    let receiver_data_frame_decode_ok_count = require_u64_field(
        &receiver_summary,
        "native_receiver_data_frame_decode_ok_count",
        &mut violations,
    );
    let observed_duplicate_received = network_received_total.saturating_sub(included);
    let duplicate_canonical_included = included.saturating_sub(delivered_unique_count);
    let semantic_sequence = recovery_probe
        .get("semantic_head")
        .and_then(|value| value.get("sequence"))
        .and_then(Value::as_u64)
        .unwrap_or_default();
    let semantic_head_monotonic = recovery_probe
        .get("semantic_head_current_recovered")
        .and_then(Value::as_bool)
        == Some(true)
        && recovery_probe
            .get("semantic_head_by_height_recovered")
            .and_then(Value::as_bool)
            == Some(true)
        && semantic_sequence >= included;
    let receipt_index_consistent = recovery_probe
        .get("receipt_index_recovered")
        .and_then(Value::as_bool)
        == Some(true)
        && recovery_probe
            .get("receipt_count")
            .and_then(Value::as_u64)
            .unwrap_or_default()
            == delivered_unique_count;

    if summary_str(&receiver_summary, "execution_kernel") != "AOEM" {
        violations.push(format!(
            "execution_kernel={} expected AOEM",
            summary_str(&receiver_summary, "execution_kernel")
        ));
    }
    if summary_str(&receiver_summary, "aoem_concurrency_owner") != "AOEM_runtime" {
        violations.push(format!(
            "aoem_concurrency_owner={} expected AOEM_runtime",
            summary_str(&receiver_summary, "aoem_concurrency_owner")
        ));
    }
    if summary_str(&receiver_summary, "host_concurrency_policy")
        != "host_drives_lifecycle_only_no_rust_execution_scheduler"
    {
        violations.push(format!(
            "host_concurrency_policy={} expected host lifecycle only",
            summary_str(&receiver_summary, "host_concurrency_policy")
        ));
    }
    require_aoem_owned_production_v1(&receiver_summary, &mut violations);
    if delivered_unique_count < tx_count.saturating_sub(max_unique_loss) {
        violations.push(format!(
            "received_unique={delivered_unique_count} below budgeted minimum {}",
            tx_count.saturating_sub(max_unique_loss)
        ));
    }
    if duplicate_bps > 0 && observed_duplicate_received == 0 {
        violations.push("duplicate mode enabled but observed_duplicate_received=0".to_string());
    }
    for (field, observed) in [
        ("network_received_total", network_received_total),
        ("receiver_udp_packet_recv_count", receiver_socket_recv_count),
        (
            "receiver_udp_packet_decode_attempt_count",
            receiver_decode_attempt_count,
        ),
        (
            "receiver_udp_packet_decode_ok_count",
            receiver_decode_ok_count,
        ),
        (
            "native_receiver_data_frame_decode_ok_count",
            receiver_data_frame_decode_ok_count,
        ),
    ] {
        if observed != sent_packets {
            violations.push(format!(
                "{field}={observed} expected sent_packets={sent_packets}"
            ));
        }
    }
    if aoem_executed_total != delivered_unique_count {
        violations.push(format!(
            "aoem_executed_total={} expected delivered_unique={delivered_unique_count}",
            aoem_executed_total
        ));
    }
    if included != delivered_unique_count {
        violations.push(format!(
            "included_canonical_total={included} expected delivered_unique={delivered_unique_count}"
        ));
    }
    if duplicate_canonical_included != 0 {
        violations.push(format!(
            "duplicate_canonical_included={duplicate_canonical_included} expected 0"
        ));
    }
    if !semantic_head_monotonic {
        violations.push("semantic_head_monotonic=false".to_string());
    }
    if !receipt_index_consistent {
        violations.push("receipt_index_consistent=false".to_string());
    }
    if queue_pending_last != 0 {
        violations.push(format!(
            "queue_pending_last={} expected 0",
            queue_pending_last
        ));
    }
    if queue_dropped_last != 0 {
        violations.push(format!(
            "queue_dropped_last={} expected 0",
            queue_dropped_last
        ));
    }
    if queue_rejected_last != 0 {
        violations.push(format!(
            "queue_rejected_last={} expected 0",
            queue_rejected_last
        ));
    }
    for field in [
        "receiver_udp_packet_decode_error_count",
        "native_receiver_classifier_drop_count",
        "native_receiver_data_frame_decode_error_count",
        "native_receiver_source_pin_drop_count",
        "native_receiver_auth_drop_count",
        "native_receiver_run_id_mismatch_count",
        "ledger_receipt_proof_missing_sequence_mapping_count",
        "ledger_canonical_proof_missing_sequence_mapping_count",
    ] {
        let observed = require_u64_field(&receiver_summary, field, &mut violations);
        if observed != 0 {
            violations.push(format!("{field}={observed} expected 0"));
        }
    }

    let accepted = violations.is_empty();
    let report = serde_json::json!({
        "schema": REPORT_SCHEMA_V1,
        "method": "supervm_native_pipeline_network_fault_gate",
        "accepted": accepted,
        "chain_id": chain_id,
        "tx_count": tx_count,
        "store_path": store_path,
        "boundaries": {
            "lifecycle_structure": "frozen",
            "execution_kernel": "AOEM",
            "aoem_concurrency_owner": "AOEM_runtime",
            "host_concurrency_policy": "host_drives_lifecycle_only_no_rust_execution_scheduler",
            "product_entry": "pending_only",
            "receipt_state_source": "AOEM_semantic_graph_v3",
            "state_receipt_owner": "AOEM_runtime",
            "host_store_role": "validated_query_and_lifecycle_projection",
            "transport": "novorudp",
            "commit": "aoem_submit_semantic_graph_v3_atomic_persistence",
            "legacy_host_transitional_fallback": false,
            "legacy_canonical_double_write": false,
            "canonical_body_head_recovery": "not_claimed_by_this_gate"
        },
        "fault_injection": {
            "fault_injection_enabled": true,
            "packet_loss_bps": loss_bps,
            "duplicate_bps": duplicate_bps,
            "delay_ms": delay_ms,
            "reorder_bps": reorder_bps,
            "seed": seed,
            "scheduled_packets": schedule.len() as u64,
            "sent_packets": sent_packets,
            "dropped_packets": dropped_packets,
            "tx_unique_total": tx_count,
            "received_unique": delivered_unique_count,
            "unique_loss": unique_loss,
            "max_unique_loss": max_unique_loss,
            "duplicate_sent_packets": duplicate_sent_packets,
            "authenticated_received_packets": network_received_total,
            "duplicate_received": observed_duplicate_received
        },
        "validation": {
            "duplicate_canonical_included": duplicate_canonical_included,
            "semantic_head_monotonic": semantic_head_monotonic,
            "receipt_index_consistent": receipt_index_consistent,
            "receiver_socket_recv_count": receiver_socket_recv_count,
            "receiver_decode_attempt_count": receiver_decode_attempt_count,
            "receiver_decode_ok_count": receiver_decode_ok_count,
            "receiver_data_frame_decode_ok_count": receiver_data_frame_decode_ok_count,
            "network_received_total": network_received_total,
            "queue_pending_last": queue_pending_last,
            "queue_dropped_last": queue_dropped_last,
            "queue_rejected_last": queue_rejected_last
        },
        "receiver_summary": receiver_summary,
        "recovery_probe": recovery_probe,
        "sent_by_hash": sent_by_hash,
        "violations": violations
    });
    let path = report_path();
    write_report(path.as_path(), &report)?;
    println!(
        "{}",
        serde_json::to_string_pretty(&report).context("encode network fault report failed")?
    );
    if !accepted {
        bail!(
            "native pipeline network fault gate failed: {}",
            path.display()
        );
    }
    Ok(())
}

fn hex_lower(bytes: &[u8]) -> String {
    let mut out = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        out.push_str(&format!("{byte:02x}"));
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn required_u64_field_rejects_missing_and_wrong_type() {
        let summary = serde_json::json!({
            "present": 7,
            "wrong_type": "0",
        });
        let mut violations = Vec::new();

        assert_eq!(require_u64_field(&summary, "present", &mut violations), 7);
        assert_eq!(require_u64_field(&summary, "missing", &mut violations), 0);
        assert_eq!(
            require_u64_field(&summary, "wrong_type", &mut violations),
            0
        );
        assert_eq!(
            violations,
            [
                "missing is missing".to_string(),
                "wrong_type must be present as u64".to_string(),
            ]
        );
    }

    #[test]
    fn native_fixture_signers_remain_unique_beyond_single_byte_boundary() {
        let chain_id = 9_998_904;
        let fixtures = build_native_payloads(chain_id, 257).expect("build 257 native fixtures");
        let mut callers = BTreeSet::<Vec<u8>>::new();
        let mut hashes = BTreeSet::<[u8; 32]>::new();

        for fixture in fixtures {
            let tx = novovm_protocol::decode_nov_native_tx_wire_v1(fixture.payload.as_slice())
                .expect("decode signed native fixture");
            let NovTxKindV1::Execute(execute) = tx.kind else {
                panic!("fixture must decode as execute intent");
            };
            assert_eq!(execute.nonce, 0);
            assert_eq!(execute.caller.len(), 20);
            assert!(callers.insert(execute.caller));
            assert!(hashes.insert(fixture.tx_hash));
        }

        assert_eq!(callers.len(), 257);
        assert_eq!(hashes.len(), 257);
    }
}
