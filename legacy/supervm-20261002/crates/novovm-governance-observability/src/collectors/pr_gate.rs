#![forbid(unsafe_code)]

use serde::{Deserialize, Serialize};

use crate::events::{GovernanceEvent, GovernanceEventEnvelope};

#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize)]
pub struct GateMetrics {
    pub total_prs: u64,
    pub rejected_no_trigger: u64,
    pub rejected_invalid_trigger: u64,
    pub passed_with_trigger: u64,
    pub governance_required_changes: u64,
    pub governance_missing_proof: u64,
}

impl GateMetrics {
    #[must_use]
    pub fn rejected_no_trigger_ratio(self) -> f64 {
        ratio(self.rejected_no_trigger, self.total_prs)
    }
}

#[must_use]
pub fn collect_gate_metrics(events: &[GovernanceEventEnvelope]) -> GateMetrics {
    let mut metrics = GateMetrics::default();
    for envelope in events {
        let GovernanceEvent::PrGateEvaluated {
            has_trigger,
            trigger_valid,
            accepted,
            reason,
            governance_control_change,
            has_governance_proof,
            ..
        } = &envelope.event
        else {
            continue;
        };

        metrics.total_prs = metrics.total_prs.saturating_add(1);

        if *governance_control_change {
            metrics.governance_required_changes =
                metrics.governance_required_changes.saturating_add(1);
            if !has_governance_proof {
                metrics.governance_missing_proof =
                    metrics.governance_missing_proof.saturating_add(1);
            }
        }

        if *accepted && *has_trigger {
            metrics.passed_with_trigger = metrics.passed_with_trigger.saturating_add(1);
        }

        if !accepted {
            if !has_trigger {
                metrics.rejected_no_trigger = metrics.rejected_no_trigger.saturating_add(1);
                continue;
            }

            let invalid_trigger = match trigger_valid {
                Some(valid) => !valid,
                None => reason
                    .as_deref()
                    .map(|raw| {
                        let normalized = raw.to_ascii_lowercase();
                        normalized.contains("invalid_trigger")
                            || normalized.contains("trigger_not_met")
                            || normalized.contains("trigger_invalid")
                    })
                    .unwrap_or(false),
            };
            if invalid_trigger {
                metrics.rejected_invalid_trigger =
                    metrics.rejected_invalid_trigger.saturating_add(1);
            }
        }
    }
    metrics
}

fn ratio(numerator: u64, denominator: u64) -> f64 {
    if denominator == 0 {
        return 0.0;
    }
    numerator as f64 / denominator as f64
}
