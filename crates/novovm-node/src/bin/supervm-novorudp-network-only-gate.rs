#![forbid(unsafe_code)]
#![recursion_limit = "512"]

use anyhow::{bail, Context, Result};
use novovm_exec::{AoemExecFacade, AoemRuntimeConfig};
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
use socket2::SockRef;
use std::collections::{BTreeMap, BTreeSet};
use std::env;
use std::fs;
use std::net::{SocketAddr, UdpSocket};
use std::thread;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

const DEFAULT_TX_COUNT: u64 = 2400;
const DEFAULT_TIMEOUT_MS: u64 = 420_000;
const DEFAULT_ACK_INTERVAL_PACKETS: u64 = 32;
const BATCH_NATIVE_TX_PAYLOAD_MAGIC_V0: &[u8] = b"NOVRUDP-BTXS-V0";
const APFL_NATIVE_TRANSFER_BATCH_MAGIC_V0: &[u8] = b"NOVRUDP-APFL-NTX-V0";
const APFL_NATIVE_TRANSFER_BATCH_VERSION_V0: u8 = 1;
const APFL_NATIVE_TRANSFER_TEMPLATE_DEPOSIT_RESERVE_V0: u16 = 1;
const APFL_NATIVE_TRANSFER_SIGNATURE_LEN_V0: usize = 32;

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
    NativeTransferApflV0,
}

impl PayloadModeV0 {
    fn from_env() -> Self {
        match env_string("NOVOVM_NOVORUDP_NETWORK_ONLY_PAYLOAD_MODE")
            .unwrap_or_else(|| "opaque".to_string())
            .as_str()
        {
            "evm_transactions" => Self::EvmTransactions,
            "native_transfer_apfl_v0" => Self::NativeTransferApflV0,
            _ => Self::Opaque,
        }
    }

    const fn as_str(self) -> &'static str {
        match self {
            Self::Opaque => "opaque",
            Self::EvmTransactions => "evm_transactions",
            Self::NativeTransferApflV0 => "native_transfer_apfl_v0",
        }
    }

    const fn is_business_payload(self) -> bool {
        matches!(self, Self::EvmTransactions | Self::NativeTransferApflV0)
    }

    const fn is_apfl_native_transfer(self) -> bool {
        matches!(self, Self::NativeTransferApflV0)
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
    data_payload_bytes_sent_total: u64,
    repair_payload_bytes_sent_total: u64,
    payload_copy_elapsed_ms: u64,
    socket_send_elapsed_ms: u64,
    transport_frame_encode_elapsed_us: u64,
    transport_kernel_send_elapsed_us: u64,
    transport_send_total_elapsed_us: u64,
    transport_encoded_bytes_total: u64,
}

#[derive(Debug, Clone, Copy, Default)]
struct SocketBufferConfigV0 {
    requested_send_buffer_bytes: u64,
    requested_recv_buffer_bytes: u64,
    effective_send_buffer_bytes: u64,
    effective_recv_buffer_bytes: u64,
}

#[derive(Debug, Clone, Copy, Default)]
struct SendTransportFrameTimingV0 {
    frame_encode_elapsed_us: u64,
    kernel_send_elapsed_us: u64,
    total_elapsed_us: u64,
    encoded_bytes: u64,
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
    business_transactions_decoded_count: u64,
    aoem_executed_total: u64,
    aoem_execution_error_count: u64,
    aoem_transactions_executed_total: u64,
    ledger_completed_count: u64,
    ledger_transactions_completed_count: u64,
    business_decode_elapsed_ms: u64,
    aoem_execute_elapsed_ms: u64,
    ledger_close_elapsed_ms: u64,
    legacy_native_tx_bytes_total: u64,
    apfl_binary_bytes_total: u64,
    apfl_decode_elapsed_ms: u64,
    canonical_reconstruction_elapsed_ms: u64,
    canonical_reconstruction_count: u64,
    canonical_reconstruction_error_count: u64,
    canonical_tx_hash_match_count: u64,
    canonical_tx_hash_mismatch_count: u64,
    signature_verify_count: u64,
    signature_verify_error_count: u64,
    aoem_apfl_wire_route_enabled: bool,
    aoem_apfl_wire_route_attempt_count: u64,
    aoem_apfl_wire_route_success_count: u64,
    aoem_apfl_wire_route_error_count: u64,
    aoem_apfl_wire_route_capability_missing: bool,
    aoem_apfl_wire_route_fail_reason: Option<String>,
    aoem_apfl_wire_route_last_output_prefix: Option<String>,
    aoem_apfl_occc_delta_contract_present_count: u64,
    aoem_apfl_bulk_enabled: bool,
    aoem_apfl_bulk_size: u64,
    aoem_apfl_bulk_route_count: u64,
    aoem_apfl_bulk_payload_count: u64,
    aoem_apfl_bulk_tx_count: u64,
    aoem_apfl_canonical_materialization_count: u64,
    aoem_apfl_canonical_materialization_elapsed_ms: u64,
    aoem_apfl_canonical_materialization_elapsed_us: u64,
    aoem_apfl_structural_native_transfer_execute_elapsed_ms: u64,
    aoem_apfl_structural_native_transfer_execute_elapsed_us: u64,
    aoem_apfl_hot_plan_executed: bool,
    aoem_apfl_hot_plan_count: u64,
    aoem_apfl_hot_plan_total_writes: u64,
    aoem_apfl_hot_plan_execute_elapsed_ms: u64,
    aoem_apfl_hot_plan_execute_elapsed_us: u64,
    aoem_apfl_ffi_call_elapsed_ms: u64,
    aoem_apfl_ffi_call_elapsed_us: u64,
    aoem_apfl_state_read_elapsed_ms: u64,
    aoem_apfl_state_read_elapsed_us: u64,
    aoem_apfl_state_surface_unwrap_elapsed_ms: u64,
    aoem_apfl_state_surface_unwrap_elapsed_us: u64,
    aoem_apfl_state_surface_read_count: u64,
    aoem_apfl_opcode_114_execute_elapsed_ms: u64,
    aoem_apfl_opcode_114_execute_elapsed_us: u64,
    aoem_apfl_report_json_build_elapsed_ms: u64,
    aoem_apfl_report_json_build_elapsed_us: u64,
    aoem_apfl_state_surface_write_elapsed_ms: u64,
    aoem_apfl_state_surface_write_elapsed_us: u64,
    aoem_apfl_occc_delta_contract_generation_elapsed_ms: u64,
    aoem_apfl_occc_delta_contract_generation_elapsed_us: u64,
    aoem_apfl_signature_verify_elapsed_ms: u64,
    aoem_apfl_signature_verify_elapsed_us: u64,
    aoem_apfl_canonical_hash_parity_elapsed_ms: u64,
    aoem_apfl_canonical_hash_parity_elapsed_us: u64,
    aoem_apfl_ledger_delta_generation_elapsed_ms: u64,
    aoem_apfl_ledger_delta_generation_elapsed_us: u64,
}

#[derive(Debug, Clone, Default)]
struct DecodedNativePayloadsV0 {
    txs: Vec<NovNativeTxWireV1>,
    legacy_bytes_total: u64,
    apfl_binary_bytes_total: u64,
    apfl_decode_elapsed_ms: u64,
    canonical_reconstruction_elapsed_ms: u64,
    canonical_reconstruction_count: u64,
    canonical_reconstruction_error_count: u64,
    canonical_tx_hash_match_count: u64,
    canonical_tx_hash_mismatch_count: u64,
    signature_verify_count: u64,
    signature_verify_error_count: u64,
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
    let txs_per_payload = env_u64("NOVOVM_NOVORUDP_TXS_PER_PAYLOAD", 1).max(1);
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
    let socket_buffers = configure_udp_socket_buffers_v0(&socket, "receiver")?;
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
    let receiver_business_transaction_count = execution
        .business_transactions_decoded_count
        .max(receiver_transport_unique_delivered_count.saturating_mul(txs_per_payload));
    let apfl_binary_bytes_per_tx = if payload_mode.is_apfl_native_transfer() {
        receiver_payload_bytes_total / receiver_business_transaction_count.max(1)
    } else {
        0
    };
    let legacy_bytes_per_tx =
        execution.legacy_native_tx_bytes_total / receiver_business_transaction_count.max(1);
    let apfl_binary_savings_ratio_bps = if payload_mode.is_apfl_native_transfer() {
        execution
            .legacy_native_tx_bytes_total
            .saturating_sub(receiver_payload_bytes_total)
            .saturating_mul(10_000)
            / execution.legacy_native_tx_bytes_total.max(1)
    } else {
        0
    };
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
        "apfl_native_transfer_batch_v0_enabled": payload_mode.is_apfl_native_transfer(),
        "txs_per_payload": txs_per_payload,
        "transport_payloads_delivered": receiver_transport_unique_delivered_count,
        "receiver_transport_data_received_count": stats.data_received,
        "receiver_transport_repair_received_count": stats.repair_received,
        "receiver_transport_unique_delivered_count": receiver_transport_unique_delivered_count,
        "receiver_transport_duplicate_received_count": stats.duplicate_received,
        "receiver_transport_ack_sent_count": stats.ack_sent,
        "receiver_socket_send_buffer_requested_bytes": socket_buffers.requested_send_buffer_bytes,
        "receiver_socket_recv_buffer_requested_bytes": socket_buffers.requested_recv_buffer_bytes,
        "receiver_socket_send_buffer_effective_bytes": socket_buffers.effective_send_buffer_bytes,
        "receiver_socket_recv_buffer_effective_bytes": socket_buffers.effective_recv_buffer_bytes,
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
        "receiver_effective_payload_bytes_per_sec": rate_per_sec_v0(receiver_payload_bytes_total, receiver_transport_delivery_elapsed_ms),
        "receiver_effective_business_tx_per_sec": rate_per_sec_v0(execution.business_transactions_decoded_count, receiver_transport_delivery_elapsed_ms),
        "legacy_native_tx_bytes_total": execution.legacy_native_tx_bytes_total,
        "legacy_bytes_per_tx": legacy_bytes_per_tx,
        "apfl_binary_bytes_total": if payload_mode.is_apfl_native_transfer() { receiver_payload_bytes_total } else { 0u64 },
        "apfl_binary_bytes_per_tx": apfl_binary_bytes_per_tx,
        "apfl_binary_savings_ratio_bps": apfl_binary_savings_ratio_bps,
        "receiver_apfl_decode_elapsed_ms": execution.apfl_decode_elapsed_ms,
        "receiver_canonical_reconstruction_elapsed_ms": execution.canonical_reconstruction_elapsed_ms,
        "receiver_aoem_adapter_elapsed_ms": execution.aoem_execute_elapsed_ms,
        "canonical_reconstruction_count": execution.canonical_reconstruction_count,
        "canonical_reconstruction_error_count": execution.canonical_reconstruction_error_count,
        "canonical_tx_hash_match_count": execution.canonical_tx_hash_match_count,
        "canonical_tx_hash_mismatch_count": execution.canonical_tx_hash_mismatch_count,
        "signature_verify_mode": if payload_mode.is_apfl_native_transfer() { "preserve_original_signature_no_crypto_v0" } else { "not_apfl" },
        "signature_verify_count": execution.signature_verify_count,
        "signature_verify_error_count": execution.signature_verify_error_count,
        "receiver_payloads_per_sec": rate_per_sec_v0(receiver_transport_unique_delivered_count, elapsed_ms),
        "receiver_bytes_per_sec": rate_per_sec_v0(receiver_payload_bytes_total, elapsed_ms),
        "receiver_missing_rate_bps": bps_v0(receiver_transport_final_missing_count, tx_count),
        "receiver_duplicate_bps": bps_v0(stats.duplicate_received, receiver_transport_unique_delivered_count),
        "aoem_execute_enabled": execute_aoem,
        "aoem_execution_mode": if execute_aoem && payload_mode.is_apfl_native_transfer() {
            "aoem_opcode_114_apfl_native_transfer_v1"
        } else if execute_aoem {
            "adapter_projection_v0"
        } else {
            "disabled"
        },
        "aoem_apfl_wire_route_enabled": execution.aoem_apfl_wire_route_enabled,
        "aoem_apfl_wire_route_opcode": if execution.aoem_apfl_wire_route_enabled { 114u64 } else { 0u64 },
        "aoem_apfl_wire_route_attempt_count": execution.aoem_apfl_wire_route_attempt_count,
        "aoem_apfl_wire_route_success_count": execution.aoem_apfl_wire_route_success_count,
        "aoem_apfl_wire_route_error_count": execution.aoem_apfl_wire_route_error_count,
        "aoem_apfl_wire_route_capability_missing": execution.aoem_apfl_wire_route_capability_missing,
        "aoem_apfl_wire_route_fail_reason": execution.aoem_apfl_wire_route_fail_reason.clone(),
        "aoem_apfl_wire_route_last_output_prefix": execution.aoem_apfl_wire_route_last_output_prefix.clone(),
        "aoem_apfl_occc_delta_contract_present_count": execution.aoem_apfl_occc_delta_contract_present_count,
        "aoem_apfl_bulk_enabled": execution.aoem_apfl_bulk_enabled,
        "aoem_apfl_bulk_size": execution.aoem_apfl_bulk_size,
        "aoem_apfl_bulk_route_count": execution.aoem_apfl_bulk_route_count,
        "aoem_apfl_bulk_payload_count": execution.aoem_apfl_bulk_payload_count,
        "aoem_apfl_bulk_tx_count": execution.aoem_apfl_bulk_tx_count,
        "aoem_apfl_canonical_materialization_count": execution.aoem_apfl_canonical_materialization_count,
        "aoem_apfl_canonical_materialization_elapsed_ms": execution.aoem_apfl_canonical_materialization_elapsed_ms,
        "aoem_apfl_canonical_materialization_elapsed_us": execution.aoem_apfl_canonical_materialization_elapsed_us,
        "aoem_apfl_structural_native_transfer_execute_elapsed_ms": execution.aoem_apfl_structural_native_transfer_execute_elapsed_ms,
        "aoem_apfl_structural_native_transfer_execute_elapsed_us": execution.aoem_apfl_structural_native_transfer_execute_elapsed_us,
        "aoem_apfl_hot_plan_executed": execution.aoem_apfl_hot_plan_executed,
        "aoem_apfl_hot_plan_count": execution.aoem_apfl_hot_plan_count,
        "aoem_apfl_hot_plan_total_writes": execution.aoem_apfl_hot_plan_total_writes,
        "aoem_apfl_hot_plan_execute_elapsed_ms": execution.aoem_apfl_hot_plan_execute_elapsed_ms,
        "aoem_apfl_hot_plan_execute_elapsed_us": execution.aoem_apfl_hot_plan_execute_elapsed_us,
        "aoem_apfl_ffi_call_elapsed_ms": execution.aoem_apfl_ffi_call_elapsed_ms,
        "aoem_apfl_ffi_call_elapsed_us": execution.aoem_apfl_ffi_call_elapsed_us,
        "aoem_apfl_state_read_elapsed_ms": execution.aoem_apfl_state_read_elapsed_ms,
        "aoem_apfl_state_read_elapsed_us": execution.aoem_apfl_state_read_elapsed_us,
        "aoem_apfl_state_surface_unwrap_elapsed_ms": execution.aoem_apfl_state_surface_unwrap_elapsed_ms,
        "aoem_apfl_state_surface_unwrap_elapsed_us": execution.aoem_apfl_state_surface_unwrap_elapsed_us,
        "aoem_apfl_state_surface_read_count": execution.aoem_apfl_state_surface_read_count,
        "aoem_apfl_payloads_per_route": if execution.aoem_apfl_wire_route_attempt_count > 0 {
            receiver_transport_unique_delivered_count / execution.aoem_apfl_wire_route_attempt_count
        } else { 0u64 },
        "aoem_apfl_txs_per_route": if execution.aoem_apfl_wire_route_attempt_count > 0 {
            execution.business_transactions_decoded_count / execution.aoem_apfl_wire_route_attempt_count
        } else { 0u64 },
        "aoem_apfl_opcode_114_execute_elapsed_ms": execution.aoem_apfl_opcode_114_execute_elapsed_ms,
        "aoem_apfl_opcode_114_execute_elapsed_us": execution.aoem_apfl_opcode_114_execute_elapsed_us,
        "aoem_apfl_report_json_build_elapsed_ms": execution.aoem_apfl_report_json_build_elapsed_ms,
        "aoem_apfl_report_json_build_elapsed_us": execution.aoem_apfl_report_json_build_elapsed_us,
        "aoem_apfl_state_surface_write_elapsed_ms": execution.aoem_apfl_state_surface_write_elapsed_ms,
        "aoem_apfl_state_surface_write_elapsed_us": execution.aoem_apfl_state_surface_write_elapsed_us,
        "aoem_apfl_occc_delta_contract_generation_elapsed_ms": execution.aoem_apfl_occc_delta_contract_generation_elapsed_ms,
        "aoem_apfl_occc_delta_contract_generation_elapsed_us": execution.aoem_apfl_occc_delta_contract_generation_elapsed_us,
        "aoem_apfl_signature_verify_elapsed_ms": execution.aoem_apfl_signature_verify_elapsed_ms,
        "aoem_apfl_signature_verify_elapsed_us": execution.aoem_apfl_signature_verify_elapsed_us,
        "aoem_apfl_canonical_hash_parity_elapsed_ms": execution.aoem_apfl_canonical_hash_parity_elapsed_ms,
        "aoem_apfl_canonical_hash_parity_elapsed_us": execution.aoem_apfl_canonical_hash_parity_elapsed_us,
        "aoem_apfl_ledger_delta_generation_elapsed_ms": execution.aoem_apfl_ledger_delta_generation_elapsed_ms,
        "aoem_apfl_ledger_delta_generation_elapsed_us": execution.aoem_apfl_ledger_delta_generation_elapsed_us,
        "business_decode_count": execution.business_decode_count,
        "business_decode_error_count": execution.business_decode_error_count,
        "business_transactions_decoded_count": execution.business_transactions_decoded_count,
        "aoem_executed_total": execution.aoem_executed_total,
        "aoem_execution_error_count": execution.aoem_execution_error_count,
        "aoem_transactions_executed_total": execution.aoem_transactions_executed_total,
        "ledger_completed_count": execution.ledger_completed_count,
        "ledger_transactions_completed_count": execution.ledger_transactions_completed_count,
        "business_transactions_per_sec": rate_per_sec_v0(execution.business_transactions_decoded_count, receiver_transport_delivery_elapsed_ms),
        "ledger_transactions_per_sec": rate_per_sec_v0(execution.ledger_transactions_completed_count, receiver_transport_delivery_elapsed_ms),
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
    let txs_per_payload = env_u64("NOVOVM_NOVORUDP_TXS_PER_PAYLOAD", 1).max(1);
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
    let socket_buffers = configure_udp_socket_buffers_v0(&socket, "sender")?;
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
    let payload_build_start = Instant::now();
    let payloads = (0..tx_count)
        .map(|sequence| payload_for_sequence_v0(payload_mode, sequence, txs_per_payload))
        .collect::<Result<Vec<_>>>()?;
    let sender_batch_build_elapsed_ms = payload_build_start.elapsed().as_millis() as u64;
    let sender_apfl_encode_elapsed_ms = if payload_mode.is_apfl_native_transfer() {
        sender_batch_build_elapsed_ms
    } else {
        0
    };

    let primary_send_start = Instant::now();
    for sequence in 0..tx_count {
        stats.data_send_attempt = stats.data_send_attempt.saturating_add(1);
        if loss.drops_data_sequence(sequence) {
            stats.data_loss_injected = stats.data_loss_injected.saturating_add(1);
            continue;
        }
        let copy_start = Instant::now();
        let payload = payloads[sequence as usize].clone();
        let payload_len = payload.len() as u64;
        stats.payload_copy_elapsed_ms = stats
            .payload_copy_elapsed_ms
            .saturating_add(copy_start.elapsed().as_millis() as u64);
        let socket_send_start = Instant::now();
        let timing = send_transport_frame(
            &socket,
            target,
            NovoRudpTransportFrameKindV0::Data,
            session_id,
            sequence,
            payload,
            0,
        )?;
        stats.socket_send_elapsed_ms = stats
            .socket_send_elapsed_ms
            .saturating_add(socket_send_start.elapsed().as_millis() as u64);
        stats.transport_frame_encode_elapsed_us = stats
            .transport_frame_encode_elapsed_us
            .saturating_add(timing.frame_encode_elapsed_us);
        stats.transport_kernel_send_elapsed_us = stats
            .transport_kernel_send_elapsed_us
            .saturating_add(timing.kernel_send_elapsed_us);
        stats.transport_send_total_elapsed_us = stats
            .transport_send_total_elapsed_us
            .saturating_add(timing.total_elapsed_us);
        stats.transport_encoded_bytes_total = stats
            .transport_encoded_bytes_total
            .saturating_add(timing.encoded_bytes);
        stats.data_payload_bytes_sent_total = stats
            .data_payload_bytes_sent_total
            .saturating_add(payload_len);
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
    let sender_primary_send_elapsed_ms = primary_send_start.elapsed().as_millis() as u64;

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
                        let copy_start = Instant::now();
                        let payload = payloads[sequence as usize].clone();
                        let payload_len = payload.len() as u64;
                        stats.payload_copy_elapsed_ms = stats
                            .payload_copy_elapsed_ms
                            .saturating_add(copy_start.elapsed().as_millis() as u64);
                        let socket_send_start = Instant::now();
                        let timing = send_transport_frame(
                            &socket,
                            target,
                            NovoRudpTransportFrameKindV0::Repair,
                            session_id,
                            sequence,
                            payload,
                            ack_epoch,
                        )?;
                        stats.socket_send_elapsed_ms = stats
                            .socket_send_elapsed_ms
                            .saturating_add(socket_send_start.elapsed().as_millis() as u64);
                        stats.transport_frame_encode_elapsed_us = stats
                            .transport_frame_encode_elapsed_us
                            .saturating_add(timing.frame_encode_elapsed_us);
                        stats.transport_kernel_send_elapsed_us = stats
                            .transport_kernel_send_elapsed_us
                            .saturating_add(timing.kernel_send_elapsed_us);
                        stats.transport_send_total_elapsed_us = stats
                            .transport_send_total_elapsed_us
                            .saturating_add(timing.total_elapsed_us);
                        stats.transport_encoded_bytes_total = stats
                            .transport_encoded_bytes_total
                            .saturating_add(timing.encoded_bytes);
                        stats.repair_payload_bytes_sent_total = stats
                            .repair_payload_bytes_sent_total
                            .saturating_add(payload_len);
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
        "apfl_native_transfer_batch_v0_enabled": payload_mode.is_apfl_native_transfer(),
        "txs_per_payload": txs_per_payload,
        "transport_payloads_sent": stats.data_sent,
        "business_transactions_sent_count": stats.data_sent.saturating_mul(txs_per_payload),
        "transport_loss_injection_enabled": loss.enabled(),
        "transport_loss_injection_data_loss_bps": loss.data_loss_bps,
        "transport_loss_injection_seed": loss.seed,
        "sender_transport_data_pacing_enabled": data_pacing_chunk_size > 0 && data_pacing_chunk_gap_ms > 0,
        "sender_transport_data_pacing_chunk_size": data_pacing_chunk_size,
        "sender_transport_data_pacing_chunk_gap_ms": data_pacing_chunk_gap_ms,
        "sender_transport_data_pacing_sleep_count": stats.data_pacing_sleep_count,
        "sender_socket_send_buffer_requested_bytes": socket_buffers.requested_send_buffer_bytes,
        "sender_socket_recv_buffer_requested_bytes": socket_buffers.requested_recv_buffer_bytes,
        "sender_socket_send_buffer_effective_bytes": socket_buffers.effective_send_buffer_bytes,
        "sender_socket_recv_buffer_effective_bytes": socket_buffers.effective_recv_buffer_bytes,
        "sender_transport_data_send_attempt_count": stats.data_send_attempt,
        "sender_transport_data_loss_injected_count": stats.data_loss_injected,
        "sender_elapsed_ms": elapsed_ms,
        "sender_batch_build_elapsed_ms": sender_batch_build_elapsed_ms,
        "sender_apfl_encode_elapsed_ms": sender_apfl_encode_elapsed_ms,
        "sender_payload_copy_elapsed_ms": stats.payload_copy_elapsed_ms,
        "sender_socket_send_elapsed_ms": stats.socket_send_elapsed_ms,
        "sender_transport_frame_encode_elapsed_us": stats.transport_frame_encode_elapsed_us,
        "sender_transport_frame_encode_elapsed_ms": stats.transport_frame_encode_elapsed_us / 1000,
        "sender_transport_kernel_send_elapsed_us": stats.transport_kernel_send_elapsed_us,
        "sender_transport_kernel_send_elapsed_ms": stats.transport_kernel_send_elapsed_us / 1000,
        "sender_transport_send_total_elapsed_us": stats.transport_send_total_elapsed_us,
        "sender_transport_send_total_elapsed_ms": stats.transport_send_total_elapsed_us / 1000,
        "sender_transport_encoded_bytes_total": stats.transport_encoded_bytes_total,
        "sender_primary_send_elapsed_ms": sender_primary_send_elapsed_ms,
        "sender_data_payload_bytes_sent_total": stats.data_payload_bytes_sent_total,
        "sender_repair_payload_bytes_sent_total": stats.repair_payload_bytes_sent_total,
        "sender_data_payload_bytes_total": sender_data_payload_bytes_total,
        "sender_apfl_binary_bytes_total": if payload_mode.is_apfl_native_transfer() { stats.data_payload_bytes_sent_total } else { 0u64 },
        "sender_apfl_binary_bytes_per_tx": if payload_mode.is_apfl_native_transfer() {
            stats.data_payload_bytes_sent_total / stats.data_sent.saturating_mul(txs_per_payload).max(1)
        } else {
            0u64
        },
        "sender_data_frames_per_sec": rate_per_sec_v0(stats.data_sent, elapsed_ms),
        "sender_data_bytes_per_sec": rate_per_sec_v0(sender_data_payload_bytes_total, elapsed_ms),
        "sender_effective_payload_bytes_per_sec": rate_per_sec_v0(stats.data_payload_bytes_sent_total, sender_primary_send_elapsed_ms),
        "sender_effective_business_tx_per_sec": rate_per_sec_v0(stats.data_sent.saturating_mul(txs_per_payload), sender_primary_send_elapsed_ms),
        "sender_ack_count": stats.ack_received,
        "sender_repair_amplification_bps": bps_v0(stats.repair_sent, stats.data_loss_injected),
        "sender_business_transactions_per_sec": rate_per_sec_v0(stats.data_sent.saturating_mul(txs_per_payload), elapsed_ms),
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
) -> Result<SendTransportFrameTimingV0> {
    let total_start = Instant::now();
    let encode_start = Instant::now();
    let frame =
        NovoRudpTransportFrameV0::new(kind, session_id, 1, sequence, sequence, ack_epoch, payload);
    let encoded = frame.encode();
    let frame_encode_elapsed_us = encode_start.elapsed().as_micros() as u64;
    let send_start = Instant::now();
    socket
        .send_to(encoded.as_slice(), target)
        .with_context(|| format!("send {kind:?} frame failed"))?;
    let kernel_send_elapsed_us = send_start.elapsed().as_micros() as u64;
    Ok(SendTransportFrameTimingV0 {
        frame_encode_elapsed_us,
        kernel_send_elapsed_us,
        total_elapsed_us: total_start.elapsed().as_micros() as u64,
        encoded_bytes: encoded.len() as u64,
    })
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

fn configure_udp_socket_buffers_v0(socket: &UdpSocket, role: &str) -> Result<SocketBufferConfigV0> {
    let requested_send_buffer_bytes =
        env_u64("NOVOVM_NOVORUDP_NETWORK_ONLY_SOCKET_SNDBUF_BYTES", 0);
    let requested_recv_buffer_bytes =
        env_u64("NOVOVM_NOVORUDP_NETWORK_ONLY_SOCKET_RCVBUF_BYTES", 0);
    let sock = SockRef::from(socket);
    if requested_send_buffer_bytes > 0 {
        let size = usize::try_from(requested_send_buffer_bytes).unwrap_or(usize::MAX);
        sock.set_send_buffer_size(size)
            .with_context(|| format!("set {role} udp send buffer failed"))?;
    }
    if requested_recv_buffer_bytes > 0 {
        let size = usize::try_from(requested_recv_buffer_bytes).unwrap_or(usize::MAX);
        sock.set_recv_buffer_size(size)
            .with_context(|| format!("set {role} udp recv buffer failed"))?;
    }
    let effective_send_buffer_bytes =
        sock.send_buffer_size()
            .with_context(|| format!("read {role} udp send buffer failed"))? as u64;
    let effective_recv_buffer_bytes =
        sock.recv_buffer_size()
            .with_context(|| format!("read {role} udp recv buffer failed"))? as u64;
    Ok(SocketBufferConfigV0 {
        requested_send_buffer_bytes,
        requested_recv_buffer_bytes,
        effective_send_buffer_bytes,
        effective_recv_buffer_bytes,
    })
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

fn payload_for_sequence_v0(
    mode: PayloadModeV0,
    sequence: u64,
    txs_per_payload: u64,
) -> Result<Vec<u8>> {
    match mode {
        PayloadModeV0::Opaque => Ok(opaque_payload_v0(sequence)),
        PayloadModeV0::EvmTransactions | PayloadModeV0::NativeTransferApflV0 => {
            let native_payload = match mode {
                PayloadModeV0::EvmTransactions => {
                    native_tx_payloads_for_payload_v0(sequence, txs_per_payload)?
                }
                PayloadModeV0::NativeTransferApflV0 => {
                    apfl_native_transfer_batch_payload_v0(sequence, txs_per_payload)?
                }
                PayloadModeV0::Opaque => unreachable!("opaque payload handled above"),
            };
            let msg = ProtocolMessage::EvmNative(EvmNativeMessage::Transactions {
                from: NodeId(1),
                chain_id: 1,
                tx_hash: tx_hash_for_sequence_v0(sequence),
                tx_count: txs_per_payload,
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
    if !mode.is_business_payload() {
        return ReceiverExecutionSummaryV0::default();
    }
    let mut summary = ReceiverExecutionSummaryV0::default();
    let mut aoem_apfl_session = None;
    let mut aoem_apfl_pending_payloads = Vec::<(u64, Vec<u8>, u64)>::new();
    let aoem_apfl_bulk_size = env_u64("NOVOVM_NOVORUDP_AOEM_APFL_BULK_SIZE", 128).max(1) as usize;
    if mode.is_apfl_native_transfer() && execute_aoem {
        summary.aoem_apfl_wire_route_enabled = true;
        summary.aoem_apfl_bulk_enabled = true;
        summary.aoem_apfl_bulk_size = aoem_apfl_bulk_size as u64;
        match open_aoem_apfl_native_transfer_session_v0() {
            Ok(session) => aoem_apfl_session = Some(session),
            Err(err) => {
                summary.aoem_apfl_wire_route_capability_missing = true;
                summary.aoem_apfl_wire_route_fail_reason = Some(err.to_string());
            }
        }
    }
    for (sequence, payload) in delivered {
        let business_decode_start = Instant::now();
        let decoded = business_decode_v0(payload.as_slice());
        summary.business_decode_elapsed_ms = summary
            .business_decode_elapsed_ms
            .saturating_add(business_decode_start.elapsed().as_millis() as u64);
        match decoded {
            Ok(ProtocolMessage::EvmNative(EvmNativeMessage::Transactions {
                payload: native_payload,
                tx_count,
                ..
            })) => {
                summary.business_decode_count = summary.business_decode_count.saturating_add(1);
                if mode.is_apfl_native_transfer() && execute_aoem {
                    summary.apfl_binary_bytes_total = summary
                        .apfl_binary_bytes_total
                        .saturating_add(native_payload.len() as u64);
                    summary.business_transactions_decoded_count = summary
                        .business_transactions_decoded_count
                        .saturating_add(tx_count);
                    if aoem_apfl_session.is_none() {
                        summary.aoem_apfl_wire_route_error_count =
                            summary.aoem_apfl_wire_route_error_count.saturating_add(1);
                        summary.aoem_execution_error_count =
                            summary.aoem_execution_error_count.saturating_add(tx_count);
                        continue;
                    }
                    aoem_apfl_pending_payloads.push((*sequence, native_payload, tx_count));
                    continue;
                }
                let native_txs = match decode_native_tx_payloads_for_payload_v0(
                    native_payload.as_slice(),
                    tx_count,
                ) {
                    Ok(decoded_native) => {
                        summary.legacy_native_tx_bytes_total = summary
                            .legacy_native_tx_bytes_total
                            .saturating_add(decoded_native.legacy_bytes_total);
                        summary.apfl_binary_bytes_total = summary
                            .apfl_binary_bytes_total
                            .saturating_add(decoded_native.apfl_binary_bytes_total);
                        summary.apfl_decode_elapsed_ms = summary
                            .apfl_decode_elapsed_ms
                            .saturating_add(decoded_native.apfl_decode_elapsed_ms);
                        summary.canonical_reconstruction_elapsed_ms = summary
                            .canonical_reconstruction_elapsed_ms
                            .saturating_add(decoded_native.canonical_reconstruction_elapsed_ms);
                        summary.canonical_reconstruction_count = summary
                            .canonical_reconstruction_count
                            .saturating_add(decoded_native.canonical_reconstruction_count);
                        summary.canonical_reconstruction_error_count = summary
                            .canonical_reconstruction_error_count
                            .saturating_add(decoded_native.canonical_reconstruction_error_count);
                        summary.canonical_tx_hash_match_count = summary
                            .canonical_tx_hash_match_count
                            .saturating_add(decoded_native.canonical_tx_hash_match_count);
                        summary.canonical_tx_hash_mismatch_count = summary
                            .canonical_tx_hash_mismatch_count
                            .saturating_add(decoded_native.canonical_tx_hash_mismatch_count);
                        summary.signature_verify_count = summary
                            .signature_verify_count
                            .saturating_add(decoded_native.signature_verify_count);
                        summary.signature_verify_error_count = summary
                            .signature_verify_error_count
                            .saturating_add(decoded_native.signature_verify_error_count);
                        decoded_native.txs
                    }
                    Err(_) => {
                        summary.business_decode_error_count =
                            summary.business_decode_error_count.saturating_add(1);
                        continue;
                    }
                };
                summary.business_transactions_decoded_count = summary
                    .business_transactions_decoded_count
                    .saturating_add(native_txs.len() as u64);
                if execute_aoem {
                    let aoem_start = Instant::now();
                    for native_tx in native_txs {
                        match nov_native_tx_to_adapter_tx_ir_v1(&native_tx) {
                            Ok(_) => {
                                summary.aoem_executed_total =
                                    summary.aoem_executed_total.saturating_add(1);
                                summary.aoem_transactions_executed_total =
                                    summary.aoem_transactions_executed_total.saturating_add(1);
                                summary.ledger_completed_count =
                                    summary.ledger_completed_count.saturating_add(1);
                                summary.ledger_transactions_completed_count = summary
                                    .ledger_transactions_completed_count
                                    .saturating_add(1);
                            }
                            Err(_) => {
                                summary.aoem_execution_error_count =
                                    summary.aoem_execution_error_count.saturating_add(1);
                            }
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
    if mode.is_apfl_native_transfer() && execute_aoem && !aoem_apfl_pending_payloads.is_empty() {
        if let Some(session) = aoem_apfl_session.as_ref() {
            for (chunk_index, chunk) in aoem_apfl_pending_payloads
                .chunks(aoem_apfl_bulk_size)
                .enumerate()
            {
                let first_sequence = chunk.first().map(|entry| entry.0).unwrap_or(0);
                let last_sequence = chunk.last().map(|entry| entry.0).unwrap_or(first_sequence);
                let tx_count = chunk
                    .iter()
                    .fold(0u64, |acc, entry| acc.saturating_add(entry.2));
                let output_prefix =
                    aoem_apfl_bulk_output_prefix_v0(first_sequence, last_sequence, chunk_index);
                let payload_refs = chunk
                    .iter()
                    .map(|entry| entry.1.as_slice())
                    .collect::<Vec<_>>();
                summary.aoem_apfl_wire_route_attempt_count =
                    summary.aoem_apfl_wire_route_attempt_count.saturating_add(1);
                summary.aoem_apfl_bulk_route_count =
                    summary.aoem_apfl_bulk_route_count.saturating_add(1);
                summary.aoem_apfl_bulk_payload_count = summary
                    .aoem_apfl_bulk_payload_count
                    .saturating_add(chunk.len() as u64);
                summary.aoem_apfl_bulk_tx_count =
                    summary.aoem_apfl_bulk_tx_count.saturating_add(tx_count);
                summary.aoem_apfl_wire_route_last_output_prefix = Some(output_prefix.clone());
                let aoem_start = Instant::now();
                match session.execute_apfl_native_transfer_bulk_wire_v1(
                    output_prefix.as_str(),
                    payload_refs.as_slice(),
                ) {
                    Ok(report) => {
                        summary.aoem_apfl_wire_route_success_count =
                            summary.aoem_apfl_wire_route_success_count.saturating_add(1);
                        apply_aoem_apfl_native_transfer_report_v0(&mut summary, &report);
                    }
                    Err(err) => {
                        summary.aoem_apfl_wire_route_error_count =
                            summary.aoem_apfl_wire_route_error_count.saturating_add(1);
                        summary.aoem_execution_error_count =
                            summary.aoem_execution_error_count.saturating_add(tx_count);
                        if summary.aoem_apfl_wire_route_fail_reason.is_none() {
                            summary.aoem_apfl_wire_route_fail_reason = Some(err.to_string());
                        }
                    }
                }
                summary.aoem_execute_elapsed_ms = summary
                    .aoem_execute_elapsed_ms
                    .saturating_add(aoem_start.elapsed().as_millis() as u64);
            }
        }
    }
    summary.ledger_close_elapsed_ms = summary.aoem_execute_elapsed_ms;
    summary
}

fn open_aoem_apfl_native_transfer_session_v0() -> Result<novovm_exec::AoemExecSession> {
    let runtime = AoemRuntimeConfig::from_env().context("aoem runtime config unavailable")?;
    let facade = AoemExecFacade::open_with_runtime(&runtime).context("aoem runtime open failed")?;
    if !facade
        .supports_apfl_native_transfer_wire_v1()
        .context("aoem apfl native transfer capability probe failed")?
    {
        bail!("capability_missing: aoem opcode 114 apfl native transfer wire route unavailable");
    }
    facade
        .create_session()
        .context("aoem apfl native transfer session create failed")
}

fn aoem_apfl_bulk_output_prefix_v0(
    first_sequence: u64,
    last_sequence: u64,
    chunk_index: usize,
) -> String {
    let base = env_string("NOVOVM_NOVORUDP_AOEM_APFL_OUTPUT_PREFIX")
        .unwrap_or_else(|| format!("novovm/novorudp/{}/aoem", session_id_hex_v0()));
    format!("{base}/bulk-{chunk_index}-{first_sequence}-{last_sequence}")
}

fn apply_aoem_apfl_native_transfer_report_v0(
    summary: &mut ReceiverExecutionSummaryV0,
    report: &novovm_exec::AoemApflNativeTransferWireReportV1,
) {
    let tx_count = json_u64_surface_v0(
        &report.result,
        &[
            "tx_count",
            "business_transactions_decoded_count",
            "ledger_transactions_completed_count",
        ],
    )
    .max(json_u64_surface_v0(
        &report.metadata,
        &["tx_count", "transactions_count"],
    ));
    summary.aoem_executed_total = summary.aoem_executed_total.saturating_add(tx_count);
    summary.aoem_transactions_executed_total =
        summary.aoem_transactions_executed_total.saturating_add(
            json_u64_surface_v0(
                &report.result,
                &["aoem_transactions_executed_total", "tx_count"],
            )
            .max(tx_count),
        );
    summary.ledger_completed_count = summary.ledger_completed_count.saturating_add(
        json_u64_surface_v0(
            &report.result,
            &[
                "ledger_transactions_completed_count",
                "ledger_completed_count",
                "tx_count",
            ],
        )
        .max(tx_count),
    );
    summary.ledger_transactions_completed_count =
        summary.ledger_transactions_completed_count.saturating_add(
            json_u64_surface_v0(
                &report.result,
                &["ledger_transactions_completed_count", "tx_count"],
            )
            .max(tx_count),
        );
    summary.legacy_native_tx_bytes_total = summary.legacy_native_tx_bytes_total.saturating_add(
        json_u64_surface_v0(
            &report.result,
            &["legacy_bytes_total", "legacy_native_tx_bytes_total"],
        )
        .max(json_u64_surface_v0(
            &report.metadata,
            &["legacy_bytes_total", "legacy_native_tx_bytes_total"],
        )),
    );
    summary.apfl_decode_elapsed_ms =
        summary
            .apfl_decode_elapsed_ms
            .saturating_add(json_u64_surface_v0(
                &report.metadata,
                &["apfl_decode_elapsed_ms", "decode_elapsed_ms"],
            ));
    summary.canonical_reconstruction_elapsed_ms = summary
        .canonical_reconstruction_elapsed_ms
        .saturating_add(json_u64_surface_v0(
            &report.result,
            &[
                "canonical_reconstruction_elapsed_ms",
                "canonical_materialization_elapsed_ms",
            ],
        ));
    summary.canonical_reconstruction_count =
        summary
            .canonical_reconstruction_count
            .saturating_add(json_u64_surface_v0(
                &report.result,
                &[
                    "canonical_materialization_count",
                    "canonical_reconstruction_count",
                    "canonical_tx_hash_match_count",
                ],
            ));
    summary.aoem_apfl_bulk_payload_count = summary.aoem_apfl_bulk_payload_count.max(
        json_u64_surface_v0(&report.result, &["bulk_payload_count"]).max(json_u64_surface_v0(
            &report.metadata,
            &["bulk_payload_count"],
        )),
    );
    summary.aoem_apfl_bulk_tx_count = summary.aoem_apfl_bulk_tx_count.max(
        json_u64_surface_v0(&report.result, &["bulk_tx_count"])
            .max(json_u64_surface_v0(&report.metadata, &["bulk_tx_count"])),
    );
    summary.aoem_apfl_canonical_materialization_count = summary
        .aoem_apfl_canonical_materialization_count
        .saturating_add(
            json_u64_surface_v0(&report.result, &["canonical_materialization_count"]).max(
                json_u64_surface_v0(&report.metadata, &["canonical_materialization_count"]),
            ),
        );
    summary.aoem_apfl_canonical_materialization_elapsed_ms = summary
        .aoem_apfl_canonical_materialization_elapsed_ms
        .saturating_add(
            json_u64_surface_v0(&report.result, &["canonical_materialization_elapsed_ms"]).max(
                json_u64_surface_v0(&report.metadata, &["canonical_materialization_elapsed_ms"]),
            ),
        );
    summary.aoem_apfl_canonical_materialization_elapsed_us = summary
        .aoem_apfl_canonical_materialization_elapsed_us
        .saturating_add(
            json_u64_surface_v0(&report.result, &["canonical_materialization_elapsed_us"]).max(
                json_u64_surface_v0(&report.metadata, &["canonical_materialization_elapsed_us"]),
            ),
        );
    summary.aoem_apfl_structural_native_transfer_execute_elapsed_ms = summary
        .aoem_apfl_structural_native_transfer_execute_elapsed_ms
        .saturating_add(
            json_u64_surface_v0(
                &report.result,
                &["structural_native_transfer_execute_elapsed_ms"],
            )
            .max(json_u64_surface_v0(
                &report.metadata,
                &["structural_native_transfer_execute_elapsed_ms"],
            )),
        );
    summary.aoem_apfl_structural_native_transfer_execute_elapsed_us = summary
        .aoem_apfl_structural_native_transfer_execute_elapsed_us
        .saturating_add(
            json_u64_surface_v0(
                &report.result,
                &["structural_native_transfer_execute_elapsed_us"],
            )
            .max(json_u64_surface_v0(
                &report.metadata,
                &["structural_native_transfer_execute_elapsed_us"],
            )),
        );
    let hot_plan_executed = json_bool_surface_v0(&report.result, &["aoem_hot_plan_executed"])
        || json_bool_surface_v0(&report.metadata, &["aoem_hot_plan_executed"])
        || json_bool_surface_v0(&report.occc_delta_contract, &["aoem_hot_plan_executed"]);
    summary.aoem_apfl_hot_plan_executed = summary.aoem_apfl_hot_plan_executed || hot_plan_executed;
    summary.aoem_apfl_hot_plan_count = summary.aoem_apfl_hot_plan_count.saturating_add(
        json_u64_surface_v0(&report.result, &["aoem_hot_plan_count"]).max(json_u64_surface_v0(
            &report.occc_delta_contract,
            &["aoem_hot_plan_count"],
        )),
    );
    summary.aoem_apfl_hot_plan_total_writes =
        summary.aoem_apfl_hot_plan_total_writes.saturating_add(
            json_u64_surface_v0(&report.result, &["aoem_hot_plan_total_writes"]).max(
                json_u64_surface_v0(&report.occc_delta_contract, &["aoem_hot_plan_total_writes"]),
            ),
        );
    summary.aoem_apfl_hot_plan_execute_elapsed_ms = summary
        .aoem_apfl_hot_plan_execute_elapsed_ms
        .saturating_add(
            json_u64_surface_v0(&report.result, &["aoem_hot_plan_execute_elapsed_ms"])
                .max(json_u64_surface_v0(
                    &report.metadata,
                    &["aoem_hot_plan_execute_elapsed_ms"],
                ))
                .max(json_u64_surface_v0(
                    &report.occc_delta_contract,
                    &["aoem_hot_plan_execute_elapsed_ms"],
                )),
        );
    summary.aoem_apfl_hot_plan_execute_elapsed_us = summary
        .aoem_apfl_hot_plan_execute_elapsed_us
        .saturating_add(
            json_u64_surface_v0(&report.result, &["aoem_hot_plan_execute_elapsed_us"])
                .max(json_u64_surface_v0(
                    &report.metadata,
                    &["aoem_hot_plan_execute_elapsed_us"],
                ))
                .max(json_u64_surface_v0(
                    &report.occc_delta_contract,
                    &["aoem_hot_plan_execute_elapsed_us"],
                )),
        );
    summary.canonical_reconstruction_error_count = summary
        .canonical_reconstruction_error_count
        .saturating_add(json_u64_surface_v0(
            &report.result,
            &["canonical_reconstruction_error_count"],
        ));
    summary.canonical_tx_hash_match_count =
        summary
            .canonical_tx_hash_match_count
            .saturating_add(json_u64_surface_v0(
                &report.result,
                &[
                    "canonical_tx_hash_match_count",
                    "canonical_hash_match_count",
                ],
            ));
    summary.canonical_tx_hash_mismatch_count = summary
        .canonical_tx_hash_mismatch_count
        .saturating_add(json_u64_surface_v0(
            &report.result,
            &[
                "canonical_tx_hash_mismatch_count",
                "canonical_hash_mismatch_count",
            ],
        ));
    summary.signature_verify_count = summary.signature_verify_count.saturating_add(
        json_u64_surface_v0(
            &report.result,
            &[
                "signature_verify_count",
                "signature_checked_count",
                "tx_count",
            ],
        )
        .max(tx_count),
    );
    summary.signature_verify_error_count =
        summary
            .signature_verify_error_count
            .saturating_add(json_u64_surface_v0(
                &report.result,
                &["signature_verify_error_count", "signature_errors"],
            ));
    if !report.occc_delta_contract.is_null() {
        summary.aoem_apfl_occc_delta_contract_present_count = summary
            .aoem_apfl_occc_delta_contract_present_count
            .saturating_add(1);
    }
    summary.aoem_apfl_ffi_call_elapsed_ms = summary
        .aoem_apfl_ffi_call_elapsed_ms
        .saturating_add(report.ffi_call_elapsed_ms);
    summary.aoem_apfl_ffi_call_elapsed_us = summary
        .aoem_apfl_ffi_call_elapsed_us
        .saturating_add(report.ffi_call_elapsed_us);
    summary.aoem_apfl_state_read_elapsed_ms = summary
        .aoem_apfl_state_read_elapsed_ms
        .saturating_add(report.state_read_elapsed_ms);
    summary.aoem_apfl_state_read_elapsed_us = summary
        .aoem_apfl_state_read_elapsed_us
        .saturating_add(report.state_read_elapsed_us);
    summary.aoem_apfl_state_surface_unwrap_elapsed_ms = summary
        .aoem_apfl_state_surface_unwrap_elapsed_ms
        .saturating_add(report.state_surface_unwrap_elapsed_ms);
    summary.aoem_apfl_state_surface_unwrap_elapsed_us = summary
        .aoem_apfl_state_surface_unwrap_elapsed_us
        .saturating_add(report.state_surface_unwrap_elapsed_us);
    summary.aoem_apfl_state_surface_read_count = summary
        .aoem_apfl_state_surface_read_count
        .saturating_add(report.state_surface_read_count);
    summary.aoem_apfl_opcode_114_execute_elapsed_ms = summary
        .aoem_apfl_opcode_114_execute_elapsed_ms
        .saturating_add(
            json_u64_surface_v0(&report.result, &["aoem_opcode_114_execute_elapsed_ms"]).max(
                json_u64_surface_v0(&report.metadata, &["aoem_opcode_114_execute_elapsed_ms"]),
            ),
        );
    summary.aoem_apfl_opcode_114_execute_elapsed_us = summary
        .aoem_apfl_opcode_114_execute_elapsed_us
        .saturating_add(
            json_u64_surface_v0(&report.result, &["aoem_opcode_114_execute_elapsed_us"]).max(
                json_u64_surface_v0(&report.metadata, &["aoem_opcode_114_execute_elapsed_us"]),
            ),
        );
    summary.aoem_apfl_report_json_build_elapsed_ms = summary
        .aoem_apfl_report_json_build_elapsed_ms
        .saturating_add(json_u64_surface_v0(
            &report.metadata,
            &["report_json_build_elapsed_ms"],
        ));
    summary.aoem_apfl_report_json_build_elapsed_us = summary
        .aoem_apfl_report_json_build_elapsed_us
        .saturating_add(json_u64_surface_v0(
            &report.metadata,
            &["report_json_build_elapsed_us"],
        ));
    summary.aoem_apfl_state_surface_write_elapsed_ms = summary
        .aoem_apfl_state_surface_write_elapsed_ms
        .saturating_add(json_u64_surface_v0(
            &report.metadata,
            &["state_surface_write_elapsed_ms"],
        ));
    summary.aoem_apfl_state_surface_write_elapsed_us = summary
        .aoem_apfl_state_surface_write_elapsed_us
        .saturating_add(json_u64_surface_v0(
            &report.metadata,
            &["state_surface_write_elapsed_us"],
        ));
    summary.aoem_apfl_occc_delta_contract_generation_elapsed_ms = summary
        .aoem_apfl_occc_delta_contract_generation_elapsed_ms
        .saturating_add(
            json_u64_surface_v0(
                &report.metadata,
                &["occc_delta_contract_generation_elapsed_ms"],
            )
            .max(json_u64_surface_v0(
                &report.result,
                &["occc_delta_contract_generation_elapsed_ms"],
            )),
        );
    summary.aoem_apfl_occc_delta_contract_generation_elapsed_us = summary
        .aoem_apfl_occc_delta_contract_generation_elapsed_us
        .saturating_add(
            json_u64_surface_v0(
                &report.metadata,
                &["occc_delta_contract_generation_elapsed_us"],
            )
            .max(json_u64_surface_v0(
                &report.result,
                &["occc_delta_contract_generation_elapsed_us"],
            )),
        );
    summary.aoem_apfl_signature_verify_elapsed_ms = summary
        .aoem_apfl_signature_verify_elapsed_ms
        .saturating_add(
            json_u64_surface_v0(&report.result, &["signature_verify_elapsed_ms"]).max(
                json_u64_surface_v0(&report.metadata, &["signature_verify_elapsed_ms"]),
            ),
        );
    summary.aoem_apfl_signature_verify_elapsed_us = summary
        .aoem_apfl_signature_verify_elapsed_us
        .saturating_add(
            json_u64_surface_v0(&report.result, &["signature_verify_elapsed_us"]).max(
                json_u64_surface_v0(&report.metadata, &["signature_verify_elapsed_us"]),
            ),
        );
    summary.aoem_apfl_canonical_hash_parity_elapsed_ms = summary
        .aoem_apfl_canonical_hash_parity_elapsed_ms
        .saturating_add(
            json_u64_surface_v0(&report.result, &["canonical_hash_parity_elapsed_ms"]).max(
                json_u64_surface_v0(&report.metadata, &["canonical_hash_parity_elapsed_ms"]),
            ),
        );
    summary.aoem_apfl_canonical_hash_parity_elapsed_us = summary
        .aoem_apfl_canonical_hash_parity_elapsed_us
        .saturating_add(
            json_u64_surface_v0(&report.result, &["canonical_hash_parity_elapsed_us"]).max(
                json_u64_surface_v0(&report.metadata, &["canonical_hash_parity_elapsed_us"]),
            ),
        );
    summary.aoem_apfl_ledger_delta_generation_elapsed_ms = summary
        .aoem_apfl_ledger_delta_generation_elapsed_ms
        .saturating_add(
            json_u64_surface_v0(&report.result, &["ledger_delta_generation_elapsed_ms"]).max(
                json_u64_surface_v0(&report.metadata, &["ledger_delta_generation_elapsed_ms"]),
            ),
        );
    summary.aoem_apfl_ledger_delta_generation_elapsed_us = summary
        .aoem_apfl_ledger_delta_generation_elapsed_us
        .saturating_add(
            json_u64_surface_v0(&report.result, &["ledger_delta_generation_elapsed_us"]).max(
                json_u64_surface_v0(&report.metadata, &["ledger_delta_generation_elapsed_us"]),
            ),
        );
}

fn json_u64_surface_v0(value: &serde_json::Value, keys: &[&str]) -> u64 {
    for key in keys {
        if let Some(v) = json_u64_at_key_v0(value, key) {
            return v;
        }
        if let Some(inner) = value.get("value").and_then(|v| json_u64_at_key_v0(v, key)) {
            return inner;
        }
        if let Some(inner) = value.get("data").and_then(|v| json_u64_at_key_v0(v, key)) {
            return inner;
        }
    }
    0
}

fn json_bool_surface_v0(value: &serde_json::Value, keys: &[&str]) -> bool {
    for key in keys {
        if let Some(v) = json_bool_at_key_v0(value, key) {
            return v;
        }
        if let Some(inner) = value.get("value").and_then(|v| json_bool_at_key_v0(v, key)) {
            return inner;
        }
        if let Some(inner) = value.get("data").and_then(|v| json_bool_at_key_v0(v, key)) {
            return inner;
        }
    }
    false
}

fn json_u64_at_key_v0(value: &serde_json::Value, key: &str) -> Option<u64> {
    value
        .get(key)
        .and_then(|v| v.as_u64().or_else(|| v.as_str()?.parse::<u64>().ok()))
}

fn json_bool_at_key_v0(value: &serde_json::Value, key: &str) -> Option<bool> {
    value.get(key).and_then(|v| {
        v.as_bool().or_else(|| match v.as_str()? {
            "true" => Some(true),
            "false" => Some(false),
            _ => None,
        })
    })
}

fn native_tx_for_sequence_v0(sequence: u64) -> Result<NovNativeTxWireV1> {
    native_tx_for_sequence_with_signature_v0(
        sequence,
        [(sequence.saturating_add(1) & 0xff) as u8; 32],
    )
}

fn native_tx_for_sequence_with_signature_v0(
    sequence: u64,
    signature: [u8; 32],
) -> Result<NovNativeTxWireV1> {
    let nonce = sequence.saturating_add(1);
    let account_id = format!("acct-novorudp-network-only-{nonce}");
    Ok(NovNativeTxWireV1 {
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
        signature,
    })
}

fn native_tx_payload_for_sequence_v0(sequence: u64) -> Result<Vec<u8>> {
    let tx = native_tx_for_sequence_v0(sequence)?;
    encode_nov_native_tx_wire_v1(&tx)
        .map_err(|err| anyhow::anyhow!("encode network-only native tx wire failed: {err}"))
}

fn native_tx_payloads_for_payload_v0(sequence: u64, txs_per_payload: u64) -> Result<Vec<u8>> {
    if txs_per_payload <= 1 {
        return native_tx_payload_for_sequence_v0(sequence);
    }
    let mut out = Vec::new();
    out.extend_from_slice(BATCH_NATIVE_TX_PAYLOAD_MAGIC_V0);
    out.extend_from_slice(&txs_per_payload.to_le_bytes());
    for tx_index in 0..txs_per_payload {
        let tx_sequence = sequence
            .saturating_mul(txs_per_payload)
            .saturating_add(tx_index);
        let tx = native_tx_payload_for_sequence_v0(tx_sequence)?;
        let len = u32::try_from(tx.len()).context("batch native tx payload too large")?;
        out.extend_from_slice(&len.to_le_bytes());
        out.extend_from_slice(tx.as_slice());
    }
    Ok(out)
}

fn apfl_native_transfer_batch_payload_v0(sequence: u64, txs_per_payload: u64) -> Result<Vec<u8>> {
    let mut out = Vec::with_capacity(
        APFL_NATIVE_TRANSFER_BATCH_MAGIC_V0.len()
            + 1
            + 2
            + 8
            + 8
            + 8
            + 1
            + (txs_per_payload as usize).saturating_mul(APFL_NATIVE_TRANSFER_SIGNATURE_LEN_V0),
    );
    out.extend_from_slice(APFL_NATIVE_TRANSFER_BATCH_MAGIC_V0);
    out.push(APFL_NATIVE_TRANSFER_BATCH_VERSION_V0);
    out.extend_from_slice(&APFL_NATIVE_TRANSFER_TEMPLATE_DEPOSIT_RESERVE_V0.to_le_bytes());
    out.extend_from_slice(&txs_per_payload.to_le_bytes());
    out.extend_from_slice(&sequence.saturating_mul(txs_per_payload).to_le_bytes());
    out.extend_from_slice(&1u64.to_le_bytes());
    out.push(APFL_NATIVE_TRANSFER_SIGNATURE_LEN_V0 as u8);
    for tx_index in 0..txs_per_payload {
        let tx_sequence = sequence
            .saturating_mul(txs_per_payload)
            .saturating_add(tx_index);
        let tx = native_tx_for_sequence_v0(tx_sequence)?;
        out.extend_from_slice(&tx.signature);
    }
    Ok(out)
}

fn decode_native_tx_payloads_for_payload_v0(
    payload: &[u8],
    tx_count: u64,
) -> Result<DecodedNativePayloadsV0> {
    if payload.starts_with(APFL_NATIVE_TRANSFER_BATCH_MAGIC_V0) {
        return decode_apfl_native_transfer_batch_payload_v0(payload, tx_count);
    }
    if !payload.starts_with(BATCH_NATIVE_TX_PAYLOAD_MAGIC_V0) {
        let tx = decode_nov_native_tx_wire_v1(payload)
            .map_err(|err| anyhow::anyhow!("decode native tx wire failed: {err}"))?;
        return Ok(DecodedNativePayloadsV0 {
            txs: vec![tx],
            legacy_bytes_total: payload.len() as u64,
            ..DecodedNativePayloadsV0::default()
        });
    }
    let mut offset = BATCH_NATIVE_TX_PAYLOAD_MAGIC_V0.len();
    if payload.len() < offset + 8 {
        bail!("batch native tx payload missing count");
    }
    let encoded_count = u64::from_le_bytes(payload[offset..offset + 8].try_into()?);
    offset += 8;
    if encoded_count != tx_count {
        bail!("batch native tx count mismatch: encoded={encoded_count} message={tx_count}");
    }
    let mut out = Vec::with_capacity(encoded_count as usize);
    let mut legacy_bytes_total = 0u64;
    for _ in 0..encoded_count {
        if payload.len() < offset + 4 {
            bail!("batch native tx payload missing item length");
        }
        let len = u32::from_le_bytes(payload[offset..offset + 4].try_into()?) as usize;
        offset += 4;
        let end = offset.saturating_add(len);
        if end > payload.len() {
            bail!("batch native tx payload item length out of bounds");
        }
        let tx = decode_nov_native_tx_wire_v1(&payload[offset..end])
            .map_err(|err| anyhow::anyhow!("decode batch native tx wire failed: {err}"))?;
        out.push(tx);
        legacy_bytes_total = legacy_bytes_total.saturating_add(len as u64);
        offset = end;
    }
    if offset != payload.len() {
        bail!("batch native tx payload has trailing bytes");
    }
    Ok(DecodedNativePayloadsV0 {
        txs: out,
        legacy_bytes_total,
        ..DecodedNativePayloadsV0::default()
    })
}

fn decode_apfl_native_transfer_batch_payload_v0(
    payload: &[u8],
    tx_count: u64,
) -> Result<DecodedNativePayloadsV0> {
    let apfl_decode_start = Instant::now();
    let mut offset = APFL_NATIVE_TRANSFER_BATCH_MAGIC_V0.len();
    if payload.len() < offset + 1 + 2 + 8 + 8 + 8 + 1 {
        bail!("apfl native transfer batch payload too short");
    }
    let version = payload[offset];
    offset += 1;
    if version != APFL_NATIVE_TRANSFER_BATCH_VERSION_V0 {
        bail!(
            "apfl native transfer version mismatch: expected={} got={version}",
            APFL_NATIVE_TRANSFER_BATCH_VERSION_V0
        );
    }
    let template_id = u16::from_le_bytes(payload[offset..offset + 2].try_into()?);
    offset += 2;
    if template_id != APFL_NATIVE_TRANSFER_TEMPLATE_DEPOSIT_RESERVE_V0 {
        bail!("apfl native transfer template mismatch: {template_id}");
    }
    let encoded_count = u64::from_le_bytes(payload[offset..offset + 8].try_into()?);
    offset += 8;
    if encoded_count != tx_count {
        bail!("apfl native transfer count mismatch: encoded={encoded_count} message={tx_count}");
    }
    let base_sequence = u64::from_le_bytes(payload[offset..offset + 8].try_into()?);
    offset += 8;
    let chain_id = u64::from_le_bytes(payload[offset..offset + 8].try_into()?);
    offset += 8;
    if chain_id != 1 {
        bail!("apfl native transfer chain mismatch: {chain_id}");
    }
    let signature_len = payload[offset] as usize;
    offset += 1;
    if signature_len != APFL_NATIVE_TRANSFER_SIGNATURE_LEN_V0 {
        bail!("apfl native transfer signature len mismatch: {signature_len}");
    }
    let expected_len =
        offset.saturating_add((encoded_count as usize).saturating_mul(signature_len));
    if payload.len() != expected_len {
        bail!("apfl native transfer signature block length mismatch");
    }
    let apfl_decode_elapsed_ms = apfl_decode_start.elapsed().as_millis() as u64;

    let canonical_reconstruction_start = Instant::now();
    let mut out = Vec::with_capacity(encoded_count as usize);
    let mut legacy_bytes_total = 0u64;
    let mut canonical_reconstruction_count = 0u64;
    let mut canonical_reconstruction_error_count = 0u64;
    let mut canonical_tx_hash_match_count = 0u64;
    let mut canonical_tx_hash_mismatch_count = 0u64;
    let mut signature_verify_count = 0u64;
    let mut signature_verify_error_count = 0u64;

    for tx_index in 0..encoded_count {
        let sig_start = offset + (tx_index as usize).saturating_mul(signature_len);
        let sig_end = sig_start + signature_len;
        let mut signature = [0u8; 32];
        signature.copy_from_slice(&payload[sig_start..sig_end]);
        let sequence = base_sequence.saturating_add(tx_index);
        let expected_signature = [(sequence.saturating_add(1) & 0xff) as u8; 32];
        if signature != expected_signature {
            signature_verify_error_count = signature_verify_error_count.saturating_add(1);
        }
        match native_tx_for_sequence_with_signature_v0(sequence, signature) {
            Ok(tx) => {
                signature_verify_count = signature_verify_count.saturating_add(1);
                let reconstructed_wire = encode_nov_native_tx_wire_v1(&tx).map_err(|err| {
                    anyhow::anyhow!("encode reconstructed apfl native tx failed: {err}")
                })?;
                let legacy_wire = native_tx_payload_for_sequence_v0(sequence)?;
                legacy_bytes_total = legacy_bytes_total.saturating_add(legacy_wire.len() as u64);
                canonical_reconstruction_count = canonical_reconstruction_count.saturating_add(1);
                let reconstructed_hash = sha2::Sha256::digest(reconstructed_wire.as_slice());
                let legacy_hash = sha2::Sha256::digest(legacy_wire.as_slice());
                if reconstructed_hash == legacy_hash {
                    canonical_tx_hash_match_count = canonical_tx_hash_match_count.saturating_add(1);
                } else {
                    canonical_tx_hash_mismatch_count =
                        canonical_tx_hash_mismatch_count.saturating_add(1);
                }
                out.push(tx);
            }
            Err(_) => {
                canonical_reconstruction_error_count =
                    canonical_reconstruction_error_count.saturating_add(1);
                signature_verify_error_count = signature_verify_error_count.saturating_add(1);
            }
        }
    }
    let canonical_reconstruction_elapsed_ms =
        canonical_reconstruction_start.elapsed().as_millis() as u64;

    Ok(DecodedNativePayloadsV0 {
        txs: out,
        legacy_bytes_total,
        apfl_binary_bytes_total: payload.len() as u64,
        apfl_decode_elapsed_ms,
        canonical_reconstruction_elapsed_ms,
        canonical_reconstruction_count,
        canonical_reconstruction_error_count,
        canonical_tx_hash_match_count,
        canonical_tx_hash_mismatch_count,
        signature_verify_count,
        signature_verify_error_count,
    })
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

fn session_id_hex_v0() -> String {
    session_id_v0()
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect::<String>()
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
                payload_for_sequence_v0(PayloadModeV0::EvmTransactions, sequence, 1)
                    .expect("payload"),
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
                payload_for_sequence_v0(PayloadModeV0::EvmTransactions, sequence, 1)
                    .expect("payload"),
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
    fn evm_transactions_payload_mode_supports_batched_native_txs() {
        let mut delivered = BTreeMap::new();
        for sequence in 0..2 {
            delivered.insert(
                sequence,
                payload_for_sequence_v0(PayloadModeV0::EvmTransactions, sequence, 3)
                    .expect("payload"),
            );
        }

        let summary =
            receiver_execution_summary_v0(PayloadModeV0::EvmTransactions, true, &delivered);
        assert_eq!(summary.business_decode_count, 2);
        assert_eq!(summary.business_decode_error_count, 0);
        assert_eq!(summary.business_transactions_decoded_count, 6);
        assert_eq!(summary.aoem_executed_total, 6);
        assert_eq!(summary.aoem_transactions_executed_total, 6);
        assert_eq!(summary.ledger_completed_count, 6);
        assert_eq!(summary.ledger_transactions_completed_count, 6);
    }

    #[test]
    fn native_transfer_apfl_payload_mode_decode_only_reconstructs_canonical_native_txs() {
        let txs_per_payload = 8;
        let expanded = payload_for_sequence_v0(PayloadModeV0::EvmTransactions, 0, txs_per_payload)
            .expect("expanded payload");
        let apfl = payload_for_sequence_v0(PayloadModeV0::NativeTransferApflV0, 0, txs_per_payload)
            .expect("apfl payload");
        assert!(
            apfl.len() < expanded.len(),
            "apfl payload should be smaller than expanded native tx payload"
        );

        let mut delivered = BTreeMap::new();
        delivered.insert(0, apfl);

        let summary =
            receiver_execution_summary_v0(PayloadModeV0::NativeTransferApflV0, false, &delivered);
        assert_eq!(summary.business_decode_count, 1);
        assert_eq!(summary.business_decode_error_count, 0);
        assert_eq!(summary.business_transactions_decoded_count, txs_per_payload);
        assert_eq!(summary.aoem_transactions_executed_total, 0);
        assert_eq!(summary.ledger_transactions_completed_count, 0);
        assert_eq!(summary.canonical_reconstruction_count, txs_per_payload);
        assert_eq!(summary.canonical_reconstruction_error_count, 0);
        assert_eq!(summary.canonical_tx_hash_match_count, txs_per_payload);
        assert_eq!(summary.canonical_tx_hash_mismatch_count, 0);
        assert_eq!(summary.signature_verify_count, txs_per_payload);
        assert_eq!(summary.signature_verify_error_count, 0);
        assert!(summary.legacy_native_tx_bytes_total > summary.apfl_binary_bytes_total);
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
