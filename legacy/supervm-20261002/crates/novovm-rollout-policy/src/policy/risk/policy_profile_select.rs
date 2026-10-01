#![forbid(unsafe_code)]

use anyhow::{bail, Context, Result};
use serde_json::{json, Map, Value};
use std::env;

use crate::output::print_success_json;

#[derive(Debug)]
struct Args {
    risk_policy_json: String,
    requested_profile: String,
}

fn parse_args_from<I>(iter: I) -> Result<Args>
where
    I: IntoIterator<Item = String>,
{
    let mut out = Args {
        risk_policy_json: "{}".to_string(),
        requested_profile: String::new(),
    };
    let mut it = iter.into_iter();
    while let Some(flag) = it.next() {
        let val = it
            .next()
            .with_context(|| format!("missing value for {}", flag))?;
        match flag.as_str() {
            "--risk-policy-json" => out.risk_policy_json = val,
            "--requested-profile" => out.requested_profile = val,
            _ => bail!("unknown arg: {}", flag),
        }
    }
    Ok(out)
}

fn parse_object(raw: &str) -> Map<String, Value> {
    match serde_json::from_str::<Value>(raw) {
        Ok(Value::Object(m)) => m,
        _ => Map::new(),
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

pub fn run_from_env() -> Result<()> {
    run_with_args(env::args().skip(1))
}

pub fn run_with_args<I>(iter: I) -> Result<()>
where
    I: IntoIterator<Item = String>,
{
    let args = parse_args_from(iter)?;
    let risk_policy = parse_object(&args.risk_policy_json);
    let policy_profiles = match risk_policy.get("policy_profiles") {
        Some(Value::Object(m)) => m.clone(),
        _ => Map::new(),
    };
    let active_profile_cfg = value_to_string(risk_policy.get("active_profile"));
    let requested = args.requested_profile.trim().to_string();

    let mut sorted_keys: Vec<String> = policy_profiles.keys().cloned().collect();
    sorted_keys.sort();
    let first_key = sorted_keys.first().cloned().unwrap_or_default();

    let selected = if !requested.is_empty() && policy_profiles.contains_key(&requested) {
        requested.clone()
    } else if !active_profile_cfg.is_empty() && policy_profiles.contains_key(&active_profile_cfg) {
        active_profile_cfg.clone()
    } else if policy_profiles.contains_key("production") {
        "production".to_string()
    } else if !first_key.is_empty() {
        first_key
    } else if !active_profile_cfg.is_empty() {
        active_profile_cfg.clone()
    } else if !requested.is_empty() {
        requested.clone()
    } else {
        "production".to_string()
    };

    let resolved = if !requested.is_empty() {
        policy_profiles.contains_key(&requested)
    } else if !active_profile_cfg.is_empty() {
        policy_profiles.contains_key(&active_profile_cfg)
    } else {
        policy_profiles.contains_key("production") || !policy_profiles.is_empty()
    };

    let profile = match policy_profiles.get(&selected) {
        Some(Value::Object(m)) => Value::Object(m.clone()),
        _ => Value::Object(Map::new()),
    };

    let out = json!({
        "requested_profile": requested,
        "selected_profile": selected,
        "resolved": resolved,
        "reason": if resolved { "resolved" } else { "fallback_selected" },
        "profile": profile
    });
    print_success_json("risk", "policy-profile-select", &out)
}
