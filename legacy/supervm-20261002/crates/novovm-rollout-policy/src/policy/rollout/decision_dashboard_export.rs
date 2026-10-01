#![forbid(unsafe_code)]

use anyhow::{bail, Context, Result};
use serde_json::{Map, Value};
use std::env;
use std::fs;
use std::path::Path;

#[derive(Debug)]
struct Args {
    audit_file: String,
    output_file: String,
    mode: String,
    tail: usize,
    since_utc: String,
}

fn normalize_mode(raw: &str) -> String {
    let m = raw.trim().to_ascii_lowercase();
    if m == "delivery" || m == "summary" || m == "both" {
        m
    } else {
        "both".to_string()
    }
}

fn parse_args(raw_args: &[String]) -> Result<Args> {
    let mut out = Args {
        audit_file: "artifacts/runtime/rollout/control-plane-audit.jsonl".to_string(),
        output_file: "artifacts/runtime/rollout/control-plane-decision-dashboard.jsonl".to_string(),
        mode: "both".to_string(),
        tail: 0,
        since_utc: String::new(),
    };
    let mut it = raw_args.iter();
    while let Some(flag) = it.next() {
        let val = it
            .next()
            .with_context(|| format!("missing value for {}", flag))?;
        match flag.as_str() {
            "--audit-file" => out.audit_file = val.clone(),
            "--output-file" => out.output_file = val.clone(),
            "--mode" => out.mode = normalize_mode(val),
            "--tail" => out.tail = val.parse().unwrap_or(0),
            "--since-utc" => out.since_utc = val.trim().to_string(),
            _ => bail!("unknown arg: {}", flag),
        }
    }
    out.mode = normalize_mode(&out.mode);
    Ok(out)
}

fn should_include_result(result: &str, mode: &str) -> bool {
    let r = result.trim().to_ascii_lowercase();
    if mode == "delivery" {
        return r == "rollout_decision_delivery";
    }
    if mode == "summary" {
        return r == "rollout_decision_summary";
    }
    r == "rollout_decision_delivery" || r == "rollout_decision_summary"
}

fn s(v: Option<&Value>) -> String {
    match v {
        Some(Value::String(x)) => x.to_string(),
        Some(Value::Number(x)) => x.to_string(),
        Some(Value::Bool(x)) => {
            if *x {
                "true".to_string()
            } else {
                "false".to_string()
            }
        }
        _ => String::new(),
    }
}

fn b(v: Option<&Value>) -> bool {
    match v {
        Some(Value::Bool(x)) => *x,
        Some(Value::Number(x)) => x.as_i64().unwrap_or(0) != 0,
        Some(Value::String(x)) => {
            let t = x.trim().to_ascii_lowercase();
            matches!(t.as_str(), "1" | "true" | "yes" | "on")
        }
        _ => false,
    }
}

fn i(v: Option<&Value>) -> i64 {
    match v {
        Some(Value::Number(x)) => x.as_i64().unwrap_or(0),
        Some(Value::String(x)) => x.parse::<i64>().unwrap_or(0),
        Some(Value::Bool(x)) => {
            if *x {
                1
            } else {
                0
            }
        }
        _ => 0,
    }
}

fn f(v: Option<&Value>) -> f64 {
    match v {
        Some(Value::Number(x)) => x.as_f64().unwrap_or(0.0),
        Some(Value::String(x)) => x.parse::<f64>().unwrap_or(0.0),
        Some(Value::Bool(x)) => {
            if *x {
                1.0
            } else {
                0.0
            }
        }
        _ => 0.0,
    }
}

fn read_lines(path: &str) -> Result<Vec<String>> {
    if !Path::new(path).exists() {
        bail!("audit file not found: {}", path);
    }
    let raw = fs::read_to_string(path).with_context(|| format!("read file failed: {}", path))?;
    Ok(raw.lines().map(|v| v.to_string()).collect())
}

pub fn run_with_args(raw_args: &[String]) -> Result<()> {
    let args = parse_args(raw_args)?;
    let mut lines = read_lines(&args.audit_file)?;
    if args.tail > 0 && lines.len() > args.tail {
        lines = lines.split_off(lines.len() - args.tail);
    }

    let mut out_lines: Vec<String> = Vec::new();
    let mut accepted: u64 = 0;
    let mut skipped: u64 = 0;

    for raw in lines {
        if raw.trim().is_empty() {
            continue;
        }
        let evt: Value = match serde_json::from_str(&raw) {
            Ok(v) => v,
            Err(_) => {
                skipped = skipped.saturating_add(1);
                continue;
            }
        };
        let obj = match evt.as_object() {
            Some(v) => v,
            None => {
                skipped = skipped.saturating_add(1);
                continue;
            }
        };
        let result = s(obj.get("result"));
        if !should_include_result(&result, &args.mode) {
            continue;
        }
        let ts = s(obj.get("timestamp_utc"));
        if !args.since_utc.is_empty() && !ts.is_empty() && ts < args.since_utc {
            continue;
        }

        let mut normalized = Map::new();
        normalized.insert("timestamp_utc".to_string(), Value::String(ts));
        normalized.insert("event_result".to_string(), Value::String(result));
        normalized.insert("source".to_string(), Value::String(s(obj.get("source"))));
        normalized.insert("action".to_string(), Value::String(s(obj.get("action"))));
        normalized.insert(
            "control_operation_id".to_string(),
            Value::String(s(obj.get("control_operation_id"))),
        );
        normalized.insert(
            "controller_id".to_string(),
            Value::String(s(obj.get("controller_id"))),
        );
        normalized.insert(
            "queue_file".to_string(),
            Value::String(s(obj.get("queue_file"))),
        );
        normalized.insert(
            "decision_alert_level".to_string(),
            Value::String(s(obj.get("decision_alert_level"))),
        );
        normalized.insert(
            "decision_alert_channel".to_string(),
            Value::String(s(obj.get("decision_alert_channel"))),
        );
        normalized.insert(
            "decision_alert_target".to_string(),
            Value::String(s(obj.get("decision_alert_target"))),
        );
        normalized.insert(
            "decision_delivery_type".to_string(),
            Value::String(s(obj.get("decision_delivery_type"))),
        );
        normalized.insert(
            "decision_delivery_action".to_string(),
            Value::String(s(obj.get("decision_delivery_action"))),
        );
        normalized.insert(
            "delivery_status".to_string(),
            Value::String(s(obj.get("delivery_status"))),
        );
        normalized.insert(
            "delivery_ok".to_string(),
            Value::Bool(b(obj.get("delivery_ok"))),
        );
        normalized.insert(
            "dispatch_blocked".to_string(),
            Value::Bool(b(obj.get("dispatch_blocked"))),
        );
        normalized.insert(
            "effective_max_concurrent".to_string(),
            Value::from(i(obj.get("effective_max_concurrent"))),
        );
        normalized.insert(
            "effective_pause_seconds".to_string(),
            Value::from(i(obj.get("effective_pause_seconds"))),
        );
        normalized.insert(
            "worst_site_id".to_string(),
            Value::String(s(obj.get("worst_site_id"))),
        );
        normalized.insert(
            "worst_level".to_string(),
            Value::String(s(obj.get("worst_level"))),
        );
        normalized.insert(
            "worst_score".to_string(),
            Value::from(f(obj.get("worst_score"))),
        );
        normalized.insert(
            "risk_policy_active_profile".to_string(),
            Value::String(s(obj.get("risk_policy_active_profile"))),
        );
        normalized.insert(
            "failover_converge_scope".to_string(),
            Value::String(s(obj.get("failover_converge_scope"))),
        );
        normalized.insert(
            "failover_converge_config_enabled".to_string(),
            Value::Bool(b(obj.get("failover_converge_config_enabled"))),
        );
        normalized.insert(
            "failover_converge_enabled".to_string(),
            Value::Bool(b(obj.get("failover_converge_enabled"))),
        );
        normalized.insert(
            "failover_converge_max_concurrent".to_string(),
            Value::from(i(obj.get("failover_converge_max_concurrent"))),
        );
        normalized.insert(
            "failover_converge_min_dispatch_pause_seconds".to_string(),
            Value::from(i(obj.get("failover_converge_min_dispatch_pause_seconds"))),
        );
        normalized.insert(
            "failover_converge_block_on_snapshot_red".to_string(),
            Value::Bool(b(obj.get("failover_converge_block_on_snapshot_red"))),
        );
        normalized.insert(
            "failover_converge_block_on_replay_red".to_string(),
            Value::Bool(b(obj.get("failover_converge_block_on_replay_red"))),
        );
        normalized.insert("error".to_string(), Value::String(s(obj.get("error"))));

        out_lines.push(
            serde_json::to_string(&Value::Object(normalized)).context("serialize line failed")?,
        );
        accepted = accepted.saturating_add(1);
    }

    if let Some(parent) = Path::new(&args.output_file).parent() {
        if !parent.as_os_str().is_empty() {
            fs::create_dir_all(parent)
                .with_context(|| format!("create output dir failed: {}", parent.display()))?;
        }
    }
    let mut output_text = String::new();
    if !out_lines.is_empty() {
        output_text = out_lines.join("\n");
        output_text.push('\n');
    }
    fs::write(&args.output_file, output_text)
        .with_context(|| format!("write output failed: {}", args.output_file))?;

    println!(
        "decision_dashboard_export_done: accepted={} skipped={} output={}",
        accepted, skipped, args.output_file
    );
    Ok(())
}

pub fn run_cli() -> Result<()> {
    let raw_args: Vec<String> = env::args().skip(1).collect();
    run_with_args(&raw_args)
}
