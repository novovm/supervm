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
use std::collections::BTreeMap;
use std::fs;
use std::net::UdpSocket;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Output, Stdio};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

const REPORT_SCHEMA_V1: &str = "novovm-native-pipeline-remote-reentry-dedup-report/v1";
const PENDING_POLICY_V1: &str = "volatile";

#[derive(Debug, Clone)]
struct NativeFixtureTxV1 {
    index: u64,
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

fn div_ceil_u64(value: u64, divisor: u64) -> u64 {
    if divisor == 0 {
        return 0;
    }
    value.saturating_add(divisor).saturating_sub(1) / divisor
}

fn should_inherit_receiver_env_v1(key: &str) -> bool {
    key != "NOVOVM_NODE_MODE"
        && key != "NOVOVM_NATIVE_SEND_RAW_TRANSACTION_PIPELINE_ONLY"
        && !key.starts_with("NOVOVM_NATIVE_EXECUTION_")
        && !key.starts_with("NOVOVM_NATIVE_PIPELINE_")
        && !key.starts_with("NOVOVM_NOVORUDP_")
        && !key.starts_with("NOVOVM_NETWORK_")
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
        "novovm-native-pipeline-remote-reentry-{chain_id}-{}-{}.json",
        std::process::id(),
        unix_ms_now()
    ))
}

fn default_report_path() -> PathBuf {
    PathBuf::from("artifacts/native-pipeline/native-pipeline-remote-reentry-dedup-report.json")
}

fn report_path() -> PathBuf {
    string_env_nonempty("NOVOVM_NATIVE_PIPELINE_REMOTE_REENTRY_REPORT_PATH")
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

fn build_native_payloads(chain_id: u64, count: u64) -> Result<Vec<NativeFixtureTxV1>> {
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
                .context("encode remote reentry fixture args failed")?,
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
            .map_err(|err| anyhow::anyhow!("encode remote reentry native tx failed: {err}"))?;
        out.push(NativeFixtureTxV1 {
            index,
            tx_hash,
            payload,
        });
    }
    Ok(out)
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
    expected_execution_count: u64,
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
        expected_execution_count,
    } = input;
    let aoem_persistence_path = store_path.with_extension("aoem-persistence");
    let aoem_owned_state_db_path = store_path.with_extension("aoem-owned.rocksdb");
    let mut cmd = Command::new(node_bin);
    cmd.env_clear();
    for (key, value) in std::env::vars() {
        if !should_inherit_receiver_env_v1(key.as_str()) {
            continue;
        }
        cmd.env(key, value);
    }
    let base_envs = [
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
            if expected_execution_count > 0 {
                "true".to_string()
            } else {
                "false".to_string()
            },
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
            expected_execution_count.to_string(),
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
            format!("remote-reentry-chain-{chain_id}"),
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
    for (key, value) in base_envs {
        cmd.env(key, value);
    }
    if expected_execution_count > 0 {
        let execution_ticks = div_ceil_u64(expected_execution_count, batch_budget.max(1)).max(1);
        cmd.env(
            "NOVOVM_NATIVE_EXECUTION_PIPELINE_MIN_NONEMPTY_AOEM_BATCH_TICKS",
            execution_ticks.to_string(),
        );
        cmd.env(
            "NOVOVM_NATIVE_EXECUTION_PIPELINE_MIN_NONEMPTY_PROOF_TICKS",
            execution_ticks.to_string(),
        );
        cmd.env(
            "NOVOVM_NATIVE_EXECUTION_PIPELINE_MIN_NONEMPTY_COMMIT_TICKS",
            execution_ticks.to_string(),
        );
    }
    cmd.stdout(Stdio::piped()).stderr(Stdio::piped());
    cmd.spawn().with_context(|| {
        format!(
            "spawn remote reentry receiver failed: bin={} addr={receiver_addr}",
            node_bin.display()
        )
    })
}

fn wait_receiver_ready(
    child: &mut Child,
    progress_report_path: &Path,
    timeout_ms: u64,
    label: &str,
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
            .with_context(|| format!("poll {label} receiver process failed"))?
        {
            bail!("{label} receiver exited before readiness: status={status}");
        }
        if started_at.elapsed() >= Duration::from_millis(timeout_ms.max(1)) {
            bail!(
                "{label} receiver readiness timed out after {}ms: progress_report={}",
                timeout_ms,
                progress_report_path.display()
            );
        }
        std::thread::sleep(Duration::from_millis(10));
    }
}

fn progress_u64_field(summary: &Value, field: &str, label: &str) -> Result<u64> {
    summary
        .get(field)
        .with_context(|| format!("{label} progress summary missing {field}"))?
        .as_u64()
        .with_context(|| format!("{label} progress summary {field} must be u64"))
}

fn wait_receiver_delivery_progress(
    child: &mut Child,
    progress_report_path: &Path,
    minimum_received: u64,
    timeout_ms: u64,
    label: &str,
) -> Result<Value> {
    let started_at = Instant::now();
    loop {
        if let Ok(encoded) = fs::read(progress_report_path) {
            if let Ok(report) = serde_json::from_slice::<Value>(&encoded) {
                if report.get("schema").and_then(Value::as_str)
                    == Some("novovm-native-execution-pipeline-progress-report/v1")
                {
                    let summary = report
                        .get("summary")
                        .with_context(|| format!("{label} progress report missing summary"))?;
                    let network_received =
                        progress_u64_field(summary, "network_received_total", label)?;
                    let udp_decode_ok =
                        progress_u64_field(summary, "receiver_udp_packet_decode_ok_count", label)?;
                    let data_frame_decode_ok = progress_u64_field(
                        summary,
                        "native_receiver_data_frame_decode_ok_count",
                        label,
                    )?;
                    let auth_drop =
                        progress_u64_field(summary, "native_receiver_auth_drop_count", label)?;
                    if auth_drop != 0 {
                        bail!(
                            "{label} progress observed native_receiver_auth_drop_count={auth_drop}"
                        );
                    }
                    if network_received >= minimum_received
                        && udp_decode_ok >= minimum_received
                        && data_frame_decode_ok >= minimum_received
                    {
                        return Ok(summary.clone());
                    }
                }
            }
        }
        if let Some(status) = child
            .try_wait()
            .with_context(|| format!("poll {label} receiver delivery progress failed"))?
        {
            bail!(
                "{label} receiver exited before delivery evidence reached {minimum_received}: status={status}"
            );
        }
        if started_at.elapsed() >= Duration::from_millis(timeout_ms.max(1)) {
            bail!(
                "{label} receiver delivery evidence timed out after {}ms: minimum_received={} progress_report={}",
                timeout_ms,
                minimum_received,
                progress_report_path.display()
            );
        }
        std::thread::sleep(Duration::from_millis(10));
    }
}

fn parse_summary(output: Output, label: &str) -> Result<Value> {
    if !output.status.success() {
        bail!(
            "{label} receiver failed: status={} stdout={} stderr={}",
            output.status,
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        );
    }
    serde_json::from_slice::<Value>(&output.stdout).with_context(|| {
        format!(
            "{label} receiver did not return JSON summary: stdout={} stderr={}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        )
    })
}

fn require_u64_field(value: &Value, field: &str, label: &str, violations: &mut Vec<String>) -> u64 {
    match value.get(field) {
        Some(field_value) => match field_value.as_u64() {
            Some(field_value) => field_value,
            None => {
                violations.push(format!("{label}.{field} must be present as u64"));
                0
            }
        },
        None => {
            violations.push(format!("{label}.{field} is missing"));
            0
        }
    }
}

fn summary_str<'a>(summary: &'a Value, field: &str) -> &'a str {
    summary.get(field).and_then(Value::as_str).unwrap_or("-")
}

fn require_semantic_sequence(probe: &Value, label: &str, violations: &mut Vec<String>) -> u64 {
    let Some(semantic_head) = probe.get("semantic_head") else {
        violations.push(format!("{label}.semantic_head is missing"));
        return 0;
    };
    require_u64_field(
        semantic_head,
        "sequence",
        format!("{label}.semantic_head").as_str(),
        violations,
    )
}

fn write_report(path: &Path, report: &Value) -> Result<()> {
    if let Some(parent) = path.parent() {
        if !parent.as_os_str().is_empty() {
            fs::create_dir_all(parent).with_context(|| {
                format!("create remote reentry report dir: {}", parent.display())
            })?;
        }
    }
    let encoded =
        serde_json::to_string_pretty(report).context("encode remote reentry report failed")?;
    fs::write(path, encoded)
        .with_context(|| format!("write remote reentry report failed: {}", path.display()))
}

struct DuplicateRoundsInput<'a> {
    chain_id: u64,
    sender_node: u64,
    receiver_node: u64,
    sender_addr: &'a str,
    receiver_addr: &'a str,
    txs: &'a [NativeFixtureTxV1],
    first_round: u64,
    round_count: u64,
    delay_ms: u64,
    control_frame_auth_key: &'a str,
    run_id: &'a str,
}

fn send_duplicate_rounds(input: DuplicateRoundsInput<'_>) -> Result<BTreeMap<String, u64>> {
    let DuplicateRoundsInput {
        chain_id,
        sender_node,
        receiver_node,
        sender_addr,
        receiver_addr,
        txs,
        first_round,
        round_count,
        delay_ms,
        control_frame_auth_key,
        run_id,
    } = input;
    let sender = UdpTransport::bind_for_chain(NodeId(sender_node), sender_addr, chain_id)
        .with_context(|| format!("bind remote reentry sender UDP failed: {sender_addr}"))?;
    sender
        .register_peer(NodeId(receiver_node), receiver_addr)
        .with_context(|| {
            format!("register remote reentry receiver peer failed: {receiver_addr}")
        })?;
    let mut sent_by_hash = BTreeMap::<String, u64>::new();
    let end_round = first_round.saturating_add(round_count);
    for round in first_round..end_round {
        for tx in txs {
            // Duplicate re-entry is not a repair-window claim. Keep the
            // business count stable and authenticate copy identity separately.
            let tx_count = 1;
            let msg = ProtocolMessage::EvmNative(EvmNativeMessage::Transactions {
                from: NodeId(sender_node),
                chain_id,
                tx_hash: tx.tx_hash,
                tx_count,
                payload: tx.payload.clone(),
                transport_auth: Some(build_evm_native_transaction_frame_auth_v1(
                    EvmNativeTransactionFrameAuthInputV1 {
                        from: NodeId(sender_node),
                        chain_id,
                        tx_hash: &tx.tx_hash,
                        tx_count,
                        payload: tx.payload.as_slice(),
                        frame_kind: "primary",
                        run_id,
                        sequence: tx.index,
                        copy_index: round,
                        key: control_frame_auth_key,
                    },
                )),
            });
            sender.send(NodeId(receiver_node), msg).with_context(|| {
                format!(
                    "send remote reentry packet index={} round={} failed",
                    tx.index, round
                )
            })?;
            *sent_by_hash.entry(hex_lower(&tx.tx_hash)).or_default() += 1;
            if delay_ms > 0 {
                std::thread::sleep(std::time::Duration::from_millis(delay_ms));
            }
        }
    }
    Ok(sent_by_hash)
}

fn validate_common_boundaries(summary: &Value, label: &str, violations: &mut Vec<String>) {
    if summary_str(summary, "execution_kernel") != "AOEM" {
        violations.push(format!(
            "{label} execution_kernel={} expected AOEM",
            summary_str(summary, "execution_kernel")
        ));
    }
    if summary_str(summary, "aoem_concurrency_owner") != "AOEM_runtime" {
        violations.push(format!(
            "{label} aoem_concurrency_owner={} expected AOEM_runtime",
            summary_str(summary, "aoem_concurrency_owner")
        ));
    }
    if summary_str(summary, "host_concurrency_policy")
        != "host_drives_lifecycle_only_no_rust_execution_scheduler"
    {
        violations.push(format!(
            "{label} host_concurrency_policy={} expected host lifecycle only",
            summary_str(summary, "host_concurrency_policy")
        ));
    }
}

fn validate_aoem_owned_production(summary: &Value, label: &str, violations: &mut Vec<String>) {
    for field in [
        "tx_ingress_aoem_gate_config_production_candidate",
        "aoem_owned_single_path_enforced",
        "aoem_native_tx_batch_production_candidate_result_ok",
        "aoem_owned_regression_signable",
    ] {
        if summary.get(field).and_then(Value::as_bool) != Some(true) {
            violations.push(format!("{label}.{field} is not true"));
        }
    }
    for field in [
        "legacy_host_transitional_fallback_used",
        "aoem_native_tx_batch_production_fallback_used",
        "aoem_native_tx_batch_production_double_write_legacy_canonical",
    ] {
        if summary.get(field).and_then(Value::as_bool) != Some(false) {
            violations.push(format!("{label}.{field} is not false"));
        }
    }
    for field in ["tx_ingress_selected_path", "tx_ingress_production_target"] {
        if summary_str(summary, field) != "aoem_runtime_owned_state_persistence" {
            violations.push(format!(
                "{label}.{field}={} expected aoem_runtime_owned_state_persistence",
                summary_str(summary, field)
            ));
        }
    }
}

fn main() -> Result<()> {
    let chain_id = u64_env("NOVOVM_NATIVE_PIPELINE_REMOTE_REENTRY_CHAIN_ID", 9_998_903)?;
    let tx_count = u64_env("NOVOVM_NATIVE_PIPELINE_REMOTE_REENTRY_TX_COUNT", 16)?.max(1);
    let duplicate_rounds =
        u64_env("NOVOVM_NATIVE_PIPELINE_REMOTE_REENTRY_DUPLICATE_ROUNDS", 3)?.max(2);
    let batch_budget = u64_env("NOVOVM_NATIVE_PIPELINE_REMOTE_REENTRY_BATCH_BUDGET", 8)?.max(1);
    let recv_budget = u64_env("NOVOVM_NATIVE_PIPELINE_REMOTE_REENTRY_RECV_BUDGET", 128)?.max(1);
    let tick_interval_ms =
        u64_env("NOVOVM_NATIVE_PIPELINE_REMOTE_REENTRY_TICK_INTERVAL_MS", 10)?.max(1);
    let startup_wait_ms = u64_env(
        "NOVOVM_NATIVE_PIPELINE_REMOTE_REENTRY_STARTUP_WAIT_MS",
        10_000,
    )?;
    let delay_ms = u64_env("NOVOVM_NATIVE_PIPELINE_REMOTE_REENTRY_SEND_DELAY_MS", 1)?;
    let send_window_ticks = div_ceil_u64(
        tx_count
            .saturating_mul(duplicate_rounds)
            .saturating_mul(delay_ms),
        tick_interval_ms,
    );
    let receiver_ticks = u64_env(
        "NOVOVM_NATIVE_PIPELINE_REMOTE_REENTRY_RECEIVER_TICKS",
        div_ceil_u64(tx_count, batch_budget)
            .saturating_add(send_window_ticks)
            .saturating_add(tx_count.saturating_mul(2).max(64)),
    )?;
    let restart_ticks = u64_env(
        "NOVOVM_NATIVE_PIPELINE_REMOTE_REENTRY_RESTART_TICKS",
        send_window_ticks.saturating_add(tx_count.max(64)),
    )?;
    let sender_node = u64_env(
        "NOVOVM_NATIVE_PIPELINE_REMOTE_REENTRY_SENDER_NODE",
        9_991_930,
    )?;
    let receiver_node = u64_env(
        "NOVOVM_NATIVE_PIPELINE_REMOTE_REENTRY_RECEIVER_NODE",
        9_991_931,
    )?;
    let node_bin = novovm_node_bin();
    if !node_bin.exists() {
        bail!(
            "novovm-node binary not found: {}; build with `cargo build -p novovm-node --bins` or set NOVOVM_NATIVE_PIPELINE_NODE_BIN",
            node_bin.display()
        );
    }

    let store_path = temp_store_path(chain_id);
    let gate_instance = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos();
    let control_frame_auth_key =
        format!("novovm-remote-reentry-gate-key-{chain_id}-{gate_instance}");
    let run_id = format!("novovm-remote-reentry-gate-run-{chain_id}-{gate_instance}");
    let initial_progress_report_path = store_path.with_extension("initial-progress.json");
    let restart_progress_report_path = store_path.with_extension("restart-progress.json");
    let txs = build_native_payloads(chain_id, tx_count)?;
    let expected_sent_packets = tx_count.saturating_mul(duplicate_rounds);
    let expected_duplicate_received = expected_sent_packets.saturating_sub(tx_count);

    let sender_addr = reserve_udp_addr()?;
    let receiver_addr = reserve_udp_addr()?;
    let mut receiver = spawn_receiver(ReceiverSpawnInput {
        node_bin: node_bin.as_path(),
        chain_id,
        receiver_node,
        sender_node,
        receiver_addr: receiver_addr.as_str(),
        sender_addr: sender_addr.as_str(),
        store_path: store_path.as_path(),
        progress_report_path: initial_progress_report_path.as_path(),
        control_frame_auth_key: control_frame_auth_key.as_str(),
        run_id: run_id.as_str(),
        receiver_ticks,
        tick_interval_ms,
        batch_budget,
        recv_budget,
        expected_execution_count: tx_count,
    })?;
    wait_receiver_ready(
        &mut receiver,
        initial_progress_report_path.as_path(),
        startup_wait_ms,
        "initial",
    )?;
    let mut initial_sent_by_hash = send_duplicate_rounds(DuplicateRoundsInput {
        chain_id,
        sender_node,
        receiver_node,
        sender_addr: sender_addr.as_str(),
        receiver_addr: receiver_addr.as_str(),
        txs: txs.as_slice(),
        first_round: 0,
        round_count: 1,
        delay_ms,
        control_frame_auth_key: control_frame_auth_key.as_str(),
        run_id: run_id.as_str(),
    })?;
    let initial_primary_delivery_summary = wait_receiver_delivery_progress(
        &mut receiver,
        initial_progress_report_path.as_path(),
        tx_count,
        startup_wait_ms,
        "initial_primary",
    )?;
    let duplicate_sent_by_hash = send_duplicate_rounds(DuplicateRoundsInput {
        chain_id,
        sender_node,
        receiver_node,
        sender_addr: sender_addr.as_str(),
        receiver_addr: receiver_addr.as_str(),
        txs: txs.as_slice(),
        first_round: 1,
        round_count: duplicate_rounds.saturating_sub(1),
        delay_ms,
        control_frame_auth_key: control_frame_auth_key.as_str(),
        run_id: run_id.as_str(),
    })?;
    for (tx_hash, sent) in duplicate_sent_by_hash {
        *initial_sent_by_hash.entry(tx_hash).or_default() += sent;
    }
    let initial_summary = parse_summary(
        receiver
            .wait_with_output()
            .context("wait initial remote reentry receiver failed")?,
        "initial",
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
        format!("remote-reentry-chain-{chain_id}"),
    );
    let initial_probe = get_nov_native_execution_store_recovery_probe_v1(store_path.as_path())?;

    let restart_sender_addr = reserve_udp_addr()?;
    let restart_receiver_addr = reserve_udp_addr()?;
    let mut restart_receiver = spawn_receiver(ReceiverSpawnInput {
        node_bin: node_bin.as_path(),
        chain_id,
        receiver_node,
        sender_node,
        receiver_addr: restart_receiver_addr.as_str(),
        sender_addr: restart_sender_addr.as_str(),
        store_path: store_path.as_path(),
        progress_report_path: restart_progress_report_path.as_path(),
        control_frame_auth_key: control_frame_auth_key.as_str(),
        run_id: run_id.as_str(),
        receiver_ticks: restart_ticks,
        tick_interval_ms,
        batch_budget,
        recv_budget,
        expected_execution_count: 0,
    })?;
    wait_receiver_ready(
        &mut restart_receiver,
        restart_progress_report_path.as_path(),
        startup_wait_ms,
        "restart",
    )?;
    let restart_sent_by_hash = send_duplicate_rounds(DuplicateRoundsInput {
        chain_id,
        sender_node,
        receiver_node,
        sender_addr: restart_sender_addr.as_str(),
        receiver_addr: restart_receiver_addr.as_str(),
        txs: txs.as_slice(),
        first_round: 0,
        round_count: duplicate_rounds,
        delay_ms,
        control_frame_auth_key: control_frame_auth_key.as_str(),
        run_id: run_id.as_str(),
    })?;
    let restart_summary = parse_summary(
        restart_receiver
            .wait_with_output()
            .context("wait restart remote reentry receiver failed")?,
        "restart",
    )?;
    let restart_probe = get_nov_native_execution_store_recovery_probe_v1(store_path.as_path())?;

    let mut violations = Vec::<String>::new();
    let initial_included = require_u64_field(
        &initial_summary,
        "included_canonical_total",
        "initial",
        &mut violations,
    );
    let initial_aoem_executed = require_u64_field(
        &initial_summary,
        "aoem_executed_total",
        "initial",
        &mut violations,
    );
    let initial_queue_pending = require_u64_field(
        &initial_summary,
        "queue_pending_last",
        "initial",
        &mut violations,
    );
    let initial_receipt_count = require_u64_field(
        &initial_probe,
        "receipt_count",
        "initial_recovery_probe",
        &mut violations,
    );
    let initial_sequence =
        require_semantic_sequence(&initial_probe, "initial_recovery_probe", &mut violations);
    let restart_included = require_u64_field(
        &restart_summary,
        "included_canonical_total",
        "restart",
        &mut violations,
    );
    let restart_aoem_executed = require_u64_field(
        &restart_summary,
        "aoem_executed_total",
        "restart",
        &mut violations,
    );
    let restart_queue_pending = require_u64_field(
        &restart_summary,
        "queue_pending_last",
        "restart",
        &mut violations,
    );
    let restart_receipt_count = require_u64_field(
        &restart_probe,
        "receipt_count",
        "restart_recovery_probe",
        &mut violations,
    );
    let restart_sequence =
        require_semantic_sequence(&restart_probe, "restart_recovery_probe", &mut violations);

    let initial_primary_received = require_u64_field(
        &initial_primary_delivery_summary,
        "network_received_total",
        "initial_primary_progress",
        &mut violations,
    );
    let initial_primary_udp_decode_ok = require_u64_field(
        &initial_primary_delivery_summary,
        "receiver_udp_packet_decode_ok_count",
        "initial_primary_progress",
        &mut violations,
    );
    let initial_primary_data_decode_ok = require_u64_field(
        &initial_primary_delivery_summary,
        "native_receiver_data_frame_decode_ok_count",
        "initial_primary_progress",
        &mut violations,
    );
    let initial_received = require_u64_field(
        &initial_summary,
        "network_received_total",
        "initial",
        &mut violations,
    );
    let initial_udp_decode_ok = require_u64_field(
        &initial_summary,
        "receiver_udp_packet_decode_ok_count",
        "initial",
        &mut violations,
    );
    let initial_data_decode_ok = require_u64_field(
        &initial_summary,
        "native_receiver_data_frame_decode_ok_count",
        "initial",
        &mut violations,
    );
    let restart_received = require_u64_field(
        &restart_summary,
        "network_received_total",
        "restart",
        &mut violations,
    );
    let restart_udp_decode_ok = require_u64_field(
        &restart_summary,
        "receiver_udp_packet_decode_ok_count",
        "restart",
        &mut violations,
    );
    let restart_data_decode_ok = require_u64_field(
        &restart_summary,
        "native_receiver_data_frame_decode_ok_count",
        "restart",
        &mut violations,
    );
    let duplicate_received = initial_received.saturating_sub(initial_primary_received);
    let duplicate_udp_decode_ok =
        initial_udp_decode_ok.saturating_sub(initial_primary_udp_decode_ok);
    let duplicate_data_decode_ok =
        initial_data_decode_ok.saturating_sub(initial_primary_data_decode_ok);
    let duplicate_canonical_included = initial_included.saturating_sub(tx_count);
    let duplicate_receipt = initial_receipt_count.saturating_sub(tx_count);
    let duplicate_canonical_after_restart = restart_included;
    let duplicate_receipt_after_restart =
        restart_receipt_count.saturating_sub(initial_receipt_count);
    let semantic_head_extra_advance = restart_sequence.saturating_sub(initial_sequence);
    let duplicate_dirty_commit = duplicate_receipt_after_restart
        .saturating_add(semantic_head_extra_advance)
        .saturating_add(restart_included);
    let receipt_index_consistent = initial_probe
        .get("receipt_index_recovered")
        .and_then(Value::as_bool)
        == Some(true)
        && restart_probe
            .get("receipt_index_recovered")
            .and_then(Value::as_bool)
            == Some(true)
        && restart_receipt_count == tx_count;
    let semantic_head_monotonic = initial_probe
        .get("semantic_head_current_recovered")
        .and_then(Value::as_bool)
        == Some(true)
        && restart_probe
            .get("semantic_head_current_recovered")
            .and_then(Value::as_bool)
            == Some(true)
        && initial_sequence >= initial_included
        && restart_sequence == initial_sequence;

    validate_common_boundaries(&initial_summary, "initial", &mut violations);
    validate_common_boundaries(&restart_summary, "restart", &mut violations);
    validate_aoem_owned_production(&initial_summary, "initial", &mut violations);
    for (label, field, observed, expected) in [
        (
            "initial_primary",
            "network_received_total",
            initial_primary_received,
            tx_count,
        ),
        (
            "initial_primary",
            "receiver_udp_packet_decode_ok_count",
            initial_primary_udp_decode_ok,
            tx_count,
        ),
        (
            "initial_primary",
            "native_receiver_data_frame_decode_ok_count",
            initial_primary_data_decode_ok,
            tx_count,
        ),
        (
            "initial_final",
            "network_received_total",
            initial_received,
            expected_sent_packets,
        ),
        (
            "initial_final",
            "receiver_udp_packet_decode_ok_count",
            initial_udp_decode_ok,
            expected_sent_packets,
        ),
        (
            "initial_final",
            "native_receiver_data_frame_decode_ok_count",
            initial_data_decode_ok,
            expected_sent_packets,
        ),
        (
            "duplicate_delta",
            "network_received_total",
            duplicate_received,
            expected_duplicate_received,
        ),
        (
            "duplicate_delta",
            "receiver_udp_packet_decode_ok_count",
            duplicate_udp_decode_ok,
            expected_duplicate_received,
        ),
        (
            "duplicate_delta",
            "native_receiver_data_frame_decode_ok_count",
            duplicate_data_decode_ok,
            expected_duplicate_received,
        ),
        (
            "restart",
            "network_received_total",
            restart_received,
            expected_sent_packets,
        ),
        (
            "restart",
            "receiver_udp_packet_decode_ok_count",
            restart_udp_decode_ok,
            expected_sent_packets,
        ),
        (
            "restart",
            "native_receiver_data_frame_decode_ok_count",
            restart_data_decode_ok,
            expected_sent_packets,
        ),
    ] {
        if observed != expected {
            violations.push(format!("{label}.{field}={observed} expected {expected}"));
        }
    }
    if duplicate_received == 0 {
        violations.push("duplicate network_received delta=0 expected >0".to_string());
    }
    if restart_received == 0 {
        violations.push("restart network_received_total=0 expected >0".to_string());
    }
    if initial_aoem_executed != tx_count {
        violations.push(format!(
            "initial aoem_executed_total={initial_aoem_executed} expected {tx_count}"
        ));
    }
    if initial_included != tx_count {
        violations.push(format!(
            "initial included_canonical_total={initial_included} expected {tx_count}"
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
    if restart_aoem_executed != 0 {
        violations.push(format!(
            "restart aoem_executed_total={restart_aoem_executed} expected 0 for already receipted reentry"
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
    if duplicate_dirty_commit != 0 {
        violations.push(format!(
            "duplicate_dirty_commit={duplicate_dirty_commit} expected 0"
        ));
    }
    if semantic_head_extra_advance != 0 {
        violations.push(format!(
            "semantic_head_extra_advance={semantic_head_extra_advance} expected 0"
        ));
    }
    if !receipt_index_consistent {
        violations.push("receipt_index_consistent=false".to_string());
    }
    if !semantic_head_monotonic {
        violations.push("semantic_head_monotonic=false".to_string());
    }
    if initial_queue_pending != 0 {
        violations.push(format!(
            "initial queue_pending_last={initial_queue_pending} expected 0"
        ));
    }
    if restart_queue_pending != 0 {
        violations.push(format!(
            "restart queue_pending_last={restart_queue_pending} expected 0"
        ));
    }
    for (label, summary) in [
        (
            "initial_primary_progress",
            &initial_primary_delivery_summary,
        ),
        ("initial", &initial_summary),
        ("restart", &restart_summary),
    ] {
        for field in [
            "native_receiver_auth_drop_count",
            "native_receiver_data_frame_decode_error_count",
            "native_receiver_run_id_mismatch_count",
            "native_receiver_session_id_mismatch_count",
            "native_receiver_source_pin_drop_count",
            "receiver_udp_packet_decode_error_count",
        ] {
            let observed = require_u64_field(summary, field, label, &mut violations);
            if observed != 0 {
                violations.push(format!("{label}.{field}={observed} expected 0"));
            }
        }
    }
    for (label, summary) in [("initial", &initial_summary), ("restart", &restart_summary)] {
        for field in [
            "ledger_receipt_proof_missing_sequence_mapping_count",
            "ledger_canonical_proof_missing_sequence_mapping_count",
        ] {
            let observed = require_u64_field(summary, field, label, &mut violations);
            if observed != 0 {
                violations.push(format!("{label}.{field}={observed} expected 0"));
            }
        }
    }

    let accepted = violations.is_empty();
    let report = serde_json::json!({
        "schema": REPORT_SCHEMA_V1,
        "method": "supervm_native_pipeline_remote_reentry_dedup_gate",
        "accepted": accepted,
        "chain_id": chain_id,
        "tx_count": tx_count,
        "store_path": store_path,
        "pending_policy": PENDING_POLICY_V1,
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
        "remote_reentry": {
            "remote_reentry_dedup_ok": accepted,
            "duplicate_rounds": duplicate_rounds,
            "sent_packets": expected_sent_packets,
            "received_unique": initial_primary_received,
            "duplicate_received": duplicate_received,
            "canonical_unique_included": initial_included,
            "duplicate_canonical_included": duplicate_canonical_included,
            "duplicate_receipt": duplicate_receipt,
            "duplicate_dirty_commit": duplicate_dirty_commit,
            "semantic_head_extra_advance": semantic_head_extra_advance,
            "duplicate_canonical_after_restart": duplicate_canonical_after_restart,
            "duplicate_receipt_after_restart": duplicate_receipt_after_restart,
            "receipt_index_consistent": receipt_index_consistent,
            "semantic_head_monotonic": semantic_head_monotonic,
            "queue_pending_last": restart_queue_pending,
            "delivery_evidence": {
                "primary": {
                    "network_received_total": initial_primary_received,
                    "udp_decode_ok_count": initial_primary_udp_decode_ok,
                    "data_frame_decode_ok_count": initial_primary_data_decode_ok,
                    "auth_through_count": initial_primary_received
                },
                "duplicate_delta": {
                    "network_received_count": duplicate_received,
                    "udp_decode_ok_count": duplicate_udp_decode_ok,
                    "data_frame_decode_ok_count": duplicate_data_decode_ok,
                    "auth_through_count": duplicate_received
                },
                "restart": {
                    "network_received_total": restart_received,
                    "udp_decode_ok_count": restart_udp_decode_ok,
                    "data_frame_decode_ok_count": restart_data_decode_ok,
                    "auth_through_count": restart_received
                }
            }
        },
        "scenarios": {
            "duplicate_packet_reentry": {
                "duplicate_received": duplicate_received,
                "duplicate_canonical_included": duplicate_canonical_included,
                "duplicate_receipt": duplicate_receipt
            },
            "duplicate_broadcast_reentry": {
                "duplicate_rounds": duplicate_rounds,
                "canonical_unique_included": initial_included,
                "duplicate_canonical_included": duplicate_canonical_included
            },
            "duplicate_after_canonical": {
                "second_ingress_after_initial_canonical": true,
                "restart_aoem_executed_total": restart_aoem_executed,
                "duplicate_canonical_after_restart": duplicate_canonical_after_restart,
                "duplicate_receipt_after_restart": duplicate_receipt_after_restart
            },
            "duplicate_after_restart": {
                "receiver_restarted": true,
                "network_received_total": restart_received,
                "duplicate_canonical_after_restart": duplicate_canonical_after_restart,
                "duplicate_receipt_after_restart": duplicate_receipt_after_restart,
                "semantic_head_extra_advance": semantic_head_extra_advance
            }
        },
        "initial_primary_delivery_summary": initial_primary_delivery_summary,
        "initial_summary": initial_summary,
        "restart_summary": restart_summary,
        "initial_recovery_probe": initial_probe,
        "restart_recovery_probe": restart_probe,
        "initial_sent_by_hash": initial_sent_by_hash,
        "restart_sent_by_hash": restart_sent_by_hash,
        "violations": violations
    });
    let path = report_path();
    write_report(path.as_path(), &report)?;
    println!(
        "{}",
        serde_json::to_string_pretty(&report).context("encode remote reentry report failed")?
    );
    if !accepted {
        bail!(
            "native pipeline remote reentry dedup gate failed: {}",
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
    use std::collections::BTreeSet;

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

    #[test]
    fn receiver_env_inheritance_excludes_transport_configuration_families() {
        for key in [
            "NOVOVM_NOVORUDP_CONTROL_FRAME_AUTH_KEY",
            "NOVOVM_NOVORUDP_SOURCE_PINNING_ENABLED",
            "NOVOVM_NETWORK_CONTROL_FRAME_AUTH_KEY",
            "NOVOVM_NETWORK_RECEIVER_RATE_LIMIT_ENABLED",
        ] {
            assert!(!should_inherit_receiver_env_v1(key), "{key}");
        }
        assert!(should_inherit_receiver_env_v1("PATH"));
        assert!(should_inherit_receiver_env_v1("NOVOVM_AOEM_VARIANT"));
    }

    #[test]
    fn require_u64_field_accepts_zero_and_rejects_missing_or_wrong_type() {
        let summary = serde_json::json!({
            "zero": 0u64,
            "wrong": "0",
        });
        let mut violations = Vec::new();

        assert_eq!(
            require_u64_field(&summary, "zero", "summary", &mut violations),
            0
        );
        assert!(violations.is_empty());

        assert_eq!(
            require_u64_field(&summary, "missing", "summary", &mut violations),
            0
        );
        assert_eq!(
            require_u64_field(&summary, "wrong", "summary", &mut violations),
            0
        );
        assert_eq!(violations.len(), 2);
        assert!(violations[0].contains("summary.missing is missing"));
        assert!(violations[1].contains("summary.wrong must be present as u64"));
    }
}
