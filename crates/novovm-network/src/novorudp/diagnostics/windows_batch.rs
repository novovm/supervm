//! Joint samples from the original synchronous Windows batch owner. Counters
//! cover all profiled attempts; only sampled durations occupy the bounded ring.
use novovm_udp_batch::SegmentedBatchObservation;
use serde::Serialize;
use std::{cell::RefCell, io, time::Instant};

const CAPACITY: usize = 1024;
const NAMES: [&str; 5] = [
    "lock_and_option",
    "allocation",
    "concat",
    "send_to",
    "result_check",
];

#[derive(Default, Serialize)]
struct Totals {
    calls: u64,
    requested_datagrams: u64,
    requested_bytes: u64,
    option_change_attempts: u64,
    allocation_requests: u64,
    allocated_capacity_bytes_total: u64,
    maximum_single_capacity_bytes: u64,
    batches_with_capacity_change: u64,
    copied_bytes: u64,
    send_calls: u64,
    raw_written_bytes: u64,
    complete_batches: u64,
    accepted_datagrams: u64,
    accepted_bytes: u64,
    failed_batches: u64,
    setup_failures: u64,
    would_block: u64,
    short_writes: u64,
}

#[derive(Clone, Copy, Serialize)]
struct Sample {
    call: u64,
    datagrams: usize,
    requested_bytes: usize,
    option_change_attempted: bool,
    allocation_requests: usize,
    capacity_before_copy: usize,
    capacity_after_copy: usize,
    copied_bytes: usize,
    send_called: bool,
    written_bytes: Option<usize>,
    outcome: &'static str,
    phase_ns: [Option<u64>; 5],
    total_ns: Option<u64>,
}

struct Stats {
    totals: Totals,
    rows: [Option<Sample>; CAPACITY],
    samples_seen: u64,
    next: usize,
}
thread_local! {
    static STATS: RefCell<Stats> = RefCell::new(Stats {
        totals: Totals::default(), rows: [None; CAPACITY], samples_seen: 0, next: 0,
    });
}

pub(crate) struct Observation {
    call: u64,
    datagrams: usize,
    requested_bytes: usize,
    started: Option<Instant>,
    setup_ns: Option<u64>,
    setup_completed: bool,
    pub option_change_attempted: bool,
    pub platform: SegmentedBatchObservation,
}
impl Observation {
    pub(crate) fn setup_done(&mut self) {
        self.setup_ns = elapsed(self.started);
        self.setup_completed = true;
    }
}

pub(crate) fn begin(datagrams: &[&[u8]]) -> Option<Observation> {
    if !super::io_cost_enabled() {
        return None;
    }
    Some(begin_profiled(datagrams))
}

fn begin_profiled(datagrams: &[&[u8]]) -> Observation {
    let call = STATS.with(|stats| {
        let mut stats = stats.borrow_mut();
        let call = stats.totals.calls;
        stats.totals.calls += 1;
        call
    });
    let measure_time = call.is_multiple_of(61);
    let requested_bytes = datagrams.iter().map(|d| d.len()).sum();
    Observation {
        call,
        datagrams: datagrams.len(),
        requested_bytes,
        started: measure_time.then(Instant::now),
        setup_ns: None,
        setup_completed: false,
        option_change_attempted: false,
        platform: SegmentedBatchObservation {
            measure_time,
            ..Default::default()
        },
    }
}

pub(crate) fn finish(observation: Option<Observation>, result: &io::Result<usize>) {
    let Some(mut o) = observation else { return };
    if !o.setup_completed {
        o.setup_ns = elapsed(o.started);
    }
    let total_ns = elapsed(o.started);
    let p = &o.platform;
    STATS.with(|stats| {
        let mut stats = stats.borrow_mut();
        let t = &mut stats.totals;
        t.requested_datagrams += o.datagrams as u64;
        t.requested_bytes += o.requested_bytes as u64;
        t.option_change_attempts += u64::from(o.option_change_attempted);
        t.allocation_requests += p.allocation_requests as u64;
        t.allocated_capacity_bytes_total += p.capacity_before_copy as u64;
        t.maximum_single_capacity_bytes = t
            .maximum_single_capacity_bytes
            .max(p.capacity_after_copy as u64);
        t.batches_with_capacity_change +=
            u64::from(p.capacity_before_copy != p.capacity_after_copy);
        t.copied_bytes += p.copied_bytes as u64;
        t.send_calls += u64::from(p.send_called);
        t.raw_written_bytes += p.written_bytes.unwrap_or(0) as u64;
        let outcome = match result {
            Ok(count) => {
                t.complete_batches += 1;
                t.accepted_datagrams += *count as u64;
                t.accepted_bytes += p.written_bytes.unwrap_or(0) as u64;
                "complete"
            }
            Err(error) => {
                t.failed_batches += 1;
                if !o.setup_completed {
                    t.setup_failures += 1;
                    "setup_error"
                } else if p.written_bytes.is_some() {
                    t.short_writes += 1;
                    "short_write"
                } else if error.kind() == io::ErrorKind::WouldBlock {
                    t.would_block += 1;
                    "would_block"
                } else if p.send_called {
                    "send_error"
                } else {
                    "validation_or_platform_error"
                }
            }
        };
        if o.started.is_some() {
            let row = Sample {
                call: o.call,
                datagrams: o.datagrams,
                requested_bytes: o.requested_bytes,
                option_change_attempted: o.option_change_attempted,
                allocation_requests: p.allocation_requests,
                capacity_before_copy: p.capacity_before_copy,
                capacity_after_copy: p.capacity_after_copy,
                copied_bytes: p.copied_bytes,
                send_called: p.send_called,
                written_bytes: p.written_bytes,
                outcome,
                phase_ns: [
                    o.setup_ns,
                    p.phase_ns[0],
                    p.phase_ns[1],
                    p.phase_ns[2],
                    p.phase_ns[3],
                ],
                total_ns,
            };
            let next = stats.next;
            stats.rows[next] = Some(row);
            stats.next = (next + 1) % CAPACITY;
            stats.samples_seen += 1;
        }
    });
}

fn elapsed(start: Option<Instant>) -> Option<u64> {
    start.map(|s| s.elapsed().as_nanos().min(u64::MAX as u128) as u64)
}

pub(super) fn snapshot() -> serde_json::Value {
    STATS.with(|stats| {
        let stats = stats.borrow();
        let mut rows = stats.rows.iter().flatten().copied().collect::<Vec<_>>();
        rows.sort_unstable_by_key(|r| r.call);
        let phases = NAMES.iter().enumerate().map(|(index, name)| {
            let mut values = rows.iter().filter_map(|r| r.phase_ns[index]).collect::<Vec<_>>();
            values.sort_unstable();
            let quantile = |percent: usize| values.get((values.len() * percent).div_ceil(100).saturating_sub(1)).copied();
            (*name, serde_json::json!({"samples":values.len(), "sampled_ns":values.iter().map(|x| *x as u128).sum::<u128>(),
                "p50_ns":quantile(50),"p95_ns":quantile(95),"p99_ns":quantile(99),"max_ns":values.last()}))
        }).collect::<std::collections::BTreeMap<_, _>>();
        serde_json::json!({
            "scope":"current poll thread; original exclusive Windows segmented batch owner only; nonempty UDP batches; counts all profiled attempts, durations every 61st SAME batch. Independent intervals, not CPU time. Validation, bookkeeping, buffer drop and other gaps remain in total/residual. Vec allocation requests/capacity are not global allocator events. Socket acceptance is not peer delivery. No timing or counters when IO profiling disabled.",
            "capacity":CAPACITY,"sample_every_calls":61,"samples_seen":stats.samples_seen,
            "omitted_samples":stats.samples_seen.saturating_sub(rows.len() as u64),
            "quantile_scope":"retained joint samples only, nearest rank; last 1024 if ring wraps; unreached/unsampled is null",
            "phase_order":NAMES,"totals":stats.totals,"phases":phases,"samples":rows
        })
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn joint_samples_are_bounded_and_unreached_phases_are_null() {
        std::thread::spawn(|| {
            for _ in 0..CAPACITY * 61 + 1 {
                let mut o = begin_profiled(&[&[1, 2, 3]]);
                o.setup_done();
                finish(Some(o), &Ok(1));
            }
            let view = snapshot();
            assert_eq!(view["samples_seen"], CAPACITY + 1);
            assert_eq!(view["omitted_samples"], 1);
            assert_eq!(view["samples"].as_array().unwrap().len(), CAPACITY);
            assert_eq!(view["samples"][0]["call"], 61);
            assert_eq!(
                view["phases"]["allocation"]["p50_ns"],
                serde_json::Value::Null
            );
            for row in view["samples"].as_array().unwrap() {
                let total = row["total_ns"].as_u64().unwrap();
                let sum = row["phase_ns"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .filter_map(|v| v.as_u64())
                    .sum::<u64>();
                assert!(sum <= total);
            }
        })
        .join()
        .unwrap();
    }

    #[test]
    fn result_accounting_never_turns_short_write_or_error_into_success() {
        std::thread::spawn(|| {
            for (kind, written, called) in [
                (io::ErrorKind::WouldBlock, None, true),
                (io::ErrorKind::WriteZero, Some(2), true),
                (io::ErrorKind::InvalidInput, None, false),
            ] {
                let mut o = begin_profiled(&[&[1, 2, 3]]);
                o.setup_done();
                o.platform.send_called = called;
                o.platform.written_bytes = written;
                finish(Some(o), &Err(kind.into()));
            }
            let mut setup_failure = begin_profiled(&[&[1, 2, 3]]);
            setup_failure.option_change_attempted = true;
            finish(
                Some(setup_failure),
                &Err(io::ErrorKind::PermissionDenied.into()),
            );
            let view = snapshot();
            assert_eq!(view["totals"]["calls"], 4);
            assert_eq!(view["totals"]["failed_batches"], 4);
            assert_eq!(view["totals"]["would_block"], 1);
            assert_eq!(view["totals"]["short_writes"], 1);
            assert_eq!(view["totals"]["setup_failures"], 1);
            assert_eq!(view["totals"]["raw_written_bytes"], 2);
            assert_eq!(view["totals"]["accepted_datagrams"], 0);
            assert_eq!(view["totals"]["accepted_bytes"], 0);
        })
        .join()
        .unwrap();
    }
}
