#![forbid(unsafe_code)]

use anyhow::{bail, Context, Result};
use serde::{Deserialize, Serialize};
use serde_json::json;
use std::env;
use std::fs;
use std::path::Path;
use std::time::{SystemTime, UNIX_EPOCH};

use crate::output::print_success_json;

#[derive(Debug)]
struct Args {
    state_file: String,
    grade: String,
    window_samples: usize,
    min_green_rate: f64,
    max_red_in_window: usize,
    block_on_violation: bool,
    now_unix_ms: u64,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
struct SloSample {
    #[serde(default)]
    grade: String,
    #[serde(default)]
    observed_at_unix_ms: u64,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
struct SloLast {
    #[serde(default)]
    total: usize,
    #[serde(default)]
    green: usize,
    #[serde(default)]
    yellow: usize,
    #[serde(default)]
    red: usize,
    #[serde(default)]
    green_rate: f64,
    #[serde(default)]
    score: f64,
    #[serde(default)]
    violation: bool,
    #[serde(default)]
    reason: String,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
struct SloState {
    #[serde(default = "one")]
    version: u64,
    #[serde(default)]
    updated_at: String,
    #[serde(default)]
    samples: Vec<SloSample>,
    #[serde(default)]
    window_samples: usize,
    #[serde(default)]
    min_green_rate: f64,
    #[serde(default)]
    max_red_in_window: usize,
    #[serde(default)]
    last: SloLast,
}

fn one() -> u64 {
    1
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

fn parse_bool(raw: &str) -> bool {
    matches!(
        raw.trim().to_ascii_lowercase().as_str(),
        "1" | "true" | "yes" | "on"
    )
}

fn normalize_grade(raw: &str) -> String {
    match raw.trim().to_ascii_lowercase().as_str() {
        "green" => "green".to_string(),
        "red" => "red".to_string(),
        "yellow" | "orange" => "yellow".to_string(),
        _ => "yellow".to_string(),
    }
}

fn parse_args_from<I>(iter: I) -> Result<Args>
where
    I: IntoIterator<Item = String>,
{
    let mut args = Args {
        state_file: String::new(),
        grade: "yellow".to_string(),
        window_samples: 60,
        min_green_rate: 0.95,
        max_red_in_window: 0,
        block_on_violation: false,
        now_unix_ms: now_ms(),
    };
    let mut it = iter.into_iter();
    while let Some(flag) = it.next() {
        let val = it
            .next()
            .with_context(|| format!("missing value for {}", flag))?;
        match flag.as_str() {
            "--state-file" => args.state_file = val,
            "--grade" => args.grade = val,
            "--window-samples" => args.window_samples = val.parse().unwrap_or(60),
            "--min-green-rate" => args.min_green_rate = val.parse().unwrap_or(0.95),
            "--max-red-in-window" => args.max_red_in_window = val.parse().unwrap_or(0),
            "--block-on-violation" => args.block_on_violation = parse_bool(&val),
            "--now-unix-ms" => args.now_unix_ms = val.parse().unwrap_or_else(|_| now_ms()),
            _ => bail!("unknown arg: {}", flag),
        }
    }
    if args.state_file.trim().is_empty() {
        bail!("--state-file is required");
    }
    args.grade = normalize_grade(&args.grade);
    args.window_samples = args.window_samples.max(5);
    args.min_green_rate = clamp(args.min_green_rate, 0.0, 1.0);
    Ok(args)
}

fn load_state(path: &str) -> SloState {
    match fs::read_to_string(path) {
        Ok(raw) => serde_json::from_str::<SloState>(&raw).unwrap_or_default(),
        Err(_) => SloState::default(),
    }
}

fn save_state(path: &str, state: &SloState) -> Result<()> {
    let p = Path::new(path);
    if let Some(parent) = p.parent() {
        if !parent.as_os_str().is_empty() {
            fs::create_dir_all(parent)
                .with_context(|| format!("create dir failed: {}", parent.display()))?;
        }
    }
    fs::write(
        p,
        serde_json::to_vec_pretty(state).context("serialize state failed")?,
    )
    .with_context(|| format!("write state failed: {}", p.display()))?;
    Ok(())
}

pub fn run_from_env() -> Result<()> {
    run_with_args(env::args().skip(1))
}

pub fn run_with_args<I>(iter: I) -> Result<()>
where
    I: IntoIterator<Item = String>,
{
    let args = parse_args_from(iter)?;
    let mut state = load_state(&args.state_file);
    state.samples.push(SloSample {
        grade: args.grade.clone(),
        observed_at_unix_ms: args.now_unix_ms,
    });
    if state.samples.len() > args.window_samples {
        let keep_from = state.samples.len() - args.window_samples;
        state.samples = state.samples.split_off(keep_from);
    }

    let mut green = 0usize;
    let mut yellow = 0usize;
    let mut red = 0usize;
    for sample in &state.samples {
        match normalize_grade(&sample.grade).as_str() {
            "green" => green += 1,
            "red" => red += 1,
            _ => yellow += 1,
        }
    }
    let total = state.samples.len();
    let green_rate = if total == 0 {
        1.0
    } else {
        green as f64 / total as f64
    };
    let score = if total == 0 {
        100.0
    } else {
        ((green as f64 * 100.0) + (yellow as f64 * 60.0)) / total as f64
    };
    let mut reasons = Vec::new();
    if green_rate < args.min_green_rate {
        reasons.push(format!(
            "green_rate={:.4} < min={}",
            green_rate, args.min_green_rate
        ));
    }
    if red > args.max_red_in_window {
        reasons.push(format!(
            "red_count={} > max={}",
            red, args.max_red_in_window
        ));
    }
    let violation = !reasons.is_empty();
    let reason = reasons.join("; ");

    state.updated_at = args.now_unix_ms.to_string();
    state.window_samples = args.window_samples;
    state.min_green_rate = args.min_green_rate;
    state.max_red_in_window = args.max_red_in_window;
    state.last = SloLast {
        total,
        green,
        yellow,
        red,
        green_rate: (green_rate * 1_000_000.0).round() / 1_000_000.0,
        score: (score * 10_000.0).round() / 10_000.0,
        violation,
        reason: reason.clone(),
    };
    save_state(&args.state_file, &state)?;

    let out = json!({
        "grade": args.grade,
        "total": total,
        "green": green,
        "yellow": yellow,
        "red": red,
        "green_rate": state.last.green_rate,
        "score": state.last.score,
        "violation": violation,
        "reason": reason,
        "block_dispatch": args.block_on_violation && violation
    });
    print_success_json("risk", "slo-evaluate", &out)
}
