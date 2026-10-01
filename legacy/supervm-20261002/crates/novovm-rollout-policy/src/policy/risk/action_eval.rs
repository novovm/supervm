#![forbid(unsafe_code)]

use anyhow::{bail, Context, Result};
use serde::{Deserialize, Serialize};
use serde_json::json;
use std::env;

#[derive(Debug)]
struct Args {
    source: String,
    level: String,
    site_priority: i32,
    scope: String,
    matrix_json: String,
}

#[derive(Debug, Clone, Deserialize)]
struct MatrixRule {
    #[serde(default)]
    source: String,
    #[serde(default)]
    level: String,
    #[serde(default)]
    cap_concurrent: i32,
    #[serde(default)]
    pause_seconds: i32,
    #[serde(default)]
    block_dispatch: bool,
    #[serde(default = "default_min_site_priority")]
    min_site_priority: i32,
}

fn default_min_site_priority() -> i32 {
    i32::MIN
}

#[derive(Debug, Clone, Serialize)]
struct EvalResult {
    level: String,
    cap_concurrent: i32,
    pause_seconds: i32,
    block_dispatch: bool,
    scope: String,
    rule_source: String,
    min_site_priority: i32,
    priority_gate: String,
}

fn parse_args_from<I>(iter: I) -> Result<Args>
where
    I: IntoIterator<Item = String>,
{
    let mut out = Args {
        source: "cycle".to_string(),
        level: "green".to_string(),
        site_priority: 0,
        scope: "global".to_string(),
        matrix_json: "[]".to_string(),
    };
    let mut it = iter.into_iter();
    while let Some(flag) = it.next() {
        let val = it
            .next()
            .with_context(|| format!("missing value for {}", flag))?;
        match flag.as_str() {
            "--source" => out.source = val,
            "--level" => out.level = val,
            "--site-priority" => out.site_priority = val.parse().unwrap_or(0),
            "--scope" => out.scope = val,
            "--matrix-json" => out.matrix_json = val,
            _ => bail!("unknown arg: {}", flag),
        }
    }
    Ok(out)
}

fn norm_source(raw: &str) -> String {
    let s = raw.trim().to_ascii_lowercase();
    if s == "startup" || s == "cycle" {
        s
    } else {
        "cycle".to_string()
    }
}

fn norm_level(raw: &str) -> String {
    let s = raw.trim().to_ascii_lowercase();
    if s == "green" || s == "yellow" || s == "orange" || s == "red" {
        s
    } else {
        "green".to_string()
    }
}

fn sanitize_rule(mut r: MatrixRule) -> MatrixRule {
    r.source = if r.source.trim().is_empty() {
        "*".to_string()
    } else {
        r.source.trim().to_ascii_lowercase()
    };
    if r.source != "*" && r.source != "startup" && r.source != "cycle" {
        r.source = "*".to_string();
    }
    r.level = norm_level(&r.level);
    r.cap_concurrent = r.cap_concurrent.max(0);
    r.pause_seconds = r.pause_seconds.max(0);
    r
}

fn pick_best(candidates: &[MatrixRule]) -> Option<MatrixRule> {
    if candidates.is_empty() {
        return None;
    }
    let mut best = candidates[0].clone();
    for c in candidates.iter().skip(1) {
        if c.min_site_priority > best.min_site_priority {
            best = c.clone();
        }
    }
    Some(best)
}

pub fn run_from_env() -> Result<()> {
    run_with_args(env::args().skip(1))
}

pub fn run_with_args<I>(iter: I) -> Result<()>
where
    I: IntoIterator<Item = String>,
{
    let args = parse_args_from(iter)?;
    let source_norm = norm_source(&args.source);
    let level_norm = norm_level(&args.level);
    let matrix_raw: Vec<MatrixRule> = serde_json::from_str(&args.matrix_json).unwrap_or_default();
    let matrix: Vec<MatrixRule> = matrix_raw.into_iter().map(sanitize_rule).collect();

    let mut source_candidates: Vec<MatrixRule> = Vec::new();
    let mut wildcard_candidates: Vec<MatrixRule> = Vec::new();
    let mut fallback_entry: Option<MatrixRule> = None;

    for it in &matrix {
        if it.level != level_norm {
            continue;
        }
        if it.source == source_norm {
            if fallback_entry.is_none() {
                fallback_entry = Some(it.clone());
            }
            if it.min_site_priority <= args.site_priority {
                source_candidates.push(it.clone());
            }
            continue;
        }
        if it.source == "*" {
            if fallback_entry.is_none() {
                fallback_entry = Some(it.clone());
            }
            if it.min_site_priority <= args.site_priority {
                wildcard_candidates.push(it.clone());
            }
        }
    }

    let mut result = EvalResult {
        level: "green".to_string(),
        cap_concurrent: 0,
        pause_seconds: 0,
        block_dispatch: false,
        scope: args.scope,
        rule_source: String::new(),
        min_site_priority: i32::MIN,
        priority_gate: "disabled".to_string(),
    };

    if let Some(entry) = pick_best(&source_candidates).or_else(|| pick_best(&wildcard_candidates)) {
        result.level = entry.level;
        result.cap_concurrent = entry.cap_concurrent.max(0);
        result.pause_seconds = entry.pause_seconds.max(0);
        result.block_dispatch = entry.block_dispatch;
        result.rule_source = entry.source;
        result.min_site_priority = entry.min_site_priority;
        result.priority_gate = "pass".to_string();
    } else if let Some(fallback) = fallback_entry {
        result.level = fallback.level;
        result.cap_concurrent = fallback.cap_concurrent.max(0);
        result.pause_seconds = fallback.pause_seconds.max(0);
        result.block_dispatch = true;
        result.rule_source = fallback.source;
        result.min_site_priority = fallback.min_site_priority;
        result.priority_gate = "blocked_by_site_priority".to_string();
    }

    println!(
        "{}",
        serde_json::to_string(&json!(result)).context("serialize output failed")?
    );
    Ok(())
}
