//! Opt-in, bounded local evidence. No payloads, addresses, identity keys, log
//! writer, task, timer or changes to transport acceptance/retry decisions.
use serde::Serialize;
use sha2::{Digest, Sha256};
use std::{collections::VecDeque, sync::Mutex, time::Instant};

#[derive(Clone, Copy, Debug, thiserror::Error)]
#[repr(usize)]
pub enum NativeRejectReasonV1 {
    #[error("wire budget")]
    WireBudget,
    #[error("oversized datagram")]
    Oversize,
    #[error("handshake rejected")]
    Handshake,
    #[error("unknown source")]
    UnknownSource,
    #[error("no active session")]
    NoSession,
    #[error("secure route rejected")]
    SecureRoute,
    #[error("secure nonce rejected")]
    SecureNonce,
    #[error("secure replay rejected before authentication")]
    SecureReplay,
    #[error("secure authentication rejected")]
    SecureAuthentication,
    #[error("wire or frame decode rejected")]
    WireOrFrame,
    #[error("authenticated stream denied")]
    StreamDenied,
    #[error("unclassified open error")]
    OpenOther,
    #[error("unknown object stream")]
    UnknownStream,
    #[error("short object ACK")]
    AckShort,
    #[error("object ACK rejected")]
    ObjectAck,
    #[error("object fragment rejected")]
    ObjectFragment,
}
#[derive(Clone, Copy, Debug, Default, Serialize)]
pub struct NativeRejectCountsV1 {
    pub counts: [u64; 16],
}
impl NativeRejectCountsV1 {
    pub fn record(&mut self, reason: NativeRejectReasonV1) {
        let value = &mut self.counts[reason as usize];
        *value = value.saturating_add(1);
    }
    pub fn total(&self) -> u64 {
        self.counts.iter().fold(0u64, |a, b| a.saturating_add(*b))
    }
    pub fn merge(&mut self, other: Self) {
        for (a, b) in self.counts.iter_mut().zip(other.counts) {
            *a = a.saturating_add(b);
        }
    }
}

pub fn transfer_tag_v1(session: [u8; 16], stream: u64, object: [u8; 32]) -> [u8; 32] {
    // The fresh authenticated session is the per-session random salt. A tag
    // cannot link the same object across sessions/nodes or reveal a stable ID.
    let mut hash = Sha256::new();
    hash.update(b"novonet-local-transfer-evidence-v1");
    hash.update(session);
    hash.update(stream.to_be_bytes());
    hash.update(object);
    hash.finalize().into()
}

#[derive(Clone, Copy)]
pub enum NativeProgressStageV1 {
    SendAdmitted,
    Fragment,
    Hash,
    Verified,
    WorkStarted,
    WorkFinished,
    ResponseQueued,
    AckProgress,
    TransportDone,
    ResponseValidated,
    StoredReceiptCreated,
}
#[derive(Clone, Debug, Serialize)]
pub struct NativeTransferEvidenceV1 {
    pub tag: [u8; 32],
    pub first_seen_us: u64,
    pub last_progress_us: u64,
    pub transport_done_us: Option<u64>,
    pub response_validated_us: Option<u64>,
    pub fragments: u64,
    pub assemblies_started: u64,
    pub assembly_start_observed: bool,
    /// Cumulative over the retained tag, unlike the current assembly's fragments.
    pub duplicate_data: u64,
    pub duplicate_repair: u64,
    /// Shape-checked completed-history hits, not a bytewise payload comparison.
    pub completed_history_hits: u64,
    pub total_fragments: Option<u64>,
    pub first_fragment_us: Option<u64>,
    pub last_fragment_us: Option<u64>,
    pub fragments_complete_us: Option<u64>,
    pub last_fragment_index: Option<u64>,
    pub repair_fragments: u64,
    pub hash_chunks: u64,
    pub first_hash_us: Option<u64>,
    pub last_hash_us: Option<u64>,
    pub send_admitted_us: Option<u64>,
    /// New sender jobs observed while this object record is retained. A live
    /// duplicate enqueue does not start another attempt; eviction loses history.
    pub send_attempts_observed: u64,
    pub send_terminal_us: Option<u64>,
    pub send_terminal_state: Option<&'static str>,
    pub send: Option<NativeSendEvidenceV1>,
    pub feedback_sent: NativeFeedbackEvidenceV1,
    pub feedback_received: NativeFeedbackEvidenceV1,
    pub acknowledged_best: u64,
    pub verified_us: Option<u64>,
    pub work_entered_us: Option<u64>,
    pub work_finished_us: Option<u64>,
    pub stored_receipt_created_us: Option<u64>,
    pub response_queued_us: Option<u64>,
    pub response_tag: Option<[u8; 32]>,
}
#[derive(Clone, Debug, Default, Serialize)]
pub struct NativeFeedbackEvidenceV1 {
    pub packets: u64,
    pub first_us: Option<u64>,
    pub last_us: Option<u64>,
    pub max_gap_us: u64,
    pub omitted_windows: u64,
    // One sample per 64-position advancement, plus the terminal ACK. Bounds
    // remain independent of the number of feedback packets during loss.
    pub windows: Vec<NativeFeedbackWindowV1>,
}
#[derive(Clone, Debug, Serialize)]
pub struct NativeFeedbackWindowV1 {
    pub at_us: u64,
    pub epoch: u64,
    pub missing: u64,
    pub first_missing: Option<u64>,
}
/// Actual completed carrier writes, not queue admission or prepared frames.
#[derive(Clone, Debug, Serialize)]
pub struct NativeSendEvidenceV1 {
    pub polls: NativeSendPollEvidenceV1,
    pub initial_packets: u64,
    pub repair_packets: u64,
    pub probe_packets: u64,
    pub first_initial_us: Option<u64>,
    pub last_initial_us: Option<u64>,
    pub initial_complete_us: Option<u64>,
    pub first_repair_us: Option<u64>,
    pub first_repair_index: Option<u64>,
    pub repair_retry_passes: u64,
    pub first_repair_retry_us: Option<u64>,
    pub first_repair_retry_index: Option<u64>,
    pub last_repair_us: Option<u64>,
    pub first_probe_us: Option<u64>,
    pub last_probe_us: Option<u64>,
    pub last_repair_index: Option<u64>,
}
#[derive(Clone, Copy, Debug, Default, Serialize)]
pub struct NativeSendPollEvidenceV1 {
    pub calls: u64,
    pub offered_budget: u64,
    pub initial_calls: u64,
    pub initial_max_gap_us: u64,
    pub pacing_wait: u64,
    pub feedback_wait: u64,
    pub backoff_wait: u64,
    pub send_blocked: u64,
    pub udp_would_block: u64,
    pub batch_limited: u64,
}

#[derive(Clone, Debug, Default, Serialize)]
pub struct NativeLinkPollEvidenceV1 {
    pub active_calls: u64,
    pub offered_budget: u64,
    pub reported_data_writes: u64,
    pub jobs_visited: u64,
    pub jobs_unvisited: u64,
    pub budget_exhausted: u64,
    pub deferred_yield: u64,
    pub visit_cap: u64,
    pub one_pass_end: u64,
}
/// Counts continue after object handoff and are not truncated by object eviction.
#[derive(Clone, Copy, Debug, Default, Serialize)]
pub struct NativeReceiveTotalsV1 {
    pub accepted_data: u64,
    pub accepted_repair: u64,
    pub duplicate_data: u64,
    pub duplicate_repair: u64,
    pub history_data: u64,
    pub history_repair: u64,
}
/// Retained only by an explicitly observed sender. Publishing once per poll
/// avoids taking the observer lock for every datagram in that send quantum.
#[derive(Clone, Copy, Debug, Default)]
pub(crate) struct NativeSendProgressV1 {
    pub polls: NativeSendPollEvidenceV1,
    pub last_poll: Option<Instant>,
    pub initial_packets: u64,
    pub repair_packets: u64,
    pub probe_packets: u64,
    pub first_initial: Option<Instant>,
    pub last_initial: Option<Instant>,
    pub initial_complete: Option<Instant>,
    pub first_repair: Option<Instant>,
    pub first_repair_index: Option<u64>,
    pub repair_retry_passes: u64,
    pub first_repair_retry: Option<Instant>,
    pub first_repair_retry_index: Option<u64>,
    pub last_repair: Option<Instant>,
    pub first_probe: Option<Instant>,
    pub last_probe: Option<Instant>,
    pub last_repair_index: Option<u64>,
}
#[derive(Clone, Debug, Serialize)]
pub struct NativeEvidenceSnapshotV1 {
    pub scope: &'static str,
    pub elapsed_us: u64,
    pub rejection_names: [&'static str; 16],
    pub rejections: NativeRejectCountsV1,
    pub first_unattributed_reject_us: Option<u64>,
    pub last_unattributed_reject_us: Option<u64>,
    pub omitted_records: u64,
    pub evicted_records: u64,
    pub expired_records: u64,
    pub objects: Vec<NativeTransferEvidenceV1>,
    /// All link lanes, independent of the per-object stream filter.
    pub send_polls: NativeLinkPollEvidenceV1,
    pub receive_totals: NativeReceiveTotalsV1,
}
#[derive(Clone, Copy, Debug, Serialize)]
pub struct NativeEvidenceRetentionV1 {
    pub retained_records: usize,
    pub omitted_records: u64,
    pub evicted_records: u64,
    pub expired_records: u64,
}
struct State {
    snapshot: NativeEvidenceSnapshotV1,
    records: VecDeque<NativeTransferEvidenceV1>,
}
pub struct NativeTransferDiagnosticsV1 {
    origin: Instant,
    stream_filter: Option<u64>,
    state: Mutex<State>,
}
impl State {
    fn expire(&mut self, at: u64) {
        let before = self.records.len();
        self.records
            .retain(|r| at.saturating_sub(r.first_seen_us) < 120_000_000);
        self.snapshot.expired_records = self
            .snapshot
            .expired_records
            .saturating_add((before - self.records.len()) as u64);
    }
    fn record(&mut self, tag: [u8; 32], at: u64) -> Option<&mut NativeTransferEvidenceV1> {
        self.expire(at);
        let index = match self.records.iter().position(|r| r.tag == tag) {
            Some(index) => index,
            None => {
                if self.records.len() == 256 {
                    if let Some(index) = self.records.iter().position(|r| {
                        r.transport_done_us.is_some()
                            || r.send_terminal_us.is_some()
                            || r.response_validated_us.is_some()
                            || r.response_tag.is_some_and(|tag| {
                                self.records.iter().any(|response| {
                                    response.tag == tag && response.transport_done_us.is_some()
                                })
                            })
                    }) {
                        self.records.remove(index);
                        self.snapshot.evicted_records =
                            self.snapshot.evicted_records.saturating_add(1);
                    } else {
                        self.snapshot.omitted_records =
                            self.snapshot.omitted_records.saturating_add(1);
                        return None;
                    }
                }
                self.records.push_back(NativeTransferEvidenceV1 {
                    tag,
                    first_seen_us: at,
                    last_progress_us: at,
                    transport_done_us: None,
                    response_validated_us: None,
                    fragments: 0,
                    assemblies_started: 0,
                    assembly_start_observed: false,
                    duplicate_data: 0,
                    duplicate_repair: 0,
                    completed_history_hits: 0,
                    total_fragments: None,
                    first_fragment_us: None,
                    last_fragment_us: None,
                    fragments_complete_us: None,
                    last_fragment_index: None,
                    repair_fragments: 0,
                    hash_chunks: 0,
                    first_hash_us: None,
                    last_hash_us: None,
                    send_admitted_us: None,
                    send_attempts_observed: 0,
                    send_terminal_us: None,
                    send_terminal_state: None,
                    send: None,
                    feedback_sent: NativeFeedbackEvidenceV1::default(),
                    feedback_received: NativeFeedbackEvidenceV1::default(),
                    acknowledged_best: 0,
                    verified_us: None,
                    work_entered_us: None,
                    work_finished_us: None,
                    stored_receipt_created_us: None,
                    response_queued_us: None,
                    response_tag: None,
                });
                self.records.len() - 1
            }
        };
        self.records.get_mut(index)
    }
}
impl NativeTransferDiagnosticsV1 {
    pub fn new() -> Self {
        Self { origin: Instant::now(), stream_filter: None, state: Mutex::new(State {
            snapshot: NativeEvidenceSnapshotV1 {
                scope: "local monotonic clock; rejection association unknown; bounded test evidence",
                elapsed_us: 0,
                rejection_names: ["wire_budget","oversize","handshake","unknown_source","no_session","secure_route","secure_nonce","secure_replay_pre_auth","secure_authentication","wire_or_frame","stream_denied","open_other","unknown_stream","ack_short","object_ack","object_fragment"],
                rejections: NativeRejectCountsV1::default(), first_unattributed_reject_us: None,
                last_unattributed_reject_us: None, omitted_records: 0,
                evicted_records: 0, expired_records: 0, objects: Vec::new(),
                send_polls: NativeLinkPollEvidenceV1::default(),
                receive_totals: NativeReceiveTotalsV1::default(),
            }, records: VecDeque::new(),
        }) }
    }
    /// Scope a local observer before installation; excluded lanes perform no
    /// tag hashing or record work and do not consume the fixed record budget.
    pub fn for_stream(stream: u64) -> Self {
        Self {
            stream_filter: Some(stream),
            ..Self::new()
        }
    }
    pub(crate) fn tracks_stream(&self, stream: u64) -> bool {
        self.stream_filter.is_none_or(|selected| selected == stream)
    }
    pub fn elapsed_us(&self, now: Instant) -> u64 {
        self.micros(now)
    }
    fn micros(&self, now: Instant) -> u64 {
        now.saturating_duration_since(self.origin)
            .as_micros()
            .min(u64::MAX as u128) as u64
    }
    pub fn reject(&self, counts: NativeRejectCountsV1, now: Instant) {
        if counts.total() == 0 {
            return;
        }
        if let Ok(mut s) = self.state.lock() {
            let at = self.micros(now);
            s.expire(at);
            s.snapshot.first_unattributed_reject_us.get_or_insert(at);
            s.snapshot.last_unattributed_reject_us = Some(at);
            s.snapshot.rejections.merge(counts);
        }
    }
    pub(crate) fn send_poll(
        &self,
        queued: usize,
        visited: usize,
        budget: usize,
        written: usize,
        deferred: bool,
    ) {
        if queued == 0 {
            return;
        }
        let Ok(mut s) = self.state.lock() else { return };
        let p = &mut s.snapshot.send_polls;
        p.active_calls = p.active_calls.saturating_add(1);
        p.offered_budget = p.offered_budget.saturating_add(budget as u64);
        p.reported_data_writes = p.reported_data_writes.saturating_add(written as u64);
        p.jobs_visited = p.jobs_visited.saturating_add(visited as u64);
        p.jobs_unvisited = p
            .jobs_unvisited
            .saturating_add(queued.saturating_sub(visited) as u64);
        if written == budget {
            p.budget_exhausted = p.budget_exhausted.saturating_add(1);
        } else if deferred && written > 0 {
            p.deferred_yield = p.deferred_yield.saturating_add(1);
        } else if visited < queued {
            p.visit_cap = p.visit_cap.saturating_add(1);
        } else {
            p.one_pass_end = p.one_pass_end.saturating_add(1);
        }
    }
    pub(crate) fn progress(
        &self,
        tag: [u8; 32],
        stage: NativeProgressStageV1,
        amount: u64,
        now: Instant,
    ) {
        let Ok(mut s) = self.state.lock() else {
            return;
        };
        let at = self.micros(now);
        // Evidence retention never prolongs transport/business TTL or custody.
        let Some(r) = s.record(tag, at) else { return };
        if matches!(stage, NativeProgressStageV1::AckProgress) && amount <= r.acknowledged_best {
            return;
        }
        r.last_progress_us = at;
        match stage {
            NativeProgressStageV1::SendAdmitted => {
                r.send_attempts_observed = r.send_attempts_observed.saturating_add(1);
                r.send_admitted_us = Some(at);
                r.send_terminal_us = None;
                r.send_terminal_state = None;
                r.send = None;
                r.feedback_received = NativeFeedbackEvidenceV1::default();
                r.acknowledged_best = 0;
                r.transport_done_us = None;
                r.total_fragments.get_or_insert(amount);
            }
            NativeProgressStageV1::Fragment => r.fragments = r.fragments.saturating_add(1),
            NativeProgressStageV1::Hash => {
                r.hash_chunks = r.hash_chunks.saturating_add(1);
                r.first_hash_us.get_or_insert(at);
                r.last_hash_us = Some(at);
            }
            NativeProgressStageV1::Verified => {
                r.verified_us.get_or_insert(at);
            }
            NativeProgressStageV1::WorkStarted => {
                r.work_entered_us.get_or_insert(at);
            }
            NativeProgressStageV1::WorkFinished => {
                r.work_finished_us.get_or_insert(at);
            }
            NativeProgressStageV1::ResponseQueued => {
                r.response_queued_us.get_or_insert(at);
            }
            NativeProgressStageV1::AckProgress => r.acknowledged_best = amount,
            NativeProgressStageV1::TransportDone => {
                r.transport_done_us.get_or_insert(at);
            }
            NativeProgressStageV1::ResponseValidated => {
                r.response_validated_us.get_or_insert(at);
            }
            NativeProgressStageV1::StoredReceiptCreated => {
                r.stored_receipt_created_us.get_or_insert(at);
            }
        }
    }
    pub(crate) fn fragment(
        &self,
        tag: [u8; 32],
        observation: super::fragments::FragmentObservation,
        index: u64,
        repair: bool,
        now: Instant,
    ) {
        let Ok(mut s) = self.state.lock() else { return };
        let at = self.micros(now);
        use super::fragments::FragmentObservation;
        let totals = &mut s.snapshot.receive_totals;
        let count = match (observation, repair) {
            (FragmentObservation::Accepted { .. }, false) => &mut totals.accepted_data,
            (FragmentObservation::Accepted { .. }, true) => &mut totals.accepted_repair,
            (FragmentObservation::Duplicate { completed: false }, false) => {
                &mut totals.duplicate_data
            }
            (FragmentObservation::Duplicate { completed: false }, true) => {
                &mut totals.duplicate_repair
            }
            (FragmentObservation::Duplicate { completed: true }, false) => &mut totals.history_data,
            (FragmentObservation::Duplicate { completed: true }, true) => {
                &mut totals.history_repair
            }
        };
        *count = count.saturating_add(1);
        let Some(r) = s.record(tag, at) else { return };
        let super::fragments::FragmentObservation::Accepted {
            started,
            received,
            total,
        } = observation
        else {
            if matches!(
                observation,
                super::fragments::FragmentObservation::Duplicate { completed: true }
            ) {
                r.completed_history_hits = r.completed_history_hits.saturating_add(1);
            } else if repair {
                r.duplicate_repair = r.duplicate_repair.saturating_add(1);
            } else {
                r.duplicate_data = r.duplicate_data.saturating_add(1);
            }
            return;
        };
        if started {
            r.assemblies_started = r.assemblies_started.saturating_add(1);
            r.assembly_start_observed = true;
            r.first_fragment_us = None;
            r.fragments_complete_us = None;
            r.repair_fragments = 0;
            r.hash_chunks = 0;
            r.first_hash_us = None;
            r.last_hash_us = None;
            r.verified_us = None;
        }
        r.last_progress_us = at;
        r.fragments = received as u64;
        r.total_fragments = Some(total as u64);
        r.first_fragment_us.get_or_insert(at);
        r.last_fragment_us = Some(at);
        r.last_fragment_index = Some(index);
        r.repair_fragments = r.repair_fragments.saturating_add(u64::from(repair));
        if received == total {
            r.fragments_complete_us.get_or_insert(at);
        }
    }
    pub(crate) fn send_progress(&self, tag: [u8; 32], p: NativeSendProgressV1, now: Instant) {
        let Ok(mut s) = self.state.lock() else { return };
        let Some(r) = s.record(tag, self.micros(now)) else {
            return;
        };
        if let Some(last) = [p.last_initial, p.last_repair, p.last_probe]
            .into_iter()
            .flatten()
            .max()
        {
            r.last_progress_us = r.last_progress_us.max(self.micros(last));
        }
        r.send = Some(NativeSendEvidenceV1 {
            polls: p.polls,
            initial_packets: p.initial_packets,
            repair_packets: p.repair_packets,
            probe_packets: p.probe_packets,
            first_initial_us: p.first_initial.map(|at| self.micros(at)),
            last_initial_us: p.last_initial.map(|at| self.micros(at)),
            initial_complete_us: p.initial_complete.map(|at| self.micros(at)),
            first_repair_us: p.first_repair.map(|at| self.micros(at)),
            first_repair_index: p.first_repair_index,
            repair_retry_passes: p.repair_retry_passes,
            first_repair_retry_us: p.first_repair_retry.map(|at| self.micros(at)),
            first_repair_retry_index: p.first_repair_retry_index,
            last_repair_us: p.last_repair.map(|at| self.micros(at)),
            first_probe_us: p.first_probe.map(|at| self.micros(at)),
            last_probe_us: p.last_probe.map(|at| self.micros(at)),
            last_repair_index: p.last_repair_index,
        });
    }
    pub(crate) fn send_finished(&self, tag: [u8; 32], state: &'static str, now: Instant) {
        let Ok(mut s) = self.state.lock() else { return };
        let at = self.micros(now);
        let Some(r) = s.record(tag, at) else { return };
        r.send_terminal_us.get_or_insert(at);
        r.send_terminal_state.get_or_insert(state);
    }
    pub(crate) fn feedback(
        &self,
        tag: [u8; 32],
        sent: bool,
        epoch: u64,
        missing: u64,
        first_missing: Option<u64>,
        now: Instant,
    ) {
        let Ok(mut s) = self.state.lock() else { return };
        let at = self.micros(now);
        let Some(r) = s.record(tag, at) else { return };
        let f = if sent {
            &mut r.feedback_sent
        } else {
            &mut r.feedback_received
        };
        f.packets += 1;
        f.first_us.get_or_insert(at);
        if let Some(previous) = f.last_us {
            f.max_gap_us = f.max_gap_us.max(at.saturating_sub(previous));
        }
        f.last_us = Some(at);
        if f.windows
            .last()
            .is_none_or(|w| w.first_missing.map(|n| n / 64) != first_missing.map(|n| n / 64))
        {
            if f.windows.len() < 256 {
                f.windows.push(NativeFeedbackWindowV1 {
                    at_us: at,
                    epoch,
                    missing,
                    first_missing,
                });
            } else {
                f.omitted_windows += 1;
            }
        }
    }
    #[cfg(feature = "iroh-transport")]
    pub(crate) fn response(&self, request: [u8; 32], response: [u8; 32], now: Instant) {
        self.progress(request, NativeProgressStageV1::ResponseQueued, 0, now);
        if let Ok(mut s) = self.state.lock() {
            if let Some(r) = s.records.iter_mut().find(|r| r.tag == request) {
                r.response_tag = Some(response);
            }
        }
    }
    pub fn snapshot(&self) -> Option<NativeEvidenceSnapshotV1> {
        self.snapshot_at(Instant::now())
    }
    /// Read one retained record without refreshing its lifetime or expiring
    /// unrelated records. The owner can export it at its normal handoff.
    pub fn snapshot_transfer(&self, tag: [u8; 32]) -> Option<NativeTransferEvidenceV1> {
        self.state
            .lock()
            .ok()?
            .records
            .iter()
            .find(|record| record.tag == tag)
            .cloned()
    }
    /// Scalar coverage evidence without cloning or changing the record set.
    pub fn snapshot_retention(&self) -> Option<NativeEvidenceRetentionV1> {
        let s = self.state.lock().ok()?;
        Some(NativeEvidenceRetentionV1 {
            retained_records: s.records.len(),
            omitted_records: s.snapshot.omitted_records,
            evicted_records: s.snapshot.evicted_records,
            expired_records: s.snapshot.expired_records,
        })
    }
    /// Cumulative observer-wide rejection counters, not attributed to one object.
    pub fn snapshot_rejections(&self) -> Option<NativeRejectCountsV1> {
        Some(self.state.lock().ok()?.snapshot.rejections)
    }
    pub fn snapshot_send_polls(&self) -> Option<NativeLinkPollEvidenceV1> {
        Some(self.state.lock().ok()?.snapshot.send_polls.clone())
    }
    pub fn snapshot_receive_totals(&self) -> Option<NativeReceiveTotalsV1> {
        Some(self.state.lock().ok()?.snapshot.receive_totals)
    }
    fn snapshot_at(&self, now: Instant) -> Option<NativeEvidenceSnapshotV1> {
        let mut s = self.state.lock().ok()?;
        s.expire(self.micros(now));
        let mut snapshot = s.snapshot.clone();
        snapshot.elapsed_us = self.micros(now);
        snapshot.objects = s.records.iter().cloned().collect();
        Some(snapshot)
    }
}
impl Default for NativeTransferDiagnosticsV1 {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;
    #[test]
    fn feedback_tracks_actual_window_progress_with_bounded_evidence() {
        let d = NativeTransferDiagnosticsV1::new();
        let tag = [6; 32];
        for n in 0..300 {
            d.feedback(
                tag,
                true,
                n + 1,
                20_000 - n * 64,
                Some(n * 64),
                d.origin + Duration::from_millis(n),
            );
        }
        let record = d.snapshot_transfer(tag).unwrap();
        assert_eq!(record.feedback_sent.packets, 300);
        assert_eq!(record.feedback_sent.windows.len(), 256);
        assert_eq!(record.feedback_sent.omitted_windows, 44);
        assert_eq!(record.feedback_sent.max_gap_us, 1000);
        assert_eq!(record.feedback_received.packets, 0);
        d.feedback(
            tag,
            false,
            2,
            19_936,
            Some(64),
            d.origin + Duration::from_millis(350),
        );
        assert_eq!(
            d.snapshot_transfer(tag).unwrap().feedback_received.windows[0].epoch,
            2
        );
    }
    #[test]
    fn fragment_completion_and_read_only_snapshot_keep_separate_boundaries() {
        let d = NativeTransferDiagnosticsV1::for_stream(11);
        let tag = [4; 32];
        let now = d.origin;
        assert!(d.tracks_stream(11));
        assert!(!d.tracks_stream(12));
        use super::super::fragments::FragmentObservation::Accepted;
        d.fragment(
            tag,
            Accepted {
                started: true,
                received: 1,
                total: 2,
            },
            1,
            false,
            now,
        );
        d.fragment(
            tag,
            Accepted {
                started: false,
                received: 2,
                total: 2,
            },
            0,
            true,
            now + Duration::from_millis(7),
        );
        let complete = d.snapshot_transfer(tag).unwrap();
        assert_eq!(complete.first_fragment_us, Some(0));
        assert_eq!(complete.fragments_complete_us, Some(7000));
        assert_eq!(complete.last_fragment_index, Some(0));
        assert_eq!(complete.repair_fragments, 1);
        assert!(complete.first_hash_us.is_none());
        d.progress(
            tag,
            NativeProgressStageV1::Hash,
            1,
            now + Duration::from_millis(8),
        );
        d.progress(
            tag,
            NativeProgressStageV1::Hash,
            1,
            now + Duration::from_millis(9),
        );
        d.progress(
            tag,
            NativeProgressStageV1::Verified,
            0,
            now + Duration::from_millis(10),
        );
        let verified = d.snapshot_transfer(tag).unwrap();
        assert_eq!(verified.first_hash_us, Some(8000));
        assert_eq!(verified.last_hash_us, Some(9000));
        assert_eq!(verified.verified_us, Some(10000));
        assert_eq!(d.snapshot_transfer(tag).unwrap().last_progress_us, 10000);
        assert_eq!(d.state.lock().unwrap().snapshot.expired_records, 0);
        let expired = d.snapshot_at(now + Duration::from_secs(120)).unwrap();
        assert_eq!(expired.elapsed_us, 120_000_000);
        assert_eq!(expired.expired_records, 1);
        assert!(d.snapshot_transfer(tag).is_none());
        // Completed-history packets arrive after the first handoff snapshot;
        // stream totals must retain them even after the object record expires.
        use super::super::fragments::FragmentObservation::Duplicate;
        d.fragment(
            tag,
            Duplicate { completed: true },
            0,
            true,
            now + Duration::from_secs(121),
        );
        let totals = d.snapshot_receive_totals().unwrap();
        assert_eq!(totals.accepted_data, 1);
        assert_eq!(totals.accepted_repair, 1);
        assert_eq!(totals.history_repair, 1);
        assert_eq!(totals.duplicate_repair, 0);
        assert_eq!(complete.completed_history_hits, 0);
    }
    #[test]
    fn authenticated_session_salts_tags_and_progress_never_follows_repeat_ack() {
        let a = transfer_tag_v1([1; 16], 7, [2; 32]);
        assert_ne!(a, transfer_tag_v1([3; 16], 7, [2; 32]));
        assert_ne!(a, transfer_tag_v1([1; 16], 8, [2; 32]));
        let d = NativeTransferDiagnosticsV1::new();
        let now = d.origin;
        d.progress(a, NativeProgressStageV1::AckProgress, 2, now);
        d.progress(
            a,
            NativeProgressStageV1::AckProgress,
            1,
            now + Duration::from_secs(1),
        );
        d.progress(
            a,
            NativeProgressStageV1::AckProgress,
            2,
            now + Duration::from_secs(2),
        );
        let s = d.snapshot().unwrap();
        assert_eq!(s.objects[0].last_progress_us, 0);
        assert_eq!(s.objects[0].acknowledged_best, 2);
        let mut rejects = NativeRejectCountsV1::default();
        rejects.record(NativeRejectReasonV1::SecureReplay);
        d.reject(rejects, now + Duration::from_secs(3));
        let s = d.snapshot().unwrap();
        assert_eq!(s.first_unattributed_reject_us, Some(3_000_000));
        assert_eq!(s.objects[0].last_progress_us, 0); // no invented object association
        d.progress(
            a,
            NativeProgressStageV1::AckProgress,
            3,
            now + Duration::from_secs(4),
        );
        assert_eq!(d.snapshot().unwrap().objects[0].last_progress_us, 4_000_000);
    }
    #[test]
    fn bounded_evidence_preserves_active_records_and_reports_omission() {
        let d = NativeTransferDiagnosticsV1::new();
        let now = d.origin;
        for i in 0u64..300 {
            let mut id = [0u8; 32];
            id[..8].copy_from_slice(&i.to_be_bytes());
            d.progress(id, NativeProgressStageV1::Fragment, 1, now);
        }
        let s = d.snapshot().unwrap();
        assert_eq!(s.objects.len(), 256);
        assert_eq!(s.omitted_records, 44);
        d.progress(
            [9; 32],
            NativeProgressStageV1::Fragment,
            1,
            now + Duration::from_secs(120),
        );
        assert_eq!(d.snapshot().unwrap().objects.len(), 1);
    }
    #[test]
    fn idle_snapshot_expires_and_finished_work_keeps_blocked_response_evidence() {
        let d = NativeTransferDiagnosticsV1::new();
        let now = d.origin;
        for i in 0u64..256 {
            let mut id = [0u8; 32];
            id[..8].copy_from_slice(&i.to_be_bytes());
            d.progress(id, NativeProgressStageV1::WorkFinished, 0, now);
        }
        d.progress([9; 32], NativeProgressStageV1::Fragment, 1, now);
        assert_eq!(d.snapshot_at(now).unwrap().omitted_records, 1);
        let s = d.snapshot_at(now + Duration::from_secs(120)).unwrap();
        assert!(s.objects.is_empty());
        assert_eq!(s.expired_records, 256);
    }
    #[test]
    fn rejection_counts_saturate_and_do_not_change_category_order() {
        let mut counts = NativeRejectCountsV1 {
            counts: [u64::MAX; 16],
        };
        counts.record(NativeRejectReasonV1::ObjectAck);
        counts.merge(counts);
        assert_eq!(counts.total(), u64::MAX);
        assert_eq!(counts.counts[14], u64::MAX);
    }
}
