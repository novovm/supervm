//! Opt-in, bounded process-local stage timing. Never changes protocol decisions.
//! Durations are inclusive wall time: nested and concurrent phases must NOT be
//! added as a critical path or called CPU utilization. The separately enabled
//! critical timeline retains only bounded stage identifiers, never transaction
//! bodies, signatures, keys or protocol authority.
use serde_json::{json, Value};
use std::collections::VecDeque;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::OnceLock;
use std::time::{Instant, SystemTime, UNIX_EPOCH};

const CRITICAL_EVENTS: usize = 2048;

#[derive(Clone, Copy)]
pub(crate) enum CriticalEvent {
    SignatureBatchDone,
    AuthorityAdmissionDone,
    CandidateExecutionDone,
    CandidateRegistered,
    ProposalReceived,
    VoteReceived,
    LocalVoteDurable,
    PrevoteQcAvailable,
    PrecommitQcAvailable,
    DecisionStaged,
    DecisionDurableAck,
    ReceiptProjectionVisible,
}

impl CriticalEvent {
    fn name(self) -> &'static str {
        match self {
            Self::SignatureBatchDone => "signature_batch_done",
            Self::AuthorityAdmissionDone => "authority_admission_done",
            Self::CandidateExecutionDone => "candidate_execution_done",
            Self::CandidateRegistered => "candidate_registered",
            Self::ProposalReceived => "proposal_received",
            Self::VoteReceived => "vote_received",
            Self::LocalVoteDurable => "local_vote_durable",
            Self::PrevoteQcAvailable => "prevote_qc_available",
            Self::PrecommitQcAvailable => "precommit_qc_available",
            Self::DecisionStaged => "decision_staged",
            Self::DecisionDurableAck => "decision_durable_ack",
            Self::ReceiptProjectionVisible => "receipt_projection_visible",
        }
    }
}

#[derive(Clone)]
struct CriticalRecord {
    seq: u64,
    elapsed_ns: u64,
    wall_unix_us: u64,
    event: &'static str,
    height: u64,
    round: Option<u64>,
    value: Option<[u8; 32]>,
    body: Option<[u8; 32]>,
    duration_ns: Option<u64>,
    fragment: Option<[u8; 32]>,
}

struct CriticalBuffer {
    records: VecDeque<CriticalRecord>,
    dropped: u64,
    next_seq: u64,
}

impl CriticalBuffer {
    const fn new() -> Self {
        Self {
            records: VecDeque::new(),
            dropped: 0,
            next_seq: 0,
        }
    }

    fn push(&mut self, mut record: CriticalRecord) {
        record.seq = self.next_seq;
        self.next_seq = self.next_seq.saturating_add(1);
        if self.records.len() == CRITICAL_EVENTS {
            self.dropped = self.dropped.saturating_add(1);
            return;
        }
        self.records.push_back(record);
    }

    fn push_latest(&mut self, record: CriticalRecord) {
        if self.records.len() == CRITICAL_EVENTS {
            self.records.pop_front();
            self.dropped = self.dropped.saturating_add(1);
        }
        self.push(record);
    }
}

fn wall_unix_us() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_micros().min(u128::from(u64::MAX)) as u64)
        .unwrap_or(0)
}

static CRITICAL: std::sync::Mutex<CriticalBuffer> = std::sync::Mutex::new(CriticalBuffer::new());
static CRITICAL_START: OnceLock<Instant> = OnceLock::new();
// Reuse the same bounded record/clock implementation; keep delivery diagnostics
// from evicting the execution/consensus timeline. No payload or signing data.
static DELIVERY: std::sync::Mutex<CriticalBuffer> = std::sync::Mutex::new(CriticalBuffer::new());
static CONTROL: std::sync::Mutex<CriticalBuffer> = std::sync::Mutex::new(CriticalBuffer::new());
const ASSET_EVENTS: usize = 8192;
static ASSET: std::sync::Mutex<(VecDeque<Value>, u64)> =
    std::sync::Mutex::new((VecDeque::new(), 0));

pub(crate) fn asset_enabled() -> bool {
    static ENABLED: OnceLock<bool> = OnceLock::new();
    enabled()
        && critical_enabled()
        && *ENABLED.get_or_init(|| {
            std::env::var("NOVOVM_PROFILE_ASSET_TRACE").ok().as_deref() == Some("1")
        })
}

pub(crate) fn asset_track(id: &[u8; 32]) -> bool {
    asset_enabled() && id[0] == 0
}

/// Diagnostic identity of existing ordered transaction IDs, not signed bytes
/// or a signature/authority cache key. Never participates in admission.
pub(crate) fn asset_ids(ids: &[[u8; 32]]) -> String {
    use sha2::{Digest, Sha256};
    let mut digest = Sha256::new();
    for id in ids {
        digest.update(id);
    }
    hash(Some(digest.finalize().into()))
        .as_str()
        .unwrap()
        .to_owned()
}

pub(crate) fn asset_event(event: &'static str, height: u64, detail: impl FnOnce() -> Value) {
    if !asset_enabled() {
        return;
    }
    let started = CRITICAL_START.get_or_init(Instant::now);
    let elapsed_ns = started.elapsed().as_nanos().min(u128::from(u64::MAX)) as u64;
    let wall_unix_us = wall_unix_us();
    let detail = detail();
    if let Ok(mut buffer) = ASSET.lock() {
        if buffer.0.len() == ASSET_EVENTS {
            buffer.0.pop_front();
            buffer.1 = buffer.1.saturating_add(1);
        }
        buffer.0.push_back(
            json!({"event":event,"height":height,"elapsed_ns":elapsed_ns,
            "wall_unix_us":wall_unix_us,"detail":detail}),
        );
    }
}

fn delivery_enabled() -> bool {
    static ENABLED: OnceLock<bool> = OnceLock::new();
    *ENABLED.get_or_init(|| {
        std::env::var("NOVOVM_PROFILE_NATIVE_DELIVERY")
            .ok()
            .as_deref()
            == Some("1")
    })
}

pub(crate) fn delivery_event(
    event: &'static str,
    height: u64,
    peer: [u8; 32],
    fragment: [u8; 32],
    purposes: u8,
) {
    if !delivery_enabled() {
        return;
    }
    let started = CRITICAL_START.get_or_init(Instant::now);
    if let Ok(mut buffer) = DELIVERY.lock() {
        // Retain the latest window for a late stall, with explicit loss count.
        buffer.push_latest(CriticalRecord {
            seq: 0,
            elapsed_ns: started.elapsed().as_nanos().min(u128::from(u64::MAX)) as u64,
            wall_unix_us: wall_unix_us(),
            event,
            height,
            round: Some(u64::from(purposes)),
            value: Some(peer),
            body: Some(fragment),
            duration_ns: None,
            fragment: None,
        });
    }
}

fn critical_enabled() -> bool {
    static ENABLED: OnceLock<bool> = OnceLock::new();
    *ENABLED.get_or_init(|| {
        std::env::var("NOVOVM_PROFILE_CRITICAL_PATH")
            .ok()
            .as_deref()
            == Some("1")
    })
}

pub(crate) fn critical_event(
    event: CriticalEvent,
    height: u64,
    round: u64,
    value: Option<[u8; 32]>,
    body: Option<[u8; 32]>,
) {
    if !critical_enabled() {
        return;
    }
    let started = CRITICAL_START.get_or_init(Instant::now);
    let elapsed_ns = started.elapsed().as_nanos().min(u128::from(u64::MAX)) as u64;
    if let Ok(mut buffer) = CRITICAL.lock() {
        buffer.push(CriticalRecord {
            seq: 0,
            elapsed_ns,
            wall_unix_us: wall_unix_us(),
            event: event.name(),
            height,
            round: Some(round),
            value,
            body,
            duration_ns: None,
            fragment: None,
        });
    }
}

pub(crate) fn critical_projection_visible(height: u64, value: [u8; 32]) {
    if !critical_enabled() {
        return;
    }
    let started = CRITICAL_START.get_or_init(Instant::now);
    let elapsed_ns = started.elapsed().as_nanos().min(u128::from(u64::MAX)) as u64;
    if let Ok(mut buffer) = CRITICAL.lock() {
        let source = buffer
            .records
            .iter()
            .rev()
            .find(|record| record.value == Some(value) && record.event == "decision_durable_ack")
            .cloned();
        buffer.push(CriticalRecord {
            seq: 0,
            elapsed_ns,
            wall_unix_us: wall_unix_us(),
            event: CriticalEvent::ReceiptProjectionVisible.name(),
            height,
            round: source.as_ref().and_then(|record| record.round),
            value: Some(value),
            body: source.and_then(|record| record.body),
            duration_ns: None,
            fragment: None,
        });
    }
}

fn hash(value: Option<[u8; 32]>) -> Value {
    value.map_or(Value::Null, |bytes| {
        Value::String(bytes.iter().map(|byte| format!("{byte:02x}")).collect())
    })
}

#[derive(Clone, Copy)]
pub(crate) enum Phase {
    Admission,
    Authentication,
    Bind,
    Capture,
    FinalizeCapture,
    ExecuteCpu,
    ExecuteInteger,
    IntegerWire,
    Reduction,
    StateTree,
    Packet,
    IoNodes,
    IoMetadataRead,
    IoMetadataWrite,
    IoPersist,
    IoRecover,
    Lifecycle,
    Consensus,
    Projection,
    Gossip,
    Http,
    Idle,
    Yield,
    QueueAdmission,
    QueueCandidate,
    IntegerCompile,
    IntegerProjection,
    CandidateRegistration,
    NativeIo,
    NativeAssembly,
    NativeQuery,
    NativeSelection,
    NativePrepareEncode,
    NativePrepareHash,
    NativeDecode,
    BodyIdentity,
    BodyCharge,
    NativeIoGap,
    NativeIoPeriod,
    TickStatus,
    TickEncode,
    TickWrite,
    TickOutput,
}
const NAMES: [&str; 43] = [
    "admission",
    "authentication",
    "bind",
    "capture",
    "finalize_capture",
    "execute_cpu",
    "execute_integer",
    "integer_wire",
    "reduction",
    "state_tree",
    "packet",
    "io_nodes",
    "io_metadata_read",
    "io_metadata_write",
    "io_persist",
    "io_recover",
    "lifecycle",
    "consensus",
    "projection",
    "gossip",
    "http",
    "idle",
    "yield",
    "queue_admission",
    "queue_candidate",
    "integer_compile",
    "integer_projection",
    "candidate_registration",
    "native_io",
    "native_assembly",
    "native_query",
    "native_selection",
    "native_prepare_encode",
    "native_prepare_hash",
    "native_decode",
    "body_identity",
    "body_charge",
    "native_io_gap",
    "native_io_period",
    "tick_status",
    "tick_encode",
    "tick_write",
    "tick_output",
];
struct Counter {
    calls: AtomicU64,
    ns: AtomicU64,
    max_ns: AtomicU64,
}
impl Counter {
    const fn new() -> Self {
        Self {
            calls: AtomicU64::new(0),
            ns: AtomicU64::new(0),
            max_ns: AtomicU64::new(0),
        }
    }
    fn record(&self, ns: u64) {
        let _ = self
            .calls
            .fetch_update(Ordering::Relaxed, Ordering::Relaxed, |v| {
                Some(v.saturating_add(1))
            });
        let _ = self
            .ns
            .fetch_update(Ordering::Relaxed, Ordering::Relaxed, |v| {
                Some(v.saturating_add(ns))
            });
        self.max_ns.fetch_max(ns, Ordering::Relaxed);
    }
}
static COUNTERS: [Counter; NAMES.len()] = [const { Counter::new() }; NAMES.len()];

// One bounded local record, no transaction/body retention and no consensus input.
static INTEGER: std::sync::Mutex<Option<Value>> = std::sync::Mutex::new(None);
// Legacy v7 oracle diagnostics only. Product dataflow uses IntegerWire timing;
// never substitute JSON-oracle device samples for the binary production path.
#[cfg(test)]
pub(crate) fn record_integer(
    p: &novovm_exec::AoemIntegerResourceProfileV1,
    active: usize,
    capacity: usize,
) {
    if !enabled() {
        return;
    }
    let Ok(mut guard) = INTEGER.lock() else {
        return;
    };
    if guard.as_ref().and_then(|v| v["resource_id"].as_u64()) != Some(p.resource_id) {
        *guard = Some(
            json!({"resource_id":p.resource_id,"calls":0,"gpu_samples":0,"active_rows":0,"capacity_rows":0,"sums_us":{}}),
        );
    }
    let value = guard.as_mut().unwrap();
    for (name, n) in [
        ("calls", 1),
        ("gpu_samples", u64::from(p.diagnostics.gpu_us.is_some())),
        ("active_rows", active as u64),
        ("capacity_rows", capacity as u64),
    ] {
        value[name] = value[name].as_u64().unwrap_or(0).saturating_add(n).into();
    }
    let d = &p.diagnostics;
    for (name, n) in [
        ("encode", p.encode_us),
        ("exchange", p.exchange_us),
        ("decode", p.decode_us),
        ("upload", d.upload_us),
        ("dispatch", d.dispatch_us),
        ("record", d.record_us),
        ("submit", d.submit_us),
        ("wait", d.wait_us),
        ("gpu", d.gpu_us.unwrap_or(0)),
        ("read_decode", d.read_decode_us),
        ("aoem_total", d.total_us),
    ] {
        value["sums_us"][name] = value["sums_us"][name]
            .as_u64()
            .unwrap_or(0)
            .saturating_add(n)
            .into();
    }
    value["last"] = serde_json::to_value(p).expect("finite validated integer diagnostics");
}
pub(crate) fn enabled() -> bool {
    static ENABLED: OnceLock<bool> = OnceLock::new();
    *ENABLED.get_or_init(|| std::env::var("NOVOVM_PROFILE_STAGES").ok().as_deref() == Some("1"))
}
pub(crate) struct Span(Phase, Option<Instant>);
pub(crate) fn mark() -> Option<Instant> {
    enabled().then(|| {
        CRITICAL_START.get_or_init(Instant::now);
        Instant::now()
    })
}
pub(crate) fn work(
    phase: Phase,
    start: Option<Instant>,
    height: u64,
    body: Option<[u8; 32]>,
    fragment: Option<[u8; 32]>,
) {
    work_interval(phase, start, mark(), height, body, fragment);
}

pub(crate) fn work_interval(
    phase: Phase,
    start: Option<Instant>,
    end: Option<Instant>,
    height: u64,
    body: Option<[u8; 32]>,
    fragment: Option<[u8; 32]>,
) {
    let (Some(start), Some(end)) = (start, end) else {
        return;
    };
    let duration_ns = end
        .duration_since(start)
        .as_nanos()
        .min(u128::from(u64::MAX)) as u64;
    COUNTERS[phase as usize].record(duration_ns);
    if !critical_enabled() || duration_ns < 1_000_000 {
        return;
    }
    let record = CriticalRecord {
        seq: 0,
        elapsed_ns: end
            .duration_since(*CRITICAL_START.get_or_init(|| start))
            .as_nanos()
            .min(u128::from(u64::MAX)) as u64,
        wall_unix_us: wall_unix_us(),
        event: NAMES[phase as usize],
        height,
        round: None,
        value: None,
        body,
        duration_ns: Some(duration_ns),
        fragment,
    };
    // Keep first-height synchronous work as well as a bounded latest window.
    if height == 1
        && (!matches!(phase, Phase::NativeIoGap | Phase::NativeIoPeriod)
            || duration_ns >= 5_000_000)
    {
        if let Ok(mut buffer) = CRITICAL.lock() {
            buffer.push(record.clone());
        }
    }
    if let Ok(mut buffer) = CONTROL.lock() {
        buffer.push_latest(record);
    }
}
pub(crate) fn elapsed(phase: Phase, start: Option<Instant>) {
    drop(Span(phase, start));
}
pub(crate) fn span(phase: Phase) -> Span {
    Span(phase, mark())
}
impl Drop for Span {
    fn drop(&mut self) {
        if let Some(start) = self.1 {
            COUNTERS[self.0 as usize]
                .record(start.elapsed().as_nanos().min(u128::from(u64::MAX)) as u64);
        }
    }
}
pub(crate) fn snapshot() -> Value {
    let phases: serde_json::Map<String, Value> = if enabled() {
        NAMES
            .iter()
            .zip(&COUNTERS)
            .map(|(name, c)| {
                (
                    (*name).to_owned(),
                    json!({
                        "calls": c.calls.load(Ordering::Relaxed),
                        "wall_ns": c.ns.load(Ordering::Relaxed),
                        "max_wall_ns": c.max_ns.load(Ordering::Relaxed),
                    }),
                )
            })
            .collect()
    } else {
        Default::default()
    };
    let integer = if enabled() {
        INTEGER.lock().ok().and_then(|v| v.clone())
    } else {
        None
    };
    let critical = if critical_enabled() {
        CRITICAL.lock().ok().map(|buffer| {
            json!({
                "enabled":true,
                "process_id":std::process::id(),
                "capacity":CRITICAL_EVENTS,
                "dropped":buffer.dropped,
                "clock":"process-local monotonic elapsed_ns; wall_unix_us is cross-process alignment assistance and not an ordering authority",
                "events":buffer.records.iter().map(|record| json!({
                    "seq":record.seq,
                    "elapsed_ns":record.elapsed_ns,
                    "wall_unix_us":record.wall_unix_us,
                    "event":record.event,
                    "height":record.height,
                    "round":record.round,
                    "value":hash(record.value),
                    "body":hash(record.body),
                    "duration_ns":record.duration_ns,
                    "fragment":hash(record.fragment),
                })).collect::<Vec<_>>()
            })
        }).unwrap_or_else(|| json!({"enabled":true,"capacity":CRITICAL_EVENTS,"unavailable":"lock poisoned"}))
    } else {
        json!({"enabled":false,"capacity":CRITICAL_EVENTS,"dropped":0,"events":[]})
    };
    let delivery = if delivery_enabled() {
        DELIVERY.lock().ok().map(|buffer| json!({
            "enabled":true, "capacity":CRITICAL_EVENTS, "evicted":buffer.dropped,
            "scope":"bounded latest local ownership/transport events; purpose bits 1=realtime 2=recovery; height 0=not carried; transport DONE is not finality",
            "events":buffer.records.iter().map(|record| json!({
                "seq":record.seq, "elapsed_ns":record.elapsed_ns,
                "wall_unix_us":record.wall_unix_us, "event":record.event,
                "height":record.height, "purposes":record.round,
                "peer":hash(record.value), "fragment":hash(record.body)
            })).collect::<Vec<_>>()
        })).unwrap_or_else(|| json!({"enabled":true,"unavailable":"lock poisoned"}))
    } else {
        json!({"enabled":false,"capacity":CRITICAL_EVENTS,"events":[]})
    };
    let control = if enabled() && critical_enabled() {
        CONTROL.lock().ok().map(|buffer| json!({
            "enabled":true, "capacity":CRITICAL_EVENTS, "evicted":buffer.dropped,
            "scope":"latest synchronous work and IO intervals >=1ms; elapsed_ns is interval end; start=end-duration_ns; IO period includes IO, IO gap excludes previous IO; neither is CPU idle; fragment and body are distinct identifiers",
            "events":buffer.records.iter().map(|record| json!({
                "seq":record.seq,"elapsed_ns":record.elapsed_ns,"duration_ns":record.duration_ns,
                "wall_unix_us":record.wall_unix_us,"event":record.event,"height":record.height,
                "body":hash(record.body),"fragment":hash(record.fragment)
            })).collect::<Vec<_>>()
        })).unwrap_or_else(|| json!({"enabled":true,"unavailable":"lock poisoned"}))
    } else {
        json!({"enabled":false,"events":[]})
    };
    let asset = if asset_enabled() {
        ASSET.lock().ok().map(|buffer| json!({
            "enabled":true,"capacity":ASSET_EVENTS,"evicted":buffer.1,
            "scope":"local source_ref is only the live Arc address, never global identity or authority; correlate its lifetime with scope/fragment/row; sampled row IDs have first byte zero; batch counts and ordered-ID digests cover full inputs; diagnostic metadata is not signature evidence",
            "events":buffer.0
        })).unwrap_or_else(|| json!({"enabled":true,"unavailable":"lock poisoned"}))
    } else {
        json!({"enabled":false,"events":[]})
    };
    json!({"enabled":enabled(),"scope":"process lifetime inclusive wall time; overlapping/nested phases; non-atomic snapshot; not CPU time or consensus evidence", "phases":phases,"integer_resource":integer,"critical_path":critical,"native_delivery":delivery,"control_work":control,"asset_ingress":asset})
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn asset_ids_bind_existing_id_order_without_signed_payload() {
        use sha2::{Digest, Sha256};
        let ids = [[1; 32], [2; 32]];
        let expected = Sha256::digest(ids.concat());
        let expected: [u8; 32] = expected.into();
        assert_eq!(asset_ids(&ids), hash(Some(expected)).as_str().unwrap());
        assert_ne!(asset_ids(&ids), asset_ids(&[ids[1], ids[0]]));
        assert_ne!(asset_ids(&ids), asset_ids(&ids[..1]));
    }

    #[test]
    fn asset_trace_only_builds_details_when_explicitly_enabled() {
        let called = std::cell::Cell::new(false);
        asset_event("test_asset_detail", 0, || {
            called.set(true);
            json!({"test":true})
        });
        assert_eq!(called.get(), asset_enabled());
    }

    #[test]
    fn counters_are_bounded_and_keep_maximum_without_overflow() {
        let counter = Counter::new();
        counter.record(7);
        counter.record(3);
        assert_eq!(counter.calls.load(Ordering::Relaxed), 2);
        assert_eq!(counter.ns.load(Ordering::Relaxed), 10);
        assert_eq!(counter.max_ns.load(Ordering::Relaxed), 7);
        counter.record(u64::MAX);
        counter.record(1);
        assert_eq!(counter.ns.load(Ordering::Relaxed), u64::MAX);
        assert_eq!(counter.max_ns.load(Ordering::Relaxed), u64::MAX);
        assert_eq!(Phase::TickOutput as usize + 1, NAMES.len());
    }

    #[test]
    fn critical_buffer_is_fixed_capacity_and_counts_drops() {
        let mut buffer = CriticalBuffer::new();
        for elapsed_ns in 0..=CRITICAL_EVENTS as u64 {
            buffer.push(CriticalRecord {
                seq: 0,
                elapsed_ns,
                wall_unix_us: 0,
                event: "test",
                height: 1,
                round: Some(0),
                value: None,
                body: None,
                duration_ns: None,
                fragment: None,
            });
        }
        assert_eq!(buffer.records.len(), CRITICAL_EVENTS);
        assert_eq!(buffer.dropped, 1);
    }

    #[test]
    fn delivery_buffer_retains_latest_window_and_counts_evictions() {
        let mut buffer = CriticalBuffer::new();
        for elapsed_ns in 0..(CRITICAL_EVENTS + 3) as u64 {
            buffer.push_latest(CriticalRecord {
                seq: 0,
                elapsed_ns,
                wall_unix_us: 0,
                event: "test",
                height: 1,
                round: Some(2),
                value: None,
                body: None,
                duration_ns: None,
                fragment: None,
            });
        }
        assert_eq!(buffer.records.len(), CRITICAL_EVENTS);
        assert_eq!(buffer.dropped, 3);
        assert_eq!(buffer.records.front().unwrap().seq, 3);
        assert_eq!(
            buffer.records.back().unwrap().seq,
            CRITICAL_EVENTS as u64 + 2
        );
    }
}
