#![forbid(unsafe_code)]

use crate::metrics::GovernanceAssessment;

#[must_use]
pub fn render_prometheus_text(summary: &GovernanceAssessment) -> String {
    let mut lines = Vec::new();
    lines.push(
        "# HELP novovm_governance_gate_total_prs Total PRs evaluated by governance gate"
            .to_string(),
    );
    lines.push("# TYPE novovm_governance_gate_total_prs gauge".to_string());
    lines.push(format!(
        "novovm_governance_gate_total_prs {}",
        summary.gate.total_prs
    ));
    lines.push(format!(
        "novovm_governance_gate_rejected_no_trigger {}",
        summary.gate.rejected_no_trigger
    ));
    lines.push(format!(
        "novovm_governance_gate_rejected_invalid_trigger {}",
        summary.gate.rejected_invalid_trigger
    ));
    lines.push(format!(
        "novovm_governance_gate_passed_with_trigger {}",
        summary.gate.passed_with_trigger
    ));
    lines.push(format!(
        "novovm_governance_runtime_pq_required {}",
        summary.runtime.pq_required_requests
    ));
    lines.push(format!(
        "novovm_governance_runtime_pq_rejected {}",
        summary.runtime.pq_rejected
    ));
    lines.push(format!(
        "novovm_governance_runtime_privacy_required {}",
        summary.runtime.privacy_required_requests
    ));
    lines.push(format!(
        "novovm_governance_runtime_privacy_rejected {}",
        summary.runtime.privacy_rejected
    ));
    lines.push(format!(
        "novovm_governance_runtime_mapped_asset_register_attempts {}",
        summary.runtime.mapped_asset_register_attempts
    ));
    lines.push(format!(
        "novovm_governance_runtime_mapped_asset_blocked_by_nogo {}",
        summary.runtime.mapped_asset_blocked_by_nogo
    ));
    lines.push(format!(
        "novovm_governance_runtime_mapped_asset_blocked_by_rule {}",
        summary.runtime.mapped_asset_blocked_by_rule
    ));
    lines.push(format!(
        "novovm_governance_runtime_mapped_asset_blocked_by_capacity {}",
        summary.runtime.mapped_asset_blocked_by_capacity
    ));
    lines.push(format!(
        "novovm_governance_runtime_external_inflow_demand_raw {}",
        summary.runtime.external_inflow_demand_raw
    ));
    lines.push(format!(
        "novovm_governance_runtime_external_inflow_demand_qualified {}",
        summary.runtime.external_inflow_demand_qualified
    ));
    lines.push(format!(
        "novovm_governance_runtime_shadow_register_verified {}",
        summary.runtime.shadow_register_verified
    ));
    lines.push(format!(
        "novovm_governance_runtime_shadow_burn_completed {}",
        summary.runtime.shadow_burn_completed
    ));
    lines.push(format!(
        "novovm_governance_runtime_shadow_release_completed {}",
        summary.runtime.shadow_release_completed
    ));
    lines.push(format!(
        "novovm_governance_runtime_shadow_closed_loops {}",
        summary.runtime.shadow_closed_loops
    ));
    lines.push(format!(
        "novovm_governance_runtime_execution_policy_errors {}",
        summary.runtime.execution_policy_errors
    ));
    lines.push(format!(
        "novovm_governance_phase4_decision_go {}",
        if summary.phase4_decision.decision == crate::metrics::Phase4Decision::Go {
            1
        } else {
            0
        }
    ));
    lines.push(format!(
        "novovm_governance_phase4_blocked_consecutive_cycles {}",
        summary.phase4_decision.blocked_consecutive_cycles
    ));
    lines.push(format!(
        "novovm_governance_phase4_blocked_rule_consecutive_cycles {}",
        summary.phase4_decision.blocked_rule_consecutive_cycles
    ));
    lines.push(format!(
        "novovm_governance_phase4_blocked_capacity_consecutive_cycles {}",
        summary.phase4_decision.blocked_capacity_consecutive_cycles
    ));
    lines.push(format!(
        "novovm_governance_phase4_blocked_consecutive_cycles_required {}",
        summary.phase4_decision.blocked_consecutive_cycles_required
    ));
    lines.push(format!(
        "novovm_governance_phase4_inflow_consecutive_cycles {}",
        summary.phase4_decision.inflow_consecutive_cycles
    ));
    lines.push(format!(
        "novovm_governance_phase4_inflow_consecutive_cycles_required {}",
        summary.phase4_decision.inflow_consecutive_cycles_required
    ));
    lines.push(format!(
        "novovm_governance_phase4_privacy_rejected_rate {}",
        summary.phase4_decision.privacy_rejected_rate
    ));
    lines.push(format!(
        "novovm_governance_phase4_privacy_required_requests {}",
        summary.phase4_decision.privacy_required_requests
    ));
    lines.push(format!(
        "novovm_governance_phase4_privacy_min_required_requests {}",
        summary.phase4_decision.privacy_min_required_requests
    ));
    lines.push(format!(
        "novovm_governance_phase4_shadow_closed_loops {}",
        summary.phase4_decision.shadow_closed_loops
    ));
    lines.push(format!(
        "novovm_governance_phase4_shadow_closed_loops_required {}",
        summary.phase4_decision.shadow_closed_loops_required
    ));
    lines.push(format!(
        "novovm_governance_phase4_shadow_register_verified {}",
        summary.phase4_decision.shadow_register_verified
    ));
    lines.push(format!(
        "novovm_governance_phase4_shadow_burn_completed {}",
        summary.phase4_decision.shadow_burn_completed
    ));
    lines.push(format!(
        "novovm_governance_phase4_shadow_release_completed {}",
        summary.phase4_decision.shadow_release_completed
    ));
    lines.push(format!(
        "novovm_governance_phase4_shadow_closed_loop_rate {}",
        summary.phase4_decision.shadow_closed_loop_rate
    ));
    lines.push(format!(
        "novovm_governance_phase4_shadow_min_register_samples {}",
        summary.phase4_decision.shadow_min_register_samples
    ));
    lines.push(format!(
        "novovm_governance_phase4_shadow_closed_loop_rate_threshold {}",
        summary.phase4_decision.shadow_closed_loop_rate_threshold
    ));
    lines.push(format!(
        "novovm_governance_phase4_shadow_criteria_sample_size_met {}",
        if summary.phase4_decision.criteria_shadow_sample_size {
            1
        } else {
            0
        }
    ));
    lines.push(format!(
        "novovm_governance_phase4_shadow_criteria_closed_loop_rate_met {}",
        if summary.phase4_decision.criteria_shadow_closed_loop_rate {
            1
        } else {
            0
        }
    ));
    let bottleneck = summary.phase4_decision.shadow_bottleneck_stage.as_str();
    for stage in ["register_to_burn", "burn_to_release", "release_to_closed"] {
        lines.push(format!(
            "novovm_governance_phase4_shadow_bottleneck_stage{{stage=\"{}\"}} {}",
            stage,
            if bottleneck == stage { 1 } else { 0 }
        ));
    }
    for trigger in &summary.triggers {
        lines.push(format!(
            "novovm_governance_trigger_evaluated{{trigger_type=\"{}\"}} {}",
            trigger.trigger_type, trigger.evaluated
        ));
        lines.push(format!(
            "novovm_governance_trigger_satisfied{{trigger_type=\"{}\"}} {}",
            trigger.trigger_type, trigger.satisfied
        ));
        lines.push(format!(
            "novovm_governance_trigger_rejected{{trigger_type=\"{}\"}} {}",
            trigger.trigger_type, trigger.rejected
        ));
        lines.push(format!(
            "novovm_governance_trigger_avg_evidence_score{{trigger_type=\"{}\"}} {}",
            trigger.trigger_type, trigger.avg_evidence_score
        ));
    }
    lines.join("\n")
}
