#![forbid(unsafe_code)]

use anyhow::{bail, Context, Result};
use serde_json::{Map, Value};
use std::env;
use std::fs;
use std::net::{SocketAddr, TcpStream, ToSocketAddrs};
use std::path::PathBuf;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use crate::output::print_success_json;

#[derive(Debug)]
struct Args {
    directory_file: PathBuf,
    mode: String,
    probe_timeout_ms: u64,
    alpha: f64,
}

#[derive(Debug)]
struct ProbeResult {
    ok: bool,
    latency_ms: Option<u64>,
    status: String,
}

fn clamp(v: f64, min: f64, max: f64) -> f64 {
    v.clamp(min, max)
}

fn clamp_u64(v: u64, min: u64, max: u64) -> u64 {
    v.clamp(min, max)
}

fn now_unix_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|v| v.as_millis() as u64)
        .unwrap_or(0)
}

fn normalize_mode(raw: &str) -> String {
    let m = raw.trim().to_ascii_lowercase();
    match m.as_str() {
        "auto" | "http" | "tcp" | "none" => m,
        _ => "auto".to_string(),
    }
}

fn score_latency_ms(latency_ms: Option<u64>) -> f64 {
    let Some(lat) = latency_ms else {
        return 0.6;
    };
    if lat <= 80 {
        return 1.0;
    }
    if lat <= 180 {
        return 0.95;
    }
    if lat <= 350 {
        return 0.85;
    }
    if lat <= 700 {
        return 0.7;
    }
    if lat <= 1200 {
        return 0.5;
    }
    0.35
}

fn value_as_string(v: Option<&Value>) -> String {
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

fn value_as_f64(v: Option<&Value>, default: f64) -> f64 {
    match v {
        Some(Value::Number(n)) => n.as_f64().unwrap_or(default),
        Some(Value::String(s)) => s.parse::<f64>().unwrap_or(default),
        Some(Value::Bool(b)) => {
            if *b {
                1.0
            } else {
                0.0
            }
        }
        _ => default,
    }
}

fn value_as_bool(v: Option<&Value>, default: bool) -> bool {
    match v {
        Some(Value::Bool(b)) => *b,
        Some(Value::Number(n)) => n.as_i64().unwrap_or(0) != 0,
        Some(Value::String(s)) => {
            let t = s.trim().to_ascii_lowercase();
            matches!(t.as_str(), "1" | "true" | "yes" | "on")
        }
        _ => default,
    }
}

fn value_as_i64(v: Option<&Value>, default: i64) -> i64 {
    match v {
        Some(Value::Number(n)) => n.as_i64().unwrap_or(default),
        Some(Value::String(s)) => s.parse::<i64>().unwrap_or(default),
        _ => default,
    }
}

fn to_addr_list(host: &str, port: u16) -> Vec<SocketAddr> {
    let addr = format!("{}:{}", host, port);
    match addr.to_socket_addrs() {
        Ok(it) => it.collect(),
        Err(_) => Vec::new(),
    }
}

fn test_tcp_probe(host: &str, port: u16, timeout_ms: u64) -> ProbeResult {
    let start = Instant::now();
    let timeout = Duration::from_millis(timeout_ms);
    let addrs = to_addr_list(host, port);
    if addrs.is_empty() {
        return ProbeResult {
            ok: false,
            latency_ms: None,
            status: "resolve_failed".to_string(),
        };
    }
    for addr in addrs {
        match TcpStream::connect_timeout(&addr, timeout) {
            Ok(stream) => {
                let _ = stream.shutdown(std::net::Shutdown::Both);
                return ProbeResult {
                    ok: true,
                    latency_ms: Some(start.elapsed().as_millis() as u64),
                    status: "ok".to_string(),
                };
            }
            Err(e) => {
                if e.kind() == std::io::ErrorKind::TimedOut {
                    return ProbeResult {
                        ok: false,
                        latency_ms: None,
                        status: "timeout".to_string(),
                    };
                }
            }
        }
    }
    ProbeResult {
        ok: false,
        latency_ms: None,
        status: "connect_failed".to_string(),
    }
}

fn test_http_probe(url: &str, timeout_ms: u64) -> ProbeResult {
    let start = Instant::now();
    let agent = ureq::AgentBuilder::new()
        .timeout_connect(Duration::from_millis(timeout_ms))
        .timeout_read(Duration::from_millis(timeout_ms))
        .timeout_write(Duration::from_millis(timeout_ms))
        .build();
    let result = agent.head(url).call();
    match result {
        Ok(resp) => {
            let code = resp.status();
            ProbeResult {
                ok: (200..500).contains(&code),
                latency_ms: Some(start.elapsed().as_millis() as u64),
                status: format!("http_{}", code),
            }
        }
        Err(ureq::Error::Status(code, _)) => ProbeResult {
            ok: (200..500).contains(&code),
            latency_ms: Some(start.elapsed().as_millis() as u64),
            status: format!("http_{}", code),
        },
        Err(e) => {
            let status = e.to_string();
            ProbeResult {
                ok: false,
                latency_ms: None,
                status,
            }
        }
    }
}

fn resolve_probe(relay: &Map<String, Value>, mode: &str, timeout_ms: u64) -> Option<ProbeResult> {
    let probe_url = value_as_string(relay.get("probe_url"));
    let probe_host = value_as_string(relay.get("probe_host"));
    let probe_port = value_as_i64(relay.get("probe_port"), 0);
    if mode == "none" {
        return None;
    }
    if mode == "http" {
        if probe_url.is_empty() {
            return None;
        }
        return Some(test_http_probe(&probe_url, timeout_ms));
    }
    if mode == "tcp" {
        if probe_host.is_empty() || probe_port <= 0 {
            return None;
        }
        return Some(test_tcp_probe(&probe_host, probe_port as u16, timeout_ms));
    }
    if !probe_url.is_empty() {
        return Some(test_http_probe(&probe_url, timeout_ms));
    }
    if !probe_host.is_empty() && probe_port > 0 {
        return Some(test_tcp_probe(&probe_host, probe_port as u16, timeout_ms));
    }
    None
}

fn parse_args_from<I>(iter: I) -> Result<Args>
where
    I: IntoIterator<Item = String>,
{
    let mut out = Args {
        directory_file: PathBuf::new(),
        mode: "auto".to_string(),
        probe_timeout_ms: 800,
        alpha: 0.2,
    };
    let mut it = iter.into_iter();
    while let Some(flag) = it.next() {
        let val = it
            .next()
            .with_context(|| format!("missing value for {}", flag))?;
        match flag.as_str() {
            "--directory-file" => out.directory_file = PathBuf::from(val),
            "--mode" => out.mode = normalize_mode(&val),
            "--probe-timeout-ms" => out.probe_timeout_ms = val.parse().unwrap_or(800),
            "--alpha" => out.alpha = val.parse().unwrap_or(0.2),
            _ => bail!("unknown arg: {}", flag),
        }
    }
    if out.directory_file.as_os_str().is_empty() {
        bail!("--directory-file is required");
    }
    out.mode = normalize_mode(&out.mode);
    out.probe_timeout_ms = clamp_u64(out.probe_timeout_ms, 100, 15_000);
    out.alpha = clamp(out.alpha, 0.01, 1.0);
    Ok(out)
}

pub fn run_from_env() -> Result<()> {
    run_with_args(env::args().skip(1))
}

pub fn run_with_args<I>(iter: I) -> Result<()>
where
    I: IntoIterator<Item = String>,
{
    let args = parse_args_from(iter)?;
    if !args.directory_file.exists() {
        bail!(
            "overlay relay directory file not found: {}",
            args.directory_file.display()
        );
    }
    let raw = fs::read_to_string(&args.directory_file)
        .with_context(|| format!("read file failed: {}", args.directory_file.display()))?;
    if raw.trim().is_empty() {
        bail!(
            "overlay relay directory file is empty: {}",
            args.directory_file.display()
        );
    }
    let mut root: Value = serde_json::from_str(&raw)
        .with_context(|| format!("parse json failed: {}", args.directory_file.display()))?;
    let root_obj = root
        .as_object_mut()
        .ok_or_else(|| anyhow::anyhow!("overlay relay directory root invalid"))?;
    let relays = root_obj
        .get_mut("relays")
        .and_then(Value::as_array_mut)
        .ok_or_else(|| anyhow::anyhow!("overlay relay directory missing relays array"))?;

    let mut probed = 0u64;
    let mut updated = 0u64;
    let mut failed = 0u64;
    let now = now_unix_ms().to_string();

    for relay_v in relays {
        let Some(relay_obj) = relay_v.as_object_mut() else {
            continue;
        };
        let enabled = value_as_bool(relay_obj.get("enabled"), true);
        if !enabled {
            continue;
        }
        let old_health = clamp(value_as_f64(relay_obj.get("health"), 1.0), 0.0, 1.0);
        let probe = resolve_probe(relay_obj, &args.mode, args.probe_timeout_ms);
        let Some(probe) = probe else {
            continue;
        };
        probed = probed.saturating_add(1);
        let score = if probe.ok {
            score_latency_ms(probe.latency_ms)
        } else {
            failed = failed.saturating_add(1);
            0.05
        };
        let mut new_health = ((1.0 - args.alpha) * old_health) + (args.alpha * score);
        new_health = clamp(new_health, 0.0, 1.0);
        let rounded = (new_health * 10000.0).round() / 10000.0;
        relay_obj.insert("health".to_string(), Value::from(rounded));
        relay_obj.insert("last_probe_ok".to_string(), Value::Bool(probe.ok));
        if let Some(lat) = probe.latency_ms {
            relay_obj.insert("last_probe_latency_ms".to_string(), Value::from(lat));
        } else {
            relay_obj.remove("last_probe_latency_ms");
        }
        relay_obj.insert("last_probe_status".to_string(), Value::String(probe.status));
        relay_obj.insert("last_probe_at".to_string(), Value::String(now.clone()));
        updated = updated.saturating_add(1);
    }

    root_obj.insert("updated_at".to_string(), Value::String(now));
    fs::write(
        &args.directory_file,
        serde_json::to_vec_pretty(&root).context("serialize root failed")?,
    )
    .with_context(|| format!("write file failed: {}", args.directory_file.display()))?;

    let out = serde_json::json!({
        "mode": args.mode,
        "directory_file": args.directory_file.display().to_string(),
        "probed": probed,
        "updated": updated,
        "failed": failed,
        "probe_timeout_ms": args.probe_timeout_ms,
        "alpha": args.alpha
    });
    print_success_json("overlay", "relay-health-refresh", &out)
}
