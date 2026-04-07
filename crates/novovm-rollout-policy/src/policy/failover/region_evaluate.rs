#![forbid(unsafe_code)]

use super::{evaluate_region_score, refresh_region_gate, RegionFailoverState};
use anyhow::{bail, Context, Result};
use std::env;
use std::fs;
use std::path::Path;
use std::time::{SystemTime, UNIX_EPOCH};

#[derive(Debug)]
struct Args {
    state_file: String,
    region: String,
    region_score: f64,
    failover_threshold: f64,
    cooldown_seconds: u64,
    now_unix_ms: u64,
}

fn now_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|v| v.as_millis() as u64)
        .unwrap_or(0)
}

fn clamp(v: f64, min: f64, max: f64) -> f64 {
    v.clamp(min, max)
}

fn clamp_u64(v: u64, min: u64, max: u64) -> u64 {
    v.clamp(min, max)
}

fn norm_region(raw: &str) -> String {
    let t = raw.trim();
    if t.is_empty() {
        "global".to_string()
    } else {
        t.to_ascii_lowercase()
    }
}

fn parse_args_from<I>(iter: I) -> Result<Args>
where
    I: IntoIterator<Item = String>,
{
    let mut args = Args {
        state_file: String::new(),
        region: String::new(),
        region_score: 0.0,
        failover_threshold: 0.5,
        cooldown_seconds: 120,
        now_unix_ms: now_ms(),
    };
    let mut it = iter.into_iter();
    while let Some(flag) = it.next() {
        let val = it
            .next()
            .with_context(|| format!("missing value for {}", flag))?;
        match flag.as_str() {
            "--state-file" => args.state_file = val,
            "--region" => args.region = val,
            "--region-score" => args.region_score = val.parse().unwrap_or(0.0),
            "--region-failover-threshold" => args.failover_threshold = val.parse().unwrap_or(0.5),
            "--region-cooldown-seconds" => args.cooldown_seconds = val.parse().unwrap_or(120),
            "--now-unix-ms" => args.now_unix_ms = val.parse().unwrap_or_else(|_| now_ms()),
            _ => bail!("unknown arg: {}", flag),
        }
    }
    if args.state_file.trim().is_empty() {
        bail!("--state-file is required");
    }
    if args.region.trim().is_empty() {
        bail!("--region is required");
    }
    args.region_score = clamp(args.region_score, 0.0, 1.0);
    args.failover_threshold = clamp(args.failover_threshold, 0.0, 1.0);
    args.cooldown_seconds = clamp_u64(args.cooldown_seconds, 1, 86400);
    Ok(args)
}

fn load_json(path: &str) -> RegionFailoverState {
    match fs::read_to_string(path) {
        Ok(raw) => serde_json::from_str::<RegionFailoverState>(&raw).unwrap_or_default(),
        Err(_) => RegionFailoverState::default(),
    }
}

fn save_json(path: &str, obj: &RegionFailoverState) -> Result<()> {
    let p = Path::new(path);
    if let Some(parent) = p.parent() {
        if !parent.as_os_str().is_empty() {
            fs::create_dir_all(parent)
                .with_context(|| format!("create dir failed: {}", parent.display()))?;
        }
    }
    fs::write(
        p,
        serde_json::to_vec_pretty(obj).context("serialize json failed")?,
    )
    .with_context(|| format!("write state file failed: {}", p.display()))?;
    Ok(())
}

fn fmt_score(v: f64) -> String {
    let mut s = format!("{:.4}", v);
    while s.contains('.') && s.ends_with('0') {
        s.pop();
    }
    if s.ends_with('.') {
        s.pop();
    }
    if s.is_empty() {
        "0".to_string()
    } else {
        s
    }
}

pub fn run_from_env() -> Result<()> {
    run_with_args(env::args().skip(1))
}

pub fn run_with_args<I>(iter: I) -> Result<()>
where
    I: IntoIterator<Item = String>,
{
    let args = parse_args_from(iter)?;
    let now = args.now_unix_ms;
    let region_key = norm_region(&args.region);
    let mut state = load_json(&args.state_file);
    let (outcome, active, success, fail, consecutive_failures) = {
        let rec = state.regions.entry(region_key.clone()).or_default();
        refresh_region_gate(now, rec);
        let outcome = evaluate_region_score(
            now,
            rec,
            args.region_score,
            args.failover_threshold,
            args.cooldown_seconds,
        );
        (
            outcome,
            rec.degraded_until_unix_ms <= now,
            rec.success,
            rec.fail,
            rec.consecutive_failures,
        )
    };
    state.updated_at = now.to_string();
    save_json(&args.state_file, &state)?;
    let failover_reason_out = if outcome.reason.is_empty() {
        "none"
    } else {
        outcome.reason.as_str()
    };
    println!(
        "failover_region_evaluate_done: region={} region_score={} region_active={} region_success={} region_fail={} region_consecutive_failures={} region_failover_reason={} region_recover_at_unix_ms={}",
        region_key,
        fmt_score(outcome.average_score),
        if active { "true" } else { "false" },
        success,
        fail,
        consecutive_failures,
        failover_reason_out,
        outcome.recover_at_unix_ms
    );
    Ok(())
}
