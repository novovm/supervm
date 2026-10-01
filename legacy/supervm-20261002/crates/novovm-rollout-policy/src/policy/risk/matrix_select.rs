#![forbid(unsafe_code)]

use anyhow::{bail, Context, Result};
use serde_json::{json, Map, Value};
use std::env;

#[derive(Debug)]
struct Args {
    site_id: String,
    site_region_map_json: String,
    site_matrix_json: String,
    region_matrix_json: String,
    global_matrix_json: String,
}

fn parse_args_from<I>(iter: I) -> Result<Args>
where
    I: IntoIterator<Item = String>,
{
    let mut out = Args {
        site_id: String::new(),
        site_region_map_json: "{}".to_string(),
        site_matrix_json: "{}".to_string(),
        region_matrix_json: "{}".to_string(),
        global_matrix_json: "[]".to_string(),
    };
    let mut it = iter.into_iter();
    while let Some(flag) = it.next() {
        let val = it
            .next()
            .with_context(|| format!("missing value for {}", flag))?;
        match flag.as_str() {
            "--site-id" => out.site_id = val,
            "--site-region-map-json" => out.site_region_map_json = val,
            "--site-matrix-json" => out.site_matrix_json = val,
            "--region-matrix-json" => out.region_matrix_json = val,
            "--global-matrix-json" => out.global_matrix_json = val,
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

fn parse_array(raw: &str) -> Vec<Value> {
    match serde_json::from_str::<Value>(raw) {
        Ok(Value::Array(v)) => v,
        _ => Vec::new(),
    }
}

fn get_case_insensitive<'a>(m: &'a Map<String, Value>, key: &str) -> Option<&'a Value> {
    if let Some(v) = m.get(key) {
        return Some(v);
    }
    m.iter()
        .find(|(k, _)| k.eq_ignore_ascii_case(key))
        .map(|(_, v)| v)
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

fn value_to_array(v: Option<&Value>) -> Vec<Value> {
    match v {
        Some(Value::Array(a)) => a.clone(),
        _ => Vec::new(),
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
    let site = args.site_id.trim().to_string();
    let site_region_map = parse_object(&args.site_region_map_json);
    let site_matrix = parse_object(&args.site_matrix_json);
    let region_matrix = parse_object(&args.region_matrix_json);
    let global_matrix = parse_array(&args.global_matrix_json);

    if !site.is_empty() {
        let site_rules = value_to_array(get_case_insensitive(&site_matrix, &site));
        if !site_rules.is_empty() {
            let out = json!({
                "scope": format!("site:{}", site),
                "matrix": site_rules
            });
            println!(
                "{}",
                serde_json::to_string(&out).context("serialize output failed")?
            );
            return Ok(());
        }
    }

    let region_raw = value_to_string(get_case_insensitive(&site_region_map, &site));
    let region = region_raw.trim().to_ascii_uppercase();
    if !region.is_empty() {
        let region_rules = value_to_array(get_case_insensitive(&region_matrix, &region));
        if !region_rules.is_empty() {
            let out = json!({
                "scope": format!("region:{}", region),
                "matrix": region_rules
            });
            println!(
                "{}",
                serde_json::to_string(&out).context("serialize output failed")?
            );
            return Ok(());
        }
    }

    let out = json!({
        "scope": "global",
        "matrix": global_matrix
    });
    println!(
        "{}",
        serde_json::to_string(&out).context("serialize output failed")?
    );
    Ok(())
}
