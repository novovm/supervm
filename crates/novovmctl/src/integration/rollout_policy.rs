use std::process::Command;

use serde::de::DeserializeOwned;

use crate::error::CtlError;
use crate::model::rollout_control::{
    CircuitBreakerEvaluateResult, ControllerDispatchEvaluateResult, PolicyProfileSelectResult,
    SloEvaluateResult,
};
use crate::model::up::AutoProfileDecision;

#[derive(serde::Deserialize)]
struct PolicySuccessEnvelope<T> {
    ok: Option<bool>,
    data: Option<T>,
}

#[allow(clippy::too_many_arguments)]
pub fn auto_profile_select(
    policy_bin: &str,
    runtime_file: &str,
    current_profile: Option<&str>,
    state_file: Option<&str>,
    profiles: Option<&str>,
    min_hold_seconds: Option<u64>,
    switch_margin: Option<f64>,
    switchback_cooldown_seconds: Option<u64>,
    recheck_seconds: Option<u64>,
) -> Result<AutoProfileDecision, CtlError> {
    let mut args = vec![
        "overlay".to_string(),
        "auto-profile-select".to_string(),
        "--runtime-file".to_string(),
        runtime_file.to_string(),
    ];

    if let Some(profile) = current_profile {
        args.push("--current-profile".to_string());
        args.push(profile.to_string());
    }
    if let Some(path) = state_file {
        args.push("--state-file".to_string());
        args.push(path.to_string());
    }
    if let Some(raw) = profiles {
        args.push("--profiles".to_string());
        args.push(raw.to_string());
    }
    if let Some(v) = min_hold_seconds {
        args.push("--min-hold-seconds".to_string());
        args.push(v.to_string());
    }
    if let Some(v) = switch_margin {
        args.push("--switch-margin".to_string());
        args.push(v.to_string());
    }
    if let Some(v) = switchback_cooldown_seconds {
        args.push("--switchback-cooldown-seconds".to_string());
        args.push(v.to_string());
    }
    if let Some(v) = recheck_seconds {
        args.push("--recheck-seconds".to_string());
        args.push(v.to_string());
    }

    invoke_json(policy_bin, &args)
}

pub fn controller_dispatch_evaluate(
    policy_bin: &str,
    queue_file: &str,
    plan_action: &str,
    controller_id: Option<&str>,
    operation_id: Option<&str>,
) -> Result<ControllerDispatchEvaluateResult, CtlError> {
    let mut args = vec![
        "rollout".to_string(),
        "controller-dispatch-evaluate".to_string(),
        "--queue-file".to_string(),
        queue_file.to_string(),
        "--plan-action".to_string(),
        plan_action.to_string(),
    ];

    if let Some(v) = controller_id {
        args.push("--controller-id".to_string());
        args.push(v.to_string());
    }
    if let Some(v) = operation_id {
        args.push("--operation-id".to_string());
        args.push(v.to_string());
    }

    invoke_json(policy_bin, &args)
}

#[allow(clippy::too_many_arguments)]
pub fn risk_slo_evaluate(
    policy_bin: &str,
    state_file: &str,
    grade: &str,
    window_samples: usize,
    min_green_rate: f64,
    max_red_in_window: usize,
    block_on_violation: bool,
    now_unix_ms: u64,
) -> Result<SloEvaluateResult, CtlError> {
    let args = vec![
        "risk".to_string(),
        "slo-evaluate".to_string(),
        "--state-file".to_string(),
        state_file.to_string(),
        "--grade".to_string(),
        grade.to_string(),
        "--window-samples".to_string(),
        window_samples.to_string(),
        "--min-green-rate".to_string(),
        min_green_rate.to_string(),
        "--max-red-in-window".to_string(),
        max_red_in_window.to_string(),
        "--block-on-violation".to_string(),
        block_on_violation.to_string(),
        "--now-unix-ms".to_string(),
        now_unix_ms.to_string(),
    ];

    invoke_json(policy_bin, &args)
}

#[allow(clippy::too_many_arguments)]
pub fn risk_circuit_breaker_evaluate(
    policy_bin: &str,
    score: f64,
    base_concurrent: u32,
    base_pause: u64,
    yellow_concurrent: u32,
    yellow_pause: u64,
    red_block: bool,
    matrix_json: &str,
) -> Result<CircuitBreakerEvaluateResult, CtlError> {
    let args = vec![
        "risk".to_string(),
        "circuit-breaker-evaluate".to_string(),
        "--score".to_string(),
        score.to_string(),
        "--base-concurrent".to_string(),
        base_concurrent.to_string(),
        "--base-pause".to_string(),
        base_pause.to_string(),
        "--yellow-concurrent".to_string(),
        yellow_concurrent.to_string(),
        "--yellow-pause".to_string(),
        yellow_pause.to_string(),
        "--red-block".to_string(),
        red_block.to_string(),
        "--matrix-json".to_string(),
        matrix_json.to_string(),
    ];

    invoke_json(policy_bin, &args)
}

pub fn risk_policy_profile_select(
    policy_bin: &str,
    risk_policy_json: &str,
    requested_profile: &str,
) -> Result<PolicyProfileSelectResult, CtlError> {
    let args = vec![
        "risk".to_string(),
        "policy-profile-select".to_string(),
        "--risk-policy-json".to_string(),
        risk_policy_json.to_string(),
        "--requested-profile".to_string(),
        requested_profile.to_string(),
    ];

    invoke_json(policy_bin, &args)
}

fn invoke_json<T: DeserializeOwned>(policy_bin: &str, args: &[String]) -> Result<T, CtlError> {
    let output = Command::new(policy_bin).args(args).output().map_err(|e| {
        CtlError::ProcessLaunchFailed(format!(
            "launch `{policy_bin} {}` failed: {e}",
            args.join(" ")
        ))
    })?;

    if !output.status.success() {
        return Err(CtlError::IntegrationFailed(format!(
            "`{policy_bin} {}` exited non-zero: {}",
            args.join(" "),
            String::from_utf8_lossy(&output.stderr)
        )));
    }

    let stdout = String::from_utf8(output.stdout)
        .map_err(|e| CtlError::IntegrationFailed(format!("stdout not utf8: {e}")))?;

    if let Ok(envelope) = serde_json::from_str::<PolicySuccessEnvelope<T>>(&stdout) {
        if envelope.ok.unwrap_or(false) {
            if let Some(data) = envelope.data {
                return Ok(data);
            }
        }
    }

    serde_json::from_str::<T>(&stdout).map_err(|e| {
        CtlError::IntegrationFailed(format!(
            "failed to parse json from `{policy_bin} {}`: {e}; stdout={stdout}",
            args.join(" ")
        ))
    })
}
