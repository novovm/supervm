use crate::audit;
use crate::cli::LifecycleArgs;
use crate::commands::up::{build_up_audit_record, warmup_effective_up_config};
use crate::error::CtlError;
use crate::model::lifecycle::{LifecycleAuditRecord, ManagedIngressManifest};
use crate::model::up::{EffectiveUpConfig, UpAuditRecord};
use crate::output;
use crate::runtime::{env, paths};
use serde_json::{json, Map, Value};
use std::fs::{self, OpenOptions};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::thread;
use std::time::Duration;

const DEFAULT_RELEASE_ROOT: &str = "artifacts/runtime/releases";
const DEFAULT_RUNTIME_STATE_FILE: &str = "artifacts/runtime/lifecycle/state.json";
const DEFAULT_RUNTIME_PID_FILE: &str = "artifacts/runtime/lifecycle/novovm-up.pid";
const DEFAULT_RUNTIME_LOG_DIR: &str = "artifacts/runtime/lifecycle/logs";
const DEFAULT_START_GRACE_SECONDS: u64 = 6;
const DEFAULT_UPGRADE_HEALTH_SECONDS: u64 = 12;

struct StartLifecycleResult {
    audit: UpAuditRecord,
    managed_ingress: Option<ManagedIngressManifest>,
}

pub fn run(args: LifecycleArgs) -> Result<(), CtlError> {
    let command_name = "lifecycle";
    let audit_path = env::resolve_audit_file(args.audit_file.as_deref());
    let result = inner_run(args);
    match result {
        Ok(record) => {
            if let Some(audit_file) = audit_path.as_deref() {
                audit::append_success_jsonl(audit_file, command_name, &record)?;
            }
            output::print_lifecycle_summary(
                &record.action,
                &record.runtime_state_file,
                &record.current_release,
                &record.previous_release,
                record.running,
                record.pid,
                record.current_profile.as_deref(),
                record.current_role_profile.as_deref(),
                record.current_runtime_profile.as_deref(),
                record.node_group.as_deref(),
                record.upgrade_window.as_deref(),
                record.updated,
                record.applied,
                &record.reason,
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

fn inner_run(args: LifecycleArgs) -> Result<LifecycleAuditRecord, CtlError> {
    let action = normalize_action(&args.action)?;
    let repo_root = resolve_repo_root(args.repo_root.as_deref())?;
    let release_root = resolve_path(
        &repo_root,
        args.release_root.as_deref(),
        DEFAULT_RELEASE_ROOT,
    );
    let runtime_state_path = resolve_path(
        &repo_root,
        args.runtime_state_file.as_deref(),
        DEFAULT_RUNTIME_STATE_FILE,
    );
    let runtime_pid_path = resolve_path(
        &repo_root,
        args.runtime_pid_file.as_deref(),
        DEFAULT_RUNTIME_PID_FILE,
    );
    let runtime_log_dir = resolve_path(
        &repo_root,
        args.runtime_log_dir.as_deref(),
        DEFAULT_RUNTIME_LOG_DIR,
    );
    let state_exists = runtime_state_path.exists();
    let mut state = load_state_or_default(&runtime_state_path)?;
    let mut updated = false;
    let mut applied = false;
    let mut registered_release = None;
    let mut target_release = None;
    let mut rollback_release = None;
    let mut launch_audit = None;
    let mut managed_ingress = None;
    let reason: String;

    match action.as_str() {
        "status" => reason = "status_snapshot".to_string(),
        "register" => {
            let version = require_trimmed(args.version.as_deref(), "register requires --version")?;
            let node_binary_from = require_trimmed(
                args.node_binary_from.as_deref(),
                "register requires --node-binary-from",
            )?;
            let release_dir = release_root.join(version);
            let node_source = resolve_input_path(&repo_root, node_binary_from);
            if !node_source.exists() {
                return Err(CtlError::BinaryNotFound(format!(
                    "node source binary not found: {}",
                    node_source.display()
                )));
            }
            if !args.dry_run {
                fs::create_dir_all(&release_dir).map_err(|e| {
                    CtlError::FileWriteFailed(format!(
                        "create release dir `{}`: {e}",
                        release_dir.display()
                    ))
                })?;
                copy_overwrite(
                    &node_source,
                    &release_dir.join(binary_file_name("novovm-node")),
                )?;
                if let Some(gateway_from) = args.gateway_binary_from.as_deref() {
                    let gateway_source = resolve_input_path(&repo_root, gateway_from);
                    if !gateway_source.exists() {
                        return Err(CtlError::BinaryNotFound(format!(
                            "gateway source binary not found: {}",
                            gateway_source.display()
                        )));
                    }
                    copy_overwrite(
                        &gateway_source,
                        &release_dir.join(binary_file_name("novovm-evm-gateway")),
                    )?;
                }
            }
            let current_release = string_at(&state, &["current_release"]).unwrap_or_default();
            if args.set_current || current_release.is_empty() {
                if !current_release.is_empty() && current_release != version {
                    set_string_path(&mut state, &["previous_release"], current_release);
                }
                set_string_path(&mut state, &["current_release"], version.to_string());
                updated = true;
            }
            registered_release = Some(version.to_string());
            applied = !args.dry_run;
            reason = "registered_release".to_string();
            if updated && !args.dry_run {
                write_state(&runtime_state_path, &state)?;
            }
        }
        "set-runtime" => {
            apply_template_runtime(
                &mut state,
                &repo_root,
                args.runtime_template_file.as_deref(),
            )?;
            apply_runtime_update(&mut state, &args);
            updated = true;
            reason = if args.restart_after_set_runtime {
                "runtime_updated_and_restarted".to_string()
            } else {
                "runtime_updated".to_string()
            };
            if args.print_effective_state {
                print_effective_state(&state)?;
            }
            if !args.dry_run {
                write_state(&runtime_state_path, &state)?;
            }
            if args.restart_after_set_runtime {
                if args.dry_run {
                    let start_result = start_from_state(
                        &args,
                        &state,
                        &release_root,
                        &runtime_pid_path,
                        &runtime_log_dir,
                        args.start_grace_seconds
                            .unwrap_or(DEFAULT_START_GRACE_SECONDS),
                    )?;
                    launch_audit = Some(start_result.audit);
                    managed_ingress = start_result.managed_ingress;
                } else {
                    let _ = stop_managed_process(&runtime_pid_path)?;
                    let start_result = start_from_state(
                        &args,
                        &state,
                        &release_root,
                        &runtime_pid_path,
                        &runtime_log_dir,
                        args.start_grace_seconds
                            .unwrap_or(DEFAULT_START_GRACE_SECONDS),
                    )?;
                    launch_audit = Some(start_result.audit);
                    managed_ingress = start_result.managed_ingress;
                }
                applied = !args.dry_run;
            } else {
                applied = !args.dry_run;
            }
        }
        "set-policy" => {
            if args.node_group.is_none() && args.upgrade_window.is_none() {
                return Err(CtlError::InvalidArgument(
                    "set-policy requires --node-group or --upgrade-window".to_string(),
                ));
            }
            apply_policy_update(&mut state, &args);
            updated = true;
            applied = !args.dry_run;
            reason = "policy_updated".to_string();
            if args.print_effective_state {
                print_effective_state(&state)?;
            }
            if !args.dry_run {
                write_state(&runtime_state_path, &state)?;
            }
        }
        "start" => {
            if let Some(version) = args.version.as_deref() {
                let current_release = string_at(&state, &["current_release"]).unwrap_or_default();
                if !current_release.is_empty() && current_release != version {
                    set_string_path(&mut state, &["previous_release"], current_release);
                }
                set_string_path(&mut state, &["current_release"], version.to_string());
            }
            ensure_current_release(&state, "start requires current release or --version")?;
            apply_template_runtime(
                &mut state,
                &repo_root,
                args.runtime_template_file.as_deref(),
            )?;
            apply_runtime_update(&mut state, &args);
            updated = true;
            let (running, _) = managed_process_status(&runtime_pid_path)?;
            if running {
                if args.force && !args.dry_run {
                    let _ = stop_managed_process(&runtime_pid_path)?;
                } else if !args.force {
                    return Err(CtlError::IntegrationFailed("lifecycle start refused because managed process is already running; use --force to replace it".to_string()));
                }
            }
            if args.print_effective_state {
                print_effective_state(&state)?;
            }
            if !args.dry_run {
                write_state(&runtime_state_path, &state)?;
            }
            let start_result = start_from_state(
                &args,
                &state,
                &release_root,
                &runtime_pid_path,
                &runtime_log_dir,
                args.start_grace_seconds
                    .unwrap_or(DEFAULT_START_GRACE_SECONDS),
            )?;
            launch_audit = Some(start_result.audit);
            managed_ingress = start_result.managed_ingress;
            applied = !args.dry_run;
            reason = if args.force {
                "started_after_force_replace".to_string()
            } else {
                "started".to_string()
            };
        }
        "stop" => {
            let (was_running, _) = if args.dry_run {
                managed_process_status(&runtime_pid_path)?
            } else {
                stop_managed_process(&runtime_pid_path)?
            };
            applied = !args.dry_run && was_running;
            reason = if was_running {
                "stopped".to_string()
            } else {
                "already_stopped".to_string()
            };
        }
        "upgrade" => {
            let target = require_trimmed(
                args.target_version.as_deref(),
                "upgrade requires --target-version",
            )?;
            let old_release = ensure_current_release(
                &state,
                "upgrade requires current release in lifecycle state",
            )?
            .to_string();
            if let Some(required_group) = args.require_node_group.as_deref() {
                let current_group =
                    string_at(&state, &["governance", "node_group"]).unwrap_or_default();
                if current_group != required_group {
                    return Err(CtlError::IntegrationFailed(format!(
                        "upgrade blocked by node group guard: required={} current={}",
                        required_group, current_group
                    )));
                }
            }
            apply_template_runtime(
                &mut state,
                &repo_root,
                args.runtime_template_file.as_deref(),
            )?;
            apply_runtime_update(&mut state, &args);
            updated = true;
            target_release = Some(target.to_string());
            reason = "upgraded".to_string();
            resolve_release_node_binary(&release_root, target)?;
            if args.print_effective_state {
                let mut state_preview = state.clone();
                set_string_path(
                    &mut state_preview,
                    &["previous_release"],
                    old_release.clone(),
                );
                set_string_path(&mut state_preview, &["current_release"], target.to_string());
                print_effective_state(&state_preview)?;
            }
            if args.dry_run {
                set_string_path(&mut state, &["previous_release"], old_release);
                set_string_path(&mut state, &["current_release"], target.to_string());
            } else {
                let _ = stop_managed_process(&runtime_pid_path)?;
                set_string_path(&mut state, &["previous_release"], old_release.clone());
                set_string_path(&mut state, &["current_release"], target.to_string());
                write_state(&runtime_state_path, &state)?;
                match start_from_state(
                    &args,
                    &state,
                    &release_root,
                    &runtime_pid_path,
                    &runtime_log_dir,
                    args.start_grace_seconds
                        .unwrap_or(DEFAULT_START_GRACE_SECONDS),
                ) {
                    Ok(start_result) => {
                        let health_wait = args
                            .upgrade_health_seconds
                            .unwrap_or(DEFAULT_UPGRADE_HEALTH_SECONDS);
                        if health_wait > 0 {
                            thread::sleep(Duration::from_secs(health_wait));
                        }
                        let (healthy, _) = managed_process_status(&runtime_pid_path)?;
                        if !healthy {
                            let _ = stop_managed_process(&runtime_pid_path)?;
                            set_string_path(&mut state, &["current_release"], old_release.clone());
                            set_string_path(&mut state, &["previous_release"], target.to_string());
                            write_state(&runtime_state_path, &state)?;
                            let _ = start_from_state(
                                &args,
                                &state,
                                &release_root,
                                &runtime_pid_path,
                                &runtime_log_dir,
                                args.start_grace_seconds
                                    .unwrap_or(DEFAULT_START_GRACE_SECONDS),
                            )?;
                            return Err(CtlError::IntegrationFailed(format!(
                                "upgrade health check failed after switching to {}",
                                target
                            )));
                        }
                        launch_audit = Some(start_result.audit);
                        managed_ingress = start_result.managed_ingress;
                        applied = true;
                    }
                    Err(err) => {
                        let _ = stop_managed_process(&runtime_pid_path)?;
                        set_string_path(&mut state, &["current_release"], old_release.clone());
                        set_string_path(&mut state, &["previous_release"], target.to_string());
                        write_state(&runtime_state_path, &state)?;
                        let _ = start_from_state(
                            &args,
                            &state,
                            &release_root,
                            &runtime_pid_path,
                            &runtime_log_dir,
                            args.start_grace_seconds
                                .unwrap_or(DEFAULT_START_GRACE_SECONDS),
                        )?;
                        return Err(CtlError::IntegrationFailed(format!(
                            "upgrade failed and restored previous release {}: {}",
                            old_release, err
                        )));
                    }
                }
            }
        }
        "rollback" => {
            let current_release = ensure_current_release(
                &state,
                "rollback requires current release in lifecycle state",
            )?
            .to_string();
            let target = args
                .rollback_version
                .as_deref()
                .filter(|v| !v.trim().is_empty())
                .map(|v| v.trim().to_string())
                .or_else(|| string_at(&state, &["previous_release"]))
                .ok_or_else(|| {
                    CtlError::InvalidArgument(
                        "rollback requires --rollback-version or previous_release in state"
                            .to_string(),
                    )
                })?;
            apply_template_runtime(
                &mut state,
                &repo_root,
                args.runtime_template_file.as_deref(),
            )?;
            apply_runtime_update(&mut state, &args);
            updated = true;
            rollback_release = Some(target.clone());
            reason = "rolled_back".to_string();
            resolve_release_node_binary(&release_root, &target)?;
            if args.print_effective_state {
                let mut state_preview = state.clone();
                set_string_path(&mut state_preview, &["current_release"], target.clone());
                set_string_path(
                    &mut state_preview,
                    &["previous_release"],
                    current_release.clone(),
                );
                print_effective_state(&state_preview)?;
            }
            if args.dry_run {
                set_string_path(&mut state, &["current_release"], target.clone());
                set_string_path(&mut state, &["previous_release"], current_release.clone());
            } else {
                let _ = stop_managed_process(&runtime_pid_path)?;
                set_string_path(&mut state, &["current_release"], target.clone());
                set_string_path(&mut state, &["previous_release"], current_release.clone());
                write_state(&runtime_state_path, &state)?;
                let start_result = start_from_state(
                    &args,
                    &state,
                    &release_root,
                    &runtime_pid_path,
                    &runtime_log_dir,
                    args.start_grace_seconds
                        .unwrap_or(DEFAULT_START_GRACE_SECONDS),
                )?;
                launch_audit = Some(start_result.audit);
                managed_ingress = start_result.managed_ingress;
                applied = true;
            }
        }
        _ => {
            return Err(CtlError::IntegrationFailed(format!(
                "lifecycle action `{}` not implemented in novovmctl",
                action
            )))
        }
    }

    if args.print_effective_state
        && !matches!(
            action.as_str(),
            "set-runtime" | "set-policy" | "start" | "upgrade" | "rollback"
        )
    {
        print_effective_state(&state)?;
    }

    let (running, pid) = managed_process_status(&runtime_pid_path)?;
    Ok(LifecycleAuditRecord {
        action,
        repo_root: repo_root.display().to_string(),
        release_root: release_root.display().to_string(),
        runtime_state_file: runtime_state_path.display().to_string(),
        runtime_pid_file: runtime_pid_path.display().to_string(),
        runtime_log_dir: runtime_log_dir.display().to_string(),
        state_exists,
        updated,
        applied,
        running,
        pid,
        current_release: string_at(&state, &["current_release"]).unwrap_or_default(),
        previous_release: string_at(&state, &["previous_release"]).unwrap_or_default(),
        registered_release,
        target_release,
        rollback_release,
        current_profile: string_at(&state, &["runtime", "profile"]),
        current_role_profile: string_at(&state, &["runtime", "role_profile"]),
        current_runtime_profile: string_at(&state, &["runtime", "overlay_route_runtime_profile"]),
        node_group: string_at(&state, &["governance", "node_group"]),
        upgrade_window: string_at(&state, &["governance", "upgrade_window"]),
        launch_audit,
        managed_ingress,
        reason,
        state,
    })
}

fn normalize_action(raw: &str) -> Result<String, CtlError> {
    let action = raw.trim().to_ascii_lowercase();
    if action.is_empty() {
        return Ok("status".to_string());
    }
    match action.as_str() {
        "register" | "start" | "stop" | "status" | "upgrade" | "rollback" | "set-runtime"
        | "set-policy" => Ok(action),
        _ => Err(CtlError::InvalidArgument(format!(
            "unsupported lifecycle action: {}",
            raw
        ))),
    }
}

fn resolve_repo_root(explicit: Option<&str>) -> Result<PathBuf, CtlError> {
    let root = if let Some(path) = explicit {
        PathBuf::from(path)
    } else {
        std::env::current_dir()
            .map_err(|e| CtlError::FileReadFailed(format!("resolve current dir: {e}")))?
    };
    Ok(if root.is_absolute() {
        root
    } else {
        std::env::current_dir()
            .map_err(|e| CtlError::FileReadFailed(format!("resolve current dir: {e}")))?
            .join(root)
    })
}

fn resolve_path(root: &Path, explicit: Option<&str>, default_rel: &str) -> PathBuf {
    let candidate = PathBuf::from(explicit.unwrap_or(default_rel));
    if candidate.is_absolute() {
        candidate
    } else {
        root.join(candidate)
    }
}

fn resolve_input_path(root: &Path, raw: &str) -> PathBuf {
    let candidate = PathBuf::from(raw);
    if candidate.is_absolute() {
        candidate
    } else {
        root.join(candidate)
    }
}

fn load_state_or_default(path: &Path) -> Result<Value, CtlError> {
    match fs::read_to_string(path) {
        Ok(raw) => {
            let mut value = serde_json::from_str::<Value>(&raw).map_err(|e| {
                CtlError::FileReadFailed(format!("parse state file `{}`: {e}", path.display()))
            })?;
            normalize_state_shape(&mut value);
            Ok(value)
        }
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => Ok(default_state()),
        Err(err) => Err(CtlError::FileReadFailed(format!(
            "read state file `{}`: {err}",
            path.display()
        ))),
    }
}

fn default_state() -> Value {
    json!({"version":1,"current_release":"","previous_release":"","runtime":{},"governance":{"node_group":"","upgrade_window":""}})
}

fn normalize_state_shape(state: &mut Value) {
    let root = ensure_object_path(state, &[]);
    root.entry("version".to_string())
        .or_insert_with(|| json!(1));
    root.entry("current_release".to_string())
        .or_insert_with(|| Value::String(String::new()));
    root.entry("previous_release".to_string())
        .or_insert_with(|| Value::String(String::new()));
    let runtime = ensure_object_path(state, &["runtime"]);
    runtime
        .entry("profile".to_string())
        .or_insert_with(|| Value::String("prod".to_string()));
    runtime
        .entry("role_profile".to_string())
        .or_insert_with(|| Value::String("full".to_string()));
    let governance = ensure_object_path(state, &["governance"]);
    governance
        .entry("node_group".to_string())
        .or_insert_with(|| Value::String(String::new()));
    governance
        .entry("upgrade_window".to_string())
        .or_insert_with(|| Value::String(String::new()));
}

fn write_state(path: &Path, state: &Value) -> Result<(), CtlError> {
    if let Some(parent) = path.parent() {
        if !parent.as_os_str().is_empty() {
            fs::create_dir_all(parent).map_err(|e| {
                CtlError::FileWriteFailed(format!(
                    "create state parent dir `{}`: {e}",
                    path.display()
                ))
            })?;
        }
    }
    let rendered = serde_json::to_string_pretty(state).map_err(|e| {
        CtlError::FileWriteFailed(format!("serialize state `{}`: {e}", path.display()))
    })?;
    fs::write(path, rendered).map_err(|e| {
        CtlError::FileWriteFailed(format!("write state file `{}`: {e}", path.display()))
    })
}

fn print_effective_state(state: &Value) -> Result<(), CtlError> {
    let rendered = serde_json::to_string_pretty(state)
        .map_err(|e| CtlError::FileWriteFailed(format!("serialize effective state: {e}")))?;
    println!("{rendered}");
    Ok(())
}
fn apply_template_runtime(
    state: &mut Value,
    repo_root: &Path,
    template_file: Option<&str>,
) -> Result<(), CtlError> {
    let Some(template_file) = template_file.filter(|v| !v.trim().is_empty()) else {
        return Ok(());
    };
    let template_path = resolve_input_path(repo_root, template_file);
    let raw = fs::read_to_string(&template_path).map_err(|e| {
        CtlError::FileReadFailed(format!(
            "read runtime template `{}`: {e}",
            template_path.display()
        ))
    })?;
    let template_json = serde_json::from_str::<Value>(&raw).map_err(|e| {
        CtlError::FileReadFailed(format!(
            "parse runtime template `{}`: {e}",
            template_path.display()
        ))
    })?;
    let template_runtime = template_json
        .get("runtime")
        .filter(|v| v.is_object())
        .unwrap_or(&template_json);
    if let Some(obj) = template_runtime.as_object() {
        let runtime = ensure_object_path(state, &["runtime"]);
        for key in [
            "profile",
            "role_profile",
            "overlay_route_mode",
            "overlay_route_runtime_file",
            "overlay_route_runtime_profile",
            "overlay_route_relay_directory_file",
            "use_node_watch_mode",
            "poll_ms",
            "node_watch_batch_max_files",
            "idle_exit_seconds",
            "overlay_route_auto_profile_enabled",
            "overlay_route_auto_profile_state_file",
            "overlay_route_auto_profile_profiles",
            "overlay_route_auto_profile_min_hold_seconds",
            "overlay_route_auto_profile_switch_margin",
            "overlay_route_auto_profile_switchback_cooldown_seconds",
            "overlay_route_auto_profile_recheck_seconds",
            "overlay_route_auto_profile_binary_path",
        ] {
            if let Some(value) = obj.get(key) {
                runtime.insert(key.to_string(), value.clone());
            }
        }
    }
    Ok(())
}

fn apply_runtime_update(state: &mut Value, args: &LifecycleArgs) {
    let runtime = ensure_object_path(state, &["runtime"]);
    set_opt_string(runtime, "profile", args.profile.as_deref());
    set_opt_string(runtime, "role_profile", args.role_profile.as_deref());
    set_opt_string(
        runtime,
        "overlay_route_mode",
        args.overlay_route_mode.as_deref(),
    );
    set_opt_string(
        runtime,
        "overlay_route_runtime_file",
        args.overlay_route_runtime_file.as_deref(),
    );
    set_opt_string(
        runtime,
        "overlay_route_runtime_profile",
        args.overlay_route_runtime_profile.as_deref(),
    );
    set_opt_string(
        runtime,
        "overlay_route_relay_directory_file",
        args.overlay_route_relay_directory_file.as_deref(),
    );
    if args.use_node_watch_mode {
        runtime.insert("use_node_watch_mode".to_string(), Value::Bool(true));
    }
    set_opt_u64(runtime, "poll_ms", args.poll_ms);
    set_opt_usize(
        runtime,
        "node_watch_batch_max_files",
        args.node_watch_batch_max_files,
    );
    set_opt_u64(runtime, "idle_exit_seconds", args.idle_exit_seconds);
    if args.auto_profile_enabled {
        runtime.insert(
            "overlay_route_auto_profile_enabled".to_string(),
            Value::Bool(true),
        );
    }
    set_opt_string(
        runtime,
        "overlay_route_auto_profile_state_file",
        args.auto_profile_state_file.as_deref(),
    );
    set_opt_string(
        runtime,
        "overlay_route_auto_profile_profiles",
        args.auto_profile_profiles.as_deref(),
    );
    set_opt_u64(
        runtime,
        "overlay_route_auto_profile_min_hold_seconds",
        args.auto_profile_min_hold_seconds,
    );
    set_opt_f64(
        runtime,
        "overlay_route_auto_profile_switch_margin",
        args.auto_profile_switch_margin,
    );
    set_opt_u64(
        runtime,
        "overlay_route_auto_profile_switchback_cooldown_seconds",
        args.auto_profile_switchback_cooldown_seconds,
    );
    set_opt_u64(
        runtime,
        "overlay_route_auto_profile_recheck_seconds",
        args.auto_profile_recheck_seconds,
    );
    set_opt_string(
        runtime,
        "overlay_route_auto_profile_binary_path",
        args.policy_cli_binary_file.as_deref(),
    );
}

fn apply_policy_update(state: &mut Value, args: &LifecycleArgs) {
    let governance = ensure_object_path(state, &["governance"]);
    set_opt_string(governance, "node_group", args.node_group.as_deref());
    set_opt_string(governance, "upgrade_window", args.upgrade_window.as_deref());
}

fn ensure_current_release<'a>(state: &'a Value, err: &str) -> Result<&'a str, CtlError> {
    state
        .get("current_release")
        .and_then(|v| v.as_str())
        .filter(|v| !v.trim().is_empty())
        .ok_or_else(|| CtlError::InvalidArgument(err.to_string()))
}

fn resolve_release_node_binary(release_root: &Path, version: &str) -> Result<String, CtlError> {
    let node_bin = release_root
        .join(version)
        .join(binary_file_name("novovm-node"));
    if !node_bin.exists() {
        return Err(CtlError::BinaryNotFound(format!(
            "registered release node binary not found: {}",
            node_bin.display()
        )));
    }
    Ok(node_bin.display().to_string())
}

fn binary_file_name(base: &str) -> String {
    if cfg!(windows) {
        format!("{base}.exe")
    } else {
        base.to_string()
    }
}

fn copy_overwrite(from: &Path, to: &Path) -> Result<(), CtlError> {
    if to.exists() {
        let _ = fs::remove_file(to);
    }
    fs::copy(from, to).map_err(|e| {
        CtlError::FileWriteFailed(format!(
            "copy `{}` -> `{}`: {e}",
            from.display(),
            to.display()
        ))
    })?;
    Ok(())
}

fn managed_process_status(pid_file_path: &Path) -> Result<(bool, Option<u32>), CtlError> {
    let Some(pid) = read_pid_file(pid_file_path)? else {
        return Ok((false, None));
    };
    Ok((is_process_running(pid), Some(pid)))
}

fn read_pid_file(pid_file_path: &Path) -> Result<Option<u32>, CtlError> {
    if !pid_file_path.exists() {
        return Ok(None);
    }
    let raw = fs::read_to_string(pid_file_path).map_err(|e| {
        CtlError::FileReadFailed(format!("read pid file `{}`: {e}", pid_file_path.display()))
    })?;
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return Ok(None);
    }
    let pid = trimmed.parse::<u32>().map_err(|e| {
        CtlError::FileReadFailed(format!("parse pid file `{}`: {e}", pid_file_path.display()))
    })?;
    Ok(Some(pid))
}

fn stop_managed_process(pid_file_path: &Path) -> Result<(bool, Option<u32>), CtlError> {
    let pid = read_pid_file(pid_file_path)?;
    let Some(pid) = pid else {
        if pid_file_path.exists() {
            let _ = fs::remove_file(pid_file_path);
        }
        return Ok((false, None));
    };
    let was_running = is_process_running(pid);
    if was_running {
        terminate_process(pid)?;
    }
    if pid_file_path.exists() {
        let _ = fs::remove_file(pid_file_path);
    }
    Ok((was_running, Some(pid)))
}

fn start_from_state(
    args: &LifecycleArgs,
    state: &Value,
    release_root: &Path,
    runtime_pid_path: &Path,
    runtime_log_dir: &Path,
    start_grace_seconds: u64,
) -> Result<StartLifecycleResult, CtlError> {
    let current_release = ensure_current_release(state, "current release missing")?;
    let node_bin = resolve_release_node_binary(release_root, current_release)?;
    let runtime_policy_bin = string_at(
        state,
        &["runtime", "overlay_route_auto_profile_binary_path"],
    );
    let policy_bin = paths::resolve_policy_binary(
        args.policy_cli_binary_file
            .as_deref()
            .or(runtime_policy_bin.as_deref()),
    )?;
    let log_file = runtime_log_dir
        .join(format!("novovm-node-{}.log", current_release))
        .display()
        .to_string();
    let mut effective = EffectiveUpConfig {
        profile: string_at(state, &["runtime", "profile"]).unwrap_or_else(|| "prod".to_string()),
        role_profile: string_at(state, &["runtime", "role_profile"])
            .unwrap_or_else(|| "full".to_string()),
        overlay_route_runtime_file: string_at(state, &["runtime", "overlay_route_runtime_file"]),
        overlay_route_runtime_profile: string_at(
            state,
            &["runtime", "overlay_route_runtime_profile"],
        ),
        overlay_route_mode: string_at(state, &["runtime", "overlay_route_mode"]),
        overlay_route_relay_directory_file: string_at(
            state,
            &["runtime", "overlay_route_relay_directory_file"],
        ),
        policy_bin,
        node_bin,
        use_node_watch_mode: bool_at(state, &["runtime", "use_node_watch_mode"]).unwrap_or(false),
        poll_ms: u64_at(state, &["runtime", "poll_ms"]),
        node_watch_batch_max_files: usize_at(state, &["runtime", "node_watch_batch_max_files"]),
        idle_exit_seconds: u64_at(state, &["runtime", "idle_exit_seconds"]),
        no_gateway: bool_at(state, &["runtime", "no_gateway"]).unwrap_or(false),
        lean_io: bool_at(state, &["runtime", "lean_io"]).unwrap_or(false),
        supervisor_poll_ms: u64_at(state, &["runtime", "supervisor_poll_ms"]),
        gateway_bind: string_at(state, &["runtime", "gateway_bind"]),
        gateway_spool_dir: string_at(state, &["runtime", "gateway_spool_dir"]),
        gateway_max_requests: u64_at(state, &["runtime", "gateway_max_requests"]).map(|v| v as u32),
        ops_wire_dir: string_at(state, &["runtime", "ops_wire_dir"]),
        ops_wire_watch_done_dir: string_at(state, &["runtime", "ops_wire_watch_done_dir"]),
        ops_wire_watch_failed_dir: string_at(state, &["runtime", "ops_wire_watch_failed_dir"]),
        ops_wire_watch_drop_failed: bool_at(state, &["runtime", "ops_wire_watch_drop_failed"])
            .unwrap_or(false),
        auto_profile_state_file: string_at(
            state,
            &["runtime", "overlay_route_auto_profile_state_file"],
        ),
        auto_profile_profiles: string_at(
            state,
            &["runtime", "overlay_route_auto_profile_profiles"],
        ),
        auto_profile_min_hold_seconds: u64_at(
            state,
            &["runtime", "overlay_route_auto_profile_min_hold_seconds"],
        ),
        auto_profile_switch_margin: f64_at(
            state,
            &["runtime", "overlay_route_auto_profile_switch_margin"],
        ),
        auto_profile_switchback_cooldown_seconds: u64_at(
            state,
            &[
                "runtime",
                "overlay_route_auto_profile_switchback_cooldown_seconds",
            ],
        ),
        auto_profile_recheck_seconds: u64_at(
            state,
            &["runtime", "overlay_route_auto_profile_recheck_seconds"],
        ),
        auto_profile_enabled: bool_at(state, &["runtime", "overlay_route_auto_profile_enabled"])
            .unwrap_or(false),
        skip_policy_warmup: false,
        foreground: false,
        dry_run: args.dry_run,
        audit_file: args.audit_file.clone(),
        log_file: Some(log_file),
    };
    let (auto_profile_decision, mut reason) = warmup_effective_up_config(&mut effective)?;
    let mut managed_ingress = None;
    let launched = if effective.dry_run {
        reason = "dry_run".to_string();
        false
    } else {
        let ingress_manifest = ensure_managed_ingress_dir(effective.log_file.as_deref())?;
        let pid = spawn_effective_node(&effective, runtime_pid_path, &ingress_manifest)?;
        managed_ingress = Some(ingress_manifest.clone());
        if start_grace_seconds > 0 {
            thread::sleep(Duration::from_secs(start_grace_seconds));
        }
        if !is_process_running(pid) {
            return Err(CtlError::IntegrationFailed(format!(
                "node exited during lifecycle start grace window: pid={} ingress_dir={} bootstrap_file={} bootstrap_seeded={} opsw1_count_after_seed={}",
                pid,
                ingress_manifest.dir,
                ingress_manifest.bootstrap_file,
                ingress_manifest.bootstrap_seeded,
                ingress_manifest.opsw1_count_after_seed
            )));
        }
        true
    };
    Ok(StartLifecycleResult {
        audit: build_up_audit_record(&effective, auto_profile_decision, launched, reason),
        managed_ingress,
    })
}

fn spawn_effective_node(
    effective: &EffectiveUpConfig,
    runtime_pid_path: &Path,
    managed_ingress: &ManagedIngressManifest,
) -> Result<u32, CtlError> {
    if let Some(parent) = runtime_pid_path.parent() {
        if !parent.as_os_str().is_empty() {
            fs::create_dir_all(parent).map_err(|e| {
                CtlError::FileWriteFailed(format!(
                    "create pid parent dir `{}`: {e}",
                    runtime_pid_path.display()
                ))
            })?;
        }
    }
    let mut command = Command::new(&effective.node_bin);
    command.arg("--profile").arg(&effective.profile);
    command.arg("--role-profile").arg(&effective.role_profile);
    if let Some(v) = effective.overlay_route_runtime_file.as_deref() {
        command.arg("--overlay-route-runtime-file").arg(v);
    }
    if let Some(v) = effective.overlay_route_runtime_profile.as_deref() {
        command.arg("--overlay-route-runtime-profile").arg(v);
    }
    if let Some(v) = effective.overlay_route_mode.as_deref() {
        command.arg("--overlay-route-mode").arg(v);
    }
    if let Some(v) = effective.overlay_route_relay_directory_file.as_deref() {
        command.arg("--overlay-route-relay-directory-file").arg(v);
    }
    command.env_remove("NOVOVM_TX_WIRE_FILE");
    command.env_remove("NOVOVM_OPS_WIRE_FILE");
    command.env_remove("NOVOVM_OPS_WIRE_DIR");
    command.env("NOVOVM_D1_INGRESS_MODE", "ops_wire_v1");
    command.env("NOVOVM_OPS_WIRE_DIR", &managed_ingress.dir);
    command.env("NOVOVM_OPS_WIRE_WATCH", "1");
    if let Some(v) = effective.poll_ms {
        command.env("NOVOVM_OPS_WIRE_WATCH_POLL_MS", v.to_string());
    }
    if let Some(v) = effective.node_watch_batch_max_files {
        command.env("NOVOVM_OPS_WIRE_WATCH_BATCH_MAX_FILES", v.to_string());
    }
    if let Some(v) = effective.idle_exit_seconds {
        command.env("NOVOVM_OPS_WIRE_WATCH_IDLE_EXIT_SECONDS", v.to_string());
    }
    if let Some(log_file) = effective.log_file.as_deref() {
        let log_path = PathBuf::from(log_file);
        if let Some(parent) = log_path.parent() {
            if !parent.as_os_str().is_empty() {
                fs::create_dir_all(parent).map_err(|e| {
                    CtlError::FileWriteFailed(format!(
                        "create log parent dir `{}`: {e}",
                        log_path.display()
                    ))
                })?;
            }
        }
        let stdout_file = OpenOptions::new()
            .create(true)
            .append(true)
            .open(&log_path)
            .map_err(|e| {
                CtlError::FileWriteFailed(format!("open log file `{}`: {e}", log_path.display()))
            })?;
        let stderr_file = stdout_file.try_clone().map_err(|e| {
            CtlError::FileWriteFailed(format!("clone log handle `{}`: {e}", log_path.display()))
        })?;
        command.stdout(Stdio::from(stdout_file));
        command.stderr(Stdio::from(stderr_file));
    }
    let child = command
        .spawn()
        .map_err(|e| CtlError::ProcessLaunchFailed(format!("spawn lifecycle managed node: {e}")))?;
    let pid = child.id();
    fs::write(runtime_pid_path, format!("{pid}\n")).map_err(|e| {
        CtlError::FileWriteFailed(format!(
            "write pid file `{}`: {e}",
            runtime_pid_path.display()
        ))
    })?;
    Ok(pid)
}

fn ensure_managed_ingress_dir(log_file: Option<&str>) -> Result<ManagedIngressManifest, CtlError> {
    let ingress_dir = if let Some(log_file) = log_file {
        let log_path = PathBuf::from(log_file);
        let log_dir = log_path
            .parent()
            .map(Path::to_path_buf)
            .unwrap_or_else(|| PathBuf::from("artifacts/runtime/lifecycle/logs"));
        log_dir.join("managed-ingress")
    } else {
        PathBuf::from("artifacts/runtime/lifecycle/logs/managed-ingress")
    };
    fs::create_dir_all(&ingress_dir).map_err(|e| {
        CtlError::FileWriteFailed(format!(
            "create managed ingress dir `{}`: {e}",
            ingress_dir.display()
        ))
    })?;
    let bootstrap_file = ingress_dir.join("__bootstrap.opsw1");
    let has_opsw1 = fs::read_dir(&ingress_dir)
        .map_err(|e| {
            CtlError::FileWriteFailed(format!(
                "read managed ingress dir `{}`: {e}",
                ingress_dir.display()
            ))
        })?
        .filter_map(Result::ok)
        .any(|entry| {
            entry
                .path()
                .extension()
                .and_then(|ext| ext.to_str())
                .map(|ext| ext.eq_ignore_ascii_case("opsw1"))
                .unwrap_or(false)
        });
    let bootstrap_seeded = !has_opsw1 || !bootstrap_file.exists();
    if bootstrap_seeded {
        fs::write(&bootstrap_file, minimal_ops_wire_v1_bootstrap_bytes()).map_err(|e| {
            CtlError::FileWriteFailed(format!(
                "write managed ingress bootstrap `{}`: {e}",
                bootstrap_file.display()
            ))
        })?;
    }
    let opsw1_count_after_seed = fs::read_dir(&ingress_dir)
        .map_err(|e| {
            CtlError::FileWriteFailed(format!(
                "read managed ingress dir `{}` after seed: {e}",
                ingress_dir.display()
            ))
        })?
        .filter_map(Result::ok)
        .filter(|entry| {
            entry
                .path()
                .extension()
                .and_then(|ext| ext.to_str())
                .map(|ext| ext.eq_ignore_ascii_case("opsw1"))
                .unwrap_or(false)
        })
        .count();
    Ok(ManagedIngressManifest {
        dir: ingress_dir.display().to_string(),
        bootstrap_file: bootstrap_file.display().to_string(),
        bootstrap_seeded,
        bootstrap_bytes: minimal_ops_wire_v1_bootstrap_bytes().len(),
        opsw1_count_after_seed,
    })
}

fn minimal_ops_wire_v1_bootstrap_bytes() -> [u8; 13] {
    [b'A', b'O', b'V', b'2', 0, 1, 0, 0, 0, 0, 0, 0, 0]
}

fn is_process_running(pid: u32) -> bool {
    #[cfg(windows)]
    {
        Command::new("tasklist")
            .args(["/FI", &format!("PID eq {pid}"), "/FO", "CSV", "/NH"])
            .output()
            .map(|o| {
                o.status.success()
                    && String::from_utf8_lossy(&o.stdout).contains(&format!("\"{}\"", pid))
            })
            .unwrap_or(false)
    }
    #[cfg(not(windows))]
    {
        Command::new("kill")
            .arg("-0")
            .arg(pid.to_string())
            .status()
            .map(|s| s.success())
            .unwrap_or(false)
    }
}

fn terminate_process(pid: u32) -> Result<(), CtlError> {
    #[cfg(windows)]
    {
        let status = Command::new("taskkill")
            .args(["/PID", &pid.to_string(), "/T", "/F"])
            .status()
            .map_err(|e| CtlError::ProcessLaunchFailed(format!("taskkill pid {}: {}", pid, e)))?;
        if !status.success() {
            return Err(CtlError::IntegrationFailed(format!(
                "taskkill failed for pid {}",
                pid
            )));
        }
    }
    #[cfg(not(windows))]
    {
        let status = Command::new("kill")
            .arg(pid.to_string())
            .status()
            .map_err(|e| CtlError::ProcessLaunchFailed(format!("kill pid {}: {}", pid, e)))?;
        if !status.success() {
            return Err(CtlError::IntegrationFailed(format!(
                "kill failed for pid {}",
                pid
            )));
        }
    }
    Ok(())
}

fn require_trimmed<'a>(value: Option<&'a str>, error: &str) -> Result<&'a str, CtlError> {
    value
        .map(str::trim)
        .filter(|v| !v.is_empty())
        .ok_or_else(|| CtlError::InvalidArgument(error.to_string()))
}

fn ensure_object_path<'a>(value: &'a mut Value, path: &[&str]) -> &'a mut Map<String, Value> {
    let mut current = value;
    for key in path {
        if !current.is_object() {
            *current = Value::Object(Map::new());
        }
        let obj = current.as_object_mut().expect("object ensured");
        current = obj
            .entry((*key).to_string())
            .or_insert_with(|| Value::Object(Map::new()));
    }
    if !current.is_object() {
        *current = Value::Object(Map::new());
    }
    current.as_object_mut().expect("object ensured")
}

fn set_opt_string(target: &mut Map<String, Value>, key: &str, value: Option<&str>) {
    if let Some(v) = value {
        target.insert(key.to_string(), Value::String(v.to_string()));
    }
}
fn set_opt_u64(target: &mut Map<String, Value>, key: &str, value: Option<u64>) {
    if let Some(v) = value {
        target.insert(key.to_string(), Value::Number(v.into()));
    }
}
fn set_opt_usize(target: &mut Map<String, Value>, key: &str, value: Option<usize>) {
    if let Some(v) = value {
        target.insert(key.to_string(), Value::Number((v as u64).into()));
    }
}
fn set_opt_f64(target: &mut Map<String, Value>, key: &str, value: Option<f64>) {
    if let Some(v) = value {
        if let Some(number) = serde_json::Number::from_f64(v) {
            target.insert(key.to_string(), Value::Number(number));
        }
    }
}
fn set_string_path(state: &mut Value, path: &[&str], value: String) {
    let (parent_path, leaf) = path.split_at(path.len() - 1);
    let parent = ensure_object_path(state, parent_path);
    parent.insert(leaf[0].to_string(), Value::String(value));
}
fn string_at(value: &Value, path: &[&str]) -> Option<String> {
    let mut current = value;
    for key in path {
        current = current.get(*key)?;
    }
    match current {
        Value::String(s) => Some(s.clone()),
        Value::Number(n) => Some(n.to_string()),
        Value::Bool(v) => Some(v.to_string()),
        _ => None,
    }
}
fn bool_at(value: &Value, path: &[&str]) -> Option<bool> {
    let mut current = value;
    for key in path {
        current = current.get(*key)?;
    }
    current.as_bool()
}
fn u64_at(value: &Value, path: &[&str]) -> Option<u64> {
    let mut current = value;
    for key in path {
        current = current.get(*key)?;
    }
    current.as_u64()
}
fn usize_at(value: &Value, path: &[&str]) -> Option<usize> {
    u64_at(value, path).map(|v| v as usize)
}
fn f64_at(value: &Value, path: &[&str]) -> Option<f64> {
    let mut current = value;
    for key in path {
        current = current.get(*key)?;
    }
    current.as_f64()
}
