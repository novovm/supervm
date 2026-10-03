use serde::{Deserialize, Serialize};
use std::collections::HashMap;

pub mod policy_matrix_build;
pub mod region_evaluate;
pub mod seed_evaluate;

#[derive(Default, Debug, Clone, Serialize, Deserialize)]
pub struct SeedFailoverState {
    #[serde(default = "one")]
    pub version: u64,
    #[serde(default)]
    pub updated_at: String,
    #[serde(default)]
    pub sources: HashMap<String, SeedRec>,
}

#[derive(Default, Debug, Clone, Serialize, Deserialize)]
pub struct SeedRec {
    #[serde(default)]
    pub success: u64,
    #[serde(default)]
    pub fail: u64,
    #[serde(default)]
    pub consecutive_failures: u64,
    #[serde(default)]
    pub degraded_until_unix_ms: u64,
    #[serde(default)]
    pub degraded_reason: String,
    #[serde(default)]
    pub last_selected_unix_ms: u64,
    #[serde(default)]
    pub last_recovered_unix_ms: u64,
}

#[derive(Default, Debug, Clone, Serialize, Deserialize)]
pub struct RegionFailoverState {
    #[serde(default = "one")]
    pub version: u64,
    #[serde(default)]
    pub updated_at: String,
    #[serde(default)]
    pub regions: HashMap<String, RegionRec>,
}

#[derive(Default, Debug, Clone, Serialize, Deserialize)]
pub struct RegionRec {
    #[serde(default)]
    pub success: u64,
    #[serde(default)]
    pub fail: u64,
    #[serde(default)]
    pub consecutive_failures: u64,
    #[serde(default)]
    pub degraded_until_unix_ms: u64,
    #[serde(default)]
    pub degraded_reason: String,
    #[serde(default)]
    pub last_recovered_unix_ms: u64,
    #[serde(default)]
    pub last_evaluated_unix_ms: u64,
}

#[derive(Debug, Clone)]
pub struct SeedGateOutcome {
    pub available: bool,
    pub recover_at_unix_ms: u64,
    pub reason: String,
}

#[derive(Debug, Clone)]
pub struct SeedFailureOutcome {
    pub degraded: bool,
    pub reason: String,
    pub recover_at_unix_ms: u64,
    pub success_rate: f64,
}

#[derive(Debug, Clone)]
pub struct RegionGateOutcome {
    pub available: bool,
    pub recover_at_unix_ms: u64,
    pub reason: String,
}

#[derive(Debug, Clone)]
pub struct RegionEvaluationOutcome {
    pub degraded: bool,
    pub reason: String,
    pub recover_at_unix_ms: u64,
    pub average_score: f64,
}

fn one() -> u64 {
    1
}

pub fn refresh_seed_gate(now_unix_ms: u64, rec: &mut SeedRec) -> SeedGateOutcome {
    if rec.degraded_until_unix_ms > 0 && rec.degraded_until_unix_ms <= now_unix_ms {
        rec.degraded_until_unix_ms = 0;
        rec.degraded_reason.clear();
        rec.last_recovered_unix_ms = now_unix_ms;
    }
    let available = rec.degraded_until_unix_ms <= now_unix_ms;
    SeedGateOutcome {
        available,
        recover_at_unix_ms: if available {
            0
        } else {
            rec.degraded_until_unix_ms
        },
        reason: if available {
            String::new()
        } else if rec.degraded_reason.trim().is_empty() {
            "cooldown_active".to_string()
        } else {
            rec.degraded_reason.clone()
        },
    }
}

pub fn record_seed_success(now_unix_ms: u64, rec: &mut SeedRec) {
    rec.success = rec.success.saturating_add(1);
    rec.consecutive_failures = 0;
    rec.degraded_until_unix_ms = 0;
    rec.degraded_reason.clear();
    rec.last_selected_unix_ms = now_unix_ms;
}

pub fn record_seed_failure(
    now_unix_ms: u64,
    rec: &mut SeedRec,
    success_rate_threshold: f64,
    cooldown_seconds: u64,
    max_consecutive_failures: u64,
) -> SeedFailureOutcome {
    rec.fail = rec.fail.saturating_add(1);
    rec.consecutive_failures = rec.consecutive_failures.saturating_add(1);
    rec.last_selected_unix_ms = now_unix_ms;
    let total = rec.success.saturating_add(rec.fail);
    let success_rate = if total == 0 {
        1.0
    } else {
        rec.success as f64 / total as f64
    };
    let mut reason = String::new();
    if success_rate < success_rate_threshold {
        reason = "success_rate_below_threshold".to_string();
    }
    if rec.consecutive_failures >= max_consecutive_failures {
        reason = "consecutive_failures_exceeded".to_string();
    }
    if !reason.is_empty() {
        rec.degraded_until_unix_ms = now_unix_ms.saturating_add(cooldown_seconds * 1000);
        rec.degraded_reason = reason.clone();
    }
    SeedFailureOutcome {
        degraded: !reason.is_empty(),
        reason,
        recover_at_unix_ms: rec.degraded_until_unix_ms,
        success_rate,
    }
}

pub fn refresh_region_gate(now_unix_ms: u64, rec: &mut RegionRec) -> RegionGateOutcome {
    if rec.degraded_until_unix_ms > 0 && rec.degraded_until_unix_ms <= now_unix_ms {
        rec.degraded_until_unix_ms = 0;
        rec.degraded_reason.clear();
        rec.last_recovered_unix_ms = now_unix_ms;
    }
    let available = rec.degraded_until_unix_ms <= now_unix_ms;
    RegionGateOutcome {
        available,
        recover_at_unix_ms: if available {
            0
        } else {
            rec.degraded_until_unix_ms
        },
        reason: if available {
            String::new()
        } else if rec.degraded_reason.trim().is_empty() {
            "cooldown_active".to_string()
        } else {
            rec.degraded_reason.clone()
        },
    }
}

pub fn evaluate_region_score(
    now_unix_ms: u64,
    rec: &mut RegionRec,
    average_score: f64,
    failover_threshold: f64,
    cooldown_seconds: u64,
) -> RegionEvaluationOutcome {
    rec.last_evaluated_unix_ms = now_unix_ms;
    let mut reason = String::new();
    if average_score < failover_threshold {
        rec.fail = rec.fail.saturating_add(1);
        rec.consecutive_failures = rec.consecutive_failures.saturating_add(1);
        rec.degraded_until_unix_ms = now_unix_ms.saturating_add(cooldown_seconds * 1000);
        rec.degraded_reason = "region_score_below_threshold".to_string();
        reason = rec.degraded_reason.clone();
    } else {
        rec.success = rec.success.saturating_add(1);
        rec.consecutive_failures = 0;
        rec.degraded_until_unix_ms = 0;
        rec.degraded_reason.clear();
    }
    RegionEvaluationOutcome {
        degraded: !reason.is_empty(),
        reason,
        recover_at_unix_ms: rec.degraded_until_unix_ms,
        average_score,
    }
}
