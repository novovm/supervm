#![forbid(unsafe_code)]

use anyhow::{bail, Context, Result};
use serde_json::{json, Value};
use std::collections::HashMap;
use std::env;

use crate::output::print_success_json;

#[derive(Debug)]
struct Args {
    role: String,
    blocked: bool,
    alert_target_map_json: String,
    alert_delivery_type_map_json: String,
}

fn parse_bool(raw: &str) -> bool {
    matches!(
        raw.trim().to_ascii_lowercase().as_str(),
        "1" | "true" | "yes" | "on"
    )
}

fn parse_map(raw: &str) -> HashMap<String, String> {
    let mut out = HashMap::new();
    let val: Value = match serde_json::from_str(raw) {
        Ok(v) => v,
        Err(_) => Value::Null,
    };
    if let Some(obj) = val.as_object() {
        for (k, v) in obj {
            let key = k.trim().to_ascii_lowercase();
            let value = match v {
                Value::String(s) => s.trim().to_string(),
                Value::Number(n) => n.to_string(),
                Value::Bool(b) => {
                    if *b {
                        "true".to_string()
                    } else {
                        "false".to_string()
                    }
                }
                _ => String::new(),
            };
            if !key.is_empty() && !value.is_empty() {
                out.insert(key, value);
            }
        }
    }
    out
}

fn parse_args(raw_args: &[String]) -> Result<Args> {
    let mut out = Args {
        role: String::new(),
        blocked: false,
        alert_target_map_json: "{}".to_string(),
        alert_delivery_type_map_json: "{}".to_string(),
    };
    let mut it = raw_args.iter();
    while let Some(flag) = it.next() {
        let val = it
            .next()
            .with_context(|| format!("missing value for {}", flag))?;
        match flag.as_str() {
            "--role" => out.role = val.clone(),
            "--blocked" => out.blocked = parse_bool(val),
            "--alert-target-map-json" => out.alert_target_map_json = val.clone(),
            "--alert-delivery-type-map-json" => out.alert_delivery_type_map_json = val.clone(),
            _ => bail!("unknown arg: {}", flag),
        }
    }
    Ok(out)
}

fn resolve_alert_level(role: &str, blocked: bool) -> String {
    if !blocked {
        return "info".to_string();
    }
    let role_norm = role.trim().to_ascii_lowercase();
    if role_norm.contains("l1") || role_norm.contains("finality") || role_norm.contains("arbit") {
        return "critical".to_string();
    }
    if role_norm.contains("l2") || role_norm.contains("exec") || role_norm.contains("prover") {
        return "high".to_string();
    }
    if role_norm.contains("l3")
        || role_norm.contains("edge")
        || role_norm.contains("route")
        || role_norm.contains("gateway")
    {
        return "medium".to_string();
    }
    "high".to_string()
}

fn resolve_alert_channel(role: &str, alert_level: &str, blocked: bool) -> String {
    let role_norm = role.trim().to_ascii_lowercase();
    if !blocked {
        if role_norm.contains("l1") || role_norm.contains("finality") || role_norm.contains("arbit")
        {
            return "l1-observe".to_string();
        }
        if role_norm.contains("l2") || role_norm.contains("exec") || role_norm.contains("prover") {
            return "l2-observe".to_string();
        }
        if role_norm.contains("l3")
            || role_norm.contains("edge")
            || role_norm.contains("route")
            || role_norm.contains("gateway")
        {
            return "l3-observe".to_string();
        }
        return "ops-observe".to_string();
    }
    let level_norm = alert_level.trim().to_ascii_lowercase();
    if level_norm == "critical" {
        return "l1-pager".to_string();
    }
    if level_norm == "high" {
        return "l2-oncall".to_string();
    }
    if level_norm == "medium" {
        return "l3-oncall".to_string();
    }
    "ops-oncall".to_string()
}

fn resolve_alert_target(alert_channel: &str, target_map: &HashMap<String, String>) -> String {
    let key = alert_channel.trim().to_ascii_lowercase();
    if key.is_empty() {
        return String::new();
    }
    if let Some(v) = target_map.get(&key) {
        return v.to_string();
    }
    key
}

fn resolve_delivery_type(alert_target: &str, delivery_map: &HashMap<String, String>) -> String {
    let target_norm = alert_target.trim().to_ascii_lowercase();
    if target_norm.is_empty() {
        return "webhook".to_string();
    }
    if let Some(v) = delivery_map.get(&target_norm) {
        let vv = v.trim().to_ascii_lowercase();
        if vv == "webhook" || vv == "im" || vv == "email" {
            return vv;
        }
    }
    if target_norm.contains('@') {
        return "email".to_string();
    }
    if target_norm.starts_with("observe:")
        || target_norm.starts_with("im:")
        || target_norm.starts_with("chat:")
    {
        return "im".to_string();
    }
    "webhook".to_string()
}

pub fn run_with_args(raw_args: &[String]) -> Result<()> {
    let args = parse_args(raw_args)?;
    let target_map = parse_map(&args.alert_target_map_json);
    let delivery_map = parse_map(&args.alert_delivery_type_map_json);
    let alert_level = resolve_alert_level(&args.role, args.blocked);
    let alert_channel = resolve_alert_channel(&args.role, &alert_level, args.blocked);
    let alert_target = resolve_alert_target(&alert_channel, &target_map);
    let delivery_type = resolve_delivery_type(&alert_target, &delivery_map);
    let delivery_action = format!("dispatch:{}:{}", delivery_type, alert_target);
    let out = json!({
        "decision_alert_level": alert_level,
        "decision_alert_channel": alert_channel,
        "decision_alert_target": alert_target,
        "decision_delivery_type": delivery_type,
        "decision_delivery_action": delivery_action
    });
    print_success_json("rollout", "decision-route", &out)
}

pub fn run_cli() -> Result<()> {
    let raw_args: Vec<String> = env::args().skip(1).collect();
    run_with_args(&raw_args)
}
