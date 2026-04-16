#![forbid(unsafe_code)]

use anyhow::{bail, Context, Result};
use chrono::Utc;
use novovm_network::{
    default_eth_fullnode_native_worker_runtime_snapshot_path_v1,
    load_eth_fullnode_native_worker_runtime_snapshot_from_path_v1,
    EthFullnodeNativeWorkerRuntimeSnapshotV1,
};
use serde::Serialize;
use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::thread;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

pub const MAINLINE_SOAK_REPORT_SCHEMA_V1: &str = "supervm-mainline-soak-report/v1";
pub const MAINLINE_NIGHTLY_SOAK_GATE_REPORT_SCHEMA_V1: &str =
    "supervm-mainline-nightly-soak-gate-report/v1";

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
}

#[derive(Debug, Clone, Serialize)]
pub struct MainlineSoakSamplingStatsV1 {
    pub read_attempt_count: u64,
    pub read_success_count: u64,
    pub read_error_count: u64,
    pub wrong_chain_snapshot_count: u64,
}

#[derive(Debug, Clone, Serialize)]
pub struct MainlineSoakCounterDeltaV1 {
    pub execution_budget_hit_delta: u64,
    pub execution_deferred_delta: u64,
    pub execution_time_slice_exceeded_delta: u64,
    pub header_updates_delta: u64,
    pub body_updates_delta: u64,
    pub sync_requests_delta: u64,
}

#[derive(Debug, Clone, Serialize)]
pub struct MainlineSoakMetricsV1 {
    pub throttle_hits_per_hour: f64,
    pub throttle_hit_rate_bps_estimated: u64,
    pub header_updates_per_hour: f64,
    pub body_updates_per_hour: f64,
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
    pub chain_id: u64,
    pub snapshot_path: String,
    pub started_unix_ms: u128,
    pub ended_unix_ms: u128,
    pub requested_duration_seconds: u64,
    pub observed_elapsed_seconds: u64,
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
    observed_unix_ms: u128,
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
) -> Option<MainlineSoakSamplePointV1> {
    if snapshot.chain_id != chain_id {
        return None;
    }
    Some(MainlineSoakSamplePointV1 {
        observed_unix_ms: now_unix_ms_v1(),
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
        header_updates_delta: saturating_delta_u64_v1(first.header_updates, last.header_updates),
        body_updates_delta: saturating_delta_u64_v1(first.body_updates, last.body_updates),
        sync_requests_delta: saturating_delta_u64_v1(first.sync_requests, last.sync_requests),
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
        let peak_ts = samples[peak_index].observed_unix_ms;
        let elapsed_after_peak = last.observed_unix_ms.saturating_sub(peak_ts);
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

pub fn run_mainline_soak_v1(config: &MainlineSoakConfigV1) -> Result<MainlineSoakReportV1> {
    if config.sample_interval_seconds == 0 {
        bail!("sample_interval_seconds must be >= 1");
    }

    let started_unix_ms = now_unix_ms_v1();
    let requested_duration_ms = (config.duration_seconds as u128) * 1_000u128;
    let deadline_unix_ms = started_unix_ms.saturating_add(requested_duration_ms);
    let sample_sleep = Duration::from_secs(config.sample_interval_seconds);
    let mut sampling = MainlineSoakSamplingStatsV1 {
        read_attempt_count: 0,
        read_success_count: 0,
        read_error_count: 0,
        wrong_chain_snapshot_count: 0,
    };
    let mut samples = Vec::new();
    let mut dynamic_pending_peak_threshold = config.thresholds.max_pending_queue_depth_peak;

    loop {
        sampling.read_attempt_count = sampling.read_attempt_count.saturating_add(1);
        match load_eth_fullnode_native_worker_runtime_snapshot_from_path_v1(
            config.snapshot_path.as_path(),
        ) {
            Ok(snapshot) => {
                sampling.read_success_count = sampling.read_success_count.saturating_add(1);
                if let Some(point) = sample_from_snapshot_v1(config.chain_id, snapshot) {
                    if dynamic_pending_peak_threshold.is_none()
                        && point.runtime_pending_tx_snapshot_limit > 0
                    {
                        dynamic_pending_peak_threshold =
                            Some(point.runtime_pending_tx_snapshot_limit.saturating_mul(2));
                    }
                    samples.push(point);
                } else {
                    sampling.wrong_chain_snapshot_count =
                        sampling.wrong_chain_snapshot_count.saturating_add(1);
                }
            }
            Err(_) => {
                sampling.read_error_count = sampling.read_error_count.saturating_add(1);
            }
        }

        if now_unix_ms_v1() >= deadline_unix_ms {
            break;
        }
        thread::sleep(sample_sleep);
    }

    if samples.is_empty() {
        bail!(
            "no runtime samples collected from {} for chain_id={}",
            config.snapshot_path.display(),
            config.chain_id
        );
    }

    let ended_unix_ms = now_unix_ms_v1();
    let observed_elapsed_ms = ended_unix_ms.saturating_sub(started_unix_ms);
    let observed_elapsed_seconds = (observed_elapsed_ms / 1_000u128) as u64;
    let sample_elapsed_ms = if samples.len() >= 2 {
        let first = samples.first().expect("non-empty");
        let last = samples.last().expect("non-empty");
        let snapshot_elapsed = (last.snapshot_updated_at_unix_ms as u128)
            .saturating_sub(first.snapshot_updated_at_unix_ms as u128);
        let observed_elapsed = last.observed_unix_ms.saturating_sub(first.observed_unix_ms);
        snapshot_elapsed.max(observed_elapsed)
    } else {
        observed_elapsed_ms
    };

    let (counters, metrics) =
        compute_mainline_soak_metrics_v1(samples.as_slice(), sample_elapsed_ms);
    let mut thresholds = config.thresholds.clone();
    if thresholds.max_pending_queue_depth_peak.is_none() {
        thresholds.max_pending_queue_depth_peak = dynamic_pending_peak_threshold;
    }
    let evaluation = evaluate_mainline_soak_v1(&metrics, &thresholds);

    Ok(MainlineSoakReportV1 {
        schema: MAINLINE_SOAK_REPORT_SCHEMA_V1,
        generated_utc: Utc::now().to_rfc3339(),
        profile: config.profile.clone(),
        chain_id: config.chain_id,
        snapshot_path: config.snapshot_path.display().to_string(),
        started_unix_ms,
        ended_unix_ms,
        requested_duration_seconds: config.duration_seconds,
        observed_elapsed_seconds,
        sample_interval_seconds: config.sample_interval_seconds,
        sample_count: samples.len(),
        sampling,
        counters,
        metrics,
        thresholds,
        evaluation,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

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
            observed_unix_ms: input.ts,
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
