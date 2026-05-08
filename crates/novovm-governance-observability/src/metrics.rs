#![forbid(unsafe_code)]

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use crate::collectors::{
    pr_gate::{collect_gate_metrics, GateMetrics},
    runtime::{collect_runtime_signals, RuntimeSignals},
    trigger::{collect_trigger_metrics, TriggerMetrics},
};
use crate::events::{GovernanceEvent, GovernanceEventEnvelope};

const MILLIS_PER_CYCLE_DAY: u64 = 86_400_000;

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub struct Phase4DecisionThresholds {
    pub window_cycles: u64,
    pub blocked_per_cycle_threshold: u64,
    pub blocked_consecutive_cycles_required: u64,
    pub privacy_rejected_rate_threshold: f64,
    pub privacy_min_required_requests: u64,
    pub external_inflow_per_cycle_threshold: u64,
    pub inflow_consecutive_cycles_required: u64,
    pub shadow_closed_loops_required: u64,
    pub shadow_min_register_samples: u64,
    pub shadow_closed_loop_rate_threshold: f64,
}

impl Default for Phase4DecisionThresholds {
    fn default() -> Self {
        Self {
            window_cycles: 7,
            blocked_per_cycle_threshold: 5,
            blocked_consecutive_cycles_required: 3,
            privacy_rejected_rate_threshold: 0.4,
            privacy_min_required_requests: 50,
            external_inflow_per_cycle_threshold: 10,
            inflow_consecutive_cycles_required: 3,
            shadow_closed_loops_required: 1,
            shadow_min_register_samples: 20,
            shadow_closed_loop_rate_threshold: 0.6,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Phase4Decision {
    Go,
    NoGo,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Phase4DecisionReport {
    pub decision: Phase4Decision,
    pub decision_scope: String,
    pub window_cycles: u64,
    pub blocked_consecutive_cycles: u64,
    pub blocked_rule_consecutive_cycles: u64,
    pub blocked_capacity_consecutive_cycles: u64,
    pub blocked_consecutive_cycles_required: u64,
    pub blocked_per_cycle_threshold: u64,
    pub inflow_consecutive_cycles: u64,
    pub inflow_consecutive_cycles_required: u64,
    pub inflow_per_cycle_threshold: u64,
    pub privacy_required_requests: u64,
    pub privacy_min_required_requests: u64,
    pub privacy_rejected: u64,
    pub privacy_rejected_rate: f64,
    pub privacy_rejected_rate_threshold: f64,
    pub criteria_blocked_pressure: bool,
    pub criteria_privacy_bottleneck: bool,
    pub criteria_external_inflow: bool,
    pub criteria_shadow_closed_loop: bool,
    pub criteria_shadow_sample_size: bool,
    pub criteria_shadow_closed_loop_rate: bool,
    pub shadow_register_verified: u64,
    pub shadow_burn_completed: u64,
    pub shadow_release_completed: u64,
    pub shadow_closed_loops: u64,
    pub shadow_closed_loops_required: u64,
    pub shadow_closed_loop_rate: f64,
    pub shadow_min_register_samples: u64,
    pub shadow_closed_loop_rate_threshold: f64,
    pub shadow_bottleneck_stage: String,
    pub rationale: Vec<String>,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub struct GovernanceThresholds {
    pub gate_rejected_no_trigger_warn_ratio: f64,
    pub privacy_rejected_signal_ratio: f64,
    pub phase4_blocked_threshold: u64,
    pub phase4_decision: Phase4DecisionThresholds,
}

impl Default for GovernanceThresholds {
    fn default() -> Self {
        Self {
            gate_rejected_no_trigger_warn_ratio: 0.8,
            privacy_rejected_signal_ratio: 0.4,
            phase4_blocked_threshold: 10,
            phase4_decision: Phase4DecisionThresholds::default(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GovernanceAssessment {
    pub gate: GateMetrics,
    pub triggers: Vec<TriggerMetrics>,
    pub runtime: RuntimeSignals,
    pub phase4_decision: Phase4DecisionReport,
    pub warnings: Vec<String>,
    pub signals: Vec<String>,
}

#[must_use]
pub fn build_governance_assessment(
    events: &[GovernanceEventEnvelope],
    thresholds: GovernanceThresholds,
) -> GovernanceAssessment {
    let gate = collect_gate_metrics(events);
    let triggers = collect_trigger_metrics(events);
    let runtime = collect_runtime_signals(events);
    let phase4_decision = evaluate_phase4_decision(events, thresholds.phase4_decision);

    let mut warnings = Vec::new();
    let mut signals = Vec::new();

    let rejected_no_trigger_ratio = gate.rejected_no_trigger_ratio();
    if gate.total_prs > 0
        && rejected_no_trigger_ratio > thresholds.gate_rejected_no_trigger_warn_ratio
    {
        warnings.push(format!(
            "Gate may be too strict: rejected_no_trigger/total_prs = {:.2}%",
            rejected_no_trigger_ratio * 100.0
        ));
    }

    let privacy_rejected_ratio = runtime.privacy_rejected_ratio();
    if runtime.privacy_required_requests > 0
        && privacy_rejected_ratio > thresholds.privacy_rejected_signal_ratio
    {
        signals.push(format!(
            "Privacy capacity insufficient: privacy_rejected/privacy_required = {:.2}%",
            privacy_rejected_ratio * 100.0
        ));
    }

    if runtime.external_inflow_demand_qualified > 0
        && runtime.mapped_asset_blocked_by_nogo >= thresholds.phase4_blocked_threshold
    {
        signals.push(format!(
            "Phase 4 trigger candidate: blocked_by_nogo={} qualified_inflow_demand={}",
            runtime.mapped_asset_blocked_by_nogo, runtime.external_inflow_demand_qualified
        ));
    }
    if runtime.external_inflow_demand_qualified > 0 && runtime.shadow_closed_loops == 0 {
        warnings.push(
            "Phase 4 shadow evidence missing: qualified inflow exists but no closed shadow loops"
                .to_string(),
        );
    }
    if runtime.external_inflow_demand_qualified > 0
        && runtime.shadow_register_verified < thresholds.phase4_decision.shadow_min_register_samples
    {
        warnings.push(format!(
            "Phase 4 shadow sample too thin: shadow_register_verified={} min_required={}",
            runtime.shadow_register_verified,
            thresholds.phase4_decision.shadow_min_register_samples
        ));
    }
    if runtime.shadow_register_verified > 0
        && phase4_decision.shadow_bottleneck_stage != "none"
        && phase4_decision.shadow_bottleneck_stage != "no_data"
    {
        signals.push(format!(
            "Shadow bottleneck detected at stage={}",
            phase4_decision.shadow_bottleneck_stage
        ));
    }

    if phase4_decision.decision == Phase4Decision::Go {
        signals.push(format!(
            "Phase 4 quantitative decision=Go (blocked_consecutive={} inflow_consecutive={} privacy_rejected_rate={:.2}% shadow_closed_loops={} shadow_closed_loop_rate={:.2}%)",
            phase4_decision.blocked_consecutive_cycles,
            phase4_decision.inflow_consecutive_cycles,
            phase4_decision.privacy_rejected_rate * 100.0,
            phase4_decision.shadow_closed_loops,
            phase4_decision.shadow_closed_loop_rate * 100.0
        ));
    }

    GovernanceAssessment {
        gate,
        triggers,
        runtime,
        phase4_decision,
        warnings,
        signals,
    }
}

#[derive(Debug, Clone, Copy, Default)]
struct CyclePressure {
    blocked_rule: u64,
    blocked_capacity: u64,
    external_inflow_demand_raw: u64,
    external_inflow_demand_qualified: u64,
}

fn evaluate_phase4_decision(
    events: &[GovernanceEventEnvelope],
    thresholds: Phase4DecisionThresholds,
) -> Phase4DecisionReport {
    if events.is_empty() {
        return Phase4DecisionReport {
            decision: Phase4Decision::NoGo,
            decision_scope: "go_allows_governance_proposal_vote_path_not_auto_activation"
                .to_string(),
            window_cycles: thresholds.window_cycles.max(1),
            blocked_consecutive_cycles: 0,
            blocked_rule_consecutive_cycles: 0,
            blocked_capacity_consecutive_cycles: 0,
            blocked_consecutive_cycles_required: thresholds.blocked_consecutive_cycles_required,
            blocked_per_cycle_threshold: thresholds.blocked_per_cycle_threshold,
            inflow_consecutive_cycles: 0,
            inflow_consecutive_cycles_required: thresholds.inflow_consecutive_cycles_required,
            inflow_per_cycle_threshold: thresholds.external_inflow_per_cycle_threshold,
            privacy_required_requests: 0,
            privacy_min_required_requests: thresholds.privacy_min_required_requests,
            privacy_rejected: 0,
            privacy_rejected_rate: 0.0,
            privacy_rejected_rate_threshold: thresholds.privacy_rejected_rate_threshold,
            criteria_blocked_pressure: false,
            criteria_privacy_bottleneck: false,
            criteria_external_inflow: false,
            criteria_shadow_closed_loop: false,
            criteria_shadow_sample_size: false,
            criteria_shadow_closed_loop_rate: false,
            shadow_register_verified: 0,
            shadow_burn_completed: 0,
            shadow_release_completed: 0,
            shadow_closed_loops: 0,
            shadow_closed_loops_required: thresholds.shadow_closed_loops_required,
            shadow_closed_loop_rate: 0.0,
            shadow_min_register_samples: thresholds.shadow_min_register_samples,
            shadow_closed_loop_rate_threshold: thresholds.shadow_closed_loop_rate_threshold,
            shadow_bottleneck_stage: "no_data".to_string(),
            rationale: vec!["insufficient evidence: no governance events".to_string()],
        };
    }

    let window_cycles = thresholds.window_cycles.max(1);
    let latest_cycle = events
        .iter()
        .map(|event| event.at_unix_ms / MILLIS_PER_CYCLE_DAY)
        .max()
        .unwrap_or(0);
    let start_cycle = latest_cycle.saturating_sub(window_cycles - 1);

    let mut pressure_by_cycle = BTreeMap::<u64, CyclePressure>::new();
    for cycle in start_cycle..=latest_cycle {
        pressure_by_cycle.insert(cycle, CyclePressure::default());
    }

    let mut window_events = Vec::new();
    let mut has_external_inflow_observed = false;
    for event in events {
        let cycle = event.at_unix_ms / MILLIS_PER_CYCLE_DAY;
        if cycle < start_cycle || cycle > latest_cycle {
            continue;
        }
        window_events.push(event.clone());
        let Some(pressure) = pressure_by_cycle.get_mut(&cycle) else {
            continue;
        };
        match &event.event {
            GovernanceEvent::ExternalInflowDemandObserved {
                channel, qualified, ..
            } => {
                if is_mapped_lock_register_channel(channel) {
                    has_external_inflow_observed = true;
                    pressure.external_inflow_demand_raw =
                        pressure.external_inflow_demand_raw.saturating_add(1);
                    if *qualified {
                        pressure.external_inflow_demand_qualified =
                            pressure.external_inflow_demand_qualified.saturating_add(1);
                    }
                }
            }
            GovernanceEvent::Phase4Blocked {
                reason, block_kind, ..
            } => {
                let normalized_kind = block_kind
                    .as_deref()
                    .map(normalize_token)
                    .or_else(|| infer_block_kind_from_reason(reason.as_str()));
                match normalized_kind.as_deref() {
                    Some("capacity") => {
                        pressure.blocked_capacity = pressure.blocked_capacity.saturating_add(1);
                    }
                    _ => {
                        pressure.blocked_rule = pressure.blocked_rule.saturating_add(1);
                    }
                }
            }
            _ => {}
        }
    }
    if !has_external_inflow_observed {
        for event in &window_events {
            let cycle = event.at_unix_ms / MILLIS_PER_CYCLE_DAY;
            let Some(pressure) = pressure_by_cycle.get_mut(&cycle) else {
                continue;
            };
            let GovernanceEvent::MappedAssetOperationObserved { operation, .. } = &event.event
            else {
                continue;
            };
            if !is_register_lock_operation(operation) {
                continue;
            }
            pressure.external_inflow_demand_raw =
                pressure.external_inflow_demand_raw.saturating_add(1);
            pressure.external_inflow_demand_qualified =
                pressure.external_inflow_demand_qualified.saturating_add(1);
        }
    }

    let blocked_consecutive_cycles =
        count_consecutive_cycles_from_latest(&pressure_by_cycle, latest_cycle, |entry| {
            entry.blocked_rule.saturating_add(entry.blocked_capacity)
                >= thresholds.blocked_per_cycle_threshold
        });
    let blocked_rule_consecutive_cycles =
        count_consecutive_cycles_from_latest(&pressure_by_cycle, latest_cycle, |entry| {
            entry.blocked_rule >= thresholds.blocked_per_cycle_threshold
        });
    let blocked_capacity_consecutive_cycles =
        count_consecutive_cycles_from_latest(&pressure_by_cycle, latest_cycle, |entry| {
            entry.blocked_capacity >= thresholds.blocked_per_cycle_threshold
        });
    let inflow_consecutive_cycles =
        count_consecutive_cycles_from_latest(&pressure_by_cycle, latest_cycle, |entry| {
            entry.external_inflow_demand_qualified >= thresholds.external_inflow_per_cycle_threshold
        });

    let runtime = collect_runtime_signals(&window_events);
    let privacy_rejected_rate = runtime.privacy_rejected_ratio();

    let criteria_blocked_pressure =
        blocked_consecutive_cycles >= thresholds.blocked_consecutive_cycles_required;
    let criteria_external_inflow =
        inflow_consecutive_cycles >= thresholds.inflow_consecutive_cycles_required;
    let criteria_privacy_bottleneck = runtime.privacy_required_requests
        >= thresholds.privacy_min_required_requests
        && privacy_rejected_rate >= thresholds.privacy_rejected_rate_threshold;
    let criteria_shadow_closed_loop =
        runtime.shadow_closed_loops >= thresholds.shadow_closed_loops_required;
    let criteria_shadow_sample_size =
        runtime.shadow_register_verified >= thresholds.shadow_min_register_samples;
    let shadow_closed_loop_rate = ratio(
        runtime.shadow_closed_loops,
        runtime.shadow_register_verified,
    );
    let criteria_shadow_closed_loop_rate = runtime.shadow_register_verified > 0
        && shadow_closed_loop_rate >= thresholds.shadow_closed_loop_rate_threshold;
    let shadow_bottleneck_stage = shadow_bottleneck_stage(
        runtime.shadow_register_verified,
        runtime.shadow_burn_completed,
        runtime.shadow_release_completed,
        runtime.shadow_closed_loops,
    );

    let decision = if criteria_blocked_pressure
        && criteria_external_inflow
        && criteria_privacy_bottleneck
        && criteria_shadow_closed_loop
        && criteria_shadow_sample_size
        && criteria_shadow_closed_loop_rate
    {
        Phase4Decision::Go
    } else {
        Phase4Decision::NoGo
    };

    let mut rationale = Vec::new();
    rationale.push(format!(
        "blocked pressure {}: consecutive_cycles={} (rule={} capacity={}) required={} per_cycle_threshold={}",
        if criteria_blocked_pressure {
            "met"
        } else {
            "not_met"
        },
        blocked_consecutive_cycles,
        blocked_rule_consecutive_cycles,
        blocked_capacity_consecutive_cycles,
        thresholds.blocked_consecutive_cycles_required,
        thresholds.blocked_per_cycle_threshold
    ));
    rationale.push(format!(
        "external inflow (qualified) {}: consecutive_cycles={} required={} per_cycle_threshold={}",
        if criteria_external_inflow {
            "met"
        } else {
            "not_met"
        },
        inflow_consecutive_cycles,
        thresholds.inflow_consecutive_cycles_required,
        thresholds.external_inflow_per_cycle_threshold
    ));
    rationale.push(format!(
        "privacy bottleneck {}: rejected_rate={:.2}% threshold={:.2}% required_requests={} min_required={}",
        if criteria_privacy_bottleneck {
            "met"
        } else {
            "not_met"
        },
        privacy_rejected_rate * 100.0,
        thresholds.privacy_rejected_rate_threshold * 100.0,
        runtime.privacy_required_requests,
        thresholds.privacy_min_required_requests
    ));
    rationale.push(format!(
        "shadow closed loop {}: closed_loops={} required={}",
        if criteria_shadow_closed_loop {
            "met"
        } else {
            "not_met"
        },
        runtime.shadow_closed_loops,
        thresholds.shadow_closed_loops_required
    ));
    rationale.push(format!(
        "shadow sample size {}: register_verified={} min_required={}",
        if criteria_shadow_sample_size {
            "met"
        } else {
            "not_met"
        },
        runtime.shadow_register_verified,
        thresholds.shadow_min_register_samples
    ));
    rationale.push(format!(
        "shadow closed loop rate {}: rate={:.2}% threshold={:.2}% (closed_loops={} register_verified={})",
        if criteria_shadow_closed_loop_rate {
            "met"
        } else {
            "not_met"
        },
        shadow_closed_loop_rate * 100.0,
        thresholds.shadow_closed_loop_rate_threshold * 100.0,
        runtime.shadow_closed_loops,
        runtime.shadow_register_verified
    ));
    rationale.push(format!(
        "shadow bottleneck stage={}",
        shadow_bottleneck_stage
    ));

    Phase4DecisionReport {
        decision,
        decision_scope: "go_allows_governance_proposal_vote_path_not_auto_activation".to_string(),
        window_cycles,
        blocked_consecutive_cycles,
        blocked_rule_consecutive_cycles,
        blocked_capacity_consecutive_cycles,
        blocked_consecutive_cycles_required: thresholds.blocked_consecutive_cycles_required,
        blocked_per_cycle_threshold: thresholds.blocked_per_cycle_threshold,
        inflow_consecutive_cycles,
        inflow_consecutive_cycles_required: thresholds.inflow_consecutive_cycles_required,
        inflow_per_cycle_threshold: thresholds.external_inflow_per_cycle_threshold,
        privacy_required_requests: runtime.privacy_required_requests,
        privacy_min_required_requests: thresholds.privacy_min_required_requests,
        privacy_rejected: runtime.privacy_rejected,
        privacy_rejected_rate,
        privacy_rejected_rate_threshold: thresholds.privacy_rejected_rate_threshold,
        criteria_blocked_pressure,
        criteria_privacy_bottleneck,
        criteria_external_inflow,
        criteria_shadow_closed_loop,
        criteria_shadow_sample_size,
        criteria_shadow_closed_loop_rate,
        shadow_register_verified: runtime.shadow_register_verified,
        shadow_burn_completed: runtime.shadow_burn_completed,
        shadow_release_completed: runtime.shadow_release_completed,
        shadow_closed_loops: runtime.shadow_closed_loops,
        shadow_closed_loops_required: thresholds.shadow_closed_loops_required,
        shadow_closed_loop_rate,
        shadow_min_register_samples: thresholds.shadow_min_register_samples,
        shadow_closed_loop_rate_threshold: thresholds.shadow_closed_loop_rate_threshold,
        shadow_bottleneck_stage,
        rationale,
    }
}

fn count_consecutive_cycles_from_latest(
    pressure_by_cycle: &BTreeMap<u64, CyclePressure>,
    latest_cycle: u64,
    predicate: impl Fn(&CyclePressure) -> bool,
) -> u64 {
    let mut count = 0u64;
    let mut cycle = latest_cycle;
    loop {
        let Some(entry) = pressure_by_cycle.get(&cycle) else {
            break;
        };
        if !predicate(entry) {
            break;
        }
        count = count.saturating_add(1);
        if cycle == 0 {
            break;
        }
        cycle = cycle.saturating_sub(1);
    }
    count
}

fn ratio(numerator: u64, denominator: u64) -> f64 {
    if denominator == 0 {
        return 0.0;
    }
    numerator as f64 / denominator as f64
}

fn shadow_bottleneck_stage(register: u64, burn: u64, release: u64, closed: u64) -> String {
    if register == 0 {
        return "no_data".to_string();
    }
    let drop_register_to_burn = register.saturating_sub(burn);
    let drop_burn_to_release = burn.saturating_sub(release);
    let drop_release_to_closed = release.saturating_sub(closed);

    let mut max_drop = 0u64;
    let mut stage = "none";
    if drop_register_to_burn > max_drop {
        max_drop = drop_register_to_burn;
        stage = "register_to_burn";
    }
    if drop_burn_to_release > max_drop {
        max_drop = drop_burn_to_release;
        stage = "burn_to_release";
    }
    if drop_release_to_closed > max_drop {
        max_drop = drop_release_to_closed;
        stage = "release_to_closed";
    }
    if max_drop == 0 {
        "none".to_string()
    } else {
        stage.to_string()
    }
}

fn is_mapped_lock_register_channel(channel: &str) -> bool {
    let normalized = normalize_token(channel);
    normalized == "mappedlockregister" || normalized == "mappedlockregistershadow"
}

fn is_register_lock_operation(operation: &str) -> bool {
    let normalized = normalize_token(operation);
    normalized == "registerlock" || normalized == "shadowregisterlock"
}

fn normalize_token(raw: &str) -> String {
    raw.chars()
        .filter(|ch| ch.is_ascii_alphanumeric())
        .flat_map(char::to_lowercase)
        .collect()
}

fn infer_block_kind_from_reason(reason: &str) -> Option<String> {
    let normalized = normalize_token(reason);
    if normalized.contains("capacity") || normalized.contains("insufficient") {
        return Some("capacity".to_string());
    }
    if normalized.contains("nogo") || normalized.contains("policy") || normalized.contains("rule") {
        return Some("rule".to_string());
    }
    None
}

#[cfg(test)]
mod tests {
    use super::{build_governance_assessment, GovernanceThresholds, Phase4Decision};
    use crate::events::{GovernanceEvent, GovernanceEventEnvelope};

    #[test]
    fn governance_assessment_flags_expected_warnings_and_signals() {
        let events = vec![
            GovernanceEventEnvelope {
                at_unix_ms: 1_000_000,
                source: "test".to_string(),
                event: GovernanceEvent::PrGateEvaluated {
                    pr_id: "100".to_string(),
                    has_trigger: false,
                    trigger_valid: None,
                    accepted: false,
                    reason: Some("missing_trigger_payload".to_string()),
                    trigger_type: None,
                    governance_control_change: false,
                    has_governance_proof: true,
                },
            },
            GovernanceEventEnvelope {
                at_unix_ms: 1_000_001,
                source: "test".to_string(),
                event: GovernanceEvent::RuntimePolicyEvaluated {
                    policy: "PrivacyRequired".to_string(),
                    required: true,
                    accepted: false,
                    reason: Some("ERR_PRIVACY_REQUIRED_BUT_PATH_NOT_AVAILABLE".to_string()),
                    qualified_demand: Some(true),
                    account_id: Some("uca-a".to_string()),
                    demand_source: Some("test".to_string()),
                },
            },
            GovernanceEventEnvelope {
                at_unix_ms: 1_000_002,
                source: "test".to_string(),
                event: GovernanceEvent::MappedAssetOperationObserved {
                    operation: "register_lock".to_string(),
                    accepted: false,
                    account_id: Some("uca-a".to_string()),
                    mapping_id: None,
                    reason: Some("phase4_nogo_enforced".to_string()),
                    demand_quality: Some("qualified".to_string()),
                },
            },
            GovernanceEventEnvelope {
                at_unix_ms: 1_000_003,
                source: "test".to_string(),
                event: GovernanceEvent::Phase4Blocked {
                    reason: "phase4_nogo_enforced".to_string(),
                    context: "register_lock".to_string(),
                    block_kind: Some("rule".to_string()),
                    demand_quality: Some("qualified".to_string()),
                },
            },
        ];

        let thresholds = GovernanceThresholds {
            gate_rejected_no_trigger_warn_ratio: 0.5,
            privacy_rejected_signal_ratio: 0.2,
            phase4_blocked_threshold: 1,
            ..GovernanceThresholds::default()
        };
        let out = build_governance_assessment(&events, thresholds);
        assert!(!out.warnings.is_empty());
        assert!(!out.signals.is_empty());
    }

    #[test]
    fn phase4_decision_turns_go_when_quantitative_criteria_are_met() {
        let mut events = Vec::new();
        let day_ms = 86_400_000u64;
        for day in 100..=102u64 {
            let mapping_id = format!("shadow-map-{day}");
            for index in 0..6u64 {
                events.push(GovernanceEventEnvelope {
                    at_unix_ms: day * day_ms + index,
                    source: "test".to_string(),
                    event: GovernanceEvent::Phase4Blocked {
                        reason: "phase4_nogo_enforced".to_string(),
                        context: "register_lock".to_string(),
                        block_kind: Some("rule".to_string()),
                        demand_quality: Some("qualified".to_string()),
                    },
                });
            }
            for index in 0..12u64 {
                events.push(GovernanceEventEnvelope {
                    at_unix_ms: day * day_ms + 100 + index,
                    source: "test".to_string(),
                    event: GovernanceEvent::ExternalInflowDemandObserved {
                        channel: "mapped_lock_register".to_string(),
                        qualified: true,
                        accepted: false,
                        account_id: Some("uca-a".to_string()),
                        source_chain: Some("ethereum".to_string()),
                        amount: Some(1),
                        reason: Some("phase4_nogo_enforced".to_string()),
                    },
                });
                events.push(GovernanceEventEnvelope {
                    at_unix_ms: day * day_ms + 100 + index,
                    source: "test".to_string(),
                    event: GovernanceEvent::MappedAssetOperationObserved {
                        operation: "shadow_register_lock".to_string(),
                        accepted: false,
                        account_id: Some("uca-a".to_string()),
                        mapping_id: None,
                        reason: Some("phase4_nogo_enforced".to_string()),
                        demand_quality: Some("qualified".to_string()),
                    },
                });
            }
            events.push(GovernanceEventEnvelope {
                at_unix_ms: day * day_ms + 600,
                source: "test".to_string(),
                event: GovernanceEvent::MappedAssetOperationObserved {
                    operation: "shadow_register_lock".to_string(),
                    accepted: true,
                    account_id: Some("uca-a".to_string()),
                    mapping_id: Some(mapping_id.clone()),
                    reason: None,
                    demand_quality: Some("qualified".to_string()),
                },
            });
            events.push(GovernanceEventEnvelope {
                at_unix_ms: day * day_ms + 601,
                source: "test".to_string(),
                event: GovernanceEvent::MappedAssetOperationObserved {
                    operation: "shadow_burn_mapped".to_string(),
                    accepted: true,
                    account_id: Some("uca-a".to_string()),
                    mapping_id: Some(mapping_id.clone()),
                    reason: None,
                    demand_quality: Some("qualified".to_string()),
                },
            });
            events.push(GovernanceEventEnvelope {
                at_unix_ms: day * day_ms + 602,
                source: "test".to_string(),
                event: GovernanceEvent::MappedAssetOperationObserved {
                    operation: "shadow_release_source".to_string(),
                    accepted: true,
                    account_id: Some("uca-a".to_string()),
                    mapping_id: Some(mapping_id),
                    reason: None,
                    demand_quality: Some("qualified".to_string()),
                },
            });
            for index in 0..20u64 {
                events.push(GovernanceEventEnvelope {
                    at_unix_ms: day * day_ms + 200 + index,
                    source: "test".to_string(),
                    event: GovernanceEvent::RuntimePolicyEvaluated {
                        policy: "PrivacyRequired".to_string(),
                        required: true,
                        accepted: index % 2 == 0,
                        reason: if index % 2 == 0 {
                            None
                        } else {
                            Some("ERR_PRIVACY_REQUIRED_BUT_PATH_NOT_AVAILABLE".to_string())
                        },
                        qualified_demand: Some(true),
                        account_id: Some("uca-a".to_string()),
                        demand_source: Some("test".to_string()),
                    },
                });
            }
        }

        let mut thresholds = GovernanceThresholds::default();
        thresholds.phase4_decision.window_cycles = 3;
        thresholds.phase4_decision.blocked_per_cycle_threshold = 5;
        thresholds
            .phase4_decision
            .blocked_consecutive_cycles_required = 3;
        thresholds
            .phase4_decision
            .external_inflow_per_cycle_threshold = 10;
        thresholds
            .phase4_decision
            .inflow_consecutive_cycles_required = 3;
        thresholds.phase4_decision.privacy_rejected_rate_threshold = 0.4;
        thresholds.phase4_decision.privacy_min_required_requests = 50;
        thresholds.phase4_decision.shadow_closed_loops_required = 1;
        thresholds.phase4_decision.shadow_min_register_samples = 3;
        thresholds.phase4_decision.shadow_closed_loop_rate_threshold = 0.9;

        let out = build_governance_assessment(&events, thresholds);
        assert_eq!(out.phase4_decision.decision, Phase4Decision::Go);
        assert!(out.phase4_decision.criteria_shadow_closed_loop);
        assert!(out.phase4_decision.criteria_shadow_sample_size);
        assert!(out.phase4_decision.criteria_shadow_closed_loop_rate);
        assert_eq!(out.phase4_decision.shadow_bottleneck_stage, "none");
    }

    #[test]
    fn phase4_decision_reports_shadow_bottleneck_under_thin_conversion() {
        let day_ms = 86_400_000u64;
        let day = 200u64;
        let mut events = Vec::new();
        for index in 0..12u64 {
            events.push(GovernanceEventEnvelope {
                at_unix_ms: day * day_ms + index,
                source: "test".to_string(),
                event: GovernanceEvent::MappedAssetOperationObserved {
                    operation: "shadow_register_lock".to_string(),
                    accepted: true,
                    account_id: Some("uca-b".to_string()),
                    mapping_id: Some(format!("map-{index}")),
                    reason: None,
                    demand_quality: Some("qualified".to_string()),
                },
            });
        }
        for index in 0..3u64 {
            events.push(GovernanceEventEnvelope {
                at_unix_ms: day * day_ms + 100 + index,
                source: "test".to_string(),
                event: GovernanceEvent::MappedAssetOperationObserved {
                    operation: "shadow_burn_mapped".to_string(),
                    accepted: true,
                    account_id: Some("uca-b".to_string()),
                    mapping_id: Some(format!("map-{index}")),
                    reason: None,
                    demand_quality: Some("qualified".to_string()),
                },
            });
        }
        for index in 0..2u64 {
            events.push(GovernanceEventEnvelope {
                at_unix_ms: day * day_ms + 200 + index,
                source: "test".to_string(),
                event: GovernanceEvent::MappedAssetOperationObserved {
                    operation: "shadow_release_source".to_string(),
                    accepted: true,
                    account_id: Some("uca-b".to_string()),
                    mapping_id: Some(format!("map-{index}")),
                    reason: None,
                    demand_quality: Some("qualified".to_string()),
                },
            });
        }
        let mut thresholds = GovernanceThresholds::default();
        thresholds.phase4_decision.window_cycles = 1;
        thresholds.phase4_decision.shadow_min_register_samples = 10;
        thresholds.phase4_decision.shadow_closed_loops_required = 2;
        thresholds.phase4_decision.shadow_closed_loop_rate_threshold = 0.5;
        let out = build_governance_assessment(&events, thresholds);
        assert_eq!(
            out.phase4_decision.shadow_bottleneck_stage,
            "register_to_burn"
        );
        assert!(!out.phase4_decision.criteria_shadow_closed_loop_rate);
        assert_eq!(out.phase4_decision.decision, Phase4Decision::NoGo);
    }
}
