//! Original outbound bytes remain charged while any relay outcome is pending.
//! Reservations identify queue entries, not peer delivery or application ACKs.

use super::QueueLimits;
use anyhow::{bail, Context, Result};
use std::collections::{BTreeMap, VecDeque};
use std::time::{Duration, Instant};

struct Entry {
    id: u64,
    bytes: Vec<u8>,
    size: usize,
    enqueued: Instant,
    reservation: Option<u64>,
}

pub(super) struct OutboundQueues {
    peers: BTreeMap<String, VecDeque<Entry>>,
    pub(super) count: usize,
    pub(super) bytes: usize,
    turn: usize,
    next_id: u64,
    limits: QueueLimits,
}

impl OutboundQueues {
    pub(super) fn new(peers: &[String], limits: QueueLimits) -> Self {
        Self {
            peers: peers
                .iter()
                .map(|peer| (peer.clone(), VecDeque::new()))
                .collect(),
            count: 0,
            bytes: 0,
            turn: 0,
            next_id: 1,
            limits,
        }
    }

    pub(super) fn contains_peer(&self, peer: &str) -> bool {
        self.peers.contains_key(peer)
    }

    /// A failed admission returns the exact original and changes no counters,
    /// queue order, fair turn, or ID allocation. Only the owner removes items.
    pub(super) fn push(
        &mut self,
        peer: &str,
        bytes: Vec<u8>,
        size: usize,
        now: Instant,
    ) -> Result<(), Vec<u8>> {
        let Some(queue) = self.peers.get_mut(peer) else {
            return Err(bytes);
        };
        let Some(next_id) = self.next_id.checked_add(1) else {
            return Err(bytes);
        };
        let peer_bytes: usize = queue.iter().map(|entry| entry.size).sum();
        if size != bytes.len()
            || self.count >= self.limits.max_messages
            || queue.len() >= self.limits.peer_max_messages
            || size > self.limits.max_bytes.saturating_sub(self.bytes)
            || size > self.limits.peer_max_bytes.saturating_sub(peer_bytes)
        {
            return Err(bytes);
        }
        queue.push_back(Entry {
            id: self.next_id,
            bytes,
            size,
            enqueued: now,
            reservation: None,
        });
        self.next_id = next_id;
        self.count += 1;
        self.bytes += size;
        Ok(())
    }

    /// An in-flight original cannot expire, even when its TTL passes. Ready
    /// entries behind it can expire independently without disturbing its ID.
    pub(super) fn expire(&mut self, now: Instant, ttl: Duration) -> u64 {
        let mut removed = 0usize;
        let mut removed_bytes = 0usize;
        for queue in self.peers.values_mut() {
            queue.retain(|entry| {
                let expired = entry.reservation.is_none()
                    && now.saturating_duration_since(entry.enqueued) >= ttl;
                if expired {
                    removed += 1;
                    removed_bytes += entry.size;
                }
                !expired
            });
        }
        self.count -= removed;
        self.bytes -= removed_bytes;
        removed as u64
    }

    /// The owner expires ready entries immediately before reserving. Its
    /// eligibility predicate also applies the current peer generation and
    /// available per-peer/global in-flight window. Reserving neither releases
    /// queue capacity nor removes the original; the clone is only for sealing.
    pub(super) fn reserve_next(
        &mut self,
        mut eligible: impl FnMut(&str) -> bool,
        reservation: u64,
    ) -> Option<(String, u64, Vec<u8>)> {
        // Reservations must be unique among outstanding entries, independently
        // of the client's wire ticket. Fail closed without consuming a turn.
        if self.peers.values().any(|queue| {
            queue
                .iter()
                .any(|entry| entry.reservation == Some(reservation))
        }) {
            return None;
        }
        let selected = self
            .peers
            .iter()
            .enumerate()
            .skip(self.turn)
            .chain(self.peers.iter().enumerate().take(self.turn))
            .find_map(|(index, (peer, queue))| {
                let position = queue.iter().position(|entry| entry.reservation.is_none())?;
                eligible(peer).then(|| (index, peer.clone(), position))
            });
        let (index, peer, position) = selected?;
        let queue = self.peers.get_mut(&peer)?;
        let entry = queue.get_mut(position)?;
        entry.reservation = Some(reservation);
        let result = (peer, entry.id, entry.bytes.clone());
        self.turn = (index + 1) % self.peers.len();
        Some(result)
    }

    /// Readiness does not advance the fair turn. In-flight-only, expired-only,
    /// inactive or window-blocked queues must not perpetually wake a socket.
    pub(super) fn has_ready(
        &self,
        mut eligible: impl FnMut(&str) -> bool,
        now: Instant,
        ttl: Duration,
    ) -> bool {
        self.peers.iter().any(|(peer, queue)| {
            queue.iter().any(|entry| {
                entry.reservation.is_none() && now.saturating_duration_since(entry.enqueued) < ttl
            }) && eligible(peer)
        })
    }

    /// Settle one exact original. The caller has already authenticated and
    /// correlated the outcome with its connection and E2E generation. A reject
    /// releases only this reservation, retaining FIFO position and original TTL.
    pub(super) fn settle_exact(
        &mut self,
        peer: &str,
        id: u64,
        reservation: u64,
        accepted: bool,
    ) -> Result<()> {
        let queue = self.peers.get_mut(peer).context("unknown outbound peer")?;
        let position = queue
            .iter()
            .position(|entry| entry.id == id)
            .context("outbound outcome entry is missing")?;
        if queue[position].reservation != Some(reservation) {
            bail!("outbound outcome reservation mismatch");
        }
        if accepted {
            let entry = queue
                .remove(position)
                .context("outbound entry disappeared")?;
            self.count -= 1;
            self.bytes -= entry.size;
        } else {
            queue[position].reservation = None;
        }
        Ok(())
    }

    /// Called only when the sole connection owner has retired its old client
    /// and outcome table. No old outcome may then be settled against new tags.
    /// Original timestamps, IDs, queue order and both budgets are unchanged.
    pub(super) fn release_all(&mut self) {
        for queue in self.peers.values_mut() {
            for entry in queue {
                entry.reservation = None;
            }
        }
    }

    #[cfg(test)]
    pub(super) fn turn(&self) -> usize {
        self.turn
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn queues(limits: QueueLimits) -> OutboundQueues {
        OutboundQueues::new(&["a".into(), "b".into()], limits)
    }

    fn reserve(queues: &mut OutboundQueues, tag: u64) -> (String, u64, Vec<u8>) {
        queues.reserve_next(|_| true, tag).unwrap()
    }

    fn assert_accounting(queues: &OutboundQueues) {
        assert_eq!(
            queues.count,
            queues.peers.values().map(VecDeque::len).sum::<usize>()
        );
        assert_eq!(
            queues.bytes,
            queues
                .peers
                .values()
                .flat_map(|queue| queue.iter())
                .map(|entry| entry.bytes.len())
                .sum::<usize>()
        );
    }

    #[test]
    fn out_of_order_outcomes_remove_only_the_exact_reserved_original() {
        let mut queues = queues(QueueLimits::default());
        let now = Instant::now();
        for bytes in [vec![1], vec![2; 2], vec![3; 3]] {
            queues.push("a", bytes.clone(), bytes.len(), now).unwrap();
        }
        let first = reserve(&mut queues, 101);
        let second = reserve(&mut queues, 102);
        let third = reserve(&mut queues, 103);
        assert_eq!((queues.count, queues.bytes), (3, 6));
        assert_eq!(
            (&first.2, &second.2, &third.2),
            (&vec![1], &vec![2; 2], &vec![3; 3])
        );
        queues.settle_exact("a", third.1, 103, true).unwrap();
        assert_eq!((queues.count, queues.bytes), (2, 3));
        queues.settle_exact("a", first.1, 101, true).unwrap();
        queues.settle_exact("a", second.1, 102, true).unwrap();
        assert_eq!((queues.count, queues.bytes), (0, 0));
        assert!(queues.settle_exact("a", third.1, 103, true).is_err());
        assert_accounting(&queues);
    }

    #[test]
    fn wrong_peer_id_or_tag_cannot_release_budget_or_mutate_a_reservation() {
        let mut queues = queues(QueueLimits::default());
        queues.push("a", vec![7; 3], 3, Instant::now()).unwrap();
        let (_, id, _) = reserve(&mut queues, 10);
        for (peer, entry_id, tag) in [
            ("b", id, 10),
            ("missing", id, 10),
            ("a", id + 1, 10),
            ("a", id, 11),
        ] {
            assert!(queues.settle_exact(peer, entry_id, tag, true).is_err());
            assert!(queues.settle_exact(peer, entry_id, tag, false).is_err());
            assert_eq!((queues.count, queues.bytes), (1, 3));
            assert_eq!(queues.peers["a"][0].reservation, Some(10));
        }
        queues.settle_exact("a", id, 10, true).unwrap();
        assert_accounting(&queues);
    }

    #[test]
    fn expiry_skips_in_flight_but_removes_ready_entries_behind_it() {
        let mut queues = queues(QueueLimits::default());
        let now = Instant::now();
        let ttl = Duration::from_secs(1);
        let old = now - ttl;
        queues.push("a", vec![1; 2], 2, old).unwrap();
        queues.push("a", vec![2; 3], 3, old).unwrap();
        queues.push("a", vec![3; 4], 4, now).unwrap();
        let (_, id, _) = reserve(&mut queues, 11);
        assert_eq!(queues.expire(now, ttl), 1);
        assert_eq!((queues.count, queues.bytes), (2, 6));
        assert_eq!(queues.peers["a"][0].id, id);
        assert_eq!(queues.peers["a"][0].enqueued, old);
        queues.settle_exact("a", id, 11, true).unwrap();
        assert_eq!((queues.count, queues.bytes), (1, 4));
        assert_accounting(&queues);
    }

    #[test]
    fn rejection_keeps_original_timestamp_and_does_not_undo_later_acceptance() {
        let mut queues = queues(QueueLimits::default());
        let now = Instant::now();
        let ttl = Duration::from_secs(1);
        queues.push("a", vec![1], 1, now).unwrap();
        queues.push("a", vec![2; 2], 2, now).unwrap();
        let (_, first, _) = reserve(&mut queues, 20);
        let (_, second, _) = reserve(&mut queues, 21);
        queues.settle_exact("a", first, 20, false).unwrap();
        queues.settle_exact("a", second, 21, true).unwrap();
        assert_eq!((queues.count, queues.bytes), (1, 1));
        let (_, retried, original) = reserve(&mut queues, 22);
        assert_eq!((retried, original), (first, vec![1]));
        assert!(queues.settle_exact("a", first, 20, true).is_err());
        queues.settle_exact("a", first, 22, false).unwrap();
        assert_eq!(queues.peers["a"][0].enqueued, now);
        assert_eq!(queues.expire(now + ttl, ttl), 1);
        assert_accounting(&queues);
    }

    #[test]
    fn reservations_stay_charged_against_global_and_peer_count_and_byte_limits() {
        let limits = QueueLimits {
            max_messages: 2,
            max_bytes: 5,
            peer_max_messages: 1,
            peer_max_bytes: 3,
        };
        let mut queues = queues(limits);
        let now = Instant::now();
        queues.push("a", vec![1; 3], 3, now).unwrap();
        let (_, first, _) = reserve(&mut queues, 1);
        assert_eq!(queues.push("a", vec![2], 1, now), Err(vec![2]));
        assert_eq!(queues.push("b", vec![3; 3], 3, now), Err(vec![3; 3]));
        queues.push("b", vec![4; 2], 2, now).unwrap();
        reserve(&mut queues, 2);
        assert_eq!(queues.push("b", vec![5], 1, now), Err(vec![5]));
        assert_eq!((queues.count, queues.bytes), (2, 5));
        queues.settle_exact("a", first, 1, false).unwrap();
        assert_eq!(queues.push("a", vec![6], 1, now), Err(vec![6]));
        assert_accounting(&queues);
    }

    #[test]
    fn all_in_flight_or_ineligible_or_expired_queues_do_not_request_another_wake() {
        let mut queues = queues(QueueLimits::default());
        let now = Instant::now();
        let ttl = Duration::from_secs(1);
        queues.push("a", vec![1], 1, now).unwrap();
        assert!(!queues.has_ready(|_| false, now, ttl));
        assert!(queues.has_ready(|peer| peer == "a", now, ttl));
        reserve(&mut queues, 1);
        assert!(!queues.has_ready(|_| true, now, ttl));
        queues.push("a", vec![2], 1, now - ttl).unwrap();
        assert!(!queues.has_ready(|_| true, now, ttl));
        queues.push("b", vec![3], 1, now).unwrap();
        let turn = queues.turn();
        for _ in 0..5 {
            assert!(queues.has_ready(|peer| peer == "b", now, ttl));
            assert_eq!(queues.turn(), turn);
        }
        assert_eq!(queues.expire(now, ttl), 1);
        assert_eq!(reserve(&mut queues, 2).0, "b");
        assert!(!queues.has_ready(|_| true, now, ttl));
        assert_accounting(&queues);
    }

    #[test]
    fn fair_turns_skip_reserved_fronts_without_reusing_a_tag() {
        let mut queues = queues(QueueLimits::default());
        let now = Instant::now();
        for (peer, bytes) in [("a", vec![1]), ("a", vec![2]), ("b", vec![3])] {
            queues.push(peer, bytes, 1, now).unwrap();
        }
        let first = reserve(&mut queues, 30);
        let turn = queues.turn();
        assert!(queues.reserve_next(|_| true, 30).is_none());
        assert_eq!(queues.turn(), turn);
        let second = reserve(&mut queues, 31);
        let third = reserve(&mut queues, 32);
        assert_eq!(
            (first.0.as_str(), second.0.as_str(), third.0.as_str()),
            ("a", "b", "a")
        );
        assert_eq!((first.2, second.2, third.2), (vec![1], vec![3], vec![2]));
        assert_ne!(first.1, third.1);
        assert!(queues.reserve_next(|_| true, 33).is_none());
        assert_accounting(&queues);
    }

    #[test]
    fn connection_cleanup_preserves_originals_ids_deadlines_and_budgets() {
        let mut queues = queues(QueueLimits::default());
        let now = Instant::now();
        let ttl = Duration::from_secs(1);
        queues.push("a", vec![1; 2], 2, now).unwrap();
        queues.push("b", vec![2; 3], 3, now).unwrap();
        let first = reserve(&mut queues, 40);
        let second = reserve(&mut queues, 41);
        let next_id = queues.next_id;
        let turn = queues.turn();
        queues.release_all();
        queues.release_all();
        assert_eq!((queues.count, queues.bytes), (2, 5));
        assert_eq!((queues.next_id, queues.turn()), (next_id, turn));
        assert!(queues.settle_exact(&first.0, first.1, 40, true).is_err());
        assert!(queues.settle_exact(&second.0, second.1, 41, true).is_err());
        let retried = reserve(&mut queues, 42);
        assert_eq!(retried, first);
        assert!(queues.settle_exact(&first.0, first.1, 40, true).is_err());
        assert_eq!((queues.count, queues.bytes), (2, 5));
        assert_eq!(queues.peers["a"][0].enqueued, now);
        queues.release_all();
        assert_eq!(queues.expire(now + ttl, ttl), 2);
        queues.push("a", vec![3], 1, now + ttl).unwrap();
        assert!(reserve(&mut queues, 43).1 > second.1);
        assert_accounting(&queues);
    }

    #[test]
    fn failed_admission_does_not_consume_ids_and_exhausted_ids_never_wrap() {
        let mut queues = queues(QueueLimits::default());
        let now = Instant::now();
        assert!(queues.contains_peer("a"));
        assert!(!queues.contains_peer("missing"));
        let next_id = queues.next_id;
        assert_eq!(queues.push("missing", vec![1], 1, now), Err(vec![1]));
        assert_eq!(queues.push("a", vec![2], 0, now), Err(vec![2]));
        assert_eq!(queues.next_id, next_id);
        queues.next_id = u64::MAX - 1;
        queues.push("a", vec![3], 1, now).unwrap();
        assert_eq!(reserve(&mut queues, 1).1, u64::MAX - 1);
        assert_eq!(queues.push("a", vec![4], 1, now), Err(vec![4]));
        queues.release_all();
        assert_eq!(
            queues.expire(now + Duration::from_secs(1), Duration::from_secs(1)),
            1
        );
        assert_eq!(queues.push("b", vec![5], 1, now), Err(vec![5]));
        assert_eq!((queues.count, queues.bytes), (0, 0));
    }
}
