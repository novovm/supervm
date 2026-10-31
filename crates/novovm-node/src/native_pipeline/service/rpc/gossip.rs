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
) -> Vec<Hash> {
    let mut hashes = Vec::new();
    let mut bytes = 0;
    for hash in order {
        let Some(entry) = pending.get(hash) else {
            continue;
        };
        if entry.gossip_missing & (1_u64 << peer) == 0 {
            continue;
        }
        if hashes.len() == batch_size
            || bytes + entry.canonical_len
                > if matches!(entry.input, AdmissionInput::Uca { .. }) {
                    super::super::uca_body_byte_limit(batch_size)
                } else {
                    super::super::body_byte_limit(batch_size)
                }
        {
            break;
        }
        bytes += entry.canonical_len;
        hashes.push(*hash);
    }
    hashes
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
        if self.gossip_repair.prefer_repair
            && self
                .gossip_offer
                .as_ref()
                .is_some_and(|offer| offer.repair_peer.is_none())
            && self.gossip_repair.next_peer(&available).is_some()
        {
            // This fresh offer has no channel reservation. Reconsider the
            // promised repair turn when its peer reopens, without losing or
            // duplicating the fresh rows already owned by the original queue.
            let offer = self.gossip_offer.take().expect("retained fresh offer");
            for hash in offer.hashes.into_iter().rev() {
                if self.pending.contains_key(&hash) {
                    self.gossip_order.push_front(hash);
                } else {
                    self.gossip_queued.remove(&hash);
                }
            }
        }
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
            let (hashes, repair_peer) = if use_repair {
                let peer = repair.expect("eligible repair recipient");
                let hashes = select_repair(&self.order, &self.pending, peer, self.batch_size);
                (hashes, Some(peer))
            } else {
                let mut hashes = Vec::new();
                let mut bytes = 0;
                while let Some(hash) = self.gossip_order.front().copied() {
                    if let Some(entry) = self.pending.get(&hash) {
                        if hashes.len() == self.batch_size
                            || bytes + entry.canonical_len > self.input_body_limit()
                        {
                            break;
                        }
                        bytes += entry.canonical_len;
                        hashes.push(hash);
                    } else {
                        self.gossip_queued.remove(&hash);
                    }
                    self.gossip_order.pop_front();
                }
                (hashes, None)
            };
            if !hashes.is_empty() {
                let message = if self.is_uca() {
                    self.node
                        .controller
                        .uca_transactions_message(self.selected_uca_batch(&hashes)?)?
                } else {
                    self.node
                        .controller
                        .apfl_transactions_message(selected_apfl_batch(
                            &hashes,
                            &self.pending,
                            self.batch_size,
                        )?)?
                };
                if let Message::AssetTransactions { scope, batch } = message.as_ref() {
                    crate::native_pipeline::profile::asset_event(
                        "gossip_batch_prepared",
                        self.node.controller.context().height,
                        || {
                            json!({"source_ref": Arc::as_ptr(batch) as usize,
                            "rows": hashes.len(), "session": hex(&scope.session),
                                "sequence": scope.sequence,
                                "ordered_ids": crate::native_pipeline::profile::asset_ids(&hashes),
                                "repair_peer": repair_peer})
                        },
                    );
                }
                self.gossip_offer = Some(GossipOffer {
                    hashes,
                    message,
                    repair_peer,
                    profile_wait: false,
                });
            }
        }
        let Some(offer) = &mut self.gossip_offer else {
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
            if !offer.profile_wait && crate::native_pipeline::profile::asset_enabled() {
                self.node
                    .controller
                    .profile_transactions_wait(&offer.message, &requested);
                offer.profile_wait = true;
            }
            return Ok(());
        };
        if let Message::AssetTransactions { scope, .. } = offer.message.as_ref() {
            crate::native_pipeline::profile::asset_event(
                "gossip_offer_reserved",
                self.node.controller.context().height,
                || {
                    json!({"session":scope.session,"sequence":scope.sequence,
                        "repair_peer":offer.repair_peer,"wait_observed":offer.profile_wait,
                        "requested":requested,"reserved":reserved,
                        "repair_mask_before":self.gossip_repair.mask,
                        "prefer_repair_before":self.gossip_repair.prefer_repair})
                },
            );
        }
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

    fn open_first_peer_slot(nodes: &mut [RpcLifecycle]) -> Result<()> {
        let deadline = Instant::now() + Duration::from_secs(10);
        while !nodes[0].node.controller.transactions_available_recipients()[0] {
            ensure!(
                Instant::now() < deadline,
                "original UDP/DONE did not reopen peer slot"
            );
            // Only the first remote drains. Other recipients retain their real
            // original handles; no synthetic completion or queue removal.
            nodes[0].node.poll(Instant::now())?;
            nodes[1].node.poll(Instant::now())?;
            while nodes[1].node.controller.take_transactions().is_some() {}
            std::thread::sleep(Duration::from_millis(1));
        }
        Ok(())
    }

    #[test]
    #[ignore = "real trusted AOEM; original asset admission, gossip owner and UDP/DONE, run alone"]
    fn retained_fresh_offer_yields_reopened_slot_to_older_missing_input() -> Result<()> {
        super::super::super::uca_native_tests::with_uca_gossip_service_fixture(|nodes, signed| {
            retained_fresh_offer_case(nodes, signed, false)
        })
    }

    #[test]
    #[ignore = "real trusted AOEM; original pool retirement and UDP/DONE, no fabricated finality, run alone"]
    fn retained_fresh_offer_requeues_live_rows_in_order_without_reviving_retired_input(
    ) -> Result<()> {
        super::super::super::uca_native_tests::with_uca_gossip_service_fixture(|nodes, signed| {
            retained_fresh_offer_case(nodes, signed, true)
        })
    }

    fn retained_fresh_offer_case(
        nodes: &mut [RpcLifecycle],
        signed: &[novovm_protocol::uca_spend::SignedAssetSpendV3],
        retire_retained_row: bool,
    ) -> Result<()> {
        use novovm_protocol::uca_spend_batch::{AssetSpendBatchLimitsV3, AssetSpendBatchV3};
        let hashes = signed.iter().map(uca::hash).collect::<Result<Vec<_>>>()?;
        let bytes = signed
            .iter()
            .map(|row| row.encode())
            .collect::<Result<Vec<_>, _>>()?;
        let mut sources = Vec::new();
        for raw in &bytes {
            let source = Arc::new(AssetSpendBatchV3::from_raw(
                std::slice::from_ref(raw),
                AssetSpendBatchLimitsV3 {
                    transactions: 1,
                    transaction_bytes: 8192,
                    body_bytes: 8192,
                },
            )?);
            let rpc = &mut nodes[0];
            rpc.ingress_batch_boundary = false;
            rpc.incoming_from_client = false;
            rpc.incoming = Some((
                rpc.node
                    .controller
                    .uca_transactions_message(source.clone())?,
                0,
            ));
            let deadline = Instant::now() + Duration::from_secs(10);
            while rpc.incoming.is_some() || rpc.admission.has_pending_native_admission() {
                ensure!(
                    Instant::now() < deadline,
                    "original signed asset admission timeout"
                );
                rpc.poll_admission()?;
                std::thread::yield_now();
            }
            sources.push(source);
        }
        let rpc = &mut nodes[0];
        assert_eq!(rpc.pending.len(), signed.len());
        assert!(rpc.gossip_order.is_empty());
        rpc.batch_size = 1; // Keep one old and one new obligation distinct.
        let mut retained_bytes = rpc.pending_retention.bytes;
        assert_eq!(
            rpc.node.controller.transactions_available_recipients(),
            vec![true; 3]
        );
        for _ in 0..2 {
            let blocker = rpc
                .node
                .controller
                .uca_transactions_message(sources[0].clone())?;
            assert_eq!(
                rpc.node
                    .controller
                    .try_submit_transactions_to(&blocker, &[true, false, false])?,
                Some(vec![true, false, false])
            );
        }
        for hash in &hashes[..2] {
            rpc.queue_gossip(*hash);
            rpc.poll_gossip()?;
            assert_eq!(rpc.pending[hash].gossip_missing, 1);
        }
        assert_eq!(rpc.gossip_repair.counts[0], 2);
        assert!(rpc.gossip_repair.prefer_repair);
        assert_eq!(
            rpc.node.controller.transactions_available_recipients(),
            vec![false; 3]
        );
        if retire_retained_row {
            rpc.batch_size = 2;
            rpc.queue_gossip(hashes[4]);
        }
        rpc.queue_gossip(hashes[2]);
        rpc.poll_gossip()?;
        let waiting = rpc
            .gossip_offer
            .as_ref()
            .expect("fresh offer retained while all peers full");
        assert_eq!(
            waiting.hashes,
            if retire_retained_row {
                vec![hashes[4], hashes[2]]
            } else {
                vec![hashes[2]]
            }
        );
        assert!(waiting.repair_peer.is_none());
        assert!(rpc.gossip_order.is_empty());
        assert!(rpc.gossip_queued.contains(&hashes[2]));

        if retire_retained_row {
            rpc.queue_gossip(hashes[2]); // Already owned by the retained offer.
            rpc.queue_gossip(hashes[3]);
            rpc.queue_gossip(hashes[3]);
            assert_eq!(rpc.gossip_order, VecDeque::from([hashes[3]]));
            // Model a changed published nonce projection, then exercise
            // the real cleanup. This is a pool lifecycle condition, not
            // a synthetic receipt, executed transaction or finality proof.
            let stale = &rpc.pending[&hashes[4]];
            let released = sources[4].retained_bytes()? + stale.retained_metadata_bytes();
            rpc.nonces.published(stale.signer, stale.nonce + 1);
            rpc.retire_stale_pending();
            retained_bytes -= released;
            assert_eq!(rpc.pending_retention.bytes, retained_bytes);
            assert!(!rpc.pending.contains_key(&hashes[4]));
            assert!(!rpc.gossip_queued.contains(&hashes[4]));
        }

        open_first_peer_slot(nodes)?;
        let rpc = &mut nodes[0];
        assert_eq!(
            rpc.node.controller.transactions_available_recipients(),
            vec![true, false, false]
        );
        rpc.poll_gossip()?;
        assert_eq!(
            rpc.gossip_repair.accepted_batches, 1,
            "a retained fresh offer must not bypass the already selected repair turn"
        );
        assert_eq!(rpc.pending[&hashes[0]].gossip_missing, 0);
        assert_eq!(
            rpc.pending[&hashes[1]].gossip_missing,
            if retire_retained_row { 0 } else { 1 }
        );
        let resumed = if retire_retained_row {
            vec![hashes[2], hashes[3]]
        } else {
            vec![hashes[2]]
        };
        assert_eq!(rpc.gossip_order, VecDeque::from(resumed.clone()));
        assert_eq!(rpc.gossip_queued, resumed.into_iter().collect());
        assert!(rpc.gossip_offer.is_none());
        assert!(!rpc.gossip_repair.prefer_repair);

        // The next reopened opportunity belongs to new input even though
        // another old repair remains: preemption must not starve fanout.
        open_first_peer_slot(nodes)?;
        nodes[0].poll_gossip()?;
        let rpc = &mut nodes[0];
        assert_eq!(rpc.gossip_repair.accepted_batches, 1);
        assert!(rpc.gossip_queued.is_empty() && rpc.gossip_order.is_empty());
        assert_eq!(rpc.pending[&hashes[2]].gossip_missing, 0b110);
        assert_eq!(
            rpc.pending[&hashes[1]].gossip_missing,
            if retire_retained_row { 0 } else { 1 }
        );
        assert!(rpc.gossip_repair.prefer_repair);
        if !retire_retained_row {
            open_first_peer_slot(nodes)?;
            nodes[0].poll_gossip()?;
        }
        let rpc = &nodes[0];
        assert_eq!(
            rpc.gossip_repair.accepted_batches,
            if retire_retained_row { 1 } else { 2 }
        );
        assert_eq!(rpc.pending[&hashes[1]].gossip_missing, 0);
        assert_eq!(
            rpc.gossip_repair.counts[..3],
            if retire_retained_row {
                [0, 2, 2]
            } else {
                [0, 1, 1]
            }
        );
        let retained = signed.len() - usize::from(retire_retained_row);
        assert_eq!(rpc.pending.len(), retained);
        assert_eq!(rpc.reservations.len(), retained);
        assert_eq!(rpc.pending_retention.bytes, retained_bytes);
        for (index, hash) in hashes.iter().enumerate() {
            if retire_retained_row && index == 4 {
                assert!(!rpc.pending.contains_key(hash));
                continue;
            }
            let AdmissionInput::Uca { batch, index: row } = &rpc.pending[hash].input else {
                panic!("signed asset source changed");
            };
            assert!(Arc::ptr_eq(batch, &sources[index]));
            assert_eq!(batch.raw_transaction(*row)?, bytes[index]);
        }
        assert!(
            rpc.node.controller.head().is_none(),
            "delivery is not finality"
        );
        Ok(())
    }

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
                    input: AdmissionInput::Raw(vec![n; 8]),
                    admitted: None,
                    canonical_len: 8,
                    signer: [1; 32],
                    nonce: n as u64,
                    gossip_missing: if n == 0 { 0 } else { 0b101 },
                },
            );
        }
        let hashes = select_repair(&order, &pending, 2, 2);
        assert_eq!(hashes, vec![[1; 32], [2; 32]]);
        assert!(select_repair(&order, &pending, 1, 2).is_empty());
        pending.remove(&[1; 32]); // finalized/stale data must not be resurrected
        assert_eq!(
            select_repair(&order, &pending, 2, 2),
            vec![[2; 32], [3; 32]]
        );
        assert_eq!(pending.len(), 4);
    }

    #[test]
    fn repair_selects_signed_structural_rows_without_raw_projection() -> Result<()> {
        let raws: Vec<_> = (0..4)
            .map(|nonce| super::super::super::tests::signed_raw(19, nonce))
            .collect();
        let source = apfl_batch(&raws, 8)?;
        let mut pending = BTreeMap::new();
        let mut order = VecDeque::new();
        for (index, raw) in raws.iter().enumerate() {
            let checked = authenticate_transfer_v3(raw, 71, 1024)?;
            let hash = checked.tx_hash();
            order.push_back(hash);
            pending.insert(
                hash,
                Pending {
                    input: AdmissionInput::Apfl {
                        batch: source.clone(),
                        index,
                    },
                    admitted: None,
                    canonical_len: raw.len(),
                    signer: checked.nonce_identity(),
                    nonce: checked.transfer().nonce,
                    gossip_missing: if index == 0 { 0 } else { 0b100 },
                },
            );
        }
        let selected = select_repair(&order, &pending, 2, 2);
        assert_eq!(selected, vec![order[1], order[2]]);
        let repair = selected_apfl_batch(&selected, &pending, 2)?;
        for (index, raw) in raws[1..3].iter().enumerate() {
            assert!(repair.row(index)?.matches_canonical_bytes(raw)?);
        }
        for entry in pending.values() {
            let AdmissionInput::Apfl { batch, .. } = &entry.input else {
                panic!("repair expanded pending source");
            };
            assert!(Arc::ptr_eq(batch, &source));
        }
        pending.remove(&order[1]);
        assert_eq!(
            select_repair(&order, &pending, 2, 2),
            vec![order[2], order[3]]
        );
        assert!(select_repair(&order, &pending, 1, 2).is_empty());
        Ok(())
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
