//! Bounded local timings, never authorization, billing or delivery evidence.
//! No peer, mailbox, object, capability, content, or wall-clock fields.
use serde::Serialize;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Instant;

#[derive(Default)]
pub(super) struct Counter {
    started: AtomicU64,
    completed: AtomicU64,
    rejected: AtomicU64,
    interrupted: AtomicU64,
    total_us: AtomicU64,
    max_us: AtomicU64,
}

#[derive(Serialize)]
pub struct TimingV1 {
    started: u64,
    completed: u64,
    rejected: u64,
    interrupted: u64,
    total_us: u64,
    max_us: u64,
}

fn add(value: &AtomicU64, amount: u64) {
    let _ = value.fetch_update(Ordering::Relaxed, Ordering::Relaxed, |old| {
        Some(old.saturating_add(amount))
    });
}

impl Counter {
    pub(super) fn start(&self) -> Sample<'_> {
        add(&self.started, 1);
        Sample {
            counter: self,
            start: Instant::now(),
            completed: false,
        }
    }
    fn snapshot(&self) -> TimingV1 {
        TimingV1 {
            started: self.started.load(Ordering::Relaxed),
            completed: self.completed.load(Ordering::Relaxed),
            rejected: self.rejected.load(Ordering::Relaxed),
            interrupted: self.interrupted.load(Ordering::Relaxed),
            total_us: self.total_us.load(Ordering::Relaxed),
            max_us: self.max_us.load(Ordering::Relaxed),
        }
    }
}

pub(super) struct Sample<'a> {
    counter: &'a Counter,
    start: Instant,
    completed: bool,
}
impl Sample<'_> {
    pub(super) fn complete(mut self, rejected: bool) {
        self.completed = true;
        add(&self.counter.completed, 1);
        if rejected {
            add(&self.counter.rejected, 1);
        }
    }
}
impl Drop for Sample<'_> {
    fn drop(&mut self) {
        if !self.completed {
            add(&self.counter.interrupted, 1);
        }
        let us = self.start.elapsed().as_micros().min(u64::MAX as u128) as u64;
        add(&self.counter.total_us, us);
        self.counter.max_us.fetch_max(us, Ordering::Relaxed);
    }
}

#[derive(Default)]
pub(super) struct Timings {
    pub(super) admission: Counter,
    pub(super) put: Counter,
    pub(super) fetch: Counter,
    pub(super) consume: Counter,
    pub(super) watch: Counter,
    pub(super) other: Counter,
    pub(super) response_enqueue: Counter,
}

/// Process-lifetime approximate atomic snapshots. Watch includes intentional
/// long polling. Response enqueue does not prove remote receipt. Interrupted
/// includes errors/cancellation and may include already committed storage.
#[derive(Serialize)]
pub struct MailboxTimingsV1 {
    version: u8,
    admission: TimingV1,
    put: TimingV1,
    fetch: TimingV1,
    consume: TimingV1,
    watch: TimingV1,
    other: TimingV1,
    response_enqueue: TimingV1,
}
impl Timings {
    pub(super) fn snapshot(&self) -> MailboxTimingsV1 {
        MailboxTimingsV1 {
            version: 1,
            admission: self.admission.snapshot(),
            put: self.put.snapshot(),
            fetch: self.fetch.snapshot(),
            consume: self.consume.snapshot(),
            watch: self.watch.snapshot(),
            other: self.other.snapshot(),
            response_enqueue: self.response_enqueue.snapshot(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn failure_and_cancel_are_not_counted_as_completed_or_delivery() {
        let timings = Timings::default();
        timings.put.start().complete(false);
        timings.put.start().complete(true);
        drop(timings.put.start());
        let s = timings.snapshot();
        assert_eq!(
            (
                s.put.started,
                s.put.completed,
                s.put.rejected,
                s.put.interrupted
            ),
            (3, 2, 1, 1)
        );
        assert_eq!(s.fetch.started, 0);
        assert!(s.put.total_us >= s.put.max_us);
        let v = serde_json::to_value(s).unwrap();
        assert_eq!(v.as_object().unwrap().len(), 8);
        assert_eq!(v["put"].as_object().unwrap().len(), 6);
    }
}
