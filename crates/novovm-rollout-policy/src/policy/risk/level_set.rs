#![forbid(unsafe_code)]

use anyhow::{bail, Context, Result};
use serde_json::{json, Map, Value};
use std::collections::BTreeSet;
use std::env;

#[derive(Debug)]
struct Args {
    raw_set_json: String,
}

fn parse_args_from<I>(iter: I) -> Result<Args>
where
    I: IntoIterator<Item = String>,
{
    let mut out = Args {
        raw_set_json: "null".to_string(),
    };
    let mut it = iter.into_iter();
    while let Some(flag) = it.next() {
        let val = it
            .next()
            .with_context(|| format!("missing value for {}", flag))?;
        match flag.as_str() {
            "--raw-set-json" => out.raw_set_json = val,
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
    let raw_val: Value = serde_json::from_str(&args.raw_set_json).unwrap_or(Value::Null);
    let levels_set = parse_level_set(&raw_val);
    let levels: Vec<String> = levels_set.iter().cloned().collect();
    let mut set_obj = Map::new();
    for lvl in &levels {
        set_obj.insert(lvl.clone(), Value::Bool(true));
    }
    let out = json!({
        "levels": levels,
        "set": set_obj
    });
    println!(
        "{}",
        serde_json::to_string(&out).context("serialize output failed")?
    );
    Ok(())
}
