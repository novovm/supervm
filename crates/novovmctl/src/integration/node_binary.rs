use std::process::{Command, Stdio};
use std::time::{SystemTime, UNIX_EPOCH};

use crate::error::CtlError;
use crate::model::up::EffectiveUpConfig;

pub fn launch_node(config: &EffectiveUpConfig) -> Result<(), CtlError> {
    let mut cmd = Command::new(&config.node_bin);
    const MANUAL_ROUTE_ENV_LOCK_KEYS: [&str; 30] = [
        "NOVOVM_L3_POLICY_MODE",
        "NOVOVM_L3_PROFILE_STICKY_MARGIN",
        "NOVOVM_L3_PROFILE_RUNTIME_FEEDBACK_SCALE",
        "NOVOVM_L3_PROFILE_CANDIDATE_LIMIT",
        "NOVOVM_L3_PROFILE_MODE_POLICY",
        "NOVOVM_L3_PROFILE_MODE_POLICY_GOVERNANCE",
        "NOVOVM_L3_PROFILE_MODE_MIN",
        "NOVOVM_L3_PROFILE_MODE_MAX",
        "NOVOVM_L3_PROFILE_FAMILY",
        "NOVOVM_L3_PROFILE_FAMILY_GOVERNANCE",
        "NOVOVM_L3_PROFILE_FAMILY_MIN",
        "NOVOVM_L3_PROFILE_FAMILY_MAX",
        "NOVOVM_L3_POLICY_PROFILE_VERSION",
        "NOVOVM_L3_POLICY_PROFILE_VERSION_GOVERNANCE",
        "NOVOVM_L3_POLICY_PROFILE_DEFAULT",
        "NOVOVM_OVERLAY_ROUTE_MODE",
        "NOVOVM_OVERLAY_ROUTE_REGION",
        "NOVOVM_OVERLAY_ROUTE_STRATEGY",
        "NOVOVM_OVERLAY_ROUTE_ENFORCE_MULTI_HOP",
        "NOVOVM_OVERLAY_ROUTE_HOP_COUNT",
        "NOVOVM_OVERLAY_ROUTE_MIN_HOPS",
        "NOVOVM_OVERLAY_ROUTE_HOP_SLOT_SECONDS",
        "NOVOVM_OVERLAY_ROUTE_RELAY_BUCKETS",
        "NOVOVM_OVERLAY_ROUTE_RELAY_SET_SIZE",
        "NOVOVM_OVERLAY_ROUTE_RELAY_ROTATE_SECONDS",
        "NOVOVM_OVERLAY_ROUTE_RELAY_CANDIDATES",
        "NOVOVM_OVERLAY_ROUTE_FORCE_STRATEGY",
        "NOVOVM_OVERLAY_ROUTE_FORCE_RELAY_ID",
        "NOVOVM_OVERLAY_ROUTE_FORCE_HOP_COUNT",
        "NOVOVM_OVERLAY_ROUTE_ID",
    ];
    let sched_token = format!(
        "novovmctl-{}-{}",
        std::process::id(),
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_millis())
            .unwrap_or(0)
    );

    cmd.arg("--profile").arg(&config.profile);
    cmd.arg("--role-profile").arg(&config.role_profile);
    for key in MANUAL_ROUTE_ENV_LOCK_KEYS {
        cmd.env_remove(key);
    }
    cmd.env("NOVOVM_SCHED_SOURCE", "novovmctl");
    cmd.env("NOVOVM_SCHED_TOKEN", &sched_token);
    cmd.env("NOVOVM_SCHED_REQUIRED", "1");
    cmd.env("NOVOVM_SINGLE_SOURCE_STRICT", "1");
    cmd.env("NOVOVM_SUPERVM_MANUAL_ROUTE_ENV_LOCK", "1");

    if let Some(v) = config.overlay_route_runtime_file.as_deref() {
        cmd.arg("--overlay-route-runtime-file").arg(v);
    }
    if let Some(v) = config.overlay_route_runtime_profile.as_deref() {
        cmd.arg("--overlay-route-runtime-profile").arg(v);
    }
    if let Some(v) = config.overlay_route_mode.as_deref() {
        cmd.arg("--overlay-route-mode").arg(v);
    }
    if let Some(v) = config.overlay_route_relay_directory_file.as_deref() {
        cmd.arg("--overlay-route-relay-directory-file").arg(v);
    }

    if config.use_node_watch_mode {
        cmd.env("NOVOVM_OPS_WIRE_WATCH", "true");
    }
    if let Some(v) = config.ops_wire_dir.as_deref() {
        cmd.env("NOVOVM_OPS_WIRE_DIR", v);
    }
    if let Some(v) = config.poll_ms {
        cmd.env("NOVOVM_OPS_WIRE_WATCH_POLL_MS", v.to_string());
    }
    if let Some(v) = config.node_watch_batch_max_files {
        cmd.env("NOVOVM_OPS_WIRE_WATCH_BATCH_MAX_FILES", v.to_string());
    }
    if let Some(v) = config.idle_exit_seconds {
        cmd.env("NOVOVM_OPS_WIRE_WATCH_IDLE_EXIT_SECONDS", v.to_string());
    }
    if let Some(v) = config.ops_wire_watch_done_dir.as_deref() {
        cmd.env("NOVOVM_OPS_WIRE_WATCH_DONE_DIR", v);
    }
    if let Some(v) = config.ops_wire_watch_failed_dir.as_deref() {
        cmd.env("NOVOVM_OPS_WIRE_WATCH_FAILED_DIR", v);
    }
    if config.ops_wire_watch_drop_failed {
        cmd.env("NOVOVM_OPS_WIRE_WATCH_DROP_FAILED", "1");
    }
    if let Some(v) = config.gateway_bind.as_deref() {
        cmd.env("NOVOVM_GATEWAY_BIND", v);
    }
    if let Some(v) = config.gateway_spool_dir.as_deref() {
        cmd.env("NOVOVM_GATEWAY_SPOOL_DIR", v);
    }
    if let Some(v) = config.gateway_max_requests {
        cmd.env("NOVOVM_GATEWAY_MAX_REQUESTS", v.to_string());
    }

    if config.foreground {
        let status = cmd
            .status()
            .map_err(|e| CtlError::ProcessLaunchFailed(format!("launch node foreground: {e}")))?;
        if !status.success() {
            return Err(CtlError::IntegrationFailed(
                "novovm-node exited non-zero".to_string(),
            ));
        }
        return Ok(());
    }

    if let Some(log_file) = config.log_file.as_deref() {
        let file = std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(log_file)
            .map_err(|e| CtlError::FileWriteFailed(format!("open log file `{log_file}`: {e}")))?;
        let stderr_file = file
            .try_clone()
            .map_err(|e| CtlError::FileWriteFailed(format!("clone log file handle: {e}")))?;
        cmd.stdout(Stdio::from(file));
        cmd.stderr(Stdio::from(stderr_file));
    }

    cmd.spawn()
        .map_err(|e| CtlError::ProcessLaunchFailed(format!("spawn node background: {e}")))?;

    Ok(())
}
