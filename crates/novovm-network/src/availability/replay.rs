use crate::availability::{QueueStore, QueuedRequest};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ReplayResult {
    Applied,
    DuplicateIgnored,
    RetryLater,
    PermanentlyRejected,
}

pub trait ReplayApplier {
    fn apply(&mut self, req: &QueuedRequest) -> ReplayResult;
}

#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub struct ReplayStats {
    pub applied: u64,
    pub retry_later: u64,
    pub permanently_rejected: u64,
}

#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct ReplayRunReport {
    pub pending_total: u64,
    pub applied_total: u64,
    pub duplicate_ignored_total: u64,
    pub retry_later_total: u64,
    pub permanently_rejected_total: u64,
    pub applied_request_ids: Vec<String>,
}

impl ReplayRunReport {
    pub fn stats(&self) -> ReplayStats {
        ReplayStats {
            applied: self.applied_total.saturating_add(self.duplicate_ignored_total),
            retry_later: self.retry_later_total,
            permanently_rejected: self.permanently_rejected_total,
        }
    }
}

pub fn run_replay(
    store: &mut dyn QueueStore,
    applier: &mut dyn ReplayApplier,
) -> Result<ReplayRunReport, String> {
    run_replay_with(store, |req| applier.apply(req))
}

pub fn run_replay_with(
    store: &mut dyn QueueStore,
    mut apply: impl FnMut(&QueuedRequest) -> ReplayResult,
) -> Result<ReplayRunReport, String> {
    let pending = store.list_pending();
    let mut report = ReplayRunReport {
        pending_total: pending.len() as u64,
        ..ReplayRunReport::default()
    };

    for req in pending {
        match apply(&req) {
            ReplayResult::Applied => {
                store.remove(&req.request_id)?;
                report.applied_total = report.applied_total.saturating_add(1);
                report.applied_request_ids.push(req.request_id);
            }
            ReplayResult::DuplicateIgnored => {
                store.remove(&req.request_id)?;
                report.duplicate_ignored_total = report.duplicate_ignored_total.saturating_add(1);
            }
            ReplayResult::RetryLater => {
                report.retry_later_total = report.retry_later_total.saturating_add(1);
            }
            ReplayResult::PermanentlyRejected => {
                report.permanently_rejected_total =
                    report.permanently_rejected_total.saturating_add(1);
            }
        }
    }

    Ok(report)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::availability::{InMemoryQueueStore, QueueStore, QueuedRequest};

    #[test]
    fn replay_applied_removes_request_and_retry_keeps_request() {
        let mut store = InMemoryQueueStore::new();
        store
            .enqueue(QueuedRequest {
                request_id: "req-a".to_string(),
                idempotent_key: "id-a".to_string(),
                created_unix_ms: 1,
                payload: vec![1],
            })
            .expect("enqueue req-a");
        store
            .enqueue(QueuedRequest {
                request_id: "req-b".to_string(),
                idempotent_key: "id-b".to_string(),
                created_unix_ms: 2,
                payload: vec![2],
            })
            .expect("enqueue req-b");

        let report = run_replay_with(&mut store, |req| {
            if req.request_id == "req-a" {
                ReplayResult::Applied
            } else {
                ReplayResult::RetryLater
            }
        })
        .expect("run replay");

        assert_eq!(report.pending_total, 2);
        assert_eq!(report.applied_total, 1);
        assert_eq!(report.retry_later_total, 1);
        assert_eq!(report.applied_request_ids, vec!["req-a".to_string()]);

        let pending = store.list_pending();
        assert_eq!(pending.len(), 1);
        assert_eq!(pending[0].request_id, "req-b");

        let stats = report.stats();
        assert_eq!(stats.applied, 1);
        assert_eq!(stats.retry_later, 1);
        assert_eq!(stats.permanently_rejected, 0);
    }
}
