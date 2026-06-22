#![forbid(unsafe_code)]
#![recursion_limit = "1024"]

use anyhow::{bail, Context, Result};
use novovm_network::{Transport, UdpTransport};
use novovm_node::tx_ingress::{
    get_nov_native_execution_store_recovery_probe_v1,
    get_nov_native_execution_store_rocksdb_memory_probe_v1,
    nov_native_execution_store_rocksdb_path_v1, nov_native_tx_to_adapter_tx_ir_v1,
    NOV_NATIVE_AOEM_NATIVE_TX_BATCH_PRODUCTION_CANDIDATE_ENV,
    NOV_NATIVE_AOEM_NATIVE_TX_BATCH_SHADOW_ENV, NOV_NATIVE_AOEM_RUNTIME_WORKER_PIPELINE_ENV,
};
use novovm_protocol::{
    encode_nov_native_tx_wire_v1, EvmNativeMessage, NodeId, NovExecuteTxV1, NovExecutionModeV1,
    NovExecutionPolicyV1, NovExecutionTargetV1, NovFeePolicyV1, NovNativeTxWireV1,
    NovPrivacyModeV1, NovTxKindV1, NovVerificationModeV1, ProtocolMessage,
};
use serde_json::Value;
use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::io::BufRead;
use std::net::UdpSocket;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Output, Stdio};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

const REPORT_SCHEMA_V1: &str = "novovm-native-pipeline-cross-machine-udp-soak-report/v1";
const MEMORY_BISECT_SCHEMA_V1: &str = "novovm-native-pipeline-memory-bisect-report/v1";
const AOEM_RUNTIME_OWNED_PRODUCTION_TARGET_V1: &str = "aoem_runtime_owned_state_persistence";
const NOV_NATIVE_AOEM_NATIVE_TX_BATCH_COMPARE_ENV: &str = "NOVOVM_AOEM_NATIVE_TX_BATCH_COMPARE";
const NOV_NATIVE_LEGACY_HOST_TRANSITIONAL_FALLBACK_ENV: &str =
    "NOVOVM_LEGACY_HOST_TRANSITIONAL_FALLBACK";
const RECEIVER_CHILD_AOEM_OWNERSHIP_ENVS_V1: &[&str] = &[
    NOV_NATIVE_AOEM_NATIVE_TX_BATCH_PRODUCTION_CANDIDATE_ENV,
    NOV_NATIVE_AOEM_NATIVE_TX_BATCH_SHADOW_ENV,
    NOV_NATIVE_AOEM_NATIVE_TX_BATCH_COMPARE_ENV,
    NOV_NATIVE_AOEM_RUNTIME_WORKER_PIPELINE_ENV,
];
const MEMORY_PROBE_TOGGLES_V1: &[(&str, &str, bool)] = &[
    (
        "disable_proof_projection",
        "NOVOVM_NATIVE_PIPELINE_MEMORY_PROBE_DISABLE_PROOF_PROJECTION",
        true,
    ),
    (
        "disable_receipt_projection",
        "NOVOVM_NATIVE_PIPELINE_MEMORY_PROBE_DISABLE_RECEIPT_PROJECTION",
        true,
    ),
    (
        "disable_canonical_projection",
        "NOVOVM_NATIVE_PIPELINE_MEMORY_PROBE_DISABLE_CANONICAL_PROJECTION",
        true,
    ),
    (
        "disable_broadcast_reporting",
        "NOVOVM_NATIVE_PIPELINE_MEMORY_PROBE_DISABLE_BROADCAST_REPORTING",
        false,
    ),
    (
        "disable_recovery_probe",
        "NOVOVM_NATIVE_PIPELINE_MEMORY_PROBE_DISABLE_RECOVERY_PROBE",
        false,
    ),
    (
        "disable_diagnostics_samples",
        "NOVOVM_NATIVE_PIPELINE_MEMORY_PROBE_DISABLE_DIAGNOSTICS_SAMPLES",
        false,
    ),
    (
        "disable_json_report_serialization",
        "NOVOVM_NATIVE_PIPELINE_MEMORY_PROBE_DISABLE_JSON_REPORT_SERIALIZATION",
        false,
    ),
    (
        "disable_semantic_ledger_mirror",
        "NOVOVM_NATIVE_PIPELINE_MEMORY_PROBE_DISABLE_SEMANTIC_LEDGER_MIRROR",
        true,
    ),
    (
        "minimal_aoem_result",
        "NOVOVM_NATIVE_PIPELINE_MEMORY_PROBE_MINIMAL_AOEM_RESULT",
        true,
    ),
    (
        "no_receipt_body_cache",
        "NOVOVM_NATIVE_PIPELINE_MEMORY_PROBE_NO_RECEIPT_BODY_CACHE",
        true,
    ),
];

#[derive(Debug, Clone)]
struct NativeFixtureTxV1 {
    index: u64,
    copy_index: u64,
    tx_hash: [u8; 32],
    payload: Vec<u8>,
    dropped: bool,
}

#[derive(Debug, Clone, Copy)]
struct FaultConfigV1 {
    enabled: bool,
    loss_bps: u64,
    duplicate_bps: u64,
    delay_ms: u64,
    reorder_bps: u64,
    seed: u64,
}

#[derive(Debug, Clone)]
struct SendScheduleStatsV1 {
    scheduled_packets: u64,
    sent_packets: u64,
    dropped_packets: u64,
    duplicated_packets: u64,
    delayed_packets: u64,
    reordered_packets: u64,
    sent_unique: u64,
    send_retry_count: u64,
    send_would_block_count: u64,
    send_failed_count: u64,
    send_failure_first_index: Option<u64>,
    send_failure_first_copy_index: Option<u64>,
    send_failure_first_error: Option<String>,
    sent_by_hash: BTreeMap<String, u64>,
}

#[derive(Debug, Clone, Copy)]
struct SustainedConfigV1 {
    enabled: bool,
    duration_seconds: u64,
    tx_per_round: u64,
    round_interval_ms: u64,
}

#[derive(Debug, Clone, Copy)]
struct TailRepairConfigV1 {
    enabled: bool,
    rounds: u64,
    interval_ms: u64,
    require_ack: bool,
    missing_sample_limit: u64,
    fallback_tail_window: u64,
    packet_copies: u64,
    tail_packet_copies: u64,
    batch_size: u64,
    batch_pause_ms: u64,
    tail_batch_pause_ms: u64,
    round_pause_ms: u64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum TransportProfileV1 {
    NovoRudp,
}

impl TransportProfileV1 {
    fn from_env() -> Result<Self> {
        let transport = string_env_nonempty("NOVOVM_NATIVE_PIPELINE_TRANSPORT");
        let legacy_profile = string_env_nonempty("NOVOVM_NATIVE_PIPELINE_TRANSPORT_PROFILE");
        if legacy_profile.is_some() {
            bail!(
                "NOVOVM_NATIVE_PIPELINE_TRANSPORT_PROFILE is not supported; set NOVOVM_NATIVE_PIPELINE_TRANSPORT=novorudp"
            );
        }
        Ok(
            match transport
                .unwrap_or_else(|| "novorudp".to_string())
                .to_ascii_lowercase()
                .as_str()
            {
                "novorudp" | "novo-rudp" | "novo_rudp" => Self::NovoRudp,
                other => {
                    bail!("unsupported NOVOVM_NATIVE_PIPELINE_TRANSPORT={other}; expected novorudp")
                }
            },
        )
    }

    fn as_str(self) -> &'static str {
        match self {
            Self::NovoRudp => "novorudp",
        }
    }
}

#[derive(Debug, Clone, Copy)]
struct NovoRudpConfigV1 {
    enabled: bool,
    window_size: u64,
    packet_copies: u64,
    tail_packet_copies: u64,
    batch_size: u64,
    batch_pause_ms: u64,
    window_ack_wait_ms: u64,
    max_window_retries: u64,
    tail_window_max_retries: u64,
    tail_window_packet_copies: u64,
    tail_window_batch_size: u64,
    tail_window_batch_pause_ms: u64,
    tail_window_ack_wait_ms: u64,
    ack_progress_interval_ms: u64,
    no_progress_backoff: bool,
}

impl NovoRudpConfigV1 {
    fn from_env(profile: TransportProfileV1) -> Result<Self> {
        Ok(Self {
            enabled: profile == TransportProfileV1::NovoRudp,
            window_size: u64_env("NOVOVM_NOVORUDP_REPAIR_WINDOW_SIZE", 64)?.max(1),
            packet_copies: u64_env("NOVOVM_NOVORUDP_REPAIR_PACKET_COPIES", 2)?.max(1),
            tail_packet_copies: u64_env("NOVOVM_NOVORUDP_REPAIR_TAIL_PACKET_COPIES", 3)?.max(1),
            batch_size: u64_env("NOVOVM_NOVORUDP_REPAIR_BATCH_SIZE", 16)?.max(1),
            batch_pause_ms: u64_env("NOVOVM_NOVORUDP_REPAIR_BATCH_PAUSE_MS", 10)?,
            window_ack_wait_ms: u64_env("NOVOVM_NOVORUDP_REPAIR_WINDOW_ACK_WAIT_MS", 1000)?,
            max_window_retries: u64_env("NOVOVM_NOVORUDP_REPAIR_MAX_WINDOW_RETRIES", 8)?.max(1),
            tail_window_max_retries: u64_env("NOVOVM_NOVORUDP_TAIL_WINDOW_MAX_RETRIES", 16)?.max(1),
            tail_window_packet_copies: u64_env("NOVOVM_NOVORUDP_TAIL_WINDOW_PACKET_COPIES", 6)?
                .max(1),
            tail_window_batch_size: u64_env("NOVOVM_NOVORUDP_TAIL_WINDOW_BATCH_SIZE", 8)?.max(1),
            tail_window_batch_pause_ms: u64_env("NOVOVM_NOVORUDP_TAIL_WINDOW_BATCH_PAUSE_MS", 20)?,
            tail_window_ack_wait_ms: u64_env("NOVOVM_NOVORUDP_TAIL_WINDOW_ACK_WAIT_MS", 1500)?,
            ack_progress_interval_ms: u64_env("NOVOVM_NOVORUDP_ACK_PROGRESS_INTERVAL_MS", 250)?
                .max(1),
            no_progress_backoff: bool_env("NOVOVM_NOVORUDP_REPAIR_NO_PROGRESS_BACKOFF")
                || string_env_nonempty("NOVOVM_NOVORUDP_REPAIR_NO_PROGRESS_BACKOFF").is_none(),
        })
    }

    fn repair_config(self, base: TailRepairConfigV1) -> TailRepairConfigV1 {
        if !self.enabled {
            return base;
        }
        TailRepairConfigV1 {
            packet_copies: self.packet_copies,
            tail_packet_copies: self.tail_packet_copies,
            batch_size: self.batch_size,
            batch_pause_ms: self.batch_pause_ms,
            tail_batch_pause_ms: self.batch_pause_ms,
            round_pause_ms: base.round_pause_ms,
            ..base
        }
    }
}

#[derive(Debug, Clone)]
struct UdpAckConfigV1 {
    enabled: bool,
    bind_addr: String,
    target_addr: Option<String>,
    recv_timeout_ms: u64,
}

#[derive(Debug, Clone, Default)]
struct UdpAckStateV1 {
    received_count: u64,
    latest_epoch: u64,
    latest_missing_count: u64,
    missing_ranges_full_count: u64,
    highest_sequence_seen: Option<u64>,
    latest_ranges: Vec<MissingRangeV1>,
    novorudp_current_window_id: Option<u64>,
    novorudp_current_window: Option<MissingRangeV1>,
    novorudp_current_window_missing_count: u64,
    novorudp_current_window_missing_ranges: Vec<MissingRangeV1>,
    receiver_done: bool,
}

#[derive(Debug, Clone, Default)]
struct ReceiverAckSendStatusV1 {
    enabled: bool,
    attempted_count: u64,
    send_ok_count: u64,
    send_error_count: u64,
    missing_target_count: u64,
    bind_error_count: u64,
    target_addr: Option<String>,
    bind_addr: Option<String>,
    local_addr: Option<String>,
    last_error: Option<String>,
}

#[derive(Debug, Clone, Copy)]
struct UdpSendRetryConfigV1 {
    max_retries: u64,
    backoff_ms: u64,
    backoff_max_ms: u64,
}

#[derive(Debug, Clone, Copy, Default)]
struct UdpSendRetryStatsV1 {
    retry_count: u64,
    would_block_count: u64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct MissingRangeV1 {
    start: u64,
    end_inclusive: u64,
}

#[derive(Debug, Clone)]
struct ReceiverDiagnosticsConfigV1 {
    enabled: bool,
    sample_interval_ms: u64,
    stall_windows: u64,
    pending_drain_no_progress_timeout_ms: u64,
    memory_sample_enabled: bool,
    max_working_set_bytes: u64,
    min_canonical_delta: u64,
    max_elapsed_ms: u64,
    primary_send_duration_ms: u64,
    repair_drain_timeout_ms: u64,
    final_ack_timeout_ms: u64,
    absolute_max_ms: u64,
    report_path: PathBuf,
}

#[derive(Debug, Clone, Default)]
struct ReceiverDiagnosticsStateV1 {
    samples: Vec<Value>,
    last_canonical: u64,
    stall_windows: u64,
    pending_drain_no_progress_ms: u64,
    fail_reason: Option<String>,
    samples_dropped: u64,
    first_working_set_bytes: Option<u64>,
    last_working_set_bytes: Option<u64>,
}

fn string_env_nonempty(name: &str) -> Option<String> {
    std::env::var(name)
        .ok()
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
}

fn first_string_env_nonempty(names: &[&str]) -> Option<String> {
    names.iter().find_map(|name| string_env_nonempty(name))
}

fn u64_env(name: &str, default: u64) -> Result<u64> {
    let Some(raw) = string_env_nonempty(name) else {
        return Ok(default);
    };
    raw.parse::<u64>()
        .with_context(|| format!("{name} must be u64"))
}

fn u64_env_alias(names: &[&str], default: u64) -> Result<u64> {
    for name in names {
        if let Some(raw) = string_env_nonempty(name) {
            return raw
                .parse::<u64>()
                .with_context(|| format!("{name} must be u64"));
        }
    }
    Ok(default)
}

fn u64_seconds_env_alias_ms(names: &[&str], default_ms: u64) -> Result<u64> {
    for name in names {
        if let Some(raw) = string_env_nonempty(name) {
            return raw
                .parse::<u64>()
                .map(|seconds| seconds.saturating_mul(1_000))
                .with_context(|| format!("{name} must be u64 seconds"));
        }
    }
    Ok(default_ms)
}

fn u64_seconds_or_ms_env(
    seconds_names: &[&str],
    ms_names: &[&str],
    default_ms: u64,
) -> Result<u64> {
    if env_any(seconds_names) {
        return u64_seconds_env_alias_ms(seconds_names, default_ms);
    }
    u64_env_alias(ms_names, default_ms)
}

fn env_any(names: &[&str]) -> bool {
    names.iter().any(|name| string_env_nonempty(name).is_some())
}

fn bool_env(name: &str) -> bool {
    string_env_nonempty(name)
        .map(|value| {
            let lower = value.to_ascii_lowercase();
            lower == "1" || lower == "true" || lower == "yes" || lower == "on"
        })
        .unwrap_or(false)
}

fn now_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_millis() as u64)
        .unwrap_or_default()
}

fn current_bin_name_contains(pattern: &str) -> bool {
    std::env::current_exe()
        .ok()
        .and_then(|path| {
            path.file_name()
                .map(|name| name.to_string_lossy().to_string())
        })
        .map(|name| name.to_ascii_lowercase().contains(pattern))
        .unwrap_or(false)
}

fn unix_ms_now() -> u128 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_millis())
        .unwrap_or_default()
}

fn div_ceil_u64(value: u64, divisor: u64) -> u64 {
    if divisor == 0 {
        return 0;
    }
    value.saturating_add(divisor).saturating_sub(1) / divisor
}

fn reserve_udp_addr() -> Result<String> {
    let socket = UdpSocket::bind("127.0.0.1:0").context("reserve udp addr failed")?;
    Ok(socket
        .local_addr()
        .context("read reserved udp addr failed")?
        .to_string())
}

fn temp_store_path(chain_id: u64, role: &str) -> PathBuf {
    std::env::temp_dir().join(format!(
        "novovm-native-pipeline-cross-machine-{role}-{chain_id}-{}-{}.json",
        std::process::id(),
        unix_ms_now()
    ))
}

fn default_report_path(role: &str) -> PathBuf {
    PathBuf::from(format!(
        "artifacts/native-pipeline/native-pipeline-cross-machine-{role}-report.json"
    ))
}

fn report_path(role: &str) -> PathBuf {
    first_string_env_nonempty(&[
        "NOVOVM_NATIVE_PIPELINE_REPORT_PATH",
        "NOVOVM_NATIVE_PIPELINE_CROSS_MACHINE_REPORT_PATH",
    ])
    .map(PathBuf::from)
    .unwrap_or_else(|| default_report_path(role))
}

fn sender_progress_report_path() -> PathBuf {
    if let Some(path) = first_string_env_nonempty(&[
        "NOVOVM_NATIVE_PIPELINE_SENDER_PROGRESS_REPORT_PATH",
        "NOVOVM_NATIVE_PIPELINE_SENDER_LIVE_REPORT_PATH",
    ]) {
        return PathBuf::from(path);
    }
    let report = report_path("sender");
    let Some(file_name) = report.file_name().and_then(|name| name.to_str()) else {
        return PathBuf::from("artifacts/native-pipeline/sender-progress-report.json");
    };
    let progress_name = if let Some(stem) = file_name.strip_suffix(".json") {
        format!("{stem}.progress.json")
    } else {
        format!("{file_name}.progress.json")
    };
    report.with_file_name(progress_name)
}

fn diagnostics_report_path() -> PathBuf {
    first_string_env_nonempty(&[
        "NOVOVM_NATIVE_PIPELINE_DIAGNOSTICS_REPORT_PATH",
        "NOVOVM_NATIVE_PIPELINE_PROGRESS_REPORT_PATH",
    ])
    .map(PathBuf::from)
    .unwrap_or_else(|| {
        PathBuf::from("artifacts/native-pipeline/receiver-sustained-diagnostics-report.json")
    })
}

fn ack_report_path() -> PathBuf {
    first_string_env_nonempty(&[
        "NOVOVM_NATIVE_PIPELINE_ACK_REPORT_PATH",
        "NOVOVM_NATIVE_PIPELINE_RECEIVER_ACK_REPORT_PATH",
    ])
    .map(PathBuf::from)
    .unwrap_or_else(|| PathBuf::from("artifacts/native-pipeline/receiver-sustained-ack.json"))
}

fn receiver_stdout_log_path() -> PathBuf {
    first_string_env_nonempty(&["NOVOVM_NATIVE_PIPELINE_RECEIVER_STDOUT_LOG_PATH"])
        .map(PathBuf::from)
        .unwrap_or_else(|| {
            PathBuf::from(
                "artifacts/native-pipeline/receiver-cross-machine-sustained-5min-stdout.log",
            )
        })
}

fn receiver_stderr_log_path() -> PathBuf {
    first_string_env_nonempty(&["NOVOVM_NATIVE_PIPELINE_RECEIVER_STDERR_LOG_PATH"])
        .map(PathBuf::from)
        .unwrap_or_else(|| {
            PathBuf::from(
                "artifacts/native-pipeline/receiver-cross-machine-sustained-5min-stderr.log",
            )
        })
}

fn receiver_exit_report_path() -> PathBuf {
    first_string_env_nonempty(&["NOVOVM_NATIVE_PIPELINE_RECEIVER_EXIT_REPORT_PATH"])
        .map(PathBuf::from)
        .unwrap_or_else(|| {
            PathBuf::from(
                "artifacts/native-pipeline/receiver-cross-machine-sustained-5min-exit.json",
            )
        })
}

fn memory_bisect_report_path() -> PathBuf {
    first_string_env_nonempty(&["NOVOVM_NATIVE_PIPELINE_MEMORY_BISECT_REPORT_PATH"])
        .map(PathBuf::from)
        .unwrap_or_else(|| {
            PathBuf::from("artifacts/native-pipeline/native-pipeline-memory-bisect-report.json")
        })
}

fn store_path(chain_id: u64, role: &str) -> PathBuf {
    first_string_env_nonempty(&[
        "NOVOVM_NATIVE_PIPELINE_STORE_PATH",
        "NOVOVM_NATIVE_EXECUTION_TICK_STORE_PATH",
    ])
    .map(PathBuf::from)
    .unwrap_or_else(|| temp_store_path(chain_id, role))
}

fn semantic_ledger_mirror_path(store_path: &Path) -> PathBuf {
    if let Some(path) = string_env_nonempty("NOVOVM_NATIVE_AOEM_SEMANTIC_LEDGER_MIRROR") {
        return PathBuf::from(path);
    }
    let mut raw = store_path.as_os_str().to_os_string();
    raw.push(".aoem-semantic-ledger.jsonl");
    PathBuf::from(raw)
}

fn pipeline_progress_report_path(store_path: &Path) -> PathBuf {
    let mut raw = store_path.as_os_str().to_os_string();
    raw.push(".pipeline-progress.json");
    PathBuf::from(raw)
}

fn receiver_diagnostics_config() -> Result<ReceiverDiagnosticsConfigV1> {
    let enabled = bool_env("NOVOVM_NATIVE_PIPELINE_PROGRESS_WATCHDOG_ENABLED")
        || bool_env("NOVOVM_NATIVE_PIPELINE_DIAGNOSTICS_ENABLED");
    let sample_interval_ms =
        u64_env("NOVOVM_NATIVE_PIPELINE_PROGRESS_SAMPLE_INTERVAL_MS", 5_000)?.max(250);
    let stall_windows = u64_env("NOVOVM_NATIVE_PIPELINE_PROGRESS_STALL_WINDOWS", 3)?.max(1);
    let memory_sample_enabled = bool_env("NOVOVM_NATIVE_PIPELINE_MEMORY_SAMPLE_ENABLED") || enabled;
    let default_max_working_set = if memory_sample_enabled {
        8 * 1024 * 1024 * 1024u64
    } else {
        0
    };
    let sustained_duration_ms =
        u64_env("NOVOVM_NATIVE_PIPELINE_SUSTAINED_DURATION_SECONDS", 0)?.saturating_mul(1_000);
    let tail_repair_rounds = u64_env("NOVOVM_NATIVE_PIPELINE_TAIL_REPAIR_ROUNDS", 3)?;
    let tail_repair_interval_ms = u64_env("NOVOVM_NATIVE_PIPELINE_TAIL_REPAIR_INTERVAL_MS", 1_000)?;
    let transport_profile = TransportProfileV1::from_env()?;
    let novorudp_enabled = NovoRudpConfigV1::from_env(transport_profile)
        .map(|config| config.enabled)
        .unwrap_or(false);
    let is_novorudp_two_hour_profile = novorudp_enabled && sustained_duration_ms >= 7_200_000;
    let default_repair_drain_timeout_seconds = if is_novorudp_two_hour_profile {
        5_400
    } else {
        900
    };
    let repair_drain_timeout_ms = u64_env(
        "NOVOVM_NOVORUDP_REPAIR_DRAIN_TIMEOUT_SECONDS",
        default_repair_drain_timeout_seconds,
    )?
    .saturating_mul(1_000);
    let final_ack_timeout_ms =
        u64_env("NOVOVM_NOVORUDP_FINAL_ACK_TIMEOUT_SECONDS", 120)?.saturating_mul(1_000);
    let default_absolute_max_ms = if sustained_duration_ms > 0 {
        if is_novorudp_two_hour_profile {
            12_600_000
        } else {
            sustained_duration_ms.saturating_add(repair_drain_timeout_ms)
        }
    } else {
        0
    };
    let absolute_max_ms = u64_seconds_env_alias_ms(
        &[
            "NOVOVM_NOVORUDP_ABSOLUTE_MAX_TIMEOUT_SECONDS",
            "NOVOVM_NOVORUDP_ABSOLUTE_MAX_SECONDS",
        ],
        default_absolute_max_ms,
    )?;
    let default_max_elapsed_ms = if novorudp_enabled && sustained_duration_ms > 0 {
        absolute_max_ms
    } else if sustained_duration_ms > 0 {
        sustained_duration_ms
            .saturating_add(tail_repair_rounds.saturating_mul(tail_repair_interval_ms))
            .saturating_add(60_000)
    } else {
        0
    };
    Ok(ReceiverDiagnosticsConfigV1 {
        enabled,
        sample_interval_ms,
        stall_windows,
        pending_drain_no_progress_timeout_ms: u64_env(
            "NOVOVM_NATIVE_PIPELINE_PENDING_DRAIN_NO_PROGRESS_TIMEOUT_MS",
            if is_novorudp_two_hour_profile {
                180_000
            } else {
                sample_interval_ms.saturating_mul(stall_windows).max(30_000)
            },
        )?
        .max(sample_interval_ms),
        memory_sample_enabled,
        max_working_set_bytes: u64_env(
            "NOVOVM_NATIVE_PIPELINE_MEMORY_MAX_WORKING_SET_BYTES",
            default_max_working_set,
        )?,
        min_canonical_delta: u64_env("NOVOVM_NATIVE_PIPELINE_PROGRESS_MIN_CANONICAL_DELTA", 0)?,
        max_elapsed_ms: u64_env(
            "NOVOVM_NATIVE_PIPELINE_RECEIVER_MAX_ELAPSED_MS",
            default_max_elapsed_ms,
        )?,
        primary_send_duration_ms: sustained_duration_ms,
        repair_drain_timeout_ms,
        final_ack_timeout_ms,
        absolute_max_ms,
        report_path: diagnostics_report_path(),
    })
}

fn receiver_completion_phase_v1(
    config: &ReceiverDiagnosticsConfigV1,
    elapsed_ms: u64,
    stable_progress: u64,
    expected_tx_count: u64,
    pending_count: u64,
) -> &'static str {
    if stable_progress >= expected_tx_count && pending_count == 0 {
        return "completed";
    }
    if config.primary_send_duration_ms == 0 || elapsed_ms < config.primary_send_duration_ms {
        return "primary_send";
    }
    if stable_progress < expected_tx_count {
        return "repair_convergence";
    }
    if pending_count > 0 {
        return "receiver_drain";
    }
    "final_ack_wait"
}

fn novovm_node_bin() -> PathBuf {
    if let Some(path) = string_env_nonempty("NOVOVM_NATIVE_PIPELINE_NODE_BIN") {
        return PathBuf::from(path);
    }
    let Ok(current) = std::env::current_exe() else {
        return PathBuf::from("novovm-node");
    };
    let Some(dir) = current.parent() else {
        return PathBuf::from("novovm-node");
    };
    let exe = if cfg!(windows) {
        "novovm-node.exe"
    } else {
        "novovm-node"
    };
    dir.join(exe)
}

fn build_native_payloads_from_index(
    chain_id: u64,
    start_index: u64,
    count: u64,
) -> Result<Vec<NativeFixtureTxV1>> {
    let mut out = Vec::with_capacity(count as usize);
    for local_index in 0..count {
        let index = start_index.saturating_add(local_index);
        let nonce = index.saturating_add(1);
        let account_id = format!("acct-native-cross-machine-{nonce}");
        let tx = NovNativeTxWireV1 {
            chain_id,
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
                .context("encode cross-machine fixture args failed")?,
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
        let ir = nov_native_tx_to_adapter_tx_ir_v1(&tx)?;
        let mut tx_hash = [0u8; 32];
        let copy_len = ir.hash.len().min(32);
        tx_hash[..copy_len].copy_from_slice(&ir.hash[..copy_len]);
        let payload = encode_nov_native_tx_wire_v1(&tx)
            .map_err(|err| anyhow::anyhow!("encode cross-machine native tx failed: {err}"))?;
        out.push(NativeFixtureTxV1 {
            index,
            copy_index: 0,
            tx_hash,
            payload,
            dropped: false,
        });
    }
    Ok(out)
}

fn merge_send_stats(target: &mut SendScheduleStatsV1, next: SendScheduleStatsV1) {
    target.scheduled_packets = target
        .scheduled_packets
        .saturating_add(next.scheduled_packets);
    target.sent_packets = target.sent_packets.saturating_add(next.sent_packets);
    target.dropped_packets = target.dropped_packets.saturating_add(next.dropped_packets);
    target.duplicated_packets = target
        .duplicated_packets
        .saturating_add(next.duplicated_packets);
    target.delayed_packets = target.delayed_packets.saturating_add(next.delayed_packets);
    target.reordered_packets = target
        .reordered_packets
        .saturating_add(next.reordered_packets);
    target.send_retry_count = target
        .send_retry_count
        .saturating_add(next.send_retry_count);
    target.send_would_block_count = target
        .send_would_block_count
        .saturating_add(next.send_would_block_count);
    target.send_failed_count = target
        .send_failed_count
        .saturating_add(next.send_failed_count);
    if target.send_failure_first_error.is_none() {
        target.send_failure_first_index = next.send_failure_first_index;
        target.send_failure_first_copy_index = next.send_failure_first_copy_index;
        target.send_failure_first_error = next.send_failure_first_error;
    }
    for (hash, count) in next.sent_by_hash {
        *target.sent_by_hash.entry(hash).or_default() += count;
    }
    target.sent_unique = target
        .sent_by_hash
        .keys()
        .count()
        .try_into()
        .unwrap_or(u64::MAX);
}

fn empty_send_stats() -> SendScheduleStatsV1 {
    SendScheduleStatsV1 {
        scheduled_packets: 0,
        sent_packets: 0,
        dropped_packets: 0,
        duplicated_packets: 0,
        delayed_packets: 0,
        reordered_packets: 0,
        sent_unique: 0,
        send_retry_count: 0,
        send_would_block_count: 0,
        send_failed_count: 0,
        send_failure_first_index: None,
        send_failure_first_copy_index: None,
        send_failure_first_error: None,
        sent_by_hash: BTreeMap::new(),
    }
}

fn default_udp_send_retry_config() -> UdpSendRetryConfigV1 {
    UdpSendRetryConfigV1 {
        max_retries: 10,
        backoff_ms: 5,
        backoff_max_ms: 100,
    }
}

fn default_udp_ack_config() -> UdpAckConfigV1 {
    UdpAckConfigV1 {
        enabled: true,
        bind_addr: "0.0.0.0:0".to_string(),
        target_addr: None,
        recv_timeout_ms: 250,
    }
}

fn is_retryable_udp_send_error(error: &str) -> bool {
    let lower = error.to_ascii_lowercase();
    lower.contains("wouldblock")
        || lower.contains("would block")
        || lower.contains("resource temporarily unavailable")
        || lower.contains("os error 11")
        || lower.contains("os error 10035")
        || lower.contains("temporarily unavailable")
}

fn safe_send_with_retry(
    sender: &UdpTransport,
    receiver_node: NodeId,
    msg: ProtocolMessage,
    retry: UdpSendRetryConfigV1,
) -> std::result::Result<UdpSendRetryStatsV1, String> {
    let mut stats = UdpSendRetryStatsV1::default();
    let mut retry_attempt = 0u64;
    loop {
        match sender.send(receiver_node, msg.clone()) {
            Ok(()) => return Ok(stats),
            Err(err) => {
                let error = err.to_string();
                if !is_retryable_udp_send_error(error.as_str())
                    || retry_attempt >= retry.max_retries
                {
                    return Err(error);
                }
                stats.would_block_count = stats.would_block_count.saturating_add(1);
                stats.retry_count = stats.retry_count.saturating_add(1);
                let backoff_ms = retry
                    .backoff_ms
                    .saturating_mul(retry_attempt.saturating_add(1))
                    .min(retry.backoff_max_ms);
                if backoff_ms > 0 {
                    std::thread::sleep(Duration::from_millis(backoff_ms));
                }
                retry_attempt = retry_attempt.saturating_add(1);
            }
        }
    }
}

fn build_tail_repair_payloads(
    chain_id: u64,
    tx_count: u64,
    repair_round: u64,
) -> Result<Vec<NativeFixtureTxV1>> {
    let mut txs = build_native_payloads_from_index(chain_id, 0, tx_count)?;
    let copy_index = repair_round.saturating_add(1);
    for tx in &mut txs {
        tx.copy_index = copy_index;
        tx.dropped = false;
    }
    Ok(txs)
}

fn missing_ranges_from_progress(progress: u64, expected: u64, limit: u64) -> Vec<MissingRangeV1> {
    if progress >= expected || limit == 0 {
        return Vec::new();
    }
    vec![MissingRangeV1 {
        start: progress,
        end_inclusive: expected.saturating_sub(1),
    }]
}

fn missing_ranges_to_json(ranges: &[MissingRangeV1], limit: u64) -> Value {
    let limited = ranges.iter().take(limit as usize).map(|range| {
        serde_json::json!({
            "start": range.start,
            "end_inclusive": range.end_inclusive,
            "count": range.end_inclusive.saturating_sub(range.start).saturating_add(1),
        })
    });
    serde_json::Value::Array(limited.collect())
}

fn missing_ranges_from_json(value: &Value) -> Vec<MissingRangeV1> {
    value
        .as_array()
        .map(|items| {
            items
                .iter()
                .filter_map(|item| {
                    let start = item.get("start").and_then(Value::as_u64)?;
                    let end_inclusive = item.get("end_inclusive").and_then(Value::as_u64)?;
                    (end_inclusive >= start).then_some(MissingRangeV1 {
                        start,
                        end_inclusive,
                    })
                })
                .collect::<Vec<_>>()
        })
        .unwrap_or_default()
}

fn missing_ranges_overlap_count(a: &[MissingRangeV1], b: &[MissingRangeV1]) -> u64 {
    let mut count = 0u64;
    for left in a {
        for right in b {
            let start = left.start.max(right.start);
            let end = left.end_inclusive.min(right.end_inclusive);
            if end >= start {
                count = count.saturating_add(end.saturating_sub(start).saturating_add(1));
            }
        }
    }
    count
}

fn missing_ranges_count(ranges: &[MissingRangeV1]) -> u64 {
    ranges
        .iter()
        .map(|range| {
            range
                .end_inclusive
                .saturating_sub(range.start)
                .saturating_add(1)
        })
        .sum()
}

fn normalize_missing_ranges(ranges: &[MissingRangeV1], expected: u64) -> Vec<MissingRangeV1> {
    if expected == 0 {
        return Vec::new();
    }
    let max_index = expected.saturating_sub(1);
    let mut normalized = ranges
        .iter()
        .filter_map(|range| {
            let start = range.start.min(max_index);
            let end_inclusive = range.end_inclusive.min(max_index);
            (end_inclusive >= start).then_some(MissingRangeV1 {
                start,
                end_inclusive,
            })
        })
        .collect::<Vec<_>>();
    normalized.sort_by_key(|range| (range.start, range.end_inclusive));
    let mut merged = Vec::<MissingRangeV1>::new();
    for range in normalized {
        if let Some(last) = merged.last_mut() {
            if range.start <= last.end_inclusive.saturating_add(1) {
                last.end_inclusive = last.end_inclusive.max(range.end_inclusive);
                continue;
            }
        }
        merged.push(range);
    }
    merged
}

fn missing_ranges_intersection_with_window(
    ranges: &[MissingRangeV1],
    start: u64,
    end_inclusive: u64,
) -> Vec<MissingRangeV1> {
    if end_inclusive < start {
        return Vec::new();
    }
    let mut out = Vec::<MissingRangeV1>::new();
    for range in ranges {
        let left = range.start.max(start);
        let right = range.end_inclusive.min(end_inclusive);
        if right >= left {
            out.push(MissingRangeV1 {
                start: left,
                end_inclusive: right,
            });
        }
    }
    out
}

fn missing_ranges_intersection_many(
    left_ranges: &[MissingRangeV1],
    right_ranges: &[MissingRangeV1],
) -> Vec<MissingRangeV1> {
    let mut out = Vec::<MissingRangeV1>::new();
    for left in left_ranges {
        for right in right_ranges {
            let start = left.start.max(right.start);
            let end_inclusive = left.end_inclusive.min(right.end_inclusive);
            if end_inclusive >= start {
                out.push(MissingRangeV1 {
                    start,
                    end_inclusive,
                });
            }
        }
    }
    normalize_missing_ranges(out.as_slice(), u64::MAX)
}

fn novorudp_sender_repair_coverage_report_v1(
    latest_ack_missing_ranges: &[MissingRangeV1],
    repair_sent_ranges: &[MissingRangeV1],
    expected: u64,
    repair_sequence_sent_count: u64,
    sample_limit: u64,
) -> Value {
    let ack_missing = normalize_missing_ranges(latest_ack_missing_ranges, expected);
    let repair_sent = normalize_missing_ranges(repair_sent_ranges, expected);
    let repair_sent_unique_count = missing_ranges_count(repair_sent.as_slice());
    let overlap_ack_missing_count =
        missing_ranges_overlap_count(repair_sent.as_slice(), ack_missing.as_slice());
    let duplicate_sequence_count =
        repair_sequence_sent_count.saturating_sub(repair_sent_unique_count);
    let duplicate_waste_ratio_bps = if repair_sequence_sent_count == 0 {
        0
    } else {
        duplicate_sequence_count.saturating_mul(10_000) / repair_sequence_sent_count
    };

    serde_json::json!({
        "sender_latest_ack_missing_ranges_sample": missing_ranges_to_json(
            ack_missing.as_slice(),
            sample_limit,
        ),
        "sender_latest_ack_missing_ranges_full_count": ack_missing.len(),
        "sender_latest_ack_missing_sequence_count": missing_ranges_count(ack_missing.as_slice()),
        "sender_repair_sent_ranges_sample": missing_ranges_to_json(
            repair_sent.as_slice(),
            sample_limit,
        ),
        "sender_repair_sent_ranges_full_count": repair_sent.len(),
        "sender_repair_sent_total_sequence_count": repair_sequence_sent_count,
        "sender_repair_sent_unique_sequence_count": repair_sent_unique_count,
        "sender_repair_sent_overlap_ack_missing_count": overlap_ack_missing_count,
        "sender_repair_sent_new_missing_coverage_count": overlap_ack_missing_count,
        "sender_repair_sent_duplicate_ranges_count": duplicate_sequence_count,
        "sender_repair_sent_duplicate_sequence_count": duplicate_sequence_count,
        "repair_duplicate_waste_ratio_bps": duplicate_waste_ratio_bps,
    })
}

fn first_missing_window_ranges(
    ranges: &[MissingRangeV1],
    expected: u64,
    window_size: u64,
) -> Option<(u64, MissingRangeV1, Vec<MissingRangeV1>)> {
    if expected == 0 || window_size == 0 {
        return None;
    }
    let normalized = normalize_missing_ranges(ranges, expected);
    let first = normalized.first()?;
    let window_start = first.start;
    let window_end = window_start
        .saturating_add(window_size.saturating_sub(1))
        .min(expected.saturating_sub(1));
    let window = MissingRangeV1 {
        start: window_start,
        end_inclusive: window_end,
    };
    let window_ranges =
        missing_ranges_intersection_with_window(normalized.as_slice(), window_start, window_end);
    if window_ranges.is_empty() {
        return None;
    }
    let window_id = window_start / window_size;
    Some((window_id, window, window_ranges))
}

#[derive(Debug, Clone)]
struct NovoRudpRepairSelectionV1 {
    window_id: u64,
    window: MissingRangeV1,
    ranges: Vec<MissingRangeV1>,
    used_full_missing_bitmap: bool,
}

fn select_novorudp_repair_ranges_from_ack(
    ranges: &[MissingRangeV1],
    expected: u64,
    window_size: u64,
    _latest_missing_count: u64,
    _missing_ranges_full_count: u64,
    _max_window_retries: u64,
) -> Option<NovoRudpRepairSelectionV1> {
    let normalized = normalize_missing_ranges(ranges, expected);
    let (window_id, window, window_ranges) =
        first_missing_window_ranges(normalized.as_slice(), expected, window_size)?;
    Some(NovoRudpRepairSelectionV1 {
        window_id,
        window,
        ranges: window_ranges,
        used_full_missing_bitmap: false,
    })
}

fn select_novorudp_repair_ranges_from_receiver_ack(
    ack: &UdpAckStateV1,
    expected: u64,
    window_size: u64,
    max_window_retries: u64,
) -> Option<NovoRudpRepairSelectionV1> {
    if ack.receiver_done || ack.latest_missing_count == 0 {
        return None;
    }
    if let Some(window) = ack.novorudp_current_window {
        if ack.novorudp_current_window_missing_count == 0 {
            return None;
        }
        let window_ranges = if ack.novorudp_current_window_missing_ranges.is_empty() {
            missing_ranges_intersection_with_window(
                ack.latest_ranges.as_slice(),
                window.start,
                window.end_inclusive,
            )
        } else {
            missing_ranges_intersection_with_window(
                ack.novorudp_current_window_missing_ranges.as_slice(),
                window.start,
                window.end_inclusive,
            )
        };
        if !window_ranges.is_empty() {
            return Some(NovoRudpRepairSelectionV1 {
                window_id: ack
                    .novorudp_current_window_id
                    .unwrap_or_else(|| window.start / window_size.max(1)),
                window,
                ranges: window_ranges,
                used_full_missing_bitmap: true,
            });
        }
    }
    select_novorudp_repair_ranges_from_ack(
        ack.latest_ranges.as_slice(),
        expected,
        window_size,
        ack.latest_missing_count,
        ack.missing_ranges_full_count,
        max_window_retries,
    )
}

fn tail_gap_range_from_ack(
    tx_count: u64,
    latest_missing_count: Option<u64>,
    highest_sequence_seen: Option<u64>,
) -> Option<MissingRangeV1> {
    let highest = highest_sequence_seen?;
    if tx_count == 0 || latest_missing_count.unwrap_or_default() == 0 {
        return None;
    }
    let last = tx_count.saturating_sub(1);
    if highest >= last {
        return None;
    }
    Some(MissingRangeV1 {
        start: highest.saturating_add(1),
        end_inclusive: last,
    })
}

fn novorudp_tail_gap_coverage_limit(window_size: u64, tail_window_max_retries: u64) -> u64 {
    window_size
        .max(1)
        .saturating_mul(tail_window_max_retries.max(1))
}

fn novorudp_should_send_tail_gap(
    gap: MissingRangeV1,
    window_size: u64,
    tail_window_max_retries: u64,
) -> bool {
    missing_ranges_count(&[gap])
        <= novorudp_tail_gap_coverage_limit(window_size, tail_window_max_retries)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum NovoRudpSenderTimeoutDecisionV1 {
    Continue,
    NoProgressTimeout,
    AbsoluteTimeout,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct NovoRudpSenderRepairBudgetProfileV1 {
    profile: String,
    primary_send_duration_seconds: u64,
    repair_continuation_timeout_ms: u64,
    repair_no_progress_timeout_ms: u64,
    absolute_max_timeout_ms: u64,
    extend_repair_deadline_on_ack_progress: bool,
}

fn novorudp_default_profile_name(enabled: bool, sustained_duration_seconds: u64) -> &'static str {
    if !enabled {
        "novorudp-custom"
    } else if sustained_duration_seconds >= 7_200 {
        "novorudp-2h"
    } else if sustained_duration_seconds >= 1_800 {
        "novorudp-30min"
    } else {
        "novorudp-custom"
    }
}

fn novorudp_sender_repair_budget_profile_v1(
    novorudp_enabled: bool,
    sustained_duration_seconds: u64,
    sender_hard_timeout_ms: u64,
) -> Result<NovoRudpSenderRepairBudgetProfileV1> {
    let profile = string_env_nonempty("NOVOVM_NOVORUDP_PROFILE").unwrap_or_else(|| {
        novorudp_default_profile_name(novorudp_enabled, sustained_duration_seconds).to_string()
    });
    let is_two_hour_profile = novorudp_enabled && sustained_duration_seconds >= 7_200;
    let default_no_progress_timeout_ms = if novorudp_enabled {
        if is_two_hour_profile {
            300_000
        } else {
            120_000
        }
    } else {
        sender_hard_timeout_ms
    };
    let repair_no_progress_timeout_ms = u64_seconds_or_ms_env(
        &["NOVOVM_NOVORUDP_REPAIR_NO_PROGRESS_TIMEOUT_SECONDS"],
        &["NOVOVM_NOVORUDP_REPAIR_NO_PROGRESS_TIMEOUT_MS"],
        default_no_progress_timeout_ms,
    )?
    .max(1);
    let default_repair_continuation_timeout_ms = if novorudp_enabled {
        if is_two_hour_profile {
            3_600_000
        } else {
            sender_hard_timeout_ms.saturating_sub(sustained_duration_seconds.saturating_mul(1_000))
        }
    } else {
        0
    };
    let repair_continuation_timeout_ms = u64_seconds_env_alias_ms(
        &["NOVOVM_NOVORUDP_REPAIR_CONTINUATION_TIMEOUT_SECONDS"],
        default_repair_continuation_timeout_ms,
    )?;
    let default_absolute_max_timeout_ms = if novorudp_enabled {
        if is_two_hour_profile {
            12_600_000
        } else {
            sustained_duration_seconds
                .saturating_mul(1000)
                .saturating_add(900_000)
                .max(sender_hard_timeout_ms)
        }
    } else {
        sender_hard_timeout_ms
    };
    let absolute_max_timeout_ms = u64_seconds_or_ms_env(
        &["NOVOVM_NOVORUDP_ABSOLUTE_MAX_TIMEOUT_SECONDS"],
        &["NOVOVM_NOVORUDP_ABSOLUTE_SENDER_MAX_TIMEOUT_MS"],
        default_absolute_max_timeout_ms,
    )?
    .max(sender_hard_timeout_ms.max(1));
    let extend_repair_deadline_on_ack_progress =
        bool_env("NOVOVM_NOVORUDP_EXTEND_REPAIR_DEADLINE_ON_ACK_PROGRESS")
            || string_env_nonempty("NOVOVM_NOVORUDP_EXTEND_REPAIR_DEADLINE_ON_ACK_PROGRESS")
                .is_none();

    Ok(NovoRudpSenderRepairBudgetProfileV1 {
        profile,
        primary_send_duration_seconds: sustained_duration_seconds,
        repair_continuation_timeout_ms,
        repair_no_progress_timeout_ms,
        absolute_max_timeout_ms,
        extend_repair_deadline_on_ack_progress,
    })
}

fn novorudp_sender_timeout_decision_v1(
    elapsed_ms: u64,
    no_progress_elapsed_ms: u64,
    no_progress_timeout_ms: u64,
    absolute_sender_max_timeout_ms: u64,
    receiver_done: bool,
    missing_count: u64,
) -> NovoRudpSenderTimeoutDecisionV1 {
    if receiver_done && missing_count == 0 {
        return NovoRudpSenderTimeoutDecisionV1::Continue;
    }
    if absolute_sender_max_timeout_ms > 0 && elapsed_ms >= absolute_sender_max_timeout_ms {
        return NovoRudpSenderTimeoutDecisionV1::AbsoluteTimeout;
    }
    if no_progress_timeout_ms > 0 && no_progress_elapsed_ms >= no_progress_timeout_ms {
        return NovoRudpSenderTimeoutDecisionV1::NoProgressTimeout;
    }
    NovoRudpSenderTimeoutDecisionV1::Continue
}

#[cfg(test)]
fn merge_tail_gap_into_repair_ranges(
    selected_ranges: &[MissingRangeV1],
    tail_gap: Option<MissingRangeV1>,
    expected: u64,
) -> Vec<MissingRangeV1> {
    let mut merged = selected_ranges.to_vec();
    if let Some(gap) = tail_gap {
        let gap_count = missing_ranges_count(&[gap]);
        let existing_overlap = missing_ranges_overlap_count(&[gap], selected_ranges);
        if existing_overlap < gap_count {
            merged.push(gap);
        }
    }
    normalize_missing_ranges(merged.as_slice(), expected)
}

#[cfg(test)]
mod novorudp_tests {
    use super::*;
    use std::sync::atomic::{AtomicU64, Ordering};

    static TEST_DIAGNOSTICS_REPORT_COUNTER: AtomicU64 = AtomicU64::new(0);

    fn with_sender_hard_timeout_env<T>(timeout_ms: u64, run: impl FnOnce() -> T) -> T {
        let previous_timeout = std::env::var_os("NOVOVM_NATIVE_PIPELINE_SENDER_HARD_TIMEOUT_MS");
        let previous_report = std::env::var_os("NOVOVM_NATIVE_PIPELINE_SENDER_REPORT_ON_TIMEOUT");
        let previous_exit =
            std::env::var_os("NOVOVM_NATIVE_PIPELINE_SENDER_EXIT_ON_REPAIR_TIMEOUT");
        std::env::set_var(
            "NOVOVM_NATIVE_PIPELINE_SENDER_HARD_TIMEOUT_MS",
            timeout_ms.to_string(),
        );
        std::env::set_var("NOVOVM_NATIVE_PIPELINE_SENDER_REPORT_ON_TIMEOUT", "1");
        std::env::set_var("NOVOVM_NATIVE_PIPELINE_SENDER_EXIT_ON_REPAIR_TIMEOUT", "1");
        let result = run();
        match previous_timeout {
            Some(value) => {
                std::env::set_var("NOVOVM_NATIVE_PIPELINE_SENDER_HARD_TIMEOUT_MS", value)
            }
            None => std::env::remove_var("NOVOVM_NATIVE_PIPELINE_SENDER_HARD_TIMEOUT_MS"),
        }
        match previous_report {
            Some(value) => {
                std::env::set_var("NOVOVM_NATIVE_PIPELINE_SENDER_REPORT_ON_TIMEOUT", value)
            }
            None => std::env::remove_var("NOVOVM_NATIVE_PIPELINE_SENDER_REPORT_ON_TIMEOUT"),
        }
        match previous_exit {
            Some(value) => std::env::set_var(
                "NOVOVM_NATIVE_PIPELINE_SENDER_EXIT_ON_REPAIR_TIMEOUT",
                value,
            ),
            None => std::env::remove_var("NOVOVM_NATIVE_PIPELINE_SENDER_EXIT_ON_REPAIR_TIMEOUT"),
        }
        result
    }

    fn sender_timeout_tail_repair_config() -> TailRepairConfigV1 {
        TailRepairConfigV1 {
            enabled: true,
            rounds: 8,
            interval_ms: 10,
            require_ack: true,
            missing_sample_limit: 64,
            fallback_tail_window: 64,
            packet_copies: 1,
            tail_packet_copies: 1,
            batch_size: 8,
            batch_pause_ms: 0,
            tail_batch_pause_ms: 0,
            round_pause_ms: 10,
        }
    }

    fn sender_timeout_novorudp_config() -> NovoRudpConfigV1 {
        NovoRudpConfigV1 {
            enabled: true,
            window_size: 64,
            packet_copies: 1,
            tail_packet_copies: 1,
            batch_size: 8,
            batch_pause_ms: 0,
            window_ack_wait_ms: 10,
            max_window_retries: 32,
            tail_window_max_retries: 32,
            tail_window_packet_copies: 1,
            tail_window_batch_size: 8,
            tail_window_batch_pause_ms: 0,
            tail_window_ack_wait_ms: 10,
            ack_progress_interval_ms: 10,
            no_progress_backoff: true,
        }
    }

    fn with_env_var<T>(key: &str, value: Option<&str>, run: impl FnOnce() -> T) -> T {
        let previous = std::env::var_os(key);
        match value {
            Some(value) => std::env::set_var(key, value),
            None => std::env::remove_var(key),
        }
        let result = run();
        match previous {
            Some(value) => std::env::set_var(key, value),
            None => std::env::remove_var(key),
        }
        result
    }

    fn pipeline_liveness_sample(
        elapsed_ms: u64,
        pending: u64,
        child_ticks: u64,
        object_ready: u64,
        batch_ready: u64,
        batch_received: u64,
        tx_ingress_calls: u64,
        result_ready: u64,
        result_verified: u64,
        closed: u64,
    ) -> Value {
        serde_json::json!({
            "elapsed_ms": elapsed_ms,
            "receiver_udp_packet_recv_count": object_ready,
            "received_unique_total": object_ready,
            "aoem_executed_total": closed,
            "canonical_unique_included_total": closed,
            "receiver_ledger_close_count": closed,
            "ledger_durable_missing_count": 57_600u64.saturating_sub(closed),
            "queue_pending_last": pending,
            "receiver_child_tick_count": child_ticks,
            "receiver_aoem_tick_count": tx_ingress_calls,
            "receiver_pending_selected_count": tx_ingress_calls.saturating_mul(32),
            "network_receiver_object_ready_count": object_ready,
            "object_assembler_batch_ready_count": batch_ready,
            "aoem_runtime_worker_batch_received_count": batch_received,
            "aoem_runtime_worker_tx_ingress_call_count": tx_ingress_calls,
            "aoem_runtime_worker_result_ready_count": result_ready,
            "finality_report_worker_result_verified_count": result_verified,
        })
    }

    #[test]
    fn receiver_ack_backchannel_reports_missing_target_without_silent_success() {
        let path = std::env::temp_dir().join(format!(
            "novovm-receiver-ack-missing-target-{}.json",
            now_ms()
        ));
        with_env_var(
            "NOVOVM_NATIVE_PIPELINE_ACK_REPORT_PATH",
            path.to_str(),
            || {
                with_env_var("NOVOVM_NATIVE_PIPELINE_ACK_TARGET_ADDR", None, || {
                    with_env_var("NOVOVM_NATIVE_PIPELINE_SENDER_ACK_ADDR", None, || {
                        let status = send_receiver_udp_ack_with_summary(8, 4, 8, 9, None);
                        assert_eq!(status.send_ok_count, 0);
                        assert_eq!(status.missing_target_count, 1);
                        assert_eq!(status.last_error.as_deref(), Some("ack_target_missing"));

                        let report =
                            serde_json::from_slice::<Value>(&fs::read(path.as_path()).unwrap())
                                .unwrap();
                        assert_eq!(report["receiver_ack_send_ok_count"].as_u64(), Some(0));
                        assert_eq!(
                            report["receiver_ack_missing_target_count"].as_u64(),
                            Some(1)
                        );
                        assert_eq!(
                            report["receiver_ack_last_send_error"].as_str(),
                            Some("ack_target_missing")
                        );
                    })
                })
            },
        );
        let _ = fs::remove_file(path);
    }

    #[test]
    fn receiver_ack_backchannel_send_ok_reaches_sender_socket() {
        let listener = UdpSocket::bind("127.0.0.1:0").expect("ack listener");
        listener
            .set_read_timeout(Some(Duration::from_millis(1_000)))
            .expect("ack read timeout");
        let target = listener.local_addr().expect("listener addr").to_string();
        let path = std::env::temp_dir().join(format!("novovm-receiver-ack-ok-{}.json", now_ms()));
        with_env_var(
            "NOVOVM_NATIVE_PIPELINE_ACK_REPORT_PATH",
            path.to_str(),
            || {
                with_env_var(
                    "NOVOVM_NATIVE_PIPELINE_ACK_TARGET_ADDR",
                    Some(target.as_str()),
                    || {
                        with_env_var("NOVOVM_NATIVE_PIPELINE_SENDER_ACK_ADDR", None, || {
                            with_env_var(
                                "NOVOVM_NATIVE_PIPELINE_ACK_BIND_ADDR",
                                Some("127.0.0.1:0"),
                                || {
                                    let status =
                                        send_receiver_udp_ack_with_summary(8, 4, 8, 10, None);
                                    assert_eq!(status.attempted_count, 1);
                                    assert_eq!(status.send_ok_count, 1);
                                    assert_eq!(status.send_error_count, 0);

                                    let mut buf = [0u8; 4096];
                                    let (len, _src) =
                                        listener.recv_from(&mut buf).expect("ack packet");
                                    let packet =
                                        serde_json::from_slice::<Value>(&buf[..len]).unwrap();
                                    assert_eq!(packet["ack_epoch"].as_u64(), Some(10));

                                    let report = serde_json::from_slice::<Value>(
                                        &fs::read(path.as_path()).unwrap(),
                                    )
                                    .unwrap();
                                    assert_eq!(
                                        report["receiver_ack_send_ok_count"].as_u64(),
                                        Some(1)
                                    );
                                    assert_eq!(
                                        report["receiver_ack_target_addr"].as_str(),
                                        Some(target.as_str())
                                    );
                                },
                            )
                        })
                    },
                )
            },
        );
        let _ = fs::remove_file(path);
    }

    fn with_clean_repair_budget_env<T>(run: impl FnOnce() -> T) -> T {
        with_env_var("NOVOVM_NOVORUDP_PROFILE", None, || {
            with_env_var(
                "NOVOVM_NOVORUDP_REPAIR_CONTINUATION_TIMEOUT_SECONDS",
                None,
                || {
                    with_env_var(
                        "NOVOVM_NOVORUDP_REPAIR_NO_PROGRESS_TIMEOUT_SECONDS",
                        None,
                        || {
                            with_env_var(
                                "NOVOVM_NOVORUDP_REPAIR_NO_PROGRESS_TIMEOUT_MS",
                                None,
                                || {
                                    with_env_var(
                                        "NOVOVM_NOVORUDP_ABSOLUTE_MAX_TIMEOUT_SECONDS",
                                        None,
                                        || {
                                            with_env_var(
                                                "NOVOVM_NOVORUDP_ABSOLUTE_SENDER_MAX_TIMEOUT_MS",
                                                None,
                                                || {
                                                    with_env_var(
                                    "NOVOVM_NOVORUDP_EXTEND_REPAIR_DEADLINE_ON_ACK_PROGRESS",
                                    None,
                                    run,
                                )
                                                },
                                            )
                                        },
                                    )
                                },
                            )
                        },
                    )
                },
            )
        })
    }

    #[test]
    fn two_hour_profile_does_not_use_30min_absolute_timeout() {
        with_clean_repair_budget_env(|| {
            let profile = novorudp_sender_repair_budget_profile_v1(true, 7_200, 7_320_000)
                .expect("2h budget profile");

            assert_eq!(profile.profile, "novorudp-2h");
            assert_eq!(profile.primary_send_duration_seconds, 7_200);
            assert_eq!(profile.repair_continuation_timeout_ms, 3_600_000);
            assert_eq!(profile.repair_no_progress_timeout_ms, 300_000);
            assert_eq!(profile.absolute_max_timeout_ms, 12_600_000);
            assert!(profile.extend_repair_deadline_on_ack_progress);
        });
    }

    #[test]
    fn repair_continues_while_ack_missing_progresses() {
        with_clean_repair_budget_env(|| {
            let profile = novorudp_sender_repair_budget_profile_v1(true, 7_200, 7_320_000)
                .expect("2h budget profile");
            let decision = novorudp_sender_timeout_decision_v1(
                8_440_784,
                0,
                profile.repair_no_progress_timeout_ms,
                profile.absolute_max_timeout_ms,
                false,
                23_432,
            );

            assert_eq!(decision, NovoRudpSenderTimeoutDecisionV1::Continue);
        });
    }

    #[test]
    fn repair_fails_only_after_no_progress_timeout() {
        with_clean_repair_budget_env(|| {
            let profile = novorudp_sender_repair_budget_profile_v1(true, 7_200, 7_320_000)
                .expect("2h budget profile");
            let decision = novorudp_sender_timeout_decision_v1(
                8_000_000,
                profile.repair_no_progress_timeout_ms,
                profile.repair_no_progress_timeout_ms,
                profile.absolute_max_timeout_ms,
                false,
                23_432,
            );

            assert_eq!(decision, NovoRudpSenderTimeoutDecisionV1::NoProgressTimeout);
        });
    }

    #[test]
    fn absolute_timeout_reports_profile_budget() {
        with_clean_repair_budget_env(|| {
            with_env_var(
                "NOVOVM_NOVORUDP_ABSOLUTE_MAX_TIMEOUT_SECONDS",
                Some("13000"),
                || {
                    let profile = novorudp_sender_repair_budget_profile_v1(true, 7_200, 7_320_000)
                        .expect("2h budget profile");
                    let decision = novorudp_sender_timeout_decision_v1(
                        13_000_000,
                        0,
                        profile.repair_no_progress_timeout_ms,
                        profile.absolute_max_timeout_ms,
                        false,
                        23_432,
                    );

                    assert_eq!(profile.absolute_max_timeout_ms, 13_000_000);
                    assert_eq!(decision, NovoRudpSenderTimeoutDecisionV1::AbsoluteTimeout);
                },
            );
        });
    }

    #[test]
    fn receiver_done_ack_ends_repair_before_absolute_timeout() {
        let decision =
            novorudp_sender_timeout_decision_v1(12_600_001, 0, 300_000, 12_600_000, true, 0);

        assert_eq!(decision, NovoRudpSenderTimeoutDecisionV1::Continue);
    }

    #[test]
    fn sender_progress_report_path_uses_sidecar_by_default() {
        with_env_var(
            "NOVOVM_NATIVE_PIPELINE_REPORT_PATH",
            Some("artifacts/native-pipeline/sender-cross-machine-novorudp-2h-report.json"),
            || {
                with_env_var(
                    "NOVOVM_NATIVE_PIPELINE_SENDER_PROGRESS_REPORT_PATH",
                    None,
                    || {
                        let path = sender_progress_report_path();

                        assert!(path
                            .to_string_lossy()
                            .ends_with("sender-cross-machine-novorudp-2h-report.progress.json"));
                    },
                );
            },
        );
    }

    #[test]
    fn sender_live_progress_report_contains_ack_drain_fields() {
        let path =
            std::env::temp_dir().join(format!("novorudp-sender-live-progress-{}.json", now_ms()));
        write_sender_live_progress_report_v1(
            path.as_path(),
            10_000,
            57_600,
            80,
            80,
            0,
            12,
            44,
            3,
            9_500,
            41,
            Some(57_520),
            Some(79),
            false,
            false,
        )
        .expect("write sender live progress");
        let value = read_json_file(path.as_path()).expect("read sender live progress");
        let _ = fs::remove_file(path.as_path());

        assert_eq!(
            value.get("report_type").and_then(Value::as_str),
            Some("sender_live_progress_v1")
        );
        assert_eq!(value["primary_ack_drain_count"].as_u64(), Some(12));
        assert_eq!(value["primary_ack_received_count"].as_u64(), Some(44));
        assert_eq!(value["latest_ack_missing_count"].as_u64(), Some(57_520));
        assert_eq!(value["last_sent_sequence"].as_u64(), Some(79));
    }

    fn receiver_phase_test_config() -> ReceiverDiagnosticsConfigV1 {
        ReceiverDiagnosticsConfigV1 {
            enabled: true,
            sample_interval_ms: 250,
            stall_windows: 2,
            pending_drain_no_progress_timeout_ms: 30_000,
            memory_sample_enabled: true,
            max_working_set_bytes: 0,
            min_canonical_delta: 0,
            max_elapsed_ms: 2_700_000,
            primary_send_duration_ms: 1_800_000,
            repair_drain_timeout_ms: 900_000,
            final_ack_timeout_ms: 120_000,
            absolute_max_ms: 2_700_000,
            report_path: PathBuf::from("unused.json"),
        }
    }

    #[test]
    fn receiver_child_env_preserves_novorudp_expected_tx_count() {
        let envs = receiver_child_expected_total_envs_v1(14_400);
        assert!(envs
            .iter()
            .any(|(key, value)| { *key == "NOVOVM_NATIVE_PIPELINE_TX_COUNT" && value == "14400" }));
        assert!(envs.iter().any(|(key, value)| {
            *key == "NOVOVM_NATIVE_EXECUTION_PIPELINE_EXPECTED_TX_COUNT" && value == "14400"
        }));
    }

    #[test]
    fn receiver_child_inherits_tx_count_env() {
        let envs = receiver_child_expected_total_envs_v1(14_400);
        let tx_count = envs
            .iter()
            .find(|(key, _)| *key == "NOVOVM_NATIVE_PIPELINE_TX_COUNT")
            .map(|(_, value)| value.as_str());
        let expected_total = envs
            .iter()
            .find(|(key, _)| *key == "NOVOVM_NATIVE_EXECUTION_PIPELINE_EXPECTED_TX_COUNT")
            .map(|(_, value)| value.as_str());

        assert_eq!(tx_count, Some("14400"));
        assert_eq!(expected_total, Some("14400"));
    }

    #[test]
    fn receiver_child_inherits_aoem_owned_candidate_envs() {
        with_env_var(
            NOV_NATIVE_AOEM_NATIVE_TX_BATCH_PRODUCTION_CANDIDATE_ENV,
            Some("1"),
            || {
                with_env_var(
                    NOV_NATIVE_AOEM_NATIVE_TX_BATCH_SHADOW_ENV,
                    Some("1"),
                    || {
                        with_env_var(
                            NOV_NATIVE_AOEM_NATIVE_TX_BATCH_COMPARE_ENV,
                            Some("1"),
                            || {
                                let envs = receiver_child_aoem_ownership_envs_v1();

                                assert!(envs.iter().any(|(key, value)| {
                                    *key == NOV_NATIVE_AOEM_NATIVE_TX_BATCH_PRODUCTION_CANDIDATE_ENV
                                        && value == "1"
                                }));
                                assert!(envs.iter().any(|(key, value)| {
                                    *key == NOV_NATIVE_AOEM_NATIVE_TX_BATCH_SHADOW_ENV
                                        && value == "1"
                                }));
                                assert!(envs.iter().any(|(key, value)| {
                                    *key == NOV_NATIVE_AOEM_NATIVE_TX_BATCH_COMPARE_ENV
                                        && value == "1"
                                }));
                            },
                        )
                    },
                )
            },
        );
    }

    fn receiver_validation_probe_v1(tx_count: u64) -> Value {
        serde_json::json!({
            "receipt_count": tx_count,
            "semantic_head": {
                "sequence": tx_count,
            },
            "semantic_head_current_recovered": true,
            "semantic_head_by_height_recovered": true,
            "receipt_index_recovered": true,
        })
    }

    fn receiver_validation_summary_v1(tx_count: u64) -> Value {
        serde_json::json!({
            "accepted": true,
            "execution_kernel": "AOEM",
            "aoem_concurrency_owner": "AOEM_runtime",
            "host_concurrency_policy": "host_drives_lifecycle_only_no_rust_execution_scheduler",
            "ingress_total_last": tx_count,
            "aoem_executed_total": tx_count,
            "included_canonical_total": tx_count,
            "queue_pending_last": 0,
            "aoem_native_tx_batch_production_candidate_enabled": true,
            "aoem_native_tx_batch_production_candidate_result_ok": true,
            "aoem_native_tx_batch_production_owner": AOEM_RUNTIME_OWNED_PRODUCTION_TARGET_V1,
            "tx_ingress_production_target": AOEM_RUNTIME_OWNED_PRODUCTION_TARGET_V1,
            "tx_ingress_selected_path": AOEM_RUNTIME_OWNED_PRODUCTION_TARGET_V1,
            "child_runtime_aoem_gate_config_source": "receiver_child_runtime",
            "tx_ingress_aoem_gate_config_source": "receiver_child_runtime",
            "tx_ingress_aoem_gate_config_production_candidate": true,
            "tx_ingress_aoem_gate_config_shadow": true,
            "tx_ingress_aoem_gate_config_compare": true,
            "aoem_owned_child_runtime_gate_propagated_to_tx_ingress": true,
            "aoem_owned_single_path_enforced": true,
            "legacy_host_transitional_fallback_gate_enabled": false,
            "legacy_host_transitional_fallback_used": false,
            "legacy_host_transitional_success_suppressed_by_aoem_gate": false,
            "aoem_owned_regression_signable": true,
            "aoem_owned_signoff_blocker_reasons": [],
            "tx_ingress_real_callsite": "nov_sendRawTransactionBatch",
            "tx_ingress_called_with_explicit_aoem_gate_config": true,
            "receiver_final_summary_aoem_fields_present": true,
            "receiver_final_summary_aoem_fields_defaulted": false,
            "receiver_final_summary_aoem_fields_missing_reasons": [],
            "aoem_owned_gate_fail_reason": "",
            "aoem_native_tx_batch_production_receipt_count": tx_count,
            "aoem_native_tx_batch_production_canonical_proof_count": tx_count,
            "aoem_native_tx_batch_production_ledger_close_proof_count": tx_count,
            "aoem_native_tx_batch_production_state_delta_root_present": true,
            "aoem_native_tx_batch_production_snapshot_metadata_present": true,
            "aoem_native_tx_batch_production_fallback_used": false,
            "aoem_native_tx_batch_production_mismatch_reasons": [],
            "aoem_native_tx_batch_production_double_write_legacy_canonical": false,
        })
    }

    #[test]
    fn receiver_validation_accepts_aoem_owned_production_candidate() {
        with_env_var(
            NOV_NATIVE_AOEM_NATIVE_TX_BATCH_PRODUCTION_CANDIDATE_ENV,
            Some("1"),
            || {
                let tx_count = 4;
                let summary = receiver_validation_summary_v1(tx_count);
                let probe = receiver_validation_probe_v1(tx_count);
                let (validation, violations) = validate_receiver_report(&summary, &probe, tx_count);

                assert!(violations.is_empty(), "{violations:?}");
                assert_eq!(
                    validation["aoem_production_candidate"]
                        ["aoem_native_tx_batch_production_owner"]
                        .as_str(),
                    Some(AOEM_RUNTIME_OWNED_PRODUCTION_TARGET_V1)
                );
                assert_eq!(
                    validation["aoem_production_candidate"]
                        ["aoem_native_tx_batch_production_fallback_used"]
                        .as_bool(),
                    Some(false)
                );
            },
        );
    }

    #[test]
    fn receiver_validation_rejects_missing_aoem_owned_candidate_fields_when_gate_on() {
        with_env_var(
            NOV_NATIVE_AOEM_NATIVE_TX_BATCH_PRODUCTION_CANDIDATE_ENV,
            Some("1"),
            || {
                let tx_count = 4;
                let mut summary = receiver_validation_summary_v1(tx_count);
                summary
                    .as_object_mut()
                    .expect("summary object")
                    .remove("aoem_native_tx_batch_production_candidate_enabled");
                let probe = receiver_validation_probe_v1(tx_count);
                let (_validation, violations) =
                    validate_receiver_report(&summary, &probe, tx_count);

                assert!(violations.iter().any(|item| item.contains(
                    "aoem_native_tx_batch_production_candidate_enabled=false expected true"
                )));
            },
        );
    }

    #[test]
    fn receiver_final_summary_rejects_defaulted_aoem_fields_when_gate_requested() {
        with_env_var(
            NOV_NATIVE_AOEM_NATIVE_TX_BATCH_PRODUCTION_CANDIDATE_ENV,
            Some("1"),
            || {
                let tx_count = 4;
                let mut summary = receiver_validation_summary_v1(tx_count);
                summary["receiver_final_summary_aoem_fields_defaulted"] = serde_json::json!(true);
                summary["receiver_final_summary_aoem_fields_missing_reasons"] =
                    serde_json::json!(["tx_ingress_aoem_fields_not_observed"]);
                summary["aoem_owned_gate_fail_reason"] =
                    serde_json::json!("aoem_owned_gate_requested_but_summary_fields_missing");
                let probe = receiver_validation_probe_v1(tx_count);
                let (_validation, violations) =
                    validate_receiver_report(&summary, &probe, tx_count);

                assert!(violations.iter().any(|item| item
                    .contains("receiver_final_summary_aoem_fields_defaulted=true expected false")));
                assert!(violations
                    .iter()
                    .any(|item| item
                        .contains("aoem_owned_gate_requested_but_summary_fields_missing")));
            },
        );
    }

    #[test]
    fn receiver_fails_when_gate_requested_but_tx_ingress_not_aoem_owned() {
        with_env_var(
            NOV_NATIVE_AOEM_NATIVE_TX_BATCH_PRODUCTION_CANDIDATE_ENV,
            Some("1"),
            || {
                let tx_count = 4;
                let mut summary = receiver_validation_summary_v1(tx_count);
                summary["tx_ingress_selected_path"] = serde_json::json!("legacy_host_transitional");
                summary["aoem_owned_gate_fail_reason"] =
                    serde_json::json!("aoem_owned_gate_requested_but_tx_ingress_not_aoem_owned");
                let probe = receiver_validation_probe_v1(tx_count);
                let (_validation, violations) =
                    validate_receiver_report(&summary, &probe, tx_count);

                assert!(violations.iter().any(|item| item
                    .contains("tx_ingress_selected_path=legacy_host_transitional expected")));
                assert!(violations
                    .iter()
                    .any(|item| item
                        .contains("aoem_owned_gate_requested_but_tx_ingress_not_aoem_owned")));
            },
        );
    }

    #[test]
    fn compact_receiver_summary_preserves_aoem_owned_candidate_fields() {
        let summary = receiver_validation_summary_v1(4);
        let compact = compact_receiver_summary_for_report(summary);

        assert_eq!(
            compact["aoem_native_tx_batch_production_candidate_enabled"].as_bool(),
            Some(true)
        );
        assert_eq!(
            compact["aoem_native_tx_batch_production_owner"].as_str(),
            Some(AOEM_RUNTIME_OWNED_PRODUCTION_TARGET_V1)
        );
        assert_eq!(
            compact["aoem_native_tx_batch_production_receipt_count"].as_u64(),
            Some(4)
        );
    }

    #[test]
    fn mini_receiver_reports_single_path_fields_in_live_diagnostics() {
        with_env_var(
            NOV_NATIVE_AOEM_NATIVE_TX_BATCH_PRODUCTION_CANDIDATE_ENV,
            Some("1"),
            || {
                let mut summary = receiver_validation_summary_v1(480);
                summary["child_expected_total_from_config"] = serde_json::json!(480);
                summary["ledger_expected_count"] = serde_json::json!(480);
                let sample = diagnostics_summary_sample(
                    Instant::now(),
                    &summary,
                    serde_json::json!({}),
                    serde_json::json!({}),
                    serde_json::json!({}),
                    480,
                );

                assert_eq!(
                    sample["aoem_owned_single_path_enforced"].as_bool(),
                    Some(true)
                );
                assert_eq!(
                    sample["tx_ingress_called_with_explicit_aoem_gate_config"].as_bool(),
                    Some(true)
                );
                assert_eq!(
                    sample["tx_ingress_aoem_gate_config_source"].as_str(),
                    Some("receiver_child_runtime")
                );
                assert_eq!(
                    sample["tx_ingress_selected_path"].as_str(),
                    Some(AOEM_RUNTIME_OWNED_PRODUCTION_TARGET_V1)
                );
                assert_eq!(
                    sample["aoem_owned_regression_signable"].as_bool(),
                    Some(true)
                );
            },
        );
    }

    #[test]
    fn mini_receiver_fails_when_single_path_fields_missing_under_gate() {
        with_env_var(
            NOV_NATIVE_AOEM_NATIVE_TX_BATCH_PRODUCTION_CANDIDATE_ENV,
            Some("1"),
            || {
                let summary = serde_json::json!({
                    "ingress_total_last": 480,
                    "included_canonical_total": 480,
                    "aoem_executed_total": 480,
                    "ledger_completed_count": 480,
                    "ledger_expected_count": 480,
                    "child_expected_total_from_config": 480,
                    "queue_pending_last": 0,
                });
                let sample = diagnostics_summary_sample(
                    Instant::now(),
                    &summary,
                    serde_json::json!({}),
                    serde_json::json!({}),
                    serde_json::json!({}),
                    480,
                );

                assert_eq!(sample["accepted"].as_bool(), Some(false));
                assert_eq!(
                    sample["fail_reason"].as_str(),
                    Some("aoem_owned_single_path_diagnostics_missing_under_gate")
                );
                assert_eq!(
                    sample["aoem_owned_single_path_enforced"].as_bool(),
                    Some(false)
                );
            },
        );
    }

    #[test]
    fn mini_receiver_reports_tail_missing_ranges() {
        let summary = serde_json::json!({
            "ingress_total_last": 448,
            "included_canonical_total": 448,
            "aoem_executed_total": 448,
            "ledger_completed_count": 448,
            "ledger_expected_count": 480,
            "child_expected_total_from_config": 480,
            "ledger_durable_missing_count": 32,
            "ledger_durable_missing_ranges_sample": [
                {"start": 448, "end_inclusive": 479, "count": 32}
            ],
            "queue_pending_last": 0,
        });
        let sample = diagnostics_summary_sample(
            Instant::now(),
            &summary,
            serde_json::json!({}),
            serde_json::json!({}),
            serde_json::json!({}),
            448,
        );

        assert_eq!(sample["mini_expected_tx_count"].as_u64(), Some(480));
        assert_eq!(sample["mini_completed_tx_count"].as_u64(), Some(448));
        assert_eq!(sample["mini_tail_missing_count"].as_u64(), Some(32));
        assert_eq!(
            sample["mini_tail_missing_ranges_sample"][0]["start"].as_u64(),
            Some(448)
        );
        assert_eq!(
            sample["mini_tail_repair_stall_reason"].as_str(),
            Some("pending_empty_waiting_for_sender_repair")
        );
    }

    #[test]
    fn mini_tail_missing_ranges_are_reported() {
        let summary = serde_json::json!({
            "aoem_executed_total": 464,
            "included_canonical_total": 464,
            "ledger_completed_count": 464,
            "ledger_expected_count": 480,
            "child_expected_total_from_config": 480,
            "ledger_durable_missing_count": 16,
            "ledger_durable_missing_ranges_sample": [
                {"start": 464, "end_inclusive": 479, "count": 16}
            ],
            "queue_pending_last": 0,
        });
        let sample = diagnostics_summary_sample(
            Instant::now() - Duration::from_secs(90),
            &summary,
            serde_json::json!({}),
            serde_json::json!({}),
            serde_json::json!({}),
            464,
        );

        assert_eq!(sample["mini_tail_missing_count"].as_u64(), Some(16));
        assert_eq!(
            sample["mini_tail_missing_ranges_sample"][0]["start"].as_u64(),
            Some(464)
        );
        assert_eq!(
            sample["receiver_durable_missing_ranges_sample"][0]["end_inclusive"].as_u64(),
            Some(479)
        );
    }

    #[test]
    fn mini_ack_contains_tail_missing_ranges() {
        let summary = serde_json::json!({
            "aoem_executed_total": 464,
            "included_canonical_total": 464,
            "ledger_completed_count": 464,
            "ledger_expected_count": 480,
            "child_expected_total_from_config": 480,
            "ledger_durable_missing_count": 16,
            "ledger_durable_missing_ranges_sample": [
                {"start": 464, "end_inclusive": 479, "count": 16}
            ],
            "queue_pending_last": 0,
        });
        let sample = diagnostics_summary_sample(
            Instant::now() - Duration::from_secs(90),
            &summary,
            serde_json::json!({}),
            serde_json::json!({}),
            serde_json::json!({}),
            464,
        );

        assert_eq!(sample["receiver_ack_missing_count"].as_u64(), Some(16));
        assert_eq!(
            sample["receiver_latest_ack_missing_ranges_sample"][0]["start"].as_u64(),
            Some(464)
        );
        assert_eq!(
            sample["receiver_waiting_for_sender_repair"].as_bool(),
            Some(true)
        );
    }

    #[test]
    fn sender_tail_repair_covers_ack_missing_ranges() {
        let ack_missing = vec![MissingRangeV1 {
            start: 464,
            end_inclusive: 479,
        }];
        let repair_sent = vec![MissingRangeV1 {
            start: 464,
            end_inclusive: 479,
        }];
        let report = novorudp_sender_repair_coverage_report_v1(
            ack_missing.as_slice(),
            repair_sent.as_slice(),
            480,
            16,
            8,
        );

        assert_eq!(
            report["sender_repair_sent_overlap_ack_missing_count"].as_u64(),
            Some(16)
        );
        assert_eq!(
            report["sender_repair_sent_new_missing_coverage_count"].as_u64(),
            Some(16)
        );
        assert_eq!(report["repair_duplicate_waste_ratio_bps"].as_u64(), Some(0));
    }

    #[test]
    fn receiver_tail_repair_received_overlap_missing() {
        let summary = serde_json::json!({
            "aoem_executed_total": 464,
            "included_canonical_total": 464,
            "ledger_completed_count": 464,
            "ledger_expected_count": 480,
            "child_expected_total_from_config": 480,
            "ledger_durable_missing_count": 16,
            "ledger_durable_missing_ranges_sample": [
                {"start": 464, "end_inclusive": 479, "count": 16}
            ],
            "repair_packet_received_count": 4,
            "repair_sequence_received_count": 16,
            "repair_sequence_received_ranges_sample": [
                {"start": 464, "end_inclusive": 479, "count": 16}
            ],
            "queue_pending_last": 0,
        });
        let sample = diagnostics_summary_sample(
            Instant::now() - Duration::from_secs(90),
            &summary,
            serde_json::json!({}),
            serde_json::json!({}),
            serde_json::json!({}),
            464,
        );

        assert_eq!(
            sample["receiver_repair_received_overlap_missing_count"].as_u64(),
            Some(16)
        );
        assert_eq!(
            sample["receiver_repair_packet_recv_count"].as_u64(),
            Some(4)
        );
        assert_eq!(
            sample["mini_repair_received_overlap_missing_count"].as_u64(),
            Some(16)
        );
    }

    #[test]
    fn receiver_tail_repair_executes_and_closes_ledger() {
        let summary = serde_json::json!({
            "aoem_executed_total": 480,
            "included_canonical_total": 480,
            "ledger_completed_count": 480,
            "ledger_expected_count": 480,
            "child_expected_total_from_config": 480,
            "ledger_durable_missing_count": 0,
            "repair_packet_received_count": 4,
            "repair_sequence_received_ranges_sample": [
                {"start": 464, "end_inclusive": 479, "count": 16}
            ],
            "repair_sequence_accepted_ranges_sample": [
                {"start": 464, "end_inclusive": 479, "count": 16}
            ],
            "repair_sequence_admitted_to_aoem_ranges_sample": [
                {"start": 464, "end_inclusive": 479, "count": 16}
            ],
            "queue_pending_last": 0,
        });
        let sample = diagnostics_summary_sample(
            Instant::now() - Duration::from_secs(90),
            &summary,
            serde_json::json!({}),
            serde_json::json!({}),
            serde_json::json!({}),
            480,
        );

        assert_eq!(sample["mini_tail_missing_count"].as_u64(), Some(0));
        assert_eq!(
            sample["mini_tail_repair_stall_reason"].as_str(),
            Some("closed")
        );
        assert_eq!(
            sample["receiver_repair_ledger_closed_overlap_missing_count"].as_u64(),
            Some(0)
        );
    }

    #[test]
    fn mini_receiver_fails_when_tail_missing_not_closed() {
        with_env_var(
            NOV_NATIVE_AOEM_NATIVE_TX_BATCH_PRODUCTION_CANDIDATE_ENV,
            Some("1"),
            || {
                let mut summary = receiver_validation_summary_v1(448);
                summary["ingress_total_last"] = serde_json::json!(448);
                summary["included_canonical_total"] = serde_json::json!(448);
                summary["aoem_executed_total"] = serde_json::json!(448);
                summary["ledger_completed_count"] = serde_json::json!(448);
                summary["ledger_expected_count"] = serde_json::json!(480);
                summary["child_expected_total_from_config"] = serde_json::json!(480);
                summary["ledger_durable_missing_count"] = serde_json::json!(32);
                summary["ledger_durable_missing_ranges_sample"] =
                    serde_json::json!([{"start": 448, "end_inclusive": 479, "count": 32}]);
                summary["queue_pending_last"] = serde_json::json!(0);
                let sample = diagnostics_summary_sample(
                    Instant::now() - Duration::from_secs(180),
                    &summary,
                    serde_json::json!({}),
                    serde_json::json!({}),
                    serde_json::json!({}),
                    448,
                );

                assert_eq!(sample["accepted"].as_bool(), Some(false));
                assert_eq!(
                    sample["fail_reason"].as_str(),
                    Some("mini_tail_repair_missing_not_closed")
                );
                assert_eq!(
                    sample["mini_waiting_for_sender_repair"].as_bool(),
                    Some(true)
                );
            },
        );
    }

    #[test]
    fn mini_fails_when_tail_repair_not_closed() {
        with_env_var(
            NOV_NATIVE_AOEM_NATIVE_TX_BATCH_PRODUCTION_CANDIDATE_ENV,
            Some("1"),
            || {
                let mut summary = receiver_validation_summary_v1(464);
                summary["aoem_executed_total"] = serde_json::json!(464);
                summary["included_canonical_total"] = serde_json::json!(464);
                summary["ledger_completed_count"] = serde_json::json!(464);
                summary["ledger_expected_count"] = serde_json::json!(480);
                summary["child_expected_total_from_config"] = serde_json::json!(480);
                summary["ledger_durable_missing_count"] = serde_json::json!(16);
                summary["ledger_durable_missing_ranges_sample"] =
                    serde_json::json!([{"start": 464, "end_inclusive": 479, "count": 16}]);
                summary["queue_pending_last"] = serde_json::json!(0);
                let sample = diagnostics_summary_sample(
                    Instant::now() - Duration::from_secs(90),
                    &summary,
                    serde_json::json!({}),
                    serde_json::json!({}),
                    serde_json::json!({}),
                    464,
                );

                assert_eq!(sample["accepted"].as_bool(), Some(false));
                assert_eq!(
                    sample["fail_reason"].as_str(),
                    Some("mini_tail_repair_missing_not_closed")
                );
            },
        );
    }

    #[test]
    fn mini_480_closes_after_tail_repair() {
        with_env_var(
            NOV_NATIVE_AOEM_NATIVE_TX_BATCH_PRODUCTION_CANDIDATE_ENV,
            Some("1"),
            || {
                let mut summary = receiver_validation_summary_v1(480);
                summary["child_expected_total_from_config"] = serde_json::json!(480);
                summary["ledger_expected_count"] = serde_json::json!(480);
                summary["ledger_durable_missing_count"] = serde_json::json!(0);
                let sample = diagnostics_summary_sample(
                    Instant::now() - Duration::from_secs(180),
                    &summary,
                    serde_json::json!({}),
                    serde_json::json!({}),
                    serde_json::json!({}),
                    480,
                );

                assert_eq!(sample["mini_tail_missing_count"].as_u64(), Some(0));
                assert_eq!(
                    sample["mini_tail_repair_stall_reason"].as_str(),
                    Some("closed")
                );
                assert!(sample.get("fail_reason").is_none());
            },
        );
    }

    fn test_diagnostics_config(report_path: PathBuf) -> ReceiverDiagnosticsConfigV1 {
        ReceiverDiagnosticsConfigV1 {
            enabled: true,
            sample_interval_ms: 5_000,
            stall_windows: 2,
            pending_drain_no_progress_timeout_ms: 30_000,
            memory_sample_enabled: true,
            max_working_set_bytes: 0,
            min_canonical_delta: 0,
            max_elapsed_ms: 0,
            primary_send_duration_ms: 60_000,
            repair_drain_timeout_ms: 60_000,
            final_ack_timeout_ms: 10_000,
            absolute_max_ms: 180_000,
            report_path,
        }
    }

    fn write_test_diagnostics_report(
        stale_live_sample: Value,
        final_closed_sample: Value,
    ) -> Value {
        write_test_diagnostics_report_for_tx_count(stale_live_sample, final_closed_sample, 480)
    }

    fn write_test_diagnostics_report_for_tx_count(
        stale_live_sample: Value,
        final_closed_sample: Value,
        tx_count: u64,
    ) -> Value {
        write_test_diagnostics_report_samples_for_tx_count(
            vec![stale_live_sample, final_closed_sample],
            tx_count,
        )
    }

    fn write_test_diagnostics_report_samples_for_tx_count(
        samples: Vec<Value>,
        tx_count: u64,
    ) -> Value {
        let unique = TEST_DIAGNOSTICS_REPORT_COUNTER.fetch_add(1, Ordering::Relaxed);
        let path = std::env::temp_dir().join(format!(
            "novovm-mini-final-diagnostics-{}-{}-{}.json",
            std::process::id(),
            now_ms(),
            unique
        ));
        let config = test_diagnostics_config(path.clone());
        let state = ReceiverDiagnosticsStateV1 {
            samples,
            last_canonical: 480,
            stall_windows: 0,
            pending_drain_no_progress_ms: 0,
            fail_reason: None,
            samples_dropped: 0,
            first_working_set_bytes: Some(1),
            last_working_set_bytes: Some(1),
        };
        write_diagnostics_report(&config, &state, true, 42, tx_count).unwrap();
        let report = serde_json::from_slice::<Value>(&fs::read(path.as_path()).unwrap()).unwrap();
        let _ = fs::remove_file(path);
        report
    }

    fn stale_tail_live_sample_for_test() -> Value {
        serde_json::json!({
            "elapsed_ms": 99_000,
            "process_working_set_bytes": 1,
            "process_private_bytes": 1,
            "mini_completed_tx_count": 440,
            "mini_tail_missing_count": 40,
            "aoem_owned_regression_signable": true,
            "accepted": false,
            "fail_reason": "mini_tail_repair_missing_not_closed",
            "aoem_owned_signoff_blocker_reasons": ["mini_tail_repair_missing_not_closed"]
        })
    }

    fn final_closed_sample_for_test() -> Value {
        serde_json::json!({
            "elapsed_ms": 103_000,
            "final_closed_child_sample": true,
            "final_closed_child_sample_available": true,
            "receiver_exit_phase": "completed",
            "received_unique_total": 32,
            "mini_completed_tx_count": 480,
            "mini_tail_missing_count": 0,
            "canonical_unique_included_total": 480,
            "receiver_ledger_close_count": 480,
            "aoem_native_tx_batch_production_receipt_count": 480,
            "aoem_native_tx_batch_production_canonical_proof_count": 480,
            "aoem_native_tx_batch_production_ledger_close_proof_count": 480,
            "aoem_owned_regression_signable": true,
            "accepted": true,
            "aoem_owned_signoff_blocker_reasons": []
        })
    }

    #[test]
    fn mini_final_closed_sample_overrides_stale_live_sample() {
        let report = write_test_diagnostics_report(
            stale_tail_live_sample_for_test(),
            final_closed_sample_for_test(),
        );

        assert_eq!(report["accepted"].as_bool(), Some(true));
        assert!(report["fail_reason"].is_null());
        assert_eq!(
            report["diagnostics_final_sample_mini_completed_tx_count"].as_u64(),
            Some(480)
        );
        assert_eq!(
            report["diagnostics_final_sample_mini_tail_missing_count"].as_u64(),
            Some(0)
        );
    }

    #[test]
    fn mini_final_pass_does_not_leak_stale_tail_fail_reason() {
        let report = write_test_diagnostics_report(
            stale_tail_live_sample_for_test(),
            final_closed_sample_for_test(),
        );

        assert_eq!(
            report["stale_live_sample_fail_reason_ignored"].as_bool(),
            Some(true)
        );
        assert_eq!(
            report["stale_live_sample_fail_reason"].as_str(),
            Some("mini_tail_repair_missing_not_closed")
        );
        assert!(report["fail_reason"].is_null());
    }

    #[test]
    fn diagnostics_marks_last_live_child_sample_stale_after_completed() {
        let report = write_test_diagnostics_report(
            stale_tail_live_sample_for_test(),
            final_closed_sample_for_test(),
        );

        assert_eq!(report["last_live_child_sample_stale"].as_bool(), Some(true));
        assert_eq!(
            report["final_closed_child_sample_available"].as_bool(),
            Some(true)
        );
    }

    #[test]
    fn diagnostics_signoff_sample_source_is_final_closed_when_available() {
        let report = write_test_diagnostics_report(
            stale_tail_live_sample_for_test(),
            final_closed_sample_for_test(),
        );

        assert_eq!(
            report["diagnostics_signoff_sample_source"].as_str(),
            Some("final_closed_child_sample")
        );
        assert_eq!(
            report["diagnostics_signoff_sample"]["mini_completed_tx_count"].as_u64(),
            Some(480)
        );
    }

    #[test]
    fn aoem_owned_signable_not_overwritten_by_stale_live_sample() {
        let mut stale = stale_tail_live_sample_for_test();
        stale["aoem_owned_regression_signable"] = serde_json::json!(false);
        let report = write_test_diagnostics_report(stale, final_closed_sample_for_test());

        assert_eq!(
            report["diagnostics_final_sample_aoem_owned_regression_signable"].as_bool(),
            Some(true)
        );
        assert_eq!(
            report["diagnostics_signoff_sample"]["aoem_owned_regression_signable"].as_bool(),
            Some(true)
        );
    }

    #[test]
    fn mini_final_closed_sample_exports_tps_sync_pass() {
        let report = write_test_diagnostics_report(
            stale_tail_live_sample_for_test(),
            final_closed_sample_for_test(),
        );

        assert_eq!(report["mini_tps_sync_pass"].as_bool(), Some(true));
        assert_eq!(
            report["diagnostics_final_sample_mini_tps_sync_pass"].as_bool(),
            Some(true)
        );
        assert!(report["mini_tps_sync_fail_reason"].is_null());
    }

    #[test]
    fn mini_final_tps_uses_final_closed_counters_not_retained_view() {
        let report = write_test_diagnostics_report(
            stale_tail_live_sample_for_test(),
            final_closed_sample_for_test(),
        );

        assert_eq!(
            report["mini_tps_sync_sample_source"].as_str(),
            Some("final_closed_child_sample")
        );
        assert_eq!(
            report["final_closed_child_sample_uses_retained_view"].as_bool(),
            Some(true)
        );
        assert_eq!(report["final_run_close_tps_counter"].as_u64(), Some(480));
        assert_eq!(report["final_completed_tx_count"].as_u64(), Some(480));
    }

    #[test]
    fn mini_final_tps_does_not_zero_when_480_closed() {
        let report = write_test_diagnostics_report(
            stale_tail_live_sample_for_test(),
            final_closed_sample_for_test(),
        );

        assert!(report["final_run_close_tps_x1000"].as_u64().unwrap_or(0) > 0);
        assert!(
            report["mini_b_aoem_closed_tx_tps_x1000"]
                .as_u64()
                .unwrap_or(0)
                > 0
        );
        assert!(report["mini_b_ledger_tx_tps_x1000"].as_u64().unwrap_or(0) > 0);
    }

    #[test]
    fn mini_final_tps_marks_retained_view_not_comparable() {
        let report = write_test_diagnostics_report(
            stale_tail_live_sample_for_test(),
            final_closed_sample_for_test(),
        );

        assert_eq!(
            report["mini_tps_sync_comparable_network_source"].as_str(),
            Some("final_closed_counters")
        );
        assert_eq!(
            report["final_closed_child_sample_counter_source"].as_str(),
            Some("mini_completed_canonical_ledger_proof_counters")
        );
        assert_eq!(
            report["mini_tps_sync_live_counter_source"].as_str(),
            Some("live_delta_counters_diagnostic_only")
        );
    }

    #[test]
    fn mini_final_tps_fails_with_sample_source_invalid_when_no_valid_counter() {
        let mut final_sample = final_closed_sample_for_test();
        final_sample["mini_completed_tx_count"] = serde_json::json!(0);
        final_sample["canonical_unique_included_total"] = serde_json::json!(0);
        final_sample["receiver_ledger_close_count"] = serde_json::json!(0);
        let report = write_test_diagnostics_report(stale_tail_live_sample_for_test(), final_sample);

        assert_eq!(report["mini_tps_sync_pass"].as_bool(), Some(false));
        assert_eq!(
            report["mini_tps_sync_fail_reason"].as_str(),
            Some("mini_tps_sample_source_invalid")
        );
    }

    #[test]
    fn mini_final_tps_can_fail_when_real_run_tps_below_sender() {
        let mut stale = stale_tail_live_sample_for_test();
        stale["elapsed_ms"] = serde_json::json!(1_000);
        stale["mini_completed_tx_count"] = serde_json::json!(1);
        let mut final_sample = final_closed_sample_for_test();
        final_sample["elapsed_ms"] = serde_json::json!(121_000);
        let report = write_test_diagnostics_report(stale, final_sample);

        assert_eq!(report["mini_tps_sync_pass"].as_bool(), Some(false));
        assert!(report["final_run_tps_sync_fail_reasons"]
            .as_array()
            .unwrap()
            .iter()
            .any(|reason| reason.as_str() == Some("final_run_close_tps_below_sender")));
    }

    #[test]
    fn mini_final_tps_passes_when_480_closed_and_run_tps_above_threshold() {
        let report = write_test_diagnostics_report(
            stale_tail_live_sample_for_test(),
            final_closed_sample_for_test(),
        );

        assert_eq!(report["final_run_tps_sync_pass"].as_bool(), Some(true));
        assert_eq!(report["mini_tps_sync_pass"].as_bool(), Some(true));
        assert!(report["final_run_close_tps_x1000"].as_u64().unwrap_or(0) >= 8_000);
    }

    fn pipeline_14400_live_sample_for_test() -> Value {
        serde_json::json!({
            "elapsed_ms": 350_000,
            "process_working_set_bytes": 1,
            "process_private_bytes": 1,
            "received_unique_total": 2_000,
            "canonical_unique_included_total": 2_000,
            "receiver_ledger_close_count": 2_000,
            "aoem_executed_total": 2_000,
            "network_receiver_object_ready_count": 2_000,
            "object_assembler_batch_ready_count": 16,
            "aoem_runtime_worker_batch_received_count": 16,
            "aoem_runtime_worker_tx_ingress_call_count": 16,
            "aoem_runtime_worker_result_ready_count": 16,
            "finality_report_worker_result_verified_count": 16
        })
    }

    fn pipeline_14400_final_sample_for_test(elapsed_ms: u64) -> Value {
        serde_json::json!({
            "elapsed_ms": elapsed_ms,
            "final_closed_child_sample": true,
            "final_closed_child_sample_available": true,
            "receiver_exit_phase": "completed",
            "received_unique_total": 14_400,
            "canonical_unique_included_total": 14_400,
            "receiver_ledger_close_count": 14_400,
            "aoem_executed_total": 14_400,
            "aoem_native_tx_batch_production_receipt_count": 14_400,
            "aoem_native_tx_batch_production_canonical_proof_count": 14_400,
            "aoem_native_tx_batch_production_ledger_close_proof_count": 14_400,
            "network_receiver_object_ready_count": 14_400,
            "object_assembler_batch_ready_count": 113,
            "aoem_runtime_worker_batch_received_count": 113,
            "aoem_runtime_worker_tx_ingress_call_count": 113,
            "aoem_runtime_worker_result_ready_count": 113,
            "finality_report_worker_result_verified_count": 113,
            "receiver_pipeline_backpressure_reason": "none",
            "aoem_runtime_worker_scheduler": "ready_queue_active_drain",
            "aoem_runtime_worker_active_sleep_ms": 0,
            "aoem_runtime_worker_idle_sleep_ms": 10
        })
    }

    #[test]
    fn performance_report_breaks_down_sender_receiver_tail_windows() {
        let report = write_test_diagnostics_report_for_tx_count(
            pipeline_14400_live_sample_for_test(),
            pipeline_14400_final_sample_for_test(1_936_000),
            14_400,
        );

        assert_eq!(
            report["receiver_total_elapsed_ms"].as_u64(),
            Some(1_936_000)
        );
        assert_eq!(report["receiver_first_tx_seen_ms"].as_u64(), Some(350_000));
        assert_eq!(report["receiver_first_close_ms"].as_u64(), Some(350_000));
        assert_eq!(report["receiver_last_close_ms"].as_u64(), Some(1_936_000));
        assert_eq!(
            report["receiver_active_close_window_ms"].as_u64(),
            Some(1_586_000)
        );
        assert_eq!(report["finalization_tail_ms"].as_u64(), Some(0));
        assert_eq!(
            report["performance_wall_clock_breakdown"]["receiver_total_elapsed_ms"].as_u64(),
            Some(1_936_000)
        );
    }

    #[test]
    fn pipeline_reports_active_close_tps_separately_from_total_elapsed() {
        let report = write_test_diagnostics_report_for_tx_count(
            pipeline_14400_live_sample_for_test(),
            pipeline_14400_final_sample_for_test(1_936_000),
            14_400,
        );

        assert_eq!(
            report["strict_30min_wall_clock_elapsed_ms"].as_u64(),
            Some(1_936_000)
        );
        assert_eq!(
            report["strict_30min_wall_clock_gap_ms"].as_u64(),
            Some(136_000)
        );
        assert_eq!(
            report["performance_window_start_source"].as_str(),
            Some("first_tx_seen")
        );
        assert_eq!(
            report["performance_window_end_source"].as_str(),
            Some("final_close")
        );
        assert_eq!(
            report["performance_window_elapsed_ms"].as_u64(),
            Some(1_586_000)
        );
        assert_eq!(report["active_close_tx_count"].as_u64(), Some(14_400));
        assert!(report["active_close_tps_x1000"].as_u64().unwrap_or(0) > 9_000);
        assert_eq!(
            report["receiver_active_close_counter_delta"].as_u64(),
            Some(12_400)
        );
        assert!(
            report["receiver_active_close_tps_x1000"]
                .as_u64()
                .unwrap_or(0)
                > 7_000
        );
        assert!(
            report["receiver_total_close_tps_x1000"]
                .as_u64()
                .unwrap_or(0)
                < 8_000
        );
    }

    #[test]
    fn pipeline_reports_tail_finalization_ms() {
        let close_sample = pipeline_14400_final_sample_for_test(1_800_000);
        let report = write_test_diagnostics_report_samples_for_tx_count(
            vec![
                pipeline_14400_live_sample_for_test(),
                close_sample,
                pipeline_14400_final_sample_for_test(1_936_000),
            ],
            14_400,
        );

        assert_eq!(report["receiver_last_close_ms"].as_u64(), Some(1_800_000));
        assert_eq!(report["finalization_tail_ms"].as_u64(), Some(136_000));
    }

    #[test]
    fn pipeline_reports_worker_batch_size_and_inflight() {
        let report = write_test_diagnostics_report_for_tx_count(
            pipeline_14400_live_sample_for_test(),
            pipeline_14400_final_sample_for_test(1_936_000),
            14_400,
        );

        assert_eq!(
            report["aoem_runtime_worker_tx_ingress_call_count"].as_u64(),
            Some(113)
        );
        assert_eq!(
            report["aoem_runtime_worker_batch_size_avg"].as_u64(),
            Some(127)
        );
        assert_eq!(
            report["aoem_runtime_worker_inflight_batch_count"].as_u64(),
            Some(0)
        );
    }

    #[test]
    fn pipeline_reports_backpressure_reason_when_stage_lags() {
        let mut final_sample = pipeline_14400_final_sample_for_test(1_936_000);
        final_sample["object_assembler_batch_ready_count"] = serde_json::json!(100);
        final_sample["aoem_runtime_worker_batch_received_count"] = serde_json::json!(100);
        final_sample["aoem_runtime_worker_tx_ingress_call_count"] = serde_json::json!(100);
        final_sample["aoem_runtime_worker_result_ready_count"] = serde_json::json!(100);
        final_sample["finality_report_worker_result_verified_count"] = serde_json::json!(80);
        let report = write_test_diagnostics_report_for_tx_count(
            pipeline_14400_live_sample_for_test(),
            final_sample,
            14_400,
        );

        assert_eq!(
            report["pipeline_backpressure_reason"].as_str(),
            Some("finality_report_worker_lag")
        );
    }

    #[test]
    fn strict_30min_gate_fails_when_elapsed_exceeds_1800s() {
        let report = write_test_diagnostics_report_for_tx_count(
            pipeline_14400_live_sample_for_test(),
            pipeline_14400_final_sample_for_test(1_936_000),
            14_400,
        );

        assert_eq!(
            report["strict_30min_wall_clock_performance_pass"].as_bool(),
            Some(true)
        );
        assert_eq!(
            report["total_elapsed_exceeded_due_to_pre_first_tx_wait"].as_bool(),
            Some(true)
        );
        assert_eq!(
            report["strict_30min_wall_clock_fail_reasons"]
                .as_array()
                .unwrap()
                .len(),
            0
        );
    }

    #[test]
    fn strict_30min_gate_fails_when_active_window_exceeds_1800s() {
        let report = write_test_diagnostics_report_for_tx_count(
            pipeline_14400_live_sample_for_test(),
            pipeline_14400_final_sample_for_test(2_200_000),
            14_400,
        );

        assert_eq!(
            report["strict_30min_wall_clock_performance_pass"].as_bool(),
            Some(false)
        );
        assert!(report["strict_30min_wall_clock_fail_reasons"]
            .as_array()
            .unwrap()
            .iter()
            .any(|reason| reason.as_str() == Some("active_performance_window_exceeded_30min")));
    }

    #[test]
    fn strict_30min_gate_can_pass_when_active_close_and_tail_within_budget() {
        let report = write_test_diagnostics_report_for_tx_count(
            pipeline_14400_live_sample_for_test(),
            pipeline_14400_final_sample_for_test(1_790_000),
            14_400,
        );

        assert_eq!(
            report["strict_30min_wall_clock_performance_pass"].as_bool(),
            Some(true)
        );
        assert_eq!(
            report["strict_30min_wall_clock_fail_reasons"]
                .as_array()
                .unwrap()
                .len(),
            0
        );
    }

    #[test]
    fn performance_window_excludes_pre_first_tx_wait() {
        let report = write_test_diagnostics_report_for_tx_count(
            pipeline_14400_live_sample_for_test(),
            pipeline_14400_final_sample_for_test(1_936_000),
            14_400,
        );

        assert_eq!(report["pre_first_tx_wait_ms"].as_u64(), Some(350_000));
        assert_eq!(
            report["total_elapsed_exceeded_due_to_pre_first_tx_wait"].as_bool(),
            Some(true)
        );
        assert_eq!(
            report["strict_30min_performance_gate_window"].as_str(),
            Some("first_tx_seen_to_final_close")
        );
    }

    #[test]
    fn active_close_tps_uses_active_close_tx_count() {
        let report = write_test_diagnostics_report_for_tx_count(
            pipeline_14400_live_sample_for_test(),
            pipeline_14400_final_sample_for_test(1_936_000),
            14_400,
        );

        assert_eq!(report["active_close_tx_count"].as_u64(), Some(14_400));
        assert_eq!(report["active_close_window_ms"].as_u64(), Some(1_586_000));
        assert_eq!(
            report["active_close_tps_x1000"].as_u64(),
            Some(14_400u64.saturating_mul(1_000_000) / 1_586_000)
        );
        assert_eq!(report["total_close_tx_count"].as_u64(), Some(14_400));
    }

    #[test]
    fn mini_receiver_does_not_accept_legacy_close_without_aoem_owned_single_path() {
        with_env_var(
            NOV_NATIVE_AOEM_NATIVE_TX_BATCH_PRODUCTION_CANDIDATE_ENV,
            Some("1"),
            || {
                let summary = serde_json::json!({
                    "accepted": true,
                    "ingress_total_last": 480,
                    "included_canonical_total": 480,
                    "aoem_executed_total": 480,
                    "ledger_completed_count": 480,
                    "ledger_expected_count": 480,
                    "child_expected_total_from_config": 480,
                    "queue_pending_last": 0,
                });
                let sample = diagnostics_summary_sample(
                    Instant::now(),
                    &summary,
                    serde_json::json!({}),
                    serde_json::json!({}),
                    serde_json::json!({}),
                    480,
                );

                assert_eq!(sample["accepted"].as_bool(), Some(false));
                assert_eq!(
                    sample["fail_reason"].as_str(),
                    Some("aoem_owned_single_path_diagnostics_missing_under_gate")
                );
            },
        );
    }

    #[test]
    fn receiver_child_initializes_ledger_expected_range() {
        let summary = serde_json::json!({
            "included_canonical_total": 0,
            "aoem_executed_total": 0,
            "queue_pending_last": 0,
            "ledger_expected_range_start": 0,
            "ledger_expected_range_end": 14399,
            "ledger_expected_count": 14400,
            "ledger_durable_missing_count": 14400,
            "ledger_durable_missing_derived_from_expected_range": true,
            "child_env_tx_count_raw": "14400",
            "child_expected_total_from_env": 14400,
            "child_expected_total_from_config": 14400,
            "child_ledger_expected_range_init_called": true,
            "child_ledger_expected_range_init_source": "child_expected_total_env",
            "child_ledger_expected_range_init_error": null,
            "child_progress_summary_source": "child_runtime",
        });
        let sample = diagnostics_summary_sample(
            Instant::now(),
            &summary,
            serde_json::json!({}),
            serde_json::json!({}),
            serde_json::json!({}),
            0,
        );

        assert_eq!(sample["ledger_expected_range_start"].as_u64(), Some(0));
        assert_eq!(sample["ledger_expected_range_end"].as_u64(), Some(14_399));
        assert_eq!(sample["ledger_expected_count"].as_u64(), Some(14_400));
        assert_eq!(
            sample["ledger_durable_missing_derived_from_expected_range"].as_bool(),
            Some(true)
        );
        assert_eq!(
            sample["child_ledger_expected_range_init_called"].as_bool(),
            Some(true)
        );
    }

    #[test]
    fn novorudp_receiver_fails_fast_when_expected_total_missing() {
        assert!(novorudp_receiver_expected_total_missing_v1(
            "novorudp", "receiver", 0
        ));
        assert!(!novorudp_receiver_expected_total_missing_v1(
            "novorudp", "sender", 0
        ));
        assert!(!novorudp_receiver_expected_total_missing_v1(
            "novorudp", "receiver", 14_400
        ));
    }

    #[test]
    fn child_progress_summary_reports_child_ledger_expected_count() {
        let summary = serde_json::json!({
            "included_canonical_total": 128,
            "aoem_executed_total": 128,
            "queue_pending_last": 0,
            "ledger_expected_count": 2400,
            "child_env_tx_count_raw": "2400",
            "child_expected_total_from_env": 2400,
            "child_expected_total_from_config": 2400,
            "child_progress_summary_source": "child_runtime",
        });
        let sample = diagnostics_summary_sample(
            Instant::now(),
            &summary,
            serde_json::json!({}),
            serde_json::json!({}),
            serde_json::json!({}),
            0,
        );

        assert_eq!(sample["ledger_expected_count"].as_u64(), Some(2400));
        assert_eq!(sample["child_expected_total_from_env"].as_u64(), Some(2400));
        assert_eq!(
            sample["child_progress_summary_source"].as_str(),
            Some("child_runtime")
        );
    }

    #[test]
    fn final_missing_requires_nonzero_expected_ledger() {
        assert!(final_missing_without_expected_ledger_v1(248, 0));
        assert!(!final_missing_without_expected_ledger_v1(0, 0));
        assert!(!final_missing_without_expected_ledger_v1(248, 14_400));
    }

    #[test]
    fn receiver_timeout_uses_phased_completion_not_send_duration_cutoff() {
        let config = receiver_phase_test_config();
        assert_eq!(
            receiver_completion_phase_v1(&config, 1_799_999, 14_112, 14_400, 0),
            "primary_send"
        );
        assert_eq!(
            receiver_completion_phase_v1(&config, 1_800_000, 14_112, 14_400, 0),
            "repair_convergence"
        );
        assert_eq!(
            receiver_completion_phase_v1(&config, 1_900_000, 14_400, 14_400, 4),
            "receiver_drain"
        );
        assert_eq!(
            receiver_completion_phase_v1(&config, 1_900_000, 14_400, 14_400, 0),
            "completed"
        );
    }

    fn sender_timeout_sustained_config(tx_count: u64) -> SustainedConfigV1 {
        SustainedConfigV1 {
            enabled: false,
            duration_seconds: 0,
            tx_per_round: tx_count,
            round_interval_ms: 0,
        }
    }

    fn ledger_receipt_progress_summary_for_test() -> Value {
        serde_json::json!({
            "included_canonical_total": 14112,
            "aoem_executed_total": 14112,
            "queue_pending_last": 0,
            "ledger_final_missing_admitted_count": 1853,
            "ledger_final_missing_candidate_count": 304,
            "ledger_final_missing_candidate_payload_available_count": 0,
            "ledger_final_missing_candidate_payload_missing_count": 304,
            "ledger_final_missing_candidate_tx_hash_mapping_missing_count": 0,
            "ledger_final_missing_candidate_raw_tx_build_error_count": 0,
            "ledger_final_missing_batch_blocked_by_payload_missing_count": 304,
            "ledger_final_missing_batch_blocked_by_stage_filter_count": 0,
            "ledger_final_missing_batch_blocked_by_scan_limit_count": 0,
            "ledger_final_missing_batch_blocked_by_batch_limit_count": 0,
            "ledger_final_missing_batch_blocked_by_raw_tx_build_error_count": 0,
            "ledger_final_missing_batch_blocked_by_tx_hash_mapping_missing_count": 0,
            "ledger_final_missing_batch_blocked_reason": "payload_missing",
            "ledger_final_missing_actual_batch_count": 288,
            "ledger_final_missing_actual_batch_ranges_sample": [
                {"start": 14112, "end_inclusive": 14399, "count": 288}
            ],
            "ledger_final_missing_raw_txs_count": 288,
            "ledger_final_missing_batch_result_count": 288,
            "ledger_final_missing_receipt_written_count": 288,
            "ledger_final_missing_receipt_missing_after_admission_count": 0,
            "ledger_final_missing_inflight_count": 0,
            "ledger_final_missing_retryable_count": 0,
            "ledger_final_missing_requeued_after_no_receipt_count": 0,
            "ledger_final_missing_admitted_but_no_receipt_invariant_violation_count": 0,
            "ledger_admission_counter_is_actual_batch": true,
        })
    }

    #[test]
    fn diagnostics_sample_preserves_ledger_receipt_completion_fields() {
        let summary = ledger_receipt_progress_summary_for_test();
        let sample = diagnostics_summary_sample(
            Instant::now(),
            &summary,
            serde_json::json!({"line_count": 14112, "bytes": 1}),
            serde_json::json!({}),
            serde_json::json!({}),
            14080,
        );

        assert_eq!(
            sample["ledger_final_missing_actual_batch_count"].as_u64(),
            Some(288)
        );
        assert_eq!(
            sample["ledger_final_missing_receipt_written_count"].as_u64(),
            Some(288)
        );
        assert_eq!(
            sample["ledger_final_missing_receipt_missing_after_admission_count"].as_u64(),
            Some(0)
        );
        assert_eq!(
            sample["ledger_final_missing_candidate_payload_missing_count"].as_u64(),
            Some(304)
        );
        assert_eq!(
            sample["ledger_final_missing_batch_blocked_reason"].as_str(),
            Some("payload_missing")
        );
        assert_eq!(
            sample["ledger_receipt_completion_attribution_available"].as_bool(),
            Some(true)
        );
        assert_eq!(
            sample["ledger_admission_counter_is_actual_batch"].as_bool(),
            Some(true)
        );
    }

    #[test]
    fn timeout_synthetic_report_preserves_ledger_receipt_completion_fields() {
        let source = ledger_receipt_progress_summary_for_test();
        let mut synthetic_validation = serde_json::json!({});
        apply_ledger_receipt_completion_fields_v1(&mut synthetic_validation, Some(&source));

        assert_eq!(
            synthetic_validation["ledger_final_missing_actual_batch_count"].as_u64(),
            Some(288)
        );
        assert_eq!(
            synthetic_validation["ledger_final_missing_receipt_written_count"].as_u64(),
            Some(288)
        );
        assert_eq!(
            synthetic_validation
                ["ledger_final_missing_admitted_but_no_receipt_invariant_violation_count"]
                .as_u64(),
            Some(0)
        );
        assert_eq!(
            synthetic_validation["ledger_final_missing_batch_blocked_reason"].as_str(),
            Some("payload_missing")
        );
        assert_eq!(
            synthetic_validation["ledger_receipt_completion_attribution_available"].as_bool(),
            Some(true)
        );
        assert!(
            synthetic_validation["ledger_receipt_completion_attribution_missing_reason"].is_null()
        );
    }

    #[test]
    fn wrapper_falls_back_to_progress_summary_for_ledger_receipt_fields() {
        let source = ledger_receipt_progress_summary_for_test();
        let last_sample_without_new_fields = serde_json::json!({
            "repair_packet_received_count": 10224,
            "ledger_final_missing_admitted_count": 1853,
        });
        let repair_source = if last_sample_without_new_fields
            .get("ledger_final_missing_actual_batch_count")
            .is_some()
        {
            Some(&last_sample_without_new_fields)
        } else {
            Some(&source)
        };
        let mut report = serde_json::json!({});
        apply_ledger_receipt_completion_fields_v1(&mut report, repair_source);

        assert_eq!(
            report["ledger_final_missing_actual_batch_count"].as_u64(),
            Some(288)
        );
        assert_eq!(
            report["ledger_final_missing_receipt_written_count"].as_u64(),
            Some(288)
        );
        assert_eq!(
            report["ledger_receipt_completion_attribution_available"].as_bool(),
            Some(true)
        );
        assert_eq!(
            report["ledger_final_missing_batch_blocked_reason"].as_str(),
            Some("payload_missing")
        );
    }

    #[test]
    fn timeout_synthetic_report_preserves_final_missing_blocked_reason() {
        let source = ledger_receipt_progress_summary_for_test();
        let mut synthetic_validation = serde_json::json!({});
        apply_ledger_receipt_completion_fields_v1(&mut synthetic_validation, Some(&source));

        assert_eq!(
            synthetic_validation["ledger_final_missing_candidate_payload_missing_count"].as_u64(),
            Some(304)
        );
        assert_eq!(
            synthetic_validation["ledger_final_missing_batch_blocked_by_payload_missing_count"]
                .as_u64(),
            Some(304)
        );
        assert_eq!(
            synthetic_validation["ledger_final_missing_batch_blocked_reason"].as_str(),
            Some("payload_missing")
        );
    }

    #[test]
    fn wrapper_final_report_preserves_final_missing_blocked_reason() {
        let source = ledger_receipt_progress_summary_for_test();
        let mut report = serde_json::json!({});
        apply_ledger_receipt_completion_fields_v1(&mut report, Some(&source));

        assert_eq!(
            report["ledger_final_missing_candidate_payload_missing_count"].as_u64(),
            Some(304)
        );
        assert_eq!(
            report["ledger_final_missing_batch_blocked_by_payload_missing_count"].as_u64(),
            Some(304)
        );
        assert_eq!(
            report["ledger_final_missing_batch_blocked_reason"].as_str(),
            Some("payload_missing")
        );
    }

    #[test]
    fn timeout_synthetic_final_preserves_runtime_blocked_reason() {
        let source = serde_json::json!({
            "ledger_final_missing_candidate_count": 304,
            "ledger_final_missing_actual_batch_count": 0,
            "ledger_final_missing_receipt_written_count": 0,
            "ledger_final_missing_receipt_missing_after_admission_count": 0,
            "ledger_final_missing_batch_blocked_reason": "",
        });
        let mut synthetic_validation = serde_json::json!({});
        apply_ledger_receipt_completion_fields_v1(&mut synthetic_validation, Some(&source));

        assert_eq!(
            synthetic_validation["ledger_final_missing_batch_blocked_reason"].as_str(),
            Some("classification_path_not_reached")
        );
        assert_eq!(
            synthetic_validation
                ["ledger_final_missing_batch_blocked_by_classification_path_not_reached_count"]
                .as_u64(),
            Some(304)
        );
    }

    #[test]
    fn receiver_ingress_drain_attribution_preserves_layered_counters() {
        let summary = serde_json::json!({
            "network_received_total": 2520,
            "ingress_submitted_total": 696,
            "product_ingress_submitted_total": 696,
            "ingress_total_last": 696,
            "queue_pending_last": 664,
            "queue_active_pending_last": 664,
            "queue_rejected_last": 0,
            "ingress_error_ticks": 0,
            "ticks": 220,
            "nonempty_aoem_batch_ticks": 118,
            "queue_admitted_total": 3768,
            "max_network_received_per_tick": 8,
            "max_queue_admitted_per_tick": 32,
            "included_canonical_total": 32,
            "aoem_executed_total": 3768,
            "ledger_completed_count": 3768,
            "ledger_receipt_proof_close_success_count": 3768,
        });
        let sample = diagnostics_summary_sample(
            Instant::now(),
            &summary,
            serde_json::json!({"line_count": 3768, "bytes": 1}),
            serde_json::json!({}),
            serde_json::json!({}),
            32,
        );

        assert_eq!(
            sample["receiver_udp_packet_recv_count"].as_u64(),
            Some(2520)
        );
        assert_eq!(
            sample["receiver_udp_packet_decode_ok_count"].as_u64(),
            Some(696)
        );
        assert_eq!(sample["receiver_pending_active_count"].as_u64(), Some(664));
        assert_eq!(
            sample["receiver_pending_selected_count"].as_u64(),
            Some(3768)
        );
        assert_eq!(
            sample["receiver_aoem_batch_result_count"].as_u64(),
            Some(3768)
        );
        assert_eq!(
            sample["receiver_canonical_included_count"].as_u64(),
            Some(32)
        );
        assert_eq!(
            sample["receiver_drain_attribution_stage"].as_str(),
            Some("receipt_canonical_projection_or_summary_lag")
        );
        assert_eq!(
            sample["summary_consistency_violation_count"].as_u64(),
            Some(2)
        );
        assert_eq!(
            sample["summary_source_ledger"].as_str(),
            Some("child_progress.ledger_completed_count")
        );
    }

    #[test]
    fn receiver_drain_delta_flags_receipt_canonical_projection_stall() {
        let mut previous = serde_json::json!({
            "receiver_udp_packet_recv_count": 100,
            "received_unique_total": 32,
            "aoem_executed_total": 32,
            "canonical_unique_included_total": 32,
            "receiver_ledger_close_count": 32,
            "receiver_child_tick_count": 10,
            "receiver_aoem_tick_count": 1,
            "receiver_pending_selected_count": 32,
            "queue_pending_last": 32,
        });
        annotate_receiver_ingress_drain_delta_v1(&mut previous, None);

        let mut sample = serde_json::json!({
            "receiver_udp_packet_recv_count": 140,
            "received_unique_total": 40,
            "aoem_executed_total": 64,
            "canonical_unique_included_total": 32,
            "receiver_ledger_close_count": 64,
            "receiver_child_tick_count": 11,
            "receiver_aoem_tick_count": 2,
            "receiver_pending_selected_count": 64,
            "queue_pending_last": 64,
        });
        annotate_receiver_ingress_drain_delta_v1(&mut sample, Some(&previous));

        assert_eq!(
            sample["receiver_aoem_executed_delta_raw"].as_u64(),
            Some(32)
        );
        assert_eq!(sample["receiver_canonical_delta_raw"].as_u64(), Some(0));
        assert_eq!(
            sample["receiver_receipt_canonical_projection_stall"].as_bool(),
            Some(true)
        );
        assert_eq!(
            sample["receiver_drain_stall_reason"].as_str(),
            Some("receipt_canonical_projection_stall")
        );
    }

    #[test]
    fn receiver_projection_proof_total_prevents_retained_canonical_false_stall() {
        let summary = serde_json::json!({
            "network_received_total": 3744,
            "ingress_submitted_total": 616,
            "ingress_total_last": 616,
            "queue_pending_last": 584,
            "queue_active_pending_last": 584,
            "ticks": 538,
            "nonempty_aoem_batch_ticks": 107,
            "queue_admitted_total": 3104,
            "included_canonical_retained_last": 32,
            "included_canonical_projected_total": 3104,
            "canonical_projection_success_ticks": 107,
            "included_canonical_total_source": "ledger_canonical_proof_close_success_count",
            "included_canonical_total": 3104,
            "aoem_executed_total": 3104,
            "ledger_completed_count": 3104,
            "ledger_missing_closed_by_receipt_count": 3104,
            "ledger_missing_closed_by_canonical_count": 3104,
            "ledger_receipt_proof_close_success_count": 3104,
            "ledger_canonical_proof_close_success_count": 3104,
        });
        let mut sample = diagnostics_summary_sample(
            Instant::now(),
            &summary,
            serde_json::json!({"line_count": 3104, "bytes": 1}),
            serde_json::json!({}),
            serde_json::json!({}),
            3072,
        );
        let previous = serde_json::json!({
            "receiver_udp_packet_recv_count": 3704,
            "received_unique_total": 608,
            "aoem_executed_total": 3072,
            "canonical_unique_included_total": 3072,
            "receiver_ledger_close_count": 3072,
            "receiver_child_tick_count": 537,
            "receiver_aoem_tick_count": 106,
            "receiver_pending_selected_count": 3072,
            "queue_pending_last": 576,
        });
        annotate_receiver_ingress_drain_delta_v1(&mut sample, Some(&previous));

        assert_eq!(
            sample["receiver_canonical_included_count"].as_u64(),
            Some(3104)
        );
        assert_eq!(
            sample["receiver_canonical_retained_count"].as_u64(),
            Some(32)
        );
        assert_eq!(
            sample["receiver_summary_canonical_source"].as_str(),
            Some("ledger_canonical_proof_close_success_count")
        );
        assert_eq!(
            sample["receiver_canonical_projection_stall_reason"].as_str(),
            Some("queue_retained_summary_lags_projection_proof")
        );
        assert_eq!(
            sample["summary_consistency_violation_count"].as_u64(),
            Some(0)
        );
        assert_eq!(
            sample["receiver_receipt_canonical_projection_stall"].as_bool(),
            Some(false)
        );
        assert_eq!(
            sample["receiver_drain_stall_reason"].as_str(),
            Some("progressing")
        );
    }

    #[test]
    fn receiver_repair_coverage_reports_overlap_with_durable_missing() {
        let summary = serde_json::json!({
            "included_canonical_total": 100,
            "aoem_executed_total": 100,
            "ledger_completed_count": 100,
            "ledger_durable_missing_count": 100,
            "ledger_durable_missing_ranges_sample": [
                {"start": 100, "end_inclusive": 199, "count": 100}
            ],
            "repair_sequence_received_ranges_sample": [
                {"start": 150, "end_inclusive": 249, "count": 100}
            ],
            "repair_sequence_accepted_ranges_sample": [
                {"start": 160, "end_inclusive": 170, "count": 11}
            ],
            "repair_sequence_admitted_to_aoem_ranges_sample": [
                {"start": 180, "end_inclusive": 190, "count": 11}
            ],
            "repair_sequence_duplicate_ranges_sample": [
                {"start": 240, "end_inclusive": 249, "count": 10}
            ],
        });
        let sample = diagnostics_summary_sample(
            Instant::now(),
            &summary,
            serde_json::json!({}),
            serde_json::json!({}),
            serde_json::json!({}),
            100,
        );

        assert_eq!(
            sample["receiver_durable_missing_ranges_sequence_count"].as_u64(),
            Some(100)
        );
        assert_eq!(
            sample["receiver_repair_received_overlap_missing_count"].as_u64(),
            Some(50)
        );
        assert_eq!(
            sample["receiver_repair_accepted_overlap_missing_count"].as_u64(),
            Some(11)
        );
        assert_eq!(
            sample["receiver_repair_executed_overlap_missing_count"].as_u64(),
            Some(11)
        );
        assert_eq!(
            sample["receiver_repair_duplicate_ranges_count"].as_u64(),
            Some(10)
        );
        assert_eq!(
            sample["repair_duplicate_waste_ratio_bps"].as_u64(),
            Some(1000)
        );
    }

    #[test]
    fn receiver_delta_reports_repair_convergence_rate() {
        let mut previous = serde_json::json!({
            "elapsed_ms": 60_000,
            "receiver_udp_packet_recv_count": 1_000,
            "received_unique_total": 500,
            "aoem_executed_total": 100,
            "canonical_unique_included_total": 100,
            "receiver_ledger_close_count": 100,
            "ledger_durable_missing_count": 200,
            "receiver_child_tick_count": 10,
            "receiver_aoem_tick_count": 3,
            "receiver_pending_selected_count": 100,
            "queue_pending_last": 0,
        });
        annotate_receiver_ingress_drain_delta_v1(&mut previous, None);

        let mut sample = serde_json::json!({
            "elapsed_ms": 120_000,
            "receiver_udp_packet_recv_count": 1_100,
            "received_unique_total": 550,
            "aoem_executed_total": 150,
            "canonical_unique_included_total": 150,
            "receiver_ledger_close_count": 150,
            "ledger_durable_missing_count": 150,
            "receiver_child_tick_count": 20,
            "receiver_aoem_tick_count": 5,
            "receiver_pending_selected_count": 150,
            "queue_pending_last": 0,
        });
        annotate_receiver_ingress_drain_delta_v1(&mut sample, Some(&previous));

        assert_eq!(sample["receiver_ledger_close_delta_raw"].as_u64(), Some(50));
        assert_eq!(sample["missing_count_delta_per_minute"].as_u64(), Some(50));
        assert_eq!(
            sample["repair_convergence_rate_tps_x1000"].as_u64(),
            Some(833)
        );
        assert_eq!(
            sample["repair_effective_completion_per_1000_packets"].as_u64(),
            Some(500)
        );
        assert_eq!(
            sample["receiver_durable_missing_delta_direction"].as_str(),
            Some("decrease")
        );
    }

    #[test]
    fn pipeline_reports_stage_liveness_counters() {
        let previous = pipeline_liveness_sample(10_000, 32, 10, 100, 4, 4, 4, 4, 4, 100);
        let mut sample = pipeline_liveness_sample(15_000, 64, 12, 132, 5, 5, 5, 5, 5, 132);
        annotate_receiver_ingress_drain_delta_v1(&mut sample, Some(&previous));

        assert_eq!(
            sample["network_receiver_object_ready_delta"].as_u64(),
            Some(32)
        );
        assert_eq!(
            sample["object_assembler_batch_ready_delta"].as_u64(),
            Some(1)
        );
        assert_eq!(
            sample["aoem_runtime_worker_batch_received_delta"].as_u64(),
            Some(1)
        );
        assert_eq!(
            sample["aoem_runtime_worker_tx_ingress_call_delta"].as_u64(),
            Some(1)
        );
        assert_eq!(
            sample["aoem_runtime_worker_result_ready_delta"].as_u64(),
            Some(1)
        );
        assert_eq!(
            sample["finality_report_worker_result_verified_delta"].as_u64(),
            Some(1)
        );
        assert_eq!(sample["queue_pending_delta"].as_i64(), Some(32));
        assert_eq!(
            sample["pipeline_pending_drain_stall"].as_bool(),
            Some(false)
        );
    }

    #[test]
    fn pipeline_detects_pending_drain_stall_when_pending_nonzero() {
        let previous =
            pipeline_liveness_sample(10_000, 1_920, 10, 29_400, 920, 920, 920, 920, 920, 29_400);
        let mut sample =
            pipeline_liveness_sample(15_000, 1_952, 10, 29_400, 920, 920, 920, 920, 920, 29_400);
        annotate_receiver_ingress_drain_delta_v1(&mut sample, Some(&previous));

        assert_eq!(sample["pipeline_pending_drain_stall"].as_bool(), Some(true));
        assert_eq!(
            sample["pipeline_pending_drain_stall_reason"].as_str(),
            Some("pending_drain_callsite_stall")
        );
        assert_eq!(
            sample["receiver_drain_stall_reason"].as_str(),
            Some("pending_drain_callsite_stall")
        );
        assert_eq!(sample["receiver_child_tick_stall_ms"].as_u64(), Some(5_000));
    }

    #[test]
    fn pipeline_reports_assembler_to_runtime_handoff_stall() {
        let previous = pipeline_liveness_sample(10_000, 64, 10, 100, 4, 4, 4, 4, 4, 100);
        let mut sample = pipeline_liveness_sample(15_000, 96, 11, 132, 5, 4, 4, 4, 4, 100);
        annotate_receiver_ingress_drain_delta_v1(&mut sample, Some(&previous));

        assert_eq!(
            sample["receiver_drain_stall_reason"].as_str(),
            Some("aoem_runtime_worker_input_stall")
        );
        assert_eq!(
            sample["aoem_runtime_worker_stall_reason"].as_str(),
            Some("batch_ready_not_received")
        );
    }

    #[test]
    fn pipeline_reports_runtime_worker_submit_stall() {
        let previous = pipeline_liveness_sample(10_000, 64, 10, 100, 4, 4, 4, 4, 4, 100);
        let mut sample = pipeline_liveness_sample(15_000, 96, 11, 132, 5, 5, 4, 4, 4, 100);
        annotate_receiver_ingress_drain_delta_v1(&mut sample, Some(&previous));

        assert_eq!(
            sample["receiver_drain_stall_reason"].as_str(),
            Some("aoem_runtime_worker_submit_stall")
        );
        assert_eq!(
            sample["aoem_runtime_worker_stall_reason"].as_str(),
            Some("batch_received_not_submitted")
        );
    }

    #[test]
    fn pipeline_reports_result_drain_stall() {
        let previous = pipeline_liveness_sample(10_000, 64, 10, 100, 4, 4, 4, 4, 4, 100);
        let mut sample = pipeline_liveness_sample(15_000, 96, 11, 132, 5, 5, 5, 4, 4, 100);
        annotate_receiver_ingress_drain_delta_v1(&mut sample, Some(&previous));

        assert_eq!(
            sample["receiver_drain_stall_reason"].as_str(),
            Some("aoem_runtime_worker_result_drain_stall")
        );
        assert_eq!(
            sample["aoem_runtime_worker_stall_reason"].as_str(),
            Some("tx_ingress_call_without_result")
        );
    }

    #[test]
    fn pipeline_reports_finality_worker_backpressure() {
        let previous = pipeline_liveness_sample(10_000, 64, 10, 100, 4, 4, 4, 4, 4, 100);
        let mut sample = pipeline_liveness_sample(15_000, 96, 11, 132, 5, 5, 5, 5, 4, 100);
        annotate_receiver_ingress_drain_delta_v1(&mut sample, Some(&previous));

        assert_eq!(
            sample["receiver_drain_stall_reason"].as_str(),
            Some("finality_report_worker_backpressure")
        );
        assert_eq!(
            sample["finality_report_worker_backpressure_reason"].as_str(),
            Some("result_ready_not_verified")
        );
    }

    #[test]
    fn pipeline_does_not_report_tail_repair_when_pending_nonzero() {
        let previous = pipeline_liveness_sample(10_000, 64, 10, 100, 4, 4, 4, 4, 4, 100);
        let mut sample = pipeline_liveness_sample(15_000, 96, 10, 100, 4, 4, 4, 4, 4, 100);
        annotate_receiver_ingress_drain_delta_v1(&mut sample, Some(&previous));

        assert_eq!(
            sample["receiver_drain_stall_reason"].as_str(),
            Some("pending_drain_callsite_stall")
        );
        assert_ne!(
            sample["receiver_drain_stall_reason"].as_str(),
            Some("waiting_for_sender")
        );
    }

    #[test]
    fn pipeline_pending_nonzero_disallows_waiting_for_sender() {
        let previous = pipeline_liveness_sample(10_000, 64, 10, 100, 4, 4, 4, 4, 4, 100);
        let mut sample = pipeline_liveness_sample(15_000, 96, 10, 100, 4, 4, 4, 4, 4, 100);
        sample["waiting_for_sender"] = serde_json::json!(true);
        annotate_receiver_ingress_drain_delta_v1(&mut sample, Some(&previous));

        assert_eq!(
            sample["pipeline_pending_drain_stall_reason"].as_str(),
            Some("pending_drain_callsite_stall")
        );
        assert_ne!(
            sample["receiver_drain_stall_reason"].as_str(),
            Some("waiting_for_sender")
        );
    }

    #[test]
    fn pipeline_reports_pending_drain_callsite_idle_while_pending() {
        let previous = pipeline_liveness_sample(10_000, 128, 10, 200, 8, 8, 8, 8, 8, 200);
        let mut sample = pipeline_liveness_sample(15_000, 128, 10, 200, 8, 8, 8, 8, 8, 200);
        annotate_receiver_ingress_drain_delta_v1(&mut sample, Some(&previous));

        assert_eq!(sample["receiver_child_tick_delta"].as_u64(), Some(0));
        assert_eq!(
            sample["receiver_child_tick_stall_reason"].as_str(),
            Some("pending_drain_callsite_stall")
        );
        assert_eq!(sample["pending_drain_attempt_delta"].as_u64(), Some(0));
        assert_eq!(sample["pending_drain_success_delta"].as_u64(), Some(0));
    }

    #[test]
    fn pipeline_fails_closed_when_pending_nonzero_and_drain_attempt_stops() {
        let previous =
            pipeline_liveness_sample(10_000, 2_640, 453, 2_672, 453, 453, 453, 453, 453, 27_936);
        let mut sample =
            pipeline_liveness_sample(16_819, 2_640, 453, 2_672, 453, 453, 453, 453, 453, 27_936);
        annotate_receiver_ingress_drain_delta_v1(&mut sample, Some(&previous));

        assert_eq!(
            sample["receiver_drain_stall_reason"].as_str(),
            Some("pending_drain_callsite_stall")
        );
        assert_eq!(sample["pipeline_pending_drain_stall"].as_bool(), Some(true));
        assert_eq!(sample["pending_drain_attempt_delta"].as_u64(), Some(0));
    }

    #[test]
    fn pipeline_drain_callsite_handoff_to_runtime_worker_smoke() {
        let previous = pipeline_liveness_sample(10_000, 128, 10, 200, 8, 8, 8, 8, 8, 200);
        let mut sample = pipeline_liveness_sample(15_000, 96, 11, 232, 9, 9, 9, 9, 9, 232);
        annotate_receiver_ingress_drain_delta_v1(&mut sample, Some(&previous));

        assert_eq!(
            sample["pipeline_pending_drain_stall_reason"].as_str(),
            Some("none")
        );
        assert_eq!(
            sample["receiver_drain_stall_reason"].as_str(),
            Some("progressing")
        );
        assert_eq!(sample["pending_drain_success_delta"].as_u64(), Some(32));
    }

    #[test]
    fn pipeline_pending_drain_recovers_after_idle_window() {
        let previous = pipeline_liveness_sample(10_000, 128, 10, 200, 8, 8, 8, 8, 8, 200);
        let mut sample = pipeline_liveness_sample(15_000, 96, 11, 232, 9, 9, 9, 9, 9, 232);
        annotate_receiver_ingress_drain_delta_v1(&mut sample, Some(&previous));

        assert_eq!(
            sample["pipeline_pending_drain_stall"].as_bool(),
            Some(false)
        );
        assert_eq!(
            sample["pipeline_pending_drain_stall_reason"].as_str(),
            Some("none")
        );
        assert_eq!(sample["receiver_child_tick_stall_ms"].as_u64(), Some(0));
    }

    #[test]
    fn mini_tps_sync_does_not_compare_udp_packet_tps_to_tx_tps() {
        with_env_var(
            "NOVOVM_NATIVE_PIPELINE_SUSTAINED_TX_PER_ROUND",
            Some("8"),
            || {
                with_env_var(
                    "NOVOVM_NATIVE_PIPELINE_SUSTAINED_ROUND_INTERVAL_MS",
                    Some("1000"),
                    || {
                        let previous = serde_json::json!({
                            "elapsed_ms": 10_000,
                            "receiver_udp_packet_recv_count": 80,
                            "received_unique_total": 80,
                            "aoem_executed_total": 80,
                            "canonical_unique_included_total": 80,
                            "receiver_ledger_close_count": 80,
                            "ledger_durable_missing_count": 400,
                            "receiver_child_tick_count": 10,
                            "receiver_aoem_tick_count": 10,
                            "receiver_pending_selected_count": 80,
                            "queue_pending_last": 0,
                            "mini_expected_tx_count": 480,
                        });
                        let mut sample = serde_json::json!({
                            "elapsed_ms": 20_000,
                            "receiver_udp_packet_recv_count": 1_080,
                            "received_unique_total": 160,
                            "aoem_executed_total": 160,
                            "canonical_unique_included_total": 160,
                            "receiver_ledger_close_count": 160,
                            "ledger_durable_missing_count": 320,
                            "receiver_child_tick_count": 20,
                            "receiver_aoem_tick_count": 20,
                            "receiver_pending_selected_count": 160,
                            "queue_pending_last": 0,
                            "mini_expected_tx_count": 480,
                        });
                        annotate_receiver_ingress_drain_delta_v1(&mut sample, Some(&previous));

                        assert_eq!(sample["mini_tps_sync_pass"].as_bool(), Some(true));
                        assert_eq!(sample["mini_a_send_tps_x1000"].as_u64(), Some(8000));
                        assert_eq!(sample["mini_b_udp_packet_tps_x1000"].as_u64(), Some(100000));
                        assert_eq!(
                            sample["mini_b_network_received_tps_x1000"].as_u64(),
                            Some(8000)
                        );
                        assert_eq!(sample["mini_b_ledger_tps_x1000"].as_u64(), Some(8000));
                        assert!(!sample["mini_tps_sync_fail_reasons"]
                            .as_array()
                            .unwrap()
                            .iter()
                            .any(|reason| reason.as_str() == Some("b_queue_admit_below_network")));
                    },
                );
            },
        );
    }

    #[test]
    fn mini_tps_sync_uses_transport_object_tps_as_network_comparable_source() {
        with_env_var(
            "NOVOVM_NATIVE_PIPELINE_SUSTAINED_TX_PER_ROUND",
            Some("8"),
            || {
                with_env_var(
                    "NOVOVM_NATIVE_PIPELINE_SUSTAINED_ROUND_INTERVAL_MS",
                    Some("1000"),
                    || {
                        let previous = serde_json::json!({
                            "elapsed_ms": 10_000,
                            "receiver_udp_packet_recv_count": 100,
                            "received_unique_total": 80,
                            "aoem_executed_total": 80,
                            "canonical_unique_included_total": 80,
                            "receiver_ledger_close_count": 80,
                            "ledger_durable_missing_count": 400,
                            "receiver_child_tick_count": 10,
                            "receiver_aoem_tick_count": 10,
                            "receiver_pending_selected_count": 80,
                            "queue_pending_last": 0,
                            "mini_expected_tx_count": 480,
                        });
                        let mut sample = serde_json::json!({
                            "elapsed_ms": 20_000,
                            "receiver_udp_packet_recv_count": 1_100,
                            "received_unique_total": 160,
                            "aoem_executed_total": 160,
                            "canonical_unique_included_total": 160,
                            "receiver_ledger_close_count": 160,
                            "ledger_durable_missing_count": 320,
                            "receiver_child_tick_count": 20,
                            "receiver_aoem_tick_count": 20,
                            "receiver_pending_selected_count": 160,
                            "queue_pending_last": 0,
                            "mini_expected_tx_count": 480,
                        });
                        annotate_receiver_ingress_drain_delta_v1(&mut sample, Some(&previous));

                        assert_eq!(
                            sample["mini_tps_sync_comparable_network_source"].as_str(),
                            Some("receiver_sequence_unique_delta")
                        );
                        assert_eq!(
                            sample["mini_b_transport_object_ready_tps_x1000"].as_u64(),
                            Some(8000)
                        );
                        assert_eq!(
                            sample["mini_b_network_received_tps_x1000"].as_u64(),
                            sample["mini_b_transport_object_ready_tps_x1000"].as_u64()
                        );
                    },
                );
            },
        );
    }

    #[test]
    fn mini_tps_sync_reports_packet_to_tx_ratio() {
        with_env_var(
            "NOVOVM_NATIVE_PIPELINE_SUSTAINED_TX_PER_ROUND",
            Some("8"),
            || {
                with_env_var(
                    "NOVOVM_NATIVE_PIPELINE_SUSTAINED_ROUND_INTERVAL_MS",
                    Some("1000"),
                    || {
                        let previous = serde_json::json!({
                            "elapsed_ms": 10_000,
                            "receiver_udp_packet_recv_count": 0,
                            "received_unique_total": 80,
                            "aoem_executed_total": 80,
                            "canonical_unique_included_total": 80,
                            "receiver_ledger_close_count": 80,
                            "ledger_durable_missing_count": 400,
                            "receiver_child_tick_count": 10,
                            "receiver_aoem_tick_count": 10,
                            "receiver_pending_selected_count": 80,
                            "queue_pending_last": 0,
                            "mini_expected_tx_count": 480,
                        });
                        let mut sample = serde_json::json!({
                            "elapsed_ms": 20_000,
                            "receiver_udp_packet_recv_count": 960,
                            "received_unique_total": 160,
                            "aoem_executed_total": 160,
                            "canonical_unique_included_total": 160,
                            "receiver_ledger_close_count": 160,
                            "ledger_durable_missing_count": 320,
                            "receiver_child_tick_count": 20,
                            "receiver_aoem_tick_count": 20,
                            "receiver_pending_selected_count": 160,
                            "queue_pending_last": 0,
                            "mini_expected_tx_count": 480,
                        });
                        annotate_receiver_ingress_drain_delta_v1(&mut sample, Some(&previous));

                        assert_eq!(
                            sample["mini_tps_sync_packet_to_tx_ratio"].as_u64(),
                            Some(12000)
                        );
                    },
                );
            },
        );
    }

    #[test]
    fn mini_tps_sync_reports_aoem_close_below_sender_when_tx_admit_ok() {
        with_env_var(
            "NOVOVM_NATIVE_PIPELINE_SUSTAINED_TX_PER_ROUND",
            Some("8"),
            || {
                with_env_var(
                    "NOVOVM_NATIVE_PIPELINE_SUSTAINED_ROUND_INTERVAL_MS",
                    Some("1000"),
                    || {
                        let previous = serde_json::json!({
                            "elapsed_ms": 10_000,
                            "receiver_udp_packet_recv_count": 80,
                            "received_unique_total": 80,
                            "aoem_executed_total": 50,
                            "canonical_unique_included_total": 50,
                            "receiver_ledger_close_count": 50,
                            "ledger_durable_missing_count": 430,
                            "receiver_child_tick_count": 10,
                            "receiver_aoem_tick_count": 10,
                            "receiver_pending_selected_count": 80,
                            "queue_pending_last": 30,
                            "mini_expected_tx_count": 480,
                        });
                        let mut sample = serde_json::json!({
                            "elapsed_ms": 20_000,
                            "receiver_udp_packet_recv_count": 160,
                            "received_unique_total": 160,
                            "aoem_executed_total": 100,
                            "canonical_unique_included_total": 100,
                            "receiver_ledger_close_count": 100,
                            "ledger_durable_missing_count": 380,
                            "receiver_child_tick_count": 20,
                            "receiver_aoem_tick_count": 20,
                            "receiver_pending_selected_count": 160,
                            "queue_pending_last": 60,
                            "mini_expected_tx_count": 480,
                        });
                        annotate_receiver_ingress_drain_delta_v1(&mut sample, Some(&previous));

                        assert_eq!(sample["mini_tps_sync_pass"].as_bool(), Some(false));
                        let reasons = sample["mini_tps_sync_fail_reasons"].as_array().unwrap();
                        assert!(reasons
                            .iter()
                            .any(|reason| reason.as_str() == Some("b_aoem_close_below_sender")));
                        assert!(reasons
                            .iter()
                            .any(|reason| reason.as_str() == Some("b_aoem_close_below_admitted")));
                    },
                );
            },
        );
    }

    #[test]
    fn mini_tps_sync_passes_when_final_480_closed_and_normalized_tps_ok() {
        with_env_var(
            "NOVOVM_NATIVE_PIPELINE_SUSTAINED_TX_PER_ROUND",
            Some("8"),
            || {
                with_env_var(
                    "NOVOVM_NATIVE_PIPELINE_SUSTAINED_ROUND_INTERVAL_MS",
                    Some("1000"),
                    || {
                        let previous = serde_json::json!({
                            "elapsed_ms": 50_000,
                            "receiver_udp_packet_recv_count": 3_000,
                            "received_unique_total": 400,
                            "aoem_executed_total": 400,
                            "canonical_unique_included_total": 400,
                            "receiver_ledger_close_count": 400,
                            "ledger_durable_missing_count": 80,
                            "receiver_child_tick_count": 50,
                            "receiver_aoem_tick_count": 50,
                            "receiver_pending_selected_count": 400,
                            "queue_pending_last": 0,
                            "mini_expected_tx_count": 480,
                        });
                        let mut sample = serde_json::json!({
                            "elapsed_ms": 60_000,
                            "receiver_udp_packet_recv_count": 4_200,
                            "received_unique_total": 480,
                            "aoem_executed_total": 480,
                            "canonical_unique_included_total": 480,
                            "receiver_ledger_close_count": 480,
                            "ledger_durable_missing_count": 0,
                            "receiver_child_tick_count": 60,
                            "receiver_aoem_tick_count": 60,
                            "receiver_pending_selected_count": 480,
                            "queue_pending_last": 0,
                            "mini_expected_tx_count": 480,
                        });
                        annotate_receiver_ingress_drain_delta_v1(&mut sample, Some(&previous));

                        assert_eq!(sample["mini_tps_sync_pass"].as_bool(), Some(true));
                        assert_eq!(sample["mini_b_ledger_tx_tps_x1000"].as_u64(), Some(8000));
                    },
                );
            },
        );
    }

    #[test]
    fn mini_tps_sync_fails_when_aoem_close_tps_below_sender_after_normalization() {
        with_env_var(
            "NOVOVM_NATIVE_PIPELINE_SUSTAINED_TX_PER_ROUND",
            Some("8"),
            || {
                with_env_var(
                    "NOVOVM_NATIVE_PIPELINE_SUSTAINED_ROUND_INTERVAL_MS",
                    Some("1000"),
                    || {
                        let previous = serde_json::json!({
                            "elapsed_ms": 10_000,
                            "receiver_udp_packet_recv_count": 2_000,
                            "received_unique_total": 80,
                            "aoem_executed_total": 58,
                            "canonical_unique_included_total": 58,
                            "receiver_ledger_close_count": 58,
                            "ledger_durable_missing_count": 422,
                            "receiver_child_tick_count": 10,
                            "receiver_aoem_tick_count": 10,
                            "receiver_pending_selected_count": 80,
                            "queue_pending_last": 22,
                            "mini_expected_tx_count": 480,
                        });
                        let mut sample = serde_json::json!({
                            "elapsed_ms": 20_000,
                            "receiver_udp_packet_recv_count": 3_200,
                            "received_unique_total": 160,
                            "aoem_executed_total": 116,
                            "canonical_unique_included_total": 116,
                            "receiver_ledger_close_count": 116,
                            "ledger_durable_missing_count": 364,
                            "receiver_child_tick_count": 20,
                            "receiver_aoem_tick_count": 20,
                            "receiver_pending_selected_count": 160,
                            "queue_pending_last": 44,
                            "mini_expected_tx_count": 480,
                        });
                        annotate_receiver_ingress_drain_delta_v1(&mut sample, Some(&previous));

                        assert_eq!(sample["mini_tps_sync_pass"].as_bool(), Some(false));
                        assert_eq!(
                            sample["mini_b_transport_object_ready_tps_x1000"].as_u64(),
                            Some(8000)
                        );
                        assert_eq!(
                            sample["mini_b_aoem_closed_tx_tps_x1000"].as_u64(),
                            Some(5800)
                        );
                        assert!(sample["mini_tps_sync_fail_reasons"]
                            .as_array()
                            .unwrap()
                            .iter()
                            .any(|reason| reason.as_str() == Some("b_aoem_close_below_sender")));
                    },
                );
            },
        );
    }

    #[test]
    fn sender_repair_coverage_computes_ack_overlap_and_duplicate_waste() {
        let latest_ack_missing = vec![
            MissingRangeV1 {
                start: 100,
                end_inclusive: 199,
            },
            MissingRangeV1 {
                start: 300,
                end_inclusive: 399,
            },
        ];
        let repair_sent = vec![
            MissingRangeV1 {
                start: 100,
                end_inclusive: 149,
            },
            MissingRangeV1 {
                start: 100,
                end_inclusive: 149,
            },
            MissingRangeV1 {
                start: 500,
                end_inclusive: 549,
            },
        ];
        let report = novorudp_sender_repair_coverage_report_v1(
            latest_ack_missing.as_slice(),
            repair_sent.as_slice(),
            1_000,
            150,
            8,
        );

        assert_eq!(
            report["sender_latest_ack_missing_sequence_count"].as_u64(),
            Some(200)
        );
        assert_eq!(
            report["sender_repair_sent_unique_sequence_count"].as_u64(),
            Some(100)
        );
        assert_eq!(
            report["sender_repair_sent_overlap_ack_missing_count"].as_u64(),
            Some(50)
        );
        assert_eq!(
            report["sender_repair_sent_duplicate_sequence_count"].as_u64(),
            Some(50)
        );
        assert_eq!(
            report["repair_duplicate_waste_ratio_bps"].as_u64(),
            Some(3333)
        );
    }

    #[test]
    fn wrapper_final_preserves_runtime_blocked_reason() {
        let source = serde_json::json!({
            "ledger_final_missing_candidate_count": 304,
            "ledger_final_missing_actual_batch_count": 0,
            "ledger_final_missing_receipt_written_count": 0,
            "ledger_final_missing_receipt_missing_after_admission_count": 0,
            "ledger_final_missing_batch_blocked_reason": "",
        });
        let mut wrapper_report = serde_json::json!({});
        apply_ledger_receipt_completion_fields_v1(&mut wrapper_report, Some(&source));

        assert_eq!(
            wrapper_report["ledger_final_missing_batch_blocked_reason"].as_str(),
            Some("classification_path_not_reached")
        );
        assert_eq!(
            wrapper_report
                ["ledger_final_missing_batch_blocked_by_classification_path_not_reached_count"]
                .as_u64(),
            Some(304)
        );
    }

    #[test]
    fn ledger_candidate_empty_rehydrates_from_durable_missing() {
        let summary = serde_json::json!({
            "included_canonical_total": 14072,
            "aoem_executed_total": 14072,
            "queue_pending_last": 0,
            "ledger_durable_missing_count": 328,
            "ledger_durable_missing_ranges_sample": [
                {"start": 14072, "end_inclusive": 14399, "count": 328}
            ],
            "ledger_durable_missing_bitmap_available": true,
            "ledger_durable_missing_source": "sequence_lifecycle_ledger",
            "ledger_final_missing_candidate_count": 0,
            "ledger_candidate_empty_but_durable_missing_count": 328,
            "ledger_missing_without_candidate_count": 328,
            "ledger_candidate_rehydrated_count": 64,
            "ledger_missing_without_retryable_count": 0,
        });
        let sample = diagnostics_summary_sample(
            Instant::now(),
            &summary,
            serde_json::json!({"line_count": 14072, "bytes": 1}),
            serde_json::json!({}),
            serde_json::json!({}),
            14064,
        );

        assert_eq!(sample["ledger_durable_missing_count"].as_u64(), Some(328));
        assert_eq!(
            sample["ledger_candidate_empty_but_durable_missing_count"].as_u64(),
            Some(328)
        );
        assert_eq!(
            sample["ledger_candidate_rehydrated_count"].as_u64(),
            Some(64)
        );
        assert_eq!(
            sample["ledger_durable_missing_bitmap_available"].as_bool(),
            Some(true)
        );
    }

    #[test]
    fn admitted_counter_must_match_actual_batch() {
        let source = serde_json::json!({
            "ledger_final_missing_admitted_count": 1853,
            "ledger_final_missing_actual_batch_count": 0,
            "ledger_final_missing_receipt_written_count": 0,
            "ledger_final_missing_receipt_missing_after_admission_count": 1853,
            "ledger_admission_counter_is_actual_batch": false,
            "ledger_admission_counter_mismatch_reason": "admitted_counter_without_actual_batch",
        });
        let mut synthetic = serde_json::json!({});
        apply_ledger_receipt_completion_fields_v1(&mut synthetic, Some(&source));

        assert_eq!(
            synthetic["ledger_admission_counter_is_actual_batch"].as_bool(),
            Some(false)
        );
        assert_eq!(
            synthetic["ledger_admission_counter_mismatch_reason"].as_str(),
            Some("admitted_counter_without_actual_batch")
        );
        assert_eq!(
            synthetic["ledger_final_missing_actual_batch_count"].as_u64(),
            Some(0)
        );
    }

    #[test]
    fn novorudp_first_missing_window_limits_repair_scope() {
        let ranges = vec![MissingRangeV1 {
            start: 14112,
            end_inclusive: 14399,
        }];
        let (window_id, window, window_ranges) =
            first_missing_window_ranges(ranges.as_slice(), 14400, 64)
                .expect("window should be selected");

        assert_eq!(window_id, 220);
        assert_eq!(window.start, 14112);
        assert_eq!(window.end_inclusive, 14175);
        assert_eq!(missing_ranges_count(window_ranges.as_slice()), 64);
        assert_eq!(window_ranges.len(), 1);
        assert_eq!(window_ranges[0].start, 14112);
        assert_eq!(window_ranges[0].end_inclusive, 14175);
    }

    #[test]
    fn novorudp_first_missing_window_intersects_sparse_ranges() {
        let ranges = vec![
            MissingRangeV1 {
                start: 100,
                end_inclusive: 120,
            },
            MissingRangeV1 {
                start: 190,
                end_inclusive: 220,
            },
        ];
        let (_, window, window_ranges) = first_missing_window_ranges(ranges.as_slice(), 512, 64)
            .expect("window should be selected");

        assert_eq!(window.start, 100);
        assert_eq!(window.end_inclusive, 163);
        assert_eq!(missing_ranges_count(window_ranges.as_slice()), 21);
        assert_eq!(window_ranges.len(), 1);
    }

    #[test]
    fn novorudp_tail_window_repair_completes_last_window() {
        let ranges = vec![MissingRangeV1 {
            start: 14160,
            end_inclusive: 14399,
        }];
        let (_, first_window, first_window_ranges) =
            first_missing_window_ranges(ranges.as_slice(), 14400, 64).expect("first repair window");
        assert_eq!(first_window.start, 14160);
        assert_eq!(first_window.end_inclusive, 14223);

        let tail_gap = tail_gap_range_from_ack(14400, Some(66), Some(14333))
            .expect("tail gap must be detected");
        assert_eq!(tail_gap.start, 14334);
        assert_eq!(tail_gap.end_inclusive, 14399);

        let repair_ranges = merge_tail_gap_into_repair_ranges(
            first_window_ranges.as_slice(),
            Some(tail_gap),
            14400,
        );
        assert!(
            missing_ranges_overlap_count(&repair_ranges, &[tail_gap])
                == missing_ranges_count(&[tail_gap]),
            "tail gap must be present even when the first missing window is lower"
        );

        let mut current_window_missing_count = missing_ranges_count(&[tail_gap]);
        let mut retry_count = 0u64;
        while current_window_missing_count > 0 && retry_count < 16 {
            retry_count = retry_count.saturating_add(1);
            current_window_missing_count = 0;
        }
        assert_eq!(current_window_missing_count, 0);
        assert!(retry_count <= 16);
    }

    #[test]
    fn novorudp_tail_window_does_not_complete_on_max_sequence_only() {
        let current_missing = vec![
            MissingRangeV1 {
                start: 14156,
                end_inclusive: 14333,
            },
            MissingRangeV1 {
                start: 14399,
                end_inclusive: 14399,
            },
        ];
        let still_missing = vec![MissingRangeV1 {
            start: 14334,
            end_inclusive: 14398,
        }];
        let repair_received_max = 14399u64;
        let current_window_missing_count = missing_ranges_count(still_missing.as_slice());

        assert_eq!(repair_received_max, 14399);
        assert!(current_window_missing_count > 0);
        assert!(
            missing_ranges_overlap_count(still_missing.as_slice(), current_missing.as_slice()) == 0,
            "max sequence coverage must not imply full current missing coverage"
        );
    }

    #[test]
    fn novorudp_tail_window_repairs_current_missing_bitmap_until_zero() {
        let config = sender_timeout_novorudp_config();
        let ack_round_1 = vec![
            MissingRangeV1 {
                start: 14156,
                end_inclusive: 14175,
            },
            MissingRangeV1 {
                start: 14270,
                end_inclusive: 14331,
            },
        ];
        let first = select_novorudp_repair_ranges_from_ack(
            ack_round_1.as_slice(),
            14400,
            config.window_size,
            82,
            ack_round_1.len() as u64,
            config.tail_window_max_retries,
        )
        .expect("repair selection");
        assert!(!first.used_full_missing_bitmap);
        assert_eq!(first.window.start, 14156);
        assert_eq!(first.window.end_inclusive, 14219);
        assert_eq!(missing_ranges_count(first.ranges.as_slice()), 20);

        let ack_round_2 = vec![MissingRangeV1 {
            start: 14312,
            end_inclusive: 14331,
        }];
        let second = select_novorudp_repair_ranges_from_ack(
            ack_round_2.as_slice(),
            14400,
            config.window_size,
            20,
            ack_round_2.len() as u64,
            config.tail_window_max_retries,
        )
        .expect("second repair selection");
        assert!(!second.used_full_missing_bitmap);
        assert_eq!(missing_ranges_count(second.ranges.as_slice()), 20);

        let ack_round_3: Vec<MissingRangeV1> = Vec::new();
        let done = select_novorudp_repair_ranges_from_ack(
            ack_round_3.as_slice(),
            14400,
            config.window_size,
            0,
            0,
            config.tail_window_max_retries,
        );
        assert!(done.is_none());
    }

    #[test]
    fn novorudp_current_missing_full_ranges_not_sample_only() {
        let config = sender_timeout_novorudp_config();
        let ranges = vec![MissingRangeV1 {
            start: 14156,
            end_inclusive: 14399,
        }];
        let selection = select_novorudp_repair_ranges_from_ack(
            ranges.as_slice(),
            14400,
            config.window_size,
            244,
            ranges.len() as u64,
            config.tail_window_max_retries,
        )
        .expect("full current missing selection");

        assert!(!selection.used_full_missing_bitmap);
        assert_eq!(selection.window.start, 14156);
        assert_eq!(selection.window.end_inclusive, 14219);
        assert_eq!(
            selection.ranges,
            vec![MissingRangeV1 {
                start: 14156,
                end_inclusive: 14219,
            }]
        );
        assert_eq!(missing_ranges_count(selection.ranges.as_slice()), 64);
    }

    #[test]
    fn novorudp_sender_recomputes_repair_window_when_ack_progresses() {
        let config = sender_timeout_novorudp_config();
        let old_ack_ranges = vec![MissingRangeV1 {
            start: 3752,
            end_inclusive: 14399,
        }];
        let old_selection = select_novorudp_repair_ranges_from_ack(
            old_ack_ranges.as_slice(),
            14400,
            config.window_size,
            10648,
            old_ack_ranges.len() as u64,
            config.tail_window_max_retries,
        )
        .expect("old ack selection");
        assert_eq!(old_selection.window.start, 3752);
        assert_eq!(old_selection.window.end_inclusive, 3815);

        let stale_tail_gap =
            tail_gap_range_from_ack(14400, Some(10648), Some(3751)).expect("stale tail gap");
        assert!(!novorudp_should_send_tail_gap(
            stale_tail_gap,
            config.window_size,
            config.tail_window_max_retries
        ));

        let new_ack_ranges = vec![MissingRangeV1 {
            start: 14163,
            end_inclusive: 14399,
        }];
        let new_selection = select_novorudp_repair_ranges_from_ack(
            new_ack_ranges.as_slice(),
            14400,
            config.window_size,
            237,
            new_ack_ranges.len() as u64,
            config.tail_window_max_retries,
        )
        .expect("new ack selection");
        assert!(!new_selection.used_full_missing_bitmap);
        assert_eq!(new_selection.window.start, 14163);
        assert_eq!(new_selection.window.end_inclusive, 14226);
        assert_eq!(
            new_selection.ranges,
            vec![MissingRangeV1 {
                start: 14163,
                end_inclusive: 14226,
            }]
        );
    }

    #[test]
    fn novorudp_sender_uses_latest_ack_first_missing_window_bitmap() {
        let config = sender_timeout_novorudp_config();
        let ranges = vec![MissingRangeV1 {
            start: 14162,
            end_inclusive: 14399,
        }];
        let selection = select_novorudp_repair_ranges_from_ack(
            ranges.as_slice(),
            14400,
            config.window_size,
            238,
            ranges.len() as u64,
            config.tail_window_max_retries,
        )
        .expect("latest ack selection");

        assert!(!selection.used_full_missing_bitmap);
        assert_eq!(selection.ranges[0].start, 14162);
        assert_eq!(selection.ranges[0].end_inclusive, 14225);
        assert_eq!(missing_ranges_count(selection.ranges.as_slice()), 64);
    }

    #[test]
    fn novorudp_sender_does_not_send_huge_tail_gap_from_stale_ack() {
        let config = sender_timeout_novorudp_config();
        let stale_tail_gap =
            tail_gap_range_from_ack(14400, Some(10648), Some(3751)).expect("stale tail gap");
        let final_tail_gap =
            tail_gap_range_from_ack(14400, Some(238), Some(14161)).expect("final tail gap");

        assert_eq!(stale_tail_gap.start, 3752);
        assert_eq!(stale_tail_gap.end_inclusive, 14399);
        assert!(!novorudp_should_send_tail_gap(
            stale_tail_gap,
            config.window_size,
            config.tail_window_max_retries
        ));
        assert!(novorudp_should_send_tail_gap(
            final_tail_gap,
            config.window_size,
            config.tail_window_max_retries
        ));
    }

    #[test]
    fn novorudp_sender_does_not_loop_forever_on_stale_ack() {
        let config = sender_timeout_novorudp_config();
        let stale_ack_epoch_before = 106u64;
        let stale_ack_epoch_after = 106u64;
        let stale_tail_gap =
            tail_gap_range_from_ack(14400, Some(10648), Some(3751)).expect("stale tail gap");
        let stale_ack_repair_aborted_count = if stale_ack_epoch_after == stale_ack_epoch_before
            && !novorudp_should_send_tail_gap(
                stale_tail_gap,
                config.window_size,
                config.tail_window_max_retries,
            ) {
            1
        } else {
            0
        };

        assert_eq!(stale_ack_repair_aborted_count, 1);
        assert!(!novorudp_should_send_tail_gap(
            stale_tail_gap,
            config.window_size,
            config.tail_window_max_retries
        ));
    }

    #[test]
    fn real_sender_uses_receiver_owned_current_window() {
        let ack_value = serde_json::json!({
            "schema": "novovm-native-pipeline-cross-machine-sustained-ack/v1",
            "expected_tx_total": 14400,
            "missing_count": 248,
            "missing_ranges_full_count": 1,
            "missing_ranges_sample": [{"start": 14152, "end_inclusive": 14399}],
            "novorudp_current_window_id": 221,
            "novorudp_current_window_start": 14152,
            "novorudp_current_window_end_inclusive": 14215,
            "novorudp_current_window_missing_count": 64,
            "novorudp_current_window_missing_ranges_sample": [{"start": 14152, "end_inclusive": 14215}],
            "ack_epoch": 500,
            "receiver_done": false,
        });
        let ack = parse_ack_value(&ack_value, 256).expect("ack");
        let selection = select_novorudp_repair_ranges_from_receiver_ack(&ack, 14400, 64, 16)
            .expect("selection");

        assert_eq!(
            selection.window,
            MissingRangeV1 {
                start: 14152,
                end_inclusive: 14215
            }
        );
        assert_eq!(
            selection.ranges,
            vec![MissingRangeV1 {
                start: 14152,
                end_inclusive: 14215
            }]
        );
        assert!(selection.used_full_missing_bitmap);
    }

    #[test]
    fn ledger_ack_missing_bitmap_is_source_of_truth_for_receiver_ack() {
        let summary = serde_json::json!({
            "missing_count": 280,
            "missing_ranges_sample": [{"start": 14120, "end_inclusive": 14399}],
            "included_canonical_total": 14120,
            "aoem_executed_total": 14120,
        });
        let ack = receiver_ack_report_value_with_summary(14_400, 12_152, 256, 9, Some(&summary));

        assert_eq!(
            ack.get("missing_bitmap_source").and_then(Value::as_str),
            Some("progress_summary")
        );
        assert_eq!(ack.get("missing_count").and_then(Value::as_u64), Some(280));
        assert_eq!(
            ack.get("novorudp_current_window_start")
                .and_then(Value::as_u64),
            Some(14_120)
        );
        assert_eq!(
            ack.get("novorudp_current_window_end_inclusive")
                .and_then(Value::as_u64),
            Some(14_183)
        );
        assert_eq!(
            ack.get("novorudp_current_window_missing_count")
                .and_then(Value::as_u64),
            Some(64)
        );
    }

    #[test]
    fn receiver_ack_uses_progress_summary_missing_bitmap_when_available() {
        let summary = serde_json::json!({
            "missing_count": 280,
            "missing_ranges_sample": [{"start": 14120, "end_inclusive": 14399}],
            "ledger_durable_missing_count": 336,
            "ledger_durable_missing_ranges_sample": [{"start": 14064, "end_inclusive": 14399}],
            "ledger_durable_missing_bitmap_available": true,
        });
        let ack = receiver_ack_report_value_with_summary(14_400, 14_064, 256, 10, Some(&summary));

        assert_eq!(
            ack.get("missing_bitmap_source").and_then(Value::as_str),
            Some("progress_summary")
        );
        assert_eq!(
            ack.get("ack_source_selection_reason")
                .and_then(Value::as_str),
            Some("progress_summary_missing_bitmap")
        );
        assert_eq!(
            ack.get("novorudp_current_window_start")
                .and_then(Value::as_u64),
            Some(14_120)
        );
    }

    #[test]
    fn receiver_ack_uses_ledger_missing_bitmap_before_stable_fallback() {
        let summary = serde_json::json!({
            "ledger_durable_missing_count": 280,
            "ledger_durable_missing_ranges_sample": [{"start": 14120, "end_inclusive": 14399}],
            "ledger_durable_missing_bitmap_available": true,
            "ledger_final_missing_admitted_count": 1182,
        });
        let ack = receiver_ack_report_value_with_summary(14_400, 14_064, 256, 11, Some(&summary));

        assert_eq!(
            ack.get("missing_bitmap_source").and_then(Value::as_str),
            Some("ledger")
        );
        assert_eq!(
            ack.get("ack_source_selection_reason")
                .and_then(Value::as_str),
            Some("ledger_durable_missing_bitmap")
        );
        assert_eq!(
            ack.get("novorudp_current_window_start")
                .and_then(Value::as_u64),
            Some(14_120)
        );
        assert_eq!(
            ack.get("ledger_missing_bitmap_available")
                .and_then(Value::as_bool),
            Some(true)
        );
        assert_eq!(
            ack.get("ack_used_durable_ledger_missing_bitmap")
                .and_then(Value::as_bool),
            Some(true)
        );
    }

    #[test]
    fn ledger_ack_missing_bitmap_survives_candidate_bucket_empty() {
        let summary = serde_json::json!({
            "ledger_durable_missing_count": 328,
            "ledger_durable_missing_ranges_sample": [{"start": 14072, "end_inclusive": 14399}],
            "ledger_durable_missing_bitmap_available": true,
            "ledger_final_missing_candidate_count": 0,
            "ledger_final_missing_candidate_ranges_sample": [],
        });
        let ack = receiver_ack_report_value_with_summary(14_400, 14_072, 256, 14, Some(&summary));

        assert_eq!(
            ack.get("missing_bitmap_source").and_then(Value::as_str),
            Some("ledger")
        );
        assert_eq!(
            ack.get("ack_source_selection_reason")
                .and_then(Value::as_str),
            Some("ledger_durable_missing_bitmap")
        );
        assert_eq!(
            ack.get("ack_fallback_due_to_candidate_empty_count")
                .and_then(Value::as_u64),
            Some(0)
        );
        assert_eq!(
            ack.get("novorudp_current_window_start")
                .and_then(Value::as_u64),
            Some(14_072)
        );
    }

    #[test]
    fn receiver_ack_reports_fallback_reason_when_source_unavailable() {
        let ack = receiver_ack_report_value_with_summary(14_400, 14_064, 256, 12, None);

        assert_eq!(
            ack.get("missing_bitmap_source").and_then(Value::as_str),
            Some("stable_progress_fallback")
        );
        assert_eq!(
            ack.get("missing_bitmap_fallback_reason")
                .and_then(Value::as_str),
            Some("progress_summary_unavailable")
        );
        assert_eq!(
            ack.get("novorudp_current_window_start")
                .and_then(Value::as_u64),
            Some(14_064)
        );
    }

    #[test]
    fn receiver_ack_does_not_fallback_when_ledger_admission_present() {
        let summary = serde_json::json!({
            "ledger_durable_missing_count": 336,
            "ledger_durable_missing_ranges_sample": [{"start": 14064, "end_inclusive": 14399}],
            "ledger_durable_missing_bitmap_available": true,
            "ledger_final_missing_candidate_count": 0,
            "ledger_final_missing_admitted_count": 1182,
        });
        let ack = receiver_ack_report_value_with_summary(14_400, 13_024, 256, 13, Some(&summary));

        assert_ne!(
            ack.get("missing_bitmap_source").and_then(Value::as_str),
            Some("stable_progress_fallback")
        );
        assert_eq!(
            ack.get("missing_bitmap_source").and_then(Value::as_str),
            Some("ledger")
        );
    }

    #[test]
    fn ack_does_not_fallback_when_durable_missing_exists() {
        let summary = serde_json::json!({
            "ledger_durable_missing_count": 67,
            "ledger_durable_missing_ranges_sample": [{"start": 14333, "end_inclusive": 14399}],
            "ledger_durable_missing_bitmap_available": true,
            "ledger_final_missing_candidate_count": 0,
        });
        let ack = receiver_ack_report_value_with_summary(14_400, 14_333, 256, 15, Some(&summary));

        assert_ne!(
            ack.get("missing_bitmap_source").and_then(Value::as_str),
            Some("stable_progress_fallback")
        );
        assert_eq!(
            ack.get("missing_bitmap_source").and_then(Value::as_str),
            Some("ledger")
        );
        assert_eq!(
            ack.get("missing_bitmap_fallback_reason"),
            Some(&Value::Null)
        );
    }

    #[test]
    fn tail_gap_14072_14399_uses_durable_missing_window() {
        let summary = serde_json::json!({
            "ledger_durable_missing_count": 328,
            "ledger_durable_missing_ranges_sample": [{"start": 14072, "end_inclusive": 14399}],
            "ledger_durable_missing_bitmap_available": true,
            "ledger_final_missing_candidate_count": 0,
        });
        let ack = receiver_ack_report_value_with_summary(14_400, 12_000, 256, 16, Some(&summary));

        assert_eq!(ack.get("missing_count").and_then(Value::as_u64), Some(328));
        assert_eq!(
            ack.get("novorudp_current_window_start")
                .and_then(Value::as_u64),
            Some(14_072)
        );
        assert_eq!(
            ack.get("novorudp_current_window_end_inclusive")
                .and_then(Value::as_u64),
            Some(14_135)
        );
        assert_eq!(
            ack.get("novorudp_current_window_missing_count")
                .and_then(Value::as_u64),
            Some(64)
        );
    }

    #[test]
    fn durable_missing_includes_unreceipted_final_tail() {
        let mut ledger = novovm_network::NovoRudpSequenceLifecycleLedger::new(14_400, 64);
        for sequence in 0..14_144 {
            ledger.mark_receipt_written(sequence, 1);
        }
        for sequence in 14_144..14_400 {
            ledger.observe_repair_received(sequence, [sequence as u8; 32], true, 2);
            ledger.mark_pending_active(sequence, 3, "repair_enqueued");
        }

        let missing = ledger.ack_missing_bitmap();
        let missing_count = missing
            .iter()
            .fold(0u64, |total, range| total.saturating_add(range.count()));
        assert_eq!(missing_count, 256);
        assert_eq!(
            missing,
            vec![novovm_network::NovoRudpRange::new(14_144, 14_399)]
        );
    }

    #[test]
    fn received_enqueued_does_not_clear_durable_missing() {
        let mut ledger = novovm_network::NovoRudpSequenceLifecycleLedger::new(14_400, 64);
        for sequence in 0..14_144 {
            ledger.mark_receipt_written(sequence, 1);
        }
        ledger.observe_repair_received(14_144, [1u8; 32], true, 2);
        ledger.mark_pending_active(14_144, 3, "repair_enqueued");
        ledger.mark_admitted_to_aoem(14_144, 4);

        assert!(ledger
            .ack_missing_bitmap()
            .iter()
            .any(|range| { range.start <= 14_144 && range.end_inclusive >= 14_144 }));
    }

    #[test]
    fn receipt_closes_durable_missing() {
        let mut ledger = novovm_network::NovoRudpSequenceLifecycleLedger::new(14_400, 64);
        for sequence in 0..14_144 {
            ledger.mark_receipt_written(sequence, 1);
        }
        ledger.observe_repair_received(14_144, [1u8; 32], true, 2);
        ledger.mark_pending_active(14_144, 3, "repair_enqueued");
        assert!(ledger
            .ack_missing_bitmap()
            .iter()
            .any(|range| { range.start <= 14_144 && range.end_inclusive >= 14_144 }));

        ledger.mark_receipt_written(14_144, 4);
        assert!(!ledger
            .ack_missing_bitmap()
            .iter()
            .any(|range| { range.start <= 14_144 && range.end_inclusive >= 14_144 }));
    }

    #[test]
    fn candidate_bucket_cannot_exceed_durable_missing_truth() {
        let summary = serde_json::json!({
            "included_canonical_total": 14144,
            "aoem_executed_total": 14144,
            "ledger_durable_missing_count": 0,
            "ledger_final_missing_candidate_count": 256,
            "ledger_candidate_count_exceeds_durable_missing_invariant_violation_count": 256,
            "ledger_final_missing_without_durable_missing_count": 256,
        });
        let sample = diagnostics_summary_sample(
            Instant::now(),
            &summary,
            serde_json::json!({"line_count": 14144, "bytes": 1}),
            serde_json::json!({}),
            serde_json::json!({}),
            14144,
        );

        assert_eq!(
            sample["ledger_candidate_count_exceeds_durable_missing_invariant_violation_count"]
                .as_u64(),
            Some(256)
        );
        assert_eq!(
            sample["ledger_final_missing_without_durable_missing_count"].as_u64(),
            Some(256)
        );
    }

    #[test]
    fn tail_gap_14144_14399_durable_missing_ack_window() {
        let summary = serde_json::json!({
            "ledger_expected_range_start": 0,
            "ledger_expected_range_end": 14399,
            "ledger_expected_count": 14400,
            "ledger_completed_count": 14144,
            "ledger_durable_missing_count": 256,
            "ledger_durable_missing_ranges_sample": [{"start": 14144, "end_inclusive": 14399}],
            "ledger_durable_missing_bitmap_available": true,
            "ledger_durable_missing_derived_from_expected_range": true,
        });
        let ack = receiver_ack_report_value_with_summary(14_400, 14_144, 256, 17, Some(&summary));

        assert_eq!(
            ack.get("missing_bitmap_source").and_then(Value::as_str),
            Some("ledger")
        );
        assert_eq!(
            ack.get("ack_source_selection_reason")
                .and_then(Value::as_str),
            Some("ledger_durable_missing_bitmap")
        );
        assert_eq!(
            ack.get("novorudp_current_window_start")
                .and_then(Value::as_u64),
            Some(14_144)
        );
        assert_eq!(
            ack.get("novorudp_current_window_missing_count")
                .and_then(Value::as_u64),
            Some(64)
        );
        assert_ne!(
            ack.get("missing_bitmap_source").and_then(Value::as_str),
            Some("stable_progress_fallback")
        );
    }

    #[test]
    fn real_sender_rejects_stale_ack_for_repair() {
        let latest_epoch = 500u64;
        let stale_ack = serde_json::json!({
            "schema": "novovm-native-pipeline-cross-machine-sustained-ack/v1",
            "missing_count": 10648,
            "missing_ranges_full_count": 1,
            "missing_ranges_sample": [{"start": 3752, "end_inclusive": 14399}],
            "novorudp_current_window_id": 58,
            "novorudp_current_window_start": 3752,
            "novorudp_current_window_end_inclusive": 3815,
            "novorudp_current_window_missing_count": 64,
            "novorudp_current_window_missing_ranges_sample": [{"start": 3752, "end_inclusive": 3815}],
            "ack_epoch": 499,
            "receiver_done": false,
        });
        let ack = parse_ack_value(&stale_ack, 256).expect("ack");

        assert!(ack.latest_epoch <= latest_epoch);
        assert!(
            ack.latest_epoch > latest_epoch
                || select_novorudp_repair_ranges_from_receiver_ack(&ack, 14400, 64, 16).is_some()
        );
        assert!(
            ack.latest_epoch <= latest_epoch,
            "real sender gate must reject this ack before repair selection"
        );
    }

    #[test]
    fn real_sender_does_not_advance_until_window_bitmap_zero() {
        let ack_value = serde_json::json!({
            "schema": "novovm-native-pipeline-cross-machine-sustained-ack/v1",
            "missing_count": 32,
            "missing_ranges_full_count": 1,
            "missing_ranges_sample": [{"start": 14184, "end_inclusive": 14215}],
            "novorudp_current_window_id": 221,
            "novorudp_current_window_start": 14152,
            "novorudp_current_window_end_inclusive": 14215,
            "novorudp_current_window_missing_count": 32,
            "novorudp_current_window_missing_ranges_sample": [{"start": 14184, "end_inclusive": 14215}],
            "ack_epoch": 501,
            "receiver_done": false,
        });
        let ack = parse_ack_value(&ack_value, 256).expect("ack");
        let selection = select_novorudp_repair_ranges_from_receiver_ack(&ack, 14400, 64, 16)
            .expect("selection");

        assert_eq!(ack.novorudp_current_window_missing_count, 32);
        assert_eq!(
            selection.window,
            MissingRangeV1 {
                start: 14152,
                end_inclusive: 14215
            }
        );
        assert_eq!(
            selection.ranges,
            vec![MissingRangeV1 {
                start: 14184,
                end_inclusive: 14215
            }]
        );
        assert!(!ack.receiver_done);
    }

    #[test]
    fn real_sender_uses_full_current_missing_bitmap_not_sample() {
        let ack_value = serde_json::json!({
            "schema": "novovm-native-pipeline-cross-machine-sustained-ack/v1",
            "missing_count": 238,
            "missing_ranges_full_count": 1,
            "missing_ranges_sample": [{"start": 14162, "end_inclusive": 14399}],
            "novorudp_current_window_id": 221,
            "novorudp_current_window_start": 14162,
            "novorudp_current_window_end_inclusive": 14225,
            "novorudp_current_window_missing_count": 64,
            "novorudp_current_window_missing_ranges_sample": [
                {"start": 14162, "end_inclusive": 14175},
                {"start": 14190, "end_inclusive": 14225}
            ],
            "ack_epoch": 502,
            "receiver_done": false,
        });
        let ack = parse_ack_value(&ack_value, 256).expect("ack");
        let selection = select_novorudp_repair_ranges_from_receiver_ack(&ack, 14400, 64, 16)
            .expect("selection");

        assert_eq!(selection.ranges.len(), 2);
        assert_eq!(missing_ranges_count(selection.ranges.as_slice()), 50);
        assert_eq!(selection.window.start, 14162);
        assert_eq!(selection.window.end_inclusive, 14225);
    }

    #[test]
    fn real_receiver_ack_advances_window_only_after_bitmap_zero() {
        let ack_before = receiver_ack_report_value(14400, 14152, 256, 1);
        let before = parse_ack_value(&ack_before, 256).expect("ack before");
        assert_eq!(
            before.novorudp_current_window,
            Some(MissingRangeV1 {
                start: 14152,
                end_inclusive: 14215,
            })
        );
        assert_eq!(before.novorudp_current_window_missing_count, 64);

        let ack_after = receiver_ack_report_value(14400, 14216, 256, 2);
        let after = parse_ack_value(&ack_after, 256).expect("ack after");
        assert_eq!(
            after.novorudp_current_window,
            Some(MissingRangeV1 {
                start: 14216,
                end_inclusive: 14279,
            })
        );
        assert_eq!(after.novorudp_current_window_missing_count, 64);
    }

    #[test]
    fn novorudp_receiver_emits_ack_when_window_bitmap_changes() {
        let mut epoch = 0u64;
        let before_epoch = next_receiver_ack_epoch(&mut epoch);
        let before = parse_ack_value(
            &receiver_ack_report_value(14400, 14152, 256, before_epoch),
            256,
        )
        .expect("before ack");
        let after_epoch = next_receiver_ack_epoch(&mut epoch);
        let after = parse_ack_value(
            &receiver_ack_report_value(14400, 14162, 256, after_epoch),
            256,
        )
        .expect("after ack");

        assert!(after.latest_epoch > before.latest_epoch);
        assert!(after.latest_missing_count < before.latest_missing_count);
        assert!(
            after.novorudp_current_window.unwrap().start
                > before.novorudp_current_window.unwrap().start
        );
    }

    #[test]
    fn novorudp_sender_waits_for_fresh_ack_after_window_repair() {
        let stale_ack = parse_ack_value(
            &serde_json::json!({
                "packet_type": "native_pipeline_ack_v1",
                "expected_tx_total": 14400,
                "missing_count": 248,
                "missing_ranges_full_count": 1,
                "missing_ranges_sample": [{"start": 14152, "end_inclusive": 14399}],
                "novorudp_current_window_id": 221,
                "novorudp_current_window_start": 14152,
                "novorudp_current_window_end_inclusive": 14215,
                "novorudp_current_window_missing_count": 64,
                "novorudp_current_window_missing_ranges_sample": [{"start": 14152, "end_inclusive": 14215}],
                "ack_epoch": 700,
                "receiver_done": false,
            }),
            256,
        )
        .expect("stale ack");
        let fresh_ack = parse_ack_value(
            &serde_json::json!({
                "packet_type": "native_pipeline_ack_v1",
                "expected_tx_total": 14400,
                "missing_count": 238,
                "missing_ranges_full_count": 1,
                "missing_ranges_sample": [{"start": 14162, "end_inclusive": 14399}],
                "novorudp_current_window_id": 221,
                "novorudp_current_window_start": 14152,
                "novorudp_current_window_end_inclusive": 14215,
                "novorudp_current_window_missing_count": 54,
                "novorudp_current_window_missing_ranges_sample": [{"start": 14162, "end_inclusive": 14215}],
                "ack_epoch": 701,
                "receiver_done": false,
            }),
            256,
        )
        .expect("fresh ack");

        let latest_epoch = 700u64;
        assert!(stale_ack.latest_epoch <= latest_epoch);
        assert!(
            fresh_ack.latest_epoch > latest_epoch
                && fresh_ack.latest_missing_count < stale_ack.latest_missing_count
        );
        let selection = select_novorudp_repair_ranges_from_receiver_ack(&fresh_ack, 14400, 64, 16)
            .expect("fresh ack should drive next repair");
        assert_eq!(selection.window_id, 221);
        assert_eq!(missing_ranges_count(selection.ranges.as_slice()), 54);
    }

    #[test]
    fn novorudp_window_progress_closure_converges_current_window() {
        let mut epoch = 0u64;
        let mut progress = 14152u64;
        let expected = 14400u64;
        let mut observed_progress = false;

        while progress < 14216 {
            let ack_epoch = next_receiver_ack_epoch(&mut epoch);
            let ack = parse_ack_value(
                &receiver_ack_report_value(expected, progress, 256, ack_epoch),
                256,
            )
            .expect("ack");
            let selection = select_novorudp_repair_ranges_from_receiver_ack(&ack, expected, 64, 16)
                .expect("window repair selection");
            assert_eq!(selection.window.start, progress);
            assert!(missing_ranges_count(selection.ranges.as_slice()) <= 64);
            progress = progress.saturating_add(16).min(14216);
            let next_ack_epoch = next_receiver_ack_epoch(&mut epoch);
            let next_ack = parse_ack_value(
                &receiver_ack_report_value(expected, progress, 256, next_ack_epoch),
                256,
            )
            .expect("next ack");
            observed_progress |= next_ack.latest_missing_count < ack.latest_missing_count;
        }

        let done_ack_epoch = next_receiver_ack_epoch(&mut epoch);
        let done_ack = parse_ack_value(
            &receiver_ack_report_value(expected, progress, 256, done_ack_epoch),
            256,
        )
        .expect("done-window ack");
        assert!(observed_progress);
        assert_eq!(done_ack.novorudp_current_window_missing_count, 64);
        assert_eq!(done_ack.novorudp_current_window.unwrap().start, 14216);
    }

    #[test]
    fn novorudp_receiver_progress_ack_interval_is_not_diagnostics_interval() {
        with_env_var("NOVOVM_NATIVE_PIPELINE_TRANSPORT", Some("novorudp"), || {
            with_env_var(
                "NOVOVM_NATIVE_PIPELINE_PROGRESS_SAMPLE_INTERVAL_MS",
                Some("5000"),
                || {
                    with_env_var(
                        "NOVOVM_NOVORUDP_ACK_PROGRESS_INTERVAL_MS",
                        Some("250"),
                        || {
                            assert_eq!(receiver_child_progress_report_interval_ms(), 250);
                        },
                    )
                },
            )
        });
    }

    #[test]
    fn transport_profile_uses_canonical_transport_env() {
        with_env_var("NOVOVM_NATIVE_PIPELINE_TRANSPORT_PROFILE", None, || {
            with_env_var("NOVOVM_NATIVE_PIPELINE_TRANSPORT", Some("novorudp"), || {
                assert_eq!(
                    TransportProfileV1::from_env().expect("transport profile"),
                    TransportProfileV1::NovoRudp
                );
            })
        });
    }

    #[test]
    fn transport_profile_rejects_legacy_profile_env_for_novorudp() {
        with_env_var("NOVOVM_NATIVE_PIPELINE_TRANSPORT", None, || {
            with_env_var(
                "NOVOVM_NATIVE_PIPELINE_TRANSPORT_PROFILE",
                Some("novorudp"),
                || {
                    let err = TransportProfileV1::from_env().expect_err("legacy profile rejected");
                    assert!(err
                        .to_string()
                        .contains("NOVOVM_NATIVE_PIPELINE_TRANSPORT_PROFILE is not supported"));
                },
            )
        });
    }

    #[test]
    fn transport_profile_defaults_to_novorudp() {
        with_env_var("NOVOVM_NATIVE_PIPELINE_TRANSPORT", None, || {
            with_env_var("NOVOVM_NATIVE_PIPELINE_TRANSPORT_PROFILE", None, || {
                assert_eq!(
                    TransportProfileV1::from_env().expect("default transport profile"),
                    TransportProfileV1::NovoRudp
                );
            })
        });
    }

    #[test]
    fn novorudp_sender_extends_repair_deadline_while_ack_progresses() {
        let decision =
            novorudp_sender_timeout_decision_v1(1_920_000, 0, 120_000, 2_700_000, false, 2_248);

        assert_eq!(decision, NovoRudpSenderTimeoutDecisionV1::Continue);
    }

    #[test]
    fn novorudp_sender_times_out_only_after_true_no_progress() {
        let decision = novorudp_sender_timeout_decision_v1(
            1_920_000, 120_000, 120_000, 2_700_000, false, 2_248,
        );

        assert_eq!(decision, NovoRudpSenderTimeoutDecisionV1::NoProgressTimeout);
    }

    #[test]
    fn novorudp_sender_continues_moving_window_until_receiver_done() {
        let config = sender_timeout_novorudp_config();
        let early_ack = parse_ack_value(
            &serde_json::json!({
                "packet_type": "native_pipeline_ack_v1",
                "expected_tx_total": 14400,
                "missing_count": 2248,
                "missing_ranges_full_count": 1,
                "missing_ranges_sample": [{"start": 12152, "end_inclusive": 14399}],
                "novorudp_current_window_id": 189,
                "novorudp_current_window_start": 12152,
                "novorudp_current_window_end_inclusive": 12215,
                "novorudp_current_window_missing_count": 64,
                "novorudp_current_window_missing_ranges_sample": [{"start": 12152, "end_inclusive": 12215}],
                "ack_epoch": 900,
                "receiver_done": false,
            }),
            256,
        )
        .expect("early ack");
        let late_ack = parse_ack_value(
            &serde_json::json!({
                "packet_type": "native_pipeline_ack_v1",
                "expected_tx_total": 14400,
                "missing_count": 280,
                "missing_ranges_full_count": 1,
                "missing_ranges_sample": [{"start": 14120, "end_inclusive": 14399}],
                "novorudp_current_window_id": 220,
                "novorudp_current_window_start": 14120,
                "novorudp_current_window_end_inclusive": 14183,
                "novorudp_current_window_missing_count": 64,
                "novorudp_current_window_missing_ranges_sample": [{"start": 14120, "end_inclusive": 14183}],
                "ack_epoch": 901,
                "receiver_done": false,
            }),
            256,
        )
        .expect("late ack");

        let early = select_novorudp_repair_ranges_from_receiver_ack(
            &early_ack,
            14400,
            config.window_size,
            config.tail_window_max_retries,
        )
        .expect("early selection");
        let late = select_novorudp_repair_ranges_from_receiver_ack(
            &late_ack,
            14400,
            config.window_size,
            config.tail_window_max_retries,
        )
        .expect("late selection");

        assert_eq!(early.ranges[0].end_inclusive, 12215);
        assert_eq!(late.ranges[0].start, 14120);
        assert!(late_ack.latest_epoch > early_ack.latest_epoch);
        assert!(late_ack.latest_missing_count < early_ack.latest_missing_count);
    }

    #[test]
    fn novorudp_sender_finalization_times_out_when_receiver_never_done() {
        with_sender_hard_timeout_env(200, || {
            let chain_id = 92_001;
            let tx_count = 8;
            let sender_addr = reserve_udp_addr().expect("sender addr");
            let receiver_addr = reserve_udp_addr().expect("receiver addr");
            let ack_bind_addr = reserve_udp_addr().expect("ack bind addr");
            let ack_target_addr = ack_bind_addr.clone();
            let ack_thread = std::thread::spawn(move || {
                let socket = UdpSocket::bind("127.0.0.1:0").expect("ack sender socket");
                for epoch in 1..=32_u64 {
                    let ack = receiver_ack_report_value(tx_count, 4, 64, epoch);
                    let _ = socket.send_to(ack.to_string().as_bytes(), ack_target_addr.as_str());
                    std::thread::sleep(Duration::from_millis(10));
                }
            });

            let report = run_sender(
                chain_id,
                tx_count,
                1,
                2,
                sender_addr.as_str(),
                receiver_addr.as_str(),
                FaultConfigV1 {
                    enabled: false,
                    loss_bps: 0,
                    duplicate_bps: 0,
                    delay_ms: 0,
                    reorder_bps: 0,
                    seed: 0,
                },
                sender_timeout_sustained_config(tx_count),
                sender_timeout_tail_repair_config(),
                default_udp_send_retry_config(),
                UdpAckConfigV1 {
                    enabled: true,
                    bind_addr: ack_bind_addr,
                    target_addr: None,
                    recv_timeout_ms: 10,
                },
                sender_timeout_novorudp_config(),
            )
            .expect("sender must return a fail report instead of hanging");
            let _ = ack_thread.join();

            assert_eq!(report["accepted"].as_bool(), Some(false));
            assert_eq!(
                report["fail_reason"].as_str(),
                Some("receiver_repair_incomplete")
            );
            assert_eq!(report["report_written"].as_bool(), Some(true));
            assert_eq!(report["sender_hard_timeout_reached"].as_bool(), Some(false));
            assert_eq!(
                report["repair_progress_observed_count"]
                    .as_u64()
                    .unwrap_or_default()
                    > 0,
                true
            );
            assert!(
                report["tail_repair"]["tail_repair_udp_ack_received_count"]
                    .as_u64()
                    .unwrap_or_default()
                    > 0
            );
            assert_eq!(
                report["tail_repair"]["latest_ack_receiver_done"].as_bool(),
                Some(false)
            );
        });
    }

    #[test]
    fn novorudp_sender_writes_report_on_no_ack_timeout() {
        with_sender_hard_timeout_env(160, || {
            let chain_id = 92_002;
            let tx_count = 4;
            let sender_addr = reserve_udp_addr().expect("sender addr");
            let receiver_addr = reserve_udp_addr().expect("receiver addr");
            let ack_bind_addr = reserve_udp_addr().expect("ack bind addr");

            let report = run_sender(
                chain_id,
                tx_count,
                1,
                2,
                sender_addr.as_str(),
                receiver_addr.as_str(),
                FaultConfigV1 {
                    enabled: false,
                    loss_bps: 0,
                    duplicate_bps: 0,
                    delay_ms: 0,
                    reorder_bps: 0,
                    seed: 0,
                },
                sender_timeout_sustained_config(tx_count),
                sender_timeout_tail_repair_config(),
                default_udp_send_retry_config(),
                UdpAckConfigV1 {
                    enabled: true,
                    bind_addr: ack_bind_addr,
                    target_addr: None,
                    recv_timeout_ms: 10,
                },
                sender_timeout_novorudp_config(),
            )
            .expect("sender must return a no-ack fail report instead of hanging");

            assert_eq!(report["accepted"].as_bool(), Some(false));
            assert_eq!(
                report["fail_reason"].as_str(),
                Some("sender_finalization_timeout")
            );
            assert_eq!(report["report_written"].as_bool(), Some(true));
            assert_eq!(report["sender_hard_timeout_reached"].as_bool(), Some(true));
            assert_eq!(
                report["tail_repair"]["tail_repair_ack_received_count"].as_u64(),
                Some(0)
            );
        });
    }
}

fn read_missing_ranges_from_ack(path: &Path, limit: u64) -> Option<Vec<MissingRangeV1>> {
    let raw = fs::read_to_string(path).ok()?;
    let value = serde_json::from_str::<Value>(raw.as_str()).ok()?;
    let ranges = value
        .get("missing_ranges_sample")
        .and_then(Value::as_array)?
        .iter()
        .filter_map(|item| {
            let start = item.get("start").and_then(Value::as_u64)?;
            let end_inclusive = item.get("end_inclusive").and_then(Value::as_u64)?;
            if end_inclusive < start {
                return None;
            }
            Some(MissingRangeV1 {
                start,
                end_inclusive,
            })
        })
        .take(limit as usize)
        .collect::<Vec<_>>();
    Some(ranges)
}

fn missing_ranges_from_value_key(value: &Value, key: &str, limit: u64) -> Vec<MissingRangeV1> {
    value
        .get(key)
        .and_then(Value::as_array)
        .map(|items| {
            items
                .iter()
                .filter_map(|item| {
                    let start = item.get("start").and_then(Value::as_u64)?;
                    let end_inclusive = item.get("end_inclusive").and_then(Value::as_u64)?;
                    if end_inclusive < start {
                        return None;
                    }
                    Some(MissingRangeV1 {
                        start,
                        end_inclusive,
                    })
                })
                .take(limit as usize)
                .collect::<Vec<_>>()
        })
        .unwrap_or_default()
}

fn parse_ack_value(value: &Value, limit: u64) -> Option<UdpAckStateV1> {
    if value.get("packet_type").and_then(Value::as_str) != Some("native_pipeline_ack_v1")
        && value.get("schema").and_then(Value::as_str)
            != Some("novovm-native-pipeline-cross-machine-sustained-ack/v1")
    {
        return None;
    }
    let ranges = missing_ranges_from_value_key(value, "missing_ranges_sample", limit);
    let current_window_missing_ranges = missing_ranges_from_value_key(
        value,
        "novorudp_current_window_missing_ranges_sample",
        limit,
    );
    let current_window = value
        .get("novorudp_current_window_start")
        .and_then(Value::as_u64)
        .zip(
            value
                .get("novorudp_current_window_end_inclusive")
                .and_then(Value::as_u64),
        )
        .and_then(|(start, end_inclusive)| {
            (end_inclusive >= start).then_some(MissingRangeV1 {
                start,
                end_inclusive,
            })
        });
    Some(UdpAckStateV1 {
        received_count: 1,
        latest_epoch: value
            .get("ack_epoch")
            .and_then(Value::as_u64)
            .unwrap_or_default(),
        latest_missing_count: value
            .get("missing_count")
            .and_then(Value::as_u64)
            .unwrap_or_default(),
        missing_ranges_full_count: value
            .get("missing_ranges_full_count")
            .and_then(Value::as_u64)
            .unwrap_or_else(|| ranges.len().try_into().unwrap_or(u64::MAX)),
        highest_sequence_seen: value.get("highest_sequence_seen").and_then(Value::as_u64),
        latest_ranges: ranges,
        novorudp_current_window_id: value
            .get("novorudp_current_window_id")
            .and_then(Value::as_u64),
        novorudp_current_window: current_window,
        novorudp_current_window_missing_count: value
            .get("novorudp_current_window_missing_count")
            .and_then(Value::as_u64)
            .unwrap_or_else(|| missing_ranges_count(current_window_missing_ranges.as_slice())),
        novorudp_current_window_missing_ranges: current_window_missing_ranges,
        receiver_done: value
            .get("receiver_done")
            .and_then(Value::as_bool)
            .unwrap_or(false),
    })
}

fn drain_udp_ack_socket(socket: &UdpSocket, limit: u64, wait_ms: u64) -> UdpAckStateV1 {
    let started = Instant::now();
    let mut state = UdpAckStateV1::default();
    let mut buf = vec![0u8; 65_536];
    loop {
        match socket.recv_from(buf.as_mut_slice()) {
            Ok((len, _src)) => {
                if let Ok(value) = serde_json::from_slice::<Value>(&buf[..len]) {
                    if let Some(mut next) = parse_ack_value(&value, limit) {
                        next.received_count = state.received_count.saturating_add(1);
                        state = next;
                    }
                }
            }
            Err(err)
                if err.kind() == std::io::ErrorKind::WouldBlock
                    || err.kind() == std::io::ErrorKind::TimedOut =>
            {
                if started.elapsed() >= Duration::from_millis(wait_ms) {
                    break;
                }
                std::thread::sleep(Duration::from_millis(25));
            }
            Err(_) => break,
        }
    }
    state
}

fn build_tail_repair_payloads_from_ranges(
    chain_id: u64,
    ranges: &[MissingRangeV1],
    repair_round: u64,
) -> Result<Vec<NativeFixtureTxV1>> {
    let mut out = Vec::<NativeFixtureTxV1>::new();
    let copy_index = repair_round.saturating_add(1);
    for range in ranges {
        let count = range
            .end_inclusive
            .saturating_sub(range.start)
            .saturating_add(1);
        let mut txs = build_native_payloads_from_index(chain_id, range.start, count)?;
        for tx in &mut txs {
            tx.copy_index = copy_index;
            tx.dropped = false;
        }
        out.extend(txs);
    }
    Ok(out)
}

fn build_tail_repair_fallback_payloads(
    chain_id: u64,
    tx_count: u64,
    repair_round: u64,
    tail_window: u64,
) -> Result<Vec<NativeFixtureTxV1>> {
    if tail_window == 0 || tail_window >= tx_count {
        return build_tail_repair_payloads(chain_id, tx_count, repair_round);
    }
    let start = tx_count.saturating_sub(tail_window);
    let range = MissingRangeV1 {
        start,
        end_inclusive: tx_count.saturating_sub(1),
    };
    build_tail_repair_payloads_from_ranges(chain_id, &[range], repair_round)
}

fn loss_roll_bps(seed: u64, index: u64, copy_index: u64) -> u64 {
    let mut x = seed
        ^ index.wrapping_mul(0x9e37_79b9_7f4a_7c15)
        ^ copy_index.wrapping_mul(0xbf58_476d_1ce4_e5b9);
    x ^= x >> 30;
    x = x.wrapping_mul(0xbf58_476d_1ce4_e5b9);
    x ^= x >> 27;
    x = x.wrapping_mul(0x94d0_49bb_1331_11eb);
    x ^= x >> 31;
    x % 10_000
}

fn apply_fault_schedule(
    base: &[NativeFixtureTxV1],
    fault: FaultConfigV1,
) -> Vec<NativeFixtureTxV1> {
    if !fault.enabled {
        return base.to_vec();
    }
    let duplicate_all = fault.duplicate_bps >= 10_000;
    let mut scheduled = Vec::with_capacity(base.len().saturating_mul(2));
    for tx in base {
        let mut first = tx.clone();
        first.copy_index = 0;
        first.dropped =
            loss_roll_bps(fault.seed, first.index, first.copy_index) < fault.loss_bps.min(10_000);
        scheduled.push(first);

        let duplicate_this = duplicate_all
            || loss_roll_bps(fault.seed ^ 0xa11c_e55d, tx.index, 1)
                < fault.duplicate_bps.min(10_000);
        if duplicate_this {
            let mut dup = tx.clone();
            dup.copy_index = 1;
            dup.dropped =
                loss_roll_bps(fault.seed, dup.index, dup.copy_index) < fault.loss_bps.min(10_000);
            scheduled.push(dup);
        }
    }
    if fault.reorder_bps > 0 {
        let chunk = if fault.reorder_bps >= 10_000 { 4 } else { 8 };
        for part in scheduled.chunks_mut(chunk) {
            part.reverse();
        }
    }
    scheduled
}

fn receiver_child_expected_total_envs_v1(expected_tx_count: u64) -> [(&'static str, String); 2] {
    [
        (
            "NOVOVM_NATIVE_PIPELINE_TX_COUNT",
            expected_tx_count.to_string(),
        ),
        (
            "NOVOVM_NATIVE_EXECUTION_PIPELINE_EXPECTED_TX_COUNT",
            expected_tx_count.to_string(),
        ),
    ]
}

fn receiver_child_aoem_ownership_envs_v1() -> Vec<(&'static str, String)> {
    RECEIVER_CHILD_AOEM_OWNERSHIP_ENVS_V1
        .iter()
        .filter_map(|name| string_env_nonempty(name).map(|value| (*name, value)))
        .collect()
}

fn receiver_child_spawn_env_has_v1(name: &str) -> bool {
    receiver_child_aoem_ownership_envs_v1()
        .iter()
        .any(|(key, value)| *key == name && !value.trim().is_empty())
}

fn annotate_receiver_aoem_gate_trace_v1(summary: &mut Value) {
    if let Some(map) = summary.as_object_mut() {
        map.insert(
            "wrapper_env_aoem_production_candidate".to_string(),
            serde_json::json!(bool_env(
                NOV_NATIVE_AOEM_NATIVE_TX_BATCH_PRODUCTION_CANDIDATE_ENV
            )),
        );
        map.insert(
            "wrapper_env_aoem_shadow".to_string(),
            serde_json::json!(bool_env(NOV_NATIVE_AOEM_NATIVE_TX_BATCH_SHADOW_ENV)),
        );
        map.insert(
            "wrapper_env_aoem_compare".to_string(),
            serde_json::json!(bool_env(NOV_NATIVE_AOEM_NATIVE_TX_BATCH_COMPARE_ENV)),
        );
        map.insert(
            "child_spawn_env_aoem_production_candidate".to_string(),
            serde_json::json!(receiver_child_spawn_env_has_v1(
                NOV_NATIVE_AOEM_NATIVE_TX_BATCH_PRODUCTION_CANDIDATE_ENV,
            )),
        );
        map.insert(
            "child_spawn_env_aoem_shadow".to_string(),
            serde_json::json!(receiver_child_spawn_env_has_v1(
                NOV_NATIVE_AOEM_NATIVE_TX_BATCH_SHADOW_ENV,
            )),
        );
        map.insert(
            "child_spawn_env_aoem_compare".to_string(),
            serde_json::json!(receiver_child_spawn_env_has_v1(
                NOV_NATIVE_AOEM_NATIVE_TX_BATCH_COMPARE_ENV,
            )),
        );
    }
}

#[cfg(test)]
fn novorudp_receiver_expected_total_missing_v1(
    transport: &str,
    role: &str,
    expected_total: u64,
) -> bool {
    transport.eq_ignore_ascii_case("novorudp")
        && role.eq_ignore_ascii_case("receiver")
        && expected_total == 0
}

fn final_missing_without_expected_ledger_v1(
    final_missing_sequence_count: u64,
    ledger_expected_count: u64,
) -> bool {
    final_missing_sequence_count > 0 && ledger_expected_count == 0
}

fn aoem_runtime_worker_pipeline_enabled_env_v1() -> bool {
    bool_env("NOVOVM_AOEM_RUNTIME_WORKER_PIPELINE")
}

fn aoem_runtime_worker_pipeline_u64_env_v1(name: &str, default: u64) -> u64 {
    u64_env(name, default).unwrap_or(default).max(1)
}

fn spawn_receiver_node(
    node_bin: &Path,
    chain_id: u64,
    receiver_node: u64,
    listen_addr: &str,
    store_path: &Path,
    expected_tx_count: u64,
    max_ticks: u64,
    tick_interval_ms: u64,
    batch_budget: u64,
    recv_budget: u64,
) -> Result<Child> {
    let mut cmd = Command::new(node_bin);
    let pipeline_enabled = aoem_runtime_worker_pipeline_enabled_env_v1();
    let worker_tick_interval_ms = if pipeline_enabled {
        aoem_runtime_worker_pipeline_u64_env_v1(
            "NOVOVM_AOEM_RUNTIME_WORKER_LOOP_INTERVAL_MS",
            tick_interval_ms.min(10),
        )
    } else {
        tick_interval_ms
    };
    let worker_batch_budget = if pipeline_enabled {
        aoem_runtime_worker_pipeline_u64_env_v1(
            "NOVOVM_AOEM_RUNTIME_WORKER_BATCH_BUDGET",
            batch_budget.max(128),
        )
    } else {
        batch_budget
    };
    let worker_recv_budget = if pipeline_enabled {
        aoem_runtime_worker_pipeline_u64_env_v1(
            "NOVOVM_AOEM_RUNTIME_WORKER_RECV_BUDGET",
            recv_budget.max(512),
        )
    } else {
        recv_budget
    };
    let worker_time_slice_ms = if pipeline_enabled {
        aoem_runtime_worker_pipeline_u64_env_v1("NOVOVM_AOEM_RUNTIME_WORKER_TIME_SLICE_MS", 1_000)
    } else {
        250
    };
    let worker_max_ticks = if pipeline_enabled && worker_tick_interval_ms < tick_interval_ms {
        let requested_wall_ms = max_ticks.saturating_mul(tick_interval_ms);
        div_ceil_u64(requested_wall_ms, worker_tick_interval_ms).max(max_ticks)
    } else {
        max_ticks
    };
    cmd.env_clear();
    for (key, value) in std::env::vars() {
        if key == "NOVOVM_NODE_MODE"
            || key == "NOVOVM_NATIVE_SEND_RAW_TRANSACTION_PIPELINE_ONLY"
            || key.starts_with("NOVOVM_NATIVE_EXECUTION_")
            || key.starts_with("NOVOVM_NATIVE_PIPELINE_")
        {
            continue;
        }
        cmd.env(key, value);
    }
    let envs = [
        ("NOVOVM_NODE_MODE", "native_execution_pipeline".to_string()),
        (
            "NOVOVM_NATIVE_EXECUTION_TICK_CHAIN_ID",
            chain_id.to_string(),
        ),
        (
            "NOVOVM_NATIVE_EXECUTION_TICK_MAX_TICKS",
            worker_max_ticks.to_string(),
        ),
        (
            "NOVOVM_NATIVE_EXECUTION_TICK_INTERVAL_MS",
            worker_tick_interval_ms.to_string(),
        ),
        (
            "NOVOVM_NATIVE_EXECUTION_TICK_HARD_BUDGET",
            worker_batch_budget.to_string(),
        ),
        (
            "NOVOVM_NATIVE_EXECUTION_TICK_TARGET_BUDGET",
            worker_batch_budget.to_string(),
        ),
        (
            "NOVOVM_NATIVE_EXECUTION_TICK_EFFECTIVE_BUDGET",
            worker_batch_budget.to_string(),
        ),
        (
            "NOVOVM_NATIVE_EXECUTION_TICK_HARD_TIME_SLICE_MS",
            worker_time_slice_ms.to_string(),
        ),
        (
            "NOVOVM_NATIVE_EXECUTION_TICK_TARGET_TIME_SLICE_MS",
            worker_time_slice_ms.to_string(),
        ),
        (
            "NOVOVM_NATIVE_EXECUTION_TICK_EFFECTIVE_TIME_SLICE_MS",
            worker_time_slice_ms.to_string(),
        ),
        (
            "NOVOVM_NATIVE_EXECUTION_TICK_STORE_PATH",
            store_path.display().to_string(),
        ),
        (
            "NOVOVM_NATIVE_EXECUTION_STORE_BACKEND",
            "rocksdb".to_string(),
        ),
        (
            "NOVOVM_NATIVE_SEND_RAW_TRANSACTION_PIPELINE_ONLY",
            "true".to_string(),
        ),
        (
            "NOVOVM_NATIVE_EXECUTION_PIPELINE_REQUIRE_ROCKSDB_STORE",
            "false".to_string(),
        ),
        (
            "NOVOVM_NATIVE_EXECUTION_PIPELINE_UDP_ENABLED",
            "true".to_string(),
        ),
        (
            "NOVOVM_NATIVE_EXECUTION_PIPELINE_UDP_LISTEN_ADDR",
            listen_addr.to_string(),
        ),
        (
            "NOVOVM_NATIVE_EXECUTION_PIPELINE_LOCAL_NODE",
            receiver_node.to_string(),
        ),
        (
            "NOVOVM_NATIVE_EXECUTION_PIPELINE_UDP_RECV_BUDGET",
            worker_recv_budget.to_string(),
        ),
        (
            "NOVOVM_NATIVE_EXECUTION_PIPELINE_UDP_BROADCAST_ENABLED",
            "false".to_string(),
        ),
        (
            "NOVOVM_NATIVE_EXECUTION_PIPELINE_BROADCAST_ENABLED",
            "false".to_string(),
        ),
        (
            "NOVOVM_NATIVE_EXECUTION_PIPELINE_REQUIRE_PROGRESS",
            "false".to_string(),
        ),
        (
            "NOVOVM_NATIVE_EXECUTION_PIPELINE_QUIET_TICKS",
            "true".to_string(),
        ),
        (
            "NOVOVM_NATIVE_EXECUTION_PIPELINE_MIN_AOEM_EXECUTED",
            expected_tx_count.to_string(),
        ),
        (
            "NOVOVM_NATIVE_EXECUTION_PIPELINE_MIN_INGRESS_TOTAL",
            "0".to_string(),
        ),
        (
            "NOVOVM_NATIVE_EXECUTION_PIPELINE_MIN_INCLUDED_CANONICAL_TOTAL",
            "0".to_string(),
        ),
        (
            "NOVOVM_NATIVE_EXECUTION_PIPELINE_MAX_QUEUE_PENDING_LAST",
            "0".to_string(),
        ),
        (
            "NOVOVM_NATIVE_EXECUTION_TICK_SCAN_LIMIT",
            expected_tx_count
                .clamp(recv_budget.max(1), 65_536)
                .to_string(),
        ),
        (
            "NOVOVM_NATIVE_EXECUTION_PIPELINE_PROGRESS_REPORT_PATH",
            pipeline_progress_report_path(store_path)
                .display()
                .to_string(),
        ),
        (
            "NOVOVM_NATIVE_EXECUTION_PIPELINE_PROGRESS_REPORT_INTERVAL_MS",
            receiver_child_progress_report_interval_ms().to_string(),
        ),
        (
            "NOVOVM_NATIVE_EXECUTION_PIPELINE_EXIT_WHEN_SUMMARY_VALID",
            "true".to_string(),
        ),
    ];
    for (key, value) in envs {
        cmd.env(key, value);
    }
    for (key, value) in receiver_child_expected_total_envs_v1(expected_tx_count) {
        cmd.env(key, value);
    }
    for (key, value) in receiver_child_aoem_ownership_envs_v1() {
        cmd.env(key, value);
    }
    cmd.env("NOVOVM_NATIVE_PIPELINE_ROLE", "receiver");
    if let Some(transport) = string_env_nonempty("NOVOVM_NATIVE_PIPELINE_TRANSPORT") {
        cmd.env("NOVOVM_NATIVE_PIPELINE_TRANSPORT", transport);
    }
    for (_, env_name, _) in MEMORY_PROBE_TOGGLES_V1 {
        if let Some(value) = string_env_nonempty(env_name) {
            cmd.env(env_name, value);
        }
    }
    for legacy_env in [
        "NOVOVM_NATIVE_PIPELINE_DISABLE_PROOF_PROJECTION_FOR_MEMORY_PROBE",
        "NOVOVM_NATIVE_PIPELINE_DISABLE_CANONICAL_PROJECTION_FOR_MEMORY_PROBE",
        "NOVOVM_NATIVE_PIPELINE_DISABLE_REPORT_SERIALIZATION_FOR_MEMORY_PROBE",
        "NOVOVM_NATIVE_PIPELINE_DISABLE_RECOVERY_PROBE_FOR_MEMORY_PROBE",
    ] {
        if let Some(value) = string_env_nonempty(legacy_env) {
            cmd.env(legacy_env, value);
        }
    }
    for ack_env in [
        "NOVOVM_NATIVE_PIPELINE_UDP_ACK_ENABLED",
        "NOVOVM_NATIVE_PIPELINE_ACK_TARGET_ADDR",
        "NOVOVM_NATIVE_PIPELINE_SENDER_ACK_ADDR",
        "NOVOVM_NATIVE_PIPELINE_ACK_BIND_ADDR",
    ] {
        if let Some(value) = string_env_nonempty(ack_env) {
            cmd.env(ack_env, value);
        }
    }
    if let Some(peers) = string_env_nonempty("NOVOVM_NATIVE_PIPELINE_PEERS")
        .or_else(|| string_env_nonempty("NOVOVM_NATIVE_EXECUTION_PIPELINE_UDP_PEERS"))
    {
        cmd.env("NOVOVM_NATIVE_EXECUTION_PIPELINE_UDP_PEERS", peers);
    }
    cmd.stdout(Stdio::piped()).stderr(Stdio::piped());
    cmd.spawn().with_context(|| {
        format!(
            "spawn cross-machine receiver failed: bin={} listen_addr={listen_addr}",
            node_bin.display()
        )
    })
}

fn receiver_child_progress_report_interval_ms() -> u64 {
    let configured = u64_env("NOVOVM_NATIVE_PIPELINE_PROGRESS_SAMPLE_INTERVAL_MS", 5000)
        .unwrap_or(5000)
        .max(1);
    let profile = TransportProfileV1::from_env().unwrap_or(TransportProfileV1::NovoRudp);
    let novorudp = NovoRudpConfigV1::from_env(profile).unwrap_or(NovoRudpConfigV1 {
        enabled: true,
        window_size: 64,
        packet_copies: 2,
        tail_packet_copies: 3,
        batch_size: 16,
        batch_pause_ms: 10,
        window_ack_wait_ms: 1000,
        max_window_retries: 8,
        tail_window_max_retries: 16,
        tail_window_packet_copies: 6,
        tail_window_batch_size: 8,
        tail_window_batch_pause_ms: 20,
        tail_window_ack_wait_ms: 1500,
        ack_progress_interval_ms: 250,
        no_progress_backoff: true,
    });
    if novorudp.enabled {
        configured.min(novorudp.ack_progress_interval_ms.max(1))
    } else {
        configured
    }
}

fn parse_summary(output: Output, label: &str) -> Result<Value> {
    parse_summary_ref(&output, label)
}

fn parse_summary_ref(output: &Output, label: &str) -> Result<Value> {
    if !output.status.success() {
        bail!(
            "{label} failed: status={} stderr={}",
            output.status,
            String::from_utf8_lossy(&output.stderr)
        );
    }
    serde_json::from_slice::<Value>(output.stdout.as_slice()).with_context(|| {
        format!(
            "{label} did not return JSON summary: stdout={} stderr={}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        )
    })
}

fn run_receiver_node(
    node_bin: &Path,
    chain_id: u64,
    receiver_node: u64,
    listen_addr: &str,
    store_path: &Path,
    expected_tx_count: u64,
    max_ticks: u64,
    tick_interval_ms: u64,
    batch_budget: u64,
    recv_budget: u64,
) -> Result<Value> {
    let diagnostics = receiver_diagnostics_config()?;
    let mut child = spawn_receiver_node(
        node_bin,
        chain_id,
        receiver_node,
        listen_addr,
        store_path,
        expected_tx_count,
        max_ticks,
        tick_interval_ms,
        batch_budget,
        recv_budget,
    )?;
    if !diagnostics.enabled {
        return parse_summary(
            child
                .wait_with_output()
                .context("wait cross-machine receiver failed")?,
            "cross-machine receiver",
        );
    }

    let child_pid = child.id();
    let started_at = Instant::now();
    let mut last_sample_at = Instant::now()
        .checked_sub(Duration::from_millis(diagnostics.sample_interval_ms))
        .unwrap_or_else(Instant::now);
    let profile = TransportProfileV1::from_env()?;
    let novorudp = NovoRudpConfigV1::from_env(profile)?;
    let receiver_ack_progress_interval_ms = if novorudp.enabled {
        novorudp.ack_progress_interval_ms.max(1)
    } else {
        diagnostics.sample_interval_ms.max(1)
    };
    let mut last_ack_progress_at = Instant::now()
        .checked_sub(Duration::from_millis(receiver_ack_progress_interval_ms))
        .unwrap_or_else(Instant::now);
    let ack_sample_limit =
        u64_env("NOVOVM_NATIVE_PIPELINE_MISSING_SAMPLE_LIMIT", 256).unwrap_or(256);
    let mut receiver_ack_epoch = 0u64;
    let mut state = ReceiverDiagnosticsStateV1::default();
    let ledger_path = semantic_ledger_mirror_path(store_path);
    let progress_path = pipeline_progress_report_path(store_path);
    loop {
        if child
            .try_wait()
            .context("poll cross-machine receiver failed")?
            .is_some()
        {
            let output = child
                .wait_with_output()
                .context("wait cross-machine receiver failed")?;
            let (stdout_path, stderr_path, output_artifact_error) =
                persist_child_output_artifacts(&output);
            let summary_result = parse_summary_ref(&output, "cross-machine receiver");
            let mut summary = match summary_result {
                Ok(summary) => summary,
                Err(err) => {
                    let ledger_stats = semantic_ledger_stats(ledger_path.as_path());
                    let rocksdb_probe =
                        get_nov_native_execution_store_rocksdb_memory_probe_v1(store_path);
                    let memory_sample = if diagnostics.memory_sample_enabled {
                        process_memory_sample(child_pid)
                    } else {
                        serde_json::json!({})
                    };
                    let progress_summary = read_pipeline_progress_summary(progress_path.as_path());
                    let mut sample = if let Some(progress) = progress_summary.as_ref() {
                        diagnostics_summary_sample(
                            started_at,
                            progress,
                            ledger_stats,
                            rocksdb_probe,
                            memory_sample,
                            state.last_canonical,
                        )
                    } else {
                        serde_json::json!({
                            "elapsed_ms": started_at.elapsed().as_millis() as u64,
                            "stable_progress_total": state.last_canonical,
                            "aoem_executed_total": 0u64,
                            "queue_pending_last": 0u64,
                            "semantic_ledger_mirror": ledger_stats,
                            "rocksdb_memory_probe": rocksdb_probe,
                            "process_memory": memory_sample,
                        })
                    };
                    sample["child_exit_parse_error"] = serde_json::json!(err.to_string());
                    if let Some(error) = output_artifact_error.as_ref() {
                        sample["output_artifact_error"] = serde_json::json!(error);
                    }
                    annotate_receiver_ingress_drain_delta_v1(&mut sample, state.samples.last());
                    state.samples.push(sample);
                    if state.samples.len() > 256 {
                        let drop_count = state.samples.len().saturating_sub(256);
                        state.samples.drain(0..drop_count);
                        state.samples_dropped = state
                            .samples_dropped
                            .saturating_add(drop_count.try_into().unwrap_or(u64::MAX));
                    }
                    let reason = classify_child_exit_failure(&output, Some(&err));
                    state.fail_reason = Some(reason.clone());
                    write_diagnostics_report(
                        &diagnostics,
                        &state,
                        false,
                        child_pid,
                        expected_tx_count,
                    )?;
                    write_synthetic_receiver_failure_report(
                        expected_tx_count,
                        reason.as_str(),
                        &state,
                    )?;
                    write_receiver_exit_report(
                        child_pid,
                        Some(&output),
                        stdout_path.as_path(),
                        stderr_path.as_path(),
                        diagnostics.report_path.as_path(),
                        expected_tx_count,
                        None,
                        &state,
                        reason.as_str(),
                        false,
                        true,
                        false,
                    )?;
                    return Err(err);
                }
            };
            let ledger_stats = semantic_ledger_stats(ledger_path.as_path());
            let rocksdb_probe = get_nov_native_execution_store_rocksdb_memory_probe_v1(store_path);
            let memory_sample = if diagnostics.memory_sample_enabled {
                process_memory_sample(child_pid)
            } else {
                serde_json::json!({})
            };
            let mut sample = diagnostics_summary_sample(
                started_at,
                &summary,
                ledger_stats,
                rocksdb_probe,
                memory_sample,
                state.last_canonical,
            );
            let final_progress = summary_u64(&summary, "aoem_executed_total")
                .max(summary_u64(&summary, "included_canonical_total"))
                .max(
                    sample
                        .get("semantic_ledger_mirror")
                        .and_then(|value| value.get("line_count"))
                        .and_then(Value::as_u64)
                        .unwrap_or_default(),
                );
            let sample_limit = ack_sample_limit;
            let final_ack_start_epoch = next_receiver_ack_epoch(&mut receiver_ack_epoch);
            let final_ack_status = emit_receiver_progress_ack_with_summary(
                expected_tx_count,
                final_progress,
                sample_limit,
                final_ack_start_epoch,
                Some(&summary),
            );
            annotate_receiver_ack_send_status_v1(&mut summary, &final_ack_status);
            annotate_receiver_ack_send_status_v1(&mut sample, &final_ack_status);
            let final_ack_repeat_count =
                u64_env("NOVOVM_NATIVE_PIPELINE_FINAL_ACK_REPEAT_COUNT", 10).unwrap_or(10);
            let (final_ack_sent_count, final_ack_last_epoch) =
                if final_progress >= expected_tx_count {
                    repeat_final_receiver_udp_ack(
                        expected_tx_count,
                        sample_limit,
                        final_ack_start_epoch,
                    )
                } else {
                    (0, final_ack_start_epoch)
                };
            summary["final_ack_repeat_count"] = serde_json::json!(final_ack_repeat_count);
            summary["final_ack_sent_count"] = serde_json::json!(final_ack_sent_count);
            summary["final_ack_last_epoch"] = serde_json::json!(final_ack_last_epoch);
            sample["final_closed_child_sample"] = serde_json::json!(true);
            sample["final_closed_child_sample_available"] = serde_json::json!(true);
            sample["diagnostics_signoff_sample_source"] =
                serde_json::json!("final_closed_child_sample");
            sample["receiver_exit_phase"] = serde_json::json!("completed");
            annotate_receiver_ingress_drain_delta_v1(&mut sample, state.samples.last());
            state.samples.push(sample);
            write_diagnostics_report(&diagnostics, &state, true, child_pid, expected_tx_count)?;
            write_receiver_exit_report(
                child_pid,
                Some(&output),
                stdout_path.as_path(),
                stderr_path.as_path(),
                diagnostics.report_path.as_path(),
                expected_tx_count,
                Some(&summary),
                &state,
                "normal_pass",
                true,
                true,
                false,
            )?;
            return Ok(summary);
        }

        if last_ack_progress_at.elapsed()
            >= Duration::from_millis(receiver_ack_progress_interval_ms)
        {
            let progress_summary = read_pipeline_progress_summary(progress_path.as_path());
            let stable_progress = stable_progress_from_progress_summary(
                progress_summary.as_ref(),
                ledger_path.as_path(),
                state.last_canonical,
            );
            let epoch = next_receiver_ack_epoch(&mut receiver_ack_epoch);
            let _ack_status = emit_receiver_progress_ack_with_summary(
                expected_tx_count,
                stable_progress,
                ack_sample_limit,
                epoch,
                progress_summary.as_ref(),
            );
            last_ack_progress_at = Instant::now();
        }

        if last_sample_at.elapsed() >= Duration::from_millis(diagnostics.sample_interval_ms) {
            let ledger_stats = semantic_ledger_stats(ledger_path.as_path());
            let rocksdb_probe = live_receiver_child_rocksdb_memory_probe_v1(store_path);
            let memory_sample = if diagnostics.memory_sample_enabled {
                process_memory_sample(child_pid)
            } else {
                serde_json::json!({})
            };
            let progress_summary = read_pipeline_progress_summary(progress_path.as_path());
            let canonical = progress_summary
                .as_ref()
                .map(|summary| summary_u64(summary, "included_canonical_total"))
                .unwrap_or_else(|| {
                    ledger_stats
                        .get("line_count")
                        .and_then(Value::as_u64)
                        .unwrap_or_default()
                });
            let ledger_progress = ledger_stats
                .get("line_count")
                .and_then(Value::as_u64)
                .unwrap_or_default();
            let aoem_progress = progress_summary
                .as_ref()
                .map(|summary| summary_u64(summary, "aoem_executed_total"))
                .unwrap_or_default();
            let stable_progress = canonical.max(ledger_progress).max(aoem_progress);
            let delta = stable_progress.saturating_sub(state.last_canonical);
            let mut sample = if let Some(summary) = progress_summary.as_ref() {
                diagnostics_summary_sample(
                    started_at,
                    summary,
                    ledger_stats,
                    rocksdb_probe,
                    memory_sample,
                    state.last_canonical,
                )
            } else {
                serde_json::json!({
                "elapsed_ms": started_at.elapsed().as_millis() as u64,
                "received_unique_total": null,
                "canonical_unique_included_total": canonical,
                "stable_progress_total": stable_progress,
                "canonical_delta_since_last_sample": delta,
                "pending_count": null,
                "eligible_count": null,
                "skipped_ineligible_count": null,
                "skipped_already_receipted_count": null,
                "skipped_missing_payload_total": null,
                "skipped_non_native_payload_total": null,
                "skipped_chain_mismatch_total": null,
                "receipt_lookup_count": null,
                "receipt_lookup_hit_count": null,
                "receipt_lookup_miss_count": null,
                "receipt_lookup_elapsed_ms": null,
                "aoem_executed_total": stable_progress,
                "aoem_executed_delta": delta,
                "aoem_batch_elapsed_ms": null,
                "proof_items_total": null,
                "proof_delta": null,
                "proof_elapsed_ms": null,
                "commit_items_total": null,
                "commit_delta": null,
                "rocksdb_read_elapsed_ms": null,
                "rocksdb_write_elapsed_ms": null,
                "semantic_head_height": stable_progress,
                "semantic_head_monotonic": true,
                "semantic_ledger_mirror": ledger_stats,
                "rocksdb_memory_probe": rocksdb_probe,
                "process_memory": memory_sample,
                "queue_pending_last": null,
                "queue_dropped_total": null,
                "queue_rejected_total": null,
                })
            };
            sample["pipeline_progress_report_path"] =
                serde_json::json!(progress_path.display().to_string());
            let pending_count = sample
                .get("pending_count")
                .and_then(Value::as_u64)
                .unwrap_or_default();
            let waiting_for_sender = pending_count == 0 && stable_progress < expected_tx_count;
            let elapsed_ms = started_at.elapsed().as_millis() as u64;
            let receiver_phase = receiver_completion_phase_v1(
                &diagnostics,
                elapsed_ms,
                stable_progress,
                expected_tx_count,
                pending_count,
            );
            sample["waiting_for_sender"] = serde_json::json!(waiting_for_sender);
            sample["receiver_exit_phase"] = serde_json::json!(receiver_phase);
            sample["primary_send_completed"] = serde_json::json!(
                diagnostics.primary_send_duration_ms > 0
                    && elapsed_ms >= diagnostics.primary_send_duration_ms
            );
            sample["repair_convergence_started"] = serde_json::json!(
                diagnostics.primary_send_duration_ms > 0
                    && elapsed_ms >= diagnostics.primary_send_duration_ms
                    && stable_progress < expected_tx_count
            );
            sample["repair_convergence_completed"] =
                serde_json::json!(stable_progress >= expected_tx_count);
            sample["receiver_drain_completed"] =
                serde_json::json!(stable_progress >= expected_tx_count && pending_count == 0);
            sample["final_ack_received"] =
                serde_json::json!(stable_progress >= expected_tx_count && pending_count == 0);
            sample["absolute_timeout_reached"] = serde_json::json!(
                diagnostics.max_elapsed_ms > 0 && elapsed_ms >= diagnostics.max_elapsed_ms
            );
            sample["no_progress_timeout_reached"] = serde_json::json!(false);
            if delta == 0 && pending_count > 0 && stable_progress < expected_tx_count {
                state.stall_windows = state.stall_windows.saturating_add(1);
            } else {
                state.stall_windows = 0;
            }
            let working_set = memory_working_set_bytes(&sample["process_memory"]);
            if working_set > 0 {
                sample["process_working_set_bytes"] = serde_json::json!(working_set);
                if state.first_working_set_bytes.is_none() {
                    state.first_working_set_bytes = Some(working_set);
                }
                state.last_working_set_bytes = Some(working_set);
                if let Some(first) = state.first_working_set_bytes {
                    let elapsed_minutes = started_at.elapsed().as_secs().max(1) as f64 / 60.0;
                    let delta = working_set.saturating_sub(first) as f64;
                    sample["working_set_delta_per_minute"] =
                        serde_json::json!((delta / elapsed_minutes) as u64);
                }
            }
            let mut fail_reason = None;
            if diagnostics.max_working_set_bytes > 0
                && working_set > diagnostics.max_working_set_bytes
            {
                fail_reason = Some(format!(
                    "process_working_set_exceeded: working_set={} max={}",
                    working_set, diagnostics.max_working_set_bytes
                ));
            }
            if diagnostics.max_elapsed_ms > 0
                && elapsed_ms >= diagnostics.max_elapsed_ms
                && stable_progress < expected_tx_count
                && pending_count == 0
            {
                fail_reason = Some(format!(
                    "receiver_expected_tx_timeout: phase=failed_absolute_timeout progress={} expected={} elapsed_ms={} max_elapsed_ms={}",
                    stable_progress,
                    expected_tx_count,
                    elapsed_ms,
                    diagnostics.max_elapsed_ms
                ));
            }
            let epoch = next_receiver_ack_epoch(&mut receiver_ack_epoch);
            let ack_status = emit_receiver_progress_ack_with_summary(
                expected_tx_count,
                stable_progress,
                ack_sample_limit,
                epoch,
                progress_summary.as_ref(),
            );
            sample["novorudp_ack_progress_interval_ms"] =
                serde_json::json!(receiver_ack_progress_interval_ms);
            sample["novorudp_ack_epoch_after_sample"] = serde_json::json!(epoch);
            sample["receiver_ack_epoch"] = serde_json::json!(epoch);
            annotate_receiver_ack_send_status_v1(&mut sample, &ack_status);
            sample["receiver_ack_sent_count"] = serde_json::json!(ack_status.send_ok_count);
            if novorudp.enabled
                && stable_progress < expected_tx_count
                && ack_status.send_ok_count == 0
            {
                let reason = ack_status
                    .last_error
                    .clone()
                    .unwrap_or_else(|| "ack_send_ok_zero".to_string());
                sample["receiver_ack_backchannel_fail_reason"] = serde_json::json!(reason.clone());
                fail_reason = Some(format!("receiver_ack_backchannel_send_failed: {reason}"));
            }
            annotate_receiver_ingress_drain_delta_v1(&mut sample, state.samples.last());
            let pending_drain_stall_reason = sample
                .get("pipeline_pending_drain_stall_reason")
                .and_then(Value::as_str)
                .unwrap_or("none")
                .to_string();
            let pending_drain_active = pending_count > 0
                && stable_progress < expected_tx_count
                && pending_drain_stall_reason == "none";
            let pending_drain_idle_while_pending = pending_count > 0
                && stable_progress < expected_tx_count
                && pending_drain_stall_reason == "pending_drain_callsite_stall";
            let sample_delta_ms = sample_u64(&sample, "receiver_delta_elapsed_ms")
                .max(diagnostics.sample_interval_ms);
            if delta == 0 && pending_count > 0 && stable_progress < expected_tx_count {
                state.pending_drain_no_progress_ms = state
                    .pending_drain_no_progress_ms
                    .saturating_add(sample_delta_ms);
            } else {
                state.pending_drain_no_progress_ms = 0;
            }
            sample["pending_drain_callsite_active"] = serde_json::json!(pending_drain_active);
            sample["pending_drain_callsite_idle_while_pending"] =
                serde_json::json!(pending_drain_idle_while_pending);
            sample["pending_drain_no_progress_ms"] =
                serde_json::json!(state.pending_drain_no_progress_ms);
            sample["pending_nonzero_active_drain_enforced"] =
                serde_json::json!(pending_count > 0 && stable_progress < expected_tx_count);
            sample["pending_drain_scheduler_state"] =
                serde_json::json!(if pending_drain_idle_while_pending {
                    "idle_while_pending"
                } else if pending_drain_active {
                    "active"
                } else if pending_count > 0 {
                    "pending_backpressured"
                } else {
                    "idle_no_pending"
                });
            sample["pending_drain_wakeup_source"] = serde_json::json!(if pending_count > 0 {
                "pending_nonzero"
            } else {
                "none"
            });
            sample["pending_drain_blocker_reason"] = serde_json::json!(pending_drain_stall_reason);
            if diagnostics.min_canonical_delta > 0
                && delta < diagnostics.min_canonical_delta
                && pending_count > 0
                && stable_progress < expected_tx_count
            {
                fail_reason = Some(format!(
                    "canonical_progress_below_min_delta: delta={} min={}",
                    delta, diagnostics.min_canonical_delta
                ));
            }
            if state.stall_windows >= diagnostics.stall_windows
                && pending_count == 0
                && stable_progress < expected_tx_count
            {
                fail_reason = Some("canonical_progress_stall".to_string());
            }
            if state.pending_drain_no_progress_ms
                >= diagnostics.pending_drain_no_progress_timeout_ms
                && pending_count > 0
                && stable_progress < expected_tx_count
            {
                fail_reason = Some(format!(
                    "pipeline_pending_drain_stall:{}",
                    sample
                        .get("pipeline_pending_drain_stall_reason")
                        .and_then(Value::as_str)
                        .unwrap_or("unknown")
                ));
            }
            state.last_canonical = stable_progress;
            state.samples.push(sample);
            if state.samples.len() > 256 {
                let drop_count = state.samples.len().saturating_sub(256);
                state.samples.drain(0..drop_count);
                state.samples_dropped = state
                    .samples_dropped
                    .saturating_add(drop_count.try_into().unwrap_or(u64::MAX));
            }
            if receiver_phase == "completed" {
                if let Some(mut summary) = progress_summary {
                    let final_ack_start_epoch = next_receiver_ack_epoch(&mut receiver_ack_epoch);
                    let final_ack_status = emit_receiver_progress_ack_with_summary(
                        expected_tx_count,
                        stable_progress,
                        ack_sample_limit,
                        final_ack_start_epoch,
                        Some(&summary),
                    );
                    annotate_receiver_ack_send_status_v1(&mut summary, &final_ack_status);
                    let final_ack_repeat_count =
                        u64_env("NOVOVM_NATIVE_PIPELINE_FINAL_ACK_REPEAT_COUNT", 10).unwrap_or(10);
                    let (final_ack_sent_count, final_ack_last_epoch) =
                        repeat_final_receiver_udp_ack(
                            expected_tx_count,
                            ack_sample_limit,
                            final_ack_start_epoch,
                        );
                    summary["final_ack_repeat_count"] = serde_json::json!(final_ack_repeat_count);
                    summary["final_ack_sent_count"] = serde_json::json!(final_ack_sent_count);
                    summary["final_ack_last_epoch"] = serde_json::json!(final_ack_last_epoch);
                    let final_ledger_stats = semantic_ledger_stats(ledger_path.as_path());
                    let final_rocksdb_probe =
                        live_receiver_child_rocksdb_memory_probe_v1(store_path);
                    let final_memory_sample = if diagnostics.memory_sample_enabled {
                        process_memory_sample(child_pid)
                    } else {
                        serde_json::json!({})
                    };
                    let mut final_sample = diagnostics_summary_sample(
                        started_at,
                        &summary,
                        final_ledger_stats,
                        final_rocksdb_probe,
                        final_memory_sample,
                        state.last_canonical,
                    );
                    final_sample["pipeline_progress_report_path"] =
                        serde_json::json!(progress_path.display().to_string());
                    final_sample["receiver_exit_phase"] = serde_json::json!("completed");
                    final_sample["repair_convergence_completed"] = serde_json::json!(true);
                    final_sample["receiver_drain_completed"] = serde_json::json!(true);
                    final_sample["final_ack_received"] = serde_json::json!(true);
                    final_sample["final_closed_child_sample"] = serde_json::json!(true);
                    final_sample["final_closed_child_sample_available"] = serde_json::json!(true);
                    final_sample["diagnostics_signoff_sample_source"] =
                        serde_json::json!("final_closed_child_sample");
                    annotate_receiver_ingress_drain_delta_v1(
                        &mut final_sample,
                        state.samples.last(),
                    );
                    state.samples.push(final_sample);
                    let _ = child.kill();
                    let output = child
                        .wait_with_output()
                        .context("wait completed cross-machine receiver failed")?;
                    let (stdout_path, stderr_path, output_artifact_error) =
                        persist_child_output_artifacts(&output);
                    if let Some(error) = output_artifact_error {
                        if let Some(last) = state.samples.last_mut() {
                            last["output_artifact_error"] = serde_json::json!(error);
                        }
                    }
                    write_diagnostics_report(
                        &diagnostics,
                        &state,
                        true,
                        child_pid,
                        expected_tx_count,
                    )?;
                    write_receiver_exit_report(
                        child_pid,
                        Some(&output),
                        stdout_path.as_path(),
                        stderr_path.as_path(),
                        diagnostics.report_path.as_path(),
                        expected_tx_count,
                        Some(&summary),
                        &state,
                        "completed_live_summary",
                        true,
                        true,
                        true,
                    )?;
                    return Ok(summary);
                }
            }
            if let Some(reason) = fail_reason {
                state.fail_reason = Some(reason.clone());
                let _ = child.kill();
                let output = child
                    .wait_with_output()
                    .context("wait killed cross-machine receiver failed")?;
                let (stdout_path, stderr_path, output_artifact_error) =
                    persist_child_output_artifacts(&output);
                if let Some(error) = output_artifact_error {
                    if let Some(last) = state.samples.last_mut() {
                        last["output_artifact_error"] = serde_json::json!(error);
                    }
                }
                write_diagnostics_report(
                    &diagnostics,
                    &state,
                    false,
                    child_pid,
                    expected_tx_count,
                )?;
                write_synthetic_receiver_failure_report(
                    expected_tx_count,
                    reason.as_str(),
                    &state,
                )?;
                write_receiver_exit_report(
                    child_pid,
                    Some(&output),
                    stdout_path.as_path(),
                    stderr_path.as_path(),
                    diagnostics.report_path.as_path(),
                    expected_tx_count,
                    None,
                    &state,
                    reason.as_str(),
                    false,
                    true,
                    true,
                )?;
                bail!("cross-machine receiver diagnostics failed: {reason}");
            }
            write_diagnostics_report(&diagnostics, &state, false, child_pid, expected_tx_count)?;
            last_sample_at = Instant::now();
        }
        std::thread::sleep(Duration::from_millis(100));
    }
}

fn summary_u64(summary: &Value, field: &str) -> u64 {
    summary
        .get(field)
        .and_then(Value::as_u64)
        .unwrap_or_default()
}

fn summary_str<'a>(summary: &'a Value, field: &str) -> &'a str {
    summary.get(field).and_then(Value::as_str).unwrap_or("-")
}

fn probe_u64(probe: &Value, field: &str) -> u64 {
    probe.get(field).and_then(Value::as_u64).unwrap_or_default()
}

fn semantic_sequence(probe: &Value) -> u64 {
    probe
        .get("semantic_head")
        .and_then(|value| value.get("sequence"))
        .and_then(Value::as_u64)
        .unwrap_or_default()
}

fn write_report(path: &Path, report: &Value) -> Result<()> {
    if let Some(parent) = path.parent() {
        if !parent.as_os_str().is_empty() {
            fs::create_dir_all(parent).with_context(|| {
                format!("create cross-machine report dir: {}", parent.display())
            })?;
        }
    }
    let encoded =
        serde_json::to_string_pretty(report).context("encode cross-machine report failed")?;
    fs::write(path, encoded)
        .with_context(|| format!("write cross-machine report failed: {}", path.display()))
}

fn write_receiver_ack_report(
    expected_tx_count: u64,
    stable_progress: u64,
    sample_limit: u64,
    ack_epoch: u64,
) -> Result<()> {
    let report =
        receiver_ack_report_value(expected_tx_count, stable_progress, sample_limit, ack_epoch);
    write_report(ack_report_path().as_path(), &report)
}

fn write_receiver_ack_report_with_summary(
    expected_tx_count: u64,
    stable_progress: u64,
    sample_limit: u64,
    ack_epoch: u64,
    progress_summary: Option<&Value>,
) -> Result<()> {
    let report = receiver_ack_report_value_with_summary(
        expected_tx_count,
        stable_progress,
        sample_limit,
        ack_epoch,
        progress_summary,
    );
    write_report(ack_report_path().as_path(), &report)
}

#[allow(clippy::too_many_arguments)]
fn write_sender_live_progress_report_v1(
    path: &Path,
    elapsed_ms: u64,
    tx_count: u64,
    sent_unique_target: u64,
    sent_packets: u64,
    send_failed_count: u64,
    primary_ack_drain_count: u64,
    primary_ack_received_count: u64,
    primary_ack_drain_empty_count: u64,
    primary_ack_last_consumed_elapsed_ms: u64,
    latest_ack_epoch: u64,
    latest_ack_missing_count: Option<u64>,
    latest_ack_highest_sequence_seen: Option<u64>,
    latest_ack_receiver_done: bool,
    sender_completed: bool,
) -> Result<()> {
    let elapsed_seconds = elapsed_ms as f64 / 1000.0;
    let current_send_rate_tps = if elapsed_seconds > 0.0 {
        sent_unique_target as f64 / elapsed_seconds
    } else {
        0.0
    };
    let last_sent_sequence = sent_unique_target.checked_sub(1);
    write_report(
        path,
        &serde_json::json!({
            "schema": REPORT_SCHEMA_V1,
            "role": "sender",
            "report_type": "sender_live_progress_v1",
            "elapsed_ms": elapsed_ms,
            "tx_count": tx_count,
            "sender_round_count": sent_unique_target,
            "primary_sent_count": sent_unique_target,
            "last_sent_sequence": last_sent_sequence,
            "send_packet_count": sent_packets,
            "send_failed_count": send_failed_count,
            "sender_completed": sender_completed,
            "sender_hard_timeout_reached": false,
            "last_send_at_ms": elapsed_ms,
            "current_send_rate_tps": current_send_rate_tps,
            "primary_ack_drain_count": primary_ack_drain_count,
            "primary_ack_received_count": primary_ack_received_count,
            "primary_ack_drain_empty_count": primary_ack_drain_empty_count,
            "primary_ack_last_consumed_elapsed_ms": primary_ack_last_consumed_elapsed_ms,
            "latest_ack_epoch": latest_ack_epoch,
            "latest_ack_missing_count": latest_ack_missing_count,
            "latest_ack_highest_sequence_seen": latest_ack_highest_sequence_seen,
            "latest_ack_receiver_done": latest_ack_receiver_done,
        }),
    )
}

fn receiver_ack_report_value(
    expected_tx_count: u64,
    stable_progress: u64,
    sample_limit: u64,
    ack_epoch: u64,
) -> Value {
    receiver_ack_report_value_with_summary(
        expected_tx_count,
        stable_progress,
        sample_limit,
        ack_epoch,
        None,
    )
}

fn receiver_ack_report_value_with_summary(
    expected_tx_count: u64,
    stable_progress: u64,
    sample_limit: u64,
    ack_epoch: u64,
    progress_summary: Option<&Value>,
) -> Value {
    let progress_summary_ranges = progress_summary
        .map(|summary| {
            missing_ranges_from_value_key(summary, "missing_ranges_sample", sample_limit)
        })
        .unwrap_or_default();
    let progress_summary_missing_count = progress_summary
        .and_then(|summary| summary.get("missing_count"))
        .and_then(Value::as_u64);
    let progress_summary_available = progress_summary.is_some();
    let progress_summary_missing_available =
        progress_summary_missing_count.is_some() && !progress_summary_ranges.is_empty();
    let ledger_ranges = progress_summary
        .map(|summary| {
            missing_ranges_from_value_key(
                summary,
                "ledger_durable_missing_ranges_sample",
                sample_limit,
            )
        })
        .unwrap_or_default();
    let ledger_missing_count = progress_summary
        .and_then(|summary| summary.get("ledger_durable_missing_count"))
        .and_then(Value::as_u64)
        .filter(|count| *count > 0)
        .unwrap_or_else(|| missing_ranges_count(ledger_ranges.as_slice()));
    let ledger_summary_available = progress_summary.is_some_and(|summary| {
        summary.get("ledger_durable_missing_count").is_some()
            || summary
                .get("ledger_durable_missing_ranges_sample")
                .is_some()
            || summary
                .get("ledger_durable_missing_bitmap_available")
                .is_some()
    });
    let ledger_missing_bitmap_available = ledger_missing_count > 0 && !ledger_ranges.is_empty();
    let (ranges, missing_count, missing_bitmap_source, fallback_reason, source_reason) =
        if progress_summary_missing_count == Some(0) {
            (
                Vec::new(),
                0,
                "progress_summary",
                Value::Null,
                "progress_summary_missing_zero",
            )
        } else if progress_summary_missing_available {
            (
                progress_summary_ranges,
                progress_summary_missing_count.unwrap_or_default(),
                "progress_summary",
                Value::Null,
                "progress_summary_missing_bitmap",
            )
        } else if ledger_missing_bitmap_available {
            (
                ledger_ranges,
                ledger_missing_count,
                "ledger",
                Value::Null,
                "ledger_durable_missing_bitmap",
            )
        } else {
            let reason = if progress_summary.is_none() {
                "progress_summary_unavailable"
            } else if ledger_summary_available {
                "ledger_missing_bitmap_unavailable"
            } else {
                "no_missing_bitmap_fields_available"
            };
            (
                missing_ranges_from_progress(stable_progress, expected_tx_count, sample_limit),
                expected_tx_count.saturating_sub(stable_progress),
                "stable_progress_fallback",
                serde_json::json!(reason),
                "stable_progress_fallback",
            )
        };
    let missing_ranges_full_count = ranges.len();
    let progress_summary_last_updated_ms = progress_summary
        .and_then(|summary| {
            summary
                .get("timestamp_ms")
                .or_else(|| summary.get("last_updated_ms"))
                .or_else(|| summary.get("elapsed_ms"))
        })
        .and_then(Value::as_u64);
    let ledger_final_missing_count = progress_summary
        .and_then(|summary| summary.get("ledger_durable_missing_count"))
        .and_then(Value::as_u64)
        .unwrap_or_default();
    let ledger_candidate_count = progress_summary
        .and_then(|summary| summary.get("ledger_final_missing_candidate_count"))
        .and_then(Value::as_u64)
        .unwrap_or_default();
    let ack_fallback_due_to_candidate_empty_count = if missing_bitmap_source
        == "stable_progress_fallback"
        && ledger_summary_available
        && ledger_candidate_count == 0
    {
        1u64
    } else {
        0u64
    };
    let progress_summary_missing_count_value = progress_summary_missing_count.unwrap_or_default();
    let transport_profile = TransportProfileV1::from_env().unwrap_or(TransportProfileV1::NovoRudp);
    let novorudp = NovoRudpConfigV1::from_env(transport_profile).unwrap_or(NovoRudpConfigV1 {
        enabled: true,
        window_size: 64,
        packet_copies: 2,
        tail_packet_copies: 3,
        batch_size: 16,
        batch_pause_ms: 10,
        window_ack_wait_ms: 1000,
        max_window_retries: 8,
        tail_window_max_retries: 16,
        tail_window_packet_copies: 6,
        tail_window_batch_size: 8,
        tail_window_batch_pause_ms: 20,
        tail_window_ack_wait_ms: 1500,
        ack_progress_interval_ms: 250,
        no_progress_backoff: true,
    });
    let current_window = first_missing_window_ranges(
        ranges.as_slice(),
        expected_tx_count,
        novorudp.window_size.max(1),
    );
    let highest_sequence_seen = if stable_progress == 0 {
        Value::Null
    } else {
        serde_json::json!(stable_progress.saturating_sub(1))
    };
    serde_json::json!({
        "schema": "novovm-native-pipeline-cross-machine-sustained-ack/v1",
        "packet_type": "native_pipeline_ack_v1",
        "expected_tx_total": expected_tx_count,
        "received_unique_count": stable_progress,
        "canonical_unique_included": stable_progress,
        "receipt_count": stable_progress,
        "highest_sequence_seen": highest_sequence_seen,
        "missing_count": missing_count,
        "missing_ranges_full_count": missing_ranges_full_count,
        "missing_ranges_sample_truncated": (missing_ranges_full_count as u64) > sample_limit,
        "missing_ranges_sample": missing_ranges_to_json(ranges.as_slice(), sample_limit),
        "missing_bitmap_source": missing_bitmap_source,
        "missing_bitmap_fallback_reason": fallback_reason,
        "ledger_summary_available": ledger_summary_available,
        "progress_summary_available": progress_summary_available,
        "progress_summary_last_updated_ms": progress_summary_last_updated_ms,
        "ledger_missing_bitmap_available": ledger_missing_bitmap_available,
        "ack_used_durable_ledger_missing_bitmap": missing_bitmap_source == "ledger",
        "ack_fallback_due_to_candidate_empty_count": ack_fallback_due_to_candidate_empty_count,
        "ack_source_selection_reason": source_reason,
        "ledger_final_missing_count": ledger_final_missing_count,
        "progress_summary_missing_count": progress_summary_missing_count_value,
        "ack_epoch": ack_epoch,
        "timestamp_ms": now_ms(),
        "receiver_done": missing_count == 0 && stable_progress >= expected_tx_count,
        "transport_profile": transport_profile.as_str(),
        "novorudp_enabled": novorudp.enabled,
        "novorudp_window_size": novorudp.window_size,
        "novorudp_current_window_id": current_window.as_ref().map(|(id, _, _)| *id),
        "novorudp_current_window_start": current_window.as_ref().map(|(_, window, _)| window.start),
        "novorudp_current_window_end_inclusive": current_window.as_ref().map(|(_, window, _)| window.end_inclusive),
        "novorudp_current_window_missing_count": current_window
            .as_ref()
            .map(|(_, _, window_ranges)| missing_ranges_count(window_ranges.as_slice()))
            .unwrap_or(0),
        "novorudp_current_window_missing_ranges_sample": current_window
            .as_ref()
            .map(|(_, _, window_ranges)| missing_ranges_to_json(window_ranges.as_slice(), sample_limit))
            .unwrap_or_else(|| serde_json::json!([])),
    })
}

fn send_receiver_udp_ack(
    expected_tx_count: u64,
    stable_progress: u64,
    sample_limit: u64,
    ack_epoch: u64,
) -> ReceiverAckSendStatusV1 {
    send_receiver_udp_ack_with_summary(
        expected_tx_count,
        stable_progress,
        sample_limit,
        ack_epoch,
        None,
    )
}

fn send_receiver_udp_ack_with_summary(
    expected_tx_count: u64,
    stable_progress: u64,
    sample_limit: u64,
    ack_epoch: u64,
    progress_summary: Option<&Value>,
) -> ReceiverAckSendStatusV1 {
    let Some(target_addr) = first_string_env_nonempty(&[
        "NOVOVM_NATIVE_PIPELINE_ACK_TARGET_ADDR",
        "NOVOVM_NATIVE_PIPELINE_SENDER_ACK_ADDR",
    ]) else {
        let status = ReceiverAckSendStatusV1 {
            enabled: true,
            missing_target_count: 1,
            last_error: Some("ack_target_missing".to_string()),
            ..Default::default()
        };
        let mut report = receiver_ack_report_value_with_summary(
            expected_tx_count,
            stable_progress,
            sample_limit,
            ack_epoch,
            progress_summary,
        );
        annotate_receiver_ack_send_status_v1(&mut report, &status);
        let _ = write_report(ack_report_path().as_path(), &report);
        return status;
    };
    let enabled = bool_env("NOVOVM_NATIVE_PIPELINE_UDP_ACK_ENABLED")
        || string_env_nonempty("NOVOVM_NATIVE_PIPELINE_UDP_ACK_ENABLED").is_none();
    if !enabled {
        let status = ReceiverAckSendStatusV1 {
            enabled: false,
            target_addr: Some(target_addr),
            last_error: Some("ack_disabled".to_string()),
            ..Default::default()
        };
        let mut report = receiver_ack_report_value_with_summary(
            expected_tx_count,
            stable_progress,
            sample_limit,
            ack_epoch,
            progress_summary,
        );
        annotate_receiver_ack_send_status_v1(&mut report, &status);
        let _ = write_report(ack_report_path().as_path(), &report);
        return status;
    }
    let mut report = receiver_ack_report_value_with_summary(
        expected_tx_count,
        stable_progress,
        sample_limit,
        ack_epoch,
        progress_summary,
    );
    let Ok(payload) = serde_json::to_vec(&report) else {
        let status = ReceiverAckSendStatusV1 {
            enabled: true,
            target_addr: Some(target_addr),
            last_error: Some("ack_payload_encode_failed".to_string()),
            ..Default::default()
        };
        annotate_receiver_ack_send_status_v1(&mut report, &status);
        let _ = write_report(ack_report_path().as_path(), &report);
        return status;
    };
    let bind_addr = first_string_env_nonempty(&["NOVOVM_NATIVE_PIPELINE_ACK_BIND_ADDR"])
        .unwrap_or_else(|| "0.0.0.0:0".to_string());
    let Ok(socket) = UdpSocket::bind(bind_addr.as_str()) else {
        let status = ReceiverAckSendStatusV1 {
            enabled: true,
            bind_addr: Some(bind_addr),
            target_addr: Some(target_addr),
            bind_error_count: 1,
            last_error: Some("ack_bind_failed".to_string()),
            ..Default::default()
        };
        annotate_receiver_ack_send_status_v1(&mut report, &status);
        let _ = write_report(ack_report_path().as_path(), &report);
        return status;
    };
    let local_addr = socket.local_addr().ok().map(|addr| addr.to_string());
    let mut status = ReceiverAckSendStatusV1 {
        enabled: true,
        attempted_count: 1,
        bind_addr: Some(bind_addr),
        target_addr: Some(target_addr.clone()),
        local_addr,
        ..Default::default()
    };
    match socket.send_to(payload.as_slice(), target_addr.as_str()) {
        Ok(_) => status.send_ok_count = 1,
        Err(err) => {
            status.send_error_count = 1;
            status.last_error = Some(err.to_string());
        }
    }
    annotate_receiver_ack_send_status_v1(&mut report, &status);
    let _ = write_report(ack_report_path().as_path(), &report);
    status
}

fn annotate_receiver_ack_send_status_v1(value: &mut Value, status: &ReceiverAckSendStatusV1) {
    value["receiver_ack_backchannel_enabled"] = serde_json::json!(status.enabled);
    value["receiver_ack_target_addr"] = status
        .target_addr
        .as_ref()
        .map(|addr| serde_json::json!(addr))
        .unwrap_or(Value::Null);
    value["receiver_ack_bind_addr"] = status
        .bind_addr
        .as_ref()
        .map(|addr| serde_json::json!(addr))
        .unwrap_or(Value::Null);
    value["receiver_ack_local_addr"] = status
        .local_addr
        .as_ref()
        .map(|addr| serde_json::json!(addr))
        .unwrap_or(Value::Null);
    value["receiver_ack_packet_attempted_count"] = serde_json::json!(status.attempted_count);
    value["receiver_ack_packet_sent_count"] = serde_json::json!(status.send_ok_count);
    value["receiver_ack_send_ok_count"] = serde_json::json!(status.send_ok_count);
    value["receiver_ack_send_error_count"] = serde_json::json!(status.send_error_count);
    value["receiver_ack_missing_target_count"] = serde_json::json!(status.missing_target_count);
    value["receiver_ack_bind_error_count"] = serde_json::json!(status.bind_error_count);
    value["receiver_ack_last_send_error"] = status
        .last_error
        .as_ref()
        .map(|err| serde_json::json!(err))
        .unwrap_or(Value::Null);
}

fn next_receiver_ack_epoch(epoch: &mut u64) -> u64 {
    *epoch = epoch.saturating_add(1);
    *epoch
}

fn stable_progress_from_progress_summary(
    progress_summary: Option<&Value>,
    ledger_path: &Path,
    fallback_progress: u64,
) -> u64 {
    let ledger_progress = semantic_ledger_stats(ledger_path)
        .get("line_count")
        .and_then(Value::as_u64)
        .unwrap_or_default();
    let canonical = progress_summary
        .map(|summary| summary_u64(summary, "included_canonical_total"))
        .unwrap_or_default();
    let aoem = progress_summary
        .map(|summary| summary_u64(summary, "aoem_executed_total"))
        .unwrap_or_default();
    canonical
        .max(ledger_progress)
        .max(aoem)
        .max(fallback_progress)
}

fn emit_receiver_progress_ack_with_summary(
    expected_tx_count: u64,
    stable_progress: u64,
    sample_limit: u64,
    ack_epoch: u64,
    progress_summary: Option<&Value>,
) -> ReceiverAckSendStatusV1 {
    let _ = write_receiver_ack_report_with_summary(
        expected_tx_count,
        stable_progress,
        sample_limit,
        ack_epoch,
        progress_summary,
    );
    send_receiver_udp_ack_with_summary(
        expected_tx_count,
        stable_progress,
        sample_limit,
        ack_epoch,
        progress_summary,
    )
}

fn repeat_final_receiver_udp_ack(
    expected_tx_count: u64,
    sample_limit: u64,
    start_epoch: u64,
) -> (u64, u64) {
    let repeat_count = u64_env("NOVOVM_NATIVE_PIPELINE_FINAL_ACK_REPEAT_COUNT", 10).unwrap_or(10);
    let repeat_interval_ms =
        u64_env("NOVOVM_NATIVE_PIPELINE_FINAL_ACK_REPEAT_INTERVAL_MS", 500).unwrap_or(500);
    let mut sent = 0u64;
    let mut last_epoch = start_epoch;
    for offset in 0..repeat_count {
        let epoch = start_epoch.saturating_add(offset).saturating_add(1);
        let _ =
            write_receiver_ack_report(expected_tx_count, expected_tx_count, sample_limit, epoch);
        let status =
            send_receiver_udp_ack(expected_tx_count, expected_tx_count, sample_limit, epoch);
        sent = sent.saturating_add(status.send_ok_count);
        last_epoch = epoch;
        if repeat_interval_ms > 0 && offset + 1 < repeat_count {
            std::thread::sleep(Duration::from_millis(repeat_interval_ms));
        }
    }
    (sent, last_epoch)
}

fn write_artifact_bytes(path: &Path, bytes: &[u8]) -> Result<()> {
    if let Some(parent) = path.parent() {
        if !parent.as_os_str().is_empty() {
            fs::create_dir_all(parent)
                .with_context(|| format!("create artifact dir failed: {}", parent.display()))?;
        }
    }
    fs::write(path, bytes).with_context(|| format!("write artifact failed: {}", path.display()))
}

fn persist_child_output_artifacts(output: &Output) -> (PathBuf, PathBuf, Option<String>) {
    let stdout_path = receiver_stdout_log_path();
    let stderr_path = receiver_stderr_log_path();
    let mut error = None;
    if let Err(err) = write_artifact_bytes(stdout_path.as_path(), output.stdout.as_slice()) {
        error = Some(format!("stdout_log_write_failed: {err}"));
    }
    if let Err(err) = write_artifact_bytes(stderr_path.as_path(), output.stderr.as_slice()) {
        let item = format!("stderr_log_write_failed: {err}");
        error = Some(error.map_or(item.clone(), |prev| format!("{prev}; {item}")));
    }
    (stdout_path, stderr_path, error)
}

fn child_exit_status_json(output: &Output) -> Value {
    serde_json::json!({
        "success": output.status.success(),
        "code": output.status.code(),
        "status": output.status.to_string(),
    })
}

fn classify_child_exit_failure(output: &Output, parse_error: Option<&anyhow::Error>) -> String {
    let stderr = String::from_utf8_lossy(output.stderr.as_slice()).to_ascii_lowercase();
    if stderr.contains("panicked") || stderr.contains("panic") {
        return "child_panic".to_string();
    }
    if stderr.contains("failed to create lock file")
        && stderr.contains("rocksdb")
        && stderr.contains("lock")
    {
        return "rocksdb_lock_conflict".to_string();
    }
    if stderr.contains("open nov native execution rocksdb failed") && stderr.contains("lock") {
        return "rocksdb_lock_conflict".to_string();
    }
    if !output.status.success() {
        return "child_nonzero_exit".to_string();
    }
    if parse_error.is_some() {
        return "child_early_exit_no_report".to_string();
    }
    "child_early_exit_no_report".to_string()
}

fn output_stderr_tail(output: Option<&Output>, max_chars: usize) -> Option<String> {
    output.map(|out| {
        let stderr = String::from_utf8_lossy(out.stderr.as_slice());
        let chars: Vec<char> = stderr.chars().collect();
        let start = chars.len().saturating_sub(max_chars);
        chars[start..].iter().collect::<String>()
    })
}

fn live_receiver_child_rocksdb_memory_probe_v1(store_path: &Path) -> Value {
    let rocksdb_path = nov_native_execution_store_rocksdb_path_v1(store_path);
    serde_json::json!({
        "method": "nov_getNativeExecutionStoreRocksDbMemoryProbe",
        "rocksdb_path": rocksdb_path.display().to_string(),
        "rocksdb_exists": rocksdb_path.exists(),
        "rocksdb_opened": false,
        "rocksdb_probe_skipped": true,
        "rocksdb_probe_skipped_reason": "live_receiver_child_holds_lock",
        "rocksdb_total_estimated_memory_bytes": 0u64,
        "rocksdb_block_cache_estimated_bytes": 0u64,
        "rocksdb_memtable_estimated_bytes": 0u64,
        "rocksdb_index_filter_estimated_bytes": 0u64,
        "rocksdb_memory_probe_supported": false,
    })
}

fn write_receiver_exit_report(
    child_pid: u32,
    output: Option<&Output>,
    stdout_path: &Path,
    stderr_path: &Path,
    diagnostics_path: &Path,
    expected_tx_count: u64,
    summary: Option<&Value>,
    state: &ReceiverDiagnosticsStateV1,
    fail_reason: &str,
    final_report_written: bool,
    diagnostics_report_written: bool,
    child_was_killed: bool,
) -> Result<()> {
    let last_sample = state.samples.last();
    let stable_progress_total = last_sample
        .and_then(|sample| sample.get("stable_progress_total"))
        .and_then(Value::as_u64)
        .or_else(|| summary.map(|value| summary_u64(value, "aoem_executed_total")))
        .unwrap_or_default();
    let aoem_executed_total = last_sample
        .and_then(|sample| sample.get("aoem_executed_total"))
        .and_then(Value::as_u64)
        .or_else(|| summary.map(|value| summary_u64(value, "aoem_executed_total")))
        .unwrap_or_default();
    let queue_pending_last = last_sample
        .and_then(|sample| sample.get("queue_pending_last"))
        .and_then(Value::as_u64)
        .or_else(|| summary.map(|value| summary_u64(value, "queue_pending_last")))
        .unwrap_or_default();
    let child_panicked_detected = output
        .map(|out| {
            let stderr = String::from_utf8_lossy(out.stderr.as_slice()).to_ascii_lowercase();
            stderr.contains("panic") || stderr.contains("panicked")
        })
        .unwrap_or(false);
    let report = serde_json::json!({
        "schema": "novovm-native-pipeline-receiver-exit-forensics/v1",
        "child_pid": child_pid,
        "child_exit": output.map(child_exit_status_json).unwrap_or(serde_json::Value::Null),
        "child_exit_code": output.and_then(|out| out.status.code()),
        "child_exit_status": output.map(|out| out.status.to_string()),
        "child_was_killed": child_was_killed,
        "child_panicked_detected": child_panicked_detected,
        "child_stderr_tail": output_stderr_tail(output, 4096),
        "stdout_path": stdout_path.display().to_string(),
        "stderr_path": stderr_path.display().to_string(),
        "diagnostics_path": diagnostics_path.display().to_string(),
        "final_report_written": final_report_written,
        "diagnostics_report_written": diagnostics_report_written,
        "stable_progress_total": stable_progress_total,
        "expected_tx_total": expected_tx_count,
        "aoem_executed_total": aoem_executed_total,
        "queue_pending_last": queue_pending_last,
        "receiver_exit_phase": last_sample
            .and_then(|sample| sample.get("receiver_exit_phase"))
            .cloned(),
        "primary_send_completed": last_sample
            .and_then(|sample| sample.get("primary_send_completed"))
            .cloned(),
        "repair_convergence_started": last_sample
            .and_then(|sample| sample.get("repair_convergence_started"))
            .cloned(),
        "repair_convergence_completed": last_sample
            .and_then(|sample| sample.get("repair_convergence_completed"))
            .cloned(),
        "receiver_drain_completed": last_sample
            .and_then(|sample| sample.get("receiver_drain_completed"))
            .cloned(),
        "final_ack_received": last_sample
            .and_then(|sample| sample.get("final_ack_received"))
            .cloned(),
        "absolute_timeout_reached": last_sample
            .and_then(|sample| sample.get("absolute_timeout_reached"))
            .cloned(),
        "no_progress_timeout_reached": last_sample
            .and_then(|sample| sample.get("no_progress_timeout_reached"))
            .cloned(),
        "last_sample_elapsed_ms": last_sample
            .and_then(|sample| sample.get("elapsed_ms"))
            .and_then(Value::as_u64),
        "fail_reason": fail_reason,
    });
    write_report(receiver_exit_report_path().as_path(), &report)
}

fn write_synthetic_receiver_failure_report(
    expected_tx_count: u64,
    fail_reason: &str,
    state: &ReceiverDiagnosticsStateV1,
) -> Result<()> {
    let last_sample = state.samples.last();
    let progress_summary_from_path = last_sample
        .and_then(|sample| sample.get("pipeline_progress_report_path"))
        .and_then(Value::as_str)
        .map(Path::new)
        .and_then(read_pipeline_progress_summary);
    let repair_source = if last_sample
        .and_then(|sample| sample.get("repair_packet_received_count"))
        .is_some()
    {
        last_sample
    } else {
        progress_summary_from_path.as_ref()
    };
    let ledger_receipt_source = if ledger_receipt_completion_attribution_available_v1(repair_source)
    {
        repair_source
    } else {
        progress_summary_from_path
            .as_ref()
            .filter(|summary| ledger_receipt_completion_attribution_available_v1(Some(summary)))
            .or(repair_source)
    };
    let ack_snapshot = fs::read_to_string(ack_report_path())
        .ok()
        .and_then(|raw| serde_json::from_str::<Value>(raw.as_str()).ok());
    let stable_progress_total = last_sample
        .and_then(|sample| sample.get("stable_progress_total"))
        .and_then(Value::as_u64)
        .unwrap_or_default();
    let aoem_executed_total = last_sample
        .and_then(|sample| sample.get("aoem_executed_total"))
        .and_then(Value::as_u64)
        .unwrap_or_default();
    let queue_pending_last = last_sample
        .and_then(|sample| sample.get("queue_pending_last"))
        .and_then(Value::as_u64)
        .unwrap_or_default();
    let final_missing_ranges_sample = ack_snapshot
        .as_ref()
        .and_then(|ack| ack.get("missing_ranges_sample"))
        .cloned()
        .unwrap_or_else(|| {
            missing_ranges_to_json(
                missing_ranges_from_progress(stable_progress_total, expected_tx_count, 256)
                    .as_slice(),
                256,
            )
        });
    let final_missing_sequence_count = ack_snapshot
        .as_ref()
        .and_then(|ack| ack.get("missing_count"))
        .and_then(Value::as_u64)
        .unwrap_or_else(|| expected_tx_count.saturating_sub(stable_progress_total));
    let final_missing_ranges = missing_ranges_from_json(&final_missing_ranges_sample);
    let ledger_expected_count_for_invariant = repair_source
        .and_then(|sample| sample.get("ledger_expected_count"))
        .and_then(Value::as_u64)
        .unwrap_or_default();
    let repair_received_ranges = repair_source
        .and_then(|sample| sample.get("repair_sequence_received_ranges_sample"))
        .map(missing_ranges_from_json)
        .unwrap_or_default();
    let repair_accepted_ranges = repair_source
        .and_then(|sample| sample.get("repair_sequence_accepted_ranges_sample"))
        .map(missing_ranges_from_json)
        .unwrap_or_default();
    let repair_enqueued_ranges = repair_source
        .and_then(|sample| sample.get("repair_sequence_enqueued_ranges_sample"))
        .map(missing_ranges_from_json)
        .unwrap_or_default();
    let repair_already_receipted_ranges = repair_source
        .and_then(|sample| sample.get("repair_sequence_already_receipted_ranges_sample"))
        .map(missing_ranges_from_json)
        .unwrap_or_default();
    let repair_duplicate_ranges = repair_source
        .and_then(|sample| sample.get("repair_sequence_duplicate_ranges_sample"))
        .map(missing_ranges_from_json)
        .unwrap_or_default();
    let repair_admitted_to_aoem_ranges = repair_source
        .and_then(|sample| sample.get("repair_sequence_admitted_to_aoem_ranges_sample"))
        .map(missing_ranges_from_json)
        .unwrap_or_default();
    let repair_packet_received_count = repair_source
        .and_then(|sample| sample.get("repair_packet_received_count"))
        .and_then(Value::as_u64);
    let repair_attribution_available = repair_packet_received_count.is_some();
    let repair_received_final_missing_overlap_count = missing_ranges_overlap_count(
        final_missing_ranges.as_slice(),
        repair_received_ranges.as_slice(),
    );
    let repair_accepted_final_missing_overlap_count = missing_ranges_overlap_count(
        final_missing_ranges.as_slice(),
        repair_accepted_ranges.as_slice(),
    );
    let repair_enqueued_final_missing_overlap_count = missing_ranges_overlap_count(
        final_missing_ranges.as_slice(),
        repair_enqueued_ranges.as_slice(),
    );
    let repair_already_receipted_final_missing_overlap_count = missing_ranges_overlap_count(
        final_missing_ranges.as_slice(),
        repair_already_receipted_ranges.as_slice(),
    );
    let repair_duplicate_final_missing_overlap_count = missing_ranges_overlap_count(
        final_missing_ranges.as_slice(),
        repair_duplicate_ranges.as_slice(),
    );
    let repair_admitted_to_aoem_final_missing_overlap_count = missing_ranges_overlap_count(
        final_missing_ranges.as_slice(),
        repair_admitted_to_aoem_ranges.as_slice(),
    );
    let receipt_index_false_positive_suspected =
        repair_already_receipted_final_missing_overlap_count > 0;
    let repair_accepted_but_not_effective_count = repair_accepted_final_missing_overlap_count
        .saturating_sub(
            repair_enqueued_final_missing_overlap_count
                .saturating_add(repair_already_receipted_final_missing_overlap_count),
        );
    let mut repair_accepted_but_not_effective_reason_counts = serde_json::Map::new();
    if repair_accepted_but_not_effective_count > 0 {
        repair_accepted_but_not_effective_reason_counts.insert(
            "accepted_not_enqueued_for_final_missing".to_string(),
            serde_json::json!(repair_accepted_but_not_effective_count),
        );
    }
    let mut report = serde_json::json!({
        "schema": REPORT_SCHEMA_V1,
        "role": "receiver",
        "accepted": false,
        "synthetic_failure_report": true,
        "fail_reason": fail_reason,
        "receiver_exit_phase": last_sample
            .and_then(|sample| sample.get("receiver_exit_phase"))
            .cloned(),
        "primary_send_completed": last_sample
            .and_then(|sample| sample.get("primary_send_completed"))
            .cloned(),
        "repair_convergence_started": last_sample
            .and_then(|sample| sample.get("repair_convergence_started"))
            .cloned(),
        "repair_convergence_completed": last_sample
            .and_then(|sample| sample.get("repair_convergence_completed"))
            .cloned(),
        "receiver_drain_completed": last_sample
            .and_then(|sample| sample.get("receiver_drain_completed"))
            .cloned(),
        "final_ack_received": last_sample
            .and_then(|sample| sample.get("final_ack_received"))
            .cloned(),
        "absolute_timeout_reached": last_sample
            .and_then(|sample| sample.get("absolute_timeout_reached"))
            .cloned(),
        "no_progress_timeout_reached": last_sample
            .and_then(|sample| sample.get("no_progress_timeout_reached"))
            .cloned(),
        "tx_count": expected_tx_count,
        "validation": {
            "received_unique": stable_progress_total,
            "canonical_unique_included": stable_progress_total,
            "duplicate_canonical_included": 0u64,
            "duplicate_receipt": 0u64,
            "queue_pending_last": queue_pending_last,
            "semantic_head_monotonic": true,
            "receipt_index_consistent": false,
            "aoem_concurrency_owner": "AOEM_runtime",
            "final_missing_sequence_count": final_missing_sequence_count,
            "final_missing_ranges_sample": final_missing_ranges_sample,
            "final_missing_without_expected_ledger_invariant_violation": final_missing_without_expected_ledger_v1(
                final_missing_sequence_count,
                ledger_expected_count_for_invariant,
            ),
            "repair_attribution_available": repair_attribution_available,
            "repair_packet_received_count": repair_packet_received_count,
            "repair_packet_decode_failed_count": repair_source
                .and_then(|sample| sample.get("repair_packet_decode_failed_count"))
                .and_then(Value::as_u64),
            "repair_sequence_received_count": repair_source
                .and_then(|sample| sample.get("repair_sequence_received_count"))
                .and_then(Value::as_u64),
            "repair_sequence_received_min": repair_source
                .and_then(|sample| sample.get("repair_sequence_received_min"))
                .cloned(),
            "repair_sequence_received_max": repair_source
                .and_then(|sample| sample.get("repair_sequence_received_max"))
                .cloned(),
            "repair_sequence_received_final_missing_overlap_count": repair_received_final_missing_overlap_count,
            "repair_sequence_accepted_count": repair_source
                .and_then(|sample| sample.get("repair_sequence_accepted_count"))
                .and_then(Value::as_u64),
            "repair_sequence_accepted_final_missing_overlap_count": repair_accepted_final_missing_overlap_count,
            "repair_sequence_enqueued_count": repair_source
                .and_then(|sample| sample.get("repair_sequence_enqueued_count"))
                .and_then(Value::as_u64),
            "repair_sequence_enqueued_final_missing_overlap_count": repair_enqueued_final_missing_overlap_count,
            "repair_sequence_admitted_to_aoem_final_missing_overlap_count": repair_admitted_to_aoem_final_missing_overlap_count,
            "repair_sequence_already_receipted_final_missing_overlap_count": repair_already_receipted_final_missing_overlap_count,
            "repair_sequence_pending_duplicate_final_missing_overlap_count": repair_duplicate_final_missing_overlap_count,
            "repair_final_missing_force_enqueued_count": repair_enqueued_final_missing_overlap_count,
            "repair_final_missing_already_pending_count": repair_duplicate_final_missing_overlap_count,
            "repair_final_missing_receipt_hit_count": repair_already_receipted_final_missing_overlap_count,
            "repair_final_missing_enqueue_failed_count": repair_accepted_but_not_effective_count,
            "repair_final_missing_enqueue_failed_reason_counts": Value::Object(repair_accepted_but_not_effective_reason_counts.clone()),
            "repair_attempted_unreceipted_count": repair_source
                .and_then(|sample| sample.get("repair_attempted_unreceipted_count"))
                .and_then(Value::as_u64),
            "repair_attempted_unreceipted_final_missing_overlap_count": repair_source
                .and_then(|sample| sample.get("repair_attempted_unreceipted_final_missing_overlap_count"))
                .and_then(Value::as_u64),
            "repair_attempted_unreceipted_requeued_count": repair_source
                .and_then(|sample| sample.get("repair_attempted_unreceipted_requeued_count"))
                .and_then(Value::as_u64),
            "repair_attempted_unreceipted_requeue_failed_count": repair_source
                .and_then(|sample| sample.get("repair_attempted_unreceipted_requeue_failed_count"))
                .and_then(Value::as_u64),
            "repair_final_missing_payload_available_count": repair_source
                .and_then(|sample| sample.get("repair_final_missing_payload_available_count"))
                .and_then(Value::as_u64),
            "repair_final_missing_payload_available_but_inactive_count": repair_source
                .and_then(|sample| sample.get("repair_final_missing_payload_available_but_inactive_count"))
                .and_then(Value::as_u64),
            "repair_final_missing_invariant_violation_count": repair_source
                .and_then(|sample| sample.get("repair_final_missing_invariant_violation_count"))
                .and_then(Value::as_u64),
            "repair_final_missing_sequence_to_tx_hash_count": repair_source
                .and_then(|sample| sample.get("repair_final_missing_sequence_to_tx_hash_count"))
                .and_then(Value::as_u64),
            "repair_final_missing_tx_hash_payload_hit_count": repair_source
                .and_then(|sample| sample.get("repair_final_missing_tx_hash_payload_hit_count"))
                .and_then(Value::as_u64),
            "repair_final_missing_payload_missing_by_sequence_count": repair_source
                .and_then(|sample| sample.get("repair_final_missing_payload_missing_by_sequence_count"))
                .and_then(Value::as_u64),
            "repair_final_missing_payload_missing_ranges_sample": repair_source
                .and_then(|sample| sample.get("repair_final_missing_payload_missing_ranges_sample"))
                .cloned()
                .unwrap_or_else(|| serde_json::json!([])),
            "repair_sequence_payload_index_count": repair_source
                .and_then(|sample| sample.get("repair_sequence_payload_index_count"))
                .and_then(Value::as_u64),
            "repair_sequence_payload_index_final_missing_overlap_count": repair_source
                .and_then(|sample| sample.get("repair_sequence_payload_index_final_missing_overlap_count"))
                .and_then(Value::as_u64),
            "repair_sequence_payload_index_evicted_count": repair_source
                .and_then(|sample| sample.get("repair_sequence_payload_index_evicted_count"))
                .and_then(Value::as_u64),
            "repair_payload_retention_false_negative_suspected": repair_source
                .and_then(|sample| sample.get("repair_payload_retention_false_negative_suspected"))
                .and_then(Value::as_bool),
            "repair_final_missing_payload_recovered_count": repair_source
                .and_then(|sample| sample.get("repair_final_missing_payload_recovered_count"))
                .and_then(Value::as_u64),
            "repair_final_missing_payload_recovered_requeued_count": repair_source
                .and_then(|sample| sample.get("repair_final_missing_payload_recovered_requeued_count"))
                .and_then(Value::as_u64),
            "ledger_expected_range_start": repair_source
                .and_then(|sample| sample.get("ledger_expected_range_start"))
                .cloned(),
            "ledger_expected_range_end": repair_source
                .and_then(|sample| sample.get("ledger_expected_range_end"))
                .cloned(),
            "ledger_expected_count": repair_source
                .and_then(|sample| sample.get("ledger_expected_count"))
                .and_then(Value::as_u64),
            "child_env_tx_count_raw": repair_source
                .and_then(|sample| sample.get("child_env_tx_count_raw"))
                .cloned(),
            "child_expected_total_from_env": repair_source
                .and_then(|sample| sample.get("child_expected_total_from_env"))
                .and_then(Value::as_u64),
            "child_expected_total_from_config": repair_source
                .and_then(|sample| sample.get("child_expected_total_from_config"))
                .and_then(Value::as_u64),
            "child_ledger_expected_range_init_called": repair_source
                .and_then(|sample| sample.get("child_ledger_expected_range_init_called"))
                .and_then(Value::as_bool),
            "child_ledger_expected_range_init_source": repair_source
                .and_then(|sample| sample.get("child_ledger_expected_range_init_source"))
                .cloned(),
            "child_ledger_expected_range_init_error": repair_source
                .and_then(|sample| sample.get("child_ledger_expected_range_init_error"))
                .cloned(),
            "child_progress_summary_source": repair_source
                .and_then(|sample| sample.get("child_progress_summary_source"))
                .cloned(),
            "wrapper_progress_summary_source": repair_source
                .and_then(|sample| sample.get("wrapper_progress_summary_source"))
                .cloned()
                .unwrap_or_else(|| serde_json::json!("receiver_wrapper_child_progress_report")),
            "ledger_completed_count": repair_source
                .and_then(|sample| sample.get("ledger_completed_count"))
                .and_then(Value::as_u64),
            "ledger_durable_missing_count": repair_source
                .and_then(|sample| sample.get("ledger_durable_missing_count"))
                .and_then(Value::as_u64),
            "ledger_durable_missing_ranges_sample": repair_source
                .and_then(|sample| sample.get("ledger_durable_missing_ranges_sample"))
                .cloned()
                .unwrap_or_else(|| serde_json::json!([])),
            "ledger_durable_missing_bitmap_available": repair_source
                .and_then(|sample| sample.get("ledger_durable_missing_bitmap_available"))
                .and_then(Value::as_bool),
            "ledger_durable_missing_source": repair_source
                .and_then(|sample| sample.get("ledger_durable_missing_source"))
                .cloned()
                .unwrap_or_else(|| serde_json::json!("")),
            "ledger_durable_missing_derived_from_expected_range": repair_source
                .and_then(|sample| sample.get("ledger_durable_missing_derived_from_expected_range"))
                .and_then(Value::as_bool),
            "ledger_missing_closed_by_receipt_count": repair_source
                .and_then(|sample| sample.get("ledger_missing_closed_by_receipt_count"))
                .and_then(Value::as_u64),
            "ledger_missing_closed_by_canonical_count": repair_source
                .and_then(|sample| sample.get("ledger_missing_closed_by_canonical_count"))
                .and_then(Value::as_u64),
            "ledger_missing_incorrectly_closed_by_received_count": repair_source
                .and_then(|sample| sample.get("ledger_missing_incorrectly_closed_by_received_count"))
                .and_then(Value::as_u64),
            "ledger_missing_incorrectly_closed_by_enqueued_count": repair_source
                .and_then(|sample| sample.get("ledger_missing_incorrectly_closed_by_enqueued_count"))
                .and_then(Value::as_u64),
            "ledger_candidate_rehydrated_count": repair_source
                .and_then(|sample| sample.get("ledger_candidate_rehydrated_count"))
                .and_then(Value::as_u64),
            "ledger_candidate_empty_but_durable_missing_count": repair_source
                .and_then(|sample| sample.get("ledger_candidate_empty_but_durable_missing_count"))
                .and_then(Value::as_u64),
            "ledger_missing_without_candidate_count": repair_source
                .and_then(|sample| sample.get("ledger_missing_without_candidate_count"))
                .and_then(Value::as_u64),
            "ledger_missing_without_retryable_count": repair_source
                .and_then(|sample| sample.get("ledger_missing_without_retryable_count"))
                .and_then(Value::as_u64),
            "ledger_candidate_count_exceeds_durable_missing_invariant_violation_count": repair_source
                .and_then(|sample| sample.get("ledger_candidate_count_exceeds_durable_missing_invariant_violation_count"))
                .and_then(Value::as_u64),
            "ledger_final_missing_without_durable_missing_count": repair_source
                .and_then(|sample| sample.get("ledger_final_missing_without_durable_missing_count"))
                .and_then(Value::as_u64),
            "ledger_final_missing_candidate_count": repair_source
                .and_then(|sample| sample.get("ledger_final_missing_candidate_count"))
                .and_then(Value::as_u64),
            "ledger_final_missing_candidate_ranges_sample": repair_source
                .and_then(|sample| sample.get("ledger_final_missing_candidate_ranges_sample"))
                .cloned()
                .unwrap_or_else(|| serde_json::json!([])),
            "ledger_final_missing_requeued_before_admission_count": repair_source
                .and_then(|sample| sample.get("ledger_final_missing_requeued_before_admission_count"))
                .and_then(Value::as_u64),
            "ledger_final_missing_admitted_count": repair_source
                .and_then(|sample| sample.get("ledger_final_missing_admitted_count"))
                .and_then(Value::as_u64),
            "ledger_final_missing_admitted_ranges_sample": repair_source
                .and_then(|sample| sample.get("ledger_final_missing_admitted_ranges_sample"))
                .cloned()
                .unwrap_or_else(|| serde_json::json!([])),
            "ledger_final_missing_admission_skipped_count": repair_source
                .and_then(|sample| sample.get("ledger_final_missing_admission_skipped_count"))
                .and_then(Value::as_u64),
            "ledger_final_missing_admission_skip_reason_counts": repair_source
                .and_then(|sample| sample.get("ledger_final_missing_admission_skip_reason_counts"))
                .cloned()
                .unwrap_or_else(|| serde_json::json!({})),
            "admission_used_ledger_final_missing_bucket": repair_source
                .and_then(|sample| sample.get("admission_used_ledger_final_missing_bucket"))
                .and_then(Value::as_bool),
            "repair_sequence_duplicate_count": repair_source
                .and_then(|sample| sample.get("repair_sequence_duplicate_count"))
                .and_then(Value::as_u64),
            "repair_sequence_rejected_count": repair_source
                .and_then(|sample| sample.get("repair_sequence_rejected_count"))
                .and_then(Value::as_u64),
            "repair_reject_reason_counts": repair_source
                .and_then(|sample| sample.get("repair_reject_reason_counts"))
                .cloned()
                .unwrap_or_else(|| serde_json::json!({})),
            "receipt_index_false_positive_suspected": receipt_index_false_positive_suspected,
            "repair_accepted_but_not_effective_count": repair_accepted_but_not_effective_count,
            "repair_accepted_but_not_effective_reason_counts": Value::Object(repair_accepted_but_not_effective_reason_counts.clone()),
        },
        "receiver_summary": {
            "accepted": false,
            "aoem_executed_total": aoem_executed_total,
            "queue_pending_last": queue_pending_last,
            "progress_score": stable_progress_total,
            "repair_attribution_available": repair_attribution_available,
            "repair_packet_received_count": repair_packet_received_count,
            "repair_sequence_received_max": repair_source
                .and_then(|sample| sample.get("repair_sequence_received_max"))
                .cloned(),
            "repair_sequence_accepted_count": repair_source
                .and_then(|sample| sample.get("repair_sequence_accepted_count"))
                .and_then(Value::as_u64),
            "repair_sequence_enqueued_count": repair_source
                .and_then(|sample| sample.get("repair_sequence_enqueued_count"))
                .and_then(Value::as_u64),
            "repair_sequence_received_final_missing_overlap_count": repair_received_final_missing_overlap_count,
            "repair_sequence_accepted_final_missing_overlap_count": repair_accepted_final_missing_overlap_count,
            "repair_sequence_enqueued_final_missing_overlap_count": repair_enqueued_final_missing_overlap_count,
            "repair_sequence_admitted_to_aoem_final_missing_overlap_count": repair_admitted_to_aoem_final_missing_overlap_count,
            "repair_sequence_already_receipted_final_missing_overlap_count": repair_already_receipted_final_missing_overlap_count,
            "repair_sequence_pending_duplicate_final_missing_overlap_count": repair_duplicate_final_missing_overlap_count,
            "repair_final_missing_force_enqueued_count": repair_enqueued_final_missing_overlap_count,
            "repair_final_missing_already_pending_count": repair_duplicate_final_missing_overlap_count,
            "repair_final_missing_receipt_hit_count": repair_already_receipted_final_missing_overlap_count,
            "repair_final_missing_enqueue_failed_count": repair_accepted_but_not_effective_count,
            "repair_final_missing_enqueue_failed_reason_counts": Value::Object(repair_accepted_but_not_effective_reason_counts),
            "repair_attempted_unreceipted_count": repair_source
                .and_then(|sample| sample.get("repair_attempted_unreceipted_count"))
                .and_then(Value::as_u64),
            "repair_attempted_unreceipted_final_missing_overlap_count": repair_source
                .and_then(|sample| sample.get("repair_attempted_unreceipted_final_missing_overlap_count"))
                .and_then(Value::as_u64),
            "repair_attempted_unreceipted_requeued_count": repair_source
                .and_then(|sample| sample.get("repair_attempted_unreceipted_requeued_count"))
                .and_then(Value::as_u64),
            "repair_attempted_unreceipted_requeue_failed_count": repair_source
                .and_then(|sample| sample.get("repair_attempted_unreceipted_requeue_failed_count"))
                .and_then(Value::as_u64),
            "repair_final_missing_payload_available_count": repair_source
                .and_then(|sample| sample.get("repair_final_missing_payload_available_count"))
                .and_then(Value::as_u64),
            "repair_final_missing_payload_available_but_inactive_count": repair_source
                .and_then(|sample| sample.get("repair_final_missing_payload_available_but_inactive_count"))
                .and_then(Value::as_u64),
            "repair_final_missing_invariant_violation_count": repair_source
                .and_then(|sample| sample.get("repair_final_missing_invariant_violation_count"))
                .and_then(Value::as_u64),
            "repair_final_missing_sequence_to_tx_hash_count": repair_source
                .and_then(|sample| sample.get("repair_final_missing_sequence_to_tx_hash_count"))
                .and_then(Value::as_u64),
            "repair_final_missing_tx_hash_payload_hit_count": repair_source
                .and_then(|sample| sample.get("repair_final_missing_tx_hash_payload_hit_count"))
                .and_then(Value::as_u64),
            "repair_final_missing_payload_missing_by_sequence_count": repair_source
                .and_then(|sample| sample.get("repair_final_missing_payload_missing_by_sequence_count"))
                .and_then(Value::as_u64),
            "repair_final_missing_payload_missing_ranges_sample": repair_source
                .and_then(|sample| sample.get("repair_final_missing_payload_missing_ranges_sample"))
                .cloned()
                .unwrap_or_else(|| serde_json::json!([])),
            "repair_sequence_payload_index_count": repair_source
                .and_then(|sample| sample.get("repair_sequence_payload_index_count"))
                .and_then(Value::as_u64),
            "repair_sequence_payload_index_final_missing_overlap_count": repair_source
                .and_then(|sample| sample.get("repair_sequence_payload_index_final_missing_overlap_count"))
                .and_then(Value::as_u64),
            "repair_sequence_payload_index_evicted_count": repair_source
                .and_then(|sample| sample.get("repair_sequence_payload_index_evicted_count"))
                .and_then(Value::as_u64),
            "repair_payload_retention_false_negative_suspected": repair_source
                .and_then(|sample| sample.get("repair_payload_retention_false_negative_suspected"))
                .and_then(Value::as_bool),
            "repair_final_missing_payload_recovered_count": repair_source
                .and_then(|sample| sample.get("repair_final_missing_payload_recovered_count"))
                .and_then(Value::as_u64),
            "repair_final_missing_payload_recovered_requeued_count": repair_source
                .and_then(|sample| sample.get("repair_final_missing_payload_recovered_requeued_count"))
                .and_then(Value::as_u64),
            "ledger_expected_range_start": repair_source
                .and_then(|sample| sample.get("ledger_expected_range_start"))
                .cloned(),
            "ledger_expected_range_end": repair_source
                .and_then(|sample| sample.get("ledger_expected_range_end"))
                .cloned(),
            "ledger_expected_count": repair_source
                .and_then(|sample| sample.get("ledger_expected_count"))
                .and_then(Value::as_u64),
            "ledger_completed_count": repair_source
                .and_then(|sample| sample.get("ledger_completed_count"))
                .and_then(Value::as_u64),
            "ledger_durable_missing_count": repair_source
                .and_then(|sample| sample.get("ledger_durable_missing_count"))
                .and_then(Value::as_u64),
            "ledger_durable_missing_ranges_sample": repair_source
                .and_then(|sample| sample.get("ledger_durable_missing_ranges_sample"))
                .cloned()
                .unwrap_or_else(|| serde_json::json!([])),
            "ledger_durable_missing_bitmap_available": repair_source
                .and_then(|sample| sample.get("ledger_durable_missing_bitmap_available"))
                .and_then(Value::as_bool),
            "ledger_durable_missing_source": repair_source
                .and_then(|sample| sample.get("ledger_durable_missing_source"))
                .cloned()
                .unwrap_or_else(|| serde_json::json!("")),
            "ledger_durable_missing_derived_from_expected_range": repair_source
                .and_then(|sample| sample.get("ledger_durable_missing_derived_from_expected_range"))
                .and_then(Value::as_bool),
            "ledger_missing_closed_by_receipt_count": repair_source
                .and_then(|sample| sample.get("ledger_missing_closed_by_receipt_count"))
                .and_then(Value::as_u64),
            "ledger_missing_closed_by_canonical_count": repair_source
                .and_then(|sample| sample.get("ledger_missing_closed_by_canonical_count"))
                .and_then(Value::as_u64),
            "ledger_missing_incorrectly_closed_by_received_count": repair_source
                .and_then(|sample| sample.get("ledger_missing_incorrectly_closed_by_received_count"))
                .and_then(Value::as_u64),
            "ledger_missing_incorrectly_closed_by_enqueued_count": repair_source
                .and_then(|sample| sample.get("ledger_missing_incorrectly_closed_by_enqueued_count"))
                .and_then(Value::as_u64),
            "ledger_candidate_rehydrated_count": repair_source
                .and_then(|sample| sample.get("ledger_candidate_rehydrated_count"))
                .and_then(Value::as_u64),
            "ledger_candidate_empty_but_durable_missing_count": repair_source
                .and_then(|sample| sample.get("ledger_candidate_empty_but_durable_missing_count"))
                .and_then(Value::as_u64),
            "ledger_missing_without_candidate_count": repair_source
                .and_then(|sample| sample.get("ledger_missing_without_candidate_count"))
                .and_then(Value::as_u64),
            "ledger_missing_without_retryable_count": repair_source
                .and_then(|sample| sample.get("ledger_missing_without_retryable_count"))
                .and_then(Value::as_u64),
            "ledger_candidate_count_exceeds_durable_missing_invariant_violation_count": repair_source
                .and_then(|sample| sample.get("ledger_candidate_count_exceeds_durable_missing_invariant_violation_count"))
                .and_then(Value::as_u64),
            "ledger_final_missing_without_durable_missing_count": repair_source
                .and_then(|sample| sample.get("ledger_final_missing_without_durable_missing_count"))
                .and_then(Value::as_u64),
            "ledger_final_missing_candidate_count": repair_source
                .and_then(|sample| sample.get("ledger_final_missing_candidate_count"))
                .and_then(Value::as_u64),
            "ledger_final_missing_candidate_ranges_sample": repair_source
                .and_then(|sample| sample.get("ledger_final_missing_candidate_ranges_sample"))
                .cloned()
                .unwrap_or_else(|| serde_json::json!([])),
            "ledger_final_missing_requeued_before_admission_count": repair_source
                .and_then(|sample| sample.get("ledger_final_missing_requeued_before_admission_count"))
                .and_then(Value::as_u64),
            "ledger_final_missing_admitted_count": repair_source
                .and_then(|sample| sample.get("ledger_final_missing_admitted_count"))
                .and_then(Value::as_u64),
            "ledger_final_missing_admitted_ranges_sample": repair_source
                .and_then(|sample| sample.get("ledger_final_missing_admitted_ranges_sample"))
                .cloned()
                .unwrap_or_else(|| serde_json::json!([])),
            "ledger_final_missing_admission_skipped_count": repair_source
                .and_then(|sample| sample.get("ledger_final_missing_admission_skipped_count"))
                .and_then(Value::as_u64),
            "ledger_final_missing_admission_skip_reason_counts": repair_source
                .and_then(|sample| sample.get("ledger_final_missing_admission_skip_reason_counts"))
                .cloned()
                .unwrap_or_else(|| serde_json::json!({})),
            "admission_used_ledger_final_missing_bucket": repair_source
                .and_then(|sample| sample.get("admission_used_ledger_final_missing_bucket"))
                .and_then(Value::as_bool),
        },
        "receiver_ack_snapshot": {
            "expected_tx_total": ack_snapshot
                .as_ref()
                .and_then(|ack| ack.get("expected_tx_total"))
                .and_then(Value::as_u64),
            "received_unique_count": ack_snapshot
                .as_ref()
                .and_then(|ack| ack.get("received_unique_count"))
                .and_then(Value::as_u64),
            "highest_sequence_seen": ack_snapshot
                .as_ref()
                .and_then(|ack| ack.get("highest_sequence_seen"))
                .cloned(),
            "missing_count": ack_snapshot
                .as_ref()
                .and_then(|ack| ack.get("missing_count"))
                .and_then(Value::as_u64),
            "missing_ranges_sample": ack_snapshot
                .as_ref()
                .and_then(|ack| ack.get("missing_ranges_sample"))
                .cloned()
                .unwrap_or_else(|| serde_json::json!([])),
            "ack_epoch": ack_snapshot
                .as_ref()
                .and_then(|ack| ack.get("ack_epoch"))
                .and_then(Value::as_u64),
            "receiver_done": ack_snapshot
                .as_ref()
                .and_then(|ack| ack.get("receiver_done"))
                .and_then(Value::as_bool),
        },
        "violations": [
            format!("receiver exited before expected_tx_total: progress={stable_progress_total} expected={expected_tx_count}"),
        ],
    });
    if let Some(validation) = report.get_mut("validation") {
        apply_ledger_receipt_completion_fields_v1(validation, ledger_receipt_source);
    }
    if let Some(receiver_summary) = report.get_mut("receiver_summary") {
        apply_ledger_receipt_completion_fields_v1(receiver_summary, ledger_receipt_source);
    }
    apply_ledger_receipt_completion_fields_v1(&mut report, ledger_receipt_source);
    let ledger_durable_missing_count = ledger_receipt_source
        .map(|sample| summary_u64(sample, "ledger_durable_missing_count"))
        .unwrap_or_default();
    if final_missing_sequence_count > 0 && ledger_durable_missing_count == 0 {
        let false_completed_sample = final_missing_ranges_sample.clone();
        for target in ["validation", "receiver_summary"] {
            if let Some(obj) = report.get_mut(target).and_then(Value::as_object_mut) {
                obj.insert(
                    "ledger_false_completed_invariant_violation_count".to_string(),
                    serde_json::json!(final_missing_sequence_count),
                );
                obj.insert(
                    "ledger_false_completed_sequences_sample".to_string(),
                    false_completed_sample.clone(),
                );
                obj.insert(
                    "ledger_validation_final_missing_overlap_count".to_string(),
                    serde_json::json!(final_missing_sequence_count),
                );
                obj.insert(
                    "ledger_durable_missing_validation_mismatch_count".to_string(),
                    serde_json::json!(final_missing_sequence_count),
                );
                obj.insert(
                    "trace_first_divergence_stage".to_string(),
                    serde_json::json!("ledger_false_completed"),
                );
                obj.insert(
                    "trace_false_completed_sequences_sample".to_string(),
                    false_completed_sample.clone(),
                );
            }
        }
        if let Some(obj) = report.as_object_mut() {
            obj.insert(
                "ledger_false_completed_invariant_violation_count".to_string(),
                serde_json::json!(final_missing_sequence_count),
            );
            obj.insert(
                "ledger_false_completed_sequences_sample".to_string(),
                false_completed_sample.clone(),
            );
            obj.insert(
                "ledger_validation_final_missing_overlap_count".to_string(),
                serde_json::json!(final_missing_sequence_count),
            );
            obj.insert(
                "ledger_durable_missing_validation_mismatch_count".to_string(),
                serde_json::json!(final_missing_sequence_count),
            );
            obj.insert(
                "trace_first_divergence_stage".to_string(),
                serde_json::json!("ledger_false_completed"),
            );
            obj.insert(
                "trace_false_completed_sequences_sample".to_string(),
                false_completed_sample,
            );
        }
    }
    write_report(report_path("receiver").as_path(), &report)
}

fn semantic_ledger_stats(path: &Path) -> Value {
    let Ok(metadata) = fs::metadata(path) else {
        return serde_json::json!({
            "path": path,
            "exists": false,
            "line_count": 0u64,
            "bytes": 0u64,
        });
    };
    let line_count = fs::File::open(path)
        .ok()
        .map(|file| {
            std::io::BufReader::new(file)
                .lines()
                .filter(|line| line.as_ref().is_ok_and(|item| !item.trim().is_empty()))
                .count() as u64
        })
        .unwrap_or_default();
    serde_json::json!({
        "path": path,
        "exists": true,
        "line_count": line_count,
        "bytes": metadata.len(),
    })
}

fn read_pipeline_progress_summary(path: &Path) -> Option<Value> {
    let raw = fs::read_to_string(path).ok()?;
    let value = serde_json::from_str::<Value>(raw.as_str()).ok()?;
    value.get("summary").cloned()
}

#[cfg(windows)]
fn process_memory_sample(pid: u32) -> Value {
    let script = format!(
        "$c=Get-CimInstance Win32_PerfFormattedData_PerfProc_Process -Filter \"IDProcess={pid}\" -ErrorAction SilentlyContinue; \
         if ($c) {{ \
         [pscustomobject]@{{\
            WorkingSet64=[int64]$c.WorkingSet;\
            PrivateMemorySize64=[int64]$c.PrivateBytes;\
            VirtualMemorySize64=[int64]$c.VirtualBytes;\
            PagedMemorySize64=0;\
            PagedSystemMemorySize64=0;\
            NonpagedSystemMemorySize64=0;\
            HandleCount=[int64]$c.HandleCount;\
            ThreadCount=[int64]$c.ThreadCount;\
            CPU=0;\
            SampleMethod='cim_perfproc'\
         }} | ConvertTo-Json -Compress; exit 0 }}; \
         $p=Get-Process -Id {pid} -ErrorAction Stop; \
         [pscustomobject]@{{\
            WorkingSet64=$p.WorkingSet64;\
            PrivateMemorySize64=$p.PrivateMemorySize64;\
            VirtualMemorySize64=$p.VirtualMemorySize64;\
            PagedMemorySize64=$p.PagedMemorySize64;\
            PagedSystemMemorySize64=$p.PagedSystemMemorySize64;\
            NonpagedSystemMemorySize64=$p.NonpagedSystemMemorySize64;\
            HandleCount=$p.HandleCount;\
            ThreadCount=$p.Threads.Count;\
            CPU=$p.CPU;\
            SampleMethod='get_process_threads_count_fallback'\
         }} | ConvertTo-Json -Compress"
    );
    match Command::new("powershell")
        .args(["-NoProfile", "-Command", script.as_str()])
        .output()
    {
        Ok(output) if output.status.success() => serde_json::from_slice::<Value>(&output.stdout)
            .unwrap_or_else(|_| {
                serde_json::json!({
                    "sample_ok": false,
                    "error": "parse_windows_process_memory_sample_failed",
                })
            }),
        Ok(output) => serde_json::json!({
            "sample_ok": false,
            "error": "windows_process_memory_sample_failed",
            "stderr": String::from_utf8_lossy(&output.stderr),
        }),
        Err(err) => serde_json::json!({
            "sample_ok": false,
            "error": format!("spawn_windows_process_memory_sample_failed: {err}"),
        }),
    }
}

#[cfg(not(windows))]
fn process_memory_sample(pid: u32) -> Value {
    let status_path = PathBuf::from(format!("/proc/{pid}/status"));
    let raw = fs::read_to_string(status_path.as_path()).unwrap_or_default();
    let mut vm_rss_kb = 0u64;
    let mut vm_data_kb = 0u64;
    let mut vm_size_kb = 0u64;
    for line in raw.lines() {
        if let Some(rest) = line.strip_prefix("VmRSS:") {
            vm_rss_kb = rest
                .split_whitespace()
                .next()
                .and_then(|value| value.parse::<u64>().ok())
                .unwrap_or_default();
        }
        if let Some(rest) = line.strip_prefix("VmData:") {
            vm_data_kb = rest
                .split_whitespace()
                .next()
                .and_then(|value| value.parse::<u64>().ok())
                .unwrap_or_default();
        }
        if let Some(rest) = line.strip_prefix("VmSize:") {
            vm_size_kb = rest
                .split_whitespace()
                .next()
                .and_then(|value| value.parse::<u64>().ok())
                .unwrap_or_default();
        }
    }
    serde_json::json!({
        "WorkingSet64": vm_rss_kb.saturating_mul(1024),
        "PrivateMemorySize64": vm_data_kb.saturating_mul(1024),
        "VirtualMemorySize64": vm_size_kb.saturating_mul(1024),
    })
}

fn memory_working_set_bytes(sample: &Value) -> u64 {
    sample
        .get("WorkingSet64")
        .or_else(|| sample.get("working_set_bytes"))
        .and_then(Value::as_u64)
        .unwrap_or_default()
}

fn memory_private_bytes(sample: &Value) -> u64 {
    sample
        .get("PrivateMemorySize64")
        .or_else(|| sample.get("private_bytes"))
        .and_then(Value::as_u64)
        .unwrap_or_default()
}

fn memory_virtual_bytes(sample: &Value) -> u64 {
    sample
        .get("VirtualMemorySize64")
        .or_else(|| sample.get("virtual_bytes"))
        .and_then(Value::as_u64)
        .unwrap_or_default()
}

fn memory_paged_bytes(sample: &Value) -> u64 {
    sample
        .get("PagedMemorySize64")
        .or_else(|| sample.get("paged_bytes"))
        .and_then(Value::as_u64)
        .unwrap_or_default()
}

fn memory_paged_system_bytes(sample: &Value) -> u64 {
    sample
        .get("PagedSystemMemorySize64")
        .or_else(|| sample.get("paged_system_bytes"))
        .and_then(Value::as_u64)
        .unwrap_or_default()
}

fn memory_nonpaged_system_bytes(sample: &Value) -> u64 {
    sample
        .get("NonpagedSystemMemorySize64")
        .or_else(|| sample.get("nonpaged_system_bytes"))
        .and_then(Value::as_u64)
        .unwrap_or_default()
}

fn memory_handle_count(sample: &Value) -> u64 {
    sample
        .get("HandleCount")
        .or_else(|| sample.get("handle_count"))
        .and_then(Value::as_u64)
        .unwrap_or_default()
}

fn memory_thread_count(sample: &Value) -> u64 {
    sample
        .get("ThreadCount")
        .or_else(|| sample.get("thread_count"))
        .and_then(Value::as_u64)
        .unwrap_or_default()
}

fn bytes_per_1000_tx(bytes: u64, tx_count: u64) -> u64 {
    if tx_count == 0 {
        return 0;
    }
    bytes.saturating_mul(1000) / tx_count
}

fn probe_bool_env(name: &str) -> bool {
    string_env_nonempty(name)
        .map(|value| {
            matches!(
                value.to_ascii_lowercase().as_str(),
                "1" | "true" | "yes" | "on"
            )
        })
        .unwrap_or(false)
}

fn memory_probe_switches_report() -> Value {
    let mut map = serde_json::Map::new();
    for (name, env_name, probe_only_not_functional) in MEMORY_PROBE_TOGGLES_V1 {
        map.insert(
            (*name).to_string(),
            serde_json::json!(probe_bool_env(env_name)),
        );
        map.insert(
            format!("{name}_probe_only_not_functional"),
            serde_json::json!(*probe_only_not_functional && probe_bool_env(env_name)),
        );
    }
    map.insert(
        "disable_proof_projection_for_memory_probe".to_string(),
        serde_json::json!(
            probe_bool_env("NOVOVM_NATIVE_PIPELINE_DISABLE_PROOF_PROJECTION_FOR_MEMORY_PROBE")
                || probe_bool_env("NOVOVM_NATIVE_PIPELINE_MEMORY_PROBE_DISABLE_PROOF_PROJECTION")
        ),
    );
    map.insert(
        "disable_canonical_projection_for_memory_probe".to_string(),
        serde_json::json!(
            probe_bool_env("NOVOVM_NATIVE_PIPELINE_DISABLE_CANONICAL_PROJECTION_FOR_MEMORY_PROBE")
                || probe_bool_env(
                    "NOVOVM_NATIVE_PIPELINE_MEMORY_PROBE_DISABLE_CANONICAL_PROJECTION"
                )
        ),
    );
    map.insert(
        "disable_report_serialization_for_memory_probe".to_string(),
        serde_json::json!(
            probe_bool_env("NOVOVM_NATIVE_PIPELINE_DISABLE_REPORT_SERIALIZATION_FOR_MEMORY_PROBE")
                || probe_bool_env(
                    "NOVOVM_NATIVE_PIPELINE_MEMORY_PROBE_DISABLE_JSON_REPORT_SERIALIZATION"
                )
        ),
    );
    map.insert(
        "disable_recovery_probe_for_memory_probe".to_string(),
        serde_json::json!(
            probe_bool_env("NOVOVM_NATIVE_PIPELINE_DISABLE_RECOVERY_PROBE_FOR_MEMORY_PROBE")
                || probe_bool_env("NOVOVM_NATIVE_PIPELINE_MEMORY_PROBE_DISABLE_RECOVERY_PROBE")
        ),
    );
    map.insert(
        "applies_to_production_default".to_string(),
        serde_json::json!(false),
    );
    map.insert(
        "lifecycle_structure_changed".to_string(),
        serde_json::json!(false),
    );
    Value::Object(map)
}

fn mini_expected_send_tps_x1000_from_env_v1() -> u64 {
    let tx_per_round = u64_env("NOVOVM_NATIVE_PIPELINE_SUSTAINED_TX_PER_ROUND", 32)
        .unwrap_or(32)
        .max(1);
    let round_interval_ms = u64_env("NOVOVM_NATIVE_PIPELINE_SUSTAINED_ROUND_INTERVAL_MS", 1_000)
        .unwrap_or(1_000)
        .max(1);
    tx_per_round.saturating_mul(1_000_000) / round_interval_ms
}

fn mini_tps_below_threshold_v1(actual_x1000: u64, expected_x1000: u64) -> bool {
    expected_x1000 > 0 && actual_x1000.saturating_mul(10) < expected_x1000.saturating_mul(9)
}

fn annotate_mini_tps_sync_gate_v1(sample: &mut Value) {
    let expected = sample_u64(sample, "mini_expected_tx_count")
        .max(sample_u64(sample, "mini_completed_tx_count"))
        .max(sample_u64(sample, "canonical_unique_included_total"))
        .max(sample_u64(sample, "receiver_ledger_close_count"))
        .max(sample_u64(
            sample,
            "aoem_native_tx_batch_production_receipt_count",
        ))
        .max(sample_u64(
            sample,
            "aoem_native_tx_batch_production_canonical_proof_count",
        ))
        .max(sample_u64(
            sample,
            "aoem_native_tx_batch_production_ledger_close_proof_count",
        ));
    if expected == 0 || expected > 480 {
        sample["mini_tps_sync_gate_applicable"] = serde_json::json!(false);
        return;
    }
    let elapsed_delta_ms = sample_u64(sample, "receiver_delta_elapsed_ms");
    let sender_tps_x1000 = mini_expected_send_tps_x1000_from_env_v1();
    let rate_x1000 = |delta: u64| -> u64 {
        if elapsed_delta_ms == 0 {
            0
        } else {
            delta.saturating_mul(1_000_000) / elapsed_delta_ms
        }
    };
    let udp_packet_tps = rate_x1000(sample_u64(sample, "receiver_udp_packet_recv_delta"));
    let transport_object_tps = rate_x1000(sample_u64(sample, "receiver_sequence_unique_delta"));
    let tx_object_admitted_tps = rate_x1000(sample_u64(sample, "receiver_pending_selected_delta"));
    let queue_admitted_tps = tx_object_admitted_tps;
    let aoem_tps = rate_x1000(sample_u64(sample, "receiver_aoem_executed_delta_raw"));
    let canonical_tps = rate_x1000(sample_u64(sample, "receiver_canonical_delta_raw"));
    let ledger_tps = rate_x1000(sample_u64(sample, "receiver_ledger_close_delta_raw"));
    let packet_to_tx_ratio = if transport_object_tps == 0 {
        0
    } else {
        udp_packet_tps.saturating_mul(1_000) / transport_object_tps
    };
    let pending_accumulating = sample
        .get("receiver_pending_delta_direction")
        .and_then(Value::as_str)
        == Some("increase")
        && sample_u64(sample, "queue_pending_last") > 0;
    let mut reasons = Vec::<String>::new();
    if mini_tps_below_threshold_v1(transport_object_tps, sender_tps_x1000) {
        push_json_string_unique(&mut reasons, "b_transport_object_below_sender");
    }
    if mini_tps_below_threshold_v1(queue_admitted_tps, transport_object_tps) {
        push_json_string_unique(&mut reasons, "b_queue_admit_below_transport_object");
    }
    if mini_tps_below_threshold_v1(aoem_tps, sender_tps_x1000) {
        push_json_string_unique(&mut reasons, "b_aoem_close_below_sender");
    }
    if mini_tps_below_threshold_v1(aoem_tps, queue_admitted_tps) {
        push_json_string_unique(&mut reasons, "b_aoem_close_below_admitted");
    }
    if mini_tps_below_threshold_v1(canonical_tps, aoem_tps) {
        push_json_string_unique(&mut reasons, "b_canonical_close_below_aoem");
    }
    if mini_tps_below_threshold_v1(ledger_tps, canonical_tps) {
        push_json_string_unique(&mut reasons, "b_ledger_close_below_canonical");
    }
    if pending_accumulating {
        push_json_string_unique(&mut reasons, "pending_accumulating");
    }
    sample["mini_tps_sync_gate_applicable"] = serde_json::json!(true);
    sample["mini_a_send_tps_x1000"] = serde_json::json!(sender_tps_x1000);
    sample["mini_a_send_tps"] = serde_json::json!(sender_tps_x1000 / 1000);
    sample["mini_b_udp_packet_tps_x1000"] = serde_json::json!(udp_packet_tps);
    sample["mini_b_transport_object_ready_tps_x1000"] = serde_json::json!(transport_object_tps);
    sample["mini_b_sequence_unique_tps_x1000"] = serde_json::json!(transport_object_tps);
    sample["mini_b_tx_object_admitted_tps_x1000"] = serde_json::json!(tx_object_admitted_tps);
    sample["mini_b_queue_admitted_tx_tps_x1000"] = serde_json::json!(queue_admitted_tps);
    sample["mini_b_aoem_closed_tx_tps_x1000"] = serde_json::json!(aoem_tps);
    sample["mini_b_canonical_tx_tps_x1000"] = serde_json::json!(canonical_tps);
    sample["mini_b_ledger_tx_tps_x1000"] = serde_json::json!(ledger_tps);
    sample["mini_tps_sync_metric_units"] =
        serde_json::json!("x1000_tx_per_second; udp_packet_tps_diagnostic_only");
    sample["mini_tps_sync_comparable_network_source"] =
        serde_json::json!("receiver_sequence_unique_delta");
    sample["mini_tps_sync_packet_to_tx_ratio"] = serde_json::json!(packet_to_tx_ratio);
    // Backward-compatible aliases now use tx-comparable units, not UDP packet rate.
    sample["mini_b_network_received_tps_x1000"] = serde_json::json!(transport_object_tps);
    sample["mini_b_queue_admitted_tps_x1000"] = serde_json::json!(queue_admitted_tps);
    sample["mini_b_aoem_closed_tps_x1000"] = serde_json::json!(aoem_tps);
    sample["mini_b_canonical_tps_x1000"] = serde_json::json!(canonical_tps);
    sample["mini_b_ledger_tps_x1000"] = serde_json::json!(ledger_tps);
    sample["mini_pending_delta_per_window"] = serde_json::json!(if pending_accumulating {
        i64::try_from(sample_u64(sample, "receiver_pending_delta_abs")).unwrap_or(i64::MAX)
    } else {
        -i64::try_from(sample_u64(sample, "receiver_pending_delta_abs")).unwrap_or(i64::MAX)
    });
    sample["mini_tps_sync_pass"] = serde_json::json!(reasons.is_empty());
    sample["mini_tps_sync_fail_reason"] = if reasons.is_empty() {
        Value::Null
    } else {
        serde_json::json!(reasons.join(","))
    };
    sample["mini_tps_sync_fail_reasons"] = serde_json::json!(reasons);
}

fn annotate_mini_final_run_tps_sync_v1(sample: &mut Value, first_progress_elapsed_ms: Option<u64>) {
    if sample
        .get("final_closed_child_sample")
        .and_then(Value::as_bool)
        != Some(true)
    {
        return;
    }
    let expected = sample_u64(sample, "mini_expected_tx_count")
        .max(sample_u64(sample, "mini_completed_tx_count"))
        .max(sample_u64(sample, "canonical_unique_included_total"))
        .max(sample_u64(sample, "receiver_ledger_close_count"))
        .max(sample_u64(
            sample,
            "aoem_native_tx_batch_production_receipt_count",
        ))
        .max(sample_u64(
            sample,
            "aoem_native_tx_batch_production_canonical_proof_count",
        ))
        .max(sample_u64(
            sample,
            "aoem_native_tx_batch_production_ledger_close_proof_count",
        ));
    if expected == 0 || expected > 480 {
        return;
    }
    let sender_tps_x1000 = mini_expected_send_tps_x1000_from_env_v1();
    let final_elapsed_ms = sample_u64(sample, "elapsed_ms");
    let final_completed = sample_u64(sample, "mini_completed_tx_count");
    let final_canonical = sample_u64(sample, "canonical_unique_included_total")
        .max(sample_u64(sample, "receiver_canonical_included_count"))
        .max(sample_u64(
            sample,
            "aoem_native_tx_batch_production_canonical_proof_count",
        ));
    let final_ledger = sample_u64(sample, "receiver_ledger_close_count")
        .max(sample_u64(sample, "ledger_completed_count"))
        .max(sample_u64(
            sample,
            "aoem_native_tx_batch_production_ledger_close_proof_count",
        ));
    let final_proof = sample_u64(sample, "proof_items_total")
        .max(sample_u64(
            sample,
            "aoem_native_tx_batch_production_receipt_count",
        ))
        .max(sample_u64(
            sample,
            "aoem_native_tx_batch_production_canonical_proof_count",
        ))
        .max(sample_u64(
            sample,
            "aoem_native_tx_batch_production_ledger_close_proof_count",
        ));
    let window_ms = first_progress_elapsed_ms
        .and_then(|first| final_elapsed_ms.checked_sub(first))
        .filter(|window| *window > 0)
        .unwrap_or(0);
    let rate_x1000 = |count: u64| -> u64 {
        if window_ms == 0 {
            0
        } else {
            count.saturating_mul(1_000_000) / window_ms
        }
    };
    let close_tps = rate_x1000(final_completed);
    let canonical_tps = rate_x1000(final_canonical);
    let ledger_tps = rate_x1000(final_ledger);
    let proof_tps = rate_x1000(final_proof);
    let retained_view_received = sample_u64(sample, "received_unique_total");
    let uses_retained_view = retained_view_received > 0
        && final_completed > 0
        && retained_view_received != final_completed;

    let mut reasons = Vec::<String>::new();
    let source_valid =
        window_ms > 0 && final_completed > 0 && final_canonical > 0 && final_ledger > 0;
    if !source_valid {
        push_json_string_unique(&mut reasons, "mini_tps_sample_source_invalid");
    } else {
        if mini_tps_below_threshold_v1(close_tps, sender_tps_x1000) {
            push_json_string_unique(&mut reasons, "final_run_close_tps_below_sender");
        }
        if mini_tps_below_threshold_v1(canonical_tps, close_tps) {
            push_json_string_unique(&mut reasons, "b_canonical_close_below_aoem");
        }
        if mini_tps_below_threshold_v1(ledger_tps, canonical_tps) {
            push_json_string_unique(&mut reasons, "b_ledger_close_below_canonical");
        }
    }

    sample["mini_tps_sync_sample_source"] = serde_json::json!("final_closed_child_sample");
    sample["mini_tps_sync_sample_source_valid"] = serde_json::json!(source_valid);
    sample["mini_tps_sync_final_counter_source"] = serde_json::json!("final_closed_counters");
    sample["mini_tps_sync_live_counter_source"] =
        serde_json::json!("live_delta_counters_diagnostic_only");
    sample["final_closed_child_sample_counter_source"] =
        serde_json::json!("mini_completed_canonical_ledger_proof_counters");
    sample["final_closed_child_sample_uses_retained_view"] = serde_json::json!(uses_retained_view);
    sample["final_run_close_tps_x1000"] = serde_json::json!(close_tps);
    sample["final_run_close_tps_window_ms"] = serde_json::json!(window_ms);
    sample["final_run_close_tps_counter"] = serde_json::json!(final_completed);
    sample["final_completed_tx_count"] = serde_json::json!(final_completed);
    sample["final_canonical_tx_count"] = serde_json::json!(final_canonical);
    sample["final_ledger_closed_tx_count"] = serde_json::json!(final_ledger);
    sample["final_proof_count"] = serde_json::json!(final_proof);
    sample["final_run_aoem_close_tps_x1000"] = serde_json::json!(close_tps);
    sample["final_run_canonical_tps_x1000"] = serde_json::json!(canonical_tps);
    sample["final_run_ledger_tps_x1000"] = serde_json::json!(ledger_tps);
    sample["final_run_proof_tps_x1000"] = serde_json::json!(proof_tps);
    sample["final_run_tps_sync_pass"] = serde_json::json!(reasons.is_empty());
    sample["final_run_tps_sync_fail_reasons"] = serde_json::json!(reasons.clone());

    sample["mini_tps_sync_gate_applicable"] = serde_json::json!(true);
    sample["mini_tps_sync_metric_units"] =
        serde_json::json!("x1000_tx_per_second; final_closed_counters");
    sample["mini_tps_sync_comparable_network_source"] = serde_json::json!("final_closed_counters");
    sample["mini_a_send_tps_x1000"] = serde_json::json!(sender_tps_x1000);
    sample["mini_b_transport_object_ready_tps_x1000"] = serde_json::json!(close_tps);
    sample["mini_b_sequence_unique_tps_x1000"] = serde_json::json!(close_tps);
    sample["mini_b_tx_object_admitted_tps_x1000"] = serde_json::json!(close_tps);
    sample["mini_b_queue_admitted_tx_tps_x1000"] = serde_json::json!(close_tps);
    sample["mini_b_aoem_closed_tx_tps_x1000"] = serde_json::json!(close_tps);
    sample["mini_b_canonical_tx_tps_x1000"] = serde_json::json!(canonical_tps);
    sample["mini_b_ledger_tx_tps_x1000"] = serde_json::json!(ledger_tps);
    sample["mini_b_network_received_tps_x1000"] = serde_json::json!(close_tps);
    sample["mini_b_queue_admitted_tps_x1000"] = serde_json::json!(close_tps);
    sample["mini_b_aoem_closed_tps_x1000"] = serde_json::json!(close_tps);
    sample["mini_b_canonical_tps_x1000"] = serde_json::json!(canonical_tps);
    sample["mini_b_ledger_tps_x1000"] = serde_json::json!(ledger_tps);
    sample["mini_tps_sync_pass"] = serde_json::json!(reasons.is_empty());
    sample["mini_tps_sync_fail_reason"] = if reasons.is_empty() {
        Value::Null
    } else {
        serde_json::json!(reasons.join(","))
    };
    sample["mini_tps_sync_fail_reasons"] = serde_json::json!(reasons);
}

fn sample_u64(sample: &Value, key: &str) -> u64 {
    sample.get(key).and_then(Value::as_u64).unwrap_or_default()
}

fn sample_bool(sample: &Value, key: &str) -> Option<bool> {
    sample.get(key).and_then(Value::as_bool)
}

fn sample_string(sample: &Value, key: &str) -> Option<String> {
    sample.get(key).and_then(Value::as_str).map(str::to_owned)
}

fn is_live_child_memory_sample(sample: &Value) -> bool {
    sample_u64(sample, "process_working_set_bytes") > 0
        || sample_u64(sample, "process_private_bytes") > 0
}

fn last_live_child_sample(samples: &[Value]) -> Option<&Value> {
    samples
        .iter()
        .rev()
        .find(|sample| is_live_child_memory_sample(sample))
}

fn final_closed_child_sample(samples: &[Value]) -> Option<&Value> {
    samples.iter().rev().find(|sample| {
        sample
            .get("final_closed_child_sample")
            .and_then(Value::as_bool)
            == Some(true)
    })
}

fn mini_tps_progress_counter_v1(sample: &Value) -> u64 {
    sample_u64(sample, "mini_completed_tx_count")
        .max(sample_u64(sample, "received_unique_total"))
        .max(sample_u64(sample, "aoem_executed_total"))
        .max(sample_u64(sample, "canonical_unique_included_total"))
        .max(sample_u64(sample, "receiver_ledger_close_count"))
}

fn first_mini_tps_progress_elapsed_ms_v1(samples: &[Value]) -> Option<u64> {
    samples
        .iter()
        .find(|sample| mini_tps_progress_counter_v1(sample) > 0)
        .map(|sample| sample_u64(sample, "elapsed_ms"))
}

fn receiver_object_ready_counter_v1(sample: &Value) -> u64 {
    sample_u64(sample, "received_unique_total")
        .max(sample_u64(sample, "network_receiver_object_ready_count"))
        .max(sample_u64(sample, "ingress_total_last"))
        .max(sample_u64(sample, "network_received_total"))
}

fn receiver_close_counter_v1(sample: &Value) -> u64 {
    sample_u64(sample, "mini_completed_tx_count")
        .max(sample_u64(sample, "aoem_executed_total"))
        .max(sample_u64(sample, "canonical_unique_included_total"))
        .max(sample_u64(sample, "receiver_ledger_close_count"))
        .max(sample_u64(sample, "ledger_completed_count"))
        .max(sample_u64(
            sample,
            "aoem_native_tx_batch_production_receipt_count",
        ))
        .max(sample_u64(
            sample,
            "aoem_native_tx_batch_production_canonical_proof_count",
        ))
        .max(sample_u64(
            sample,
            "aoem_native_tx_batch_production_ledger_close_proof_count",
        ))
}

fn sample_elapsed_ms_option_v1(sample: &Value) -> Option<u64> {
    sample.get("elapsed_ms").and_then(Value::as_u64)
}

fn receiver_first_counter_sample_v1<F>(samples: &[Value], counter: F) -> Option<&Value>
where
    F: Fn(&Value) -> u64,
{
    samples.iter().find(|sample| counter(sample) > 0)
}

fn receiver_last_counter_sample_v1<F>(samples: &[Value], counter: F) -> Option<&Value>
where
    F: Fn(&Value) -> u64,
{
    samples.iter().rev().find(|sample| counter(sample) > 0)
}

fn receiver_last_counter_progress_sample_v1<F>(samples: &[Value], counter: F) -> Option<&Value>
where
    F: Fn(&Value) -> u64,
{
    let mut previous = 0u64;
    let mut last_progress = None;
    for sample in samples {
        let current = counter(sample);
        if current > previous {
            last_progress = Some(sample);
        }
        previous = current;
    }
    last_progress
}

fn rate_x1000_v1(count: u64, elapsed_ms: u64) -> u64 {
    if elapsed_ms == 0 {
        0
    } else {
        count.saturating_mul(1_000_000) / elapsed_ms
    }
}

fn receiver_pipeline_backpressure_reason_v1(signoff_sample: Option<&Value>) -> String {
    let Some(sample) = signoff_sample else {
        return "no_signoff_sample".to_string();
    };
    let object_ready = sample_u64(sample, "network_receiver_object_ready_count");
    let batch_ready = sample_u64(sample, "object_assembler_batch_ready_count");
    let batch_received = sample_u64(sample, "aoem_runtime_worker_batch_received_count");
    let ingress_calls = sample_u64(sample, "aoem_runtime_worker_tx_ingress_call_count");
    let result_ready = sample_u64(sample, "aoem_runtime_worker_result_ready_count");
    let verified = sample_u64(sample, "finality_report_worker_result_verified_count");
    if object_ready > 0 && batch_ready == 0 {
        "object_assembler_lag".to_string()
    } else if batch_ready > batch_received {
        "aoem_runtime_worker_receive_lag".to_string()
    } else if batch_received > ingress_calls {
        "aoem_runtime_worker_submit_lag".to_string()
    } else if ingress_calls > result_ready {
        "aoem_runtime_worker_result_drain_lag".to_string()
    } else if result_ready > verified {
        "finality_report_worker_lag".to_string()
    } else {
        sample
            .get("receiver_pipeline_backpressure_reason")
            .and_then(Value::as_str)
            .filter(|reason| !reason.trim().is_empty())
            .unwrap_or("none")
            .to_string()
    }
}

fn receiver_wall_clock_performance_breakdown_v1(
    samples: &[Value],
    signoff_sample: Option<&Value>,
    expected_tx_count: u64,
) -> Value {
    const STRICT_30MIN_WALL_CLOCK_BUDGET_MS: u64 = 1_800_000;
    let first_tx_sample =
        receiver_first_counter_sample_v1(samples, receiver_object_ready_counter_v1);
    let last_tx_sample =
        receiver_last_counter_progress_sample_v1(samples, receiver_object_ready_counter_v1)
            .or_else(|| receiver_last_counter_sample_v1(samples, receiver_object_ready_counter_v1));
    let first_close_sample = receiver_first_counter_sample_v1(samples, receiver_close_counter_v1);
    let last_close_sample =
        receiver_last_counter_progress_sample_v1(samples, receiver_close_counter_v1)
            .or_else(|| receiver_last_counter_sample_v1(samples, receiver_close_counter_v1));
    let final_sample = signoff_sample.or_else(|| samples.last());

    let receiver_total_elapsed_ms =
        final_sample.map_or(0, |sample| sample_u64(sample, "elapsed_ms"));
    let first_tx_ms = first_tx_sample.and_then(sample_elapsed_ms_option_v1);
    let last_tx_ms = last_tx_sample.and_then(sample_elapsed_ms_option_v1);
    let first_close_ms = first_close_sample.and_then(sample_elapsed_ms_option_v1);
    let last_close_ms = last_close_sample.and_then(sample_elapsed_ms_option_v1);
    let first_close_count = first_close_sample.map_or(0, receiver_close_counter_v1);
    let last_close_count = last_close_sample.map_or(0, receiver_close_counter_v1);
    let total_close_count = final_sample.map_or(0, receiver_close_counter_v1);
    let receiver_close_delta_window_ms = first_close_ms
        .zip(last_close_ms)
        .map(|(first, last)| last.saturating_sub(first))
        .unwrap_or(0);
    let active_close_counter_delta = last_close_count.saturating_sub(first_close_count);
    let receiver_close_delta_tps_x1000 =
        rate_x1000_v1(active_close_counter_delta, receiver_close_delta_window_ms);
    let total_close_tps_x1000 = rate_x1000_v1(total_close_count, receiver_total_elapsed_ms);
    let finalization_tail_ms = last_close_ms
        .map(|last| receiver_total_elapsed_ms.saturating_sub(last))
        .unwrap_or(0);
    let performance_window_start_ms = first_tx_ms.or(first_close_ms).unwrap_or(0);
    let performance_window_start_source = if first_tx_ms.is_some() {
        "first_tx_seen"
    } else if first_close_ms.is_some() {
        "first_close"
    } else {
        "receiver_start"
    };
    let performance_window_end_ms = last_close_ms.unwrap_or(receiver_total_elapsed_ms);
    let performance_window_end_source = if last_close_ms.is_some() {
        "final_close"
    } else {
        "receiver_total_elapsed"
    };
    let performance_window_elapsed_ms =
        performance_window_end_ms.saturating_sub(performance_window_start_ms);
    let active_close_tx_count = last_close_count.max(total_close_count);
    let active_close_tps_x1000 =
        rate_x1000_v1(active_close_tx_count, performance_window_elapsed_ms);
    let strict_target_tps_x1000 =
        rate_x1000_v1(expected_tx_count, STRICT_30MIN_WALL_CLOCK_BUDGET_MS);
    let mut strict_fail_reasons = Vec::<String>::new();
    if performance_window_elapsed_ms > STRICT_30MIN_WALL_CLOCK_BUDGET_MS {
        push_json_string_unique(
            &mut strict_fail_reasons,
            "active_performance_window_exceeded_30min",
        );
    }
    if expected_tx_count > 0
        && performance_window_elapsed_ms > 0
        && active_close_tps_x1000 < strict_target_tps_x1000
    {
        push_json_string_unique(
            &mut strict_fail_reasons,
            "receiver_active_close_tps_below_strict_target",
        );
    }
    if finalization_tail_ms > 60_000 {
        push_json_string_unique(&mut strict_fail_reasons, "finalization_tail_over_60s");
    }
    let total_elapsed_exceeded_due_to_pre_first_tx_wait = receiver_total_elapsed_ms
        > STRICT_30MIN_WALL_CLOCK_BUDGET_MS
        && performance_window_elapsed_ms <= STRICT_30MIN_WALL_CLOCK_BUDGET_MS
        && performance_window_start_ms > 0;
    let object_ready = signoff_sample.map_or(0, |sample| {
        sample_u64(sample, "network_receiver_object_ready_count")
    });
    let batch_ready = signoff_sample.map_or(0, |sample| {
        sample_u64(sample, "object_assembler_batch_ready_count")
    });
    let batch_received = signoff_sample.map_or(0, |sample| {
        sample_u64(sample, "aoem_runtime_worker_batch_received_count")
    });
    let tx_ingress_calls = signoff_sample.map_or(0, |sample| {
        sample_u64(sample, "aoem_runtime_worker_tx_ingress_call_count")
    });
    let result_ready = signoff_sample.map_or(0, |sample| {
        sample_u64(sample, "aoem_runtime_worker_result_ready_count")
    });
    let result_verified = signoff_sample.map_or(0, |sample| {
        sample_u64(sample, "finality_report_worker_result_verified_count")
    });
    let avg_batch_size = if tx_ingress_calls == 0 {
        0
    } else {
        total_close_count / tx_ingress_calls
    };
    let backpressure_reason = receiver_pipeline_backpressure_reason_v1(signoff_sample);

    serde_json::json!({
        "sender_primary_send_elapsed_ms": Value::Null,
        "sender_primary_send_tps_x1000": Value::Null,
        "sender_primary_send_completed_at_ms": Value::Null,
        "sender_wait_after_primary_send_ms": Value::Null,
        "receiver_total_elapsed_ms": receiver_total_elapsed_ms,
        "receiver_first_tx_seen_ms": first_tx_ms,
        "receiver_last_tx_seen_ms": last_tx_ms,
        "receiver_first_close_ms": first_close_ms,
        "receiver_last_close_ms": last_close_ms,
        "receiver_active_close_window_ms": performance_window_elapsed_ms,
        "receiver_active_close_counter_start": first_close_count,
        "receiver_active_close_counter_end": last_close_count,
        "receiver_active_close_counter_delta": active_close_counter_delta,
        "receiver_active_close_tps_x1000": active_close_tps_x1000,
        "receiver_close_delta_window_ms": receiver_close_delta_window_ms,
        "receiver_close_delta_tps_x1000": receiver_close_delta_tps_x1000,
        "receiver_total_close_tps_x1000": total_close_tps_x1000,
        "performance_window_start_source": performance_window_start_source,
        "performance_window_start_ms": performance_window_start_ms,
        "performance_window_end_source": performance_window_end_source,
        "performance_window_end_ms": performance_window_end_ms,
        "performance_window_elapsed_ms": performance_window_elapsed_ms,
        "pre_first_tx_wait_ms": first_tx_ms.unwrap_or(0),
        "receiver_pre_first_tx_wait_ms": first_tx_ms.unwrap_or(0),
        "receiver_pre_first_close_wait_ms": first_close_ms.unwrap_or(0),
        "active_close_tx_count": active_close_tx_count,
        "active_close_window_ms": performance_window_elapsed_ms,
        "active_close_tps_x1000": active_close_tps_x1000,
        "total_elapsed_ms": receiver_total_elapsed_ms,
        "total_close_tx_count": total_close_count,
        "total_close_tps_x1000": total_close_tps_x1000,
        "finalization_tail_ms": finalization_tail_ms,
        "tail_repair_wait_ms": Value::Null,
        "receiver_done_ack_wait_ms": Value::Null,
        "ack_receiver_done_sent_at_ms": Value::Null,
        "sender_receiver_done_ack_seen_at_ms": Value::Null,
        "network_receiver_object_ready_count": object_ready,
        "object_assembler_batch_ready_count": batch_ready,
        "aoem_runtime_worker_batch_received_count": batch_received,
        "aoem_runtime_worker_tx_ingress_call_count": tx_ingress_calls,
        "aoem_runtime_worker_result_ready_count": result_ready,
        "finality_report_worker_result_verified_count": result_verified,
        "pipeline_stage_lag_ms": Value::Null,
        "pipeline_backpressure_reason": backpressure_reason,
        "aoem_runtime_worker_batch_size_avg": avg_batch_size,
        "aoem_runtime_worker_batch_size_p50": Value::Null,
        "aoem_runtime_worker_batch_size_p90": Value::Null,
        "aoem_runtime_worker_inflight_batch_count": sample_u64(signoff_sample.unwrap_or(&Value::Null), "aoem_runtime_worker_inflight_batch_count"),
        "aoem_runtime_worker_max_inflight_batches": sample_u64(signoff_sample.unwrap_or(&Value::Null), "aoem_runtime_worker_max_inflight_batches"),
        "aoem_runtime_worker_submit_elapsed_ms": Value::Null,
        "aoem_runtime_worker_result_drain_elapsed_ms": Value::Null,
        "object_assembler_flush_delay_ms": Value::Null,
        "finality_report_worker_backpressure_ms": Value::Null,
        "diagnostics_report_write_elapsed_ms": Value::Null,
        "strict_30min_wall_clock_budget_ms": STRICT_30MIN_WALL_CLOCK_BUDGET_MS,
        "strict_30min_wall_clock_elapsed_ms": receiver_total_elapsed_ms,
        "strict_30min_wall_clock_gap_ms": receiver_total_elapsed_ms.saturating_sub(STRICT_30MIN_WALL_CLOCK_BUDGET_MS),
        "strict_30min_performance_gate_window": "first_tx_seen_to_final_close",
        "strict_30min_performance_pass": strict_fail_reasons.is_empty(),
        "strict_30min_performance_fail_reason": if strict_fail_reasons.is_empty() {
            Value::Null
        } else {
            serde_json::json!(strict_fail_reasons.join(","))
        },
        "total_elapsed_exceeded_due_to_pre_first_tx_wait": total_elapsed_exceeded_due_to_pre_first_tx_wait,
        "strict_30min_target_close_tps_x1000": strict_target_tps_x1000,
        "strict_30min_wall_clock_performance_pass": strict_fail_reasons.is_empty(),
        "strict_30min_wall_clock_fail_reasons": strict_fail_reasons,
    })
}

fn first_live_child_sample(samples: &[Value]) -> Option<&Value> {
    samples
        .iter()
        .find(|sample| is_live_child_memory_sample(sample))
}

fn peak_live_child_sample(samples: &[Value]) -> Option<&Value> {
    samples
        .iter()
        .filter(|sample| is_live_child_memory_sample(sample))
        .max_by_key(|sample| sample_u64(sample, "process_working_set_bytes"))
}

fn post_exit_sample_count(samples: &[Value]) -> u64 {
    samples
        .iter()
        .filter(|sample| !is_live_child_memory_sample(sample))
        .count()
        .try_into()
        .unwrap_or(u64::MAX)
}

const LEDGER_RECEIPT_COMPLETION_U64_FIELDS_V1: &[&str] = &[
    "ledger_final_missing_actual_batch_count",
    "ledger_final_missing_raw_txs_count",
    "ledger_final_missing_batch_result_count",
    "ledger_final_missing_receipt_written_count",
    "ledger_final_missing_receipt_missing_after_admission_count",
    "ledger_final_missing_inflight_count",
    "ledger_final_missing_retryable_count",
    "ledger_final_missing_requeued_after_no_receipt_count",
    "ledger_final_missing_admitted_but_no_receipt_invariant_violation_count",
    "ledger_final_missing_candidate_payload_available_count",
    "ledger_final_missing_candidate_payload_missing_count",
    "ledger_final_missing_candidate_tx_hash_mapping_missing_count",
    "ledger_final_missing_candidate_raw_tx_build_error_count",
    "ledger_final_missing_payload_available_selected_count",
    "ledger_final_missing_payload_available_not_selected_count",
    "ledger_final_missing_selectable_count",
    "ledger_final_missing_selector_input_count",
    "ledger_final_missing_selector_output_count",
    "ledger_final_missing_selector_skipped_by_old_pending_view_count",
    "ledger_final_missing_selected_not_pushed_to_raw_txs_count",
    "ledger_final_missing_raw_txs_push_attempt_count",
    "ledger_final_missing_raw_txs_push_success_count",
    "ledger_final_missing_raw_txs_nonempty_but_not_submitted_count",
    "ledger_final_missing_batch_blocked_by_payload_missing_count",
    "ledger_final_missing_batch_blocked_by_stage_filter_count",
    "ledger_final_missing_batch_blocked_by_scan_limit_count",
    "ledger_final_missing_batch_blocked_by_batch_limit_count",
    "ledger_final_missing_batch_blocked_by_raw_tx_build_error_count",
    "ledger_final_missing_batch_blocked_by_tx_hash_mapping_missing_count",
    "ledger_final_missing_batch_blocked_by_payload_available_not_selected_count",
    "ledger_final_missing_batch_blocked_by_selected_not_pushed_count",
    "ledger_final_missing_batch_blocked_by_raw_txs_nonempty_not_submitted_count",
    "ledger_final_missing_batch_blocked_by_batch_not_full_count",
    "ledger_final_missing_batch_blocked_by_no_tick_executed_count",
    "ledger_final_missing_batch_blocked_by_classification_path_not_reached_count",
    "ledger_final_missing_batch_blocked_by_unknown_invariant_violation_count",
    "ledger_final_missing_batch_limit_config",
    "ledger_final_missing_reserved_batch_budget",
    "ledger_final_missing_batch_budget_before_fill",
    "ledger_final_missing_batch_budget_after_fill",
    "ledger_final_missing_batch_blocked_by_limit_after_actual_fill_count",
    "ledger_final_missing_batch_limit_zero_count",
    "ledger_final_missing_preempted_normal_pending_count",
];

const LEDGER_RECEIPT_COMPLETION_ARRAY_FIELDS_V1: &[&str] = &[
    "ledger_final_missing_actual_batch_ranges_sample",
    "ledger_final_missing_candidate_payload_available_ranges_sample",
    "ledger_final_missing_payload_available_selected_ranges_sample",
    "trace_success_sequences_sample",
    "trace_failed_sequences_sample",
    "trace_candidate_payload_available_not_selected_sequences",
    "trace_selected_not_pushed_sequences",
    "trace_pushed_not_batched_sequences",
    "trace_batched_not_receipted_sequences",
];

fn ledger_receipt_completion_attribution_available_v1(source: Option<&Value>) -> bool {
    source.is_some_and(|value| {
        value
            .get("ledger_final_missing_actual_batch_count")
            .is_some()
            && value
                .get("ledger_final_missing_receipt_written_count")
                .is_some()
            && value
                .get("ledger_final_missing_receipt_missing_after_admission_count")
                .is_some()
    })
}

fn ledger_receipt_completion_missing_reason_v1(source: Option<&Value>) -> Option<&'static str> {
    if source.is_none() {
        return Some("missing_progress_summary");
    }
    let source = source.expect("checked is_some");
    if source
        .get("ledger_final_missing_actual_batch_count")
        .is_none()
    {
        return Some("missing_actual_batch_count");
    }
    if source
        .get("ledger_final_missing_receipt_written_count")
        .is_none()
    {
        return Some("missing_receipt_written_count");
    }
    if source
        .get("ledger_final_missing_receipt_missing_after_admission_count")
        .is_none()
    {
        return Some("missing_receipt_missing_after_admission_count");
    }
    None
}

fn apply_ledger_receipt_completion_fields_v1(target: &mut Value, source: Option<&Value>) {
    let Some(target_obj) = target.as_object_mut() else {
        return;
    };
    for field in LEDGER_RECEIPT_COMPLETION_U64_FIELDS_V1 {
        target_obj.insert(
            (*field).to_string(),
            serde_json::json!(source.map(|value| summary_u64(value, field)).unwrap_or(0)),
        );
    }
    for field in LEDGER_RECEIPT_COMPLETION_ARRAY_FIELDS_V1 {
        target_obj.insert(
            (*field).to_string(),
            source
                .and_then(|value| value.get(*field))
                .cloned()
                .unwrap_or_else(|| serde_json::json!([])),
        );
    }
    target_obj.insert(
        "ledger_admission_counter_is_actual_batch".to_string(),
        serde_json::json!(source
            .and_then(|value| value.get("ledger_admission_counter_is_actual_batch"))
            .and_then(Value::as_bool)
            .unwrap_or(false)),
    );
    target_obj.insert(
        "ledger_final_missing_batch_nonempty_submitted".to_string(),
        serde_json::json!(source
            .and_then(|value| value.get("ledger_final_missing_batch_nonempty_submitted"))
            .and_then(Value::as_bool)
            .unwrap_or(false)),
    );
    target_obj.insert(
        "ledger_final_missing_selector_used_durable_bucket".to_string(),
        serde_json::json!(source
            .and_then(|value| value.get("ledger_final_missing_selector_used_durable_bucket"))
            .and_then(Value::as_bool)
            .unwrap_or(false)),
    );
    target_obj.insert(
        "novorudp_trace_enabled".to_string(),
        serde_json::json!(source
            .and_then(|value| value.get("novorudp_trace_enabled"))
            .and_then(Value::as_bool)
            .unwrap_or(false)),
    );
    target_obj.insert(
        "trace_first_divergence_stage".to_string(),
        source
            .and_then(|value| value.get("trace_first_divergence_stage"))
            .cloned()
            .unwrap_or_else(|| serde_json::json!("")),
    );
    target_obj.insert(
        "trace_first_divergence_sequence".to_string(),
        source
            .and_then(|value| value.get("trace_first_divergence_sequence"))
            .cloned()
            .unwrap_or(Value::Null),
    );
    target_obj.insert(
        "trace_success_vs_failed_diff_summary".to_string(),
        source
            .and_then(|value| value.get("trace_success_vs_failed_diff_summary"))
            .cloned()
            .unwrap_or_else(|| serde_json::json!({})),
    );
    target_obj.insert(
        "ledger_final_missing_payload_available_selection_skipped_reason".to_string(),
        source
            .and_then(|value| {
                value.get("ledger_final_missing_payload_available_selection_skipped_reason")
            })
            .cloned()
            .unwrap_or_else(|| serde_json::json!("")),
    );
    target_obj.insert(
        "ledger_admission_counter_mismatch_reason".to_string(),
        source
            .and_then(|value| value.get("ledger_admission_counter_mismatch_reason"))
            .cloned()
            .unwrap_or_else(|| serde_json::json!("")),
    );
    let mut blocked_reason = source
        .and_then(|value| value.get("ledger_final_missing_batch_blocked_reason"))
        .and_then(Value::as_str)
        .unwrap_or_default()
        .to_string();
    if blocked_reason.trim().is_empty()
        && source
            .map(|value| summary_u64(value, "ledger_final_missing_candidate_count") > 0)
            .unwrap_or(false)
        && source
            .map(|value| summary_u64(value, "ledger_final_missing_actual_batch_count") == 0)
            .unwrap_or(false)
    {
        blocked_reason = "classification_path_not_reached".to_string();
        let current = target_obj
            .get("ledger_final_missing_batch_blocked_by_classification_path_not_reached_count")
            .and_then(Value::as_u64)
            .unwrap_or_default();
        let candidate_count = source
            .map(|value| summary_u64(value, "ledger_final_missing_candidate_count"))
            .unwrap_or_default();
        target_obj.insert(
            "ledger_final_missing_batch_blocked_by_classification_path_not_reached_count"
                .to_string(),
            serde_json::json!(current.max(candidate_count)),
        );
    }
    target_obj.insert(
        "ledger_final_missing_batch_blocked_reason".to_string(),
        serde_json::json!(blocked_reason),
    );
    let attribution_available = ledger_receipt_completion_attribution_available_v1(source);
    target_obj.insert(
        "ledger_receipt_completion_attribution_available".to_string(),
        serde_json::json!(attribution_available),
    );
    target_obj.insert(
        "ledger_receipt_completion_attribution_missing_reason".to_string(),
        ledger_receipt_completion_missing_reason_v1(source)
            .map(|reason| serde_json::json!(reason))
            .unwrap_or(Value::Null),
    );
}

fn receiver_summary_consistency_reasons_v1(
    aoem: u64,
    canonical: u64,
    ledger_completed: u64,
) -> Vec<&'static str> {
    let mut reasons = Vec::new();
    if aoem > canonical {
        reasons.push("aoem_executed_gt_canonical");
    }
    if ledger_completed > canonical {
        reasons.push("ledger_completed_gt_canonical");
    }
    if ledger_completed > aoem {
        reasons.push("ledger_completed_gt_aoem");
    }
    reasons
}

fn receiver_drain_attribution_stage_v1(
    network_received_total: u64,
    ingress_submitted_total: u64,
    pending_last: u64,
    ticks: u64,
    queue_admitted_total: u64,
    nonempty_aoem_batch_ticks: u64,
    aoem: u64,
    canonical: u64,
    ledger_completed: u64,
) -> &'static str {
    if network_received_total == 0 && ingress_submitted_total == 0 {
        "waiting_for_udp"
    } else if pending_last > 0 && ticks == 0 {
        "receiver_child_tick_stall"
    } else if pending_last > 0 && ticks > 0 && queue_admitted_total == 0 {
        "admission_drain_stall"
    } else if queue_admitted_total > 0 && nonempty_aoem_batch_ticks == 0 {
        "batch_submit_stall"
    } else if aoem > canonical {
        "receipt_canonical_projection_or_summary_lag"
    } else if ledger_completed > canonical {
        "ledger_canonical_summary_lag"
    } else {
        "progressing"
    }
}

fn annotate_receiver_ingress_drain_delta_v1(sample: &mut Value, previous: Option<&Value>) {
    let Some(previous) = previous else {
        sample["receiver_ingress_drain_delta_available"] = serde_json::json!(false);
        sample["receiver_drain_stall_reason"] = serde_json::json!("first_sample");
        sample["repair_convergence_rate_tps"] = serde_json::json!(0u64);
        sample["repair_convergence_rate_tps_x1000"] = serde_json::json!(0u64);
        sample["missing_count_delta_per_minute"] = serde_json::json!(0u64);
        sample["missing_count_decrease_per_minute"] = serde_json::json!(0u64);
        sample["repair_effective_completion_per_1000_packets"] = serde_json::json!(0u64);
        return;
    };

    let elapsed_ms = sample_u64(sample, "elapsed_ms");
    let previous_elapsed_ms = sample_u64(previous, "elapsed_ms");
    let elapsed_delta_ms = elapsed_ms.saturating_sub(previous_elapsed_ms);
    let network_received = summary_u64(sample, "receiver_udp_packet_recv_count");
    let previous_network_received = summary_u64(previous, "receiver_udp_packet_recv_count");
    let received_unique = summary_u64(sample, "received_unique_total");
    let previous_received_unique = summary_u64(previous, "received_unique_total");
    let aoem = summary_u64(sample, "aoem_executed_total");
    let previous_aoem = summary_u64(previous, "aoem_executed_total");
    let canonical = summary_u64(sample, "canonical_unique_included_total");
    let previous_canonical = summary_u64(previous, "canonical_unique_included_total");
    let ledger_close = summary_u64(sample, "receiver_ledger_close_count");
    let previous_ledger_close = summary_u64(previous, "receiver_ledger_close_count");
    let ticks = summary_u64(sample, "receiver_child_tick_count");
    let previous_ticks = summary_u64(previous, "receiver_child_tick_count");
    let aoem_ticks = summary_u64(sample, "receiver_aoem_tick_count");
    let previous_aoem_ticks = summary_u64(previous, "receiver_aoem_tick_count");
    let pending_selected = summary_u64(sample, "receiver_pending_selected_count");
    let previous_pending_selected = summary_u64(previous, "receiver_pending_selected_count");
    let pending_last = summary_u64(sample, "queue_pending_last");
    let previous_pending_last = summary_u64(previous, "queue_pending_last");
    let durable_missing = summary_u64(sample, "ledger_durable_missing_count");
    let previous_durable_missing = summary_u64(previous, "ledger_durable_missing_count");
    let object_ready = summary_u64(sample, "network_receiver_object_ready_count");
    let previous_object_ready = summary_u64(previous, "network_receiver_object_ready_count");
    let batch_ready = summary_u64(sample, "object_assembler_batch_ready_count");
    let previous_batch_ready = summary_u64(previous, "object_assembler_batch_ready_count");
    let batch_received = summary_u64(sample, "aoem_runtime_worker_batch_received_count");
    let previous_batch_received = summary_u64(previous, "aoem_runtime_worker_batch_received_count");
    let tx_ingress_calls = summary_u64(sample, "aoem_runtime_worker_tx_ingress_call_count");
    let previous_tx_ingress_calls =
        summary_u64(previous, "aoem_runtime_worker_tx_ingress_call_count");
    let result_ready = summary_u64(sample, "aoem_runtime_worker_result_ready_count");
    let previous_result_ready = summary_u64(previous, "aoem_runtime_worker_result_ready_count");
    let result_verified = summary_u64(sample, "finality_report_worker_result_verified_count");
    let previous_result_verified =
        summary_u64(previous, "finality_report_worker_result_verified_count");

    let network_received_delta = network_received.saturating_sub(previous_network_received);
    let received_unique_delta = received_unique.saturating_sub(previous_received_unique);
    let aoem_delta = aoem.saturating_sub(previous_aoem);
    let canonical_delta = canonical.saturating_sub(previous_canonical);
    let ledger_close_delta = ledger_close.saturating_sub(previous_ledger_close);
    let child_tick_delta = ticks.saturating_sub(previous_ticks);
    let aoem_tick_delta = aoem_ticks.saturating_sub(previous_aoem_ticks);
    let pending_selected_delta = pending_selected.saturating_sub(previous_pending_selected);
    let object_ready_delta = object_ready.saturating_sub(previous_object_ready);
    let batch_ready_delta = batch_ready.saturating_sub(previous_batch_ready);
    let batch_received_delta = batch_received.saturating_sub(previous_batch_received);
    let tx_ingress_call_delta = tx_ingress_calls.saturating_sub(previous_tx_ingress_calls);
    let result_ready_delta = result_ready.saturating_sub(previous_result_ready);
    let result_verified_delta = result_verified.saturating_sub(previous_result_verified);
    let pending_delta = if pending_last >= previous_pending_last {
        pending_last
            .saturating_sub(previous_pending_last)
            .min(i64::MAX as u64) as i64
    } else {
        -(previous_pending_last
            .saturating_sub(pending_last)
            .min(i64::MAX as u64) as i64)
    };
    let pending_delta_direction = if pending_last > previous_pending_last {
        "increase"
    } else if pending_last < previous_pending_last {
        "decrease"
    } else {
        "stable"
    };
    let pending_delta_abs = pending_last.abs_diff(previous_pending_last);
    let durable_missing_delta_abs = durable_missing.abs_diff(previous_durable_missing);
    let durable_missing_delta_direction = if durable_missing > previous_durable_missing {
        "increase"
    } else if durable_missing < previous_durable_missing {
        "decrease"
    } else {
        "stable"
    };
    let durable_missing_decrease = previous_durable_missing.saturating_sub(durable_missing);
    let repair_convergence_rate_tps_x1000 = if elapsed_delta_ms == 0 {
        0
    } else {
        ledger_close_delta.saturating_mul(1_000_000) / elapsed_delta_ms
    };
    let repair_convergence_rate_tps = repair_convergence_rate_tps_x1000 / 1000;
    let missing_count_decrease_per_minute = if elapsed_delta_ms == 0 {
        0
    } else {
        durable_missing_decrease.saturating_mul(60_000) / elapsed_delta_ms
    };
    let repair_effective_completion_per_1000_packets = if network_received_delta == 0 {
        0
    } else {
        ledger_close_delta.saturating_mul(1000) / network_received_delta
    };

    let pipeline_stage_liveness_stalled = pending_last > 0
        && canonical_delta == 0
        && ledger_close_delta == 0
        && (object_ready_delta > 0
            || batch_ready_delta > 0
            || batch_received_delta > 0
            || tx_ingress_call_delta > 0
            || result_ready_delta > 0
            || result_verified_delta > 0
            || child_tick_delta == 0
            || pending_selected_delta == 0);
    let all_pipeline_stage_deltas_zero = object_ready_delta == 0
        && batch_ready_delta == 0
        && batch_received_delta == 0
        && tx_ingress_call_delta == 0
        && result_ready_delta == 0
        && result_verified_delta == 0;
    let pipeline_pending_drain_stall_reason = if pending_last > 0 && child_tick_delta == 0 {
        "pending_drain_callsite_stall"
    } else if pipeline_stage_liveness_stalled && object_ready_delta > 0 && batch_ready_delta == 0 {
        "object_assembler_stall"
    } else if pipeline_stage_liveness_stalled && batch_ready_delta > 0 && batch_received_delta == 0
    {
        "aoem_runtime_worker_input_stall"
    } else if pipeline_stage_liveness_stalled
        && batch_received_delta > 0
        && tx_ingress_call_delta == 0
    {
        "aoem_runtime_worker_submit_stall"
    } else if pipeline_stage_liveness_stalled
        && tx_ingress_call_delta > 0
        && result_ready_delta == 0
    {
        "aoem_runtime_worker_result_drain_stall"
    } else if pipeline_stage_liveness_stalled
        && result_ready_delta > 0
        && result_verified_delta == 0
    {
        "finality_report_worker_backpressure"
    } else if pipeline_stage_liveness_stalled
        && result_verified_delta > 0
        && canonical_delta == 0
        && ledger_close_delta == 0
    {
        "proof_close_ledger_projection_stall"
    } else if pipeline_stage_liveness_stalled && pending_selected_delta == 0 {
        "pending_drain_callsite_stall"
    } else if pending_last > 0 && all_pipeline_stage_deltas_zero && canonical_delta == 0 {
        "pending_drain_callsite_stall"
    } else {
        "none"
    };
    let pipeline_pending_drain_stall =
        pending_last > 0 && pipeline_pending_drain_stall_reason != "none";
    let stall_reason = if pipeline_pending_drain_stall {
        pipeline_pending_drain_stall_reason
    } else if pending_last > 0 && child_tick_delta == 0 {
        "receiver_child_tick_stall"
    } else if pending_last > 0
        && child_tick_delta > 0
        && pending_selected_delta == 0
        && aoem_delta == 0
    {
        "admission_drain_stall"
    } else if pending_selected_delta > 0 && aoem_delta == 0 {
        "batch_submit_stall"
    } else if aoem_delta > 0 && canonical_delta == 0 {
        "receipt_canonical_projection_stall"
    } else if ledger_close_delta > 0 && canonical_delta == 0 {
        "canonical_summary_lag"
    } else if network_received_delta == 0 && received_unique_delta == 0 && pending_last == 0 {
        "waiting_for_sender"
    } else {
        "progressing"
    };

    sample["receiver_ingress_drain_delta_available"] = serde_json::json!(true);
    sample["receiver_delta_elapsed_ms"] = serde_json::json!(elapsed_delta_ms);
    sample["receiver_udp_packet_recv_delta"] = serde_json::json!(network_received_delta);
    sample["receiver_sequence_unique_delta"] = serde_json::json!(received_unique_delta);
    sample["receiver_aoem_executed_delta_raw"] = serde_json::json!(aoem_delta);
    sample["receiver_canonical_delta_raw"] = serde_json::json!(canonical_delta);
    sample["receiver_ledger_close_delta_raw"] = serde_json::json!(ledger_close_delta);
    sample["receiver_child_tick_delta"] = serde_json::json!(child_tick_delta);
    sample["receiver_aoem_tick_delta"] = serde_json::json!(aoem_tick_delta);
    sample["receiver_pending_selected_delta"] = serde_json::json!(pending_selected_delta);
    sample["network_receiver_object_ready_delta"] = serde_json::json!(object_ready_delta);
    sample["object_assembler_batch_ready_delta"] = serde_json::json!(batch_ready_delta);
    sample["aoem_runtime_worker_batch_received_delta"] = serde_json::json!(batch_received_delta);
    sample["aoem_runtime_worker_tx_ingress_call_delta"] = serde_json::json!(tx_ingress_call_delta);
    sample["aoem_runtime_worker_result_ready_delta"] = serde_json::json!(result_ready_delta);
    sample["finality_report_worker_result_verified_delta"] =
        serde_json::json!(result_verified_delta);
    sample["queue_pending_delta"] = serde_json::json!(pending_delta);
    sample["receiver_pending_delta_abs"] = serde_json::json!(pending_delta_abs);
    sample["receiver_pending_delta_direction"] = serde_json::json!(pending_delta_direction);
    sample["receiver_durable_missing_delta_abs"] = serde_json::json!(durable_missing_delta_abs);
    sample["receiver_durable_missing_delta_direction"] =
        serde_json::json!(durable_missing_delta_direction);
    sample["receiver_durable_missing_decrease_delta"] = serde_json::json!(durable_missing_decrease);
    sample["repair_convergence_rate_tps"] = serde_json::json!(repair_convergence_rate_tps);
    sample["repair_convergence_rate_tps_x1000"] =
        serde_json::json!(repair_convergence_rate_tps_x1000);
    sample["missing_count_delta_per_minute"] = serde_json::json!(missing_count_decrease_per_minute);
    sample["missing_count_decrease_per_minute"] =
        serde_json::json!(missing_count_decrease_per_minute);
    sample["repair_effective_completion_per_1000_packets"] =
        serde_json::json!(repair_effective_completion_per_1000_packets);
    sample["receiver_child_tick_stall"] =
        serde_json::json!(pending_last > 0 && child_tick_delta == 0);
    sample["receiver_child_tick_last_progress_ms"] = if child_tick_delta > 0 {
        serde_json::json!(elapsed_ms)
    } else {
        Value::Null
    };
    sample["receiver_child_tick_stall_ms"] =
        serde_json::json!(if pending_last > 0 && child_tick_delta == 0 {
            elapsed_delta_ms
        } else {
            0
        });
    sample["receiver_child_tick_stall_reason"] =
        serde_json::json!(if pending_last > 0 && child_tick_delta == 0 {
            "pending_drain_callsite_stall"
        } else {
            "none"
        });
    sample["receiver_admission_drain_stall"] = serde_json::json!(
        pending_last > 0 && child_tick_delta > 0 && pending_selected_delta == 0 && aoem_delta == 0
    );
    sample["receiver_batch_submit_stall"] =
        serde_json::json!(pending_selected_delta > 0 && aoem_delta == 0);
    sample["receiver_receipt_canonical_projection_stall"] =
        serde_json::json!(aoem_delta > 0 && canonical_delta == 0);
    sample["pipeline_stage_liveness_stalled"] = serde_json::json!(pipeline_stage_liveness_stalled);
    sample["pipeline_pending_drain_stall"] = serde_json::json!(pipeline_pending_drain_stall);
    sample["pipeline_pending_drain_stall_reason"] =
        serde_json::json!(pipeline_pending_drain_stall_reason);
    sample["object_assembler_stall_reason"] = serde_json::json!(
        if pipeline_pending_drain_stall_reason == "object_assembler_stall" {
            "object_ready_not_batched"
        } else {
            "none"
        }
    );
    sample["aoem_runtime_worker_stall_reason"] =
        serde_json::json!(if pipeline_pending_drain_stall_reason
            == "aoem_runtime_worker_input_stall"
        {
            "batch_ready_not_received"
        } else if pipeline_pending_drain_stall_reason == "aoem_runtime_worker_submit_stall" {
            "batch_received_not_submitted"
        } else if pipeline_pending_drain_stall_reason == "aoem_runtime_worker_result_drain_stall" {
            "tx_ingress_call_without_result"
        } else {
            "none"
        });
    sample["aoem_runtime_worker_backpressure_reason"] = serde_json::json!(
        if pipeline_pending_drain_stall_reason.starts_with("aoem_runtime_worker") {
            pipeline_pending_drain_stall_reason
        } else {
            "none"
        }
    );
    sample["finality_report_worker_backpressure_reason"] =
        serde_json::json!(if pipeline_pending_drain_stall_reason
            == "finality_report_worker_backpressure"
        {
            "result_ready_not_verified"
        } else {
            "none"
        });
    sample["pending_drain_attempt_count"] =
        serde_json::json!(child_tick_delta.max(pending_selected_delta));
    sample["pending_drain_success_count"] = serde_json::json!(pending_selected_delta);
    sample["pending_drain_attempt_delta"] =
        serde_json::json!(child_tick_delta.max(pending_selected_delta));
    sample["pending_drain_success_delta"] = serde_json::json!(pending_selected_delta);
    sample["pending_drain_callsite_last_attempt_ms"] =
        if child_tick_delta > 0 || pending_selected_delta > 0 {
            serde_json::json!(elapsed_ms)
        } else {
            Value::Null
        };
    sample["pending_drain_callsite_last_success_ms"] = if pending_selected_delta > 0 {
        serde_json::json!(elapsed_ms)
    } else {
        Value::Null
    };
    sample["pending_drain_zero_count"] =
        serde_json::json!(if pending_last > 0 && pending_selected_delta == 0 {
            child_tick_delta
        } else {
            0
        });
    sample["canonical_progress_last_ms"] = if canonical_delta > 0 {
        serde_json::json!(elapsed_ms)
    } else {
        Value::Null
    };
    sample["ledger_progress_last_ms"] = if ledger_close_delta > 0 {
        serde_json::json!(elapsed_ms)
    } else {
        Value::Null
    };
    sample["receiver_drain_stall_reason"] = serde_json::json!(stall_reason);
    annotate_mini_tps_sync_gate_v1(sample);
}

fn annotate_receiver_repair_range_coverage_v1(sample: &mut Value) {
    let durable_missing = missing_ranges_from_json(
        sample
            .get("ledger_durable_missing_ranges_sample")
            .unwrap_or(&Value::Null),
    );
    let repair_received = missing_ranges_from_json(
        sample
            .get("repair_sequence_received_ranges_sample")
            .unwrap_or(&Value::Null),
    );
    let repair_accepted = missing_ranges_from_json(
        sample
            .get("repair_sequence_accepted_ranges_sample")
            .unwrap_or(&Value::Null),
    );
    let repair_executed = missing_ranges_from_json(
        sample
            .get("repair_sequence_admitted_to_aoem_ranges_sample")
            .unwrap_or(&Value::Null),
    );
    let repair_duplicate = missing_ranges_from_json(
        sample
            .get("repair_sequence_duplicate_ranges_sample")
            .unwrap_or(&Value::Null),
    );

    let durable_missing_count = missing_ranges_count(durable_missing.as_slice());
    let received_overlap =
        missing_ranges_overlap_count(repair_received.as_slice(), durable_missing.as_slice());
    let accepted_overlap =
        missing_ranges_overlap_count(repair_accepted.as_slice(), durable_missing.as_slice());
    let executed_overlap =
        missing_ranges_overlap_count(repair_executed.as_slice(), durable_missing.as_slice());
    let duplicate_sequence_count = missing_ranges_count(repair_duplicate.as_slice());
    let received_sequence_count = missing_ranges_count(repair_received.as_slice());
    let duplicate_waste_ratio_bps = if received_sequence_count == 0 {
        0
    } else {
        duplicate_sequence_count.saturating_mul(10_000) / received_sequence_count
    };

    sample["receiver_durable_missing_ranges_sample"] =
        missing_ranges_to_json(durable_missing.as_slice(), u64::MAX);
    sample["receiver_durable_missing_ranges_sequence_count"] =
        serde_json::json!(durable_missing_count);
    sample["receiver_repair_received_ranges_sample"] =
        missing_ranges_to_json(repair_received.as_slice(), u64::MAX);
    sample["receiver_repair_accepted_ranges_sample"] =
        missing_ranges_to_json(repair_accepted.as_slice(), u64::MAX);
    sample["receiver_repair_executed_ranges_sample"] =
        missing_ranges_to_json(repair_executed.as_slice(), u64::MAX);
    sample["receiver_repair_received_overlap_missing_count"] = serde_json::json!(received_overlap);
    sample["receiver_repair_accepted_overlap_missing_count"] = serde_json::json!(accepted_overlap);
    sample["receiver_repair_executed_overlap_missing_count"] = serde_json::json!(executed_overlap);
    sample["receiver_repair_received_new_missing_coverage_count"] =
        serde_json::json!(received_overlap);
    sample["receiver_repair_duplicate_ranges_count"] = serde_json::json!(duplicate_sequence_count);
    sample["repair_duplicate_waste_ratio"] = serde_json::json!(duplicate_waste_ratio_bps);
    sample["repair_duplicate_waste_ratio_bps"] = serde_json::json!(duplicate_waste_ratio_bps);
}

fn diagnostics_summary_sample(
    started_at: Instant,
    summary: &Value,
    ledger_stats: Value,
    rocksdb_probe: Value,
    memory_sample: Value,
    previous_canonical: u64,
) -> Value {
    let canonical = summary_u64(summary, "included_canonical_total");
    let aoem = summary_u64(summary, "aoem_executed_total");
    let ledger_completed = summary_u64(summary, "ledger_completed_count");
    let network_received_total = summary_u64(summary, "network_received_total");
    let ingress_submitted_total = summary_u64(summary, "ingress_submitted_total");
    let product_ingress_submitted_total = summary_u64(summary, "product_ingress_submitted_total");
    let queue_admitted_total = summary_u64(summary, "queue_admitted_total");
    let nonempty_aoem_batch_ticks = summary_u64(summary, "nonempty_aoem_batch_ticks");
    let pending_last = summary_u64(summary, "queue_pending_last");
    let active_pending = summary_u64(summary, "queue_active_pending_last");
    let ledger_lines = ledger_stats
        .get("line_count")
        .and_then(Value::as_u64)
        .unwrap_or_default();
    let stable_progress_total = canonical.max(aoem).max(ledger_lines);
    let proof = summary_u64(summary, "max_proof_items_per_tick");
    let commit = summary_u64(summary, "max_commit_items_per_tick");
    let max_queue_admitted = summary_u64(summary, "max_queue_admitted_per_tick");
    let max_network_received = summary_u64(summary, "max_network_received_per_tick");
    let max_broadcast_tx = summary_u64(summary, "max_broadcast_tx_per_tick");
    let ticks = summary_u64(summary, "ticks");
    let working_set_bytes = memory_working_set_bytes(&memory_sample);
    let private_bytes = memory_private_bytes(&memory_sample);
    let virtual_bytes = memory_virtual_bytes(&memory_sample);
    let paged_bytes = memory_paged_bytes(&memory_sample);
    let paged_system_bytes = memory_paged_system_bytes(&memory_sample);
    let nonpaged_system_bytes = memory_nonpaged_system_bytes(&memory_sample);
    let process_handle_count = memory_handle_count(&memory_sample);
    let process_thread_count = memory_thread_count(&memory_sample);
    let thread_count_per_1000_tx = if stable_progress_total == 0 {
        0
    } else {
        process_thread_count.saturating_mul(1000) / stable_progress_total
    };
    let handle_count_per_1000_tx = if stable_progress_total == 0 {
        0
    } else {
        process_handle_count.saturating_mul(1000) / stable_progress_total
    };
    let thread_growth_suspected = stable_progress_total >= 256 && thread_count_per_1000_tx > 128;
    let runtime_current_view_bytes =
        summary_u64(summary, "queue_tx_count_last").saturating_mul(256);
    let diagnostics_report_estimated_bytes = summary_u64(summary, "ticks")
        .saturating_mul(0)
        .saturating_add(0);
    let semantic_ledger_mirror_bytes = ledger_stats
        .get("bytes")
        .and_then(Value::as_u64)
        .unwrap_or_default();
    let rocksdb_total_estimated_memory_bytes = rocksdb_probe
        .get("rocksdb_total_estimated_memory_bytes")
        .and_then(Value::as_u64)
        .unwrap_or_default();
    let rocksdb_block_cache_estimated_bytes = rocksdb_probe
        .get("rocksdb_block_cache_estimated_bytes")
        .and_then(Value::as_u64)
        .unwrap_or_default();
    let rocksdb_memtable_estimated_bytes = rocksdb_probe
        .get("rocksdb_memtable_estimated_bytes")
        .and_then(Value::as_u64)
        .unwrap_or_default();
    let rocksdb_index_filter_estimated_bytes = rocksdb_probe
        .get("rocksdb_index_filter_estimated_bytes")
        .and_then(Value::as_u64)
        .unwrap_or_default();
    let native_store_materialized_bytes =
        summary_u64(summary, "native_store_materialized_estimated_bytes_max");
    let native_store_clone_bytes =
        summary_u64(summary, "native_store_previous_clone_estimated_bytes_max");
    let rust_estimated_retained_bytes = runtime_current_view_bytes
        .saturating_add(semantic_ledger_mirror_bytes)
        .saturating_add(native_store_materialized_bytes)
        .saturating_add(native_store_clone_bytes);
    let attributed_bytes =
        rust_estimated_retained_bytes.saturating_add(rocksdb_total_estimated_memory_bytes);
    let unattributed_working_set_bytes = working_set_bytes.saturating_sub(attributed_bytes);
    let unattributed_private_bytes = private_bytes.saturating_sub(attributed_bytes);
    let native_heap_unattributed_bytes = unattributed_private_bytes;
    let working_set_minus_private_bytes = working_set_bytes.saturating_sub(private_bytes);
    let private_minus_working_set_bytes = private_bytes.saturating_sub(working_set_bytes);
    let working_set_bytes_per_1000_tx = bytes_per_1000_tx(working_set_bytes, stable_progress_total);
    let private_bytes_per_1000_tx = bytes_per_1000_tx(private_bytes, stable_progress_total);
    let native_heap_unattributed_bytes_per_1000_tx =
        bytes_per_1000_tx(native_heap_unattributed_bytes, stable_progress_total);
    let attributed_bytes_per_1000_tx = bytes_per_1000_tx(attributed_bytes, stable_progress_total);
    let aoem_batch_input_bytes = max_queue_admitted.saturating_mul(1024);
    let aoem_batch_output_bytes =
        summary_u64(summary, "max_aoem_batch_executed_per_tick").saturating_mul(2048);
    let aoem_runtime_estimated_bytes =
        aoem_batch_input_bytes.saturating_add(aoem_batch_output_bytes);
    let proof_projection_bytes = proof.saturating_mul(1024);
    let receipt_projection_bytes = native_store_materialized_bytes
        .saturating_add(native_store_clone_bytes)
        .saturating_add(commit.saturating_mul(1024));
    let canonical_projection_bytes = summary_u64(summary, "included_canonical_last")
        .saturating_add(summary_u64(summary, "included_canonical_total"))
        .saturating_mul(256);
    let udp_receive_buffer_bytes = max_network_received.saturating_mul(4096);
    let decode_buffer_bytes = max_network_received.saturating_mul(2048);
    let json_serialization_buffer_bytes = ticks.min(256).saturating_mul(2048);
    let tick_vec_capacity_bytes = max_queue_admitted
        .saturating_add(proof)
        .saturating_add(commit)
        .saturating_add(max_broadcast_tx)
        .saturating_mul(256);
    let batch_vec_capacity_bytes = max_queue_admitted
        .max(summary_u64(summary, "max_aoem_batch_executed_per_tick"))
        .saturating_mul(1024);
    let stage_estimated_bytes_total = aoem_runtime_estimated_bytes
        .saturating_add(proof_projection_bytes)
        .saturating_add(receipt_projection_bytes)
        .saturating_add(canonical_projection_bytes)
        .saturating_add(udp_receive_buffer_bytes)
        .saturating_add(decode_buffer_bytes)
        .saturating_add(json_serialization_buffer_bytes)
        .saturating_add(tick_vec_capacity_bytes)
        .saturating_add(batch_vec_capacity_bytes);
    let unknown_native_heap_source = native_heap_unattributed_bytes
        > stage_estimated_bytes_total.saturating_add(64 * 1024 * 1024);
    let native_heap_unattributed_bytes_per_tick = if ticks == 0 {
        0
    } else {
        native_heap_unattributed_bytes / ticks
    };
    let large_allocation_suspected_stage = if native_store_materialized_bytes
        .saturating_add(native_store_clone_bytes)
        > 64 * 1024 * 1024
    {
        "native_store_materialization"
    } else if unknown_native_heap_source {
        "unknown_native_heap_source"
    } else if aoem_runtime_estimated_bytes > stage_estimated_bytes_total / 2 {
        "aoem_batch_buffers"
    } else {
        "none"
    };
    let thread_growth_stage_suspected = if thread_growth_suspected
        && unknown_native_heap_source
        && private_bytes > working_set_bytes.saturating_sub(256 * 1024 * 1024)
    {
        "child_runtime_or_aoem_ffi_worker_pool"
    } else if thread_growth_suspected {
        "receiver_child_thread_growth"
    } else {
        "none"
    };
    let allocator_fragmentation_suspected =
        unattributed_private_bytes > attributed_bytes.max(64 * 1024 * 1024);
    let working_set_not_returned_suspected =
        working_set_bytes > private_bytes.saturating_add(256 * 1024 * 1024) && private_bytes > 0;
    let summary_consistency_reasons =
        receiver_summary_consistency_reasons_v1(aoem, canonical, ledger_completed);
    let summary_consistency_violation_count = summary_consistency_reasons.len() as u64;
    let receiver_drain_attribution_stage = receiver_drain_attribution_stage_v1(
        network_received_total,
        ingress_submitted_total,
        pending_last,
        ticks,
        queue_admitted_total,
        nonempty_aoem_batch_ticks,
        aoem,
        canonical,
        ledger_completed,
    );
    let mut out = serde_json::json!({
        "elapsed_ms": started_at.elapsed().as_millis() as u64,
        "received_unique_total": summary_u64(summary, "ingress_total_last"),
        "canonical_unique_included_total": canonical,
        "stable_progress_total": stable_progress_total,
        "canonical_delta_since_last_sample": stable_progress_total.saturating_sub(previous_canonical),
        "pending_count": summary_u64(summary, "queue_pending_last"),
        "eligible_count": null,
        "skipped_ineligible_count": summary_u64(summary, "skipped_ineligible_stage_total"),
        "skipped_already_receipted_count": summary_u64(summary, "skipped_already_receipted_total"),
        "skipped_missing_payload_total": summary_u64(summary, "skipped_missing_payload_total"),
        "skipped_non_native_payload_total": summary_u64(summary, "skipped_non_native_payload_total"),
        "skipped_chain_mismatch_total": summary_u64(summary, "skipped_chain_mismatch_total"),
        "receipt_lookup_count": null,
        "receipt_lookup_hit_count": summary_u64(summary, "skipped_already_receipted_total"),
        "receipt_lookup_miss_count": null,
        "receipt_lookup_elapsed_ms": null,
        "aoem_executed_total": aoem,
        "aoem_executed_delta": stable_progress_total.saturating_sub(previous_canonical),
        "aoem_batch_elapsed_ms": null,
        "proof_items_total": proof,
        "proof_delta": null,
        "proof_elapsed_ms": null,
        "commit_items_total": commit,
        "commit_delta": null,
        "rocksdb_read_elapsed_ms": null,
        "rocksdb_write_elapsed_ms": null,
        "semantic_head_height": canonical,
        "semantic_head_monotonic": true,
        "semantic_ledger_mirror": ledger_stats,
        "rocksdb_memory_probe": rocksdb_probe,
        "process_memory": memory_sample,
        "process_working_set_bytes": working_set_bytes,
        "process_private_bytes": private_bytes,
        "virtual_bytes": virtual_bytes,
        "process_virtual_bytes": virtual_bytes,
        "process_paged_bytes": paged_bytes,
        "process_paged_system_bytes": paged_system_bytes,
        "process_nonpaged_system_bytes": nonpaged_system_bytes,
        "process_handle_count": process_handle_count,
        "process_thread_count": process_thread_count,
        "thread_count": process_thread_count,
        "thread_count_per_1000_tx": thread_count_per_1000_tx,
        "handle_count": process_handle_count,
        "handle_count_per_1000_tx": handle_count_per_1000_tx,
        "thread_growth_suspected": thread_growth_suspected,
        "thread_growth_stage_suspected": thread_growth_stage_suspected,
        "runtime_created_count": 0u64,
        "tokio_runtime_created_count": 0u64,
        "blocking_task_spawn_count": 0u64,
        "std_thread_spawn_count": 0u64,
        "aoem_worker_pool_created_count": 0u64,
        "rocksdb_probe_thread_count": 0u64,
        "diagnostics_thread_count": 0u64,
        "report_writer_thread_count": 0u64,
        "rust_estimated_retained_bytes": rust_estimated_retained_bytes,
        "pending_runtime_estimated_bytes": runtime_current_view_bytes,
        "runtime_current_view_bytes_estimate": runtime_current_view_bytes,
        "diagnostics_report_estimated_bytes": diagnostics_report_estimated_bytes,
        "semantic_ledger_mirror_bytes": semantic_ledger_mirror_bytes,
        "jsonl_writer_buffer_bytes": 0u64,
        "native_store_materialized_estimated_bytes": native_store_materialized_bytes,
        "native_store_previous_clone_estimated_bytes": native_store_clone_bytes,
        "rocksdb_total_estimated_memory_bytes": rocksdb_total_estimated_memory_bytes,
        "rocksdb_block_cache_estimated_bytes": rocksdb_block_cache_estimated_bytes,
        "rocksdb_memtable_estimated_bytes": rocksdb_memtable_estimated_bytes,
        "rocksdb_index_filter_estimated_bytes": rocksdb_index_filter_estimated_bytes,
        "native_heap_unattributed_bytes": native_heap_unattributed_bytes,
        "unattributed_private_bytes": unattributed_private_bytes,
        "unattributed_working_set_bytes": unattributed_working_set_bytes,
        "working_set_minus_private_bytes": working_set_minus_private_bytes,
        "private_minus_working_set_bytes": private_minus_working_set_bytes,
        "allocator_fragmentation_suspected": allocator_fragmentation_suspected,
        "working_set_not_returned_suspected": working_set_not_returned_suspected,
        "working_set_bytes_per_1000_tx": working_set_bytes_per_1000_tx,
        "private_bytes_per_1000_tx": private_bytes_per_1000_tx,
        "native_heap_unattributed_bytes_per_1000_tx": native_heap_unattributed_bytes_per_1000_tx,
        "attributed_bytes_per_1000_tx": attributed_bytes_per_1000_tx,
        "queue_pending_last": pending_last,
        "queue_dropped_total": summary_u64(summary, "queue_dropped_last"),
        "queue_rejected_total": summary_u64(summary, "queue_rejected_last"),
        "receiver_udp_packet_recv_count": network_received_total,
        "receiver_udp_packet_recv_source": "network.udp.received_count_total",
        "receiver_udp_packet_recv_max_per_tick": max_network_received,
        "receiver_udp_packet_decode_ok_count": ingress_submitted_total,
        "receiver_udp_packet_decode_ok_source": "ingress_drive.submitted_total",
        "receiver_udp_packet_decode_error_count": summary_u64(summary, "ingress_error_ticks"),
        "receiver_sequence_accepted_count": summary_u64(summary, "ingress_total_last"),
        "receiver_sequence_duplicate_count": summary_u64(summary, "repair_sequence_duplicate_count"),
        "receiver_sequence_rejected_count": summary_u64(summary, "queue_rejected_last").saturating_add(summary_u64(summary, "repair_sequence_rejected_count")),
        "receiver_pending_enqueue_count": product_ingress_submitted_total.max(ingress_submitted_total),
        "receiver_pending_active_count": active_pending,
        "receiver_child_tick_count": ticks,
        "receiver_aoem_tick_count": nonempty_aoem_batch_ticks,
        "receiver_pending_selected_count": queue_admitted_total,
        "receiver_raw_txs_count": queue_admitted_total,
        "receiver_actual_batch_count": nonempty_aoem_batch_ticks,
        "receiver_actual_batch_tx_count": aoem,
        "receiver_aoem_batch_submit_count": nonempty_aoem_batch_ticks,
        "receiver_aoem_batch_result_count": aoem,
        "receiver_receipt_written_count": summary_u64(summary, "ledger_receipt_proof_close_success_count").max(summary_u64(summary, "ledger_missing_closed_by_receipt_count")),
        "receiver_canonical_project_attempt_count": summary_u64(summary, "canonical_projection_success_ticks"),
        "receiver_canonical_project_success_count": summary_u64(summary, "included_canonical_projected_total"),
        "receiver_canonical_included_count": canonical,
        "receiver_canonical_retained_count": summary_u64(summary, "included_canonical_retained_last"),
        "receiver_ledger_close_by_receipt_count": summary_u64(summary, "ledger_missing_closed_by_receipt_count"),
        "receiver_ledger_close_by_canonical_count": summary_u64(summary, "ledger_missing_closed_by_canonical_count"),
        "receiver_ledger_close_count": ledger_completed,
        "summary_source_canonical": summary.get("included_canonical_total_source").cloned().unwrap_or_else(|| serde_json::json!("child_progress.included_canonical_total")),
        "summary_source_aoem": "child_progress.aoem_executed_total",
        "summary_source_ledger": "child_progress.ledger_completed_count",
        "summary_source_pending": "child_progress.queue_pending_last",
        "receiver_summary_canonical_source": summary.get("included_canonical_total_source").cloned().unwrap_or_else(|| serde_json::json!("child_progress.included_canonical_total")),
        "receiver_summary_ledger_source": "child_progress.ledger_completed_count",
        "receiver_canonical_projection_stall_reason": if aoem > canonical {
            "canonical_total_lags_aoem"
        } else if summary_u64(summary, "included_canonical_retained_last") < canonical {
            "queue_retained_summary_lags_projection_proof"
        } else {
            "none"
        },
        "summary_consistency_violation_count": summary_consistency_violation_count,
        "summary_consistency_violation_reasons": summary_consistency_reasons,
        "summary_aoem_gt_canonical_lag_count": aoem.saturating_sub(canonical),
        "summary_ledger_gt_canonical_lag_count": ledger_completed.saturating_sub(canonical),
        "receiver_drain_attribution_stage": receiver_drain_attribution_stage,
        "repair_packet_received_count": summary_u64(summary, "repair_packet_received_count"),
        "repair_packet_decode_failed_count": summary_u64(summary, "repair_packet_decode_failed_count"),
        "repair_sequence_received_count": summary_u64(summary, "repair_sequence_received_count"),
        "repair_sequence_received_min": summary.get("repair_sequence_received_min").cloned().unwrap_or(Value::Null),
        "repair_sequence_received_max": summary.get("repair_sequence_received_max").cloned().unwrap_or(Value::Null),
        "repair_sequence_received_ranges_sample": summary.get("repair_sequence_received_ranges_sample").cloned().unwrap_or_else(|| serde_json::json!([])),
        "repair_sequence_accepted_ranges_sample": summary.get("repair_sequence_accepted_ranges_sample").cloned().unwrap_or_else(|| serde_json::json!([])),
        "repair_sequence_enqueued_ranges_sample": summary.get("repair_sequence_enqueued_ranges_sample").cloned().unwrap_or_else(|| serde_json::json!([])),
        "repair_sequence_already_receipted_ranges_sample": summary.get("repair_sequence_already_receipted_ranges_sample").cloned().unwrap_or_else(|| serde_json::json!([])),
        "repair_sequence_duplicate_ranges_sample": summary.get("repair_sequence_duplicate_ranges_sample").cloned().unwrap_or_else(|| serde_json::json!([])),
        "repair_sequence_admitted_to_aoem_ranges_sample": summary.get("repair_sequence_admitted_to_aoem_ranges_sample").cloned().unwrap_or_else(|| serde_json::json!([])),
        "repair_sequence_accepted_count": summary_u64(summary, "repair_sequence_accepted_count"),
        "repair_sequence_duplicate_count": summary_u64(summary, "repair_sequence_duplicate_count"),
        "repair_sequence_rejected_count": summary_u64(summary, "repair_sequence_rejected_count"),
        "repair_reject_reason_counts": summary.get("repair_reject_reason_counts").cloned().unwrap_or_else(|| serde_json::json!({})),
        "repair_reject_reason_samples": summary.get("repair_reject_reason_samples").cloned().unwrap_or_else(|| serde_json::json!([])),
        "repair_sequence_already_receipted_count": summary_u64(summary, "repair_sequence_already_receipted_count"),
        "repair_sequence_wrong_run_id_count": summary_u64(summary, "repair_sequence_wrong_run_id_count"),
        "repair_sequence_wrong_chain_id_count": summary_u64(summary, "repair_sequence_wrong_chain_id_count"),
        "repair_sequence_out_of_range_count": summary_u64(summary, "repair_sequence_out_of_range_count"),
        "repair_sequence_stale_count": summary_u64(summary, "repair_sequence_stale_count"),
        "repair_sequence_enqueued_count": summary_u64(summary, "repair_sequence_enqueued_count"),
        "repair_sequence_admitted_to_aoem_count": summary_u64(summary, "repair_sequence_admitted_to_aoem_count"),
        "repair_attempted_unreceipted_count": summary_u64(summary, "repair_attempted_unreceipted_count"),
        "repair_attempted_unreceipted_final_missing_overlap_count": summary_u64(summary, "repair_attempted_unreceipted_final_missing_overlap_count"),
        "repair_attempted_unreceipted_requeued_count": summary_u64(summary, "repair_attempted_unreceipted_requeued_count"),
        "repair_attempted_unreceipted_requeue_failed_count": summary_u64(summary, "repair_attempted_unreceipted_requeue_failed_count"),
        "repair_final_missing_payload_available_count": summary_u64(summary, "repair_final_missing_payload_available_count"),
        "repair_final_missing_payload_available_but_inactive_count": summary_u64(summary, "repair_final_missing_payload_available_but_inactive_count"),
        "repair_final_missing_invariant_violation_count": summary_u64(summary, "repair_final_missing_invariant_violation_count"),
        "repair_final_missing_sequence_to_tx_hash_count": summary_u64(summary, "repair_final_missing_sequence_to_tx_hash_count"),
        "repair_final_missing_tx_hash_payload_hit_count": summary_u64(summary, "repair_final_missing_tx_hash_payload_hit_count"),
        "repair_final_missing_payload_missing_by_sequence_count": summary_u64(summary, "repair_final_missing_payload_missing_by_sequence_count"),
        "repair_final_missing_payload_missing_ranges_sample": summary.get("repair_final_missing_payload_missing_ranges_sample").cloned().unwrap_or_else(|| serde_json::json!([])),
        "repair_sequence_payload_index_count": summary_u64(summary, "repair_sequence_payload_index_count"),
        "repair_sequence_payload_index_final_missing_overlap_count": summary_u64(summary, "repair_sequence_payload_index_final_missing_overlap_count"),
        "repair_sequence_payload_index_evicted_count": summary_u64(summary, "repair_sequence_payload_index_evicted_count"),
        "repair_payload_retention_false_negative_suspected": summary.get("repair_payload_retention_false_negative_suspected").and_then(Value::as_bool).unwrap_or(false),
        "repair_final_missing_payload_recovered_count": summary_u64(summary, "repair_final_missing_payload_recovered_count"),
        "repair_final_missing_payload_recovered_requeued_count": summary_u64(summary, "repair_final_missing_payload_recovered_requeued_count"),
        "ledger_expected_range_start": summary.get("ledger_expected_range_start").cloned().unwrap_or(Value::Null),
        "ledger_expected_range_end": summary.get("ledger_expected_range_end").cloned().unwrap_or(Value::Null),
        "ledger_expected_count": summary_u64(summary, "ledger_expected_count"),
        "ledger_completed_count": summary_u64(summary, "ledger_completed_count"),
        "ledger_durable_missing_count": summary_u64(summary, "ledger_durable_missing_count"),
        "ledger_durable_missing_ranges_sample": summary.get("ledger_durable_missing_ranges_sample").cloned().unwrap_or_else(|| serde_json::json!([])),
        "ledger_durable_missing_bitmap_available": summary.get("ledger_durable_missing_bitmap_available").and_then(Value::as_bool).unwrap_or(false),
        "ledger_durable_missing_source": summary.get("ledger_durable_missing_source").cloned().unwrap_or_else(|| serde_json::json!("")),
        "ledger_durable_missing_derived_from_expected_range": summary.get("ledger_durable_missing_derived_from_expected_range").and_then(Value::as_bool).unwrap_or(false),
        "ledger_missing_closed_by_receipt_count": summary_u64(summary, "ledger_missing_closed_by_receipt_count"),
        "ledger_missing_closed_by_canonical_count": summary_u64(summary, "ledger_missing_closed_by_canonical_count"),
        "ledger_missing_incorrectly_closed_by_received_count": summary_u64(summary, "ledger_missing_incorrectly_closed_by_received_count"),
        "ledger_missing_incorrectly_closed_by_enqueued_count": summary_u64(summary, "ledger_missing_incorrectly_closed_by_enqueued_count"),
        "ledger_close_source": summary.get("ledger_close_source").cloned().unwrap_or_else(|| serde_json::json!("")),
        "ledger_receipt_close_proof_count": summary_u64(summary, "ledger_receipt_close_proof_count"),
        "ledger_canonical_close_proof_count": summary_u64(summary, "ledger_canonical_close_proof_count"),
        "ledger_prefix_close_count": summary_u64(summary, "ledger_prefix_close_count"),
        "ledger_synthetic_close_count": summary_u64(summary, "ledger_synthetic_close_count"),
        "ledger_close_without_receipt_index_count": summary_u64(summary, "ledger_close_without_receipt_index_count"),
        "ledger_close_without_canonical_proof_count": summary_u64(summary, "ledger_close_without_canonical_proof_count"),
        "ledger_false_completed_invariant_violation_count": summary_u64(summary, "ledger_false_completed_invariant_violation_count"),
        "ledger_false_completed_sequences_sample": summary.get("ledger_false_completed_sequences_sample").cloned().unwrap_or_else(|| serde_json::json!([])),
        "ledger_validation_final_missing_overlap_count": summary_u64(summary, "ledger_validation_final_missing_overlap_count"),
        "ledger_durable_missing_validation_mismatch_count": summary_u64(summary, "ledger_durable_missing_validation_mismatch_count"),
        "ledger_receipt_proof_writer_called_count": summary_u64(summary, "ledger_receipt_proof_writer_called_count"),
        "ledger_canonical_proof_writer_called_count": summary_u64(summary, "ledger_canonical_proof_writer_called_count"),
        "ledger_receipt_proof_tx_hash_count": summary_u64(summary, "ledger_receipt_proof_tx_hash_count"),
        "ledger_canonical_proof_tx_hash_count": summary_u64(summary, "ledger_canonical_proof_tx_hash_count"),
        "ledger_receipt_proof_close_success_count": summary_u64(summary, "ledger_receipt_proof_close_success_count"),
        "ledger_canonical_proof_close_success_count": summary_u64(summary, "ledger_canonical_proof_close_success_count"),
        "ledger_receipt_proof_missing_sequence_mapping_count": summary_u64(summary, "ledger_receipt_proof_missing_sequence_mapping_count"),
        "ledger_canonical_proof_missing_sequence_mapping_count": summary_u64(summary, "ledger_canonical_proof_missing_sequence_mapping_count"),
        "ledger_close_blocked_by_count_only_canonical_progress_count": summary_u64(summary, "ledger_close_blocked_by_count_only_canonical_progress_count"),
        "ledger_close_blocked_reason": summary.get("ledger_close_blocked_reason").cloned().unwrap_or_else(|| serde_json::json!("")),
        "ledger_close_writer_runtime_instance": summary.get("ledger_close_writer_runtime_instance").cloned().unwrap_or_else(|| serde_json::json!("")),
        "ledger_close_writer_child_runtime_match": summary.get("ledger_close_writer_child_runtime_match").cloned().unwrap_or_else(|| serde_json::json!(false)),
        "ledger_completed_ranges_sample": summary.get("ledger_completed_ranges_sample").cloned().unwrap_or_else(|| serde_json::json!([])),
        "ack_current_window_start_after_proof_close": summary.get("ack_current_window_start_after_proof_close").cloned().unwrap_or(Value::Null),
        "ledger_candidate_rehydrated_count": summary_u64(summary, "ledger_candidate_rehydrated_count"),
        "ledger_candidate_empty_but_durable_missing_count": summary_u64(summary, "ledger_candidate_empty_but_durable_missing_count"),
        "ledger_missing_without_candidate_count": summary_u64(summary, "ledger_missing_without_candidate_count"),
        "ledger_missing_without_retryable_count": summary_u64(summary, "ledger_missing_without_retryable_count"),
        "ledger_candidate_count_exceeds_durable_missing_invariant_violation_count": summary_u64(summary, "ledger_candidate_count_exceeds_durable_missing_invariant_violation_count"),
        "ledger_final_missing_without_durable_missing_count": summary_u64(summary, "ledger_final_missing_without_durable_missing_count"),
        "ledger_final_missing_candidate_count": summary_u64(summary, "ledger_final_missing_candidate_count"),
        "ledger_final_missing_candidate_ranges_sample": summary.get("ledger_final_missing_candidate_ranges_sample").cloned().unwrap_or_else(|| serde_json::json!([])),
        "ledger_final_missing_requeued_before_admission_count": summary_u64(summary, "ledger_final_missing_requeued_before_admission_count"),
        "ledger_final_missing_admitted_count": summary_u64(summary, "ledger_final_missing_admitted_count"),
        "ledger_final_missing_admitted_ranges_sample": summary.get("ledger_final_missing_admitted_ranges_sample").cloned().unwrap_or_else(|| serde_json::json!([])),
        "ledger_final_missing_candidate_payload_available_count": summary_u64(summary, "ledger_final_missing_candidate_payload_available_count"),
        "ledger_final_missing_candidate_payload_available_ranges_sample": summary.get("ledger_final_missing_candidate_payload_available_ranges_sample").cloned().unwrap_or_else(|| serde_json::json!([])),
        "ledger_final_missing_candidate_payload_missing_count": summary_u64(summary, "ledger_final_missing_candidate_payload_missing_count"),
        "ledger_final_missing_candidate_tx_hash_mapping_missing_count": summary_u64(summary, "ledger_final_missing_candidate_tx_hash_mapping_missing_count"),
        "ledger_final_missing_candidate_raw_tx_build_error_count": summary_u64(summary, "ledger_final_missing_candidate_raw_tx_build_error_count"),
        "ledger_final_missing_payload_available_selected_count": summary_u64(summary, "ledger_final_missing_payload_available_selected_count"),
        "ledger_final_missing_payload_available_selected_ranges_sample": summary.get("ledger_final_missing_payload_available_selected_ranges_sample").cloned().unwrap_or_else(|| serde_json::json!([])),
        "ledger_final_missing_payload_available_not_selected_count": summary_u64(summary, "ledger_final_missing_payload_available_not_selected_count"),
        "ledger_final_missing_payload_available_selection_skipped_reason": summary.get("ledger_final_missing_payload_available_selection_skipped_reason").cloned().unwrap_or_else(|| serde_json::json!("")),
        "ledger_final_missing_selectable_count": summary_u64(summary, "ledger_final_missing_selectable_count"),
        "ledger_final_missing_selector_input_count": summary_u64(summary, "ledger_final_missing_selector_input_count"),
        "ledger_final_missing_selector_output_count": summary_u64(summary, "ledger_final_missing_selector_output_count"),
        "ledger_final_missing_selector_used_durable_bucket": summary.get("ledger_final_missing_selector_used_durable_bucket").and_then(Value::as_bool).unwrap_or(false),
        "ledger_final_missing_selector_skipped_by_old_pending_view_count": summary_u64(summary, "ledger_final_missing_selector_skipped_by_old_pending_view_count"),
        "ledger_final_missing_selected_not_pushed_to_raw_txs_count": summary_u64(summary, "ledger_final_missing_selected_not_pushed_to_raw_txs_count"),
        "ledger_final_missing_raw_txs_push_attempt_count": summary_u64(summary, "ledger_final_missing_raw_txs_push_attempt_count"),
        "ledger_final_missing_raw_txs_push_success_count": summary_u64(summary, "ledger_final_missing_raw_txs_push_success_count"),
        "ledger_final_missing_raw_txs_nonempty_but_not_submitted_count": summary_u64(summary, "ledger_final_missing_raw_txs_nonempty_but_not_submitted_count"),
        "ledger_final_missing_batch_blocked_reason": summary.get("ledger_final_missing_batch_blocked_reason").cloned().unwrap_or_else(|| serde_json::json!("")),
        "ledger_final_missing_batch_blocked_by_payload_missing_count": summary_u64(summary, "ledger_final_missing_batch_blocked_by_payload_missing_count"),
        "ledger_final_missing_batch_blocked_by_stage_filter_count": summary_u64(summary, "ledger_final_missing_batch_blocked_by_stage_filter_count"),
        "ledger_final_missing_batch_blocked_by_scan_limit_count": summary_u64(summary, "ledger_final_missing_batch_blocked_by_scan_limit_count"),
        "ledger_final_missing_batch_blocked_by_batch_limit_count": summary_u64(summary, "ledger_final_missing_batch_blocked_by_batch_limit_count"),
        "ledger_final_missing_batch_blocked_by_raw_tx_build_error_count": summary_u64(summary, "ledger_final_missing_batch_blocked_by_raw_tx_build_error_count"),
        "ledger_final_missing_batch_blocked_by_tx_hash_mapping_missing_count": summary_u64(summary, "ledger_final_missing_batch_blocked_by_tx_hash_mapping_missing_count"),
        "ledger_final_missing_batch_blocked_by_payload_available_not_selected_count": summary_u64(summary, "ledger_final_missing_batch_blocked_by_payload_available_not_selected_count"),
        "ledger_final_missing_batch_blocked_by_selected_not_pushed_count": summary_u64(summary, "ledger_final_missing_batch_blocked_by_selected_not_pushed_count"),
        "ledger_final_missing_batch_blocked_by_raw_txs_nonempty_not_submitted_count": summary_u64(summary, "ledger_final_missing_batch_blocked_by_raw_txs_nonempty_not_submitted_count"),
        "ledger_final_missing_batch_blocked_by_batch_not_full_count": summary_u64(summary, "ledger_final_missing_batch_blocked_by_batch_not_full_count"),
        "ledger_final_missing_batch_blocked_by_no_tick_executed_count": summary_u64(summary, "ledger_final_missing_batch_blocked_by_no_tick_executed_count"),
        "ledger_final_missing_batch_blocked_by_classification_path_not_reached_count": summary_u64(summary, "ledger_final_missing_batch_blocked_by_classification_path_not_reached_count"),
        "ledger_final_missing_batch_blocked_by_unknown_invariant_violation_count": summary_u64(summary, "ledger_final_missing_batch_blocked_by_unknown_invariant_violation_count"),
        "ledger_final_missing_batch_limit_config": summary_u64(summary, "ledger_final_missing_batch_limit_config"),
        "ledger_final_missing_reserved_batch_budget": summary_u64(summary, "ledger_final_missing_reserved_batch_budget"),
        "ledger_final_missing_batch_budget_before_fill": summary_u64(summary, "ledger_final_missing_batch_budget_before_fill"),
        "ledger_final_missing_batch_budget_after_fill": summary_u64(summary, "ledger_final_missing_batch_budget_after_fill"),
        "ledger_final_missing_batch_blocked_by_limit_after_actual_fill_count": summary_u64(summary, "ledger_final_missing_batch_blocked_by_limit_after_actual_fill_count"),
        "ledger_final_missing_batch_limit_zero_count": summary_u64(summary, "ledger_final_missing_batch_limit_zero_count"),
        "ledger_final_missing_preempted_normal_pending_count": summary_u64(summary, "ledger_final_missing_preempted_normal_pending_count"),
        "ledger_final_missing_batch_nonempty_submitted": summary.get("ledger_final_missing_batch_nonempty_submitted").and_then(Value::as_bool).unwrap_or(false),
        "ledger_final_missing_actual_batch_count": summary_u64(summary, "ledger_final_missing_actual_batch_count"),
        "ledger_final_missing_actual_batch_ranges_sample": summary.get("ledger_final_missing_actual_batch_ranges_sample").cloned().unwrap_or_else(|| serde_json::json!([])),
        "ledger_final_missing_raw_txs_count": summary_u64(summary, "ledger_final_missing_raw_txs_count"),
        "ledger_final_missing_batch_result_count": summary_u64(summary, "ledger_final_missing_batch_result_count"),
        "ledger_final_missing_receipt_written_count": summary_u64(summary, "ledger_final_missing_receipt_written_count"),
        "ledger_final_missing_receipt_missing_after_admission_count": summary_u64(summary, "ledger_final_missing_receipt_missing_after_admission_count"),
        "ledger_final_missing_inflight_count": summary_u64(summary, "ledger_final_missing_inflight_count"),
        "ledger_final_missing_retryable_count": summary_u64(summary, "ledger_final_missing_retryable_count"),
        "ledger_final_missing_requeued_after_no_receipt_count": summary_u64(summary, "ledger_final_missing_requeued_after_no_receipt_count"),
        "ledger_final_missing_admitted_but_no_receipt_invariant_violation_count": summary_u64(summary, "ledger_final_missing_admitted_but_no_receipt_invariant_violation_count"),
        "ledger_admission_counter_is_actual_batch": summary.get("ledger_admission_counter_is_actual_batch").and_then(Value::as_bool).unwrap_or(false),
        "ledger_admission_counter_mismatch_reason": summary.get("ledger_admission_counter_mismatch_reason").cloned().unwrap_or_else(|| serde_json::json!("")),
        "novorudp_trace_enabled": summary.get("novorudp_trace_enabled").and_then(Value::as_bool).unwrap_or(false),
        "trace_success_sequences_sample": summary.get("trace_success_sequences_sample").cloned().unwrap_or_else(|| serde_json::json!([])),
        "trace_failed_sequences_sample": summary.get("trace_failed_sequences_sample").cloned().unwrap_or_else(|| serde_json::json!([])),
        "trace_first_divergence_stage": summary.get("trace_first_divergence_stage").cloned().unwrap_or_else(|| serde_json::json!("")),
        "trace_first_divergence_sequence": summary.get("trace_first_divergence_sequence").cloned().unwrap_or(Value::Null),
        "trace_success_vs_failed_diff_summary": summary.get("trace_success_vs_failed_diff_summary").cloned().unwrap_or_else(|| serde_json::json!({})),
        "trace_candidate_payload_available_not_selected_sequences": summary.get("trace_candidate_payload_available_not_selected_sequences").cloned().unwrap_or_else(|| serde_json::json!([])),
        "trace_selected_not_pushed_sequences": summary.get("trace_selected_not_pushed_sequences").cloned().unwrap_or_else(|| serde_json::json!([])),
        "trace_pushed_not_batched_sequences": summary.get("trace_pushed_not_batched_sequences").cloned().unwrap_or_else(|| serde_json::json!([])),
        "trace_batched_not_receipted_sequences": summary.get("trace_batched_not_receipted_sequences").cloned().unwrap_or_else(|| serde_json::json!([])),
        "ledger_receipt_completion_attribution_available": ledger_receipt_completion_attribution_available_v1(Some(summary)),
        "ledger_receipt_completion_attribution_missing_reason": ledger_receipt_completion_missing_reason_v1(Some(summary)),
        "ledger_final_missing_admission_skipped_count": summary_u64(summary, "ledger_final_missing_admission_skipped_count"),
        "ledger_final_missing_admission_skip_reason_counts": summary.get("ledger_final_missing_admission_skip_reason_counts").cloned().unwrap_or_else(|| serde_json::json!({})),
        "admission_used_ledger_final_missing_bucket": summary.get("admission_used_ledger_final_missing_bucket").and_then(Value::as_bool).unwrap_or(false),
        "ticks": summary_u64(summary, "ticks"),
        "ticks_per_sec_x1000": summary_u64(summary, "ticks_per_sec_x1000"),
    });
    out["child_env_tx_count_raw"] = summary
        .get("child_env_tx_count_raw")
        .cloned()
        .unwrap_or(Value::Null);
    out["child_expected_total_from_env"] =
        serde_json::json!(summary_u64(summary, "child_expected_total_from_env"));
    out["child_expected_total_from_config"] =
        serde_json::json!(summary_u64(summary, "child_expected_total_from_config"));
    out["child_ledger_expected_range_init_called"] = serde_json::json!(summary
        .get("child_ledger_expected_range_init_called")
        .and_then(Value::as_bool)
        .unwrap_or(false));
    out["child_ledger_expected_range_init_source"] = summary
        .get("child_ledger_expected_range_init_source")
        .cloned()
        .unwrap_or_else(|| serde_json::json!(""));
    out["child_ledger_expected_range_init_error"] = summary
        .get("child_ledger_expected_range_init_error")
        .cloned()
        .unwrap_or(Value::Null);
    out["child_progress_summary_source"] = summary
        .get("child_progress_summary_source")
        .cloned()
        .unwrap_or_else(|| serde_json::json!(""));
    out["wrapper_progress_summary_source"] =
        serde_json::json!("receiver_wrapper_child_progress_report");
    out["aoem_runtime_estimated_bytes"] = serde_json::json!(aoem_runtime_estimated_bytes);
    out["aoem_batch_input_bytes"] = serde_json::json!(aoem_batch_input_bytes);
    out["aoem_batch_output_bytes"] = serde_json::json!(aoem_batch_output_bytes);
    out["aoem_projection_estimated_bytes"] = serde_json::json!(aoem_runtime_estimated_bytes);
    out["proof_projection_bytes"] = serde_json::json!(proof_projection_bytes);
    out["receipt_projection_bytes"] = serde_json::json!(receipt_projection_bytes);
    out["canonical_projection_bytes"] = serde_json::json!(canonical_projection_bytes);
    out["udp_receive_buffer_bytes"] = serde_json::json!(udp_receive_buffer_bytes);
    out["decode_buffer_bytes"] = serde_json::json!(decode_buffer_bytes);
    out["json_serialization_buffer_bytes"] = serde_json::json!(json_serialization_buffer_bytes);
    out["tick_vec_capacity_bytes"] = serde_json::json!(tick_vec_capacity_bytes);
    out["batch_vec_capacity_bytes"] = serde_json::json!(batch_vec_capacity_bytes);
    out["stage_estimated_bytes_total"] = serde_json::json!(stage_estimated_bytes_total);
    out["native_heap_unattributed_bytes_per_tick"] =
        serde_json::json!(native_heap_unattributed_bytes_per_tick);
    out["unknown_native_heap_source"] = serde_json::json!(unknown_native_heap_source);
    out["large_allocation_suspected_stage"] = serde_json::json!(large_allocation_suspected_stage);
    out["native_heap_source_isolation_confidence"] =
        serde_json::json!(if unknown_native_heap_source {
            "low_unknown_dominates"
        } else {
            "estimated_stage_attribution"
        });
    out["memory_probe_stage_switches"] = memory_probe_switches_report();
    out["active_pending_count"] =
        serde_json::json!(summary_u64(summary, "queue_active_pending_last"));
    out["historical_pending_count"] =
        serde_json::json!(summary_u64(summary, "queue_historical_pending_last"));
    out["current_view_received_retained"] =
        serde_json::json!(summary_u64(summary, "ingress_total_last"));
    out["current_view_included_retained"] =
        serde_json::json!(summary_u64(summary, "included_canonical_last"));
    out["current_view_dropped_retained"] =
        serde_json::json!(summary_u64(summary, "queue_dropped_last"));
    out["queue_dropped_last_active"] = serde_json::json!(0u64);
    out["queue_dropped_total_cumulative"] =
        serde_json::json!(summary_u64(summary, "queue_dropped_last"));
    out["historical_compacted_total"] =
        serde_json::json!(summary_u64(summary, "historical_compacted_total"));
    out["historical_payload_bytes_freed"] =
        serde_json::json!(summary_u64(summary, "historical_payload_bytes_freed"));
    out["tombstone_retained_count"] =
        serde_json::json!(summary_u64(summary, "tombstone_retained_count"));
    out["tombstone_evicted_count"] =
        serde_json::json!(summary_u64(summary, "tombstone_evicted_count"));
    out["historical_pending_after_compaction"] =
        serde_json::json!(summary_u64(summary, "historical_pending_after_compaction"));
    out["included_retained_after_compaction"] =
        serde_json::json!(summary_u64(summary, "included_retained_after_compaction"));
    out["dropped_retained_after_compaction"] =
        serde_json::json!(summary_u64(summary, "dropped_retained_after_compaction"));
    out["runtime_current_view_bytes_estimate"] =
        serde_json::json!(summary_u64(summary, "queue_tx_count_last").saturating_mul(256));
    annotate_receiver_repair_range_coverage_v1(&mut out);
    annotate_aoem_owned_single_path_live_diagnostics_v1(&mut out, summary);
    annotate_mini_receiver_tail_repair_diagnostics_v1(&mut out, summary);
    out
}

fn push_json_string_unique(items: &mut Vec<String>, value: &str) {
    if !items.iter().any(|item| item == value) {
        items.push(value.to_string());
    }
}

fn sample_string_vec(sample: &Value, key: &str) -> Vec<String> {
    sample
        .get(key)
        .and_then(Value::as_array)
        .map(|items| {
            items
                .iter()
                .filter_map(Value::as_str)
                .map(ToOwned::to_owned)
                .collect::<Vec<_>>()
        })
        .unwrap_or_default()
}

fn expected_tx_count_from_summary_v1(summary: &Value) -> u64 {
    summary_u64(summary, "child_expected_total_from_config")
        .max(summary_u64(summary, "child_expected_total_from_env"))
        .max(summary_u64(summary, "ledger_expected_count"))
}

fn completed_tx_count_from_summary_v1(summary: &Value) -> u64 {
    summary_u64(summary, "ledger_completed_count")
        .max(summary_u64(summary, "aoem_executed_total"))
        .max(summary_u64(summary, "included_canonical_total"))
}

fn annotate_aoem_owned_single_path_live_diagnostics_v1(sample: &mut Value, summary: &Value) {
    let copied_fields = [
        "aoem_owned_single_path_enforced",
        "legacy_host_transitional_fallback_gate_enabled",
        "legacy_host_transitional_fallback_used",
        "tx_ingress_real_callsite",
        "receiver_pipeline_mode",
        "network_receiver_object_ready_count",
        "network_receiver_calls_production_tx_ingress",
        "object_assembler_batch_ready_count",
        "object_assembler_commitment_ok_count",
        "aoem_runtime_worker_batch_received_count",
        "aoem_runtime_worker_tx_ingress_call_count",
        "aoem_runtime_worker_tx_ingress_callsite",
        "aoem_runtime_worker_result_ready_count",
        "finality_report_worker_result_verified_count",
        "finality_report_worker_final_report_written",
        "tx_ingress_called_by_network_receiver",
        "tx_ingress_called_by_aoem_runtime_worker",
        "receiver_pipeline_stage_lag",
        "receiver_pipeline_backpressure_reason",
        "tx_ingress_called_with_explicit_aoem_gate_config",
        "tx_ingress_aoem_gate_config_source",
        "tx_ingress_aoem_gate_config_production_candidate",
        "tx_ingress_selected_path",
        "tx_ingress_production_target",
        "aoem_owned_regression_signable",
        "aoem_owned_signoff_blocker_reasons",
    ];
    for field in copied_fields {
        sample[field] = summary.get(field).cloned().unwrap_or(Value::Null);
    }

    if !bool_env(NOV_NATIVE_AOEM_NATIVE_TX_BATCH_PRODUCTION_CANDIDATE_ENV) {
        return;
    }

    let missing_or_defaulted = summary
        .get("aoem_owned_single_path_enforced")
        .and_then(Value::as_bool)
        != Some(true)
        || summary
            .get("tx_ingress_called_with_explicit_aoem_gate_config")
            .and_then(Value::as_bool)
            != Some(true)
        || summary
            .get("tx_ingress_aoem_gate_config_source")
            .and_then(Value::as_str)
            != Some("receiver_child_runtime")
        || summary
            .get("tx_ingress_aoem_gate_config_production_candidate")
            .and_then(Value::as_bool)
            != Some(true)
        || summary
            .get("tx_ingress_selected_path")
            .and_then(Value::as_str)
            != Some(AOEM_RUNTIME_OWNED_PRODUCTION_TARGET_V1)
        || summary
            .get("tx_ingress_production_target")
            .and_then(Value::as_str)
            != Some(AOEM_RUNTIME_OWNED_PRODUCTION_TARGET_V1)
        || summary
            .get("aoem_owned_regression_signable")
            .and_then(Value::as_bool)
            != Some(true);

    if missing_or_defaulted {
        let mut blockers = sample_string_vec(sample, "aoem_owned_signoff_blocker_reasons");
        push_json_string_unique(
            &mut blockers,
            "aoem_owned_single_path_diagnostics_missing_under_gate",
        );
        sample["aoem_owned_signoff_blocker_reasons"] = serde_json::json!(blockers);
        sample["aoem_owned_single_path_enforced"] = serde_json::json!(false);
        sample["tx_ingress_called_with_explicit_aoem_gate_config"] = serde_json::json!(false);
        sample["aoem_owned_regression_signable"] = serde_json::json!(false);
        sample["accepted"] = serde_json::json!(false);
        sample["fail_reason"] =
            serde_json::json!("aoem_owned_single_path_diagnostics_missing_under_gate");
        sample["aoem_owned_gate_fail_reason"] =
            serde_json::json!("aoem_owned_single_path_diagnostics_missing_under_gate");
    }
}

fn annotate_mini_receiver_tail_repair_diagnostics_v1(sample: &mut Value, summary: &Value) {
    let expected = expected_tx_count_from_summary_v1(summary);
    let completed = completed_tx_count_from_summary_v1(summary);
    let durable_missing_count = summary_u64(summary, "ledger_durable_missing_count");
    let tail_missing_count = if durable_missing_count > 0 {
        durable_missing_count
    } else {
        expected.saturating_sub(completed)
    };
    let mut tail_missing_ranges = missing_ranges_from_json(
        sample
            .get("ledger_durable_missing_ranges_sample")
            .unwrap_or(&Value::Null),
    );
    if tail_missing_ranges.is_empty() && tail_missing_count > 0 && expected > completed {
        tail_missing_ranges = missing_ranges_from_progress(completed, expected, 1);
    }
    let queue_pending = summary_u64(summary, "queue_pending_last");
    let repair_received_overlap =
        summary_u64(summary, "receiver_repair_received_overlap_missing_count").max(sample_u64(
            sample,
            "receiver_repair_received_overlap_missing_count",
        ));
    let repair_accepted_overlap =
        summary_u64(summary, "receiver_repair_accepted_overlap_missing_count").max(sample_u64(
            sample,
            "receiver_repair_accepted_overlap_missing_count",
        ));
    let repair_executed_overlap =
        summary_u64(summary, "receiver_repair_executed_overlap_missing_count").max(sample_u64(
            sample,
            "receiver_repair_executed_overlap_missing_count",
        ));

    sample["mini_expected_tx_count"] = serde_json::json!(expected);
    sample["mini_completed_tx_count"] = serde_json::json!(completed);
    sample["mini_tail_missing_count"] = serde_json::json!(tail_missing_count);
    sample["mini_tail_missing_ranges_sample"] =
        missing_ranges_to_json(tail_missing_ranges.as_slice(), 8);
    sample["receiver_durable_missing_ranges_sample"] =
        missing_ranges_to_json(tail_missing_ranges.as_slice(), u64::MAX);
    sample["mini_waiting_for_sender_repair"] =
        serde_json::json!(tail_missing_count > 0 && queue_pending == 0);
    sample["receiver_waiting_for_sender_repair"] =
        serde_json::json!(tail_missing_count > 0 && queue_pending == 0);
    sample["mini_latest_ack_missing_count"] =
        serde_json::json!(summary_u64(summary, "latest_ack_missing_count").max(tail_missing_count));
    sample["receiver_ack_missing_count"] =
        serde_json::json!(summary_u64(summary, "latest_ack_missing_count").max(tail_missing_count));
    sample["mini_latest_ack_missing_ranges_sample"] =
        if let Some(value) = summary.get("latest_ack_missing_ranges_sample") {
            value.clone()
        } else {
            missing_ranges_to_json(tail_missing_ranges.as_slice(), 8)
        };
    sample["receiver_latest_ack_missing_ranges_sample"] =
        sample["mini_latest_ack_missing_ranges_sample"].clone();
    sample["receiver_ack_epoch"] = summary
        .get("final_ack_last_epoch")
        .cloned()
        .or_else(|| summary.get("ack_epoch").cloned())
        .unwrap_or(Value::Null);
    sample["receiver_ack_sent_count"] =
        serde_json::json!(summary_u64(summary, "final_ack_sent_count")
            .max(summary_u64(summary, "ack_sent_count")));
    sample["receiver_repair_packet_recv_count"] =
        serde_json::json!(summary_u64(summary, "repair_packet_received_count"));
    sample["mini_repair_received_overlap_missing_count"] =
        serde_json::json!(repair_received_overlap);
    sample["mini_repair_accepted_overlap_missing_count"] =
        serde_json::json!(repair_accepted_overlap);
    sample["mini_repair_executed_overlap_missing_count"] =
        serde_json::json!(repair_executed_overlap);
    sample["receiver_repair_ledger_closed_overlap_missing_count"] =
        serde_json::json!(repair_executed_overlap);

    let stall_reason = if tail_missing_count == 0 {
        "closed"
    } else if queue_pending == 0 {
        "pending_empty_waiting_for_sender_repair"
    } else {
        "pending_not_drained"
    };
    sample["mini_tail_repair_stall_reason"] = serde_json::json!(stall_reason);

    let elapsed_ms = sample_u64(sample, "elapsed_ms");
    if bool_env(NOV_NATIVE_AOEM_NATIVE_TX_BATCH_PRODUCTION_CANDIDATE_ENV)
        && expected > 0
        && expected <= 480
        && tail_missing_count > 0
        && queue_pending == 0
        && elapsed_ms >= 90_000
    {
        let mut blockers = sample_string_vec(sample, "aoem_owned_signoff_blocker_reasons");
        push_json_string_unique(&mut blockers, "mini_tail_repair_missing_not_closed");
        sample["aoem_owned_signoff_blocker_reasons"] = serde_json::json!(blockers);
        sample["accepted"] = serde_json::json!(false);
        if sample.get("fail_reason").and_then(Value::as_str).is_none() {
            sample["fail_reason"] = serde_json::json!("mini_tail_repair_missing_not_closed");
        }
    }
}

fn write_diagnostics_report(
    config: &ReceiverDiagnosticsConfigV1,
    state: &ReceiverDiagnosticsStateV1,
    accepted: bool,
    child_pid: u32,
    tx_count: u64,
) -> Result<()> {
    let last_sample_any = state.samples.last();
    let first_live_sample = first_live_child_sample(state.samples.as_slice());
    let last_live_sample = last_live_child_sample(state.samples.as_slice());
    let final_closed_sample_raw = final_closed_child_sample(state.samples.as_slice());
    let first_progress_elapsed_ms = first_mini_tps_progress_elapsed_ms_v1(state.samples.as_slice());
    let mut final_closed_sample_owned = final_closed_sample_raw.cloned();
    if let Some(sample) = final_closed_sample_owned.as_mut() {
        annotate_mini_final_run_tps_sync_v1(sample, first_progress_elapsed_ms);
    }
    let final_closed_sample = final_closed_sample_owned.as_ref();
    let signoff_sample_owned = final_closed_sample_owned
        .clone()
        .or_else(|| last_sample_any.cloned());
    let signoff_sample = signoff_sample_owned.as_ref();
    let diagnostics_signoff_sample_source = if final_closed_sample_raw.is_some() {
        "final_closed_child_sample"
    } else if last_sample_any.is_some() {
        "last_sample_any"
    } else {
        "none"
    };
    let last_live_child_sample_stale = final_closed_sample.is_some()
        && last_live_sample
            .map(|sample| {
                sample
                    .get("final_closed_child_sample")
                    .and_then(Value::as_bool)
                    != Some(true)
            })
            .unwrap_or(false);
    let peak_live_sample = peak_live_child_sample(state.samples.as_slice());
    let memory_summary_sample = peak_live_sample.or(last_live_sample);
    let post_exit_samples = post_exit_sample_count(state.samples.as_slice());
    let post_exit_working_set_zeroed = last_sample_any
        .map(|sample| {
            !is_live_child_memory_sample(sample)
                && sample.get("process_working_set_bytes").is_some()
        })
        .unwrap_or(false);
    let memory_summary_source = if peak_live_sample.is_some() {
        "live_peak"
    } else if last_live_sample.is_some() {
        "live_last"
    } else if post_exit_samples > 0 {
        "post_exit_invalid"
    } else {
        "none"
    };
    let signoff_fail_reason = signoff_sample
        .and_then(|sample| sample.get("fail_reason"))
        .and_then(Value::as_str)
        .filter(|reason| !reason.trim().is_empty())
        .map(ToOwned::to_owned)
        .or_else(|| {
            if final_closed_sample.is_some() {
                None
            } else {
                state.fail_reason.clone()
            }
        });
    let stale_live_fail_reason = if last_live_child_sample_stale {
        last_live_sample
            .and_then(|sample| sample.get("fail_reason"))
            .and_then(Value::as_str)
            .filter(|reason| !reason.trim().is_empty())
            .map(ToOwned::to_owned)
    } else {
        None
    };
    let stale_live_sample_fail_reason_ignored = final_closed_sample.is_some()
        && signoff_fail_reason.is_none()
        && stale_live_fail_reason.is_some();
    let latest_blockers = signoff_sample
        .map(|sample| sample_string_vec(sample, "aoem_owned_signoff_blocker_reasons"))
        .unwrap_or_default();
    let effective_accepted =
        accepted && signoff_fail_reason.is_none() && latest_blockers.is_empty();
    let performance_breakdown = receiver_wall_clock_performance_breakdown_v1(
        state.samples.as_slice(),
        signoff_sample,
        tx_count,
    );
    let mut report = serde_json::json!({
        "schema": "novovm-native-pipeline-cross-machine-sustained-diagnostics/v1",
        "accepted": effective_accepted,
        "child_pid": child_pid,
        "expected_tx_count": tx_count,
        "sample_interval_ms": config.sample_interval_ms,
        "stall_windows": config.stall_windows,
        "pending_drain_no_progress_timeout_ms": config.pending_drain_no_progress_timeout_ms,
        "pending_drain_no_progress_ms": state.pending_drain_no_progress_ms,
        "memory_sample_enabled": config.memory_sample_enabled,
        "max_working_set_bytes": config.max_working_set_bytes,
        "min_canonical_delta": config.min_canonical_delta,
        "max_elapsed_ms": config.max_elapsed_ms,
        "primary_send_duration_ms": config.primary_send_duration_ms,
        "repair_drain_timeout_ms": config.repair_drain_timeout_ms,
        "final_ack_timeout_ms": config.final_ack_timeout_ms,
        "absolute_max_ms": config.absolute_max_ms,
        "fail_reason": signoff_fail_reason,
        "accepted_input_before_signoff_blocker_check": accepted,
        "aoem_owned_signoff_blocker_reasons": latest_blockers,
        "final_closed_child_sample_available": final_closed_sample.is_some(),
        "final_closed_child_sample": final_closed_sample.cloned(),
        "last_live_child_sample_stale": last_live_child_sample_stale,
        "diagnostics_signoff_sample_source": diagnostics_signoff_sample_source,
        "diagnostics_signoff_sample": signoff_sample.cloned(),
        "diagnostics_final_sample_mini_completed_tx_count": final_closed_sample
            .map(|sample| sample_u64(sample, "mini_completed_tx_count")),
        "diagnostics_final_sample_mini_tail_missing_count": final_closed_sample
            .map(|sample| sample_u64(sample, "mini_tail_missing_count")),
        "diagnostics_final_sample_aoem_owned_regression_signable": final_closed_sample
            .and_then(|sample| sample.get("aoem_owned_regression_signable"))
            .and_then(Value::as_bool),
        "diagnostics_final_sample_fail_reason": final_closed_sample
            .and_then(|sample| sample.get("fail_reason"))
            .and_then(Value::as_str),
        "diagnostics_final_sample_mini_tps_sync_pass": final_closed_sample
            .and_then(|sample| sample.get("mini_tps_sync_pass"))
            .and_then(Value::as_bool),
        "diagnostics_final_sample_mini_tps_sync_fail_reason": final_closed_sample
            .and_then(|sample| sample.get("mini_tps_sync_fail_reason"))
            .cloned(),
        "mini_tps_sync_pass": signoff_sample
            .and_then(|sample| sample.get("mini_tps_sync_pass"))
            .and_then(Value::as_bool),
        "mini_tps_sync_fail_reason": signoff_sample
            .and_then(|sample| sample.get("mini_tps_sync_fail_reason"))
            .cloned(),
        "mini_tps_sync_sample_source": signoff_sample
            .and_then(|sample| sample.get("mini_tps_sync_sample_source"))
            .cloned(),
        "mini_tps_sync_sample_source_valid": signoff_sample
            .and_then(|sample| sample.get("mini_tps_sync_sample_source_valid"))
            .and_then(Value::as_bool),
        "mini_tps_sync_final_counter_source": signoff_sample
            .and_then(|sample| sample.get("mini_tps_sync_final_counter_source"))
            .cloned(),
        "mini_tps_sync_live_counter_source": signoff_sample
            .and_then(|sample| sample.get("mini_tps_sync_live_counter_source"))
            .cloned(),
        "final_closed_child_sample_counter_source": signoff_sample
            .and_then(|sample| sample.get("final_closed_child_sample_counter_source"))
            .cloned(),
        "final_closed_child_sample_uses_retained_view": signoff_sample
            .and_then(|sample| sample.get("final_closed_child_sample_uses_retained_view"))
            .and_then(Value::as_bool),
        "final_run_close_tps_x1000": signoff_sample
            .map(|sample| sample_u64(sample, "final_run_close_tps_x1000")),
        "final_run_close_tps_window_ms": signoff_sample
            .map(|sample| sample_u64(sample, "final_run_close_tps_window_ms")),
        "final_run_close_tps_counter": signoff_sample
            .map(|sample| sample_u64(sample, "final_run_close_tps_counter")),
        "final_completed_tx_count": signoff_sample
            .map(|sample| sample_u64(sample, "final_completed_tx_count")),
        "final_canonical_tx_count": signoff_sample
            .map(|sample| sample_u64(sample, "final_canonical_tx_count")),
        "final_ledger_closed_tx_count": signoff_sample
            .map(|sample| sample_u64(sample, "final_ledger_closed_tx_count")),
        "final_proof_count": signoff_sample
            .map(|sample| sample_u64(sample, "final_proof_count")),
        "final_run_aoem_close_tps_x1000": signoff_sample
            .map(|sample| sample_u64(sample, "final_run_aoem_close_tps_x1000")),
        "final_run_canonical_tps_x1000": signoff_sample
            .map(|sample| sample_u64(sample, "final_run_canonical_tps_x1000")),
        "final_run_ledger_tps_x1000": signoff_sample
            .map(|sample| sample_u64(sample, "final_run_ledger_tps_x1000")),
        "final_run_tps_sync_pass": signoff_sample
            .and_then(|sample| sample.get("final_run_tps_sync_pass"))
            .and_then(Value::as_bool),
        "final_run_tps_sync_fail_reasons": signoff_sample
            .and_then(|sample| sample.get("final_run_tps_sync_fail_reasons"))
            .cloned(),
        "receiver_child_tick_count": signoff_sample
            .map(|sample| sample_u64(sample, "receiver_child_tick_count")),
        "receiver_child_tick_delta": signoff_sample
            .map(|sample| sample_u64(sample, "receiver_child_tick_delta")),
        "receiver_child_tick_stall_ms": signoff_sample
            .map(|sample| sample_u64(sample, "receiver_child_tick_stall_ms")),
        "receiver_child_tick_stall_reason": signoff_sample
            .and_then(|sample| sample.get("receiver_child_tick_stall_reason"))
            .cloned(),
        "receiver_drain_stall_reason": signoff_sample
            .and_then(|sample| sample.get("receiver_drain_stall_reason"))
            .cloned(),
        "pipeline_pending_drain_stall": signoff_sample
            .and_then(|sample| sample.get("pipeline_pending_drain_stall"))
            .and_then(Value::as_bool),
        "pipeline_pending_drain_stall_reason": signoff_sample
            .and_then(|sample| sample.get("pipeline_pending_drain_stall_reason"))
            .cloned(),
        "pipeline_stage_liveness_stalled": signoff_sample
            .and_then(|sample| sample.get("pipeline_stage_liveness_stalled"))
            .and_then(Value::as_bool),
        "network_receiver_object_ready_delta": signoff_sample
            .map(|sample| sample_u64(sample, "network_receiver_object_ready_delta")),
        "object_assembler_batch_ready_delta": signoff_sample
            .map(|sample| sample_u64(sample, "object_assembler_batch_ready_delta")),
        "aoem_runtime_worker_batch_received_delta": signoff_sample
            .map(|sample| sample_u64(sample, "aoem_runtime_worker_batch_received_delta")),
        "aoem_runtime_worker_tx_ingress_call_delta": signoff_sample
            .map(|sample| sample_u64(sample, "aoem_runtime_worker_tx_ingress_call_delta")),
        "aoem_runtime_worker_result_ready_delta": signoff_sample
            .map(|sample| sample_u64(sample, "aoem_runtime_worker_result_ready_delta")),
        "finality_report_worker_result_verified_delta": signoff_sample
            .map(|sample| sample_u64(sample, "finality_report_worker_result_verified_delta")),
        "queue_pending_last": signoff_sample
            .map(|sample| sample_u64(sample, "queue_pending_last")),
        "queue_pending_delta": signoff_sample
            .and_then(|sample| sample.get("queue_pending_delta"))
            .cloned(),
        "pending_drain_attempt_count": signoff_sample
            .map(|sample| sample_u64(sample, "pending_drain_attempt_count")),
        "pending_drain_success_count": signoff_sample
            .map(|sample| sample_u64(sample, "pending_drain_success_count")),
        "pending_drain_zero_count": signoff_sample
            .map(|sample| sample_u64(sample, "pending_drain_zero_count")),
        "pending_drain_attempt_delta": signoff_sample
            .map(|sample| sample_u64(sample, "pending_drain_attempt_delta")),
        "pending_drain_success_delta": signoff_sample
            .map(|sample| sample_u64(sample, "pending_drain_success_delta")),
        "pending_drain_callsite_last_attempt_ms": signoff_sample
            .and_then(|sample| sample.get("pending_drain_callsite_last_attempt_ms"))
            .cloned(),
        "pending_drain_callsite_last_success_ms": signoff_sample
            .and_then(|sample| sample.get("pending_drain_callsite_last_success_ms"))
            .cloned(),
        "pending_drain_callsite_active": signoff_sample
            .and_then(|sample| sample.get("pending_drain_callsite_active"))
            .and_then(Value::as_bool),
        "pending_drain_callsite_idle_while_pending": signoff_sample
            .and_then(|sample| sample.get("pending_drain_callsite_idle_while_pending"))
            .and_then(Value::as_bool),
        "pending_drain_scheduler_state": signoff_sample
            .and_then(|sample| sample.get("pending_drain_scheduler_state"))
            .cloned(),
        "pending_drain_wakeup_source": signoff_sample
            .and_then(|sample| sample.get("pending_drain_wakeup_source"))
            .cloned(),
        "pending_drain_blocker_reason": signoff_sample
            .and_then(|sample| sample.get("pending_drain_blocker_reason"))
            .cloned(),
        "pending_drain_no_progress_ms": signoff_sample
            .map(|sample| sample_u64(sample, "pending_drain_no_progress_ms")),
        "pending_nonzero_active_drain_enforced": signoff_sample
            .and_then(|sample| sample.get("pending_nonzero_active_drain_enforced"))
            .and_then(Value::as_bool),
        "object_assembler_stall_reason": signoff_sample
            .and_then(|sample| sample.get("object_assembler_stall_reason"))
            .cloned(),
        "aoem_runtime_worker_stall_reason": signoff_sample
            .and_then(|sample| sample.get("aoem_runtime_worker_stall_reason"))
            .cloned(),
        "aoem_runtime_worker_backpressure_reason": signoff_sample
            .and_then(|sample| sample.get("aoem_runtime_worker_backpressure_reason"))
            .cloned(),
        "finality_report_worker_backpressure_reason": signoff_sample
            .and_then(|sample| sample.get("finality_report_worker_backpressure_reason"))
            .cloned(),
        "canonical_progress_last_ms": signoff_sample
            .and_then(|sample| sample.get("canonical_progress_last_ms"))
            .cloned(),
        "ledger_progress_last_ms": signoff_sample
            .and_then(|sample| sample.get("ledger_progress_last_ms"))
            .cloned(),
        "mini_a_send_tps_x1000": signoff_sample
            .map(|sample| sample_u64(sample, "mini_a_send_tps_x1000")),
        "mini_b_udp_packet_tps_x1000": signoff_sample
            .map(|sample| sample_u64(sample, "mini_b_udp_packet_tps_x1000")),
        "mini_b_transport_object_ready_tps_x1000": signoff_sample
            .map(|sample| sample_u64(sample, "mini_b_transport_object_ready_tps_x1000")),
        "mini_b_sequence_unique_tps_x1000": signoff_sample
            .map(|sample| sample_u64(sample, "mini_b_sequence_unique_tps_x1000")),
        "mini_b_tx_object_admitted_tps_x1000": signoff_sample
            .map(|sample| sample_u64(sample, "mini_b_tx_object_admitted_tps_x1000")),
        "mini_b_queue_admitted_tx_tps_x1000": signoff_sample
            .map(|sample| sample_u64(sample, "mini_b_queue_admitted_tx_tps_x1000")),
        "mini_b_aoem_closed_tx_tps_x1000": signoff_sample
            .map(|sample| sample_u64(sample, "mini_b_aoem_closed_tx_tps_x1000")),
        "mini_b_canonical_tx_tps_x1000": signoff_sample
            .map(|sample| sample_u64(sample, "mini_b_canonical_tx_tps_x1000")),
        "mini_b_ledger_tx_tps_x1000": signoff_sample
            .map(|sample| sample_u64(sample, "mini_b_ledger_tx_tps_x1000")),
        "mini_tps_sync_metric_units": signoff_sample
            .and_then(|sample| sample.get("mini_tps_sync_metric_units"))
            .cloned(),
        "mini_tps_sync_comparable_network_source": signoff_sample
            .and_then(|sample| sample.get("mini_tps_sync_comparable_network_source"))
            .cloned(),
        "mini_tps_sync_packet_to_tx_ratio": signoff_sample
            .map(|sample| sample_u64(sample, "mini_tps_sync_packet_to_tx_ratio")),
        "mini_b_network_received_tps_x1000": signoff_sample
            .map(|sample| sample_u64(sample, "mini_b_network_received_tps_x1000")),
        "mini_b_queue_admitted_tps_x1000": signoff_sample
            .map(|sample| sample_u64(sample, "mini_b_queue_admitted_tps_x1000")),
        "mini_b_aoem_closed_tps_x1000": signoff_sample
            .map(|sample| sample_u64(sample, "mini_b_aoem_closed_tps_x1000")),
        "mini_b_canonical_tps_x1000": signoff_sample
            .map(|sample| sample_u64(sample, "mini_b_canonical_tps_x1000")),
        "mini_b_ledger_tps_x1000": signoff_sample
            .map(|sample| sample_u64(sample, "mini_b_ledger_tps_x1000")),
        "stale_live_sample_fail_reason_ignored": stale_live_sample_fail_reason_ignored,
        "stale_live_sample_fail_reason": stale_live_fail_reason,
        "diagnostics_samples_retained": state.samples.len(),
        "diagnostics_samples_dropped": state.samples_dropped,
        "sample_count": state.samples.len(),
        "first_working_set_bytes": state.first_working_set_bytes,
        "last_working_set_bytes": state.last_working_set_bytes,
        "working_set_delta_total_bytes": state
            .last_working_set_bytes
            .zip(state.first_working_set_bytes)
            .map(|(last, first)| last.saturating_sub(first)),
        "last_sample_any": last_sample_any.cloned(),
        "first_live_child_sample": first_live_sample.cloned(),
        "last_live_child_sample": last_live_sample.cloned(),
        "peak_live_child_sample": peak_live_sample.cloned(),
        "post_exit_sample_present": post_exit_samples > 0,
        "post_exit_sample_count": post_exit_samples,
        "live_sample_count": state.samples.len().saturating_sub(post_exit_samples as usize),
        "post_exit_working_set_zeroed": post_exit_working_set_zeroed,
        "memory_summary_source": memory_summary_source,
        "last_sample_any_process_working_set_bytes": last_sample_any
            .map(|sample| sample_u64(sample, "process_working_set_bytes")),
        "last_sample_any_process_private_bytes": last_sample_any
            .map(|sample| sample_u64(sample, "process_private_bytes")),
        "last_process_working_set_bytes": last_live_sample
            .map(|sample| sample_u64(sample, "process_working_set_bytes")),
        "last_process_private_bytes": last_live_sample
            .map(|sample| sample_u64(sample, "process_private_bytes")),
        "last_process_virtual_bytes": last_live_sample
            .map(|sample| sample_u64(sample, "process_virtual_bytes")),
        "first_live_thread_count": first_live_sample
            .map(|sample| sample_u64(sample, "process_thread_count")),
        "last_live_thread_count": last_live_sample
            .map(|sample| sample_u64(sample, "process_thread_count")),
        "peak_live_thread_count": state
            .samples
            .iter()
            .filter(|sample| is_live_child_memory_sample(sample))
            .map(|sample| sample_u64(sample, "process_thread_count"))
            .max(),
        "thread_count_delta": first_live_sample
            .zip(last_live_sample)
            .map(|(first, last)| sample_u64(last, "process_thread_count").saturating_sub(sample_u64(first, "process_thread_count"))),
        "thread_count_per_1000_tx": memory_summary_sample
            .map(|sample| sample_u64(sample, "thread_count_per_1000_tx")),
        "handle_count_delta": first_live_sample
            .zip(last_live_sample)
            .map(|(first, last)| sample_u64(last, "process_handle_count").saturating_sub(sample_u64(first, "process_handle_count"))),
        "handle_count_per_1000_tx": memory_summary_sample
            .map(|sample| sample_u64(sample, "handle_count_per_1000_tx")),
        "thread_growth_suspected": memory_summary_sample
            .and_then(|sample| sample_bool(sample, "thread_growth_suspected")),
        "thread_growth_stage_suspected": memory_summary_sample
            .and_then(|sample| sample_string(sample, "thread_growth_stage_suspected")),
        "last_rust_estimated_retained_bytes": last_live_sample
            .map(|sample| sample_u64(sample, "rust_estimated_retained_bytes")),
        "last_native_heap_unattributed_bytes": last_live_sample
            .map(|sample| sample_u64(sample, "native_heap_unattributed_bytes")),
        "last_unattributed_working_set_bytes": last_live_sample
            .map(|sample| sample_u64(sample, "unattributed_working_set_bytes")),
        "last_rocksdb_total_estimated_memory_bytes": last_live_sample
            .map(|sample| sample_u64(sample, "rocksdb_total_estimated_memory_bytes")),
        "last_working_set_bytes_per_1000_tx": last_live_sample
            .map(|sample| sample_u64(sample, "working_set_bytes_per_1000_tx")),
        "last_private_bytes_per_1000_tx": last_live_sample
            .map(|sample| sample_u64(sample, "private_bytes_per_1000_tx")),
        "last_native_heap_unattributed_bytes_per_1000_tx": last_live_sample
            .map(|sample| sample_u64(sample, "native_heap_unattributed_bytes_per_1000_tx")),
        "last_repair_packet_received_count": last_sample_any
            .map(|sample| sample_u64(sample, "repair_packet_received_count")),
        "last_repair_sequence_received_count": last_sample_any
            .map(|sample| sample_u64(sample, "repair_sequence_received_count")),
        "last_repair_sequence_received_min": last_sample_any
            .and_then(|sample| sample.get("repair_sequence_received_min"))
            .cloned(),
        "last_repair_sequence_received_max": last_sample_any
            .and_then(|sample| sample.get("repair_sequence_received_max"))
            .cloned(),
        "last_repair_sequence_accepted_count": last_sample_any
            .map(|sample| sample_u64(sample, "repair_sequence_accepted_count")),
        "last_repair_sequence_enqueued_count": last_sample_any
            .map(|sample| sample_u64(sample, "repair_sequence_enqueued_count")),
        "last_repair_sequence_duplicate_count": last_sample_any
            .map(|sample| sample_u64(sample, "repair_sequence_duplicate_count")),
        "last_repair_sequence_rejected_count": last_sample_any
            .map(|sample| sample_u64(sample, "repair_sequence_rejected_count")),
        "last_repair_reject_reason_counts": last_sample_any
            .and_then(|sample| sample.get("repair_reject_reason_counts"))
            .cloned(),
        "peak_live_working_set_bytes": peak_live_sample
            .map(|sample| sample_u64(sample, "process_working_set_bytes")),
        "peak_live_private_bytes": peak_live_sample
            .map(|sample| sample_u64(sample, "process_private_bytes")),
        "peak_live_native_heap_unattributed_bytes": peak_live_sample
            .map(|sample| sample_u64(sample, "native_heap_unattributed_bytes")),
        "peak_live_process_virtual_bytes": peak_live_sample
            .map(|sample| sample_u64(sample, "process_virtual_bytes")),
        "peak_live_allocator_fragmentation_suspected": peak_live_sample
            .and_then(|sample| sample_bool(sample, "allocator_fragmentation_suspected")),
        "peak_live_working_set_not_returned_suspected": peak_live_sample
            .and_then(|sample| sample_bool(sample, "working_set_not_returned_suspected")),
        "allocator_fragmentation_suspected": memory_summary_sample
            .and_then(|sample| sample_bool(sample, "allocator_fragmentation_suspected")),
        "working_set_not_returned_suspected": memory_summary_sample
            .and_then(|sample| sample_bool(sample, "working_set_not_returned_suspected")),
        "summary_aoem_runtime_estimated_bytes": memory_summary_sample
            .map(|sample| sample_u64(sample, "aoem_runtime_estimated_bytes")),
        "summary_aoem_batch_input_bytes": memory_summary_sample
            .map(|sample| sample_u64(sample, "aoem_batch_input_bytes")),
        "summary_aoem_batch_output_bytes": memory_summary_sample
            .map(|sample| sample_u64(sample, "aoem_batch_output_bytes")),
        "summary_proof_projection_bytes": memory_summary_sample
            .map(|sample| sample_u64(sample, "proof_projection_bytes")),
        "summary_receipt_projection_bytes": memory_summary_sample
            .map(|sample| sample_u64(sample, "receipt_projection_bytes")),
        "summary_canonical_projection_bytes": memory_summary_sample
            .map(|sample| sample_u64(sample, "canonical_projection_bytes")),
        "summary_udp_receive_buffer_bytes": memory_summary_sample
            .map(|sample| sample_u64(sample, "udp_receive_buffer_bytes")),
        "summary_decode_buffer_bytes": memory_summary_sample
            .map(|sample| sample_u64(sample, "decode_buffer_bytes")),
        "summary_json_serialization_buffer_bytes": memory_summary_sample
            .map(|sample| sample_u64(sample, "json_serialization_buffer_bytes")),
        "summary_tick_vec_capacity_bytes": memory_summary_sample
            .map(|sample| sample_u64(sample, "tick_vec_capacity_bytes")),
        "summary_batch_vec_capacity_bytes": memory_summary_sample
            .map(|sample| sample_u64(sample, "batch_vec_capacity_bytes")),
        "summary_stage_estimated_bytes_total": memory_summary_sample
            .map(|sample| sample_u64(sample, "stage_estimated_bytes_total")),
        "summary_native_heap_unattributed_bytes_per_tick": memory_summary_sample
            .map(|sample| sample_u64(sample, "native_heap_unattributed_bytes_per_tick")),
        "summary_unknown_native_heap_source": memory_summary_sample
            .and_then(|sample| sample_bool(sample, "unknown_native_heap_source")),
        "summary_large_allocation_suspected_stage": memory_summary_sample
            .and_then(|sample| sample_string(sample, "large_allocation_suspected_stage")),
        "summary_native_heap_source_isolation_confidence": memory_summary_sample
            .and_then(|sample| sample_string(sample, "native_heap_source_isolation_confidence")),
        "performance_wall_clock_breakdown": performance_breakdown,
        "samples": state.samples,
    });
    if let Some(performance_fields) = report
        .get("performance_wall_clock_breakdown")
        .and_then(Value::as_object)
        .cloned()
    {
        if let Some(report_map) = report.as_object_mut() {
            for (key, value) in performance_fields {
                report_map.insert(key, value);
            }
        }
    }
    write_report(config.report_path.as_path(), &report)
}

fn send_scheduled_batch(
    chain_id: u64,
    sender_node: u64,
    receiver_node: u64,
    sender_addr: &str,
    receiver_addr: &str,
    txs: &[NativeFixtureTxV1],
    delay_ms: u64,
    retry: UdpSendRetryConfigV1,
) -> Result<SendScheduleStatsV1> {
    let sender = UdpTransport::bind_for_chain(NodeId(sender_node), sender_addr, chain_id)
        .with_context(|| format!("bind cross-machine sender UDP failed: {sender_addr}"))?;
    sender
        .register_peer(NodeId(receiver_node), receiver_addr)
        .with_context(|| format!("register cross-machine receiver peer failed: {receiver_addr}"))?;
    let mut sent_by_hash = BTreeMap::<String, u64>::new();
    let mut sent_unique = BTreeSet::<String>::new();
    let mut sent_packets = 0u64;
    let mut dropped_packets = 0u64;
    let mut send_retry_count = 0u64;
    let mut send_would_block_count = 0u64;
    let duplicated_packets = txs
        .iter()
        .filter(|tx| tx.copy_index > 0)
        .count()
        .try_into()
        .unwrap_or(u64::MAX);
    let reordered_packets = txs
        .windows(2)
        .filter(|pair| pair[0].index > pair[1].index)
        .count()
        .try_into()
        .unwrap_or(u64::MAX);
    for tx in txs {
        if tx.dropped {
            dropped_packets = dropped_packets.saturating_add(1);
            continue;
        }
        let msg = ProtocolMessage::EvmNative(EvmNativeMessage::Transactions {
            from: NodeId(sender_node),
            chain_id,
            tx_hash: tx.tx_hash,
            tx_count: tx.copy_index.saturating_add(1).max(1),
            payload: tx.payload.clone(),
        });
        match safe_send_with_retry(&sender, NodeId(receiver_node), msg, retry) {
            Ok(retry_stats) => {
                send_retry_count = send_retry_count.saturating_add(retry_stats.retry_count);
                send_would_block_count =
                    send_would_block_count.saturating_add(retry_stats.would_block_count);
                sent_packets = sent_packets.saturating_add(1);
                let hash = hex_lower(&tx.tx_hash);
                sent_unique.insert(hash.clone());
                *sent_by_hash.entry(hash).or_default() += 1;
                if delay_ms > 0 {
                    std::thread::sleep(std::time::Duration::from_millis(delay_ms));
                }
                continue;
            }
            Err(error) => {
                if is_retryable_udp_send_error(error.as_str()) {
                    send_would_block_count =
                        send_would_block_count.saturating_add(retry.max_retries.saturating_add(1));
                    send_retry_count = send_retry_count.saturating_add(retry.max_retries);
                }
                return Ok(SendScheduleStatsV1 {
                    scheduled_packets: txs.len().try_into().unwrap_or(u64::MAX),
                    sent_packets,
                    dropped_packets,
                    duplicated_packets,
                    delayed_packets: if delay_ms > 0 { sent_packets } else { 0 },
                    reordered_packets,
                    sent_unique: sent_unique.len().try_into().unwrap_or(u64::MAX),
                    send_retry_count,
                    send_would_block_count,
                    send_failed_count: 1,
                    send_failure_first_index: Some(tx.index),
                    send_failure_first_copy_index: Some(tx.copy_index),
                    send_failure_first_error: Some(error),
                    sent_by_hash,
                });
            }
        }
    }
    Ok(SendScheduleStatsV1 {
        scheduled_packets: txs.len().try_into().unwrap_or(u64::MAX),
        sent_packets,
        dropped_packets,
        duplicated_packets,
        delayed_packets: if delay_ms > 0 { sent_packets } else { 0 },
        reordered_packets,
        sent_unique: sent_unique.len().try_into().unwrap_or(u64::MAX),
        send_retry_count,
        send_would_block_count,
        send_failed_count: 0,
        send_failure_first_index: None,
        send_failure_first_copy_index: None,
        send_failure_first_error: None,
        sent_by_hash,
    })
}

fn send_repair_payloads_paced(
    chain_id: u64,
    sender_node: u64,
    receiver_node: u64,
    sender_addr: &str,
    receiver_addr: &str,
    txs: &[NativeFixtureTxV1],
    repair_round: u64,
    tail_repair: TailRepairConfigV1,
    packet_copies_override: Option<u64>,
    batch_size_override: Option<u64>,
    batch_pause_ms_override: Option<u64>,
    retry: UdpSendRetryConfigV1,
) -> Result<SendScheduleStatsV1> {
    let copies = packet_copies_override
        .unwrap_or(tail_repair.packet_copies)
        .max(1);
    let batch_size = batch_size_override.unwrap_or(tail_repair.batch_size).max(1) as usize;
    let batch_pause_ms = batch_pause_ms_override.unwrap_or(tail_repair.batch_pause_ms);
    let mut out = empty_send_stats();
    for copy in 0..copies {
        let copy_index = repair_round
            .saturating_add(1)
            .saturating_mul(1_000)
            .saturating_add(copy)
            .saturating_add(1);
        for chunk in txs.chunks(batch_size) {
            let mut chunk_txs = chunk.to_vec();
            for tx in &mut chunk_txs {
                tx.copy_index = copy_index;
                tx.dropped = false;
            }
            let stats = send_scheduled_batch(
                chain_id,
                sender_node,
                receiver_node,
                sender_addr,
                receiver_addr,
                chunk_txs.as_slice(),
                0,
                retry,
            )?;
            merge_send_stats(&mut out, stats);
            if out.send_failed_count > 0 {
                return Ok(out);
            }
            if batch_pause_ms > 0 {
                std::thread::sleep(Duration::from_millis(batch_pause_ms));
            }
        }
    }
    Ok(out)
}

fn validate_boundaries(summary: &Value, violations: &mut Vec<String>) {
    if summary_str(summary, "execution_kernel") != "AOEM" {
        violations.push(format!(
            "execution_kernel={} expected AOEM",
            summary_str(summary, "execution_kernel")
        ));
    }
    if summary_str(summary, "aoem_concurrency_owner") != "AOEM_runtime" {
        violations.push(format!(
            "aoem_concurrency_owner={} expected AOEM_runtime",
            summary_str(summary, "aoem_concurrency_owner")
        ));
    }
    if summary_str(summary, "host_concurrency_policy")
        != "host_drives_lifecycle_only_no_rust_execution_scheduler"
    {
        violations.push(format!(
            "host_concurrency_policy={} expected host lifecycle only",
            summary_str(summary, "host_concurrency_policy")
        ));
    }
}

fn summary_bool(summary: &Value, field: &str) -> bool {
    summary.get(field).and_then(Value::as_bool).unwrap_or(false)
}

fn summary_array_is_empty(summary: &Value, field: &str) -> bool {
    summary
        .get(field)
        .and_then(Value::as_array)
        .map(Vec::is_empty)
        .unwrap_or(false)
}

fn validate_aoem_production_candidate_summary(
    summary: &Value,
    tx_count: u64,
    violations: &mut Vec<String>,
) -> Value {
    let enabled = summary_bool(summary, "aoem_native_tx_batch_production_candidate_enabled");
    let result_ok = summary_bool(
        summary,
        "aoem_native_tx_batch_production_candidate_result_ok",
    );
    let owner = summary_str(summary, "aoem_native_tx_batch_production_owner").to_string();
    let target = summary_str(summary, "tx_ingress_production_target").to_string();
    let selected_path = summary_str(summary, "tx_ingress_selected_path").to_string();
    let final_summary_fields_present =
        summary_bool(summary, "receiver_final_summary_aoem_fields_present");
    let final_summary_fields_defaulted =
        summary_bool(summary, "receiver_final_summary_aoem_fields_defaulted");
    let aoem_owned_gate_fail_reason =
        summary_str(summary, "aoem_owned_gate_fail_reason").to_string();
    let child_runtime_gate_source =
        summary_str(summary, "child_runtime_aoem_gate_config_source").to_string();
    let tx_ingress_gate_source =
        summary_str(summary, "tx_ingress_aoem_gate_config_source").to_string();
    let tx_ingress_gate_production_candidate =
        summary_bool(summary, "tx_ingress_aoem_gate_config_production_candidate");
    let tx_ingress_gate_shadow = summary_bool(summary, "tx_ingress_aoem_gate_config_shadow");
    let tx_ingress_gate_compare = summary_bool(summary, "tx_ingress_aoem_gate_config_compare");
    let child_gate_propagated = summary_bool(
        summary,
        "aoem_owned_child_runtime_gate_propagated_to_tx_ingress",
    );
    let single_path_enforced = summary_bool(summary, "aoem_owned_single_path_enforced");
    let legacy_fallback_gate_enabled =
        summary_bool(summary, "legacy_host_transitional_fallback_gate_enabled");
    let legacy_fallback_used = summary_bool(summary, "legacy_host_transitional_fallback_used");
    let legacy_success_suppressed = summary_bool(
        summary,
        "legacy_host_transitional_success_suppressed_by_aoem_gate",
    );
    let regression_signable = summary_bool(summary, "aoem_owned_regression_signable");
    let explicit_gate_config =
        summary_bool(summary, "tx_ingress_called_with_explicit_aoem_gate_config");
    let receipt_count = summary_u64(summary, "aoem_native_tx_batch_production_receipt_count");
    let canonical_proof_count = summary_u64(
        summary,
        "aoem_native_tx_batch_production_canonical_proof_count",
    );
    let ledger_close_proof_count = summary_u64(
        summary,
        "aoem_native_tx_batch_production_ledger_close_proof_count",
    );
    let state_delta_root_present = summary_bool(
        summary,
        "aoem_native_tx_batch_production_state_delta_root_present",
    );
    let snapshot_metadata_present = summary_bool(
        summary,
        "aoem_native_tx_batch_production_snapshot_metadata_present",
    );
    let fallback_used = summary_bool(summary, "aoem_native_tx_batch_production_fallback_used");
    let mismatch_reasons_empty =
        summary_array_is_empty(summary, "aoem_native_tx_batch_production_mismatch_reasons");
    let double_write = summary_bool(
        summary,
        "aoem_native_tx_batch_production_double_write_legacy_canonical",
    );

    if bool_env(NOV_NATIVE_AOEM_NATIVE_TX_BATCH_PRODUCTION_CANDIDATE_ENV) {
        if !final_summary_fields_present {
            violations
                .push("receiver_final_summary_aoem_fields_present=false expected true".to_string());
        }
        if final_summary_fields_defaulted {
            violations.push(
                "receiver_final_summary_aoem_fields_defaulted=true expected false".to_string(),
            );
        }
        if !aoem_owned_gate_fail_reason.is_empty() {
            violations.push(format!(
                "aoem_owned_gate_fail_reason={aoem_owned_gate_fail_reason} expected empty"
            ));
        }
        if child_runtime_gate_source != "receiver_child_runtime" {
            violations.push(format!(
                "child_runtime_aoem_gate_config_source={child_runtime_gate_source} expected receiver_child_runtime"
            ));
        }
        if tx_ingress_gate_source != "receiver_child_runtime" {
            violations.push(format!(
                "tx_ingress_aoem_gate_config_source={tx_ingress_gate_source} expected receiver_child_runtime"
            ));
        }
        if !tx_ingress_gate_production_candidate {
            violations.push(
                "tx_ingress_aoem_gate_config_production_candidate=false expected true".to_string(),
            );
        }
        if !tx_ingress_gate_shadow {
            violations.push("tx_ingress_aoem_gate_config_shadow=false expected true".to_string());
        }
        if !tx_ingress_gate_compare {
            violations.push("tx_ingress_aoem_gate_config_compare=false expected true".to_string());
        }
        if !child_gate_propagated {
            violations.push(
                "aoem_owned_child_runtime_gate_propagated_to_tx_ingress=false expected true"
                    .to_string(),
            );
        }
        if !single_path_enforced {
            violations.push("aoem_owned_single_path_enforced=false expected true".to_string());
        }
        if legacy_fallback_gate_enabled
            && !bool_env(NOV_NATIVE_LEGACY_HOST_TRANSITIONAL_FALLBACK_ENV)
        {
            violations.push(
                "legacy_host_transitional_fallback_gate_enabled=true without wrapper fallback env"
                    .to_string(),
            );
        }
        if legacy_fallback_used {
            violations
                .push("legacy_host_transitional_fallback_used=true expected false".to_string());
        }
        if legacy_success_suppressed {
            violations.push(
                "legacy_host_transitional_success_suppressed_by_aoem_gate=true expected false"
                    .to_string(),
            );
        }
        if !regression_signable {
            violations.push("aoem_owned_regression_signable=false expected true".to_string());
        }
        if !explicit_gate_config {
            violations.push(
                "tx_ingress_called_with_explicit_aoem_gate_config=false expected true".to_string(),
            );
        }
        if !enabled {
            violations.push(
                "aoem_native_tx_batch_production_candidate_enabled=false expected true".to_string(),
            );
        }
        if !result_ok {
            violations.push(
                "aoem_native_tx_batch_production_candidate_result_ok=false expected true"
                    .to_string(),
            );
        }
        if owner != AOEM_RUNTIME_OWNED_PRODUCTION_TARGET_V1 {
            violations.push(format!(
                "aoem_native_tx_batch_production_owner={owner} expected {AOEM_RUNTIME_OWNED_PRODUCTION_TARGET_V1}"
            ));
        }
        if target != AOEM_RUNTIME_OWNED_PRODUCTION_TARGET_V1 {
            violations.push(format!(
                "tx_ingress_production_target={target} expected {AOEM_RUNTIME_OWNED_PRODUCTION_TARGET_V1}"
            ));
        }
        if selected_path != AOEM_RUNTIME_OWNED_PRODUCTION_TARGET_V1 {
            violations.push(format!(
                "tx_ingress_selected_path={selected_path} expected {AOEM_RUNTIME_OWNED_PRODUCTION_TARGET_V1}"
            ));
        }
        if receipt_count != tx_count {
            violations.push(format!(
                "aoem_native_tx_batch_production_receipt_count={receipt_count} expected {tx_count}"
            ));
        }
        if canonical_proof_count != tx_count {
            violations.push(format!(
                "aoem_native_tx_batch_production_canonical_proof_count={canonical_proof_count} expected {tx_count}"
            ));
        }
        if ledger_close_proof_count != tx_count {
            violations.push(format!(
                "aoem_native_tx_batch_production_ledger_close_proof_count={ledger_close_proof_count} expected {tx_count}"
            ));
        }
        if !state_delta_root_present {
            violations.push(
                "aoem_native_tx_batch_production_state_delta_root_present=false expected true"
                    .to_string(),
            );
        }
        if !snapshot_metadata_present {
            violations.push(
                "aoem_native_tx_batch_production_snapshot_metadata_present=false expected true"
                    .to_string(),
            );
        }
        if fallback_used {
            violations.push(
                "aoem_native_tx_batch_production_fallback_used=true expected false".to_string(),
            );
        }
        if !mismatch_reasons_empty {
            violations.push(
                "aoem_native_tx_batch_production_mismatch_reasons nonempty expected []".to_string(),
            );
        }
        if double_write {
            violations.push(
                "aoem_native_tx_batch_production_double_write_legacy_canonical=true expected false"
                    .to_string(),
            );
        }
    }

    serde_json::json!({
        "aoem_native_tx_batch_production_candidate_enabled": enabled,
        "aoem_native_tx_batch_production_candidate_result_ok": result_ok,
        "aoem_native_tx_batch_production_owner": owner,
        "tx_ingress_production_target": target,
        "tx_ingress_selected_path": selected_path,
        "receiver_final_summary_aoem_fields_present": final_summary_fields_present,
        "receiver_final_summary_aoem_fields_defaulted": final_summary_fields_defaulted,
        "receiver_final_summary_aoem_fields_missing_reasons": summary
            .get("receiver_final_summary_aoem_fields_missing_reasons")
            .cloned()
            .unwrap_or_else(|| serde_json::json!([])),
        "aoem_owned_gate_fail_reason": aoem_owned_gate_fail_reason,
        "child_runtime_aoem_gate_config_source": child_runtime_gate_source,
        "tx_ingress_aoem_gate_config_source": tx_ingress_gate_source,
        "tx_ingress_aoem_gate_config_production_candidate": tx_ingress_gate_production_candidate,
        "tx_ingress_aoem_gate_config_shadow": tx_ingress_gate_shadow,
        "tx_ingress_aoem_gate_config_compare": tx_ingress_gate_compare,
        "aoem_owned_child_runtime_gate_propagated_to_tx_ingress": child_gate_propagated,
        "aoem_owned_single_path_enforced": single_path_enforced,
        "legacy_host_transitional_fallback_gate_enabled": legacy_fallback_gate_enabled,
        "legacy_host_transitional_fallback_used": legacy_fallback_used,
        "legacy_host_transitional_success_suppressed_by_aoem_gate": legacy_success_suppressed,
        "aoem_owned_regression_signable": regression_signable,
        "aoem_owned_signoff_blocker_reasons": summary
            .get("aoem_owned_signoff_blocker_reasons")
            .cloned()
            .unwrap_or_else(|| serde_json::json!([])),
        "tx_ingress_real_callsite": summary
            .get("tx_ingress_real_callsite")
            .cloned()
            .unwrap_or(Value::Null),
        "tx_ingress_called_with_explicit_aoem_gate_config": explicit_gate_config,
        "aoem_native_tx_batch_production_receipt_count": receipt_count,
        "aoem_native_tx_batch_production_canonical_proof_count": canonical_proof_count,
        "aoem_native_tx_batch_production_ledger_close_proof_count": ledger_close_proof_count,
        "aoem_native_tx_batch_production_state_delta_root_present": state_delta_root_present,
        "aoem_native_tx_batch_production_snapshot_metadata_present": snapshot_metadata_present,
        "aoem_native_tx_batch_production_fallback_used": fallback_used,
        "aoem_native_tx_batch_production_mismatch_reasons": summary
            .get("aoem_native_tx_batch_production_mismatch_reasons")
            .cloned()
            .unwrap_or_else(|| serde_json::json!([])),
        "aoem_native_tx_batch_production_double_write_legacy_canonical": double_write,
    })
}

fn validate_receiver_report(summary: &Value, probe: &Value, tx_count: u64) -> (Value, Vec<String>) {
    let receipt_count = probe_u64(probe, "receipt_count");
    let semantic_sequence = semantic_sequence(probe);
    let received_unique = summary_u64(summary, "ingress_total_last")
        .max(summary_u64(summary, "aoem_executed_total"))
        .max(receipt_count);
    let canonical_unique_included = summary_u64(summary, "included_canonical_total")
        .max(receipt_count)
        .max(semantic_sequence);
    let duplicate_canonical_included = canonical_unique_included.saturating_sub(tx_count);
    let duplicate_receipt = receipt_count.saturating_sub(tx_count);
    let semantic_head_monotonic = probe
        .get("semantic_head_current_recovered")
        .and_then(Value::as_bool)
        == Some(true)
        && probe
            .get("semantic_head_by_height_recovered")
            .and_then(Value::as_bool)
            == Some(true)
        && semantic_sequence >= canonical_unique_included;
    let receipt_index_consistent = probe
        .get("receipt_index_recovered")
        .and_then(Value::as_bool)
        == Some(true)
        && receipt_count == tx_count;
    let final_missing_sequence_count = tx_count.saturating_sub(received_unique.min(tx_count));
    let final_missing_ranges = missing_ranges_from_progress(received_unique, tx_count, 1);
    let repair_received_ranges = missing_ranges_from_json(
        summary
            .get("repair_sequence_received_ranges_sample")
            .unwrap_or(&Value::Null),
    );
    let repair_accepted_ranges = missing_ranges_from_json(
        summary
            .get("repair_sequence_accepted_ranges_sample")
            .unwrap_or(&Value::Null),
    );
    let repair_enqueued_ranges = missing_ranges_from_json(
        summary
            .get("repair_sequence_enqueued_ranges_sample")
            .unwrap_or(&Value::Null),
    );
    let repair_already_receipted_ranges = missing_ranges_from_json(
        summary
            .get("repair_sequence_already_receipted_ranges_sample")
            .unwrap_or(&Value::Null),
    );
    let repair_duplicate_ranges = missing_ranges_from_json(
        summary
            .get("repair_sequence_duplicate_ranges_sample")
            .unwrap_or(&Value::Null),
    );
    let repair_admitted_to_aoem_ranges = missing_ranges_from_json(
        summary
            .get("repair_sequence_admitted_to_aoem_ranges_sample")
            .unwrap_or(&Value::Null),
    );
    let repair_sequence_received_final_missing_overlap_count =
        missing_ranges_overlap_count(&final_missing_ranges, &repair_received_ranges);
    let repair_sequence_accepted_final_missing_overlap_count =
        missing_ranges_overlap_count(&final_missing_ranges, &repair_accepted_ranges);
    let repair_sequence_enqueued_final_missing_overlap_count =
        missing_ranges_overlap_count(&final_missing_ranges, &repair_enqueued_ranges);
    let repair_sequence_already_receipted_final_missing_overlap_count =
        missing_ranges_overlap_count(&final_missing_ranges, &repair_already_receipted_ranges);
    let repair_sequence_duplicate_final_missing_overlap_count =
        missing_ranges_overlap_count(&final_missing_ranges, &repair_duplicate_ranges);
    let repair_sequence_admitted_to_aoem_final_missing_overlap_count =
        missing_ranges_overlap_count(&final_missing_ranges, &repair_admitted_to_aoem_ranges);
    let receipt_index_false_positive_suspected =
        repair_sequence_already_receipted_final_missing_overlap_count > 0;
    let repair_accepted_but_not_effective_count =
        repair_sequence_accepted_final_missing_overlap_count
            .saturating_sub(repair_sequence_enqueued_final_missing_overlap_count)
            .saturating_sub(repair_sequence_already_receipted_final_missing_overlap_count);
    let mut repair_accepted_but_not_effective_reason_counts = serde_json::Map::new();
    if receipt_index_false_positive_suspected {
        repair_accepted_but_not_effective_reason_counts.insert(
            "already_receipted_final_missing_overlap".to_string(),
            serde_json::json!(repair_sequence_already_receipted_final_missing_overlap_count),
        );
    }
    if repair_accepted_but_not_effective_count > 0 {
        repair_accepted_but_not_effective_reason_counts.insert(
            "accepted_not_enqueued_or_receipted_overlap".to_string(),
            serde_json::json!(repair_accepted_but_not_effective_count),
        );
    }
    let mut violations = Vec::<String>::new();
    validate_boundaries(summary, &mut violations);
    let aoem_production_candidate =
        validate_aoem_production_candidate_summary(summary, tx_count, &mut violations);
    if received_unique != tx_count {
        violations.push(format!(
            "received_unique={received_unique} expected tx_count={tx_count}"
        ));
    }
    if canonical_unique_included != tx_count {
        violations.push(format!(
            "canonical_unique_included={canonical_unique_included} expected tx_count={tx_count}"
        ));
    }
    if duplicate_canonical_included != 0 {
        violations.push(format!(
            "duplicate_canonical_included={duplicate_canonical_included} expected 0"
        ));
    }
    if duplicate_receipt != 0 {
        violations.push(format!("duplicate_receipt={duplicate_receipt} expected 0"));
    }
    if summary_u64(summary, "queue_pending_last") != 0 {
        violations.push(format!(
            "queue_pending_last={} expected 0",
            summary_u64(summary, "queue_pending_last")
        ));
    }
    if !semantic_head_monotonic {
        violations.push("semantic_head_monotonic=false".to_string());
    }
    if !receipt_index_consistent {
        violations.push("receipt_index_consistent=false".to_string());
    }
    (
        serde_json::json!({
            "received_unique": received_unique,
            "canonical_unique_included": canonical_unique_included,
            "duplicate_canonical_included": duplicate_canonical_included,
            "duplicate_receipt": duplicate_receipt,
            "queue_pending_last": summary_u64(summary, "queue_pending_last"),
            "semantic_head_monotonic": semantic_head_monotonic,
            "receipt_index_consistent": receipt_index_consistent,
            "aoem_concurrency_owner": summary_str(summary, "aoem_concurrency_owner"),
            "final_missing_sequence_count": final_missing_sequence_count,
            "final_missing_ranges_sample": missing_ranges_to_json(final_missing_ranges.as_slice(), 256),
            "repair_sequence_received_final_missing_overlap_count": repair_sequence_received_final_missing_overlap_count,
            "repair_sequence_accepted_final_missing_overlap_count": repair_sequence_accepted_final_missing_overlap_count,
            "repair_sequence_enqueued_final_missing_overlap_count": repair_sequence_enqueued_final_missing_overlap_count,
            "repair_sequence_admitted_to_aoem_final_missing_overlap_count": repair_sequence_admitted_to_aoem_final_missing_overlap_count,
            "repair_sequence_already_receipted_final_missing_overlap_count": repair_sequence_already_receipted_final_missing_overlap_count,
            "repair_sequence_pending_duplicate_final_missing_overlap_count": repair_sequence_duplicate_final_missing_overlap_count,
            "receipt_index_hit_for_final_missing_count": repair_sequence_already_receipted_final_missing_overlap_count,
            "receipt_index_hit_for_final_missing_sample": missing_ranges_to_json(repair_already_receipted_ranges.as_slice(), 256),
            "receipt_index_false_positive_suspected": receipt_index_false_positive_suspected,
            "repair_final_missing_force_enqueued_count": repair_sequence_enqueued_final_missing_overlap_count,
            "repair_final_missing_already_pending_count": repair_sequence_duplicate_final_missing_overlap_count,
            "repair_final_missing_receipt_hit_count": repair_sequence_already_receipted_final_missing_overlap_count,
            "repair_final_missing_enqueue_failed_count": repair_accepted_but_not_effective_count,
            "repair_final_missing_enqueue_failed_reason_counts": Value::Object(repair_accepted_but_not_effective_reason_counts.clone()),
            "repair_accepted_but_not_effective_count": repair_accepted_but_not_effective_count,
            "repair_accepted_but_not_effective_reason_counts": Value::Object(repair_accepted_but_not_effective_reason_counts),
            "repair_accepted_but_not_effective_ranges_sample": missing_ranges_to_json(final_missing_ranges.as_slice(), 256),
            "aoem_production_candidate": aoem_production_candidate,
        }),
        violations,
    )
}

fn run_sender(
    chain_id: u64,
    tx_count: u64,
    sender_node: u64,
    receiver_node: u64,
    sender_addr: &str,
    receiver_addr: &str,
    fault: FaultConfigV1,
    sustained: SustainedConfigV1,
    tail_repair: TailRepairConfigV1,
    udp_send_retry: UdpSendRetryConfigV1,
    udp_ack: UdpAckConfigV1,
    novorudp: NovoRudpConfigV1,
) -> Result<Value> {
    let sender_started = Instant::now();
    let default_sender_hard_timeout_ms = sustained
        .duration_seconds
        .saturating_mul(1000)
        .saturating_add(120_000)
        .max(1);
    let sender_hard_timeout_ms = u64_env(
        "NOVOVM_NATIVE_PIPELINE_SENDER_HARD_TIMEOUT_MS",
        default_sender_hard_timeout_ms,
    )?;
    let repair_budget_profile = novorudp_sender_repair_budget_profile_v1(
        novorudp.enabled,
        sustained.duration_seconds,
        sender_hard_timeout_ms,
    )?;
    let repair_no_progress_timeout_ms = repair_budget_profile.repair_no_progress_timeout_ms;
    let absolute_sender_max_timeout_ms = repair_budget_profile.absolute_max_timeout_ms;
    let repair_continuation_timeout_ms = repair_budget_profile.repair_continuation_timeout_ms;
    let extend_repair_deadline_on_ack_progress =
        repair_budget_profile.extend_repair_deadline_on_ack_progress;
    let sender_report_on_timeout = bool_env("NOVOVM_NATIVE_PIPELINE_SENDER_REPORT_ON_TIMEOUT")
        || string_env_nonempty("NOVOVM_NATIVE_PIPELINE_SENDER_REPORT_ON_TIMEOUT").is_none();
    let sender_exit_on_repair_timeout =
        bool_env("NOVOVM_NATIVE_PIPELINE_SENDER_EXIT_ON_REPAIR_TIMEOUT")
            || string_env_nonempty("NOVOVM_NATIVE_PIPELINE_SENDER_EXIT_ON_REPAIR_TIMEOUT")
                .is_none();
    let mut sender_hard_timeout_reached = false;
    let mut absolute_sender_timeout_reached = false;
    let mut sender_repair_no_progress_timeout_reached = false;
    let mut repair_progress_observed_count = 0u64;
    let mut repair_progress_last_observed_ms = 0u64;
    let mut repair_continuation_deadline_extended_count = 0u64;
    let mut no_progress_started_at: Option<Instant> = None;
    let mut no_progress_elapsed_ms = 0u64;
    let mut repair_still_progressing_at_hard_timeout = false;
    let mut sender_exit_reason = "not_finished".to_string();
    let mut stats = empty_send_stats();
    let mut repair_stats = empty_send_stats();
    let mut tail_repair_ack_received_count = 0u64;
    let mut tail_repair_udp_ack_received_count = 0u64;
    let mut tail_repair_missing_ranges_seen = 0u64;
    let mut tail_repair_fallback_used_count = 0u64;
    let mut tail_repair_file_ack_used_count = 0u64;
    let mut tail_repair_udp_ack_used_count = 0u64;
    let mut final_missing_count = tx_count;
    let mut latest_ack_missing_count: Option<u64> = None;
    let mut latest_ack_missing_ranges_sample = Vec::<MissingRangeV1>::new();
    let mut latest_ack_missing_ranges_full_count: Option<u64> = None;
    let mut latest_ack_highest_sequence_seen: Option<u64> = None;
    let mut latest_ack_receiver_done = false;
    let mut tail_repair_latest_ack_epoch = 0u64;
    let primary_ack_drain_enabled = novorudp.enabled
        && udp_ack.enabled
        && (bool_env("NOVOVM_NOVORUDP_PRIMARY_SEND_ACK_DRAIN_ENABLED")
            || string_env_nonempty("NOVOVM_NOVORUDP_PRIMARY_SEND_ACK_DRAIN_ENABLED").is_none());
    let primary_ack_drain_interval_ms =
        u64_env("NOVOVM_NOVORUDP_PRIMARY_SEND_ACK_DRAIN_INTERVAL_MS", 250)?.max(1);
    let sender_live_report_interval_ms =
        u64_env("NOVOVM_NOVORUDP_SENDER_LIVE_REPORT_INTERVAL_MS", 5_000)?;
    let sender_progress_path = sender_progress_report_path();
    let mut primary_ack_drain_count = 0u64;
    let mut primary_ack_received_count = 0u64;
    let mut primary_ack_drain_empty_count = 0u64;
    let mut primary_ack_last_consumed_elapsed_ms = 0u64;
    let mut sender_live_report_write_count = 0u64;
    let mut sender_live_report_last_elapsed_ms = 0u64;
    let mut last_sender_live_report_at = Instant::now();
    let mut repair_used_full_missing_ranges = false;
    let mut repair_sequence_sent_ranges = Vec::<MissingRangeV1>::new();
    let mut repair_sequence_sent_count = 0u64;
    let mut repair_sequence_sent_min: Option<u64> = None;
    let mut repair_sequence_sent_max: Option<u64> = None;
    let mut repair_rounds_detail_sample = Vec::<Value>::new();
    let mut repair_no_progress_rounds = 0u64;
    let mut tail_gap_detected = false;
    let mut tail_gap_range: Option<MissingRangeV1> = None;
    let mut tail_gap_repair_sent_count = 0u64;
    let mut tail_gap_repair_packet_count = 0u64;
    let mut tail_gap_repair_rounds = 0u64;
    let mut tail_gap_ack_after_missing_count: Option<u64> = None;
    let mut novorudp_window_round_count = 0u64;
    let mut novorudp_window_success_count = 0u64;
    let mut novorudp_window_failed_count = 0u64;
    let mut novorudp_window_no_progress_count = 0u64;
    let mut novorudp_windows_detail_sample = Vec::<Value>::new();
    let mut novorudp_window_retry_counts = BTreeMap::<u64, u64>::new();
    let mut current_missing_bitmap_used = false;
    let mut repair_used_full_missing_bitmap = false;
    let mut tail_window_missing_before: Option<u64> = None;
    let mut tail_window_missing_after: Option<u64> = None;
    let mut tail_window_missing_delta: Option<u64> = None;
    let mut tail_window_remaining_missing_count: Option<u64> = None;
    let mut tail_window_remaining_missing_ranges_sample = Vec::<MissingRangeV1>::new();
    let mut tail_window_success_by_bitmap = false;
    let tail_window_success_by_max_sequence_only = false;
    let mut current_window_repair_sequence_sent_count = 0u64;
    let mut current_window_repair_missing_sequence_sent_count = 0u64;
    let mut current_window_repair_missing_sequence_covered_count = 0u64;
    let mut current_window_ack_missing_count_after: Option<u64> = None;
    let mut latest_ack_received_at: Option<Instant> = None;
    let mut latest_ack_stale_rounds = 0u64;
    let mut latest_ack_stale_duration_ms = 0u64;
    let mut ack_epoch_at_repair_start: Option<u64> = None;
    let mut ack_epoch_at_repair_end: Option<u64> = None;
    let mut ack_highest_sequence_seen_at_repair_start: Option<u64> = None;
    let mut ack_highest_sequence_seen_at_repair_end: Option<u64> = None;
    let mut repair_window_recomputed_count = 0u64;
    let mut repair_window_recomputed_due_to_ack_progress = 0u64;
    let mut stale_ack_repair_aborted_count = 0u64;
    let moving_window_enabled = novorudp.enabled;
    let mut moving_window_last_range: Option<MissingRangeV1> = None;
    let mut moving_window_last_ack_epoch: Option<u64> = None;
    let final_ack_wait_ms = u64_env("NOVOVM_NATIVE_PIPELINE_FINAL_ACK_WAIT_MS", 10_000)?;
    let final_ack_poll_ms = u64_env("NOVOVM_NATIVE_PIPELINE_FINAL_ACK_POLL_MS", 500)?.max(1);
    let mut final_ack_wait_elapsed_ms = 0u64;
    let mut final_ack_received_after_repair = false;
    let mut final_ack_epoch: Option<u64> = None;
    let mut final_ack_missing_count: Option<u64> = None;
    let mut final_ack_receiver_done: Option<bool> = None;
    let mut final_ack_grace_timeout = false;
    let ack_socket = if udp_ack.enabled {
        let socket = UdpSocket::bind(udp_ack.bind_addr.as_str())
            .with_context(|| format!("bind sender UDP ack socket failed: {}", udp_ack.bind_addr))?;
        socket
            .set_nonblocking(true)
            .context("set sender UDP ack socket nonblocking failed")?;
        Some(socket)
    } else {
        None
    };
    let ack_socket_addr = ack_socket
        .as_ref()
        .and_then(|socket| socket.local_addr().ok())
        .map(|addr| addr.to_string());
    macro_rules! drain_primary_sender_ack {
        () => {{
            if primary_ack_drain_enabled {
                if let Some(socket) = ack_socket.as_ref() {
                    primary_ack_drain_count = primary_ack_drain_count.saturating_add(1);
                    let state = drain_udp_ack_socket(socket, tail_repair.missing_sample_limit, 0);
                    if state.received_count > 0 {
                        primary_ack_received_count =
                            primary_ack_received_count.saturating_add(state.received_count);
                        tail_repair_ack_received_count =
                            tail_repair_ack_received_count.saturating_add(state.received_count);
                        tail_repair_udp_ack_received_count =
                            tail_repair_udp_ack_received_count.saturating_add(state.received_count);
                        let previous_epoch = tail_repair_latest_ack_epoch;
                        let previous_highest = latest_ack_highest_sequence_seen;
                        tail_repair_latest_ack_epoch =
                            tail_repair_latest_ack_epoch.max(state.latest_epoch);
                        latest_ack_received_at = Some(Instant::now());
                        primary_ack_last_consumed_elapsed_ms =
                            sender_started.elapsed().as_millis() as u64;
                        latest_ack_missing_count = Some(state.latest_missing_count);
                        latest_ack_missing_ranges_sample = state.latest_ranges.clone();
                        latest_ack_missing_ranges_full_count =
                            Some(state.missing_ranges_full_count);
                        latest_ack_highest_sequence_seen = state.highest_sequence_seen;
                        latest_ack_receiver_done = state.receiver_done;
                        final_missing_count = state.latest_missing_count;
                        if novorudp.enabled
                            && state.latest_epoch > previous_epoch
                            && state.highest_sequence_seen.unwrap_or_default()
                                > previous_highest.unwrap_or_default()
                        {
                            repair_window_recomputed_count =
                                repair_window_recomputed_count.saturating_add(1);
                            repair_window_recomputed_due_to_ack_progress =
                                repair_window_recomputed_due_to_ack_progress.saturating_add(1);
                        }
                    } else {
                        primary_ack_drain_empty_count =
                            primary_ack_drain_empty_count.saturating_add(1);
                    }
                }
            }
        }};
    }
    macro_rules! maybe_write_sender_live_progress {
        ($force:expr, $sent_unique_target:expr) => {{
            if sender_live_report_interval_ms > 0
                && ($force
                    || last_sender_live_report_at.elapsed()
                        >= Duration::from_millis(sender_live_report_interval_ms))
            {
                sender_live_report_last_elapsed_ms = sender_started.elapsed().as_millis() as u64;
                if write_sender_live_progress_report_v1(
                    sender_progress_path.as_path(),
                    sender_live_report_last_elapsed_ms,
                    tx_count,
                    $sent_unique_target,
                    stats.sent_packets,
                    stats.send_failed_count,
                    primary_ack_drain_count,
                    primary_ack_received_count,
                    primary_ack_drain_empty_count,
                    primary_ack_last_consumed_elapsed_ms,
                    tail_repair_latest_ack_epoch,
                    latest_ack_missing_count,
                    latest_ack_highest_sequence_seen,
                    latest_ack_receiver_done,
                    $sent_unique_target == tx_count && stats.send_failed_count == 0,
                )
                .is_ok()
                {
                    sender_live_report_write_count =
                        sender_live_report_write_count.saturating_add(1);
                    if !$force {
                        last_sender_live_report_at = Instant::now();
                    }
                }
            }
        }};
    }
    let tx_per_round = if sustained.enabled {
        sustained.tx_per_round.max(1)
    } else {
        tx_count
    };
    let rounds = div_ceil_u64(tx_count, tx_per_round).max(1);
    let mut sent_unique_target = 0u64;
    for round in 0..rounds {
        if sender_hard_timeout_ms > 0
            && sender_started.elapsed() >= Duration::from_millis(sender_hard_timeout_ms)
        {
            sender_hard_timeout_reached = true;
            break;
        }
        let remaining = tx_count.saturating_sub(sent_unique_target);
        if remaining == 0 {
            break;
        }
        let round_tx_count = remaining.min(tx_per_round);
        let txs = build_native_payloads_from_index(chain_id, sent_unique_target, round_tx_count)?;
        let scheduled = apply_fault_schedule(txs.as_slice(), fault);
        let round_stats = send_scheduled_batch(
            chain_id,
            sender_node,
            receiver_node,
            sender_addr,
            receiver_addr,
            scheduled.as_slice(),
            fault.delay_ms,
            udp_send_retry,
        )?;
        sent_unique_target = sent_unique_target.saturating_add(round_tx_count);
        merge_send_stats(&mut stats, round_stats);
        drain_primary_sender_ack!();
        maybe_write_sender_live_progress!(false, sent_unique_target);
        if stats.send_failed_count > 0 {
            break;
        }
        if sustained.enabled && round + 1 < rounds && sustained.round_interval_ms > 0 {
            let mut slept_ms = 0u64;
            while slept_ms < sustained.round_interval_ms {
                let remaining_ms = sustained.round_interval_ms.saturating_sub(slept_ms);
                let sleep_ms = if primary_ack_drain_enabled {
                    remaining_ms.min(primary_ack_drain_interval_ms)
                } else {
                    remaining_ms
                };
                std::thread::sleep(std::time::Duration::from_millis(sleep_ms));
                slept_ms = slept_ms.saturating_add(sleep_ms);
                drain_primary_sender_ack!();
                maybe_write_sender_live_progress!(false, sent_unique_target);
            }
        }
    }
    maybe_write_sender_live_progress!(true, sent_unique_target);
    let _sender_live_report_timer_anchor = last_sender_live_report_at;
    let mut repair_rounds_used = 0u64;
    if stats.send_failed_count == 0 && tail_repair.enabled && tail_repair.rounds > 0 {
        let repair_loop_rounds = if novorudp.enabled {
            div_ceil_u64(tx_count.max(1), novorudp.window_size.max(1))
                .saturating_mul(novorudp.max_window_retries)
                .saturating_add(tail_repair.rounds)
                .max(tail_repair.rounds)
        } else {
            tail_repair.rounds
        };
        let repair_send_config = novorudp.repair_config(tail_repair);
        for repair_round in 0..repair_loop_rounds {
            if novorudp.enabled {
                let elapsed_ms = sender_started.elapsed().as_millis() as u64;
                no_progress_elapsed_ms = no_progress_started_at
                    .map(|started| started.elapsed().as_millis() as u64)
                    .unwrap_or_default();
                match novorudp_sender_timeout_decision_v1(
                    elapsed_ms,
                    no_progress_elapsed_ms,
                    repair_no_progress_timeout_ms,
                    absolute_sender_max_timeout_ms,
                    latest_ack_receiver_done,
                    latest_ack_missing_count.unwrap_or(final_missing_count),
                ) {
                    NovoRudpSenderTimeoutDecisionV1::Continue => {}
                    NovoRudpSenderTimeoutDecisionV1::NoProgressTimeout => {
                        sender_repair_no_progress_timeout_reached = true;
                        sender_hard_timeout_reached = true;
                        sender_exit_reason = "sender_repair_no_progress_timeout".to_string();
                        break;
                    }
                    NovoRudpSenderTimeoutDecisionV1::AbsoluteTimeout => {
                        absolute_sender_timeout_reached = true;
                        sender_hard_timeout_reached = true;
                        sender_exit_reason = "sender_absolute_timeout".to_string();
                        break;
                    }
                }
                if sender_hard_timeout_ms > 0
                    && elapsed_ms >= sender_hard_timeout_ms
                    && no_progress_started_at.is_none()
                {
                    repair_still_progressing_at_hard_timeout = true;
                }
            } else if sender_hard_timeout_ms > 0
                && sender_started.elapsed() >= Duration::from_millis(sender_hard_timeout_ms)
            {
                sender_hard_timeout_reached = true;
                sender_exit_reason = "sender_finalization_timeout".to_string();
                break;
            }
            if tail_repair.round_pause_ms > 0 {
                std::thread::sleep(std::time::Duration::from_millis(tail_repair.round_pause_ms));
            }
            if !novorudp.enabled
                && sender_hard_timeout_ms > 0
                && sender_started.elapsed() >= Duration::from_millis(sender_hard_timeout_ms)
            {
                sender_hard_timeout_reached = true;
                sender_exit_reason = "sender_finalization_timeout".to_string();
                break;
            }
            let udp_ack_state = ack_socket.as_ref().map(|socket| {
                drain_udp_ack_socket(
                    socket,
                    tail_repair.missing_sample_limit,
                    udp_ack.recv_timeout_ms,
                )
            });
            let ack_epoch_before = udp_ack_state
                .as_ref()
                .filter(|state| state.received_count > 0)
                .map(|state| state.latest_epoch)
                .filter(|epoch| *epoch > 0);
            let missing_count_before = udp_ack_state
                .as_ref()
                .filter(|state| state.received_count > 0)
                .map(|state| state.latest_missing_count)
                .or(latest_ack_missing_count)
                .unwrap_or_else(|| tx_count.saturating_sub(stats.sent_unique));
            if let Some(state) = udp_ack_state.as_ref() {
                if state.received_count > 0 {
                    let previous_epoch = tail_repair_latest_ack_epoch;
                    let previous_highest = latest_ack_highest_sequence_seen;
                    tail_repair_ack_received_count =
                        tail_repair_ack_received_count.saturating_add(state.received_count);
                    tail_repair_udp_ack_received_count =
                        tail_repair_udp_ack_received_count.saturating_add(state.received_count);
                    tail_repair_latest_ack_epoch =
                        tail_repair_latest_ack_epoch.max(state.latest_epoch);
                    latest_ack_received_at = Some(Instant::now());
                    ack_epoch_at_repair_start = Some(state.latest_epoch);
                    ack_highest_sequence_seen_at_repair_start = state.highest_sequence_seen;
                    if novorudp.enabled
                        && state.latest_epoch > previous_epoch
                        && state.highest_sequence_seen.unwrap_or_default()
                            > previous_highest.unwrap_or_default()
                    {
                        repair_window_recomputed_count =
                            repair_window_recomputed_count.saturating_add(1);
                        repair_window_recomputed_due_to_ack_progress =
                            repair_window_recomputed_due_to_ack_progress.saturating_add(1);
                    }
                    latest_ack_missing_count = Some(state.latest_missing_count);
                    latest_ack_missing_ranges_sample = state.latest_ranges.clone();
                    latest_ack_missing_ranges_full_count = Some(state.missing_ranges_full_count);
                    latest_ack_highest_sequence_seen = state.highest_sequence_seen;
                    latest_ack_receiver_done = state.receiver_done;
                    final_missing_count = state.latest_missing_count;
                    if state.receiver_done && state.latest_missing_count == 0 {
                        repair_rounds_used = repair_rounds_used.saturating_add(1);
                        break;
                    }
                }
            }
            let udp_ack_for_repair = udp_ack_state
                .as_ref()
                .filter(|state| state.received_count > 0 && state.latest_missing_count > 0);
            let udp_ranges = udp_ack_for_repair
                .filter(|state| !state.latest_ranges.is_empty())
                .map(|state| state.latest_ranges.clone());
            let ack_path = ack_report_path();
            let file_ack_ranges =
                read_missing_ranges_from_ack(ack_path.as_path(), tail_repair.missing_sample_limit);
            let mut novorudp_window_id_this_round: Option<u64> = None;
            let mut novorudp_window_range_this_round: Option<MissingRangeV1> = None;
            let mut novorudp_selected_ranges_this_round = Vec::<MissingRangeV1>::new();
            let mut novorudp_used_full_missing_bitmap_this_round = false;
            let mut txs = if let Some(state) = udp_ack_for_repair {
                tail_repair_udp_ack_used_count = tail_repair_udp_ack_used_count.saturating_add(1);
                let selected_ranges = if novorudp.enabled {
                    if let Some(selection) = select_novorudp_repair_ranges_from_receiver_ack(
                        state,
                        tx_count,
                        novorudp.window_size,
                        novorudp.tail_window_max_retries,
                    ) {
                        novorudp_window_id_this_round = Some(selection.window_id);
                        novorudp_window_range_this_round = Some(selection.window);
                        novorudp_used_full_missing_bitmap_this_round =
                            selection.used_full_missing_bitmap;
                        if selection.used_full_missing_bitmap {
                            current_missing_bitmap_used = true;
                            repair_used_full_missing_bitmap = true;
                        }
                        novorudp_selected_ranges_this_round = selection.ranges.clone();
                        selection.ranges
                    } else {
                        Vec::new()
                    }
                } else {
                    state.latest_ranges.clone()
                };
                repair_used_full_missing_ranges = if novorudp.enabled {
                    repair_used_full_missing_ranges || novorudp_used_full_missing_bitmap_this_round
                } else {
                    state.missing_ranges_full_count
                        <= state.latest_ranges.len().try_into().unwrap_or(u64::MAX)
                };
                tail_repair_missing_ranges_seen = tail_repair_missing_ranges_seen
                    .saturating_add(state.latest_ranges.len().try_into().unwrap_or(u64::MAX));
                final_missing_count = missing_ranges_count(selected_ranges.as_slice());
                repair_sequence_sent_count =
                    repair_sequence_sent_count.saturating_add(final_missing_count);
                if novorudp.enabled {
                    current_window_repair_sequence_sent_count =
                        current_window_repair_sequence_sent_count
                            .saturating_add(final_missing_count);
                    current_window_repair_missing_sequence_sent_count =
                        current_window_repair_missing_sequence_sent_count
                            .saturating_add(final_missing_count);
                    tail_window_missing_before = Some(missing_count_before);
                }
                repair_sequence_sent_ranges.extend(selected_ranges.iter().copied());
                for range in &selected_ranges {
                    repair_sequence_sent_min = Some(
                        repair_sequence_sent_min
                            .map(|current| current.min(range.start))
                            .unwrap_or(range.start),
                    );
                    repair_sequence_sent_max = Some(
                        repair_sequence_sent_max
                            .map(|current| current.max(range.end_inclusive))
                            .unwrap_or(range.end_inclusive),
                    );
                }
                build_tail_repair_payloads_from_ranges(
                    chain_id,
                    selected_ranges.as_slice(),
                    repair_round,
                )?
            } else if let Some(ranges) =
                file_ack_ranges.as_ref().filter(|ranges| !ranges.is_empty())
            {
                tail_repair_file_ack_used_count = tail_repair_file_ack_used_count.saturating_add(1);
                tail_repair_ack_received_count = tail_repair_ack_received_count.saturating_add(1);
                let selected_ranges = if novorudp.enabled {
                    if let Some(selection) = select_novorudp_repair_ranges_from_ack(
                        ranges.as_slice(),
                        tx_count,
                        novorudp.window_size,
                        missing_count_before,
                        ranges.len().try_into().unwrap_or(u64::MAX),
                        novorudp.tail_window_max_retries,
                    ) {
                        novorudp_window_id_this_round = Some(selection.window_id);
                        novorudp_window_range_this_round = Some(selection.window);
                        novorudp_used_full_missing_bitmap_this_round =
                            selection.used_full_missing_bitmap;
                        if selection.used_full_missing_bitmap {
                            current_missing_bitmap_used = true;
                            repair_used_full_missing_bitmap = true;
                        }
                        novorudp_selected_ranges_this_round = selection.ranges.clone();
                        selection.ranges
                    } else {
                        Vec::new()
                    }
                } else {
                    ranges.clone()
                };
                tail_repair_missing_ranges_seen = tail_repair_missing_ranges_seen
                    .saturating_add(ranges.len().try_into().unwrap_or(u64::MAX));
                latest_ack_missing_ranges_sample = ranges.clone();
                final_missing_count = missing_ranges_count(selected_ranges.as_slice());
                repair_sequence_sent_count =
                    repair_sequence_sent_count.saturating_add(final_missing_count);
                if novorudp.enabled {
                    current_window_repair_sequence_sent_count =
                        current_window_repair_sequence_sent_count
                            .saturating_add(final_missing_count);
                    current_window_repair_missing_sequence_sent_count =
                        current_window_repair_missing_sequence_sent_count
                            .saturating_add(final_missing_count);
                    tail_window_missing_before = Some(missing_count_before);
                }
                repair_sequence_sent_ranges.extend(selected_ranges.iter().copied());
                for range in &selected_ranges {
                    repair_sequence_sent_min = Some(
                        repair_sequence_sent_min
                            .map(|current| current.min(range.start))
                            .unwrap_or(range.start),
                    );
                    repair_sequence_sent_max = Some(
                        repair_sequence_sent_max
                            .map(|current| current.max(range.end_inclusive))
                            .unwrap_or(range.end_inclusive),
                    );
                }
                build_tail_repair_payloads_from_ranges(
                    chain_id,
                    selected_ranges.as_slice(),
                    repair_round,
                )?
            } else if tail_repair.require_ack {
                final_missing_count = tx_count.saturating_sub(stats.sent_unique);
                Vec::new()
            } else {
                tail_repair_fallback_used_count = tail_repair_fallback_used_count.saturating_add(1);
                let start = if tail_repair.fallback_tail_window == 0
                    || tail_repair.fallback_tail_window >= tx_count
                {
                    0
                } else {
                    tx_count.saturating_sub(tail_repair.fallback_tail_window)
                };
                let fallback_range = MissingRangeV1 {
                    start,
                    end_inclusive: tx_count.saturating_sub(1),
                };
                let fallback_count = missing_ranges_count(&[fallback_range]);
                repair_sequence_sent_count =
                    repair_sequence_sent_count.saturating_add(fallback_count);
                repair_sequence_sent_ranges.push(fallback_range);
                repair_sequence_sent_min = Some(
                    repair_sequence_sent_min
                        .map(|current| current.min(fallback_range.start))
                        .unwrap_or(fallback_range.start),
                );
                repair_sequence_sent_max = Some(
                    repair_sequence_sent_max
                        .map(|current| current.max(fallback_range.end_inclusive))
                        .unwrap_or(fallback_range.end_inclusive),
                );
                build_tail_repair_fallback_payloads(
                    chain_id,
                    tx_count,
                    repair_round,
                    tail_repair.fallback_tail_window,
                )?
            };
            let mut sequence_sent_ranges_this_round = if let Some(ranges) = udp_ranges.as_ref() {
                if novorudp.enabled {
                    if !novorudp_selected_ranges_this_round.is_empty() {
                        novorudp_selected_ranges_this_round.clone()
                    } else {
                        novorudp_window_range_this_round
                            .map(|window| vec![window])
                            .unwrap_or_default()
                    }
                } else {
                    ranges.clone()
                }
            } else if let Some(ranges) =
                file_ack_ranges.as_ref().filter(|ranges| !ranges.is_empty())
            {
                if novorudp.enabled {
                    if !novorudp_selected_ranges_this_round.is_empty() {
                        novorudp_selected_ranges_this_round.clone()
                    } else {
                        novorudp_window_range_this_round
                            .map(|window| vec![window])
                            .unwrap_or_default()
                    }
                } else {
                    ranges.clone()
                }
            } else if !txs.is_empty() {
                let min = txs.iter().map(|tx| tx.index).min().unwrap_or(0);
                let max = txs.iter().map(|tx| tx.index).max().unwrap_or(0);
                vec![MissingRangeV1 {
                    start: min,
                    end_inclusive: max,
                }]
            } else {
                Vec::new()
            };
            let raw_tail_gap_this_round = tail_gap_range_from_ack(
                tx_count,
                latest_ack_missing_count,
                latest_ack_highest_sequence_seen,
            );
            let tail_gap_this_round = if novorudp.enabled {
                raw_tail_gap_this_round.filter(|gap| {
                    novorudp_should_send_tail_gap(
                        *gap,
                        novorudp.window_size,
                        novorudp.tail_window_max_retries,
                    )
                })
            } else {
                raw_tail_gap_this_round
            };
            if novorudp.enabled
                && raw_tail_gap_this_round.is_some()
                && tail_gap_this_round.is_none()
            {
                stale_ack_repair_aborted_count = stale_ack_repair_aborted_count.saturating_add(1);
            }
            let mut tail_gap_sent_count_this_round = 0u64;
            if let Some(gap) = tail_gap_this_round {
                let gap_count = missing_ranges_count(&[gap]);
                let existing_overlap = missing_ranges_overlap_count(
                    &[gap],
                    sequence_sent_ranges_this_round.as_slice(),
                );
                if existing_overlap < gap_count {
                    tail_gap_sent_count_this_round = gap_count;
                    tail_gap_detected = true;
                    tail_gap_range = Some(gap);
                    moving_window_last_range = Some(gap);
                    moving_window_last_ack_epoch = Some(tail_repair_latest_ack_epoch);
                    tail_gap_repair_rounds = tail_gap_repair_rounds.saturating_add(1);
                    tail_gap_repair_sent_count =
                        tail_gap_repair_sent_count.saturating_add(gap_count);
                    sequence_sent_ranges_this_round.push(gap);
                    let mut tail_txs =
                        build_tail_repair_payloads_from_ranges(chain_id, &[gap], repair_round)?;
                    txs.append(&mut tail_txs);
                    repair_sequence_sent_count =
                        repair_sequence_sent_count.saturating_add(gap_count);
                    repair_sequence_sent_ranges.push(gap);
                    repair_sequence_sent_min = Some(
                        repair_sequence_sent_min
                            .map(|current| current.min(gap.start))
                            .unwrap_or(gap.start),
                    );
                    repair_sequence_sent_max = Some(
                        repair_sequence_sent_max
                            .map(|current| current.max(gap.end_inclusive))
                            .unwrap_or(gap.end_inclusive),
                    );
                }
            }
            sequence_sent_ranges_this_round =
                normalize_missing_ranges(sequence_sent_ranges_this_round.as_slice(), tx_count);
            let sequence_sent_count_this_round = txs.len().try_into().unwrap_or(u64::MAX);
            let (sequence_sent_min_this_round, sequence_sent_max_this_round) = if !txs.is_empty() {
                let mut min = None::<u64>;
                let mut max = None::<u64>;
                for tx in &txs {
                    min = Some(min.map(|current| current.min(tx.index)).unwrap_or(tx.index));
                    max = Some(max.map(|current| current.max(tx.index)).unwrap_or(tx.index));
                }
                (min, max)
            } else {
                (None, None)
            };
            if txs.is_empty() {
                repair_rounds_used = repair_rounds_used.saturating_add(1);
                if repair_rounds_detail_sample.len() < tail_repair.missing_sample_limit as usize {
                    repair_rounds_detail_sample.push(serde_json::json!({
                        "round_index": repair_round,
                        "ack_epoch_before": ack_epoch_before,
                        "missing_count_before": missing_count_before,
                        "missing_ranges_before_sample": missing_ranges_to_json(sequence_sent_ranges_this_round.as_slice(), tail_repair.missing_sample_limit),
                        "sequence_sent_count": 0,
                        "packet_sent_count": 0,
                        "sequence_sent_min": Value::Null,
                        "sequence_sent_max": Value::Null,
                        "sequence_sent_ranges_sample": [],
                        "tail_gap_detected": tail_gap_this_round.is_some(),
                        "tail_gap_range": missing_ranges_to_json(tail_gap_this_round.as_slice(), 1),
                        "ack_epoch_after": tail_repair_latest_ack_epoch,
                        "missing_count_after": latest_ack_missing_count,
                        "missing_delta": 0,
                        "no_progress": true,
                    }));
                }
                if sender_exit_on_repair_timeout
                    && sender_hard_timeout_ms > 0
                    && sender_started.elapsed() >= Duration::from_millis(sender_hard_timeout_ms)
                {
                    sender_hard_timeout_reached = true;
                    break;
                }
                continue;
            }
            let round_stats = send_repair_payloads_paced(
                chain_id,
                sender_node,
                receiver_node,
                sender_addr,
                receiver_addr,
                txs.as_slice(),
                repair_round,
                if novorudp.enabled {
                    repair_send_config
                } else {
                    tail_repair
                },
                tail_gap_this_round.map(|_| {
                    if novorudp.enabled {
                        novorudp.tail_window_packet_copies
                    } else {
                        tail_repair.tail_packet_copies
                    }
                }),
                tail_gap_this_round.map(|_| {
                    if novorudp.enabled {
                        novorudp.tail_window_batch_size
                    } else {
                        tail_repair.batch_size
                    }
                }),
                tail_gap_this_round.map(|_| {
                    if novorudp.enabled {
                        novorudp.tail_window_batch_pause_ms
                    } else {
                        tail_repair.tail_batch_pause_ms
                    }
                }),
                udp_send_retry,
            )?;
            let round_packet_sent_count = round_stats.sent_packets;
            if tail_gap_sent_count_this_round > 0 {
                tail_gap_repair_packet_count = tail_gap_repair_packet_count.saturating_add(
                    tail_gap_sent_count_this_round.saturating_mul(if novorudp.enabled {
                        novorudp.tail_window_packet_copies
                    } else {
                        tail_repair.tail_packet_copies
                    }),
                );
            }
            merge_send_stats(&mut repair_stats, round_stats);
            repair_rounds_used = repair_rounds_used.saturating_add(1);
            if repair_stats.send_failed_count > 0 {
                break;
            }
            if !novorudp.enabled
                && sender_hard_timeout_ms > 0
                && sender_started.elapsed() >= Duration::from_millis(sender_hard_timeout_ms)
            {
                sender_hard_timeout_reached = true;
                sender_exit_reason = "sender_finalization_timeout".to_string();
                break;
            }
            if tail_repair.round_pause_ms > 0 {
                std::thread::sleep(Duration::from_millis(tail_repair.round_pause_ms));
            }
            if !novorudp.enabled
                && sender_hard_timeout_ms > 0
                && sender_started.elapsed() >= Duration::from_millis(sender_hard_timeout_ms)
            {
                sender_hard_timeout_reached = true;
                sender_exit_reason = "sender_finalization_timeout".to_string();
                break;
            }
            let ack_wait_ms = if novorudp.enabled {
                if tail_gap_this_round.is_some() {
                    novorudp.tail_window_ack_wait_ms
                } else {
                    novorudp.window_ack_wait_ms
                }
            } else {
                udp_ack.recv_timeout_ms
            };
            let ack_after = ack_socket.as_ref().map(|socket| {
                drain_udp_ack_socket(socket, tail_repair.missing_sample_limit, ack_wait_ms)
            });
            let mut missing_count_after = latest_ack_missing_count.unwrap_or(final_missing_count);
            let mut ack_epoch_after = tail_repair_latest_ack_epoch;
            if let Some(state) = ack_after.as_ref() {
                if state.received_count > 0 {
                    tail_repair_ack_received_count =
                        tail_repair_ack_received_count.saturating_add(state.received_count);
                    tail_repair_udp_ack_received_count =
                        tail_repair_udp_ack_received_count.saturating_add(state.received_count);
                    let previous_epoch = tail_repair_latest_ack_epoch;
                    let previous_highest = latest_ack_highest_sequence_seen;
                    tail_repair_latest_ack_epoch =
                        tail_repair_latest_ack_epoch.max(state.latest_epoch);
                    latest_ack_received_at = Some(Instant::now());
                    latest_ack_missing_count = Some(state.latest_missing_count);
                    latest_ack_missing_ranges_sample = state.latest_ranges.clone();
                    latest_ack_missing_ranges_full_count = Some(state.missing_ranges_full_count);
                    latest_ack_highest_sequence_seen = state.highest_sequence_seen;
                    latest_ack_receiver_done = state.receiver_done;
                    ack_epoch_at_repair_end = Some(state.latest_epoch);
                    ack_highest_sequence_seen_at_repair_end = state.highest_sequence_seen;
                    if novorudp.enabled
                        && state.latest_epoch > previous_epoch
                        && state.highest_sequence_seen.unwrap_or_default()
                            > previous_highest.unwrap_or_default()
                    {
                        repair_window_recomputed_count =
                            repair_window_recomputed_count.saturating_add(1);
                        repair_window_recomputed_due_to_ack_progress =
                            repair_window_recomputed_due_to_ack_progress.saturating_add(1);
                    }
                    final_missing_count = state.latest_missing_count;
                    missing_count_after = state.latest_missing_count;
                    ack_epoch_after = state.latest_epoch;
                    if tail_gap_this_round.is_some() {
                        tail_gap_ack_after_missing_count = Some(state.latest_missing_count);
                    }
                    if novorudp.enabled {
                        let remaining_in_sent_ranges = missing_ranges_overlap_count(
                            state.latest_ranges.as_slice(),
                            sequence_sent_ranges_this_round.as_slice(),
                        );
                        current_window_ack_missing_count_after = Some(remaining_in_sent_ranges);
                        tail_window_remaining_missing_count = Some(remaining_in_sent_ranges);
                        tail_window_remaining_missing_ranges_sample =
                            missing_ranges_intersection_many(
                                state.latest_ranges.as_slice(),
                                sequence_sent_ranges_this_round.as_slice(),
                            );
                        if remaining_in_sent_ranges == 0 && state.latest_missing_count == 0 {
                            tail_window_success_by_bitmap = true;
                        }
                    }
                }
            }
            let missing_delta = missing_count_before.saturating_sub(missing_count_after);
            let ack_progressed_this_round = ack_epoch_after > ack_epoch_before.unwrap_or_default();
            let highest_progressed_this_round = ack_highest_sequence_seen_at_repair_end
                .unwrap_or_default()
                > ack_highest_sequence_seen_at_repair_start.unwrap_or_default();
            let repair_progress_this_round =
                missing_delta > 0 || ack_progressed_this_round || highest_progressed_this_round;
            if novorudp.enabled {
                if repair_progress_this_round {
                    repair_progress_observed_count =
                        repair_progress_observed_count.saturating_add(1);
                    repair_progress_last_observed_ms = sender_started.elapsed().as_millis() as u64;
                    if extend_repair_deadline_on_ack_progress {
                        repair_continuation_deadline_extended_count =
                            repair_continuation_deadline_extended_count.saturating_add(1);
                    }
                    no_progress_started_at = None;
                    no_progress_elapsed_ms = 0;
                } else {
                    let started = no_progress_started_at.get_or_insert_with(Instant::now);
                    no_progress_elapsed_ms = started.elapsed().as_millis() as u64;
                }
            }
            if novorudp.enabled {
                tail_window_missing_after = Some(missing_count_after);
                tail_window_missing_delta = Some(missing_delta);
                current_window_repair_missing_sequence_covered_count =
                    current_window_repair_missing_sequence_covered_count
                        .saturating_add(missing_delta.min(sequence_sent_count_this_round));
            }
            if ack_after
                .as_ref()
                .map(|state| state.received_count == 0)
                .unwrap_or(true)
            {
                latest_ack_stale_rounds = latest_ack_stale_rounds.saturating_add(1);
                latest_ack_stale_duration_ms = latest_ack_received_at
                    .map(|received_at| received_at.elapsed().as_millis() as u64)
                    .unwrap_or_default();
            }
            let no_progress = missing_count_after >= missing_count_before;
            if no_progress {
                repair_no_progress_rounds = repair_no_progress_rounds.saturating_add(1);
            }
            if let Some(window_id) = novorudp_window_id_this_round {
                novorudp_window_round_count = novorudp_window_round_count.saturating_add(1);
                let retry_count = if no_progress {
                    let entry = novorudp_window_retry_counts.entry(window_id).or_insert(0);
                    *entry = entry.saturating_add(1);
                    novorudp_window_no_progress_count =
                        novorudp_window_no_progress_count.saturating_add(1);
                    *entry
                } else {
                    novorudp_window_retry_counts.remove(&window_id);
                    novorudp_window_success_count = novorudp_window_success_count.saturating_add(1);
                    0
                };
                if novorudp_windows_detail_sample.len() < tail_repair.missing_sample_limit as usize
                {
                    novorudp_windows_detail_sample.push(serde_json::json!({
                        "round_index": repair_round,
                        "window_id": window_id,
                        "window_range": missing_ranges_to_json(novorudp_window_range_this_round.as_slice(), 1),
                        "missing_count_before": missing_count_before,
                        "missing_count_after": missing_count_after,
                        "missing_delta": missing_delta,
                        "sequence_sent_count": sequence_sent_count_this_round,
                        "packet_sent_count": round_packet_sent_count,
                        "retry_count_for_window": retry_count,
                        "no_progress": no_progress,
                        "current_missing_bitmap_used": novorudp_used_full_missing_bitmap_this_round,
                        "repair_used_full_missing_bitmap": novorudp_used_full_missing_bitmap_this_round,
                        "current_window_ack_missing_count_after": current_window_ack_missing_count_after,
                        "ack_epoch_before": ack_epoch_before,
                        "ack_epoch_after": ack_epoch_after,
                    }));
                }
                if no_progress
                    && retry_count
                        >= if tail_gap_this_round.is_some() {
                            novorudp.tail_window_max_retries
                        } else {
                            novorudp.max_window_retries
                        }
                    && novorudp.no_progress_backoff
                {
                    novorudp_window_failed_count = novorudp_window_failed_count.saturating_add(1);
                    break;
                }
            }
            if repair_rounds_detail_sample.len() < tail_repair.missing_sample_limit as usize {
                repair_rounds_detail_sample.push(serde_json::json!({
                    "round_index": repair_round,
                    "ack_epoch_before": ack_epoch_before,
                    "missing_count_before": missing_count_before,
                    "missing_ranges_before_sample": missing_ranges_to_json(sequence_sent_ranges_this_round.as_slice(), tail_repair.missing_sample_limit),
                    "sequence_sent_count": sequence_sent_count_this_round,
                    "packet_sent_count": round_packet_sent_count,
                    "sequence_sent_min": sequence_sent_min_this_round,
                    "sequence_sent_max": sequence_sent_max_this_round,
                    "sequence_sent_ranges_sample": missing_ranges_to_json(sequence_sent_ranges_this_round.as_slice(), tail_repair.missing_sample_limit),
                    "tail_gap_detected": tail_gap_this_round.is_some(),
                    "tail_gap_range": missing_ranges_to_json(tail_gap_this_round.as_slice(), 1),
                    "tail_packet_copies_used": tail_gap_this_round.map(|_| if novorudp.enabled { novorudp.tail_window_packet_copies } else { tail_repair.tail_packet_copies }).unwrap_or(tail_repair.packet_copies),
                    "tail_batch_size_used": tail_gap_this_round.map(|_| if novorudp.enabled { novorudp.tail_window_batch_size } else { tail_repair.batch_size }).unwrap_or(tail_repair.batch_size),
                    "tail_batch_pause_ms_used": tail_gap_this_round.map(|_| if novorudp.enabled { novorudp.tail_window_batch_pause_ms } else { tail_repair.tail_batch_pause_ms }).unwrap_or(tail_repair.batch_pause_ms),
                    "current_missing_bitmap_used": novorudp_used_full_missing_bitmap_this_round,
                    "repair_used_full_missing_bitmap": novorudp_used_full_missing_bitmap_this_round,
                    "current_window_ack_missing_count_after": current_window_ack_missing_count_after,
                    "ack_epoch_after": ack_epoch_after,
                    "missing_count_after": missing_count_after,
                    "missing_delta": missing_delta,
                    "no_progress": no_progress,
                }));
            }
            if latest_ack_receiver_done && latest_ack_missing_count == Some(0) {
                break;
            }
        }
        merge_send_stats(&mut stats, repair_stats.clone());
    }
    let sender_completed = stats.sent_unique == tx_count && stats.send_failed_count == 0;
    if tail_repair.enabled
        && sender_completed
        && !sender_hard_timeout_reached
        && !(latest_ack_receiver_done && latest_ack_missing_count == Some(0))
        && final_ack_wait_ms > 0
    {
        let started = Instant::now();
        loop {
            if started.elapsed() >= Duration::from_millis(final_ack_wait_ms) {
                final_ack_grace_timeout = true;
                break;
            }
            std::thread::sleep(Duration::from_millis(final_ack_poll_ms));
            let Some(socket) = ack_socket.as_ref() else {
                final_ack_grace_timeout = true;
                break;
            };
            let state = drain_udp_ack_socket(
                socket,
                tail_repair.missing_sample_limit,
                udp_ack.recv_timeout_ms,
            );
            final_ack_wait_elapsed_ms = started.elapsed().as_millis() as u64;
            if state.received_count == 0 {
                continue;
            }
            tail_repair_ack_received_count =
                tail_repair_ack_received_count.saturating_add(state.received_count);
            tail_repair_udp_ack_received_count =
                tail_repair_udp_ack_received_count.saturating_add(state.received_count);
            tail_repair_latest_ack_epoch = tail_repair_latest_ack_epoch.max(state.latest_epoch);
            latest_ack_received_at = Some(Instant::now());
            latest_ack_missing_count = Some(state.latest_missing_count);
            latest_ack_missing_ranges_sample = state.latest_ranges.clone();
            latest_ack_missing_ranges_full_count = Some(state.missing_ranges_full_count);
            latest_ack_highest_sequence_seen = state.highest_sequence_seen;
            latest_ack_receiver_done = state.receiver_done;
            final_missing_count = state.latest_missing_count;
            if state.receiver_done && state.latest_missing_count == 0 {
                final_ack_received_after_repair = true;
                final_ack_epoch = Some(state.latest_epoch);
                final_ack_missing_count = Some(0);
                final_ack_receiver_done = Some(true);
                break;
            }
        }
        if final_ack_wait_elapsed_ms == 0 {
            final_ack_wait_elapsed_ms = started.elapsed().as_millis() as u64;
        }
    }
    if tail_repair_ack_received_count == 0 {
        final_missing_count = tx_count.saturating_sub(stats.sent_unique);
    }
    let receiver_final_done = if latest_ack_receiver_done && latest_ack_missing_count == Some(0) {
        Some(true)
    } else if latest_ack_missing_count.is_some() {
        Some(false)
    } else {
        None
    };
    let receiver_final_missing_count = if receiver_final_done == Some(true) {
        Some(0u64)
    } else if latest_ack_missing_count.is_some() {
        latest_ack_missing_count
    } else {
        None
    };
    let final_missing_count_source = if receiver_final_missing_count.is_some() {
        if receiver_final_done == Some(true) {
            "receiver_done_ack"
        } else {
            "latest_ack_snapshot"
        }
    } else if latest_ack_missing_count.is_some() {
        "latest_ack_snapshot"
    } else if tail_repair_ack_received_count == 0 {
        "no_ack"
    } else {
        "unknown"
    };
    let tail_repair_success = if tail_repair.enabled {
        receiver_final_done == Some(true) && receiver_final_missing_count == Some(0)
    } else {
        true
    };
    let repair_budget_exhausted =
        tail_repair.enabled && repair_rounds_used >= tail_repair.rounds && !tail_repair_success;
    let repair_waited_for_receiver_done = tail_repair.enabled;
    if receiver_final_missing_count.is_some() {
        final_missing_count = receiver_final_missing_count.unwrap_or(final_missing_count);
    }
    let tail_repair_completion_reason = if !tail_repair.enabled {
        "tail_repair_disabled"
    } else if tail_repair_success {
        if final_ack_received_after_repair {
            "receiver_done_ack_after_grace"
        } else {
            "receiver_done_ack"
        }
    } else if sender_hard_timeout_reached {
        if sender_repair_no_progress_timeout_reached {
            "sender_repair_no_progress_timeout"
        } else if absolute_sender_timeout_reached {
            "sender_absolute_timeout"
        } else if tail_repair_ack_received_count == 0 {
            "sender_hard_timeout_no_ack"
        } else if latest_ack_stale_rounds > 0
            && latest_ack_missing_count.unwrap_or_default() > 0
            && !latest_ack_receiver_done
        {
            "sender_finalization_timeout_stale_ack"
        } else {
            "sender_hard_timeout_latest_ack_missing"
        }
    } else if tail_repair_ack_received_count == 0 {
        "no_ack"
    } else if final_ack_grace_timeout {
        "final_ack_grace_timeout_latest_ack_missing"
    } else if repair_budget_exhausted {
        "repair_budget_exhausted_latest_ack_missing"
    } else {
        "latest_ack_missing"
    };
    let sender_repair_coverage = novorudp_sender_repair_coverage_report_v1(
        latest_ack_missing_ranges_sample.as_slice(),
        repair_sequence_sent_ranges.as_slice(),
        tx_count,
        repair_sequence_sent_count,
        tail_repair.missing_sample_limit,
    );
    let accepted = sender_completed && tail_repair_success;
    let fail_reason = if accepted {
        None
    } else if sender_repair_no_progress_timeout_reached {
        Some("sender_repair_no_progress_timeout")
    } else if absolute_sender_timeout_reached {
        Some("sender_absolute_timeout")
    } else if sender_hard_timeout_reached {
        if latest_ack_stale_rounds > 0
            && latest_ack_missing_count.unwrap_or_default() > 0
            && !latest_ack_receiver_done
        {
            Some("sender_finalization_timeout_stale_ack")
        } else {
            Some("sender_finalization_timeout")
        }
    } else if !sender_completed {
        Some("sender_send_incomplete")
    } else if tail_repair_ack_received_count == 0 {
        Some("receiver_repair_no_ack")
    } else {
        Some("receiver_repair_incomplete")
    };
    if accepted && sender_exit_reason == "not_finished" {
        sender_exit_reason = "accepted".to_string();
    } else if !accepted && sender_exit_reason == "not_finished" {
        sender_exit_reason = fail_reason
            .unwrap_or("receiver_repair_incomplete")
            .to_string();
    }
    let mut report = serde_json::json!({
        "schema": REPORT_SCHEMA_V1,
        "role": "sender",
        "accepted": accepted,
        "fail_reason": fail_reason,
        "report_written": sender_report_on_timeout || !sender_hard_timeout_reached,
        "elapsed_ms": sender_started.elapsed().as_millis() as u64,
        "novorudp_profile": repair_budget_profile.profile,
        "primary_send_duration_seconds": repair_budget_profile.primary_send_duration_seconds,
        "repair_continuation_timeout_seconds": repair_continuation_timeout_ms / 1_000,
        "repair_no_progress_timeout_seconds": repair_no_progress_timeout_ms / 1_000,
        "absolute_max_timeout_seconds": absolute_sender_max_timeout_ms / 1_000,
        "repair_deadline_extended_count": repair_continuation_deadline_extended_count,
        "repair_last_progress_observed_at_ms": repair_progress_last_observed_ms,
        "repair_progress_still_active_at_absolute_timeout": absolute_sender_timeout_reached
            && repair_progress_observed_count > 0
            && !sender_repair_no_progress_timeout_reached,
        "repair_exit_reason": sender_exit_reason,
        "repair_continuation_budget_exhausted": absolute_sender_timeout_reached
            && !sender_repair_no_progress_timeout_reached,
        "repair_no_progress_budget_exhausted": sender_repair_no_progress_timeout_reached,
        "hard_timeout_ms": sender_hard_timeout_ms,
        "sender_hard_timeout_reached": sender_hard_timeout_reached,
        "absolute_sender_max_timeout_ms": absolute_sender_max_timeout_ms,
        "absolute_sender_timeout_reached": absolute_sender_timeout_reached,
        "no_progress_timeout_ms": repair_no_progress_timeout_ms,
        "no_progress_elapsed_ms": no_progress_elapsed_ms,
        "sender_repair_no_progress_timeout_reached": sender_repair_no_progress_timeout_reached,
        "repair_progress_observed_count": repair_progress_observed_count,
        "repair_progress_last_observed_ms": repair_progress_last_observed_ms,
        "repair_continuation_deadline_extended_count": repair_continuation_deadline_extended_count,
        "repair_continuation_deadline_ms": repair_progress_last_observed_ms
            .saturating_add(repair_no_progress_timeout_ms),
        "repair_continuation_timeout_ms": repair_continuation_timeout_ms,
        "extend_repair_deadline_on_ack_progress": extend_repair_deadline_on_ack_progress,
        "primary_send_ack_drain_enabled": primary_ack_drain_enabled,
        "primary_send_ack_drain_interval_ms": primary_ack_drain_interval_ms,
        "primary_send_ack_drain_count": primary_ack_drain_count,
        "primary_send_ack_received_count": primary_ack_received_count,
        "primary_send_ack_drain_empty_count": primary_ack_drain_empty_count,
        "primary_send_ack_last_consumed_elapsed_ms": primary_ack_last_consumed_elapsed_ms,
        "sender_live_report_path": sender_progress_path.to_string_lossy(),
        "sender_live_report_interval_ms": sender_live_report_interval_ms,
        "sender_live_report_write_count": sender_live_report_write_count,
        "sender_live_report_last_elapsed_ms": sender_live_report_last_elapsed_ms,
        "repair_still_progressing_at_hard_timeout": repair_still_progressing_at_hard_timeout,
        "sender_exit_reason": sender_exit_reason,
        "sender_report_on_timeout": sender_report_on_timeout,
        "sender_exit_on_repair_timeout": sender_exit_on_repair_timeout,
        "chain_id": chain_id,
        "tx_count": tx_count,
        "tx_target_total": tx_count,
        "sender_node": sender_node,
        "receiver_node": receiver_node,
        "sender_addr": sender_addr,
        "receiver_addr": receiver_addr,
        "transport_profile": "novorudp",
        "novorudp": {
            "enabled": novorudp.enabled,
            "window_size": novorudp.window_size,
            "packet_copies": novorudp.packet_copies,
            "tail_packet_copies": novorudp.tail_packet_copies,
            "batch_size": novorudp.batch_size,
            "batch_pause_ms": novorudp.batch_pause_ms,
            "window_ack_wait_ms": novorudp.window_ack_wait_ms,
            "max_window_retries": novorudp.max_window_retries,
            "tail_window_max_retries": novorudp.tail_window_max_retries,
            "tail_window_packet_copies": novorudp.tail_window_packet_copies,
            "tail_window_batch_size": novorudp.tail_window_batch_size,
            "tail_window_batch_pause_ms": novorudp.tail_window_batch_pause_ms,
            "tail_window_ack_wait_ms": novorudp.tail_window_ack_wait_ms,
            "ack_progress_interval_ms": novorudp.ack_progress_interval_ms,
            "no_progress_backoff": novorudp.no_progress_backoff,
            "window_round_count": novorudp_window_round_count,
            "window_success_count": novorudp_window_success_count,
            "window_failed_count": novorudp_window_failed_count,
            "window_no_progress_count": novorudp_window_no_progress_count,
            "current_missing_bitmap_used": current_missing_bitmap_used,
            "repair_used_full_missing_bitmap": repair_used_full_missing_bitmap,
            "tail_window_missing_before": tail_window_missing_before,
            "tail_window_missing_after": tail_window_missing_after,
            "tail_window_missing_delta": tail_window_missing_delta,
            "tail_window_remaining_missing_count": tail_window_remaining_missing_count,
            "tail_window_remaining_missing_ranges_sample": missing_ranges_to_json(
                tail_window_remaining_missing_ranges_sample.as_slice(),
                tail_repair.missing_sample_limit,
            ),
            "tail_window_success_by_bitmap": tail_window_success_by_bitmap,
            "tail_window_success_by_max_sequence_only": tail_window_success_by_max_sequence_only,
            "current_window_repair_sequence_sent_count": current_window_repair_sequence_sent_count,
            "current_window_repair_missing_sequence_sent_count": current_window_repair_missing_sequence_sent_count,
            "current_window_repair_missing_sequence_covered_count": current_window_repair_missing_sequence_covered_count,
            "current_window_ack_missing_count_after": current_window_ack_missing_count_after,
            "latest_ack_age_ms": latest_ack_received_at.map(|received_at| received_at.elapsed().as_millis() as u64),
            "latest_ack_stale_rounds": latest_ack_stale_rounds,
            "latest_ack_stale_duration_ms": latest_ack_stale_duration_ms,
            "ack_epoch_at_repair_start": ack_epoch_at_repair_start,
            "ack_epoch_at_repair_end": ack_epoch_at_repair_end,
            "ack_highest_sequence_seen_at_repair_start": ack_highest_sequence_seen_at_repair_start,
            "ack_highest_sequence_seen_at_repair_end": ack_highest_sequence_seen_at_repair_end,
            "repair_window_recomputed_count": repair_window_recomputed_count,
            "repair_window_recomputed_due_to_ack_progress": repair_window_recomputed_due_to_ack_progress,
            "stale_ack_repair_aborted_count": stale_ack_repair_aborted_count,
            "moving_window_enabled": moving_window_enabled,
            "moving_window_last_range": missing_ranges_to_json(moving_window_last_range.as_slice(), 1),
            "moving_window_last_ack_epoch": moving_window_last_ack_epoch,
            "windows_detail_sample": novorudp_windows_detail_sample,
        },
        "sender_completed": sender_completed,
        "sent_packets": stats.sent_packets,
        "send_retry_count": stats.send_retry_count,
        "send_would_block_count": stats.send_would_block_count,
        "send_failed_count": stats.send_failed_count,
        "send_failure_first_index": stats.send_failure_first_index,
        "send_failure_first_copy_index": stats.send_failure_first_copy_index,
        "send_failure_first_error": stats.send_failure_first_error,
        "send_failure_type": if stats.send_failed_count > 0 { Some("udp_send_retry_exhausted") } else { None },
        "udp_send_retry": {
            "max_retries": udp_send_retry.max_retries,
            "backoff_ms": udp_send_retry.backoff_ms,
            "backoff_max_ms": udp_send_retry.backoff_max_ms,
        },
        "clean_network": {
            "packet_loss": fault.loss_bps,
            "duplicate": fault.duplicate_bps,
            "delay_ms": fault.delay_ms,
            "reorder": fault.reorder_bps,
            "sent_count": stats.sent_packets,
            "sent_unique": stats.sent_unique,
        },
        "fault_injection": {
            "enabled": fault.enabled,
            "packet_loss_bps": fault.loss_bps,
            "duplicate_bps": fault.duplicate_bps,
            "delay_ms": fault.delay_ms,
            "reorder_bps": fault.reorder_bps,
            "seed": fault.seed,
            "scheduled_packets": stats.scheduled_packets,
            "sent_packets": stats.sent_packets,
            "dropped_packets": stats.dropped_packets,
            "duplicated_packets": stats.duplicated_packets,
            "delayed_packets": stats.delayed_packets,
            "reordered_packets": stats.reordered_packets,
            "sent_unique": stats.sent_unique,
            "send_retry_count": stats.send_retry_count,
            "send_would_block_count": stats.send_would_block_count,
            "send_failed_count": stats.send_failed_count,
        },
        "sustained": {
            "enabled": sustained.enabled,
            "duration_seconds": sustained.duration_seconds,
            "rounds": rounds,
            "tx_per_round": tx_per_round,
            "round_interval_ms": sustained.round_interval_ms,
            "tx_submitted_total": stats.sent_unique,
            "tx_target_total": tx_count,
            "sender_completed": sender_completed,
        },
        "tail_repair": {
            "enabled": tail_repair.enabled,
            "rounds_configured": tail_repair.rounds,
            "interval_ms": tail_repair.interval_ms,
            "repair_pacing_enabled": true,
            "repair_packet_copies": tail_repair.packet_copies,
            "repair_tail_packet_copies": tail_repair.tail_packet_copies,
            "repair_batch_size": tail_repair.batch_size,
            "repair_batch_pause_ms": tail_repair.batch_pause_ms,
            "repair_tail_batch_pause_ms": tail_repair.tail_batch_pause_ms,
            "repair_round_pause_ms": tail_repair.round_pause_ms,
            "repair_round_count": repair_rounds_used,
            "repair_rounds_detail_sample": repair_rounds_detail_sample,
            "repair_no_progress_rounds": repair_no_progress_rounds,
            "repair_total_packet_copies_sent": repair_stats.sent_packets,
            "repair_rounds_used": repair_rounds_used,
            "tail_repair_ack_received_count": tail_repair_ack_received_count,
            "tail_repair_udp_ack_received_count": tail_repair_udp_ack_received_count,
            "tail_repair_latest_ack_epoch": tail_repair_latest_ack_epoch,
            "latest_ack_epoch": tail_repair_latest_ack_epoch,
            "latest_ack_missing_count": latest_ack_missing_count,
            "latest_ack_missing_ranges_full_count": latest_ack_missing_ranges_full_count,
            "latest_ack_highest_sequence_seen": latest_ack_highest_sequence_seen,
            "latest_ack_receiver_done": latest_ack_receiver_done,
            "repair_used_full_missing_ranges": repair_used_full_missing_ranges,
            "tail_gap_detected": tail_gap_detected,
            "tail_gap_range": missing_ranges_to_json(tail_gap_range.as_slice(), 1),
            "tail_gap_repair_sent_count": tail_gap_repair_sent_count,
            "tail_gap_repair_packet_count": tail_gap_repair_packet_count,
            "tail_gap_repair_rounds": tail_gap_repair_rounds,
            "tail_gap_ack_after_missing_count": tail_gap_ack_after_missing_count,
            "current_missing_bitmap_used": current_missing_bitmap_used,
            "repair_used_full_missing_bitmap": repair_used_full_missing_bitmap,
            "tail_window_missing_before": tail_window_missing_before,
            "tail_window_missing_after": tail_window_missing_after,
            "tail_window_missing_delta": tail_window_missing_delta,
            "tail_window_remaining_missing_count": tail_window_remaining_missing_count,
            "tail_window_remaining_missing_ranges_sample": missing_ranges_to_json(
                tail_window_remaining_missing_ranges_sample.as_slice(),
                tail_repair.missing_sample_limit,
            ),
            "tail_window_success_by_bitmap": tail_window_success_by_bitmap,
            "tail_window_success_by_max_sequence_only": tail_window_success_by_max_sequence_only,
            "current_window_repair_sequence_sent_count": current_window_repair_sequence_sent_count,
            "current_window_repair_missing_sequence_sent_count": current_window_repair_missing_sequence_sent_count,
            "current_window_repair_missing_sequence_covered_count": current_window_repair_missing_sequence_covered_count,
            "current_window_ack_missing_count_after": current_window_ack_missing_count_after,
            "latest_ack_age_ms": latest_ack_received_at.map(|received_at| received_at.elapsed().as_millis() as u64),
            "latest_ack_stale_rounds": latest_ack_stale_rounds,
            "latest_ack_stale_duration_ms": latest_ack_stale_duration_ms,
            "ack_epoch_at_repair_start": ack_epoch_at_repair_start,
            "ack_epoch_at_repair_end": ack_epoch_at_repair_end,
            "ack_highest_sequence_seen_at_repair_start": ack_highest_sequence_seen_at_repair_start,
            "ack_highest_sequence_seen_at_repair_end": ack_highest_sequence_seen_at_repair_end,
            "repair_window_recomputed_count": repair_window_recomputed_count,
            "repair_window_recomputed_due_to_ack_progress": repair_window_recomputed_due_to_ack_progress,
            "stale_ack_repair_aborted_count": stale_ack_repair_aborted_count,
            "moving_window_enabled": moving_window_enabled,
            "moving_window_last_range": missing_ranges_to_json(moving_window_last_range.as_slice(), 1),
            "moving_window_last_ack_epoch": moving_window_last_ack_epoch,
            "receiver_final_missing_count": receiver_final_missing_count,
            "receiver_final_done": receiver_final_done,
            "final_ack_wait_enabled": final_ack_wait_ms > 0,
            "final_ack_wait_ms": final_ack_wait_ms,
            "final_ack_poll_ms": final_ack_poll_ms,
            "final_ack_wait_elapsed_ms": final_ack_wait_elapsed_ms,
            "final_ack_received_after_repair": final_ack_received_after_repair,
            "final_ack_epoch": final_ack_epoch,
            "final_ack_missing_count": final_ack_missing_count,
            "final_ack_receiver_done": final_ack_receiver_done,
            "final_ack_grace_timeout": final_ack_grace_timeout,
            "tail_repair_completion_reason": tail_repair_completion_reason,
            "repair_budget_exhausted": repair_budget_exhausted,
            "repair_waited_for_receiver_done": repair_waited_for_receiver_done,
            "tail_repair_missing_ranges_seen": tail_repair_missing_ranges_seen,
            "tail_repair_fallback_used_count": tail_repair_fallback_used_count,
            "tail_repair_file_ack_used_count": tail_repair_file_ack_used_count,
            "tail_repair_udp_ack_used_count": tail_repair_udp_ack_used_count,
            "tail_repair_used_udp_ack": tail_repair_udp_ack_used_count > 0,
            "tail_repair_used_file_ack": tail_repair_file_ack_used_count > 0,
            "tail_repair_used_tail_window_fallback": tail_repair_fallback_used_count > 0,
            "tail_repair_require_ack": tail_repair.require_ack,
            "missing_sample_limit": tail_repair.missing_sample_limit,
            "fallback_tail_window": tail_repair.fallback_tail_window,
            "initial_sent_total": stats.sent_packets.saturating_sub(repair_stats.sent_packets),
            "repair_sent_total": repair_stats.sent_packets,
            "repair_scheduled_total": repair_stats.scheduled_packets,
            "repair_send_retry_count": repair_stats.send_retry_count,
            "repair_send_would_block_count": repair_stats.send_would_block_count,
            "repair_send_failed_count": repair_stats.send_failed_count,
            "repair_sequence_sent_count": repair_sequence_sent_count,
            "repair_sequence_sent_ranges": missing_ranges_to_json(
                repair_sequence_sent_ranges.as_slice(),
                tail_repair.missing_sample_limit,
            ),
            "repair_sequence_sent_ranges_sample": missing_ranges_to_json(
                repair_sequence_sent_ranges.as_slice(),
                tail_repair.missing_sample_limit,
            ),
            "repair_sequence_sent_ranges_total": repair_sequence_sent_ranges.len(),
            "repair_sequence_sent_ranges_full_count": repair_sequence_sent_ranges.len(),
            "repair_sequence_sent_min": repair_sequence_sent_min,
            "repair_sequence_sent_max": repair_sequence_sent_max,
            "repair_packet_sent_count": repair_stats.sent_packets,
            "final_missing_count_source": final_missing_count_source,
            "final_missing_count": final_missing_count,
            "tail_repair_success": tail_repair_success,
        },
        "udp_ack": {
            "enabled": udp_ack.enabled,
            "bind_addr": udp_ack.bind_addr,
            "local_addr": ack_socket_addr,
            "target_addr": udp_ack.target_addr,
            "recv_timeout_ms": udp_ack.recv_timeout_ms,
        },
        "sent_by_hash": stats.sent_by_hash,
        "violations": if accepted { Vec::<String>::new() } else { vec![format!(
            "{}: tx_submitted_total={} expected={} send_failed_count={} latest_ack_missing_count={:?} latest_ack_receiver_done={} receiver_final_done={:?} final_missing_count={} first_failure_index={:?} first_failure_copy={:?} first_failure_error={:?}",
            fail_reason.unwrap_or("sender_send_incomplete"),
            stats.sent_unique,
            tx_count,
            stats.send_failed_count,
            latest_ack_missing_count,
            latest_ack_receiver_done,
            receiver_final_done,
            final_missing_count,
            stats.send_failure_first_index,
            stats.send_failure_first_copy_index,
            stats.send_failure_first_error,
        )] },
    });
    if let Some(coverage) = sender_repair_coverage.as_object() {
        if let Some(report_map) = report.as_object_mut() {
            report_map.insert(
                "repair_convergence_attribution".to_string(),
                sender_repair_coverage.clone(),
            );
            for (key, value) in coverage {
                report_map.insert(key.clone(), value.clone());
            }
            if let Some(tail_repair_map) = report_map
                .get_mut("tail_repair")
                .and_then(Value::as_object_mut)
            {
                for (key, value) in coverage {
                    tail_repair_map.insert(key.clone(), value.clone());
                }
            }
        }
    }
    Ok(compact_sender_report_for_report(report))
}

fn run_receiver(
    chain_id: u64,
    tx_count: u64,
    receiver_node: u64,
    listen_addr: &str,
    node_bin: &Path,
    store_path: &Path,
    max_ticks: u64,
    tick_interval_ms: u64,
    batch_budget: u64,
    recv_budget: u64,
    sustained: SustainedConfigV1,
) -> Result<Value> {
    let mut receiver_summary = run_receiver_node(
        node_bin,
        chain_id,
        receiver_node,
        listen_addr,
        store_path,
        tx_count,
        max_ticks,
        tick_interval_ms,
        batch_budget,
        recv_budget,
    )?;
    annotate_receiver_aoem_gate_trace_v1(&mut receiver_summary);
    std::env::set_var("NOVOVM_NATIVE_EXECUTION_STORE_BACKEND", "rocksdb");
    let recovery_probe = get_nov_native_execution_store_recovery_probe_v1(store_path)?;
    let (validation, violations) =
        validate_receiver_report(&receiver_summary, &recovery_probe, tx_count);
    let accepted = violations.is_empty();
    Ok(serde_json::json!({
        "schema": REPORT_SCHEMA_V1,
        "role": "receiver",
        "accepted": accepted,
        "chain_id": chain_id,
        "tx_count": tx_count,
        "receiver_node": receiver_node,
        "listen_addr": listen_addr,
        "store_path": store_path,
        "clean_network": {
            "packet_loss": 0,
            "duplicate": 0,
            "delay": 0,
            "reorder": 0
        },
        "sustained": {
            "enabled": sustained.enabled,
            "duration_seconds": sustained.duration_seconds,
            "tx_per_round": sustained.tx_per_round,
            "round_interval_ms": sustained.round_interval_ms,
            "expected_tx_total": tx_count,
        },
        "boundaries": {
            "lifecycle_structure": "frozen",
            "execution_kernel": "AOEM",
            "aoem_concurrency_owner": "AOEM_runtime",
            "host_concurrency_policy": "host_drives_lifecycle_only_no_rust_execution_scheduler",
            "product_entry": "pending_only",
            "receipt_state_source": "AOEM_tick_lifecycle",
            "commit": "dirty_sharded_atomic_commit",
            "canonical_body_head_recovery": "not_claimed_by_this_gate"
        },
        "validation": validation,
        "receiver_summary": compact_receiver_summary_for_report(receiver_summary),
        "recovery_probe": compact_probe_for_report(recovery_probe),
        "violations": violations
    }))
}

fn run_local_smoke(
    chain_id: u64,
    tx_count: u64,
    sender_node: u64,
    receiver_node: u64,
    node_bin: &Path,
    store_path: &Path,
    max_ticks: u64,
    tick_interval_ms: u64,
    batch_budget: u64,
    recv_budget: u64,
    startup_wait_ms: u64,
    fault: FaultConfigV1,
    sustained: SustainedConfigV1,
    tail_repair: TailRepairConfigV1,
    novorudp: NovoRudpConfigV1,
) -> Result<Value> {
    let sender_addr = reserve_udp_addr()?;
    let receiver_addr = reserve_udp_addr()?;
    let local_ack_addr = if novorudp.enabled {
        Some(reserve_udp_addr()?)
    } else {
        None
    };
    let previous_ack_enabled = std::env::var_os("NOVOVM_NATIVE_PIPELINE_UDP_ACK_ENABLED");
    let previous_ack_target = std::env::var_os("NOVOVM_NATIVE_PIPELINE_ACK_TARGET_ADDR");
    if let Some(ack_addr) = local_ack_addr.as_ref() {
        std::env::set_var("NOVOVM_NATIVE_PIPELINE_UDP_ACK_ENABLED", "1");
        std::env::set_var("NOVOVM_NATIVE_PIPELINE_ACK_TARGET_ADDR", ack_addr);
    }
    let child = spawn_receiver_node(
        node_bin,
        chain_id,
        receiver_node,
        receiver_addr.as_str(),
        store_path,
        tx_count,
        max_ticks,
        tick_interval_ms,
        batch_budget,
        recv_budget,
    )?;
    match previous_ack_enabled {
        Some(value) => std::env::set_var("NOVOVM_NATIVE_PIPELINE_UDP_ACK_ENABLED", value),
        None => std::env::remove_var("NOVOVM_NATIVE_PIPELINE_UDP_ACK_ENABLED"),
    }
    match previous_ack_target {
        Some(value) => std::env::set_var("NOVOVM_NATIVE_PIPELINE_ACK_TARGET_ADDR", value),
        None => std::env::remove_var("NOVOVM_NATIVE_PIPELINE_ACK_TARGET_ADDR"),
    }
    std::thread::sleep(std::time::Duration::from_millis(startup_wait_ms));
    let udp_ack = local_ack_addr
        .as_ref()
        .map(|ack_addr| UdpAckConfigV1 {
            enabled: true,
            bind_addr: ack_addr.clone(),
            target_addr: None,
            recv_timeout_ms: 1000,
        })
        .unwrap_or_else(default_udp_ack_config);
    let sender_report = run_sender(
        chain_id,
        tx_count,
        sender_node,
        receiver_node,
        sender_addr.as_str(),
        receiver_addr.as_str(),
        FaultConfigV1 {
            delay_ms: if fault.enabled { fault.delay_ms } else { 1 },
            ..fault
        },
        sustained,
        tail_repair,
        default_udp_send_retry_config(),
        udp_ack,
        novorudp,
    )?;
    let mut receiver_summary = parse_summary(
        child
            .wait_with_output()
            .context("wait local cross-machine smoke receiver failed")?,
        "local cross-machine smoke receiver",
    )?;
    annotate_receiver_aoem_gate_trace_v1(&mut receiver_summary);
    std::env::set_var("NOVOVM_NATIVE_EXECUTION_STORE_BACKEND", "rocksdb");
    let recovery_probe = get_nov_native_execution_store_recovery_probe_v1(store_path)?;
    let (validation, violations) =
        validate_receiver_report(&receiver_summary, &recovery_probe, tx_count);
    let sender_transport_report_accepted = sender_report
        .get("accepted")
        .and_then(Value::as_bool)
        .unwrap_or(false);
    let accepted = violations.is_empty()
        && receiver_summary
            .get("accepted")
            .and_then(Value::as_bool)
            .unwrap_or(false);
    Ok(serde_json::json!({
        "schema": REPORT_SCHEMA_V1,
        "role": "local-smoke",
        "accepted": accepted,
        "sender_transport_report_accepted": sender_transport_report_accepted,
        "chain_id": chain_id,
        "tx_count": tx_count,
        "sender_addr": sender_addr,
        "receiver_addr": receiver_addr,
        "sender_report": compact_sender_report_for_report(sender_report),
        "validation": validation,
        "receiver_summary": compact_receiver_summary_for_report(receiver_summary),
        "recovery_probe": compact_probe_for_report(recovery_probe),
        "violations": violations
    }))
}

fn clear_memory_probe_toggle_envs() {
    for (_, env_name, _) in MEMORY_PROBE_TOGGLES_V1 {
        std::env::remove_var(env_name);
    }
}

fn read_json_file(path: &Path) -> Option<Value> {
    let raw = fs::read_to_string(path).ok()?;
    serde_json::from_str(raw.as_str()).ok()
}

fn memory_bisect_variant_report(
    name: &str,
    toggle_env: Option<&str>,
    probe_only_not_functional: bool,
    diagnostics_report: Option<&Value>,
    receiver_summary: Option<&Value>,
    error: Option<&str>,
) -> Value {
    let peak_private = diagnostics_report
        .map(|report| sample_u64(report, "peak_live_private_bytes"))
        .unwrap_or_default();
    let peak_native_heap = diagnostics_report
        .map(|report| sample_u64(report, "peak_live_native_heap_unattributed_bytes"))
        .unwrap_or_default();
    serde_json::json!({
        "toggle_name": name,
        "toggle_env": toggle_env,
        "toggle_enabled": toggle_env.is_some(),
        "toggle_applied_to_execution": false,
        "probe_only_not_functional": probe_only_not_functional && toggle_env.is_some(),
        "accepted": receiver_summary
            .and_then(|summary| summary.get("accepted"))
            .and_then(Value::as_bool)
            .unwrap_or(false),
        "aoem_executed_total": receiver_summary
            .map(|summary| summary_u64(summary, "aoem_executed_total"))
            .unwrap_or_default(),
        "queue_pending_last": receiver_summary
            .map(|summary| summary_u64(summary, "queue_pending_last"))
            .unwrap_or_default(),
        "peak_private_bytes": peak_private,
        "peak_native_heap_unattributed_bytes": peak_native_heap,
        "memory_summary_source": diagnostics_report
            .and_then(|report| report.get("memory_summary_source"))
            .and_then(Value::as_str),
        "summary_unknown_native_heap_source": diagnostics_report
            .and_then(|report| report.get("summary_unknown_native_heap_source"))
            .and_then(Value::as_bool),
        "summary_large_allocation_suspected_stage": diagnostics_report
            .and_then(|report| report.get("summary_large_allocation_suspected_stage"))
            .and_then(Value::as_str),
        "error": error,
    })
}

#[allow(clippy::too_many_arguments)]
fn run_memory_bisect_variant(
    variant_name: &str,
    toggle_env: Option<&str>,
    probe_only_not_functional: bool,
    chain_id: u64,
    tx_count: u64,
    sender_node: u64,
    receiver_node: u64,
    node_bin: &Path,
    tick_interval_ms: u64,
    batch_budget: u64,
    recv_budget: u64,
    startup_wait_ms: u64,
) -> Value {
    clear_memory_probe_toggle_envs();
    if let Some(env_name) = toggle_env {
        std::env::set_var(env_name, "1");
    }
    let sender_addr = match reserve_udp_addr() {
        Ok(addr) => addr,
        Err(err) => {
            let error = err.to_string();
            return memory_bisect_variant_report(
                variant_name,
                toggle_env,
                probe_only_not_functional,
                None,
                None,
                Some(error.as_str()),
            );
        }
    };
    let receiver_addr = match reserve_udp_addr() {
        Ok(addr) => addr,
        Err(err) => {
            let error = err.to_string();
            return memory_bisect_variant_report(
                variant_name,
                toggle_env,
                probe_only_not_functional,
                None,
                None,
                Some(error.as_str()),
            );
        }
    };
    let safe_variant = variant_name.replace(['\\', '/', ':', ' '], "_");
    let store = temp_store_path(chain_id, safe_variant.as_str());
    let diagnostics_path = PathBuf::from(format!(
        "artifacts/native-pipeline/memory-bisect-{safe_variant}-diagnostics.json"
    ));
    let stdout_path = PathBuf::from(format!(
        "artifacts/native-pipeline/memory-bisect-{safe_variant}-stdout.log"
    ));
    let stderr_path = PathBuf::from(format!(
        "artifacts/native-pipeline/memory-bisect-{safe_variant}-stderr.log"
    ));
    let exit_path = PathBuf::from(format!(
        "artifacts/native-pipeline/memory-bisect-{safe_variant}-exit.json"
    ));
    std::env::set_var(
        "NOVOVM_NATIVE_PIPELINE_DIAGNOSTICS_REPORT_PATH",
        diagnostics_path.as_os_str(),
    );
    std::env::set_var(
        "NOVOVM_NATIVE_PIPELINE_RECEIVER_STDOUT_LOG_PATH",
        stdout_path.as_os_str(),
    );
    std::env::set_var(
        "NOVOVM_NATIVE_PIPELINE_RECEIVER_STDERR_LOG_PATH",
        stderr_path.as_os_str(),
    );
    std::env::set_var(
        "NOVOVM_NATIVE_PIPELINE_RECEIVER_EXIT_REPORT_PATH",
        exit_path.as_os_str(),
    );
    std::env::set_var("NOVOVM_NATIVE_PIPELINE_PROGRESS_WATCHDOG_ENABLED", "1");
    std::env::set_var("NOVOVM_NATIVE_PIPELINE_PROGRESS_SAMPLE_INTERVAL_MS", "500");
    std::env::set_var("NOVOVM_NATIVE_PIPELINE_MEMORY_SAMPLE_ENABLED", "1");
    let node_bin = node_bin.to_path_buf();
    let store_for_thread = store.clone();
    let receiver_addr_for_thread = receiver_addr.clone();
    let handle = std::thread::spawn(move || {
        run_receiver_node(
            node_bin.as_path(),
            chain_id,
            receiver_node,
            receiver_addr_for_thread.as_str(),
            store_for_thread.as_path(),
            tx_count,
            div_ceil_u64(tx_count, batch_budget).saturating_add(180),
            tick_interval_ms,
            batch_budget,
            recv_budget,
        )
    });
    std::thread::sleep(Duration::from_millis(startup_wait_ms));
    let sender_result = run_sender(
        chain_id,
        tx_count,
        sender_node,
        receiver_node,
        sender_addr.as_str(),
        receiver_addr.as_str(),
        FaultConfigV1 {
            enabled: false,
            loss_bps: 0,
            duplicate_bps: 0,
            delay_ms: 0,
            reorder_bps: 0,
            seed: 0,
        },
        SustainedConfigV1 {
            enabled: false,
            duration_seconds: 0,
            tx_per_round: tx_count,
            round_interval_ms: 0,
        },
        TailRepairConfigV1 {
            enabled: true,
            rounds: 1,
            interval_ms: 200,
            require_ack: false,
            missing_sample_limit: 256,
            fallback_tail_window: tx_count.min(2048),
            packet_copies: 1,
            tail_packet_copies: 1,
            batch_size: 64,
            batch_pause_ms: 0,
            tail_batch_pause_ms: 0,
            round_pause_ms: 200,
        },
        default_udp_send_retry_config(),
        default_udp_ack_config(),
        NovoRudpConfigV1 {
            enabled: false,
            window_size: 64,
            packet_copies: 2,
            tail_packet_copies: 3,
            batch_size: 16,
            batch_pause_ms: 10,
            window_ack_wait_ms: 1000,
            max_window_retries: 8,
            tail_window_max_retries: 16,
            tail_window_packet_copies: 6,
            tail_window_batch_size: 8,
            tail_window_batch_pause_ms: 20,
            tail_window_ack_wait_ms: 1500,
            ack_progress_interval_ms: 250,
            no_progress_backoff: true,
        },
    );
    let receiver_result = match handle.join() {
        Ok(result) => result,
        Err(_) => Err(anyhow::anyhow!("memory bisect receiver thread panicked")),
    };
    let diagnostics_report = read_json_file(diagnostics_path.as_path());
    let mut error_parts = Vec::<String>::new();
    if let Err(err) = sender_result.as_ref() {
        error_parts.push(format!("sender: {err}"));
    }
    if let Err(err) = receiver_result.as_ref() {
        error_parts.push(format!("receiver: {err}"));
    }
    let error = if error_parts.is_empty() {
        None
    } else {
        Some(error_parts.join("; "))
    };
    memory_bisect_variant_report(
        variant_name,
        toggle_env,
        probe_only_not_functional,
        diagnostics_report.as_ref(),
        receiver_result.as_ref().ok(),
        error.as_deref(),
    )
}

#[allow(clippy::too_many_arguments)]
fn run_memory_bisect_gate(
    chain_id: u64,
    tx_count: u64,
    sender_node: u64,
    receiver_node: u64,
    node_bin: &Path,
    tick_interval_ms: u64,
    batch_budget: u64,
    recv_budget: u64,
    startup_wait_ms: u64,
) -> Result<Value> {
    let mut variants = Vec::<Value>::new();
    variants.push(run_memory_bisect_variant(
        "baseline",
        None,
        false,
        chain_id,
        tx_count,
        sender_node,
        receiver_node,
        node_bin,
        tick_interval_ms,
        batch_budget,
        recv_budget,
        startup_wait_ms,
    ));
    for (toggle_name, env_name, probe_only_not_functional) in MEMORY_PROBE_TOGGLES_V1 {
        variants.push(run_memory_bisect_variant(
            toggle_name,
            Some(env_name),
            *probe_only_not_functional,
            chain_id,
            tx_count,
            sender_node,
            receiver_node,
            node_bin,
            tick_interval_ms,
            batch_budget,
            recv_budget,
            startup_wait_ms,
        ));
    }
    clear_memory_probe_toggle_envs();
    let baseline_private = variants
        .first()
        .map(|variant| sample_u64(variant, "peak_private_bytes"))
        .unwrap_or_default();
    let baseline_native = variants
        .first()
        .map(|variant| sample_u64(variant, "peak_native_heap_unattributed_bytes"))
        .unwrap_or_default();
    let mut best_stage = "allocator_or_external_native_heap_suspected".to_string();
    let mut best_reduction_percent = 0u64;
    for variant in variants.iter_mut().skip(1) {
        let private = sample_u64(variant, "peak_private_bytes");
        let native = sample_u64(variant, "peak_native_heap_unattributed_bytes");
        let delta_private = baseline_private.saturating_sub(private);
        let delta_native = baseline_native.saturating_sub(native);
        let reduction_percent = if baseline_private == 0 {
            0
        } else {
            delta_private.saturating_mul(100) / baseline_private
        };
        variant["baseline_peak_private_bytes"] = serde_json::json!(baseline_private);
        variant["baseline_peak_native_heap_unattributed_bytes"] =
            serde_json::json!(baseline_native);
        variant["delta_private_bytes"] = serde_json::json!(delta_private);
        variant["delta_native_heap_unattributed_bytes"] = serde_json::json!(delta_native);
        variant["reduction_percent"] = serde_json::json!(reduction_percent);
        if reduction_percent > best_reduction_percent {
            best_reduction_percent = reduction_percent;
            best_stage = variant
                .get("toggle_name")
                .and_then(Value::as_str)
                .unwrap_or("unknown")
                .to_string();
        }
    }
    let suspected_stage = if best_reduction_percent >= 30 {
        best_stage
    } else {
        "allocator_or_external_native_heap_suspected".to_string()
    };
    let accepted = variants
        .first()
        .and_then(|variant| variant.get("accepted"))
        .and_then(Value::as_bool)
        == Some(true);
    Ok(serde_json::json!({
        "schema": MEMORY_BISECT_SCHEMA_V1,
        "accepted": accepted,
        "tx_count": tx_count,
        "chain_id": chain_id,
        "baseline_peak_private_bytes": baseline_private,
        "baseline_peak_native_heap_unattributed_bytes": baseline_native,
        "suspected_stage": suspected_stage,
        "confidence": if best_reduction_percent >= 30 { "toggle_delta_over_30_percent" } else { "low_no_toggle_reduction_over_30_percent" },
        "best_reduction_percent": best_reduction_percent,
        "memory_plateau_signed": false,
        "pipeline_lifecycle_changed": false,
        "aoem_concurrency_owner": "AOEM_runtime",
        "product_entry": "pending_only",
        "notes": [
            "probe toggles are memory attribution controls only",
            "probe_only_not_functional toggles must not be used to sign functional production behavior",
            "if no toggle reduces private/native heap materially, use external heap profiler attribution"
        ],
        "variants": variants,
    }))
}

fn main() -> Result<()> {
    let role = first_string_env_nonempty(&[
        "NOVOVM_NATIVE_PIPELINE_ROLE",
        "NOVOVM_NATIVE_PIPELINE_CROSS_MACHINE_ROLE",
    ])
    .unwrap_or_else(|| "local-smoke".to_string())
    .to_ascii_lowercase();
    let memory_bisect_binary = current_bin_name_contains("memory-bisect");
    let sustained_binary = current_bin_name_contains("sustained");
    let sustained_env = env_any(&[
        "NOVOVM_NATIVE_PIPELINE_SUSTAINED_ENABLED",
        "NOVOVM_NATIVE_PIPELINE_SUSTAINED_DURATION_SECONDS",
        "NOVOVM_NATIVE_PIPELINE_SUSTAINED_TX_PER_ROUND",
        "NOVOVM_NATIVE_PIPELINE_SUSTAINED_ROUND_INTERVAL_MS",
    ]);
    let sustained_enabled = sustained_binary || sustained_env;
    let chain_id = u64_env_alias(
        &[
            "NOVOVM_NATIVE_PIPELINE_CHAIN_ID",
            "NOVOVM_NATIVE_PIPELINE_CROSS_MACHINE_CHAIN_ID",
        ],
        9_998_904,
    )?;
    let tx_count = u64_env_alias(
        &[
            "NOVOVM_NATIVE_PIPELINE_TX_COUNT",
            "NOVOVM_NATIVE_PIPELINE_CROSS_MACHINE_TX_COUNT",
        ],
        if memory_bisect_binary {
            512
        } else if sustained_enabled {
            256
        } else {
            32
        },
    )?
    .max(1);
    let batch_budget = u64_env_alias(
        &[
            "NOVOVM_NATIVE_PIPELINE_BATCH_BUDGET",
            "NOVOVM_NATIVE_PIPELINE_CROSS_MACHINE_BATCH_BUDGET",
        ],
        if sustained_enabled { 32 } else { 8 },
    )?
    .max(1);
    let recv_budget = u64_env_alias(
        &[
            "NOVOVM_NATIVE_PIPELINE_RECV_BUDGET",
            "NOVOVM_NATIVE_PIPELINE_CROSS_MACHINE_RECV_BUDGET",
        ],
        128,
    )?
    .max(1);
    let tick_interval_ms = u64_env_alias(
        &[
            "NOVOVM_NATIVE_PIPELINE_TICK_INTERVAL_MS",
            "NOVOVM_NATIVE_PIPELINE_CROSS_MACHINE_TICK_INTERVAL_MS",
        ],
        100,
    )?
    .max(1);
    let max_ticks = u64_env_alias(
        &[
            "NOVOVM_NATIVE_PIPELINE_MAX_TICKS",
            "NOVOVM_NATIVE_PIPELINE_CROSS_MACHINE_MAX_TICKS",
        ],
        if role == "receiver" {
            3600
        } else {
            div_ceil_u64(tx_count, batch_budget).saturating_add(180)
        },
    )?
    .max(1);
    let startup_wait_ms = u64_env("NOVOVM_NATIVE_PIPELINE_STARTUP_WAIT_MS", 500)?;
    let fault_binary = current_bin_name_contains("fault");
    let fault_env = env_any(&[
        "NOVOVM_NATIVE_PIPELINE_FAULT_PACKET_LOSS_BPS",
        "NOVOVM_NATIVE_PIPELINE_FAULT_DUPLICATE_BPS",
        "NOVOVM_NATIVE_PIPELINE_FAULT_DELAY_MS",
        "NOVOVM_NATIVE_PIPELINE_FAULT_REORDER_BPS",
        "NOVOVM_NATIVE_PIPELINE_FAULT_SEED",
    ]);
    let fault_enabled = fault_binary || fault_env;
    let fault = FaultConfigV1 {
        enabled: fault_enabled,
        loss_bps: u64_env(
            "NOVOVM_NATIVE_PIPELINE_FAULT_PACKET_LOSS_BPS",
            if fault_enabled { 200 } else { 0 },
        )?
        .min(10_000),
        duplicate_bps: u64_env(
            "NOVOVM_NATIVE_PIPELINE_FAULT_DUPLICATE_BPS",
            if fault_enabled { 3000 } else { 0 },
        )?
        .min(10_000),
        delay_ms: u64_env(
            "NOVOVM_NATIVE_PIPELINE_FAULT_DELAY_MS",
            if fault_enabled { 20 } else { 0 },
        )?,
        reorder_bps: u64_env(
            "NOVOVM_NATIVE_PIPELINE_FAULT_REORDER_BPS",
            if fault_enabled { 1000 } else { 0 },
        )?
        .min(10_000),
        seed: u64_env(
            "NOVOVM_NATIVE_PIPELINE_FAULT_SEED",
            if fault_enabled { 123 } else { 0 },
        )?,
    };
    let tx_per_round = u64_env("NOVOVM_NATIVE_PIPELINE_SUSTAINED_TX_PER_ROUND", 32)?.max(1);
    let sustained_rounds = div_ceil_u64(tx_count, tx_per_round).max(1);
    let duration_seconds = u64_env(
        "NOVOVM_NATIVE_PIPELINE_SUSTAINED_DURATION_SECONDS",
        if sustained_enabled { 1800 } else { 0 },
    )?;
    let default_round_interval_ms = if sustained_enabled && sustained_rounds > 1 {
        duration_seconds
            .saturating_mul(1_000)
            .checked_div(sustained_rounds.saturating_sub(1))
            .unwrap_or(0)
    } else {
        0
    };
    let sustained = SustainedConfigV1 {
        enabled: sustained_enabled,
        duration_seconds,
        tx_per_round,
        round_interval_ms: u64_env(
            "NOVOVM_NATIVE_PIPELINE_SUSTAINED_ROUND_INTERVAL_MS",
            default_round_interval_ms,
        )?,
    };
    let tail_repair_enabled = bool_env("NOVOVM_NATIVE_PIPELINE_TAIL_REPAIR_ENABLED")
        || (sustained.enabled
            && string_env_nonempty("NOVOVM_NATIVE_PIPELINE_TAIL_REPAIR_ENABLED").is_none());
    let tail_repair = TailRepairConfigV1 {
        enabled: tail_repair_enabled,
        rounds: u64_env(
            "NOVOVM_NATIVE_PIPELINE_TAIL_REPAIR_ROUNDS",
            if tail_repair_enabled { 3 } else { 0 },
        )?,
        interval_ms: u64_env(
            "NOVOVM_NATIVE_PIPELINE_TAIL_REPAIR_INTERVAL_MS",
            if tail_repair_enabled { 1000 } else { 0 },
        )?,
        require_ack: bool_env("NOVOVM_NATIVE_PIPELINE_TAIL_REPAIR_REQUIRE_ACK"),
        missing_sample_limit: u64_env("NOVOVM_NATIVE_PIPELINE_MISSING_SAMPLE_LIMIT", 256)?,
        fallback_tail_window: u64_env(
            "NOVOVM_NATIVE_PIPELINE_TAIL_REPAIR_FALLBACK_TAIL_WINDOW",
            tx_count.min(2048),
        )?,
        packet_copies: u64_env("NOVOVM_NATIVE_PIPELINE_REPAIR_PACKET_COPIES", 3)?.max(1),
        tail_packet_copies: u64_env("NOVOVM_NATIVE_PIPELINE_REPAIR_TAIL_PACKET_COPIES", 6)?.max(1),
        batch_size: u64_env("NOVOVM_NATIVE_PIPELINE_REPAIR_BATCH_SIZE", 64)?.max(1),
        batch_pause_ms: u64_env("NOVOVM_NATIVE_PIPELINE_REPAIR_BATCH_PAUSE_MS", 5)?,
        tail_batch_pause_ms: u64_env("NOVOVM_NATIVE_PIPELINE_REPAIR_TAIL_BATCH_PAUSE_MS", 10)?,
        round_pause_ms: u64_env(
            "NOVOVM_NATIVE_PIPELINE_REPAIR_ROUND_PAUSE_MS",
            u64_env(
                "NOVOVM_NATIVE_PIPELINE_TAIL_REPAIR_INTERVAL_MS",
                if tail_repair_enabled { 1000 } else { 0 },
            )?,
        )?,
    };
    let udp_send_retry = UdpSendRetryConfigV1 {
        max_retries: u64_env("NOVOVM_NATIVE_PIPELINE_UDP_SEND_RETRY_MAX", 10)?,
        backoff_ms: u64_env("NOVOVM_NATIVE_PIPELINE_UDP_SEND_RETRY_BACKOFF_MS", 5)?,
        backoff_max_ms: u64_env("NOVOVM_NATIVE_PIPELINE_UDP_SEND_RETRY_BACKOFF_MAX_MS", 100)?
            .max(1),
    };
    let udp_ack = UdpAckConfigV1 {
        enabled: bool_env("NOVOVM_NATIVE_PIPELINE_UDP_ACK_ENABLED")
            || string_env_nonempty("NOVOVM_NATIVE_PIPELINE_UDP_ACK_ENABLED").is_none(),
        bind_addr: first_string_env_nonempty(&["NOVOVM_NATIVE_PIPELINE_ACK_BIND_ADDR"])
            .unwrap_or_else(|| "0.0.0.0:0".to_string()),
        target_addr: first_string_env_nonempty(&[
            "NOVOVM_NATIVE_PIPELINE_ACK_TARGET_ADDR",
            "NOVOVM_NATIVE_PIPELINE_SENDER_ACK_ADDR",
        ]),
        recv_timeout_ms: u64_env("NOVOVM_NATIVE_PIPELINE_ACK_RECV_TIMEOUT_MS", 250)?,
    };
    let transport_profile = TransportProfileV1::from_env()?;
    let novorudp = NovoRudpConfigV1::from_env(transport_profile)?;
    let sender_node = u64_env_alias(
        &[
            "NOVOVM_NATIVE_PIPELINE_SENDER_NODE",
            "NOVOVM_NATIVE_PIPELINE_CROSS_MACHINE_SENDER_NODE",
        ],
        9_991_940,
    )?;
    let receiver_node = u64_env_alias(
        &[
            "NOVOVM_NATIVE_PIPELINE_RECEIVER_NODE",
            "NOVOVM_NATIVE_PIPELINE_CROSS_MACHINE_RECEIVER_NODE",
        ],
        9_991_941,
    )?;
    let path = report_path(role.as_str());
    let node_bin = novovm_node_bin();
    let store = store_path(chain_id, role.as_str());
    if memory_bisect_binary {
        if !node_bin.exists() {
            bail!(
                "novovm-node binary not found: {}; build with `cargo build -p novovm-node --bins` or set NOVOVM_NATIVE_PIPELINE_NODE_BIN",
                node_bin.display()
            );
        }
        let report = run_memory_bisect_gate(
            chain_id,
            tx_count,
            sender_node,
            receiver_node,
            node_bin.as_path(),
            tick_interval_ms.min(20),
            batch_budget.max(64),
            recv_budget.max(256),
            startup_wait_ms.min(100),
        )?;
        let path = memory_bisect_report_path();
        write_report(path.as_path(), &report)?;
        println!(
            "{}",
            serde_json::to_string_pretty(&report).context("encode memory bisect report failed")?
        );
        if !report
            .get("accepted")
            .and_then(Value::as_bool)
            .unwrap_or(false)
        {
            bail!("native pipeline memory bisect failed: {}", path.display());
        }
        return Ok(());
    }
    if matches!(role.as_str(), "receiver" | "local-smoke" | "local_smoke") && !node_bin.exists() {
        bail!(
            "novovm-node binary not found: {}; build with `cargo build -p novovm-node --bins` or set NOVOVM_NATIVE_PIPELINE_NODE_BIN",
            node_bin.display()
        );
    }

    let report = match role.as_str() {
        "receiver" => {
            let listen_addr = first_string_env_nonempty(&[
                "NOVOVM_NATIVE_PIPELINE_LISTEN_ADDR",
                "NOVOVM_NATIVE_PIPELINE_RECEIVER_LISTEN_ADDR",
            ])
            .unwrap_or_else(|| "0.0.0.0:39001".to_string());
            run_receiver(
                chain_id,
                tx_count,
                receiver_node,
                listen_addr.as_str(),
                node_bin.as_path(),
                store.as_path(),
                max_ticks,
                tick_interval_ms,
                batch_budget,
                recv_budget,
                sustained,
            )?
        }
        "sender" => {
            let receiver_addr = first_string_env_nonempty(&[
                "NOVOVM_NATIVE_PIPELINE_RECEIVER_ADDR",
                "NOVOVM_NATIVE_PIPELINE_PEER_ADDR",
            ])
            .ok_or_else(|| {
                anyhow::anyhow!(
                    "sender role requires NOVOVM_NATIVE_PIPELINE_RECEIVER_ADDR=host:port"
                )
            })?;
            let sender_addr = first_string_env_nonempty(&[
                "NOVOVM_NATIVE_PIPELINE_LISTEN_ADDR",
                "NOVOVM_NATIVE_PIPELINE_SENDER_LISTEN_ADDR",
            ])
            .unwrap_or_else(|| "0.0.0.0:0".to_string());
            run_sender(
                chain_id,
                tx_count,
                sender_node,
                receiver_node,
                sender_addr.as_str(),
                receiver_addr.as_str(),
                fault,
                sustained,
                tail_repair,
                udp_send_retry,
                udp_ack,
                novorudp,
            )?
        }
        "local-smoke" | "local_smoke" => run_local_smoke(
            chain_id,
            tx_count,
            sender_node,
            receiver_node,
            node_bin.as_path(),
            store.as_path(),
            max_ticks,
            tick_interval_ms,
            batch_budget,
            recv_budget,
            startup_wait_ms,
            fault,
            sustained,
            tail_repair,
            novorudp,
        )?,
        other => bail!("unknown NOVOVM_NATIVE_PIPELINE_ROLE: {other}"),
    };
    write_report(path.as_path(), &report)?;
    println!(
        "{}",
        serde_json::to_string_pretty(&report).context("encode cross-machine report failed")?
    );
    if !report
        .get("accepted")
        .and_then(Value::as_bool)
        .unwrap_or(false)
    {
        bail!("cross-machine UDP soak failed: {}", path.display());
    }
    Ok(())
}

fn compact_tx_hash_array_value(value: &Value) -> Value {
    let Some(items) = value.as_array() else {
        return value.clone();
    };
    let mut digest = RollingDigestV1::default();
    let mut first = Vec::<Value>::new();
    let mut last = Vec::<Value>::new();
    for item in items {
        if let Some(raw) = item.as_str() {
            digest.update(raw.as_bytes());
        }
        if first.len() < 8 {
            first.push(item.clone());
        }
    }
    let start = items.len().saturating_sub(8);
    for item in items.iter().skip(start) {
        last.push(item.clone());
    }
    serde_json::json!({
        "omitted": true,
        "count": items.len(),
        "digest": digest.finish_hex(),
        "first_samples": first,
        "last_samples": last,
    })
}

fn report_array_len_recursive(value: &Value) -> usize {
    match value {
        Value::Array(items) => {
            items.len() + items.iter().map(report_array_len_recursive).sum::<usize>()
        }
        Value::Object(map) => map.values().map(report_array_len_recursive).sum(),
        _ => 0,
    }
}

#[derive(Debug, Default)]
struct RollingDigestV1 {
    state: u64,
    count: u64,
}

impl RollingDigestV1 {
    fn update(&mut self, bytes: &[u8]) {
        const FNV_OFFSET: u64 = 0xcbf29ce484222325;
        const FNV_PRIME: u64 = 0x100000001b3;
        if self.count == 0 && self.state == 0 {
            self.state = FNV_OFFSET;
        }
        for byte in bytes {
            self.state ^= u64::from(*byte);
            self.state = self.state.wrapping_mul(FNV_PRIME);
        }
        self.count = self.count.saturating_add(1);
    }

    fn finish_hex(&self) -> String {
        format!("{:016x}:{}", self.state, self.count)
    }
}

fn compact_probe_for_report(mut probe: Value) -> Value {
    if let Some(map) = probe.as_object_mut() {
        if let Some(value) = map.get("receipt_hashes").cloned() {
            map.insert(
                "receipt_hashes".to_string(),
                compact_tx_hash_array_value(&value),
            );
        }
    }
    probe
}

fn compact_receiver_summary_for_report(summary: Value) -> Value {
    serde_json::json!({
        "accepted": summary.get("accepted").cloned().unwrap_or(Value::Null),
        "execution_kernel": summary.get("execution_kernel").cloned().unwrap_or(Value::Null),
        "aoem_concurrency_owner": summary.get("aoem_concurrency_owner").cloned().unwrap_or(Value::Null),
        "host_concurrency_policy": summary.get("host_concurrency_policy").cloned().unwrap_or(Value::Null),
        "wrapper_env_aoem_production_candidate": summary.get("wrapper_env_aoem_production_candidate").cloned().unwrap_or(Value::Null),
        "wrapper_env_aoem_shadow": summary.get("wrapper_env_aoem_shadow").cloned().unwrap_or(Value::Null),
        "wrapper_env_aoem_compare": summary.get("wrapper_env_aoem_compare").cloned().unwrap_or(Value::Null),
        "child_spawn_env_aoem_production_candidate": summary.get("child_spawn_env_aoem_production_candidate").cloned().unwrap_or(Value::Null),
        "child_spawn_env_aoem_shadow": summary.get("child_spawn_env_aoem_shadow").cloned().unwrap_or(Value::Null),
        "child_spawn_env_aoem_compare": summary.get("child_spawn_env_aoem_compare").cloned().unwrap_or(Value::Null),
        "child_runtime_env_aoem_production_candidate": summary.get("child_runtime_env_aoem_production_candidate").cloned().unwrap_or(Value::Null),
        "child_runtime_env_aoem_shadow": summary.get("child_runtime_env_aoem_shadow").cloned().unwrap_or(Value::Null),
        "child_runtime_env_aoem_compare": summary.get("child_runtime_env_aoem_compare").cloned().unwrap_or(Value::Null),
        "child_runtime_aoem_gate_config_source": summary.get("child_runtime_aoem_gate_config_source").cloned().unwrap_or(Value::Null),
        "tx_ingress_env_aoem_production_candidate": summary.get("tx_ingress_env_aoem_production_candidate").cloned().unwrap_or(Value::Null),
        "tx_ingress_env_aoem_shadow": summary.get("tx_ingress_env_aoem_shadow").cloned().unwrap_or(Value::Null),
        "tx_ingress_env_aoem_compare": summary.get("tx_ingress_env_aoem_compare").cloned().unwrap_or(Value::Null),
        "tx_ingress_aoem_gate_config_source": summary.get("tx_ingress_aoem_gate_config_source").cloned().unwrap_or(Value::Null),
        "tx_ingress_aoem_gate_config_explicit": summary.get("tx_ingress_aoem_gate_config_explicit").cloned().unwrap_or(Value::Null),
        "tx_ingress_aoem_gate_config_production_candidate": summary.get("tx_ingress_aoem_gate_config_production_candidate").cloned().unwrap_or(Value::Null),
        "tx_ingress_aoem_gate_config_shadow": summary.get("tx_ingress_aoem_gate_config_shadow").cloned().unwrap_or(Value::Null),
        "tx_ingress_aoem_gate_config_compare": summary.get("tx_ingress_aoem_gate_config_compare").cloned().unwrap_or(Value::Null),
        "aoem_owned_child_runtime_gate_propagated_to_tx_ingress": summary.get("aoem_owned_child_runtime_gate_propagated_to_tx_ingress").cloned().unwrap_or(Value::Null),
        "aoem_owned_single_path_enforced": summary.get("aoem_owned_single_path_enforced").cloned().unwrap_or(Value::Null),
        "legacy_host_transitional_fallback_gate_enabled": summary.get("legacy_host_transitional_fallback_gate_enabled").cloned().unwrap_or(Value::Null),
        "legacy_host_transitional_fallback_used": summary.get("legacy_host_transitional_fallback_used").cloned().unwrap_or(Value::Null),
        "legacy_host_transitional_success_suppressed_by_aoem_gate": summary.get("legacy_host_transitional_success_suppressed_by_aoem_gate").cloned().unwrap_or(Value::Null),
        "aoem_owned_regression_signable": summary.get("aoem_owned_regression_signable").cloned().unwrap_or(Value::Null),
        "aoem_owned_signoff_blocker_reasons": summary.get("aoem_owned_signoff_blocker_reasons").cloned().unwrap_or_else(|| serde_json::json!([])),
        "tx_ingress_real_callsite": summary.get("tx_ingress_real_callsite").cloned().unwrap_or(Value::Null),
        "receiver_pipeline_mode": summary.get("receiver_pipeline_mode").cloned().unwrap_or(Value::Null),
        "network_receiver_object_ready_count": summary_u64(&summary, "network_receiver_object_ready_count"),
        "network_receiver_calls_production_tx_ingress": summary.get("network_receiver_calls_production_tx_ingress").cloned().unwrap_or(Value::Null),
        "object_assembler_batch_ready_count": summary_u64(&summary, "object_assembler_batch_ready_count"),
        "object_assembler_commitment_ok_count": summary_u64(&summary, "object_assembler_commitment_ok_count"),
        "aoem_runtime_worker_batch_received_count": summary_u64(&summary, "aoem_runtime_worker_batch_received_count"),
        "aoem_runtime_worker_tx_ingress_call_count": summary_u64(&summary, "aoem_runtime_worker_tx_ingress_call_count"),
        "aoem_runtime_worker_tx_ingress_callsite": summary.get("aoem_runtime_worker_tx_ingress_callsite").cloned().unwrap_or(Value::Null),
        "aoem_runtime_worker_result_ready_count": summary_u64(&summary, "aoem_runtime_worker_result_ready_count"),
        "finality_report_worker_result_verified_count": summary_u64(&summary, "finality_report_worker_result_verified_count"),
        "finality_report_worker_final_report_written": summary.get("finality_report_worker_final_report_written").cloned().unwrap_or(Value::Null),
        "tx_ingress_called_by_network_receiver": summary.get("tx_ingress_called_by_network_receiver").cloned().unwrap_or(Value::Null),
        "tx_ingress_called_by_aoem_runtime_worker": summary.get("tx_ingress_called_by_aoem_runtime_worker").cloned().unwrap_or(Value::Null),
        "receiver_pipeline_stage_lag": summary.get("receiver_pipeline_stage_lag").cloned().unwrap_or_else(|| serde_json::json!({})),
        "receiver_pipeline_backpressure_reason": summary.get("receiver_pipeline_backpressure_reason").cloned().unwrap_or(Value::Null),
        "aoem_runtime_worker_scheduler": summary.get("aoem_runtime_worker_scheduler").cloned().unwrap_or(Value::Null),
        "aoem_runtime_worker_active_sleep_ms": summary.get("aoem_runtime_worker_active_sleep_ms").cloned().unwrap_or(Value::Null),
        "aoem_runtime_worker_idle_sleep_ms": summary.get("aoem_runtime_worker_idle_sleep_ms").cloned().unwrap_or(Value::Null),
        "tx_ingress_called_with_explicit_aoem_gate_config": summary.get("tx_ingress_called_with_explicit_aoem_gate_config").cloned().unwrap_or(Value::Null),
        "tx_ingress_selected_path": summary.get("tx_ingress_selected_path").cloned().unwrap_or(Value::Null),
        "tx_ingress_aoem_production_candidate_gate_reason": summary.get("tx_ingress_aoem_production_candidate_gate_reason").cloned().unwrap_or(Value::Null),
        "receiver_final_summary_aoem_fields_source": summary.get("receiver_final_summary_aoem_fields_source").cloned().unwrap_or(Value::Null),
        "receiver_final_summary_aoem_fields_present": summary.get("receiver_final_summary_aoem_fields_present").cloned().unwrap_or(Value::Null),
        "receiver_final_summary_aoem_fields_defaulted": summary.get("receiver_final_summary_aoem_fields_defaulted").cloned().unwrap_or(Value::Null),
        "receiver_final_summary_aoem_fields_missing_reasons": summary.get("receiver_final_summary_aoem_fields_missing_reasons").cloned().unwrap_or_else(|| serde_json::json!([])),
        "aoem_owned_gate_fail_reason": summary.get("aoem_owned_gate_fail_reason").cloned().unwrap_or(Value::Null),
        "aoem_native_tx_batch_production_candidate_enabled": summary.get("aoem_native_tx_batch_production_candidate_enabled").cloned().unwrap_or(Value::Null),
        "aoem_native_tx_batch_production_candidate_result_ok": summary.get("aoem_native_tx_batch_production_candidate_result_ok").cloned().unwrap_or(Value::Null),
        "aoem_native_tx_batch_production_owner": summary.get("aoem_native_tx_batch_production_owner").cloned().unwrap_or(Value::Null),
        "tx_ingress_production_target": summary.get("tx_ingress_production_target").cloned().unwrap_or(Value::Null),
        "aoem_native_tx_batch_production_receipt_count": summary_u64(&summary, "aoem_native_tx_batch_production_receipt_count"),
        "aoem_native_tx_batch_production_canonical_proof_count": summary_u64(&summary, "aoem_native_tx_batch_production_canonical_proof_count"),
        "aoem_native_tx_batch_production_ledger_close_proof_count": summary_u64(&summary, "aoem_native_tx_batch_production_ledger_close_proof_count"),
        "aoem_native_tx_batch_production_state_delta_root_present": summary.get("aoem_native_tx_batch_production_state_delta_root_present").cloned().unwrap_or(Value::Null),
        "aoem_native_tx_batch_production_snapshot_metadata_present": summary.get("aoem_native_tx_batch_production_snapshot_metadata_present").cloned().unwrap_or(Value::Null),
        "aoem_native_tx_batch_production_fallback_used": summary.get("aoem_native_tx_batch_production_fallback_used").cloned().unwrap_or(Value::Null),
        "aoem_native_tx_batch_production_mismatch_reasons": summary.get("aoem_native_tx_batch_production_mismatch_reasons").cloned().unwrap_or_else(|| serde_json::json!([])),
        "aoem_native_tx_batch_production_double_write_legacy_canonical": summary.get("aoem_native_tx_batch_production_double_write_legacy_canonical").cloned().unwrap_or(Value::Null),
        "ticks": summary_u64(&summary, "ticks"),
        "elapsed_ms": summary_u64(&summary, "elapsed_ms"),
        "ticks_per_sec_x1000": summary_u64(&summary, "ticks_per_sec_x1000"),
        "progress_score": summary_u64(&summary, "progress_score"),
        "aoem_executed_total": summary_u64(&summary, "aoem_executed_total"),
        "aoem_deferred_total": summary_u64(&summary, "aoem_deferred_total"),
        "aoem_runtime_open_count": summary_u64(&summary, "aoem_runtime_open_count"),
        "aoem_handle_created_count": summary_u64(&summary, "aoem_handle_created_count"),
        "aoem_session_created_count": summary_u64(&summary, "aoem_session_created_count"),
        "aoem_session_reused_count": summary_u64(&summary, "aoem_session_reused_count"),
        "aoem_worker_pool_created_count": summary_u64(&summary, "aoem_worker_pool_created_count"),
        "tokio_runtime_created_count": summary_u64(&summary, "tokio_runtime_created_count"),
        "std_thread_spawn_count": summary_u64(&summary, "std_thread_spawn_count"),
        "spawn_blocking_count": summary_u64(&summary, "spawn_blocking_count"),
        "included_canonical_total": summary_u64(&summary, "included_canonical_total"),
        "included_canonical_last": summary_u64(&summary, "included_canonical_last"),
        "ingress_total_last": summary_u64(&summary, "ingress_total_last"),
        "repair_packet_received_count": summary_u64(&summary, "repair_packet_received_count"),
        "repair_packet_decode_failed_count": summary_u64(&summary, "repair_packet_decode_failed_count"),
        "repair_sequence_received_count": summary_u64(&summary, "repair_sequence_received_count"),
        "repair_sequence_received_min": summary.get("repair_sequence_received_min").cloned().unwrap_or(Value::Null),
        "repair_sequence_received_max": summary.get("repair_sequence_received_max").cloned().unwrap_or(Value::Null),
        "repair_sequence_received_ranges_sample": summary.get("repair_sequence_received_ranges_sample").cloned().unwrap_or_else(|| serde_json::json!([])),
        "repair_sequence_accepted_ranges_sample": summary.get("repair_sequence_accepted_ranges_sample").cloned().unwrap_or_else(|| serde_json::json!([])),
        "repair_sequence_enqueued_ranges_sample": summary.get("repair_sequence_enqueued_ranges_sample").cloned().unwrap_or_else(|| serde_json::json!([])),
        "repair_sequence_already_receipted_ranges_sample": summary.get("repair_sequence_already_receipted_ranges_sample").cloned().unwrap_or_else(|| serde_json::json!([])),
        "repair_sequence_duplicate_ranges_sample": summary.get("repair_sequence_duplicate_ranges_sample").cloned().unwrap_or_else(|| serde_json::json!([])),
        "repair_sequence_admitted_to_aoem_ranges_sample": summary.get("repair_sequence_admitted_to_aoem_ranges_sample").cloned().unwrap_or_else(|| serde_json::json!([])),
        "repair_sequence_accepted_count": summary_u64(&summary, "repair_sequence_accepted_count"),
        "repair_sequence_duplicate_count": summary_u64(&summary, "repair_sequence_duplicate_count"),
        "repair_sequence_rejected_count": summary_u64(&summary, "repair_sequence_rejected_count"),
        "repair_reject_reason_counts": summary.get("repair_reject_reason_counts").cloned().unwrap_or_else(|| serde_json::json!({})),
        "repair_reject_reason_samples": summary.get("repair_reject_reason_samples").cloned().unwrap_or_else(|| serde_json::json!([])),
        "repair_sequence_already_receipted_count": summary_u64(&summary, "repair_sequence_already_receipted_count"),
        "repair_sequence_wrong_run_id_count": summary_u64(&summary, "repair_sequence_wrong_run_id_count"),
        "repair_sequence_wrong_chain_id_count": summary_u64(&summary, "repair_sequence_wrong_chain_id_count"),
        "repair_sequence_out_of_range_count": summary_u64(&summary, "repair_sequence_out_of_range_count"),
        "repair_sequence_stale_count": summary_u64(&summary, "repair_sequence_stale_count"),
        "repair_sequence_enqueued_count": summary_u64(&summary, "repair_sequence_enqueued_count"),
        "repair_sequence_admitted_to_aoem_count": summary_u64(&summary, "repair_sequence_admitted_to_aoem_count"),
        "repair_attempted_unreceipted_count": summary_u64(&summary, "repair_attempted_unreceipted_count"),
        "repair_attempted_unreceipted_final_missing_overlap_count": summary_u64(&summary, "repair_attempted_unreceipted_final_missing_overlap_count"),
        "repair_attempted_unreceipted_requeued_count": summary_u64(&summary, "repair_attempted_unreceipted_requeued_count"),
        "repair_attempted_unreceipted_requeue_failed_count": summary_u64(&summary, "repair_attempted_unreceipted_requeue_failed_count"),
        "repair_final_missing_payload_available_count": summary_u64(&summary, "repair_final_missing_payload_available_count"),
        "repair_final_missing_payload_available_but_inactive_count": summary_u64(&summary, "repair_final_missing_payload_available_but_inactive_count"),
        "repair_final_missing_invariant_violation_count": summary_u64(&summary, "repair_final_missing_invariant_violation_count"),
        "repair_final_missing_sequence_to_tx_hash_count": summary_u64(&summary, "repair_final_missing_sequence_to_tx_hash_count"),
        "repair_final_missing_tx_hash_payload_hit_count": summary_u64(&summary, "repair_final_missing_tx_hash_payload_hit_count"),
        "repair_final_missing_payload_missing_by_sequence_count": summary_u64(&summary, "repair_final_missing_payload_missing_by_sequence_count"),
        "repair_final_missing_payload_missing_ranges_sample": summary.get("repair_final_missing_payload_missing_ranges_sample").cloned().unwrap_or_else(|| serde_json::json!([])),
        "repair_sequence_payload_index_count": summary_u64(&summary, "repair_sequence_payload_index_count"),
        "repair_sequence_payload_index_final_missing_overlap_count": summary_u64(&summary, "repair_sequence_payload_index_final_missing_overlap_count"),
        "repair_sequence_payload_index_evicted_count": summary_u64(&summary, "repair_sequence_payload_index_evicted_count"),
        "repair_payload_retention_false_negative_suspected": summary.get("repair_payload_retention_false_negative_suspected").and_then(Value::as_bool).unwrap_or(false),
        "repair_final_missing_payload_recovered_count": summary_u64(&summary, "repair_final_missing_payload_recovered_count"),
        "repair_final_missing_payload_recovered_requeued_count": summary_u64(&summary, "repair_final_missing_payload_recovered_requeued_count"),
        "ledger_expected_range_start": summary.get("ledger_expected_range_start").cloned().unwrap_or(Value::Null),
        "ledger_expected_range_end": summary.get("ledger_expected_range_end").cloned().unwrap_or(Value::Null),
        "ledger_expected_count": summary_u64(&summary, "ledger_expected_count"),
        "ledger_completed_count": summary_u64(&summary, "ledger_completed_count"),
        "ledger_durable_missing_count": summary_u64(&summary, "ledger_durable_missing_count"),
        "ledger_durable_missing_ranges_sample": summary.get("ledger_durable_missing_ranges_sample").cloned().unwrap_or_else(|| serde_json::json!([])),
        "ledger_durable_missing_bitmap_available": summary.get("ledger_durable_missing_bitmap_available").and_then(Value::as_bool).unwrap_or(false),
        "ledger_durable_missing_source": summary.get("ledger_durable_missing_source").cloned().unwrap_or_else(|| serde_json::json!("")),
        "ledger_durable_missing_derived_from_expected_range": summary.get("ledger_durable_missing_derived_from_expected_range").and_then(Value::as_bool).unwrap_or(false),
        "ledger_missing_closed_by_receipt_count": summary_u64(&summary, "ledger_missing_closed_by_receipt_count"),
        "ledger_missing_closed_by_canonical_count": summary_u64(&summary, "ledger_missing_closed_by_canonical_count"),
        "ledger_missing_incorrectly_closed_by_received_count": summary_u64(&summary, "ledger_missing_incorrectly_closed_by_received_count"),
        "ledger_missing_incorrectly_closed_by_enqueued_count": summary_u64(&summary, "ledger_missing_incorrectly_closed_by_enqueued_count"),
        "ledger_close_source": summary.get("ledger_close_source").cloned().unwrap_or_else(|| serde_json::json!("")),
        "ledger_receipt_close_proof_count": summary_u64(&summary, "ledger_receipt_close_proof_count"),
        "ledger_canonical_close_proof_count": summary_u64(&summary, "ledger_canonical_close_proof_count"),
        "ledger_prefix_close_count": summary_u64(&summary, "ledger_prefix_close_count"),
        "ledger_synthetic_close_count": summary_u64(&summary, "ledger_synthetic_close_count"),
        "ledger_close_without_receipt_index_count": summary_u64(&summary, "ledger_close_without_receipt_index_count"),
        "ledger_close_without_canonical_proof_count": summary_u64(&summary, "ledger_close_without_canonical_proof_count"),
        "ledger_false_completed_invariant_violation_count": summary_u64(&summary, "ledger_false_completed_invariant_violation_count"),
        "ledger_false_completed_sequences_sample": summary.get("ledger_false_completed_sequences_sample").cloned().unwrap_or_else(|| serde_json::json!([])),
        "ledger_validation_final_missing_overlap_count": summary_u64(&summary, "ledger_validation_final_missing_overlap_count"),
        "ledger_durable_missing_validation_mismatch_count": summary_u64(&summary, "ledger_durable_missing_validation_mismatch_count"),
        "ledger_receipt_proof_writer_called_count": summary_u64(&summary, "ledger_receipt_proof_writer_called_count"),
        "ledger_canonical_proof_writer_called_count": summary_u64(&summary, "ledger_canonical_proof_writer_called_count"),
        "ledger_receipt_proof_tx_hash_count": summary_u64(&summary, "ledger_receipt_proof_tx_hash_count"),
        "ledger_canonical_proof_tx_hash_count": summary_u64(&summary, "ledger_canonical_proof_tx_hash_count"),
        "ledger_receipt_proof_close_success_count": summary_u64(&summary, "ledger_receipt_proof_close_success_count"),
        "ledger_canonical_proof_close_success_count": summary_u64(&summary, "ledger_canonical_proof_close_success_count"),
        "ledger_receipt_proof_missing_sequence_mapping_count": summary_u64(&summary, "ledger_receipt_proof_missing_sequence_mapping_count"),
        "ledger_canonical_proof_missing_sequence_mapping_count": summary_u64(&summary, "ledger_canonical_proof_missing_sequence_mapping_count"),
        "ledger_close_blocked_by_count_only_canonical_progress_count": summary_u64(&summary, "ledger_close_blocked_by_count_only_canonical_progress_count"),
        "ledger_close_blocked_reason": summary.get("ledger_close_blocked_reason").cloned().unwrap_or_else(|| serde_json::json!("")),
        "ledger_close_writer_runtime_instance": summary.get("ledger_close_writer_runtime_instance").cloned().unwrap_or_else(|| serde_json::json!("")),
        "ledger_close_writer_child_runtime_match": summary.get("ledger_close_writer_child_runtime_match").cloned().unwrap_or_else(|| serde_json::json!(false)),
        "ledger_completed_ranges_sample": summary.get("ledger_completed_ranges_sample").cloned().unwrap_or_else(|| serde_json::json!([])),
        "ack_current_window_start_after_proof_close": summary.get("ack_current_window_start_after_proof_close").cloned().unwrap_or(Value::Null),
        "ledger_candidate_rehydrated_count": summary_u64(&summary, "ledger_candidate_rehydrated_count"),
        "ledger_candidate_empty_but_durable_missing_count": summary_u64(&summary, "ledger_candidate_empty_but_durable_missing_count"),
        "ledger_missing_without_candidate_count": summary_u64(&summary, "ledger_missing_without_candidate_count"),
        "ledger_missing_without_retryable_count": summary_u64(&summary, "ledger_missing_without_retryable_count"),
        "ledger_candidate_count_exceeds_durable_missing_invariant_violation_count": summary_u64(&summary, "ledger_candidate_count_exceeds_durable_missing_invariant_violation_count"),
        "ledger_final_missing_without_durable_missing_count": summary_u64(&summary, "ledger_final_missing_without_durable_missing_count"),
        "ledger_final_missing_candidate_count": summary_u64(&summary, "ledger_final_missing_candidate_count"),
        "ledger_final_missing_candidate_ranges_sample": summary.get("ledger_final_missing_candidate_ranges_sample").cloned().unwrap_or_else(|| serde_json::json!([])),
        "ledger_final_missing_requeued_before_admission_count": summary_u64(&summary, "ledger_final_missing_requeued_before_admission_count"),
        "ledger_final_missing_admitted_count": summary_u64(&summary, "ledger_final_missing_admitted_count"),
        "ledger_final_missing_admitted_ranges_sample": summary.get("ledger_final_missing_admitted_ranges_sample").cloned().unwrap_or_else(|| serde_json::json!([])),
        "ledger_final_missing_candidate_payload_available_count": summary_u64(&summary, "ledger_final_missing_candidate_payload_available_count"),
        "ledger_final_missing_candidate_payload_available_ranges_sample": summary.get("ledger_final_missing_candidate_payload_available_ranges_sample").cloned().unwrap_or_else(|| serde_json::json!([])),
        "ledger_final_missing_candidate_payload_missing_count": summary_u64(&summary, "ledger_final_missing_candidate_payload_missing_count"),
        "ledger_final_missing_candidate_tx_hash_mapping_missing_count": summary_u64(&summary, "ledger_final_missing_candidate_tx_hash_mapping_missing_count"),
        "ledger_final_missing_candidate_raw_tx_build_error_count": summary_u64(&summary, "ledger_final_missing_candidate_raw_tx_build_error_count"),
        "ledger_final_missing_payload_available_selected_count": summary_u64(&summary, "ledger_final_missing_payload_available_selected_count"),
        "ledger_final_missing_payload_available_selected_ranges_sample": summary.get("ledger_final_missing_payload_available_selected_ranges_sample").cloned().unwrap_or_else(|| serde_json::json!([])),
        "ledger_final_missing_payload_available_not_selected_count": summary_u64(&summary, "ledger_final_missing_payload_available_not_selected_count"),
        "ledger_final_missing_payload_available_selection_skipped_reason": summary.get("ledger_final_missing_payload_available_selection_skipped_reason").cloned().unwrap_or_else(|| serde_json::json!("")),
        "ledger_final_missing_selectable_count": summary_u64(&summary, "ledger_final_missing_selectable_count"),
        "ledger_final_missing_selector_input_count": summary_u64(&summary, "ledger_final_missing_selector_input_count"),
        "ledger_final_missing_selector_output_count": summary_u64(&summary, "ledger_final_missing_selector_output_count"),
        "ledger_final_missing_selector_used_durable_bucket": summary.get("ledger_final_missing_selector_used_durable_bucket").and_then(Value::as_bool).unwrap_or(false),
        "ledger_final_missing_selector_skipped_by_old_pending_view_count": summary_u64(&summary, "ledger_final_missing_selector_skipped_by_old_pending_view_count"),
        "ledger_final_missing_selected_not_pushed_to_raw_txs_count": summary_u64(&summary, "ledger_final_missing_selected_not_pushed_to_raw_txs_count"),
        "ledger_final_missing_raw_txs_push_attempt_count": summary_u64(&summary, "ledger_final_missing_raw_txs_push_attempt_count"),
        "ledger_final_missing_raw_txs_push_success_count": summary_u64(&summary, "ledger_final_missing_raw_txs_push_success_count"),
        "ledger_final_missing_raw_txs_nonempty_but_not_submitted_count": summary_u64(&summary, "ledger_final_missing_raw_txs_nonempty_but_not_submitted_count"),
        "ledger_final_missing_batch_blocked_reason": summary.get("ledger_final_missing_batch_blocked_reason").cloned().unwrap_or_else(|| serde_json::json!("")),
        "ledger_final_missing_batch_blocked_by_payload_missing_count": summary_u64(&summary, "ledger_final_missing_batch_blocked_by_payload_missing_count"),
        "ledger_final_missing_batch_blocked_by_stage_filter_count": summary_u64(&summary, "ledger_final_missing_batch_blocked_by_stage_filter_count"),
        "ledger_final_missing_batch_blocked_by_scan_limit_count": summary_u64(&summary, "ledger_final_missing_batch_blocked_by_scan_limit_count"),
        "ledger_final_missing_batch_blocked_by_batch_limit_count": summary_u64(&summary, "ledger_final_missing_batch_blocked_by_batch_limit_count"),
        "ledger_final_missing_batch_blocked_by_raw_tx_build_error_count": summary_u64(&summary, "ledger_final_missing_batch_blocked_by_raw_tx_build_error_count"),
        "ledger_final_missing_batch_blocked_by_tx_hash_mapping_missing_count": summary_u64(&summary, "ledger_final_missing_batch_blocked_by_tx_hash_mapping_missing_count"),
        "ledger_final_missing_batch_blocked_by_payload_available_not_selected_count": summary_u64(&summary, "ledger_final_missing_batch_blocked_by_payload_available_not_selected_count"),
        "ledger_final_missing_batch_blocked_by_selected_not_pushed_count": summary_u64(&summary, "ledger_final_missing_batch_blocked_by_selected_not_pushed_count"),
        "ledger_final_missing_batch_blocked_by_raw_txs_nonempty_not_submitted_count": summary_u64(&summary, "ledger_final_missing_batch_blocked_by_raw_txs_nonempty_not_submitted_count"),
        "ledger_final_missing_batch_blocked_by_batch_not_full_count": summary_u64(&summary, "ledger_final_missing_batch_blocked_by_batch_not_full_count"),
        "ledger_final_missing_batch_blocked_by_no_tick_executed_count": summary_u64(&summary, "ledger_final_missing_batch_blocked_by_no_tick_executed_count"),
        "ledger_final_missing_batch_blocked_by_classification_path_not_reached_count": summary_u64(&summary, "ledger_final_missing_batch_blocked_by_classification_path_not_reached_count"),
        "ledger_final_missing_batch_blocked_by_unknown_invariant_violation_count": summary_u64(&summary, "ledger_final_missing_batch_blocked_by_unknown_invariant_violation_count"),
        "ledger_final_missing_batch_limit_config": summary_u64(&summary, "ledger_final_missing_batch_limit_config"),
        "ledger_final_missing_reserved_batch_budget": summary_u64(&summary, "ledger_final_missing_reserved_batch_budget"),
        "ledger_final_missing_batch_budget_before_fill": summary_u64(&summary, "ledger_final_missing_batch_budget_before_fill"),
        "ledger_final_missing_batch_budget_after_fill": summary_u64(&summary, "ledger_final_missing_batch_budget_after_fill"),
        "ledger_final_missing_batch_blocked_by_limit_after_actual_fill_count": summary_u64(&summary, "ledger_final_missing_batch_blocked_by_limit_after_actual_fill_count"),
        "ledger_final_missing_batch_limit_zero_count": summary_u64(&summary, "ledger_final_missing_batch_limit_zero_count"),
        "ledger_final_missing_preempted_normal_pending_count": summary_u64(&summary, "ledger_final_missing_preempted_normal_pending_count"),
        "ledger_final_missing_batch_nonempty_submitted": summary.get("ledger_final_missing_batch_nonempty_submitted").and_then(Value::as_bool).unwrap_or(false),
        "ledger_final_missing_actual_batch_count": summary_u64(&summary, "ledger_final_missing_actual_batch_count"),
        "ledger_final_missing_actual_batch_ranges_sample": summary.get("ledger_final_missing_actual_batch_ranges_sample").cloned().unwrap_or_else(|| serde_json::json!([])),
        "ledger_final_missing_raw_txs_count": summary_u64(&summary, "ledger_final_missing_raw_txs_count"),
        "ledger_final_missing_batch_result_count": summary_u64(&summary, "ledger_final_missing_batch_result_count"),
        "ledger_final_missing_receipt_written_count": summary_u64(&summary, "ledger_final_missing_receipt_written_count"),
        "ledger_final_missing_receipt_missing_after_admission_count": summary_u64(&summary, "ledger_final_missing_receipt_missing_after_admission_count"),
        "ledger_final_missing_admitted_but_no_receipt_invariant_violation_count": summary_u64(&summary, "ledger_final_missing_admitted_but_no_receipt_invariant_violation_count"),
        "ledger_admission_counter_is_actual_batch": summary.get("ledger_admission_counter_is_actual_batch").and_then(Value::as_bool).unwrap_or(false),
        "ledger_admission_counter_mismatch_reason": summary.get("ledger_admission_counter_mismatch_reason").cloned().unwrap_or_else(|| serde_json::json!("")),
        "novorudp_trace_enabled": summary.get("novorudp_trace_enabled").and_then(Value::as_bool).unwrap_or(false),
        "trace_success_sequences_sample": summary.get("trace_success_sequences_sample").cloned().unwrap_or_else(|| serde_json::json!([])),
        "trace_failed_sequences_sample": summary.get("trace_failed_sequences_sample").cloned().unwrap_or_else(|| serde_json::json!([])),
        "trace_first_divergence_stage": summary.get("trace_first_divergence_stage").cloned().unwrap_or_else(|| serde_json::json!("")),
        "trace_first_divergence_sequence": summary.get("trace_first_divergence_sequence").cloned().unwrap_or(Value::Null),
        "trace_success_vs_failed_diff_summary": summary.get("trace_success_vs_failed_diff_summary").cloned().unwrap_or_else(|| serde_json::json!({})),
        "trace_candidate_payload_available_not_selected_sequences": summary.get("trace_candidate_payload_available_not_selected_sequences").cloned().unwrap_or_else(|| serde_json::json!([])),
        "trace_selected_not_pushed_sequences": summary.get("trace_selected_not_pushed_sequences").cloned().unwrap_or_else(|| serde_json::json!([])),
        "trace_pushed_not_batched_sequences": summary.get("trace_pushed_not_batched_sequences").cloned().unwrap_or_else(|| serde_json::json!([])),
        "trace_batched_not_receipted_sequences": summary.get("trace_batched_not_receipted_sequences").cloned().unwrap_or_else(|| serde_json::json!([])),
        "ledger_final_missing_admission_skipped_count": summary_u64(&summary, "ledger_final_missing_admission_skipped_count"),
        "ledger_final_missing_admission_skip_reason_counts": summary.get("ledger_final_missing_admission_skip_reason_counts").cloned().unwrap_or_else(|| serde_json::json!({})),
        "admission_used_ledger_final_missing_bucket": summary.get("admission_used_ledger_final_missing_bucket").and_then(Value::as_bool).unwrap_or(false),
        "queue_tx_count_last": summary_u64(&summary, "queue_tx_count_last"),
        "queue_active_pending_last": summary_u64(&summary, "queue_active_pending_last"),
        "queue_historical_pending_last": summary_u64(&summary, "queue_historical_pending_last"),
        "queue_seen_last": summary_u64(&summary, "queue_seen_last"),
        "queue_pending_last": summary_u64(&summary, "queue_pending_last"),
        "queue_dropped_last": summary_u64(&summary, "queue_dropped_last"),
        "queue_rejected_last": summary_u64(&summary, "queue_rejected_last"),
        "historical_compacted_total": summary_u64(&summary, "historical_compacted_total"),
        "historical_payload_bytes_freed": summary_u64(&summary, "historical_payload_bytes_freed"),
        "tombstone_retained_count": summary_u64(&summary, "tombstone_retained_count"),
        "tombstone_evicted_count": summary_u64(&summary, "tombstone_evicted_count"),
        "historical_pending_after_compaction": summary_u64(&summary, "historical_pending_after_compaction"),
        "included_retained_after_compaction": summary_u64(&summary, "included_retained_after_compaction"),
        "dropped_retained_after_compaction": summary_u64(&summary, "dropped_retained_after_compaction"),
        "runtime_current_view_bytes_estimate": summary_u64(&summary, "queue_tx_count_last").saturating_mul(256),
        "broadcast_tx_last": summary_u64(&summary, "broadcast_tx_last"),
        "broadcast_candidates_last": summary_u64(&summary, "broadcast_candidates_last"),
        "skipped_ineligible_stage_total": summary_u64(&summary, "skipped_ineligible_stage_total"),
        "skipped_missing_payload_total": summary_u64(&summary, "skipped_missing_payload_total"),
        "skipped_non_native_payload_total": summary_u64(&summary, "skipped_non_native_payload_total"),
        "skipped_chain_mismatch_total": summary_u64(&summary, "skipped_chain_mismatch_total"),
        "skipped_already_receipted_total": summary_u64(&summary, "skipped_already_receipted_total"),
        "max_network_received_per_tick": summary_u64(&summary, "max_network_received_per_tick"),
        "max_queue_admitted_per_tick": summary_u64(&summary, "max_queue_admitted_per_tick"),
        "max_aoem_batch_executed_per_tick": summary_u64(&summary, "max_aoem_batch_executed_per_tick"),
        "max_proof_items_per_tick": summary_u64(&summary, "max_proof_items_per_tick"),
        "max_commit_items_per_tick": summary_u64(&summary, "max_commit_items_per_tick"),
        "max_broadcast_tx_per_tick": summary_u64(&summary, "max_broadcast_tx_per_tick"),
        "native_store_backend": summary.get("native_store_backend").cloned().unwrap_or(Value::Null),
        "native_store_commit_model": summary.get("native_store_commit_model").cloned().unwrap_or(Value::Null),
        "native_store_backend_path": summary.get("native_store_backend_path").cloned().unwrap_or(Value::Null),
        "native_store_precommit_materialized_ticks": summary_u64(&summary, "native_store_precommit_materialized_ticks"),
        "native_store_materialized_receipts_max": summary_u64(&summary, "native_store_materialized_receipts_max"),
        "native_store_materialized_estimated_bytes_max": summary_u64(&summary, "native_store_materialized_estimated_bytes_max"),
        "native_store_previous_clone_receipts_max": summary_u64(&summary, "native_store_previous_clone_receipts_max"),
        "native_store_previous_clone_estimated_bytes_max": summary_u64(&summary, "native_store_previous_clone_estimated_bytes_max"),
        "native_store_materialization_risk_last": summary.get("native_store_materialization_risk_last").cloned().unwrap_or(Value::Null),
        "report_tx_hash_list_len": report_array_len_recursive(&summary),
        "report_receipt_key_list_len": 0,
        "tick_result_omitted": summary.get("tick_result").is_some(),
        "lifecycle_omitted": summary.get("lifecycle").is_some(),
        "raw_runtime_summary_omitted": true,
    })
}

fn compact_sender_report_for_report(mut report: Value) -> Value {
    if let Some(map) = report.as_object_mut() {
        if let Some(sent_by_hash) = map.remove("sent_by_hash") {
            let count = sent_by_hash.as_object().map_or(0, serde_json::Map::len);
            let mut digest = RollingDigestV1::default();
            let mut first = Vec::<Value>::new();
            if let Some(obj) = sent_by_hash.as_object() {
                for (key, value) in obj {
                    digest.update(key.as_bytes());
                    if first.len() < 8 {
                        first.push(serde_json::json!({"tx_hash": key, "count": value}));
                    }
                }
            }
            map.insert(
                "sent_by_hash".to_string(),
                serde_json::json!({
                    "omitted": true,
                    "count": count,
                    "digest": digest.finish_hex(),
                    "samples": first,
                }),
            );
        }
    }
    report
}

fn hex_lower(bytes: &[u8]) -> String {
    let mut out = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        out.push_str(&format!("{byte:02x}"));
    }
    out
}
