#![forbid(unsafe_code)]

use anyhow::{bail, Context, Result};
use serde::{Deserialize, Serialize};
use serde_json::json;
use std::env;

use crate::output::print_success_json;

#[derive(Debug)]
struct Args {
    score: f64,
    base_concurrent: i32,
    base_pause: i32,
    yellow_concurrent: i32,
    yellow_pause: i32,
    red_block: bool,
    matrix_json: String,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
struct CircuitRule {
    #[serde(default)]
    name: String,
    #[serde(default)]
    min_score: f64,
    #[serde(default = "default_max_score")]
    max_score: f64,
    #[serde(default = "default_concurrent")]
    max_concurrent_plans: i32,
    #[serde(default)]
    dispatch_pause_seconds: i32,
    #[serde(default)]
    block_dispatch: bool,
}

fn default_max_score() -> f64 {
    101.0
}

fn default_concurrent() -> i32 {
    1
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

fn parse_args_from<I>(iter: I) -> Result<Args>
where
    I: IntoIterator<Item = String>,
{
    let mut args = Args {
        score: 100.0,
        base_concurrent: 1,
        base_pause: 0,
        yellow_concurrent: 1,
        yellow_pause: 3,
        red_block: true,
        matrix_json: "[]".to_string(),
    };
    let mut it = iter.into_iter();
    while let Some(flag) = it.next() {
        let val = it
            .next()
            .with_context(|| format!("missing value for {}", flag))?;
        match flag.as_str() {
            "--score" => args.score = val.parse().unwrap_or(100.0),
            "--base-concurrent" => args.base_concurrent = val.parse().unwrap_or(1),
            "--base-pause" => args.base_pause = val.parse().unwrap_or(0),
            "--yellow-concurrent" => args.yellow_concurrent = val.parse().unwrap_or(1),
            "--yellow-pause" => args.yellow_pause = val.parse().unwrap_or(3),
            "--red-block" => args.red_block = parse_bool(&val),
            "--matrix-json" => args.matrix_json = val,
            _ => bail!("unknown arg: {}", flag),
        }
    }
    args.score = clamp(args.score, 0.0, 100.0);
    args.base_concurrent = args.base_concurrent.max(1);
    args.base_pause = args.base_pause.max(0);
    args.yellow_concurrent = args.yellow_concurrent.max(1);
    args.yellow_pause = args.yellow_pause.max(0);
    Ok(args)
}

fn build_default_matrix(
    base_concurrent: i32,
    base_pause: i32,
    yellow_concurrent: i32,
    yellow_pause: i32,
    red_block: bool,
) -> Vec<CircuitRule> {
    let green_concurrent = base_concurrent.max(1);
    let green_pause = base_pause.max(0);
    let yellow_cc = yellow_concurrent.max(1).min(green_concurrent);
    let yellow_ps = yellow_pause.max(green_pause);
    let red_cc = yellow_cc.max(1);
    let red_ps = yellow_ps.max(green_pause);
    vec![
        CircuitRule {
            name: "green".to_string(),
            min_score: 95.0,
            max_score: 101.0,
            max_concurrent_plans: green_concurrent,
            dispatch_pause_seconds: green_pause,
            block_dispatch: false,
        },
        CircuitRule {
            name: "yellow".to_string(),
            min_score: 80.0,
            max_score: 95.0,
            max_concurrent_plans: yellow_cc,
            dispatch_pause_seconds: yellow_ps,
            block_dispatch: false,
        },
        CircuitRule {
            name: "red".to_string(),
            min_score: 0.0,
            max_score: 80.0,
            max_concurrent_plans: red_cc,
            dispatch_pause_seconds: red_ps,
            block_dispatch: red_block,
        },
    ]
}

fn sanitize_rules(
    raw: Vec<CircuitRule>,
    base_concurrent: i32,
    base_pause: i32,
) -> Vec<CircuitRule> {
    let mut out = Vec::new();
    for (idx, mut rule) in raw.into_iter().enumerate() {
        if rule.name.trim().is_empty() {
            rule.name = format!("rule-{}", idx + 1);
        }
        rule.min_score = clamp(rule.min_score, 0.0, 100.0);
        rule.max_score = clamp(rule.max_score.max(rule.min_score), 0.0, 101.0);
        rule.max_concurrent_plans = rule.max_concurrent_plans.max(base_concurrent).max(1);
        rule.dispatch_pause_seconds = rule.dispatch_pause_seconds.max(base_pause).max(0);
        out.push(rule);
    }
    out.sort_by(|a, b| {
        b.min_score
            .total_cmp(&a.min_score)
            .then_with(|| b.max_score.total_cmp(&a.max_score))
            .then_with(|| a.name.cmp(&b.name))
    });
    out
}

fn match_rule(rules: &[CircuitRule], score: f64) -> Option<&CircuitRule> {
    for rule in rules {
        if score >= rule.min_score && score < rule.max_score {
            return Some(rule);
        }
    }
    rules.last()
}

pub fn run_from_env() -> Result<()> {
    run_with_args(env::args().skip(1))
}

pub fn run_with_args<I>(iter: I) -> Result<()>
where
    I: IntoIterator<Item = String>,
{
    let args = parse_args_from(iter)?;
    let raw_rules: Vec<CircuitRule> = serde_json::from_str(&args.matrix_json).unwrap_or_default();
    let rules = if raw_rules.is_empty() {
        build_default_matrix(
            args.base_concurrent,
            args.base_pause,
            args.yellow_concurrent,
            args.yellow_pause,
            args.red_block,
        )
    } else {
        sanitize_rules(raw_rules, args.base_concurrent, args.base_pause)
    };
    let rule = match_rule(&rules, args.score)
        .cloned()
        .unwrap_or(CircuitRule {
            name: "fallback".to_string(),
            min_score: 0.0,
            max_score: 101.0,
            max_concurrent_plans: args.base_concurrent,
            dispatch_pause_seconds: args.base_pause,
            block_dispatch: false,
        });
    let out = json!({
        "rule": rule.name,
        "score": args.score,
        "min_score": rule.min_score,
        "max_score": rule.max_score,
        "max_concurrent_plans": rule.max_concurrent_plans,
        "dispatch_pause_seconds": rule.dispatch_pause_seconds,
        "block_dispatch": rule.block_dispatch,
        "reason": rule.name
    });
    print_success_json("risk", "circuit-breaker-evaluate", &out)
}
