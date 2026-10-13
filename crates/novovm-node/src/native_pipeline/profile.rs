//! Opt-in, bounded process-local stage timing. Never changes protocol decisions.
//! Durations are inclusive wall time: nested and concurrent phases must NOT be
//! added as a critical path or called CPU utilization. No per-transaction log.
use serde_json::{json, Value};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::OnceLock;
use std::time::Instant;

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
}
const NAMES: [&str; 23] = [
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
pub(crate) fn enabled() -> bool {
    static ENABLED: OnceLock<bool> = OnceLock::new();
    *ENABLED.get_or_init(|| std::env::var("NOVOVM_PROFILE_STAGES").ok().as_deref() == Some("1"))
}
pub(crate) struct Span(Phase, Option<Instant>);
pub(crate) fn span(phase: Phase) -> Span {
    Span(phase, enabled().then(Instant::now))
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
    json!({"enabled":enabled(),"scope":"process lifetime inclusive wall time; overlapping/nested phases; non-atomic snapshot; not CPU time or consensus evidence", "phases":phases})
}

#[cfg(test)]
mod tests {
    use super::*;
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
        assert_eq!(Phase::Yield as usize + 1, NAMES.len());
    }
}
