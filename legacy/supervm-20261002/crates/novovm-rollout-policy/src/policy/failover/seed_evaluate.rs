#![forbid(unsafe_code)]

use super::{record_seed_failure, record_seed_success, refresh_seed_gate, SeedFailoverState};
use anyhow::{bail, Context, Result};
use std::env;
use std::fs;
use std::path::Path;
use std::time::{SystemTime, UNIX_EPOCH};

#[derive(Debug)]
struct Args {
    state_file: String,
    seed: String,
    event: String,
    success_rate_threshold: f64,
    cooldown_seconds: u64,
    max_consecutive_failures: u64,
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

fn norm_source(raw: &str) -> String {
    let t = raw.trim();
    if t.is_empty() {
        "__empty__".to_string()
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
        seed: String::new(),
        event: "gate".to_string(),
        success_rate_threshold: 0.5,
        cooldown_seconds: 120,
        max_consecutive_failures: 3,
        now_unix_ms: now_ms(),
    };
    let mut it = iter.into_iter();
    while let Some(flag) = it.next() {
        let val = it
            .next()
            .with_context(|| format!("missing value for {}", flag))?;
        match flag.as_str() {
            "--state-file" => args.state_file = val,
            "--seed" | "--source" => args.seed = val,
            "--event" => args.event = val.trim().to_ascii_lowercase(),
            "--success-rate-threshold" => args.success_rate_threshold = val.parse().unwrap_or(0.5),
            "--cooldown-seconds" => args.cooldown_seconds = val.parse().unwrap_or(120),
            "--max-consecutive-failures" => {
                args.max_consecutive_failures = val.parse().unwrap_or(3)
            }
            "--now-unix-ms" => args.now_unix_ms = val.parse().unwrap_or_else(|_| now_ms()),
            _ => bail!("unknown arg: {}", flag),
        }
    }
    if args.state_file.trim().is_empty() {
        bail!("--state-file is required");
    }
    if args.seed.trim().is_empty() {
        bail!("--seed is required");
    }
    if !matches!(args.event.as_str(), "gate" | "success" | "fail") {
        bail!("--event must be gate|success|fail");
    }
    args.success_rate_threshold = clamp(args.success_rate_threshold, 0.0, 1.0);
    args.cooldown_seconds = clamp_u64(args.cooldown_seconds, 1, 86400);
    args.max_consecutive_failures = clamp_u64(args.max_consecutive_failures, 1, 100);
    Ok(args)
}

fn load_json(path: &str) -> SeedFailoverState {
    match fs::read_to_string(path) {
        Ok(raw) => serde_json::from_str::<SeedFailoverState>(&raw).unwrap_or_default(),
        Err(_) => SeedFailoverState::default(),
    }
}

fn save_json(path: &str, obj: &SeedFailoverState) -> Result<()> {
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
    let seed_key = norm_source(&args.seed);
    let mut state = load_json(&args.state_file);
    let (failover_reason, recover_at, success_rate, allowed, success, fail, consecutive_failures) = {
        let rec = state.sources.entry(seed_key.clone()).or_default();
        let gate = refresh_seed_gate(now, rec);
        let mut failover_reason = if gate.available {
            String::new()
        } else {
            gate.reason.clone()
        };
        let mut recover_at = gate.recover_at_unix_ms;
        let mut success_rate = {
            let total = rec.success.saturating_add(rec.fail);
            if total == 0 {
                1.0
            } else {
                rec.success as f64 / total as f64
            }
        };
        match args.event.as_str() {
            "success" => {
                record_seed_success(now, rec);
                failover_reason.clear();
                recover_at = 0;
                success_rate = {
                    let total = rec.success.saturating_add(rec.fail);
                    if total == 0 {
                        1.0
                    } else {
                        rec.success as f64 / total as f64
                    }
                };
            }
            "fail" if gate.available => {
                let outcome = record_seed_failure(
                    now,
                    rec,
                    args.success_rate_threshold,
                    args.cooldown_seconds,
                    args.max_consecutive_failures,
                );
                success_rate = outcome.success_rate;
                if outcome.degraded {
                    failover_reason = outcome.reason;
                    recover_at = outcome.recover_at_unix_ms;
                }
            }
            _ => {}
        }
        (
            failover_reason,
            recover_at,
            success_rate,
            rec.degraded_until_unix_ms <= now,
            rec.success,
            rec.fail,
            rec.consecutive_failures,
        )
    };
    state.updated_at = now.to_string();
    save_json(&args.state_file, &state)?;
    let failover_reason_out = if failover_reason.is_empty() {
        "none"
    } else {
        failover_reason.as_str()
    };
    println!(
        "failover_seed_evaluate_done: seed={} event={} seed_allowed={} seed_success={} seed_fail={} seed_success_rate={} seed_consecutive_failures={} seed_failover_reason={} seed_recover_at_unix_ms={}",
        seed_key,
        args.event,
        if allowed { "true" } else { "false" },
        success,
        fail,
        fmt_score(success_rate),
        consecutive_failures,
        failover_reason_out,
        recover_at
    );
    Ok(())
}
