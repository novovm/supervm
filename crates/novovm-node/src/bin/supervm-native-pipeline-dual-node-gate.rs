#![forbid(unsafe_code)]

use anyhow::{bail, Context, Result};
use serde_json::Value;
use std::net::UdpSocket;
use std::path::PathBuf;
use std::process::{Command, Output, Stdio};
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

fn main() -> Result<()> {
    let chain_id = u64_env("NOVOVM_NATIVE_PIPELINE_DUAL_GATE_CHAIN_ID", 9_998_895)?;
    let tx_count = u64_env("NOVOVM_NATIVE_PIPELINE_DUAL_GATE_TX_COUNT", 3)?;
    let sender_ticks = u64_env(
        "NOVOVM_NATIVE_PIPELINE_DUAL_GATE_SENDER_TICKS",
        tx_count.max(3),
    )?;
    let receiver_ticks = u64_env(
        "NOVOVM_NATIVE_PIPELINE_DUAL_GATE_RECEIVER_TICKS",
        tx_count.max(6) + 12,
    )?;
    let sender_node = u64_env("NOVOVM_NATIVE_PIPELINE_DUAL_GATE_SENDER_NODE", 9_991_895)?;
    let receiver_node = u64_env("NOVOVM_NATIVE_PIPELINE_DUAL_GATE_RECEIVER_NODE", 9_991_896)?;
    let sender_addr = string_env_nonempty("NOVOVM_NATIVE_PIPELINE_DUAL_GATE_SENDER_ADDR")
        .map(Ok)
        .unwrap_or_else(reserve_udp_addr_v1)?;
    let receiver_addr = string_env_nonempty("NOVOVM_NATIVE_PIPELINE_DUAL_GATE_RECEIVER_ADDR")
        .map(Ok)
        .unwrap_or_else(reserve_udp_addr_v1)?;
    let sender_store = temp_store_path_v1("sender", chain_id);
    let receiver_store = temp_store_path_v1("receiver", chain_id);
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
                "100".to_string(),
            ),
            ("NOVOVM_NATIVE_EXECUTION_TICK_HARD_BUDGET", "1".to_string()),
            (
                "NOVOVM_NATIVE_EXECUTION_TICK_TARGET_BUDGET",
                "1".to_string(),
            ),
            (
                "NOVOVM_NATIVE_EXECUTION_TICK_EFFECTIVE_BUDGET",
                "1".to_string(),
            ),
            (
                "NOVOVM_NATIVE_EXECUTION_TICK_STORE_PATH",
                store.display().to_string(),
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
                "1".to_string(),
            ),
            (
                "NOVOVM_NATIVE_EXECUTION_PIPELINE_REQUIRE_PROGRESS",
                "true".to_string(),
            ),
            (
                "NOVOVM_NATIVE_EXECUTION_PIPELINE_QUIET_TICKS",
                "true".to_string(),
            ),
        ]
    };

    let receiver_peer = format!("{sender_node}={sender_addr}");
    let sender_peer = format!("{receiver_node}={receiver_addr}");
    let mut receiver_env = common(
        receiver_ticks,
        &receiver_store,
        receiver_node,
        &receiver_addr,
        &receiver_peer,
    );
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
    ]);

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
            "NOVOVM_NATIVE_EXECUTION_PIPELINE_INGRESS_FIXTURE_TX_COUNT",
            tx_count.to_string(),
        ),
        (
            "NOVOVM_NATIVE_EXECUTION_PIPELINE_INGRESS_MAX_PER_TICK",
            "1".to_string(),
        ),
        (
            "NOVOVM_NATIVE_EXECUTION_PIPELINE_MIN_INGRESS_SUBMITTED",
            tx_count.to_string(),
        ),
        (
            "NOVOVM_NATIVE_EXECUTION_PIPELINE_MIN_BROADCAST_TX",
            tx_count.to_string(),
        ),
        (
            "NOVOVM_NATIVE_EXECUTION_PIPELINE_MIN_BROADCAST_DISPATCH",
            tx_count.to_string(),
        ),
    ]);

    let receiver = {
        let mut cmd = Command::new(&node_bin);
        cmd.env_clear();
        for (key, value) in std::env::vars() {
            cmd.env(key, value);
        }
        for (key, value) in &receiver_env {
            cmd.env(key, value);
        }
        cmd.stdout(Stdio::piped()).stderr(Stdio::piped());
        cmd.spawn()
            .with_context(|| format!("spawn receiver node failed: {}", node_bin.display()))?
    };
    std::thread::sleep(std::time::Duration::from_millis(300));
    let sender_out = run_node_v1(&node_bin, sender_env.as_slice())?;
    let receiver_out = receiver
        .wait_with_output()
        .context("wait receiver node failed")?;
    let sender_summary = parse_summary_v1(&sender_out, "sender")?;
    let receiver_summary = parse_summary_v1(&receiver_out, "receiver")?;

    let validation = (|| -> Result<()> {
        for (label, summary) in [("sender", &sender_summary), ("receiver", &receiver_summary)] {
            require_eq_str(summary, "execution_kernel", "AOEM", label)?;
            require_eq_str(summary, "aoem_concurrency_owner", "AOEM_runtime", label)?;
            require_eq_str(
                summary,
                "host_concurrency_policy",
                "host_drives_lifecycle_only_no_rust_execution_scheduler",
                label,
            )?;
            require_min(summary, "network_ok_ticks", 1, label)?;
        }
        require_min(
            &sender_summary,
            "ingress_submitted_total",
            tx_count,
            "sender",
        )?;
        require_min(
            &sender_summary,
            "broadcast_tx_total_last",
            tx_count,
            "sender",
        )?;
        require_min(
            &sender_summary,
            "broadcast_dispatch_total_last",
            tx_count,
            "sender",
        )?;
        require_min(
            &receiver_summary,
            "aoem_executed_total",
            tx_count,
            "receiver",
        )?;
        require_min(&receiver_summary, "proof_ticks", tx_count, "receiver")?;
        require_min(&receiver_summary, "commit_ticks", tx_count, "receiver")?;
        require_min(
            &receiver_summary,
            "included_canonical_total",
            tx_count,
            "receiver",
        )?;
        if summary_u64(&receiver_summary, "queue_pending_last") != 0 {
            bail!(
                "receiver summary gate failed: queue_pending_last={} expected 0",
                summary_u64(&receiver_summary, "queue_pending_last")
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

    println!(
        "{}",
        serde_json::to_string_pretty(&serde_json::json!({
            "method": "supervm_native_pipeline_dual_node_gate",
            "accepted": true,
            "chain_id": chain_id,
            "tx_count": tx_count,
            "sender_addr": sender_addr,
            "receiver_addr": receiver_addr,
            "sender_summary": sender_summary,
            "receiver_summary": receiver_summary,
        }))
        .context("encode dual node gate report failed")?
    );
    Ok(())
}
