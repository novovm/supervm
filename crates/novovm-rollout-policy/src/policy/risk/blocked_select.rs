#![forbid(unsafe_code)]

use anyhow::{bail, Context, Result};
use serde_json::{json, Map, Value};
use std::collections::{BTreeSet, HashSet};
use std::env;

#[derive(Debug)]
struct Args {
    site_id: String,
    default_set_json: String,
    site_map_json: String,
    region_map_json: String,
    site_region_map_json: String,
}

fn parse_args_from<I>(iter: I) -> Result<Args>
where
    I: IntoIterator<Item = String>,
{
    let mut out = Args {
        site_id: String::new(),
        default_set_json: "{}".to_string(),
        site_map_json: "{}".to_string(),
        region_map_json: "{}".to_string(),
        site_region_map_json: "{}".to_string(),
    };
    let mut it = iter.into_iter();
    while let Some(flag) = it.next() {
        let val = it
            .next()
            .with_context(|| format!("missing value for {}", flag))?;
        match flag.as_str() {
            "--site-id" => out.site_id = val,
            "--default-set-json" => out.default_set_json = val,
            "--site-map-json" => out.site_map_json = val,
            "--region-map-json" => out.region_map_json = val,
            "--site-region-map-json" => out.site_region_map_json = val,
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

fn normalize_level(raw: &str) -> Option<String> {
    let v = raw.trim().to_ascii_lowercase();
    match v.as_str() {
        "green" | "yellow" | "orange" | "red" => Some(v),
        _ => None,
    }
}

fn parse_level_set(value: &Value) -> BTreeSet<String> {
    let mut set = BTreeSet::new();
    match value {
        Value::Array(arr) => {
            for it in arr {
                if let Value::String(s) = it {
                    if let Some(v) = normalize_level(s) {
                        set.insert(v);
                    }
                }
            }
        }
        Value::Object(obj) => {
            for (k, v) in obj {
                let include = match v {
                    Value::Bool(b) => *b,
                    Value::Number(n) => n.as_i64().unwrap_or(0) != 0,
                    Value::String(s) => {
                        let x = s.trim().to_ascii_lowercase();
                        matches!(x.as_str(), "1" | "true" | "yes" | "on")
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
            for part in s.split(',') {
                if let Some(level) = normalize_level(part) {
                    set.insert(level);
                }
            }
        }
        _ => {}
    }
    set
}

fn get_case_insensitive<'a>(m: &'a Map<String, Value>, key: &str) -> Option<&'a Value> {
    if let Some(v) = m.get(key) {
        return Some(v);
    }
    m.iter()
        .find(|(k, _)| k.eq_ignore_ascii_case(key))
        .map(|(_, v)| v)
}

fn get_region_for_site(site_region_map: &Map<String, Value>, site: &str) -> String {
    if site.trim().is_empty() {
        return String::new();
    }
    let raw = match get_case_insensitive(site_region_map, site) {
        Some(Value::String(s)) => s.trim().to_string(),
        Some(Value::Number(n)) => n.to_string(),
        _ => String::new(),
    };
    raw.trim().to_ascii_uppercase()
}

pub fn run_from_env() -> Result<()> {
    run_with_args(env::args().skip(1))
}

pub fn run_with_args<I>(iter: I) -> Result<()>
where
    I: IntoIterator<Item = String>,
{
    let args = parse_args_from(iter)?;
    let site = args.site_id.trim().to_string();
    let site_map = parse_object(&args.site_map_json);
    let region_map = parse_object(&args.region_map_json);
    let site_region_map = parse_object(&args.site_region_map_json);

    let default_val: Value = serde_json::from_str(&args.default_set_json).unwrap_or(Value::Null);
    let default_set = parse_level_set(&default_val);

    let mut scope = "global".to_string();
    let mut selected = default_set.clone();

    if !site.is_empty() {
        if let Some(v) = get_case_insensitive(&site_map, &site) {
            let site_set = parse_level_set(v);
            if !site_set.is_empty() {
                scope = format!("site:{}", site);
                selected = site_set;
            }
        }
    }

    if scope == "global" {
        let region = get_region_for_site(&site_region_map, &site);
        if !region.is_empty() {
            if let Some(v) = get_case_insensitive(&region_map, &region) {
                let region_set = parse_level_set(v);
                if !region_set.is_empty() {
                    scope = format!("region:{}", region);
                    selected = region_set;
                }
            }
        }
    }

    let mut blocked_levels: Vec<String> = selected.into_iter().collect();
    blocked_levels.sort();
    let blocked_set: HashSet<String> = blocked_levels.iter().cloned().collect();

    let out = json!({
        "scope": scope,
        "blocked_levels": blocked_levels,
        "blocked_set": blocked_set
    });
    println!(
        "{}",
        serde_json::to_string(&out).context("serialize output failed")?
    );
    Ok(())
}
