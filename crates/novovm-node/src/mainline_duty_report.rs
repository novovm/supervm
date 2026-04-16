#![forbid(unsafe_code)]

use anyhow::{Context, Result};
use chrono::Utc;
use serde_json::Value;
use std::fs;
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MainlineDutyLevelV1 {
    Green,
    Yellow,
    Red,
}

impl MainlineDutyLevelV1 {
    #[must_use]
    pub fn emoji(self) -> &'static str {
        match self {
            Self::Green => "🟢",
            Self::Yellow => "🟡",
            Self::Red => "🔴",
        }
    }

    #[must_use]
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Green => "GREEN",
            Self::Yellow => "YELLOW",
            Self::Red => "RED",
        }
    }
}

#[derive(Debug, Clone)]
pub struct MainlineSoakProfileDigestV1 {
    pub profile: String,
    pub pass: bool,
    pub violation_count: u64,
    pub sample_count: u64,
    pub observed_elapsed_seconds: u64,
    pub throttle_hit_rate_bps_estimated: u64,
    pub body_updates_per_hour: f64,
    pub pending_queue_depth_peak: u64,
    pub pending_queue_recovery_per_hour: f64,
    pub target_oscillation_bps: u64,
    pub time_slice_target_utilization_peak_bps: u64,
    pub top_execution_target_reason: String,
    pub top_execution_target_reason_share_bps: u64,
    pub max_throttle_hit_rate_bps_estimated: Option<u64>,
    pub max_pending_queue_depth_peak: Option<u64>,
    pub max_target_oscillation_bps: Option<u64>,
    pub max_time_slice_target_utilization_peak_bps: Option<u64>,
    pub max_top_execution_target_reason_share_bps: Option<u64>,
    pub violation_codes: Vec<String>,
}

#[derive(Debug, Clone)]
pub struct MainlineDutyReportInputV1 {
    pub generated_utc: String,
    pub owner: String,
    pub workflow_run_url: Option<String>,
    pub nightly_overall_pass: bool,
    pub nightly_profile_summary: Vec<(String, bool, u64)>,
    pub six_hour: MainlineSoakProfileDigestV1,
    pub twenty_four_hour: MainlineSoakProfileDigestV1,
}

#[derive(Debug, Clone)]
pub struct MainlineDutyReportOutputV1 {
    pub level: MainlineDutyLevelV1,
    pub primary_issue: String,
    pub markdown: String,
}

#[must_use]
pub fn default_mainline_nightly_gate_report_path_v1() -> PathBuf {
    PathBuf::from("artifacts/mainline/mainline-nightly-soak-gate-report.json")
}

#[must_use]
pub fn default_mainline_soak_6h_report_path_v1() -> PathBuf {
    PathBuf::from("artifacts/mainline/mainline-soak-6h.json")
}

#[must_use]
pub fn default_mainline_soak_24h_report_path_v1() -> PathBuf {
    PathBuf::from("artifacts/mainline/mainline-soak-24h.json")
}

#[must_use]
pub fn default_mainline_duty_report_markdown_path_v1() -> PathBuf {
    let date = Utc::now().format("%Y-%m-%d").to_string();
    PathBuf::from(format!("artifacts/mainline/mainline-duty-report-{date}.md"))
}

fn load_json_v1(path: &Path) -> Result<Value> {
    let raw = fs::read_to_string(path)
        .with_context(|| format!("read duty source json failed: {}", path.display()))?;
    let parsed = serde_json::from_str::<Value>(raw.as_str())
        .with_context(|| format!("parse duty source json failed: {}", path.display()))?;
    Ok(parsed)
}

fn v_as_u64(v: Option<&Value>) -> u64 {
    match v {
        Some(Value::Number(n)) => n.as_u64().unwrap_or(0),
        Some(Value::String(raw)) => raw.parse::<u64>().unwrap_or(0),
        _ => 0,
    }
}

fn v_as_f64(v: Option<&Value>) -> f64 {
    match v {
        Some(Value::Number(n)) => n.as_f64().unwrap_or(0.0),
        Some(Value::String(raw)) => raw.parse::<f64>().unwrap_or(0.0),
        _ => 0.0,
    }
}

fn v_as_bool(v: Option<&Value>) -> bool {
    match v {
        Some(Value::Bool(value)) => *value,
        Some(Value::String(raw)) => matches!(raw.as_str(), "true" | "1" | "yes" | "on"),
        _ => false,
    }
}

fn v_as_string(v: Option<&Value>) -> String {
    match v {
        Some(Value::String(value)) => value.trim().to_string(),
        Some(other) => other.to_string(),
        None => String::new(),
    }
}

fn opt_u64(v: Option<&Value>) -> Option<u64> {
    match v {
        Some(Value::Number(n)) => n.as_u64(),
        Some(Value::String(raw)) => raw.parse::<u64>().ok(),
        _ => None,
    }
}

fn map_get<'a>(v: &'a Value, key: &str) -> Option<&'a Value> {
    v.as_object()?.get(key)
}

fn parse_soak_digest_v1(raw: &Value, fallback_profile: &str) -> MainlineSoakProfileDigestV1 {
    let metrics = map_get(raw, "metrics").cloned().unwrap_or(Value::Null);
    let thresholds = map_get(raw, "thresholds").cloned().unwrap_or(Value::Null);
    let evaluation = map_get(raw, "evaluation").cloned().unwrap_or(Value::Null);
    let violation_codes = map_get(&evaluation, "violations")
        .and_then(Value::as_array)
        .map(|items| {
            items
                .iter()
                .filter_map(|item| map_get(item, "code"))
                .map(|value| v_as_string(Some(value)))
                .filter(|code| !code.is_empty())
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();

    MainlineSoakProfileDigestV1 {
        profile: {
            let p = v_as_string(map_get(raw, "profile"));
            if p.is_empty() {
                fallback_profile.to_string()
            } else {
                p
            }
        },
        pass: v_as_bool(map_get(&evaluation, "pass")),
        violation_count: v_as_u64(map_get(&evaluation, "violation_count")),
        sample_count: v_as_u64(map_get(raw, "sample_count")),
        observed_elapsed_seconds: v_as_u64(map_get(raw, "observed_elapsed_seconds")),
        throttle_hit_rate_bps_estimated: v_as_u64(map_get(
            &metrics,
            "throttle_hit_rate_bps_estimated",
        )),
        body_updates_per_hour: v_as_f64(map_get(&metrics, "body_updates_per_hour")),
        pending_queue_depth_peak: v_as_u64(map_get(&metrics, "pending_queue_depth_peak")),
        pending_queue_recovery_per_hour: v_as_f64(map_get(
            &metrics,
            "pending_queue_recovery_per_hour",
        )),
        target_oscillation_bps: v_as_u64(map_get(&metrics, "target_oscillation_bps")),
        time_slice_target_utilization_peak_bps: v_as_u64(map_get(
            &metrics,
            "time_slice_target_utilization_peak_bps",
        )),
        top_execution_target_reason: v_as_string(map_get(&metrics, "top_execution_target_reason")),
        top_execution_target_reason_share_bps: v_as_u64(map_get(
            &metrics,
            "top_execution_target_reason_share_bps",
        )),
        max_throttle_hit_rate_bps_estimated: opt_u64(map_get(
            &thresholds,
            "max_throttle_hit_rate_bps_estimated",
        )),
        max_pending_queue_depth_peak: opt_u64(map_get(&thresholds, "max_pending_queue_depth_peak")),
        max_target_oscillation_bps: opt_u64(map_get(&thresholds, "max_target_oscillation_bps")),
        max_time_slice_target_utilization_peak_bps: opt_u64(map_get(
            &thresholds,
            "max_time_slice_target_utilization_peak_bps",
        )),
        max_top_execution_target_reason_share_bps: opt_u64(map_get(
            &thresholds,
            "max_top_execution_target_reason_share_bps",
        )),
        violation_codes,
    }
}

fn parse_nightly_summary_v1(raw: &Value) -> (bool, Vec<(String, bool, u64)>) {
    let overall = v_as_bool(map_get(raw, "overall_pass"));
    let profiles = map_get(raw, "profile_results")
        .and_then(Value::as_array)
        .map(|items| {
            items
                .iter()
                .map(|item| {
                    let profile = v_as_string(map_get(item, "profile"));
                    let pass = v_as_bool(map_get(item, "pass"));
                    let violations = v_as_u64(map_get(item, "violation_count"));
                    (profile, pass, violations)
                })
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();
    (overall, profiles)
}

fn ratio_vs_threshold_bps_v1(value: u64, threshold: Option<u64>) -> Option<u64> {
    let limit = threshold?;
    if limit == 0 {
        return None;
    }
    Some(((value as u128) * 10_000u128 / (limit as u128)) as u64)
}

fn derive_level_and_issue_v1(input: &MainlineDutyReportInputV1) -> (MainlineDutyLevelV1, String) {
    if !input.nightly_overall_pass || !input.six_hour.pass || !input.twenty_four_hour.pass {
        let primary = input
            .twenty_four_hour
            .violation_codes
            .first()
            .cloned()
            .or_else(|| input.six_hour.violation_codes.first().cloned())
            .unwrap_or_else(|| "nightly_gate_failed".to_string());
        return (MainlineDutyLevelV1::Red, primary);
    }

    let mut yellow_signals = Vec::new();
    if ratio_vs_threshold_bps_v1(
        input.twenty_four_hour.throttle_hit_rate_bps_estimated,
        input.twenty_four_hour.max_throttle_hit_rate_bps_estimated,
    )
    .is_some_and(|ratio| ratio >= 8_500)
    {
        yellow_signals.push("throttle_hit_rate_near_limit");
    }
    if ratio_vs_threshold_bps_v1(
        input.twenty_four_hour.pending_queue_depth_peak,
        input.twenty_four_hour.max_pending_queue_depth_peak,
    )
    .is_some_and(|ratio| ratio >= 8_500)
    {
        yellow_signals.push("pending_queue_depth_near_limit");
    }
    if ratio_vs_threshold_bps_v1(
        input.twenty_four_hour.target_oscillation_bps,
        input.twenty_four_hour.max_target_oscillation_bps,
    )
    .is_some_and(|ratio| ratio >= 8_500)
    {
        yellow_signals.push("target_oscillation_near_limit");
    }
    if ratio_vs_threshold_bps_v1(
        input
            .twenty_four_hour
            .time_slice_target_utilization_peak_bps,
        input
            .twenty_four_hour
            .max_time_slice_target_utilization_peak_bps,
    )
    .is_some_and(|ratio| ratio >= 9_000)
    {
        yellow_signals.push("time_slice_utilization_near_limit");
    }
    if ratio_vs_threshold_bps_v1(
        input.twenty_four_hour.top_execution_target_reason_share_bps,
        input
            .twenty_four_hour
            .max_top_execution_target_reason_share_bps,
    )
    .is_some_and(|ratio| ratio >= 9_000)
    {
        yellow_signals.push("target_reason_concentration_near_limit");
    }

    if let Some(first) = yellow_signals.first() {
        return (MainlineDutyLevelV1::Yellow, (*first).to_string());
    }

    (MainlineDutyLevelV1::Green, "none".to_string())
}

fn profile_block_v1(title: &str, digest: &MainlineSoakProfileDigestV1) -> String {
    format!(
        "- {title}\n  - pass: {}\n  - sample_count: {}\n  - observed_elapsed_seconds: {}\n  - violation_count: {}\n  - throttle_hit_rate_bps_estimated: {}\n  - body_updates_per_hour: {:.3}\n  - pending_queue_depth_peak: {}\n  - pending_queue_recovery_per_hour: {:.3}\n  - target_oscillation_bps: {}\n  - time_slice_target_utilization_peak_bps: {}\n  - top_execution_target_reason: {}\n  - top_execution_target_reason_share_bps: {}\n",
        digest.pass,
        digest.sample_count,
        digest.observed_elapsed_seconds,
        digest.violation_count,
        digest.throttle_hit_rate_bps_estimated,
        digest.body_updates_per_hour,
        digest.pending_queue_depth_peak,
        digest.pending_queue_recovery_per_hour,
        digest.target_oscillation_bps,
        digest.time_slice_target_utilization_peak_bps,
        if digest.top_execution_target_reason.is_empty() {
            "unknown"
        } else {
            digest.top_execution_target_reason.as_str()
        },
        digest.top_execution_target_reason_share_bps,
    )
}

pub fn render_mainline_duty_markdown_v1(
    input: &MainlineDutyReportInputV1,
) -> MainlineDutyReportOutputV1 {
    let (level, primary_issue) = derive_level_and_issue_v1(input);
    let nightly_profile_lines = if input.nightly_profile_summary.is_empty() {
        "- (no profile_results in nightly report)".to_string()
    } else {
        input
            .nightly_profile_summary
            .iter()
            .map(|(profile, pass, violations)| {
                format!("- {profile}: pass={pass}, violation_count={violations}")
            })
            .collect::<Vec<_>>()
            .join("\n")
    };

    let workflow_line = input
        .workflow_run_url
        .as_ref()
        .map(|url| format!("workflow_run: {url}"))
        .unwrap_or_else(|| "workflow_run: (fill in)".to_string());

    let markdown = format!(
        "[EVM NIGHTLY SOAK DAILY]\n\
date_utc: {}\n\
owner: {}\n\
{}\n\
nightly_gate_overall_pass: {}\n\
profiles: 6h/24h\n\n\
【总体结论】\n\
{} {} \n\n\
【主因】\n\
{}\n\n\
【Nightly profile 摘要】\n\
{}\n\n\
【关键指标】\n\
{}\
{}\n\
【问题摘要】\n\
(仅写 1~2 句)\n\n\
【处置动作】\n\
- 参数调整:\n\
- 是否回滚:\n\
- 是否加入样本:\n\n\
【风险判断】\n\
(是否可能影响主网)\n\n\
【明日关注点】\n\
(只写 1 条)\n",
        input.generated_utc,
        if input.owner.trim().is_empty() {
            "待填写"
        } else {
            input.owner.as_str()
        },
        workflow_line,
        input.nightly_overall_pass,
        level.emoji(),
        level.as_str(),
        primary_issue,
        nightly_profile_lines,
        profile_block_v1("6h_summary", &input.six_hour),
        profile_block_v1("24h_summary", &input.twenty_four_hour),
    );

    MainlineDutyReportOutputV1 {
        level,
        primary_issue,
        markdown,
    }
}

pub fn load_mainline_duty_input_from_paths_v1(
    nightly_report_path: &Path,
    soak_6h_report_path: &Path,
    soak_24h_report_path: &Path,
    owner: String,
    workflow_run_url: Option<String>,
) -> Result<MainlineDutyReportInputV1> {
    let nightly_raw = load_json_v1(nightly_report_path)?;
    let soak_6h_raw = load_json_v1(soak_6h_report_path)?;
    let soak_24h_raw = load_json_v1(soak_24h_report_path)?;
    let (nightly_overall_pass, nightly_profile_summary) = parse_nightly_summary_v1(&nightly_raw);
    let six_hour = parse_soak_digest_v1(&soak_6h_raw, "6h");
    let twenty_four_hour = parse_soak_digest_v1(&soak_24h_raw, "24h");
    Ok(MainlineDutyReportInputV1 {
        generated_utc: Utc::now().to_rfc3339(),
        owner,
        workflow_run_url,
        nightly_overall_pass,
        nightly_profile_summary,
        six_hour,
        twenty_four_hour,
    })
}

pub fn write_mainline_duty_markdown_v1(path: &Path, markdown: &str) -> Result<()> {
    if let Some(parent) = path.parent() {
        if !parent.as_os_str().is_empty() {
            fs::create_dir_all(parent).with_context(|| {
                format!("create duty report directory failed: {}", parent.display())
            })?;
        }
    }
    fs::write(path, markdown)
        .with_context(|| format!("write duty report failed: {}", path.display()))?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn level_is_red_when_nightly_failed() {
        let digest = MainlineSoakProfileDigestV1 {
            profile: "6h".to_string(),
            pass: true,
            violation_count: 0,
            sample_count: 10,
            observed_elapsed_seconds: 100,
            throttle_hit_rate_bps_estimated: 100,
            body_updates_per_hour: 1.0,
            pending_queue_depth_peak: 10,
            pending_queue_recovery_per_hour: 1.0,
            target_oscillation_bps: 100,
            time_slice_target_utilization_peak_bps: 100,
            top_execution_target_reason: "idle".to_string(),
            top_execution_target_reason_share_bps: 100,
            max_throttle_hit_rate_bps_estimated: Some(9_000),
            max_pending_queue_depth_peak: Some(100),
            max_target_oscillation_bps: Some(9_000),
            max_time_slice_target_utilization_peak_bps: Some(10_000),
            max_top_execution_target_reason_share_bps: Some(10_000),
            violation_codes: vec![],
        };
        let input = MainlineDutyReportInputV1 {
            generated_utc: "2026-01-01T00:00:00Z".to_string(),
            owner: "ops".to_string(),
            workflow_run_url: None,
            nightly_overall_pass: false,
            nightly_profile_summary: vec![],
            six_hour: digest.clone(),
            twenty_four_hour: digest,
        };
        let out = render_mainline_duty_markdown_v1(&input);
        assert_eq!(out.level, MainlineDutyLevelV1::Red);
    }

    #[test]
    fn level_is_yellow_when_metric_near_limit() {
        let digest = MainlineSoakProfileDigestV1 {
            profile: "24h".to_string(),
            pass: true,
            violation_count: 0,
            sample_count: 10,
            observed_elapsed_seconds: 100,
            throttle_hit_rate_bps_estimated: 8_800,
            body_updates_per_hour: 1.0,
            pending_queue_depth_peak: 10,
            pending_queue_recovery_per_hour: 1.0,
            target_oscillation_bps: 100,
            time_slice_target_utilization_peak_bps: 100,
            top_execution_target_reason: "throttle_backoff".to_string(),
            top_execution_target_reason_share_bps: 100,
            max_throttle_hit_rate_bps_estimated: Some(9_000),
            max_pending_queue_depth_peak: Some(100),
            max_target_oscillation_bps: Some(9_000),
            max_time_slice_target_utilization_peak_bps: Some(10_000),
            max_top_execution_target_reason_share_bps: Some(10_000),
            violation_codes: vec![],
        };
        let input = MainlineDutyReportInputV1 {
            generated_utc: "2026-01-01T00:00:00Z".to_string(),
            owner: "ops".to_string(),
            workflow_run_url: None,
            nightly_overall_pass: true,
            nightly_profile_summary: vec![],
            six_hour: digest.clone(),
            twenty_four_hour: digest,
        };
        let out = render_mainline_duty_markdown_v1(&input);
        assert_eq!(out.level, MainlineDutyLevelV1::Yellow);
    }
}
