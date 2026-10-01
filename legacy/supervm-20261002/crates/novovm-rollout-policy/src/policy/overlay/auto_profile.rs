#![forbid(unsafe_code)]

use anyhow::{bail, Context, Result};
use serde::{de::DeserializeOwned, Deserialize, Serialize};
use std::collections::{BTreeMap, HashMap};
use std::env;
use std::fs;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use crate::output::print_success_json;

#[derive(Debug)]
struct CliArgs {
    runtime_file: PathBuf,
    state_file: PathBuf,
    current_profile: String,
    profiles: Vec<String>,
    min_hold_seconds: u64,
    switch_margin: f64,
    switchback_cooldown_seconds: u64,
    recheck_seconds: u64,
}

#[derive(Debug, Deserialize)]
struct OverlayRuntimeFile {
    #[serde(default)]
    profiles: HashMap<String, OverlayRuntimeProfile>,
}

#[derive(Debug, Clone, Deserialize)]
struct OverlayRuntimeProfile {
    #[serde(default)]
    region: String,
    #[serde(default)]
    relay_directory_file: String,
    #[serde(default)]
    relay_discovery_seed_failover_state_file: String,
    relay_discovery_region_failover_threshold: Option<f64>,
    relay_discovery_seed_success_rate_threshold: Option<f64>,
}

#[derive(Debug, Default, Deserialize)]
struct RelayDirectory {
    #[serde(default)]
    relays: Vec<RelayRecord>,
    #[serde(default)]
    discovery_region_failover_state: HashMap<String, RegionFailoverRecord>,
}

#[derive(Debug, Default, Deserialize)]
struct RelayRecord {
    #[allow(dead_code)]
    id: Option<String>,
    region: Option<String>,
    relay_score: Option<f64>,
    health: Option<f64>,
    enabled: Option<bool>,
}

#[derive(Debug, Default, Deserialize)]
struct RegionFailoverRecord {
    degraded_until_unix_ms: Option<u64>,
    consecutive_failures: Option<u64>,
    fail: Option<u64>,
    #[allow(dead_code)]
    success: Option<u64>,
}

#[derive(Debug, Default, Deserialize)]
struct SeedFailoverState {
    #[serde(default)]
    sources: HashMap<String, SeedFailoverRecord>,
}

#[derive(Debug, Default, Deserialize)]
struct SeedFailoverRecord {
    success: Option<u64>,
    fail: Option<u64>,
}

#[derive(Debug, Default, Serialize, Deserialize)]
struct AutoProfileState {
    #[serde(default)]
    current_profile: String,
    #[serde(default)]
    previous_profile: String,
    #[serde(default)]
    last_switch_unix_ms: u64,
    #[serde(default)]
    last_switch_reason: String,
    #[serde(default)]
    profile_history: HashMap<String, AutoProfileHistory>,
}

#[derive(Debug, Default, Serialize, Deserialize)]
struct AutoProfileHistory {
    #[serde(default)]
    last_selected_unix_ms: u64,
    #[serde(default)]
    last_rejected_reason: String,
    #[serde(default)]
    last_switched_out_unix_ms: u64,
}

#[derive(Debug, Serialize)]
struct ProfileMetrics {
    region: String,
    region_health_score: f64,
    relay_avg_score: f64,
    seed_success_rate: f64,
    recent_failover_count: u64,
    cooldown_active: bool,
    region_failover_threshold: f64,
    seed_success_rate_threshold: f64,
    score: f64,
}

#[derive(Debug, Serialize)]
struct AutoProfileDecision {
    selected_profile: String,
    previous_profile: String,
    action: String,
    score: f64,
    reason: String,
    switch_blocked_by_cooldown: bool,
    switch_blocked_reason: String,
    next_recheck_after_seconds: u64,
    profile_scores: BTreeMap<String, f64>,
}

fn now_unix_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|v| v.as_millis() as u64)
        .unwrap_or(0)
}

fn clamp01(v: f64) -> f64 {
    v.clamp(0.0, 1.0)
}

fn normalize_region(region: &str) -> String {
    let trimmed = region.trim();
    if trimmed.is_empty() {
        "global".to_string()
    } else {
        trimmed.to_ascii_lowercase()
    }
}

fn split_list(raw: &str) -> Vec<String> {
    raw.split([',', ';'])
        .map(str::trim)
        .filter(|v| !v.is_empty())
        .map(ToString::to_string)
        .collect()
}

fn parse_next<T: std::str::FromStr>(args: &mut std::vec::IntoIter<String>, flag: &str) -> Result<T>
where
    <T as std::str::FromStr>::Err: std::fmt::Display,
{
    let raw = args
        .next()
        .with_context(|| format!("missing value for {flag}"))?;
    raw.parse::<T>()
        .map_err(|e| anyhow::anyhow!("invalid value for {flag}: {raw}, err={e}"))
}

fn parse_args_from<I>(iter: I) -> Result<CliArgs>
where
    I: IntoIterator<Item = String>,
{
    let mut runtime_file = PathBuf::new();
    let mut state_file =
        PathBuf::from("artifacts/runtime/lifecycle/overlay.auto-profile.state.json");
    let mut current_profile = String::new();
    let mut profiles = vec![
        "prod-cn".to_string(),
        "prod-eu".to_string(),
        "prod-us".to_string(),
    ];
    let mut min_hold_seconds = 180u64;
    let mut switch_margin = 0.08f64;
    let mut switchback_cooldown_seconds = 300u64;
    let mut recheck_seconds = 30u64;

    let mut args: Vec<String> = iter.into_iter().collect();
    let mut it = std::mem::take(&mut args).into_iter();
    while let Some(flag) = it.next() {
        match flag.as_str() {
            "--runtime-file" => runtime_file = PathBuf::from(parse_next::<String>(&mut it, &flag)?),
            "--state-file" => state_file = PathBuf::from(parse_next::<String>(&mut it, &flag)?),
            "--current-profile" => current_profile = parse_next::<String>(&mut it, &flag)?,
            "--profiles" => {
                let raw = parse_next::<String>(&mut it, &flag)?;
                let list = split_list(&raw);
                if !list.is_empty() {
                    profiles = list;
                }
            }
            "--min-hold-seconds" => min_hold_seconds = parse_next::<u64>(&mut it, &flag)?,
            "--switch-margin" => switch_margin = parse_next::<f64>(&mut it, &flag)?,
            "--switchback-cooldown-seconds" => {
                switchback_cooldown_seconds = parse_next::<u64>(&mut it, &flag)?
            }
            "--recheck-seconds" => recheck_seconds = parse_next::<u64>(&mut it, &flag)?,
            _ => bail!("unknown arg: {flag}"),
        }
    }

    if runtime_file.as_os_str().is_empty() {
        bail!("--runtime-file is required");
    }
    if profiles.is_empty() {
        bail!("profiles is empty");
    }
    if switch_margin < 0.0 {
        switch_margin = 0.0;
    }
    if min_hold_seconds == 0 {
        min_hold_seconds = 1;
    }
    if switchback_cooldown_seconds == 0 {
        switchback_cooldown_seconds = 1;
    }
    if recheck_seconds == 0 {
        recheck_seconds = 1;
    }

    Ok(CliArgs {
        runtime_file,
        state_file,
        current_profile,
        profiles,
        min_hold_seconds,
        switch_margin,
        switchback_cooldown_seconds,
        recheck_seconds,
    })
}

pub fn run_from_env() -> Result<()> {
    run_with_args(env::args().skip(1))
}

fn resolve_path(base_file: &Path, raw: &str, fallback: &str) -> PathBuf {
    let use_raw = if raw.trim().is_empty() { fallback } else { raw };
    let p = PathBuf::from(use_raw);
    if p.is_absolute() {
        return p;
    }
    let base_dir = base_file.parent().unwrap_or_else(|| Path::new("."));
    base_dir.join(p)
}

fn read_json_file<T: DeserializeOwned>(path: &Path) -> Result<T> {
    let raw = fs::read_to_string(path)
        .with_context(|| format!("read json file failed: {}", path.display()))?;
    let obj = serde_json::from_str::<T>(&raw)
        .with_context(|| format!("parse json file failed: {}", path.display()))?;
    Ok(obj)
}

fn read_json_file_or_default<T: DeserializeOwned + Default>(path: &Path) -> T {
    match fs::read_to_string(path) {
        Ok(raw) => serde_json::from_str::<T>(&raw).unwrap_or_default(),
        Err(_) => T::default(),
    }
}

fn avg(values: &[f64], default: f64) -> f64 {
    if values.is_empty() {
        return default;
    }
    let mut sum = 0.0f64;
    for v in values {
        sum += *v;
    }
    clamp01(sum / values.len() as f64)
}

fn find_region_state<'a>(
    map: &'a HashMap<String, RegionFailoverRecord>,
    region_key: &str,
) -> Option<&'a RegionFailoverRecord> {
    if let Some(v) = map.get(region_key) {
        return Some(v);
    }
    for (k, v) in map {
        if normalize_region(k) == region_key {
            return Some(v);
        }
    }
    None
}

fn compute_seed_success_rate(seed_state: &SeedFailoverState) -> f64 {
    let mut success_total = 0u64;
    let mut fail_total = 0u64;
    for rec in seed_state.sources.values() {
        success_total = success_total.saturating_add(rec.success.unwrap_or(0));
        fail_total = fail_total.saturating_add(rec.fail.unwrap_or(0));
    }
    let total = success_total.saturating_add(fail_total);
    if total == 0 {
        return 1.0;
    }
    clamp01(success_total as f64 / total as f64)
}

fn profile_metrics(
    runtime_file: &Path,
    profile: &OverlayRuntimeProfile,
    now_ms: u64,
) -> ProfileMetrics {
    let region = if profile.region.trim().is_empty() {
        "global".to_string()
    } else {
        profile.region.trim().to_string()
    };
    let region_key = normalize_region(&region);

    let directory_path = resolve_path(
        runtime_file,
        &profile.relay_directory_file,
        "config/runtime/lifecycle/overlay.relay.directory.json",
    );
    let directory: RelayDirectory = read_json_file_or_default(&directory_path);

    let mut region_scores = Vec::new();
    let mut all_scores = Vec::new();
    for relay in &directory.relays {
        if relay.enabled == Some(false) {
            continue;
        }
        let score = clamp01(relay.relay_score.or(relay.health).unwrap_or(0.5));
        let relay_region_key = normalize_region(relay.region.as_deref().unwrap_or("global"));
        all_scores.push(score);
        if relay_region_key == region_key {
            region_scores.push(score);
        }
    }
    if region_scores.is_empty() && !all_scores.is_empty() {
        region_scores = all_scores.clone();
    }
    let region_health_score = avg(&region_scores, 0.5);
    let relay_avg_score = avg(&region_scores, region_health_score);

    let region_state = find_region_state(&directory.discovery_region_failover_state, &region_key);
    let degraded_until = region_state
        .and_then(|v| v.degraded_until_unix_ms)
        .unwrap_or(0);
    let cooldown_active = degraded_until > now_ms;
    let recent_failover_count = region_state
        .and_then(|v| v.consecutive_failures.or(v.fail))
        .unwrap_or(0);

    let seed_state_path = resolve_path(
        runtime_file,
        &profile.relay_discovery_seed_failover_state_file,
        "artifacts/runtime/lifecycle/overlay.relay.discovery.seed-failover.state.json",
    );
    let seed_state: SeedFailoverState = read_json_file_or_default(&seed_state_path);
    let seed_success_rate = compute_seed_success_rate(&seed_state);

    let region_failover_threshold = clamp01(
        profile
            .relay_discovery_region_failover_threshold
            .unwrap_or(0.5),
    );
    let seed_success_rate_threshold = clamp01(
        profile
            .relay_discovery_seed_success_rate_threshold
            .unwrap_or(0.5),
    );

    let cooldown_penalty = if cooldown_active { 1.0 } else { 0.0 };
    let score = clamp01(
        0.40 * region_health_score + 0.30 * relay_avg_score + 0.20 * seed_success_rate
            - 0.08 * recent_failover_count as f64
            - 0.15 * cooldown_penalty,
    );

    ProfileMetrics {
        region,
        region_health_score,
        relay_avg_score,
        seed_success_rate,
        recent_failover_count,
        cooldown_active,
        region_failover_threshold,
        seed_success_rate_threshold,
        score,
    }
}

fn ensure_history(state: &mut AutoProfileState, profiles: &[String]) {
    for p in profiles {
        state.profile_history.entry(p.clone()).or_default();
    }
}

pub fn run_with_args<I>(iter: I) -> Result<()>
where
    I: IntoIterator<Item = String>,
{
    let args = parse_args_from(iter)?;
    let now_ms = now_unix_ms();

    let runtime: OverlayRuntimeFile = read_json_file(&args.runtime_file)?;
    let mut state: AutoProfileState = read_json_file_or_default(&args.state_file);
    ensure_history(&mut state, &args.profiles);

    let mut metrics_map: BTreeMap<String, ProfileMetrics> = BTreeMap::new();
    for profile_name in &args.profiles {
        let profile = runtime
            .profiles
            .get(profile_name)
            .with_context(|| format!("runtime profile not found: {profile_name}"))?;
        metrics_map.insert(
            profile_name.clone(),
            profile_metrics(&args.runtime_file, profile, now_ms),
        );
    }

    let mut current_profile = if !args.current_profile.trim().is_empty() {
        args.current_profile.trim().to_string()
    } else if !state.current_profile.trim().is_empty() {
        state.current_profile.trim().to_string()
    } else {
        args.profiles
            .first()
            .cloned()
            .context("profiles is empty")?
    };
    if !metrics_map.contains_key(&current_profile) {
        current_profile = args
            .profiles
            .first()
            .cloned()
            .context("profiles is empty")?;
    }

    let mut profile_scores = BTreeMap::new();
    for (name, m) in &metrics_map {
        profile_scores.insert(name.clone(), m.score);
    }

    let (candidate_profile, candidate_metrics) = metrics_map
        .iter()
        .max_by(|a, b| {
            a.1.score
                .partial_cmp(&b.1.score)
                .unwrap_or(std::cmp::Ordering::Equal)
                .then_with(|| b.0.cmp(a.0))
        })
        .context("no candidate profile")?;
    let current_metrics = metrics_map
        .get(&current_profile)
        .with_context(|| format!("current profile missing metrics: {current_profile}"))?;

    let hard_failure_current = current_metrics.cooldown_active
        || current_metrics.region_health_score < current_metrics.region_failover_threshold
        || current_metrics.seed_success_rate < current_metrics.seed_success_rate_threshold;

    let (selected_profile, action, reason, blocked_reason, blocked_by_cooldown) =
        if hard_failure_current {
            if candidate_profile != &current_profile {
                (
                    candidate_profile.clone(),
                    "switch".to_string(),
                    "current_hard_failure".to_string(),
                    String::new(),
                    false,
                )
            } else {
                (
                    current_profile.clone(),
                    "hold".to_string(),
                    "current_hard_failure_no_candidate".to_string(),
                    String::new(),
                    false,
                )
            }
        } else {
            let min_hold_ms = args.min_hold_seconds.saturating_mul(1000);
            if state.last_switch_unix_ms > 0
                && now_ms.saturating_sub(state.last_switch_unix_ms) < min_hold_ms
            {
                (
                    current_profile.clone(),
                    "hold".to_string(),
                    "min_hold_active".to_string(),
                    "min_hold_active".to_string(),
                    true,
                )
            } else if candidate_profile != &current_profile {
                let switchback_cooldown_ms = args.switchback_cooldown_seconds.saturating_mul(1000);
                if !state.previous_profile.trim().is_empty()
                    && state.previous_profile == *candidate_profile
                    && state.last_switch_unix_ms > 0
                    && now_ms.saturating_sub(state.last_switch_unix_ms) < switchback_cooldown_ms
                {
                    (
                        current_profile.clone(),
                        "hold".to_string(),
                        "switchback_cooldown".to_string(),
                        "switchback_cooldown".to_string(),
                        true,
                    )
                } else if candidate_metrics.score >= current_metrics.score + args.switch_margin {
                    (
                        candidate_profile.clone(),
                        "switch".to_string(),
                        "candidate_score_higher".to_string(),
                        String::new(),
                        false,
                    )
                } else {
                    (
                        current_profile.clone(),
                        "keep".to_string(),
                        "current_stable".to_string(),
                        String::new(),
                        false,
                    )
                }
            } else {
                (
                    current_profile.clone(),
                    "keep".to_string(),
                    "current_best".to_string(),
                    String::new(),
                    false,
                )
            }
        };

    let previous_profile = current_profile.clone();
    if action == "switch" {
        state.previous_profile = previous_profile.clone();
        state.current_profile = selected_profile.clone();
        state.last_switch_unix_ms = now_ms;
        state.last_switch_reason = reason.clone();
        if let Some(h) = state.profile_history.get_mut(&previous_profile) {
            h.last_switched_out_unix_ms = now_ms;
            h.last_rejected_reason = "switched_out".to_string();
        }
        if let Some(h) = state.profile_history.get_mut(&selected_profile) {
            h.last_selected_unix_ms = now_ms;
            h.last_rejected_reason.clear();
        }
    } else {
        state.current_profile = selected_profile.clone();
        for (name, history) in &mut state.profile_history {
            if name == &selected_profile {
                history.last_selected_unix_ms = now_ms;
                history.last_rejected_reason.clear();
            } else if name == candidate_profile && action == "hold" {
                history.last_rejected_reason = if blocked_reason.is_empty() {
                    reason.clone()
                } else {
                    blocked_reason.clone()
                };
            } else {
                history.last_rejected_reason = "score_lower".to_string();
            }
        }
    }

    if let Some(parent) = args.state_file.parent() {
        if !parent.as_os_str().is_empty() {
            fs::create_dir_all(parent)
                .with_context(|| format!("create state dir failed: {}", parent.display()))?;
        }
    }
    fs::write(
        &args.state_file,
        serde_json::to_vec_pretty(&state).context("serialize state failed")?,
    )
    .with_context(|| format!("write state failed: {}", args.state_file.display()))?;

    let selected_score = metrics_map
        .get(&selected_profile)
        .map(|v| v.score)
        .unwrap_or(0.0);
    let out = AutoProfileDecision {
        selected_profile,
        previous_profile,
        action,
        score: selected_score,
        reason,
        switch_blocked_by_cooldown: blocked_by_cooldown,
        switch_blocked_reason: blocked_reason,
        next_recheck_after_seconds: args.recheck_seconds,
        profile_scores,
    };
    print_success_json("overlay", "auto-profile-select", &out)
}
