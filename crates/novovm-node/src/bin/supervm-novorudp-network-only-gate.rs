#![forbid(unsafe_code)]

use anyhow::{bail, Context, Result};
use novovm_network::{NovoRudpRange, NovoRudpTransportFrameKindV0, NovoRudpTransportFrameV0};
use serde::{Deserialize, Serialize};
use serde_json::json;
use sha2::Digest;
use std::collections::{BTreeMap, BTreeSet};
use std::env;
use std::fs;
use std::net::{SocketAddr, UdpSocket};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

const DEFAULT_TX_COUNT: u64 = 2400;
const DEFAULT_TIMEOUT_MS: u64 = 420_000;
const DEFAULT_ACK_INTERVAL_PACKETS: u64 = 32;

#[derive(Debug, Clone, Serialize, Deserialize)]
struct NetworkOnlyAckV0 {
    expected_total: u64,
    received_unique_count: u64,
    missing_ranges: Vec<NovoRudpRange>,
    receiver_done: bool,
    ack_epoch: u64,
}

#[derive(Debug, Clone, Default)]
struct ReceiverStats {
    data_received: u64,
    repair_received: u64,
    duplicate_received: u64,
    ack_sent: u64,
    decode_error_count: u64,
}

#[derive(Debug, Clone, Default)]
struct SenderStats {
    data_sent: u64,
    repair_sent: u64,
    duplicate_sent: u64,
    ack_received: u64,
    decode_error_count: u64,
}

fn main() -> Result<()> {
    let role = env_string("NOVOVM_NOVORUDP_NETWORK_ONLY_ROLE")
        .or_else(|| env_string("NOVOVM_NATIVE_PIPELINE_ROLE"))
        .unwrap_or_else(|| "sender".to_string());
    match role.as_str() {
        "receiver" => run_receiver(),
        "sender" => run_sender(),
        other => bail!("unknown network-only role: {other}"),
    }
}

fn run_receiver() -> Result<()> {
    let bind_addr = env_string("NOVOVM_NOVORUDP_NETWORK_ONLY_LISTEN_ADDR")
        .unwrap_or_else(|| "0.0.0.0:39011".to_string());
    let report_path = env_string("NOVOVM_NOVORUDP_NETWORK_ONLY_REPORT_PATH")
        .unwrap_or_else(|| "artifacts/native-pipeline/novorudp-network-only-receiver.json".into());
    let tx_count = env_u64("NOVOVM_NOVORUDP_NETWORK_ONLY_TX_COUNT", DEFAULT_TX_COUNT);
    let timeout = Duration::from_millis(env_u64(
        "NOVOVM_NOVORUDP_NETWORK_ONLY_TIMEOUT_MS",
        DEFAULT_TIMEOUT_MS,
    ));
    let ack_every = env_u64(
        "NOVOVM_NOVORUDP_NETWORK_ONLY_ACK_INTERVAL_PACKETS",
        DEFAULT_ACK_INTERVAL_PACKETS,
    )
    .max(1);
    let session_id = session_id_v0();
    let socket = UdpSocket::bind(bind_addr.as_str())
        .with_context(|| format!("bind receiver socket failed: {bind_addr}"))?;
    socket
        .set_read_timeout(Some(Duration::from_millis(100)))
        .context("set receiver read timeout failed")?;

    let start = Instant::now();
    let mut delivered = BTreeMap::<u64, Vec<u8>>::new();
    let mut stats = ReceiverStats::default();
    let mut last_peer = None::<SocketAddr>;
    let mut ack_epoch = 0u64;
    let mut packet_since_ack = 0u64;
    let mut buf = vec![0u8; 128 * 1024];

    while start.elapsed() < timeout {
        match socket.recv_from(buf.as_mut_slice()) {
            Ok((n, src)) => {
                let frame = match NovoRudpTransportFrameV0::decode(&buf[..n]) {
                    Ok(frame) => frame,
                    Err(_) => {
                        stats.decode_error_count = stats.decode_error_count.saturating_add(1);
                        continue;
                    }
                };
                if frame.session_id != session_id {
                    continue;
                }
                last_peer = Some(src);
                match frame.kind {
                    NovoRudpTransportFrameKindV0::Data => {
                        stats.data_received = stats.data_received.saturating_add(1);
                        if delivered.insert(frame.sequence, frame.payload).is_some() {
                            stats.duplicate_received = stats.duplicate_received.saturating_add(1);
                        }
                    }
                    NovoRudpTransportFrameKindV0::Repair => {
                        stats.repair_received = stats.repair_received.saturating_add(1);
                        if delivered.insert(frame.sequence, frame.payload).is_some() {
                            stats.duplicate_received = stats.duplicate_received.saturating_add(1);
                        }
                    }
                    _ => {}
                }
                packet_since_ack = packet_since_ack.saturating_add(1);
                let done = delivered.len() as u64 >= tx_count;
                if done || packet_since_ack >= ack_every {
                    if let Some(peer) = last_peer {
                        ack_epoch = ack_epoch.saturating_add(1);
                        send_ack(
                            &socket, peer, session_id, tx_count, &delivered, ack_epoch, done,
                        )?;
                        stats.ack_sent = stats.ack_sent.saturating_add(1);
                        packet_since_ack = 0;
                    }
                }
                if done {
                    break;
                }
            }
            Err(e)
                if e.kind() == std::io::ErrorKind::WouldBlock
                    || e.kind() == std::io::ErrorKind::TimedOut =>
            {
                if let Some(peer) = last_peer {
                    ack_epoch = ack_epoch.saturating_add(1);
                    let done = delivered.len() as u64 >= tx_count;
                    send_ack(
                        &socket, peer, session_id, tx_count, &delivered, ack_epoch, done,
                    )?;
                    stats.ack_sent = stats.ack_sent.saturating_add(1);
                    if done {
                        break;
                    }
                }
            }
            Err(e) => return Err(e).context("receiver recv_from failed"),
        }
    }

    let missing = missing_ranges(tx_count, &delivered);
    let report = json!({
        "schema": "novorudp-network-only-gate-v0",
        "role": "receiver",
        "accepted": missing.is_empty(),
        "transport_frame_v0_enabled": true,
        "network_only_gate_enabled": true,
        "receiver_transport_data_received_count": stats.data_received,
        "receiver_transport_repair_received_count": stats.repair_received,
        "receiver_transport_unique_delivered_count": delivered.len() as u64,
        "receiver_transport_duplicate_received_count": stats.duplicate_received,
        "receiver_transport_ack_sent_count": stats.ack_sent,
        "receiver_transport_final_missing_count": missing_count(&missing),
        "receiver_transport_final_missing_ranges": missing,
        "receiver_transport_done": delivered.len() as u64 == tx_count,
        "business_decode_count": 0u64,
        "aoem_executed_total": 0u64,
        "ledger_completed_count": 0u64,
        "decode_error_count": stats.decode_error_count,
        "elapsed_ms": start.elapsed().as_millis() as u64,
    });
    write_json_report(&report_path, &report)?;
    if report["accepted"].as_bool() == Some(true) {
        Ok(())
    } else {
        bail!(
            "network-only receiver missing={}",
            report["receiver_transport_final_missing_count"]
        )
    }
}

fn run_sender() -> Result<()> {
    let target_addr = env_string("NOVOVM_NOVORUDP_NETWORK_ONLY_RECEIVER_ADDR")
        .unwrap_or_else(|| "127.0.0.1:39011".to_string());
    let bind_addr = env_string("NOVOVM_NOVORUDP_NETWORK_ONLY_SENDER_BIND_ADDR")
        .unwrap_or_else(|| "0.0.0.0:39010".to_string());
    let report_path = env_string("NOVOVM_NOVORUDP_NETWORK_ONLY_REPORT_PATH")
        .unwrap_or_else(|| "artifacts/native-pipeline/novorudp-network-only-sender.json".into());
    let tx_count = env_u64("NOVOVM_NOVORUDP_NETWORK_ONLY_TX_COUNT", DEFAULT_TX_COUNT);
    let timeout = Duration::from_millis(env_u64(
        "NOVOVM_NOVORUDP_NETWORK_ONLY_TIMEOUT_MS",
        DEFAULT_TIMEOUT_MS,
    ));
    let session_id = session_id_v0();
    let target: SocketAddr = target_addr
        .parse()
        .with_context(|| format!("parse receiver addr failed: {target_addr}"))?;
    let socket = UdpSocket::bind(bind_addr.as_str())
        .with_context(|| format!("bind sender socket failed: {bind_addr}"))?;
    socket
        .set_read_timeout(Some(Duration::from_millis(100)))
        .context("set sender read timeout failed")?;

    let start = Instant::now();
    let mut stats = SenderStats::default();
    let mut sent_once = BTreeSet::<u64>::new();
    let mut latest_missing = vec![NovoRudpRange {
        start: 0,
        end_inclusive: tx_count.saturating_sub(1),
    }];
    let payloads = (0..tx_count)
        .map(|sequence| opaque_payload_v0(sequence))
        .collect::<Vec<_>>();

    for sequence in 0..tx_count {
        send_transport_frame(
            &socket,
            target,
            NovoRudpTransportFrameKindV0::Data,
            session_id,
            sequence,
            payloads[sequence as usize].clone(),
            0,
        )?;
        if !sent_once.insert(sequence) {
            stats.duplicate_sent = stats.duplicate_sent.saturating_add(1);
        }
        stats.data_sent = stats.data_sent.saturating_add(1);
    }

    let mut buf = vec![0u8; 128 * 1024];
    let mut done = false;
    let mut pending_repair = None::<(Vec<NovoRudpRange>, u64)>;
    while start.elapsed() < timeout {
        match socket.recv_from(buf.as_mut_slice()) {
            Ok((n, _)) => {
                let frame = match NovoRudpTransportFrameV0::decode(&buf[..n]) {
                    Ok(frame) => frame,
                    Err(_) => {
                        stats.decode_error_count = stats.decode_error_count.saturating_add(1);
                        continue;
                    }
                };
                if frame.kind != NovoRudpTransportFrameKindV0::Ack || frame.session_id != session_id
                {
                    continue;
                }
                let ack: NetworkOnlyAckV0 =
                    serde_json::from_slice(frame.payload.as_slice()).context("decode ack json")?;
                stats.ack_received = stats.ack_received.saturating_add(1);
                latest_missing = ack.missing_ranges.clone();
                if ack.receiver_done && latest_missing.is_empty() {
                    done = true;
                    break;
                }
                pending_repair = Some((latest_missing.clone(), ack.ack_epoch));
            }
            Err(e)
                if e.kind() == std::io::ErrorKind::WouldBlock
                    || e.kind() == std::io::ErrorKind::TimedOut =>
            {
                let Some((repair_ranges, ack_epoch)) = pending_repair.take() else {
                    continue;
                };
                for range in repair_ranges {
                    for sequence in range.start..=range.end_inclusive {
                        if sequence >= tx_count {
                            continue;
                        }
                        send_transport_frame(
                            &socket,
                            target,
                            NovoRudpTransportFrameKindV0::Repair,
                            session_id,
                            sequence,
                            payloads[sequence as usize].clone(),
                            ack_epoch,
                        )?;
                        if !sent_once.insert(sequence) {
                            stats.duplicate_sent = stats.duplicate_sent.saturating_add(1);
                        }
                        stats.repair_sent = stats.repair_sent.saturating_add(1);
                    }
                }
            }
            Err(e) => return Err(e).context("sender recv ack failed"),
        }
    }

    let final_missing = missing_count(&latest_missing);
    let report = json!({
        "schema": "novorudp-network-only-gate-v0",
        "role": "sender",
        "accepted": done,
        "transport_frame_v0_enabled": true,
        "network_only_gate_enabled": true,
        "sender_transport_data_sent_count": stats.data_sent,
        "sender_transport_repair_sent_count": stats.repair_sent,
        "sender_transport_ack_received_count": stats.ack_received,
        "sender_transport_duplicate_sent_count": stats.duplicate_sent,
        "sender_transport_missing_final_count": final_missing,
        "sender_transport_final_missing_ranges": latest_missing,
        "business_decode_count": 0u64,
        "aoem_executed_total": 0u64,
        "ledger_completed_count": 0u64,
        "decode_error_count": stats.decode_error_count,
        "elapsed_ms": start.elapsed().as_millis() as u64,
    });
    write_json_report(&report_path, &report)?;
    if done {
        Ok(())
    } else {
        bail!("network-only sender missing={final_missing}")
    }
}

fn send_transport_frame(
    socket: &UdpSocket,
    target: SocketAddr,
    kind: NovoRudpTransportFrameKindV0,
    session_id: [u8; 16],
    sequence: u64,
    payload: Vec<u8>,
    ack_epoch: u64,
) -> Result<()> {
    let frame =
        NovoRudpTransportFrameV0::new(kind, session_id, 1, sequence, sequence, ack_epoch, payload);
    let encoded = frame.encode();
    socket
        .send_to(encoded.as_slice(), target)
        .with_context(|| format!("send {kind:?} frame failed"))?;
    Ok(())
}

fn send_ack(
    socket: &UdpSocket,
    target: SocketAddr,
    session_id: [u8; 16],
    expected_total: u64,
    delivered: &BTreeMap<u64, Vec<u8>>,
    ack_epoch: u64,
    receiver_done: bool,
) -> Result<()> {
    let ack = NetworkOnlyAckV0 {
        expected_total,
        received_unique_count: delivered.len() as u64,
        missing_ranges: missing_ranges(expected_total, delivered),
        receiver_done,
        ack_epoch,
    };
    let payload = serde_json::to_vec(&ack).context("encode ack json")?;
    let frame = NovoRudpTransportFrameV0::new(
        NovoRudpTransportFrameKindV0::Ack,
        session_id,
        1,
        0,
        0,
        ack_epoch,
        payload,
    );
    socket
        .send_to(frame.encode().as_slice(), target)
        .context("send ack frame failed")?;
    Ok(())
}

fn missing_ranges(expected_total: u64, delivered: &BTreeMap<u64, Vec<u8>>) -> Vec<NovoRudpRange> {
    let mut ranges = Vec::new();
    let mut start = None::<u64>;
    let mut prev = None::<u64>;
    for sequence in 0..expected_total {
        if delivered.contains_key(&sequence) {
            continue;
        }
        match (start, prev) {
            (Some(_), Some(p)) if sequence == p.saturating_add(1) => prev = Some(sequence),
            (Some(s), Some(p)) => {
                ranges.push(NovoRudpRange {
                    start: s,
                    end_inclusive: p,
                });
                start = Some(sequence);
                prev = Some(sequence);
            }
            _ => {
                start = Some(sequence);
                prev = Some(sequence);
            }
        }
    }
    if let (Some(s), Some(p)) = (start, prev) {
        ranges.push(NovoRudpRange {
            start: s,
            end_inclusive: p,
        });
    }
    ranges
}

fn missing_count(ranges: &[NovoRudpRange]) -> u64 {
    ranges.iter().fold(0, |acc, range| {
        acc.saturating_add(
            range
                .end_inclusive
                .saturating_sub(range.start)
                .saturating_add(1),
        )
    })
}

fn opaque_payload_v0(sequence: u64) -> Vec<u8> {
    let mut out = Vec::with_capacity(64);
    out.extend_from_slice(b"novorudp-network-only-opaque-payload-v0:");
    out.extend_from_slice(&sequence.to_le_bytes());
    out
}

fn session_id_v0() -> [u8; 16] {
    let source = env_string("NOVOVM_NOVORUDP_NETWORK_ONLY_SESSION_ID")
        .or_else(|| env_string("NOVOVM_NOVORUDP_SESSION_ID"))
        .unwrap_or_else(|| "novorudp-network-only-v0".to_string());
    let digest = sha2::Sha256::digest(source.as_bytes());
    let mut out = [0u8; 16];
    out.copy_from_slice(&digest[..16]);
    out
}

fn write_json_report(path: &str, value: &serde_json::Value) -> Result<()> {
    if let Some(parent) = std::path::Path::new(path).parent() {
        fs::create_dir_all(parent)
            .with_context(|| format!("create report dir failed: {parent:?}"))?;
    }
    let mut with_time = value.clone();
    if let Some(obj) = with_time.as_object_mut() {
        obj.insert("timestamp_ms".into(), json!(now_ms()));
    }
    fs::write(path, serde_json::to_vec_pretty(&with_time)?)
        .with_context(|| format!("write report failed: {path}"))?;
    Ok(())
}

fn now_ms() -> u128 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis()
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn network_only_missing_ranges_are_transport_only() {
        let mut delivered = BTreeMap::new();
        delivered.insert(0, vec![0]);
        delivered.insert(3, vec![3]);
        delivered.insert(4, vec![4]);

        assert_eq!(
            missing_ranges(6, &delivered),
            vec![
                NovoRudpRange {
                    start: 1,
                    end_inclusive: 2
                },
                NovoRudpRange {
                    start: 5,
                    end_inclusive: 5
                }
            ]
        );
    }
}
