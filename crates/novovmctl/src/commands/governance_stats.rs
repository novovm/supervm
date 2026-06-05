use std::path::{Path, PathBuf};

use serde::Serialize;

use crate::cli::GovernanceStatsArgs;
use crate::error::CtlError;
use crate::output;
use crate::runtime::files;
use novovm_governance_observability::export::prometheus::render_prometheus_text;
use novovm_governance_observability::{
    build_governance_assessment, default_governance_events_dir, discover_governance_event_files,
    load_governance_events_from_paths, GovernanceAssessment, GovernanceThresholds, Phase4Decision,
    Phase4DecisionThresholds,
};

const DEFAULT_PHASE4_GATE_REPORT_JSON: &str =
    "artifacts/governance/phase4-governance-gate-report.json";
const DEFAULT_PHASE4_GATE_REPORT_MD: &str = "artifacts/governance/phase4-governance-gate-report.md";

#[derive(Debug, Serialize)]
struct GovernanceStatsReport {
    event_files: Vec<String>,
    event_count: usize,
    thresholds: GovernanceThresholds,
    assessment: GovernanceAssessment,
    phase4_gate_report: Phase4GovernanceGateReportV1,
    phase4_gate_report_artifacts: Phase4GateReportArtifacts,
}

#[derive(Debug, Clone, Serialize, Default)]
struct Phase4GateReportArtifacts {
    json_report_path: Option<String>,
    markdown_report_path: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
struct Phase4GateActionPolicy {
    enter_governance_proposal_path: bool,
    auto_activation_allowed: bool,
    required_next_steps: Vec<String>,
}

#[derive(Debug, Clone, Serialize)]
struct Phase4GovernanceGateReportV1 {
    schema: String,
    generated_at_unix_ms: u64,
    decision: String,
    recommendation: String,
    decision_scope: String,
    action_policy: Phase4GateActionPolicy,
    phase4_decision: novovm_governance_observability::Phase4DecisionReport,
    runtime_signals: novovm_governance_observability::RuntimeSignals,
    gate_metrics: novovm_governance_observability::GateMetrics,
    trigger_metrics: Vec<novovm_governance_observability::TriggerMetrics>,
    warnings: Vec<String>,
    signals: Vec<String>,
    thresholds: GovernanceThresholds,
    event_count: usize,
    source_event_files: Vec<String>,
}

pub fn run(args: GovernanceStatsArgs) -> Result<(), CtlError> {
    let mut report = inner_run(&args)?;
    report.phase4_gate_report_artifacts = maybe_write_phase4_gate_report_artifacts(&args, &report)?;

    if args.as_prometheus {
        println!("{}", render_prometheus_text(&report.assessment));
        return Ok(());
    }

    print_text_summary(&report.assessment, &report.phase4_gate_report);
    if let Some(path) = report
        .phase4_gate_report_artifacts
        .json_report_path
        .as_deref()
    {
        println!("Phase4 gate report (json): {path}");
    }
    if let Some(path) = report
        .phase4_gate_report_artifacts
        .markdown_report_path
        .as_deref()
    {
        println!("Phase4 gate report (md): {path}");
    }
    output::print_success_json("governance-stats", &report)?;
    Ok(())
}

fn inner_run(args: &GovernanceStatsArgs) -> Result<GovernanceStatsReport, CtlError> {
    let files = resolve_event_files(args)?;
    let events = load_governance_events_from_paths(&files)
        .map_err(|e| CtlError::FileReadFailed(format!("load governance events failed: {e}")))?;
    let thresholds = GovernanceThresholds {
        phase4_blocked_threshold: args.phase4_block_threshold,
        phase4_decision: Phase4DecisionThresholds {
            window_cycles: args.phase4_window_cycles.max(1),
            blocked_per_cycle_threshold: args.phase4_blocked_per_cycle_threshold,
            blocked_consecutive_cycles_required: args.phase4_blocked_consecutive_cycles,
            privacy_rejected_rate_threshold: args.phase4_privacy_rejected_rate_threshold,
            privacy_min_required_requests: args.phase4_privacy_min_required_requests,
            external_inflow_per_cycle_threshold: args.phase4_inflow_per_cycle_threshold,
            inflow_consecutive_cycles_required: args.phase4_inflow_consecutive_cycles,
            shadow_closed_loops_required: args.phase4_shadow_closed_loops_required,
            shadow_min_register_samples: args.phase4_shadow_min_register_samples,
            shadow_closed_loop_rate_threshold: args.phase4_shadow_closed_loop_rate_threshold,
        },
        ..GovernanceThresholds::default()
    };
    let assessment = build_governance_assessment(&events, thresholds);
    let event_files = files
        .iter()
        .map(|path| path.display().to_string())
        .collect::<Vec<_>>();
    let phase4_gate_report =
        build_phase4_gate_report(&assessment, thresholds, event_files.clone(), events.len());

    Ok(GovernanceStatsReport {
        event_files,
        event_count: events.len(),
        thresholds,
        assessment,
        phase4_gate_report,
        phase4_gate_report_artifacts: Phase4GateReportArtifacts::default(),
    })
}

fn build_phase4_gate_report(
    assessment: &GovernanceAssessment,
    thresholds: GovernanceThresholds,
    source_event_files: Vec<String>,
    event_count: usize,
) -> Phase4GovernanceGateReportV1 {
    let enter_governance_proposal_path = assessment.phase4_decision.decision == Phase4Decision::Go;
    let recommendation = if enter_governance_proposal_path {
        "enter_governance_proposal_vote_timelock_review".to_string()
    } else {
        "remain_no_go_collect_more_evidence".to_string()
    };
    let decision = if enter_governance_proposal_path {
        "go".to_string()
    } else {
        "no-go".to_string()
    };
    let required_next_steps = if enter_governance_proposal_path {
        vec![
            "start Phase4 governance proposal".to_string(),
            "collect vote evidence and timelock schedule".to_string(),
            "run Phase4 MVP slice in controlled shadow mode before activation".to_string(),
        ]
    } else {
        let mut steps = vec![
            "remain in No-Go".to_string(),
            "continue collecting qualified inflow/capacity evidence".to_string(),
            "re-evaluate in next governance cycle".to_string(),
        ];
        if assessment.phase4_decision.shadow_bottleneck_stage != "none"
            && assessment.phase4_decision.shadow_bottleneck_stage != "no_data"
        {
            steps.push(format!(
                "stabilize shadow bottleneck stage: {}",
                assessment.phase4_decision.shadow_bottleneck_stage
            ));
        }
        steps
    };

    Phase4GovernanceGateReportV1 {
        schema: "novovm.phase4-governance-gate-report.v1".to_string(),
        generated_at_unix_ms: output::now_unix_ms(),
        decision,
        recommendation,
        decision_scope: assessment.phase4_decision.decision_scope.clone(),
        action_policy: Phase4GateActionPolicy {
            enter_governance_proposal_path,
            auto_activation_allowed: false,
            required_next_steps,
        },
        phase4_decision: assessment.phase4_decision.clone(),
        runtime_signals: assessment.runtime,
        gate_metrics: assessment.gate,
        trigger_metrics: assessment.triggers.clone(),
        warnings: assessment.warnings.clone(),
        signals: assessment.signals.clone(),
        thresholds,
        event_count,
        source_event_files,
    }
}

fn resolve_event_files(args: &GovernanceStatsArgs) -> Result<Vec<PathBuf>, CtlError> {
    if !args.events_file.is_empty() {
        let mut out = Vec::with_capacity(args.events_file.len());
        for raw in &args.events_file {
            let path = PathBuf::from(raw);
            if !path.exists() {
                return Err(CtlError::FileReadFailed(format!(
                    "governance events file not found: {}",
                    path.display()
                )));
            }
            out.push(path);
        }
        out.sort();
        return Ok(out);
    }

    let dir = args
        .events_dir
        .as_deref()
        .map(PathBuf::from)
        .unwrap_or_else(default_governance_events_dir);
    discover_governance_event_files(dir.as_path()).map_err(|e| {
        CtlError::FileReadFailed(format!(
            "discover governance events failed: dir={} error={e}",
            dir.display()
        ))
    })
}

fn maybe_write_phase4_gate_report_artifacts(
    args: &GovernanceStatsArgs,
    report: &GovernanceStatsReport,
) -> Result<Phase4GateReportArtifacts, CtlError> {
    let json_path = args.phase4_gate_report_out.clone().or_else(|| {
        if args.write_phase4_gate_report {
            Some(DEFAULT_PHASE4_GATE_REPORT_JSON.to_string())
        } else {
            None
        }
    });
    let markdown_path = args.phase4_gate_report_md_out.clone().or_else(|| {
        if args.write_phase4_gate_report {
            Some(DEFAULT_PHASE4_GATE_REPORT_MD.to_string())
        } else {
            None
        }
    });

    if let Some(path) = json_path.as_deref() {
        files::write_json_pretty(path, &report.phase4_gate_report)?;
    }
    if let Some(path) = markdown_path.as_deref() {
        let rendered = render_phase4_gate_report_markdown(&report.phase4_gate_report);
        write_text_file(path, rendered.as_str())?;
    }

    Ok(Phase4GateReportArtifacts {
        json_report_path: json_path,
        markdown_report_path: markdown_path,
    })
}

fn write_text_file(path: &str, content: &str) -> Result<(), CtlError> {
    if let Some(parent) = Path::new(path).parent() {
        std::fs::create_dir_all(parent).map_err(|e| {
            CtlError::FileWriteFailed(format!("create parent dir for `{path}`: {e}"))
        })?;
    }
    std::fs::write(path, content)
        .map_err(|e| CtlError::FileWriteFailed(format!("write `{path}`: {e}")))?;
    Ok(())
}

fn render_phase4_gate_report_markdown(report: &Phase4GovernanceGateReportV1) -> String {
    let mut out = String::new();
    out.push_str("# Phase4 Governance Gate Report v1\n\n");
    out.push_str(&format!(
        "- generated_at_unix_ms: {}\n",
        report.generated_at_unix_ms
    ));
    out.push_str(&format!("- decision: {}\n", report.decision));
    out.push_str(&format!("- recommendation: {}\n", report.recommendation));
    out.push_str(&format!("- decision_scope: {}\n", report.decision_scope));
    out.push_str(&format!(
        "- enter_governance_proposal_path: {}\n",
        report.action_policy.enter_governance_proposal_path
    ));
    out.push_str(&format!(
        "- auto_activation_allowed: {}\n\n",
        report.action_policy.auto_activation_allowed
    ));

    out.push_str("## Criteria\n\n");
    out.push_str(&format!(
        "- blocked_pressure: {} (consecutive={} required={} per_cycle_threshold={})\n",
        report.phase4_decision.criteria_blocked_pressure,
        report.phase4_decision.blocked_consecutive_cycles,
        report.phase4_decision.blocked_consecutive_cycles_required,
        report.phase4_decision.blocked_per_cycle_threshold
    ));
    out.push_str(&format!(
        "- blocked_rule_consecutive_cycles: {}\n",
        report.phase4_decision.blocked_rule_consecutive_cycles
    ));
    out.push_str(&format!(
        "- blocked_capacity_consecutive_cycles: {}\n",
        report.phase4_decision.blocked_capacity_consecutive_cycles
    ));
    out.push_str(&format!(
        "- external_inflow: {} (consecutive={} required={} per_cycle_threshold={})\n",
        report.phase4_decision.criteria_external_inflow,
        report.phase4_decision.inflow_consecutive_cycles,
        report.phase4_decision.inflow_consecutive_cycles_required,
        report.phase4_decision.inflow_per_cycle_threshold
    ));
    out.push_str(&format!(
        "- privacy_bottleneck: {} (rejected_rate={:.2}% threshold={:.2}% required_requests={} min_required={})\n\n",
        report.phase4_decision.criteria_privacy_bottleneck,
        report.phase4_decision.privacy_rejected_rate * 100.0,
        report.phase4_decision.privacy_rejected_rate_threshold * 100.0,
        report.phase4_decision.privacy_required_requests,
        report.phase4_decision.privacy_min_required_requests
    ));
    out.push_str(&format!(
        "- shadow_closed_loop: {} (closed_loops={} required={})\n\n",
        report.phase4_decision.criteria_shadow_closed_loop,
        report.phase4_decision.shadow_closed_loops,
        report.phase4_decision.shadow_closed_loops_required
    ));
    out.push_str(&format!(
        "- shadow_sample_size: {} (register_verified={} min_required={})\n",
        report.phase4_decision.criteria_shadow_sample_size,
        report.phase4_decision.shadow_register_verified,
        report.phase4_decision.shadow_min_register_samples
    ));
    out.push_str(&format!(
        "- shadow_closed_loop_rate: {} (rate={:.2}% threshold={:.2}%)\n",
        report.phase4_decision.criteria_shadow_closed_loop_rate,
        report.phase4_decision.shadow_closed_loop_rate * 100.0,
        report.phase4_decision.shadow_closed_loop_rate_threshold * 100.0
    ));
    out.push_str(&format!(
        "- shadow_bottleneck_stage: {}\n\n",
        report.phase4_decision.shadow_bottleneck_stage
    ));

    out.push_str("## Runtime Signals\n\n");
    out.push_str(&format!(
        "- mapped_asset_register_attempts: {}\n",
        report.runtime_signals.mapped_asset_register_attempts
    ));
    out.push_str(&format!(
        "- mapped_asset_blocked_by_nogo: {} (rule={} capacity={})\n",
        report.runtime_signals.mapped_asset_blocked_by_nogo,
        report.runtime_signals.mapped_asset_blocked_by_rule,
        report.runtime_signals.mapped_asset_blocked_by_capacity
    ));
    out.push_str(&format!(
        "- external_inflow_demand: raw={} qualified={}\n",
        report.runtime_signals.external_inflow_demand_raw,
        report.runtime_signals.external_inflow_demand_qualified
    ));
    out.push_str(&format!(
        "- privacy_required_requests: {} privacy_rejected: {}\n\n",
        report.runtime_signals.privacy_required_requests, report.runtime_signals.privacy_rejected
    ));
    out.push_str(&format!(
        "- shadow_flow: register_verified={} burn_completed={} release_completed={} closed_loops={}\n\n",
        report.runtime_signals.shadow_register_verified,
        report.runtime_signals.shadow_burn_completed,
        report.runtime_signals.shadow_release_completed,
        report.runtime_signals.shadow_closed_loops
    ));

    out.push_str("## Required Next Steps\n\n");
    for step in &report.action_policy.required_next_steps {
        out.push_str(&format!("- {step}\n"));
    }
    out.push_str("\n## Rationale\n\n");
    for line in &report.phase4_decision.rationale {
        out.push_str(&format!("- {line}\n"));
    }
    out
}

fn print_text_summary(
    assessment: &GovernanceAssessment,
    gate_report: &Phase4GovernanceGateReportV1,
) {
    println!("PR Gate:");
    println!("  total={}", assessment.gate.total_prs);
    println!(
        "  rejected_no_trigger={} ({})",
        assessment.gate.rejected_no_trigger,
        format_ratio(
            assessment.gate.rejected_no_trigger,
            assessment.gate.total_prs
        )
    );
    println!(
        "  rejected_invalid_trigger={}",
        assessment.gate.rejected_invalid_trigger
    );
    println!(
        "  passed_with_trigger={}",
        assessment.gate.passed_with_trigger
    );
    println!();

    println!("Trigger:");
    if assessment.triggers.is_empty() {
        println!("  evaluated=0");
    } else {
        for trigger in &assessment.triggers {
            println!(
                "  {} evaluated={} satisfied={} ({}) rejected={} avg_evidence_score={:.2}",
                trigger.trigger_type,
                trigger.evaluated,
                trigger.satisfied,
                format_ratio(trigger.satisfied, trigger.evaluated),
                trigger.rejected,
                trigger.avg_evidence_score
            );
        }
    }
    println!();

    println!("Runtime:");
    println!(
        "  pq_required={} pq_rejected={} ({})",
        assessment.runtime.pq_required_requests,
        assessment.runtime.pq_rejected,
        format_ratio(
            assessment.runtime.pq_rejected,
            assessment.runtime.pq_required_requests
        )
    );
    println!(
        "  privacy_required={} privacy_rejected={} ({})",
        assessment.runtime.privacy_required_requests,
        assessment.runtime.privacy_rejected,
        format_ratio(
            assessment.runtime.privacy_rejected,
            assessment.runtime.privacy_required_requests
        )
    );
    println!(
        "  mapped_asset_register_attempts={} blocked_by_nogo={} (rule={} capacity={})",
        assessment.runtime.mapped_asset_register_attempts,
        assessment.runtime.mapped_asset_blocked_by_nogo,
        assessment.runtime.mapped_asset_blocked_by_rule,
        assessment.runtime.mapped_asset_blocked_by_capacity
    );
    println!(
        "  external_inflow_demand: raw={} qualified={}",
        assessment.runtime.external_inflow_demand_raw,
        assessment.runtime.external_inflow_demand_qualified
    );
    println!(
        "  shadow_flow: register_verified={} burn_completed={} release_completed={} closed_loops={}",
        assessment.runtime.shadow_register_verified,
        assessment.runtime.shadow_burn_completed,
        assessment.runtime.shadow_release_completed,
        assessment.runtime.shadow_closed_loops
    );
    println!(
        "  execution_policy_errors={}",
        assessment.runtime.execution_policy_errors
    );
    println!();

    println!("Phase4 Decision:");
    println!(
        "  decision={} recommendation={}",
        gate_report.decision, gate_report.recommendation
    );
    println!("  decision_scope={}", gate_report.decision_scope);
    println!(
        "  blocked_consecutive={} (rule={} capacity={} required={} per_cycle_threshold={})",
        assessment.phase4_decision.blocked_consecutive_cycles,
        assessment.phase4_decision.blocked_rule_consecutive_cycles,
        assessment
            .phase4_decision
            .blocked_capacity_consecutive_cycles,
        assessment
            .phase4_decision
            .blocked_consecutive_cycles_required,
        assessment.phase4_decision.blocked_per_cycle_threshold
    );
    println!(
        "  inflow_consecutive={} (required={} per_cycle_threshold={})",
        assessment.phase4_decision.inflow_consecutive_cycles,
        assessment
            .phase4_decision
            .inflow_consecutive_cycles_required,
        assessment.phase4_decision.inflow_per_cycle_threshold
    );
    println!(
        "  privacy_rejected_rate={} (threshold={} required_requests={} min_required={})",
        format_args!(
            "{:.2}%",
            assessment.phase4_decision.privacy_rejected_rate * 100.0
        ),
        format_args!(
            "{:.2}%",
            assessment.phase4_decision.privacy_rejected_rate_threshold * 100.0
        ),
        assessment.phase4_decision.privacy_required_requests,
        assessment.phase4_decision.privacy_min_required_requests
    );
    println!(
        "  criteria: blocked_pressure={} external_inflow={} privacy_bottleneck={}",
        assessment.phase4_decision.criteria_blocked_pressure,
        assessment.phase4_decision.criteria_external_inflow,
        assessment.phase4_decision.criteria_privacy_bottleneck
    );
    println!(
        "  shadow_closed_loop={} (closed_loops={} required={})",
        assessment.phase4_decision.criteria_shadow_closed_loop,
        assessment.phase4_decision.shadow_closed_loops,
        assessment.phase4_decision.shadow_closed_loops_required
    );
    println!(
        "  shadow_sample_size={} (register_verified={} min_required={})",
        assessment.phase4_decision.criteria_shadow_sample_size,
        assessment.phase4_decision.shadow_register_verified,
        assessment.phase4_decision.shadow_min_register_samples
    );
    println!(
        "  shadow_closed_loop_rate={} (rate={} threshold={})",
        assessment.phase4_decision.criteria_shadow_closed_loop_rate,
        format_args!(
            "{:.2}%",
            assessment.phase4_decision.shadow_closed_loop_rate * 100.0
        ),
        format_args!(
            "{:.2}%",
            assessment.phase4_decision.shadow_closed_loop_rate_threshold * 100.0
        )
    );
    println!(
        "  shadow_bottleneck_stage={}",
        assessment.phase4_decision.shadow_bottleneck_stage
    );
    for line in &assessment.phase4_decision.rationale {
        println!("  rationale: {line}");
    }
    println!();

    println!("Assessment:");
    if assessment.warnings.is_empty() && assessment.signals.is_empty() {
        println!("  No warnings/signals. Governance baseline appears stable.");
        return;
    }
    for warning in &assessment.warnings {
        println!("  WARN: {warning}");
    }
    for signal in &assessment.signals {
        println!("  SIGNAL: {signal}");
    }
}

fn format_ratio(numerator: u64, denominator: u64) -> String {
    if denominator == 0 {
        return "-".to_string();
    }
    let ratio = numerator as f64 / denominator as f64;
    format!("{:.2}%", ratio * 100.0)
}
