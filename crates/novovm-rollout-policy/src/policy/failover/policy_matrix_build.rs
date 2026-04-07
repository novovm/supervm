#![forbid(unsafe_code)]

use anyhow::{bail, Context, Result};
use serde::Serialize;
use serde_json::{json, Map, Value};
use std::env;

#[derive(Debug)]
struct Args {
    raw_rules_json: String,
}

#[derive(Debug, Clone, Serialize)]
struct PolicyRule {
    order: i32,
    name: String,
    source: String,
    grades: Vec<String>,
    allow_auto_failover: bool,
    min_site_priority: i32,
    max_failover_count: i32,
    cooldown_seconds: i32,
    has_require_failover_mode: bool,
    require_failover_mode: bool,
}

fn parse_args(raw_args: &[String]) -> Result<Args> {
    let mut out = Args {
        raw_rules_json: "[]".to_string(),
    };
    let mut it = raw_args.iter();
    while let Some(flag) = it.next() {
        let val = it
            .next()
            .with_context(|| format!("missing value for {}", flag))?;
        match flag.as_str() {
            "--raw-rules-json" => out.raw_rules_json = val.clone(),
            _ => bail!("unknown arg: {}", flag),
        }
    }
    Ok(out)
}

fn parse_rules(raw: &str) -> Vec<Map<String, Value>> {
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

fn norm_source(raw: &str) -> String {
    let v = raw.trim().to_ascii_lowercase();
    if v.is_empty() {
        return "*".to_string();
    }
    match v.as_str() {
        "*" | "startup" | "cycle" => v,
        _ => "*".to_string(),
    }
}

fn norm_grade(raw: &str) -> Option<String> {
    let v = raw.trim().to_ascii_lowercase();
    match v.as_str() {
        "*" | "green" | "yellow" | "red" => Some(v),
        _ => None,
    }
}

fn value_to_bool(v: Option<&Value>) -> Option<bool> {
    match v {
        Some(Value::Bool(b)) => Some(*b),
        Some(Value::Number(n)) => Some(n.as_i64().unwrap_or(0) != 0),
        Some(Value::String(s)) => {
            let x = s.trim().to_ascii_lowercase();
            Some(matches!(x.as_str(), "1" | "true" | "yes" | "on"))
        }
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

fn value_to_string(v: Option<&Value>) -> String {
    match v {
        Some(Value::String(s)) => s.trim().to_string(),
        Some(Value::Number(n)) => n.to_string(),
        Some(Value::Bool(b)) => {
            if *b {
                "true".to_string()
            } else {
                "false".to_string()
            }
        }
        _ => String::new(),
    }
}

pub fn run_with_args(raw_args: &[String]) -> Result<()> {
    let args = parse_args(raw_args)?;
    let mut out_rules: Vec<PolicyRule> = Vec::new();
    let mut order: i32 = 0;

    for raw in parse_rules(&args.raw_rules_json) {
        order += 1;
        let mut name = value_to_string(raw.get("name"));
        if name.is_empty() {
            name = format!("rule-{}", order);
        }
        let source = norm_source(&value_to_string(raw.get("source")));
        let allow_auto_failover = value_to_bool(raw.get("allow_auto_failover")).unwrap_or(true);
        let min_site_priority = value_to_i32(raw.get("min_site_priority")).unwrap_or(i32::MIN);
        let max_failover_count = value_to_i32(raw.get("max_failover_count"))
            .map(|v| v.max(0))
            .unwrap_or(-1);
        let cooldown_seconds = value_to_i32(raw.get("cooldown_seconds"))
            .map(|v| v.max(1))
            .unwrap_or(-1);
        let has_require_failover_mode = raw.get("require_failover_mode").is_some();
        let require_failover_mode =
            value_to_bool(raw.get("require_failover_mode")).unwrap_or(false);

        let mut grades: Vec<String> = Vec::new();
        match raw.get("grades") {
            Some(Value::Array(arr)) => {
                for g in arr {
                    if let Some(gs) = g.as_str().and_then(norm_grade) {
                        grades.push(gs);
                    }
                }
            }
            _ => {
                if let Some(gs) = raw
                    .get("grade")
                    .and_then(|v| v.as_str())
                    .and_then(norm_grade)
                {
                    grades.push(gs);
                }
            }
        }
        if grades.is_empty() {
            grades.push("*".to_string());
        }

        out_rules.push(PolicyRule {
            order,
            name,
            source,
            grades,
            allow_auto_failover,
            min_site_priority,
            max_failover_count,
            cooldown_seconds,
            has_require_failover_mode,
            require_failover_mode,
        });
    }

    println!(
        "{}",
        serde_json::to_string(&json!({ "matrix": out_rules }))
            .context("serialize output failed")?
    );
    Ok(())
}

pub fn run_cli() -> Result<()> {
    let raw_args: Vec<String> = env::args().skip(1).collect();
    run_with_args(&raw_args)
}
