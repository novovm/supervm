use crate::audit;
use crate::cli::RolloutControlArgs;
use crate::error::CtlError;
use crate::integration::rollout_policy;
use crate::model::rollout_control::{
    CircuitBreakerEvaluateResult, ControllerDispatchEvaluateResult, PolicyProfileSelectResult,
    RolloutControlAuditRecord, RolloutControlDecision, RolloutControlPlan, RolloutQueueConfig,
    RolloutQueuePlan, SloEvaluateResult,
};
use crate::output;
use crate::runtime::{env, paths};
use serde_json::Value;
use std::fs;
use std::time::{SystemTime, UNIX_EPOCH};

pub fn run(args: RolloutControlArgs) -> Result<(), CtlError> {
    let command_name = "rollout-control";
    let audit_path = env::resolve_audit_file(args.audit_file.as_deref());
    let result = inner_run(args);

    match result {
        Ok(record) => {
            if let Some(audit_file) = audit_path.as_deref().or(record.plan.audit_file.as_deref()) {
                audit::append_success_jsonl(audit_file, command_name, &record)?;
            }

            output::print_rollout_control_summary(
                &record.plan.queue_file,
                &record.plan.plan_action,
                record.plan.controller_id.as_deref(),
                record.slo_eval.score,
                record.slo_eval.violation,
                record.circuit_eval.block_dispatch,
                record.profile_eval.selected_profile.as_deref(),
                &record.decision.reason,
                record.decision.continue_dispatch,
                record.decision.max_concurrent_plans,
                record.decision.dispatch_pause_seconds,
                record.applied,
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

fn merge_rollout_control_decision(
    rollout_eval: &ControllerDispatchEvaluateResult,
    slo_eval: &SloEvaluateResult,
    circuit_eval: &CircuitBreakerEvaluateResult,
    profile_eval: &PolicyProfileSelectResult,
) -> RolloutControlDecision {
    let block_dispatch =
        rollout_eval.block_dispatch || circuit_eval.block_dispatch || slo_eval.violation;
    let continue_dispatch = rollout_eval.continue_dispatch && !block_dispatch;

    RolloutControlDecision {
        continue_dispatch,
        max_concurrent_plans: circuit_eval.max_concurrent_plans,
        dispatch_pause_seconds: circuit_eval.dispatch_pause_seconds,
        block_dispatch,
        selected_policy_profile: profile_eval.selected_profile.clone(),
        reason: if block_dispatch {
            "blocked_by_rollout_or_risk".to_string()
        } else {
            "dispatch_allowed".to_string()
        },
    }
}

fn apply_rollout_control_decision(
    _plan: &RolloutControlPlan,
    _decision: &RolloutControlDecision,
) -> Result<(), CtlError> {
    Ok(())
}

fn inner_run(args: RolloutControlArgs) -> Result<RolloutControlAuditRecord, CtlError> {
    let policy_bin = paths::resolve_policy_binary(args.policy_cli_binary_file.as_deref())?;
    let queue = load_queue_config(&args.queue_file)?;
    let matched_plan = select_matching_plan(&queue, &args.plan_action);
    let controller_id = args
        .controller_id
        .clone()
        .or_else(|| matched_plan.and_then(nonempty_controller_id));
    let operation_id = args
        .operation_id
        .clone()
        .or_else(|| matched_plan.and_then(nonempty_operation_id));
    let audit_file = args
        .audit_file
        .clone()
        .or_else(|| matched_plan.and_then(nonempty_audit_file));

    let plan = RolloutControlPlan {
        policy_bin,
        queue_file: args.queue_file.clone(),
        plan_action: args.plan_action.clone(),
        controller_id,
        operation_id,
        audit_file,
        dry_run: args.dry_run,
        continue_on_plan_failure: args.continue_on_plan_failure,
        resume_from_snapshot: args.resume_from_snapshot,
        replay_conflicts_on_start: args.replay_conflicts_on_start,
    };

    if args.print_effective_queue {
        let rendered = serde_json::to_string_pretty(&plan)
            .map_err(|e| CtlError::FileWriteFailed(format!("serialize effective queue: {e}")))?;
        println!("{rendered}");
    }

    let rollout_eval = rollout_policy::controller_dispatch_evaluate(
        &plan.policy_bin,
        &plan.queue_file,
        &plan.plan_action,
        plan.controller_id.as_deref(),
        plan.operation_id.as_deref(),
    )?;

    let resolved_grade = resolve_health_grade(queue.state_recovery.replica_health_file.as_str());

    let slo_eval = if queue.state_recovery.slo.enabled {
        if queue.state_recovery.slo.file.trim().is_empty() {
            return Err(CtlError::InvalidArgument(
                "state_recovery.slo.file is required when slo is enabled".to_string(),
            ));
        }
        rollout_policy::risk_slo_evaluate(
            &plan.policy_bin,
            &queue.state_recovery.slo.file,
            &resolved_grade,
            queue.state_recovery.slo.window_samples,
            queue.state_recovery.slo.min_green_rate,
            queue.state_recovery.slo.max_red_in_window,
            queue.state_recovery.slo.block_on_violation,
            now_unix_ms(),
        )?
    } else {
        SloEvaluateResult {
            green_rate: Some(1.0),
            score: Some(100.0),
            violation: false,
            reason: "slo_disabled".to_string(),
        }
    };

    let circuit_eval = if queue.state_recovery.slo.circuit_breaker.enabled {
        let matrix_json =
            serde_json::to_string(&queue.state_recovery.slo.circuit_breaker.matrix)
                .map_err(|e| CtlError::FileReadFailed(format!("serialize circuit matrix: {e}")))?;
        rollout_policy::risk_circuit_breaker_evaluate(
            &plan.policy_bin,
            slo_eval.score.unwrap_or(100.0),
            queue.max_concurrent_plans.max(1),
            queue.dispatch_pause_seconds,
            queue
                .state_recovery
                .slo
                .circuit_breaker
                .yellow_max_concurrent_plans
                .max(1),
            queue
                .state_recovery
                .slo
                .circuit_breaker
                .yellow_dispatch_pause_seconds,
            queue.state_recovery.slo.circuit_breaker.red_block,
            &matrix_json,
        )?
    } else {
        CircuitBreakerEvaluateResult {
            max_concurrent_plans: Some(queue.max_concurrent_plans.max(1)),
            dispatch_pause_seconds: Some(queue.dispatch_pause_seconds),
            block_dispatch: false,
            reason: "circuit_breaker_disabled".to_string(),
        }
    };

    let risk_policy_json = serde_json::to_string(&queue.risk_policy)
        .map_err(|e| CtlError::FileReadFailed(format!("serialize risk policy: {e}")))?;
    let profile_eval = rollout_policy::risk_policy_profile_select(
        &plan.policy_bin,
        &risk_policy_json,
        queue.risk_policy.active_profile.as_str(),
    )?;

    let decision =
        merge_rollout_control_decision(&rollout_eval, &slo_eval, &circuit_eval, &profile_eval);

    let applied = if plan.dry_run {
        false
    } else {
        apply_rollout_control_decision(&plan, &decision)?;
        true
    };

    Ok(RolloutControlAuditRecord {
        plan,
        rollout_eval,
        slo_eval,
        circuit_eval,
        profile_eval,
        decision,
        applied,
    })
}

fn load_queue_config(path: &str) -> Result<RolloutQueueConfig, CtlError> {
    let raw = fs::read_to_string(path)
        .map_err(|e| CtlError::FileReadFailed(format!("read queue file `{path}`: {e}")))?;
    serde_json::from_str::<RolloutQueueConfig>(&raw)
        .map_err(|e| CtlError::FileReadFailed(format!("parse queue file `{path}`: {e}")))
}

fn select_matching_plan<'a>(
    queue: &'a RolloutQueueConfig,
    plan_action: &str,
) -> Option<&'a RolloutQueuePlan> {
    let action_norm = plan_action.trim().to_ascii_lowercase();
    queue.plans.iter().find(|plan| {
        if !plan.enabled {
            return false;
        }
        let plan_norm = plan.action.trim().to_ascii_lowercase();
        plan_norm.is_empty() || plan_norm == action_norm
    })
}

fn nonempty_controller_id(plan: &RolloutQueuePlan) -> Option<String> {
    let value = plan.controller_id.trim();
    if value.is_empty() {
        None
    } else {
        Some(value.to_string())
    }
}

fn nonempty_operation_id(plan: &RolloutQueuePlan) -> Option<String> {
    let value = plan.operation_id.trim();
    if value.is_empty() {
        None
    } else {
        Some(value.to_string())
    }
}

fn nonempty_audit_file(plan: &RolloutQueuePlan) -> Option<String> {
    let value = plan.audit_file.trim();
    if value.is_empty() {
        None
    } else {
        Some(value.to_string())
    }
}

fn now_unix_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|v| v.as_millis() as u64)
        .unwrap_or(0)
}

fn resolve_health_grade(replica_health_file: &str) -> String {
    let path = replica_health_file.trim();
    if path.is_empty() {
        return "yellow".to_string();
    }
    let raw = match fs::read_to_string(path) {
        Ok(v) => v,
        Err(_) => return "yellow".to_string(),
    };
    let value: Value = match serde_json::from_str(&raw) {
        Ok(v) => v,
        Err(_) => return "yellow".to_string(),
    };
    extract_grade(&value).unwrap_or_else(|| "yellow".to_string())
}

fn extract_grade(value: &Value) -> Option<String> {
    match value {
        Value::String(raw) => normalize_grade(raw),
        Value::Object(map) => {
            for key in [
                "effective_grade",
                "grade",
                "health_grade",
                "slo_grade",
                "last_grade",
            ] {
                if let Some(found) = map.get(key).and_then(extract_grade) {
                    return Some(found);
                }
            }
            for key in ["last", "state", "current", "replica", "health"] {
                if let Some(found) = map.get(key).and_then(extract_grade) {
                    return Some(found);
                }
            }
            None
        }
        _ => None,
    }
}

fn normalize_grade(raw: &str) -> Option<String> {
    match raw.trim().to_ascii_lowercase().as_str() {
        "green" => Some("green".to_string()),
        "yellow" | "orange" => Some("yellow".to_string()),
        "red" => Some("red".to_string()),
        _ => None,
    }
}
