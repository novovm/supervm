#![forbid(unsafe_code)]

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use crate::events::{GovernanceEvent, GovernanceEventEnvelope};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TriggerMetrics {
    pub trigger_type: String,
    pub evaluated: u64,
    pub satisfied: u64,
    pub rejected: u64,
    pub avg_evidence_score: f64,
}

impl TriggerMetrics {
    #[must_use]
    pub fn satisfied_ratio(&self) -> f64 {
        if self.evaluated == 0 {
            return 0.0;
        }
        self.satisfied as f64 / self.evaluated as f64
    }
}

#[derive(Debug, Clone, Default)]
struct TriggerAccumulator {
    evaluated: u64,
    satisfied: u64,
    rejected: u64,
    evidence_score_sum: f64,
    evidence_score_count: u64,
}

#[must_use]
pub fn collect_trigger_metrics(events: &[GovernanceEventEnvelope]) -> Vec<TriggerMetrics> {
    let mut acc = BTreeMap::<String, TriggerAccumulator>::new();
    for envelope in events {
        match &envelope.event {
            GovernanceEvent::TriggerEvaluated {
                trigger_type,
                satisfied,
                evidence_score,
                ..
            } => {
                let entry = acc.entry(trigger_type.clone()).or_default();
                entry.evaluated = entry.evaluated.saturating_add(1);
                if *satisfied {
                    entry.satisfied = entry.satisfied.saturating_add(1);
                } else {
                    entry.rejected = entry.rejected.saturating_add(1);
                }
                if let Some(score) = evidence_score {
                    entry.evidence_score_sum += score;
                    entry.evidence_score_count = entry.evidence_score_count.saturating_add(1);
                }
            }
            GovernanceEvent::PrGateEvaluated {
                trigger_type,
                has_trigger,
                accepted,
                ..
            } => {
                if !has_trigger {
                    continue;
                }
                let Some(trigger_type) = trigger_type else {
                    continue;
                };
                let entry = acc.entry(trigger_type.clone()).or_default();
                entry.evaluated = entry.evaluated.saturating_add(1);
                if *accepted {
                    entry.satisfied = entry.satisfied.saturating_add(1);
                } else {
                    entry.rejected = entry.rejected.saturating_add(1);
                }
            }
            _ => {}
        }
    }

    acc.into_iter()
        .map(|(trigger_type, item)| TriggerMetrics {
            trigger_type,
            evaluated: item.evaluated,
            satisfied: item.satisfied,
            rejected: item.rejected,
            avg_evidence_score: if item.evidence_score_count == 0 {
                0.0
            } else {
                item.evidence_score_sum / item.evidence_score_count as f64
            },
        })
        .collect()
}
