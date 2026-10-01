//! Bounded transaction staging and fair transport attempts, not authentication,
//! durable admission or permission to vote. The owner validates every raw and
//! recipient ACK; this scheduler only remembers explicitly verified peer receipts
//! in memory, separately from local transport queue acceptance.

use crate::native_block_seal::service_config::FreshTransactionTransportV1;
use crate::product_mainline_overlay::{
    ProductMainlineOverlayInboundV1 as Inbound, ProductMainlineOverlayPayloadClassV1 as Class,
};
use crate::tx_ingress::fresh_pool::{PendingTransaction, MAX_RAW_BYTES};
use anyhow::{bail, Result};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::collections::{BTreeMap, BTreeSet, VecDeque};
use std::time::{Duration, Instant};

const MAX_STAGED_ENTRIES: usize = 1024;
const MAX_STAGED_BYTES: usize = 16 * 1024 * 1024;
const WINDOW: Duration = Duration::from_secs(1);

struct Budget {
    since: Instant,
    attempts: usize,
}

impl Budget {
    fn new(now: Instant) -> Self {
        Self {
            since: now,
            attempts: 0,
        }
    }

    fn refresh(&mut self, now: Instant) -> bool {
        // The lifecycle checks monotonicity. An earlier supplied time must not
        // reset quotas here, either, and no Instant + Duration is needed.
        if now
            .checked_duration_since(self.since)
            .is_some_and(|age| age >= WINDOW)
        {
            self.since = now;
            self.attempts = 0;
            true
        } else {
            false
        }
    }
}

struct Peer {
    id: String,
    queued: VecDeque<Inbound>,
    ingress: Budget,
    gossip: Budget,
    cursor: usize,
    retry: Option<[u8; 32]>,
    sent: BTreeSet<[u8; 32]>,
    acked: BTreeMap<[u8; 32], [u8; 32]>,
}

#[derive(Default)]
struct Counters {
    staging_rejected: u64,
    ingress_attempts: u64,
    ingress_attempt_bytes: u64,
    rate_drops: u64,
    gossip_attempts: u64,
    gossip_attempt_bytes: u64,
    submitted: u64,
    backpressure: u64,
    recipient_acks_accepted: u64,
    recipient_acks_rejected: u64,
}

pub(super) struct TransactionTransport {
    limits: FreshTransactionTransportV1,
    peers: Vec<Peer>,
    peer_index: BTreeMap<String, usize>,
    ingress_cursor: usize,
    gossip_cursor: usize,
    queued_entries: usize,
    queued_bytes: usize,
    counters: Counters,
}

impl TransactionTransport {
    /// Limits and the pinned peer set have already passed service validation.
    pub(super) fn new(
        peers: impl Iterator<Item = String>,
        limits: FreshTransactionTransportV1,
        now: Instant,
    ) -> Self {
        let peers: Vec<_> = peers
            .collect::<BTreeSet<_>>()
            .into_iter()
            .map(|id| Peer {
                id,
                queued: VecDeque::new(),
                ingress: Budget::new(now),
                gossip: Budget::new(now),
                cursor: 0,
                retry: None,
                sent: BTreeSet::new(),
                acked: BTreeMap::new(),
            })
            .collect();
        let peer_index = peers
            .iter()
            .enumerate()
            .map(|(index, peer)| (peer.id.clone(), index))
            .collect();
        Self {
            limits,
            peers,
            peer_index,
            ingress_cursor: 0,
            gossip_cursor: 0,
            queued_entries: 0,
            queued_bytes: 0,
            counters: Counters::default(),
        }
    }

    pub(super) fn enqueue(&mut self, event: Inbound) -> bool {
        let bytes = event.frame.payload.len();
        let index = self.peer_index.get(&event.source_peer_id).copied();
        if event.payload_class != Class::NativeTransaction
            || bytes == 0
            || bytes > MAX_RAW_BYTES
            || index.is_none()
            || self.queued_entries >= MAX_STAGED_ENTRIES
            || bytes > MAX_STAGED_BYTES.saturating_sub(self.queued_bytes)
            || index
                .is_some_and(|index| self.peers[index].queued.len() >= self.limits.per_peer_queue)
        {
            self.counters.staging_rejected = self.counters.staging_rejected.saturating_add(1);
            return false;
        }
        self.peers[index.expect("pinned peer checked")]
            .queued
            .push_back(event);
        self.queued_entries += 1;
        self.queued_bytes += bytes;
        true
    }

    /// Both admitted and rate-rejected raws consume this poll's work budget.
    /// Work not reached within the count/byte budget remains staged. Exceeding
    /// the per-source window retains the original DROP (not retry) semantics.
    pub(super) fn drain(&mut self, now: Instant) -> (Vec<Inbound>, usize) {
        let mut result = Vec::new();
        let mut rejected = 0;
        let (mut attempts, mut bytes, mut idle) = (0, 0, 0);
        let mut byte_deferred = None;
        while attempts < self.limits.ingress_per_poll
            && bytes < self.limits.bytes_per_poll
            && idle < self.peers.len()
        {
            let index = self.ingress_cursor;
            self.ingress_cursor = (index + 1) % self.peers.len();
            let peer = &mut self.peers[index];
            let Some(event) = peer.queued.front() else {
                idle += 1;
                continue;
            };
            let size = event.frame.payload.len();
            if size > self.limits.bytes_per_poll - bytes {
                byte_deferred.get_or_insert(index);
                idle += 1;
                continue;
            }
            let event = peer.queued.pop_front().expect("front exists");
            self.queued_entries -= 1;
            self.queued_bytes -= size;
            attempts += 1;
            bytes += size;
            idle = 0;
            self.counters.ingress_attempts = self.counters.ingress_attempts.saturating_add(1);
            self.counters.ingress_attempt_bytes = self
                .counters
                .ingress_attempt_bytes
                .saturating_add(size as u64);
            peer.ingress.refresh(now);
            if peer.ingress.attempts >= self.limits.ingress_per_source_per_second {
                rejected += 1;
                self.counters.rate_drops = self.counters.rate_drops.saturating_add(1);
            } else {
                peer.ingress.attempts += 1;
                result.push(event);
            }
        }
        // A perpetually refilled small queue must not strand a larger head.
        // Each raw fits a fresh poll's byte budget, so give the first deferred
        // peer that fresh budget rather than only rotating past empty queues.
        if let Some(index) = byte_deferred {
            self.ingress_cursor = index;
        }
        (result, rejected)
    }

    /// The owner must first verify the ACK signature, chain, disposition and
    /// complete delivery binding against this exact current durable pool entry.
    /// Nothing here grants admission, finality or permission to remove that entry.
    pub(super) fn acknowledge(
        &mut self,
        peer: &str,
        entry: &PendingTransaction,
        payload_sha256: [u8; 32],
    ) -> bool {
        if !self.limits.durable_receipts {
            self.counters.recipient_acks_rejected =
                self.counters.recipient_acks_rejected.saturating_add(1);
            return false;
        }
        let Some(index) = self.peer_index.get(peer).copied() else {
            self.counters.recipient_acks_rejected =
                self.counters.recipient_acks_rejected.saturating_add(1);
            return false;
        };
        let peer = &mut self.peers[index];
        if entry.raw.is_empty()
            || entry.raw.len() > MAX_RAW_BYTES
            || <[u8; 32]>::from(Sha256::digest(&entry.raw)) != payload_sha256
            || (peer.acked.len() >= MAX_STAGED_ENTRIES && !peer.acked.contains_key(&entry.hash))
        {
            self.counters.recipient_acks_rejected =
                self.counters.recipient_acks_rejected.saturating_add(1);
            return false;
        }
        peer.acked.insert(entry.hash, payload_sha256);
        if peer.retry == Some(entry.hash) {
            peer.retry = None;
        }
        self.counters.recipient_acks_accepted =
            self.counters.recipient_acks_accepted.saturating_add(1);
        true
    }

    /// A new/failed session cannot inherit an assumption of remote retention.
    /// Retry within the remaining original rate budget; do not reset quotas or
    /// discard staged inbound transactions when clearing peer receipt knowledge.
    pub(super) fn reset_peer_receipts(&mut self, peer: &str) {
        if !self.limits.durable_receipts {
            return;
        }
        if let Some(index) = self.peer_index.get(peer).copied() {
            self.peers[index].acked.clear();
            self.peers[index].sent.clear();
        }
    }

    pub(super) fn reset_all_receipts(&mut self) {
        if !self.limits.durable_receipts {
            return;
        }
        for peer in &mut self.peers {
            peer.acked.clear();
            peer.sent.clear();
        }
    }

    fn retain_live_receipts(&mut self, entries: &[&PendingTransaction]) {
        if !self.peers.iter().any(|peer| !peer.acked.is_empty()) {
            return;
        }
        let live: BTreeMap<_, _> = entries.iter().map(|entry| (entry.hash, *entry)).collect();
        // Hash only entries referenced by a receipt, at most once per call even
        // when several peers acknowledged them. No cross-poll validation cache.
        let mut digests = BTreeMap::<[u8; 32], [u8; 32]>::new();
        for peer in &mut self.peers {
            peer.acked.retain(|hash, expected| {
                let Some(entry) = live.get(hash) else {
                    return false;
                };
                if entry.raw.is_empty() || entry.raw.len() > MAX_RAW_BYTES {
                    return false;
                }
                let actual = digests
                    .entry(*hash)
                    .or_insert_with(|| Sha256::digest(&entry.raw).into());
                actual == expected
            });
        }
    }

    /// Each call has a separate outbound count/byte budget. False means local
    /// transport backpressure, consumes an attempt, and preserves the exact
    /// hash for this peer. True is queue acceptance only, never durable receipt.
    /// The source pool is borrowed and no success or error removes its entries.
    pub(super) fn gossip(
        &mut self,
        entries: &[&PendingTransaction],
        now: Instant,
        mut send: impl FnMut(&str, &PendingTransaction) -> Result<bool>,
    ) -> Result<()> {
        if entries.len() > MAX_STAGED_ENTRIES {
            bail!("transaction gossip exceeds the durable pool entry bound");
        }
        self.retain_live_receipts(entries);
        if entries.is_empty() || self.peers.is_empty() {
            return Ok(());
        }
        let mut blocked = vec![false; self.peers.len()];
        let (mut attempts, mut bytes, mut idle) = (0, 0, 0);
        let mut byte_deferred = None;
        while attempts < self.limits.gossip_per_poll
            && bytes < self.limits.bytes_per_poll
            && idle < self.peers.len()
        {
            let index = self.gossip_cursor;
            self.gossip_cursor = (index + 1) % self.peers.len();
            let peer = &mut self.peers[index];
            if peer.gossip.refresh(now) {
                peer.sent.clear();
            }
            if blocked[index]
                || peer.gossip.attempts >= self.limits.gossip_per_peer_per_second
                || peer.sent.len() >= MAX_STAGED_ENTRIES
            {
                idle += 1;
                continue;
            }
            let retry = peer
                .retry
                .filter(|hash| !peer.acked.contains_key(hash))
                .and_then(|hash| entries.iter().position(|entry| entry.hash == hash));
            if retry.is_none() {
                peer.retry = None;
            }
            let selected = retry.or_else(|| {
                (0..entries.len())
                    .map(|offset| (peer.cursor + offset) % entries.len())
                    .find(|position| {
                        !peer.sent.contains(&entries[*position].hash)
                            && !peer.acked.contains_key(&entries[*position].hash)
                    })
            });
            let Some(position) = selected else {
                blocked[index] = true;
                idle += 1;
                continue;
            };
            let entry = entries[position];
            let size = entry.raw.len();
            if size == 0 || size > MAX_RAW_BYTES {
                bail!("transaction gossip requires bounded authenticated pool entries");
            }
            if size > self.limits.bytes_per_poll - bytes {
                byte_deferred.get_or_insert(index);
                blocked[index] = true;
                idle += 1;
                continue;
            }
            attempts += 1;
            bytes += size;
            idle = 0;
            peer.gossip.attempts += 1;
            self.counters.gossip_attempts = self.counters.gossip_attempts.saturating_add(1);
            self.counters.gossip_attempt_bytes = self
                .counters
                .gossip_attempt_bytes
                .saturating_add(size as u64);
            // Set before invoking transport so even an error cannot advance the
            // selected item. The lifecycle will fail closed on that error.
            peer.retry = Some(entry.hash);
            if send(&peer.id, entry)? {
                peer.retry = None;
                peer.sent.insert(entry.hash);
                peer.cursor = (position + 1) % entries.len();
                self.counters.submitted = self.counters.submitted.saturating_add(1);
            } else {
                blocked[index] = true;
                self.counters.backpressure = self.counters.backpressure.saturating_add(1);
            }
        }
        if let Some(index) = byte_deferred {
            self.gossip_cursor = index;
        }
        Ok(())
    }

    pub(super) fn status_json(&self) -> Value {
        json!({
            "limits": {
                "per_peer_queue":self.limits.per_peer_queue,
                "ingress_per_source_per_second":self.limits.ingress_per_source_per_second,
                "ingress_per_poll":self.limits.ingress_per_poll,
                "gossip_per_peer_per_second":self.limits.gossip_per_peer_per_second,
                "gossip_per_poll":self.limits.gossip_per_poll,
                "durable_receipts":self.limits.durable_receipts,
                "bytes_per_poll":self.limits.bytes_per_poll,
                "global_staged_entries":MAX_STAGED_ENTRIES,
                "global_staged_bytes":MAX_STAGED_BYTES,
                "max_acked_transactions_per_peer":MAX_STAGED_ENTRIES,
            },
            "queued_entries":self.queued_entries,"queued_bytes":self.queued_bytes,
            "staging_rejected":self.counters.staging_rejected,
            "ingress_attempts":self.counters.ingress_attempts,
            "ingress_attempt_bytes":self.counters.ingress_attempt_bytes,
            "rate_drops":self.counters.rate_drops,
            "gossip_attempts":self.counters.gossip_attempts,
            "gossip_attempt_bytes":self.counters.gossip_attempt_bytes,
            "submitted":self.counters.submitted,"backpressure":self.counters.backpressure,
            "submitted_is_transport_queue_acceptance":true,
            "recipient_acks_accepted":self.counters.recipient_acks_accepted,
            "recipient_acks_rejected":self.counters.recipient_acks_rejected,
            "acked_peer_transactions":self.peers.iter().map(|peer| peer.acked.len()).sum::<usize>(),
            "recipient_ack_records_are_volatile":true,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use novovm_network::{NovoRudpTransportFrameKindV0, NovoRudpTransportFrameV0};

    fn limits() -> FreshTransactionTransportV1 {
        FreshTransactionTransportV1 {
            durable_receipts: false,
            per_peer_queue: 4,
            ingress_per_source_per_second: 8,
            ingress_per_poll: 16,
            gossip_per_peer_per_second: 4,
            gossip_per_poll: 16,
            bytes_per_poll: MAX_RAW_BYTES,
        }
    }

    fn transport(
        peers: &[&str],
        limits: FreshTransactionTransportV1,
        now: Instant,
    ) -> TransactionTransport {
        TransactionTransport::new(peers.iter().map(|peer| (*peer).into()), limits, now)
    }

    fn inbound(peer: &str, marker: u8, bytes: usize) -> Inbound {
        Inbound {
            payload_class: Class::NativeTransaction,
            object_hash: [marker; 32],
            delivery_id: [marker; 32],
            payload_sha256: [marker; 32],
            original_frame_sequence: 1,
            source_peer_id: peer.into(),
            frame: NovoRudpTransportFrameV0::new(
                NovoRudpTransportFrameKindV0::Data,
                [1; 16],
                1,
                1,
                1,
                1,
                vec![marker; bytes],
            ),
        }
    }

    fn transaction(marker: u8, bytes: usize) -> PendingTransaction {
        PendingTransaction {
            hash: [marker; 32],
            raw: vec![marker; bytes],
            identity: format!("signer-{marker}"),
            nonce: 0,
        }
    }

    #[test]
    fn staging_rejects_unpinned_empty_oversize_wrong_class_and_peer_overflow() {
        let now = Instant::now();
        let mut scheduler = transport(&["a", "b", "a"], limits(), now);
        assert!(!scheduler.enqueue(inbound("unknown", 1, 1)));
        assert!(!scheduler.enqueue(inbound("a", 1, 0)));
        assert!(!scheduler.enqueue(inbound("a", 1, MAX_RAW_BYTES + 1)));
        let mut wrong = inbound("a", 1, 1);
        wrong.payload_class = Class::NativeSeal;
        assert!(!scheduler.enqueue(wrong));
        for marker in 0..4 {
            assert!(scheduler.enqueue(inbound("a", marker, 2)));
        }
        assert!(!scheduler.enqueue(inbound("a", 4, 2)));
        assert!(scheduler.enqueue(inbound("b", 5, 3)));
        let status = scheduler.status_json();
        assert_eq!(status["queued_entries"], 5);
        assert_eq!(status["queued_bytes"], 11);
        assert_eq!(status["staging_rejected"], 5);
        assert_eq!(scheduler.peers.len(), 2);
    }

    #[test]
    fn staging_global_entry_and_byte_caps_are_independent() {
        let now = Instant::now();
        let mut config = limits();
        config.per_peer_queue = 256;
        config.ingress_per_poll = 256;
        let mut scheduler = transport(&["a", "b", "c", "d", "e"], config, now);
        for peer in ["a", "b", "c", "d"] {
            for _ in 0..256 {
                assert!(scheduler.enqueue(inbound(peer, 1, 1)));
            }
        }
        assert!(!scheduler.enqueue(inbound("e", 1, 1)));
        assert_eq!(scheduler.status_json()["queued_entries"], 1024);
        scheduler.drain(now);
        assert!(scheduler.enqueue(inbound("e", 1, 1)));
        let mut scheduler = transport(&["a", "b"], config, now);
        for _ in 0..128 {
            assert!(scheduler.enqueue(inbound("a", 1, MAX_RAW_BYTES)));
            assert!(scheduler.enqueue(inbound("b", 1, MAX_RAW_BYTES)));
        }
        assert_eq!(scheduler.status_json()["queued_bytes"], MAX_STAGED_BYTES);
        assert!(!scheduler.enqueue(inbound("a", 1, 1)));
        scheduler.drain(now);
        assert!(scheduler.enqueue(inbound("a", 1, MAX_RAW_BYTES)));
    }

    #[test]
    fn ingress_round_robin_counts_rate_drops_as_work_and_defers_unprocessed() {
        let now = Instant::now();
        let mut config = limits();
        config.ingress_per_source_per_second = 1;
        config.ingress_per_poll = 2;
        let mut scheduler = transport(&["a", "b"], config, now);
        for marker in 1..=3 {
            for peer in ["a", "b"] {
                assert!(scheduler.enqueue(inbound(peer, marker, 2)));
            }
        }
        let (events, rejected) = scheduler.drain(now);
        assert_eq!(
            events
                .iter()
                .map(|event| event.source_peer_id.as_str())
                .collect::<Vec<_>>(),
            ["a", "b"]
        );
        assert_eq!(rejected, 0);
        assert_eq!(scheduler.status_json()["queued_entries"], 4);
        let (events, rejected) = scheduler.drain(now);
        assert!(events.is_empty());
        assert_eq!(rejected, 2);
        assert_eq!(scheduler.status_json()["queued_entries"], 2);
        assert_eq!(scheduler.status_json()["ingress_attempt_bytes"], 8);
        let (events, rejected) = scheduler.drain(now + WINDOW);
        assert_eq!(events.len(), 2);
        assert_eq!(rejected, 0);
        assert_eq!(scheduler.status_json()["queued_bytes"], 0);
        assert_eq!(scheduler.status_json()["ingress_attempts"], 6);
        assert_eq!(scheduler.status_json()["rate_drops"], 2);
    }

    #[test]
    fn ingress_byte_cap_defers_large_front_but_services_other_peers() {
        let now = Instant::now();
        let mut scheduler = transport(&["a", "b", "c"], limits(), now);
        assert!(scheduler.enqueue(inbound("a", 1, 40_000)));
        assert!(scheduler.enqueue(inbound("b", 2, 40_000)));
        assert!(scheduler.enqueue(inbound("c", 3, 1)));
        let (events, rejected) = scheduler.drain(now);
        assert_eq!(
            events
                .iter()
                .map(|event| event.object_hash[0])
                .collect::<Vec<_>>(),
            [1, 3]
        );
        assert_eq!(rejected, 0);
        assert_eq!(scheduler.status_json()["queued_bytes"], 40_000);
        let (events, _) = scheduler.drain(now);
        assert_eq!(events.len(), 1);
        assert_eq!(events[0].source_peer_id, "b");
    }

    #[test]
    fn empty_peers_and_empty_pool_never_call_transport() {
        let now = Instant::now();
        let entry = transaction(1, 1);
        let mut scheduler = transport(&[], limits(), now);
        assert_eq!(scheduler.drain(now), (vec![], 0));
        scheduler
            .gossip(&[&entry], now, |_, _| panic!("no peers"))
            .unwrap();
        let mut scheduler = transport(&["a"], limits(), now);
        scheduler
            .gossip(&[], now, |_, _| panic!("no entries"))
            .unwrap();
    }

    #[test]
    fn gossip_global_count_and_peer_windows_preserve_round_robin() {
        let now = Instant::now();
        let mut config = limits();
        config.gossip_per_poll = 4;
        config.gossip_per_peer_per_second = 2;
        let mut scheduler = transport(&["a", "b"], config, now);
        let entries: Vec<_> = (1..=4).map(|marker| transaction(marker, 1)).collect();
        let refs: Vec<_> = entries.iter().collect();
        let mut sent = Vec::new();
        scheduler
            .gossip(&refs, now, |peer, entry| {
                sent.push((peer.to_owned(), entry.hash[0]));
                Ok(true)
            })
            .unwrap();
        assert_eq!(
            sent,
            [
                ("a".into(), 1),
                ("b".into(), 1),
                ("a".into(), 2),
                ("b".into(), 2)
            ]
        );
        scheduler
            .gossip(&refs, now, |_, _| panic!("window exhausted"))
            .unwrap();
        sent.clear();
        scheduler
            .gossip(&refs, now + WINDOW, |peer, entry| {
                sent.push((peer.to_owned(), entry.hash[0]));
                Ok(true)
            })
            .unwrap();
        assert_eq!(
            sent,
            [
                ("a".into(), 3),
                ("b".into(), 3),
                ("a".into(), 4),
                ("b".into(), 4)
            ]
        );
    }

    #[test]
    fn gossip_successful_hash_is_not_repeated_within_a_window() {
        let now = Instant::now();
        let mut config = limits();
        config.gossip_per_peer_per_second = 8;
        let mut scheduler = transport(&["a", "b"], config, now);
        let entries: Vec<_> = (1..=3).map(|marker| transaction(marker, 1)).collect();
        let refs: Vec<_> = entries.iter().collect();
        let mut count = 0;
        scheduler
            .gossip(&refs, now, |_, _| {
                count += 1;
                Ok(true)
            })
            .unwrap();
        assert_eq!(count, 6);
        scheduler
            .gossip(&refs, now + Duration::from_millis(999), |_, _| {
                panic!("duplicate within window")
            })
            .unwrap();
        scheduler
            .gossip(&refs, now + WINDOW, |_, _| {
                count += 1;
                Ok(true)
            })
            .unwrap();
        assert_eq!(count, 12);
    }

    #[test]
    fn backpressure_retains_exact_hash_across_reordering_and_window_rollover() {
        let now = Instant::now();
        let mut config = limits();
        config.gossip_per_poll = 1;
        config.gossip_per_peer_per_second = 1;
        let mut scheduler = transport(&["a"], config, now);
        let first = transaction(1, 1);
        let second = transaction(2, 1);
        scheduler
            .gossip(&[&first, &second], now, |_, entry| {
                assert_eq!(entry.hash, first.hash);
                Ok(false)
            })
            .unwrap();
        scheduler
            .gossip(&[&second, &first], now, |_, _| {
                panic!("false must consume rate quota")
            })
            .unwrap();
        scheduler
            .gossip(&[&second, &first], now + WINDOW, |_, entry| {
                assert_eq!(entry.hash, first.hash);
                Ok(true)
            })
            .unwrap();
        assert_eq!(scheduler.status_json()["gossip_attempts"], 2);
        assert_eq!(scheduler.status_json()["backpressure"], 1);
        assert_eq!(scheduler.status_json()["submitted"], 1);
        assert_eq!(first.raw, vec![1]);
    }

    #[test]
    fn removed_retry_does_not_block_remaining_pool() {
        let now = Instant::now();
        let mut config = limits();
        config.gossip_per_poll = 1;
        let mut scheduler = transport(&["a"], config, now);
        let first = transaction(1, 1);
        let second = transaction(2, 1);
        scheduler
            .gossip(&[&first, &second], now, |_, _| Ok(false))
            .unwrap();
        scheduler
            .gossip(&[&second], now, |_, entry| {
                assert_eq!(entry.hash, second.hash);
                Ok(true)
            })
            .unwrap();
        assert!(scheduler.peers[0].retry.is_none());
    }

    #[test]
    fn blocked_peer_does_not_starve_other_peers_or_repeat_in_same_poll() {
        let now = Instant::now();
        let mut config = limits();
        config.gossip_per_poll = 4;
        let mut scheduler = transport(&["a", "b"], config, now);
        let entries: Vec<_> = (1..=3).map(|marker| transaction(marker, 1)).collect();
        let refs: Vec<_> = entries.iter().collect();
        let mut attempts = Vec::new();
        scheduler
            .gossip(&refs, now, |peer, entry| {
                attempts.push((peer.to_owned(), entry.hash[0]));
                Ok(peer != "a")
            })
            .unwrap();
        assert_eq!(
            attempts,
            [
                ("a".into(), 1),
                ("b".into(), 1),
                ("b".into(), 2),
                ("b".into(), 3)
            ]
        );
        assert_eq!(scheduler.status_json()["gossip_attempts"], 4);
        assert_eq!(scheduler.status_json()["gossip_attempt_bytes"], 4);
        assert_eq!(scheduler.peers[0].retry, Some([1; 32]));
    }

    #[test]
    fn gossip_byte_budget_counts_false_and_defers_other_peer() {
        let now = Instant::now();
        let mut scheduler = transport(&["a", "b"], limits(), now);
        let entry = transaction(1, 40_000);
        let mut attempts = Vec::new();
        scheduler
            .gossip(&[&entry], now, |peer, _| {
                attempts.push(peer.to_owned());
                Ok(false)
            })
            .unwrap();
        assert_eq!(attempts, ["a"]);
        assert_eq!(scheduler.status_json()["gossip_attempt_bytes"], 40_000);
        attempts.clear();
        scheduler
            .gossip(&[&entry], now, |peer, _| {
                attempts.push(peer.to_owned());
                Ok(true)
            })
            .unwrap();
        assert_eq!(attempts, ["b"]);
        assert_eq!(scheduler.status_json()["gossip_attempt_bytes"], 80_000);
    }

    #[test]
    fn transport_error_is_preserved_and_does_not_advance_or_delete() {
        let now = Instant::now();
        let mut scheduler = transport(&["a", "b"], limits(), now);
        let entry = transaction(1, 1);
        let error = scheduler
            .gossip(&[&entry], now, |_, _| {
                Err(std::io::Error::other("transport failure").into())
            })
            .unwrap_err();
        assert!(error.is::<std::io::Error>());
        assert_eq!(error.to_string(), "transport failure");
        assert_eq!(scheduler.peers[0].retry, Some(entry.hash));
        assert_eq!(scheduler.peers[0].cursor, 0);
        assert_eq!(scheduler.status_json()["gossip_attempts"], 1);
        assert_eq!(scheduler.status_json()["submitted"], 0);
        assert_eq!(entry.raw, [1]);
    }

    #[test]
    fn continuously_refilled_small_ingress_cannot_starve_a_large_peer() {
        let now = Instant::now();
        let mut scheduler = transport(&["a", "b", "c"], limits(), now);
        let mut large_sources = Vec::new();
        for _ in 0..6 {
            for (index, (peer, bytes)) in [("a", 40_000), ("b", 40_000), ("c", 1)]
                .into_iter()
                .enumerate()
            {
                if scheduler.peers[index].queued.is_empty() {
                    assert!(scheduler.enqueue(inbound(peer, index as u8, bytes)));
                }
            }
            let (events, rejected) = scheduler.drain(now);
            assert_eq!(rejected, 0);
            assert_eq!(
                events
                    .iter()
                    .map(|event| event.frame.payload.len())
                    .sum::<usize>(),
                40_001
            );
            large_sources.extend(
                events
                    .iter()
                    .filter(|event| event.frame.payload.len() == 40_000)
                    .map(|event| event.source_peer_id.clone()),
            );
        }
        assert_eq!(large_sources, ["a", "b", "a", "b", "a", "b"]);
    }

    #[test]
    fn changing_small_pool_entries_cannot_starve_a_large_gossip_retry() {
        let now = Instant::now();
        let mut config = limits();
        config.gossip_per_peer_per_second = 32;
        let mut scheduler = transport(&["a", "b", "c"], config, now);
        let first = transaction(1, 40_000);
        let second = transaction(2, 40_000);
        // Reach an actual blocked B retry without using the other peers' budget.
        scheduler.gossip_cursor = 1;
        scheduler.limits.gossip_per_poll = 1;
        scheduler
            .gossip(&[&second], now, |peer, _| {
                assert_eq!(peer, "b");
                Ok(false)
            })
            .unwrap();
        scheduler.limits.gossip_per_poll = config.gossip_per_poll;
        scheduler.gossip_cursor = 0;
        scheduler.peers[2].cursor = 2;
        let mut large_sources = Vec::new();
        for marker in 10..16 {
            let small = transaction(marker, 1);
            let mut refs = vec![&first, &second, &small];
            // New small transactions arrive exactly at C's next position;
            // the large pending entries change index but keep their hashes.
            refs.rotate_right((scheduler.peers[2].cursor + 1) % 3);
            let mut bytes = 0;
            scheduler
                .gossip(&refs, now, |peer, entry| {
                    bytes += entry.raw.len();
                    if entry.raw.len() > 1 {
                        large_sources.push(peer.to_owned());
                        assert_eq!(
                            entry.hash,
                            if peer == "a" { first.hash } else { second.hash }
                        );
                        Ok(false)
                    } else {
                        assert_eq!(peer, "c");
                        Ok(true)
                    }
                })
                .unwrap();
            assert_eq!(bytes, 40_001);
        }
        assert_eq!(large_sources, ["a", "b", "a", "b", "a", "b"]);
        assert_eq!(scheduler.peers[1].retry, Some(second.hash));
    }

    #[test]
    fn sent_hash_memory_is_bounded_even_when_the_pool_changes_within_window() {
        let now = Instant::now();
        let mut config = limits();
        config.gossip_per_poll = 1024;
        config.gossip_per_peer_per_second = 4096;
        let mut scheduler = transport(&["a"], config, now);
        let entries: Vec<_> = (0..1024u32)
            .map(|index| {
                let mut entry = transaction(1, 1);
                entry.hash[..4].copy_from_slice(&index.to_le_bytes());
                entry
            })
            .collect();
        let refs: Vec<_> = entries.iter().collect();
        scheduler.gossip(&refs, now, |_, _| Ok(true)).unwrap();
        assert_eq!(scheduler.peers[0].sent.len(), 1024);
        let replacement = transaction(255, 1);
        scheduler
            .gossip(&[&replacement], now, |_, _| panic!("sent set is full"))
            .unwrap();
        scheduler
            .gossip(&[&replacement], now + WINDOW, |_, _| Ok(true))
            .unwrap();
        assert_eq!(scheduler.peers[0].sent.len(), 1);
    }

    fn receipt_transport(peers: &[&str], now: Instant) -> TransactionTransport {
        let mut config = limits();
        config.durable_receipts = true;
        transport(peers, config, now)
    }

    fn raw_digest(entry: &PendingTransaction) -> [u8; 32] {
        Sha256::digest(&entry.raw).into()
    }

    #[test]
    fn recipient_receipts_require_opt_in_pinned_peer_and_matching_bounded_raw() {
        let now = Instant::now();
        let entry = transaction(1, 4);
        let mut disabled = transport(&["a"], limits(), now);
        assert!(!disabled.acknowledge("a", &entry, raw_digest(&entry)));
        assert_eq!(disabled.status_json()["acked_peer_transactions"], 0);
        disabled.gossip(&[&entry], now, |_, _| Ok(true)).unwrap();
        disabled.reset_all_receipts();
        disabled.reset_peer_receipts("a");
        disabled
            .gossip(&[&entry], now, |_, _| panic!("disabled behavior changed"))
            .unwrap();

        let mut scheduler = receipt_transport(&["a"], now);
        assert!(!scheduler.acknowledge("unknown", &entry, raw_digest(&entry)));
        assert!(!scheduler.acknowledge("a", &entry, [0; 32]));
        for entry in [transaction(1, 0), transaction(1, MAX_RAW_BYTES + 1)] {
            assert!(!scheduler.acknowledge("a", &entry, raw_digest(&entry)));
        }
        assert_eq!(scheduler.status_json()["recipient_acks_rejected"], 4);
        assert_eq!(scheduler.status_json()["acked_peer_transactions"], 0);
        assert!(scheduler.acknowledge("a", &entry, raw_digest(&entry)));
        assert_eq!(scheduler.status_json()["recipient_acks_accepted"], 1);
    }

    #[test]
    fn receipt_suppresses_only_its_peer_while_missing_ack_retries_each_window() {
        let now = Instant::now();
        let mut scheduler = receipt_transport(&["a", "b"], now);
        let entry = transaction(1, 4);
        let mut sent = Vec::new();
        scheduler
            .gossip(&[&entry], now, |peer, _| {
                sent.push(peer.to_owned());
                Ok(true)
            })
            .unwrap();
        assert_eq!(sent, ["a", "b"]);
        assert!(scheduler.acknowledge("a", &entry, raw_digest(&entry)));
        for window in 1..=3 {
            sent.clear();
            scheduler
                .gossip(&[&entry], now + WINDOW * window, |peer, _| {
                    sent.push(peer.to_owned());
                    Ok(true)
                })
                .unwrap();
            assert_eq!(sent, ["b"]);
        }
        assert_eq!(entry.raw, [1; 4]);
        assert_eq!(scheduler.status_json()["acked_peer_transactions"], 1);
        assert_eq!(scheduler.status_json()["submitted"], 5);
        assert_eq!(
            scheduler.status_json()["submitted_is_transport_queue_acceptance"],
            true
        );
    }

    #[test]
    fn receipt_for_a_backpressured_retry_does_not_block_the_next_transaction() {
        let now = Instant::now();
        let mut scheduler = receipt_transport(&["a"], now);
        let first = transaction(1, 4);
        let second = transaction(2, 4);
        scheduler
            .gossip(&[&first, &second], now, |_, entry| {
                assert_eq!(entry.hash, first.hash);
                Ok(false)
            })
            .unwrap();
        assert_eq!(scheduler.peers[0].retry, Some(first.hash));
        assert!(scheduler.acknowledge("a", &first, raw_digest(&first)));
        assert!(scheduler.peers[0].retry.is_none());
        let mut sent = Vec::new();
        scheduler
            .gossip(&[&first, &second], now, |_, entry| {
                sent.push(entry.hash);
                Ok(true)
            })
            .unwrap();
        assert_eq!(sent, [second.hash]);
        assert_eq!(scheduler.status_json()["gossip_attempts"], 2);
    }

    #[test]
    fn same_hash_changed_raw_invalidates_receipt_before_selection() {
        let now = Instant::now();
        let mut scheduler = receipt_transport(&["a"], now);
        let original = transaction(1, 4);
        assert!(scheduler.acknowledge("a", &original, raw_digest(&original)));
        scheduler
            .gossip(&[&original], now, |_, _| panic!("already acknowledged"))
            .unwrap();
        let mut changed = original.clone();
        changed.raw[0] ^= 1;
        assert!(!scheduler.acknowledge("a", &changed, raw_digest(&original)));
        let mut sent = 0;
        scheduler
            .gossip(&[&changed], now, |_, entry| {
                assert_eq!(entry.raw, changed.raw);
                sent += 1;
                Ok(true)
            })
            .unwrap();
        assert_eq!(sent, 1);
        assert_eq!(scheduler.status_json()["acked_peer_transactions"], 0);
        assert!(scheduler.acknowledge("a", &changed, raw_digest(&changed)));
        scheduler
            .gossip(&[&changed], now + WINDOW, |_, _| {
                panic!("new raw was acknowledged")
            })
            .unwrap();
    }

    #[test]
    fn receipt_memory_tracks_exact_live_pool_and_clears_when_pool_is_empty() {
        let now = Instant::now();
        let mut scheduler = receipt_transport(&["a", "b"], now);
        let first = transaction(1, 4);
        let second = transaction(2, 4);
        for peer in ["a", "b"] {
            for entry in [&first, &second] {
                assert!(scheduler.acknowledge(peer, entry, raw_digest(entry)));
            }
        }
        scheduler
            .gossip(&[&second], now, |_, _| {
                panic!("remaining entry acknowledged")
            })
            .unwrap();
        assert_eq!(scheduler.status_json()["acked_peer_transactions"], 2);
        scheduler
            .gossip(&[], now, |_, _| panic!("empty pool"))
            .unwrap();
        assert_eq!(scheduler.status_json()["acked_peer_transactions"], 0);
        let mut sent = 0;
        scheduler
            .gossip(&[&second], now, |_, _| {
                sent += 1;
                Ok(true)
            })
            .unwrap();
        assert_eq!(
            sent, 2,
            "reintroduced entries must not inherit old receipts"
        );
    }

    #[test]
    fn reconnect_resets_receipts_and_sent_but_preserves_rate_budgets_and_ingress() {
        let now = Instant::now();
        let mut scheduler = receipt_transport(&["a", "b"], now);
        scheduler.limits.gossip_per_peer_per_second = 1;
        let entry = transaction(1, 4);
        scheduler.gossip(&[&entry], now, |_, _| Ok(true)).unwrap();
        for peer in ["a", "b"] {
            assert!(scheduler.acknowledge(peer, &entry, raw_digest(&entry)));
            assert!(scheduler.enqueue(inbound(peer, 1, 3)));
        }
        scheduler.reset_peer_receipts("unknown");
        assert_eq!(scheduler.status_json()["acked_peer_transactions"], 2);
        scheduler.reset_peer_receipts("a");
        assert_eq!(scheduler.status_json()["acked_peer_transactions"], 1);
        assert!(scheduler.peers[0].sent.is_empty());
        assert_eq!(scheduler.peers[1].sent.len(), 1);
        scheduler
            .gossip(&[&entry], now, |_, _| {
                panic!("reconnect must not reset quota")
            })
            .unwrap();
        let mut sent = Vec::new();
        scheduler
            .gossip(&[&entry], now + WINDOW, |peer, _| {
                sent.push(peer.to_owned());
                Ok(true)
            })
            .unwrap();
        assert_eq!(sent, ["a"]);
        scheduler.reset_all_receipts();
        sent.clear();
        scheduler
            .gossip(&[&entry], now + WINDOW, |peer, _| {
                sent.push(peer.to_owned());
                Ok(true)
            })
            .unwrap();
        assert_eq!(sent, ["b"]);
        scheduler.reset_all_receipts();
        scheduler
            .gossip(&[&entry], now + WINDOW, |_, _| {
                panic!("all-peer reset changed quotas")
            })
            .unwrap();
        assert_eq!(scheduler.status_json()["queued_entries"], 2);
        assert_eq!(scheduler.status_json()["queued_bytes"], 6);
        assert_eq!(scheduler.drain(now).0.len(), 2);
    }

    #[test]
    fn receipt_memory_is_bounded_per_peer_even_before_pool_pruning() {
        let now = Instant::now();
        let mut scheduler = receipt_transport(&["a", "b"], now);
        let mut first = None;
        for index in 0..MAX_STAGED_ENTRIES {
            let mut entry = transaction(1, 1);
            entry.hash[..8].copy_from_slice(&(index as u64).to_le_bytes());
            assert!(scheduler.acknowledge("a", &entry, raw_digest(&entry)));
            first.get_or_insert(entry);
        }
        let first = first.unwrap();
        let replacement = transaction(255, 1);
        assert!(!scheduler.acknowledge("a", &replacement, raw_digest(&replacement)));
        assert!(scheduler.acknowledge("a", &first, raw_digest(&first)));
        assert!(scheduler.acknowledge("b", &replacement, raw_digest(&replacement)));
        assert_eq!(scheduler.peers[0].acked.len(), MAX_STAGED_ENTRIES);
        assert_eq!(scheduler.peers[1].acked.len(), 1);
        scheduler.gossip(&[&first], now, |_, _| Ok(true)).unwrap();
        assert_eq!(scheduler.peers[0].acked.len(), 1);
        assert_eq!(scheduler.peers[1].acked.len(), 0);
        assert!(scheduler.acknowledge("a", &replacement, raw_digest(&replacement)));
    }
}
