#![forbid(unsafe_code)]

use anyhow::{bail, Context, Result};
use serde::{Deserialize, Serialize};
use serde_json::{json, Map, Value};
use std::env;

#[derive(Debug)]
struct Args {
    raw_rules_json: String,
    yellow_cap: i32,
    yellow_pause: i32,
    orange_cap: i32,
    orange_pause: i32,
    red_block: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct MatrixRule {
    source: String,
    level: String,
    cap_concurrent: i32,
    pause_seconds: i32,
    block_dispatch: bool,
    min_site_priority: i32,
}

#[derive(Debug, Clone)]
struct WorkingRule {
    cap_concurrent: i32,
    pause_seconds: i32,
    block_dispatch: bool,
    min_site_priority: i32,
    idx: Option<usize>,
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
    let mut out = Args {
        raw_rules_json: "[]".to_string(),
        yellow_cap: 1,
        yellow_pause: 0,
        orange_cap: 1,
        orange_pause: 0,
        red_block: false,
    };
    let mut it = iter.into_iter();
    while let Some(flag) = it.next() {
        let val = it
            .next()
            .with_context(|| format!("missing value for {}", flag))?;
        match flag.as_str() {
            "--raw-rules-json" => out.raw_rules_json = val,
            "--yellow-cap" => out.yellow_cap = val.parse().unwrap_or(1),
            "--yellow-pause" => out.yellow_pause = val.parse().unwrap_or(0),
            "--orange-cap" => out.orange_cap = val.parse().unwrap_or(1),
            "--orange-pause" => out.orange_pause = val.parse().unwrap_or(0),
            "--red-block" => out.red_block = parse_bool(&val),
            _ => bail!("unknown arg: {}", flag),
        }
    }
    Ok(out)
}

fn norm_level(raw: &str) -> Option<String> {
    let v = raw.trim().to_ascii_lowercase();
    match v.as_str() {
        "green" | "yellow" | "orange" | "red" => Some(v),
        _ => None,
    }
}

fn norm_source(raw: &str) -> Option<String> {
    let v = raw.trim().to_ascii_lowercase();
    if v.is_empty() {
        return Some("*".to_string());
    }
    match v.as_str() {
        "*" | "startup" | "cycle" => Some(v),
        _ => None,
    }
}

fn value_to_i32(v: Option<&Value>) -> Option<i32> {
    match v {
        Some(Value::Number(n)) => n.as_i64().map(|x| x as i32),
        Some(Value::String(s)) => s.trim().parse::<i32>().ok(),
        Some(Value::Bool(b)) => Some(if *b { 1 } else { 0 }),
        _ => None,
    }
}

fn value_to_bool(v: Option<&Value>) -> Option<bool> {
    match v {
        Some(Value::Bool(b)) => Some(*b),
        Some(Value::Number(n)) => Some(n.as_i64().unwrap_or(0) != 0),
        Some(Value::String(s)) => Some(parse_bool(s)),
        _ => None,
    }
}

fn parse_raw_rules(raw: &str) -> Vec<Map<String, Value>> {
    match serde_json::from_str::<Value>(raw) {
        Ok(Value::Array(arr)) => arr
            .into_iter()
            .filter_map(|v| match v {
                Value::Object(m) => Some(m),
                _ => None,
            })
            .collect(),
        _ => Vec::new(),
    }
}

fn find_fallback(
    rules: &[MatrixRule],
    source: &str,
    level: &str,
    min_target: i32,
) -> Option<WorkingRule> {
    if let Some((idx, it)) = rules.iter().enumerate().find(|(_, it)| {
        it.source == source && it.level == level && it.min_site_priority == min_target
    }) {
        return Some(WorkingRule {
            cap_concurrent: it.cap_concurrent,
            pause_seconds: it.pause_seconds,
            block_dispatch: it.block_dispatch,
            min_site_priority: it.min_site_priority,
            idx: Some(idx),
        });
    }

    let scans = [
        (source, level, true),
        ("*", level, true),
        (source, level, false),
        ("*", level, false),
    ];
    for (scan_source, scan_level, strict_min) in scans {
        if strict_min {
            if let Some(it) = rules.iter().find(|it| {
                it.source == scan_source
                    && it.level == scan_level
                    && it.min_site_priority == i32::MIN
            }) {
                return Some(WorkingRule {
                    cap_concurrent: it.cap_concurrent,
                    pause_seconds: it.pause_seconds,
                    block_dispatch: it.block_dispatch,
                    min_site_priority: min_target,
                    idx: None,
                });
            }
        } else if let Some(it) = rules
            .iter()
            .find(|it| it.source == scan_source && it.level == scan_level)
        {
            return Some(WorkingRule {
                cap_concurrent: it.cap_concurrent,
                pause_seconds: it.pause_seconds,
                block_dispatch: it.block_dispatch,
                min_site_priority: min_target,
                idx: None,
            });
        }
    }
    None
}

pub fn run_from_env() -> Result<()> {
    run_with_args(env::args().skip(1))
}

pub fn run_with_args<I>(iter: I) -> Result<()>
where
    I: IntoIterator<Item = String>,
{
    let args = parse_args_from(iter)?;
    let mut rules = vec![
        MatrixRule {
            source: "*".to_string(),
            level: "green".to_string(),
            cap_concurrent: 0,
            pause_seconds: 0,
            block_dispatch: false,
            min_site_priority: i32::MIN,
        },
        MatrixRule {
            source: "*".to_string(),
            level: "yellow".to_string(),
            cap_concurrent: args.yellow_cap.max(1),
            pause_seconds: args.yellow_pause.max(0),
            block_dispatch: false,
            min_site_priority: i32::MIN,
        },
        MatrixRule {
            source: "*".to_string(),
            level: "orange".to_string(),
            cap_concurrent: args.orange_cap.max(1),
            pause_seconds: args.orange_pause.max(0),
            block_dispatch: false,
            min_site_priority: i32::MIN,
        },
        MatrixRule {
            source: "*".to_string(),
            level: "red".to_string(),
            cap_concurrent: args.orange_cap.max(1),
            pause_seconds: args.orange_pause.max(0),
            block_dispatch: args.red_block,
            min_site_priority: i32::MIN,
        },
    ];

    for raw in parse_raw_rules(&args.raw_rules_json) {
        let level = match raw
            .get("level")
            .and_then(|v| v.as_str())
            .and_then(norm_level)
        {
            Some(v) => v,
            None => continue,
        };
        let source = match raw.get("source") {
            Some(v) => match v.as_str().and_then(norm_source) {
                Some(s) => s,
                None => continue,
            },
            None => "*".to_string(),
        };
        let min_target = value_to_i32(raw.get("min_site_priority")).unwrap_or(i32::MIN);
        let mut entry = match find_fallback(&rules, &source, &level, min_target) {
            Some(v) => v,
            None => continue,
        };
        if let Some(v) = value_to_i32(raw.get("cap_concurrent")) {
            entry.cap_concurrent = v.max(0);
        }
        if let Some(v) = value_to_i32(raw.get("pause_seconds")) {
            entry.pause_seconds = v.max(0);
        }
        if let Some(v) = value_to_bool(raw.get("block_dispatch")) {
            entry.block_dispatch = v;
        }
        if let Some(v) = value_to_i32(raw.get("min_site_priority")) {
            entry.min_site_priority = v;
        }
        let final_entry = MatrixRule {
            source,
            level,
            cap_concurrent: entry.cap_concurrent,
            pause_seconds: entry.pause_seconds,
            block_dispatch: entry.block_dispatch,
            min_site_priority: entry.min_site_priority,
        };
        if let Some(idx) = entry.idx {
            rules[idx] = final_entry;
        } else {
            rules.push(final_entry);
        }
    }

    println!(
        "{}",
        serde_json::to_string(&json!({ "matrix": rules })).context("serialize output failed")?
    );
    Ok(())
}
