use anyhow::{Context, Result};
use serde::Serialize;
use std::time::{SystemTime, UNIX_EPOCH};

#[derive(Serialize)]
struct SuccessEnvelope<'a, T> {
    ok: bool,
    domain: &'a str,
    action: &'a str,
    timestamp_unix_ms: u64,
    data: T,
}

fn now_unix_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|v| v.as_millis() as u64)
        .unwrap_or(0)
}

pub fn print_success_json<T: Serialize>(
    domain: &'static str,
    action: &'static str,
    data: &T,
) -> Result<()> {
    let envelope = SuccessEnvelope {
        ok: true,
        domain,
        action,
        timestamp_unix_ms: now_unix_ms(),
        data,
    };
    println!(
        "{}",
        serde_json::to_string(&envelope).context("serialize success envelope failed")?
    );
    Ok(())
}
