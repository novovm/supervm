use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RolloutControlPlan {
    pub policy_bin: String,
    pub queue_file: String,
    pub plan_action: String,
    pub controller_id: Option<String>,
    pub operation_id: Option<String>,
    pub audit_file: Option<String>,
    pub dry_run: bool,
    pub continue_on_plan_failure: bool,
    pub resume_from_snapshot: bool,
    pub replay_conflicts_on_start: bool,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct RolloutQueueConfig {
    #[serde(default)]
    pub max_concurrent_plans: u32,
    #[serde(default)]
    pub dispatch_pause_seconds: u64,
    #[serde(default)]
    pub risk_policy: RiskPolicyConfig,
    #[serde(default)]
    pub state_recovery: StateRecoveryConfig,
    #[serde(default)]
    pub plans: Vec<RolloutQueuePlan>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct RolloutQueuePlan {
    #[serde(default)]
    pub name: String,
    #[serde(default)]
    pub enabled: bool,
    #[serde(default)]
    pub action: String,
    #[serde(default)]
    pub controller_id: String,
    #[serde(default)]
    pub operation_id: String,
    #[serde(default)]
    pub audit_file: String,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct RiskPolicyConfig {
    #[serde(default)]
    pub active_profile: String,
    #[serde(default)]
    pub policy_profiles: Map<String, Value>,
    #[serde(default)]
    pub alert_channel_targets: Map<String, Value>,
    #[serde(default)]
    pub alert_target_delivery_types: Map<String, Value>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct StateRecoveryConfig {
    #[serde(default)]
    pub replica_health_file: String,
    #[serde(default)]
    pub slo: SloConfig,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct SloConfig {
    #[serde(default)]
    pub enabled: bool,
    #[serde(default)]
    pub file: String,
    #[serde(default = "default_slo_window_samples")]
    pub window_samples: usize,
    #[serde(default = "default_slo_min_green_rate")]
    pub min_green_rate: f64,
    #[serde(default)]
    pub max_red_in_window: usize,
    #[serde(default)]
    pub block_on_violation: bool,
    #[serde(default)]
    pub circuit_breaker: CircuitBreakerConfig,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct CircuitBreakerConfig {
    #[serde(default)]
    pub enabled: bool,
    #[serde(default)]
    pub yellow_max_concurrent_plans: u32,
    #[serde(default)]
    pub yellow_dispatch_pause_seconds: u64,
    #[serde(default)]
    pub red_block: bool,
    #[serde(default)]
    pub matrix: Vec<Value>,
}

fn default_slo_window_samples() -> usize {
    60
}

fn default_slo_min_green_rate() -> f64 {
    0.95
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ControllerDispatchEvaluateResult {
    pub continue_dispatch: bool,
    pub block_dispatch: bool,
    pub reason: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SloEvaluateResult {
    pub green_rate: Option<f64>,
    pub score: Option<f64>,
    pub violation: bool,
    pub reason: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CircuitBreakerEvaluateResult {
    pub max_concurrent_plans: Option<u32>,
    pub dispatch_pause_seconds: Option<u64>,
    pub block_dispatch: bool,
    pub reason: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PolicyProfileSelectResult {
    pub selected_profile: Option<String>,
    pub reason: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RolloutControlDecision {
    pub continue_dispatch: bool,
    pub max_concurrent_plans: Option<u32>,
    pub dispatch_pause_seconds: Option<u64>,
    pub block_dispatch: bool,
    pub selected_policy_profile: Option<String>,
    pub reason: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RolloutControlAuditRecord {
    pub plan: RolloutControlPlan,
    pub rollout_eval: ControllerDispatchEvaluateResult,
    pub slo_eval: SloEvaluateResult,
    pub circuit_eval: CircuitBreakerEvaluateResult,
    pub profile_eval: PolicyProfileSelectResult,
    pub decision: RolloutControlDecision,
    pub applied: bool,
}
