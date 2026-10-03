#![forbid(unsafe_code)]

use anyhow::{bail, Context, Result};
use serde_json::{json, Map, Value};
use std::collections::BTreeSet;
use std::env;

#[derive(Debug)]
struct Args {
    raw_map_json: String,
    fallback_set_json: String,
    normalize_region: bool,
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
        raw_map_json: "{}".to_string(),
        fallback_set_json: "{}".to_string(),
        normalize_region: false,
    };
    let mut it = iter.into_iter();
    while let Some(flag) = it.next() {
        let val = it
            .next()
            .with_context(|| format!("missing value for {}", flag))?;
        match flag.as_str() {
            "--raw-map-json" => out.raw_map_json = val,
            "--fallback-set-json" => out.fallback_set_json = val,
            "--normalize-region" => out.normalize_region = parse_bool(&val),
            _ => bail!("unknown arg: {}", flag),
        }
    }
    Ok(out)
}

fn normalize_level(raw: &str) -> Option<String> {
    let v = raw.trim().to_ascii_lowercase();
    match v.as_str() {
        "green" | "yellow" | "orange" | "red" => Some(v),
        _ => None,
    }
}

fn parse_object(raw: &str) -> Map<String, Value> {
    match serde_json::from_str::<Value>(raw) {
        Ok(Value::Object(m)) => m,
        _ => Map::new(),
    }
}

fn parse_level_set(v: &Value) -> BTreeSet<String> {
    let mut set = BTreeSet::new();
    match v {
        Value::Array(arr) => {
            for it in arr {
                if let Value::String(s) = it {
                    if let Some(level) = normalize_level(s) {
                        set.insert(level);
                    }
                }
            }
        }
        Value::Object(obj) => {
            for (k, vv) in obj {
                let include = match vv {
                    Value::Bool(b) => *b,
                    Value::Number(n) => n.as_i64().unwrap_or(0) != 0,
                    Value::String(s) => {
                        matches!(
                            s.trim().to_ascii_lowercase().as_str(),
                            "1" | "true" | "yes" | "on"
                        )
                    }
                    _ => false,
                };
                if include {
                    if let Some(level) = normalize_level(k) {
                        set.insert(level);
                    }
                }
            }
        }
        Value::String(s) => {
            for p in s.split(',') {
                if let Some(level) = normalize_level(p) {
                    set.insert(level);
                }
            }
        }
        _ => {}
    }
    set
}

pub fn run_from_env() -> Result<()> {
    run_with_args(env::args().skip(1))
}

pub fn run_with_args<I>(iter: I) -> Result<()>
where
    I: IntoIterator<Item = String>,
{
    let args = parse_args_from(iter)?;
    let raw_map = parse_object(&args.raw_map_json);
    let fallback_val: Value = serde_json::from_str(&args.fallback_set_json).unwrap_or(Value::Null);
    let fallback_set = parse_level_set(&fallback_val);

    let mut out_map = Map::new();
    for (k, v) in raw_map {
        let mut key = k.trim().to_string();
        if key.is_empty() {
            continue;
        }
        if args.normalize_region {
            key = key.to_ascii_uppercase();
        }
        let mut set = parse_level_set(&v);
        if set.is_empty() {
            set = fallback_set.clone();
        }
        let arr: Vec<Value> = set.into_iter().map(Value::String).collect();
        out_map.insert(key, Value::Array(arr));
    }

    let out = json!({ "map": out_map });
    println!(
        "{}",
        serde_json::to_string(&out).context("serialize output failed")?
    );
    Ok(())
}
