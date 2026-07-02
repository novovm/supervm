#![forbid(unsafe_code)]

use anyhow::{bail, Context, Result};
use serde_json::json;
use std::env;
use std::fs;
use std::net::{SocketAddr, UdpSocket};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

const DEFAULT_PACKET_COUNT: u64 = 4_800;
const DEFAULT_PACKET_BYTES: usize = 4_280;
const DEFAULT_TIMEOUT_MS: u64 = 120_000;

fn main() -> Result<()> {
    match env_string("NOVOVM_UDP_RAW_ROLE")
        .unwrap_or_else(|| "receiver".to_string())
        .as_str()
    {
        "sender" => run_sender(),
        "receiver" => run_receiver(),
        role => bail!("unknown NOVOVM_UDP_RAW_ROLE={role}"),
    }
}

fn run_sender() -> Result<()> {
    let receiver_addr =
        env_string("NOVOVM_UDP_RAW_RECEIVER_ADDR").unwrap_or_else(|| "127.0.0.1:39021".to_string());
    let bind_addr =
        env_string("NOVOVM_UDP_RAW_SENDER_BIND_ADDR").unwrap_or_else(|| "0.0.0.0:39020".into());
    let report_path = env_string("NOVOVM_UDP_RAW_REPORT_PATH")
        .unwrap_or_else(|| "artifacts/native-pipeline/raw-udp-sender.json".into());
    let packet_count = env_u64("NOVOVM_UDP_RAW_PACKET_COUNT", DEFAULT_PACKET_COUNT);
    let packet_bytes = env_u64("NOVOVM_UDP_RAW_PACKET_BYTES", DEFAULT_PACKET_BYTES as u64) as usize;
    let pacing_chunk = env_u64("NOVOVM_UDP_RAW_PACING_CHUNK_SIZE", 0);
    let pacing_gap_ms = env_u64("NOVOVM_UDP_RAW_PACING_GAP_MS", 0);
    let target: SocketAddr = receiver_addr
        .parse()
        .with_context(|| format!("parse receiver addr failed: {receiver_addr}"))?;
    let socket = UdpSocket::bind(bind_addr.as_str())
        .with_context(|| format!("bind sender socket failed: {bind_addr}"))?;
    let mut payload = vec![0u8; packet_bytes.max(16)];
    let start = Instant::now();
    let mut bytes_sent = 0u64;
    let mut send_call_count = 0u64;
    let mut kernel_send_elapsed_us = 0u64;
    let mut pacing_sleep_elapsed_ms = 0u64;
    for sequence in 0..packet_count {
        payload[..8].copy_from_slice(&sequence.to_le_bytes());
        payload[8..16].copy_from_slice(&packet_count.to_le_bytes());
        let send_start = Instant::now();
        socket
            .send_to(payload.as_slice(), target)
            .context("raw udp send_to failed")?;
        kernel_send_elapsed_us =
            kernel_send_elapsed_us.saturating_add(send_start.elapsed().as_micros() as u64);
        send_call_count = send_call_count.saturating_add(1);
        bytes_sent = bytes_sent.saturating_add(payload.len() as u64);
        if pacing_chunk > 0
            && pacing_gap_ms > 0
            && send_call_count % pacing_chunk == 0
            && sequence + 1 < packet_count
        {
            let sleep_start = Instant::now();
            std::thread::sleep(Duration::from_millis(pacing_gap_ms));
            pacing_sleep_elapsed_ms =
                pacing_sleep_elapsed_ms.saturating_add(sleep_start.elapsed().as_millis() as u64);
        }
    }
    let elapsed_ms = start.elapsed().as_millis() as u64;
    let report = json!({
        "schema": "novovm-udp-raw-ceiling-bench-v0",
        "role": "sender",
        "accepted": true,
        "packet_count": packet_count,
        "packet_bytes": payload.len() as u64,
        "sender_elapsed_ms": elapsed_ms,
        "sender_send_call_count": send_call_count,
        "sender_bytes_total": bytes_sent,
        "sender_kernel_send_elapsed_us": kernel_send_elapsed_us,
        "sender_kernel_send_elapsed_ms": kernel_send_elapsed_us / 1000,
        "sender_pacing_chunk_size": pacing_chunk,
        "sender_pacing_gap_ms": pacing_gap_ms,
        "sender_pacing_sleep_elapsed_ms": pacing_sleep_elapsed_ms,
        "sender_packets_per_sec": rate_per_sec(packet_count, elapsed_ms),
        "sender_bytes_per_sec": rate_per_sec(bytes_sent, elapsed_ms),
        "timestamp_ms": now_ms(),
    });
    write_json_report(&report_path, &report)
}

fn run_receiver() -> Result<()> {
    let bind_addr =
        env_string("NOVOVM_UDP_RAW_RECEIVER_BIND_ADDR").unwrap_or_else(|| "0.0.0.0:39021".into());
    let report_path = env_string("NOVOVM_UDP_RAW_REPORT_PATH")
        .unwrap_or_else(|| "artifacts/native-pipeline/raw-udp-receiver.json".into());
    let expected_count = env_u64("NOVOVM_UDP_RAW_PACKET_COUNT", DEFAULT_PACKET_COUNT);
    let packet_bytes = env_u64("NOVOVM_UDP_RAW_PACKET_BYTES", DEFAULT_PACKET_BYTES as u64) as usize;
    let timeout = Duration::from_millis(env_u64("NOVOVM_UDP_RAW_TIMEOUT_MS", DEFAULT_TIMEOUT_MS));
    let socket = UdpSocket::bind(bind_addr.as_str())
        .with_context(|| format!("bind receiver socket failed: {bind_addr}"))?;
    socket
        .set_read_timeout(Some(Duration::from_millis(100)))
        .context("set receiver timeout failed")?;
    let start = Instant::now();
    let mut first_packet_ms = None::<u64>;
    let mut last_packet_ms = None::<u64>;
    let mut packets_received = 0u64;
    let mut bytes_received = 0u64;
    let mut duplicate_count = 0u64;
    let mut out_of_order_count = 0u64;
    let mut max_seen_sequence = None::<u64>;
    let mut seen = vec![false; expected_count as usize];
    let mut buf = vec![0u8; packet_bytes.saturating_add(256).max(2048)];
    while start.elapsed() < timeout && packets_received < expected_count {
        match socket.recv_from(buf.as_mut_slice()) {
            Ok((n, _)) => {
                let now = start.elapsed().as_millis() as u64;
                first_packet_ms.get_or_insert(now);
                last_packet_ms = Some(now);
                bytes_received = bytes_received.saturating_add(n as u64);
                if n >= 8 {
                    let mut seq_bytes = [0u8; 8];
                    seq_bytes.copy_from_slice(&buf[..8]);
                    let sequence = u64::from_le_bytes(seq_bytes);
                    if let Some(max_seen) = max_seen_sequence {
                        if sequence < max_seen {
                            out_of_order_count = out_of_order_count.saturating_add(1);
                        }
                    }
                    max_seen_sequence = Some(max_seen_sequence.unwrap_or(0).max(sequence));
                    if let Some(slot) = seen.get_mut(sequence as usize) {
                        if *slot {
                            duplicate_count = duplicate_count.saturating_add(1);
                        } else {
                            *slot = true;
                            packets_received = packets_received.saturating_add(1);
                        }
                    }
                }
            }
            Err(e)
                if e.kind() == std::io::ErrorKind::WouldBlock
                    || e.kind() == std::io::ErrorKind::TimedOut =>
            {
                continue;
            }
            Err(e) => return Err(e).context("raw udp recv_from failed"),
        }
    }
    let missing_count = expected_count.saturating_sub(packets_received);
    let elapsed_ms = start.elapsed().as_millis() as u64;
    let delivery_elapsed_ms = match (first_packet_ms, last_packet_ms) {
        (Some(first), Some(last)) => last.saturating_sub(first).max(1),
        _ => 0,
    };
    let report = json!({
        "schema": "novovm-udp-raw-ceiling-bench-v0",
        "role": "receiver",
        "accepted": missing_count == 0,
        "expected_packet_count": expected_count,
        "packet_bytes": packet_bytes as u64,
        "receiver_packets_received": packets_received,
        "receiver_bytes_received": bytes_received,
        "receiver_missing_count": missing_count,
        "receiver_duplicate_count": duplicate_count,
        "receiver_out_of_order_count": out_of_order_count,
        "receiver_first_packet_ms": first_packet_ms,
        "receiver_last_packet_ms": last_packet_ms,
        "receiver_delivery_elapsed_ms": delivery_elapsed_ms,
        "receiver_elapsed_ms": elapsed_ms,
        "receiver_packets_per_sec": rate_per_sec(packets_received, delivery_elapsed_ms),
        "receiver_bytes_per_sec": rate_per_sec(bytes_received, delivery_elapsed_ms),
        "timestamp_ms": now_ms(),
    });
    write_json_report(&report_path, &report)?;
    if missing_count == 0 {
        Ok(())
    } else {
        bail!("raw udp receiver missing={missing_count}")
    }
}

fn write_json_report(path: &str, value: &serde_json::Value) -> Result<()> {
    if let Some(parent) = std::path::Path::new(path).parent() {
        fs::create_dir_all(parent)
            .with_context(|| format!("create report dir failed: {}", parent.display()))?;
    }
    fs::write(
        path,
        serde_json::to_string_pretty(value).context("serialize json report")?,
    )
    .with_context(|| format!("write report failed: {path}"))
}

fn env_string(name: &str) -> Option<String> {
    env::var(name).ok().filter(|value| !value.trim().is_empty())
}

fn env_u64(name: &str, default: u64) -> u64 {
    env::var(name)
        .ok()
        .and_then(|value| value.parse::<u64>().ok())
        .unwrap_or(default)
}

fn rate_per_sec(count: u64, elapsed_ms: u64) -> u64 {
    if elapsed_ms == 0 {
        return 0;
    }
    ((count as u128) * 1000 / elapsed_ms as u128) as u64
}

fn now_ms() -> u128 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis()
}
