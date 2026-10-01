#![forbid(unsafe_code)]

use anyhow::{bail, Context, Result};
use chrono::Utc;
use serde_json::{Map, Value};
use std::collections::BTreeMap;
use std::env;
use std::fs;
use std::path::Path;

#[derive(Debug)]
struct Args {
    input_file: String,
    output_file: String,
    alerts_file: String,
    mode: String,
    tail: usize,
}

fn normalize_mode(raw: &str) -> String {
    let m = raw.trim().to_ascii_lowercase();
    if m == "all" || m == "blocked" {
        m
    } else {
        "all".to_string()
    }
}

fn parse_args(raw_args: &[String]) -> Result<Args> {
    let mut out = Args {
        input_file: "artifacts/runtime/rollout/control-plane-decision-dashboard.jsonl".to_string(),
        output_file: "artifacts/runtime/rollout/control-plane-decision-dashboard-state.json"
            .to_string(),
        alerts_file: "artifacts/runtime/rollout/control-plane-decision-dashboard-alerts.jsonl"
            .to_string(),
        mode: "all".to_string(),
        tail: 0,
    };
    let mut it = raw_args.iter();
    while let Some(flag) = it.next() {
        let val = it
            .next()
            .with_context(|| format!("missing value for {}", flag))?;
        match flag.as_str() {
            "--input-file" => out.input_file = val.clone(),
            "--output-file" => out.output_file = val.clone(),
            "--alerts-file" => out.alerts_file = val.clone(),
            "--mode" => out.mode = normalize_mode(val),
            "--tail" => out.tail = val.parse().unwrap_or(0),
            _ => bail!("unknown arg: {}", flag),
        }
    }
    out.mode = normalize_mode(&out.mode);
    Ok(out)
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
        bail!("decision dashboard input file not found: {}", path);
    }
    let raw = fs::read_to_string(path).with_context(|| format!("read file failed: {}", path))?;
    Ok(raw.lines().map(|x| x.to_string()).collect())
}

pub fn run_with_args(raw_args: &[String]) -> Result<()> {
    let args = parse_args(raw_args)?;
    let mut lines = read_lines(&args.input_file)?;
    if args.tail > 0 && lines.len() > args.tail {
        lines = lines.split_off(lines.len() - args.tail);
    }

    let mut events: Vec<Value> = Vec::new();
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
        if args.mode == "blocked" && !b(evt.get("dispatch_blocked")) {
            continue;
        }
        events.push(evt);
    }

    let mut total: u64 = 0;
    let mut blocked: u64 = 0;
    let mut level_counts: BTreeMap<String, u64> = BTreeMap::new();
    level_counts.insert("info".to_string(), 0);
    level_counts.insert("medium".to_string(), 0);
    level_counts.insert("high".to_string(), 0);
    level_counts.insert("critical".to_string(), 0);
    level_counts.insert("unknown".to_string(), 0);

    let mut latest_by_source: BTreeMap<String, Value> = BTreeMap::new();
    let mut latest_ts = String::new();

    for evt in &events {
        total = total.saturating_add(1);
        if b(evt.get("dispatch_blocked")) {
            blocked = blocked.saturating_add(1);
        }
        let mut level = s(evt.get("decision_alert_level"))
            .trim()
            .to_ascii_lowercase();
        if level.is_empty() {
            level = "unknown".to_string();
        }
        if !level_counts.contains_key(&level) {
            level = "unknown".to_string();
        }
        let old = *level_counts.get(&level).unwrap_or(&0);
        level_counts.insert(level, old.saturating_add(1));

        let mut source = s(evt.get("source")).trim().to_ascii_lowercase();
        if source.is_empty() {
            source = "unknown".to_string();
        }
        let ts = s(evt.get("timestamp_utc"));
        if ts > latest_ts {
            latest_ts = ts.clone();
        }
        let replace = match latest_by_source.get(&source) {
            Some(old_evt) => ts >= s(old_evt.get("timestamp_utc")),
            None => true,
        };
        if replace {
            latest_by_source.insert(source, evt.clone());
        }
    }

    let latest_rows: Vec<Value> = latest_by_source.values().cloned().collect();
    let mut level_obj = Map::new();
    level_obj.insert(
        "info".to_string(),
        Value::from(*level_counts.get("info").unwrap_or(&0)),
    );
    level_obj.insert(
        "medium".to_string(),
        Value::from(*level_counts.get("medium").unwrap_or(&0)),
    );
    level_obj.insert(
        "high".to_string(),
        Value::from(*level_counts.get("high").unwrap_or(&0)),
    );
    level_obj.insert(
        "critical".to_string(),
        Value::from(*level_counts.get("critical").unwrap_or(&0)),
    );
    level_obj.insert(
        "unknown".to_string(),
        Value::from(*level_counts.get("unknown").unwrap_or(&0)),
    );

    let mut state = Map::new();
    state.insert(
        "generated_at_utc".to_string(),
        Value::String(Utc::now().to_rfc3339()),
    );
    state.insert(
        "input_file".to_string(),
        Value::String(args.input_file.clone()),
    );
    state.insert("mode".to_string(), Value::String(args.mode.clone()));
    state.insert("tail".to_string(), Value::from(args.tail as i64));
    state.insert("total_events".to_string(), Value::from(total as i64));
    state.insert("blocked_events".to_string(), Value::from(blocked as i64));
    state.insert("parse_skipped".to_string(), Value::from(skipped as i64));
    state.insert("latest_event_utc".to_string(), Value::String(latest_ts));
    state.insert("level_counts".to_string(), Value::Object(level_obj));
    state.insert(
        "latest_by_source".to_string(),
        Value::Array(latest_rows.clone()),
    );

    if let Some(parent) = Path::new(&args.output_file).parent() {
        if !parent.as_os_str().is_empty() {
            fs::create_dir_all(parent)
                .with_context(|| format!("create output dir failed: {}", parent.display()))?;
        }
    }
    fs::write(
        &args.output_file,
        serde_json::to_vec_pretty(&Value::Object(state)).context("serialize state failed")?,
    )
    .with_context(|| format!("write state failed: {}", args.output_file))?;

    let mut alerts_lines: Vec<String> = Vec::new();
    for evt in latest_rows {
        if !b(evt.get("dispatch_blocked")) {
            continue;
        }
        let mut alert = Map::new();
        alert.insert(
            "timestamp_utc".to_string(),
            Value::String(s(evt.get("timestamp_utc"))),
        );
        alert.insert("source".to_string(), Value::String(s(evt.get("source"))));
        alert.insert(
            "decision_alert_level".to_string(),
            Value::String(s(evt.get("decision_alert_level"))),
        );
        alert.insert(
            "decision_alert_channel".to_string(),
            Value::String(s(evt.get("decision_alert_channel"))),
        );
        alert.insert(
            "decision_alert_target".to_string(),
            Value::String(s(evt.get("decision_alert_target"))),
        );
        alert.insert(
            "decision_delivery_type".to_string(),
            Value::String(s(evt.get("decision_delivery_type"))),
        );
        alert.insert(
            "decision_delivery_action".to_string(),
            Value::String(s(evt.get("decision_delivery_action"))),
        );
        alert.insert(
            "dispatch_blocked".to_string(),
            Value::Bool(b(evt.get("dispatch_blocked"))),
        );
        alert.insert(
            "worst_site_id".to_string(),
            Value::String(s(evt.get("worst_site_id"))),
        );
        alert.insert(
            "worst_level".to_string(),
            Value::String(s(evt.get("worst_level"))),
        );
        alert.insert(
            "worst_score".to_string(),
            Value::from(f(evt.get("worst_score"))),
        );
        alert.insert(
            "risk_policy_active_profile".to_string(),
            Value::String(s(evt.get("risk_policy_active_profile"))),
        );
        alert.insert(
            "failover_converge_scope".to_string(),
            Value::String(s(evt.get("failover_converge_scope"))),
        );
        alerts_lines
            .push(serde_json::to_string(&Value::Object(alert)).context("serialize alert failed")?);
    }

    if let Some(parent) = Path::new(&args.alerts_file).parent() {
        if !parent.as_os_str().is_empty() {
            fs::create_dir_all(parent)
                .with_context(|| format!("create alerts dir failed: {}", parent.display()))?;
        }
    }
    let mut alerts_text = String::new();
    if !alerts_lines.is_empty() {
        alerts_text = alerts_lines.join("\n");
        alerts_text.push('\n');
    }
    fs::write(&args.alerts_file, alerts_text)
        .with_context(|| format!("write alerts failed: {}", args.alerts_file))?;

    println!(
        "decision_dashboard_consumer_done: total={} blocked={} skipped={} state={} alerts={}",
        total, blocked, skipped, args.output_file, args.alerts_file
    );
    Ok(())
}

pub fn run_cli() -> Result<()> {
    let raw_args: Vec<String> = env::args().skip(1).collect();
    run_with_args(&raw_args)
}
