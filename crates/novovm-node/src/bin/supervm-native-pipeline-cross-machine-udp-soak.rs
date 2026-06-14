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

fn store_path(chain_id: u64, role: &str) -> PathBuf {
    first_string_env_nonempty(&[
        "NOVOVM_NATIVE_PIPELINE_STORE_PATH",
        "NOVOVM_NATIVE_EXECUTION_TICK_STORE_PATH",
    ])
    .map(PathBuf::from)
    .unwrap_or_else(|| temp_store_path(chain_id, role))
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
            "0".to_string(),
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
    if !output.status.success() {
        bail!(
            "{label} failed: status={} stderr={}",
            output.status,
            String::from_utf8_lossy(&output.stderr)
        );
    }
    serde_json::from_slice::<Value>(&output.stdout).with_context(|| {
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
    max_ticks: u64,
    tick_interval_ms: u64,
    batch_budget: u64,
    recv_budget: u64,
) -> Result<Value> {
    let child = spawn_receiver_node(
        node_bin,
        chain_id,
        receiver_node,
        listen_addr,
        store_path,
        max_ticks,
        tick_interval_ms,
        batch_budget,
        recv_budget,
    )?;
    parse_summary(
        child
            .wait_with_output()
            .context("wait cross-machine receiver failed")?,
        "cross-machine receiver",
    )
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
    let received_unique = summary_u64(summary, "ingress_total_last");
    let canonical_unique_included = summary_u64(summary, "included_canonical_total");
    let duplicate_canonical_included = canonical_unique_included.saturating_sub(tx_count);
    let receipt_count = probe_u64(probe, "receipt_count");
    let duplicate_receipt = receipt_count.saturating_sub(tx_count);
    let semantic_sequence = semantic_sequence(probe);
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
) -> Result<Value> {
    let txs = build_native_payloads(chain_id, tx_count)?;
    let scheduled = apply_fault_schedule(txs.as_slice(), fault);
    let stats = send_scheduled_batch(
        chain_id,
        sender_node,
        receiver_node,
        sender_addr,
        receiver_addr,
        scheduled.as_slice(),
        fault.delay_ms,
    )?;
    let accepted = stats.sent_unique == tx_count;
    Ok(serde_json::json!({
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
        "sent_by_hash": stats.sent_by_hash,
        "violations": if accepted { Vec::<String>::new() } else { vec!["sender did not send expected unique tx count".to_string()] },
    }))
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
) -> Result<Value> {
    let receiver_summary = run_receiver_node(
        node_bin,
        chain_id,
        receiver_node,
        listen_addr,
        store_path,
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
        "receiver_summary": receiver_summary,
        "recovery_probe": recovery_probe,
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
) -> Result<Value> {
    let sender_addr = reserve_udp_addr()?;
    let receiver_addr = reserve_udp_addr()?;
    let child = spawn_receiver_node(
        node_bin,
        chain_id,
        receiver_node,
        receiver_addr.as_str(),
        store_path,
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
        "sender_report": sender_report,
        "validation": validation,
        "receiver_summary": receiver_summary,
        "recovery_probe": recovery_probe,
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
        32,
    )?
    .max(1);
    let batch_budget = u64_env_alias(
        &[
            "NOVOVM_NATIVE_PIPELINE_BATCH_BUDGET",
            "NOVOVM_NATIVE_PIPELINE_CROSS_MACHINE_BATCH_BUDGET",
        ],
        8,
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

fn hex_lower(bytes: &[u8]) -> String {
    let mut out = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        out.push_str(&format!("{byte:02x}"));
    }
    out
}
