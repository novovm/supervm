#![forbid(unsafe_code)]

use anyhow::{bail, Context, Result};
use chrono::Utc;
use novovm_network::{
    default_eth_fullnode_native_worker_runtime_snapshot_path_v1,
    load_eth_fullnode_native_worker_runtime_snapshot_from_path_v1,
    EthFullnodeNativeWorkerRuntimeSnapshotV1, ETH_FULLNODE_NATIVE_WORKER_RUNTIME_SCHEMA_V1,
};
use serde::Serialize;
use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::thread;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

pub const MAINLINE_SOAK_REPORT_SCHEMA_V1: &str = "supervm-mainline-soak-report/v2";
pub const MAINLINE_NIGHTLY_SOAK_GATE_REPORT_SCHEMA_V1: &str =
    "supervm-mainline-nightly-soak-gate-report/v2";

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum MainlineSoakModeV2 {
    #[default]
    Workload,
    IdleHealth,
}

#[derive(Debug, Clone, Serialize)]
pub struct MainlineSoakEvidencePolicyV2 {
    pub mode: MainlineSoakModeV2,
    pub max_snapshot_age_ms: u64,
    pub min_valid_samples: u64,
    pub min_valid_sample_ratio_bps: u64,
}

impl Default for MainlineSoakEvidencePolicyV2 {
    fn default() -> Self {
        Self {
            mode: MainlineSoakModeV2::Workload,
            max_snapshot_age_ms: 180_000,
            min_valid_samples: 2,
            min_valid_sample_ratio_bps: 9_000,
        }
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct MainlineSoakThresholdsV1 {
    pub max_throttle_hits_per_hour: Option<f64>,
    pub max_throttle_hit_rate_bps_estimated: Option<u64>,
    pub min_body_updates_per_hour: Option<f64>,
    pub max_pending_queue_depth_peak: Option<u64>,
    pub min_pending_queue_recovery_per_hour: Option<f64>,
    pub max_target_oscillation_bps: Option<u64>,
    pub max_time_slice_target_utilization_peak_bps: Option<u64>,
    pub max_top_execution_target_reason_share_bps: Option<u64>,
}

impl Default for MainlineSoakThresholdsV1 {
    fn default() -> Self {
        Self {
            max_throttle_hits_per_hour: None,
            max_throttle_hit_rate_bps_estimated: Some(9_500),
            min_body_updates_per_hour: None,
            max_pending_queue_depth_peak: None,
            min_pending_queue_recovery_per_hour: None,
            max_target_oscillation_bps: Some(9_500),
            max_time_slice_target_utilization_peak_bps: Some(10_000),
            max_top_execution_target_reason_share_bps: None,
        }
    }
}

#[derive(Debug, Clone)]
pub struct MainlineSoakConfigV1 {
    pub profile: String,
    pub chain_id: u64,
    pub duration_seconds: u64,
    pub sample_interval_seconds: u64,
    pub snapshot_path: PathBuf,
    pub report_path: PathBuf,
    pub thresholds: MainlineSoakThresholdsV1,
    pub evidence_policy: MainlineSoakEvidencePolicyV2,
}

#[derive(Debug, Clone, Default, Serialize)]
pub struct MainlineSoakSamplingStatsV1 {
    pub read_attempt_count: u64,
    pub read_success_count: u64,
    pub read_error_count: u64,
    pub wrong_chain_snapshot_count: u64,
    pub wrong_schema_snapshot_count: u64,
    pub valid_sample_count: u64,
    pub duplicate_snapshot_count: u64,
    pub stale_snapshot_count: u64,
    pub future_snapshot_count: u64,
    pub timestamp_regression_count: u64,
    pub counter_regression_count: u64,
    pub observation_clock_regression_count: u64,
    pub pre_run_snapshot_count: u64,
    pub max_valid_sample_gap_ms: u128,
    pub last_attempt_valid: bool,
}

#[derive(Debug, Clone, Default, Serialize)]
pub struct MainlineSoakCounterDeltaV1 {
    pub execution_budget_hit_delta: u64,
    pub execution_deferred_delta: u64,
    pub execution_time_slice_exceeded_delta: u64,
    #[serde(rename = "sampled_header_updates")]
    pub header_updates_delta: u64,
    #[serde(rename = "sampled_body_updates")]
    pub body_updates_delta: u64,
    #[serde(rename = "sampled_sync_requests")]
    pub sync_requests_delta: u64,
}

#[derive(Debug, Clone, Default, Serialize)]
pub struct MainlineSoakMetricsV1 {
    pub throttle_hits_per_hour: f64,
    pub throttle_hit_rate_bps_estimated: u64,
    #[serde(rename = "sampled_header_updates_per_hour")]
    pub header_updates_per_hour: f64,
    #[serde(rename = "sampled_body_updates_per_hour")]
    pub body_updates_per_hour: f64,
    #[serde(rename = "sampled_sync_requests_per_hour")]
    pub sync_requests_per_hour: f64,
    pub pending_queue_depth_avg: f64,
    pub pending_queue_depth_peak: u64,
    pub pending_queue_depth_final: u64,
    pub pending_queue_recovery_per_hour: f64,
    pub target_oscillation_bps: u64,
    pub budget_target_utilization_avg_bps: u64,
    pub budget_target_utilization_peak_bps: u64,
    pub time_slice_target_utilization_avg_bps: u64,
    pub time_slice_target_utilization_peak_bps: u64,
    pub execution_target_reason_distribution: BTreeMap<String, u64>,
    pub top_execution_target_reason: Option<String>,
    pub top_execution_target_reason_share_bps: u64,
}

#[derive(Debug, Clone, Serialize)]
pub struct MainlineSoakViolationV1 {
    pub code: String,
    pub observed: String,
    pub threshold: String,
    pub detail: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct MainlineSoakEvaluationV1 {
    pub pass: bool,
    pub violation_count: usize,
    pub violations: Vec<MainlineSoakViolationV1>,
}

#[derive(Debug, Clone, Serialize)]
pub struct MainlineSoakReportV1 {
    pub schema: &'static str,
    pub generated_utc: String,
    pub profile: String,
    pub mode: MainlineSoakModeV2,
    pub validation_scope: &'static str,
    pub nominal_duration_seconds: u64,
    pub duration_requirement_met: bool,
    pub counter_semantics: &'static str,
    pub process_continuity_attested: bool,
    pub evidence_policy: MainlineSoakEvidencePolicyV2,
    pub chain_id: u64,
    pub snapshot_path: String,
    pub started_unix_ms: u128,
    pub ended_unix_ms: u128,
    pub requested_duration_seconds: u64,
    pub observed_elapsed_seconds: u64,
    pub observed_elapsed_ms: u128,
    pub sample_interval_seconds: u64,
    pub sample_count: usize,
    pub sampling: MainlineSoakSamplingStatsV1,
    pub counters: MainlineSoakCounterDeltaV1,
    pub metrics: MainlineSoakMetricsV1,
    pub thresholds: MainlineSoakThresholdsV1,
    pub evaluation: MainlineSoakEvaluationV1,
}

#[derive(Debug, Clone, Serialize)]
pub struct MainlineNightlySoakProfileResultV1 {
    pub profile: String,
    pub mode: MainlineSoakModeV2,
    pub validation_scope: &'static str,
    pub nominal_duration_seconds: u64,
    pub duration_requirement_met: bool,
    pub report_path: String,
    pub requested_duration_seconds: u64,
    pub observed_elapsed_seconds: u64,
    pub sample_interval_seconds: u64,
    pub sample_count: usize,
    pub pass: bool,
    pub violation_count: usize,
}

#[derive(Debug, Clone, Serialize)]
pub struct MainlineNightlySoakGateReportV1 {
    pub schema: &'static str,
    pub generated_utc: String,
    pub chain_id: u64,
    pub snapshot_path: String,
    pub run_mainline_gate: bool,
    pub profile_results: Vec<MainlineNightlySoakProfileResultV1>,
    pub overall_pass: bool,
}

#[derive(Debug, Clone)]
struct MainlineSoakSamplePointV1 {
    observed_elapsed_ms: u128,
    snapshot_updated_at_unix_ms: u64,
    execution_budget_hit_count: u64,
    execution_deferred_count: u64,
    execution_time_slice_exceeded_count: u64,
    header_updates: u64,
    body_updates: u64,
    sync_requests: u64,
    pending_depth: u64,
    hard_budget_per_tick: Option<u64>,
    target_budget_per_tick: Option<u64>,
    effective_budget_per_tick: Option<u64>,
    hard_time_slice_ms: Option<u64>,
    target_time_slice_ms: Option<u64>,
    effective_time_slice_ms: Option<u64>,
    target_reason: Option<String>,
    runtime_pending_tx_snapshot_limit: u64,
}

#[must_use]
pub fn default_mainline_soak_snapshot_path_v1() -> PathBuf {
    default_eth_fullnode_native_worker_runtime_snapshot_path_v1()
}

#[must_use]
pub fn default_mainline_soak_report_path_v1(profile: &str) -> PathBuf {
    let normalized = profile.trim().to_ascii_lowercase();
    PathBuf::from(format!(
        "artifacts/mainline/mainline-soak-{normalized}.json"
    ))
}

#[must_use]
pub fn default_mainline_soak_duration_seconds_v1(profile: &str) -> u64 {
    match profile.trim().to_ascii_lowercase().as_str() {
        "6h" => 6 * 60 * 60,
        "24h" => 24 * 60 * 60,
        _ => 60 * 60,
    }
}

#[must_use]
pub fn default_mainline_soak_thresholds_v1(profile: &str) -> MainlineSoakThresholdsV1 {
    let mut thresholds = MainlineSoakThresholdsV1::default();
    match profile.trim().to_ascii_lowercase().as_str() {
        "24h" => {
            thresholds.max_throttle_hit_rate_bps_estimated = Some(9_000);
            thresholds.max_target_oscillation_bps = Some(8_500);
        }
        "6h" => {
            thresholds.max_throttle_hit_rate_bps_estimated = Some(9_250);
            thresholds.max_target_oscillation_bps = Some(9_000);
        }
        _ => {}
    }
    thresholds
}

fn parse_env_u64(name: &str) -> Result<Option<u64>> {
    let Some(raw) = std::env::var(name).ok() else {
        return Ok(None);
    };
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return Ok(None);
    }
    let parsed = trimmed
        .parse::<u64>()
        .with_context(|| format!("invalid {name}: '{trimmed}'"))?;
    Ok(Some(parsed))
}

fn parse_env_f64(name: &str) -> Result<Option<f64>> {
    let Some(raw) = std::env::var(name).ok() else {
        return Ok(None);
    };
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return Ok(None);
    }
    let parsed = trimmed
        .parse::<f64>()
        .with_context(|| format!("invalid {name}: '{trimmed}'"))?;
    if !parsed.is_finite() || parsed < 0.0 {
        bail!("{name} must be finite and nonnegative");
    }
    Ok(Some(parsed))
}

pub fn apply_mainline_soak_threshold_env_overrides_v1(
    env_prefix: &str,
    thresholds: &mut MainlineSoakThresholdsV1,
) -> Result<()> {
    let key = |suffix: &str| format!("{env_prefix}{suffix}");
    if let Some(value) = parse_env_f64(key("MAX_THROTTLE_HITS_PER_HOUR").as_str())? {
        thresholds.max_throttle_hits_per_hour = Some(value);
    }
    if let Some(value) = parse_env_u64(key("MAX_THROTTLE_HIT_RATE_BPS").as_str())? {
        thresholds.max_throttle_hit_rate_bps_estimated = Some(value);
    }
    if let Some(value) = parse_env_f64(key("MIN_BODY_UPDATES_PER_HOUR").as_str())? {
        thresholds.min_body_updates_per_hour = Some(value);
    }
    if let Some(value) = parse_env_u64(key("MAX_PENDING_QUEUE_DEPTH_PEAK").as_str())? {
        thresholds.max_pending_queue_depth_peak = Some(value);
    }
    if let Some(value) = parse_env_f64(key("MIN_PENDING_QUEUE_RECOVERY_PER_HOUR").as_str())? {
        thresholds.min_pending_queue_recovery_per_hour = Some(value);
    }
    if let Some(value) = parse_env_u64(key("MAX_TARGET_OSCILLATION_BPS").as_str())? {
        thresholds.max_target_oscillation_bps = Some(value);
    }
    if let Some(value) = parse_env_u64(key("MAX_TIME_SLICE_UTILIZATION_PEAK_BPS").as_str())? {
        thresholds.max_time_slice_target_utilization_peak_bps = Some(value);
    }
    if let Some(value) = parse_env_u64(key("MAX_TOP_REASON_SHARE_BPS").as_str())? {
        thresholds.max_top_execution_target_reason_share_bps = Some(value);
    }
    Ok(())
}

pub fn apply_mainline_soak_evidence_env_overrides_v2(
    env_prefix: &str,
    policy: &mut MainlineSoakEvidencePolicyV2,
) -> Result<()> {
    let key = |suffix: &str| format!("{env_prefix}{suffix}");
    if let Ok(raw) = std::env::var(key("MODE")) {
        policy.mode = match raw.trim() {
            "" => policy.mode,
            "workload" => MainlineSoakModeV2::Workload,
            "idle_health" => MainlineSoakModeV2::IdleHealth,
            _ => bail!("{} must be workload or idle_health", key("MODE")),
        };
    }
    if let Some(value) = parse_env_u64(&key("MAX_SNAPSHOT_AGE_MS"))? {
        policy.max_snapshot_age_ms = value;
    }
    if let Some(value) = parse_env_u64(&key("MIN_VALID_SAMPLES"))? {
        policy.min_valid_samples = value;
    }
    if let Some(value) = parse_env_u64(&key("MIN_VALID_SAMPLE_RATIO_BPS"))? {
        policy.min_valid_sample_ratio_bps = value;
    }
    Ok(())
}

fn validate_soak_config_v2(config: &MainlineSoakConfigV1) -> Result<()> {
    if !matches!(config.profile.as_str(), "1h" | "6h" | "24h") {
        bail!("soak profile must be 1h, 6h or 24h; use duration override for short smoke");
    }
    if config.duration_seconds == 0 || config.duration_seconds > 7 * 24 * 60 * 60 {
        bail!("duration_seconds must be in 1..=604800");
    }
    if config.sample_interval_seconds == 0
        || config.sample_interval_seconds > config.duration_seconds
    {
        bail!("sample_interval_seconds must be in 1..=duration_seconds");
    }
    let max_samples = config
        .duration_seconds
        .div_ceil(config.sample_interval_seconds)
        + 1;
    if max_samples > 100_000 {
        bail!("soak sampling must not exceed 100000 observations");
    }
    let policy = &config.evidence_policy;
    if policy.max_snapshot_age_ms < config.sample_interval_seconds * 1_000 {
        bail!("max_snapshot_age_ms must cover at least one sampling interval");
    }
    if policy.min_valid_samples < 2 || policy.min_valid_samples > max_samples {
        bail!("min_valid_samples must be >= 2 and fit the requested sampling window");
    }
    if !(1..=10_000).contains(&policy.min_valid_sample_ratio_bps) {
        bail!("min_valid_sample_ratio_bps must be in 1..=10000");
    }
    let thresholds = &config.thresholds;
    for value in [
        thresholds.max_throttle_hits_per_hour,
        thresholds.min_body_updates_per_hour,
        thresholds.min_pending_queue_recovery_per_hour,
    ]
    .into_iter()
    .flatten()
    {
        if !value.is_finite() || value < 0.0 {
            bail!("soak rate thresholds must be finite and nonnegative");
        }
    }
    for value in [
        thresholds.max_throttle_hit_rate_bps_estimated,
        thresholds.max_target_oscillation_bps,
        thresholds.max_time_slice_target_utilization_peak_bps,
        thresholds.max_top_execution_target_reason_share_bps,
    ]
    .into_iter()
    .flatten()
    {
        if value > 10_000 {
            bail!("soak basis-point thresholds must be <= 10000");
        }
    }
    Ok(())
}

#[must_use]
fn now_unix_ms_v1() -> u128 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_millis())
        .unwrap_or_default()
}

fn sample_from_snapshot_v1(
    chain_id: u64,
    snapshot: EthFullnodeNativeWorkerRuntimeSnapshotV1,
    observed_elapsed_ms: u128,
) -> Option<MainlineSoakSamplePointV1> {
    if snapshot.chain_id != chain_id {
        return None;
    }
    Some(MainlineSoakSamplePointV1 {
        observed_elapsed_ms,
        snapshot_updated_at_unix_ms: snapshot.updated_at_unix_ms,
        execution_budget_hit_count: snapshot
            .native_execution_budget_runtime
            .execution_budget_hit_count,
        execution_deferred_count: snapshot
            .native_execution_budget_runtime
            .execution_deferred_count,
        execution_time_slice_exceeded_count: snapshot
            .native_execution_budget_runtime
            .execution_time_slice_exceeded_count,
        header_updates: snapshot.header_updates,
        body_updates: snapshot.body_updates,
        sync_requests: snapshot.sync_requests,
        pending_depth: snapshot.native_pending_tx_summary.pending_count as u64,
        hard_budget_per_tick: snapshot
            .native_execution_budget_runtime
            .hard_budget_per_tick,
        target_budget_per_tick: snapshot
            .native_execution_budget_runtime
            .target_budget_per_tick,
        effective_budget_per_tick: snapshot
            .native_execution_budget_runtime
            .effective_budget_per_tick,
        hard_time_slice_ms: snapshot.native_execution_budget_runtime.hard_time_slice_ms,
        target_time_slice_ms: snapshot
            .native_execution_budget_runtime
            .target_time_slice_ms,
        effective_time_slice_ms: snapshot
            .native_execution_budget_runtime
            .effective_time_slice_ms,
        target_reason: snapshot
            .native_execution_budget_runtime
            .last_execution_target_reason
            .map(|value| value.trim().to_string())
            .filter(|value| !value.is_empty()),
        runtime_pending_tx_snapshot_limit: snapshot
            .runtime_config
            .budget_hooks
            .runtime_pending_tx_snapshot_limit,
    })
}

fn saturating_delta_u64_v1(start: u64, end: u64) -> u64 {
    end.saturating_sub(start)
}

fn per_hour_v1(delta: u64, elapsed_ms: u128) -> f64 {
    if elapsed_ms == 0 {
        return 0.0;
    }
    (delta as f64) * 3_600_000_f64 / (elapsed_ms as f64)
}

fn ratio_bps_v1(numerator: u64, denominator: u64) -> u64 {
    if denominator == 0 {
        return 0;
    }
    ((numerator as u128) * 10_000u128 / (denominator as u128)) as u64
}

fn compute_mainline_soak_metrics_v1(
    samples: &[MainlineSoakSamplePointV1],
    elapsed_ms: u128,
) -> (MainlineSoakCounterDeltaV1, MainlineSoakMetricsV1) {
    if samples.is_empty() {
        return (Default::default(), Default::default());
    }
    let first = samples.first().expect("samples non-empty");
    let last = samples.last().expect("samples non-empty");
    let counters = MainlineSoakCounterDeltaV1 {
        execution_budget_hit_delta: saturating_delta_u64_v1(
            first.execution_budget_hit_count,
            last.execution_budget_hit_count,
        ),
        execution_deferred_delta: saturating_delta_u64_v1(
            first.execution_deferred_count,
            last.execution_deferred_count,
        ),
        execution_time_slice_exceeded_delta: saturating_delta_u64_v1(
            first.execution_time_slice_exceeded_count,
            last.execution_time_slice_exceeded_count,
        ),
        // These fields are per-drive gauges, not cumulative counters. The first
        // snapshot establishes a baseline; each subsequent distinct round counts once.
        header_updates_delta: samples.iter().skip(1).fold(0u64, |sum, sample| {
            sum.saturating_add(sample.header_updates)
        }),
        body_updates_delta: samples
            .iter()
            .skip(1)
            .fold(0u64, |sum, sample| sum.saturating_add(sample.body_updates)),
        sync_requests_delta: samples
            .iter()
            .skip(1)
            .fold(0u64, |sum, sample| sum.saturating_add(sample.sync_requests)),
    };

    let mut pending_sum = 0u128;
    let mut pending_peak = 0u64;
    let mut peak_index = 0usize;
    let mut reason_distribution: BTreeMap<String, u64> = BTreeMap::new();
    let mut target_changes = 0u64;
    let mut budget_util_sum = 0u128;
    let mut budget_util_count = 0u64;
    let mut budget_util_peak = 0u64;
    let mut slice_util_sum = 0u128;
    let mut slice_util_count = 0u64;
    let mut slice_util_peak = 0u64;

    for (idx, sample) in samples.iter().enumerate() {
        pending_sum = pending_sum.saturating_add(sample.pending_depth as u128);
        if sample.pending_depth > pending_peak {
            pending_peak = sample.pending_depth;
            peak_index = idx;
        }
        let reason = sample
            .target_reason
            .as_ref()
            .map(|value| value.to_string())
            .unwrap_or_else(|| "unknown".to_string());
        *reason_distribution.entry(reason).or_insert(0) += 1;

        if let Some(next) = samples.get(idx + 1) {
            let changed_budget = sample.effective_budget_per_tick != next.effective_budget_per_tick;
            let changed_slice = sample.effective_time_slice_ms != next.effective_time_slice_ms;
            if changed_budget || changed_slice {
                target_changes = target_changes.saturating_add(1);
            }
        }

        if let Some(hard) = sample.hard_budget_per_tick.filter(|value| *value > 0) {
            let effective = sample
                .effective_budget_per_tick
                .or(sample.target_budget_per_tick)
                .unwrap_or(hard);
            let utilization = ratio_bps_v1(effective.min(hard), hard);
            budget_util_sum = budget_util_sum.saturating_add(utilization as u128);
            budget_util_count = budget_util_count.saturating_add(1);
            budget_util_peak = budget_util_peak.max(utilization);
        }

        if let Some(hard) = sample.hard_time_slice_ms.filter(|value| *value > 0) {
            let effective = sample
                .effective_time_slice_ms
                .or(sample.target_time_slice_ms)
                .unwrap_or(hard);
            let utilization = ratio_bps_v1(effective.min(hard), hard);
            slice_util_sum = slice_util_sum.saturating_add(utilization as u128);
            slice_util_count = slice_util_count.saturating_add(1);
            slice_util_peak = slice_util_peak.max(utilization);
        }
    }

    let pending_avg = if samples.is_empty() {
        0.0
    } else {
        (pending_sum as f64) / (samples.len() as f64)
    };
    let pending_final = last.pending_depth;

    let pending_recovery_per_hour = if peak_index < samples.len().saturating_sub(1)
        && pending_peak > pending_final
    {
        let peak_ts = samples[peak_index].observed_elapsed_ms;
        let elapsed_after_peak = last.observed_elapsed_ms.saturating_sub(peak_ts);
        if elapsed_after_peak == 0 {
            0.0
        } else {
            ((pending_peak - pending_final) as f64) * 3_600_000_f64 / (elapsed_after_peak as f64)
        }
    } else {
        0.0
    };

    let top_reason = reason_distribution
        .iter()
        .max_by_key(|(_, count)| **count)
        .map(|(reason, _)| reason.to_string());
    let top_reason_share_bps = top_reason
        .as_ref()
        .and_then(|reason| reason_distribution.get(reason))
        .map(|count| ratio_bps_v1(*count, samples.len() as u64))
        .unwrap_or(0);

    let throttle_denominator = counters
        .sync_requests_delta
        .saturating_add(counters.execution_budget_hit_delta);

    let metrics = MainlineSoakMetricsV1 {
        throttle_hits_per_hour: per_hour_v1(counters.execution_budget_hit_delta, elapsed_ms),
        throttle_hit_rate_bps_estimated: ratio_bps_v1(
            counters.execution_budget_hit_delta,
            throttle_denominator,
        ),
        header_updates_per_hour: per_hour_v1(counters.header_updates_delta, elapsed_ms),
        body_updates_per_hour: per_hour_v1(counters.body_updates_delta, elapsed_ms),
        sync_requests_per_hour: per_hour_v1(counters.sync_requests_delta, elapsed_ms),
        pending_queue_depth_avg: pending_avg,
        pending_queue_depth_peak: pending_peak,
        pending_queue_depth_final: pending_final,
        pending_queue_recovery_per_hour: pending_recovery_per_hour,
        target_oscillation_bps: ratio_bps_v1(
            target_changes,
            samples.len().saturating_sub(1) as u64,
        ),
        budget_target_utilization_avg_bps: if budget_util_count == 0 {
            0
        } else {
            (budget_util_sum / (budget_util_count as u128)) as u64
        },
        budget_target_utilization_peak_bps: budget_util_peak,
        time_slice_target_utilization_avg_bps: if slice_util_count == 0 {
            0
        } else {
            (slice_util_sum / (slice_util_count as u128)) as u64
        },
        time_slice_target_utilization_peak_bps: slice_util_peak,
        execution_target_reason_distribution: reason_distribution,
        top_execution_target_reason: top_reason,
        top_execution_target_reason_share_bps: top_reason_share_bps,
    };
    (counters, metrics)
}

fn evaluate_mainline_soak_v1(
    metrics: &MainlineSoakMetricsV1,
    thresholds: &MainlineSoakThresholdsV1,
) -> MainlineSoakEvaluationV1 {
    let mut violations = Vec::new();
    if let Some(max_value) = thresholds.max_throttle_hits_per_hour {
        if metrics.throttle_hits_per_hour > max_value {
            violations.push(MainlineSoakViolationV1 {
                code: "throttle_hits_per_hour_exceeded".to_string(),
                observed: format!("{:.3}", metrics.throttle_hits_per_hour),
                threshold: format!("<= {:.3}", max_value),
                detail: "execution budget throttle hit frequency exceeded threshold".to_string(),
            });
        }
    }
    if let Some(max_value) = thresholds.max_throttle_hit_rate_bps_estimated {
        if metrics.throttle_hit_rate_bps_estimated > max_value {
            violations.push(MainlineSoakViolationV1 {
                code: "throttle_hit_rate_exceeded".to_string(),
                observed: metrics.throttle_hit_rate_bps_estimated.to_string(),
                threshold: format!("<= {max_value}"),
                detail: "estimated throttle hit rate exceeded threshold".to_string(),
            });
        }
    }
    if let Some(min_value) = thresholds.min_body_updates_per_hour {
        if metrics.body_updates_per_hour < min_value {
            violations.push(MainlineSoakViolationV1 {
                code: "body_updates_per_hour_below_min".to_string(),
                observed: format!("{:.3}", metrics.body_updates_per_hour),
                threshold: format!(">= {:.3}", min_value),
                detail: "body update throughput below threshold".to_string(),
            });
        }
    }
    if let Some(max_value) = thresholds.max_pending_queue_depth_peak {
        if metrics.pending_queue_depth_peak > max_value {
            violations.push(MainlineSoakViolationV1 {
                code: "pending_queue_depth_peak_exceeded".to_string(),
                observed: metrics.pending_queue_depth_peak.to_string(),
                threshold: format!("<= {max_value}"),
                detail: "pending queue peak depth exceeded threshold".to_string(),
            });
        }
    }
    if let Some(min_value) = thresholds.min_pending_queue_recovery_per_hour {
        if metrics.pending_queue_recovery_per_hour < min_value {
            violations.push(MainlineSoakViolationV1 {
                code: "pending_queue_recovery_below_min".to_string(),
                observed: format!("{:.3}", metrics.pending_queue_recovery_per_hour),
                threshold: format!(">= {:.3}", min_value),
                detail: "pending queue recovery speed below threshold".to_string(),
            });
        }
    }
    if let Some(max_value) = thresholds.max_target_oscillation_bps {
        if metrics.target_oscillation_bps > max_value {
            violations.push(MainlineSoakViolationV1 {
                code: "target_oscillation_exceeded".to_string(),
                observed: metrics.target_oscillation_bps.to_string(),
                threshold: format!("<= {max_value}"),
                detail: "adaptive execution target oscillation exceeded threshold".to_string(),
            });
        }
    }
    if let Some(max_value) = thresholds.max_time_slice_target_utilization_peak_bps {
        if metrics.time_slice_target_utilization_peak_bps > max_value {
            violations.push(MainlineSoakViolationV1 {
                code: "time_slice_utilization_peak_exceeded".to_string(),
                observed: metrics.time_slice_target_utilization_peak_bps.to_string(),
                threshold: format!("<= {max_value}"),
                detail: "time slice utilization peak exceeded threshold".to_string(),
            });
        }
    }
    if let Some(max_value) = thresholds.max_top_execution_target_reason_share_bps {
        if metrics.top_execution_target_reason_share_bps > max_value {
            violations.push(MainlineSoakViolationV1 {
                code: "top_execution_target_reason_share_exceeded".to_string(),
                observed: metrics.top_execution_target_reason_share_bps.to_string(),
                threshold: format!("<= {max_value}"),
                detail: "execution target reason concentration exceeded threshold".to_string(),
            });
        }
    }

    let pass = violations.is_empty();
    MainlineSoakEvaluationV1 {
        pass,
        violation_count: violations.len(),
        violations,
    }
}

pub fn write_mainline_soak_report_v1(path: &Path, report: &MainlineSoakReportV1) -> Result<()> {
    if let Some(parent) = path.parent() {
        if !parent.as_os_str().is_empty() {
            fs::create_dir_all(parent)
                .with_context(|| format!("create soak report directory: {}", parent.display()))?;
        }
    }
    let encoded = serde_json::to_string_pretty(report).context("encode soak report json")?;
    fs::write(path, format!("{encoded}\n"))
        .with_context(|| format!("write soak report: {}", path.display()))?;
    Ok(())
}

#[derive(Default)]
struct SoakObservationsV2 {
    sampling: MainlineSoakSamplingStatsV1,
    samples: Vec<MainlineSoakSamplePointV1>,
    last_observed_wall_ms: Option<u128>,
}

impl SoakObservationsV2 {
    fn observe(
        &mut self,
        config: &MainlineSoakConfigV1,
        snapshot: Result<EthFullnodeNativeWorkerRuntimeSnapshotV1>,
        started_unix_ms: u128,
        observed_wall_ms: u128,
        observed_elapsed_ms: u128,
    ) {
        let stats = &mut self.sampling;
        stats.read_attempt_count += 1;
        stats.last_attempt_valid = false;
        let clock_regressed = self
            .last_observed_wall_ms
            .is_some_and(|previous| observed_wall_ms < previous);
        if clock_regressed {
            stats.observation_clock_regression_count += 1;
        }
        self.last_observed_wall_ms = Some(observed_wall_ms);
        let snapshot = match snapshot {
            Ok(snapshot) => snapshot,
            Err(_) => {
                stats.read_error_count += 1;
                return;
            }
        };
        stats.read_success_count += 1;
        if snapshot.schema != ETH_FULLNODE_NATIVE_WORKER_RUNTIME_SCHEMA_V1 {
            stats.wrong_schema_snapshot_count += 1;
            return;
        }
        let Some(point) = sample_from_snapshot_v1(config.chain_id, snapshot, observed_elapsed_ms)
        else {
            stats.wrong_chain_snapshot_count += 1;
            return;
        };
        let updated = u128::from(point.snapshot_updated_at_unix_ms);
        if updated > observed_wall_ms {
            stats.future_snapshot_count += 1;
            return;
        }
        if updated == 0
            || observed_wall_ms - updated > u128::from(config.evidence_policy.max_snapshot_age_ms)
        {
            stats.stale_snapshot_count += 1;
            return;
        }
        if let Some(previous) = self.samples.last() {
            if point.snapshot_updated_at_unix_ms < previous.snapshot_updated_at_unix_ms {
                stats.timestamp_regression_count += 1;
                return;
            }
            // Only the budget counters are cumulative. Header/body/sync are
            // per-drive gauges and are allowed to decrease on the next drive.
            if point.execution_budget_hit_count < previous.execution_budget_hit_count
                || point.execution_deferred_count < previous.execution_deferred_count
                || point.execution_time_slice_exceeded_count
                    < previous.execution_time_slice_exceeded_count
            {
                stats.counter_regression_count += 1;
                return;
            }
            if point.snapshot_updated_at_unix_ms == previous.snapshot_updated_at_unix_ms {
                stats.duplicate_snapshot_count += 1;
                return;
            }
            if updated <= started_unix_ms {
                stats.pre_run_snapshot_count += 1;
                return;
            }
        }
        if clock_regressed {
            return;
        }
        let previous_elapsed = self.samples.last().map_or(0, |p| p.observed_elapsed_ms);
        stats.max_valid_sample_gap_ms = stats
            .max_valid_sample_gap_ms
            .max(observed_elapsed_ms.saturating_sub(previous_elapsed));
        stats.valid_sample_count += 1;
        stats.last_attempt_valid = true;
        self.samples.push(point);
    }
}

fn evidence_violation_v2(
    code: &str,
    observed: impl ToString,
    threshold: impl ToString,
) -> MainlineSoakViolationV1 {
    MainlineSoakViolationV1 {
        code: code.to_string(),
        observed: observed.to_string(),
        threshold: threshold.to_string(),
        detail: "ETH worker snapshot evidence requirement not satisfied".to_string(),
    }
}

fn finish_soak_report_v2(
    config: &MainlineSoakConfigV1,
    mut observations: SoakObservationsV2,
    started_unix_ms: u128,
    ended_unix_ms: u128,
    observed_elapsed_ms: u128,
) -> MainlineSoakReportV1 {
    let samples = &observations.samples;
    let sampling = &mut observations.sampling;
    sampling.max_valid_sample_gap_ms = sampling.max_valid_sample_gap_ms.max(
        observed_elapsed_ms.saturating_sub(samples.last().map_or(0, |p| p.observed_elapsed_ms)),
    );
    let (counters, metrics) = compute_mainline_soak_metrics_v1(samples, observed_elapsed_ms);
    let mut thresholds = config.thresholds.clone();
    if thresholds.max_pending_queue_depth_peak.is_none() {
        thresholds.max_pending_queue_depth_peak = samples.first().and_then(|sample| {
            (sample.runtime_pending_tx_snapshot_limit > 0)
                .then(|| sample.runtime_pending_tx_snapshot_limit.saturating_mul(2))
        });
    }
    let mut evaluation = evaluate_mainline_soak_v1(&metrics, &thresholds);
    let violations = &mut evaluation.violations;
    for (code, count) in [
        ("wrong_chain_snapshot", sampling.wrong_chain_snapshot_count),
        (
            "wrong_schema_snapshot",
            sampling.wrong_schema_snapshot_count,
        ),
        ("stale_snapshot", sampling.stale_snapshot_count),
        ("future_snapshot", sampling.future_snapshot_count),
        ("timestamp_regression", sampling.timestamp_regression_count),
        ("counter_regression", sampling.counter_regression_count),
        (
            "observation_clock_regression",
            sampling.observation_clock_regression_count,
        ),
        ("pre_run_snapshot_replay", sampling.pre_run_snapshot_count),
    ] {
        if count > 0 {
            violations.push(evidence_violation_v2(code, count, "0"));
        }
    }
    let policy = &config.evidence_policy;
    if sampling.valid_sample_count < policy.min_valid_samples {
        violations.push(evidence_violation_v2(
            "insufficient_valid_samples",
            sampling.valid_sample_count,
            policy.min_valid_samples,
        ));
    }
    let valid_ratio = ratio_bps_v1(sampling.valid_sample_count, sampling.read_attempt_count);
    if valid_ratio < policy.min_valid_sample_ratio_bps {
        violations.push(evidence_violation_v2(
            "valid_sample_ratio_below_min",
            valid_ratio,
            policy.min_valid_sample_ratio_bps,
        ));
    }
    if sampling.max_valid_sample_gap_ms > u128::from(policy.max_snapshot_age_ms) {
        violations.push(evidence_violation_v2(
            "sample_coverage_gap",
            sampling.max_valid_sample_gap_ms,
            policy.max_snapshot_age_ms,
        ));
    }
    if !sampling.last_attempt_valid {
        violations.push(evidence_violation_v2("last_sample_not_fresh", false, true));
    }
    if policy.mode == MainlineSoakModeV2::Workload && counters.body_updates_delta == 0 {
        violations.push(evidence_violation_v2(
            "no_sampled_body_progress",
            0,
            "> 0 after baseline",
        ));
    }
    if observed_elapsed_ms < u128::from(config.duration_seconds) * 1_000 {
        violations.push(evidence_violation_v2(
            "requested_duration_not_observed",
            observed_elapsed_ms,
            u128::from(config.duration_seconds) * 1_000,
        ));
    }
    evaluation.pass = violations.is_empty();
    evaluation.violation_count = violations.len();
    let nominal_duration_seconds = default_mainline_soak_duration_seconds_v1(&config.profile);
    // A suspended or slow two-second smoke may not acquire a six-hour label.
    let duration_requirement_met = config.duration_seconds >= nominal_duration_seconds
        && observed_elapsed_ms >= u128::from(nominal_duration_seconds) * 1_000;
    let validation_scope = match policy.mode {
        MainlineSoakModeV2::IdleHealth => "idle_health",
        MainlineSoakModeV2::Workload if duration_requirement_met => "soak",
        MainlineSoakModeV2::Workload => "short_smoke",
    };
    MainlineSoakReportV1 {
        schema: MAINLINE_SOAK_REPORT_SCHEMA_V1,
        generated_utc: Utc::now().to_rfc3339(),
        profile: config.profile.clone(),
        mode: policy.mode,
        validation_scope,
        nominal_duration_seconds,
        duration_requirement_met,
        counter_semantics: "eth_worker_sampled_rounds_and_process_budget_deltas",
        process_continuity_attested: false,
        evidence_policy: policy.clone(),
        chain_id: config.chain_id,
        snapshot_path: config.snapshot_path.display().to_string(),
        started_unix_ms,
        ended_unix_ms,
        requested_duration_seconds: config.duration_seconds,
        observed_elapsed_seconds: (observed_elapsed_ms / 1_000) as u64,
        observed_elapsed_ms,
        sample_interval_seconds: config.sample_interval_seconds,
        sample_count: samples.len(),
        sampling: observations.sampling,
        counters,
        metrics,
        thresholds,
        evaluation,
    }
}

pub fn run_mainline_soak_v1(config: &MainlineSoakConfigV1) -> Result<MainlineSoakReportV1> {
    validate_soak_config_v2(config)?;
    let started_unix_ms = now_unix_ms_v1();
    let clock = Instant::now();
    let duration = Duration::from_secs(config.duration_seconds);
    let sample_sleep = Duration::from_secs(config.sample_interval_seconds);
    let mut observations = SoakObservationsV2::default();
    loop {
        let snapshot =
            load_eth_fullnode_native_worker_runtime_snapshot_from_path_v1(&config.snapshot_path)
                .map_err(Into::into);
        observations.observe(
            config,
            snapshot,
            started_unix_ms,
            now_unix_ms_v1(),
            clock.elapsed().as_millis(),
        );
        let elapsed = clock.elapsed();
        if elapsed >= duration {
            break;
        }
        thread::sleep(sample_sleep.min(duration - elapsed));
    }
    Ok(finish_soak_report_v2(
        config,
        observations,
        started_unix_ms,
        now_unix_ms_v1(),
        clock.elapsed().as_millis(),
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    const TEST_START: u128 = 10_000_000;

    fn evidence_config() -> MainlineSoakConfigV1 {
        MainlineSoakConfigV1 {
            profile: "6h".to_string(),
            chain_id: 9998897,
            duration_seconds: 2,
            sample_interval_seconds: 1,
            snapshot_path: PathBuf::from("unused-snapshot.json"),
            report_path: PathBuf::from("unused-report.json"),
            thresholds: default_mainline_soak_thresholds_v1("6h"),
            evidence_policy: MainlineSoakEvidencePolicyV2::default(),
        }
    }

    fn snapshot(updated: u64, bodies: u64) -> EthFullnodeNativeWorkerRuntimeSnapshotV1 {
        let mut snapshot: EthFullnodeNativeWorkerRuntimeSnapshotV1 = serde_json::from_str(
            include_str!("../tests/fixtures/mainline-soak/stale-snapshot.json"),
        )
        .unwrap();
        snapshot.updated_at_unix_ms = updated;
        snapshot.body_updates = bodies;
        snapshot
    }

    fn observe_at(
        observations: &mut SoakObservationsV2,
        config: &MainlineSoakConfigV1,
        elapsed: u128,
        snapshot: Result<EthFullnodeNativeWorkerRuntimeSnapshotV1>,
    ) {
        observations.observe(config, snapshot, TEST_START, TEST_START + elapsed, elapsed);
    }

    fn finish(
        config: &MainlineSoakConfigV1,
        observations: SoakObservationsV2,
    ) -> MainlineSoakReportV1 {
        let elapsed = u128::from(config.duration_seconds) * 1_000;
        finish_soak_report_v2(
            config,
            observations,
            TEST_START,
            TEST_START + elapsed,
            elapsed,
        )
    }

    fn has_violation(report: &MainlineSoakReportV1, code: &str) -> bool {
        report
            .evaluation
            .violations
            .iter()
            .any(|violation| violation.code == code)
    }

    #[test]
    fn stale_snapshot_audit_reproduction_cannot_pass() {
        let config = evidence_config();
        let mut observations = SoakObservationsV2::default();
        for elapsed in [0, 1_000, 2_000] {
            observe_at(&mut observations, &config, elapsed, Ok(snapshot(1, 0)));
        }
        let report = finish(&config, observations);
        assert!(!report.evaluation.pass);
        assert_eq!(report.sample_count, 0);
        assert_eq!(report.sampling.stale_snapshot_count, 3);
        assert!(has_violation(&report, "stale_snapshot"));
        assert!(has_violation(&report, "insufficient_valid_samples"));
        assert!(has_violation(&report, "no_sampled_body_progress"));
        assert!(serde_json::to_value(report).is_ok());
    }

    #[test]
    fn per_round_body_gauges_can_decrease_and_are_not_cumulative_deltas() {
        let config = evidence_config();
        let mut observations = SoakObservationsV2::default();
        for (elapsed, bodies) in [(0, 8), (1_000, 0), (2_000, 2)] {
            observe_at(
                &mut observations,
                &config,
                elapsed,
                Ok(snapshot((TEST_START + elapsed) as u64, bodies)),
            );
        }
        let report = finish(&config, observations);
        assert!(report.evaluation.pass, "{:?}", report.evaluation.violations);
        assert_eq!(report.counters.body_updates_delta, 2);
        assert_eq!(report.sampling.counter_regression_count, 0);
        assert_eq!(report.metrics.body_updates_per_hour, 3_600.0);
        assert_eq!(report.validation_scope, "short_smoke");
        assert!(!report.duration_requirement_met);
        assert!(!report.process_continuity_attested);
        let json = serde_json::to_value(report).unwrap();
        assert_eq!(json["schema"], "supervm-mainline-soak-report/v2");
        assert_eq!(json["counters"]["sampled_body_updates"], 2);
        assert!(json["metrics"]
            .get("sampled_body_updates_per_hour")
            .is_some());
    }

    #[test]
    fn idle_heartbeat_is_accepted_only_with_explicit_idle_scope() {
        for mode in [MainlineSoakModeV2::Workload, MainlineSoakModeV2::IdleHealth] {
            let mut config = evidence_config();
            config.evidence_policy.mode = mode;
            let mut observations = SoakObservationsV2::default();
            for elapsed in [0, 1_000, 2_000] {
                observe_at(
                    &mut observations,
                    &config,
                    elapsed,
                    Ok(snapshot((TEST_START + elapsed) as u64, 0)),
                );
            }
            let report = finish(&config, observations);
            assert_eq!(
                report.evaluation.pass,
                mode == MainlineSoakModeV2::IdleHealth
            );
            assert_eq!(
                has_violation(&report, "no_sampled_body_progress"),
                mode == MainlineSoakModeV2::Workload
            );
        }
    }

    #[test]
    fn repeating_a_fresh_baseline_never_proves_new_work() {
        let config = evidence_config();
        let mut observations = SoakObservationsV2::default();
        for elapsed in [0, 1_000, 2_000] {
            observe_at(
                &mut observations,
                &config,
                elapsed,
                Ok(snapshot(TEST_START as u64, 100)),
            );
        }
        let report = finish(&config, observations);
        assert!(!report.evaluation.pass);
        assert_eq!(report.sample_count, 1);
        assert_eq!(report.counters.body_updates_delta, 0);
        assert_eq!(report.sampling.duplicate_snapshot_count, 2);
        assert!(has_violation(&report, "last_sample_not_fresh"));
    }

    #[test]
    fn tolerated_duplicate_does_not_double_count_a_round() {
        let mut config = evidence_config();
        config.duration_seconds = 3;
        config.evidence_policy.min_valid_sample_ratio_bps = 7_500;
        let mut observations = SoakObservationsV2::default();
        for (elapsed, updated, body) in [
            (0, 0, 99),
            (1_000, 1_000, 3),
            (2_000, 1_000, 3),
            (3_000, 3_000, 4),
        ] {
            observe_at(
                &mut observations,
                &config,
                elapsed,
                Ok(snapshot((TEST_START + updated) as u64, body)),
            );
        }
        let report = finish(&config, observations);
        assert!(report.evaluation.pass);
        assert_eq!(report.sample_count, 3);
        assert_eq!(report.sampling.duplicate_snapshot_count, 1);
        assert_eq!(report.counters.body_updates_delta, 7);
    }

    #[test]
    fn invalid_source_domains_and_timestamps_are_hard_failures() {
        for case in ["schema", "chain", "zero", "future", "regression", "clock"] {
            let config = evidence_config();
            let mut observations = SoakObservationsV2::default();
            observe_at(
                &mut observations,
                &config,
                0,
                Ok(snapshot(TEST_START as u64, 1)),
            );
            observe_at(
                &mut observations,
                &config,
                1_000,
                Ok(snapshot((TEST_START + 1_000) as u64, 1)),
            );
            let mut point = snapshot((TEST_START + 2_000) as u64, 1);
            match case {
                "schema" => point.schema = "unknown/v99".to_string(),
                "chain" => point.chain_id += 1,
                "zero" => point.updated_at_unix_ms = 0,
                "future" => point.updated_at_unix_ms += 1,
                "regression" => point.updated_at_unix_ms = (TEST_START + 500) as u64,
                "clock" => {}
                _ => unreachable!(),
            }
            let now = if case == "clock" {
                TEST_START + 999
            } else {
                TEST_START + 2_000
            };
            observations.observe(&config, Ok(point), TEST_START, now, 2_000);
            let report = finish(&config, observations);
            assert!(!report.evaluation.pass, "{case}");
            let code = match case {
                "schema" => "wrong_schema_snapshot",
                "chain" => "wrong_chain_snapshot",
                "zero" => "stale_snapshot",
                "future" => "future_snapshot",
                "regression" => "timestamp_regression",
                "clock" => "observation_clock_regression",
                _ => unreachable!(),
            };
            assert!(has_violation(&report, code), "{case}");
        }
    }

    #[test]
    fn intermediate_cumulative_reset_cannot_hide_behind_positive_end_delta() {
        let config = evidence_config();
        for field in 0..3 {
            let mut observations = SoakObservationsV2::default();
            for (elapsed, count) in [(0, 5), (1_000, 2), (2_000, 8)] {
                let mut point = snapshot((TEST_START + elapsed) as u64, 1);
                match field {
                    0 => {
                        point
                            .native_execution_budget_runtime
                            .execution_budget_hit_count = count
                    }
                    1 => {
                        point
                            .native_execution_budget_runtime
                            .execution_deferred_count = count
                    }
                    _ => {
                        point
                            .native_execution_budget_runtime
                            .execution_time_slice_exceeded_count = count
                    }
                }
                observe_at(&mut observations, &config, elapsed, Ok(point));
            }
            let report = finish(&config, observations);
            assert!(!report.evaluation.pass);
            assert!(has_violation(&report, "counter_regression"));
        }
    }

    #[test]
    fn finite_read_error_budget_does_not_allow_long_sampling_gaps() {
        for long_gap in [false, true] {
            let mut config = evidence_config();
            config.duration_seconds = 20;
            config.evidence_policy.max_snapshot_age_ms = 3_000;
            if long_gap {
                config.evidence_policy.min_valid_sample_ratio_bps = 1;
            }
            let mut observations = SoakObservationsV2::default();
            for second in 0..=20 {
                let elapsed = second * 1_000;
                let point = if second == 5 || (long_gap && (2..=15).contains(&second)) {
                    Err(anyhow::anyhow!("simulated partial write"))
                } else {
                    Ok(snapshot((TEST_START + elapsed) as u64, 1))
                };
                observe_at(&mut observations, &config, elapsed, point);
            }
            let report = finish(&config, observations);
            assert_eq!(
                report.evaluation.pass, !long_gap,
                "{:?}",
                report.evaluation.violations
            );
            assert_eq!(has_violation(&report, "sample_coverage_gap"), long_gap);
        }
    }

    #[test]
    fn beginning_end_and_all_missing_samples_fail_closed() {
        for missing in ["first", "last", "all"] {
            let mut config = evidence_config();
            config.evidence_policy.max_snapshot_age_ms = 1_000;
            config.evidence_policy.min_valid_sample_ratio_bps = 1;
            let mut observations = SoakObservationsV2::default();
            for elapsed in [0, 1_000, 2_000] {
                let absent = missing == "all"
                    || (missing == "first" && elapsed < 2_000)
                    || (missing == "last" && elapsed == 2_000);
                let input = if absent {
                    Err(anyhow::anyhow!("missing"))
                } else {
                    Ok(snapshot((TEST_START + elapsed) as u64, 1))
                };
                observe_at(&mut observations, &config, elapsed, input);
            }
            let report = finish(&config, observations);
            assert!(!report.evaluation.pass, "{missing}");
            assert!(serde_json::to_string(&report).is_ok());
        }
    }

    #[test]
    fn replayed_pre_run_rounds_cannot_supply_progress() {
        let config = evidence_config();
        let mut observations = SoakObservationsV2::default();
        observe_at(
            &mut observations,
            &config,
            0,
            Ok(snapshot((TEST_START - 1_000) as u64, 10)),
        );
        observe_at(
            &mut observations,
            &config,
            1_000,
            Ok(snapshot((TEST_START - 500) as u64, 10)),
        );
        let report = finish(&config, observations);
        assert!(has_violation(&report, "pre_run_snapshot_replay"));
        assert_eq!(report.counters.body_updates_delta, 0);
    }

    #[test]
    fn wall_clock_jump_cannot_upgrade_short_smoke_to_long_soak() {
        let config = evidence_config();
        let mut observations = SoakObservationsV2::default();
        for elapsed in [0, 1_000, 2_000] {
            let wall = TEST_START + elapsed * 20_000;
            observations.observe(
                &config,
                Ok(snapshot(wall as u64, 1)),
                TEST_START,
                wall,
                elapsed,
            );
        }
        let report = finish_soak_report_v2(
            &config,
            observations,
            TEST_START,
            TEST_START + 40_000_000,
            2_000,
        );
        assert!(report.evaluation.pass);
        assert_eq!(report.observed_elapsed_ms, 2_000);
        assert!(!report.duration_requirement_met);
        assert_eq!(report.validation_scope, "short_smoke");
    }

    #[test]
    fn complete_observed_window_is_required_for_duration_qualification() {
        let mut config = evidence_config();
        config.duration_seconds = 21_600;
        config.sample_interval_seconds = 60;
        let mut observations = SoakObservationsV2::default();
        for elapsed in (0..=21_600_000).step_by(60_000) {
            observe_at(
                &mut observations,
                &config,
                elapsed,
                Ok(snapshot((TEST_START + elapsed) as u64, 1)),
            );
        }
        let report = finish(&config, observations);
        assert!(report.evaluation.pass);
        assert!(report.duration_requirement_met);
        assert_eq!(report.validation_scope, "soak");
        // Synthetic clock observations test classification, not a six-hour real run.
        let premature = finish_soak_report_v2(
            &config,
            Default::default(),
            TEST_START,
            TEST_START + 1_000,
            1_000,
        );
        assert!(!premature.duration_requirement_met);
        assert!(has_violation(&premature, "requested_duration_not_observed"));
    }

    #[test]
    fn unsafe_or_impossible_sampling_configuration_is_rejected() {
        for case in 0..11 {
            let mut config = evidence_config();
            match case {
                0 => config.duration_seconds = 0,
                1 => config.sample_interval_seconds = 0,
                2 => config.sample_interval_seconds = 3,
                3 => config.evidence_policy.max_snapshot_age_ms = 999,
                4 => config.evidence_policy.min_valid_samples = 1,
                5 => config.evidence_policy.min_valid_samples = 4,
                6 => config.evidence_policy.min_valid_sample_ratio_bps = 0,
                7 => config.evidence_policy.min_valid_sample_ratio_bps = 10_001,
                8 => config.profile = "typo".to_string(),
                9 => config.duration_seconds = u64::MAX,
                _ => config.duration_seconds = 100_000,
            }
            assert!(validate_soak_config_v2(&config).is_err(), "case={case}");
        }
        for value in [f64::NAN, f64::INFINITY, f64::NEG_INFINITY, -1.0] {
            let mut config = evidence_config();
            config.thresholds.min_body_updates_per_hour = Some(value);
            assert!(validate_soak_config_v2(&config).is_err());
        }
    }

    struct MainlineSoakSampleInputV1 {
        ts: u128,
        updated: u64,
        pending: u64,
        hits: u64,
        deferred: u64,
        time_slice: u64,
        headers: u64,
        bodies: u64,
        sync: u64,
        hard_budget: u64,
        effective_budget: u64,
        hard_slice: u64,
        effective_slice: u64,
        reason: &'static str,
    }

    fn make_sample(input: MainlineSoakSampleInputV1) -> MainlineSoakSamplePointV1 {
        MainlineSoakSamplePointV1 {
            observed_elapsed_ms: input.ts,
            snapshot_updated_at_unix_ms: input.updated,
            execution_budget_hit_count: input.hits,
            execution_deferred_count: input.deferred,
            execution_time_slice_exceeded_count: input.time_slice,
            header_updates: input.headers,
            body_updates: input.bodies,
            sync_requests: input.sync,
            pending_depth: input.pending,
            hard_budget_per_tick: Some(input.hard_budget),
            target_budget_per_tick: Some(input.effective_budget),
            effective_budget_per_tick: Some(input.effective_budget),
            hard_time_slice_ms: Some(input.hard_slice),
            target_time_slice_ms: Some(input.effective_slice),
            effective_time_slice_ms: Some(input.effective_slice),
            target_reason: Some(input.reason.to_string()),
            runtime_pending_tx_snapshot_limit: 2_048,
        }
    }

    #[test]
    fn soak_metrics_capture_reason_distribution_and_oscillation() {
        let samples = vec![
            make_sample(MainlineSoakSampleInputV1 {
                ts: 1_000,
                updated: 1_000,
                pending: 120,
                hits: 10,
                deferred: 5,
                time_slice: 1,
                headers: 100,
                bodies: 80,
                sync: 70,
                hard_budget: 64,
                effective_budget: 48,
                hard_slice: 10,
                effective_slice: 8,
                reason: "backlog_pressure",
            }),
            make_sample(MainlineSoakSampleInputV1 {
                ts: 2_000,
                updated: 2_000,
                pending: 90,
                hits: 12,
                deferred: 7,
                time_slice: 1,
                headers: 120,
                bodies: 100,
                sync: 90,
                hard_budget: 64,
                effective_budget: 56,
                hard_slice: 10,
                effective_slice: 9,
                reason: "sync_pressure",
            }),
            make_sample(MainlineSoakSampleInputV1 {
                ts: 3_000,
                updated: 3_000,
                pending: 70,
                hits: 13,
                deferred: 8,
                time_slice: 1,
                headers: 140,
                bodies: 120,
                sync: 110,
                hard_budget: 64,
                effective_budget: 56,
                hard_slice: 10,
                effective_slice: 9,
                reason: "sync_pressure",
            }),
        ];

        let (_counters, metrics) = compute_mainline_soak_metrics_v1(samples.as_slice(), 2_000);
        assert_eq!(metrics.pending_queue_depth_peak, 120);
        assert_eq!(metrics.pending_queue_depth_final, 70);
        assert!(metrics.pending_queue_recovery_per_hour > 0.0);
        assert_eq!(
            metrics
                .execution_target_reason_distribution
                .get("sync_pressure")
                .copied(),
            Some(2)
        );
        assert_eq!(
            metrics.top_execution_target_reason.as_deref(),
            Some("sync_pressure")
        );
        assert!(metrics.target_oscillation_bps > 0);
    }

    #[test]
    fn soak_evaluation_flags_threshold_violations() {
        let metrics = MainlineSoakMetricsV1 {
            throttle_hits_per_hour: 120.0,
            throttle_hit_rate_bps_estimated: 9_900,
            header_updates_per_hour: 100.0,
            body_updates_per_hour: 50.0,
            sync_requests_per_hour: 80.0,
            pending_queue_depth_avg: 200.0,
            pending_queue_depth_peak: 500,
            pending_queue_depth_final: 450,
            pending_queue_recovery_per_hour: -1.0,
            target_oscillation_bps: 9_500,
            budget_target_utilization_avg_bps: 8_000,
            budget_target_utilization_peak_bps: 10_000,
            time_slice_target_utilization_avg_bps: 9_000,
            time_slice_target_utilization_peak_bps: 10_000,
            execution_target_reason_distribution: BTreeMap::new(),
            top_execution_target_reason: Some("throttle_backoff".to_string()),
            top_execution_target_reason_share_bps: 9_800,
        };
        let thresholds = MainlineSoakThresholdsV1 {
            max_throttle_hits_per_hour: Some(100.0),
            max_throttle_hit_rate_bps_estimated: Some(9_500),
            min_body_updates_per_hour: Some(60.0),
            max_pending_queue_depth_peak: Some(400),
            min_pending_queue_recovery_per_hour: Some(0.0),
            max_target_oscillation_bps: Some(9_000),
            max_time_slice_target_utilization_peak_bps: Some(9_500),
            max_top_execution_target_reason_share_bps: Some(9_500),
        };
        let evaluation = evaluate_mainline_soak_v1(&metrics, &thresholds);
        assert!(!evaluation.pass);
        assert!(evaluation.violation_count >= 6);
    }
}
