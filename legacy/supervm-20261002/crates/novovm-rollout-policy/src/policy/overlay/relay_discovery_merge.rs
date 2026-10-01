#![forbid(unsafe_code)]

use crate::policy::failover::{
    evaluate_region_score, record_seed_failure, record_seed_success, refresh_region_gate,
    refresh_seed_gate, RegionRec, SeedFailoverState,
};
use anyhow::{anyhow, bail, Context, Result};
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};
use std::collections::{HashMap, HashSet};
use std::env;
use std::fs;
use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use crate::output::print_success_json;

#[derive(Debug)]
struct Args {
    directory_file: PathBuf,
    discovery_file: String,
    discovery_http_urls: String,
    discovery_http_urls_file: String,
    seed_region: String,
    seed_mode: String,
    seed_profile: String,
    source_weights_json: String,
    http_timeout_ms: u64,
    default_source_weight: f64,
    default_health: f64,
    default_enabled: bool,
    source_reputation_file: String,
    source_reputation_decay: f64,
    source_penalty_on_http_fail: f64,
    source_recover_on_success: f64,
    source_blacklist_threshold: f64,
    source_denylist: String,
    seed_failover_state_file: String,
    seed_priority_json: String,
    seed_success_rate_threshold: f64,
    seed_cooldown_seconds: u64,
    seed_max_consecutive_failures: u64,
    region_priority_json: String,
    region_failover_threshold: f64,
    region_cooldown_seconds: u64,
    relay_score_smoothing_alpha: f64,
}

#[derive(Default, Debug, Clone, Serialize, Deserialize)]
struct SourceRepState {
    #[serde(default = "one")]
    version: u64,
    #[serde(default)]
    updated_at: String,
    #[serde(default)]
    sources: HashMap<String, f64>,
}

#[derive(Clone, Debug)]
struct Obs {
    id: String,
    region: String,
    roles: Vec<String>,
    enabled: bool,
    health_provided: bool,
    health: f64,
    source: String,
    weight: f64,
    probe_url: String,
    probe_host: String,
    probe_port: i64,
}

#[derive(Clone, Debug)]
struct Relay {
    id: String,
    region: String,
    roles: Vec<String>,
    enabled: bool,
    health_present: bool,
    health: f64,
    source: String,
    source_weight: f64,
    relay_score: f64,
    probe_url: String,
    probe_host: String,
    probe_port: i64,
}

fn one() -> u64 {
    1
}

fn now_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|v| v.as_millis() as u64)
        .unwrap_or(0)
}

fn clamp(v: f64, min: f64, max: f64) -> f64 {
    v.clamp(min, max)
}

fn clamp_u64(v: u64, min: u64, max: u64) -> u64 {
    v.clamp(min, max)
}

fn parse_list(raw: &str) -> Vec<String> {
    raw.split(|c: char| c == ',' || c == ';' || c.is_whitespace())
        .map(str::trim)
        .filter(|v| !v.is_empty())
        .map(ToString::to_string)
        .collect()
}

fn norm_source(raw: &str) -> String {
    let t = raw.trim();
    if t.is_empty() {
        "__empty__".to_string()
    } else {
        t.to_ascii_lowercase()
    }
}

fn norm_region(raw: &str) -> String {
    let t = raw.trim();
    if t.is_empty() {
        "global".to_string()
    } else {
        t.to_ascii_lowercase()
    }
}

fn host_of(source: &str) -> String {
    let trimmed = source.trim();
    let rest = if let Some((_, rhs)) = trimmed.split_once("://") {
        rhs
    } else {
        trimmed
    };
    let authority = rest.split('/').next().unwrap_or("");
    authority
        .split('@')
        .next_back()
        .unwrap_or("")
        .split(':')
        .next()
        .unwrap_or("")
        .trim()
        .to_ascii_lowercase()
}

fn v_str(v: Option<&Value>) -> String {
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

fn v_f64(v: Option<&Value>, d: f64) -> f64 {
    match v {
        Some(Value::Number(n)) => n.as_f64().unwrap_or(d),
        Some(Value::String(s)) => s.parse::<f64>().unwrap_or(d),
        Some(Value::Bool(b)) => {
            if *b {
                1.0
            } else {
                0.0
            }
        }
        _ => d,
    }
}

fn v_i64(v: Option<&Value>, d: i64) -> i64 {
    match v {
        Some(Value::Number(n)) => n.as_i64().unwrap_or(d),
        Some(Value::String(s)) => s.parse::<i64>().unwrap_or(d),
        _ => d,
    }
}

fn v_bool(v: Option<&Value>, d: bool) -> bool {
    match v {
        Some(Value::Bool(b)) => *b,
        Some(Value::Number(n)) => n.as_i64().unwrap_or(0) != 0,
        Some(Value::String(s)) => {
            let t = s.trim().to_ascii_lowercase();
            matches!(t.as_str(), "1" | "true" | "yes" | "on")
        }
        _ => d,
    }
}

fn roles_of(v: Option<&Value>) -> Vec<String> {
    if let Some(Value::Array(arr)) = v {
        let out: Vec<String> = arr
            .iter()
            .filter_map(|x| x.as_str().map(|s| s.trim().to_string()))
            .filter(|s| !s.is_empty())
            .collect();
        if !out.is_empty() {
            return out;
        }
    }
    if let Some(Value::String(s)) = v {
        let out = parse_list(s);
        if !out.is_empty() {
            return out;
        }
    }
    vec!["default".to_string()]
}

fn parse_args_from<I>(iter: I) -> Result<Args>
where
    I: IntoIterator<Item = String>,
{
    let mut a = Args {
        directory_file: PathBuf::new(),
        discovery_file: String::new(),
        discovery_http_urls: String::new(),
        discovery_http_urls_file: String::new(),
        seed_region: String::new(),
        seed_mode: String::new(),
        seed_profile: String::new(),
        source_weights_json: String::new(),
        http_timeout_ms: 1500,
        default_source_weight: 1.0,
        default_health: 0.85,
        default_enabled: true,
        source_reputation_file: String::new(),
        source_reputation_decay: 0.05,
        source_penalty_on_http_fail: 0.2,
        source_recover_on_success: 0.03,
        source_blacklist_threshold: 0.15,
        source_denylist: String::new(),
        seed_failover_state_file: String::new(),
        seed_priority_json: String::new(),
        seed_success_rate_threshold: 0.5,
        seed_cooldown_seconds: 120,
        seed_max_consecutive_failures: 3,
        region_priority_json: String::new(),
        region_failover_threshold: 0.5,
        region_cooldown_seconds: 120,
        relay_score_smoothing_alpha: 0.3,
    };
    let mut it = iter.into_iter();
    while let Some(flag) = it.next() {
        let val = it
            .next()
            .with_context(|| format!("missing value for {}", flag))?;
        match flag.as_str() {
            "--directory-file" => a.directory_file = PathBuf::from(val),
            "--discovery-file" => a.discovery_file = val,
            "--discovery-http-urls" => a.discovery_http_urls = val,
            "--discovery-http-urls-file" => a.discovery_http_urls_file = val,
            "--seed-region" => a.seed_region = val,
            "--seed-mode" => a.seed_mode = val,
            "--seed-profile" => a.seed_profile = val,
            "--source-weights-json" => a.source_weights_json = val,
            "--http-timeout-ms" => a.http_timeout_ms = val.parse().unwrap_or(1500),
            "--default-source-weight" => a.default_source_weight = val.parse().unwrap_or(1.0),
            "--default-health" => a.default_health = val.parse().unwrap_or(0.85),
            "--default-enabled" => a.default_enabled = v_bool(Some(&Value::String(val)), true),
            "--source-reputation-file" => a.source_reputation_file = val,
            "--source-reputation-decay" => a.source_reputation_decay = val.parse().unwrap_or(0.05),
            "--source-penalty-on-http-fail" => {
                a.source_penalty_on_http_fail = val.parse().unwrap_or(0.2)
            }
            "--source-recover-on-success" => {
                a.source_recover_on_success = val.parse().unwrap_or(0.03)
            }
            "--source-blacklist-threshold" => {
                a.source_blacklist_threshold = val.parse().unwrap_or(0.15)
            }
            "--source-denylist" => a.source_denylist = val,
            "--seed-failover-state-file" => a.seed_failover_state_file = val,
            "--seed-priority-json" => a.seed_priority_json = val,
            "--seed-success-rate-threshold" => {
                a.seed_success_rate_threshold = val.parse().unwrap_or(0.5)
            }
            "--seed-cooldown-seconds" => a.seed_cooldown_seconds = val.parse().unwrap_or(120),
            "--seed-max-consecutive-failures" => {
                a.seed_max_consecutive_failures = val.parse().unwrap_or(3)
            }
            "--region-priority-json" => a.region_priority_json = val,
            "--region-failover-threshold" => {
                a.region_failover_threshold = val.parse().unwrap_or(0.5)
            }
            "--region-cooldown-seconds" => a.region_cooldown_seconds = val.parse().unwrap_or(120),
            "--relay-score-smoothing-alpha" => {
                a.relay_score_smoothing_alpha = val.parse().unwrap_or(0.3)
            }
            _ => bail!("unknown arg: {}", flag),
        }
    }
    if a.directory_file.as_os_str().is_empty() {
        bail!("--directory-file is required");
    }
    a.http_timeout_ms = clamp_u64(a.http_timeout_ms, 100, 20000);
    a.default_source_weight = clamp(a.default_source_weight, 0.01, 10.0);
    a.default_health = clamp(a.default_health, 0.0, 1.0);
    a.source_reputation_decay = clamp(a.source_reputation_decay, 0.0, 1.0);
    a.source_penalty_on_http_fail = clamp(a.source_penalty_on_http_fail, 0.0, 1.0);
    a.source_recover_on_success = clamp(a.source_recover_on_success, 0.0, 1.0);
    a.source_blacklist_threshold = clamp(a.source_blacklist_threshold, 0.0, 1.0);
    a.seed_success_rate_threshold = clamp(a.seed_success_rate_threshold, 0.0, 1.0);
    a.seed_cooldown_seconds = clamp_u64(a.seed_cooldown_seconds, 1, 86400);
    a.seed_max_consecutive_failures = clamp_u64(a.seed_max_consecutive_failures, 1, 100);
    a.region_failover_threshold = clamp(a.region_failover_threshold, 0.0, 1.0);
    a.region_cooldown_seconds = clamp_u64(a.region_cooldown_seconds, 1, 86400);
    a.relay_score_smoothing_alpha = clamp(a.relay_score_smoothing_alpha, 0.01, 1.0);
    Ok(a)
}

pub fn run_from_env() -> Result<()> {
    run_with_args(env::args().skip(1))
}

fn load_json<T>(path: &str) -> T
where
    T: for<'de> Deserialize<'de> + Default,
{
    if path.trim().is_empty() {
        return T::default();
    }
    match fs::read_to_string(path) {
        Ok(raw) => serde_json::from_str::<T>(&raw).unwrap_or_default(),
        Err(_) => T::default(),
    }
}

fn save_json<T>(path: &str, obj: &T) -> Result<()>
where
    T: Serialize,
{
    if path.trim().is_empty() {
        return Ok(());
    }
    let p = Path::new(path);
    if let Some(parent) = p.parent() {
        if !parent.as_os_str().is_empty() {
            fs::create_dir_all(parent)
                .with_context(|| format!("create dir failed: {}", parent.display()))?;
        }
    }
    fs::write(
        p,
        serde_json::to_vec_pretty(obj).context("serialize json failed")?,
    )
    .with_context(|| format!("write file failed: {}", p.display()))?;
    Ok(())
}

fn parse_relays(value: &Value) -> Vec<Value> {
    if let Some(arr) = value.as_array() {
        return arr.clone();
    }
    if let Some(obj) = value.as_object() {
        if let Some(arr) = obj.get("relays").and_then(Value::as_array) {
            return arr.clone();
        }
    }
    Vec::new()
}

fn source_allowed(source: &str, deny: &HashSet<String>) -> bool {
    if deny.is_empty() {
        return true;
    }
    let key = norm_source(source);
    if deny.contains(&key) {
        return false;
    }
    let h = host_of(source);
    if !h.is_empty() && deny.contains(&h) {
        return false;
    }
    true
}

fn parse_weight_map(raw: &str, default_weight: f64) -> HashMap<String, f64> {
    let mut map = HashMap::new();
    map.insert("__default__".to_string(), default_weight);
    if raw.trim().is_empty() {
        return map;
    }
    if let Ok(v) = serde_json::from_str::<Value>(raw) {
        if let Some(obj) = v.as_object() {
            for (k, val) in obj {
                map.insert(
                    k.trim().to_ascii_lowercase(),
                    clamp(v_f64(Some(val), default_weight), 0.01, 10.0),
                );
            }
        }
    }
    map
}

fn parse_priority_map(raw: &str, default_priority: i64) -> HashMap<String, i64> {
    let mut map = HashMap::new();
    map.insert("__default__".to_string(), default_priority);
    if raw.trim().is_empty() {
        return map;
    }
    if let Ok(v) = serde_json::from_str::<Value>(raw) {
        if let Some(obj) = v.as_object() {
            for (k, val) in obj {
                map.insert(
                    k.trim().to_ascii_lowercase(),
                    v_i64(Some(val), default_priority),
                );
            }
        }
    }
    map
}

fn resolve_weight(map: &HashMap<String, f64>, source: &str) -> f64 {
    let default = *map.get("__default__").unwrap_or(&1.0);
    let k = source.trim().to_ascii_lowercase();
    if let Some(v) = map.get(&k) {
        return *v;
    }
    let h = host_of(source);
    if !h.is_empty() {
        if let Some(v) = map.get(&h) {
            return *v;
        }
    }
    default
}

fn resolve_priority(map: &HashMap<String, i64>, key: &str) -> i64 {
    let default = *map.get("__default__").unwrap_or(&100);
    let k = key.trim().to_ascii_lowercase();
    if let Some(v) = map.get(&k) {
        return *v;
    }
    let h = host_of(key);
    if !h.is_empty() {
        if let Some(v) = map.get(&h) {
            return *v;
        }
    }
    default
}

fn seed_urls_from_file(path: &str, region: &str, mode: &str, profile: &str) -> Vec<String> {
    if path.trim().is_empty() || !Path::new(path).exists() {
        return Vec::new();
    }
    let raw = match fs::read_to_string(path) {
        Ok(v) => v,
        Err(_) => return Vec::new(),
    };
    let root: Value = match serde_json::from_str(&raw) {
        Ok(v) => v,
        Err(_) => return Vec::new(),
    };
    let mut out = Vec::new();
    let mut push = |v: Option<&Value>| {
        if let Some(Value::Array(arr)) = v {
            for x in arr {
                if let Some(s) = x.as_str() {
                    let t = s.trim();
                    if !t.is_empty() {
                        out.push(t.to_string());
                    }
                }
            }
        } else if let Some(Value::String(s)) = v {
            out.extend(parse_list(s));
        }
    };
    if let Some(obj) = root.as_object() {
        push(obj.get("default"));
        if !region.trim().is_empty() {
            if let Some(o) = obj.get("regions").and_then(Value::as_object) {
                push(o.get(region));
            }
        }
        if !mode.trim().is_empty() {
            if let Some(o) = obj.get("modes").and_then(Value::as_object) {
                push(o.get(mode));
            }
        }
        if !profile.trim().is_empty() {
            if let Some(o) = obj.get("profiles").and_then(Value::as_object) {
                push(o.get(profile));
            }
        }
    }
    let mut seen = HashSet::new();
    out.retain(|u| seen.insert(u.clone()));
    out
}

fn obs_from_value(
    item: &Value,
    src_default: &str,
    def_enabled: bool,
    def_health: f64,
    weight: f64,
) -> Option<Obs> {
    let obj = item.as_object()?;
    let id = v_str(obj.get("id"));
    if id.is_empty() {
        return None;
    }
    let region = {
        let s = v_str(obj.get("region"));
        if s.is_empty() {
            "global".to_string()
        } else {
            s
        }
    };
    let source = {
        let s = v_str(obj.get("source"));
        if s.is_empty() {
            src_default.to_string()
        } else {
            s
        }
    };
    Some(Obs {
        id,
        region,
        roles: roles_of(obj.get("roles")),
        enabled: v_bool(obj.get("enabled"), def_enabled),
        health_provided: obj.get("health").is_some(),
        health: clamp(v_f64(obj.get("health"), def_health), 0.0, 1.0),
        source,
        weight,
        probe_url: v_str(obj.get("probe_url")),
        probe_host: v_str(obj.get("probe_host")),
        probe_port: v_i64(obj.get("probe_port"), 0),
    })
}

pub fn run_with_args<I>(iter: I) -> Result<()>
where
    I: IntoIterator<Item = String>,
{
    let args = parse_args_from(iter)?;
    let now = now_ms();

    let mut src_rep: SourceRepState = load_json(&args.source_reputation_file);
    for v in src_rep.sources.values_mut() {
        *v = clamp(*v + (1.0 - *v) * args.source_reputation_decay, 0.0, 1.0);
    }
    let mut seed_state: SeedFailoverState = load_json(&args.seed_failover_state_file);

    let deny: HashSet<String> = parse_list(&args.source_denylist)
        .into_iter()
        .map(|v| norm_source(&v))
        .collect();
    let wmap = parse_weight_map(&args.source_weights_json, args.default_source_weight);
    let spmap = parse_priority_map(&args.seed_priority_json, 100);
    let rpmap = parse_priority_map(&args.region_priority_json, 100);

    let mut obs: Vec<Obs> = Vec::new();
    let mut local_relays = 0usize;
    let mut http_ok = 0usize;
    let mut http_fail = 0usize;
    let mut http_relays = 0usize;
    let mut deny_skip = 0usize;
    let mut rep_skip = 0usize;
    let mut seed_selected = String::new();
    let mut seed_failover_reason = String::new();
    let mut seed_recover_at = 0u64;
    let mut seed_cooldown_skip = 0u64;
    let mut relay_selected = String::new();
    let mut relay_score = -1.0f64;
    let mut region_failover_reason = String::new();
    let mut region_recover_at = 0u64;

    if !args.discovery_file.trim().is_empty() {
        let raw = fs::read_to_string(&args.discovery_file)
            .with_context(|| format!("read discovery file failed: {}", args.discovery_file))?;
        let root: Value = serde_json::from_str(&raw)
            .with_context(|| format!("parse discovery file failed: {}", args.discovery_file))?;
        for item in parse_relays(&root) {
            if let Some(o) = obs_from_value(
                &item,
                "local-file",
                args.default_enabled,
                args.default_health,
                1.0,
            ) {
                local_relays += 1;
                obs.push(o);
            }
        }
    }

    let mut urls = parse_list(&args.discovery_http_urls);
    urls.extend(seed_urls_from_file(
        &args.discovery_http_urls_file,
        &args.seed_region,
        &args.seed_mode,
        &args.seed_profile,
    ));
    let mut seen = HashSet::new();
    urls.retain(|u| seen.insert(u.clone()));

    let mut candidates: Vec<(String, i64)> = Vec::new();
    for url in &urls {
        if !source_allowed(url, &deny) {
            deny_skip += 1;
            continue;
        }
        let rep = *src_rep.sources.get(&norm_source(url)).unwrap_or(&1.0);
        if rep < args.source_blacklist_threshold {
            rep_skip += 1;
            continue;
        }
        let rec = seed_state.sources.entry(norm_source(url)).or_default();
        let gate = refresh_seed_gate(now, rec);
        if !gate.available {
            seed_cooldown_skip += 1;
            if seed_recover_at == 0 || gate.recover_at_unix_ms < seed_recover_at {
                seed_recover_at = gate.recover_at_unix_ms;
            }
            continue;
        }
        candidates.push((url.clone(), resolve_priority(&spmap, url)));
    }
    candidates.sort_by(|a, b| b.1.cmp(&a.1).then_with(|| a.0.cmp(&b.0)));

    let agent = ureq::AgentBuilder::new()
        .timeout_connect(Duration::from_millis(args.http_timeout_ms))
        .timeout_read(Duration::from_millis(args.http_timeout_ms))
        .timeout_write(Duration::from_millis(args.http_timeout_ms))
        .build();

    for (url, _) in candidates {
        if seed_selected.is_empty() {
            seed_selected = norm_source(&url);
        }
        let key = norm_source(&url);
        let rep_old = *src_rep.sources.get(&key).unwrap_or(&1.0);
        let rec = seed_state.sources.entry(key.clone()).or_default();
        rec.last_selected_unix_ms = now;

        match agent.get(&url).call() {
            Ok(resp) => {
                let txt = resp.into_string().unwrap_or_default();
                if let Ok(root) = serde_json::from_str::<Value>(&txt) {
                    for item in parse_relays(&root) {
                        let mut o = if let Some(v) = obs_from_value(
                            &item,
                            &url,
                            args.default_enabled,
                            args.default_health,
                            resolve_weight(&wmap, &url) * rep_old,
                        ) {
                            v
                        } else {
                            continue;
                        };
                        if !source_allowed(&o.source, &deny) {
                            deny_skip += 1;
                            continue;
                        }
                        let rep2 = *src_rep.sources.get(&norm_source(&o.source)).unwrap_or(&1.0);
                        if rep2 < args.source_blacklist_threshold {
                            rep_skip += 1;
                            continue;
                        }
                        o.weight = (resolve_weight(&wmap, &o.source) * rep2).max(0.01);
                        obs.push(o);
                        http_relays += 1;
                    }
                }
                http_ok += 1;
                src_rep.sources.insert(
                    key,
                    clamp(
                        rep_old + (1.0 - rep_old) * args.source_recover_on_success,
                        0.0,
                        1.0,
                    ),
                );
                record_seed_success(now, rec);
            }
            Err(_) => {
                http_fail += 1;
                src_rep.sources.insert(
                    key,
                    clamp(
                        (rep_old - args.source_penalty_on_http_fail).max(0.0),
                        0.0,
                        1.0,
                    ),
                );
                let outcome = record_seed_failure(
                    now,
                    rec,
                    args.seed_success_rate_threshold,
                    args.seed_cooldown_seconds,
                    args.seed_max_consecutive_failures,
                );
                if outcome.degraded {
                    if seed_recover_at == 0 || outcome.recover_at_unix_ms < seed_recover_at {
                        seed_recover_at = outcome.recover_at_unix_ms;
                    }
                    seed_failover_reason = outcome.reason;
                }
            }
        }
    }
    if seed_selected.is_empty() && seed_recover_at > 0 && seed_failover_reason.is_empty() {
        seed_failover_reason = "all_sources_cooldown".to_string();
    }

    if obs.is_empty() {
        bail!("no relay observations from discovery file or http urls");
    }

    let mut groups: HashMap<String, Vec<Obs>> = HashMap::new();
    for o in obs {
        groups
            .entry(o.id.trim().to_ascii_lowercase())
            .or_default()
            .push(o);
    }
    let mut merged: Vec<Relay> = Vec::new();
    for rows in groups.values() {
        let mut list = rows.clone();
        list.sort_by(|a, b| {
            b.weight
                .total_cmp(&a.weight)
                .then_with(|| a.source.cmp(&b.source))
        });
        let best = list.first().ok_or_else(|| anyhow!("empty group"))?;
        let mut t = 0.0;
        let mut f = 0.0;
        let mut hn = 0.0;
        let mut hd = 0.0;
        for x in &list {
            if x.enabled {
                t += x.weight;
            } else {
                f += x.weight;
            }
            if x.health_provided {
                hn += x.health * x.weight;
                hd += x.weight;
            }
        }
        let hp = hd > 0.0;
        let h = if hp {
            clamp(hn / hd, 0.0, 1.0)
        } else {
            args.default_health
        };
        merged.push(Relay {
            id: best.id.clone(),
            region: best.region.clone(),
            roles: best.roles.clone(),
            enabled: t >= f,
            health_present: hp,
            health: h,
            source: best.source.clone(),
            source_weight: best.weight,
            relay_score: h,
            probe_url: best.probe_url.clone(),
            probe_host: best.probe_host.clone(),
            probe_port: best.probe_port,
        });
    }

    let mut dir_root: Value =
        if args.directory_file.exists() {
            serde_json::from_str(&fs::read_to_string(&args.directory_file).with_context(|| {
                format!("read directory failed: {}", args.directory_file.display())
            })?)
            .unwrap_or_else(|_| Value::Object(Map::new()))
        } else {
            Value::Object(Map::new())
        };
    if !dir_root.is_object() {
        dir_root = Value::Object(Map::new());
    }
    let root = dir_root
        .as_object_mut()
        .ok_or_else(|| anyhow!("directory root not object"))?;

    let mut relays = root
        .get("relays")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();
    let mut idx_by_id: HashMap<String, usize> = HashMap::new();
    for (i, r) in relays.iter().enumerate() {
        if let Some(id) = r
            .as_object()
            .and_then(|o| o.get("id"))
            .and_then(Value::as_str)
        {
            let k = id.trim().to_ascii_lowercase();
            if !k.is_empty() {
                idx_by_id.entry(k).or_insert(i);
            }
        }
    }

    let mut region_state: HashMap<String, RegionRec> = HashMap::new();
    if let Some(obj) = root
        .get("discovery_region_failover_state")
        .and_then(Value::as_object)
    {
        for (k, v) in obj {
            if let Ok(r) = serde_json::from_value::<RegionRec>(v.clone()) {
                region_state.insert(norm_region(k), r);
            }
        }
    }

    let mut region_scores: HashMap<String, Vec<f64>> = HashMap::new();
    for r in &mut merged {
        let key = r.id.trim().to_ascii_lowercase();
        let prev = idx_by_id
            .get(&key)
            .and_then(|i| relays.get(*i))
            .and_then(Value::as_object)
            .map(|o| {
                let rs = v_f64(o.get("relay_score"), -1.0);
                if rs >= 0.0 {
                    clamp(rs, 0.0, 1.0)
                } else {
                    clamp(v_f64(o.get("health"), r.health), 0.0, 1.0)
                }
            })
            .unwrap_or(r.health);
        r.relay_score = clamp(
            prev * (1.0 - args.relay_score_smoothing_alpha)
                + r.health * args.relay_score_smoothing_alpha,
            0.0,
            1.0,
        );
        region_scores
            .entry(norm_region(&r.region))
            .or_default()
            .push(r.relay_score);
    }

    for (rk, scores) in &region_scores {
        let rec = region_state.entry(rk.clone()).or_default();
        let avg = if scores.is_empty() {
            0.0
        } else {
            scores.iter().sum::<f64>() / scores.len() as f64
        };
        refresh_region_gate(now, rec);
        evaluate_region_score(
            now,
            rec,
            avg,
            args.region_failover_threshold,
            args.region_cooldown_seconds,
        );
    }
    for rec in region_state.values_mut() {
        refresh_region_gate(now, rec);
    }

    let mut degraded: Vec<(String, String, i64, u64)> = Vec::new();
    for (rk, rec) in &region_state {
        if rec.degraded_until_unix_ms > now {
            degraded.push((
                rk.clone(),
                rec.degraded_reason.clone(),
                resolve_priority(&rpmap, rk),
                rec.degraded_until_unix_ms,
            ));
            if region_recover_at == 0 || rec.degraded_until_unix_ms < region_recover_at {
                region_recover_at = rec.degraded_until_unix_ms;
            }
        }
    }
    degraded.sort_by(|a, b| {
        b.2.cmp(&a.2)
            .then_with(|| a.3.cmp(&b.3))
            .then_with(|| a.0.cmp(&b.0))
    });
    if let Some((rk, reason, _, _)) = degraded.first() {
        region_failover_reason = format!("region_{}_{}", rk, reason);
    }

    #[derive(Clone)]
    struct Candidate {
        id: String,
        region_priority: i64,
        relay_score: f64,
        health: f64,
        region_key: String,
    }
    let mut all = Vec::new();
    let mut active = Vec::new();
    for r in &merged {
        let region = if r.region.trim().is_empty() {
            "global"
        } else {
            r.region.as_str()
        };
        let c = Candidate {
            id: r.id.clone(),
            region_priority: resolve_priority(&rpmap, region),
            relay_score: r.relay_score,
            health: r.health,
            region_key: norm_region(region),
        };
        all.push(c.clone());
        if region_state
            .get(&c.region_key)
            .map(|x| x.degraded_until_unix_ms <= now)
            .unwrap_or(true)
        {
            active.push(c);
        }
    }
    if active.is_empty() && !all.is_empty() {
        active = all;
        if region_failover_reason.is_empty() && region_recover_at > 0 {
            region_failover_reason = "all_regions_cooldown_fallback".to_string();
        }
    }
    active.sort_by(|a, b| {
        b.region_priority
            .cmp(&a.region_priority)
            .then_with(|| b.relay_score.total_cmp(&a.relay_score))
            .then_with(|| b.health.total_cmp(&a.health))
            .then_with(|| a.id.cmp(&b.id))
    });
    if let Some(c) = active.first() {
        relay_selected = c.id.clone();
        relay_score = c.relay_score;
    }

    let mut added = 0usize;
    let mut updated = 0usize;
    for r in &merged {
        let key = r.id.trim().to_ascii_lowercase();
        let mut obj = if let Some(idx) = idx_by_id.get(&key).cloned() {
            updated += 1;
            relays
                .get(idx)
                .cloned()
                .unwrap_or_else(|| Value::Object(Map::new()))
        } else {
            added += 1;
            Value::Object(Map::new())
        };
        if !obj.is_object() {
            obj = Value::Object(Map::new());
        }
        let o = obj
            .as_object_mut()
            .ok_or_else(|| anyhow!("relay object not object"))?;
        o.insert("id".to_string(), Value::String(r.id.clone()));
        o.insert("region".to_string(), Value::String(r.region.clone()));
        o.insert(
            "roles".to_string(),
            Value::Array(r.roles.iter().map(|x| Value::String(x.clone())).collect()),
        );
        o.insert("enabled".to_string(), Value::Bool(r.enabled));
        if r.health_present || !o.contains_key("health") {
            o.insert("health".to_string(), Value::from(r.health));
        }
        o.insert("source".to_string(), Value::String(r.source.clone()));
        o.insert("source_weight".to_string(), Value::from(r.source_weight));
        o.insert("relay_score".to_string(), Value::from(r.relay_score));
        o.insert(
            "last_discovery_at".to_string(),
            Value::String(now.to_string()),
        );
        if !r.probe_url.is_empty() {
            o.insert("probe_url".to_string(), Value::String(r.probe_url.clone()));
        }
        if !r.probe_host.is_empty() {
            o.insert(
                "probe_host".to_string(),
                Value::String(r.probe_host.clone()),
            );
        }
        if r.probe_port > 0 {
            o.insert("probe_port".to_string(), Value::from(r.probe_port));
        }

        if let Some(idx) = idx_by_id.get(&key).cloned() {
            relays[idx] = obj;
        } else {
            idx_by_id.insert(key, relays.len());
            relays.push(obj);
        }
    }

    let mut region_state_obj = Map::new();
    for (k, v) in &region_state {
        region_state_obj.insert(k.clone(), serde_json::to_value(v)?);
    }
    root.insert("version".to_string(), Value::from(1u64));
    root.insert("updated_at".to_string(), Value::String(now.to_string()));
    root.insert("relays".to_string(), Value::Array(relays));
    root.insert(
        "discovery_region_failover_state".to_string(),
        Value::Object(region_state_obj),
    );

    if let Some(parent) = args.directory_file.parent() {
        if !parent.as_os_str().is_empty() {
            fs::create_dir_all(parent)
                .with_context(|| format!("create parent dir failed: {}", parent.display()))?;
        }
    }
    fs::write(
        &args.directory_file,
        serde_json::to_vec_pretty(&dir_root).context("serialize directory failed")?,
    )
    .with_context(|| format!("write directory failed: {}", args.directory_file.display()))?;

    src_rep.updated_at = now.to_string();
    seed_state.updated_at = now.to_string();
    save_json(&args.source_reputation_file, &src_rep)?;
    save_json(&args.seed_failover_state_file, &seed_state)?;

    let seed_selected_out = if seed_selected.is_empty() {
        "none"
    } else {
        seed_selected.as_str()
    };
    let seed_failover_reason_out = if seed_failover_reason.is_empty() {
        "none"
    } else {
        seed_failover_reason.as_str()
    };
    let relay_selected_out = if relay_selected.is_empty() {
        "none"
    } else {
        relay_selected.as_str()
    };
    let region_failover_reason_out = if region_failover_reason.is_empty() {
        "none"
    } else {
        region_failover_reason.as_str()
    };

    let out = serde_json::json!({
        "directory_file": args.directory_file.display().to_string(),
        "local_relays": local_relays,
        "http_sources_ok": http_ok,
        "http_sources_fail": http_fail,
        "http_relays": http_relays,
        "merged_relays": merged.len(),
        "added": added,
        "updated": updated,
        "deny_skip": deny_skip,
        "rep_blacklist_skip": rep_skip,
        "seed_selected": seed_selected_out,
        "seed_failover_reason": seed_failover_reason_out,
        "seed_recover_at_unix_ms": seed_recover_at,
        "seed_cooldown_skip": seed_cooldown_skip,
        "relay_selected": relay_selected_out,
        "relay_score": relay_score,
        "region_failover_reason": region_failover_reason_out,
        "region_recover_at_unix_ms": region_recover_at
    });
    print_success_json("overlay", "relay-discovery-merge", &out)
}
