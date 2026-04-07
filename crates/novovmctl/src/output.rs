use serde::Serialize;
use serde_json::json;
use std::time::{SystemTime, UNIX_EPOCH};

use crate::error::CtlError;

pub fn now_unix_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as u64
}

pub fn host_name() -> String {
    std::env::var("COMPUTERNAME")
        .or_else(|_| std::env::var("HOSTNAME"))
        .unwrap_or_else(|_| "unknown-host".to_string())
}

pub fn print_success_json<T: Serialize>(command: &str, data: &T) -> Result<(), CtlError> {
    let envelope = json!({
        "ok": true,
        "command": command,
        "timestamp_unix_ms": now_unix_ms(),
        "host": host_name(),
        "data": data,
    });

    let rendered = serde_json::to_string_pretty(&envelope)
        .map_err(|e| CtlError::FileWriteFailed(format!("serialize success json: {e}")))?;
    println!("{rendered}");
    Ok(())
}

pub fn print_error_json(command: &str, err: &CtlError) {
    let envelope = json!({
        "ok": false,
        "command": command,
        "timestamp_unix_ms": now_unix_ms(),
        "host": host_name(),
        "error": {
            "kind": error_kind(err),
            "message": err.to_string()
        }
    });

    match serde_json::to_string_pretty(&envelope) {
        Ok(rendered) => eprintln!("{rendered}"),
        Err(_) => eprintln!(
            "{{\"ok\":false,\"command\":\"{}\",\"error\":{{\"kind\":\"Unknown\",\"message\":\"{}\"}}}}",
            command,
            err
        ),
    }
}

#[allow(clippy::too_many_arguments)]
pub fn print_up_summary(
    profile: &str,
    role_profile: &str,
    runtime_profile: Option<&str>,
    mode: Option<&str>,
    policy_bin: &str,
    node_bin: &str,
    launched: bool,
    reason: &str,
) {
    println!("[novovmctl] command=up ok=true");
    println!(
        "[novovmctl] profile={} role={} runtime_profile={} mode={}",
        profile,
        role_profile,
        runtime_profile.unwrap_or("-"),
        mode.unwrap_or("-")
    );
    println!(
        "[novovmctl] policy_bin={} node_bin={}",
        policy_bin, node_bin
    );
    println!("[novovmctl] launched={} reason={}", launched, reason);
}

#[allow(clippy::too_many_arguments)]
pub fn print_rollout_control_summary(
    queue_file: &str,
    plan_action: &str,
    controller_id: Option<&str>,
    slo_score: Option<f64>,
    violation: bool,
    circuit_block: bool,
    selected_profile: Option<&str>,
    decision_reason: &str,
    continue_dispatch: bool,
    max_concurrent_plans: Option<u32>,
    dispatch_pause_seconds: Option<u64>,
    applied: bool,
) {
    println!("[novovmctl] command=rollout-control ok=true");
    println!(
        "[novovmctl] queue={} action={} controller={}",
        queue_file,
        plan_action,
        controller_id.unwrap_or("-")
    );
    println!(
        "[novovmctl] slo_score={} violation={} circuit_block={} profile={}",
        slo_score
            .map(|v| format!("{v:.2}"))
            .unwrap_or_else(|| "-".to_string()),
        violation,
        circuit_block,
        selected_profile.unwrap_or("-")
    );
    println!(
        "[novovmctl] decision={} continue={} max_concurrent={} pause_s={}",
        decision_reason,
        continue_dispatch,
        max_concurrent_plans
            .map(|v| v.to_string())
            .unwrap_or_else(|| "-".to_string()),
        dispatch_pause_seconds
            .map(|v| v.to_string())
            .unwrap_or_else(|| "-".to_string())
    );
    println!("[novovmctl] applied={}", applied);
}

#[allow(clippy::too_many_arguments)]
pub fn print_daemon_summary(
    profile: &str,
    role_profile: &str,
    policy_bin: &str,
    node_bin: &str,
    no_gateway: bool,
    lean_io: bool,
    use_node_watch_mode: bool,
    spool_dir: Option<&str>,
    launched_cycles: u64,
    restart_delay_seconds: u64,
    supervisor_poll_ms: u64,
    max_restarts: u64,
    reason: &str,
) {
    println!("[novovmctl] command=daemon ok=true");
    println!(
        "[novovmctl] profile={} role={} no_gateway={} watch_mode={} lean_io={}",
        profile, role_profile, no_gateway, use_node_watch_mode, lean_io
    );
    println!(
        "[novovmctl] policy_bin={} node_bin={} spool_dir={}",
        policy_bin,
        node_bin,
        spool_dir.unwrap_or("-")
    );
    println!(
        "[novovmctl] launched_cycles={} restart_delay_s={} supervisor_poll_ms={} max_restarts={} reason={}",
        launched_cycles, restart_delay_seconds, supervisor_poll_ms, max_restarts, reason
    );
}

#[allow(clippy::too_many_arguments)]
pub fn print_lifecycle_summary(
    action: &str,
    runtime_state_file: &str,
    current_release: &str,
    previous_release: &str,
    running: bool,
    pid: Option<u32>,
    current_profile: Option<&str>,
    current_role_profile: Option<&str>,
    current_runtime_profile: Option<&str>,
    node_group: Option<&str>,
    upgrade_window: Option<&str>,
    updated: bool,
    applied: bool,
    reason: &str,
) {
    println!("[novovmctl] command=lifecycle ok=true");
    println!(
        "[novovmctl] action={} state_file={} current_release={} previous_release={}",
        action, runtime_state_file, current_release, previous_release
    );
    println!(
        "[novovmctl] running={} pid={} profile={} role={} runtime_profile={}",
        running,
        pid.map(|v| v.to_string())
            .unwrap_or_else(|| "-".to_string()),
        current_profile.unwrap_or("-"),
        current_role_profile.unwrap_or("-"),
        current_runtime_profile.unwrap_or("-")
    );
    println!(
        "[novovmctl] node_group={} upgrade_window={} updated={} applied={}",
        node_group.unwrap_or("-"),
        upgrade_window.unwrap_or("-"),
        updated,
        applied
    );
    println!("[novovmctl] reason={}", reason);
}

#[allow(clippy::too_many_arguments)]
pub fn print_rollout_summary(
    action: &str,
    plan_file: &str,
    controller_id: &str,
    operation_id: Option<&str>,
    enabled_node_count: usize,
    disabled_node_count: usize,
    enabled_groups: &[String],
    ok_count: usize,
    error_count: usize,
    applied: bool,
    reason: &str,
) {
    println!("[novovmctl] command=rollout ok=true");
    println!(
        "[novovmctl] action={} plan_file={} controller={} operation={}",
        action,
        plan_file,
        controller_id,
        operation_id.unwrap_or("-")
    );
    println!(
        "[novovmctl] enabled_nodes={} disabled_nodes={} groups={}",
        enabled_node_count,
        disabled_node_count,
        if enabled_groups.is_empty() {
            "-".to_string()
        } else {
            enabled_groups.join(",")
        }
    );
    println!(
        "[novovmctl] ok_count={} error_count={} applied={} reason={}",
        ok_count, error_count, applied, reason
    );
}

fn error_kind(err: &CtlError) -> &'static str {
    match err {
        CtlError::InvalidArgument(_) => "InvalidArgument",
        CtlError::FileReadFailed(_) => "FileReadFailed",
        CtlError::FileWriteFailed(_) => "FileWriteFailed",
        CtlError::BinaryNotFound(_) => "BinaryNotFound",
        CtlError::ProcessLaunchFailed(_) => "ProcessLaunchFailed",
        CtlError::IntegrationFailed(_) => "IntegrationFailed",
    }
}
