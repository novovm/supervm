use crate::audit;
use crate::cli::UpArgs;
use crate::error::CtlError;
use crate::integration::{node_binary, rollout_policy};
use crate::model::up::{AutoProfileDecision, EffectiveUpConfig, UpAuditRecord};
use crate::output;
use crate::runtime::{env, paths};

pub fn run(args: UpArgs) -> Result<(), CtlError> {
    let command_name = "up";
    let audit_path = env::resolve_audit_file(args.audit_file.as_deref());
    let result = inner_run(args);

    match result {
        Ok((effective, audit_record)) => {
            if let Some(audit_file) = audit_path.as_deref().or(effective.audit_file.as_deref()) {
                audit::append_success_jsonl(audit_file, command_name, &audit_record)?;
            }

            output::print_up_summary(
                &effective.profile,
                &effective.role_profile,
                effective.overlay_route_runtime_profile.as_deref(),
                effective.overlay_route_mode.as_deref(),
                &effective.policy_bin,
                &effective.node_bin,
                audit_record.launched,
                &audit_record.reason,
            );
            output::print_success_json(command_name, &audit_record)?;
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

fn apply_auto_profile_decision(effective: &mut EffectiveUpConfig, decision: &AutoProfileDecision) {
    if let Some(selected_profile) = decision.selected_profile.as_deref() {
        match decision.action.as_str() {
            "switch" | "keep" | "hold" => {
                effective.overlay_route_runtime_profile = Some(selected_profile.to_string());
            }
            _ => {}
        }
    }
}

pub(crate) fn build_effective_up_config(args: &UpArgs) -> Result<EffectiveUpConfig, CtlError> {
    let policy_bin = paths::resolve_policy_binary(args.policy_cli_binary_file.as_deref())?;
    let node_bin = paths::resolve_node_binary(args.node_binary_file.as_deref())?;

    Ok(EffectiveUpConfig {
        profile: args.profile.clone(),
        role_profile: args.role_profile.clone(),
        overlay_route_runtime_file: args.overlay_route_runtime_file.clone(),
        overlay_route_runtime_profile: args.overlay_route_runtime_profile.clone(),
        overlay_route_mode: args.overlay_route_mode.clone(),
        overlay_route_relay_directory_file: args.overlay_route_relay_directory_file.clone(),
        policy_bin: policy_bin.clone(),
        node_bin: node_bin.clone(),
        use_node_watch_mode: args.use_node_watch_mode,
        poll_ms: args.poll_ms,
        node_watch_batch_max_files: args.node_watch_batch_max_files,
        idle_exit_seconds: args.idle_exit_seconds,
        no_gateway: false,
        lean_io: false,
        supervisor_poll_ms: None,
        gateway_bind: None,
        gateway_spool_dir: None,
        gateway_max_requests: None,
        ops_wire_dir: None,
        ops_wire_watch_done_dir: None,
        ops_wire_watch_failed_dir: None,
        ops_wire_watch_drop_failed: false,
        auto_profile_state_file: args.auto_profile_state_file.clone(),
        auto_profile_profiles: args.auto_profile_profiles.clone(),
        auto_profile_min_hold_seconds: args.auto_profile_min_hold_seconds,
        auto_profile_switch_margin: args.auto_profile_switch_margin,
        auto_profile_switchback_cooldown_seconds: args.auto_profile_switchback_cooldown_seconds,
        auto_profile_recheck_seconds: args.auto_profile_recheck_seconds,
        auto_profile_enabled: args.auto_profile_enabled,
        skip_policy_warmup: args.skip_policy_warmup,
        foreground: args.foreground,
        dry_run: args.dry_run,
        audit_file: args.audit_file.clone(),
        log_file: args.log_file.clone(),
    })
}

pub(crate) fn warmup_effective_up_config(
    effective: &mut EffectiveUpConfig,
) -> Result<(Option<AutoProfileDecision>, String), CtlError> {
    let mut auto_profile_decision: Option<AutoProfileDecision> = None;
    let mut reason = "direct_launch".to_string();

    if effective.auto_profile_enabled && !effective.skip_policy_warmup {
        if let Some(runtime_file) = effective.overlay_route_runtime_file.as_deref() {
            let decision = rollout_policy::auto_profile_select(
                &effective.policy_bin,
                runtime_file,
                effective.overlay_route_runtime_profile.as_deref(),
                effective.auto_profile_state_file.as_deref(),
                effective.auto_profile_profiles.as_deref(),
                effective.auto_profile_min_hold_seconds,
                effective.auto_profile_switch_margin,
                effective.auto_profile_switchback_cooldown_seconds,
                effective.auto_profile_recheck_seconds,
            )?;
            apply_auto_profile_decision(effective, &decision);
            reason = format!("auto_profile_{}", decision.action);
            auto_profile_decision = Some(decision);
        }
    }
    Ok((auto_profile_decision, reason))
}

pub(crate) fn build_up_audit_record(
    effective: &EffectiveUpConfig,
    auto_profile_decision: Option<AutoProfileDecision>,
    launched: bool,
    reason: String,
) -> UpAuditRecord {
    UpAuditRecord {
        profile: effective.profile.clone(),
        role_profile: effective.role_profile.clone(),
        policy_bin: effective.policy_bin.clone(),
        node_bin: effective.node_bin.clone(),
        overlay_route_runtime_file: effective.overlay_route_runtime_file.clone(),
        overlay_route_runtime_profile: effective.overlay_route_runtime_profile.clone(),
        overlay_route_mode: effective.overlay_route_mode.clone(),
        overlay_route_relay_directory_file: effective.overlay_route_relay_directory_file.clone(),
        use_node_watch_mode: effective.use_node_watch_mode,
        poll_ms: effective.poll_ms,
        node_watch_batch_max_files: effective.node_watch_batch_max_files,
        idle_exit_seconds: effective.idle_exit_seconds,
        auto_profile_state_file: effective.auto_profile_state_file.clone(),
        auto_profile_profiles: effective.auto_profile_profiles.clone(),
        auto_profile_min_hold_seconds: effective.auto_profile_min_hold_seconds,
        auto_profile_switch_margin: effective.auto_profile_switch_margin,
        auto_profile_switchback_cooldown_seconds: effective
            .auto_profile_switchback_cooldown_seconds,
        auto_profile_recheck_seconds: effective.auto_profile_recheck_seconds,
        auto_profile_decision,
        dry_run: effective.dry_run,
        launched,
        reason,
    }
}

fn inner_run(args: UpArgs) -> Result<(EffectiveUpConfig, UpAuditRecord), CtlError> {
    let mut effective = build_effective_up_config(&args)?;
    let (auto_profile_decision, mut reason) = warmup_effective_up_config(&mut effective)?;

    if args.print_effective_config {
        let rendered = serde_json::to_string_pretty(&effective)
            .map_err(|e| CtlError::FileWriteFailed(format!("serialize effective config: {e}")))?;
        println!("{rendered}");
    }

    let launched = if effective.dry_run {
        reason = "dry_run".to_string();
        false
    } else {
        node_binary::launch_node(&effective)?;
        true
    };

    let record = build_up_audit_record(&effective, auto_profile_decision, launched, reason);
    Ok((effective, record))
}
