//! Repair only recipients omitted at local fanout admission. No new wire,
//! payload queue, delivery promise or signing permission. A hint lives exactly
//! as long as its original entry in the existing bounded, non-durable pool.
use super::*;

pub(super) struct RepairHints {
    counts: [usize; 64],
    mask: u64,
    peer_turn: usize,
    prefer_repair: bool,
    accepted_batches: u64,
    reserved_inputs: u64,
}

impl Default for RepairHints {
    fn default() -> Self {
        Self {
            counts: [0; 64],
            mask: 0,
            peer_turn: 0,
            prefer_repair: false,
            accepted_batches: 0,
            reserved_inputs: 0,
        }
    }
}

impl RepairHints {
    fn replace(&mut self, old: u64, new: u64) {
        for peer in 0..64 {
            let bit = 1_u64 << peer;
            if old & bit != 0 && new & bit == 0 {
                self.counts[peer] -= 1;
                if self.counts[peer] == 0 {
                    self.mask &= !bit;
                }
            } else if old & bit == 0 && new & bit != 0 {
                self.counts[peer] += 1;
                self.mask |= bit;
            }
        }
    }

    pub(super) fn remove(&mut self, old: u64) {
        if old != 0 {
            self.replace(old, 0);
        }
    }

    fn next_peer(&self, available: &[bool]) -> Option<usize> {
        (0..available.len())
            .map(|offset| (self.peer_turn + offset) % available.len())
            .find(|peer| available[*peer] && self.counts[*peer] != 0)
    }

    pub(super) fn status(&self) -> Value {
        json!({"omitted_recipient_inputs": self.counts.iter().sum::<usize>(),
            "repair_batches_reserved": self.accepted_batches,
            "repair_inputs_reserved": self.reserved_inputs,
            "scope":"local omitted-recipient repair; not remote or durable receipt"})
    }
}

fn bits(mask: &[bool]) -> u64 {
    debug_assert!(mask.len() <= 64);
    mask.iter()
        .enumerate()
        .fold(0, |value, (peer, set)| value | (u64::from(*set) << peer))
}

/// Select only retained originals omitted for this recipient. Preserve source
/// pool ordering and the original count/byte bound; never reconstruct signed
/// bytes from a receipt, speculative result or unsigned transaction hash.
fn select_repair(
    order: &VecDeque<Hash>,
    pending: &BTreeMap<Hash, Pending>,
    peer: usize,
    batch_size: usize,
) -> (Vec<Hash>, Vec<Vec<u8>>) {
    let mut hashes = Vec::new();
    let mut raw = Vec::new();
    let mut bytes = 0;
    for hash in order {
        let Some(entry) = pending.get(hash) else {
            continue;
        };
        if entry.gossip_missing & (1_u64 << peer) == 0 {
            continue;
        }
        if hashes.len() == batch_size
            || bytes + entry.raw.len() > super::super::body_byte_limit(batch_size)
        {
            break;
        }
        bytes += entry.raw.len();
        hashes.push(*hash);
        raw.push(entry.raw.clone());
    }
    (hashes, raw)
}

impl RpcLifecycle {
    pub(super) fn poll_gossip(&mut self) -> Result<()> {
        if self.projection_error.is_some() {
            return Ok(());
        }
        if self.gossip_offer.is_none()
            && self.gossip_order.is_empty()
            && self.gossip_repair.mask == 0
        {
            return Ok(());
        }
        // Profile config already caps the set at 64 validators (63 remotes).
        // Do not use a validator-list index: these bits use the controller's
        // fixed peer order, including the exact order returned at admission.
        let available = self.node.controller.transactions_available_recipients();
        ensure!(
            available.len() <= 64,
            "RPC repair peer mask exceeds profile limit"
        );
        if self.gossip_offer.as_ref().is_some_and(|offer| {
            offer.repair_peer.is_some_and(|peer| {
                !available[peer]
                    || offer
                        .hashes
                        .iter()
                        .any(|hash| !self.pending.contains_key(hash))
            })
        }) {
            // This offer has NEVER been accepted by the channel owner. Keep
            // its pool hints; do not let its recipient block fresh fanout.
            self.gossip_offer = None;
        }
        if self.gossip_offer.is_none() {
            let repair = self.gossip_repair.next_peer(&available);
            let use_repair = repair.is_some()
                && (self.gossip_repair.prefer_repair || self.gossip_order.is_empty());
            let (hashes, raw, repair_peer) = if use_repair {
                let peer = repair.expect("eligible repair recipient");
                let (hashes, raw) =
                    select_repair(&self.order, &self.pending, peer, self.batch_size);
                (hashes, raw, Some(peer))
            } else {
                let mut hashes = Vec::new();
                let mut raw = Vec::new();
                let mut bytes = 0;
                while let Some(hash) = self.gossip_order.front().copied() {
                    if let Some(entry) = self.pending.get(&hash) {
                        if hashes.len() == self.batch_size
                            || bytes + entry.raw.len()
                                > super::super::body_byte_limit(self.batch_size)
                        {
                            break;
                        }
                        bytes += entry.raw.len();
                        hashes.push(hash);
                        raw.push(entry.raw.clone());
                    } else {
                        self.gossip_queued.remove(&hash);
                    }
                    self.gossip_order.pop_front();
                }
                (hashes, raw, None)
            };
            if !raw.is_empty() {
                let batch = apfl_batch(&raw, self.batch_size)?;
                self.gossip_offer = Some(GossipOffer {
                    hashes,
                    message: self.node.controller.apfl_transactions_message(batch)?,
                    repair_peer,
                });
            }
        }
        let Some(offer) = &self.gossip_offer else {
            return Ok(());
        };
        let requested: Vec<_> = (0..available.len())
            .map(|peer| offer.repair_peer.is_none_or(|repair| repair == peer))
            .collect();
        let Some(reserved) = self
            .node
            .controller
            .try_submit_transactions_to(&offer.message, &requested)?
        else {
            return Ok(());
        };
        let reserved = bits(&reserved);
        let omitted = bits(&requested) & !reserved;
        for hash in &offer.hashes {
            if let Some(entry) = self.pending.get_mut(hash) {
                let old = entry.gossip_missing;
                let new = (old | omitted) & !reserved;
                if new != old {
                    self.gossip_repair.replace(old, new);
                    entry.gossip_missing = new;
                }
            }
            if offer.repair_peer.is_none() {
                self.gossip_queued.remove(hash);
            }
        }
        if let Some(peer) = offer.repair_peer {
            self.gossip_repair.accepted_batches += 1;
            self.gossip_repair.reserved_inputs += offer.hashes.len() as u64;
            self.gossip_repair.peer_turn = peer + 1;
        }
        // At most one preparation per poll and only one retained offer, shared
        // by new fanout and repair. Neither class can starve the other.
        self.gossip_repair.prefer_repair = offer.repair_peer.is_none();
        self.gossip_offer = None;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hints_isolate_full_peer_without_retransmitting_reserved_recipients() {
        let mut hints = RepairHints::default();
        hints.replace(0, 0b101);
        assert_eq!(hints.next_peer(&[false, true, true]), Some(2));
        hints.replace(0b101, 0b001);
        assert_eq!(hints.next_peer(&[false, true, true]), None);
        assert_eq!(hints.next_peer(&[true, true, true]), Some(0));
        hints.remove(0b001);
        assert_eq!(hints.counts, [0; 64]);
    }

    #[test]
    fn repair_reuses_only_retained_originals_with_same_body_budget_and_order() {
        let mut pending = BTreeMap::new();
        let mut order = VecDeque::new();
        for n in 0..5_u8 {
            let hash = [n; 32];
            order.push_back(hash);
            pending.insert(
                hash,
                Pending {
                    raw: vec![n; 8],
                    signer: [1; 32],
                    nonce: n as u64,
                    gossip_missing: if n == 0 { 0 } else { 0b101 },
                },
            );
        }
        let (hashes, raw) = select_repair(&order, &pending, 2, 2);
        assert_eq!(hashes, vec![[1; 32], [2; 32]]);
        assert_eq!(raw, vec![vec![1; 8], vec![2; 8]]);
        assert!(select_repair(&order, &pending, 1, 2).0.is_empty());
        pending.remove(&[1; 32]); // finalized/stale data must not be resurrected
        assert_eq!(
            select_repair(&order, &pending, 2, 2).0,
            vec![[2; 32], [3; 32]]
        );
        assert_eq!(pending.len(), 4);
    }

    #[test]
    fn hints_remain_bounded_by_pool_entries_and_round_robin_peers() {
        let mut hints = RepairHints::default();
        for _ in 0..MAX_POOL {
            hints.replace(0, 0b111);
        }
        hints.peer_turn = 2;
        assert_eq!(hints.next_peer(&[true, true, true]), Some(2));
        hints.peer_turn = 3;
        assert_eq!(hints.next_peer(&[true, true, true]), Some(0));
        assert_eq!(hints.counts[0], MAX_POOL);
        for _ in 0..MAX_POOL {
            hints.remove(0b111);
        }
        assert_eq!(hints.next_peer(&[true, true, true]), None);
        assert_eq!(bits(&[true; 64]), u64::MAX);
    }
}
