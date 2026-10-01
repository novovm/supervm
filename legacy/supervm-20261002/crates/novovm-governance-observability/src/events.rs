#![forbid(unsafe_code)]

use serde::{Deserialize, Serialize};
use std::time::{SystemTime, UNIX_EPOCH};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "event", rename_all = "snake_case")]
pub enum GovernanceEvent {
    PrGateEvaluated {
        pr_id: String,
        has_trigger: bool,
        trigger_valid: Option<bool>,
        accepted: bool,
        reason: Option<String>,
        trigger_type: Option<String>,
        governance_control_change: bool,
        has_governance_proof: bool,
    },
    TriggerEvaluated {
        trigger_type: String,
        satisfied: bool,
        evidence_summary: String,
        evidence_score: Option<f64>,
    },
    RuntimePolicyEvaluated {
        policy: String,
        required: bool,
        accepted: bool,
        reason: Option<String>,
        #[serde(default)]
        qualified_demand: Option<bool>,
        #[serde(default)]
        account_id: Option<String>,
        #[serde(default)]
        demand_source: Option<String>,
    },
    RuntimeConstraintHit {
        policy: String,
        reason: String,
        #[serde(default)]
        qualified_demand: Option<bool>,
        #[serde(default)]
        account_id: Option<String>,
    },
    ExternalInflowDemandObserved {
        channel: String,
        qualified: bool,
        accepted: bool,
        #[serde(default)]
        account_id: Option<String>,
        #[serde(default)]
        source_chain: Option<String>,
        #[serde(default)]
        amount: Option<u128>,
        #[serde(default)]
        reason: Option<String>,
    },
    Phase4Blocked {
        reason: String,
        context: String,
        #[serde(default)]
        block_kind: Option<String>,
        #[serde(default)]
        demand_quality: Option<String>,
    },
    MappedAssetOperationObserved {
        operation: String,
        accepted: bool,
        account_id: Option<String>,
        mapping_id: Option<String>,
        reason: Option<String>,
        #[serde(default)]
        demand_quality: Option<String>,
    },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GovernanceEventEnvelope {
    pub at_unix_ms: u64,
    pub source: String,
    #[serde(flatten)]
    pub event: GovernanceEvent,
}

impl GovernanceEventEnvelope {
    #[must_use]
    pub fn new(source: impl Into<String>, event: GovernanceEvent) -> Self {
        Self {
            at_unix_ms: now_unix_ms(),
            source: source.into(),
            event,
        }
    }
}

#[must_use]
pub fn now_unix_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|value| value.as_millis() as u64)
        .unwrap_or(0)
}
