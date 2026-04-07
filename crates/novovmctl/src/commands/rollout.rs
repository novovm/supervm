use crate::audit;
use crate::cli::RolloutArgs;
use crate::error::CtlError;
use crate::model::rollout::{RolloutAuditRecord, RolloutNodeResult};
use crate::output;
use crate::runtime::env;
use serde::{Deserialize, Serialize};
use serde_json::to_string_pretty;
use std::collections::BTreeSet;
use std::env::current_dir;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, ExitStatus, Stdio};

const DEFAULT_GROUP_ORDER: &[&str] = &["canary", "stable"];

#[derive(Debug, Clone, Default, Deserialize, Serialize)]
struct PlanControllers {
    #[serde(default)]
    allowed_ids: Vec<String>,
}

#[derive(Debug, Clone, Default, Deserialize, Serialize)]
struct PlanGroup {
    #[serde(default)]
    name: String,
    #[serde(default)]
    max_failures: Option<usize>,
}

fn default_true() -> bool {
    true
}

#[derive(Debug, Clone, Default, Deserialize, Serialize)]
struct PlanNode {
    #[serde(default = "default_true")]
    enabled: bool,
    #[serde(default)]
    name: String,
    #[serde(default)]
    node_group: String,
    #[serde(default)]
    transport: String,
    #[serde(default)]
    remote_mode: String,
    #[serde(default)]
    repo_root: String,
    #[serde(default)]
    remote_repo_root: String,
    #[serde(default)]
    lifecycle_script_path: String,
    #[serde(default)]
    remote_host: String,
    #[serde(default)]
    remote_user: String,
    #[serde(default)]
    remote_port: Option<u16>,
    #[serde(default)]
    remote_shell: String,
    #[serde(default)]
    ssh_identity_file: String,
    #[serde(default)]
    ssh_known_hosts_file: String,
    #[serde(default)]
    ssh_strict_host_key: String,
    #[serde(default)]
    winrm_use_ssl: bool,
    #[serde(default)]
    winrm_port: Option<u16>,
    #[serde(default)]
    winrm_auth: String,
    #[serde(default)]
    winrm_operation_timeout_sec: Option<u64>,
    #[serde(default)]
    winrm_cred_user_env: String,
    #[serde(default)]
    winrm_cred_pass_env: String,
    #[serde(default)]
    upgrade_window: String,
}

#[derive(Debug, Clone, Default, Deserialize, Serialize)]
struct RolloutPlan {
    #[serde(default)]
    controllers: PlanControllers,
    #[serde(default)]
    group_order: Vec<String>,
    #[serde(default)]
    groups: Vec<PlanGroup>,
    #[serde(default)]
    nodes: Vec<PlanNode>,
}

#[derive(Debug, Clone)]
struct RolloutContext {
    repo_root: PathBuf,
    plan_path: PathBuf,
    plan: RolloutPlan,
    action: String,
    controller_id: String,
    operation_id: String,
    audit_file: Option<String>,
    group_order: Vec<String>,
    enabled_groups: Vec<String>,
    enabled_node_count: usize,
    disabled_node_count: usize,
    local_node_count: usize,
    ssh_node_count: usize,
    winrm_node_count: usize,
}

#[derive(Debug, Clone)]
struct LifecycleInvocation {
    action: String,
    repo_root: String,
    target_version: Option<String>,
    rollback_version: Option<String>,
    audit_file: Option<String>,
    overlay_route_mode: Option<String>,
    overlay_route_runtime_file: Option<String>,
    overlay_route_runtime_profile: Option<String>,
    overlay_route_relay_directory_file: Option<String>,
    enable_auto_profile: bool,
    auto_profile_state_file: Option<String>,
    auto_profile_profiles: Option<String>,
    auto_profile_min_hold_seconds: Option<u64>,
    auto_profile_switch_margin: Option<f64>,
    auto_profile_switchback_cooldown_seconds: Option<u64>,
    auto_profile_recheck_seconds: Option<u64>,
    auto_profile_binary_path: Option<String>,
    upgrade_health_seconds: Option<u64>,
    node_group: Option<String>,
    upgrade_window: Option<String>,
    require_node_group: Option<String>,
    dry_run: bool,
}

pub fn run(args: RolloutArgs) -> Result<(), CtlError> {
    let command_name = "rollout";
    let audit_path = env::resolve_audit_file(args.audit_file.as_deref());
    let result = inner_run(args);

    match result {
        Ok(record) => {
            if let Some(audit_file) = audit_path.as_deref() {
                audit::append_success_jsonl(audit_file, command_name, &record)?;
            }
            output::print_rollout_summary(
                &record.action,
                &record.plan_file,
                record.controller_id.as_deref().unwrap_or("-"),
                record.operation_id.as_deref(),
                record.enabled_node_count,
                record.disabled_node_count,
                &record.enabled_groups,
                record.ok_count,
                record.error_count,
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

fn inner_run(args: RolloutArgs) -> Result<RolloutAuditRecord, CtlError> {
    let action = normalize_action(&args.action)?;
    let repo_root =
        current_dir().map_err(|e| CtlError::FileReadFailed(format!("resolve current dir: {e}")))?;
    let plan_path = resolve_input_path(&repo_root, &args.plan_file);
    let raw = fs::read_to_string(&plan_path).map_err(|e| {
        CtlError::FileReadFailed(format!("read rollout plan `{}`: {e}", plan_path.display()))
    })?;
    let plan: RolloutPlan = serde_json::from_str(&raw).map_err(|e| {
        CtlError::FileReadFailed(format!("parse rollout plan `{}`: {e}", plan_path.display()))
    })?;

    if plan.nodes.is_empty() {
        return Err(CtlError::InvalidArgument(format!(
            "rollout plan has no nodes: {}",
            plan_path.display()
        )));
    }

    if args.print_effective_plan {
        let rendered = to_string_pretty(&plan)
            .map_err(|e| CtlError::FileWriteFailed(format!("serialize effective plan: {e}")))?;
        println!("{rendered}");
    }

    assert_controller_authorized(&plan, &args.controller_id)?;
    let group_order = resolve_group_order(&plan, &args.group_order);
    let enabled_nodes: Vec<&PlanNode> = plan.nodes.iter().filter(|node| node.enabled).collect();
    let enabled_node_count = enabled_nodes.len();
    let disabled_node_count = plan.nodes.len().saturating_sub(enabled_node_count);
    let mut enabled_groups = BTreeSet::new();
    let mut local_node_count = 0usize;
    let mut ssh_node_count = 0usize;
    let mut winrm_node_count = 0usize;

    for node in &enabled_nodes {
        enabled_groups.insert(normalized_node_group(node));
        match resolve_node_transport(node, &args.default_transport)? {
            Transport::Local => local_node_count += 1,
            Transport::Ssh => ssh_node_count += 1,
            Transport::WinRm => winrm_node_count += 1,
        }
    }

    let operation_id = args
        .operation_id
        .clone()
        .filter(|value| !value.trim().is_empty())
        .unwrap_or_else(default_operation_id);

    let context = RolloutContext {
        repo_root,
        plan_path,
        plan,
        action: action.clone(),
        controller_id: args.controller_id.clone(),
        operation_id,
        audit_file: args.audit_file.clone(),
        group_order,
        enabled_groups: enabled_groups.into_iter().collect(),
        enabled_node_count,
        disabled_node_count,
        local_node_count,
        ssh_node_count,
        winrm_node_count,
    };

    match action.as_str() {
        "status" => run_status(&args, &context),
        "set-policy" => run_set_policy(&args, &context),
        "upgrade" => run_upgrade(&args, &context),
        "rollback" => run_rollback(&args, &context),
        _ => Err(CtlError::IntegrationFailed(format!(
            "rollout action `{}` not implemented in novovmctl",
            action
        ))),
    }
}

fn run_status(
    args: &RolloutArgs,
    context: &RolloutContext,
) -> Result<RolloutAuditRecord, CtlError> {
    let mut results = Vec::new();
    let mut ok_count = 0usize;
    let mut error_count = 0usize;

    for group in &context.group_order {
        for node in nodes_by_group(&context.plan, group) {
            let invocation = LifecycleInvocation {
                action: "status".to_string(),
                repo_root: node_repo_root_for_invocation(&context.repo_root, node)?,
                target_version: None,
                rollback_version: None,
                audit_file: context.audit_file.clone(),
                overlay_route_mode: None,
                overlay_route_runtime_file: None,
                overlay_route_runtime_profile: None,
                overlay_route_relay_directory_file: None,
                enable_auto_profile: false,
                auto_profile_state_file: None,
                auto_profile_profiles: None,
                auto_profile_min_hold_seconds: None,
                auto_profile_switch_margin: None,
                auto_profile_switchback_cooldown_seconds: None,
                auto_profile_recheck_seconds: None,
                auto_profile_binary_path: None,
                upgrade_health_seconds: None,
                node_group: None,
                upgrade_window: None,
                require_node_group: None,
                dry_run: args.dry_run,
            };
            let result = invoke_lifecycle_action(args, node, group, &invocation)?;
            let failed = result.result == "error";
            tally_result(&result, &mut ok_count, &mut error_count);
            results.push(result);
            if failed && !args.continue_on_failure {
                return Err(CtlError::IntegrationFailed(format!(
                    "rollout status failed and stopped at node={}",
                    results
                        .last()
                        .map(|entry| entry.node.as_str())
                        .unwrap_or("-")
                )));
            }
        }
    }

    Ok(build_rollout_audit_record(
        args,
        context,
        ok_count,
        error_count,
        !args.dry_run,
        "status_completed".to_string(),
        results,
    ))
}

fn run_set_policy(
    args: &RolloutArgs,
    context: &RolloutContext,
) -> Result<RolloutAuditRecord, CtlError> {
    let mut results = Vec::new();
    let mut ok_count = 0usize;
    let mut error_count = 0usize;

    for group in &context.group_order {
        for node in nodes_by_group(&context.plan, group) {
            let invocation = LifecycleInvocation {
                action: "set-policy".to_string(),
                repo_root: node_repo_root_for_invocation(&context.repo_root, node)?,
                target_version: None,
                rollback_version: None,
                audit_file: context.audit_file.clone(),
                overlay_route_mode: None,
                overlay_route_runtime_file: None,
                overlay_route_runtime_profile: None,
                overlay_route_relay_directory_file: None,
                enable_auto_profile: false,
                auto_profile_state_file: None,
                auto_profile_profiles: None,
                auto_profile_min_hold_seconds: None,
                auto_profile_switch_margin: None,
                auto_profile_switchback_cooldown_seconds: None,
                auto_profile_recheck_seconds: None,
                auto_profile_binary_path: None,
                upgrade_health_seconds: None,
                node_group: Some(group.clone()),
                upgrade_window: trim_to_option(&node.upgrade_window),
                require_node_group: None,
                dry_run: args.dry_run,
            };
            let result = invoke_lifecycle_action(args, node, group, &invocation)?;
            let failed = result.result == "error";
            tally_result(&result, &mut ok_count, &mut error_count);
            results.push(result);
            if failed && !args.continue_on_failure {
                return Err(CtlError::IntegrationFailed(format!(
                    "rollout set-policy failed and stopped at node={}",
                    results
                        .last()
                        .map(|entry| entry.node.as_str())
                        .unwrap_or("-")
                )));
            }
        }
    }

    Ok(build_rollout_audit_record(
        args,
        context,
        ok_count,
        error_count,
        !args.dry_run,
        "policy_applied".to_string(),
        results,
    ))
}

fn run_upgrade(
    args: &RolloutArgs,
    context: &RolloutContext,
) -> Result<RolloutAuditRecord, CtlError> {
    let target_version = require_trimmed(
        args.target_version.as_deref(),
        "rollout upgrade requires --target-version",
    )?
    .to_string();
    let mut results = Vec::new();
    let mut ok_count = 0usize;
    let mut error_count = 0usize;

    for group in &context.group_order {
        let max_failures = group_max_failures(&context.plan, group, args.default_max_failures);
        let mut group_errors = 0usize;
        for node in nodes_by_group(&context.plan, group) {
            if !args.ignore_upgrade_window {
                let window = test_upgrade_window(node);
                if !window.in_window {
                    let blocked = RolloutNodeResult {
                        node: resolve_node_name(node),
                        node_group: group.clone(),
                        transport: resolve_node_transport(node, &args.default_transport)?
                            .as_str()
                            .to_string(),
                        lifecycle_action: "upgrade".to_string(),
                        target_version: Some(target_version.clone()),
                        rollback_version: args.rollback_version.clone(),
                        result: "blocked".to_string(),
                        error: Some(format!("upgrade blocked by window gate: {}", window.reason)),
                        dry_run: args.dry_run,
                    };
                    error_count += 1;
                    group_errors += 1;
                    results.push(blocked);
                    if !args.continue_on_failure {
                        return Err(CtlError::IntegrationFailed(format!(
                            "rollout upgrade blocked at node={}",
                            results
                                .last()
                                .map(|entry| entry.node.as_str())
                                .unwrap_or("-")
                        )));
                    }
                    if group_errors > max_failures {
                        return Err(CtlError::IntegrationFailed(format!(
                            "rollout upgrade failed over threshold: group={} errors={} max={}",
                            group, group_errors, max_failures
                        )));
                    }
                    continue;
                }
            }

            let invocation = LifecycleInvocation {
                action: "upgrade".to_string(),
                repo_root: node_repo_root_for_invocation(&context.repo_root, node)?,
                target_version: Some(target_version.clone()),
                rollback_version: None,
                audit_file: context.audit_file.clone(),
                overlay_route_mode: args.overlay_route_mode.clone(),
                overlay_route_runtime_file: args.overlay_route_runtime_file.clone(),
                overlay_route_runtime_profile: args.overlay_route_runtime_profile.clone(),
                overlay_route_relay_directory_file: args.overlay_route_relay_directory_file.clone(),
                enable_auto_profile: args.enable_auto_profile,
                auto_profile_state_file: args.auto_profile_state_file.clone(),
                auto_profile_profiles: args.auto_profile_profiles.clone(),
                auto_profile_min_hold_seconds: args.auto_profile_min_hold_seconds,
                auto_profile_switch_margin: args.auto_profile_switch_margin,
                auto_profile_switchback_cooldown_seconds: args
                    .auto_profile_switchback_cooldown_seconds,
                auto_profile_recheck_seconds: args.auto_profile_recheck_seconds,
                auto_profile_binary_path: args.auto_profile_binary_path.clone(),
                upgrade_health_seconds: Some(args.upgrade_health_seconds),
                node_group: None,
                upgrade_window: None,
                require_node_group: Some(group.clone()),
                dry_run: args.dry_run,
            };
            let result = invoke_lifecycle_action(args, node, group, &invocation)?;
            let failed = result.result == "error";
            tally_result(&result, &mut ok_count, &mut error_count);
            results.push(result);

            if failed {
                group_errors += 1;
                if args.auto_rollback_on_failure {
                    let rollback_invocation = LifecycleInvocation {
                        action: "rollback".to_string(),
                        repo_root: node_repo_root_for_invocation(&context.repo_root, node)?,
                        target_version: None,
                        rollback_version: args.rollback_version.clone(),
                        audit_file: context.audit_file.clone(),
                        overlay_route_mode: args.overlay_route_mode.clone(),
                        overlay_route_runtime_file: args.overlay_route_runtime_file.clone(),
                        overlay_route_runtime_profile: args.overlay_route_runtime_profile.clone(),
                        overlay_route_relay_directory_file: args
                            .overlay_route_relay_directory_file
                            .clone(),
                        enable_auto_profile: args.enable_auto_profile,
                        auto_profile_state_file: args.auto_profile_state_file.clone(),
                        auto_profile_profiles: args.auto_profile_profiles.clone(),
                        auto_profile_min_hold_seconds: args.auto_profile_min_hold_seconds,
                        auto_profile_switch_margin: args.auto_profile_switch_margin,
                        auto_profile_switchback_cooldown_seconds: args
                            .auto_profile_switchback_cooldown_seconds,
                        auto_profile_recheck_seconds: args.auto_profile_recheck_seconds,
                        auto_profile_binary_path: args.auto_profile_binary_path.clone(),
                        upgrade_health_seconds: None,
                        node_group: None,
                        upgrade_window: None,
                        require_node_group: None,
                        dry_run: args.dry_run,
                    };
                    let rollback_result =
                        invoke_lifecycle_action(args, node, group, &rollback_invocation)?;
                    tally_result(&rollback_result, &mut ok_count, &mut error_count);
                    results.push(rollback_result);
                }
                if !args.continue_on_failure {
                    return Err(CtlError::IntegrationFailed(format!(
                        "rollout upgrade failed and stopped at node={}",
                        results
                            .iter()
                            .rev()
                            .find(|entry| entry.lifecycle_action == "upgrade")
                            .map(|entry| entry.node.as_str())
                            .unwrap_or("-")
                    )));
                }
                if group_errors > max_failures {
                    return Err(CtlError::IntegrationFailed(format!(
                        "rollout upgrade failed over threshold: group={} errors={} max={}",
                        group, group_errors, max_failures
                    )));
                }
            }

            if args.pause_seconds_between_nodes > 0 && !args.dry_run {
                std::thread::sleep(std::time::Duration::from_secs(
                    args.pause_seconds_between_nodes,
                ));
            }
        }
    }

    if error_count > 0 {
        return Err(CtlError::IntegrationFailed(format!(
            "rollout upgrade completed with errors: {}",
            error_count
        )));
    }

    Ok(build_rollout_audit_record(
        args,
        context,
        ok_count,
        error_count,
        !args.dry_run,
        "upgrade_completed".to_string(),
        results,
    ))
}
fn run_rollback(
    args: &RolloutArgs,
    context: &RolloutContext,
) -> Result<RolloutAuditRecord, CtlError> {
    let mut results = Vec::new();
    let mut ok_count = 0usize;
    let mut error_count = 0usize;
    let mut reverse_groups = context.group_order.clone();
    reverse_groups.reverse();

    for group in &reverse_groups {
        for node in nodes_by_group(&context.plan, group) {
            let invocation = LifecycleInvocation {
                action: "rollback".to_string(),
                repo_root: node_repo_root_for_invocation(&context.repo_root, node)?,
                target_version: None,
                rollback_version: args.rollback_version.clone(),
                audit_file: context.audit_file.clone(),
                overlay_route_mode: args.overlay_route_mode.clone(),
                overlay_route_runtime_file: args.overlay_route_runtime_file.clone(),
                overlay_route_runtime_profile: args.overlay_route_runtime_profile.clone(),
                overlay_route_relay_directory_file: args.overlay_route_relay_directory_file.clone(),
                enable_auto_profile: args.enable_auto_profile,
                auto_profile_state_file: args.auto_profile_state_file.clone(),
                auto_profile_profiles: args.auto_profile_profiles.clone(),
                auto_profile_min_hold_seconds: args.auto_profile_min_hold_seconds,
                auto_profile_switch_margin: args.auto_profile_switch_margin,
                auto_profile_switchback_cooldown_seconds: args
                    .auto_profile_switchback_cooldown_seconds,
                auto_profile_recheck_seconds: args.auto_profile_recheck_seconds,
                auto_profile_binary_path: args.auto_profile_binary_path.clone(),
                upgrade_health_seconds: None,
                node_group: None,
                upgrade_window: None,
                require_node_group: None,
                dry_run: args.dry_run,
            };
            let result = invoke_lifecycle_action(args, node, group, &invocation)?;
            let failed = result.result == "error";
            tally_result(&result, &mut ok_count, &mut error_count);
            results.push(result);

            if failed && !args.continue_on_failure {
                return Err(CtlError::IntegrationFailed(format!(
                    "rollout rollback failed and stopped at node={}",
                    results
                        .last()
                        .map(|entry| entry.node.as_str())
                        .unwrap_or("-")
                )));
            }

            if args.pause_seconds_between_nodes > 0 && !args.dry_run {
                std::thread::sleep(std::time::Duration::from_secs(
                    args.pause_seconds_between_nodes,
                ));
            }
        }
    }

    if error_count > 0 {
        return Err(CtlError::IntegrationFailed(format!(
            "rollout rollback completed with errors: {}",
            error_count
        )));
    }

    Ok(build_rollout_audit_record(
        args,
        context,
        ok_count,
        error_count,
        !args.dry_run,
        "rollback_completed".to_string(),
        results,
    ))
}

fn build_rollout_audit_record(
    args: &RolloutArgs,
    context: &RolloutContext,
    ok_count: usize,
    error_count: usize,
    applied: bool,
    reason: String,
    results: Vec<RolloutNodeResult>,
) -> RolloutAuditRecord {
    RolloutAuditRecord {
        action: context.action.clone(),
        plan_file: context.plan_path.display().to_string(),
        controller_id: Some(context.controller_id.clone()),
        operation_id: Some(context.operation_id.clone()),
        allowed_controllers: context.plan.controllers.allowed_ids.clone(),
        group_order: context.group_order.clone(),
        enabled_groups: context.enabled_groups.clone(),
        enabled_node_count: context.enabled_node_count,
        disabled_node_count: context.disabled_node_count,
        local_node_count: context.local_node_count,
        ssh_node_count: context.ssh_node_count,
        winrm_node_count: context.winrm_node_count,
        target_version: args.target_version.clone(),
        rollback_version: args.rollback_version.clone(),
        ok_count,
        error_count,
        applied,
        dry_run: args.dry_run,
        reason,
        results,
    }
}

fn normalize_action(raw: &str) -> Result<String, CtlError> {
    let action = raw.trim().to_ascii_lowercase();
    if action.is_empty() {
        return Ok("status".to_string());
    }
    match action.as_str() {
        "status" | "set-policy" | "upgrade" | "rollback" => Ok(action),
        _ => Err(CtlError::InvalidArgument(format!(
            "unsupported rollout action: {}",
            raw
        ))),
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

fn assert_controller_authorized(plan: &RolloutPlan, controller_id: &str) -> Result<(), CtlError> {
    let allowed: Vec<&str> = plan
        .controllers
        .allowed_ids
        .iter()
        .map(String::as_str)
        .filter(|value| !value.trim().is_empty())
        .collect();
    if allowed.is_empty() || allowed.contains(&controller_id) {
        return Ok(());
    }
    Err(CtlError::IntegrationFailed(format!(
        "controller is not authorized by plan: controller_id={}",
        controller_id
    )))
}

fn resolve_group_order(plan: &RolloutPlan, requested: &[String]) -> Vec<String> {
    if !plan.group_order.is_empty() {
        return plan.group_order.clone();
    }
    if !requested.is_empty() {
        return requested.to_vec();
    }
    DEFAULT_GROUP_ORDER
        .iter()
        .map(|value| (*value).to_string())
        .collect()
}

fn nodes_by_group<'a>(plan: &'a RolloutPlan, group: &str) -> Vec<&'a PlanNode> {
    plan.nodes
        .iter()
        .filter(|node| node.enabled && normalized_node_group(node) == group)
        .collect()
}

fn normalized_node_group(node: &PlanNode) -> String {
    let raw = node.node_group.trim();
    if raw.is_empty() {
        "stable".to_string()
    } else {
        raw.to_string()
    }
}

fn group_max_failures(plan: &RolloutPlan, group: &str, fallback: usize) -> usize {
    plan.groups
        .iter()
        .find(|entry| entry.name.trim() == group)
        .and_then(|entry| entry.max_failures)
        .unwrap_or(fallback)
}

fn default_operation_id() -> String {
    format!("rollout-{}-{}", output::now_unix_ms(), std::process::id())
}

#[derive(Debug, Clone, Copy)]
enum Transport {
    Local,
    Ssh,
    WinRm,
}

impl Transport {
    fn as_str(self) -> &'static str {
        match self {
            Self::Local => "local",
            Self::Ssh => "ssh",
            Self::WinRm => "winrm",
        }
    }
}

fn resolve_node_transport(node: &PlanNode, default_transport: &str) -> Result<Transport, CtlError> {
    let raw = if !node.transport.trim().is_empty() {
        node.transport.trim()
    } else if !node.remote_mode.trim().is_empty() {
        node.remote_mode.trim()
    } else {
        default_transport.trim()
    };
    match raw.to_ascii_lowercase().as_str() {
        "local" => Ok(Transport::Local),
        "ssh" => Ok(Transport::Ssh),
        "winrm" => Ok(Transport::WinRm),
        other => Err(CtlError::InvalidArgument(format!(
            "unsupported node transport: {}",
            other
        ))),
    }
}

fn resolve_node_name(node: &PlanNode) -> String {
    if !node.name.trim().is_empty() {
        node.name.trim().to_string()
    } else if !node.remote_host.trim().is_empty() {
        node.remote_host.trim().to_string()
    } else if !node.repo_root.trim().is_empty() {
        node.repo_root.trim().to_string()
    } else {
        "unknown-node".to_string()
    }
}

fn node_repo_root_for_invocation(repo_root: &Path, node: &PlanNode) -> Result<String, CtlError> {
    let raw = if !node.repo_root.trim().is_empty() {
        node.repo_root.trim()
    } else {
        return Ok(repo_root.display().to_string());
    };
    Ok(resolve_input_path(repo_root, raw).display().to_string())
}

struct UpgradeWindowCheck {
    in_window: bool,
    reason: String,
}

fn test_upgrade_window(node: &PlanNode) -> UpgradeWindowCheck {
    let raw = node.upgrade_window.trim();
    if raw.is_empty() {
        return UpgradeWindowCheck {
            in_window: true,
            reason: "no window".to_string(),
        };
    }

    let Some((start, end)) = raw
        .strip_suffix("UTC")
        .map(str::trim)
        .and_then(parse_window_range)
    else {
        return UpgradeWindowCheck {
            in_window: false,
            reason: format!("invalid window format: {}", raw),
        };
    };

    let now = utc_minutes_now();
    let allowed = if start == end {
        true
    } else if start < end {
        now >= start && now < end
    } else {
        now >= start || now < end
    };

    UpgradeWindowCheck {
        in_window: allowed,
        reason: format!("window={} now_utc={:02}:{:02}", raw, now / 60, now % 60),
    }
}

fn parse_window_range(raw: &str) -> Option<(u32, u32)> {
    let mut parts = raw.split('-');
    let start = parts.next()?.trim();
    let end = parts.next()?.trim();
    if parts.next().is_some() {
        return None;
    }
    Some((parse_hhmm(start)?, parse_hhmm(end)?))
}

fn parse_hhmm(raw: &str) -> Option<u32> {
    let mut parts = raw.split(':');
    let hour: u32 = parts.next()?.parse().ok()?;
    let minute: u32 = parts.next()?.parse().ok()?;
    if parts.next().is_some() || hour > 23 || minute > 59 {
        return None;
    }
    Some(hour * 60 + minute)
}

fn utc_minutes_now() -> u32 {
    use std::time::{SystemTime, UNIX_EPOCH};
    let seconds = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs();
    let day_seconds = seconds % 86_400;
    let hours = day_seconds / 3_600;
    let minutes = (day_seconds % 3_600) / 60;
    (hours as u32) * 60 + minutes as u32
}
fn invoke_lifecycle_action(
    args: &RolloutArgs,
    node: &PlanNode,
    group: &str,
    invocation: &LifecycleInvocation,
) -> Result<RolloutNodeResult, CtlError> {
    let transport = resolve_node_transport(node, &args.default_transport)?;
    let node_name = resolve_node_name(node);
    let status = match transport {
        Transport::Local => invoke_local_lifecycle(invocation)?,
        Transport::Ssh => invoke_ssh_lifecycle(args, node, invocation)?,
        Transport::WinRm => invoke_winrm_lifecycle(args, node, invocation)?,
    };

    Ok(RolloutNodeResult {
        node: node_name,
        node_group: group.to_string(),
        transport: transport.as_str().to_string(),
        lifecycle_action: invocation.action.clone(),
        target_version: invocation.target_version.clone(),
        rollback_version: invocation.rollback_version.clone(),
        result: if status.success() {
            if invocation.dry_run {
                "dryrun".to_string()
            } else {
                "ok".to_string()
            }
        } else {
            "error".to_string()
        },
        error: if status.success() {
            None
        } else {
            Some(format!(
                "lifecycle action `{}` failed on transport={}",
                invocation.action,
                transport.as_str()
            ))
        },
        dry_run: invocation.dry_run,
    })
}

fn invoke_local_lifecycle(invocation: &LifecycleInvocation) -> Result<ExitStatus, CtlError> {
    let current_exe = std::env::current_exe()
        .map_err(|e| CtlError::ProcessLaunchFailed(format!("resolve current executable: {e}")))?;
    let mut command = Command::new(current_exe);
    command.args(build_local_lifecycle_cli_args(invocation));
    command.stdin(Stdio::inherit());
    command.stdout(Stdio::inherit());
    command.stderr(Stdio::inherit());
    command
        .status()
        .map_err(|e| CtlError::ProcessLaunchFailed(format!("invoke local lifecycle action: {e}")))
}

fn invoke_ssh_lifecycle(
    args: &RolloutArgs,
    node: &PlanNode,
    invocation: &LifecycleInvocation,
) -> Result<ExitStatus, CtlError> {
    if node.remote_host.trim().is_empty() {
        return Err(CtlError::InvalidArgument(format!(
            "node remote_host is required for ssh transport: {}",
            resolve_node_name(node)
        )));
    }
    let remote_repo_root = remote_repo_root(node)?;
    let remote_entry_override = trim_to_option(&node.lifecycle_script_path);
    let remote_body = build_remote_lifecycle_command(
        remote_entry_override.as_deref(),
        invocation,
        &remote_repo_root,
    );
    let remote_shell = if node.remote_shell.trim().is_empty() {
        args.remote_shell.trim()
    } else {
        node.remote_shell.trim()
    };
    let remote_command = format!(
        "{} -NoProfile -NonInteractive -ExecutionPolicy Bypass -EncodedCommand {}",
        remote_shell,
        encode_powershell_script(&remote_body)
    );

    let strict_host_key = trim_to_option(&node.ssh_strict_host_key)
        .unwrap_or_else(|| args.ssh_strict_host_key_checking.clone());
    let identity_file =
        trim_to_option(&node.ssh_identity_file).or_else(|| args.ssh_identity_file.clone());
    let known_hosts_file =
        trim_to_option(&node.ssh_known_hosts_file).or_else(|| args.ssh_known_hosts_file.clone());

    let mut ssh_args = Vec::new();
    if let Some(port) = node.remote_port {
        ssh_args.push("-p".to_string());
        ssh_args.push(port.to_string());
    }
    if args.remote_timeout_seconds > 0 {
        ssh_args.push("-o".to_string());
        ssh_args.push(format!("ConnectTimeout={}", args.remote_timeout_seconds));
    }
    ssh_args.push("-o".to_string());
    ssh_args.push(format!("StrictHostKeyChecking={}", strict_host_key));
    if let Some(file) = known_hosts_file {
        ssh_args.push("-o".to_string());
        ssh_args.push(format!("UserKnownHostsFile={}", file));
    }
    if let Some(file) = identity_file {
        ssh_args.push("-i".to_string());
        ssh_args.push(file);
    }
    let target = if node.remote_user.trim().is_empty() {
        node.remote_host.trim().to_string()
    } else {
        format!("{}@{}", node.remote_user.trim(), node.remote_host.trim())
    };
    ssh_args.push(target);
    ssh_args.push(remote_command);

    let mut command = Command::new(&args.ssh_binary);
    command.args(ssh_args);
    command.stdin(Stdio::inherit());
    command.stdout(Stdio::inherit());
    command.stderr(Stdio::inherit());
    command
        .status()
        .map_err(|e| CtlError::ProcessLaunchFailed(format!("invoke ssh lifecycle action: {e}")))
}

fn invoke_winrm_lifecycle(
    args: &RolloutArgs,
    node: &PlanNode,
    invocation: &LifecycleInvocation,
) -> Result<ExitStatus, CtlError> {
    if node.remote_host.trim().is_empty() {
        return Err(CtlError::InvalidArgument(format!(
            "node remote_host is required for winrm transport: {}",
            resolve_node_name(node)
        )));
    }
    let remote_repo_root = remote_repo_root(node)?;
    let remote_entry_override = trim_to_option(&node.lifecycle_script_path);
    let remote_body = build_remote_lifecycle_command(
        remote_entry_override.as_deref(),
        invocation,
        &remote_repo_root,
    );
    let auth = trim_to_option(&node.winrm_auth);
    let user_env = trim_to_option(&node.winrm_cred_user_env)
        .or_else(|| args.winrm_credential_user_env.clone());
    let pass_env = trim_to_option(&node.winrm_cred_pass_env)
        .or_else(|| args.winrm_credential_password_env.clone());
    let timeout_sec = node
        .winrm_operation_timeout_sec
        .unwrap_or(args.remote_timeout_seconds);

    if user_env.is_some() ^ pass_env.is_some() {
        return Err(CtlError::InvalidArgument(
            "winrm credential env requires both user and password env names".to_string(),
        ));
    }

    let mut script = String::from("$ErrorActionPreference='Stop';");
    script.push_str("$invoke=@{");
    script.push_str(&format!(
        "ComputerName={};",
        ps_single_quote(node.remote_host.trim())
    ));
    script.push_str("ScriptBlock={ param($command) Invoke-Expression $command };");
    script.push_str(&format!(
        "ArgumentList=@({});",
        ps_single_quote(&remote_body)
    ));
    script.push_str("ErrorAction='Stop';");
    script.push_str("};");
    if node.winrm_use_ssl {
        script.push_str("$invoke.UseSSL=$true;");
    }
    if let Some(port) = node.winrm_port {
        script.push_str(&format!("$invoke.Port={};", port));
    }
    if let Some(auth_value) = auth {
        script.push_str(&format!(
            "$invoke.Authentication={};",
            ps_single_quote(&auth_value)
        ));
    }
    if let (Some(user_env_name), Some(pass_env_name)) = (user_env, pass_env) {
        script.push_str(&format!(
            "$user=[Environment]::GetEnvironmentVariable({}); if ([string]::IsNullOrWhiteSpace($user)) {{ throw 'winrm user env missing' }};",
            ps_single_quote(&user_env_name)
        ));
        script.push_str(&format!(
            "$pass=[Environment]::GetEnvironmentVariable({}); if ([string]::IsNullOrWhiteSpace($pass)) {{ throw 'winrm password env missing' }};",
            ps_single_quote(&pass_env_name)
        ));
        script.push_str("$secure=ConvertTo-SecureString -String $pass -AsPlainText -Force;");
        script.push_str("$invoke.Credential=New-Object System.Management.Automation.PSCredential ($user,$secure);");
    }
    if timeout_sec > 0 {
        script.push_str(&format!(
            "$invoke.SessionOption=New-PSSessionOption -OperationTimeout ({} * 1000);",
            timeout_sec
        ));
    }
    script.push_str("Invoke-Command @invoke | Out-Host;");

    let mut command = Command::new("powershell");
    command.args([
        "-NoProfile",
        "-NonInteractive",
        "-ExecutionPolicy",
        "Bypass",
        "-EncodedCommand",
        &encode_powershell_script(&script),
    ]);
    command.stdin(Stdio::inherit());
    command.stdout(Stdio::inherit());
    command.stderr(Stdio::inherit());
    command
        .status()
        .map_err(|e| CtlError::ProcessLaunchFailed(format!("invoke winrm lifecycle action: {e}")))
}

fn build_local_lifecycle_cli_args(invocation: &LifecycleInvocation) -> Vec<String> {
    let mut args = vec![
        "lifecycle".to_string(),
        "--action".to_string(),
        invocation.action.clone(),
        "--repo-root".to_string(),
        invocation.repo_root.clone(),
    ];
    push_cli_pair(
        &mut args,
        "--target-version",
        invocation.target_version.as_deref(),
    );
    push_cli_pair(
        &mut args,
        "--rollback-version",
        invocation.rollback_version.as_deref(),
    );
    push_cli_pair(&mut args, "--audit-file", invocation.audit_file.as_deref());
    push_cli_pair(
        &mut args,
        "--overlay-route-mode",
        invocation.overlay_route_mode.as_deref(),
    );
    push_cli_pair(
        &mut args,
        "--overlay-route-runtime-file",
        invocation.overlay_route_runtime_file.as_deref(),
    );
    push_cli_pair(
        &mut args,
        "--overlay-route-runtime-profile",
        invocation.overlay_route_runtime_profile.as_deref(),
    );
    push_cli_pair(
        &mut args,
        "--overlay-route-relay-directory-file",
        invocation.overlay_route_relay_directory_file.as_deref(),
    );
    if invocation.enable_auto_profile {
        args.push("--auto-profile-enabled".to_string());
    }
    push_cli_pair(
        &mut args,
        "--auto-profile-state-file",
        invocation.auto_profile_state_file.as_deref(),
    );
    push_cli_pair(
        &mut args,
        "--auto-profile-profiles",
        invocation.auto_profile_profiles.as_deref(),
    );
    push_cli_number(
        &mut args,
        "--auto-profile-min-hold-seconds",
        invocation.auto_profile_min_hold_seconds,
    );
    if let Some(value) = invocation.auto_profile_switch_margin {
        args.push("--auto-profile-switch-margin".to_string());
        args.push(value.to_string());
    }
    push_cli_number(
        &mut args,
        "--auto-profile-switchback-cooldown-seconds",
        invocation.auto_profile_switchback_cooldown_seconds,
    );
    push_cli_number(
        &mut args,
        "--auto-profile-recheck-seconds",
        invocation.auto_profile_recheck_seconds,
    );
    push_cli_pair(
        &mut args,
        "--policy-cli-binary-file",
        invocation.auto_profile_binary_path.as_deref(),
    );
    push_cli_number(
        &mut args,
        "--upgrade-health-seconds",
        invocation.upgrade_health_seconds,
    );
    push_cli_pair(&mut args, "--node-group", invocation.node_group.as_deref());
    push_cli_pair(
        &mut args,
        "--upgrade-window",
        invocation.upgrade_window.as_deref(),
    );
    push_cli_pair(
        &mut args,
        "--require-node-group",
        invocation.require_node_group.as_deref(),
    );
    if invocation.dry_run {
        args.push("--dry-run".to_string());
    }
    args
}
fn build_remote_lifecycle_command(
    entry_override: Option<&str>,
    invocation: &LifecycleInvocation,
    repo_root: &str,
) -> String {
    let mut script = String::from("$ErrorActionPreference='Stop';");
    if let Some(path) = entry_override.filter(|value| !value.trim().is_empty()) {
        script.push_str(&format!("$novovmctl={};", ps_single_quote(path.trim())));
    } else {
        script.push_str("$novovmctl=$null;");
        script.push_str("$explicit=[Environment]::GetEnvironmentVariable('NOVOVMCTL_BINARY');");
        script.push_str("if (-not [string]::IsNullOrWhiteSpace($explicit) -and (Test-Path -LiteralPath $explicit)) { $novovmctl=$explicit; }");
        script.push_str("if ($null -eq $novovmctl) {");
        script.push_str("$cargoTarget=[Environment]::GetEnvironmentVariable('CARGO_TARGET_DIR');");
        script.push_str("if (-not [string]::IsNullOrWhiteSpace($cargoTarget)) {");
        script.push_str("$cargoCandidates=@(");
        script.push_str("Join-Path $cargoTarget 'release/novovmctl.exe',");
        script.push_str("Join-Path $cargoTarget 'release/novovmctl',");
        script.push_str("Join-Path $cargoTarget 'debug/novovmctl.exe',");
        script.push_str("Join-Path $cargoTarget 'debug/novovmctl'");
        script.push_str(");");
        script.push_str("foreach ($candidate in $cargoCandidates) { if (Test-Path -LiteralPath $candidate) { $novovmctl=$candidate; break } }");
        script.push_str("}}");
        script.push_str("if ($null -eq $novovmctl) {");
        script.push_str("$repoRoot=");
        script.push_str(&ps_single_quote(repo_root));
        script.push(';');
        script.push_str("$repoCandidates=@(");
        script.push_str(&ps_single_quote(&remote_novovmctl_candidate(
            repo_root,
            "target/release/novovmctl.exe",
        )));
        script.push(',');
        script.push_str(&ps_single_quote(&remote_novovmctl_candidate(
            repo_root,
            "target/release/novovmctl",
        )));
        script.push(',');
        script.push_str(&ps_single_quote(&remote_novovmctl_candidate(
            repo_root,
            "target/debug/novovmctl.exe",
        )));
        script.push(',');
        script.push_str(&ps_single_quote(&remote_novovmctl_candidate(
            repo_root,
            "target/debug/novovmctl",
        )));
        script.push_str(");");
        script.push_str("foreach ($candidate in $repoCandidates) { if (Test-Path -LiteralPath $candidate) { $novovmctl=$candidate; break } }");
        script.push('}');
        script.push_str("if ($null -eq $novovmctl) { throw 'remote novovmctl not found' }");
    }

    let mut parts = vec![
        "& $novovmctl".to_string(),
        "lifecycle".to_string(),
        "--action".to_string(),
        ps_single_quote(&invocation.action),
        "--repo-root".to_string(),
        ps_single_quote(repo_root),
    ];
    push_ps_pair(
        &mut parts,
        "--target-version",
        invocation.target_version.as_deref(),
    );
    push_ps_pair(
        &mut parts,
        "--rollback-version",
        invocation.rollback_version.as_deref(),
    );
    push_ps_pair(&mut parts, "--audit-file", invocation.audit_file.as_deref());
    push_ps_pair(
        &mut parts,
        "--overlay-route-mode",
        invocation.overlay_route_mode.as_deref(),
    );
    push_ps_pair(
        &mut parts,
        "--overlay-route-runtime-file",
        invocation.overlay_route_runtime_file.as_deref(),
    );
    push_ps_pair(
        &mut parts,
        "--overlay-route-runtime-profile",
        invocation.overlay_route_runtime_profile.as_deref(),
    );
    push_ps_pair(
        &mut parts,
        "--overlay-route-relay-directory-file",
        invocation.overlay_route_relay_directory_file.as_deref(),
    );
    if invocation.enable_auto_profile {
        parts.push("--auto-profile-enabled".to_string());
    }
    push_ps_pair(
        &mut parts,
        "--auto-profile-state-file",
        invocation.auto_profile_state_file.as_deref(),
    );
    push_ps_pair(
        &mut parts,
        "--auto-profile-profiles",
        invocation.auto_profile_profiles.as_deref(),
    );
    if let Some(value) = invocation.auto_profile_min_hold_seconds {
        parts.push("--auto-profile-min-hold-seconds".to_string());
        parts.push(ps_single_quote(&value.to_string()));
    }
    if let Some(value) = invocation.auto_profile_switch_margin {
        parts.push("--auto-profile-switch-margin".to_string());
        parts.push(ps_single_quote(&value.to_string()));
    }
    if let Some(value) = invocation.auto_profile_switchback_cooldown_seconds {
        parts.push("--auto-profile-switchback-cooldown-seconds".to_string());
        parts.push(ps_single_quote(&value.to_string()));
    }
    if let Some(value) = invocation.auto_profile_recheck_seconds {
        parts.push("--auto-profile-recheck-seconds".to_string());
        parts.push(ps_single_quote(&value.to_string()));
    }
    push_ps_pair(
        &mut parts,
        "--policy-cli-binary-file",
        invocation.auto_profile_binary_path.as_deref(),
    );
    if let Some(value) = invocation.upgrade_health_seconds {
        parts.push("--upgrade-health-seconds".to_string());
        parts.push(ps_single_quote(&value.to_string()));
    }
    push_ps_pair(&mut parts, "--node-group", invocation.node_group.as_deref());
    push_ps_pair(
        &mut parts,
        "--upgrade-window",
        invocation.upgrade_window.as_deref(),
    );
    push_ps_pair(
        &mut parts,
        "--require-node-group",
        invocation.require_node_group.as_deref(),
    );
    if invocation.dry_run {
        parts.push("--dry-run".to_string());
    }
    script.push_str(&parts.join(" "));
    script
}

fn push_cli_pair(args: &mut Vec<String>, flag: &str, value: Option<&str>) {
    if let Some(value) = value.filter(|value| !value.trim().is_empty()) {
        args.push(flag.to_string());
        args.push(value.to_string());
    }
}

fn push_cli_number<T: ToString>(args: &mut Vec<String>, flag: &str, value: Option<T>) {
    if let Some(value) = value {
        args.push(flag.to_string());
        args.push(value.to_string());
    }
}

fn push_ps_pair(parts: &mut Vec<String>, flag: &str, value: Option<&str>) {
    if let Some(value) = value.filter(|value| !value.trim().is_empty()) {
        parts.push(flag.to_string());
        parts.push(ps_single_quote(value));
    }
}

fn ps_single_quote(value: &str) -> String {
    format!("'{}'", value.replace('\'', "''"))
}

fn encode_powershell_script(script: &str) -> String {
    let bytes: Vec<u8> = script
        .encode_utf16()
        .flat_map(|unit| unit.to_le_bytes())
        .collect();
    base64_encode(&bytes)
}

fn base64_encode(bytes: &[u8]) -> String {
    const TABLE: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut out = String::new();
    let mut chunks = bytes.chunks_exact(3);
    for chunk in &mut chunks {
        let n = ((chunk[0] as u32) << 16) | ((chunk[1] as u32) << 8) | (chunk[2] as u32);
        out.push(TABLE[((n >> 18) & 0x3f) as usize] as char);
        out.push(TABLE[((n >> 12) & 0x3f) as usize] as char);
        out.push(TABLE[((n >> 6) & 0x3f) as usize] as char);
        out.push(TABLE[(n & 0x3f) as usize] as char);
    }
    let rem = chunks.remainder();
    match rem.len() {
        1 => {
            let n = (rem[0] as u32) << 16;
            out.push(TABLE[((n >> 18) & 0x3f) as usize] as char);
            out.push(TABLE[((n >> 12) & 0x3f) as usize] as char);
            out.push('=');
            out.push('=');
        }
        2 => {
            let n = ((rem[0] as u32) << 16) | ((rem[1] as u32) << 8);
            out.push(TABLE[((n >> 18) & 0x3f) as usize] as char);
            out.push(TABLE[((n >> 12) & 0x3f) as usize] as char);
            out.push(TABLE[((n >> 6) & 0x3f) as usize] as char);
            out.push('=');
        }
        _ => {}
    }
    out
}

fn remote_repo_root(node: &PlanNode) -> Result<String, CtlError> {
    trim_to_option(&node.remote_repo_root)
        .or_else(|| trim_to_option(&node.repo_root))
        .ok_or_else(|| {
            CtlError::InvalidArgument(format!(
                "node remote_repo_root or repo_root is required for remote transport: {}",
                resolve_node_name(node)
            ))
        })
}

fn remote_novovmctl_candidate(remote_repo_root: &str, suffix: &str) -> String {
    let normalized_root = remote_repo_root.trim_end_matches(['\\', '/']);
    if remote_repo_root.contains('\\') {
        format!("{}\\{}", normalized_root, suffix.replace('/', "\\"))
    } else {
        format!("{}/{}", normalized_root, suffix)
    }
}

fn trim_to_option(value: &str) -> Option<String> {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        None
    } else {
        Some(trimmed.to_string())
    }
}

fn require_trimmed<'a>(value: Option<&'a str>, error: &str) -> Result<&'a str, CtlError> {
    value
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .ok_or_else(|| CtlError::InvalidArgument(error.to_string()))
}

fn tally_result(result: &RolloutNodeResult, ok_count: &mut usize, error_count: &mut usize) {
    if result.result == "error" || result.result == "blocked" {
        *error_count += 1;
    } else {
        *ok_count += 1;
    }
}
