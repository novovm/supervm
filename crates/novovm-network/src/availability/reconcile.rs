use crate::availability::QueueStore;
use std::collections::HashSet;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ReconcileStatus {
    Applied,
    Pending,
    Unknown,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReconcileEntry {
    pub request_id: String,
    pub idempotent_key: String,
    pub status: ReconcileStatus,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReconcileReport {
    pub entries: Vec<ReconcileEntry>,
    pub replay_retry_later_total: u64,
    pub replay_rejected_total: u64,
}

#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub struct ReconcileStats {
    pub pending: u64,
    pub applied: u64,
    pub retry_later: u64,
    pub rejected: u64,
    pub unknown: u64,
}

impl ReconcileReport {
    pub fn empty() -> Self {
        Self {
            entries: Vec::new(),
            replay_retry_later_total: 0,
            replay_rejected_total: 0,
        }
    }

    pub fn stats(&self) -> ReconcileStats {
        let mut stats = ReconcileStats::default();
        for entry in &self.entries {
            match entry.status {
                ReconcileStatus::Pending => stats.pending = stats.pending.saturating_add(1),
                ReconcileStatus::Applied => stats.applied = stats.applied.saturating_add(1),
                ReconcileStatus::Unknown => stats.unknown = stats.unknown.saturating_add(1),
            }
        }
        stats.retry_later = self.replay_retry_later_total;
        stats.rejected = self.replay_rejected_total;
        stats
    }
}

pub fn build_reconcile_report(
    store: &dyn QueueStore,
    recently_applied_request_ids: &[String],
) -> ReconcileReport {
    build_reconcile_report_with_replay(store, recently_applied_request_ids, 0, 0)
}

pub fn build_reconcile_report_with_replay(
    store: &dyn QueueStore,
    recently_applied_request_ids: &[String],
    replay_retry_later_total: u64,
    replay_rejected_total: u64,
) -> ReconcileReport {
    let pending = store.list_pending();
    let pending_ids: HashSet<String> = pending.iter().map(|r| r.request_id.clone()).collect();
    let mut entries = Vec::with_capacity(pending.len() + recently_applied_request_ids.len());

    for req in pending {
        entries.push(ReconcileEntry {
            request_id: req.request_id,
            idempotent_key: req.idempotent_key,
            status: ReconcileStatus::Pending,
        });
    }

    for request_id in recently_applied_request_ids {
        let status = if pending_ids.contains(request_id) {
            ReconcileStatus::Unknown
        } else {
            ReconcileStatus::Applied
        };
        entries.push(ReconcileEntry {
            request_id: request_id.clone(),
            idempotent_key: String::new(),
            status,
        });
    }

    ReconcileReport {
        entries,
        replay_retry_later_total,
        replay_rejected_total,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::availability::{InMemoryQueueStore, QueueStore, QueuedRequest};

    #[test]
    fn reconcile_reports_pending_and_applied() {
        let mut store = InMemoryQueueStore::new();
        store
            .enqueue(QueuedRequest {
                request_id: "req-pending".to_string(),
                idempotent_key: "id-pending".to_string(),
                created_unix_ms: 1,
                payload: vec![1],
            })
            .expect("enqueue pending");

        let report = build_reconcile_report(&store, &["req-applied".to_string()]);

        assert_eq!(report.entries.len(), 2);
        assert!(report
            .entries
            .iter()
            .any(|e| { e.request_id == "req-pending" && e.status == ReconcileStatus::Pending }));
        assert!(report
            .entries
            .iter()
            .any(|e| { e.request_id == "req-applied" && e.status == ReconcileStatus::Applied }));

        let stats = report.stats();
        assert_eq!(stats.pending, 1);
        assert_eq!(stats.applied, 1);
        assert_eq!(stats.retry_later, 0);
        assert_eq!(stats.rejected, 0);
        assert_eq!(stats.unknown, 0);
    }

    #[test]
    fn reconcile_report_includes_replay_retry_and_rejected_stats() {
        let store = InMemoryQueueStore::new();
        let report = build_reconcile_report_with_replay(&store, &["req-applied".to_string()], 2, 1);

        let stats = report.stats();
        assert_eq!(stats.pending, 0);
        assert_eq!(stats.applied, 1);
        assert_eq!(stats.retry_later, 2);
        assert_eq!(stats.rejected, 1);
        assert_eq!(stats.unknown, 0);
    }
}
