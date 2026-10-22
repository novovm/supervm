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
    pub hash_chunks: u64,
    pub acknowledged_best: u64,
    pub verified_us: Option<u64>,
    pub work_entered_us: Option<u64>,
    pub work_finished_us: Option<u64>,
    pub stored_receipt_created_us: Option<u64>,
    pub response_queued_us: Option<u64>,
    pub response_tag: Option<[u8; 32]>,
}
#[derive(Clone, Debug, Serialize)]
pub struct NativeEvidenceSnapshotV1 {
    pub scope: &'static str,
    pub rejection_names: [&'static str; 16],
    pub rejections: NativeRejectCountsV1,
    pub first_unattributed_reject_us: Option<u64>,
    pub last_unattributed_reject_us: Option<u64>,
    pub omitted_records: u64,
    pub evicted_records: u64,
    pub expired_records: u64,
    pub objects: Vec<NativeTransferEvidenceV1>,
}
struct State {
    snapshot: NativeEvidenceSnapshotV1,
    records: VecDeque<NativeTransferEvidenceV1>,
}
pub struct NativeTransferDiagnosticsV1 {
    origin: Instant,
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
}
impl NativeTransferDiagnosticsV1 {
    pub fn new() -> Self {
        Self { origin: Instant::now(), state: Mutex::new(State {
            snapshot: NativeEvidenceSnapshotV1 {
                scope: "local monotonic clock; rejection association unknown; bounded test evidence",
                rejection_names: ["wire_budget","oversize","handshake","unknown_source","no_session","secure_route","secure_nonce","secure_replay_pre_auth","secure_authentication","wire_or_frame","stream_denied","open_other","unknown_stream","ack_short","object_ack","object_fragment"],
                rejections: NativeRejectCountsV1::default(), first_unattributed_reject_us: None,
                last_unattributed_reject_us: None, omitted_records: 0,
                evicted_records: 0, expired_records: 0, objects: Vec::new(),
            }, records: VecDeque::new(),
        }) }
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
        s.expire(at);
        let index = match s.records.iter().position(|r| r.tag == tag) {
            Some(index) => index,
            None => {
                if s.records.len() == 256 {
                    if let Some(index) = s.records.iter().position(|r| {
                        r.transport_done_us.is_some()
                            || r.response_validated_us.is_some()
                            || r.response_tag.is_some_and(|tag| {
                                s.records.iter().any(|response| {
                                    response.tag == tag && response.transport_done_us.is_some()
                                })
                            })
                    }) {
                        s.records.remove(index);
                        s.snapshot.evicted_records = s.snapshot.evicted_records.saturating_add(1);
                    } else {
                        s.snapshot.omitted_records = s.snapshot.omitted_records.saturating_add(1);
                        return;
                    }
                }
                s.records.push_back(NativeTransferEvidenceV1 {
                    tag,
                    first_seen_us: at,
                    last_progress_us: at,
                    transport_done_us: None,
                    response_validated_us: None,
                    fragments: 0,
                    hash_chunks: 0,
                    acknowledged_best: 0,
                    verified_us: None,
                    work_entered_us: None,
                    work_finished_us: None,
                    stored_receipt_created_us: None,
                    response_queued_us: None,
                    response_tag: None,
                });
                s.records.len() - 1
            }
        };
        let r = &mut s.records[index];
        if matches!(stage, NativeProgressStageV1::AckProgress) && amount <= r.acknowledged_best {
            return;
        }
        r.last_progress_us = at;
        match stage {
            NativeProgressStageV1::Fragment => r.fragments = r.fragments.saturating_add(1),
            NativeProgressStageV1::Hash => r.hash_chunks = r.hash_chunks.saturating_add(1),
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
    fn snapshot_at(&self, now: Instant) -> Option<NativeEvidenceSnapshotV1> {
        let mut s = self.state.lock().ok()?;
        s.expire(self.micros(now));
        let mut snapshot = s.snapshot.clone();
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
