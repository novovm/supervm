#![forbid(unsafe_code)]

use anyhow::{bail, Context, Result};
use novovm_network::{NovoRudpRange, NovoRudpTransportFrameKindV0, NovoRudpTransportFrameV0};
use novovm_node::tx_ingress::nov_native_tx_to_adapter_tx_ir_v1;
use novovm_protocol::{
    decode as business_decode_v0, decode_nov_native_tx_wire_v1, encode as business_encode_v0,
    encode_nov_native_tx_wire_v1, EvmNativeMessage, NodeId, NovExecuteTxV1, NovExecutionModeV1,
    NovExecutionPolicyV1, NovExecutionTargetV1, NovFeePolicyV1, NovNativeTxWireV1,
    NovPrivacyModeV1, NovTxKindV1, NovVerificationModeV1, ProtocolMessage,
};
use serde::{Deserialize, Serialize};
use serde_json::json;
use sha2::Digest;
use std::collections::{BTreeMap, BTreeSet};
use std::env;
use std::fs;
use std::net::{SocketAddr, UdpSocket};
use std::thread;
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

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum PayloadModeV0 {
    Opaque,
    EvmTransactions,
}

impl PayloadModeV0 {
    fn from_env() -> Self {
        match env_string("NOVOVM_NOVORUDP_NETWORK_ONLY_PAYLOAD_MODE")
            .unwrap_or_else(|| "opaque".to_string())
            .as_str()
        {
            "evm_transactions" => Self::EvmTransactions,
            _ => Self::Opaque,
        }
    }

    const fn as_str(self) -> &'static str {
        match self {
            Self::Opaque => "opaque",
            Self::EvmTransactions => "evm_transactions",
        }
    }
}

#[derive(Debug, Clone, Default)]
struct SenderStats {
    data_send_attempt: u64,
    data_sent: u64,
    data_pacing_sleep_count: u64,
    repair_sent: u64,
    duplicate_sent: u64,
    ack_received: u64,
    decode_error_count: u64,
    data_loss_injected: u64,
}

#[derive(Debug, Clone, Copy)]
struct LossInjectionConfigV0 {
    data_loss_bps: u64,
    seed: u64,
}

impl LossInjectionConfigV0 {
    fn from_env() -> Self {
        Self {
            data_loss_bps: env_u64("NOVOVM_NOVORUDP_NETWORK_ONLY_DATA_LOSS_BPS", 0).min(10_000),
            seed: env_u64(
                "NOVOVM_NOVORUDP_NETWORK_ONLY_LOSS_SEED",
                0x9e37_79b9_7f4a_7c15,
            ),
        }
    }

    const fn enabled(self) -> bool {
        self.data_loss_bps > 0
    }

    fn drops_data_sequence(self, sequence: u64) -> bool {
        if !self.enabled() {
            return false;
        }
        loss_roll_bps_v0(self.seed, sequence) < self.data_loss_bps
    }
}

#[derive(Debug, Clone, Default)]
struct ReceiverExecutionSummaryV0 {
    business_decode_count: u64,
    business_decode_error_count: u64,
    aoem_executed_total: u64,
    aoem_execution_error_count: u64,
    ledger_completed_count: u64,
    business_decode_elapsed_ms: u64,
    aoem_execute_elapsed_ms: u64,
    ledger_close_elapsed_ms: u64,
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
    let payload_mode = PayloadModeV0::from_env();
    let execute_aoem = env_bool("NOVOVM_NOVORUDP_NETWORK_ONLY_EXECUTE_AOEM");
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
    let mut first_packet_ms = None::<u64>;
    let mut last_packet_ms = None::<u64>;
    let mut transport_done_ms = None::<u64>;

    while start.elapsed() < timeout {
        match socket.recv_from(buf.as_mut_slice()) {
            Ok((n, src)) => {
                let recv_ms = start.elapsed().as_millis() as u64;
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
                first_packet_ms.get_or_insert(recv_ms);
                last_packet_ms = Some(recv_ms);
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
                    transport_done_ms = Some(start.elapsed().as_millis() as u64);
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
    let execution = receiver_execution_summary_v0(payload_mode, execute_aoem, &delivered);
    let elapsed_ms = start.elapsed().as_millis() as u64;
    let receiver_payload_bytes_total = delivered.values().fold(0u64, |acc, payload| {
        acc.saturating_add(payload.len() as u64)
    });
    let receiver_transport_unique_delivered_count = delivered.len() as u64;
    let receiver_transport_final_missing_count = missing_count(&missing);
    let first_packet_ms = first_packet_ms.unwrap_or(0);
    let last_packet_ms = last_packet_ms.unwrap_or(first_packet_ms);
    let transport_done_ms = transport_done_ms.unwrap_or(last_packet_ms);
    let receiver_transport_delivery_elapsed_ms = transport_done_ms.saturating_sub(first_packet_ms);
    let receiver_idle_wait_elapsed_ms = first_packet_ms;
    let receiver_finalization_elapsed_ms = elapsed_ms.saturating_sub(transport_done_ms);
    let report = json!({
        "schema": "novorudp-network-only-gate-v0",
        "role": "receiver",
        "accepted": missing.is_empty(),
        "transport_frame_v0_enabled": true,
        "network_only_gate_enabled": true,
        "business_payload_mode": payload_mode.as_str(),
        "receiver_transport_data_received_count": stats.data_received,
        "receiver_transport_repair_received_count": stats.repair_received,
        "receiver_transport_unique_delivered_count": receiver_transport_unique_delivered_count,
        "receiver_transport_duplicate_received_count": stats.duplicate_received,
        "receiver_transport_ack_sent_count": stats.ack_sent,
        "receiver_transport_final_missing_count": receiver_transport_final_missing_count,
        "receiver_transport_final_missing_ranges": missing,
        "receiver_transport_done": receiver_transport_unique_delivered_count == tx_count,
        "receiver_elapsed_ms": elapsed_ms,
        "receiver_first_packet_ms": first_packet_ms,
        "receiver_last_packet_ms": last_packet_ms,
        "receiver_transport_done_ms": transport_done_ms,
        "receiver_transport_delivery_elapsed_ms": receiver_transport_delivery_elapsed_ms,
        "receiver_business_decode_elapsed_ms": execution.business_decode_elapsed_ms,
        "receiver_aoem_execute_elapsed_ms": execution.aoem_execute_elapsed_ms,
        "receiver_ledger_close_elapsed_ms": execution.ledger_close_elapsed_ms,
        "receiver_finalization_elapsed_ms": receiver_finalization_elapsed_ms,
        "receiver_report_write_elapsed_ms": 0u64,
        "receiver_idle_wait_elapsed_ms": receiver_idle_wait_elapsed_ms,
        "receiver_payload_bytes_total": receiver_payload_bytes_total,
        "receiver_payloads_per_sec": rate_per_sec_v0(receiver_transport_unique_delivered_count, elapsed_ms),
        "receiver_bytes_per_sec": rate_per_sec_v0(receiver_payload_bytes_total, elapsed_ms),
        "receiver_missing_rate_bps": bps_v0(receiver_transport_final_missing_count, tx_count),
        "receiver_duplicate_bps": bps_v0(stats.duplicate_received, receiver_transport_unique_delivered_count),
        "aoem_execute_enabled": execute_aoem,
        "aoem_execution_mode": if execute_aoem { "adapter_projection_v0" } else { "disabled" },
        "business_decode_count": execution.business_decode_count,
        "business_decode_error_count": execution.business_decode_error_count,
        "aoem_executed_total": execution.aoem_executed_total,
        "aoem_execution_error_count": execution.aoem_execution_error_count,
        "ledger_completed_count": execution.ledger_completed_count,
        "decode_error_count": stats.decode_error_count,
        "elapsed_ms": elapsed_ms,
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
    let payload_mode = PayloadModeV0::from_env();
    let loss = LossInjectionConfigV0::from_env();
    let data_pacing_chunk_size = env_u64("NOVOVM_NOVORUDP_NETWORK_ONLY_DATA_PACING_CHUNK_SIZE", 32);
    let data_pacing_chunk_gap_ms =
        env_u64("NOVOVM_NOVORUDP_NETWORK_ONLY_DATA_PACING_CHUNK_GAP_MS", 5);
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
        .map(|sequence| payload_for_sequence_v0(payload_mode, sequence))
        .collect::<Result<Vec<_>>>()?;

    for sequence in 0..tx_count {
        stats.data_send_attempt = stats.data_send_attempt.saturating_add(1);
        if loss.drops_data_sequence(sequence) {
            stats.data_loss_injected = stats.data_loss_injected.saturating_add(1);
            continue;
        }
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
        if data_pacing_chunk_size > 0
            && data_pacing_chunk_gap_ms > 0
            && stats.data_sent % data_pacing_chunk_size == 0
            && sequence + 1 < tx_count
        {
            stats.data_pacing_sleep_count = stats.data_pacing_sleep_count.saturating_add(1);
            thread::sleep(Duration::from_millis(data_pacing_chunk_gap_ms));
        }
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
    let elapsed_ms = start.elapsed().as_millis() as u64;
    let sender_data_payload_bytes_total = payloads.iter().fold(0u64, |acc, payload| {
        acc.saturating_add(payload.len() as u64)
    });
    let report = json!({
        "schema": "novorudp-network-only-gate-v0",
        "role": "sender",
        "accepted": done,
        "transport_frame_v0_enabled": true,
        "network_only_gate_enabled": true,
        "business_payload_mode": payload_mode.as_str(),
        "transport_loss_injection_enabled": loss.enabled(),
        "transport_loss_injection_data_loss_bps": loss.data_loss_bps,
        "transport_loss_injection_seed": loss.seed,
        "sender_transport_data_pacing_enabled": data_pacing_chunk_size > 0 && data_pacing_chunk_gap_ms > 0,
        "sender_transport_data_pacing_chunk_size": data_pacing_chunk_size,
        "sender_transport_data_pacing_chunk_gap_ms": data_pacing_chunk_gap_ms,
        "sender_transport_data_pacing_sleep_count": stats.data_pacing_sleep_count,
        "sender_transport_data_send_attempt_count": stats.data_send_attempt,
        "sender_transport_data_loss_injected_count": stats.data_loss_injected,
        "sender_elapsed_ms": elapsed_ms,
        "sender_data_payload_bytes_total": sender_data_payload_bytes_total,
        "sender_data_frames_per_sec": rate_per_sec_v0(stats.data_sent, elapsed_ms),
        "sender_data_bytes_per_sec": rate_per_sec_v0(sender_data_payload_bytes_total, elapsed_ms),
        "sender_ack_count": stats.ack_received,
        "sender_repair_amplification_bps": bps_v0(stats.repair_sent, stats.data_loss_injected),
        "sender_transport_data_sent_count": stats.data_sent,
        "sender_transport_repair_sent_count": stats.repair_sent,
        "sender_transport_ack_received_count": stats.ack_received,
        "sender_transport_duplicate_sent_count": stats.duplicate_sent,
        "sender_transport_missing_final_count": final_missing,
        "sender_transport_final_missing_ranges": latest_missing,
        "business_decode_count": 0u64,
        "business_decode_error_count": 0u64,
        "aoem_executed_total": 0u64,
        "ledger_completed_count": 0u64,
        "decode_error_count": stats.decode_error_count,
        "elapsed_ms": elapsed_ms,
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

fn rate_per_sec_v0(count: u64, elapsed_ms: u64) -> u64 {
    count.saturating_mul(1000) / elapsed_ms.max(1)
}

fn bps_v0(numerator: u64, denominator: u64) -> u64 {
    numerator.saturating_mul(10_000) / denominator.max(1)
}

fn opaque_payload_v0(sequence: u64) -> Vec<u8> {
    let mut out = Vec::with_capacity(64);
    out.extend_from_slice(b"novorudp-network-only-opaque-payload-v0:");
    out.extend_from_slice(&sequence.to_le_bytes());
    out
}

fn payload_for_sequence_v0(mode: PayloadModeV0, sequence: u64) -> Result<Vec<u8>> {
    match mode {
        PayloadModeV0::Opaque => Ok(opaque_payload_v0(sequence)),
        PayloadModeV0::EvmTransactions => {
            let native_payload = native_tx_payload_for_sequence_v0(sequence)?;
            let msg = ProtocolMessage::EvmNative(EvmNativeMessage::Transactions {
                from: NodeId(1),
                chain_id: 1,
                tx_hash: tx_hash_for_sequence_v0(sequence),
                tx_count: 1,
                payload: native_payload,
                transport_auth: None,
            });
            business_encode_v0(&msg).context("encode evm transaction business payload")
        }
    }
}

fn receiver_execution_summary_v0(
    mode: PayloadModeV0,
    execute_aoem: bool,
    delivered: &BTreeMap<u64, Vec<u8>>,
) -> ReceiverExecutionSummaryV0 {
    if mode != PayloadModeV0::EvmTransactions {
        return ReceiverExecutionSummaryV0::default();
    }
    let mut summary = ReceiverExecutionSummaryV0::default();
    for payload in delivered.values() {
        let business_decode_start = Instant::now();
        let decoded = business_decode_v0(payload.as_slice());
        summary.business_decode_elapsed_ms = summary
            .business_decode_elapsed_ms
            .saturating_add(business_decode_start.elapsed().as_millis() as u64);
        match decoded {
            Ok(ProtocolMessage::EvmNative(EvmNativeMessage::Transactions {
                payload: native_payload,
                ..
            })) => {
                summary.business_decode_count = summary.business_decode_count.saturating_add(1);
                if execute_aoem {
                    let aoem_start = Instant::now();
                    match decode_nov_native_tx_wire_v1(native_payload.as_slice()) {
                        Ok(native_tx) => match nov_native_tx_to_adapter_tx_ir_v1(&native_tx) {
                            Ok(_) => {
                                summary.aoem_executed_total =
                                    summary.aoem_executed_total.saturating_add(1);
                                summary.ledger_completed_count =
                                    summary.ledger_completed_count.saturating_add(1);
                            }
                            Err(_) => {
                                summary.aoem_execution_error_count =
                                    summary.aoem_execution_error_count.saturating_add(1);
                            }
                        },
                        Err(_) => {
                            summary.aoem_execution_error_count =
                                summary.aoem_execution_error_count.saturating_add(1);
                        }
                    }
                    summary.aoem_execute_elapsed_ms = summary
                        .aoem_execute_elapsed_ms
                        .saturating_add(aoem_start.elapsed().as_millis() as u64);
                }
            }
            Ok(_) | Err(_) => {
                summary.business_decode_error_count =
                    summary.business_decode_error_count.saturating_add(1);
            }
        }
    }
    summary.ledger_close_elapsed_ms = summary.aoem_execute_elapsed_ms;
    summary
}

fn native_tx_payload_for_sequence_v0(sequence: u64) -> Result<Vec<u8>> {
    let nonce = sequence.saturating_add(1);
    let account_id = format!("acct-novorudp-network-only-{nonce}");
    let tx = NovNativeTxWireV1 {
        chain_id: 1,
        kind: NovTxKindV1::Execute(NovExecuteTxV1 {
            caller: vec![(nonce & 0xff) as u8; 20],
            account_id: Some(account_id.clone()),
            fee_owner_account_id: Some(account_id.clone()),
            nonce_owner_account_id: Some(account_id),
            target: NovExecutionTargetV1::NativeModule("treasury".to_string()),
            method: "deposit_reserve".to_string(),
            args: serde_json::to_vec(&serde_json::json!({
                "asset": "USDT",
                "amount": nonce,
            }))
            .context("encode network-only native tx args")?,
            execution_mode: NovExecutionModeV1::Batch,
            execution_policy: NovExecutionPolicyV1::Standard,
            privacy_mode: NovPrivacyModeV1::Public,
            verification_mode: NovVerificationModeV1::Standard,
            fee_policy: NovFeePolicyV1 {
                pay_asset: "USDT".to_string(),
                max_pay_amount: 10_000,
                slippage_bps: 100,
            },
            gas_like_limit: Some(90_000),
            nonce,
        }),
        signature: [(nonce & 0xff) as u8; 32],
    };
    encode_nov_native_tx_wire_v1(&tx)
        .map_err(|err| anyhow::anyhow!("encode network-only native tx wire failed: {err}"))
}

fn tx_hash_for_sequence_v0(sequence: u64) -> [u8; 32] {
    let digest = sha2::Sha256::digest(sequence.to_le_bytes());
    let mut out = [0u8; 32];
    out.copy_from_slice(&digest);
    out
}

fn loss_roll_bps_v0(seed: u64, sequence: u64) -> u64 {
    let mut value = seed ^ sequence.wrapping_mul(0x9e37_79b9_7f4a_7c15) ^ sequence.rotate_left(17);
    value ^= value >> 30;
    value = value.wrapping_mul(0xbf58_476d_1ce4_e5b9);
    value ^= value >> 27;
    value = value.wrapping_mul(0x94d0_49bb_1331_11eb);
    value ^= value >> 31;
    value % 10_000
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

fn env_bool(name: &str) -> bool {
    matches!(
        env::var(name)
            .ok()
            .map(|value| value.trim().to_ascii_lowercase()),
        Some(value) if value == "1" || value == "true" || value == "yes" || value == "on"
    )
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

    #[test]
    fn evm_transactions_payload_mode_decodes_after_transport_delivery_only() {
        let mut delivered = BTreeMap::new();
        for sequence in 0..4 {
            delivered.insert(
                sequence,
                payload_for_sequence_v0(PayloadModeV0::EvmTransactions, sequence).expect("payload"),
            );
        }

        let decoded =
            receiver_execution_summary_v0(PayloadModeV0::EvmTransactions, false, &delivered);
        assert_eq!(decoded.business_decode_count, 4);
        assert_eq!(decoded.business_decode_error_count, 0);
        assert_eq!(decoded.aoem_executed_total, 0);
        assert_eq!(decoded.ledger_completed_count, 0);

        let opaque = receiver_execution_summary_v0(PayloadModeV0::Opaque, true, &delivered);
        assert_eq!(
            opaque.business_decode_count, 0,
            "transport-only mode must not decode business payloads"
        );
    }

    #[test]
    fn aoem_execution_mode_projects_decoded_native_payloads_after_transport_delivery() {
        let mut delivered = BTreeMap::new();
        for sequence in 0..4 {
            delivered.insert(
                sequence,
                payload_for_sequence_v0(PayloadModeV0::EvmTransactions, sequence).expect("payload"),
            );
        }

        let summary =
            receiver_execution_summary_v0(PayloadModeV0::EvmTransactions, true, &delivered);
        assert_eq!(summary.business_decode_count, 4);
        assert_eq!(summary.business_decode_error_count, 0);
        assert_eq!(summary.aoem_executed_total, 4);
        assert_eq!(summary.aoem_execution_error_count, 0);
        assert_eq!(summary.ledger_completed_count, 4);
    }

    #[test]
    fn data_loss_injection_is_deterministic_and_disabled_by_zero_bps() {
        let disabled = LossInjectionConfigV0 {
            data_loss_bps: 0,
            seed: 7,
        };
        assert!(!disabled.enabled());
        assert!(!(0..128).any(|sequence| disabled.drops_data_sequence(sequence)));

        let enabled = LossInjectionConfigV0 {
            data_loss_bps: 500,
            seed: 7,
        };
        let first = (0..2400)
            .filter(|sequence| enabled.drops_data_sequence(*sequence))
            .collect::<Vec<_>>();
        let second = (0..2400)
            .filter(|sequence| enabled.drops_data_sequence(*sequence))
            .collect::<Vec<_>>();
        assert_eq!(first, second);
        assert!(
            !first.is_empty(),
            "5% deterministic loss should drop at least one packet in 2400 sequences"
        );
    }
}
