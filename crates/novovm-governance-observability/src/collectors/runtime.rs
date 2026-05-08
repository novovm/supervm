#![forbid(unsafe_code)]

use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

use crate::events::{GovernanceEvent, GovernanceEventEnvelope};

#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize)]
pub struct RuntimeSignals {
    pub pq_required_requests: u64,
    pub pq_rejected: u64,
    pub privacy_required_requests: u64,
    pub privacy_rejected: u64,
    pub mapped_asset_register_attempts: u64,
    pub mapped_asset_blocked_by_nogo: u64,
    pub mapped_asset_blocked_by_rule: u64,
    pub mapped_asset_blocked_by_capacity: u64,
    pub external_inflow_demand_raw: u64,
    pub external_inflow_demand_qualified: u64,
    pub shadow_register_verified: u64,
    pub shadow_burn_completed: u64,
    pub shadow_release_completed: u64,
    pub shadow_closed_loops: u64,
    pub execution_policy_errors: u64,
}

impl RuntimeSignals {
    #[must_use]
    pub fn pq_rejected_ratio(self) -> f64 {
        ratio(self.pq_rejected, self.pq_required_requests)
    }

    #[must_use]
    pub fn privacy_rejected_ratio(self) -> f64 {
        ratio(self.privacy_rejected, self.privacy_required_requests)
    }
}

#[must_use]
pub fn collect_runtime_signals(events: &[GovernanceEventEnvelope]) -> RuntimeSignals {
    let mut out = RuntimeSignals::default();
    let mut has_pq_policy_eval = false;
    let mut has_privacy_policy_eval = false;
    let mut saw_external_inflow_events = false;
    let mut shadow_flow_by_mapping = BTreeMap::<String, u8>::new();

    for envelope in events {
        match &envelope.event {
            GovernanceEvent::RuntimePolicyEvaluated {
                policy,
                required,
                accepted,
                qualified_demand,
                ..
            } => {
                let qualified = qualified_demand.unwrap_or(true);
                if !required || !qualified {
                    continue;
                }
                if policy_matches(policy, "pqrequired") {
                    has_pq_policy_eval = true;
                    out.pq_required_requests = out.pq_required_requests.saturating_add(1);
                    if !accepted {
                        out.pq_rejected = out.pq_rejected.saturating_add(1);
                        out.execution_policy_errors = out.execution_policy_errors.saturating_add(1);
                    }
                }
                if policy_matches(policy, "privacyrequired") {
                    has_privacy_policy_eval = true;
                    out.privacy_required_requests = out.privacy_required_requests.saturating_add(1);
                    if !accepted {
                        out.privacy_rejected = out.privacy_rejected.saturating_add(1);
                        out.execution_policy_errors = out.execution_policy_errors.saturating_add(1);
                    }
                }
            }
            GovernanceEvent::ExternalInflowDemandObserved {
                channel, qualified, ..
            } => {
                if !channel_matches_mapped_lock_register(channel) {
                    continue;
                }
                saw_external_inflow_events = true;
                out.external_inflow_demand_raw = out.external_inflow_demand_raw.saturating_add(1);
                if *qualified {
                    out.external_inflow_demand_qualified =
                        out.external_inflow_demand_qualified.saturating_add(1);
                }
            }
            GovernanceEvent::MappedAssetOperationObserved {
                operation,
                accepted,
                demand_quality,
                mapping_id,
                ..
            } => {
                if !operation_matches_register_lock(operation) {
                    if operation_matches_shadow_burn(operation) && *accepted {
                        out.shadow_burn_completed = out.shadow_burn_completed.saturating_add(1);
                        if let Some(mapping_id) = mapping_id {
                            let state = shadow_flow_by_mapping
                                .entry(mapping_id.clone())
                                .or_default();
                            *state |= 0b010;
                        }
                    }
                    if operation_matches_shadow_release(operation) && *accepted {
                        out.shadow_release_completed =
                            out.shadow_release_completed.saturating_add(1);
                        if let Some(mapping_id) = mapping_id {
                            let state = shadow_flow_by_mapping
                                .entry(mapping_id.clone())
                                .or_default();
                            *state |= 0b100;
                        }
                    }
                    continue;
                }
                out.mapped_asset_register_attempts =
                    out.mapped_asset_register_attempts.saturating_add(1);
                if operation_matches_shadow_register_lock(operation) && *accepted {
                    out.shadow_register_verified = out.shadow_register_verified.saturating_add(1);
                    if let Some(mapping_id) = mapping_id {
                        let state = shadow_flow_by_mapping
                            .entry(mapping_id.clone())
                            .or_default();
                        *state |= 0b001;
                    }
                }
                if !saw_external_inflow_events {
                    out.external_inflow_demand_raw =
                        out.external_inflow_demand_raw.saturating_add(1);
                    let qualified = demand_quality
                        .as_deref()
                        .map(|value| normalize_token(value) == "qualified")
                        .unwrap_or(*accepted);
                    if qualified {
                        out.external_inflow_demand_qualified =
                            out.external_inflow_demand_qualified.saturating_add(1);
                    }
                }
            }
            GovernanceEvent::Phase4Blocked {
                reason, block_kind, ..
            } => {
                out.mapped_asset_blocked_by_nogo =
                    out.mapped_asset_blocked_by_nogo.saturating_add(1);
                let normalized_kind = block_kind
                    .as_deref()
                    .map(normalize_token)
                    .or_else(|| infer_block_kind_from_reason(reason.as_str()));
                match normalized_kind.as_deref() {
                    Some("capacity") => {
                        out.mapped_asset_blocked_by_capacity =
                            out.mapped_asset_blocked_by_capacity.saturating_add(1);
                    }
                    _ => {
                        out.mapped_asset_blocked_by_rule =
                            out.mapped_asset_blocked_by_rule.saturating_add(1);
                    }
                }
            }
            _ => {}
        }
    }

    for envelope in events {
        let GovernanceEvent::RuntimeConstraintHit {
            policy,
            qualified_demand,
            ..
        } = &envelope.event
        else {
            continue;
        };
        if !qualified_demand.unwrap_or(true) {
            continue;
        }

        if policy_matches(policy, "pqrequired") && !has_pq_policy_eval {
            out.pq_required_requests = out.pq_required_requests.saturating_add(1);
            out.pq_rejected = out.pq_rejected.saturating_add(1);
            out.execution_policy_errors = out.execution_policy_errors.saturating_add(1);
        }
        if policy_matches(policy, "privacyrequired") && !has_privacy_policy_eval {
            out.privacy_required_requests = out.privacy_required_requests.saturating_add(1);
            out.privacy_rejected = out.privacy_rejected.saturating_add(1);
            out.execution_policy_errors = out.execution_policy_errors.saturating_add(1);
        }
    }

    for state in shadow_flow_by_mapping.values() {
        if *state == 0b111 {
            out.shadow_closed_loops = out.shadow_closed_loops.saturating_add(1);
        }
    }

    out
}

fn policy_matches(raw: &str, target: &str) -> bool {
    normalize_token(raw) == normalize_token(target)
}

fn operation_matches_register_lock(raw: &str) -> bool {
    let normalized = normalize_token(raw);
    normalized == "registerlock" || normalized == "shadowregisterlock"
}

fn operation_matches_shadow_register_lock(raw: &str) -> bool {
    normalize_token(raw) == "shadowregisterlock"
}

fn operation_matches_shadow_burn(raw: &str) -> bool {
    normalize_token(raw) == "shadowburnmapped"
}

fn operation_matches_shadow_release(raw: &str) -> bool {
    normalize_token(raw) == "shadowreleasesource"
}

fn channel_matches_mapped_lock_register(raw: &str) -> bool {
    let normalized = normalize_token(raw);
    normalized == "mappedlockregister" || normalized == "mappedlockregistershadow"
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

fn normalize_token(raw: &str) -> String {
    raw.chars()
        .filter(|ch| ch.is_ascii_alphanumeric())
        .flat_map(char::to_lowercase)
        .collect()
}

fn ratio(numerator: u64, denominator: u64) -> f64 {
    if denominator == 0 {
        return 0.0;
    }
    numerator as f64 / denominator as f64
}
