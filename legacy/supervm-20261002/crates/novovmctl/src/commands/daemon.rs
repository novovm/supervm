use std::env::current_dir;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::thread;
use std::time::Duration;

use crate::audit;
use crate::cli::DaemonArgs;
use crate::commands::up;
use crate::error::CtlError;
use crate::integration::node_binary;
use crate::model::daemon::DaemonAuditRecord;
use crate::output;
use crate::runtime::env;

pub fn run(args: DaemonArgs) -> Result<(), CtlError> {
    let command_name = "daemon";
    let audit_path = env::resolve_audit_file(args.up.audit_file.as_deref());
    let result = inner_run(args);

    match result {
        Ok(record) => {
            if let Some(audit_file) = audit_path.as_deref() {
                audit::append_success_jsonl(audit_file, command_name, &record)?;
            }
            output::print_daemon_summary(
                &record.profile,
                &record.role_profile,
                &record.policy_bin,
                &record.node_bin,
                record.no_gateway,
                record.lean_io,
                record.use_node_watch_mode,
                record.gateway_spool_dir.as_deref(),
                record.launched_cycles,
                record.restart_delay_seconds,
                record.supervisor_poll_ms,
                record.max_restarts,
                &record.last_reason,
            );
            output::print_success_json(command_name, &record)?;
            Ok(())
        }
        Err(err) => {
            if let Some(audit_file) = audit_path.as_deref() {
                let _ = audit::append_error_jsonl(audit_file, command_name, &err);
            }
            output::print_error_json(command_name, &err);
            Err(err)
        }
    }
}

#[allow(unused_assignments)]
fn inner_run(args: DaemonArgs) -> Result<DaemonAuditRecord, CtlError> {
    let mut launched_cycles = 0u64;
    let mut last_reason: Option<String> = None;
    let mut last_policy_bin: Option<String> = None;
    let mut last_node_bin: Option<String> = None;
    let mut last_auto_profile = None;
    let supervisor_poll_ms = args.supervisor_poll_ms.max(1);
    let spool_dir = resolve_repo_path(&args.spool_dir)?;
    let effective_lean_io = args.lean_io || args.up.profile.eq_ignore_ascii_case("prod");
    let gateway_enabled = !args.no_gateway;
    let gateway_bind = if gateway_enabled {
        validate_gateway_bind(&args.up.profile, &args.gateway_bind)?;
        Some(args.gateway_bind.clone())
    } else {
        None
    };
    let gateway_spool_dir = if gateway_enabled || args.up.use_node_watch_mode {
        Some(spool_dir.display().to_string())
    } else {
        None
    };
    let (ops_wire_done_dir, ops_wire_failed_dir, ops_wire_watch_drop_failed) =
        prepare_spool_layout(&spool_dir, args.up.use_node_watch_mode, effective_lean_io)?;

    if args.build_before_run {
        build_required_binaries()?;
    }

    loop {
        let mut effective = up::build_effective_up_config(&args.up)?;
        effective.no_gateway = args.no_gateway;
        effective.lean_io = effective_lean_io;
        effective.supervisor_poll_ms = Some(supervisor_poll_ms);
        effective.gateway_bind = gateway_bind.clone();
        effective.gateway_spool_dir = gateway_spool_dir.clone();
        effective.gateway_max_requests = if gateway_enabled {
            Some(args.gateway_max_requests)
        } else {
            None
        };
        effective.ops_wire_dir = if args.up.use_node_watch_mode {
            Some(spool_dir.display().to_string())
        } else {
            None
        };
        effective.ops_wire_watch_done_dir = ops_wire_done_dir.clone();
        effective.ops_wire_watch_failed_dir = ops_wire_failed_dir.clone();
        effective.ops_wire_watch_drop_failed = ops_wire_watch_drop_failed;
        effective.foreground = true;
        let (auto_profile_decision, _reason) = up::warmup_effective_up_config(&mut effective)?;
        last_policy_bin = Some(effective.policy_bin.clone());
        last_node_bin = Some(effective.node_bin.clone());
        last_auto_profile = auto_profile_decision;

        if effective.dry_run {
            last_reason = Some("dry_run".to_string());
            break;
        }

        launched_cycles = launched_cycles.saturating_add(1);
        match node_binary::launch_node(&effective) {
            Ok(()) => {
                last_reason = Some("node_cycle_completed".to_string());
            }
            Err(err) => {
                last_reason = Some("node_cycle_failed".to_string());
                if args.max_restarts > 0 && launched_cycles >= args.max_restarts {
                    return Err(err);
                }
                thread::sleep(Duration::from_secs(args.restart_delay_seconds.max(1)));
                continue;
            }
        }

        if args.max_restarts > 0 && launched_cycles >= args.max_restarts {
            last_reason = Some("max_restarts_reached".to_string());
            break;
        }
        thread::sleep(Duration::from_millis(supervisor_poll_ms));
    }

    Ok(DaemonAuditRecord {
        profile: args.up.profile,
        role_profile: args.up.role_profile,
        policy_bin: last_policy_bin.unwrap_or_default(),
        node_bin: last_node_bin.unwrap_or_default(),
        no_gateway: args.no_gateway,
        build_before_run: args.build_before_run,
        lean_io: effective_lean_io,
        use_node_watch_mode: args.up.use_node_watch_mode,
        poll_ms: args.up.poll_ms,
        supervisor_poll_ms,
        node_watch_batch_max_files: args.up.node_watch_batch_max_files,
        idle_exit_seconds: args.up.idle_exit_seconds,
        gateway_bind,
        gateway_spool_dir,
        gateway_max_requests: if !args.no_gateway {
            Some(args.gateway_max_requests)
        } else {
            None
        },
        ops_wire_dir: if args.up.use_node_watch_mode {
            Some(spool_dir.display().to_string())
        } else {
            None
        },
        ops_wire_watch_done_dir: ops_wire_done_dir,
        ops_wire_watch_failed_dir: ops_wire_failed_dir,
        ops_wire_watch_drop_failed,
        restart_delay_seconds: args.restart_delay_seconds.max(1),
        max_restarts: args.max_restarts,
        launched_cycles,
        last_reason: last_reason.unwrap_or_else(|| "daemon_idle".to_string()),
        dry_run: args.up.dry_run,
        auto_profile_decision: last_auto_profile,
    })
}

fn build_required_binaries() -> Result<(), CtlError> {
    let status = Command::new("cargo")
        .arg("build")
        .arg("-p")
        .arg("novovm-node")
        .arg("-p")
        .arg("novovm-rollout-policy")
        .arg("-p")
        .arg("novovmctl")
        .status()
        .map_err(|e| {
            CtlError::ProcessLaunchFailed(format!("cargo build for daemon preflight: {e}"))
        })?;
    if !status.success() {
        return Err(CtlError::IntegrationFailed(
            "daemon build-before-run cargo build exited non-zero".to_string(),
        ));
    }
    Ok(())
}

fn resolve_repo_path(value: &str) -> Result<PathBuf, CtlError> {
    let path = PathBuf::from(value);
    if path.is_absolute() {
        return Ok(path);
    }
    let cwd = current_dir().map_err(|e| {
        CtlError::FileReadFailed(format!("resolve current dir for daemon path: {e}"))
    })?;
    Ok(cwd.join(path))
}

fn prepare_spool_layout(
    spool_dir: &Path,
    use_node_watch_mode: bool,
    lean_io: bool,
) -> Result<(Option<String>, Option<String>, bool), CtlError> {
    if !use_node_watch_mode {
        return Ok((None, None, false));
    }
    fs::create_dir_all(spool_dir).map_err(|e| {
        CtlError::FileWriteFailed(format!(
            "create daemon spool dir `{}`: {e}",
            spool_dir.display()
        ))
    })?;
    if lean_io {
        return Ok((None, None, true));
    }
    let done_dir = spool_dir.join("done");
    let failed_dir = spool_dir.join("failed");
    fs::create_dir_all(&done_dir).map_err(|e| {
        CtlError::FileWriteFailed(format!(
            "create daemon done dir `{}`: {e}",
            done_dir.display()
        ))
    })?;
    fs::create_dir_all(&failed_dir).map_err(|e| {
        CtlError::FileWriteFailed(format!(
            "create daemon failed dir `{}`: {e}",
            failed_dir.display()
        ))
    })?;
    Ok((
        Some(done_dir.display().to_string()),
        Some(failed_dir.display().to_string()),
        false,
    ))
}

fn validate_gateway_bind(profile: &str, gateway_bind: &str) -> Result<(), CtlError> {
    if !profile.eq_ignore_ascii_case("prod") {
        return Ok(());
    }
    let host = gateway_bind
        .split(':')
        .next()
        .unwrap_or_default()
        .trim_matches(|c| c == '[' || c == ']')
        .trim()
        .to_ascii_lowercase();
    let is_loopback = matches!(host.as_str(), "127.0.0.1" | "localhost" | "::1");
    if is_loopback {
        return Ok(());
    }
    Err(CtlError::InvalidArgument(format!(
        "prod daemon rejects public gateway bind by default: {}",
        gateway_bind
    )))
}
