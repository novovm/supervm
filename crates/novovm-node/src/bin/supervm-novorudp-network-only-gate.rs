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
use novovm_udp_batch::sendmmsg_batch;
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
const SEND_TO_WOULD_BLOCK_YIELD_RETRIES_V0: u64 = 8;
const SEND_TO_WOULD_BLOCK_SLEEP_US_V0: u64 = 100;
const SEND_TO_WOULD_BLOCK_MAX_RETRIES_V0: u64 = 10_000;
const BACKPRESSURE_HEALTHY_RECOVERY_WINDOWS_V0: u64 = 2;

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
    recv_from_call_count: u64,
    recv_from_elapsed_us: u64,
    frame_decode_elapsed_us: u64,
    packet_dispatch_elapsed_us: u64,
    delivered_insert_elapsed_us: u64,
    ack_build_elapsed_us: u64,
    ack_send_elapsed_us: u64,
    recv_loop_total_elapsed_us: u64,
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
    transport_send_call_count: u64,
    transport_send_max_bytes: u64,
    data_send_would_block_count: u64,
    data_send_retry_count: u64,
    data_send_nonretryable_error_count: u64,
    data_send_max_retry_exceeded_count: u64,
    data_send_backoff_elapsed_us: u64,
    send_batch_call_count: u64,
    send_batch_datagram_count: u64,
    send_batch_max_datagrams: u64,
    send_batch_elapsed_us: u64,
    send_to_fallback_call_count: u64,
    pacing_sleep_elapsed_ms: u64,
    repair_send_elapsed_ms: u64,
    repair_send_call_count: u64,
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
    would_block_count: u64,
    retry_count: u64,
    nonretryable_error_count: u64,
    max_retry_exceeded_count: u64,
    backoff_elapsed_us: u64,
}

#[derive(Debug, Clone, Copy, Default)]
struct SendEncodedBatchTimingV0 {
    kernel_send_elapsed_us: u64,
    total_elapsed_us: u64,
    would_block_count: u64,
    retry_count: u64,
    nonretryable_error_count: u64,
    max_retry_exceeded_count: u64,
    backoff_elapsed_us: u64,
    send_batch_call_count: u64,
    send_batch_datagram_count: u64,
    send_batch_max_datagrams: u64,
    send_batch_elapsed_us: u64,
    send_to_fallback_call_count: u64,
}

struct EncodedTransportFrameV0 {
    sequence: u64,
    payload_len: u64,
    encoded: Vec<u8>,
    frame_encode_elapsed_us: u64,
}

struct SenderLaneV0 {
    socket: UdpSocket,
    bind_addr: SocketAddr,
    buffers: SocketBufferConfigV0,
}

#[derive(Debug, Clone, Default)]
struct SenderLaneStatsV0 {
    send_to_call_count: u64,
    send_to_elapsed_us: u64,
    bytes_total: u64,
    repair_send_to_call_count: u64,
    would_block_count: u64,
    retry_count: u64,
    send_fail_count: u64,
    max_retry_exceeded_count: u64,
    backoff_elapsed_us: u64,
}

#[derive(Debug, Clone)]
struct BackpressurePacingV0 {
    enabled: bool,
    base_chunk_size: u64,
    base_gap_ms: u64,
    current_chunk_size: u64,
    current_gap_ms: u64,
    min_chunk_size: u64,
    max_gap_ms: u64,
    effective_min_chunk_size: u64,
    effective_max_gap_ms: u64,
    window_count: u64,
    trigger_count: u64,
    would_block_trigger_count: u64,
    repair_trigger_count: u64,
    recovery_count: u64,
    adjustment_count: u64,
    total_extra_sleep_ms: u64,
    last_would_block_count: u64,
    last_repair_sent_count: u64,
    healthy_window_count: u64,
}

impl BackpressurePacingV0 {
    fn new(enabled: bool, base_chunk_size: u64, base_gap_ms: u64) -> Self {
        let base_chunk_size = base_chunk_size.max(1);
        let min_chunk_size = (base_chunk_size / 2).max(1);
        let max_gap_ms = base_gap_ms.saturating_mul(2).max(base_gap_ms);
        Self {
            enabled,
            base_chunk_size,
            base_gap_ms,
            current_chunk_size: base_chunk_size,
            current_gap_ms: base_gap_ms,
            min_chunk_size,
            max_gap_ms,
            effective_min_chunk_size: base_chunk_size,
            effective_max_gap_ms: base_gap_ms,
            window_count: 0,
            trigger_count: 0,
            would_block_trigger_count: 0,
            repair_trigger_count: 0,
            recovery_count: 0,
            adjustment_count: 0,
            total_extra_sleep_ms: 0,
            last_would_block_count: 0,
            last_repair_sent_count: 0,
            healthy_window_count: 0,
        }
    }

    const fn active_chunk_size(&self) -> u64 {
        self.current_chunk_size
    }

    const fn active_gap_ms(&self) -> u64 {
        self.current_gap_ms
    }

    fn note_sleep(&mut self) {
        if self.enabled && self.current_gap_ms > self.base_gap_ms {
            self.total_extra_sleep_ms = self
                .total_extra_sleep_ms
                .saturating_add(self.current_gap_ms - self.base_gap_ms);
        }
    }

    fn observe_window(&mut self, stats: &SenderStats) {
        if !self.enabled {
            return;
        }
        self.window_count = self.window_count.saturating_add(1);
        let new_would_block = stats
            .data_send_would_block_count
            .saturating_sub(self.last_would_block_count);
        let new_repair = stats
            .repair_sent
            .saturating_sub(self.last_repair_sent_count);
        self.last_would_block_count = stats.data_send_would_block_count;
        self.last_repair_sent_count = stats.repair_sent;

        if new_would_block > 0 || new_repair > 0 {
            self.trigger_count = self.trigger_count.saturating_add(1);
            if new_would_block > 0 {
                self.would_block_trigger_count = self.would_block_trigger_count.saturating_add(1);
            }
            if new_repair > 0 {
                self.repair_trigger_count = self.repair_trigger_count.saturating_add(1);
            }
            self.healthy_window_count = 0;
            let next_chunk = self.min_chunk_size;
            let next_gap = self.max_gap_ms;
            if self.current_chunk_size != next_chunk || self.current_gap_ms != next_gap {
                self.adjustment_count = self.adjustment_count.saturating_add(1);
                self.current_chunk_size = next_chunk;
                self.current_gap_ms = next_gap;
                self.effective_min_chunk_size =
                    self.effective_min_chunk_size.min(self.current_chunk_size);
                self.effective_max_gap_ms = self.effective_max_gap_ms.max(self.current_gap_ms);
            }
            return;
        }

        self.healthy_window_count = self.healthy_window_count.saturating_add(1);
        if self.healthy_window_count >= BACKPRESSURE_HEALTHY_RECOVERY_WINDOWS_V0
            && (self.current_chunk_size != self.base_chunk_size
                || self.current_gap_ms != self.base_gap_ms)
        {
            self.recovery_count = self.recovery_count.saturating_add(1);
            self.adjustment_count = self.adjustment_count.saturating_add(1);
            self.current_chunk_size = self.base_chunk_size;
            self.current_gap_ms = self.base_gap_ms;
            self.healthy_window_count = 0;
        }
    }
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
    let mut source_addr_counts = BTreeMap::<String, u64>::new();
    let mut source_port_counts = BTreeMap::<u16, u64>::new();
    let mut max_seen_sequence = None::<u64>;
    let mut out_of_order_count = 0u64;

    while start.elapsed() < timeout {
        let loop_start = Instant::now();
        let recv_from_start = Instant::now();
        let recv_result = socket.recv_from(buf.as_mut_slice());
        stats.recv_from_elapsed_us = stats
            .recv_from_elapsed_us
            .saturating_add(recv_from_start.elapsed().as_micros() as u64);
        match recv_result {
            Ok((n, src)) => {
                stats.recv_from_call_count = stats.recv_from_call_count.saturating_add(1);
                let recv_ms = start.elapsed().as_millis() as u64;
                let decode_start = Instant::now();
                let frame = match NovoRudpTransportFrameV0::decode(&buf[..n]) {
                    Ok(frame) => frame,
                    Err(_) => {
                        stats.frame_decode_elapsed_us = stats
                            .frame_decode_elapsed_us
                            .saturating_add(decode_start.elapsed().as_micros() as u64);
                        stats.decode_error_count = stats.decode_error_count.saturating_add(1);
                        continue;
                    }
                };
                stats.frame_decode_elapsed_us = stats
                    .frame_decode_elapsed_us
                    .saturating_add(decode_start.elapsed().as_micros() as u64);
                if frame.session_id != session_id {
                    continue;
                }
                *source_addr_counts.entry(src.to_string()).or_default() += 1;
                *source_port_counts.entry(src.port()).or_default() += 1;
                if let Some(max_seen) = max_seen_sequence {
                    if frame.sequence < max_seen {
                        out_of_order_count = out_of_order_count.saturating_add(1);
                    }
                }
                max_seen_sequence = Some(max_seen_sequence.unwrap_or(0).max(frame.sequence));
                first_packet_ms.get_or_insert(recv_ms);
                last_packet_ms = Some(recv_ms);
                last_peer = Some(src);
                let dispatch_start = Instant::now();
                match frame.kind {
                    NovoRudpTransportFrameKindV0::Data => {
                        stats.data_received = stats.data_received.saturating_add(1);
                        let insert_start = Instant::now();
                        if delivered.insert(frame.sequence, frame.payload).is_some() {
                            stats.duplicate_received = stats.duplicate_received.saturating_add(1);
                        }
                        stats.delivered_insert_elapsed_us = stats
                            .delivered_insert_elapsed_us
                            .saturating_add(insert_start.elapsed().as_micros() as u64);
                    }
                    NovoRudpTransportFrameKindV0::Repair => {
                        stats.repair_received = stats.repair_received.saturating_add(1);
                        let insert_start = Instant::now();
                        if delivered.insert(frame.sequence, frame.payload).is_some() {
                            stats.duplicate_received = stats.duplicate_received.saturating_add(1);
                        }
                        stats.delivered_insert_elapsed_us = stats
                            .delivered_insert_elapsed_us
                            .saturating_add(insert_start.elapsed().as_micros() as u64);
                    }
                    _ => {}
                }
                stats.packet_dispatch_elapsed_us = stats
                    .packet_dispatch_elapsed_us
                    .saturating_add(dispatch_start.elapsed().as_micros() as u64);
                packet_since_ack = packet_since_ack.saturating_add(1);
                let done = delivered.len() as u64 >= tx_count;
                if done || packet_since_ack >= ack_every {
                    if let Some(peer) = last_peer {
                        ack_epoch = ack_epoch.saturating_add(1);
                        let ack_timing = send_ack(
                            &socket, peer, session_id, tx_count, &delivered, ack_epoch, done,
                        )?;
                        stats.ack_build_elapsed_us = stats
                            .ack_build_elapsed_us
                            .saturating_add(ack_timing.frame_encode_elapsed_us);
                        stats.ack_send_elapsed_us = stats
                            .ack_send_elapsed_us
                            .saturating_add(ack_timing.kernel_send_elapsed_us);
                        stats.ack_sent = stats.ack_sent.saturating_add(1);
                        packet_since_ack = 0;
                    }
                }
                if done {
                    transport_done_ms = Some(start.elapsed().as_millis() as u64);
                    stats.recv_loop_total_elapsed_us = stats
                        .recv_loop_total_elapsed_us
                        .saturating_add(loop_start.elapsed().as_micros() as u64);
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
                    let ack_timing = send_ack(
                        &socket, peer, session_id, tx_count, &delivered, ack_epoch, done,
                    )?;
                    stats.ack_build_elapsed_us = stats
                        .ack_build_elapsed_us
                        .saturating_add(ack_timing.frame_encode_elapsed_us);
                    stats.ack_send_elapsed_us = stats
                        .ack_send_elapsed_us
                        .saturating_add(ack_timing.kernel_send_elapsed_us);
                    stats.ack_sent = stats.ack_sent.saturating_add(1);
                    if done {
                        break;
                    }
                }
            }
            Err(e) => return Err(e).context("receiver recv_from failed"),
        }
        stats.recv_loop_total_elapsed_us = stats
            .recv_loop_total_elapsed_us
            .saturating_add(loop_start.elapsed().as_micros() as u64);
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
        "receiver_recv_from_call_count": stats.recv_from_call_count,
        "receiver_recv_from_elapsed_us": stats.recv_from_elapsed_us,
        "receiver_recv_from_elapsed_ms": stats.recv_from_elapsed_us / 1000,
        "receiver_frame_decode_elapsed_us": stats.frame_decode_elapsed_us,
        "receiver_frame_decode_elapsed_ms": stats.frame_decode_elapsed_us / 1000,
        "receiver_packet_dispatch_elapsed_us": stats.packet_dispatch_elapsed_us,
        "receiver_packet_dispatch_elapsed_ms": stats.packet_dispatch_elapsed_us / 1000,
        "receiver_delivered_insert_elapsed_us": stats.delivered_insert_elapsed_us,
        "receiver_delivered_insert_elapsed_ms": stats.delivered_insert_elapsed_us / 1000,
        "receiver_ack_build_elapsed_us": stats.ack_build_elapsed_us,
        "receiver_ack_build_elapsed_ms": stats.ack_build_elapsed_us / 1000,
        "receiver_ack_send_elapsed_us": stats.ack_send_elapsed_us,
        "receiver_ack_send_elapsed_ms": stats.ack_send_elapsed_us / 1000,
        "receiver_recv_loop_total_elapsed_us": stats.recv_loop_total_elapsed_us,
        "receiver_recv_loop_total_elapsed_ms": stats.recv_loop_total_elapsed_us / 1000,
        "receiver_source_addr_count": source_addr_counts.len() as u64,
        "receiver_source_port_count": source_port_counts.len() as u64,
        "receiver_packets_by_source_addr": source_addr_counts,
        "receiver_packets_by_source_port": source_port_counts,
        "receiver_out_of_order_count": out_of_order_count,
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
    let backpressure_pacing_requested =
        env_bool("NOVOVM_NOVORUDP_NETWORK_ONLY_BACKPRESSURE_PACING");
    let send_batching_requested = env_bool("NOVOVM_NOVORUDP_NETWORK_ONLY_SEND_BATCHING");
    let timeout = Duration::from_millis(env_u64(
        "NOVOVM_NOVORUDP_NETWORK_ONLY_TIMEOUT_MS",
        DEFAULT_TIMEOUT_MS,
    ));
    let session_id = session_id_v0();
    let target: SocketAddr = target_addr
        .parse()
        .with_context(|| format!("parse receiver addr failed: {target_addr}"))?;
    let sender_lane_count =
        env_u64("NOVOVM_NOVORUDP_NETWORK_ONLY_SENDER_LANE_COUNT", 1).clamp(1, 16) as usize;
    let sender_lane_base_port = env_u64("NOVOVM_NOVORUDP_NETWORK_ONLY_SENDER_LANE_BASE_PORT", 0);
    let lanes = build_sender_lanes_v0(&bind_addr, sender_lane_count, sender_lane_base_port)?;
    let socket_buffers = lanes.first().map(|lane| lane.buffers).unwrap_or_default();
    let mut lane_stats = vec![SenderLaneStatsV0::default(); lanes.len()];
    let mut backpressure_pacing = BackpressurePacingV0::new(
        backpressure_pacing_requested && lanes.len() == 1 && !send_batching_requested,
        data_pacing_chunk_size,
        data_pacing_chunk_gap_ms,
    );

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
    if lanes.len() == 1 {
        if send_batching_requested {
            let (lane_stat, lane_sender_stats, sent_sequences) = send_primary_batched_v0(
                &lanes[0].socket,
                target,
                session_id,
                tx_count,
                &payloads,
                loss,
                data_pacing_chunk_size,
                data_pacing_chunk_gap_ms,
            )?;
            lane_stats[0] = lane_stat;
            merge_sender_stats_v0(&mut stats, lane_sender_stats);
            for sequence in sent_sequences {
                if !sent_once.insert(sequence) {
                    stats.duplicate_sent = stats.duplicate_sent.saturating_add(1);
                }
            }
        } else {
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
                let lane_index = 0usize;
                let socket_send_start = Instant::now();
                let timing = send_transport_frame(
                    &lanes[lane_index].socket,
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
                stats.transport_send_call_count = stats.transport_send_call_count.saturating_add(1);
                stats.transport_send_max_bytes =
                    stats.transport_send_max_bytes.max(timing.encoded_bytes);
                stats.data_send_would_block_count = stats
                    .data_send_would_block_count
                    .saturating_add(timing.would_block_count);
                stats.data_send_retry_count = stats
                    .data_send_retry_count
                    .saturating_add(timing.retry_count);
                stats.data_send_nonretryable_error_count = stats
                    .data_send_nonretryable_error_count
                    .saturating_add(timing.nonretryable_error_count);
                stats.data_send_max_retry_exceeded_count = stats
                    .data_send_max_retry_exceeded_count
                    .saturating_add(timing.max_retry_exceeded_count);
                stats.data_send_backoff_elapsed_us = stats
                    .data_send_backoff_elapsed_us
                    .saturating_add(timing.backoff_elapsed_us);
                lane_stats[lane_index].send_to_call_count =
                    lane_stats[lane_index].send_to_call_count.saturating_add(1);
                lane_stats[lane_index].send_to_elapsed_us = lane_stats[lane_index]
                    .send_to_elapsed_us
                    .saturating_add(timing.kernel_send_elapsed_us);
                lane_stats[lane_index].bytes_total = lane_stats[lane_index]
                    .bytes_total
                    .saturating_add(timing.encoded_bytes);
                lane_stats[lane_index].would_block_count = lane_stats[lane_index]
                    .would_block_count
                    .saturating_add(timing.would_block_count);
                lane_stats[lane_index].retry_count = lane_stats[lane_index]
                    .retry_count
                    .saturating_add(timing.retry_count);
                lane_stats[lane_index].send_fail_count = lane_stats[lane_index]
                    .send_fail_count
                    .saturating_add(timing.nonretryable_error_count);
                lane_stats[lane_index].max_retry_exceeded_count = lane_stats[lane_index]
                    .max_retry_exceeded_count
                    .saturating_add(timing.max_retry_exceeded_count);
                lane_stats[lane_index].backoff_elapsed_us = lane_stats[lane_index]
                    .backoff_elapsed_us
                    .saturating_add(timing.backoff_elapsed_us);
                stats.data_payload_bytes_sent_total = stats
                    .data_payload_bytes_sent_total
                    .saturating_add(payload_len);
                if !sent_once.insert(sequence) {
                    stats.duplicate_sent = stats.duplicate_sent.saturating_add(1);
                }
                stats.data_sent = stats.data_sent.saturating_add(1);
                let active_pacing_chunk_size = backpressure_pacing.active_chunk_size();
                let active_pacing_gap_ms = backpressure_pacing.active_gap_ms();
                if data_pacing_chunk_size > 0
                    && data_pacing_chunk_gap_ms > 0
                    && stats.data_sent % active_pacing_chunk_size == 0
                    && sequence + 1 < tx_count
                {
                    stats.data_pacing_sleep_count = stats.data_pacing_sleep_count.saturating_add(1);
                    let pacing_sleep_start = Instant::now();
                    thread::sleep(Duration::from_millis(active_pacing_gap_ms));
                    backpressure_pacing.note_sleep();
                    stats.pacing_sleep_elapsed_ms = stats
                        .pacing_sleep_elapsed_ms
                        .saturating_add(pacing_sleep_start.elapsed().as_millis() as u64);
                    backpressure_pacing.observe_window(&stats);
                }
            }
        }
    } else {
        let lane_results = thread::scope(|scope| -> Result<Vec<_>> {
            let mut handles = Vec::with_capacity(lanes.len());
            for lane_index in 0..lanes.len() {
                let lane_socket = &lanes[lane_index].socket;
                let payloads = &payloads;
                let lane_count = lanes.len();
                handles.push(scope.spawn(move || {
                    send_primary_lane_v0(
                        lane_index,
                        lane_count,
                        lane_socket,
                        target,
                        session_id,
                        tx_count,
                        payloads,
                        loss,
                        data_pacing_chunk_size,
                        data_pacing_chunk_gap_ms,
                    )
                }));
            }
            let mut results = Vec::with_capacity(handles.len());
            for handle in handles {
                let result = handle
                    .join()
                    .map_err(|_| anyhow::anyhow!("sender primary lane worker panicked"))??;
                results.push(result);
            }
            Ok(results)
        })?;
        for (lane_index, lane_stat, lane_sender_stats, sent_sequences) in lane_results {
            lane_stats[lane_index] = lane_stat;
            merge_sender_stats_v0(&mut stats, lane_sender_stats);
            for sequence in sent_sequences {
                if !sent_once.insert(sequence) {
                    stats.duplicate_sent = stats.duplicate_sent.saturating_add(1);
                }
            }
        }
    }
    let sender_primary_send_elapsed_ms = primary_send_start.elapsed().as_millis() as u64;

    let mut buf = vec![0u8; 128 * 1024];
    let mut done = false;
    let mut pending_repair = None::<(Vec<NovoRudpRange>, u64)>;
    let ack_poll_start = Instant::now();
    while start.elapsed() < timeout {
        match recv_ack_from_lanes_v0(&lanes, session_id, buf.as_mut_slice())? {
            Some(ack) => {
                stats.ack_received = stats.ack_received.saturating_add(1);
                latest_missing = ack.missing_ranges.clone();
                if ack.receiver_done && latest_missing.is_empty() {
                    done = true;
                    break;
                }
                pending_repair = Some((latest_missing.clone(), ack.ack_epoch));
            }
            None => {
                let Some((repair_ranges, ack_epoch)) = pending_repair.take() else {
                    if lanes.len() > 1 {
                        thread::sleep(Duration::from_millis(1));
                    }
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
                        let lane_index = (sequence as usize) % lanes.len();
                        let socket_send_start = Instant::now();
                        let timing = send_transport_frame(
                            &lanes[lane_index].socket,
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
                        stats.transport_send_call_count =
                            stats.transport_send_call_count.saturating_add(1);
                        stats.transport_send_max_bytes =
                            stats.transport_send_max_bytes.max(timing.encoded_bytes);
                        stats.data_send_would_block_count = stats
                            .data_send_would_block_count
                            .saturating_add(timing.would_block_count);
                        stats.data_send_retry_count = stats
                            .data_send_retry_count
                            .saturating_add(timing.retry_count);
                        stats.data_send_nonretryable_error_count = stats
                            .data_send_nonretryable_error_count
                            .saturating_add(timing.nonretryable_error_count);
                        stats.data_send_max_retry_exceeded_count = stats
                            .data_send_max_retry_exceeded_count
                            .saturating_add(timing.max_retry_exceeded_count);
                        stats.data_send_backoff_elapsed_us = stats
                            .data_send_backoff_elapsed_us
                            .saturating_add(timing.backoff_elapsed_us);
                        stats.repair_send_elapsed_ms = stats
                            .repair_send_elapsed_ms
                            .saturating_add(socket_send_start.elapsed().as_millis() as u64);
                        stats.repair_send_call_count =
                            stats.repair_send_call_count.saturating_add(1);
                        lane_stats[lane_index].send_to_call_count =
                            lane_stats[lane_index].send_to_call_count.saturating_add(1);
                        lane_stats[lane_index].send_to_elapsed_us = lane_stats[lane_index]
                            .send_to_elapsed_us
                            .saturating_add(timing.kernel_send_elapsed_us);
                        lane_stats[lane_index].bytes_total = lane_stats[lane_index]
                            .bytes_total
                            .saturating_add(timing.encoded_bytes);
                        lane_stats[lane_index].repair_send_to_call_count = lane_stats[lane_index]
                            .repair_send_to_call_count
                            .saturating_add(1);
                        lane_stats[lane_index].would_block_count = lane_stats[lane_index]
                            .would_block_count
                            .saturating_add(timing.would_block_count);
                        lane_stats[lane_index].retry_count = lane_stats[lane_index]
                            .retry_count
                            .saturating_add(timing.retry_count);
                        lane_stats[lane_index].send_fail_count = lane_stats[lane_index]
                            .send_fail_count
                            .saturating_add(timing.nonretryable_error_count);
                        lane_stats[lane_index].max_retry_exceeded_count = lane_stats[lane_index]
                            .max_retry_exceeded_count
                            .saturating_add(timing.max_retry_exceeded_count);
                        lane_stats[lane_index].backoff_elapsed_us = lane_stats[lane_index]
                            .backoff_elapsed_us
                            .saturating_add(timing.backoff_elapsed_us);
                        stats.repair_payload_bytes_sent_total = stats
                            .repair_payload_bytes_sent_total
                            .saturating_add(payload_len);
                        if !sent_once.insert(sequence) {
                            stats.duplicate_sent = stats.duplicate_sent.saturating_add(1);
                        }
                        stats.repair_sent = stats.repair_sent.saturating_add(1);
                    }
                }
                backpressure_pacing.observe_window(&stats);
            }
        }
    }
    let sender_ack_poll_elapsed_ms = ack_poll_start.elapsed().as_millis() as u64;

    let final_missing = missing_count(&latest_missing);
    let elapsed_ms = start.elapsed().as_millis() as u64;
    let sender_data_payload_bytes_total = payloads.iter().fold(0u64, |acc, payload| {
        acc.saturating_add(payload.len() as u64)
    });
    let sender_lane_bind_addrs = lanes
        .iter()
        .map(|lane| lane.bind_addr.to_string())
        .collect::<Vec<_>>();
    let sender_lane_send_to_call_counts = lane_stats
        .iter()
        .map(|lane| lane.send_to_call_count)
        .collect::<Vec<_>>();
    let sender_lane_send_to_elapsed_us = lane_stats
        .iter()
        .map(|lane| lane.send_to_elapsed_us)
        .collect::<Vec<_>>();
    let sender_lane_send_to_elapsed_ms = lane_stats
        .iter()
        .map(|lane| lane.send_to_elapsed_us / 1000)
        .collect::<Vec<_>>();
    let sender_lane_bytes_total = lane_stats
        .iter()
        .map(|lane| lane.bytes_total)
        .collect::<Vec<_>>();
    let sender_lane_repair_send_to_call_counts = lane_stats
        .iter()
        .map(|lane| lane.repair_send_to_call_count)
        .collect::<Vec<_>>();
    let sender_lane_would_block_counts = lane_stats
        .iter()
        .map(|lane| lane.would_block_count)
        .collect::<Vec<_>>();
    let sender_lane_retry_counts = lane_stats
        .iter()
        .map(|lane| lane.retry_count)
        .collect::<Vec<_>>();
    let sender_lane_send_fail_counts = lane_stats
        .iter()
        .map(|lane| lane.send_fail_count)
        .collect::<Vec<_>>();
    let sender_lane_max_retry_exceeded_counts = lane_stats
        .iter()
        .map(|lane| lane.max_retry_exceeded_count)
        .collect::<Vec<_>>();
    let sender_lane_backoff_elapsed_ms = lane_stats
        .iter()
        .map(|lane| lane.backoff_elapsed_us / 1000)
        .collect::<Vec<_>>();
    let sender_lane_send_buffer_effective_bytes = lanes
        .iter()
        .map(|lane| lane.buffers.effective_send_buffer_bytes)
        .collect::<Vec<_>>();
    let sender_lane_recv_buffer_effective_bytes = lanes
        .iter()
        .map(|lane| lane.buffers.effective_recv_buffer_bytes)
        .collect::<Vec<_>>();
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
        "sender_backpressure_pacing_requested": backpressure_pacing_requested,
        "sender_backpressure_pacing_enabled": backpressure_pacing.enabled,
        "sender_pacing_base_chunk_size": backpressure_pacing.base_chunk_size,
        "sender_pacing_base_gap_ms": backpressure_pacing.base_gap_ms,
        "sender_pacing_effective_min_chunk_size": backpressure_pacing.effective_min_chunk_size,
        "sender_pacing_effective_max_gap_ms": backpressure_pacing.effective_max_gap_ms,
        "sender_backpressure_last_effective_chunk_size": backpressure_pacing.current_chunk_size,
        "sender_backpressure_last_effective_gap_ms": backpressure_pacing.current_gap_ms,
        "sender_backpressure_window_count": backpressure_pacing.window_count,
        "sender_backpressure_trigger_count": backpressure_pacing.trigger_count,
        "sender_backpressure_would_block_trigger_count": backpressure_pacing.would_block_trigger_count,
        "sender_backpressure_repair_trigger_count": backpressure_pacing.repair_trigger_count,
        "sender_backpressure_recovery_count": backpressure_pacing.recovery_count,
        "sender_pacing_adjustment_count": backpressure_pacing.adjustment_count,
        "sender_backpressure_total_extra_sleep_ms": backpressure_pacing.total_extra_sleep_ms,
        "sender_send_batching_requested": send_batching_requested,
        "sender_send_batching_enabled": send_batching_requested && lanes.len() == 1,
        "sender_send_batch_call_count": stats.send_batch_call_count,
        "sender_send_batch_datagram_count": stats.send_batch_datagram_count,
        "sender_send_batch_avg_datagrams": stats.send_batch_datagram_count / stats.send_batch_call_count.max(1),
        "sender_send_batch_max_datagrams": stats.send_batch_max_datagrams,
        "sender_send_batch_elapsed_ms": stats.send_batch_elapsed_us / 1000,
        "sender_send_to_fallback_call_count": stats.send_to_fallback_call_count,
        "sender_socket_send_buffer_requested_bytes": socket_buffers.requested_send_buffer_bytes,
        "sender_socket_recv_buffer_requested_bytes": socket_buffers.requested_recv_buffer_bytes,
        "sender_socket_send_buffer_effective_bytes": socket_buffers.effective_send_buffer_bytes,
        "sender_socket_recv_buffer_effective_bytes": socket_buffers.effective_recv_buffer_bytes,
        "sender_lane_count": lanes.len() as u64,
        "sender_lane_primary_send_enabled": lanes.len() > 1,
        "sender_lane_base_port": sender_lane_base_port,
        "sender_lane_bind_addrs": sender_lane_bind_addrs,
        "sender_lane_send_to_call_counts": sender_lane_send_to_call_counts,
        "sender_lane_send_to_elapsed_us": sender_lane_send_to_elapsed_us,
        "sender_lane_send_to_elapsed_ms": sender_lane_send_to_elapsed_ms,
        "sender_lane_bytes_total": sender_lane_bytes_total,
        "sender_lane_repair_send_to_call_counts": sender_lane_repair_send_to_call_counts,
        "sender_lane_would_block_counts": sender_lane_would_block_counts,
        "sender_lane_retry_counts": sender_lane_retry_counts,
        "sender_lane_send_fail_counts": sender_lane_send_fail_counts,
        "sender_lane_max_retry_exceeded_counts": sender_lane_max_retry_exceeded_counts,
        "sender_lane_backoff_elapsed_ms": sender_lane_backoff_elapsed_ms,
        "sender_lane_send_buffer_effective_bytes": sender_lane_send_buffer_effective_bytes,
        "sender_lane_recv_buffer_effective_bytes": sender_lane_recv_buffer_effective_bytes,
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
        "sender_data_send_would_block_count": stats.data_send_would_block_count,
        "sender_data_send_retry_count": stats.data_send_retry_count,
        "sender_data_send_nonretryable_error_count": stats.data_send_nonretryable_error_count,
        "sender_data_send_max_retry_exceeded_count": stats.data_send_max_retry_exceeded_count,
        "sender_data_send_backoff_elapsed_ms": stats.data_send_backoff_elapsed_us / 1000,
        "sender_send_to_call_count": stats.transport_send_call_count,
        "sender_send_to_avg_bytes": stats.transport_encoded_bytes_total / stats.transport_send_call_count.max(1),
        "sender_send_to_max_bytes": stats.transport_send_max_bytes,
        "sender_send_to_elapsed_us": stats.transport_kernel_send_elapsed_us,
        "sender_pacing_sleep_elapsed_ms": stats.pacing_sleep_elapsed_ms,
        "sender_ack_poll_elapsed_ms": sender_ack_poll_elapsed_ms,
        "sender_repair_send_elapsed_ms": stats.repair_send_elapsed_ms,
        "sender_repair_send_call_count": stats.repair_send_call_count,
        "sender_send_loop_non_send_elapsed_ms": sender_primary_send_elapsed_ms
            .saturating_sub(stats.transport_send_total_elapsed_us / 1000)
            .saturating_sub(stats.pacing_sleep_elapsed_ms),
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

fn build_sender_lanes_v0(
    bind_addr: &str,
    lane_count: usize,
    base_port_override: u64,
) -> Result<Vec<SenderLaneV0>> {
    let base_addr = bind_addr
        .parse::<SocketAddr>()
        .with_context(|| format!("parse sender bind addr failed: {bind_addr}"))?;
    let base_port = if base_port_override > 0 {
        u16::try_from(base_port_override).context("sender lane base port exceeds u16")?
    } else {
        base_addr.port()
    };
    let mut lanes = Vec::with_capacity(lane_count);
    for lane_index in 0..lane_count {
        let lane_offset = u16::try_from(lane_index).context("sender lane index exceeds u16")?;
        let lane_port = base_port
            .checked_add(lane_offset)
            .context("sender lane port overflow")?;
        let lane_addr = if lane_count == 1 && base_port_override == 0 {
            base_addr
        } else {
            SocketAddr::new(base_addr.ip(), lane_port)
        };
        let socket = UdpSocket::bind(lane_addr)
            .with_context(|| format!("bind sender lane {lane_index} socket failed: {lane_addr}"))?;
        let buffers =
            configure_udp_socket_buffers_v0(&socket, &format!("sender lane {lane_index}"))?;
        if lane_count == 1 {
            socket
                .set_read_timeout(Some(Duration::from_millis(100)))
                .context("set sender read timeout failed")?;
        } else {
            socket
                .set_nonblocking(true)
                .with_context(|| format!("set sender lane {lane_index} nonblocking failed"))?;
        }
        lanes.push(SenderLaneV0 {
            socket,
            bind_addr: lane_addr,
            buffers,
        });
    }
    Ok(lanes)
}

#[allow(clippy::too_many_arguments)]
fn send_primary_batched_v0(
    socket: &UdpSocket,
    target: SocketAddr,
    session_id: [u8; 16],
    tx_count: u64,
    payloads: &[Vec<u8>],
    loss: LossInjectionConfigV0,
    data_pacing_chunk_size: u64,
    data_pacing_chunk_gap_ms: u64,
) -> Result<(SenderLaneStatsV0, SenderStats, Vec<u64>)> {
    let mut lane_stats = SenderLaneStatsV0::default();
    let mut stats = SenderStats::default();
    let mut sent_sequences = Vec::new();
    let batch_size = data_pacing_chunk_size.max(1) as usize;
    let mut sequence = 0u64;
    while sequence < tx_count {
        let mut batch = Vec::with_capacity(batch_size);
        while sequence < tx_count && batch.len() < batch_size {
            stats.data_send_attempt = stats.data_send_attempt.saturating_add(1);
            if loss.drops_data_sequence(sequence) {
                stats.data_loss_injected = stats.data_loss_injected.saturating_add(1);
                sequence = sequence.saturating_add(1);
                continue;
            }
            let copy_start = Instant::now();
            let payload = payloads[sequence as usize].clone();
            stats.payload_copy_elapsed_ms = stats
                .payload_copy_elapsed_ms
                .saturating_add(copy_start.elapsed().as_millis() as u64);
            batch.push(encode_transport_frame_v0(
                NovoRudpTransportFrameKindV0::Data,
                session_id,
                sequence,
                payload,
                0,
            ));
            sequence = sequence.saturating_add(1);
        }
        if batch.is_empty() {
            continue;
        }
        let timing = send_encoded_transport_batch_v0(socket, target, &batch)?;
        let batch_frame_encode_elapsed_us = batch
            .iter()
            .map(|frame| frame.frame_encode_elapsed_us)
            .sum::<u64>();
        stats.socket_send_elapsed_ms = stats
            .socket_send_elapsed_ms
            .saturating_add(timing.total_elapsed_us / 1000);
        stats.transport_frame_encode_elapsed_us = stats
            .transport_frame_encode_elapsed_us
            .saturating_add(batch_frame_encode_elapsed_us);
        stats.transport_kernel_send_elapsed_us = stats
            .transport_kernel_send_elapsed_us
            .saturating_add(timing.kernel_send_elapsed_us);
        stats.transport_send_total_elapsed_us =
            stats.transport_send_total_elapsed_us.saturating_add(
                timing
                    .total_elapsed_us
                    .saturating_add(batch_frame_encode_elapsed_us),
            );
        stats.transport_encoded_bytes_total = stats.transport_encoded_bytes_total.saturating_add(
            batch
                .iter()
                .map(|frame| frame.encoded.len() as u64)
                .sum::<u64>(),
        );
        stats.data_payload_bytes_sent_total = stats
            .data_payload_bytes_sent_total
            .saturating_add(batch.iter().map(|frame| frame.payload_len).sum::<u64>());
        stats.data_sent = stats.data_sent.saturating_add(batch.len() as u64);
        stats.transport_send_call_count = stats
            .transport_send_call_count
            .saturating_add(timing.send_to_fallback_call_count);
        stats.transport_send_max_bytes = stats.transport_send_max_bytes.max(
            batch
                .iter()
                .map(|frame| frame.encoded.len() as u64)
                .max()
                .unwrap_or_default(),
        );
        stats.data_send_would_block_count = stats
            .data_send_would_block_count
            .saturating_add(timing.would_block_count);
        stats.data_send_retry_count = stats
            .data_send_retry_count
            .saturating_add(timing.retry_count);
        stats.data_send_nonretryable_error_count = stats
            .data_send_nonretryable_error_count
            .saturating_add(timing.nonretryable_error_count);
        stats.data_send_max_retry_exceeded_count = stats
            .data_send_max_retry_exceeded_count
            .saturating_add(timing.max_retry_exceeded_count);
        stats.data_send_backoff_elapsed_us = stats
            .data_send_backoff_elapsed_us
            .saturating_add(timing.backoff_elapsed_us);
        stats.send_batch_call_count = stats
            .send_batch_call_count
            .saturating_add(timing.send_batch_call_count);
        stats.send_batch_datagram_count = stats
            .send_batch_datagram_count
            .saturating_add(timing.send_batch_datagram_count);
        stats.send_batch_max_datagrams = stats
            .send_batch_max_datagrams
            .max(timing.send_batch_max_datagrams);
        stats.send_batch_elapsed_us = stats
            .send_batch_elapsed_us
            .saturating_add(timing.send_batch_elapsed_us);
        stats.send_to_fallback_call_count = stats
            .send_to_fallback_call_count
            .saturating_add(timing.send_to_fallback_call_count);
        lane_stats.send_to_call_count = lane_stats
            .send_to_call_count
            .saturating_add(timing.send_to_fallback_call_count);
        lane_stats.send_to_elapsed_us = lane_stats
            .send_to_elapsed_us
            .saturating_add(timing.kernel_send_elapsed_us);
        lane_stats.bytes_total = lane_stats.bytes_total.saturating_add(
            batch
                .iter()
                .map(|frame| frame.encoded.len() as u64)
                .sum::<u64>(),
        );
        lane_stats.would_block_count = lane_stats
            .would_block_count
            .saturating_add(timing.would_block_count);
        lane_stats.retry_count = lane_stats.retry_count.saturating_add(timing.retry_count);
        lane_stats.send_fail_count = lane_stats
            .send_fail_count
            .saturating_add(timing.nonretryable_error_count);
        lane_stats.max_retry_exceeded_count = lane_stats
            .max_retry_exceeded_count
            .saturating_add(timing.max_retry_exceeded_count);
        lane_stats.backoff_elapsed_us = lane_stats
            .backoff_elapsed_us
            .saturating_add(timing.backoff_elapsed_us);
        sent_sequences.extend(batch.iter().map(|frame| frame.sequence));
        if data_pacing_chunk_gap_ms > 0 && sequence < tx_count {
            stats.data_pacing_sleep_count = stats.data_pacing_sleep_count.saturating_add(1);
            let pacing_sleep_start = Instant::now();
            thread::sleep(Duration::from_millis(data_pacing_chunk_gap_ms));
            stats.pacing_sleep_elapsed_ms = stats
                .pacing_sleep_elapsed_ms
                .saturating_add(pacing_sleep_start.elapsed().as_millis() as u64);
        }
    }
    Ok((lane_stats, stats, sent_sequences))
}

#[allow(clippy::too_many_arguments)]
fn send_primary_lane_v0(
    lane_index: usize,
    lane_count: usize,
    socket: &UdpSocket,
    target: SocketAddr,
    session_id: [u8; 16],
    tx_count: u64,
    payloads: &[Vec<u8>],
    loss: LossInjectionConfigV0,
    data_pacing_chunk_size: u64,
    data_pacing_chunk_gap_ms: u64,
) -> Result<(usize, SenderLaneStatsV0, SenderStats, Vec<u64>)> {
    let mut lane_stats = SenderLaneStatsV0::default();
    let mut stats = SenderStats::default();
    let mut sent_sequences = Vec::new();
    let mut lane_sent_count = 0u64;
    let mut sequence = lane_index as u64;
    while sequence < tx_count {
        stats.data_send_attempt = stats.data_send_attempt.saturating_add(1);
        if loss.drops_data_sequence(sequence) {
            stats.data_loss_injected = stats.data_loss_injected.saturating_add(1);
            sequence = sequence.saturating_add(lane_count as u64);
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
            socket,
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
        stats.transport_send_call_count = stats.transport_send_call_count.saturating_add(1);
        stats.transport_send_max_bytes = stats.transport_send_max_bytes.max(timing.encoded_bytes);
        stats.data_send_would_block_count = stats
            .data_send_would_block_count
            .saturating_add(timing.would_block_count);
        stats.data_send_retry_count = stats
            .data_send_retry_count
            .saturating_add(timing.retry_count);
        stats.data_send_nonretryable_error_count = stats
            .data_send_nonretryable_error_count
            .saturating_add(timing.nonretryable_error_count);
        stats.data_send_max_retry_exceeded_count = stats
            .data_send_max_retry_exceeded_count
            .saturating_add(timing.max_retry_exceeded_count);
        stats.data_send_backoff_elapsed_us = stats
            .data_send_backoff_elapsed_us
            .saturating_add(timing.backoff_elapsed_us);
        stats.data_payload_bytes_sent_total = stats
            .data_payload_bytes_sent_total
            .saturating_add(payload_len);
        stats.data_sent = stats.data_sent.saturating_add(1);
        lane_stats.send_to_call_count = lane_stats.send_to_call_count.saturating_add(1);
        lane_stats.send_to_elapsed_us = lane_stats
            .send_to_elapsed_us
            .saturating_add(timing.kernel_send_elapsed_us);
        lane_stats.bytes_total = lane_stats.bytes_total.saturating_add(timing.encoded_bytes);
        lane_stats.would_block_count = lane_stats
            .would_block_count
            .saturating_add(timing.would_block_count);
        lane_stats.retry_count = lane_stats.retry_count.saturating_add(timing.retry_count);
        lane_stats.send_fail_count = lane_stats
            .send_fail_count
            .saturating_add(timing.nonretryable_error_count);
        lane_stats.max_retry_exceeded_count = lane_stats
            .max_retry_exceeded_count
            .saturating_add(timing.max_retry_exceeded_count);
        lane_stats.backoff_elapsed_us = lane_stats
            .backoff_elapsed_us
            .saturating_add(timing.backoff_elapsed_us);
        sent_sequences.push(sequence);
        lane_sent_count = lane_sent_count.saturating_add(1);
        if data_pacing_chunk_size > 0
            && data_pacing_chunk_gap_ms > 0
            && lane_sent_count % data_pacing_chunk_size == 0
            && sequence.saturating_add(lane_count as u64) < tx_count
        {
            stats.data_pacing_sleep_count = stats.data_pacing_sleep_count.saturating_add(1);
            let pacing_sleep_start = Instant::now();
            thread::sleep(Duration::from_millis(data_pacing_chunk_gap_ms));
            stats.pacing_sleep_elapsed_ms = stats
                .pacing_sleep_elapsed_ms
                .saturating_add(pacing_sleep_start.elapsed().as_millis() as u64);
        }
        sequence = sequence.saturating_add(lane_count as u64);
    }
    Ok((lane_index, lane_stats, stats, sent_sequences))
}

fn merge_sender_stats_v0(dst: &mut SenderStats, src: SenderStats) {
    dst.data_send_attempt = dst.data_send_attempt.saturating_add(src.data_send_attempt);
    dst.data_sent = dst.data_sent.saturating_add(src.data_sent);
    dst.data_pacing_sleep_count = dst
        .data_pacing_sleep_count
        .saturating_add(src.data_pacing_sleep_count);
    dst.repair_sent = dst.repair_sent.saturating_add(src.repair_sent);
    dst.duplicate_sent = dst.duplicate_sent.saturating_add(src.duplicate_sent);
    dst.ack_received = dst.ack_received.saturating_add(src.ack_received);
    dst.decode_error_count = dst
        .decode_error_count
        .saturating_add(src.decode_error_count);
    dst.data_loss_injected = dst
        .data_loss_injected
        .saturating_add(src.data_loss_injected);
    dst.data_payload_bytes_sent_total = dst
        .data_payload_bytes_sent_total
        .saturating_add(src.data_payload_bytes_sent_total);
    dst.repair_payload_bytes_sent_total = dst
        .repair_payload_bytes_sent_total
        .saturating_add(src.repair_payload_bytes_sent_total);
    dst.payload_copy_elapsed_ms = dst
        .payload_copy_elapsed_ms
        .saturating_add(src.payload_copy_elapsed_ms);
    dst.socket_send_elapsed_ms = dst
        .socket_send_elapsed_ms
        .saturating_add(src.socket_send_elapsed_ms);
    dst.transport_frame_encode_elapsed_us = dst
        .transport_frame_encode_elapsed_us
        .saturating_add(src.transport_frame_encode_elapsed_us);
    dst.transport_kernel_send_elapsed_us = dst
        .transport_kernel_send_elapsed_us
        .saturating_add(src.transport_kernel_send_elapsed_us);
    dst.transport_send_total_elapsed_us = dst
        .transport_send_total_elapsed_us
        .saturating_add(src.transport_send_total_elapsed_us);
    dst.transport_encoded_bytes_total = dst
        .transport_encoded_bytes_total
        .saturating_add(src.transport_encoded_bytes_total);
    dst.transport_send_call_count = dst
        .transport_send_call_count
        .saturating_add(src.transport_send_call_count);
    dst.transport_send_max_bytes = dst
        .transport_send_max_bytes
        .max(src.transport_send_max_bytes);
    dst.data_send_would_block_count = dst
        .data_send_would_block_count
        .saturating_add(src.data_send_would_block_count);
    dst.data_send_retry_count = dst
        .data_send_retry_count
        .saturating_add(src.data_send_retry_count);
    dst.data_send_nonretryable_error_count = dst
        .data_send_nonretryable_error_count
        .saturating_add(src.data_send_nonretryable_error_count);
    dst.data_send_max_retry_exceeded_count = dst
        .data_send_max_retry_exceeded_count
        .saturating_add(src.data_send_max_retry_exceeded_count);
    dst.data_send_backoff_elapsed_us = dst
        .data_send_backoff_elapsed_us
        .saturating_add(src.data_send_backoff_elapsed_us);
    dst.send_batch_call_count = dst
        .send_batch_call_count
        .saturating_add(src.send_batch_call_count);
    dst.send_batch_datagram_count = dst
        .send_batch_datagram_count
        .saturating_add(src.send_batch_datagram_count);
    dst.send_batch_max_datagrams = dst
        .send_batch_max_datagrams
        .max(src.send_batch_max_datagrams);
    dst.send_batch_elapsed_us = dst
        .send_batch_elapsed_us
        .saturating_add(src.send_batch_elapsed_us);
    dst.send_to_fallback_call_count = dst
        .send_to_fallback_call_count
        .saturating_add(src.send_to_fallback_call_count);
    dst.pacing_sleep_elapsed_ms = dst
        .pacing_sleep_elapsed_ms
        .saturating_add(src.pacing_sleep_elapsed_ms);
    dst.repair_send_elapsed_ms = dst
        .repair_send_elapsed_ms
        .saturating_add(src.repair_send_elapsed_ms);
    dst.repair_send_call_count = dst
        .repair_send_call_count
        .saturating_add(src.repair_send_call_count);
}

fn recv_ack_from_lanes_v0(
    lanes: &[SenderLaneV0],
    session_id: [u8; 16],
    buf: &mut [u8],
) -> Result<Option<NetworkOnlyAckV0>> {
    for lane in lanes {
        match lane.socket.recv_from(buf) {
            Ok((n, _)) => {
                let Ok(frame) = NovoRudpTransportFrameV0::decode(&buf[..n]) else {
                    continue;
                };
                if frame.kind != NovoRudpTransportFrameKindV0::Ack || frame.session_id != session_id
                {
                    continue;
                }
                let ack =
                    serde_json::from_slice(frame.payload.as_slice()).context("decode ack json")?;
                return Ok(Some(ack));
            }
            Err(e)
                if e.kind() == std::io::ErrorKind::WouldBlock
                    || e.kind() == std::io::ErrorKind::TimedOut =>
            {
                continue;
            }
            Err(e) => return Err(e).context("sender recv ack failed"),
        }
    }
    Ok(None)
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
    let encoded_frame = encode_transport_frame_v0(kind, session_id, sequence, payload, ack_epoch);
    let mut retry_count = 0u64;
    let mut would_block_count = 0u64;
    let mut backoff_elapsed_us = 0u64;
    let send_start = Instant::now();
    loop {
        match socket.send_to(encoded_frame.encoded.as_slice(), target) {
            Ok(_) => break,
            Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => {
                would_block_count = would_block_count.saturating_add(1);
                if retry_count >= SEND_TO_WOULD_BLOCK_MAX_RETRIES_V0 {
                    bail!(
                        "send {kind:?} frame exceeded WouldBlock retry cap ({SEND_TO_WOULD_BLOCK_MAX_RETRIES_V0})"
                    );
                }
                retry_count = retry_count.saturating_add(1);
                let backoff_start = Instant::now();
                if retry_count <= SEND_TO_WOULD_BLOCK_YIELD_RETRIES_V0 {
                    thread::yield_now();
                } else {
                    thread::sleep(Duration::from_micros(SEND_TO_WOULD_BLOCK_SLEEP_US_V0));
                }
                backoff_elapsed_us =
                    backoff_elapsed_us.saturating_add(backoff_start.elapsed().as_micros() as u64);
            }
            Err(e) => return Err(e).with_context(|| format!("send {kind:?} frame failed")),
        }
    }
    let kernel_send_elapsed_us = send_start.elapsed().as_micros() as u64;
    Ok(SendTransportFrameTimingV0 {
        frame_encode_elapsed_us: encoded_frame.frame_encode_elapsed_us,
        kernel_send_elapsed_us,
        total_elapsed_us: total_start.elapsed().as_micros() as u64,
        encoded_bytes: encoded_frame.encoded.len() as u64,
        would_block_count,
        retry_count,
        nonretryable_error_count: 0,
        max_retry_exceeded_count: 0,
        backoff_elapsed_us,
    })
}

fn encode_transport_frame_v0(
    kind: NovoRudpTransportFrameKindV0,
    session_id: [u8; 16],
    sequence: u64,
    payload: Vec<u8>,
    ack_epoch: u64,
) -> EncodedTransportFrameV0 {
    let payload_len = payload.len() as u64;
    let encode_start = Instant::now();
    let frame =
        NovoRudpTransportFrameV0::new(kind, session_id, 1, sequence, sequence, ack_epoch, payload);
    let encoded = frame.encode();
    EncodedTransportFrameV0 {
        sequence,
        payload_len,
        encoded,
        frame_encode_elapsed_us: encode_start.elapsed().as_micros() as u64,
    }
}

fn send_encoded_transport_batch_v0(
    socket: &UdpSocket,
    target: SocketAddr,
    batch: &[EncodedTransportFrameV0],
) -> Result<SendEncodedBatchTimingV0> {
    let total_start = Instant::now();
    let mut timing = SendEncodedBatchTimingV0::default();
    let datagrams = batch
        .iter()
        .map(|frame| frame.encoded.as_slice())
        .collect::<Vec<_>>();
    let mut next = 0usize;
    while next < datagrams.len() {
        let send_start = Instant::now();
        match sendmmsg_batch(socket, target, &datagrams[next..]) {
            Ok(0) => {
                bail!("sendmmsg returned zero datagrams");
            }
            Ok(sent) => {
                timing.send_batch_call_count = timing.send_batch_call_count.saturating_add(1);
                timing.send_batch_datagram_count =
                    timing.send_batch_datagram_count.saturating_add(sent as u64);
                timing.send_batch_max_datagrams = timing.send_batch_max_datagrams.max(sent as u64);
                timing.send_batch_elapsed_us = timing
                    .send_batch_elapsed_us
                    .saturating_add(send_start.elapsed().as_micros() as u64);
                next = next.saturating_add(sent);
            }
            Err(e) if e.kind() == std::io::ErrorKind::Unsupported => {
                for datagram in &datagrams[next..] {
                    let fallback = send_encoded_datagram_with_retry_v0(socket, target, datagram)?;
                    timing.kernel_send_elapsed_us = timing
                        .kernel_send_elapsed_us
                        .saturating_add(fallback.kernel_send_elapsed_us);
                    timing.would_block_count = timing
                        .would_block_count
                        .saturating_add(fallback.would_block_count);
                    timing.retry_count = timing.retry_count.saturating_add(fallback.retry_count);
                    timing.nonretryable_error_count = timing
                        .nonretryable_error_count
                        .saturating_add(fallback.nonretryable_error_count);
                    timing.max_retry_exceeded_count = timing
                        .max_retry_exceeded_count
                        .saturating_add(fallback.max_retry_exceeded_count);
                    timing.backoff_elapsed_us = timing
                        .backoff_elapsed_us
                        .saturating_add(fallback.backoff_elapsed_us);
                    timing.send_to_fallback_call_count =
                        timing.send_to_fallback_call_count.saturating_add(1);
                }
                timing.total_elapsed_us = total_start.elapsed().as_micros() as u64;
                return Ok(timing);
            }
            Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => {
                timing.would_block_count = timing.would_block_count.saturating_add(1);
                if timing.retry_count >= SEND_TO_WOULD_BLOCK_MAX_RETRIES_V0 {
                    bail!(
                        "sendmmsg exceeded WouldBlock retry cap ({SEND_TO_WOULD_BLOCK_MAX_RETRIES_V0})"
                    );
                }
                timing.retry_count = timing.retry_count.saturating_add(1);
                let backoff_start = Instant::now();
                if timing.retry_count <= SEND_TO_WOULD_BLOCK_YIELD_RETRIES_V0 {
                    thread::yield_now();
                } else {
                    thread::sleep(Duration::from_micros(SEND_TO_WOULD_BLOCK_SLEEP_US_V0));
                }
                timing.backoff_elapsed_us = timing
                    .backoff_elapsed_us
                    .saturating_add(backoff_start.elapsed().as_micros() as u64);
            }
            Err(e) => return Err(e).context("sendmmsg batch failed"),
        }
    }
    timing.kernel_send_elapsed_us = timing.send_batch_elapsed_us;
    timing.total_elapsed_us = total_start.elapsed().as_micros() as u64;
    Ok(timing)
}

fn send_encoded_datagram_with_retry_v0(
    socket: &UdpSocket,
    target: SocketAddr,
    datagram: &[u8],
) -> Result<SendTransportFrameTimingV0> {
    let total_start = Instant::now();
    let mut retry_count = 0u64;
    let mut would_block_count = 0u64;
    let mut backoff_elapsed_us = 0u64;
    let send_start = Instant::now();
    loop {
        match socket.send_to(datagram, target) {
            Ok(_) => break,
            Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => {
                would_block_count = would_block_count.saturating_add(1);
                if retry_count >= SEND_TO_WOULD_BLOCK_MAX_RETRIES_V0 {
                    bail!(
                        "send fallback datagram exceeded WouldBlock retry cap ({SEND_TO_WOULD_BLOCK_MAX_RETRIES_V0})"
                    );
                }
                retry_count = retry_count.saturating_add(1);
                let backoff_start = Instant::now();
                if retry_count <= SEND_TO_WOULD_BLOCK_YIELD_RETRIES_V0 {
                    thread::yield_now();
                } else {
                    thread::sleep(Duration::from_micros(SEND_TO_WOULD_BLOCK_SLEEP_US_V0));
                }
                backoff_elapsed_us =
                    backoff_elapsed_us.saturating_add(backoff_start.elapsed().as_micros() as u64);
            }
            Err(e) => return Err(e).context("send fallback datagram failed"),
        }
    }
    Ok(SendTransportFrameTimingV0 {
        frame_encode_elapsed_us: 0,
        kernel_send_elapsed_us: send_start.elapsed().as_micros() as u64,
        total_elapsed_us: total_start.elapsed().as_micros() as u64,
        encoded_bytes: datagram.len() as u64,
        would_block_count,
        retry_count,
        nonretryable_error_count: 0,
        max_retry_exceeded_count: 0,
        backoff_elapsed_us,
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
) -> Result<SendTransportFrameTimingV0> {
    let total_start = Instant::now();
    let encode_start = Instant::now();
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
    let encoded = frame.encode();
    let frame_encode_elapsed_us = encode_start.elapsed().as_micros() as u64;
    let send_start = Instant::now();
    socket
        .send_to(encoded.as_slice(), target)
        .context("send ack frame failed")?;
    let kernel_send_elapsed_us = send_start.elapsed().as_micros() as u64;
    Ok(SendTransportFrameTimingV0 {
        frame_encode_elapsed_us,
        kernel_send_elapsed_us,
        total_elapsed_us: total_start.elapsed().as_micros() as u64,
        encoded_bytes: encoded.len() as u64,
        would_block_count: 0,
        retry_count: 0,
        nonretryable_error_count: 0,
        max_retry_exceeded_count: 0,
        backoff_elapsed_us: 0,
    })
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
