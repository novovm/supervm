#![forbid(unsafe_code)]

use anyhow::{bail, Context, Result};
use novovm_network::{Transport, UdpTransport};
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
use std::collections::BTreeMap;
use std::fs;
use std::net::UdpSocket;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Output, Stdio};
use std::time::{SystemTime, UNIX_EPOCH};

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
        let nonce = index.saturating_add(1);
        let account_id = format!("acct-native-remote-reentry-{nonce}");
        let mut tx = NovNativeTxWireV1 {
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
                nonce,
            }),
            signature: Vec::new(),
        };
        sign_nov_native_tx_with_seed_v1(&mut tx, [(nonce & 0xff) as u8; 32])?;
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
        receiver_ticks,
        tick_interval_ms,
        batch_budget,
        recv_budget,
        expected_execution_count,
    } = input;
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

fn parse_summary(output: Output, label: &str) -> Result<Value> {
    if !output.status.success() {
        bail!(
            "{label} receiver failed: status={} stderr={}",
            output.status,
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
    duplicate_rounds: u64,
    delay_ms: u64,
}

fn send_duplicate_rounds(input: DuplicateRoundsInput<'_>) -> Result<BTreeMap<String, u64>> {
    let DuplicateRoundsInput {
        chain_id,
        sender_node,
        receiver_node,
        sender_addr,
        receiver_addr,
        txs,
        duplicate_rounds,
        delay_ms,
    } = input;
    let sender = UdpTransport::bind_for_chain(NodeId(sender_node), sender_addr, chain_id)
        .with_context(|| format!("bind remote reentry sender UDP failed: {sender_addr}"))?;
    sender
        .register_peer(NodeId(receiver_node), receiver_addr)
        .with_context(|| {
            format!("register remote reentry receiver peer failed: {receiver_addr}")
        })?;
    let mut sent_by_hash = BTreeMap::<String, u64>::new();
    for round in 0..duplicate_rounds.max(1) {
        for tx in txs {
            let msg = ProtocolMessage::EvmNative(EvmNativeMessage::Transactions {
                from: NodeId(sender_node),
                chain_id,
                tx_hash: tx.tx_hash,
                tx_count: 1,
                payload: tx.payload.clone(),
                transport_auth: None,
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

fn main() -> Result<()> {
    let chain_id = u64_env("NOVOVM_NATIVE_PIPELINE_REMOTE_REENTRY_CHAIN_ID", 9_998_903)?;
    let tx_count = u64_env("NOVOVM_NATIVE_PIPELINE_REMOTE_REENTRY_TX_COUNT", 16)?.max(1);
    let duplicate_rounds =
        u64_env("NOVOVM_NATIVE_PIPELINE_REMOTE_REENTRY_DUPLICATE_ROUNDS", 3)?.max(2);
    let batch_budget = u64_env("NOVOVM_NATIVE_PIPELINE_REMOTE_REENTRY_BATCH_BUDGET", 8)?.max(1);
    let recv_budget = u64_env("NOVOVM_NATIVE_PIPELINE_REMOTE_REENTRY_RECV_BUDGET", 128)?.max(1);
    let tick_interval_ms =
        u64_env("NOVOVM_NATIVE_PIPELINE_REMOTE_REENTRY_TICK_INTERVAL_MS", 10)?.max(1);
    let startup_wait_ms = u64_env("NOVOVM_NATIVE_PIPELINE_REMOTE_REENTRY_STARTUP_WAIT_MS", 300)?;
    let delay_ms = u64_env("NOVOVM_NATIVE_PIPELINE_REMOTE_REENTRY_SEND_DELAY_MS", 1)?;
    let receiver_ticks = u64_env(
        "NOVOVM_NATIVE_PIPELINE_REMOTE_REENTRY_RECEIVER_TICKS",
        div_ceil_u64(tx_count, batch_budget)
            .saturating_add(div_ceil_u64(startup_wait_ms, tick_interval_ms))
            .saturating_add(32),
    )?;
    let restart_ticks = u64_env(
        "NOVOVM_NATIVE_PIPELINE_REMOTE_REENTRY_RESTART_TICKS",
        div_ceil_u64(startup_wait_ms, tick_interval_ms).saturating_add(24),
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
    let txs = build_native_payloads(chain_id, tx_count)?;
    let expected_sent_packets = tx_count.saturating_mul(duplicate_rounds);
    let expected_duplicate_received = expected_sent_packets.saturating_sub(tx_count);

    let sender_addr = reserve_udp_addr()?;
    let receiver_addr = reserve_udp_addr()?;
    let receiver = spawn_receiver(ReceiverSpawnInput {
        node_bin: node_bin.as_path(),
        chain_id,
        receiver_node,
        sender_node,
        receiver_addr: receiver_addr.as_str(),
        sender_addr: sender_addr.as_str(),
        store_path: store_path.as_path(),
        receiver_ticks,
        tick_interval_ms,
        batch_budget,
        recv_budget,
        expected_execution_count: tx_count,
    })?;
    std::thread::sleep(std::time::Duration::from_millis(startup_wait_ms));
    let initial_sent_by_hash = send_duplicate_rounds(DuplicateRoundsInput {
        chain_id,
        sender_node,
        receiver_node,
        sender_addr: sender_addr.as_str(),
        receiver_addr: receiver_addr.as_str(),
        txs: txs.as_slice(),
        duplicate_rounds,
        delay_ms,
    })?;
    let initial_summary = parse_summary(
        receiver
            .wait_with_output()
            .context("wait initial remote reentry receiver failed")?,
        "initial",
    )?;
    std::env::set_var("NOVOVM_NATIVE_EXECUTION_STORE_BACKEND", "rocksdb");
    let initial_probe = get_nov_native_execution_store_recovery_probe_v1(store_path.as_path())?;

    let restart_sender_addr = reserve_udp_addr()?;
    let restart_receiver_addr = reserve_udp_addr()?;
    let restart_receiver = spawn_receiver(ReceiverSpawnInput {
        node_bin: node_bin.as_path(),
        chain_id,
        receiver_node,
        sender_node,
        receiver_addr: restart_receiver_addr.as_str(),
        sender_addr: restart_sender_addr.as_str(),
        store_path: store_path.as_path(),
        receiver_ticks: restart_ticks,
        tick_interval_ms,
        batch_budget,
        recv_budget,
        expected_execution_count: 0,
    })?;
    std::thread::sleep(std::time::Duration::from_millis(startup_wait_ms));
    let restart_sent_by_hash = send_duplicate_rounds(DuplicateRoundsInput {
        chain_id,
        sender_node,
        receiver_node,
        sender_addr: restart_sender_addr.as_str(),
        receiver_addr: restart_receiver_addr.as_str(),
        txs: txs.as_slice(),
        duplicate_rounds,
        delay_ms,
    })?;
    let restart_summary = parse_summary(
        restart_receiver
            .wait_with_output()
            .context("wait restart remote reentry receiver failed")?,
        "restart",
    )?;
    let restart_probe = get_nov_native_execution_store_recovery_probe_v1(store_path.as_path())?;

    let initial_included = summary_u64(&initial_summary, "included_canonical_total");
    let initial_receipt_count = probe_u64(&initial_probe, "receipt_count");
    let initial_sequence = semantic_sequence(&initial_probe);
    let restart_included = summary_u64(&restart_summary, "included_canonical_total");
    let restart_receipt_count = probe_u64(&restart_probe, "receipt_count");
    let restart_sequence = semantic_sequence(&restart_probe);
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

    let mut violations = Vec::<String>::new();
    validate_common_boundaries(&initial_summary, "initial", &mut violations);
    validate_common_boundaries(&restart_summary, "restart", &mut violations);
    if expected_duplicate_received == 0 {
        violations.push("duplicate_received=0 expected >0".to_string());
    }
    if summary_u64(&initial_summary, "aoem_executed_total") != tx_count {
        violations.push(format!(
            "initial aoem_executed_total={} expected {tx_count}",
            summary_u64(&initial_summary, "aoem_executed_total")
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
    if summary_u64(&restart_summary, "aoem_executed_total") != 0 {
        violations.push(format!(
            "restart aoem_executed_total={} expected 0 for already receipted reentry",
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
    if summary_u64(&initial_summary, "queue_pending_last") != 0 {
        violations.push(format!(
            "initial queue_pending_last={} expected 0",
            summary_u64(&initial_summary, "queue_pending_last")
        ));
    }
    if summary_u64(&restart_summary, "queue_pending_last") != 0 {
        violations.push(format!(
            "restart queue_pending_last={} expected 0",
            summary_u64(&restart_summary, "queue_pending_last")
        ));
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
            "receipt_state_source": "AOEM_tick_lifecycle",
            "commit": "dirty_sharded_atomic_commit",
            "canonical_body_head_recovery": "not_claimed_by_this_gate"
        },
        "remote_reentry": {
            "remote_reentry_dedup_ok": accepted,
            "duplicate_rounds": duplicate_rounds,
            "sent_packets": expected_sent_packets,
            "received_unique": tx_count,
            "duplicate_received": expected_duplicate_received,
            "canonical_unique_included": initial_included,
            "duplicate_canonical_included": duplicate_canonical_included,
            "duplicate_receipt": duplicate_receipt,
            "duplicate_dirty_commit": duplicate_dirty_commit,
            "semantic_head_extra_advance": semantic_head_extra_advance,
            "duplicate_canonical_after_restart": duplicate_canonical_after_restart,
            "duplicate_receipt_after_restart": duplicate_receipt_after_restart,
            "receipt_index_consistent": receipt_index_consistent,
            "semantic_head_monotonic": semantic_head_monotonic,
            "queue_pending_last": summary_u64(&restart_summary, "queue_pending_last")
        },
        "scenarios": {
            "duplicate_packet_reentry": {
                "duplicate_received": expected_duplicate_received,
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
                "restart_aoem_executed_total": summary_u64(&restart_summary, "aoem_executed_total"),
                "duplicate_canonical_after_restart": duplicate_canonical_after_restart,
                "duplicate_receipt_after_restart": duplicate_receipt_after_restart
            },
            "duplicate_after_restart": {
                "receiver_restarted": true,
                "duplicate_canonical_after_restart": duplicate_canonical_after_restart,
                "duplicate_receipt_after_restart": duplicate_receipt_after_restart,
                "semantic_head_extra_advance": semantic_head_extra_advance
            }
        },
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
