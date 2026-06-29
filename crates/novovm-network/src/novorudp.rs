use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::BTreeMap;

pub const NOVORUDP_TRANSPORT_FRAME_V0_MAGIC: &[u8; 8] = b"NOVRUDP0";
pub const NOVORUDP_TRANSPORT_FRAME_V0_VERSION: u16 = 1;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum NovoRudpTransportFrameKindV0 {
    Data,
    Repair,
    Ack,
    Endpoint,
    Done,
}

impl NovoRudpTransportFrameKindV0 {
    const fn code(self) -> u8 {
        match self {
            Self::Data => 1,
            Self::Repair => 2,
            Self::Ack => 3,
            Self::Endpoint => 4,
            Self::Done => 5,
        }
    }

    const fn from_code(code: u8) -> Option<Self> {
        match code {
            1 => Some(Self::Data),
            2 => Some(Self::Repair),
            3 => Some(Self::Ack),
            4 => Some(Self::Endpoint),
            5 => Some(Self::Done),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct NovoRudpTransportFrameV0 {
    pub kind: NovoRudpTransportFrameKindV0,
    pub session_id: [u8; 16],
    pub stream_id: u64,
    pub object_id: u64,
    pub sequence: u64,
    pub ack_epoch: u64,
    pub payload: Vec<u8>,
    pub checksum: [u8; 32],
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum NovoRudpTransportFrameDecodeErrorV0 {
    TooShort { len: usize },
    BadMagic,
    UnsupportedVersion { version: u16 },
    UnknownKind { kind: u8 },
    PayloadTooLarge { len: usize },
    LengthMismatch { expected: usize, actual: usize },
    ChecksumMismatch,
}

impl std::fmt::Display for NovoRudpTransportFrameDecodeErrorV0 {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::TooShort { len } => write!(f, "novorudp frame v0 too short: len={len}"),
            Self::BadMagic => write!(f, "novorudp frame v0 bad magic"),
            Self::UnsupportedVersion { version } => {
                write!(f, "novorudp frame v0 unsupported version: {version}")
            }
            Self::UnknownKind { kind } => write!(f, "novorudp frame v0 unknown kind: {kind}"),
            Self::PayloadTooLarge { len } => {
                write!(f, "novorudp frame v0 payload too large: len={len}")
            }
            Self::LengthMismatch { expected, actual } => write!(
                f,
                "novorudp frame v0 length mismatch: expected={expected} actual={actual}"
            ),
            Self::ChecksumMismatch => write!(f, "novorudp frame v0 checksum mismatch"),
        }
    }
}

impl std::error::Error for NovoRudpTransportFrameDecodeErrorV0 {}

impl NovoRudpTransportFrameV0 {
    pub fn new(
        kind: NovoRudpTransportFrameKindV0,
        session_id: [u8; 16],
        stream_id: u64,
        object_id: u64,
        sequence: u64,
        ack_epoch: u64,
        payload: Vec<u8>,
    ) -> Self {
        let checksum = novorudp_transport_frame_checksum_v0(
            kind,
            &session_id,
            stream_id,
            object_id,
            sequence,
            ack_epoch,
            payload.as_slice(),
        );
        Self {
            kind,
            session_id,
            stream_id,
            object_id,
            sequence,
            ack_epoch,
            payload,
            checksum,
        }
    }

    #[must_use]
    pub fn encode(&self) -> Vec<u8> {
        let checksum = novorudp_transport_frame_checksum_v0(
            self.kind,
            &self.session_id,
            self.stream_id,
            self.object_id,
            self.sequence,
            self.ack_epoch,
            self.payload.as_slice(),
        );
        let payload_len = self.payload.len().min(u32::MAX as usize) as u32;
        let mut out =
            Vec::with_capacity(8 + 2 + 1 + 1 + 16 + 8 + 8 + 8 + 8 + 4 + 32 + self.payload.len());
        out.extend_from_slice(NOVORUDP_TRANSPORT_FRAME_V0_MAGIC);
        out.extend_from_slice(&NOVORUDP_TRANSPORT_FRAME_V0_VERSION.to_le_bytes());
        out.push(self.kind.code());
        out.push(0);
        out.extend_from_slice(&self.session_id);
        out.extend_from_slice(&self.stream_id.to_le_bytes());
        out.extend_from_slice(&self.object_id.to_le_bytes());
        out.extend_from_slice(&self.sequence.to_le_bytes());
        out.extend_from_slice(&self.ack_epoch.to_le_bytes());
        out.extend_from_slice(&payload_len.to_le_bytes());
        out.extend_from_slice(&checksum);
        out.extend_from_slice(&self.payload);
        out
    }

    pub fn decode(bytes: &[u8]) -> Result<Self, NovoRudpTransportFrameDecodeErrorV0> {
        const HEADER_LEN: usize = 8 + 2 + 1 + 1 + 16 + 8 + 8 + 8 + 8 + 4 + 32;
        if bytes.len() < HEADER_LEN {
            return Err(NovoRudpTransportFrameDecodeErrorV0::TooShort { len: bytes.len() });
        }
        if &bytes[..8] != NOVORUDP_TRANSPORT_FRAME_V0_MAGIC {
            return Err(NovoRudpTransportFrameDecodeErrorV0::BadMagic);
        }
        let version = u16::from_le_bytes([bytes[8], bytes[9]]);
        if version != NOVORUDP_TRANSPORT_FRAME_V0_VERSION {
            return Err(NovoRudpTransportFrameDecodeErrorV0::UnsupportedVersion { version });
        }
        let kind_code = bytes[10];
        let kind = NovoRudpTransportFrameKindV0::from_code(kind_code)
            .ok_or(NovoRudpTransportFrameDecodeErrorV0::UnknownKind { kind: kind_code })?;
        let mut offset = 12;
        let mut session_id = [0u8; 16];
        session_id.copy_from_slice(&bytes[offset..offset + 16]);
        offset += 16;
        let stream_id = read_u64_le_v0(bytes, &mut offset);
        let object_id = read_u64_le_v0(bytes, &mut offset);
        let sequence = read_u64_le_v0(bytes, &mut offset);
        let ack_epoch = read_u64_le_v0(bytes, &mut offset);
        let payload_len = u32::from_le_bytes([
            bytes[offset],
            bytes[offset + 1],
            bytes[offset + 2],
            bytes[offset + 3],
        ]) as usize;
        offset += 4;
        let mut checksum = [0u8; 32];
        checksum.copy_from_slice(&bytes[offset..offset + 32]);
        offset += 32;
        let expected = offset.saturating_add(payload_len);
        if expected != bytes.len() {
            return Err(NovoRudpTransportFrameDecodeErrorV0::LengthMismatch {
                expected,
                actual: bytes.len(),
            });
        }
        let payload = bytes[offset..].to_vec();
        let computed = novorudp_transport_frame_checksum_v0(
            kind,
            &session_id,
            stream_id,
            object_id,
            sequence,
            ack_epoch,
            payload.as_slice(),
        );
        if checksum != computed {
            return Err(NovoRudpTransportFrameDecodeErrorV0::ChecksumMismatch);
        }
        Ok(Self {
            kind,
            session_id,
            stream_id,
            object_id,
            sequence,
            ack_epoch,
            payload,
            checksum,
        })
    }
}

fn read_u64_le_v0(bytes: &[u8], offset: &mut usize) -> u64 {
    let value = u64::from_le_bytes([
        bytes[*offset],
        bytes[*offset + 1],
        bytes[*offset + 2],
        bytes[*offset + 3],
        bytes[*offset + 4],
        bytes[*offset + 5],
        bytes[*offset + 6],
        bytes[*offset + 7],
    ]);
    *offset += 8;
    value
}

fn novorudp_transport_frame_checksum_v0(
    kind: NovoRudpTransportFrameKindV0,
    session_id: &[u8; 16],
    stream_id: u64,
    object_id: u64,
    sequence: u64,
    ack_epoch: u64,
    payload: &[u8],
) -> [u8; 32] {
    let mut hasher = Sha256::new();
    hasher.update(b"novorudp-transport-frame-v0");
    hasher.update(NOVORUDP_TRANSPORT_FRAME_V0_MAGIC);
    hasher.update(NOVORUDP_TRANSPORT_FRAME_V0_VERSION.to_le_bytes());
    hasher.update([kind.code()]);
    hasher.update(session_id);
    hasher.update(stream_id.to_le_bytes());
    hasher.update(object_id.to_le_bytes());
    hasher.update(sequence.to_le_bytes());
    hasher.update(ack_epoch.to_le_bytes());
    hasher.update((payload.len() as u64).to_le_bytes());
    hasher.update(payload);
    hasher.finalize().into()
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct NovoRudpNetworkOnlyGateReportV0 {
    pub expected_count: u64,
    pub data_frame_received_count: u64,
    pub repair_frame_received_count: u64,
    pub ack_range_closed: bool,
    pub repair_frame_used_if_missing: bool,
    pub transport_delivered_count: u64,
    pub business_decode_count: u64,
    pub aoem_executed_total: u64,
    pub ledger_completed_count: u64,
}

pub fn novorudp_network_only_gate_v0(
    payloads: &[Vec<u8>],
    initially_lost_sequences: &[u64],
) -> NovoRudpNetworkOnlyGateReportV0 {
    let session_id = [0x42; 16];
    let mut delivered = BTreeMap::<u64, Vec<u8>>::new();
    let mut data_frame_received_count = 0u64;
    let mut repair_frame_received_count = 0u64;
    let lost = initially_lost_sequences
        .iter()
        .copied()
        .collect::<std::collections::BTreeSet<_>>();

    for (sequence, payload) in payloads.iter().enumerate() {
        let sequence = sequence as u64;
        let frame = NovoRudpTransportFrameV0::new(
            NovoRudpTransportFrameKindV0::Data,
            session_id,
            1,
            sequence,
            sequence,
            0,
            payload.clone(),
        );
        let decoded = NovoRudpTransportFrameV0::decode(frame.encode().as_slice())
            .expect("network-only data frame must decode");
        if !lost.contains(&sequence) {
            data_frame_received_count = data_frame_received_count.saturating_add(1);
            delivered.insert(decoded.sequence, decoded.payload);
        }
    }

    for sequence in initially_lost_sequences.iter().copied() {
        let Some(payload) = payloads.get(sequence as usize) else {
            continue;
        };
        let repair = NovoRudpTransportFrameV0::new(
            NovoRudpTransportFrameKindV0::Repair,
            session_id,
            1,
            sequence,
            sequence,
            1,
            payload.clone(),
        );
        let decoded = NovoRudpTransportFrameV0::decode(repair.encode().as_slice())
            .expect("network-only repair frame must decode");
        repair_frame_received_count = repair_frame_received_count.saturating_add(1);
        delivered.insert(decoded.sequence, decoded.payload);
    }

    let transport_delivered_count = delivered.len() as u64;
    NovoRudpNetworkOnlyGateReportV0 {
        expected_count: payloads.len() as u64,
        data_frame_received_count,
        repair_frame_received_count,
        ack_range_closed: transport_delivered_count == payloads.len() as u64,
        repair_frame_used_if_missing: !initially_lost_sequences.is_empty()
            && repair_frame_received_count > 0,
        transport_delivered_count,
        business_decode_count: 0,
        aoem_executed_total: 0,
        ledger_completed_count: 0,
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct NovoRudpRange {
    pub start: u64,
    pub end_inclusive: u64,
}

impl NovoRudpRange {
    pub fn new(start: u64, end_inclusive: u64) -> Self {
        Self {
            start,
            end_inclusive,
        }
    }

    pub fn count(self) -> u64 {
        if self.end_inclusive < self.start {
            0
        } else {
            self.end_inclusive
                .saturating_sub(self.start)
                .saturating_add(1)
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct NovoRudpWindowConfig {
    pub window_size: u64,
    pub packet_copies: u64,
    pub tail_packet_copies: u64,
    pub batch_size: u64,
    pub batch_pause_ms: u64,
    pub ack_wait_ms: u64,
    pub max_window_retries: u64,
    pub no_progress_backoff: bool,
}

impl Default for NovoRudpWindowConfig {
    fn default() -> Self {
        Self {
            window_size: 64,
            packet_copies: 2,
            tail_packet_copies: 3,
            batch_size: 16,
            batch_pause_ms: 10,
            ack_wait_ms: 1000,
            max_window_retries: 8,
            no_progress_backoff: true,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct NovoRudpRepairWindow {
    pub window_id: u64,
    pub range: NovoRudpRange,
    pub missing_ranges: Vec<NovoRudpRange>,
    pub missing_count: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct NovoRudpRepairPlan {
    pub window: NovoRudpRepairWindow,
    pub packet_copies: u64,
    pub batch_size: u64,
    pub batch_pause_ms: u64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum NovoRudpFrameKind {
    Data,
    Ack,
    Repair,
    Close,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct NovoRudpFrameHeader {
    pub version: u16,
    pub kind: NovoRudpFrameKind,
    pub session_id: [u8; 16],
    pub epoch: u64,
    pub sequence: Option<u64>,
    pub window_id: Option<u64>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct NovoRudpAckFrame {
    pub header: NovoRudpFrameHeader,
    pub expected_total: u64,
    pub receiver_done: bool,
    pub missing_count: u64,
    pub current_window: Option<NovoRudpRange>,
    pub current_window_missing_ranges: Vec<NovoRudpRange>,
}

impl NovoRudpAckFrame {
    #[must_use]
    pub fn current_window_missing_count(&self) -> u64 {
        missing_count(self.current_window_missing_ranges.as_slice())
    }

    #[must_use]
    pub fn is_window_complete(&self) -> bool {
        self.current_window_missing_count() == 0
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct NovoRudpPacingProfile {
    pub packet_copies: u64,
    pub tail_packet_copies: u64,
    pub batch_size: u64,
    pub batch_pause_ms: u64,
    pub ack_wait_ms: u64,
    pub no_progress_backoff: bool,
}

impl Default for NovoRudpPacingProfile {
    fn default() -> Self {
        Self {
            packet_copies: 2,
            tail_packet_copies: 6,
            batch_size: 16,
            batch_pause_ms: 10,
            ack_wait_ms: 1000,
            no_progress_backoff: true,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct NovoRudpSenderState {
    pub latest_ack_epoch: u64,
    pub active_window: Option<NovoRudpRange>,
    pub stale_ack_rejected_count: u64,
    pub window_advance_count: u64,
    pub no_progress_count: u64,
}

impl NovoRudpSenderState {
    #[must_use]
    pub fn new() -> Self {
        Self {
            latest_ack_epoch: 0,
            active_window: None,
            stale_ack_rejected_count: 0,
            window_advance_count: 0,
            no_progress_count: 0,
        }
    }
}

impl Default for NovoRudpSenderState {
    fn default() -> Self {
        Self::new()
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum NovoRudpSenderRepairDecision {
    ReceiverDone,
    WindowComplete,
    StaleAck { latest_epoch: u64, ack_epoch: u64 },
    Repair(NovoRudpRepairPlan),
}

pub fn sender_repair_decision_from_ack(
    state: &mut NovoRudpSenderState,
    ack: &NovoRudpAckFrame,
    config: &NovoRudpWindowConfig,
    pacing: &NovoRudpPacingProfile,
) -> NovoRudpSenderRepairDecision {
    if ack.header.epoch <= state.latest_ack_epoch {
        state.stale_ack_rejected_count = state.stale_ack_rejected_count.saturating_add(1);
        return NovoRudpSenderRepairDecision::StaleAck {
            latest_epoch: state.latest_ack_epoch,
            ack_epoch: ack.header.epoch,
        };
    }
    state.latest_ack_epoch = ack.header.epoch;
    if ack.receiver_done && ack.missing_count == 0 {
        state.active_window = None;
        return NovoRudpSenderRepairDecision::ReceiverDone;
    }
    let Some(window) = ack.current_window else {
        state.active_window = None;
        return NovoRudpSenderRepairDecision::WindowComplete;
    };
    let window_missing = normalize_missing_ranges(
        ack.current_window_missing_ranges.as_slice(),
        ack.expected_total,
    )
    .into_iter()
    .filter_map(|range| {
        let start = range.start.max(window.start);
        let end = range.end_inclusive.min(window.end_inclusive);
        (end >= start).then_some(NovoRudpRange::new(start, end))
    })
    .collect::<Vec<_>>();
    let window_missing_count = missing_count(window_missing.as_slice());
    if window_missing_count == 0 {
        state.active_window = Some(window);
        return NovoRudpSenderRepairDecision::WindowComplete;
    }
    if state.active_window.is_some_and(|current| current != window) {
        state.window_advance_count = state.window_advance_count.saturating_add(1);
    }
    state.active_window = Some(window);
    let tail_starts_at = ack.expected_total.saturating_sub(config.window_size.max(1));
    let packet_copies = if window.start >= tail_starts_at {
        pacing
            .tail_packet_copies
            .max(pacing.packet_copies)
            .max(config.tail_packet_copies)
            .max(1)
    } else {
        pacing.packet_copies.max(config.packet_copies).max(1)
    };
    NovoRudpSenderRepairDecision::Repair(NovoRudpRepairPlan {
        window: NovoRudpRepairWindow {
            window_id: window.start / config.window_size.max(1),
            range: window,
            missing_ranges: window_missing,
            missing_count: window_missing_count,
        },
        packet_copies,
        batch_size: pacing.batch_size.max(config.batch_size).max(1),
        batch_pause_ms: pacing.batch_pause_ms.max(config.batch_pause_ms),
    })
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum NovoRudpAckProgress {
    Complete,
    Progress,
    NoProgress,
}

pub fn normalize_missing_ranges(ranges: &[NovoRudpRange], expected: u64) -> Vec<NovoRudpRange> {
    if expected == 0 {
        return Vec::new();
    }
    let max = expected.saturating_sub(1);
    let mut clipped = ranges
        .iter()
        .filter_map(|range| {
            let start = range.start.min(max);
            let end = range.end_inclusive.min(max);
            (end >= start).then_some(NovoRudpRange::new(start, end))
        })
        .collect::<Vec<_>>();
    clipped.sort_by_key(|range| range.start);

    let mut merged = Vec::<NovoRudpRange>::new();
    for range in clipped {
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

pub fn missing_count(ranges: &[NovoRudpRange]) -> u64 {
    ranges
        .iter()
        .fold(0u64, |total, range| total.saturating_add(range.count()))
}

pub fn select_first_missing_window(
    ranges: &[NovoRudpRange],
    expected: u64,
    config: &NovoRudpWindowConfig,
) -> Option<NovoRudpRepairWindow> {
    let window_size = config.window_size.max(1);
    let normalized = normalize_missing_ranges(ranges, expected);
    let first = normalized.first()?;
    let window_start = first.start;
    let window_end = window_start
        .saturating_add(window_size.saturating_sub(1))
        .min(expected.saturating_sub(1));
    let mut window_ranges = Vec::<NovoRudpRange>::new();
    for range in normalized {
        let start = range.start.max(window_start);
        let end = range.end_inclusive.min(window_end);
        if end >= start {
            window_ranges.push(NovoRudpRange::new(start, end));
        }
    }
    let missing_count = missing_count(window_ranges.as_slice());
    (missing_count > 0).then_some(NovoRudpRepairWindow {
        window_id: window_start / window_size,
        range: NovoRudpRange::new(window_start, window_end),
        missing_ranges: window_ranges,
        missing_count,
    })
}

pub fn build_repair_plan(
    ranges: &[NovoRudpRange],
    expected: u64,
    config: &NovoRudpWindowConfig,
) -> Option<NovoRudpRepairPlan> {
    let window = select_first_missing_window(ranges, expected, config)?;
    let tail_starts_at = expected.saturating_sub(config.window_size.max(1));
    let packet_copies = if window.range.start >= tail_starts_at {
        config.tail_packet_copies.max(config.packet_copies).max(1)
    } else {
        config.packet_copies.max(1)
    };
    Some(NovoRudpRepairPlan {
        window,
        packet_copies,
        batch_size: config.batch_size.max(1),
        batch_pause_ms: config.batch_pause_ms,
    })
}

pub fn classify_ack_progress(previous_missing: u64, current_missing: u64) -> NovoRudpAckProgress {
    if current_missing == 0 {
        NovoRudpAckProgress::Complete
    } else if current_missing < previous_missing {
        NovoRudpAckProgress::Progress
    } else {
        NovoRudpAckProgress::NoProgress
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct NovoRudpSequenceLifecycleRecord {
    pub run_id: Option<String>,
    pub session_id: Option<String>,
    pub sequence: u64,
    pub tx_hash: Option<[u8; 32]>,
    pub window_id: Option<u64>,
    pub expected: bool,
    pub received: bool,
    pub accepted: bool,
    pub payload_retained: bool,
    pub pending_active: bool,
    pub admitted_to_aoem: bool,
    pub executed: bool,
    pub receipt_written: bool,
    pub canonical_included: bool,
    pub already_receipted: bool,
    pub duplicate_seen_count: u64,
    pub repair_attempt_count: u64,
    pub last_ack_epoch: Option<u64>,
    pub last_updated_ms: u128,
    pub last_state_reason: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct NovoRudpLifecycleSummary {
    pub expected_count: u64,
    pub missing_count: u64,
    pub final_missing_count: u64,
    pub final_missing_received_count: u64,
    pub final_missing_payload_retained_count: u64,
    pub final_missing_pending_active_count: u64,
    pub final_missing_admitted_count: u64,
    pub final_missing_receipt_count: u64,
    pub final_missing_canonical_count: u64,
    pub final_missing_invariant_violation_count: u64,
    pub final_missing_requeue_required_count: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct NovoRudpSequenceLifecycleLedger {
    pub expected_total: u64,
    pub window_size: u64,
    pub records: BTreeMap<u64, NovoRudpSequenceLifecycleRecord>,
}

impl NovoRudpSequenceLifecycleLedger {
    #[must_use]
    pub fn new(expected_total: u64, window_size: u64) -> Self {
        let mut ledger = Self {
            expected_total,
            window_size: window_size.max(1),
            records: BTreeMap::new(),
        };
        ledger.mark_expected_range(0, expected_total.saturating_sub(1), 0, "init");
        ledger
    }

    pub fn mark_expected_range(
        &mut self,
        start: u64,
        end_inclusive: u64,
        now_ms: u128,
        reason: &str,
    ) {
        if self.expected_total == 0 || end_inclusive < start {
            return;
        }
        let max = self.expected_total.saturating_sub(1);
        for sequence in start.min(max)..=end_inclusive.min(max) {
            let window_size = self.window_size.max(1);
            let record = self.record_mut(sequence, now_ms, reason);
            record.expected = true;
            record.window_id = Some(sequence / window_size);
        }
    }

    pub fn ensure_expected_total(&mut self, expected_total: u64, now_ms: u128, reason: &str) {
        if expected_total == 0 {
            return;
        }
        if self.expected_total < expected_total {
            self.expected_total = expected_total;
        }
        self.mark_expected_range(0, expected_total.saturating_sub(1), now_ms, reason);
    }

    pub fn observe_repair_received(
        &mut self,
        sequence: u64,
        tx_hash: [u8; 32],
        payload_retained: bool,
        now_ms: u128,
    ) {
        let record = self.record_mut(sequence, now_ms, "repair_received");
        if record.received {
            record.duplicate_seen_count = record.duplicate_seen_count.saturating_add(1);
        }
        record.expected = true;
        record.received = true;
        record.accepted = true;
        record.tx_hash = Some(tx_hash);
        record.payload_retained |= payload_retained;
        record.repair_attempt_count = record.repair_attempt_count.saturating_add(1);
    }

    pub fn observe_tx_hash_mapping(
        &mut self,
        sequence: u64,
        tx_hash: [u8; 32],
        now_ms: u128,
        reason: &str,
    ) {
        let record = self.record_mut(sequence, now_ms, reason);
        record.expected = true;
        record.tx_hash = Some(tx_hash);
    }

    pub fn mark_pending_active(&mut self, sequence: u64, now_ms: u128, reason: &str) {
        let record = self.record_mut(sequence, now_ms, reason);
        record.pending_active = true;
        record.already_receipted = false;
    }

    pub fn mark_admitted_to_aoem(&mut self, sequence: u64, now_ms: u128) {
        let record = self.record_mut(sequence, now_ms, "admitted_to_aoem");
        record.admitted_to_aoem = true;
        // Admission is an attempt, not completion. Keep it retryable until receipt.
        if !record.receipt_written && !record.canonical_included {
            record.pending_active = true;
        }
    }

    pub fn mark_receipt_written(&mut self, sequence: u64, now_ms: u128) {
        let record = self.record_mut(sequence, now_ms, "receipt_written");
        record.receipt_written = true;
        record.executed = true;
        record.pending_active = false;
        record.already_receipted = true;
    }

    pub fn mark_canonical_included(&mut self, sequence: u64, now_ms: u128) {
        let record = self.record_mut(sequence, now_ms, "canonical_included");
        record.canonical_included = true;
        record.receipt_written = true;
        record.executed = true;
        record.pending_active = false;
        record.already_receipted = true;
    }

    #[must_use]
    pub fn missing_ranges(&self) -> Vec<NovoRudpRange> {
        let mut ranges = Vec::<NovoRudpRange>::new();
        let mut start = None::<u64>;
        let mut previous = None::<u64>;
        for sequence in 0..self.expected_total {
            let done = self
                .records
                .get(&sequence)
                .is_some_and(|record| record.receipt_written || record.canonical_included);
            if done {
                continue;
            }
            match (start, previous) {
                (Some(_), Some(prev)) if sequence == prev.saturating_add(1) => {
                    previous = Some(sequence);
                }
                (Some(left), Some(prev)) => {
                    ranges.push(NovoRudpRange::new(left, prev));
                    start = Some(sequence);
                    previous = Some(sequence);
                }
                _ => {
                    start = Some(sequence);
                    previous = Some(sequence);
                }
            }
        }
        if let (Some(left), Some(right)) = (start, previous) {
            ranges.push(NovoRudpRange::new(left, right));
        }
        ranges
    }

    #[must_use]
    pub fn ack_missing_bitmap(&self) -> Vec<NovoRudpRange> {
        self.missing_ranges()
    }

    #[must_use]
    pub fn current_window_missing_bitmap(&self) -> Option<NovoRudpRepairWindow> {
        select_first_missing_window(
            self.ack_missing_bitmap().as_slice(),
            self.expected_total,
            &NovoRudpWindowConfig {
                window_size: self.window_size.max(1),
                ..NovoRudpWindowConfig::default()
            },
        )
    }

    #[must_use]
    pub fn final_missing_sequences(&self, final_missing_start: Option<u64>) -> Vec<u64> {
        let Some(start) = final_missing_start else {
            return Vec::new();
        };
        (start..self.expected_total)
            .filter(|sequence| {
                !self
                    .records
                    .get(sequence)
                    .is_some_and(|record| record.receipt_written || record.canonical_included)
            })
            .collect()
    }

    #[must_use]
    pub fn final_missing_with_payload(&self, final_missing_start: Option<u64>) -> Vec<u64> {
        self.final_missing_sequences(final_missing_start)
            .into_iter()
            .filter(|sequence| {
                self.records
                    .get(sequence)
                    .is_some_and(|record| record.payload_retained)
            })
            .collect()
    }

    #[must_use]
    pub fn final_missing_pending_active(&self, final_missing_start: Option<u64>) -> Vec<u64> {
        self.final_missing_sequences(final_missing_start)
            .into_iter()
            .filter(|sequence| {
                self.records
                    .get(sequence)
                    .is_some_and(|record| record.pending_active)
            })
            .collect()
    }

    #[must_use]
    pub fn final_missing_admission_candidates(&self, final_missing_start: Option<u64>) -> Vec<u64> {
        self.admission_buckets(final_missing_start, None)
            .final_missing_repair_pending
    }

    #[must_use]
    pub fn admitted_without_receipt(&self) -> Vec<u64> {
        self.records
            .iter()
            .filter_map(|(sequence, record)| {
                (record.admitted_to_aoem && !record.receipt_written && !record.canonical_included)
                    .then_some(*sequence)
            })
            .collect()
    }

    #[must_use]
    pub fn receipt_missing_after_admission(&self, final_missing_start: Option<u64>) -> Vec<u64> {
        let start = final_missing_start.unwrap_or(0);
        self.admitted_without_receipt()
            .into_iter()
            .filter(|sequence| *sequence >= start)
            .collect()
    }

    #[must_use]
    pub fn final_missing_summary(
        &self,
        final_missing_start: Option<u64>,
    ) -> NovoRudpLifecycleSummary {
        let mut summary = NovoRudpLifecycleSummary {
            expected_count: self.expected_total,
            missing_count: missing_count(self.missing_ranges().as_slice()),
            ..Default::default()
        };
        let Some(start) = final_missing_start else {
            return summary;
        };
        for sequence in start..self.expected_total {
            let record = self.records.get(&sequence);
            let done =
                record.is_some_and(|record| record.receipt_written || record.canonical_included);
            if done {
                if record.is_some_and(|record| record.receipt_written) {
                    summary.final_missing_receipt_count =
                        summary.final_missing_receipt_count.saturating_add(1);
                }
                if record.is_some_and(|record| record.canonical_included) {
                    summary.final_missing_canonical_count =
                        summary.final_missing_canonical_count.saturating_add(1);
                }
                continue;
            }
            summary.final_missing_count = summary.final_missing_count.saturating_add(1);
            if record.is_some_and(|record| record.received) {
                summary.final_missing_received_count =
                    summary.final_missing_received_count.saturating_add(1);
            }
            if record.is_some_and(|record| record.payload_retained) {
                summary.final_missing_payload_retained_count = summary
                    .final_missing_payload_retained_count
                    .saturating_add(1);
            }
            if record.is_some_and(|record| record.pending_active) {
                summary.final_missing_pending_active_count =
                    summary.final_missing_pending_active_count.saturating_add(1);
            }
            if record.is_some_and(|record| record.admitted_to_aoem) {
                summary.final_missing_admitted_count =
                    summary.final_missing_admitted_count.saturating_add(1);
            }
            if record.is_some_and(|record| {
                record.payload_retained && !record.pending_active && !record.receipt_written
            }) {
                summary.final_missing_invariant_violation_count = summary
                    .final_missing_invariant_violation_count
                    .saturating_add(1);
                summary.final_missing_requeue_required_count = summary
                    .final_missing_requeue_required_count
                    .saturating_add(1);
            }
        }
        summary
    }

    #[must_use]
    pub fn admission_buckets(
        &self,
        final_missing_start: Option<u64>,
        current_window: Option<NovoRudpRange>,
    ) -> NovoRudpAdmissionBuckets {
        let mut buckets = NovoRudpAdmissionBuckets::default();
        for record in self.records.values() {
            if !record.pending_active || record.receipt_written || record.canonical_included {
                continue;
            }
            let sequence = record.sequence;
            if final_missing_start.is_some_and(|start| sequence >= start) {
                buckets.final_missing_repair_pending.push(sequence);
            } else if current_window
                .is_some_and(|range| sequence >= range.start && sequence <= range.end_inclusive)
            {
                buckets.current_window_repair_pending.push(sequence);
            } else if record.received || record.payload_retained || record.repair_attempt_count > 0
            {
                buckets.other_repair_pending.push(sequence);
            } else {
                buckets.normal_pending.push(sequence);
            }
        }
        buckets
    }

    fn record_mut(
        &mut self,
        sequence: u64,
        now_ms: u128,
        reason: &str,
    ) -> &mut NovoRudpSequenceLifecycleRecord {
        let window_id = Some(sequence / self.window_size.max(1));
        let record =
            self.records
                .entry(sequence)
                .or_insert_with(|| NovoRudpSequenceLifecycleRecord {
                    sequence,
                    window_id,
                    expected: sequence < self.expected_total,
                    ..Default::default()
                });
        record.last_updated_ms = now_ms;
        record.last_state_reason = Some(reason.to_string());
        record
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct NovoRudpAdmissionBuckets {
    pub final_missing_repair_pending: Vec<u64>,
    pub current_window_repair_pending: Vec<u64>,
    pub other_repair_pending: Vec<u64>,
    pub normal_pending: Vec<u64>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum NovoRudpSemanticCodec {
    RawBytes,
    DictionaryDelta,
    AlgebraicTxIr,
    AoemNativeIr,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct NovoRudpAlgebraicFrame {
    pub codec: NovoRudpSemanticCodec,
    pub schema_version: u32,
    pub basis_id: Option<String>,
    pub operator_id: Option<String>,
    pub params_commitment: [u8; 32],
    pub raw_fallback_hash: Option<[u8; 32]>,
    pub deterministic: bool,
}

impl NovoRudpAlgebraicFrame {
    #[must_use]
    pub fn requires_raw_reconstruction(&self) -> bool {
        matches!(
            self.codec,
            NovoRudpSemanticCodec::RawBytes | NovoRudpSemanticCodec::DictionaryDelta
        )
    }

    #[must_use]
    pub fn can_feed_aoem_directly(&self) -> bool {
        self.deterministic
            && matches!(
                self.codec,
                NovoRudpSemanticCodec::AlgebraicTxIr | NovoRudpSemanticCodec::AoemNativeIr
            )
            && self
                .operator_id
                .as_ref()
                .is_some_and(|op| !op.trim().is_empty())
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum NovoRudpSemanticModulationDecision {
    RawReliableFrame,
    ReversibleDeltaFrame,
    AlgebraicIrFrame,
    AoemNativeIrFrame,
    RejectNondeterministicSemanticFrame,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct NovoRudpSemanticModulationProfile {
    pub allow_raw_bytes: bool,
    pub allow_reversible_delta: bool,
    pub allow_algebraic_ir: bool,
    pub allow_aoem_native_ir: bool,
    pub require_deterministic_commitment: bool,
}

impl Default for NovoRudpSemanticModulationProfile {
    fn default() -> Self {
        Self {
            allow_raw_bytes: true,
            allow_reversible_delta: true,
            allow_algebraic_ir: true,
            allow_aoem_native_ir: true,
            require_deterministic_commitment: true,
        }
    }
}

#[must_use]
pub fn evaluate_semantic_modulation_frame(
    frame: &NovoRudpAlgebraicFrame,
    profile: &NovoRudpSemanticModulationProfile,
) -> NovoRudpSemanticModulationDecision {
    if profile.require_deterministic_commitment && !frame.deterministic {
        return NovoRudpSemanticModulationDecision::RejectNondeterministicSemanticFrame;
    }
    match frame.codec {
        NovoRudpSemanticCodec::RawBytes if profile.allow_raw_bytes => {
            NovoRudpSemanticModulationDecision::RawReliableFrame
        }
        NovoRudpSemanticCodec::DictionaryDelta if profile.allow_reversible_delta => {
            NovoRudpSemanticModulationDecision::ReversibleDeltaFrame
        }
        NovoRudpSemanticCodec::AlgebraicTxIr
            if profile.allow_algebraic_ir && frame.can_feed_aoem_directly() =>
        {
            NovoRudpSemanticModulationDecision::AlgebraicIrFrame
        }
        NovoRudpSemanticCodec::AoemNativeIr
            if profile.allow_aoem_native_ir && frame.can_feed_aoem_directly() =>
        {
            NovoRudpSemanticModulationDecision::AoemNativeIrFrame
        }
        _ => NovoRudpSemanticModulationDecision::RejectNondeterministicSemanticFrame,
    }
}

#[cfg(test)]
mod tests {
    use super::{
        build_repair_plan, classify_ack_progress, evaluate_semantic_modulation_frame,
        normalize_missing_ranges, novorudp_network_only_gate_v0, select_first_missing_window,
        sender_repair_decision_from_ack, NovoRudpAckFrame, NovoRudpAckProgress,
        NovoRudpAlgebraicFrame, NovoRudpFrameHeader, NovoRudpFrameKind, NovoRudpPacingProfile,
        NovoRudpRange, NovoRudpSemanticCodec, NovoRudpSemanticModulationDecision,
        NovoRudpSemanticModulationProfile, NovoRudpSenderRepairDecision, NovoRudpSenderState,
        NovoRudpSequenceLifecycleLedger, NovoRudpTransportFrameDecodeErrorV0,
        NovoRudpTransportFrameKindV0, NovoRudpTransportFrameV0, NovoRudpWindowConfig,
        NOVORUDP_TRANSPORT_FRAME_V0_MAGIC,
    };
    use std::collections::{BTreeMap, BTreeSet};

    #[derive(Debug, Clone)]
    struct HarnessReceiver {
        expected: u64,
        window_size: u64,
        epoch: u64,
        received: BTreeSet<u64>,
        payload_available: BTreeSet<u64>,
        active_pending: BTreeSet<u64>,
        receipt: BTreeSet<u64>,
        admitted: BTreeSet<u64>,
    }

    impl HarnessReceiver {
        fn new(expected: u64, received_until_exclusive: u64, window_size: u64) -> Self {
            let mut received = BTreeSet::new();
            let mut receipt = BTreeSet::new();
            for sequence in 0..received_until_exclusive.min(expected) {
                received.insert(sequence);
                receipt.insert(sequence);
            }
            Self {
                expected,
                window_size,
                epoch: 0,
                received,
                payload_available: BTreeSet::new(),
                active_pending: BTreeSet::new(),
                receipt,
                admitted: BTreeSet::new(),
            }
        }

        fn missing_ranges(&self) -> Vec<NovoRudpRange> {
            let mut ranges = Vec::<NovoRudpRange>::new();
            let mut start = None::<u64>;
            let mut previous = None::<u64>;
            for sequence in 0..self.expected {
                if self.receipt.contains(&sequence) {
                    continue;
                }
                match (start, previous) {
                    (Some(_), Some(prev)) if sequence == prev.saturating_add(1) => {
                        previous = Some(sequence);
                    }
                    (Some(left), Some(prev)) => {
                        ranges.push(NovoRudpRange::new(left, prev));
                        start = Some(sequence);
                        previous = Some(sequence);
                    }
                    _ => {
                        start = Some(sequence);
                        previous = Some(sequence);
                    }
                }
            }
            if let (Some(left), Some(right)) = (start, previous) {
                ranges.push(NovoRudpRange::new(left, right));
            }
            ranges
        }

        fn ack(&mut self) -> NovoRudpAckFrame {
            self.epoch = self.epoch.saturating_add(1);
            let ranges = self.missing_ranges();
            let missing_count = super::missing_count(ranges.as_slice());
            let window = select_first_missing_window(
                ranges.as_slice(),
                self.expected,
                &NovoRudpWindowConfig {
                    window_size: self.window_size,
                    ..NovoRudpWindowConfig::default()
                },
            );
            NovoRudpAckFrame {
                header: NovoRudpFrameHeader {
                    version: 1,
                    kind: NovoRudpFrameKind::Ack,
                    session_id: [0x11; 16],
                    epoch: self.epoch,
                    sequence: None,
                    window_id: window.as_ref().map(|window| window.window_id),
                },
                expected_total: self.expected,
                receiver_done: missing_count == 0,
                missing_count,
                current_window: window.as_ref().map(|window| window.range),
                current_window_missing_ranges: window
                    .map(|window| window.missing_ranges)
                    .unwrap_or_default(),
            }
        }

        fn receive_repair(&mut self, sequence: u64) {
            if sequence >= self.expected || self.receipt.contains(&sequence) {
                return;
            }
            self.received.insert(sequence);
            self.payload_available.insert(sequence);
            self.active_pending.insert(sequence);
        }

        fn admit_active_pending(&mut self, limit: usize) {
            let candidates = self
                .active_pending
                .iter()
                .copied()
                .take(limit)
                .collect::<Vec<_>>();
            for sequence in candidates {
                self.active_pending.remove(&sequence);
                self.admitted.insert(sequence);
                self.receipt.insert(sequence);
            }
        }

        fn cleanup_active_pending(&mut self) {
            self.active_pending.clear();
        }

        fn requeue_payload_without_receipt(&mut self) {
            for sequence in self.payload_available.iter().copied().collect::<Vec<_>>() {
                if !self.receipt.contains(&sequence) {
                    self.active_pending.insert(sequence);
                }
            }
        }
    }

    #[derive(Debug)]
    struct LossyHarnessChannel {
        drops_remaining: BTreeMap<u64, u64>,
    }

    impl LossyHarnessChannel {
        fn with_tail_loss(start: u64, end_inclusive: u64, drops_per_sequence: u64) -> Self {
            let mut drops_remaining = BTreeMap::new();
            for sequence in start..=end_inclusive {
                drops_remaining.insert(sequence, drops_per_sequence);
            }
            Self { drops_remaining }
        }

        fn deliver(&mut self, sequences: &[u64], receiver: &mut HarnessReceiver) {
            for sequence in sequences {
                let remaining = self.drops_remaining.entry(*sequence).or_insert(0);
                if *remaining > 0 {
                    *remaining = remaining.saturating_sub(1);
                    continue;
                }
                receiver.receive_repair(*sequence);
            }
        }
    }

    #[test]
    fn normalizes_and_merges_ranges() {
        let ranges = vec![
            NovoRudpRange::new(10, 12),
            NovoRudpRange::new(13, 20),
            NovoRudpRange::new(90, 120),
        ];
        let normalized = normalize_missing_ranges(ranges.as_slice(), 100);
        assert_eq!(
            normalized,
            vec![NovoRudpRange::new(10, 20), NovoRudpRange::new(90, 99)]
        );
    }

    #[test]
    fn selects_first_missing_window_only() {
        let config = NovoRudpWindowConfig {
            window_size: 64,
            ..NovoRudpWindowConfig::default()
        };
        let ranges = vec![NovoRudpRange::new(14112, 14399)];
        let window =
            select_first_missing_window(ranges.as_slice(), 14400, &config).expect("window");
        assert_eq!(window.window_id, 220);
        assert_eq!(window.range, NovoRudpRange::new(14112, 14175));
        assert_eq!(window.missing_count, 64);
    }

    #[test]
    fn tail_window_uses_tail_packet_copies() {
        let config = NovoRudpWindowConfig {
            window_size: 64,
            packet_copies: 2,
            tail_packet_copies: 6,
            ..NovoRudpWindowConfig::default()
        };
        let plan =
            build_repair_plan(&[NovoRudpRange::new(14360, 14399)], 14400, &config).expect("plan");
        assert_eq!(plan.packet_copies, 6);
    }

    #[test]
    fn ack_progress_classification_is_strict() {
        assert_eq!(classify_ack_progress(100, 0), NovoRudpAckProgress::Complete);
        assert_eq!(
            classify_ack_progress(100, 80),
            NovoRudpAckProgress::Progress
        );
        assert_eq!(
            classify_ack_progress(100, 100),
            NovoRudpAckProgress::NoProgress
        );
    }

    #[test]
    fn sender_repair_decision_uses_receiver_owned_current_window_only() {
        let mut sender = NovoRudpSenderState::new();
        let ack = NovoRudpAckFrame {
            header: NovoRudpFrameHeader {
                version: 1,
                kind: NovoRudpFrameKind::Ack,
                session_id: [0x31; 16],
                epoch: 1,
                sequence: None,
                window_id: Some(220),
            },
            expected_total: 14_400,
            receiver_done: false,
            missing_count: 248,
            current_window: Some(NovoRudpRange::new(14_152, 14_215)),
            current_window_missing_ranges: vec![
                NovoRudpRange::new(14_152, 14_160),
                NovoRudpRange::new(14_300, 14_399),
            ],
        };

        let decision = sender_repair_decision_from_ack(
            &mut sender,
            &ack,
            &NovoRudpWindowConfig::default(),
            &NovoRudpPacingProfile::default(),
        );
        let NovoRudpSenderRepairDecision::Repair(plan) = decision else {
            panic!("expected repair plan");
        };
        assert_eq!(plan.window.range, NovoRudpRange::new(14_152, 14_215));
        assert_eq!(
            plan.window.missing_ranges,
            vec![NovoRudpRange::new(14_152, 14_160)]
        );
        assert!(
            plan.window
                .missing_ranges
                .iter()
                .all(|range| range.end_inclusive <= 14_215),
            "sender must never repair outside receiver-owned current window"
        );
    }

    #[test]
    fn sender_repair_decision_does_not_repair_completed_window() {
        let mut sender = NovoRudpSenderState::new();
        let ack = NovoRudpAckFrame {
            header: NovoRudpFrameHeader {
                version: 1,
                kind: NovoRudpFrameKind::Ack,
                session_id: [0x32; 16],
                epoch: 1,
                sequence: None,
                window_id: Some(220),
            },
            expected_total: 14_400,
            receiver_done: false,
            missing_count: 10,
            current_window: Some(NovoRudpRange::new(14_152, 14_215)),
            current_window_missing_ranges: Vec::new(),
        };

        assert!(matches!(
            sender_repair_decision_from_ack(
                &mut sender,
                &ack,
                &NovoRudpWindowConfig::default(),
                &NovoRudpPacingProfile::default()
            ),
            NovoRudpSenderRepairDecision::WindowComplete
        ));
    }

    #[test]
    fn sender_repair_decision_uses_tail_pacing_profile() {
        let mut sender = NovoRudpSenderState::new();
        let ack = NovoRudpAckFrame {
            header: NovoRudpFrameHeader {
                version: 1,
                kind: NovoRudpFrameKind::Ack,
                session_id: [0x33; 16],
                epoch: 1,
                sequence: None,
                window_id: Some(224),
            },
            expected_total: 14_400,
            receiver_done: false,
            missing_count: 64,
            current_window: Some(NovoRudpRange::new(14_336, 14_399)),
            current_window_missing_ranges: vec![NovoRudpRange::new(14_336, 14_399)],
        };
        let pacing = NovoRudpPacingProfile {
            packet_copies: 2,
            tail_packet_copies: 9,
            batch_size: 8,
            batch_pause_ms: 25,
            ack_wait_ms: 1500,
            no_progress_backoff: true,
        };

        let decision = sender_repair_decision_from_ack(
            &mut sender,
            &ack,
            &NovoRudpWindowConfig::default(),
            &pacing,
        );
        let NovoRudpSenderRepairDecision::Repair(plan) = decision else {
            panic!("expected repair plan");
        };
        assert_eq!(plan.packet_copies, 9);
        assert_eq!(plan.batch_size, 16, "config lower bound remains enforced");
        assert_eq!(plan.batch_pause_ms, 25);
    }

    #[test]
    fn novorudp_window_state_machine_converges_with_tail_loss() {
        let mut receiver = HarnessReceiver::new(14_400, 14_152, 64);
        let mut sender = NovoRudpSenderState::new();
        let mut channel = LossyHarnessChannel::with_tail_loss(14_152, 14_399, 2);
        let config = NovoRudpWindowConfig::default();
        let pacing = NovoRudpPacingProfile::default();

        for _ in 0..128 {
            let ack = receiver.ack();
            if ack.receiver_done {
                break;
            }
            let repair = match sender_repair_decision_from_ack(&mut sender, &ack, &config, &pacing)
            {
                NovoRudpSenderRepairDecision::Repair(plan) => plan
                    .window
                    .missing_ranges
                    .iter()
                    .flat_map(|range| range.start..=range.end_inclusive)
                    .collect::<Vec<_>>(),
                _ => Vec::new(),
            };
            channel.deliver(repair.as_slice(), &mut receiver);
            receiver.admit_active_pending(16);
        }
        while !receiver.active_pending.is_empty() {
            receiver.admit_active_pending(16);
        }

        let final_ack = receiver.ack();
        assert!(final_ack.receiver_done);
        assert_eq!(final_ack.missing_count, 0);
        assert_eq!(receiver.receipt.len() as u64, 14_400);
    }

    #[test]
    fn novorudp_window_state_machine_rejects_stale_ack_progress() {
        let mut sender = NovoRudpSenderState::new();
        let fresh_ack = NovoRudpAckFrame {
            header: NovoRudpFrameHeader {
                version: 1,
                kind: NovoRudpFrameKind::Ack,
                session_id: [0x22; 16],
                epoch: 10,
                sequence: None,
                window_id: Some(221),
            },
            expected_total: 14_400,
            receiver_done: false,
            missing_count: 64,
            current_window: Some(NovoRudpRange::new(14_152, 14_215)),
            current_window_missing_ranges: vec![NovoRudpRange::new(14_152, 14_215)],
        };
        let stale_ack = NovoRudpAckFrame {
            header: NovoRudpFrameHeader {
                version: 1,
                kind: NovoRudpFrameKind::Ack,
                session_id: [0x22; 16],
                epoch: 9,
                sequence: None,
                window_id: Some(58),
            },
            expected_total: 14_400,
            receiver_done: false,
            missing_count: 10_648,
            current_window: Some(NovoRudpRange::new(3_752, 3_815)),
            current_window_missing_ranges: vec![NovoRudpRange::new(3_752, 3_815)],
        };

        assert!(matches!(
            sender_repair_decision_from_ack(
                &mut sender,
                &fresh_ack,
                &NovoRudpWindowConfig::default(),
                &NovoRudpPacingProfile::default()
            ),
            NovoRudpSenderRepairDecision::Repair(_)
        ));
        assert!(matches!(
            sender_repair_decision_from_ack(
                &mut sender,
                &stale_ack,
                &NovoRudpWindowConfig::default(),
                &NovoRudpPacingProfile::default()
            ),
            NovoRudpSenderRepairDecision::StaleAck { .. }
        ));
        assert_eq!(sender.stale_ack_rejected_count, 1);
        assert_eq!(
            sender.active_window,
            Some(NovoRudpRange::new(14_152, 14_215))
        );
    }

    #[test]
    fn novorudp_window_state_machine_does_not_advance_until_window_bitmap_zero() {
        let mut receiver = HarnessReceiver::new(14_400, 14_152, 64);
        let mut sender = NovoRudpSenderState::new();
        let ack = receiver.ack();
        let first_repair = match sender_repair_decision_from_ack(
            &mut sender,
            &ack,
            &NovoRudpWindowConfig::default(),
            &NovoRudpPacingProfile::default(),
        ) {
            NovoRudpSenderRepairDecision::Repair(plan) => plan
                .window
                .missing_ranges
                .iter()
                .flat_map(|range| range.start..=range.end_inclusive)
                .collect::<Vec<_>>(),
            _ => Vec::new(),
        };
        for sequence in first_repair.iter().copied().take(32) {
            receiver.receive_repair(sequence);
        }
        receiver.admit_active_pending(64);

        let second_ack = receiver.ack();
        assert_eq!(
            second_ack.current_window,
            Some(NovoRudpRange::new(14_184, 14_247)),
            "receiver owns the next first-missing window after only partial delivery"
        );
        assert!(second_ack.missing_count > 0);
        assert!(!second_ack.receiver_done);
    }

    #[test]
    fn novorudp_window_state_machine_recovers_payload_after_cleanup() {
        let mut receiver = HarnessReceiver::new(14_400, 14_152, 64);
        receiver.receive_repair(14_152);
        receiver.cleanup_active_pending();
        assert!(receiver.active_pending.is_empty());
        assert!(receiver.payload_available.contains(&14_152));
        assert!(!receiver.receipt.contains(&14_152));

        receiver.requeue_payload_without_receipt();
        assert!(receiver.active_pending.contains(&14_152));
        receiver.admit_active_pending(1);
        assert!(receiver.receipt.contains(&14_152));
    }

    #[test]
    fn novorudp_window_state_machine_completes_14152_14399_tail_gap() {
        let mut receiver = HarnessReceiver::new(14_400, 14_152, 64);
        let mut sender = NovoRudpSenderState::new();
        let mut channel = LossyHarnessChannel::with_tail_loss(14_152, 14_399, 1);
        let config = NovoRudpWindowConfig::default();
        let pacing = NovoRudpPacingProfile::default();

        for _ in 0..96 {
            let ack = receiver.ack();
            if ack.receiver_done {
                break;
            }
            let repair = match sender_repair_decision_from_ack(&mut sender, &ack, &config, &pacing)
            {
                NovoRudpSenderRepairDecision::Repair(plan) => plan
                    .window
                    .missing_ranges
                    .iter()
                    .flat_map(|range| range.start..=range.end_inclusive)
                    .collect::<Vec<_>>(),
                _ => Vec::new(),
            };
            assert!(
                repair.iter().all(|seq| ack
                    .current_window_missing_ranges
                    .iter()
                    .any(|range| *seq >= range.start && *seq <= range.end_inclusive)),
                "sender must only repair receiver-owned current window"
            );
            channel.deliver(repair.as_slice(), &mut receiver);
            receiver.admit_active_pending(8);
            receiver.requeue_payload_without_receipt();
        }
        while !receiver.active_pending.is_empty() {
            receiver.admit_active_pending(8);
        }

        let final_ack = receiver.ack();
        assert_eq!(final_ack.missing_count, 0);
        assert!(final_ack.receiver_done);
        assert_eq!(receiver.receipt.len(), 14_400);
    }

    fn hash_for_sequence(sequence: u64) -> [u8; 32] {
        let mut hash = [0u8; 32];
        hash[..8].copy_from_slice(&sequence.to_le_bytes());
        hash
    }

    #[test]
    fn ledger_final_missing_payload_retained_requires_pending_or_requeue() {
        let mut ledger = NovoRudpSequenceLifecycleLedger::new(14_400, 64);
        ledger.observe_repair_received(14_105, hash_for_sequence(14_105), true, 10);

        let summary = ledger.final_missing_summary(Some(14_105));
        assert_eq!(summary.final_missing_payload_retained_count, 1);
        assert_eq!(summary.final_missing_pending_active_count, 0);
        assert_eq!(summary.final_missing_invariant_violation_count, 1);
        assert_eq!(summary.final_missing_requeue_required_count, 1);

        ledger.mark_pending_active(14_105, 11, "requeue");
        let repaired = ledger.final_missing_summary(Some(14_105));
        assert_eq!(repaired.final_missing_pending_active_count, 1);
        assert_eq!(repaired.final_missing_invariant_violation_count, 0);
    }

    #[test]
    fn ledger_final_missing_pending_active_enters_admission_bucket() {
        let mut ledger = NovoRudpSequenceLifecycleLedger::new(14_400, 64);
        ledger.observe_repair_received(14_105, hash_for_sequence(14_105), true, 10);
        ledger.mark_pending_active(14_105, 11, "final_missing_requeue");
        ledger.observe_repair_received(10, hash_for_sequence(10), true, 12);
        ledger.mark_pending_active(10, 13, "old_repair");

        let buckets =
            ledger.admission_buckets(Some(14_105), Some(NovoRudpRange::new(14_105, 14_168)));
        assert_eq!(buckets.final_missing_repair_pending, vec![14_105]);
        assert_eq!(buckets.other_repair_pending, vec![10]);
    }

    #[test]
    fn ledger_admitted_without_receipt_remains_retryable() {
        let mut ledger = NovoRudpSequenceLifecycleLedger::new(14_400, 64);
        ledger.observe_repair_received(14_106, hash_for_sequence(14_106), true, 10);
        ledger.mark_pending_active(14_106, 11, "requeue");
        ledger.mark_admitted_to_aoem(14_106, 12);

        let record = ledger.records.get(&14_106).expect("record");
        assert!(record.admitted_to_aoem);
        assert!(
            record.pending_active,
            "admitted without receipt must stay retryable"
        );
        assert!(!record.receipt_written);

        ledger.mark_receipt_written(14_106, 13);
        let done = ledger.records.get(&14_106).expect("done");
        assert!(done.receipt_written);
        assert!(!done.pending_active);
    }

    #[test]
    fn ledger_ack_missing_bitmap_derived_from_receipt_state() {
        let mut ledger = NovoRudpSequenceLifecycleLedger::new(8, 4);
        for sequence in 0..4 {
            ledger.mark_receipt_written(sequence, 10 + sequence as u128);
        }
        ledger.observe_repair_received(6, hash_for_sequence(6), true, 20);
        ledger.mark_pending_active(6, 21, "repair");

        assert_eq!(ledger.ack_missing_bitmap(), vec![NovoRudpRange::new(4, 7)]);
        let window = ledger.current_window_missing_bitmap().expect("window");
        assert_eq!(window.range, NovoRudpRange::new(4, 7));
        assert_eq!(window.missing_ranges, vec![NovoRudpRange::new(4, 7)]);
    }

    #[test]
    fn ledger_is_single_source_for_repair_window() {
        let mut ledger = NovoRudpSequenceLifecycleLedger::new(14_400, 64);
        for sequence in 0..14_120 {
            ledger.mark_receipt_written(sequence, 1);
        }
        ledger.observe_repair_received(14_160, hash_for_sequence(14_160), true, 2);
        ledger.mark_pending_active(14_160, 3, "repair");

        let window = ledger.current_window_missing_bitmap().expect("window");
        assert_eq!(window.range, NovoRudpRange::new(14_120, 14_183));
        assert_eq!(window.missing_count, 64);
        assert!(
            window
                .missing_ranges
                .iter()
                .any(|range| range.start == 14_120 && range.end_inclusive == 14_183),
            "current repair window must be derived from ledger receipt state, not received max"
        );
    }

    #[test]
    fn ledger_is_single_source_for_admission_candidates() {
        let mut ledger = NovoRudpSequenceLifecycleLedger::new(14_400, 64);
        ledger.observe_repair_received(14_120, hash_for_sequence(14_120), true, 2);
        ledger.mark_pending_active(14_120, 3, "repair");
        ledger.observe_repair_received(12_000, hash_for_sequence(12_000), true, 2);
        ledger.mark_pending_active(12_000, 3, "old_repair");

        assert_eq!(
            ledger.final_missing_admission_candidates(Some(14_105)),
            vec![14_120]
        );
        assert_eq!(
            ledger.final_missing_pending_active(Some(14_105)),
            vec![14_120]
        );
    }

    #[test]
    fn ledger_receipt_update_closes_sequence() {
        let mut ledger = NovoRudpSequenceLifecycleLedger::new(14_400, 64);
        ledger.observe_repair_received(14_120, hash_for_sequence(14_120), true, 2);
        ledger.mark_pending_active(14_120, 3, "repair");
        ledger.mark_admitted_to_aoem(14_120, 4);
        assert_eq!(
            ledger.receipt_missing_after_admission(Some(14_105)),
            vec![14_120]
        );

        ledger.mark_receipt_written(14_120, 5);
        assert!(ledger
            .receipt_missing_after_admission(Some(14_105))
            .is_empty());
        assert!(!ledger
            .ack_missing_bitmap()
            .iter()
            .any(|range| 14_120 >= range.start && 14_120 <= range.end_inclusive));
    }

    #[test]
    fn ledger_timeout_report_explains_final_missing_layer() {
        let mut ledger = NovoRudpSequenceLifecycleLedger::new(14_400, 64);
        for sequence in 0..14_105 {
            ledger.mark_receipt_written(sequence, 1);
        }
        ledger.observe_repair_received(14_120, hash_for_sequence(14_120), true, 2);
        ledger.mark_pending_active(14_120, 3, "repair");
        ledger.mark_admitted_to_aoem(14_120, 4);

        let summary = ledger.final_missing_summary(Some(14_105));
        assert_eq!(summary.final_missing_count, 295);
        assert_eq!(summary.final_missing_received_count, 1);
        assert_eq!(summary.final_missing_pending_active_count, 1);
        assert_eq!(summary.final_missing_admitted_count, 1);
        assert_eq!(summary.final_missing_receipt_count, 0);
    }

    #[test]
    fn ledger_window_repair_converges_tail_gap_14105_14399() {
        let mut ledger = NovoRudpSequenceLifecycleLedger::new(14_400, 64);
        for sequence in 0..14_105 {
            ledger.mark_receipt_written(sequence, 1);
        }

        for round in 0..16 {
            let missing = ledger.missing_ranges();
            if missing.is_empty() {
                break;
            }
            let window = select_first_missing_window(
                missing.as_slice(),
                14_400,
                &NovoRudpWindowConfig {
                    window_size: 64,
                    ..NovoRudpWindowConfig::default()
                },
            )
            .expect("window");
            for range in window.missing_ranges {
                for sequence in range.start..=range.end_inclusive {
                    if round < 2 && sequence % 3 == 0 {
                        continue;
                    }
                    ledger.observe_repair_received(
                        sequence,
                        hash_for_sequence(sequence),
                        true,
                        10 + round,
                    );
                    ledger.mark_pending_active(sequence, 20 + round, "repair");
                }
            }
            let candidates = ledger.admission_buckets(Some(14_105), Some(window.range));
            for sequence in candidates
                .final_missing_repair_pending
                .into_iter()
                .take(16)
                .collect::<Vec<_>>()
            {
                ledger.mark_admitted_to_aoem(sequence, 30 + round);
                ledger.mark_receipt_written(sequence, 40 + round);
            }
        }

        for _ in 0..64 {
            let candidates = ledger.admission_buckets(Some(14_105), None);
            if candidates.final_missing_repair_pending.is_empty() {
                break;
            }
            for sequence in candidates.final_missing_repair_pending.into_iter().take(16) {
                ledger.mark_admitted_to_aoem(sequence, 100);
                ledger.mark_receipt_written(sequence, 101);
            }
        }

        assert!(ledger.missing_ranges().is_empty());
        let summary = ledger.final_missing_summary(Some(14_105));
        assert_eq!(summary.final_missing_count, 0);
        assert_eq!(summary.missing_count, 0);
    }

    #[test]
    fn semantic_modulation_keeps_raw_and_delta_reversible() {
        let profile = NovoRudpSemanticModulationProfile::default();
        let frame = NovoRudpAlgebraicFrame {
            codec: NovoRudpSemanticCodec::DictionaryDelta,
            schema_version: 1,
            basis_id: Some("nov-tx-dict-v0".to_string()),
            operator_id: None,
            params_commitment: [7; 32],
            raw_fallback_hash: Some([9; 32]),
            deterministic: true,
        };

        assert!(frame.requires_raw_reconstruction());
        assert_eq!(
            evaluate_semantic_modulation_frame(&frame, &profile),
            NovoRudpSemanticModulationDecision::ReversibleDeltaFrame
        );
    }

    #[test]
    fn semantic_modulation_allows_deterministic_aoem_native_ir() {
        let profile = NovoRudpSemanticModulationProfile::default();
        let frame = NovoRudpAlgebraicFrame {
            codec: NovoRudpSemanticCodec::AoemNativeIr,
            schema_version: 1,
            basis_id: Some("aoem-algebraic-basis-v0".to_string()),
            operator_id: Some("nov.transfer_batch".to_string()),
            params_commitment: [3; 32],
            raw_fallback_hash: None,
            deterministic: true,
        };

        assert!(frame.can_feed_aoem_directly());
        assert_eq!(
            evaluate_semantic_modulation_frame(&frame, &profile),
            NovoRudpSemanticModulationDecision::AoemNativeIrFrame
        );
    }

    #[test]
    fn semantic_modulation_rejects_nondeterministic_ai_frame() {
        let profile = NovoRudpSemanticModulationProfile::default();
        let frame = NovoRudpAlgebraicFrame {
            codec: NovoRudpSemanticCodec::AlgebraicTxIr,
            schema_version: 1,
            basis_id: Some("ai-generated-template".to_string()),
            operator_id: Some("nov.transfer_batch".to_string()),
            params_commitment: [5; 32],
            raw_fallback_hash: None,
            deterministic: false,
        };

        assert_eq!(
            evaluate_semantic_modulation_frame(&frame, &profile),
            NovoRudpSemanticModulationDecision::RejectNondeterministicSemanticFrame
        );
    }

    #[test]
    fn transport_frame_v0_roundtrips_all_transport_kinds_without_business_envelope() {
        for (index, kind) in [
            NovoRudpTransportFrameKindV0::Data,
            NovoRudpTransportFrameKindV0::Repair,
            NovoRudpTransportFrameKindV0::Ack,
            NovoRudpTransportFrameKindV0::Endpoint,
            NovoRudpTransportFrameKindV0::Done,
        ]
        .into_iter()
        .enumerate()
        {
            let payload = format!("opaque-payload-{index}").into_bytes();
            let frame = NovoRudpTransportFrameV0::new(
                kind,
                [index as u8; 16],
                7,
                100 + index as u64,
                200 + index as u64,
                300 + index as u64,
                payload.clone(),
            );

            let encoded = frame.encode();
            assert_eq!(&encoded[..8], NOVORUDP_TRANSPORT_FRAME_V0_MAGIC);
            let decoded =
                NovoRudpTransportFrameV0::decode(encoded.as_slice()).expect("decode frame");

            assert_eq!(decoded.kind, kind);
            assert_eq!(decoded.stream_id, 7);
            assert_eq!(decoded.object_id, 100 + index as u64);
            assert_eq!(decoded.sequence, 200 + index as u64);
            assert_eq!(decoded.ack_epoch, 300 + index as u64);
            assert_eq!(decoded.payload, payload);
        }
    }

    #[test]
    fn transport_frame_v0_rejects_payload_tamper() {
        let frame = NovoRudpTransportFrameV0::new(
            NovoRudpTransportFrameKindV0::Data,
            [0x11; 16],
            1,
            2,
            3,
            4,
            b"opaque".to_vec(),
        );
        let mut encoded = frame.encode();
        let last = encoded.last_mut().expect("last byte");
        *last ^= 0xff;

        assert_eq!(
            NovoRudpTransportFrameV0::decode(encoded.as_slice()),
            Err(NovoRudpTransportFrameDecodeErrorV0::ChecksumMismatch)
        );
    }

    #[test]
    fn network_only_gate_v0_closes_transport_without_business_or_aoem() {
        let payloads = (0..128)
            .map(|sequence| format!("opaque-object-{sequence}").into_bytes())
            .collect::<Vec<_>>();
        let report = novorudp_network_only_gate_v0(&payloads, &[9, 10, 63, 127]);

        assert_eq!(report.expected_count, 128);
        assert_eq!(report.data_frame_received_count, 124);
        assert_eq!(report.repair_frame_received_count, 4);
        assert!(report.ack_range_closed);
        assert!(report.repair_frame_used_if_missing);
        assert_eq!(report.transport_delivered_count, 128);
        assert_eq!(report.business_decode_count, 0);
        assert_eq!(report.aoem_executed_total, 0);
        assert_eq!(report.ledger_completed_count, 0);
    }
}
