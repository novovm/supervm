#![forbid(unsafe_code)]

use anyhow::{bail, Context, Result};
use novovm_network::{Transport, UdpTransport};
use novovm_node::tx_ingress::{
    get_nov_native_execution_store_recovery_probe_v1, nov_native_tx_to_adapter_tx_ir_v1,
};
use novovm_protocol::{
    encode_nov_native_tx_wire_v1, EvmNativeMessage, NodeId, NovExecuteTxV1, NovExecutionModeV1,
    NovExecutionPolicyV1, NovExecutionTargetV1, NovFeePolicyV1, NovNativeTxWireV1,
    NovPrivacyModeV1, NovTxKindV1, NovVerificationModeV1, ProtocolMessage,
};
use serde_json::Value;
use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::net::UdpSocket;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Output, Stdio};
use std::time::{SystemTime, UNIX_EPOCH};

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
        let nonce = index.saturating_add(1);
        let account_id = format!("acct-native-network-fault-{nonce}");
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
                nonce,
            }),
            signature: [(nonce & 0xff) as u8; 32],
        };
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

fn spawn_receiver(
    node_bin: &Path,
    chain_id: u64,
    receiver_node: u64,
    sender_node: u64,
    receiver_addr: &str,
    sender_addr: &str,
    store_path: &Path,
    receiver_ticks: u64,
    tick_interval_ms: u64,
    batch_budget: u64,
    recv_budget: u64,
    expected_unique: u64,
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
            "true".to_string(),
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

fn parse_summary(output: Output) -> Result<Value> {
    if !output.status.success() {
        bail!(
            "network fault receiver failed: status={} stderr={}",
            output.status,
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
    let startup_wait_ms = u64_env("NOVOVM_NATIVE_PIPELINE_FAULT_STARTUP_WAIT_MS", 300)?;
    let delay_ms = u64_env("NOVOVM_NATIVE_PIPELINE_FAULT_DELAY_MS", 1)?;
    let loss_bps = u64_env("NOVOVM_NATIVE_PIPELINE_FAULT_PACKET_LOSS_BPS", 500)?.min(10_000);
    let duplicate_bps = u64_env("NOVOVM_NATIVE_PIPELINE_FAULT_DUPLICATE_BPS", 10_000)?;
    let reorder_bps = u64_env("NOVOVM_NATIVE_PIPELINE_FAULT_REORDER_BPS", 10_000)?;
    let seed = u64_env("NOVOVM_NATIVE_PIPELINE_FAULT_SEED", 0x5eed_2026)?;
    let max_unique_loss = u64_env("NOVOVM_NATIVE_PIPELINE_FAULT_MAX_UNIQUE_LOSS", 4)?;
    let receiver_ticks = u64_env(
        "NOVOVM_NATIVE_PIPELINE_FAULT_RECEIVER_TICKS",
        div_ceil_u64(tx_count, batch_budget)
            .saturating_add(div_ceil_u64(startup_wait_ms, tick_interval_ms))
            .saturating_add(24),
    )?;
    let sender_node = u64_env("NOVOVM_NATIVE_PIPELINE_FAULT_SENDER_NODE", 9_991_900)?;
    let receiver_node = u64_env("NOVOVM_NATIVE_PIPELINE_FAULT_RECEIVER_NODE", 9_991_901)?;
    let sender_addr = reserve_udp_addr()?;
    let receiver_addr = reserve_udp_addr()?;
    let store_path = temp_store_path(chain_id);
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
    let duplicate_received = sent_packets.saturating_sub(delivered_unique_count);
    let unique_loss = tx_count.saturating_sub(delivered_unique_count);

    let receiver = spawn_receiver(
        node_bin.as_path(),
        chain_id,
        receiver_node,
        sender_node,
        receiver_addr.as_str(),
        sender_addr.as_str(),
        store_path.as_path(),
        receiver_ticks,
        tick_interval_ms,
        batch_budget,
        recv_budget,
        delivered_unique_count,
    )?;
    std::thread::sleep(std::time::Duration::from_millis(startup_wait_ms));

    let sender = UdpTransport::bind_for_chain(NodeId(sender_node), sender_addr.as_str(), chain_id)
        .with_context(|| format!("bind fault sender UDP failed: {sender_addr}"))?;
    sender
        .register_peer(NodeId(receiver_node), receiver_addr.as_str())
        .with_context(|| format!("register receiver peer failed: {receiver_addr}"))?;

    for packet in &schedule {
        if packet.dropped {
            continue;
        }
        let msg = ProtocolMessage::EvmNative(EvmNativeMessage::Transactions {
            from: NodeId(sender_node),
            chain_id,
            tx_hash: packet.tx_hash,
            tx_count: 1,
            payload: packet.payload.clone(),
            transport_auth: None,
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
    let recovery_probe = get_nov_native_execution_store_recovery_probe_v1(store_path.as_path())?;
    let included = summary_u64(&receiver_summary, "included_canonical_total");
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

    let mut violations = Vec::<String>::new();
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
    if delivered_unique_count < tx_count.saturating_sub(max_unique_loss) {
        violations.push(format!(
            "received_unique={delivered_unique_count} below budgeted minimum {}",
            tx_count.saturating_sub(max_unique_loss)
        ));
    }
    if duplicate_bps > 0 && duplicate_received == 0 {
        violations.push("duplicate mode enabled but duplicate_received=0".to_string());
    }
    if summary_u64(&receiver_summary, "aoem_executed_total") != delivered_unique_count {
        violations.push(format!(
            "aoem_executed_total={} expected delivered_unique={delivered_unique_count}",
            summary_u64(&receiver_summary, "aoem_executed_total")
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
    if summary_u64(&receiver_summary, "queue_pending_last") != 0 {
        violations.push(format!(
            "queue_pending_last={} expected 0",
            summary_u64(&receiver_summary, "queue_pending_last")
        ));
    }
    if summary_u64(&receiver_summary, "queue_dropped_last") != 0 {
        violations.push(format!(
            "queue_dropped_last={} expected 0",
            summary_u64(&receiver_summary, "queue_dropped_last")
        ));
    }
    if summary_u64(&receiver_summary, "queue_rejected_last") != 0 {
        violations.push(format!(
            "queue_rejected_last={} expected 0",
            summary_u64(&receiver_summary, "queue_rejected_last")
        ));
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
            "receipt_state_source": "AOEM_tick_lifecycle",
            "commit": "dirty_sharded_atomic_commit",
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
            "duplicate_received": duplicate_received
        },
        "validation": {
            "duplicate_canonical_included": duplicate_canonical_included,
            "semantic_head_monotonic": semantic_head_monotonic,
            "receipt_index_consistent": receipt_index_consistent,
            "queue_pending_last": summary_u64(&receiver_summary, "queue_pending_last"),
            "queue_dropped_last": summary_u64(&receiver_summary, "queue_dropped_last"),
            "queue_rejected_last": summary_u64(&receiver_summary, "queue_rejected_last")
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
